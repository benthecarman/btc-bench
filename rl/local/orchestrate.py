#!/usr/bin/env python3
"""Drive the local 27B RL loop from the rollout host (the RTX 5090 box).

Each step: pick the newest adapter the trainer has published (never more
than one step behind the batch being made), make sure the vLLM rollout
server has it loaded, generate the step's rollout groups, and hand the
batch to the trainer on the DGX Spark. The trainer (train.py, a
long-lived container on the Spark) turns batch-N into adapter step-N,
which the next steps pull back and serve.

Standard library only; runs on the host (it needs ssh/rsync to the
Spark and docker for the rollout driver).

    python3 rl/local/orchestrate.py --run runs/rl27 --start-step 1 --steps 40
"""

import argparse
import json
import os
import subprocess
import time
import urllib.request

SPARK = "ben@spark"
SPARK_REPO = "btc-bench"
IMAGE = "btc-verl:dev"
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MODEL_HUB = "/mnt/llm-models/huggingface/hub/models--nvidia--Qwen3.8-27B-NVFP4"
SNAPSHOT = "482ca0f3832238542f8f5295dde86b5f22711d80"


def log(**kw):
    print(json.dumps({"time": time.strftime("%Y-%m-%dT%H:%M:%S"), **kw}), flush=True)


def sh(*cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def local_steps(run):
    d = f"{run}/adapters"
    return sorted(int(n.split("-")[1]) for n in os.listdir(d)
                  if n.startswith("step-") and n.split("-")[1].isdigit()
                  and os.path.exists(f"{d}/{n}/adapter_model.safetensors"))


def pull_adapters(run):
    # optimizer.pt stays on the Spark; the rollout side needs only the adapter.
    sh("rsync", "-a", "--exclude", "*.tmp", "--exclude", "optimizer.pt",
       f"{SPARK}:{SPARK_REPO}/{run}/adapters/", f"{run}/adapters/")


def vllm(server, path, body):
    req = urllib.request.Request(server + path, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as r:
        return r.read().decode()


def served_adapters(server):
    with urllib.request.urlopen(server + "/v1/models") as r:
        return {m["id"] for m in json.load(r)["data"]}


def ensure_loaded(server, step):
    name = f"step-{step}"
    loaded = served_adapters(server)
    if name not in loaded:
        vllm(server, "/v1/load_lora_adapter", {"lora_name": name, "lora_path": f"/adapters/{name}"})
    # Keep at most this one and its predecessor (--max-loras 2).
    for other in loaded:
        if other.startswith("step-") and other not in (name, f"step-{step - 1}"):
            vllm(server, "/v1/unload_lora_adapter", {"lora_name": other})
    return name


def evaluate(run, cfg, server, name, repeat=1):
    """Score the base model or an adapter on the run's eval set with the
    benchmark runner itself (single attempt, submit mode, no cap).
    Repeat r>1 is another independent sample: same model, its own request
    seed, written to <name>-r<r>."""
    out = f"{run}/eval/{name}" + (f"-r{repeat}" if repeat > 1 else "")
    if os.path.exists(f"{out}/grade.txt"):
        return
    served = "qwen3.8:27b" if name == "base" else ensure_loaded(server, int(name.split("-")[1]))
    os.makedirs(f"{run}/eval", exist_ok=True)
    toml = f"{out}.toml"
    with open(toml, "w") as f:
        f.write(f'[model."{name}"]\nprovider = "openai_compatible"\nmodel = "{served}"\n'
                f'base_url = "{server}/v1"\ntemperature = {cfg["temperature"]}\n'
                f'request_params = {{ top_p = {cfg["top_p"]}, top_k = {cfg["top_k"]}, seed = {20260903 + repeat}, '
                f'chat_template_kwargs = {{ enable_thinking = true }} }}\n')
    t = time.time()
    log(event="eval_start", model=name, repeat=repeat, dataset=cfg["eval_dataset"])
    sh("./target/release/btc-bench", "run", "--dataset", cfg["eval_dataset"], "--config", toml, "--model", name,
       "--tools", "none", "--attempts", "1", "--concurrency", str(cfg["concurrency"]), "--out", out)
    with open(f"{out}/grade.txt", "w") as g:
        sh("./target/release/btc-bench", "grade", "--dataset", cfg["eval_dataset"], "--responses",
           f"{out}/responses.jsonl", "--out", f"{out}/graded", stdout=g, stderr=subprocess.STDOUT)
    log(event="eval_done", model=name, repeat=repeat, seconds=round(time.time() - t), grade=f"{out}/grade.txt")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--run", required=True, help="run dir relative to the repo, same path on both machines")
    ap.add_argument("--start-step", type=int, required=True)
    ap.add_argument("--steps", type=int, required=True)
    ap.add_argument("--server", default="http://127.0.0.1:18010")
    args = ap.parse_args()
    os.chdir(REPO)
    for step in range(args.start_step, args.start_step + args.steps):
        # Re-read every step so the mix can change without a restart.
        cfg = json.load(open(f"{args.run}/config.json"))
        # Rollouts for batch `step` may use adapter step-2 at the oldest.
        while True:
            pull_adapters(args.run)
            newest = max(local_steps(args.run))
            if newest >= step - 2:
                break
            time.sleep(30)
        adapter = ensure_loaded(args.server, newest)
        out = f"{args.run}/batches/batch-{step}.jsonl"
        t = time.time()
        log(event="rollout_start", step=step, adapter=adapter)
        # A fixed container name: stopping this orchestrator kills the docker
        # client, not the container, so a restarted orchestrator removes any
        # rollout still running instead of generating alongside it.
        container = f"{os.path.basename(args.run)}-rollout"
        for attempt in range(3):
            subprocess.run(["docker", "rm", "-f", container], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            try:
                sh("docker", "run", "--rm", "--name", container, "--network", "host",
                   "-v", f"{REPO}:/workspace/btc-bench",
                   "-v", f"{MODEL_HUB}:/hfmodel:ro", "-w", "/workspace/btc-bench",
                   "-e", "PYTHONPATH=/workspace/btc-bench/rl",
                   "-e", f"BTCBENCH_REWARD_URL={cfg['reward_url']}", "--entrypoint", "python3", IMAGE,
                   "rl/local/rollout.py", "--parquet", cfg["parquet"], "--kind-weights", cfg["kind_weights"],
                   "--n", str(cfg["n"]), "--adapter", adapter, "--server", args.server,
                   "--model-path", f"/hfmodel/snapshots/{SNAPSHOT}", "--temperature", str(cfg["temperature"]),
                   "--top-p", str(cfg["top_p"]), "--top-k", str(cfg["top_k"]),
                   "--response-length", str(cfg["response_length"]), "--concurrency", str(cfg["concurrency"]),
                   "--seed", str(cfg["seed"] * 100003 + step), "--out", out,
                   *(["--tiers", json.dumps(cfg["tiers"])] if cfg.get("tiers") else []))
                break
            except subprocess.CalledProcessError:
                if attempt == 2:
                    raise
                log(event="rollout_retry", step=step, attempt=attempt + 1)
                time.sleep(60)
        rewards = [json.loads(l)["reward"] for l in open(out)]
        log(event="rollout_done", step=step, adapter=adapter, seconds=round(time.time() - t),
            reward_mean=sum(rewards) / len(rewards))
        sh("rsync", "-a", out, f"{SPARK}:{SPARK_REPO}/{args.run}/batches/")
        sh("ssh", SPARK, f"touch {SPARK_REPO}/{out}.done")
        # Every eval_every steps, pause rollouts to score the base model
        # (once) and the newest adapter on the frozen eval subset.
        if cfg.get("eval_every") and step % cfg["eval_every"] == 0:
            pull_adapters(args.run)
            newest = f"step-{max(local_steps(args.run))}"
            for r in range(1, cfg.get("eval_repeats", 1) + 1):
                evaluate(args.run, cfg, args.server, "base", r)
                evaluate(args.run, cfg, args.server, newest, r)


if __name__ == "__main__":
    main()
