# Composition training pilot results

Human-v2 is not saturated. None of these pilots beats the original SFT
checkpoint. Keep that checkpoint as the working baseline.

## Frozen human-v2 evaluation

Each run has 120 write and 40 tree requests, one attempt, and no grader
feedback. Missing or unextractable answers count as failures. Correctness
means semantic equivalence; tree weight is reported separately in the raw
reports. Every correct tree in these runs also earned full weight credit.

| Checkpoint | Chat write | Chat tree | Submit write | Submit tree |
|---|---:|---:|---:|---:|
| Original SFT | 32/120 | 1/40 | 34/120 | 4/40 |
| Composition SFT, assembly targets | 22/120 | 0/40 | 29/120 | 4/40 |
| Assembly parent + 24 further-SFT updates | 23/120 | 0/40 | 29/120 | 2/40 |
| Assembly parent + 24 RL updates | 23/120 | 0/40 | 33/120 | 3/40 |

The table uses RTX 5090 inference with the existing ngram decoding settings.
Temperature is 0.6, top-p 0.95, top-k 20, min-p 0, seed 20260904, and
concurrency four. Thinking is enabled. Server context is 32768; the benchmark
adds no generation or time cap. Submit also changes the system instruction,
so chat versus submit is an interface comparison, not a tool-schema-only test.

An earlier further-SFT chat run on the RTX 5060 Ti without speculative
decoding scored 27/120 write and 0/40 tree. The same-hardware repeat was chosen
before seeing its scores. Both runs remain saved. A fixed seed does not make
different server configurations produce identical samples.

The failed first composition pilot used hex targets and a shorter trace.
It scored 8/120 write and 0/40 tree in chat. Its results also remain saved;
the assembly repeat did not erase this failed experiment.

## What changed

Against the assembly parent, both final branches gained two chat write
answers and lost one. RL's gains were the five-custodian unanimous policy
and the two-committee quorum policy. The parent failed their decode and
parse gates respectively. RL lost the ordinary escrow answer at extraction.
This does not establish a broad gain in policy reasoning.

RL did better than the further-SFT control in submit mode, but still fell
short of the original checkpoint. One training seed and 24 updates are not
enough to establish which method works better at scale. The two branches
also differ in question exposure and training hardware; they are not matched
for compute. See [the experiment protocol](composed-training.md).

The RL run sampled 192 completions from 24 question groups. Seven groups had
mixed rewards. Four completions hit the 4096-token training limit and were
assigned zero reward and masked from the loss. Uniform-reward groups provide
no relative reward signal, even when held-out benchmark accuracy is low.

## Fixed training-pool probe

Each checkpoint sampled eight answers on the same 40 training-pool tasks:
28 write and 12 tree. Prepared prompts, sampling parameters, task order and
seed were checked for equality across the three new checkpoints. This probe
uses the explicit 4096-token training budget; truncated answers score zero.
These are training-pool diagnostics, not independent held-out scores.

| Checkpoint | Write correct | Tree correct | Mean tree reward | Varying groups |
|---|---:|---:|---:|---:|
| Original SFT | 63/224 | 0/96 | 0.000 | 2/40 |
| Assembly parent | 82/224 | 2/96 | 0.021 | 11/40 |
| Further SFT | 88/224 | 5/96 | 0.052 | 14/40 |
| RL | 87/224 | 11/96 | 0.115 | 14/40 |

Write reward is binary correctness. Tree reward is the benchmark's continuous
weight score, gated on equivalence. Calling all rewards binary was incorrect:
one correct control answer received less than full weight credit. No sampled
equivalent tree in this probe received zero reward. The reward function was
the same throughout; the corrected report separates correctness from reward.

RL improved this small tree probe, while human-v2 tree results did not
improve. This supports investigating transfer between prompt and policy
distributions. It does not establish that a larger RL run will fix transfer.
Samples within each group are related, and the probe has only 12 tree tasks.

## Artifacts

- Chat comparison: `runs/composed-pilot-comparison-chat/summary.md`.
- Submit comparison: `runs/composed-pilot-comparison-submit/summary.md`.
- Paired question IDs and failure classes: `comparison.json` in those folders.
- Request, policy and Miniscript spot checks:
  `runs/composed-pilot-comparison-chat/spot-checks.md`.
- Original runs: `runs/human-v2-sft` and `runs/human-v2-sft-submit`.
- Assembly runs: `runs/human-v2-composed-asm` and its `-submit` counterpart.
- Control runs: `runs/human-v2-composed-control-gpu0` and
  `runs/human-v2-composed-control-submit`; the first chat run is
  `runs/human-v2-composed-control`.
- RL runs: `runs/human-v2-composed-rl` and its `-submit` counterpart.

Raw outputs and checkpoints are retained. The fresh 24-question composition
transfer draft has not been run on a model. Its wording still needs the
user's independent review before it serves as the next holdout.
