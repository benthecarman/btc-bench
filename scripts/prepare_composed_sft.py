#!/usr/bin/env python3
"""Build verified SFT targets for the same separate tasks used by RL."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--data', type=Path, default=Path('datasets/rl-composed-training-v1.jsonl'))
    ap.add_argument('--pool', type=Path, default=Path('datasets/composed-training-v1'))
    ap.add_argument('--out', type=Path, default=Path('datasets/sft-composed-training-v1-asm.jsonl'))
    ap.add_argument('--model', default='runs/sft-qwen3-4b-think/merged')
    ap.add_argument('--max-length', type=int, default=4096)
    args = ap.parse_args()
    from transformers import AutoTokenizer
    tok = AutoTokenizer.from_pretrained(args.model)
    with tempfile.TemporaryDirectory(prefix='btc-composed-sft-') as temp:
        export = Path(temp)/'targets.jsonl'
        subprocess.run(['./target/release/btc-bench', 'sft-export', '--dataset', str(args.pool), '--out', str(export)], check=True)
        targets = {r['task_id']: r for r in map(json.loads, export.read_text().splitlines())}
    rows, lengths = [], []
    for line in args.data.read_text().splitlines():
        record = json.loads(line)
        fixture = json.loads(record['task_json'])
        if not fixture['id'].startswith(('t1-human-composed-train-', 't4-human-composed-train-')):
            raise ValueError('Only the separate composed training pool is accepted')
        if not record['thinking']:
            raise ValueError('This target builder requires thinking prompts')
        policy = fixture['reference_policy']
        if fixture['task'] == 'write':
            reference = fixture['reference_miniscript']
            target = targets[fixture['id']]
            if target['target_hex'] != fixture['reference_script_hex']:
                raise ValueError('Exported answer does not match the RL fixture')
            asm = target['target_asm']
            tool = dict(name='submit_script', arguments={'script': asm})
            derivation = f'Let me work through the spending conditions.\nAs a policy that is: {policy}\nIn Miniscript: {reference}\nWhich encodes to:\n{asm}'
        elif fixture['task'] == 'tree':
            reference = fixture['reference_descriptor']
            tool = dict(name='submit_descriptor', arguments={'descriptor': reference})
            derivation = f'This is a Taproot output, so the spending paths split between the key path and the tapleaves.\nAs a policy that is: {policy}\nA verified descriptor for those spending paths:\n{reference}'
        else:
            raise ValueError('Unexpected task kind')
        if record['prompt'].endswith('<think>\n'):
            prefix = ''
        elif record['prompt'].endswith('<|im_start|>assistant\n'):
            prefix = '<think>\n'
        else:
            raise ValueError('Unexpected thinking template; inspect before training')
        completion = prefix + derivation + '\n</think>\n\n<tool_call>\n' + json.dumps(tool) + '\n</tool_call><|im_end|>'
        n = len(tok.encode(record['prompt'] + completion, add_special_tokens=False))
        if n > args.max_length:
            raise ValueError(f"{fixture['id']} has {n} tokens; do not truncate a reference answer")
        rows.append(dict(prompt=record['prompt'], completion=completion))
        lengths.append(n)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open('x') as out:
        for row in rows:
            out.write(json.dumps(row) + '\n')
    meta = dict(source=str(args.data), source_sha256=hashlib.sha256(args.data.read_bytes()).hexdigest(),
                rows=len(rows), max_tokens=max(lengths), mean_tokens=sum(lengths)/len(lengths), model=args.model,
                script_target_format='asm', pool=str(args.pool))
    args.out.with_suffix('.meta.json').write_text(json.dumps(meta, indent=2)+'\n')
    print(json.dumps(meta, indent=2))


if __name__ == '__main__':
    main()
