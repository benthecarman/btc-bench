# Script-writing-only SFT results

The writing-only child scored 31/120 on human-v2-writing; the fresh
parent run scored 38/120. It gained eight answers and lost 15. The reserved
score changed from 0/36 to 1/36. This pilot does not support replacing the
broad parent. Training completed all 64 updates, and the original serving
model was restored.

The final checkpoint was fixed before training. No evaluation selected a checkpoint.
Missing answers remain in the denominator. Correctness uses the existing Miniscript oracle.
The 36 reserved questions share 12 spending scenarios across three contexts.

| Model | Dataset | Interface | Task | Correct | Total |
|---|---|---|---|---:|---:|
| candidate | script-writing-v1-reserved | chat | write | 1 | 36 |
| candidate | script-writing-v1-reserved | chat | legacy | 1 | 12 |
| candidate | script-writing-v1-reserved | chat | segwitv0 | 0 | 12 |
| candidate | script-writing-v1-reserved | chat | tap | 0 | 12 |
| candidate | human-v2-writing | chat | write | 31 | 120 |
| parent | script-writing-v1-reserved | chat | write | 0 | 36 |
| parent | script-writing-v1-reserved | chat | legacy | 0 | 12 |
| parent | script-writing-v1-reserved | chat | segwitv0 | 0 | 12 |
| parent | script-writing-v1-reserved | chat | tap | 0 | 12 |
| parent | human-v2-writing | chat | write | 38 | 120 |

## Paired changes from parent

| Dataset/interface | Gained | Lost | Both | Neither |
|---|---:|---:|---:|---:|
| script-writing-v1-reserved/chat | 1 | 0 | 0 | 35 |
| human-v2-writing/chat | 8 | 15 | 23 | 74 |

Writing tasks only: requirements to raw ASM in ordinary chat. This single-seed pilot compares broad step528 with a fixed writing-only SFT child. The 36 reserved questions share 12 scenario groups across contexts. Their normalized structures are absent from this training mix, but may occur in parent training. Human-v2 writing questions are observed development data. Scores use the existing Miniscript semantic oracle; reference execution assumes valid signatures. No signed-transaction or arbitrary-Script proof is claimed. Report paired gains/losses and context results; do not automatically promote.

Full prompts, outputs, and grades remain in the local
`runs/sft-script-writing-v1/` directory. Model weights and raw run artifacts
are excluded from Git. The matching experiment JSON records frozen inputs
and machine-local paths; it is a record of this run, not a portable launch
configuration.
