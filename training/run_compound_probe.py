"""Run the frozen training-only probe and restore the original local server."""
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
settings = json.loads(Path('training/rl-compound-v3-probe-v1.json').read_text())
out = Path('runs/rl-compound-v3-probe-v1')
out.mkdir()
status = {}
active = reward = None
switched = False


def record(phase):
    status['phase'] = phase
    (out / 'status.json').write_text(json.dumps(status, indent=2) + '\n')
    print(phase, flush=True)


def get(url):
    with urllib.request.urlopen(url, timeout=5) as response:
        return json.load(response)


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        process.wait()


def serve(command, name):
    env = os.environ.copy()
    env.update(CUDA_HOME='/usr/local/cuda-13.1', CUDA_VISIBLE_DEVICES='0', CUDA_DEVICE_ORDER='PCI_BUS_ID')
    env['PATH'] = '/usr/local/cuda-13.1/bin:' + env['PATH']
    with (out / f'{name}.log').open('x') as log:
        return subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)


def ready(process, alias):
    while True:
        if process.poll() is not None:
            raise RuntimeError(f'Model server exited with {process.returncode}')
        try:
            assert [m['id'] for m in get('http://127.0.0.1:8010/v1/models')['data']] == [alias]
            return
        except OSError:
            time.sleep(2)


try:
    data = Path(settings['data'])
    assert hashlib.sha256(data.read_bytes()).hexdigest() == settings['data_sha256']
    pid = settings['original_server_pid']
    original_command = [s.decode() for s in Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0') if s]
    assert str(ROOT / settings['original_model']) in original_command
    assert 'qwen3-4b-think' in original_command and '8010' in original_command
    assert [m['id'] for m in get('http://127.0.0.1:8010/v1/models')['data']] == ['qwen3-4b-think']
    with (out / 'original-command.json').open('x') as f:
        json.dump(original_command, f)
    with (out / 'reward.log').open('x') as log:
        reward = subprocess.Popen(['target/release/btc-bench', 'reward-serve', '--bind', '127.0.0.1:9901'],
                                  stdout=log, stderr=subprocess.STDOUT)
    while True:
        if reward.poll() is not None:
            raise RuntimeError('Probe reward server exited')
        try:
            get('http://127.0.0.1:9901/health')
            break
        except OSError:
            time.sleep(1)
    fixture = json.loads(json.loads(data.read_text().splitlines()[0])['task_json'])
    items = [{'task': fixture, 'answer': {'task': 'script', 'script': s}}
             for s in [fixture['reference_script_hex'], 'OP_0']]
    request = urllib.request.Request('http://127.0.0.1:9901/reward/batch',
                                    data=json.dumps({'items': items}).encode(),
                                    headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(request) as response:
        controls = json.load(response)
    assert [r['shaped'] for r in controls] == [1, 0], controls
    (out / 'reward-controls.json').write_text(json.dumps(controls, indent=2))
    record('Loading checkpoint 144 for training-only reward probe')
    switched = True
    os.kill(pid, signal.SIGTERM)
    while Path(f'/proc/{pid}/stat').exists():
        if Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] == 'Z':
            break
        time.sleep(1)
    alias = 'qwen3-4b-compound-v3-step144'
    command = [str(ROOT / settings['model']) if x == str(ROOT / settings['original_model'])
               else alias if x == 'qwen3-4b-think' else x for x in original_command]
    active = serve(command, 'model')
    ready(active, alias)
    record('Sampling 48 training questions with eight completions each')
    command = [sys.executable, 'scripts/rl_probe.py', '--data', str(data), '--model', alias,
               '--reward-url', 'http://127.0.0.1:9901/reward/batch', '--tasks', '48', '--k', '8',
               '--seed', '7', '--max-completion-length', '4096', '--out', str(out / 'raw.jsonl')]
    (out / 'probe-command.json').write_text(json.dumps(command, indent=2))
    with (out / 'probe.log').open('x') as log:
        subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
    status['probe_complete'] = True
except BaseException as error:
    status['error'] = repr(error)
    record('Probe failed; saved outputs are retained')
    raise
finally:
    stop(active)
    stop(reward)
    if switched:
        record('Restoring original model server')
        restored = serve(original_command, 'original-restored')
        status['restored_pid'] = restored.pid
        ready(restored, 'qwen3-4b-think')
        record('Original model server restored')
record('Probe complete')
