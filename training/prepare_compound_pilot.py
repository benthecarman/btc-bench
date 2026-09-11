"""Build the small approved compound-approval SFT pilot with replay."""
import hashlib
import json
from pathlib import Path
import random


def main():
    from transformers import AutoTokenizer

    parent = 'runs/sft-qwen3-4b-think/merged'
    new_path = Path('datasets/sft-compound-approvals-v1-samples.jsonl')
    old_path = Path('datasets/sft-train-think.jsonl')
    out = Path('datasets/sft-compound-approvals-v1-pilot.jsonl')
    manifest = Path('training/compound-approvals-v1-pilot.json')
    if out.exists() or manifest.exists():
        raise ValueError('Pilot output exists; preserve the recorded experiment')
    new = list(map(json.loads, new_path.read_text().splitlines()))
    old = list(map(json.loads, old_path.read_text().splitlines()))
    assert len(new) == 12
    tok = AutoTokenizer.from_pretrained(parent)
    rng = random.Random(20260908)
    selected = []
    # Preserve script, tree, and identification skills in a small replay sample.
    for tool, count in [('submit_script', 88), ('submit_descriptor', 48),
                        ('submit_identify', 8)]:
        candidates = [i for i, row in enumerate(old)
                      if f'"name": "{tool}"' in row['completion']]
        rng.shuffle(candidates)
        picked = []
        seen = set()
        for i in candidates:
            row = old[i]
            identity = json.dumps(row, sort_keys=True)
            n = len(tok.encode(row['prompt'] + row['completion'], add_special_tokens=False))
            if n <= 4096 and identity not in seen:
                picked.append(i)
                seen.add(identity)
            if len(picked) == count:
                break
        assert len(picked) == count, (tool, len(picked))
        selected.extend(picked)
    rows = [dict(row=new[i], source='new', index=i) for _ in range(4) for i in range(12)]
    rows += [dict(row=old[i], source='replay', index=i) for i in selected]
    rng.shuffle(rows)
    lengths = [len(tok.encode(r['row']['prompt'] + r['row']['completion'],
                              add_special_tokens=False)) for r in rows]
    assert len(rows) == 192 and max(lengths) <= 4096
    with out.open('x') as f:
        for r in rows:
            f.write(json.dumps(r['row']) + '\n')
    settings = dict(
        parent=parent, data=str(out),
        data_sha256=hashlib.sha256(out.read_bytes()).hexdigest(),
        sources={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in [new_path, old_path]},
        data_seed=20260908, trainer_seed=7, rows=192, new_unique=12,
        new_repetitions=4, replay_unique=144, replay_script=88,
        replay_tree=48, replay_identify=8, max_tokens=max(lengths),
        mean_tokens=sum(lengths)/len(lengths), epochs=1, optimizer_steps=24,
        learning_rate=1e-5, warmup_steps=3, gradient_accumulation_steps=8,
        save_steps=10, training_device='GPU 1, RTX 5060 Ti',
        out='runs/sft-compound-approvals-v1-pilot',
        evaluation='Training examples as a fit diagnostic; unchanged human-v2 and composition-transfer-v1 in chat and submit modes. Transfer-v1 is now a development test for this skill family.',
        row_provenance=[{k: v for k, v in r.items() if k != 'row'} for r in rows])
    manifest.write_text(json.dumps(settings, indent=2) + '\n')
    print(json.dumps({k: v for k, v in settings.items() if k != 'row_provenance'}, indent=2))


if __name__ == '__main__':
    main()
