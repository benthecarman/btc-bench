#!/usr/bin/env python3
"""Report operator shapes, not semantic novelty or historical leakage."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re


def shape(policy):
    """Erase atom values; retain operators, multiplicity and clock domains."""
    tokens = re.findall(r"[^(),\s]+|[(),]", policy)
    at = 0

    def parse():
        nonlocal at
        name = tokens[at].split("@")[-1]
        at += 1
        if at == len(tokens) or tokens[at] != "(":
            return name
        at += 1
        children = []
        while True:
            children.append(parse())
            token = tokens[at]
            at += 1
            if token == ")":
                break
            if token != ",":
                raise ValueError(f"bad policy separator {token}")
        if name == "pk":
            return ("pk",)
        if name in {"sha256", "hash160", "hash256", "ripemd160"}:
            return (name,)
        if name in {"after", "older"}:
            value = int(children[0])
            seconds = value >= 500_000_000 if name == "after" else bool(value & (1 << 22))
            return (name, "seconds" if seconds else "blocks")
        if name == "thresh":
            k, children = int(children[0]), children[1:]
            if k == len(children):
                name = "and"
            elif k == 1:
                name = "or"
            else:
                return ("thresh", k, *sorted(children, key=repr))
        if name not in {"and", "or"}:
            raise ValueError(f"unsupported operator {name}")
        flat = []
        for child in children:
            flat.extend(child[1:] if child[0] == name else [child])
        return (name, *sorted(flat, key=repr))

    result = parse()
    if at != len(tokens):
        raise ValueError("trailing policy tokens")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, default=Path("evals/human-v2.json"))
    parser.add_argument("--previous", type=Path, default=Path("evals/human-v1.json"))
    parser.add_argument("--training", type=Path, default=Path("datasets/sft-train-think.jsonl"))
    parser.add_argument("--out", type=Path, default=Path("evals/human-v2-coverage.md"))
    args = parser.parse_args()
    cases = json.loads(args.catalog.read_text())
    old = json.loads(args.previous.read_text())
    assert cases[:len(old)] == old, "previous cases changed"
    assert len({c["id"] for c in cases}) == len(cases), "duplicate IDs"
    new = cases[len(old):]
    old_shapes = {shape(c["policy"]) for c in old}
    train_shapes, rows, matched = set(), 0, 0
    for line in args.training.read_text().splitlines():
        row = json.loads(line)
        rows += 1
        found = re.search(r"^(?:As a policy that is|Written as a policy|That gives the policy|Its spending policy is): (.+)$", row["completion"], re.M)
        if found:
            train_shapes.add(shape(found[1]))
            matched += 1
    counts = Counter(c["kind"] for c in cases)
    groups = Counter(c["group"] for c in new)
    assert set(groups.values()) == {2}, "new cases must remain paired"
    lines = [
        "# Human-v2 coverage audit", "",
        f"{len(cases)} requests: {counts['write']} write and {counts['tree']} tree. "
        f"The original {len(old)} catalog entries are unchanged. "
        f"The {len(new)} additions form {len(groups)} pairs; the full set has "
        f"{len({c['group'] for c in cases})} scenario groups.", "",
        "These are fixed synthetic requests. A pair changes spending conditions, "
        "not just keys or wording. Groups can still share structures with other "
        "groups. The count of groups is not a count of independent observations.", "",
        "| Subset | Questions | Operator shapes | Questions with a shape in checked SFT traces |",
        "|---|---:|---:|---:|",
    ]
    for label, subset in [("Original", old), ("Added", new), ("All write", [c for c in cases if c['kind'] == 'write']), ("All tree", [c for c in cases if c['kind'] == 'tree']), ("Full set", cases)]:
        shapes = [shape(c["policy"]) for c in subset]
        lines.append(f"| {label} | {len(subset)} | {len(set(shapes))} | {sum(s in train_shapes for s in shapes)} |")
    new_shapes = {shape(c['policy']) for c in new}
    lines += ["", f"The additions include **{len(new_shapes - old_shapes)} operator shapes absent from human-v1**. "
              f"{sum(shape(c['policy']) not in old_shapes for c in new)} of the added requests have such a shape.", "",
              "## Method and limits", "",
              "Shapes erase key identity, hash values and lock values. They retain "
              "hash functions, absolute/relative lock domains, thresholds and repeated "
              "operands. AND/OR order is sorted and nested uses are flattened. "
              "Thresholds of one or all become OR or AND. This is a conservative "
              "operator comparison: it loses repeated-key roles and does not prove "
              "semantic novelty. It also does not equate all logically equivalent forms.", "",
              f"The training check found policy lines in {matched}/{rows} rows of "
              f"`{args.training}` ({len(train_shapes)} shapes). The four policy-line "
              "markers from `sft_traces.py` are recognized; other rows are excluded. This checks that stored file, not all "
              "historical training inputs. It does not certify a blind holdout.", "",
              "The new cases have no measured model score yet. Report correctness "
              "and tree weight separately. Keep pairs together when splitting data "
              "or estimating uncertainty; group further by policy family for a "
              "family-transfer experiment.", "",
              "## Provenance", ""]
    for path in [args.catalog, args.previous, args.training]:
        lines.append(f"- `{path}` SHA-256: `{hashlib.sha256(path.read_bytes()).hexdigest()}`")
    lines += ["", "Reproduce from the repository root:", "", "```bash", "python scripts/audit_human_catalog.py", "```", ""]
    args.out.write_text("\n".join(lines))
    print("\n".join(lines[:15]))


if __name__ == "__main__":
    main()
