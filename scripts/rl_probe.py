#!/usr/bin/env python3
"""Measure reward variation before training; save every raw completion.

Defaults share training's temperature, top-p and group size. Generation is
uncapped. To reproduce the trainer's budget, explicitly pass
--max-completion-length 4096. Both use the same prepared thinking prompts.
Use --regrade PATH to score saved samples again without model calls.
"""

import argparse
from collections import defaultdict
import json
from pathlib import Path
import random
import statistics
import urllib.request

from rl_common import (REWARD_URL, extract_answer, extract_task_answer, add_sampling_args,
                       add_thinking_arg, validate_rows, validate_sampling)


class SampleError(RuntimeError):
    def __init__(self, message, completions):
        super().__init__(message)
        self.completions = completions


def sample(url, model, row, args):
    body = {"model": model, "n": args.k, "stream": True,
            "temperature": args.temperature, "top_p": args.top_p,
            "top_k": args.top_k, "min_p": args.min_p, "seed": args.seed}
    if args.max_completion_length is not None:
        # Explicit trainer-budget probe: use the exact rendered training prompt.
        endpoint = "/completions"
        body.update(prompt=row["prompt"], max_tokens=args.max_completion_length)
    else:
        # vLLM /completions silently defaults to 16 tokens, even for null.
        # Chat has no such default. Use the same messages/tools and template mode.
        if "messages" not in row or "tools" not in row:
            raise ValueError("Rerun rl_prepare.py to store messages/tools for uncapped chat probes")
        endpoint = "/chat/completions"
        body.update(messages=row["messages"], tools=row["tools"], tool_choice="auto",
                    chat_template_kwargs={"enable_thinking": row["thinking"]})
    req = urllib.request.Request(url.rstrip("/") + endpoint,
                                 data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    choices = {i: {"text": "", "finish_reason": None, "raw_chunks": [], "tool_calls": {}}
               for i in range(args.k)}
    done = False
    try:
        with urllib.request.urlopen(req) as resp:
            for line in resp:
                line = line.decode().strip()
                if not line.startswith("data:"):
                    continue
                data = line[5:].strip()
                if data == "[DONE]":
                    done = True
                    break
                chunk = json.loads(data)
                if "error" in chunk:
                    raise RuntimeError(chunk["error"])
                for choice in chunk.get("choices", []):
                    target = choices[choice["index"]]
                    target["raw_chunks"].append(choice)
                    delta = choice.get("delta", {})
                    target["text"] += choice.get("text") or delta.get("content") or ""
                    for field in ("reasoning_content", "reasoning"):
                        if delta.get(field) is not None:
                            target["reasoning"] = target.get("reasoning", "") + delta[field]
                    for tool in delta.get("tool_calls") or []:
                        saved = target["tool_calls"].setdefault(tool["index"], {"name": "", "arguments": ""})
                        for field in ("name", "arguments"):
                            saved[field] += tool.get("function", {}).get(field) or ""
                    if choice.get("finish_reason"):
                        target["finish_reason"] = choice["finish_reason"]
        if not done or any(c["finish_reason"] is None for c in choices.values()):
            raise RuntimeError("Incomplete completion stream; do not count this as a model failure")
    except Exception as error:
        raise SampleError(str(error), list(choices.values())) from error
    for choice in choices.values():
        choice["tool_calls"] = list(choice["tool_calls"].values())
    return list(choices.values())


def answer_from_sample(choice, thinking, task=None):
    if choice.get("finish_reason") == "length":
        return ""
    if choice.get("tool_calls"):
        text = "".join("<tool_call>" + json.dumps(tool) + "</tool_call>"
                       for tool in choice["tool_calls"])
        return extract_task_answer(text, task or {})
    # A server reasoning parser has already separated reasoning from final content.
    return extract_task_answer(choice["text"], task or {}, thinking=thinking and "reasoning" not in choice)


def score(items, reward_url):
    req = urllib.request.Request(reward_url, data=json.dumps({"items": items}).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        results = json.loads(resp.read())
    if len(results) != len(items):
        raise ValueError("Reward server returned the wrong number of results")
    return results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="datasets/rl-train.jsonl")
    ap.add_argument("--model", default="qwen3-4b-think")
    ap.add_argument("--url", default="http://localhost:8010/v1")
    ap.add_argument("--reward-url", default=REWARD_URL)
    ap.add_argument("--tasks", type=int, default=40)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--out", default="runs/rl-probe.jsonl")
    ap.add_argument("--regrade", help="Saved probe JSONL; no generation requests")
    add_sampling_args(ap)
    add_thinking_arg(ap)
    args = ap.parse_args()
    validate_sampling(args)
    if args.tasks < 1:
        ap.error("--tasks must be positive")
    source = args.regrade or args.data
    with open(source) as f:
        rows = [json.loads(line) for line in f if line.strip()]
    if args.regrade:
        picked = rows
        if not picked:
            ap.error("saved probe is empty")
    else:
        validate_rows(rows, args.thinking)
        picked = random.Random(args.seed).sample(rows, min(args.tasks, len(rows)))
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    by_kind = defaultdict(list)
    usable = 0
    # Do not overwrite the expensive raw samples from an earlier probe.
    with open(args.out, "x") as out:
        for row in picked:
            if args.regrade:
                record = row
                task = json.loads(record["row"]["task_json"])
            else:
                task = json.loads(row["task_json"])
                record = {"row": row, "model": args.model, "url": args.url,
                          "sampling": {k: getattr(args, k) for k in
                                       ("temperature", "top_p", "top_k", "min_p", "seed", "k", "max_completion_length", "thinking")},
                          "completions": []}
                try:
                    record["completions"] = sample(args.url, args.model, row, args)
                except SampleError as error:
                    record.update(completions=error.completions, transport_error=str(error))
                    out.write(json.dumps(record) + "\n")
                    out.flush()
                    raise
            # Persist before scoring: a reward outage must not lose generations.
            out.write(json.dumps(record) + "\n")
            out.flush()
            if record.get("transport_error"):
                print(f"{record['row']['task_id']}: skipped incomplete transport record", flush=True)
                continue
            results = score([{"task": task, "answer": answer_from_sample(c, record["row"].get("thinking", False), task)}
                             for c in record["completions"]], args.reward_url)
            rewards = [r["shaped"] for r in results]
            if len(rewards) < 2:
                raise ValueError("A probe group needs at least two completions")
            sd, mean = statistics.pstdev(rewards), statistics.mean(rewards)
            by_kind[record["row"]["kind"]].append((mean, sd))
            usable += sd > 1e-9
            # Scores are cheap derived data; raw records above remain regradable.
            print(f"{record['row']['task_id']}: mean={mean:.3f} std={sd:.3f}", flush=True)
    print(f"{'kind':<10} {'n':>4} {'mean':>7} {'zero-var':>9}")
    for kind, vals in sorted(by_kind.items()):
        flat = sum(sd <= 1e-9 for _, sd in vals)
        print(f"{kind:<10} {len(vals):>4} {statistics.mean(v for v, _ in vals):>7.3f} {flat/len(vals):>8.0%}")
    measured = sum(len(vals) for vals in by_kind.values())
    if measured:
        print(f"\ngroups with reward variation: {usable}/{measured} = {usable/measured:.0%}")
    else:
        print("No complete groups to measure")
    print(f"raw samples: {args.out}")


if __name__ == "__main__":
    main()
