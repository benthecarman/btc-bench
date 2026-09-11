One fixed wallet SFT pass completed from broad checkpoint 528: 1,024 rows and 128 updates. No checkpoint was selected from these results. All targets passed actual trainer loss-mask checks. The original serving model was restored.

| Wallet set | Interface | Output | Parent | After SFT | Gained / lost |
|---|---|---|---:|---:|---:|
| wallet-policy-v1 | chat | template | 0/20 | 12/20 | 12 / 0 |
| wallet-policy-v1 | chat | concrete | 0/20 | 11/20 | 11 / 0 |
| wallet-policy-v1 | submit | template | 0/20 | 14/20 | 14 / 0 |
| wallet-policy-v1 | submit | concrete | 0/20 | 15/20 | 15 / 0 |
| wallet-sft-v1-reserved | chat | template | 0/16 | 8/16 | 8 / 0 |
| wallet-sft-v1-reserved | chat | concrete | 0/16 | 12/16 | 12 / 0 |
| wallet-sft-v1-reserved | submit | template | 0/16 | 11/16 | 11 / 0 |
| wallet-sft-v1-reserved | submit | concrete | 0/16 | 13/16 | 13 / 0 |

Reserved groups, combining both output forms (related questions):

| Group | Interface | Parent | After SFT |
|---|---|---:|---:|
| familiar | chat | 0/16 | 12/16 |
| new-composition | chat | 0/16 | 8/16 |
| familiar | submit | 0/16 | 14/16 |
| new-composition | submit | 0/16 | 10/16 |

Retention on previously observed development sets:

| Set | Task | Interface | Parent | After SFT | Gained / lost |
|---|---|---|---:|---:|---:|
| compound-v2-validation | write | chat | 34/48 | 35/48 | 4 / 3 |
| compound-v2-validation | write | submit | 39/48 | 36/48 | 2 / 5 |
| compound-v3-fresh | write | chat | 35/48 | 35/48 | 3 / 3 |
| compound-v3-fresh | write | submit | 36/48 | 38/48 | 2 / 0 |
| human-v2 | tree | chat | 7/40 | 5/40 | 3 / 5 |
| human-v2 | write | chat | 37/120 | 29/120 | 5 / 13 |
| human-v2 | tree | submit | 16/40 | 5/40 | 0 / 11 |
| human-v2 | write | submit | 44/120 | 40/120 | 7 / 11 |

New generation finish reasons: `{"stop": 360, "tool_calls": 358, "length": 2}`. Missing answers and context-limit outputs remain in the denominator.

The reserved check contains 16 authored scenarios, not 64 independent examples. Half use familiar structures. The other half are absent from this new training mix under normalized comparison; absence from all parent training is not established. This is a small synthetic test, not a broad human benchmark.

The wallet inference contract and runner are unchanged from the wallet baseline. The legacy tests retain their own original runner. Results across those interfaces measure different contracts.

Reasoning checks are saved separately in `runs/sft-wallet-v1/trace-audit.json`. Correct final answers alone do not validate reasoning. The teaching traces were checked separately before evaluation.

Model: `runs/sft-wallet-v1/merged-step128`. Weight SHA-256: `3f8db8312b352b6020aeaf175cb4ef4df2caa05c911181717602127cb4999e3b`.
