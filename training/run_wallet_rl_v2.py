"""Run longer RL and RL with SFT replay, then evaluate fixed checkpoints."""
import json
import os
from pathlib import Path
import signal
import sys
import time
import urllib.request

import run_wallet_rl_v1 as runner

runner.settings = json.loads(Path('training/wallet-rl-v2-experiment.json').read_text())
runner.OUT = Path(runner.settings['out'])
runner.status = {'phase': 'Starting', 'completed': [], 'results': {}, 'weights': {}}
settings, OUT, status = runner.settings, runner.OUT, runner.status


def train(arm, steps):
    path = OUT / arm
    path.mkdir()
    cmd = [settings['rl_python'], 'scripts/rl_train.py', '--data', settings['rl_data'],
           '--model', settings['parent'], '--out', str(path), '--steps', str(steps),
           '--warmup-steps', '8', '--save-steps', str(64 if steps == 256 else steps),
           '--reward-url', 'http://127.0.0.1:9902/reward/batch', '--rollout-log', str(path/'rollouts.jsonl')]
    if arm != 'rl':
        cmd += ['--sft-replay', settings['replay_data'], '--sft-replay-weight', '0.1']
    runner.record(f'Training {arm}: {steps} updates')
    started = time.time()
    runner.run(cmd, path/'train.log')
    status.setdefault('training_seconds', {})[arm] = time.time() - started
    assert 'RL-DONE' in (path/'train.log').read_text()
    state = json.loads((path/f'checkpoint-{steps}/trainer_state.json').read_text())
    assert state['global_step'] == steps
    rollouts = list(map(json.loads, (path/'rollouts.jsonl').read_text().splitlines()))
    assert len(rollouts) == steps and all(len(r['rewards']) == 8 for r in rollouts)
    if arm != 'rl':
        replay = list(map(json.loads, (path/'sft-replay.jsonl').read_text().splitlines()))
        assert [r['step'] for r in replay] == list(range(1, steps+1))
        assert [r['index'] for r in replay] == list(range(steps))
        audit = json.loads((path/'sft-mask-audit.json').read_text())
        assert len(audit) == 256
        for row in audit:
            boundary = row['prompt_tokens']
            assert row['labels'][:boundary] == [-100]*boundary
            assert row['labels'][boundary:] == row['input_ids'][boundary:]
    status['completed'].append(arm + ' training')
    runner.record(f'Completed {arm} training')


def main():
    for path, sha in settings['hashes'].items():
        assert runner.digest(path) == sha, path
    for prefix in ['parent', 'original']:
        path = settings['parent'] if prefix == 'parent' else settings['original_model']
        assert runner.digest(Path(path)/'model.safetensors') == settings[prefix+'_weight_sha256']
    runner.ready('qwen3-4b-think')
    pid = settings['original_pid']
    command = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
    assert b'qwen3-4b-think' in command and b'8010' in command
    assert str(runner.ROOT/settings['original_model']).encode() in command
    runner.reward = runner.launch(['target/release/btc-bench', 'reward-serve', '--bind', '127.0.0.1:9902'], OUT/'reward.log')
    while True:
        assert runner.reward.poll() is None
        try:
            with urllib.request.urlopen('http://127.0.0.1:9902/health', timeout=5):
                break
        except OSError:
            time.sleep(1)
    runner.run(['python3', 'training/check_wallet_reward_http.py', '--url', 'http://127.0.0.1:9902/reward/batch'], OUT/'reward-preflight.log')
    runner.record('Stopping original model server')
    runner.switched = True
    os.kill(pid, signal.SIGTERM)
    while Path(f'/proc/{pid}/stat').exists():
        if Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] == 'Z':
            break
        time.sleep(1)
    # Real-model preflight checks memory, backward scaling and update frequency.
    # Its weights are discarded; both measured arms start from the same parent.
    train('mixed-preflight', 2)
    for arm in ['rl', 'mixed']:
        train(arm, 256)
    runner.stop(runner.reward)
    runner.reward = None
    for step in [64, 128, 256]:
        for arm in ['rl', 'mixed']:
            label = f'{arm}-step{step}'
            merged = OUT/arm/f'merged-step{step}'
            runner.record(f'Merging {label}')
            runner.run([settings['sft_python'], 'scripts/merge_adapter.py', '--base', settings['parent'],
                '--adapter', str(OUT/arm/f'checkpoint-{step}'), '--out', str(merged)], OUT/f'merge-{label}.log')
            status['weights'][label] = dict(path=str(merged), sha256=runner.digest(merged/'model.safetensors'))
            alias = 'qwen3-4b-wallet-rl-v2-' + label
            runner.serve(merged, alias, label)
            status['results'][label] = {}
            for suite in settings['evaluation_suites']:
                status['results'][label][suite] = {}
                for mode in ['chat', 'submit']:
                    status['results'][label][suite][mode] = runner.evaluate(label, suite, mode, alias)
                    status['completed'].append(f'{label}/{suite}/{mode}')
                    runner.record(f'Completed {label}: {suite}, {mode}')
            runner.stop(runner.active)
            runner.active = None


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, runner.interrupted)
    try:
        main()
    except BaseException as error:
        status['error'] = repr(error)
        runner.record('Longer RL comparison failed; inspect saved logs')
        raise
    finally:
        runner.stop(runner.work)
        runner.stop(runner.active)
        runner.stop(runner.reward)
        if runner.switched:
            runner.record('Restoring original model server')
            runner.serve(Path(settings['original_model']), 'qwen3-4b-think', 'original-restored')
            status['restored_pid'] = runner.active.pid
    runner.record('Longer RL comparison complete; original server available')
