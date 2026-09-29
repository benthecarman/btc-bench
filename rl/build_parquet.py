#!/usr/bin/env python3
"""Build verl train/val parquets from a btc-bench task pool.

Every row carries the exact request `btc-bench run` sends for its task:
the runner is pointed at an in-process mock endpoint and its request
bodies are recorded, so the system prompt, user prompt and submit tool
come from the runner itself rather than from a copy that can drift.

    python3 rl/build_parquet.py --pool datasets/rl-pool-1 --out-dir datasets/rl-pool-1/verl

Row schema (verl RLHFDataset):
    prompt        [system, user] chat messages, verbatim from the runner
    data_source   "btc-bench/<kind>"; groups metrics only
    agent_name    "btc_bench"; selects rl/btc_verl/agent_loop.py
    reward_model  {style: rule, ground_truth: ""}; the fixture is the key
    extra_info    {index, task_id, kind, fixture_json, tools_json}

tools_json keeps the runner's key order: the chat template serializes
the tool schema into the prompt, so order is part of the prompt.
"""

import argparse
import collections
import http.server
import json
import random
import subprocess
import tempfile
import threading
from pathlib import Path

BENCH = "./target/release/btc-bench"
KINDS = {"t1": "write", "t2": "optimize", "t3": "identify", "t4": "tree", "t5": "judgment"}


KEY_FIELDS = ("reference_script_hex", "optimal_script_hex", "baseline_script_hex",
              "reference_descriptor", "reference_policy", "spk_hex", "inner_script_hex")
FREEZE = Path("evals/heldout-test-1-freeze.json")


def check_no_eval_overlap(fixtures):
    """Refuse a pool that shares an answer key or policy with a frozen eval set.

    `btc-bench gen --exclude` should already guarantee this; this checks it.
    """
    def keys(fs):
        return {f[k] for f in fs for k in KEY_FIELDS if f.get(k)}
    pool = keys(fixtures)
    for d in json.loads(FREEZE.read_text())["rl_pools_must_exclude"]:
        path = Path(d) / "fixtures.jsonl"
        if not path.exists():
            raise SystemExit(f"{path} missing; regenerate it to check the pool against it")
        evals = [json.loads(l) for l in path.read_text().splitlines() if l]
        # A scriptPubKey shared by several eval tasks is a protocol
        # constant (P2A is always OP_1 <4e73>), not an answer key.
        spk_counts = collections.Counter(f["spk_hex"] for f in evals if f.get("spk_hex"))
        constants = {spk for spk, n in spk_counts.items() if n > 1}
        shared = (pool & keys(evals)) - constants
        if shared:
            raise SystemExit(f"pool shares {len(shared)} answer keys or policies with {d}; "
                             f"regenerate with --exclude {d}")


def capture_requests(pool: Path, tmp: Path, concurrency: int) -> list[dict]:
    """Run the benchmark runner against a recording mock; return request bodies."""
    bodies = []
    lock = threading.Lock()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            with lock:
                bodies.append(body)
            reply = json.dumps({
                "id": "capture", "object": "chat.completion", "created": 0, "model": "capture",
                "choices": [{"index": 0, "finish_reason": "stop",
                             "message": {"role": "assistant", "content": "capture"}}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
            }).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(reply)))
            self.end_headers()
            self.wfile.write(reply)

        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    config = tmp / "models.toml"
    config.write_text(
        "[model.capture]\nprovider = \"openai_compatible\"\nmodel = \"capture\"\n"
        f"base_url = \"http://127.0.0.1:{server.server_port}/v1\"\nstream = false\n"
    )
    try:
        subprocess.run([BENCH, "run", "--dataset", str(pool), "--config", str(config),
                        "--model", "capture", "--attempts", "1", "--tools", "none",
                        "--concurrency", str(concurrency), "--out", str(tmp / "run")],
                       check=True, stdout=subprocess.DEVNULL)
    finally:
        server.shutdown()
    return bodies


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pool", type=Path, required=True)
    ap.add_argument("--out-dir", type=Path, required=True)
    ap.add_argument("--val-per-kind", type=int, default=16,
                    help="Held out per task kind for verl's validation pass.")
    ap.add_argument("--seed", type=int, default=20260928)
    ap.add_argument("--concurrency", type=int, default=32)
    args = ap.parse_args()

    manifest = json.loads((args.pool / "manifest.json").read_text())
    if manifest.get("evaluation_only"):
        raise SystemExit("This dataset is reserved for evaluation; do not use it for RL.")
    fixtures = [json.loads(line) for line in (args.pool / "fixtures.jsonl").read_text().splitlines() if line]
    if not fixtures:
        raise SystemExit("task pool is empty")
    for f in fixtures:
        if f.get("task") == "judgment" and f.get("contract_version") != 1:
            raise SystemExit("Old judgment contract; regenerate the task pool.")
    subprocess.run([BENCH, "audit", "--dataset", str(args.pool)], check=True)
    check_no_eval_overlap(fixtures)

    with tempfile.TemporaryDirectory(prefix="btc-verl-") as tmp:
        tmp = Path(tmp)
        subprocess.run([BENCH, "prompts", "--dataset", str(args.pool), "--out", str(tmp / "prompts.jsonl")],
                       check=True, stdout=subprocess.DEVNULL)
        prompts = [json.loads(line) for line in (tmp / "prompts.jsonl").read_text().splitlines() if line]
        bodies = capture_requests(args.pool, tmp, args.concurrency)

    # The request carries no task id; match on the user prompt.
    by_prompt = collections.defaultdict(list)
    for p in prompts:
        by_prompt[p["prompt"]].append(p["id"])
    if any(len(ids) > 1 for ids in by_prompt.values()):
        raise SystemExit("two tasks share a prompt; cannot attribute captured requests")
    requests = {}
    for body in bodies:
        user = [m for m in body["messages"] if m["role"] == "user"]
        if len(user) != 1 or len(body.get("tools", [])) != 1:
            raise SystemExit(f"unexpected request shape: {[m['role'] for m in body['messages']]}")
        (tid,) = by_prompt[user[0]["content"]]
        if tid in requests:
            raise SystemExit(f"{tid} was requested twice; the runner retried a mock reply")
        requests[tid] = body
    missing = {f["id"] for f in fixtures} - requests.keys()
    if missing:
        raise SystemExit(f"no captured request for {len(missing)} tasks, e.g. {sorted(missing)[:3]}")

    rows_by_kind = collections.defaultdict(list)
    for f in fixtures:
        body = requests[f["id"]]
        kind = KINDS[f["id"][:2]]
        rows_by_kind[kind].append({
            "prompt": [{"role": m["role"], "content": m["content"]} for m in body["messages"]],
            "data_source": f"btc-bench/{kind}",
            "ability": "bitcoin-script",
            "agent_name": "btc_bench",
            "reward_model": {"style": "rule", "ground_truth": ""},
            "extra_info": {"task_id": f["id"], "kind": kind,
                           "fixture_json": json.dumps(f), "tools_json": json.dumps(body["tools"])},
        })

    import pandas as pd
    rng = random.Random(args.seed)
    train, val = [], []
    for kind in sorted(rows_by_kind):
        rows = rows_by_kind[kind]
        rng.shuffle(rows)
        n_val = min(args.val_per_kind, len(rows) // 5)
        val += rows[:n_val]
        train += rows[n_val:]
    for split, rows in (("train", train), ("val", val)):
        for i, row in enumerate(rows):
            row["extra_info"]["index"] = i
    args.out_dir.mkdir(parents=True, exist_ok=True)
    pd.DataFrame(train).to_parquet(args.out_dir / "train.parquet")
    pd.DataFrame(val).to_parquet(args.out_dir / "val.parquet")
    counts = {k: len(v) for k, v in sorted(rows_by_kind.items())}
    (args.out_dir / "build.json").write_text(json.dumps({
        "pool": str(args.pool), "pool_manifest": manifest, "seed": args.seed,
        "counts": counts, "train": len(train), "val": len(val),
    }, indent=2) + "\n")
    print(f"wrote {len(train)} train / {len(val)} val rows to {args.out_dir}; kinds {counts}")


if __name__ == "__main__":
    main()
