# Script construction SFT results

This historical mixed writing/repair pilot was not promoted. Ordinary-chat
reserved writing fell from 7/16 to 5/16, while repair rose from 2/16 to
7/16. The active direction is now script writing only.

The final checkpoint was fixed before training. No evaluation selected a checkpoint.
Missing answers remain in the denominator. Correctness uses the existing Miniscript oracle.
Related write/repair and chat/submit forms are not independent observations.

| Model | Dataset | Interface | Task | Correct | Total |
|---|---|---|---|---:|---:|
| parent | human-v2 | chat | tree | 7 | 40 |
| parent | human-v2 | chat | write | 37 | 120 |
| parent | human-v2 | submit | tree | 16 | 40 |
| parent | human-v2 | submit | write | 44 | 120 |
| parent | compound-v2-validation | chat | write | 34 | 48 |
| parent | compound-v2-validation | submit | write | 39 | 48 |
| parent | compound-v3-fresh | chat | write | 35 | 48 |
| parent | compound-v3-fresh | submit | write | 36 | 48 |
| parent | script-construction-v1-reserved | chat | write | 9 | 32 |
| parent | script-construction-v1-reserved | chat | write requests | 7 | 16 |
| parent | script-construction-v1-reserved | chat | repair requests | 2 | 16 |
| parent | script-construction-v1-reserved | submit | write | 7 | 32 |
| parent | script-construction-v1-reserved | submit | write requests | 7 | 16 |
| parent | script-construction-v1-reserved | submit | repair requests | 0 | 16 |
| candidate | script-construction-v1-reserved | chat | write | 12 | 32 |
| candidate | script-construction-v1-reserved | chat | write requests | 5 | 16 |
| candidate | script-construction-v1-reserved | chat | repair requests | 7 | 16 |
| candidate | script-construction-v1-reserved | submit | write | 14 | 32 |
| candidate | script-construction-v1-reserved | submit | write requests | 9 | 16 |
| candidate | script-construction-v1-reserved | submit | repair requests | 5 | 16 |
| candidate | human-v2 | chat | tree | 3 | 40 |
| candidate | human-v2 | chat | write | 39 | 120 |
| candidate | human-v2 | submit | tree | 11 | 40 |
| candidate | human-v2 | submit | write | 41 | 120 |
| candidate | compound-v2-validation | chat | write | 33 | 48 |
| candidate | compound-v2-validation | submit | write | 39 | 48 |
| candidate | compound-v3-fresh | chat | write | 34 | 48 |
| candidate | compound-v3-fresh | submit | write | 34 | 48 |
| original | script-construction-v1-reserved | chat | write | 6 | 32 |
| original | script-construction-v1-reserved | chat | write requests | 5 | 16 |
| original | script-construction-v1-reserved | chat | repair requests | 1 | 16 |
| original | script-construction-v1-reserved | submit | write | 12 | 32 |
| original | script-construction-v1-reserved | submit | write requests | 5 | 16 |
| original | script-construction-v1-reserved | submit | repair requests | 7 | 16 |
| stock | script-construction-v1-reserved | chat | write | 0 | 32 |
| stock | script-construction-v1-reserved | chat | write requests | 0 | 16 |
| stock | script-construction-v1-reserved | chat | repair requests | 0 | 16 |
| stock | script-construction-v1-reserved | submit | write | 0 | 32 |
| stock | script-construction-v1-reserved | submit | write requests | 0 | 16 |
| stock | script-construction-v1-reserved | submit | repair requests | 0 | 16 |

## Paired changes from parent

| Dataset/interface | Gained | Lost | Both | Neither |
|---|---:|---:|---:|---:|
| script-construction-v1-reserved/chat | 6 | 3 | 6 | 17 |
| script-construction-v1-reserved/submit | 7 | 0 | 7 | 18 |
| human-v2/chat | 13 | 15 | 29 | 103 |
| human-v2/submit | 13 | 21 | 39 | 87 |
| compound-v2-validation/chat | 2 | 3 | 31 | 12 |
| compound-v2-validation/submit | 3 | 3 | 36 | 6 |
| compound-v3-fresh/chat | 1 | 2 | 33 | 12 |
| compound-v3-fresh/submit | 2 | 4 | 32 | 10 |

Single-seed construction/repair SFT pilot with replay. The reserved set has 16 related scenarios and four composition families. Their normalized shapes are absent from this SFT mix; parent training may contain them. A gain is development evidence under the Miniscript verifier. Signed transactions, arbitrary Script, and tool-assisted construction remain separate work. Correct final answers do not validate reasoning. Report paired gains/losses, finish reasons, and marked expression diagnostics. No automatic promotion.

Full prompts, outputs, and grades remain in the local
`runs/sft-script-construction-v1/` directory. Model weights and raw run artifacts
are excluded from Git. The matching experiment JSON records frozen inputs
and machine-local paths; it is a record of this run, not a portable launch
configuration.
