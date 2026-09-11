"""Build the fixed 50/50 new-task and replay curriculum for compound v2."""
import hashlib
import json
from pathlib import Path
import random


def main():
    from transformers import AutoTokenizer
    paths = [Path('datasets/sft-compound-v2-train.jsonl'), Path('datasets/sft-train-think.jsonl')]
    new, old = [list(map(json.loads, p.read_text().splitlines())) for p in paths]
    assert len(new) == 192
    tok = AutoTokenizer.from_pretrained('runs/sft-qwen3-4b-think/merged')
    rng = random.Random(20260912)
    selected = []
    for name, count in [('submit_script', 128), ('submit_descriptor', 48), ('submit_identify', 16)]:
        candidates = [i for i, r in enumerate(old) if f'"name": "{name}"' in r['completion']]
        rng.shuffle(candidates)
        seen, picked = set(), []
        for i in candidates:
            identity = json.dumps(old[i], sort_keys=True)
            n = len(tok.encode(old[i]['prompt'] + old[i]['completion'], add_special_tokens=False))
            if n <= 4096 and identity not in seen:
                seen.add(identity)
                picked.append(i)
            if len(picked) == count:
                break
        assert len(picked) == count
        selected.extend(picked)
    rows = [dict(source='new', index=i, row=r) for i, r in enumerate(new)]
    rows += [dict(source='replay', index=i, row=old[i]) for i in selected]
    rng.shuffle(rows)
    lengths = [len(tok.encode(r['row']['prompt'] + r['row']['completion'], add_special_tokens=False)) for r in rows]
    assert len(rows) == 384 and max(lengths) <= 4096
    out = Path('datasets/sft-compound-v2-mixed.jsonl')
    with out.open('x') as f:
        for r in rows:
            f.write(json.dumps(r['row']) + '\n')
    metadata = dict(data=str(out), data_sha256=hashlib.sha256(out.read_bytes()).hexdigest(),
                    sources={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
                    seed=20260912, new=192, replay=192, replay_script=128, replay_tree=48,
                    replay_identify=16, max_tokens=max(lengths), mean_tokens=sum(lengths)/len(lengths),
                    row_provenance=[{k: v for k, v in r.items() if k != 'row'} for r in rows])
    with Path('training/compound-v2-mix.json').open('x') as f:
        json.dump(metadata, f, indent=2)
        f.write('\n')
    print(json.dumps({k: v for k, v in metadata.items() if k != 'row_provenance'}, indent=2))


if __name__ == '__main__':
    main()
