#!/usr/bin/env python3
"""Compare completed runs against every fixture, including missing answers."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path


def read_rows(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def correct(result):
    return result is not None and result.get("failure") in (None, "unimproved")


def summarize(fixtures, results):
    summary = {}
    for kind in sorted({task["task"] for task in fixtures}):
        rows = [results.get(task["id"]) for task in fixtures if task["task"] == kind]
        summary[kind] = {
            "total": len(rows),
            "correct": sum(correct(row) for row in rows),
            "mean_score": sum(row["score"] if row else 0 for row in rows) / len(rows),
            "failures": dict(Counter(
                "missing answer" if row is None else row.get("failure", "correct")
                for row in rows
            )),
        }
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", type=Path, required=True)
    parser.add_argument("--run", action="append", required=True, help="LABEL=RUN_DIRECTORY; first is baseline")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    fixtures_path = args.dataset / "fixtures.jsonl"
    fixtures = read_rows(fixtures_path)
    ids = {task["id"] for task in fixtures}
    if len(ids) != len(fixtures):
        raise ValueError("Duplicate fixture IDs")
    digest = hashlib.sha256(fixtures_path.read_bytes()).hexdigest()
    runs = {}
    for spec in args.run:
        label, directory = spec.split("=", 1)
        if label in runs:
            raise ValueError(f"Duplicate run label: {label}")
        directory = Path(directory)
        metadata = json.loads((directory / "run.json").read_text())
        if metadata["dataset_manifest"]["fixtures_sha256"] != digest:
            raise ValueError(f"Different fixtures in {directory}")
        results = json.loads((directory / "graded/results.json").read_text())
        by_id = {row["task_id"]: row for row in results}
        if len(by_id) != len(results) or set(by_id) - ids:
            raise ValueError(f"Duplicate or unexpected result IDs in {directory}")
        runs[label] = {"directory": str(directory), "results": by_id,
                       "summary": summarize(fixtures, by_id)}
    baseline = next(iter(runs))
    report = {"fixture_sha256": digest, "baseline": baseline, "runs": runs}
    lines = ["# Human benchmark comparison", "",
             "All fixtures remain in each denominator. Missing answers score zero.",
             "Correct means semantic equivalence; tree score also measures weight.", "",
             "| Run | Write correct | Tree correct | Tree mean score |",
             "|---|---:|---:|---:|"]
    for label, run in runs.items():
        parts = [f'{run["summary"][kind]["correct"]}/{run["summary"][kind]["total"]}'
                 for kind in ("write", "tree")]
        lines.append(f'| {label} | {parts[0]} | {parts[1]} | {run["summary"]["tree"]["mean_score"]:.3f} |')
        run["paired"] = {}
        for kind in ("write", "tree"):
            groups = {key: [] for key in ("both", "gained", "lost", "neither")}
            for task in fixtures:
                if task["task"] != kind:
                    continue
                old = correct(runs[baseline]["results"].get(task["id"]))
                new = correct(run["results"].get(task["id"]))
                key = "both" if old and new else "lost" if old else "gained" if new else "neither"
                groups[key].append(task["id"])
            run["paired"][kind] = groups
    lines += ["", f"Paired outcomes relative to {baseline}:", "",
              "| Run | Task | Both correct | Gained | Lost | Neither |",
              "|---|---|---:|---:|---:|---:|"]
    for label, run in runs.items():
        if label == baseline:
            continue
        for kind, groups in run["paired"].items():
            counts = " | ".join(str(len(groups[key])) for key in ("both", "gained", "lost", "neither"))
            lines.append(f"| {label} | {kind} | {counts} |")
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")
    (args.out / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
