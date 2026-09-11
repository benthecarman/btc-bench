"""Match the SFT control's question counts to the actual RL rollout groups."""
from collections import Counter
import hashlib
import json
from pathlib import Path


def rows(path):
    return [json.loads(l) for l in Path(path).read_text().splitlines()]


def main():
    settings = json.loads(Path('training/rl-compound-v3-pilot-v1.json').read_text())
    prompts = rows('datasets/rl-compound-v3-train.jsonl')
    targets = rows('datasets/sft-compound-v3-submit.jsonl')
    assert len(prompts) == len(targets) == 288
    references = {}
    for prompt, target in zip(prompts, targets):
        assert prompt['prompt'] == target['prompt']
        references[prompt['task_id']] = target
    groups = rows('runs/rl-compound-v3-pilot-v1/rollouts.jsonl')
    assert [g['step'] for g in groups] == list(range(settings['steps']))
    counts = Counter()
    examples = []
    for group in groups:
        ids = group['task_ids']
        assert len(ids) == settings['k'] and len(set(ids)) == 1
        assert len(group['completions']) == len(ids)
        for tid in ids:
            examples.append(references[tid])
            counts[tid] += 1
    assert len(examples) == settings['steps'] * settings['k']
    path = Path('datasets/sft-compound-v3-rl-control-v1.jsonl')
    with path.open('x') as f:
        for row in examples:
            f.write(json.dumps(row) + '\n')
    report = dict(data=str(path), data_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                  rows=len(examples), question_counts=dict(counts),
                  rollout_source_sha256=hashlib.sha256(Path('runs/rl-compound-v3-pilot-v1/rollouts.jsonl').read_bytes()).hexdigest())
    Path('training/rl-compound-v3-control-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
