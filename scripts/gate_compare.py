#!/usr/bin/env python3
"""Compare a graded summary against a stored baseline. Exit 1 on regression."""
import json, os, re, sys

summary_path, baseline_path = sys.argv[1], sys.argv[2]
update = len(sys.argv) > 3 and sys.argv[3] == "--update"
tolerance = float(os.environ.get("GATE_TOLERANCE", "0.05"))

text = open(summary_path).read()
metrics = {}
# Row shape: | <task> | <mean> | <95% CI or blank> | <n> |
# Match on name + leading mean only, so added columns never silently
# empty the metric set again (an empty set makes the gate vacuous).
for m in re.finditer(r"\| ([a-z ()]+) \| ([0-9.]+) \|", text):
    metrics[m.group(1).strip()] = float(m.group(2))
if not metrics:
    print(f"no metrics parsed from {summary_path}; summary format drift?")
    sys.exit(1)

if update or not os.path.exists(baseline_path):
    os.makedirs(os.path.dirname(baseline_path) or ".", exist_ok=True)
    with open(baseline_path, "w") as f:
        json.dump(metrics, f, indent=1)
    print(f"baseline written: {baseline_path}")
    for k, v in metrics.items():
        print(f"  {k:22} {v:.3f}")
    sys.exit(0)

base = json.load(open(baseline_path))
failed = False
print(f"{'metric':22} {'baseline':>9} {'candidate':>10} {'delta':>8}")
for k, v in metrics.items():
    b = base.get(k)
    if b is None:
        print(f"{k:22} {'-':>9} {v:10.3f} {'new':>8}")
        continue
    d = v - b
    mark = ""
    if d < -tolerance:
        mark = "  REGRESSION"
        failed = True
    print(f"{k:22} {b:9.3f} {v:10.3f} {d:+8.3f}{mark}")
sys.exit(1 if failed else 0)
