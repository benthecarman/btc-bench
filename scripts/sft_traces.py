#!/usr/bin/env python3
"""Render the SFT curriculum with real reasoning traces.

The first SFT pass trained empty think blocks, which taught the model
not to reason at all — it emits an empty block even when the template
invites thinking, leaving RLVR nothing to improve. This builds the
same pairs with genuine derivations.

The traces are not distilled from a teacher and not rationalized
after the fact: every step is a field the generator already computed,
so the chain is correct by construction and unlimited in supply.

    English spec  ->  policy  ->  Miniscript  ->  script

Curriculum records are joined back to their fixtures on the answer
key, which `gen --exclude` guarantees is unique across pools.

Usage:
  sft_traces.py [--curriculum datasets/sft-curriculum.jsonl]
                [--out datasets/sft-train-think.jsonl]
                [--model Qwen/Qwen3-4B]
"""

import argparse
import glob
import json
import random

from transformers import AutoTokenizer

from sft_format import SYSTEM_PROMPT, tool_and_args

# Seeded phrasing variation, so the derivation is a habit of thought
# rather than a memorized preamble.
OPENERS = [
    "Let me work through the spending conditions.",
    "First, decompose what the spec requires.",
    "Break the requirement down before writing any script.",
]
POLICY_LEAD = [
    "As a policy that is:",
    "Written as a policy:",
    "That gives the policy:",
]
MS_LEAD = [
    "Compiling that to Miniscript for this context:",
    "The Miniscript for that policy:",
    "In Miniscript:",
]


def index_fixtures():
    """answer key -> fixture fields, across every pool."""
    idx = {}
    for path in sorted(glob.glob("datasets/sft-pool-*/fixtures.jsonl")):
        for line in open(path):
            fx = json.loads(line)
            inner = fx
            for v in fx.values():
                if isinstance(v, dict) and "id" in v:
                    inner = v
            for key in (
                "reference_script_hex",
                "optimal_script_hex",
                "reference_descriptor",
            ):
                if inner.get(key):
                    idx[inner[key]] = inner
    return idx


def script_observations(prompt):
    """Structural facts read off the decoded asm already in the prompt.

    Observation, not justification: every claim here is something
    visible in the script the model is looking at. The label follows
    from the shape; inventing a rationale for why a shape implies a
    protocol would be fabricated reasoning.
    """
    spk = inner = addr = ""
    for line in prompt.splitlines():
        if line.startswith("scriptPubKey"):
            spk = line.split(": ", 1)[-1]
        elif line.startswith("Address: "):
            addr = line.split(": ", 1)[-1]
        elif line.startswith("Redeem script"):
            inner = line.split(": ", 1)[-1]
    notes = []
    if addr:
        notes.append(f"the address form is {addr}")
    if spk.startswith("OP_HASH160") and spk.endswith("OP_EQUAL"):
        notes.append("the scriptPubKey is a P2SH wrapper (OP_HASH160 <20 bytes> OP_EQUAL)")
    elif spk.startswith("OP_0 ") and len(spk.split()[-1]) == 64:
        notes.append("the scriptPubKey is a v0 witness program with a 32-byte hash (P2WSH)")
    elif spk.startswith("OP_0 ") and len(spk.split()[-1]) == 40:
        notes.append("the scriptPubKey is a v0 witness program with a 20-byte hash (P2WPKH)")
    elif spk.startswith("OP_PUSHNUM_1 "):
        # P2TR and P2A are both v1 witness programs; only the program
        # length separates them. Calling every v1 program "Taproot"
        # taught the model to answer p2tr for p2a (17 of 18 wrong).
        prog = spk.split()[-1]
        if prog == "4e73":
            notes.append(
                "the scriptPubKey is OP_1 followed by the exact 2-byte "
                "program 4e73"
            )
        elif len(prog) == 64:
            notes.append(
                "the scriptPubKey is a v1 witness program with a 32-byte "
                "program (a Taproot output key)"
            )
        else:
            notes.append(
                f"the scriptPubKey is a v1 witness program with a "
                f"{len(prog) // 2}-byte program"
            )
    elif spk.startswith("OP_DUP OP_HASH160"):
        notes.append("the scriptPubKey is the P2PKH pattern")
    elif spk.startswith("OP_RETURN"):
        notes.append("the scriptPubKey starts with OP_RETURN, so the output is unspendable data")

    body = inner or spk
    if "OP_CHECKSEQUENCEVERIFY" in body or "OP_CSV" in body:
        notes.append("it carries a relative timelock (OP_CSV)")
    if "OP_CHECKLOCKTIMEVERIFY" in body or "OP_CLTV" in body:
        notes.append("it carries an absolute timelock (OP_CLTV)")
    if "OP_HASH160" in body and inner:
        notes.append("there is a HASH160 hashlock in the spending path")
    if "OP_CHECKMULTISIG" in body:
        notes.append("it ends in OP_CHECKMULTISIG, so the branch is a k-of-n multisig")
    if "OP_CHECKSIGADD" in body:
        notes.append("it counts signatures with OP_CHECKSIGADD, the tapscript multisig form")
    if "OP_IF" in body or "OP_NOTIF" in body:
        notes.append("the branches split on an OP_IF, so there is more than one spending path")
    if "OP_SIZE" in body:
        notes.append("an OP_SIZE check pins the preimage length, the HTLC pattern")
    if "OP_DROP" in body:
        notes.append("a value is dropped after being checked")
    keys = [w for w in body.split() if len(w) in (66, 64) and _is_hex(w)]
    if len(keys) > 1:
        notes.append(f"{len(keys)} public keys appear in the script")
    return notes


def _is_hex(w):
    try:
        int(w, 16)
        return True
    except ValueError:
        return False


def context_noun(inner):
    return {
        "legacy": "a P2SH redeem script",
        "segwitv0": "a P2WSH witness script",
        "tap": "a tapscript leaf",
    }.get(str(inner.get("context", "")).lower(), "a script")


def build_trace(rec, inner, rng):
    """A derivation from fields the generator already computed."""
    kind = rec["kind"]
    lines = [rng.choice(OPENERS)]

    if kind == "identify":
        lines = ["Read the script structure before naming it."]
        notes = script_observations(rec["prompt"])
        for n in notes:
            lines.append(f"- {n}")
        if not notes:
            lines.append("- the script does not match any wrapper pattern directly")
        lines.append(f"That shape is {rec['target_label']}.")
        return "\n".join(lines)

    if kind == "tree":
        lines.append(
            "This is a Taproot output, so the spending paths split between "
            "the key path and the tapleaves."
        )
        pol = inner.get("reference_policy")
        if pol:
            lines.append(f"{rng.choice(POLICY_LEAD)} {pol}")
        lines.append(
            "Putting the best-fitting branch on the key path where one fits, "
            "and nesting the rest as tapleaves:"
        )
        lines.append(rec["target_descriptor"])
        return "\n".join(lines)

    pol = inner.get("reference_policy")
    ms = inner.get("reference_miniscript")
    if kind == "optimize":
        bw, ow = inner.get("baseline_weight"), inner.get("optimal_weight")
        lines = [
            "The given script is correct, so the semantics have to be "
            "preserved exactly; only the encoding may change."
        ]
        if pol:
            lines.append(f"Its spending policy is: {pol}")
        if ms:
            lines.append(f"{rng.choice(MS_LEAD)} {ms}")
        if isinstance(bw, int) and isinstance(ow, int) and bw > ow:
            lines.append(
                f"That encoding costs {ow} weight units against the "
                f"original {bw}."
            )
    else:
        lines.append(f"The target is {context_noun(inner)}.")
        if pol:
            lines.append(f"{rng.choice(POLICY_LEAD)} {pol}")
        if ms:
            lines.append(f"{rng.choice(MS_LEAD)} {ms}")

    lines.append("Which encodes to:")
    lines.append(rec["target_asm"])
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--curriculum", default="datasets/sft-curriculum.jsonl")
    ap.add_argument("--out", default="datasets/sft-train-think.jsonl")
    ap.add_argument("--model", default="Qwen/Qwen3-4B")
    args = ap.parse_args()

    idx = index_fixtures()
    tok = AutoTokenizer.from_pretrained(args.model)
    rng = random.Random(4242)
    written = skipped = 0

    with open(args.out, "w") as out:
        for line in open(args.curriculum):
            rec = json.loads(line)
            key = rec.get("target_hex") or rec.get("target_descriptor")
            inner = idx.get(key)
            if inner is None and rec["kind"] != "identify":
                skipped += 1
                continue
            trace = build_trace(rec, inner or {}, rng)
            tool, call_args = tool_and_args(rec)
            messages = [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": rec["prompt"]},
            ]
            prompt_text = tok.apply_chat_template(
                messages,
                tools=[tool],
                add_generation_prompt=True,
                tokenize=False,
            )
            # The generation prompt already opens the think block for
            # thinking models; emit the trace, close it, then call.
            call = {
                "name": tool["function"]["name"],
                "arguments": call_args,
            }
            completion_text = (
                f"{trace}\n</think>\n\n<tool_call>\n"
                f"{json.dumps(call)}\n</tool_call><|im_end|>\n"
            )
            if not prompt_text.rstrip().endswith("<think>"):
                # Template did not open a think block; open it ourselves
                # so the target always contains a real trace.
                completion_text = "<think>\n" + completion_text
            out.write(
                json.dumps({"prompt": prompt_text, "completion": completion_text})
                + "\n"
            )
            written += 1
    print(f"wrote {written} traced pairs to {args.out} (skipped {skipped})")


if __name__ == "__main__":
    main()
