#!/usr/bin/env python3
"""Rollout prompts are token-identical to what the eval server builds.

Renders parquet rows the way the agent loop does (verl's chat-template
helper, tools passed through vLLM's request model) and compares the ids
with the serving vLLM's /tokenize for the same messages and tools.

    MODEL_PATH=/model python3 rl/tests/check_prompt_parity.py \
        --parquet datasets/rl-pool-1/verl/val.parquet --server http://127.0.0.1:18000
"""

import argparse
import json
import os
import urllib.request

import pandas as pd
from transformers import AutoProcessor, AutoTokenizer
from verl.utils.tokenizer.chat_template import apply_chat_template

from btc_verl import parse

CHAT_TEMPLATE_KWARGS = {"enable_thinking": True}


def server_ids(server, model, messages, tools):
    body = {"model": model, "messages": messages, "tools": tools, "add_generation_prompt": True,
            "chat_template_kwargs": CHAT_TEMPLATE_KWARGS}
    req = urllib.request.Request(f"{server}/tokenize", data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)["tokens"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--parquet", required=True)
    ap.add_argument("--server", required=True)
    ap.add_argument("--served-model", default="qwen3.8:27b")
    ap.add_argument("--limit", type=int, default=20)
    args = ap.parse_args()
    model_path = os.environ["MODEL_PATH"]
    tokenizer = AutoTokenizer.from_pretrained(model_path)
    try:
        processor = AutoProcessor.from_pretrained(model_path)
    except Exception:
        processor = None

    rows = pd.read_parquet(args.parquet).head(args.limit)
    mismatches = 0
    for _, row in rows.iterrows():
        messages = [dict(m) for m in row["prompt"]]
        request = parse.chat_request(messages, json.loads(row["extra_info"]["tools_json"]), CHAT_TEMPLATE_KWARGS)
        tools = parse.served_tools(request)
        text = apply_chat_template(processor or tokenizer, messages, tools=tools, add_generation_prompt=True,
                                   tokenize=False, **CHAT_TEMPLATE_KWARGS)
        ours = tokenizer(text, add_special_tokens=False)["input_ids"]
        theirs = server_ids(args.server, args.served_model, messages, json.loads(row["extra_info"]["tools_json"]))
        if ours != theirs:
            mismatches += 1
            first = next((i for i, (a, b) in enumerate(zip(ours, theirs)) if a != b), min(len(ours), len(theirs)))
            print(f"MISMATCH {row['extra_info']['task_id']}: first diff at token {first}; "
                  f"ours {tokenizer.decode(ours[first:first + 20])!r} vs server {tokenizer.decode(theirs[first:first + 20])!r}")
    print(f"{len(rows) - mismatches}/{len(rows)} prompts token-identical")
    raise SystemExit(1 if mismatches else 0)


if __name__ == "__main__":
    main()
