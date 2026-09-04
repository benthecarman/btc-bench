#!/usr/bin/env python3
"""Minimal streaming chat REPL for an OpenAI-compatible endpoint.

Defaults to the SFT model on the 5090 vLLM server. Plain chat gives
prose answers; --submit offers the bench's submit_script tool so the
trained tool-call behavior fires (the call is printed when made).

Usage:
  chat.py                 # chat with qwen3-4b-sft on :8010
  chat.py --submit        # bench-style: system prompt + submit tool
  chat.py --url http://spark:18002/v1 --model Qwen/Qwen3-4B

Commands: /reset clears history, /quit exits.
"""

import argparse
import json
import urllib.request

SYSTEM = (
    "Solve the following Bitcoin Script task. Decide your answer, then "
    "submit it by calling the submit tool exactly once. You are in an "
    "automated pipeline: there is no one to ask, so do not ask questions."
)
SUBMIT_TOOL = {
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


def stream(url, body):
    req = urllib.request.Request(
        url + "/chat/completions",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    text, calls = "", {}
    with urllib.request.urlopen(req) as resp:
        for raw in resp:
            line = raw.decode().strip()
            if not line.startswith("data: ") or line == "data: [DONE]":
                continue
            chunk = json.loads(line[6:])
            if not chunk.get("choices"):
                continue
            delta = chunk["choices"][0].get("delta", {})
            piece = delta.get("content") or ""
            if piece:
                print(piece, end="", flush=True)
                text += piece
            for tc in delta.get("tool_calls") or []:
                slot = calls.setdefault(tc.get("index", 0), {"name": "", "args": ""})
                fn = tc.get("function", {})
                slot["name"] += fn.get("name") or ""
                slot["args"] += fn.get("arguments") or ""
    print()
    for slot in calls.values():
        print(f"\n[{slot['name']}] {slot['args']}")
    return text


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://localhost:8010/v1")
    ap.add_argument("--model", default="qwen3-4b-sft")
    ap.add_argument("--submit", action="store_true")
    args = ap.parse_args()

    history = [{"role": "system", "content": SYSTEM}] if args.submit else []
    print(f"chatting with {args.model} at {args.url} — /reset, /quit")
    while True:
        try:
            user = input("\nyou> ").strip()
        except (EOFError, KeyboardInterrupt):
            break
        if not user:
            continue
        if user == "/quit":
            break
        if user == "/reset":
            history = history[:1] if args.submit else []
            print("(history cleared)")
            continue
        history.append({"role": "user", "content": user})
        body = {
            "model": args.model,
            "messages": history,
            "temperature": 0.6,
            "top_p": 0.95,
            "stream": True,
        }
        if args.submit:
            body["tools"] = [SUBMIT_TOOL]
        print()
        reply = stream(args.url, body)
        history.append({"role": "assistant", "content": reply})


if __name__ == "__main__":
    main()
