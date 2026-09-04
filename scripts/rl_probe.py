#!/usr/bin/env python3
"""Pre-flight check: does an RL pool actually produce reward variance?

GRPO's advantage is (r - group_mean) / group_std, so a group whose
rollouts all score the same contributes exactly zero gradient. A pool
can therefore look principled and still teach nothing — the first RL
run here spent four hours with 89% of steps at zero variance because
the model was already at ceiling on that pool.

This samples tasks, draws k rollouts each from the serving engine,
scores them through the reward server, and reports the fraction of
groups with usable spread. Run it before committing GPU hours.

Usage:
  rl_probe.py --data datasets/rl-train.jsonl --model qwen3-4b-think \
              [--tasks 40] [--k 8] [--url http://localhost:8010/v1]
"""

import argparse
import json
import random
import statistics
import urllib.request
from collections import defaultdict

from rl_train import extract_answer

REWARD_URL = "http://127.0.0.1:9900/reward/batch"


def sample(url, model, prompt, k):
    body = {
        "model": model,
        "prompt": prompt,
        "n": k,
        "max_tokens": 3072,
        "temperature": 1.0,
        "top_p": 0.95,
    }
    req = urllib.request.Request(
        url + "/completions",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=1200) as resp:
        return [c["text"] for c in json.loads(resp.read())["choices"]]


def score(items):
    req = urllib.request.Request(
        REWARD_URL,
        data=json.dumps({"items": items}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=1200) as resp:
        return [r["shaped"] for r in json.loads(resp.read())]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="datasets/rl-train.jsonl")
    ap.add_argument("--model", default="qwen3-4b-think")
    ap.add_argument("--url", default="http://localhost:8010/v1")
    ap.add_argument("--tasks", type=int, default=40)
    ap.add_argument("--k", type=int, default=8)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()

    rows = [json.loads(l) for l in open(args.data)]
    rng = random.Random(args.seed)
    picked = rng.sample(rows, min(args.tasks, len(rows)))

    by_kind = defaultdict(list)
    usable = 0
    for row in picked:
        comps = sample(args.url, args.model, row["prompt"], args.k)
        task = json.loads(row["task_json"])
        rewards = score(
            [{"task": task, "answer": extract_answer(c)} for c in comps]
        )
        sd = statistics.pstdev(rewards)
        mean = sum(rewards) / len(rewards)
        by_kind[row["kind"]].append((mean, sd))
        if sd > 1e-9:
            usable += 1

    print(f"{'kind':<10} {'n':>4} {'mean':>7} {'zero-var':>9}")
    for kind, vals in sorted(by_kind.items()):
        flat = sum(1 for _, sd in vals if sd <= 1e-9)
        m = sum(v for v, _ in vals) / len(vals)
        print(f"{kind:<10} {len(vals):>4} {m:>7.3f} {flat/len(vals):>8.0%}")
    frac = usable / len(picked)
    print(f"\ngroups with usable gradient: {usable}/{len(picked)} = {frac:.0%}")
    if frac < 0.3:
        print("VERDICT: pool is too saturated (or too hard) — GRPO will mostly idle.")
    else:
        print("VERDICT: pool has workable reward spread.")


if __name__ == "__main__":
    main()
