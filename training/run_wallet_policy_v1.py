"""Run the frozen wallet pilot against checkpoint 528 and restore the server."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
os.chdir(ROOT)
PLAN = Path('evals/wallet-policy-v1-experiment.json')
settings = json.loads(PLAN.read_text())
OUT = Path(settings['out'])
active = None
switched = False
status = {'phase': 'Starting', 'completed': []}


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def record(phase):
    status['phase'] = phase
    temp = OUT / 'status.tmp'
    temp.write_text(json.dumps(status, indent=2) + '\n')
    temp.replace(OUT / 'status.json')
    print(phase, flush=True)


def ready(alias, process=None):
    while True:
        if process is not None and process.poll() is not None:
            raise RuntimeError(f'Server exited: {process.returncode}')
        try:
            with urllib.request.urlopen('http://127.0.0.1:8010/v1/models', timeout=5) as response:
                assert [m['id'] for m in json.load(response)['data']] == [alias]
            return
        except OSError:
            time.sleep(2)


def serve(model, alias, name):
    global active
    env = os.environ.copy()
    env.update(CUDA_VISIBLE_DEVICES='0', CUDA_DEVICE_ORDER='PCI_BUS_ID', CUDA_HOME='/usr/local/cuda-13.1')
    env['PATH'] = '/usr/local/cuda-13.1/bin:' + env['PATH']
    command = ['/home/ben/.local/share/venvs/vllm/bin/python', '/home/ben/.local/bin/vllm',
               'serve', str(ROOT / model), '--served-model-name', alias, '--port', '8010',
               '--enable-auto-tool-choice', '--tool-call-parser', 'hermes', '--reasoning-parser', 'qwen3',
               '--gpu-memory-utilization', '0.85', '--max-model-len', '32768', '--speculative-config',
               json.dumps({'method': 'ngram', 'num_speculative_tokens': 8, 'prompt_lookup_max': 8, 'prompt_lookup_min': 2})]
    with (OUT / f'server-{name}.log').open('x') as log:
        active = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    status['server_pid'] = active.pid
    ready(alias, active)


def stop():
    global active
    if active is not None and active.poll() is None:
        active.terminate(); active.wait()
    active = None


def main():
    global switched
    for path, expected in settings['hashes'].items():
        assert digest(path) == expected, path
    assert digest(Path(settings['model']) / 'model.safetensors') == settings['model_weight_sha256']
    assert digest(Path(settings['original_model']) / 'model.safetensors') == settings['original_weight_sha256']
    ready('qwen3-4b-think')
    pid = settings['original_pid']
    cmd = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
    assert b'qwen3-4b-think' in cmd and str(ROOT / settings['original_model']).encode() in cmd and b'8010' in cmd
    record('Loading checkpoint 528 for wallet evaluation')
    switched = True
    os.kill(pid, signal.SIGTERM)
    while Path(f'/proc/{pid}/stat').exists():
        if Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] == 'Z': break
        time.sleep(1)
    alias = 'qwen3-4b-wallet-v1-step528'
    serve(Path(settings['model']), alias, 'step528')
    fixtures = list(map(json.loads, Path(settings['dataset']).joinpath('fixtures.jsonl').read_text().splitlines()))
    expected = {f['id'] for f in fixtures}
    status['results'] = {}
    for mode in ['chat', 'submit']:
        directory = OUT / mode
        record(f'Evaluating 40 wallet questions: {mode}')
        with (OUT / f'run-{mode}.log').open('x') as log:
            subprocess.run(['python3', '-u', 'scripts/run_wallet_bench.py', '--dataset', settings['dataset'],
                            '--out', str(directory), '--model', alias, '--mode', mode], check=True, stdout=log, stderr=subprocess.STDOUT)
        records = list(map(json.loads, (directory / 'responses.jsonl').read_text().splitlines()))
        assert len(records) == len(expected) and {r['task_id'] for r in records} == expected
        assert all(r.get('finish_reason') in ['stop', 'tool_calls', 'length'] for r in records)
        with (OUT / f'grade-{mode}.log').open('x') as log:
            subprocess.run(['target/release/btc-wallet-bench', 'grade', '--dataset', settings['dataset'],
                            '--responses', str(directory / 'responses.jsonl'), '--out', str(directory / 'graded')],
                           check=True, stdout=log, stderr=subprocess.STDOUT)
        status['results'][mode] = json.loads((directory / 'graded/summary.json').read_text())
        status['completed'].append(mode)
        record(f'Completed wallet evaluation: {mode}')


if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        status['error'] = repr(error)
        record('Wallet experiment failed; inspect logs')
        raise
    finally:
        stop()
        if switched:
            record('Restoring original model server')
            serve(Path(settings['original_model']), 'qwen3-4b-think', 'original-restored')
            status['restored_pid'] = active.pid
    record('Wallet evaluation complete; original server restored')
