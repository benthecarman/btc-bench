# Frozen composition transfer results

All four checkpoints scored **0/24 in both chat and submit modes**. No complete pair was solved. This set exposes a gap in compound approval construction, but success rate alone does not rank these models.

| Checkpoint | Chat correct | Submit correct | Complete pairs, chat / submit |
|---|---:|---:|---:|
| Original SFT | 0/24 | 0/24 | 0/12 / 0/12 |
| Composition SFT | 0/24 | 0/24 | 0/12 / 0/12 |
| Further-SFT control | 0/24 | 0/24 | 0/12 / 0/12 |
| RL pilot | 0/24 | 0/24 | 0/12 / 0/12 |

## Failure breakdown

| Checkpoint | Mode | Wrong semantics | Decode reject | Parse error | Unextractable |
|---|---|---:|---:|---:|---:|
| Original SFT | chat | 12 | 11 | 1 | 0 |
| Original SFT | submit | 16 | 8 | 0 | 0 |
| Composition SFT | chat | 14 | 8 | 2 | 0 |
| Composition SFT | submit | 13 | 9 | 2 | 0 |
| Further-SFT control | chat | 8 | 14 | 0 | 2 |
| Further-SFT control | submit | 8 | 11 | 4 | 1 |
| RL pilot | chat | 13 | 8 | 2 | 1 |
| RL pilot | submit | 9 | 12 | 2 | 1 |

Of 192 responses, 93 decoded and were confirmed semantically wrong. The remaining responses did not reach a semantic verdict. In particular, a Miniscript decode rejection does not prove that an arbitrary Bitcoin script is invalid or semantically wrong. These are scores under the current verifier.

All 192 requests produced complete transport records. Four generations reached the existing server context limit: control and RL, once in each mode. No separate generation or time cap was added.

## Concrete failures

- **Dropped gates:** the original model returned plain two-of-four signatures for the four-channel request. A and B can satisfy that script before the output is 80 blocks old, while the reference requires B's delay. The RL model made the same simplification.
- **Lost team boundaries:** the composition SFT model returned two-of-eight signatures for two complete teams. A1 and B1 suffice for that candidate, despite completing neither team. RL made the same error.
- **Construction failure after listing conditions:** RL listed the four separately delayed approvals in its trace, then used an invented `any2(...)` Miniscript expression and emitted a script rejected by the decoder.

The first two counterexamples follow from inspection of the saved scripts; they are not full transaction-signature executions. Requests, reference policies, verified Miniscripts and saved model traces are in `runs/transfer-v1-spot-checks.md`.

## Protocol and interpretation

The user accepted the clarified draft for evaluation. Dataset and model hashes, sampling settings, and the model list were recorded before generation in [the freeze record](composition-transfer-v1-freeze.json). Both dataset and catalog hashes were checked again after evaluation and were unchanged. The catalog remains evaluation-only.

All runs used the RTX 5090, the same vLLM configuration, a 32768-token context, thinking enabled, temperature 0.6, top-p 0.95, top-k 20, min-p 0, seed 20260904, and concurrency four. Each question had one attempt without diagnostic tools or grader feedback. Submit also changes the system instruction. Raw responses, errors, logs and configurations remain in `runs/transfer-v1-<model>-<mode>/`.

These are 12 related pairs from one composition family, not 24 independent tests of broad Bitcoin ability. Their structures are absent from the checked training policy traces, within the limits documented in [the training protocol](composed-training.md). No additional training occurred during this comparison.

Keep this as a focused stress test alongside human-v2, where scores have more room to distinguish small changes. The zero single-attempt result does not establish zero success probability under repeated sampling. A training pool needs its own reward-variation probe; these held-out questions must not become RL rollouts. If their observed failures guide future curriculum choices, treat subsequent scores as development evidence and reserve another untouched final test.

## Artifacts

- Detailed aggregate: `runs/transfer-v1-results.json`.
- Per-question comparisons: `runs/transfer-v1-comparison-chat/` and `runs/transfer-v1-comparison-submit/`.
- Execution and restoration status: `runs/transfer-v1-status.json`.
- Server controller and log: `runs/transfer-v1-driver.py` and `runs/transfer-v1-driver.log`.

The original `qwen3-4b-think` server was restored on port 8010 after the comparison.
