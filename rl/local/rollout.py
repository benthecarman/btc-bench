"""Generate GRPO rollout groups against a vLLM server serving the current
LoRA adapter, for the local two-machine loop (rollouts on the RTX 5090,
training on the DGX Spark).

Each rollout follows btc_verl's agent loops exactly: the row's own
messages and tools, rendered through vLLM's request model; turns parsed
with the eval server's parsers; answers and check-tool replies from
reward-serve (/reward or /turn). Tokens are kept as generated, with
vLLM's sampling log-probs, so the trainer can take its importance ratio
against the policy that actually sampled them.

    python3 rl/local/rollout.py --parquet train.parquet --adapter step-3 \
        --server http://127.0.0.1:18010 --prompts 8 --n 8 --out batch-4.jsonl
"""

import argparse
import asyncio
import json
import random

import aiohttp
import pandas as pd
from transformers import AutoProcessor, AutoTokenizer
from verl.utils.tokenizer.chat_template import apply_chat_template, initialize_turn_separator

from btc_verl import parse, reward_client
from btc_verl.agent_loop import INVALID_REWARD_VALUE, new_info, record_reward, render_after_turn

TEMPLATE_KWARGS = {"enable_thinking": True}


class Roller:
    def __init__(self, args):
        self.args = args
        self.tok = AutoTokenizer.from_pretrained(args.model_path)
        try:
            self.pc = AutoProcessor.from_pretrained(args.model_path)
        except Exception:
            self.pc = self.tok
        self.separator = initialize_turn_separator(self.pc, **TEMPLATE_KWARGS)
        self.eos = {self.tok.convert_tokens_to_ids(t) for t in ("<|im_end|>", "<|endoftext|>")}
        self.sem = asyncio.Semaphore(args.concurrency)

    def prompt_ids(self, messages, request):
        text = apply_chat_template(self.pc, messages, tools=parse.served_tools(request), tokenize=False,
                                   add_generation_prompt=True, **TEMPLATE_KWARGS)
        return self.tok.encode(text, add_special_tokens=False)

    async def generate(self, session, ids, max_tokens):
        a = self.args
        body = {"model": a.adapter, "prompt": ids, "max_tokens": max_tokens, "temperature": a.temperature,
                "top_p": a.top_p, "top_k": a.top_k, "logprobs": 0, "return_token_ids": True, "skip_special_tokens": False,
                "seed": random.getrandbits(31)}
        async with self.sem:
            async with session.post(f"{a.server}/v1/completions", json=body,
                                    timeout=aiohttp.ClientTimeout(total=None)) as r:
                r.raise_for_status()
                c = (await r.json())["choices"][0]
        return c["token_ids"], c["logprobs"]["token_logprobs"]

    async def rollout(self, session, row):
        messages = [dict(m) for m in row["prompt"]]
        extra = row["extra_info"]
        fixture = json.loads(extra["fixture_json"])
        request = parse.chat_request(messages, json.loads(extra["tools_json"]), TEMPLATE_KWARGS)
        prompt = self.prompt_ids(messages, request)
        tools_mode = row["agent_name"] == "btc_bench_tools"
        response, mask, logprobs = [], [], []
        info, reward, checks_used = new_info(), 0.0, 0
        while True:
            remaining = self.args.response_length - len(response)
            if remaining < 1:
                info["truncated"] = 1.0
                break
            ids, lps = await self.generate(session, prompt + response, remaining)
            response += ids
            mask += [1] * len(ids)
            logprobs += lps
            info["turns"] += 1
            if not ids or ids[-1] not in self.eos:
                info["truncated"] = 1.0
                break
            completion = parse.parse_completion(self.tok.decode(ids, skip_special_tokens=True), self.tok, request)
            try:
                if not tools_mode:
                    reward = record_reward(info, await reward_client.score(fixture, completion))
                    break
                result = await reward_client.turn(fixture, completion, checks_used)
            except reward_client.RewardUnavailable:
                reward, info["reward_unavailable"] = INVALID_REWARD_VALUE, 1.0
                break
            if result["done"]:
                reward = record_reward(info, result["reward"])
                break
            checks_used = result["checks_used"]
            replies = [{"role": "tool" if r["tool_call_id"] else "user", "content": r["content"]}
                       for r in result["replies"]]
            reply = self.separator + render_after_turn(self.pc, self.tok, replies, TEMPLATE_KWARGS)
            if len(response) + len(reply) >= self.args.response_length:
                info["truncated"] = 1.0
                break
            response += reply
            mask += [0] * len(reply)
            logprobs += [0.0] * len(reply)
        info["checks"], info["reward"] = float(checks_used), reward
        return {"task_id": extra["task_id"], "kind": extra["kind"], "prompt_ids": prompt,
                "response_ids": response, "response_mask": mask, "rollout_logprobs": logprobs,
                "reward": reward, "info": info}


async def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--parquet", required=True)
    ap.add_argument("--rows", help="comma-separated row indexes to roll out (default: random --prompts)")
    ap.add_argument("--prompts", type=int, default=8)
    ap.add_argument("--kind-weights", help="prompts per task kind, e.g. identify=2,tree=2,write=2 "
                                           "(overrides --prompts)")
    ap.add_argument("--n", type=int, default=8, help="rollouts per prompt (GRPO group size)")
    ap.add_argument("--adapter", required=True, help="served LoRA name, e.g. step-3")
    ap.add_argument("--server", default="http://127.0.0.1:18010")
    ap.add_argument("--model-path", required=True, help="tokenizer/processor of the served model")
    ap.add_argument("--temperature", type=float, required=True)
    ap.add_argument("--top-p", type=float, default=0.95)
    ap.add_argument("--top-k", type=int, default=20)
    ap.add_argument("--response-length", type=int, default=32768)
    ap.add_argument("--concurrency", type=int, default=4)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    df = pd.read_parquet(args.parquet)
    rng = random.Random(args.seed)
    if args.rows:
        idx = [int(i) for i in args.rows.split(",")]
    elif args.kind_weights:
        kinds = [x["kind"] for x in df["extra_info"]]
        idx = []
        for part in args.kind_weights.split(","):
            kind, count = part.split("=")
            idx += rng.sample([i for i, k in enumerate(kinds) if k == kind], int(count))
    else:
        idx = rng.sample(range(len(df)), args.prompts)
    roller = Roller(args)
    async with aiohttp.ClientSession() as session:
        jobs = [roller.rollout(session, df.iloc[i]) for i in idx for _ in range(args.n)]
        results = await asyncio.gather(*jobs)
    with open(args.out, "w") as f:
        for g, i in enumerate(idx):
            for r in results[g * args.n:(g + 1) * args.n]:
                f.write(json.dumps({**r, "group": g, "row": i, "adapter": args.adapter,
                                    "sampling": {"temperature": args.temperature, "top_p": args.top_p,
                                                 "top_k": args.top_k}}) + "\n")
    rewards = [r["reward"] for r in results]
    print(f"wrote {len(results)} rollouts ({len(idx)} groups) to {args.out}; mean reward "
          f"{sum(rewards) / len(rewards):.3f}")


if __name__ == "__main__":
    asyncio.run(main())
