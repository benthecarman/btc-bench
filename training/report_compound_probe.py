"""Regrade preserved samples and freeze the training subset for the RL pilot."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from rl_probe import answer_from_sample, score


def main():
    directory = Path('runs/rl-compound-v3-probe-v1')
    assert json.loads((directory / 'status.json').read_text()).get('probe_complete')
    records = [json.loads(l) for l in (directory / 'raw.jsonl').read_text().splitlines()]
    assert len(records) == 48
    scored = []
    with (directory / 'regrade-server.log').open('x') as log:
        process = subprocess.Popen(['target/release/btc-bench', 'reward-serve', '--bind', '127.0.0.1:9902'],
                                   stdout=log, stderr=subprocess.STDOUT)
    try:
        while True:
            assert process.poll() is None
            try:
                with urllib.request.urlopen('http://127.0.0.1:9902/health', timeout=5):
                    break
            except OSError:
                time.sleep(1)
        for record in records:
            assert not record.get('transport_error') and len(record['completions']) == 8
            assert all(c['finish_reason'] is not None for c in record['completions'])
            task = json.loads(record['row']['task_json'])
            results = score([{'task': task, 'answer': answer_from_sample(c, True)}
                             for c in record['completions']], 'http://127.0.0.1:9902/reward/batch')
            rewards = [r['shaped'] for r in results]
            assert set(rewards) <= {0, 1}
            scored.append(dict(task_id=record['row']['task_id'], correct=sum(rewards),
                               rewards=results, finishes=[c['finish_reason'] for c in record['completions']]))
    finally:
        process.terminate()
        process.wait()
    (directory / 'scores.json').write_text(json.dumps(scored, indent=2) + '\n')
    buckets = Counter('all correct' if r['correct'] == 8 else 'all wrong' if r['correct'] == 0 else 'mixed'
                      for r in scored)
    chosen = {r['task_id'] for r in scored if 0 < r['correct'] < 8}
    summary = dict(groups=dict(buckets), correct=sum(r['correct'] for r in scored), samples=384,
                   finish_reasons=dict(Counter(f for r in scored for f in r['finishes'])),
                   selected_task_ids=sorted(chosen))
    (directory / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    assert len(chosen) >= 4, 'Too few mixed groups for the planned pilot'
    output = Path('datasets/rl-compound-v3-pilot-v1.jsonl')
    with output.open('x') as f:
        for record in records:
            if record['row']['task_id'] in chosen:
                f.write(json.dumps(record['row']) + '\n')
    settings = dict(parent='runs/sft-compound-v3/merged-step144', data=str(output),
                    data_sha256=hashlib.sha256(output.read_bytes()).hexdigest(),
                    probe_summary=str(directory / 'summary.json'), steps=32, learning_rate=1e-5,
                    warmup_steps=3, gradient_accumulation_steps=8, batch_size=1, k=8,
                    completion_budget=4096, save_steps=16, seed=7,
                    lora_rank=64, lora_alpha=128, lora_dropout=0, reward='unshaped correctness',
                    kl_beta=0.02, loss_type='dapo', scale_rewards='group',
                    control='Same parent, optimizer updates, learning rate, LoRA settings, and exact sampled question counts; eight reference completions per RL group. No replay in either arm.',
                    selection='Compare fixed final checkpoints with the unchanged parent; no checkpoint selection from evaluation.',
                    evaluation='Existing compound-v2-validation, compound-v3-fresh, and human-v2 in chat and submit. These are observed evaluation sets, not an untouched final test.',
                    limitations='Single seed; small selected training pool; matched question and completion counts, not token count or GPU compute. SFT shuffles reference rows independently.')
    with Path('training/rl-compound-v3-pilot-v1.json').open('x') as f:
        json.dump(settings, f, indent=2)
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
