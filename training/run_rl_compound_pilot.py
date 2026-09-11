"""Run the fixed RL/SFT comparison, preserve outputs, and restore local serving."""
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
from compare_human_runs import check_completions, correct

settings = json.loads(Path('training/rl-compound-v3-pilot-v1.json').read_text())
prior = json.loads(Path('training/compound-v3-experiment.json').read_text())
paths = {'rl': Path('runs/rl-compound-v3-pilot-v1'), 'sft': Path('runs/sft-compound-v3-rl-control-v1')}
for p in paths.values():
    p.mkdir()
status = {'completed': [], 'scores': {}}
active = reward = None
switched = False
env = os.environ.copy()
env.update(CUDA_VISIBLE_DEVICES='0', CUDA_DEVICE_ORDER='PCI_BUS_ID', CUDA_HOME='/usr/local/cuda-13.1', PYTHONUNBUFFERED='1')
env['PATH'] = '/usr/local/cuda-13.1/bin:' + env['PATH']


def record(phase):
    status['phase'] = phase
    Path('runs/rl-compound-v3-pilot-v1-status.json').write_text(json.dumps(status, indent=2) + '\n')
    print(phase, flush=True)


def stop(p):
    if p is not None and p.poll() is None:
        p.terminate()
        p.wait()


def ready(p, alias):
    while True:
        if p.poll() is not None:
            raise RuntimeError(f'Model server exited with {p.returncode}')
        try:
            with urllib.request.urlopen('http://127.0.0.1:8010/v1/models', timeout=5) as r:
                assert [m['id'] for m in json.load(r)['data']] == [alias]
            return
        except OSError:
            time.sleep(2)


def launch(command, logfile):
    with Path(logfile).open('x') as log:
        return subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)


def run(command, logfile):
    Path(str(logfile) + '.command.json').write_text(json.dumps(command, indent=2) + '\n')
    p = launch(command, logfile)
    status['active_command_pid'] = p.pid
    record(status['phase'])
    if p.wait() != 0:
        raise RuntimeError(f'Command failed; inspect {logfile}')


def evaluate(arm, part, suite, mode, alias):
    directory = Path(f'runs/rl-compound-v3-pilot-v1-{arm}-{part}-{mode}')
    directory.mkdir()
    fixtures = Path('datasets') / suite / 'fixtures.jsonl'
    assert hashlib.sha256(fixtures.read_bytes()).hexdigest() == prior['fixture_hashes'][suite]
    template = Path('runs/compound-v3-step144-development-submit/inference-config.toml').read_text()
    assert 'qwen3-4b-compound-v3-step144' in template
    (directory / 'inference-config.toml').write_text(template.replace('qwen3-4b-compound-v3-step144', alias))
    record(f'Evaluating {arm} {part} {mode}')
    run(['target/release/btc-bench', 'run', '--dataset', str(fixtures.parent), '--config',
         str(directory / 'inference-config.toml'), '--model', 'human-sft', '--out', str(directory),
         '--concurrency', '4', '--tools', 'chat' if mode == 'chat' else 'none'], directory / 'run.log')
    ids = {json.loads(l)['id'] for l in fixtures.read_text().splitlines()}
    metadata = json.loads((directory / 'run.json').read_text())
    assert metadata['dataset_manifest']['fixtures_sha256'] == prior['fixture_hashes'][suite]
    check_completions(directory, metadata, ids)
    run(['target/release/btc-bench', 'grade', '--dataset', str(fixtures.parent), '--responses',
         str(directory / 'responses.jsonl'), '--out', str(directory / 'graded')], directory / 'grade.log')
    status['scores'][f'{arm}/{part}/{mode}'] = sum(correct(r) for r in json.loads((directory / 'graded/results.json').read_text()))
    status['completed'].append(f'{arm}/{part}/{mode}')


try:
    assert hashlib.sha256(Path(settings['data']).read_bytes()).hexdigest() == settings['data_sha256']
    probe = json.loads(Path('runs/rl-compound-v3-probe-v1/status.json').read_text())
    assert probe['phase'] == 'Probe complete'
    pid = probe['restored_pid']
    original_command = [s.decode() for s in Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0') if s]
    original_path = str(ROOT / prior['parent'])
    assert original_path in original_command and 'qwen3-4b-think' in original_command and '8010' in original_command
    reward = launch(['target/release/btc-bench', 'reward-serve', '--bind', '127.0.0.1:9901'], paths['rl'] / 'reward.log')
    while True:
        assert reward.poll() is None
        try:
            with urllib.request.urlopen('http://127.0.0.1:9901/health', timeout=5):
                break
        except OSError:
            time.sleep(1)
    record('Stopping original server for RL training')
    switched = True
    os.kill(pid, signal.SIGTERM)
    while Path(f'/proc/{pid}/stat').exists():
        if Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] == 'Z':
            break
        time.sleep(1)
    record('Training RL arm for 32 updates')
    run(['/home/ben/.local/share/venvs/btc-rl/bin/python', 'scripts/rl_train.py',
         '--data', settings['data'], '--model', settings['parent'], '--out', str(paths['rl']),
         '--steps', '32', '--warmup-steps', '3', '--save-steps', '16',
         '--reward-url', 'http://127.0.0.1:9901/reward/batch',
         '--rollout-log', str(paths['rl'] / 'rollouts.jsonl')], paths['rl'] / 'train.log')
    assert 'RL-DONE' in (paths['rl'] / 'train.log').read_text()
    status['completed'].append('rl training')
    stop(reward)
    reward = None
    record('Preparing the SFT control from actual sampled question counts')
    run([sys.executable, 'training/prepare_rl_sft_control.py'], paths['sft'] / 'prepare.log')
    record('Training matched SFT control for 32 updates')
    run(['/home/ben/.local/share/venvs/sft/bin/python', 'scripts/sft_train.py',
         '--data', 'datasets/sft-compound-v3-rl-control-v1.jsonl', '--model', settings['parent'],
         '--out', str(paths['sft']), '--epochs', '1', '--learning-rate', '1e-5', '--lora-dropout', '0',
         '--warmup-steps', '3', '--gradient-accumulation-steps', '8', '--save-steps', '16',
         '--audit-data', str(paths['sft'] / 'loss-mask-audit.json')], paths['sft'] / 'train.log')
    assert json.loads((paths['sft'] / 'checkpoint-32/trainer_state.json').read_text())['global_step'] == 32
    status['completed'].append('sft training')
    for arm, path in paths.items():
        record(f'Merging {arm} final checkpoint')
        run(['/home/ben/.local/share/venvs/sft/bin/python', 'scripts/merge_adapter.py', '--base', settings['parent'],
             '--adapter', str(path / 'final'), '--out', str(path / 'merged')], path / 'merge.log')
        hashes = {}
        for p in (path / 'merged').glob('*.safetensors'):
            with p.open('rb') as f:
                hashes[p.name] = hashlib.file_digest(f, 'sha256').hexdigest()
        (path / 'weight-hashes.json').write_text(json.dumps(hashes, indent=2) + '\n')
        alias = f'qwen3-4b-compound-v3-pilot-{arm}'
        command = [str(ROOT / path / 'merged') if x == original_path else alias if x == 'qwen3-4b-think' else x
                   for x in original_command]
        record(f'Loading {arm} for evaluation')
        active = launch(command, path / 'server.log')
        ready(active, alias)
        for part, suite in [('development', 'compound-v2-validation'), ('transfer', 'compound-v3-fresh'), ('human', 'human-v2')]:
            for mode in ['chat', 'submit']:
                evaluate(arm, part, suite, mode, alias)
        stop(active)
        active = None
except BaseException as error:
    status['error'] = repr(error)
    record('Pilot failed; saved artifacts are retained')
    raise
finally:
    stop(active)
    stop(reward)
    if switched:
        record('Restoring original model server')
        restored = launch(original_command, paths['rl'] / 'original-restored.log')
        status['restored_pid'] = restored.pid
        ready(restored, 'qwen3-4b-think')
        record('Original model server restored')
record('RL and SFT pilot complete')
