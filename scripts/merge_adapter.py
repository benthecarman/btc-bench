#!/usr/bin/env python3
"""Merge a local LoRA adapter into a new directory, preserving its parent."""
import argparse
import json
from pathlib import Path


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--base', required=True)
    ap.add_argument('--adapter', required=True)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    if args.out.exists():
        ap.error('output exists; preserve the earlier model')
    import torch
    from peft import PeftModel
    from transformers import AutoModelForCausalLM, AutoTokenizer
    base = AutoModelForCausalLM.from_pretrained(args.base, dtype=torch.bfloat16, device_map='cpu')
    model = PeftModel.from_pretrained(base, args.adapter).merge_and_unload(safe_merge=True)
    model.save_pretrained(args.out, safe_serialization=True)
    tokenizer_source = args.adapter if (Path(args.adapter)/'tokenizer_config.json').exists() else args.base
    AutoTokenizer.from_pretrained(tokenizer_source).save_pretrained(args.out)
    (args.out/'merge-provenance.json').write_text(json.dumps(dict(base=args.base, adapter=args.adapter, dtype='bfloat16'), indent=2)+'\n')
    print(f'Merged adapter into {args.out}')


if __name__ == '__main__':
    main()
