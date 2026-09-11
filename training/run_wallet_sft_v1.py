"""Run one frozen wallet SFT pass, measure transfer/retention, restore serving."""
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

ROOT = Path(__file__).resolve().parents[1]
os.chdir(ROOT)
sys.path.insert(0, str(ROOT / 'scripts'))
from compare_human_runs import check_completions, summarize

PLAN = Path('training/wallet-sft-v1-experiment.json')
settings = json.loads(PLAN.read_text())
RUN = Path(settings['out'])
active = work = None
switched = False
status = {'phase': 'Starting', 'completed': [], 'development': {}, 'reserved': {}}


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def record(phase):
    status['phase'] = phase
    status['updated_at_unix'] = time.time()
    tmp = RUN / 'status.tmp'
    tmp.write_text(json.dumps(status, indent=2) + '\n')
    tmp.replace(RUN / 'status.json')
    print(phase, flush=True)


def environment():
    env = os.environ.copy()
    env.update(CUDA_VISIBLE_DEVICES='0', CUDA_DEVICE_ORDER='PCI_BUS_ID', CUDA_HOME='/usr/local/cuda-13.1', PYTHONUNBUFFERED='1')
    env['PATH'] = '/usr/local/cuda-13.1/bin:' + env['PATH']
    return env


def ready(alias, process=None):
    while True:
        if process is not None and process.poll() is not None:
            raise RuntimeError(f'Server exited: {process.returncode}')
        try:
            with urllib.request.urlopen('http://127.0.0.1:8010/v1/models', timeout=5) as r:
                models = [m['id'] for m in json.load(r)['data']]
            if models != [alias]: raise RuntimeError(f'Unexpected models: {models}')
            return
        except OSError:
            time.sleep(2)


def serve(model, alias, name):
    global active
    cmd = [settings['vllm_python'], '/home/ben/.local/bin/vllm', 'serve', str(ROOT / model),
           '--served-model-name', alias, '--port', '8010', '--enable-auto-tool-choice',
           '--tool-call-parser', 'hermes', '--reasoning-parser', 'qwen3', '--gpu-memory-utilization', '0.85',
           '--max-model-len', '32768', '--speculative-config', json.dumps(settings['speculative_config'])]
    with (RUN / f'server-{name}.log').open('x') as log:
        active = subprocess.Popen(cmd, env=environment(), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    status['server_pid'] = active.pid
    ready(alias, active)


def stop_server():
    global active
    if active is not None and active.poll() is None:
        active.terminate(); active.wait()
    active = None


def run(cmd, path):
    global work
    with Path(path).open('x') as log:
        work = subprocess.Popen(cmd, env=environment(), stdout=log, stderr=subprocess.STDOUT)
        status['worker_pid'] = work.pid
        code = work.wait()
    work = None
    if code: raise RuntimeError(f'Command failed ({code}); inspect {path}')


def read_legacy(suite, directory, mode):
    d = Path(directory)
    fixtures = list(map(json.loads, Path(f'datasets/{suite}/fixtures.jsonl').read_text().splitlines()))
    ids = {f['id'] for f in fixtures}
    metadata = json.loads((d / 'run.json').read_text())
    assert metadata['dataset_manifest']['fixtures_sha256'] == settings['fixture_hashes'][suite]
    assert metadata['tools'] == ('chat' if mode == 'chat' else 'none')
    check_completions(d, metadata, ids)
    completions = list(map(json.loads, (d / ('chat-text.jsonl' if mode == 'chat' else 'responses.jsonl')).read_text().splitlines()))
    if mode == 'submit' and (d / 'failures.jsonl').exists():
        completions += list(map(json.loads, (d / 'failures.jsonl').read_text().splitlines()))
    assert len(completions) == len(ids)
    assert all(r['finish_reason'] in ['stop', 'tool_calls', 'length'] for r in completions)
    grades = json.loads((d / 'graded/results.json').read_text())
    results = {r['task_id']: r for r in grades}
    assert len(results) == len(grades) and set(results) <= ids
    return {'directory': str(d), 'summary': summarize(fixtures, results),
            'finish_reasons': dict(Counter(r['finish_reason'] for r in completions))}


def legacy(suite, mode, alias):
    d = RUN / f'step128-{suite}-{mode}'; d.mkdir()
    config = Path(settings['config_template']).read_text().replace('qwen3-4b-compound-v3-step144', alias)
    (d / 'inference-config.toml').write_text(config)
    record(f'Evaluating retention: {suite}, {mode}')
    run(['target/release/btc-bench', 'run', '--dataset', 'datasets/' + suite, '--config', str(d / 'inference-config.toml'),
         '--model', 'human-sft', '--out', str(d), '--concurrency', '4', '--tools', 'chat' if mode == 'chat' else 'none'], d / 'run.log')
    run(['target/release/btc-bench', 'grade', '--dataset', 'datasets/' + suite, '--responses', str(d / 'responses.jsonl'),
         '--out', str(d / 'graded')], d / 'grade.log')
    return read_legacy(suite, d, mode)


def wallet(label, suite, mode, alias):
    d = RUN / f'{label}-{suite}-{mode}'
    record(f'Evaluating wallet: {label}, {suite}, {mode}')
    run(['python3', '-u', 'scripts/run_wallet_bench.py', '--dataset', 'datasets/' + suite, '--out', str(d),
         '--model', alias, '--mode', mode], RUN / f'run-{label}-{suite}-{mode}.log')
    run(['target/release/btc-wallet-bench', 'grade', '--dataset', 'datasets/' + suite,
         '--responses', str(d / 'responses.jsonl'), '--out', str(d / 'graded')], RUN / f'grade-{label}-{suite}-{mode}.log')
    rows = list(map(json.loads, (d / 'responses.jsonl').read_text().splitlines()))
    fixtures = list(map(json.loads, Path(f'datasets/{suite}/fixtures.jsonl').read_text().splitlines()))
    assert len(rows) == len(fixtures) and {r['task_id'] for r in rows} == {f['id'] for f in fixtures}
    assert all(r['finish_reason'] in ['stop', 'tool_calls', 'length'] for r in rows)
    return {'directory': str(d), 'summary': json.loads((d / 'graded/summary.json').read_text()),
            'finish_reasons': dict(Counter(r['finish_reason'] for r in rows))}


def main():
    global switched
    for p, sha in settings['hashes'].items(): assert digest(p) == sha, p
    for suite, sha in settings['fixture_hashes'].items(): assert digest(f'datasets/{suite}/fixtures.jsonl') == sha
    assert digest(Path(settings['parent']) / 'model.safetensors') == settings['parent_weight_sha256']
    assert digest(Path(settings['original_model']) / 'model.safetensors') == settings['original_weight_sha256']
    status['development']['parent'] = {suite: {mode: read_legacy(suite, directory, mode) for mode, directory in modes.items()}
                                        for suite, modes in settings['parent_runs'].items()}
    ready('qwen3-4b-think')
    pid = settings['original_pid']
    args = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
    assert b'qwen3-4b-think' in args and b'8010' in args and str(ROOT / settings['original_model']).encode() in args
    record('Stopping original server for wallet SFT')
    switched = True
    os.kill(pid, signal.SIGTERM)
    while Path(f'/proc/{pid}/stat').exists():
        if Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] == 'Z': break
        time.sleep(1)
    cmd = [settings['sft_python'], 'scripts/sft_train.py', '--data', settings['data'], '--model', settings['parent'],
           '--out', str(RUN), '--epochs', '1', '--learning-rate', '2e-5', '--warmup-steps', '6',
           '--gradient-accumulation-steps', '8', '--save-steps', '64', '--audit-data', str(RUN / 'loss-mask-audit.json')]
    (RUN / 'training-command.json').write_text(json.dumps(cmd, indent=2) + '\n')
    record('Auditing loss masks and training for 128 updates')
    run(cmd, RUN / 'train.log')
    assert 'TRAIN-DONE' in (RUN / 'train.log').read_text()
    assert len(json.loads((RUN / 'loss-mask-audit.json').read_text())) == 1024
    assert json.loads((RUN / 'checkpoint-128/trainer_state.json').read_text())['global_step'] == 128
    status['completed'].append('training')
    merged = RUN / 'merged-step128'
    record('Merging the fixed final checkpoint')
    run([settings['sft_python'], 'scripts/merge_adapter.py', '--base', settings['parent'],
         '--adapter', str(RUN / 'checkpoint-128'), '--out', str(merged)], RUN / 'merge-step128.log')
    status['trained_weight_sha256'] = digest(merged / 'model.safetensors')
    alias = 'qwen3-4b-wallet-sft-v1-step128'
    serve(merged, alias, 'step128')
    status['development']['step128'] = {'wallet-policy-v1': {}}
    for mode in ['chat', 'submit']:
        status['development']['step128']['wallet-policy-v1'][mode] = wallet('step128', 'wallet-policy-v1', mode, alias)
        record(f'Completed wallet development: {mode}')
    for suite in settings['retention_suites']:
        status['development']['step128'][suite] = {}
        for mode in ['chat', 'submit']:
            status['development']['step128'][suite][mode] = legacy(suite, mode, alias)
            record(f'Completed retention: {suite}, {mode}')
    # No checkpoint selection: step128 was fixed before any new inference.
    status['reserved']['step128'] = {}
    for mode in ['chat', 'submit']:
        status['reserved']['step128'][mode] = wallet('step128', 'wallet-sft-v1-reserved', mode, alias)
        record(f'Completed reserved: step128, {mode}')
    stop_server()
    alias = 'qwen3-4b-wallet-sft-v1-parent'
    serve(Path(settings['parent']), alias, 'parent-reserved')
    status['reserved']['parent'] = {}
    for mode in ['chat', 'submit']:
        status['reserved']['parent'][mode] = wallet('parent', 'wallet-sft-v1-reserved', mode, alias)
        record(f'Completed reserved: parent, {mode}')
    stop_server()


def interrupted(signum, frame):
    raise InterruptedError(f'Signal {signum}')


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, interrupted)
    try:
        main()
    except BaseException as error:
        status['error'] = repr(error); record('Wallet SFT experiment failed; inspect logs'); raise
    finally:
        if work is not None and work.poll() is None:
            work.terminate(); work.wait()
        stop_server()
        if switched:
            record('Restoring original server')
            serve(Path(settings['original_model']), 'qwen3-4b-think', 'original-restored')
            status['restored_pid'] = active.pid
    record('Wallet SFT training and evaluation complete; original server restored')
