# rl27 results: local LoRA GRPO on Qwen3.8-27B

The step-28 adapter scores **0.401 on the headline suite against the base
model's 0.262**: +0.139, or **+53%** relative (95% CI [+0.086, +0.195]).
Write, optimize and tree improve beyond noise; identify and judgment
improve but within it. Later checkpoints scored lower on the held-out set
as answers doubled in length, so step 28 is the run's result.

These are the scores after commit 9c2ef62 (2026-10-09), which grades
working hand-written idioms (`<n> OP_CLTV OP_DROP` and others) on their
meaning instead of rejecting them at the Miniscript decode gate. The
strict-Miniscript scores the run was first reported with (0.381 vs
0.254) are reproduced by the grade report's strict line.

## Run

- Base: Qwen3.8-27B. Rollouts came from the NVFP4 checkpoint
  (`nvidia/Qwen3.8-27B-NVFP4`) with the adapter loaded in vLLM 0.27.1 on
  the RTX 5090. Training ran in bf16 on the DGX Spark (`rl/local/`).
- LoRA rank 32, alpha 32, on the Gated DeltaNet and attention projections
  and the MLP. lr 1e-4, 8 prompts × 8 rollouts per step, single-turn
  submit mode, 40 steps.
- Loss: group mean-centred advantages (no std), zero-variance groups
  dropped, truncated-IS REINFORCE (cap 2) against the rollout policy,
  per-group token mean, no KL.
- Batches 1–24 had a 32,768-token rollout budget; 25–40 had none. See
  DESIGN.md, "Length (to do)".
- Adapter: `/mnt/llm-models/huggingface/local/btc-bench/models/rl27-step-28`
  (with PROVENANCE.md). Run records are in `runs/rl27/`.

## Checkpoint selection (held-out sub80)

`datasets/heldout-test-1-sub80`: 80 tasks, mean of 2 samples per model
(step 8: 1 sample). This set chose the checkpoint, so its step-28 number
is optimistic. The headline suite below is the independent measurement.

| Checkpoint | Overall | Median answer |
|---|---:|---:|
| base | 0.314 | 17k tokens |
| step 8 | 0.393 | 6k |
| step 18 | 0.399 | 21k |
| **step 28** | **0.491** | 24k |
| step 38 | 0.429 | 43k |
| step 40 | 0.434 | 49k |

Step 40 against step 28: −0.057, P(better) 0.05 (`rl/local/decide.py`).

## Headline suite (bench-s42-lite, submit)

All 246 tasks, one attempt, no tools, no generation cap. Temperature 1.0,
top-p 0.95, top-k 20, thinking on, seed 20260904, the same NVFP4
checkpoint for both models. Base is the 27B baseline run
(`runs/baseline-27b/bench-s42-lite-submit`, served on the Spark).
Step 28 is `runs/rl27-bench/bench-s42-lite-submit` (served on the 5090,
2026-10-09). The comparison is paired by task, with a 10,000-resample
bootstrap. One sample per model, so per-kind results carry real noise.

| Kind | n | Base | Step 28 | Change | Relative | P(better) | 95% CI |
|---|---:|---:|---:|---:|---:|---:|---|
| write | 48 | 0.083 | 0.208 | +0.125 | +150% | 0.999 | [+0.042, +0.229] |
| optimize (weight) | 48 | 0.052 | 0.187 | +0.135 | +260% | 0.993 | [+0.026, +0.255] |
| identify | 52 | 0.538 | 0.596 | +0.058 | +11% | 0.881 | [−0.019, +0.154] |
| tree (weight) | 50 | 0.440 | 0.695 | +0.255 | +58% | 1.000 | [+0.126, +0.392] |
| judgment | 48 | 0.125 | 0.188 | +0.062 | +50% | 0.772 | [−0.062, +0.208] |
| **overall** | 246 | **0.254** | **0.381** | **+0.127** | **+50%** | 1.000 | [+0.076, +0.179] |

| | Base | Step 28 |
|---|---:|---:|
| Fully solved tasks | 61 | 87 |
| Unanswered (scored 0) | 3 | 4 |
| Mean tokens, solved | 16,272 | 14,370 |
| Mean tokens, unsolved | 61,959 | 40,384 |

Three of step 28's four unanswered tasks hit the 131k context without
submitting (one optimize, one identify, one judgment). The fourth was a
write task that ended without a tool call.

## Other models on the same suite

Same 246 tasks and v1 prompts, one attempt, no tools, no generation cap,
graded with the idiom rewrites. API models run at their defaults unless
an effort is named (Opus 5.5 low/high via `output_config.effort`).

| Model | write | optimize | identify | tree | judgment | overall | output tokens |
|---|---:|---:|---:|---:|---:|---:|---:|
| Qwen3.8-27B base | 0.104 | 0.052 | 0.538 | 0.440 | 0.146 | 0.262 | 12.3M |
| step-28 LoRA | 0.250 | 0.208 | 0.596 | 0.695 | 0.229 | 0.401 | 7.5M |
| GLM 5.3 Flash | 0.396 | 0.264 | 0.769 | 0.837 | 0.500 | 0.559 | 3.7M |
| GLM 5.3 | 0.729 | 0.625 | 0.673 | 0.740 | 0.667 | 0.687 | 6.7M |
| Claude Sonnet 5.5 | 0.979 | 0.705 | 0.865 | 0.979 | 0.938 | 0.893 | 179k |
| Claude Opus 5.5 (low) | 0.938 | 0.683 | 0.962 | 1.000 | 0.958 | 0.910 | 157k |
| Claude Opus 5.5 (medium) | 1.000 | 0.872 | 1.000 | 1.000 | 0.979 | 0.971 | 168k |
| Claude Opus 5.5 (high) | 1.000 | 0.799 | 1.000 | 1.000 | 0.958 | 0.953 | 193k |

- Opus 5.5 nearly saturates the suite. Before the idiom rewrites it
  scored 0.734: most of its gap was Miniscript spelling, not meaning.
  Effort barely matters: high vs medium is −0.018 (CI [−0.042,
  +0.003]); only optimize moves, where low is clearly worse.
- The GLM models ran through OpenCode Zen with `max_tokens = 131072`,
  their documented maximum; OpenCode's default of 65,536 truncated 16
  answers before it was set. GLM 5.3 has 12 unanswered tasks: 6 ran
  into the 131,072-token maximum without answering, and 6 ended with no
  finish reason or token count (dropped streams, not yet retried).
  Flash has 2 unanswered, both at the maximum.
- The Claude models answer in 157–193k output tokens for the whole
  suite; the open models use 3.7–12.3M.

## Caveats

- One sample per model on the headline suite. The overall gain is clear;
  identify and judgment are not distinguishable from noise.
- The relative numbers start from low bases. Optimize's +260% is +0.135 in
  absolute score.
- Step 28 trained mostly under the 32k rollout budget (batches 1–24), so
  it is not the result of an uncapped run. An uncapped rerun needs the
  length term proposed in DESIGN.md and the
  [length-control report](../reports/RL%20length%20control%20for%20reasoning.md).
- The base and step-28 runs were served from different hosts and batch
  sizes (Spark at concurrency 12, 5090 at 4) with the same checkpoint and
  sampling settings. That should not change the score distribution.
