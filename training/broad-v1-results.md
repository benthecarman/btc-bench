# Broad SFT results

The frozen development rule selected **step528**.

Training used 2,112 chat/submit rows, two epochs, 528 updates, and a learning rate of 0.00002. Both checkpoints start from compound-v3 checkpoint 144. All actual trainer loss masks passed.

## Checkpoint selection

All suites in this table were observed before this experiment. They are development data. Selection uses the mean of eight task/interface accuracy cells. Ties prefer fewer updates, including the unchanged parent.

| Model | Compound v2 chat / submit | Compound v3 chat / submit | Human write chat / submit | Human tree chat / submit | Mean |
|---|---:|---:|---:|---:|---:|
| parent | 39/48 / 39/48 | 35/48 / 36/48 | 34/120 / 33/120 | 3/40 / 5/40 | 0.483 |
| step264 | 33/48 / 41/48 | 30/48 / 33/48 | 36/120 / 45/120 | 6/40 / 17/40 | 0.513 |
| step528 | 34/48 / 39/48 | 35/48 / 36/48 | 37/120 / 44/120 | 7/40 / 16/40 | 0.531 |

## Reserved compositions

These 64 questions were first run after selection was saved. They contain 32 related script/descriptor pairs. They are synthetic composition checks with shared primitives and wording; they are not independent human conversations.

| Model | Script chat | Script submit | Tree chat | Tree submit |
|---|---:|---:|---:|---:|
| parent | 0/32 | 0/32 | 1/32 | 0/32 |
| step528 | 6/32 | 6/32 | 6/32 | 3/32 |

| Reserved family | Model | Script chat / submit | Tree chat / submit |
|---|---|---:|---:|
| three-conditioned-departments | parent | 0/8 / 0/8 | 0/8 / 0/8 |
| three-conditioned-departments | step528 | 2/8 / 1/8 | 0/8 / 0/8 |
| two-locked-departments | parent | 0/8 / 0/8 | 1/8 / 0/8 |
| two-locked-departments | step528 | 3/8 / 5/8 | 2/8 / 2/8 |
| alternative-evidence-bundles | parent | 0/8 / 0/8 | 0/8 / 0/8 |
| alternative-evidence-bundles | step528 | 0/8 / 0/8 | 4/8 / 1/8 |
| two-secrets-and-clock-vote | parent | 0/8 / 0/8 | 0/8 / 0/8 |
| two-secrets-and-clock-vote | step528 | 1/8 / 0/8 | 0/8 / 0/8 |

## Paired changes

| Suite | Interface | Task | Gained | Lost | Both correct | Neither |
|---|---|---|---:|---:|---:|---:|
| compound-v2-validation | chat | write | 3 | 8 | 31 | 6 |
| compound-v2-validation | submit | write | 3 | 3 | 36 | 6 |
| compound-v3-fresh | chat | write | 6 | 6 | 29 | 7 |
| compound-v3-fresh | submit | write | 6 | 6 | 30 | 6 |
| human-v2 | chat | tree | 4 | 0 | 3 | 33 |
| human-v2 | chat | write | 11 | 8 | 26 | 75 |
| human-v2 | submit | tree | 11 | 0 | 5 | 24 |
| human-v2 | submit | write | 20 | 9 | 24 | 67 |
| broad-v1-reserved | chat | tree | 5 | 0 | 1 | 26 |
| broad-v1-reserved | chat | write | 6 | 0 | 0 | 26 |
| broad-v1-reserved | submit | tree | 3 | 0 | 0 | 29 |
| broad-v1-reserved | submit | write | 6 | 0 | 0 | 26 |

## Coverage and limits

Recorded finish reasons across reported runs: {'stop': 896, 'tool_calls': 895, 'length': 1}. The saved parent human-v2 submit run contains one context-limit failure. Every fixture stays in its denominator. Missing answers score zero. No extra output or time cap was added.

Correct means semantic equivalence under the existing oracle. Tree correctness includes equivalent descriptors that do not improve reference weight; weight scores remain in the raw results. This comparison uses the same grader mode as the parent runs. Strict reference checks are separate.

The test does not establish signed-transaction correctness. The authored tree set includes an explicit owner route; it does not cover committee-only Taproot outputs. A single seed does not establish repeatability.

The original qwen3-4b-think server was restored. New checkpoints remain separate model directories.

See [the fixed experiment plan](broad-v1-experiment.json). Per-question grades and paired IDs are saved in `runs/broad-v1-results.json`; prompts, responses, and logs remain in the individual run directories.
