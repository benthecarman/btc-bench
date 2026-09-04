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
import urllib.request

from rl_common import (REWARD_URL, extract_answer, add_sampling_args,
                       add_thinking_arg, validate_rows, validate_sampling)


reward_url = REWARD_URL

def oracle_reward(completions, task_json, thinking=None, **kwargs):
    if len(completions) != len(task_json):
        raise ValueError("Completion and fixture counts differ")
    if thinking is None:
        thinking = [False] * len(completions)
    if len(thinking) != len(completions):
        raise ValueError("Completion and thinking-mode counts differ")
    items = [
        {"task": json.loads(tj), "answer": extract_answer(c, thinking=mode)}
        for c, tj, mode in zip(completions, task_json, thinking)
    ]
    req = urllib.request.Request(
        reward_url,
        data=json.dumps({"items": items}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=600) as resp:
        results = json.loads(resp.read())
    if len(results) != len(items):
        raise ValueError("Reward server returned the wrong number of results")
    return [r["shaped"] for r in results]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="datasets/rl-train.jsonl")
    ap.add_argument("--model", default="runs/sft-qwen3-4b/merged")
    ap.add_argument("--out", default="runs/rl-qwen3-4b")
    ap.add_argument("--steps", type=int, default=300)
    ap.add_argument("--reward-url", default=REWARD_URL)
    ap.add_argument("--resume-from-checkpoint", nargs="?", const=True, default=None,
                    help="Resume a checkpoint path, or the latest checkpoint in --out.")
    add_sampling_args(ap, training=True)
    add_thinking_arg(ap)
    args = ap.parse_args()
    validate_sampling(args)
    global reward_url
    reward_url = args.reward_url

    from datasets import load_dataset
    from peft import LoraConfig
    from trl import GRPOConfig, GRPOTrainer

    dataset = load_dataset("json", data_files=args.data, split="train")

    validate_rows(dataset, args.thinking)

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
        per_device_train_batch_size=args.k,
        gradient_accumulation_steps=2,
        num_generations=args.k,
        max_completion_length=args.max_completion_length,
        temperature=args.temperature,
        top_p=args.top_p,
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
    trainer.train(resume_from_checkpoint=args.resume_from_checkpoint)
    trainer.save_model(args.out + "/final")
    print(f"RL-DONE adapters in {args.out}/final")


if __name__ == "__main__":
    main()
