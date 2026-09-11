The 256-update RL and RL plus SFT replay runs completed. Both used the same SFT parent and RL question pool. The mixed run added one supervised earlier-task example per update, with loss weight 0.1. Step 256 is the fixed final comparison. Checkpoints 64 and 128 describe the learning curve.

All scores below count correct final answers. Tree weight scores are stored separately in the JSON report. All suites are observed development data; paired interfaces and representations are not independent trials.

| Development set | Parent | RL 64 | Mixed 64 | RL 128 | Mixed 128 | RL 256 | Mixed 256 |
|---|---:|---:|---:|---:|---:|---:|---:|
| wallet-policy-v1 | 52/80 | 43/80 | 43/80 | 41/80 | 42/80 | 43/80 | 42/80 |
| compound-v2-validation | 71/96 | 72/96 | 71/96 | 72/96 | 72/96 | 72/96 | 71/96 |
| compound-v3-fresh | 73/96 | 75/96 | 76/96 | 77/96 | 76/96 | 77/96 | 75/96 |
| human-v2 | 79/320 | 71/320 | 74/320 | 69/320 | 75/320 | 73/320 | 71/320 |
| wallet-sft-v1-reserved | 44/64 | 25/64 | 30/64 | 27/64 | 31/64 | 27/64 | 31/64 |

Training by 64-update segment:

| Arm | Updates | Groups with reward variation | Mean reward | Truncated answers |
|---|---|---:|---:|---:|
| rl | 1–64 | 22/64 | 0.779 | 0 |
| rl | 65–128 | 3/64 | 0.820 | 0 |
| rl | 129–192 | 1/64 | 0.842 | 0 |
| rl | 193–256 | 4/64 | 0.820 | 0 |
| mixed | 1–64 | 22/64 | 0.760 | 0 |
| mixed | 65–128 | 1/64 | 0.824 | 0 |
| mixed | 129–192 | 2/64 | 0.838 | 0 |
| mixed | 193–256 | 0/64 | 0.826 | 0 |

Training process wall time: RL 53.6 minutes; mixed 56.7 minutes. These include model initialization and saving. SFT added 256 examples and 418,065 supervised tokens. The arms are not matched by compute or data exposure.

Final wallet trace diagnostics:

| Model | Correct final with wrong/invalid policy | Correct final with wrong/invalid bodies |
|---|---:|---:|
| parent | 17 | 13 |
| rl-step256 | 11 | 12 |
| mixed-step256 | 10 | 10 |

Trace checks cover explicit marked expressions only; missing text is unmeasured. Body checks use the reference internal key. They are diagnostics and do not affect final grades.

No model was promoted. The original model server was restored. Raw generations, checkpoints, replay masks and training histories are preserved in `runs/rl-wallet-v2/`.
