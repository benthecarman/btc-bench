"""Shared rollout settings and answer extraction; no GPU dependencies."""

import argparse
import json
import os
import math
import re

REWARD_URL = os.environ.get("BTCBENCH_REWARD_URL", "http://127.0.0.1:9900/reward/batch")
TEMPERATURE = 0.6
TOP_P = 0.95
TOP_K = 20
MIN_P = 0.0
NUM_GENERATIONS = 8
TRAIN_COMPLETION_LENGTH = 4096
TOOL_CALL_RE = re.compile(r"<tool_call>\s*(\{.*?\})\s*</tool_call>", re.DOTALL)


def add_sampling_args(parser, *, training=False):
    parser.add_argument("--temperature", type=float, default=TEMPERATURE)
    parser.add_argument("--top-p", type=float, default=TOP_P)
    parser.add_argument("--top-k", type=int, default=TOP_K)
    parser.add_argument("--min-p", type=float, default=MIN_P)
    parser.add_argument("--k", type=int, default=NUM_GENERATIONS)
    parser.add_argument(
        "--max-completion-length", type=int,
        default=TRAIN_COMPLETION_LENGTH if training else None,
        help="Trainer rollout budget; probe is uncapped unless explicitly set.",
    )


def validate_sampling(args):
    if args.k < 2:
        raise ValueError("--k must be at least 2 to measure group reward variation")
    if not math.isfinite(args.temperature) or not args.temperature > 0 or not 0 < args.top_p <= 1:
        raise ValueError("temperature must be positive and top-p must be in (0, 1]")
    if args.top_k < 0 or not 0 <= args.min_p <= 1:
        raise ValueError("top-k must be nonnegative and min-p must be in [0, 1]")
    if args.max_completion_length is not None and args.max_completion_length < 1:
        raise ValueError("--max-completion-length must be positive")


def add_thinking_arg(parser):
    parser.add_argument("--thinking", action=argparse.BooleanOptionalAction, default=True,
                        help="Use thinking prompts (default); --no-thinking for a non-thinking checkpoint.")


def validate_rows(rows, thinking):
    if not rows:
        raise ValueError("RL dataset is empty")
    for row in rows:
        if row.get("thinking") is not thinking:
            raise ValueError("RL prompt thinking mode is missing or differs; rerun rl_prepare.py with the intended --thinking/--no-thinking setting")
        fixture = json.loads(row["task_json"])
        if fixture.get("task") == "judgment" and fixture.get("contract_version") != 1:
            raise ValueError("Old judgment contract; regenerate the task pool and RL prompts")


def extract_answer(completion: str, *, thinking=False, finish_reason=None):
    """Read a final submit call. Never submit a call copied inside reasoning."""
    if finish_reason == "length":
        return ""
    if thinking and "</think>" not in completion:
        return ""
    if "</think>" in completion:
        completion = completion.rsplit("</think>", 1)[1]
    elif "<think>" in completion:
        return ""
    for match in TOOL_CALL_RE.finditer(completion):
        try:
            call = json.loads(match.group(1))
            if not isinstance(call, dict):
                continue
            args = call.get("arguments")
            if isinstance(args, str):
                args = json.loads(args)
            if not isinstance(args, dict):
                continue
            for name, field, task in [
                ("submit_script", "script", "script"),
                ("submit_descriptor", "descriptor", "descriptor"),
                ("submit_identify", "label", "identify"),
            ]:
                if call.get("name") == name and isinstance(args.get(field), str):
                    return {"task": task, field: args[field]}
        except (json.JSONDecodeError, TypeError):
            continue
    return completion.strip()
