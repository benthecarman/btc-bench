#!/usr/bin/env python3
"""Render an RL task pool into a GRPO prompt dataset.

Each row carries the rendered runner conversation (same system
prompt, submit tool, and chat template as SFT) plus the full fixture
JSON — the reward function forwards that fixture verbatim to the
reward server, which grades exactly like `btc-bench grade`.

Usage:
  rl_prepare.py [--pool datasets/rl-pool-1]
                [--out datasets/rl-train.jsonl]
                [--model runs/sft-qwen3-4b/merged]
"""

import argparse
import json

from transformers import AutoTokenizer

from sft_format import SUBMIT_DESCRIPTOR, SUBMIT_IDENTIFY, SUBMIT_SCRIPT, SYSTEM_PROMPT


def kind_of(fixture):
    tid = fixture.get("id", "")
    return {"t1": "write", "t2": "optimize", "t3": "identify", "t4": "tree"}.get(
        tid[:2], "unknown"
    )


def tool_for(kind):
    if kind in ("write", "optimize"):
        return SUBMIT_SCRIPT
    if kind == "tree":
        return SUBMIT_DESCRIPTOR
    return SUBMIT_IDENTIFY


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pool", default="datasets/rl-pool-1")
    ap.add_argument("--out", default="datasets/rl-train.jsonl")
    ap.add_argument("--model", default="runs/sft-qwen3-4b/merged")
    args = ap.parse_args()

    # The runner's prompt assembly lives in Rust; `prompts` dumps the
    # exact text per fixture.
    import subprocess

    subprocess.run(
        [
            "./target/release/btc-bench",
            "prompts",
            "--dataset",
            args.pool,
            "--out",
            "/tmp/rl-prompts.jsonl",
        ],
        check=True,
    )
    prompts = {}
    for line in open("/tmp/rl-prompts.jsonl"):
        r = json.loads(line)
        prompts[r["id"]] = r["prompt"]

    tok = AutoTokenizer.from_pretrained(args.model)
    n = 0
    with open(args.out, "w") as out:
        for line in open(f"{args.pool}/fixtures.jsonl"):
            wrapper = json.loads(line)
            # Fixture files wrap each task as {"kind": {...fields}} or
            # flat; keep the whole line for the reward server.
            fixture = wrapper
            tid = None
            for v in wrapper.values():
                if isinstance(v, dict) and "id" in v:
                    tid = v["id"]
            if tid is None:
                tid = wrapper.get("id")
            kind = kind_of({"id": tid})
            rendered = tok.apply_chat_template(
                [
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user", "content": prompts[tid]},
                ],
                tools=[tool_for(kind)],
                add_generation_prompt=True,
                enable_thinking=False,
                tokenize=False,
            )
            out.write(
                json.dumps(
                    {
                        "prompt": rendered,
                        "task_json": json.dumps(fixture),
                        "kind": kind,
                        "task_id": tid,
                    }
                )
                + "\n"
            )
            n += 1
    print(f"rendered {n} RL prompts to {args.out}")


if __name__ == "__main__":
    main()
