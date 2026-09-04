#!/usr/bin/env python3
"""LoRA SFT of Qwen3-4B on the rendered curriculum.

Prompt/completion pairs from sft_format.py; loss on completions only
(trl masks the prompt for this dataset shape). LoRA over attention
and MLP projections, bf16, gradient checkpointing — fits a single
32GB card with headroom.

Usage:
  sft_train.py [--data datasets/sft-train-rendered.jsonl]
               [--model Qwen/Qwen3-4B] [--out runs/sft-qwen3-4b]
               [--epochs 2]
"""

import argparse

from datasets import load_dataset
from peft import LoraConfig
from trl import SFTConfig, SFTTrainer


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="datasets/sft-train-rendered.jsonl")
    ap.add_argument("--model", default="Qwen/Qwen3-4B")
    ap.add_argument("--out", default="runs/sft-qwen3-4b")
    ap.add_argument("--epochs", type=float, default=2.0)
    args = ap.parse_args()

    dataset = load_dataset("json", data_files=args.data, split="train")

    peft_config = LoraConfig(
        r=64,
        lora_alpha=128,
        lora_dropout=0.05,
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

    config = SFTConfig(
        output_dir=args.out,
        num_train_epochs=args.epochs,
        # bs 4 x 4096 tokens OOMs a 32GB card; same effective batch.
        per_device_train_batch_size=1,
        gradient_accumulation_steps=16,
        learning_rate=1e-4,
        lr_scheduler_type="cosine",
        # trl 1.12 SFTConfig dropped warmup_ratio; ~3% of the ~1860
        # optimizer steps.
        warmup_steps=60,
        logging_steps=10,
        save_strategy="epoch",
        bf16=True,
        max_length=4096,
        gradient_checkpointing=True,
        model_init_kwargs={
            "torch_dtype": "bfloat16",
            "attn_implementation": "sdpa",
        },
        report_to="none",
        seed=7,
    )

    trainer = SFTTrainer(
        model=args.model,
        train_dataset=dataset,
        peft_config=peft_config,
        args=config,
    )
    trainer.train()
    trainer.save_model(args.out + "/final")
    print(f"TRAIN-DONE adapters in {args.out}/final")


if __name__ == "__main__":
    main()
