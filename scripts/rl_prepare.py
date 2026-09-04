#!/usr/bin/env python3
"""Render an RL task pool with the runner's prompts and submit tools."""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from rl_common import add_thinking_arg

KINDS = {"t1": "write", "t2": "optimize", "t3": "identify", "t4": "tree", "t5": "judgment"}


def unwrap_fixture(wrapper):
    if "id" in wrapper:
        return wrapper
    candidates = [v for v in wrapper.values() if isinstance(v, dict) and "id" in v]
    if len(candidates) != 1:
        raise ValueError("Expected one fixture per JSONL row")
    return candidates[0]


def kind_of(fixture):
    kind = fixture.get("task") or KINDS.get(fixture.get("id", "")[:2])
    if kind not in KINDS.values():
        raise ValueError(f"Unknown task kind for {fixture.get('id')!r}")
    return kind


def tool_for(kind):
    from sft_format import SUBMIT_DESCRIPTOR, SUBMIT_IDENTIFY, SUBMIT_SCRIPT
    if kind in ("write", "optimize", "judgment"):
        return SUBMIT_SCRIPT
    if kind == "tree":
        return SUBMIT_DESCRIPTOR
    if kind == "identify":
        return SUBMIT_IDENTIFY
    raise ValueError(f"Unknown task kind: {kind}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pool", default="datasets/rl-pool-1")
    ap.add_argument("--out", default="datasets/rl-train.jsonl")
    ap.add_argument("--model", default="runs/sft-qwen3-4b/merged")
    add_thinking_arg(ap)
    args = ap.parse_args()
    pool = Path(args.pool)
    with (pool / "manifest.json").open() as f:
        if json.load(f).get("evaluation_only"):
            raise SystemExit("This dataset is reserved for evaluation; do not use it for RL.")
    with (pool / "fixtures.jsonl").open() as f:
        fixtures = [unwrap_fixture(json.loads(line)) for line in f if line.strip()]
    for fixture in fixtures:
        if kind_of(fixture) == "judgment" and fixture.get("contract_version") != 1:
            raise SystemExit("Old judgment contract; regenerate the task pool before RL preparation.")
    if not fixtures:
        raise SystemExit("RL task pool is empty")

    subprocess.run(["./target/release/btc-bench", "audit", "--dataset", str(pool)], check=True)
    from transformers import AutoTokenizer
    from sft_format import SYSTEM_PROMPT
    with tempfile.TemporaryDirectory(prefix="btc-bench-rl-") as tmp:
        prompt_path = Path(tmp) / "prompts.jsonl"
        subprocess.run(["./target/release/btc-bench", "prompts", "--dataset", str(pool),
                        "--out", str(prompt_path)], check=True)
        with prompt_path.open() as f:
            prompts = {row["id"]: row["prompt"] for row in map(json.loads, f)}

    tok = AutoTokenizer.from_pretrained(args.model)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w") as out:
        for fixture in fixtures:
            tid, kind = fixture["id"], kind_of(fixture)
            messages = [{"role": "system", "content": SYSTEM_PROMPT},
                        {"role": "user", "content": prompts[tid]}]
            tool_list = [tool_for(kind)]
            rendered = tok.apply_chat_template(
                messages, tools=tool_list, add_generation_prompt=True,
                enable_thinking=args.thinking, tokenize=False,
            )
            out.write(json.dumps({"prompt": rendered, "task_json": json.dumps(fixture),
                                  "kind": kind, "task_id": tid, "thinking": args.thinking,
                                  "messages": messages, "tools": tool_list}) + "\n")
    print(f"rendered {len(fixtures)} RL prompts to {args.out}; thinking={args.thinking}")


if __name__ == "__main__":
    main()
