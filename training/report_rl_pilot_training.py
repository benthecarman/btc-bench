"""Check actual training counts, finite metrics, and the SFT exposure control."""
from collections import Counter
import json
import math
from pathlib import Path


def main():
    rollout_path = Path('runs/rl-compound-v3-pilot-v1/rollouts.jsonl')
    groups = [json.loads(l) for l in rollout_path.read_text().splitlines()]
    assert [g['step'] for g in groups] == list(range(32))
    counts = Counter(tid for g in groups for tid in g['task_ids'])
    control = json.loads(Path('training/rl-compound-v3-control-v1.json').read_text())
    assert dict(counts) == control['question_counts'] and sum(counts.values()) == control['rows'] == 256
    before = {r['task_id']: r['correct'] / 8 for r in json.loads(Path('runs/rl-compound-v3-probe-v1/scores.json').read_text())}
    report = dict(groups=len(groups), samples=sum(counts.values()), unique_questions=len(counts),
                  mixed_groups=sum(len({r['shaped'] for r in g['rewards']}) > 1 for g in groups),
                  correct=sum(r['shaped'] for g in groups for r in g['rewards']),
                  truncated=sum(sum(g['truncated']) for g in groups),
                  baseline_probe_mean_with_training_question_weights=sum(before[tid] * n for tid, n in counts.items()) / 256,
                  question_counts=dict(counts), training={})
    for arm, path in [('rl', 'runs/rl-compound-v3-pilot-v1'), ('sft', 'runs/sft-compound-v3-rl-control-v1')]:
        state = json.loads(Path(path, 'checkpoint-32/trainer_state.json').read_text())
        assert state['global_step'] == 32
        history = state['log_history']
        ranges = {}
        for key in ['loss', 'grad_norm', 'kl', 'entropy']:
            values = [float(r[key]) for r in history if key in r]
            assert all(math.isfinite(v) for v in values)
            if values:
                ranges[key] = dict(min=min(values), max=max(values))
        report['training'][arm] = dict(steps=32, metric_ranges=ranges)
    audit = json.loads(Path('runs/sft-compound-v3-rl-control-v1/loss-mask-audit.json').read_text())
    assert len(audit) == 256
    report['sft_verified_loss_masks'] = len(audit)
    report['limits'] = 'Training and probe rewards use different sampled answers. Training reward is not an evaluation score. Matching counts does not match token count or GPU time.'
    Path('runs/rl-compound-v3-pilot-v1-training-summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
