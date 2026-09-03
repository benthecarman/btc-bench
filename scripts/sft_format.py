#!/usr/bin/env python3
"""Render the SFT curriculum into prompt/completion pairs.

Each pair reproduces the runner conversation exactly: the runner's
system prompt, the submit tool the runner offers for that task kind,
the task prompt as the user turn — and the completion is the
assistant turn *calling the submit tool* with the reference answer.
Training the tool-call envelope jointly with the content is the
point: the benched failure mode includes never calling the tool.

Completions carry an empty think block (the Qwen3 non-thinking
convention): the dialect needs recall, not derivation, and the
bench measured 6k-token thinking runs producing nothing gradable.
RL can reintroduce deliberate thinking later.

Output: JSONL of {"prompt": <rendered>, "completion": <rendered>}
with loss to be applied on the completion only.

Usage:
  sft_format.py [--curriculum datasets/sft-curriculum.jsonl]
                [--out datasets/sft-train-rendered.jsonl]
                [--model Qwen/Qwen3-4B]
"""

import argparse
import json

from transformers import AutoTokenizer

# Mirrors bench-cli runner.rs SYSTEM_PROMPT verbatim.
SYSTEM_PROMPT = (
    "Solve the following Bitcoin Script task. Decide your answer, then "
    "submit it by calling the submit tool exactly once. You are in an "
    "automated pipeline: there is no one to ask, so do not ask questions."
)

# Mirrors the runner's submit tool schemas (names, params, descriptions).
SUBMIT_SCRIPT = {
    "type": "function",
    "function": {
        "name": "submit_script",
        "description": "Submit the final Bitcoin script answer.",
        "parameters": {
            "type": "object",
            "properties": {
                "script": {
                    "type": "string",
                    "description": "The script, as hex or Bitcoin Core asm.",
                }
            },
            "required": ["script"],
        },
    },
}
SUBMIT_DESCRIPTOR = {
    "type": "function",
    "function": {
        "name": "submit_descriptor",
        "description": "Submit the final descriptor answer.",
        "parameters": {
            "type": "object",
            "properties": {
                "descriptor": {
                    "type": "string",
                    "description": "The descriptor, e.g. tr(KEY,{...}).",
                }
            },
            "required": ["descriptor"],
        },
    },
}
SUBMIT_IDENTIFY = {
    "type": "function",
    "function": {
        "name": "submit_identify",
        "description": "Submit the identification answer.",
        "parameters": {
            "type": "object",
            "properties": {
                "label": {"type": "string", "description": "The family label."}
            },
            "required": ["label"],
        },
    },
}


def tool_and_args(rec):
    kind = rec["kind"]
    if kind in ("write", "optimize"):
        return SUBMIT_SCRIPT, {"script": rec["target_asm"]}
    if kind == "tree":
        return SUBMIT_DESCRIPTOR, {"descriptor": rec["target_descriptor"]}
    if kind == "identify":
        return SUBMIT_IDENTIFY, {"label": rec["target_label"]}
    raise ValueError(kind)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--curriculum", default="datasets/sft-curriculum.jsonl")
    ap.add_argument("--out", default="datasets/sft-train-rendered.jsonl")
    ap.add_argument("--model", default="Qwen/Qwen3-4B")
    args = ap.parse_args()

    tok = AutoTokenizer.from_pretrained(args.model)
    n = 0
    with open(args.out, "w") as out:
        for line in open(args.curriculum):
            rec = json.loads(line)
            tool, call_args = tool_and_args(rec)
            messages = [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": rec["prompt"]},
            ]
            prompt_text = tok.apply_chat_template(
                messages,
                tools=[tool],
                add_generation_prompt=True,
                enable_thinking=False,
                tokenize=False,
            )
            completion_messages = messages + [
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "type": "function",
                            "function": {
                                "name": tool["function"]["name"],
                                "arguments": call_args,
                            },
                        }
                    ],
                }
            ]
            full_text = tok.apply_chat_template(
                completion_messages,
                tools=[tool],
                add_generation_prompt=False,
                enable_thinking=False,
                tokenize=False,
            )
            if not full_text.startswith(prompt_text):
                raise SystemExit(
                    f"template did not extend the prompt for {rec['task_id']}"
                )
            completion_text = full_text[len(prompt_text) :]
            out.write(
                json.dumps({"prompt": prompt_text, "completion": completion_text})
                + "\n"
            )
            n += 1
    print(f"rendered {n} pairs to {args.out}")


if __name__ == "__main__":
    main()
