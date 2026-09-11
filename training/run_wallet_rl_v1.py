"""Run the frozen wallet/earlier-skill RL pilot and matched reference control."""
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import urllib.request

ROOT=Path(__file__).resolve().parents[1]
os.chdir(ROOT)
sys.path.insert(0,str(ROOT/'scripts'))
from compare_human_runs import check_completions,summarize
settings=json.loads(Path('training/wallet-rl-v1-experiment.json').read_text())
OUT=Path(settings['out'])
active=reward=work=None
switched=False
status={'phase':'Starting','completed':[],'results':{}}


def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()


def record(phase):
    status['phase']=phase;status['updated_at_unix']=time.time()
    tmp=OUT/'status.tmp';tmp.write_text(json.dumps(status,indent=2)+'\n');tmp.replace(OUT/'status.json')
    print(phase,flush=True)


def env():
    e=os.environ.copy();e.update(CUDA_VISIBLE_DEVICES='0',CUDA_DEVICE_ORDER='PCI_BUS_ID',CUDA_HOME='/usr/local/cuda-13.1',PYTHONUNBUFFERED='1')
    e['PATH']='/usr/local/cuda-13.1/bin:'+e['PATH'];return e


def launch(cmd,path):
    with Path(path).open('x') as log:return subprocess.Popen(cmd,env=env(),stdout=log,stderr=subprocess.STDOUT,start_new_session=True)


def run(cmd,path):
    global work
    Path(str(path)+'.command.json').write_text(json.dumps(cmd,indent=2)+'\n')
    work=launch(cmd,path);status['worker_pid']=work.pid
    code=work.wait();work=None
    if code:raise RuntimeError(f'Command failed ({code}); inspect {path}')


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate();process.wait()


def ready(alias,process=None):
    while True:
        if process is not None and process.poll() is not None:raise RuntimeError('Model server exited')
        try:
            with urllib.request.urlopen('http://127.0.0.1:8010/v1/models',timeout=5) as r:models=[m['id'] for m in json.load(r)['data']]
            if models!=[alias]:raise RuntimeError(f'Unexpected models: {models}')
            return
        except OSError:time.sleep(2)


def serve(model,alias,name):
    global active
    cmd=[settings['vllm_python'],'/home/ben/.local/bin/vllm','serve',str(ROOT/model),
         '--served-model-name',alias,'--port','8010','--enable-auto-tool-choice','--tool-call-parser','hermes',
         '--reasoning-parser','qwen3','--gpu-memory-utilization','0.85','--max-model-len','32768','--speculative-config',
         json.dumps(settings['speculative_config'])]
    active=launch(cmd,OUT/f'server-{name}.log');status['server_pid']=active.pid;ready(alias,active)


def evaluate(arm,suite,mode,alias):
    directory=OUT/f'{arm}-{suite}-{mode}'
    record(f'Evaluating {arm}: {suite}, {mode}')
    wallet=suite.startswith('wallet-')
    if wallet:
        run(['python3','-u','scripts/run_wallet_bench.py','--dataset','datasets/'+suite,'--out',str(directory),'--model',alias,'--mode',mode],OUT/f'run-{arm}-{suite}-{mode}.log')
        run(['target/release/btc-wallet-bench','grade','--dataset','datasets/'+suite,'--responses',str(directory/'responses.jsonl'),'--out',str(directory/'graded')],OUT/f'grade-{arm}-{suite}-{mode}.log')
    else:
        directory.mkdir()
        template=Path(settings['config_template']).read_text()
        assert 'qwen3-4b-compound-v3-step144' in template
        (directory/'inference-config.toml').write_text(template.replace('qwen3-4b-compound-v3-step144',alias))
        run(['target/release/btc-bench','run','--dataset','datasets/'+suite,'--config',str(directory/'inference-config.toml'),
             '--model','human-sft','--out',str(directory),'--concurrency','4','--tools','chat' if mode=='chat' else 'none'],directory/'run.log')
        run(['target/release/btc-bench','grade','--dataset','datasets/'+suite,'--responses',str(directory/'responses.jsonl'),'--out',str(directory/'graded')],directory/'grade.log')
    fs=list(map(json.loads,Path(f'datasets/{suite}/fixtures.jsonl').read_text().splitlines()));ids={f['id'] for f in fs}
    grades=json.loads((directory/'graded/results.json').read_text());byid={r['task_id']:r for r in grades}
    assert len(byid)==len(grades) and set(byid)<=ids
    metadata=json.loads((directory/'run.json').read_text())
    if wallet:
        assert metadata['fixture_sha256']==settings['fixture_hashes'][suite]
        records=list(map(json.loads,(directory/'responses.jsonl').read_text().splitlines()))
        assert set(byid)==ids
        summary=json.loads((directory/'graded/summary.json').read_text())
    else:
        assert metadata['dataset_manifest']['fixtures_sha256']==settings['fixture_hashes'][suite]
        check_completions(directory,metadata,ids)
        records=list(map(json.loads,(directory/('chat-text.jsonl' if mode=='chat' else 'responses.jsonl')).read_text().splitlines()))
        if mode=='submit' and (directory/'failures.jsonl').exists():records+=list(map(json.loads,(directory/'failures.jsonl').read_text().splitlines()))
        summary=summarize(fs,byid)
    assert len(records)==len(ids) and {r['task_id'] for r in records}==ids
    assert all(r['finish_reason'] in ['stop','tool_calls','length'] for r in records)
    return dict(directory=str(directory),summary=summary,finish_reasons=dict(Counter(r['finish_reason'] for r in records)))


def main():
    global active,reward,switched
    for p,sha in settings['hashes'].items():assert digest(p)==sha,p
    assert digest(Path(settings['parent'])/'model.safetensors')==settings['parent_weight_sha256']
    assert digest(Path(settings['original_model'])/'model.safetensors')==settings['original_weight_sha256']
    ready('qwen3-4b-think')
    pid=settings['original_pid'];cmd=Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
    assert b'qwen3-4b-think' in cmd and b'8010' in cmd and str(ROOT/settings['original_model']).encode() in cmd
    reward=launch(['target/release/btc-bench','reward-serve','--bind','127.0.0.1:9902'],OUT/'reward.log')
    while True:
        assert reward.poll() is None
        try:
            with urllib.request.urlopen('http://127.0.0.1:9902/health',timeout=5):break
        except OSError:time.sleep(1)
    record('Testing actual HTTP reward behavior')
    run(['python3','training/check_wallet_reward_http.py','--url','http://127.0.0.1:9902/reward/batch'],OUT/'reward-preflight.log')
    record('Stopping original serving model for the probe')
    switched=True;os.kill(pid,signal.SIGTERM)
    while Path(f'/proc/{pid}/stat').exists():
        if Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[0]=='Z':break
        time.sleep(1)
    alias='qwen3-4b-wallet-rl-v1-probe'
    serve(Path(settings['parent']),alias,'probe')
    record('Probing 64 training questions with eight samples each')
    run(['python3','-u','scripts/rl_probe.py','--data',settings['probe_data'],'--model',alias,'--tasks','64','--seed','7',
         '--k','8','--max-completion-length','4096','--reward-url','http://127.0.0.1:9902/reward/batch',
         '--out',str(OUT/'probe.jsonl')],OUT/'probe.log')
    run(['python3','training/select_wallet_rl_v1.py'],OUT/'select.log')
    probe=json.loads((OUT/'probe-summary.json').read_text());status['probe']=probe['source_summary']
    status['training_eligible']=probe['train'];record('Training probe complete')
    stop(active);active=None
    if not probe['train']:
        record('Insufficient mixed-reward groups in both sources; training skipped');return
    selected=Path('datasets/rl-wallet-v1-selected.jsonl')
    status['selected_sha256']=digest(selected)
    (OUT/'selection-lock.json').write_text(json.dumps(dict(selected_sha256=status['selected_sha256'],selected_ids=probe['selected_ids'],locked_at_unix=time.time()),indent=2)+'\n')
    paths={arm:OUT/arm for arm in ['rl','sft']}
    for path in paths.values():path.mkdir()
    record('Training RL arm for 32 updates')
    run([settings['rl_python'],'scripts/rl_train.py','--data',str(selected),'--model',settings['parent'],'--out',str(paths['rl']),
         '--steps','32','--warmup-steps','3','--save-steps','16','--reward-url','http://127.0.0.1:9902/reward/batch',
         '--rollout-log',str(paths['rl']/'rollouts.jsonl')],paths['rl']/'train.log')
    assert 'RL-DONE' in (paths['rl']/'train.log').read_text();status['completed'].append('RL training')
    stop(reward);reward=None
    record('Preparing the reference SFT control from actual RL question counts')
    run(['python3','training/prepare_wallet_rl_control_v1.py'],paths['sft']/'prepare.log')
    record('Training matched SFT control for 32 updates')
    run([settings['sft_python'],'scripts/sft_train.py','--data','datasets/sft-wallet-rl-control-v1.jsonl','--model',settings['parent'],
         '--out',str(paths['sft']),'--epochs','1','--learning-rate','1e-5','--lora-dropout','0','--warmup-steps','3',
         '--gradient-accumulation-steps','8','--save-steps','16','--audit-data',str(paths['sft']/'loss-mask-audit.json')],paths['sft']/'train.log')
    assert 'TRAIN-DONE' in (paths['sft']/'train.log').read_text()
    assert len(json.loads((paths['sft']/'loss-mask-audit.json').read_text()))==256
    status['completed'].append('SFT control training')
    status['weights']={}
    for arm,path in paths.items():
        assert json.loads((path/'checkpoint-32/trainer_state.json').read_text())['global_step']==32
        record(f'Merging {arm} fixed final checkpoint')
        run([settings['sft_python'],'scripts/merge_adapter.py','--base',settings['parent'],'--adapter',str(path/'final'),
             '--out',str(path/'merged')],path/'merge.log')
        status['weights'][arm]=digest(path/'merged/model.safetensors')
        alias='qwen3-4b-wallet-rl-v1-'+arm
        serve(path/'merged',alias,arm)
        status['results'][arm]={}
        for suite in settings['evaluation_suites']:
            status['results'][arm][suite]={}
            for mode in ['chat','submit']:
                status['results'][arm][suite][mode]=evaluate(arm,suite,mode,alias)
                status['completed'].append(f'{arm}/{suite}/{mode}');record(f'Completed {arm}: {suite}, {mode}')
        stop(active);active=None


def interrupted(signum,frame):raise InterruptedError(f'Signal {signum}')


if __name__=='__main__':
    signal.signal(signal.SIGTERM,interrupted)
    try:main()
    except BaseException as error:
        status['error']=repr(error);record('RL pilot failed; inspect saved logs');raise
    finally:
        stop(work);stop(active);stop(reward)
        if switched:
            record('Restoring original model server')
            serve(Path(settings['original_model']),'qwen3-4b-think','original-restored')
            status['restored_pid']=active.pid
    record('Wallet RL pilot complete; original server available')
