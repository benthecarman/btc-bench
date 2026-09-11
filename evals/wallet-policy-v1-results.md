# Wallet-policy v1 results

Checkpoint 528 was tested before any wallet-policy training. All 40 questions are evaluation-only. Twenty scenarios each have a template and concrete descriptor request, grouped into ten families.

| Interface | Template correct | Concrete correct |
|---|---:|---:|
| chat | 0/20 | 0/20 |
| submit | 0/20 | 0/20 |

| Interface | Both forms correct | Template only | Concrete only | Neither |
|---|---:|---:|---:|---:|
| chat | 0 | 0 | 0 | 20 |
| submit | 0 | 0 | 0 | 20 |

## By family

| Family | Chat template / concrete | Submit template / concrete |
|---|---:|---:|
| native-single | 0/2 / 0/2 | 0/2 / 0/2 |
| native-quorum | 0/2 / 0/2 | 0/2 / 0/2 |
| taproot-single | 0/2 / 0/2 | 0/2 / 0/2 |
| recovery-delay | 0/2 / 0/2 | 0/2 / 0/2 |
| recovery-height | 0/2 / 0/2 | 0/2 / 0/2 |
| recovery-time | 0/2 / 0/2 | 0/2 / 0/2 |
| delayed-team | 0/2 / 0/2 | 0/2 / 0/2 |
| joint-approval | 0/2 / 0/2 | 0/2 / 0/2 |
| joint-councils | 0/2 / 0/2 | 0/2 / 0/2 |
| alternative-recovery | 0/2 / 0/2 | 0/2 / 0/2 |

## Token counts

| Interface | Form | Mean prompt tokens | Mean output tokens |
|---|---|---:|---:|
| chat | template | 128.8 | 142.8 |
| chat | concrete | 278.6 | 801.8 |
| submit | template | 304.9 | 1770.1 |
| submit | concrete | 454.6 | 733.2 |

## Coverage and interpretation

chat: 40/40 responses; finish reasons {'stop': 40}.
submit: 40/40 responses; finish reasons {'tool_calls': 39, 'length': 1}.

Both representations are at zero accuracy in this baseline. This reveals an unmet output contract, but it cannot establish whether placeholders improve construction or whether copying keys is the main problem. A future training experiment needs separate wallet-format examples before this comparison can measure that difference.

All expected questions stay in the denominator. Every reference passed strict descriptor checks, semantic comparison with a separately authored policy, and canonical-English round trips before inference.

The same scenarios and related variants appear in both forms and interfaces. These are familiar wallet patterns, not 80 independent transfer questions. Template prompts teach the placeholder/path notation; concrete prompts supply derived keys. Any difference combines representation, token load, and instruction effects. One seed does not isolate their causes.

The local pilot uses a general descriptor submit tool and non-streaming JSON without textual tool fallback. Its scores should be compared within this pilot. No model parameters were updated.

The original qwen3-4b-think model server was restored. See [the task contract](wallet-policy-v1.md), [the frozen plan](wallet-policy-v1-experiment.json), and [reference/model spot checks](wallet-policy-v1-examples.md).
