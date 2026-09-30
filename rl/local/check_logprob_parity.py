"""The trainer's log-probs match the rollout server's on the tokens it sampled.

Loads the model and adapter the way train.py does, recomputes log-probs
for rollouts written by rollout.py, and compares with vLLM's processed
log-probs on the model's own tokens.

    python3 rl/local/check_logprob_parity.py --model-path M --adapter DIR --rollouts R.jsonl --temperature 1.0
"""
import argparse, json, sys
import torch
from peft import PeftModel
from transformers import AutoModelForImageTextToText
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from train import sequence_logprobs

ap = argparse.ArgumentParser()
ap.add_argument("--model-path", required=True); ap.add_argument("--adapter", required=True)
ap.add_argument("--rollouts", required=True); ap.add_argument("--temperature", type=float, required=True)
ap.add_argument("--top-p", type=float, default=0.95); ap.add_argument("--top-k", type=int, default=20)
args = ap.parse_args()
base = AutoModelForImageTextToText.from_pretrained(args.model_path, dtype=torch.bfloat16,
                                                   attn_implementation="flash_attention_2", device_map={"": 0})
model = PeftModel.from_pretrained(base, args.adapter).eval()
diffs, outside, n = [], 0, 0
with torch.no_grad():
    for line in open(args.rollouts):
        r = json.loads(line)
        ids = torch.tensor(r["prompt_ids"] + r["response_ids"], device="cuda")
        new = sequence_logprobs(model, ids, len(r["prompt_ids"]), args)
        old = torch.tensor(r["rollout_logprobs"], device="cuda")
        mask = torch.tensor(r["response_mask"], device="cuda", dtype=torch.bool)
        fin = mask & torch.isfinite(new)
        outside += int((mask & ~torch.isfinite(new)).sum()); n += int(mask.sum())
        diffs.append((new - old)[fin].abs())
d = torch.cat(diffs)
print(f"tokens {n}; outside trainer nucleus {outside} ({outside / n:.4%}); |new-old| mean {d.mean():.4f} "
      f"median {d.median():.4f} p99 {d.quantile(0.99):.4f} max {d.max():.4f}")
