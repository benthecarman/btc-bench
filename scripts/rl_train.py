#!/usr/bin/env python3
"""GRPO RLVR on generated tasks, rewarded by the btc-bench oracle.

Rollouts are sampled by a colocated vLLM engine; each completion's
submit tool call is extracted and sent, with its full fixture, to the
reward server's /reward/batch — the same grading as `btc-bench
grade` plus the configured shaping. Completions with no extractable
answer fall back to the raw text (the server's answer parser handles
hex/asm), so partial credit shaping still applies.

The completion length limit is the trainer's rollout budget — the
bench's no-caps rule explicitly assigns that knob to the RL config;
a truncated rollout scores whatever the reward says (usually 0).

Prereqs:
  ./target/release/btc-bench reward-serve --bind 127.0.0.1:9900 \
      --shape-decode 0.05 --shape-agreement 0.2 --lint-penalty 0.02
  scripts/rl_prepare.py

Usage:
  rl_train.py [--data datasets/rl-train.jsonl]
              [--model runs/sft-qwen3-4b/merged]
              [--out runs/rl-qwen3-4b] [--steps 300]
"""

import argparse
import json
import re
import urllib.request

from datasets import load_dataset
from peft import LoraConfig
from trl import GRPOConfig, GRPOTrainer

REWARD_URL = "http://127.0.0.1:9900/reward/batch"
TOOL_CALL_RE = re.compile(r"<tool_call>\s*(\{.*?\})\s*</tool_call>", re.DOTALL)


def extract_answer(completion: str):
    """Submit tool call -> TaskAnswer JSON; else the raw text."""
    for m in TOOL_CALL_RE.finditer(completion):
        try:
            call = json.loads(m.group(1))
        except json.JSONDecodeError:
            continue
        name = call.get("name", "")
        args = call.get("arguments") or {}
        if isinstance(args, str):
            try:
                args = json.loads(args)
            except json.JSONDecodeError:
                continue
        if name == "submit_script" and "script" in args:
            return {"task": "script", "script": str(args["script"])}
        if name == "submit_descriptor" and "descriptor" in args:
            return {"task": "descriptor", "descriptor": str(args["descriptor"])}
        if name == "submit_identify" and "label" in args:
            return {"task": "identify", "label": str(args["label"])}
    return completion.strip()


def oracle_reward(completions, task_json, **kwargs):
    items = [
        {"task": json.loads(tj), "answer": extract_answer(c)}
        for c, tj in zip(completions, task_json)
    ]
    req = urllib.request.Request(
        REWARD_URL,
        data=json.dumps({"items": items}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=600) as resp:
        results = json.loads(resp.read())
    return [r["shaped"] for r in results]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="datasets/rl-train.jsonl")
    ap.add_argument("--model", default="runs/sft-qwen3-4b/merged")
    ap.add_argument("--out", default="runs/rl-qwen3-4b")
    ap.add_argument("--steps", type=int, default=300)
    args = ap.parse_args()

    dataset = load_dataset("json", data_files=args.data, split="train")

    peft_config = LoraConfig(
        r=64,
        lora_alpha=128,
        lora_dropout=0.0,
        bias="none",
        task_type="CAUSAL_LM",
        target_modules=[
            "q_proj",
            "k_proj",
            "v_proj",
            "o_proj",
            "gate_proj",
            "up_proj",
            "down_proj",
        ],
    )

    config = GRPOConfig(
        output_dir=args.out,
        max_steps=args.steps,
        learning_rate=1e-5,
        lr_scheduler_type="cosine",
        warmup_steps=10,
        per_device_train_batch_size=8,
        gradient_accumulation_steps=2,
        num_generations=8,
        max_completion_length=4096,
        temperature=0.6,
        top_p=0.95,
        beta=0.02,
        logging_steps=1,
        save_steps=50,
        bf16=True,
        gradient_checkpointing=True,
        use_vllm=True,
        vllm_mode="colocate",
        vllm_gpu_memory_utilization=0.25,
        report_to="none",
        seed=7,
    )

    trainer = GRPOTrainer(
        model=args.model,
        reward_funcs=oracle_reward,
        train_dataset=dataset,
        peft_config=peft_config,
        args=config,
    )
    trainer.train()
    trainer.save_model(args.out + "/final")
    print(f"RL-DONE adapters in {args.out}/final")


if __name__ == "__main__":
    main()
