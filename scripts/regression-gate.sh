#!/usr/bin/env bash
# Regression gate for model builds: run the fixed 60-task smoke set and
# compare against the stored baseline (tolerance via GATE_TOLERANCE).
#
#   scripts/regression-gate.sh <model-name>            # gate
#   scripts/regression-gate.sh <model-name> --update   # refresh baseline
set -euo pipefail
MODEL="${1:?usage: regression-gate.sh <model> [--update]}"
MODE="${2:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/release/btc-bench"
CONFIG="${GATE_CONFIG:-$ROOT/models.toml}"
BASELINE="$ROOT/runs/gate-baseline-$MODEL.json"

# Single-shot by design: the gate wants the fastest, least noisy read on
# a build; multi-turn would triple cost and measure feedback recovery.
"$BIN" run --dataset "$ROOT/datasets/smoke" --config "$CONFIG" \
    --model "$MODEL" --concurrency "${GATE_CONCURRENCY:-8}" --attempts 1 --out /tmp/gate-run
"$BIN" grade --dataset "$ROOT/datasets/smoke" \
    --responses /tmp/gate-run/responses.jsonl --out /tmp/gate-run/graded > /dev/null
python3 "$ROOT/scripts/gate_compare.py" /tmp/gate-run/graded/summary.md "$BASELINE" "$MODE"
