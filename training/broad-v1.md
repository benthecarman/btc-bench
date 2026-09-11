# Broader curriculum with chat and replay

This batch was prepared for SFT from compound-v3 checkpoint 144.
It expands the narrow compound-approval curriculum across eight contract
families, while retaining chat answers and earlier skills. The preparation
checks below describe the data before training. The subsequent two-epoch
run is complete: see [results](broad-v1-results.md),
[analysis](broad-v1-analysis.md), and [spot checks](broad-v1-spot-check.md).

See [eight training examples with references](broad-v1-examples.md) and
[the frozen inputs](broad-v1-freeze.json).

## Training mix

| Source | Questions or original rows | Chat rows | Submit rows |
|---|---:|---:|---:|
| New broader questions | 512 | 512 | 512 |
| Compound replay | 128 | 128 | 128 |
| Original-skill replay | 416 | 416 | 416 |
| Total | 1,056 | 1,056 | 1,056 |

The new questions contain 256 scripts and 256 Taproot descriptors. Each
script/descriptor pair shares a scenario group. The descriptor adds an
explicit immediate owner route to the script's policy. There are 256 such
groups, with 512 distinct normalized operator structures; these are related
questions, not 512 independent scenarios.

The eight training families cover mandatory signers with alternative teams,
team recovery, delivery with a secret, joint department approval, three
spending routes, recovery quorums, combined absolute and relative clocks,
and a secret required across multiple routes. Each family has 32 groups.
Requests use names and complete spending conditions. Script requests ask
for Bitcoin script; their reference answers select P2WSH. Descriptor requests
state the owner bypass separately and explicitly scope the remaining rules
to the alternative arrangement.

Compound replay contains 64 questions from the retained v2 portion and 64
from the conditioned-team v3 portion. Original replay contains 256 script
examples, 112 tree examples, and 48 identification examples. Script replay
includes the original write/optimization curriculum. Replay source rows and
indices are saved in `broad-v1-mix.json`.

Every selected question has both a submit-tool answer and an ordinary chat
answer. Chat has no tool schema and uses the benchmark's helpful-assistant
system prompt. Script and descriptor finals are labelled code blocks.
Identification finals contain the label. The chat version of an identification
request asks for a label instead of a tool call. Reasoning and reference
answer bytes are preserved through conversion.

## Reserved composition check

There are 64 reserved questions in 32 script/descriptor groups. They come
from four separately authored composition families: three conditioned
departments, two independently locked departments, alternative signed
evidence bundles, and a threshold combining two secret approvals with a
two-clock approval.

The reserved structures are absent from this training mix and the checked
historical catalogs and SFT policy traces. The historical trace scan covers
14,700 policy-bearing rows in the original 16,696-row SFT file; the remaining
rows are identification examples. The complete exclusion source hashes are
saved in `broad-v1-coverage.json`. All derivatives of a scenario stay together.
Both SFT export and RL preparation reject the reserved pool.

These are synthetic requests with shared primitives and vocabulary, not an
independent collection of human conversations. Operator normalization is
not a proof of semantic novelty. No model has been tested on this reserved
set during preparation. Keep it out of training and checkpoint selection;
use it after the candidate model and inference settings are fixed.

## Reference and formatting checks

All 512 training and 64 reserved references pass the strict grader with full
credit. The compiler also checks policy equivalence and execution of the
script or descriptor leaves. All 512 new chat finals pass the benchmark's
actual answer extractor and strict grader. Script traces contain the exact
reference policy and Miniscript; descriptor traces contain the reference
policy and a verified descriptor with Miniscript leaves.

All 2,112 prompt/target pairs preserve their token boundary and round-trip
to the complete target text. The longest pair is 3,805 tokens, below the
4,096-token context. Three oversized original replay candidates were skipped
before the fixed replay counts were selected. The actual trainer's loss-mask
audit still needs to run before model training.

The 64 training references that use the simpler compiler fallback also pass
strict grading. They are valid teaching targets under this verifier; they
are not claims of minimal script weight. All reserved references use the
optimizing reference path.

Strict grading exposed a bug in the plain signature-threshold fallback:
it added unnecessary optional branches around signatures, creating multiple
dissatisfaction choices. The fallback now uses signatures' existing
dissatisfaction directly. A regression test checks semantic equivalence and
non-malleability in both Segwit v0 and Taproot contexts. All 167 Rust tests
and 15 Python tests pass.

Manual prompt review also removed ambiguous global wording around owner
bypass paths and corrected awkward signing clauses. Earlier drafts and
their failed checks are retained under `runs/broad-v1-draft1` through
`runs/broad-v1-draft3`.

## Limits and next run

The current tree compiler requires an immediate single-key route. Thus the
new descriptor questions include an owner bypass; committee-only Taproot
outputs still need compiler support. Reference verification uses the current
Miniscript oracle, not a signed-transaction test with real witnesses.

Use the balanced mixed file for the next SFT comparison, keeping the parent
checkpoint as a baseline. Choose any checkpoints using the existing observed
development suites in both interfaces. Test the reserved set only after
that choice. Measure reward variation again before a later RL comparison;
the 14-question pilot's variation counts do not describe this broader pool.

## Reproduction and artifacts

The catalog generators and mixed-data writer reject existing outputs.
Reproduce in a clean output workspace to preserve frozen data.

```bash
python training/build_broad_v1.py
cargo build --release -p bench-cli --example compile_training_catalog --example compile_validation_catalog --example reextract_chat
target/release/examples/compile_training_catalog training/broad-v1-training.json datasets/broad-v1-training
target/release/examples/compile_validation_catalog training/broad-v1-reserved.json datasets/broad-v1-reserved
python training/verify_broad_v1.py
```

Use the SFT environment for tokenizer-dependent preparation:

```bash
python scripts/rl_prepare.py --pool datasets/broad-v1-training --out datasets/rl-broad-v1-training.jsonl --model runs/sft-compound-v3/merged-step144
python scripts/prepare_composed_sft.py --data datasets/rl-broad-v1-training.jsonl --pool datasets/broad-v1-training --out datasets/sft-broad-v1-submit.jsonl --model runs/sft-compound-v3/merged-step144
python training/prepare_broad_v1.py
target/release/examples/reextract_chat datasets/broad-v1-training datasets/broad-v1-chat-targets.jsonl runs/broad-v1-chat-reference-check
target/release/btc-bench grade --dataset datasets/broad-v1-training --responses runs/broad-v1-chat-reference-check/responses.jsonl --out runs/broad-v1-chat-reference-check/graded --standard-mode
python training/report_broad_v1.py
```

The mixed training file is `datasets/sft-broad-v1-mixed.jsonl`, SHA256
`39cb0c7217a8878245d2fbff09b6f39f0836129df74791bcd7a894b56b03ba9c`.
Dataset, source, and parent-weight hashes are saved in `broad-v1-freeze.json`.
The raw reference and chat checks remain under `runs/broad-v1-*reference-check`.
