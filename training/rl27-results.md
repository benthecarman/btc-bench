# rl27 results: local LoRA GRPO on Qwen3.8-27B

The step-28 adapter scores **0.381 on the headline suite against the base
model's 0.254**: +0.127, or **+50%** relative. Write, optimize and tree
improve beyond noise; identify and judgment improve but within it.
Later checkpoints scored lower on the held-out set as answers doubled in
length, so step 28 is the run's result.

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
