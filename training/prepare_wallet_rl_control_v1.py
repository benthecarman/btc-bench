"""Match the RL arm's actual question/completion counts with reference SFT."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import random


def main():
    root=Path('runs/rl-wallet-v1')
    gold={r['task_id']:r['row'] for r in map(json.loads,Path('datasets/rl-wallet-v1-gold.jsonl').read_text().splitlines())}
    rollouts=list(map(json.loads,(root/'rl/rollouts.jsonl').read_text().splitlines()))
    counts=Counter()
    for r in rollouts:
        assert len(r['task_ids'])==len(r['completions'])==8
        counts.update(r['task_ids'])
    assert len(rollouts)==32 and sum(counts.values())==256
    data=[gold[tid] for tid,n in counts.items() for _ in range(n)]
    random.Random(7).shuffle(data)
    path=Path('datasets/sft-wallet-rl-control-v1.jsonl')
    with path.open('x') as f:
        for row in data:f.write(json.dumps(row)+'\n')
    with (root/'sft-control-data.json').open('x') as f:
        json.dump(dict(rows=len(data),counts=dict(counts),sha256=hashlib.sha256(path.read_bytes()).hexdigest()),f,indent=2);f.write('\n')
    print('Prepared 256 matched reference completions for 32 SFT updates')


if __name__=='__main__':main()
