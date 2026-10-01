#!/usr/bin/env python3
"""Continue-or-stop decision for a local RL run, from held-out evals.

The rule is fixed before the evaluation it judges is run.

Per held-out task, average the score over the available samples of the
base model and of a candidate adapter (write, judgment, identify score
0/1; optimize and tree their weight scores). Delta = candidate - base
over all tasks, with a paired bootstrap over tasks (10,000 resamples):

- CONTINUE if P(delta > 0) >= 0.70 and no task kind regressed clearly
  (kind mean down by >= 0.15 with P(drop) >= 0.90);
- otherwise STOP.

    python3 rl/local/decide.py --run runs/rl27 --base base,base-r2 --candidate step-18,step-18-r2
"""

import argparse
import json
import random
import statistics

KINDS = {"t1": "write", "t2": "optimize", "t3": "identify", "t4": "tree", "t5": "judgment"}
P_IMPROVE, KIND_DROP, P_DROP, RESAMPLES = 0.70, 0.15, 0.90, 10_000


def scores(run, names, task_ids):
    """task_id -> mean score over the named eval dirs. graded/results.json
    lists answered tasks only; an unanswered task scores 0, as in
    `btc-bench grade`."""
    total = dict.fromkeys(task_ids, 0.0)
    for name in names:
        for r in json.load(open(f"{run}/eval/{name}/graded/results.json")):
            total[r["task_id"]] += float(r.get("score") or 0.0)
    return {t: v / len(names) for t, v in total.items()}


def bootstrap(deltas, rng):
    n = len(deltas)
    return [statistics.fmean(deltas[rng.randrange(n)] for _ in range(n)) for _ in range(RESAMPLES)]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--run", required=True)
    ap.add_argument("--base", required=True, help="comma-separated eval dir names")
    ap.add_argument("--candidate", required=True, help="comma-separated eval dir names")
    ap.add_argument("--out", help="write the decision JSON here")
    args = ap.parse_args()
    dataset = json.load(open(f"{args.run}/config.json"))["eval_dataset"]
    tasks = sorted(json.loads(l)["id"] for l in open(f"{dataset}/fixtures.jsonl") if l.strip())
    base = scores(args.run, args.base.split(","), tasks)
    cand = scores(args.run, args.candidate.split(","), tasks)
    rng = random.Random(20261001)
    deltas = [cand[t] - base[t] for t in tasks]
    boot = bootstrap(deltas, rng)
    result = {"base": args.base, "candidate": args.candidate, "tasks": len(tasks),
              "base_mean": statistics.fmean(base[t] for t in tasks),
              "candidate_mean": statistics.fmean(cand[t] for t in tasks),
              "delta": statistics.fmean(deltas), "p_improve": sum(b > 0 for b in boot) / RESAMPLES,
              "delta_ci95": [sorted(boot)[int(0.025 * RESAMPLES)], sorted(boot)[int(0.975 * RESAMPLES)]],
              "kinds": {}}
    regressed = []
    for prefix, kind in KINDS.items():
        kd = [cand[t] - base[t] for t in tasks if t.startswith(prefix)]
        if not kd:
            continue
        kb = bootstrap(kd, rng)
        k = {"n": len(kd), "base": statistics.fmean(base[t] for t in tasks if t.startswith(prefix)),
             "candidate": statistics.fmean(cand[t] for t in tasks if t.startswith(prefix)),
             "delta": statistics.fmean(kd), "p_drop": sum(b < 0 for b in kb) / RESAMPLES}
        result["kinds"][kind] = k
        if k["delta"] <= -KIND_DROP and k["p_drop"] >= P_DROP:
            regressed.append(kind)
    result["regressed_kinds"] = regressed
    result["rule"] = (f"CONTINUE if p_improve >= {P_IMPROVE} and no kind down >= {KIND_DROP} "
                      f"with p_drop >= {P_DROP}; else STOP")
    result["decision"] = "CONTINUE" if result["p_improve"] >= P_IMPROVE and not regressed else "STOP"
    text = json.dumps(result, indent=2)
    print(text)
    if args.out:
        open(args.out, "w").write(text + "\n")


if __name__ == "__main__":
    main()
