# RLVR and additional-SFT pilot results

Both methods start from compound-v3 checkpoint 144 and receive 32 optimizer updates. These are fixed final checkpoints. All evaluation questions have been observed in earlier experiments.

| Model | Development chat | Development submit | Transfer chat | Transfer submit | Human write chat | Human write submit | Human tree chat | Human tree submit |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| parent | 39/48 | 39/48 | 35/48 | 36/48 | 34/120 | 33/120 | 3/40 | 5/40 |
| rl | 39/48 | 40/48 | 33/48 | 41/48 | 33/120 | 32/120 | 3/40 | 5/40 |
| sft | 37/48 | 38/48 | 38/48 | 43/48 | 32/120 | 35/120 | 4/40 | 4/40 |

## Paired outcomes

| Arm | Suite | Interface | Task | Gained | Lost |
|---|---|---|---|---:|---:|
| rl | development | chat | write | 0 | 0 |
| rl | development | submit | write | 1 | 0 |
| rl | transfer | chat | write | 2 | 4 |
| rl | transfer | submit | write | 6 | 1 |
| rl | human | chat | tree | 0 | 0 |
| rl | human | chat | write | 1 | 2 |
| rl | human | submit | tree | 0 | 0 |
| rl | human | submit | write | 1 | 2 |
| sft | development | chat | write | 1 | 3 |
| sft | development | submit | write | 2 | 3 |
| sft | transfer | chat | write | 7 | 4 |
| sft | transfer | submit | write | 10 | 3 |
| sft | human | chat | tree | 1 | 0 |
| sft | human | chat | write | 1 | 3 |
| sft | human | submit | tree | 0 | 1 |
| sft | human | submit | write | 5 | 3 |

Every expected question remains in the denominator. Missing or unextractable answers fail. Counts measure semantic correctness; tree weight remains separate. Chat and submit share questions and are not independent trials.

See [the analysis](rl-compound-v3-pilot-v1-analysis.md), [spot checks](rl-compound-v3-pilot-v1-spot-check.md), and [design](rl-compound-v3-pilot-v1.md). Detailed results are in `runs/rl-compound-v3-pilot-v1-results.json`. The original model server is restored.
