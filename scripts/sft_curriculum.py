#!/usr/bin/env python3
"""Assemble the SFT curriculum from exported pool pairs.

Reads the sft.jsonl of each pool dataset and emits a taproot-heavy
curriculum targeting the write-bench failure profile: models write
CHECKMULTISIG inside tapscript (multi_a / OP_CHECKSIGADD is the rare
shape they never produce -- 59/84 tap write failures) and generally
fail the tapscript dialect. Composition:

- write/optimize: every tapscript-context pair is kept (this keeps
  all OP_CHECKSIGADD references by construction); legacy/segwit pairs
  are downsampled to half so the fused-CHECKSIGVERIFY forms stay
  ubiquitous without dominating.
- tree: kept whole (t4 IS the taproot design task).
- identify: small base sample (not a weak spot; label-only).

No pair is duplicated: oversampling means "keep all of the rare
context", never "repeat lines". Output is a pure function of the
pool files and the seed.

Usage: python3 scripts/sft_curriculum.py [--out datasets/sft-curriculum.jsonl]
"""

import argparse
import json
import random
from pathlib import Path

# Every generated pool is used by default: mixed-context pools plus
# the all-tap pools (gen --contexts tap) that feed the multi_a slice.
# Each pool contributes its formal export and, when present, its
# casual export (same targets, "hey write me a..." prompts) — the
# duplicate targets are deliberate: identical answers under different
# scaffolds teach scaffold invariance.
POOLS = sorted(
    str(p) for p in Path("datasets").glob("sft-pool-*") if (p / "sft.jsonl").exists()
)
EXPORTS = ["sft.jsonl", "sft-casual.jsonl"]
SEED = 7100
NON_TAP_KEEP_FRACTION = 0.5
# Identify was starved at 200 of 14,900 pairs (1.3%) because it
# "was not a weak spot"; it then became the weakest tier, at 1.00 on
# standard outputs but 0.00-0.83 on the protocol shapes, which are
# memorized taxonomy rather than a compositional skill. Keep all of
# them.
IDENTIFY_KEEP = 10_000


def tags(rec):
    out = set()
    asm = rec.get("target_asm", "") or ""
    if rec.get("style") == "casual":
        out.add("casual")
    if rec["kind"] in ("write", "optimize") and "tapscript" in rec.get("prompt", ""):
        out.add("tapscript")
    if rec["kind"] == "tree":
        out.add("taptree")
    if "OP_CHECKSIGADD" in asm:
        out.add("tap_multi")
    if "OP_CHECKSIGVERIFY" in asm:
        out.add("csv_fusion")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="datasets/sft-curriculum.jsonl")
    args = ap.parse_args()

    pairs = []
    seen = set()
    for pool in POOLS:
        for export in EXPORTS:
            path = Path(pool) / export
            if not path.exists():
                continue
            for line in open(path):
                rec = json.loads(line)
                # Answer-key dedup across pools (gen --exclude already
                # guarantees this for scripts; task_id alone would
                # falsely collapse same-index records from different
                # pools). The style is part of the key: the casual
                # duplicate of a formal pair is a distinct example.
                key = (
                    rec.get("style", "formal"),
                    rec.get("target_hex")
                    or rec.get("target_descriptor")
                    or (pool + rec["task_id"]),
                )
                if key in seen:
                    continue
                seen.add(key)
                rec["_tags"] = sorted(tags(rec))
                pairs.append(rec)

    rng = random.Random(SEED)
    curriculum = []
    non_tap = []
    identify = []
    for p in pairs:
        if p["kind"] == "identify":
            identify.append(p)
        elif p["kind"] == "tree" or "tapscript" in p["_tags"] or "casual" in p["_tags"]:
            curriculum.append(p)
        else:
            non_tap.append(p)
    rng.shuffle(non_tap)
    curriculum.extend(non_tap[: int(len(non_tap) * NON_TAP_KEEP_FRACTION)])
    rng.shuffle(identify)
    curriculum.extend(identify[:IDENTIFY_KEEP])
    rng.shuffle(curriculum)

    by_tag = {}
    by_kind = {}
    for p in curriculum:
        by_kind[p["kind"]] = by_kind.get(p["kind"], 0) + 1
        for t in p["_tags"]:
            by_tag[t] = by_tag.get(t, 0) + 1

    with open(args.out, "w") as f:
        for p in curriculum:
            rec = {k: v for k, v in p.items() if k != "_tags"}
            rec["tags"] = p["_tags"]
            f.write(json.dumps(rec) + "\n")

    print(
        json.dumps(
            {"total": len(curriculum), "by_kind": by_kind, "by_tag": by_tag},
            indent=1,
        )
    )


if __name__ == "__main__":
    main()
