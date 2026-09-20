"""Run a frozen local SFT plan, evaluate saved models, and restore serving.

All subprocess output, model answers, commands, and grades stay under the
plan's output directory. Model inference has no extra generation/time cap.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time
import urllib.request

from compare_human_runs import check_completions, correct, summarize

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f,'sha256').hexdigest()


def read_rows(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines()]


def json_file(path, data):
    with Path(path).open('x') as f:
        json.dump(data,f,indent=2); f.write('\n')


def verify_hashes(hashes):
    for path,expected in hashes.items():
        if digest(path)!=expected:
            raise ValueError(f'Frozen input changed: {path}')


def process_alive(pid):
    path = Path(f'/proc/{pid}/stat')
    try:
        return path.read_text().rsplit(')',1)[1].split()[0]!='Z'
    except FileNotFoundError:
        return False


def matches_server(args, model, alias, port):
    try:
        model_arg = args[args.index(b'serve')+1].decode()
    except (ValueError,IndexError,UnicodeDecodeError):
        return False
    return (Path(model_arg).resolve()==Path(model).resolve() and
            alias.encode() in args and str(port).encode() in args)


def checked_results(dataset, directory, mode):
    fixtures = read_rows(Path(dataset)/'fixtures.jsonl')
    ids = {f['id'] for f in fixtures}
    if len(ids)!=len(fixtures): raise ValueError('Duplicate fixture IDs')
    d = Path(directory)
    metadata = json.loads((d/'run.json').read_text())
    if metadata['dataset_manifest']['fixtures_sha256']!=digest(Path(dataset)/'fixtures.jsonl'):
        raise ValueError('Result dataset hash mismatch')
    if metadata['attempts']!=1 or metadata['tools']!=('chat' if mode=='chat' else 'none'):
        raise ValueError('Result inference contract mismatch')
    check_completions(d,metadata,ids)
    records = read_rows(d/('chat-text.jsonl' if mode=='chat' else 'responses.jsonl'))
    if mode=='submit' and (d/'failures.jsonl').exists(): records += read_rows(d/'failures.jsonl')
    if len(records)!=len(ids): raise ValueError('Duplicate completion records')
    if any(r['finish_reason'] not in ['stop','tool_calls','length'] for r in records):
        raise ValueError('Unexpected completion finish reason')
    grades = json.loads((d/'graded/results.json').read_text())
    results = {r['task_id']:r for r in grades}
    if len(results)!=len(grades) or set(results)-ids: raise ValueError('Invalid grade coverage')
    return dict(directory=str(d),summary=summarize(fixtures,results),results=results,
                finish_reasons=dict(Counter(r['finish_reason'] for r in records)))


def comparisons(fixtures, parent, candidate):
    groups = {key:[] for key in ['both','gained','lost','neither']}
    for f in fixtures:
        a,b = correct(parent.get(f['id'])),correct(candidate.get(f['id']))
        groups['both' if a and b else 'lost' if a else 'gained' if b else 'neither'].append(f['id'])
    return groups


def validate_training_rows(rows, plan):
    if not rows or len(rows)!=plan['rows']:
        raise ValueError('Training row count differs from the frozen plan')
    for row in rows:
        if row.get('split')!='training':
            raise ValueError('Training row lacks training provenance')
        if 'task_kinds' in plan and row.get('task_kind') not in plan['task_kinds']:
            raise ValueError('Training row has an excluded task kind')
        if 'training_interfaces' in plan and row.get('interface') not in plan['training_interfaces']:
            raise ValueError('Training row has an excluded interface')


class Experiment:
    def __init__(self, plan):
        self.plan_path = Path(plan)
        self.p = json.loads(self.plan_path.read_text())
        self.out = Path(self.p['out'])
        self.active = self.work = None
        self.switched = False
        self.owns_output = False
        self.status = dict(phase='Preflight',completed=[],evaluation={},plan_sha256=digest(plan))

    def record(self, phase):
        self.status.update(phase=phase,updated_at_unix=time.time())
        temporary = self.out/'status.tmp'
        temporary.write_text(json.dumps(self.status,indent=2)+'\n')
        temporary.replace(self.out/'status.json')
        print(phase,flush=True)

    def environment(self):
        env = os.environ.copy()
        env.update(self.p['environment'])
        env['PYTHONUNBUFFERED'] = '1'
        if env.get('CUDA_HOME'): env['PATH'] = env['CUDA_HOME']+'/bin:'+env['PATH']
        return env

    def run(self, command, name):
        json_file(self.out/(name+'.command.json'),command)
        with (self.out/(name+'.log')).open('x') as log:
            self.work = subprocess.Popen(command,env=self.environment(),stdout=log,stderr=subprocess.STDOUT)
            self.status['worker_pid'] = self.work.pid
            self.record(self.status['phase'])
            code = self.work.wait()
        self.work = None
        if code: raise RuntimeError(f'Command exited {code}: {name}.log')

    def ready(self, alias, process=None):
        # Startup readiness has a limit; this never limits model generation.
        deadline = time.monotonic()+600
        while time.monotonic()<deadline:
            if process is not None and process.poll() is not None:
                raise RuntimeError(f'Model server exited {process.returncode}')
            try:
                with urllib.request.urlopen(self.p['endpoint']+'/models',timeout=5) as r:
                    names = [x['id'] for x in json.load(r)['data']]
                if names!=[alias]: raise RuntimeError(f'Unexpected model server: {names}')
                return
            except OSError:
                time.sleep(2)
        raise RuntimeError('Model server startup did not finish in 600 seconds')

    def serve(self, model, alias, label):
        command = [self.p['vllm_python'],self.p['vllm_cli'],'serve',str(Path(model).resolve()),
            '--served-model-name',alias,'--port',str(self.p['port']),
            '--enable-auto-tool-choice','--tool-call-parser','hermes','--reasoning-parser','qwen3',
            '--gpu-memory-utilization','0.85','--max-model-len','32768',
            '--speculative-config',json.dumps(self.p['speculative_config'])]
        json_file(self.out/f'server-{label}.command.json',command)
        with (self.out/f'server-{label}.log').open('x') as log:
            self.active = subprocess.Popen(command,env=self.environment(),stdout=log,
                stderr=subprocess.STDOUT,start_new_session=True)
        self.status['server_pid'] = self.active.pid
        self.record(f'Loading server: {label}')
        self.ready(alias,self.active)

    def stop_server(self):
        if self.active is not None and self.active.poll() is None:
            self.active.terminate()
            self.active.wait()
        self.active = None

    def grade(self, dataset, directory, name):
        self.run(['target/release/btc-bench','grade','--dataset',str(dataset),
            '--responses',str(Path(directory)/'responses.jsonl'),'--out',str(Path(directory)/'graded')],name)

    def evaluate(self, label, dataset, mode, alias):
        suite = Path(dataset).name
        name = f'{label}-{suite}-{mode}'
        d = self.out/name
        d.mkdir()
        config = ('[model.candidate]\nprovider = "openai_compatible"\nmodel = '+json.dumps(alias)+
            '\nbase_url = '+json.dumps(self.p['endpoint'])+'\nretries = 4\ntemperature = 0.6\n\n'+
            '[model.candidate.request_params]\ntop_p = 0.95\ntop_k = 20\nmin_p = 0.0\nseed = 20260904\n'+
            'chat_template_kwargs = { enable_thinking = true }\n')
        (d/'inference-config.toml').write_text(config)
        self.record(f'Evaluating {name}')
        self.run(['target/release/btc-bench','run','--dataset',dataset,'--config',str(d/'inference-config.toml'),
            '--model','candidate','--out',str(d),'--concurrency','4','--attempts','1','--tools',
            'chat' if mode=='chat' else 'none'],'run-'+name)
        self.grade(dataset,d,'grade-'+name)
        result = checked_results(dataset,d,mode)
        self.status['evaluation'].setdefault(label,{}).setdefault(suite,{})[mode] = result
        self.status['completed'].append(name)
        self.record(f'Completed {name}')

    def report(self):
        evaluation = self.status['evaluation']
        lines = ['# '+self.p.get('report_title','Script construction SFT results'),'',
            'The final checkpoint was fixed before training. No evaluation selected a checkpoint.',
            'Missing answers remain in the denominator. Correctness uses the existing Miniscript oracle.',
            'Related write/repair and chat/submit forms are not independent observations.','',
            '| Model | Dataset | Interface | Task | Correct | Total |',
            '|---|---|---|---|---:|---:|']
        paired = {}
        for label,suites in evaluation.items():
            for suite,modes in suites.items():
                fixtures = read_rows(Path('datasets')/suite/'fixtures.jsonl')
                for mode,result in modes.items():
                    for kind,cell in result['summary'].items():
                        lines.append(f"| {label} | {suite} | {mode} | {kind} | {cell['correct']} | {cell['total']} |")
                    if suite==Path(self.p['reserved']).name:
                        for kind in ['write','repair']:
                            part = [f for f in fixtures if f['id'].endswith('-'+kind)]
                            if part:
                                count = sum(correct(result['results'].get(f['id'])) for f in part)
                                lines.append(f'| {label} | {suite} | {mode} | {kind} requests | {count} | {len(part)} |')
                    if self.p.get('report_contexts') and suite==Path(self.p['reserved']).name:
                        for context in ['legacy','segwitv0','tap']:
                            part = [f for f in fixtures if f.get('context')==context]
                            if part:
                                count = sum(correct(result['results'].get(f['id'])) for f in part)
                                lines.append(f'| {label} | {suite} | {mode} | {context} | {count} | {len(part)} |')
                    if label=='candidate' and suite in evaluation.get('parent',{}):
                        parent = evaluation['parent'][suite][mode]['results']
                        paired[f'{suite}/{mode}'] = comparisons(fixtures,parent,result['results'])
        lines += ['','## Paired changes from parent','','| Dataset/interface | Gained | Lost | Both | Neither |',
                  '|---|---:|---:|---:|---:|']
        for name,groups in paired.items():
            counts = ' | '.join(str(len(groups[k])) for k in ['gained','lost','both','neither'])
            lines.append(f'| {name} | {counts} |')
        lines += ['',self.p['interpretation'],'','Full prompts, outputs, and grades are retained in this run directory.','']
        json_file(self.out/'results.json',dict(evaluation=evaluation,paired=paired,plan_sha256=digest(self.plan_path)))
        (self.out/'results.md').write_text('\n'.join(lines))

    def trace_report(self):
        fixtures = read_rows(Path(self.p['reserved'])/'fixtures.jsonl')
        inputs = [dict(id='reference/'+f['id'],reference_policy=f['reference_policy'],
                       policy=f['reference_policy'],miniscript=f['reference_miniscript']) for f in fixtures]
        first = fixtures[0]
        inputs.append(dict(id='control/invalid',reference_policy=first['reference_policy'],
                           policy='broken(',miniscript='broken('))
        markers = dict(policy='As a policy that is|Written as a policy|That gives the policy|Its spending policy is',
                       miniscript='In Miniscript|The Miniscript for that policy|Compiling that to Miniscript for this context')
        for label,suites in self.status['evaluation'].items():
            for mode,run in suites[Path(self.p['reserved']).name].items():
                d = Path(run['directory'])
                records = read_rows(d/('chat-text.jsonl' if mode=='chat' else 'responses.jsonl'))
                if mode=='submit' and (d/'failures.jsonl').exists(): records += read_rows(d/'failures.jsonl')
                answers = {r['task_id']:r for r in records}
                for f in fixtures:
                    raw = answers[f['id']].get('raw','')
                    row = dict(id=f"{label}/{mode}/{f['id']}",reference_policy=f['reference_policy'])
                    for kind,pattern in markers.items():
                        found = re.findall(r'^(?:'+pattern+r'): (.+)$',raw,re.M)
                        row[kind] = found[-1].strip() if found else None
                    inputs.append(row)
        data = ''.join(json.dumps(row)+'\n' for row in inputs)
        (self.out/'trace-input.jsonl').write_text(data)
        checked = subprocess.run(['target/release/examples/check_trace_parts'],input=data,
                                 text=True,capture_output=True,check=True)
        (self.out/'trace-checks.jsonl').write_text(checked.stdout)
        counts = {}
        for row in map(json.loads,checked.stdout.splitlines()):
            if row['id'].startswith('reference/'):
                if any(row[k]['status']!='equivalent' for k in markers): raise ValueError('Trace reference failed')
            elif row['id']=='control/invalid':
                if any(row[k]['status']!='invalid' for k in markers): raise ValueError('Negative trace control failed')
            else:
                label,mode,_ = row['id'].split('/',2)
                count = counts.setdefault(label+'/'+mode,{k:Counter() for k in markers})
                for kind in markers: count[kind][row[kind]['status']]+=1
        json_file(self.out/'trace-summary.json',dict(counts=counts,scope=
            'Explicit marked expressions only, checked in Segwit v0 without syntax repair. Missing markers are unmeasured. These checks do not change final script grades.'))

    def execute(self):
        self.out.mkdir()
        self.owns_output = True
        self.record('Checking frozen inputs')
        verify_hashes(self.p['hashes'])
        json_file(self.out/'frozen-plan.json',self.p)
        validate_training_rows(read_rows(self.p['data']),self.p)
        if 'task_kinds' in self.p:
            for dataset in [self.p['reserved']]+self.p['retention_datasets']+self.p.get('fit_datasets',[]):
                if any(f['task'] not in self.p['task_kinds'] for f in read_rows(Path(dataset)/'fixtures.jsonl')):
                    raise ValueError('Evaluation contains an excluded task kind')
        import shutil
        source_dir = self.out/'source-snapshot'
        source_dir.mkdir()
        for filename in self.p['hashes']:
            path = Path(filename)
            if not path.is_absolute() and path.suffix in ['.py','.rs','.md','.toml']:
                destination = source_dir/path
                destination.parent.mkdir(parents=True,exist_ok=True)
                shutil.copy2(path,destination)
        original = self.p['original']
        pid = original['pid']
        args = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
        if not matches_server(args,original['model'],original['alias'],self.p['port']):
            raise ValueError('Original server process identity changed')
        self.ready(original['alias'])
        self.record('Stopping original server for training')
        self.switched = True
        os.kill(pid,signal.SIGTERM)
        while process_alive(pid): time.sleep(1)
        steps = self.p['steps']
        self.record(f'Training {steps} fixed SFT updates')
        self.run([self.p['sft_python'],'scripts/sft_train.py','--data',self.p['data'],
            '--model',self.p['parent'],'--out',str(self.out),'--epochs','1',
            '--learning-rate',str(self.p['learning_rate']),'--warmup-steps',str(self.p['warmup_steps']),
            '--gradient-accumulation-steps',str(self.p['gradient_accumulation_steps']),
            '--save-steps',str(self.p['save_steps']),'--audit-data',str(self.out/'loss-mask-audit.json')],'train')
        checkpoint = self.out/f'checkpoint-{steps}'
        state = json.loads((checkpoint/'trainer_state.json').read_text())
        if state['global_step']!=steps: raise ValueError('Wrong final update count')
        audit = json.loads((self.out/'loss-mask-audit.json').read_text())
        if len(audit)!=self.p['rows']: raise ValueError('Incomplete loss-mask audit')
        self.status['completed'].append('training')
        merged = self.out/f'merged-step{steps}'
        self.record('Merging the fixed final checkpoint')
        self.run([self.p['sft_python'],'scripts/merge_adapter.py','--base',self.p['parent'],
                  '--adapter',str(checkpoint),'--out',str(merged)],'merge')
        self.status['candidate_weight_sha256'] = digest(merged/'model.safetensors')
        # Regrade saved parent responses with the same binary before comparing.
        for suite,modes in self.p['parent_runs'].items():
            for mode,source in modes.items():
                import shutil
                target = self.out/f'parent-{suite}-{mode}'
                target.mkdir()
                for filename in ['run.json','responses.jsonl','chat-text.jsonl','failures.jsonl']:
                    path = Path(source)/filename
                    if path.exists(): shutil.copy2(path,target/filename)
                self.grade('datasets/'+suite,target,f'grade-parent-{suite}-{mode}')
                self.status['evaluation'].setdefault('parent',{}).setdefault(suite,{})[mode] = checked_results('datasets/'+suite,target,mode)
        self.status['final_checkpoint_fixed'] = True
        self.record('Fixed final checkpoint saved; starting evaluation')
        for label,model in [('candidate',str(merged)),('parent',self.p['parent'])]+list(self.p['baselines'].items()):
            alias = 'qwen3-4b-'+self.p['name']+'-'+label
            self.serve(model,alias,label)
            datasets = [self.p['reserved']]
            if label in ['candidate','parent']: datasets += self.p.get('fit_datasets',[])
            if label=='candidate' or (label=='parent' and self.p.get('evaluate_parent_retention')):
                datasets += self.p['retention_datasets']
            for dataset in datasets:
                for mode in self.p['interfaces']: self.evaluate(label,dataset,mode,alias)
            self.stop_server()
        verify_hashes(self.p['hashes'])
        if self.p.get('trace_diagnostics',True): self.trace_report()
        self.report()

    def run_and_restore(self):
        error = None
        try:
            self.execute()
        except BaseException as exc:
            error = exc
            if self.owns_output:
                self.status['error'] = repr(exc)
                self.record('Experiment failed; restoring original serving model')
        finally:
            if self.work is not None and self.work.poll() is None:
                self.work.terminate(); self.work.wait()
            self.stop_server()
            if self.switched:
                self.serve(self.p['original']['model'],self.p['original']['alias'],'original-restored')
                self.status['restored_pid'] = self.active.pid
                # Leave the restored server running after the supervisor exits.
                self.active = None
                self.record('Failed; original server restored' if error else 'Complete; original server restored')
        if error: raise error


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('plan')
    ap.add_argument('--check',action='store_true',help='Validate frozen file hashes without changing serving state.')
    args = ap.parse_args()
    os.chdir(ROOT)
    experiment = Experiment(args.plan)
    if args.check:
        verify_hashes(experiment.p['hashes'])
        print('Frozen input hashes pass')
        return
    def interrupted(signum,frame):
        raise InterruptedError(f'Signal {signum}')
    signal.signal(signal.SIGTERM,interrupted)
    experiment.run_and_restore()


if __name__=='__main__':
    main()
