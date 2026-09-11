"""Run the frozen broad SFT experiment, then restore the original server."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import urllib.request
from collections import Counter
from fractions import Fraction

ROOT = Path(__file__).resolve().parents[1]
os.chdir(ROOT)
sys.path.insert(0, str(ROOT / 'scripts'))
from compare_human_runs import check_completions, summarize

PLAN = Path('training/broad-v1-experiment.json')
RUN = Path('runs/sft-broad-v1')
STATUS = RUN / 'status.json'
settings = json.loads(PLAN.read_text())
active = None
switched = False
status = {'phase': 'Starting', 'completed': [], 'development': {}, 'reserved': {}}


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def record(phase):
    status['phase'] = phase
    temporary = STATUS.with_suffix('.tmp')
    temporary.write_text(json.dumps(status, indent=2) + '\n')
    temporary.replace(STATUS)
    print(phase, flush=True)


def environment():
    env = os.environ.copy()
    env.update(CUDA_VISIBLE_DEVICES='0', CUDA_DEVICE_ORDER='PCI_BUS_ID',
               CUDA_HOME='/usr/local/cuda-13.1', PYTHONUNBUFFERED='1')
    env['PATH'] = '/usr/local/cuda-13.1/bin:' + env['PATH']
    return env


def ready(alias, process=None):
    while True:
        if process is not None and process.poll() is not None:
            raise RuntimeError(f'Server exited: {process.returncode}')
        try:
            with urllib.request.urlopen('http://127.0.0.1:8010/v1/models', timeout=5) as response:
                models = [m['id'] for m in json.load(response)['data']]
            if models != [alias]:
                raise RuntimeError(f'Unexpected model on port 8010: {models}')
            return
        except OSError:
            time.sleep(2)


def start_server(path, alias, name):
    # Save the process handle before startup checks so finally can stop it.
    global active
    command = [settings['vllm_python'], '/home/ben/.local/bin/vllm', 'serve', str(ROOT / path),
               '--served-model-name', alias, '--port', '8010', '--enable-auto-tool-choice',
               '--tool-call-parser', 'hermes', '--reasoning-parser', 'qwen3',
               '--gpu-memory-utilization', '0.85', '--max-model-len', '32768',
               '--speculative-config', json.dumps(settings['inference_protocol']['speculative_config'])]
    with (RUN / f'server-{name}.log').open('x') as log:
        active = subprocess.Popen(command, env=environment(), stdout=log,
                                  stderr=subprocess.STDOUT, start_new_session=True)
    status['server_pid'] = active.pid
    ready(alias, active)


def stop_server():
    global active
    if active is not None and active.poll() is None:
        active.terminate()
        active.wait()
    active = None


def command_run(command, logpath):
    with Path(logpath).open('x') as log:
        subprocess.run(command, env=environment(), stdout=log, stderr=subprocess.STDOUT, check=True)


def read_results(suite, directory, mode):
    directory = Path(directory)
    fixtures_path = Path('datasets') / suite / 'fixtures.jsonl'
    assert digest(fixtures_path) == settings['fixture_hashes'][suite]
    fixtures = list(map(json.loads, fixtures_path.read_text().splitlines()))
    ids = {f['id'] for f in fixtures}
    metadata = json.loads((directory / 'run.json').read_text())
    assert metadata['dataset_manifest']['fixtures_sha256'] == settings['fixture_hashes'][suite]
    assert metadata['tools'] == ('chat' if mode == 'chat' else 'none')
    check_completions(directory, metadata, ids)
    completion_path = directory / ('chat-text.jsonl' if mode == 'chat' else 'responses.jsonl')
    completions = list(map(json.loads, completion_path.read_text().splitlines()))
    if mode != 'chat' and (directory / 'failures.jsonl').exists():
        completions += list(map(json.loads, (directory / 'failures.jsonl').read_text().splitlines()))
    assert len(completions) == len(ids), 'Duplicate completion or incomplete coverage'
    assert all(r['finish_reason'] in ('stop', 'tool_calls', 'length') for r in completions), 'Unknown finish reason'
    rows = json.loads((directory / 'graded/results.json').read_text())
    results = {r['task_id']: r for r in rows}
    assert len(results) == len(rows) and set(results) <= ids
    return {'directory': str(directory), 'summary': summarize(fixtures, results),
            'completions': len(completions),
            'finish_reasons': dict(Counter(r['finish_reason'] for r in completions))}


def evaluate(label, suite, mode, alias):
    directory = Path(f'runs/broad-v1-{label}-{suite}-{mode}')
    directory.mkdir()
    config = Path(settings['config_template']).read_text().replace(
        'qwen3-4b-compound-v3-step144', alias)
    (directory / 'inference-config.toml').write_text(config)
    record(f'Evaluating {label}: {suite}, {mode}')
    command_run(['target/release/btc-bench', 'run', '--dataset', 'datasets/' + suite,
                 '--config', str(directory / 'inference-config.toml'), '--model', 'human-sft',
                 '--out', str(directory), '--concurrency', '4', '--tools',
                 'chat' if mode == 'chat' else 'none'], directory / 'run.log')
    command_run(['target/release/btc-bench', 'grade', '--dataset', 'datasets/' + suite,
                 '--responses', str(directory / 'responses.jsonl'), '--out', str(directory / 'graded')],
                directory / 'grade.log')
    result = read_results(suite, directory, mode)
    status['completed'].append(f'{label}: {suite}, {mode}')
    return result


def selection_score(data):
    cells = [Fraction(cell['correct'], cell['total']) for modes in data.values()
             for run in modes.values() for cell in run['summary'].values()]
    assert len(cells) == 8
    return sum(cells) / len(cells)


def main():
    global switched
    freeze = json.loads(Path('training/broad-v1-freeze.json').read_text())
    for path, expected in freeze['hashes'].items():
        assert digest(path) == expected, path
    for path, expected in settings['code_hashes'].items():
        assert digest(path) == expected, path
    assert digest(Path(settings['parent']) / 'model.safetensors') == freeze['parent_weight_sha256']
    assert digest(Path(settings['original_model']) / 'model.safetensors') == settings['original_weight_sha256']
    for suite, expected in settings['fixture_hashes'].items():
        assert digest(Path('datasets') / suite / 'fixtures.jsonl') == expected
    status['development']['parent'] = {
        suite: {mode: read_results(suite, directory, mode) for mode, directory in modes.items()}
        for suite, modes in settings['parent_runs'].items()}
    ready('qwen3-4b-think')
    pid = settings['original_pid']
    args = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
    assert b'qwen3-4b-think' in args and b'8010' in args
    assert str(ROOT / settings['original_model']).encode() in args
    record('Stopping original model server for broad SFT')
    switched = True
    os.kill(pid, signal.SIGTERM)
    while Path(f'/proc/{pid}/stat').exists():
        if Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] == 'Z':
            break
        time.sleep(1)
    command = [settings['sft_python'], 'scripts/sft_train.py', '--data', settings['data'],
               '--model', settings['parent'], '--out', str(RUN), '--epochs', '2',
               '--learning-rate', str(settings['learning_rate']), '--warmup-steps', '26',
               '--gradient-accumulation-steps', '8', '--save-steps', '264',
               '--audit-data', str(RUN / 'loss-mask-audit.json')]
    (RUN / 'training-command.json').write_text(json.dumps(command, indent=2) + '\n')
    with (RUN / 'train.log').open('x') as log:
        training = subprocess.Popen(command, env=environment(), stdout=log, stderr=subprocess.STDOUT)
        status['training_pid'] = training.pid
        record('Auditing loss masks and training broad SFT for 528 updates')
        if training.wait() != 0:
            raise RuntimeError('Training failed; inspect train.log')
    assert 'TRAIN-DONE' in (RUN / 'train.log').read_text()
    assert len(json.loads((RUN / 'loss-mask-audit.json').read_text())) == 2112
    status['completed'].append('training')
    for step in settings['checkpoints']:
        label = f'step{step}'
        assert json.loads((RUN / f'checkpoint-{step}/trainer_state.json').read_text())['global_step'] == step
        merged = RUN / f'merged-{label}'
        record(f'Merging {label}')
        command_run([settings['sft_python'], 'scripts/merge_adapter.py', '--base', settings['parent'],
                     '--adapter', str(RUN / f'checkpoint-{step}'), '--out', str(merged)],
                    RUN / f'merge-{label}.log')
        hashes = {p.name: digest(p) for p in merged.glob('*.safetensors')}
        (RUN / f'weight-hashes-{label}.json').write_text(json.dumps(hashes, indent=2) + '\n')
        alias = 'qwen3-4b-broad-v1-' + label
        record(f'Loading {label}')
        start_server(merged, alias, label)
        status['development'][label] = {}
        for suite in settings['development_suites']:
            status['development'][label][suite] = {}
            for mode in ['chat', 'submit']:
                status['development'][label][suite][mode] = evaluate(label, suite, mode, alias)
                record(f'Completed {label}: {suite}, {mode}')
        stop_server()
    order = ['parent', 'step264', 'step528']
    scores = {label: selection_score(status['development'][label]) for label in order}
    chosen = max(order, key=scores.get)
    selection = {'selected': chosen, 'scores': {label: str(value) for label, value in scores.items()},
                 'rule': settings['selection'], 'reserved_inference_started': False,
                 'locked_at_unix': time.time(), 'plan_sha256': digest(PLAN)}
    with (RUN / 'selection.json').open('x') as stream:
        json.dump(selection, stream, indent=2)
        stream.write('\n')
    status['selected'] = chosen
    record(f'Selection locked: {chosen}. Starting reserved evaluation')
    for label in dict.fromkeys(['parent', chosen]):
        model = Path(settings['parent']) if label == 'parent' else RUN / ('merged-' + label)
        alias = 'qwen3-4b-broad-v1-' + label
        start_server(model, alias, 'reserved-' + label)
        status['reserved'][label] = {}
        for mode in ['chat', 'submit']:
            status['reserved'][label][mode] = evaluate(label, 'broad-v1-reserved', mode, alias)
            record(f'Completed reserved evaluation: {label}, {mode}')
        stop_server()


if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        status['error'] = repr(error)
        record('Experiment failed; inspect logs')
        raise
    finally:
        stop_server()
        if switched:
            record('Restoring original model server')
            start_server(Path(settings['original_model']), 'qwen3-4b-think', 'original-restored')
            status['restored_pid'] = active.pid
            record('Original model server restored')
    record('Broad SFT training and evaluation complete')
