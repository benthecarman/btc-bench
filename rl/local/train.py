"""GRPO trainer for the local two-machine loop: LoRA on the bf16 base, one
GPU (DGX Spark). Long-lived: loads the model once, then trains on each
rollout batch that appears in --batches and writes the next adapter.

Objective, matching rl/config/btc_bench.yaml:
- advantage = reward minus its group's mean (no std normalisation);
  groups whose valid rewards are all equal carry no signal and are
  dropped, and reward-server outages (INVALID_REWARD_VALUE) are left out;
- loss over the model's own tokens (response_mask 1): by default
  REINFORCE with a truncated importance weight (capped at 2) against the
  sampling policy, verl's bypass mode with the reinforce loss; --loss ppo
  gives the clipped ratio (0.2) instead. With one optimizer step per
  batch there is no inner-loop trust region for clipping to keep; what
  clipping would do here is drop the tokens where NVFP4 sampling and the
  bf16 trainer disagree (17% at step 1), which the weight corrects
  instead;
- prompt-mean aggregation: every kept group weighs the same, tokens are
  averaged within a group;
- one AdamW step per batch.

The old policy is the one that sampled: the ratio is taken against the
rollout's own log-probs (verl's bypass mode), which absorbs both the
NVFP4-vs-bf16 gap and a one-step-stale adapter. Those log-probs are
vLLM's processed ones (after temperature, top-k, top-p), so the trainer
computes its log-probs under the same truncation; a token that falls
outside the trainer's own truncated set is left out of the loss.

    python3 rl/local/train.py --model-path /model --adapters /adapters --batches /batches \\
        --temperature 1.0 --start-step 0
"""

import argparse
import glob
import json
import math
import os
import shutil
import time

import torch
import torch.utils.checkpoint
from peft import PeftModel
from transformers import AutoModelForImageTextToText

INVALID_REWARD_VALUE = -999.0
CHUNK = 2048


def truncated_logprobs(hidden, head, targets, temperature, top_k, top_p):
    """Log-probs of targets under temperature, then top-k, then top-p, as
    vLLM samples; -inf where a target falls outside the truncated set."""
    logits = head(hidden).float() / temperature
    top_logits, top_idx = logits.topk(top_k, dim=-1)
    with torch.no_grad():
        probs = torch.softmax(top_logits, dim=-1)
        # Keep the smallest prefix whose mass reaches top_p (at least one token).
        keep = (probs.cumsum(-1) - probs) < top_p
    kept = top_logits.masked_fill(~keep, float("-inf"))
    logz = torch.logsumexp(kept, dim=-1)
    hit = (top_idx == targets.unsqueeze(-1)) & keep
    target_logit = torch.where(hit, top_logits, torch.zeros_like(top_logits)).sum(-1)
    return torch.where(hit.any(-1), target_logit - logz, torch.full_like(logz, float("-inf")))


def sequence_logprobs(model, ids, n_prompt, args):
    """Log-probs of the response tokens (positions n_prompt..end)."""
    body = model.base_model.model.model.language_model
    head = model.base_model.model.lm_head
    hidden = body(input_ids=ids.unsqueeze(0)).last_hidden_state[0, n_prompt - 1:-1]
    targets = ids[n_prompt:]
    out = []
    for i in range(0, len(targets), CHUNK):
        out.append(torch.utils.checkpoint.checkpoint(
            truncated_logprobs, hidden[i:i + CHUNK], head, targets[i:i + CHUNK],
            args.temperature, args.top_k, args.top_p, use_reentrant=False))
    return torch.cat(out)


def train_on(model, opt, batch, args):
    groups = {}
    for r in batch:
        groups.setdefault(r["group"], []).append(r)
    kept = []
    stats = {"rollouts": len(batch), "groups": len(groups), "reward_mean": 0.0, "unavailable": 0}
    valid_rewards = [r["reward"] for r in batch if r["reward"] != INVALID_REWARD_VALUE]
    stats["reward_mean"] = sum(valid_rewards) / max(1, len(valid_rewards))
    stats["unavailable"] = len(batch) - len(valid_rewards)
    for g in groups.values():
        valid = [r for r in g if r["reward"] != INVALID_REWARD_VALUE]
        rewards = [r["reward"] for r in valid]
        if len(valid) < 2 or max(rewards) == min(rewards):
            continue
        mean = sum(rewards) / len(rewards)
        kept.append([(r, r["reward"] - mean) for r in valid])
    stats["groups_kept"] = len(kept)
    if not kept:
        return stats
    model.train()
    opt.zero_grad(set_to_none=True)
    tokens = clipped = capped = outside = 0
    loss_sum, kl_sum = 0.0, 0.0
    for group in kept:
        group_tokens = sum(sum(r["response_mask"]) for r, _ in group)
        for r, adv in group:
            ids = torch.tensor(r["prompt_ids"] + r["response_ids"], device="cuda")
            mask = torch.tensor(r["response_mask"], device="cuda", dtype=torch.bool)
            old = torch.tensor(r["rollout_logprobs"], device="cuda", dtype=torch.float32)
            new = sequence_logprobs(model, ids, len(r["prompt_ids"]), args)
            usable = mask & torch.isfinite(new)
            outside += int((mask & ~torch.isfinite(new)).sum())
            log_ratio = torch.where(usable, new - old, torch.zeros_like(new))
            ratio = torch.exp(log_ratio)
            if args.loss == "tis":
                # Truncated importance sampling against the policy that
                # sampled: REINFORCE on the trainer's log-probs, each token
                # weighted by its (capped, detached) ratio to the rollout.
                weight = ratio.detach().clamp(max=args.is_cap)
                per_token = -weight * adv * torch.where(usable, new, torch.zeros_like(new))
            else:
                unclipped = ratio * adv
                clipped_obj = torch.clamp(ratio, 1 - args.clip, 1 + args.clip) * adv
                per_token = -torch.minimum(unclipped, clipped_obj)
            # prompt-mean: each group weighs 1/len(kept), tokens averaged in-group
            weight = 1.0 / (len(kept) * group_tokens)
            loss = (per_token * usable).sum() * weight
            loss.backward()
            with torch.no_grad():
                loss_sum += float(loss)
                n = int(usable.sum())
                tokens += n
                clipped += int(((ratio - 1).abs() > args.clip)[usable].sum())
                capped += int((ratio > args.is_cap)[usable].sum())
                kl_sum += float((-log_ratio)[usable].sum())
    grad_norm = torch.nn.utils.clip_grad_norm_([p for p in model.parameters() if p.requires_grad], args.grad_clip)
    opt.step()
    opt.zero_grad(set_to_none=True)
    stats.update({"loss": loss_sum, "grad_norm": float(grad_norm), "tokens": tokens,
                  "clip_frac": clipped / max(1, tokens), "is_capped_frac": capped / max(1, tokens),
                  "outside_nucleus": outside,
                  "approx_kl_to_rollout": kl_sum / max(1, tokens)})
    return stats


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--model-path", required=True)
    ap.add_argument("--adapters", required=True, help="dir with step-N adapters; step-<start> must exist")
    ap.add_argument("--batches", required=True, help="dir the rollout driver writes batch-N.jsonl into")
    ap.add_argument("--start-step", type=int, default=0)
    ap.add_argument("--steps", type=int, default=10**9)
    ap.add_argument("--lr", type=float, default=1e-4)
    ap.add_argument("--loss", choices=["tis", "ppo"], default="tis",
                    help="tis: truncated-IS REINFORCE against the sampling policy (verl bypass + "
                         "reinforce); ppo: clipped ratio against it")
    ap.add_argument("--is-cap", type=float, default=2.0)
    ap.add_argument("--clip", type=float, default=0.2)
    ap.add_argument("--grad-clip", type=float, default=1.0)
    ap.add_argument("--temperature", type=float, required=True)
    ap.add_argument("--top-p", type=float, default=0.95)
    ap.add_argument("--top-k", type=int, default=20)
    args = ap.parse_args()
    t = time.time()
    base = AutoModelForImageTextToText.from_pretrained(args.model_path, dtype=torch.bfloat16,
                                                       attn_implementation="flash_attention_2", device_map={"": 0})
    base.gradient_checkpointing_enable(gradient_checkpointing_kwargs={"use_reentrant": False})
    base.config.use_cache = False
    model = PeftModel.from_pretrained(base, f"{args.adapters}/step-{args.start_step}", is_trainable=True)
    for p in model.parameters():
        if p.requires_grad:
            p.data = p.data.float()
    model.enable_input_require_grads()
    opt = torch.optim.AdamW([p for p in model.parameters() if p.requires_grad], lr=args.lr, weight_decay=0.0)
    state = f"{args.adapters}/step-{args.start_step}/optimizer.pt"
    if os.path.exists(state):
        opt.load_state_dict(torch.load(state, map_location="cuda"))
    print(json.dumps({"event": "loaded", "seconds": round(time.time() - t)}), flush=True)
    for step in range(args.start_step + 1, args.start_step + 1 + args.steps):
        path = f"{args.batches}/batch-{step}.jsonl"
        while not os.path.exists(path + ".done"):
            time.sleep(10)
        batch = [json.loads(l) for l in open(path)]
        t = time.time()
        stats = train_on(model, opt, batch, args)
        out = f"{args.adapters}/step-{step}"
        tmp = out + ".tmp"
        shutil.rmtree(tmp, ignore_errors=True)
        model.save_pretrained(tmp)
        torch.save(opt.state_dict(), f"{tmp}/optimizer.pt")
        os.replace(tmp, out)
        # Only the newest optimizer state is needed to resume; one per step
        # filled the Spark's disk at step 14 (1.7 GB each for the 27B).
        stale = f"{args.adapters}/step-{step - 1}/optimizer.pt"
        if os.path.exists(stale):
            os.remove(stale)
        stats.update({"event": "step", "step": step, "seconds": round(time.time() - t),
                      "adapters_used": sorted({r["adapter"] for r in batch})})
        print(json.dumps(stats), flush=True)
        with open(f"{args.adapters}/../train-log.jsonl", "a") as f:
            f.write(json.dumps(stats) + "\n")


if __name__ == "__main__":
    main()
