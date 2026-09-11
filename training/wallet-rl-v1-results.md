A fixed 32-update RL pilot and a matched 32-update reference SFT control completed from wallet SFT checkpoint 128. No evaluation result selected a checkpoint. The original serving model was restored.

Training probe (eight completions per question):

| Source | Mixed groups | Mean reward | Semantically correct samples |
|---|---:|---:|---:|
| wallet | 10/32 | 0.848 | 217/256 |
| earlier | 6/32 | 0.902 | 231/256 |

The selected pool has six wallet and six earlier tree questions. All earlier write groups had zero reward variation and were excluded by the frozen rule. Raw script performance is still tested.

Actual RL sampling:

| Source | Groups | Mixed groups | Mean reward |
|---|---:|---:|---:|
| wallet | 16 | 8 | 0.836 |
| earlier | 16 | 11 | 0.757 |

RL sampled 256 completions; 0 reached the rollout limit. The SFT control used the same question counts and 256 verified reference completions, with all actual loss masks checked. This does not match token counts or GPU compute.

| Development set | Interface | Task/output | Parent | RL | SFT control | RL gains/losses | SFT gains/losses |
|---|---|---|---:|---:|---:|---:|---:|
| wallet-policy-v1 | chat | concrete | 11/20 | 9/20 | 11/20 | 0/2 | 1/1 |
| wallet-policy-v1 | chat | template | 12/20 | 14/20 | 13/20 | 3/1 | 1/0 |
| wallet-policy-v1 | submit | concrete | 15/20 | 15/20 | 15/20 | 0/0 | 0/0 |
| wallet-policy-v1 | submit | template | 14/20 | 11/20 | 13/20 | 0/3 | 1/2 |
| compound-v2-validation | chat | write | 35/48 | 35/48 | 37/48 | 0/0 | 2/0 |
| compound-v2-validation | submit | write | 36/48 | 34/48 | 35/48 | 0/2 | 1/2 |
| compound-v3-fresh | chat | write | 35/48 | 36/48 | 33/48 | 2/1 | 1/3 |
| compound-v3-fresh | submit | write | 38/48 | 40/48 | 38/48 | 2/0 | 0/0 |
| human-v2 | chat | tree | 5/40 | 5/40 | 8/40 | 1/1 | 5/2 |
| human-v2 | chat | write | 29/120 | 28/120 | 28/120 | 3/4 | 7/8 |
| human-v2 | submit | tree | 5/40 | 6/40 | 5/40 | 1/0 | 3/3 |
| human-v2 | submit | write | 40/120 | 38/120 | 38/120 | 2/4 | 5/7 |
| wallet-sft-v1-reserved | chat | concrete | 12/16 | 11/16 | 12/16 | 0/1 | 0/0 |
| wallet-sft-v1-reserved | chat | template | 8/16 | 8/16 | 9/16 | 2/2 | 2/1 |
| wallet-sft-v1-reserved | submit | concrete | 13/16 | 11/16 | 12/16 | 0/2 | 0/1 |
| wallet-sft-v1-reserved | submit | template | 11/16 | 9/16 | 10/16 | 0/2 | 0/1 |

Human-v2 tree mean benchmark score (includes the weight objective):

| Interface | Parent | RL | SFT control |
|---|---:|---:|---:|
| chat | 0.125 | 0.125 | 0.200 |
| submit | 0.125 | 0.150 | 0.117 |

New evaluation finish reasons: `{"stop": 656, "tool_calls": 653, "length": 3}`. All questions remain in the denominator, including missing answers and context-limit generations.

All five suites have been observed before this pilot, including the formerly reserved wallet set. These results are development measurements. Related template/concrete and chat/submit results are not independent samples. This is one seed and a small pool selected for reward variation.

Wallet/write rewards test final-answer correctness; tree reward also measures weight. None directly rewards the model reasoning. Separate trace checks are diagnostic and do not affect grades.

Full rollout records, raw responses, model hashes and control counts are in `runs/rl-wallet-v1/`.
