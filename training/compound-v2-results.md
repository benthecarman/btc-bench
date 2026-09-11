# Expanded compound-approval results

Validation selects the checkpoint. The training-fit sample is diagnostic; neither set is a final test.

| Model | New arrangements | Held-out gated teams | Validation total | Training-fit sample |
|---|---:|---:|---:|---:|
| original | 0/24 | 0/24 | 0/48 | 0/24 |
| step96 | 20/24 | 2/24 | 22/48 | 19/24 |
| step192 | 21/24 | 2/24 | 23/48 | 24/24 |

The frozen selection rule chooses **step192**. Ties prefer fewer updates.

## Existing human-style benchmark

| Model | Write chat | Tree chat | Write submit | Tree submit |
|---|---:|---:|---:|---:|
| original | 32/120 | 1/40 | 34/120 | 4/40 |
| selected | 22/120 | 1/40 | 33/120 | 6/40 |

Counts measure semantic correctness and retain every expected question in the denominator. Tree weight is recorded separately in the raw comparisons.

The original server was restored after evaluation. The selected checkpoint remains a separate experiment. The existing-skill check covers human-v2 write and tree tasks; it does not measure every original task type.

See [the experiment design](compound-v2.md) and [example requests and references](compound-v2-examples.md). Full per-question results are in `runs/compound-v2-results.json`.

## Interpretation and limits

The new model solves 21/24 structurally new arrangements of familiar
approval types. That is useful transfer beyond exact training questions.
The result remains within this synthetic task distribution and its shared
vocabulary. Only 2/24 held-out gated-team questions pass, so transfer to
conditions on whole teams is weak.

The human-v2 chat write score falls by ten questions. Its paired outcomes
are six gained and sixteen lost. Submit write gains eleven and loses twelve,
for a net loss of one. Chat trees gain one and lose one; submit trees gain
three and lose one. This single-seed comparison is not a precise estimate
of expected performance, but it gives no basis to replace the original
assistant model. The original remains served on port 8010.

Training took 835.9 seconds for 192 updates. All 384 loss-mask checks passed;
all 192 new exported reference targets passed standard-mode grading. No
compiler fallback was needed. Validation and both selected-model human-v2
runs completed without transport failures or length-terminated answers.
The selected merged model is `runs/sft-compound-v2/merged-step192`, SHA-256
`4f1ff1138391c14eba84485a67754cfe347ee2040680a00ed8a766dd0a8c0b6b`.
The update-96 model is preserved separately.

## Check the intermediate explanations too

A separate check parses the generated policy and Segwit v0 Miniscript lines
as written, then checks their semantics against the reference. It does not
repair syntax. All 48 reference traces pass as positive controls. These
trace checks do not change the benchmark's final-script scores.

| Validation subset | Equivalent policy line | Equivalent Miniscript line | Correct final script |
|---|---:|---:|---:|
| New arrangements | 23/24 | 22/24 | 21/24 |
| Held-out gated teams | 1/24 | 0/24 | 2/24 |

For example, validation question 025 correctly implements a shared approval
that requires Tess, Mina, and a secret. Its assembly passes the grader, but
its explanation uses `and_v` with three children. The Miniscript parser
requires two. Both correct held-out final scripts have invalid Miniscript
lines. The policy-line counts also enforce the concrete policy grammar;
an informal three-part AND can express the intended idea while failing that
grammar. These are distinct measurements from script correctness.

Another held-out answer drops a committee's 288-block delay from its policy
and uses invalid deadline syntax. This is a policy-construction failure as
well as an encoding failure. The raw trace inputs and results are saved in
`runs/compound-v2-trace-input.jsonl` and `runs/compound-v2-trace-checks.jsonl`.
The checker is `crates/bench-cli/examples/check_trace_parts.rs`.

The next curriculum should teach conditions attached to complete teams and
include ordinary chat-answer examples, while retaining checks for both
interfaces. Keep this validation set as development data and use fresh
held-out compositions for a later transfer claim. Do not interpret its
checkpoint-selection score as a final test or replace the original model
based on this result.
