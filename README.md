# btc-bench

A benchmark for AI models that write, optimize, and identify Bitcoin
Script. Graded by a judge-free oracle: every answer is verified
mechanically, no LLM judge anywhere.

## Tasks

| Task | What the model does | Grading |
|------|-------------------|--------|
| **Write** | Compose a script from an English spending policy | Decode-gated as Miniscript, then proven semantically equivalent by exhaustive truth-table evaluation over the task's closed atom set |
| **Optimize** | Shrink a deliberately naive baseline script | Equivalence-gated, scored by weight improvement toward the compiler optimum |
| **Identify** | Label a scriptPubKey (plus redeem/witness script) with its protocol family | Label-only, binary: right family or wrong family (parameter naming is a vocabulary tax, not comprehension) |
| **Tree** | Design a full Taproot output — internal key + script tree — as a `tr()` descriptor | Lifted-semantics equivalence (unspendable key pinned false), then scored on worst-case weight between a single-leaf baseline and a balanced reference tree |

Three script contexts: legacy (P2SH redeemScript), segwit v0 (P2WSH
witnessScript), and taproot (script-path leaf tapscript).

## Quick start

```bash
cargo build --release

# Generate a fixture dataset (deterministic per seed)
btc-bench gen --out datasets/my-set --seed 42 --write 300 --optimize 300 --identify 18 --tree 150

# Generate a training set: non-eval English phrasings with varied
# structure, custom tier mix, and no task whose answer key appears in
# the eval set (family 0 stays bench-only)
btc-bench gen --out datasets/train --seed 7 --write 5000 --optimize 5000 \
    --verbal-families 1,2 --vary-structure \
    --tiers easy,medium,medium,hard --exclude datasets/my-set

# Run against a model (any OpenAI-compatible endpoint)
btc-bench run --dataset datasets/my-set --config models.toml --model my-model --concurrency 8

# Multi-turn with graded feedback between attempts (default; --attempts 1 for single-shot)
btc-bench run --dataset datasets/my-set --config models.toml --model my-model

# Tool-assisted: the model gets check_script / check_descriptor —
# the compiler-and-lint loop a developer has (reference-free)
btc-bench run --dataset datasets/my-set --config models.toml --model my-model \
    --tools basic --attempts 1

# Grade results
btc-bench grade --dataset datasets/my-set --responses runs/my-run/responses.jsonl

# With multi-turn scoring and token efficiency
btc-bench grade --dataset datasets/my-set --responses runs/my-run/responses.jsonl \
    --attempts runs/my-run/attempts.jsonl

# Re-verify a dataset (answer keys, weights, spendability)
btc-bench audit --dataset datasets/my-set

# Gate insanity findings (malleable, unsafe, ...) instead of just reporting them
btc-bench grade --dataset datasets/my-set --responses runs/my-run/responses.jsonl \
    --out runs/my-run/graded --standard-mode
```

## Datasets

`datasets/` is not tracked in git. The benchmark set is pinned by its
generator seed plus the dependency versions in the manifest: the same
seed and the same pins regenerate byte-identical fixtures, and
`btc-bench audit` re-verifies every answer key after regeneration.
Ship the fixture files themselves when publishing results, so scores
stay comparable even if a dependency bump ever shifts compiler output.

## Model configuration

Copy `models.example.toml` to `models.toml` and fill in your endpoints:

```toml
[model.my-model]
provider = "openai_compatible"
model = "your-model-name"
base_url = "http://localhost:8000/v1"
```

Multiple `base_url` entries are load-balanced round-robin:

```toml
base_url = [
    "http://workstation:8000/v1",
    "http://spark:8001/v1",
]
```

## What the oracle proves

The correctness oracle is structurally anti-cheat:

- **Decode gate**: answers must parse as valid, type-checked Miniscript
  in the task's script context. An always-true script (`OP_1`) decodes
  fine but fails equivalence — it scores 0.
- **Execution cross-check**: at fixture build (and audit) time the
  reference and baseline are proven spendable end-to-end — a real
  witness from the crate satisfier (known hash preimages, real
  timelocks, assumed signatures), executed by the crate interpreter
  under P2SH / P2WSH / P2TR wrapping. A second, independent oracle
  that shares no code with the truth table.
- **Insanity lint**: decoded answers are analyzed with miniscript's
  own sanity predicates (malleability, signature-free paths, resource
  limits, repeated keys). Findings are reported per task in results
  and in multi-turn feedback; `grade --standard-mode` turns them into
  a gate.
- **Exhaustive truth-table equivalence**: every generated task has a
  closed atom set (keys, hash preimages, timelocks). Both scripts are
  evaluated over all assignments; any single-row divergence fails.
  Complete because the generator bounds the atom space.
- **No answer leak**: multi-turn feedback names parse errors verbatim
  (they're mechanical facts) but equivalence failures never reveal the
  distinguishing assignment. Test-pinned.
- **Tree tasks**: the answer is a `tr()` descriptor, so grading lifts
  both sides to semantic policies and runs the same truth-table
  equivalence — with the provided unspendable (NUMS) internal key
  pinned unsatisfiable on both sides, since it can never sign. A
  correct design then earns credit for tree quality: worst-case input
  weight between the single-leaf baseline and a balanced reference
  tree (beating the reference clamps to full marks).

## Train/eval hygiene

- **Template families**: the English verbalizer has multiple
  hand-written template families per policy node. `--verbal-families`
  takes an explicit id list, so the train/eval split is enforced at
  generation time: family 0 is bench-only, training sets list only
  `1,2`. Every family is authored per AST node, so a paraphrase can
  never drift from the policy semantics. The sweep report breaks
  scores down by family to expose phrasing overfit.
- **Structural variation** (`--vary-structure`): and/or/thresh are
  commutative, so training prose permutes their children (seeded) and
  varies the root list shape (inline, numbered, spending-path
  framing) — the policy and answer key are untouched. This varies the
  clause tree the model must parse, not just the words; without it, a
  model can overfit the fixed template skeleton that all families
  share. Eval structure stays canonical.
- **Dedup**: `gen --exclude <eval-set>` resamples any task whose
  answer key appears in the excluded dataset (same-seed reuse is the
  realistic contamination path).
- **Canary**: every manifest carries a BIG-bench-style canary GUID. A
  model that can reproduce it trained on the dataset; scores on it are
  void.
- **Difficulty axis**: fixtures record the policy's boolean atom count
  (`atoms`), so curricula can bucket finer than the three tiers and
  the report shows score by atom count.

Summaries and sweep reports carry 95% bootstrap CIs and a
format-vs-reasoning split ("semantic accuracy given well-formed") so
parse failures are never mistaken for reasoning failures.

## Identify families

**Standards**: P2PK, P2PKH, P2WPKH, bare/P2SH/P2WSH multisig, P2TR,
OP_RETURN, P2A anchor, ordinals inscription.

**Lightning** (BOLT 3 + bolt-simple-taproot PR #1330): to_local,
to_remote under anchors, keyed anchors, offered/received HTLC (with and
without the anchors CSV clause), TR to_local, TR to_remote, TR anchor,
TR offered/accepted HTLC timeout tapleaves.

**Liquid**: federation peg (N-of-M with CSV-gated emergency backup).

## Tool-assisted mode

`run --tools basic` offers diagnostic tools beside the submit tool:
`check_script` (parse, decode gate, lint, weight) for write/optimize
and `check_descriptor` for tree tasks; identify stays tool-less.
Diagnostics are pure functions of *model-supplied* input — no tool
takes a fixture, so no tool can leak a reference, by construction.
Budget: 16 calls per task; the count lands in `responses.jsonl` as
`tool_calls` and summaries report call efficiency (solved vs
unsolved). Compare a `--tools none` run against a `--tools basic`
run of the same model: the delta is the mechanical-formatting
deficit; what remains at `basic` is the semantic gap. Keep the
headline single-shot/no-tools.

## Reward service for RL

```bash
# Unshaped: shaped == benchmark score
btc-bench reward-serve --bind 0.0.0.0:9900 --threads 8

# Shaped for training: rungs for parse/decode, a band paid by
# balanced truth-table agreement, and a lint penalty
btc-bench reward-serve --bind 0.0.0.0:9900 \
    --shape-parse 0.05 --shape-decode 0.10 --shape-agreement 0.25 \
    --shape-equivalent-floor 0.3 --lint-penalty 0.05
```

POST `/reward` with `{"task": <fixture>, "answer": "...", "shaping"?: {...}}`
(also `/reward/batch`, GET `/health`, and POST `/tool` — the same
reference-free diagnostics the tool-assisted runner offers, for RL
trainers driving their own rollout loops). Every reward response
carries:

- `score` — the benchmark score, identical to `btc-bench grade`;
- `shaped` — the training reward: score plus the configured shaping
  (per-request `shaping` overrides the server default, so one server
  can serve eval and training at once);
- `components` — the raw signals (parsed / decoded / equivalent /
  agreement / lint count) for logging or custom recombination.

The agreement band uses *balanced* truth-table agreement: the mean of
the agreement rates on reference-true and reference-false rows.
Constant scripts (`OP_1`, always-false) cap at 0.5 regardless of table
skew, and the band normalizes 0.5 to zero — so the dense signal pays
only for real semantic progress, never for the always-true hack. The
server refuses shaping configs where a non-equivalent answer could
earn more than 0.5. Requests are handled by a thread pool; grading is
milliseconds per answer.

## SFT export

```bash
btc-bench sft-export --dataset datasets/train
```

Writes one JSONL line per task: the exact runner prompt plus the
reference answer (`target_hex` + `target_asm` for write/optimize,
`target_label` for identify). Pair with
`--verbal-families 1,2 --vary-structure --exclude <eval-set>` at gen
time for cold-start SFT data that never touches the eval surface.

## Regression gate

```bash
scripts/regression-gate.sh my-model          # gate against baseline
scripts/regression-gate.sh my-model --update # refresh baseline
```

Runs a fixed 60-task smoke set and diffs against a stored per-model
baseline. Exits nonzero on regression.

## Stack

| Crate | Version |
|-------|---------|
| miniscript | 13.1.0 |
| bitcoin | 0.32.102 |
| goose-providers | 0.1.0-alpha.7 |
| Workspace MSRV | 1.94.1 |

## License

MIT
