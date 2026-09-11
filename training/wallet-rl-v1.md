This pilot tests whether a small RL update improves the wallet SFT checkpoint without more loss on earlier tasks. The parent is `runs/sft-wallet-v1/merged-step128`. The unchanged parent and a matched SFT control are both comparisons. No model is promoted automatically.

Before training, a frozen probe samples eight completions for each of 64 training questions: 32 wallet questions (16 scenarios in template/concrete form), 16 earlier script questions and 16 earlier tree questions. The wallet and earlier questions were already in the SFT training sources. The probe uses temperature 0.6, top-p 0.95, top-k 20, seed 7, thinking enabled, and an explicit 4,096-token completion budget matching RL. It uses the exact rendered submit prompts. Raw generations are preserved before grading.

Selection uses only probe reward variation. It selects equal numbers of wallet and earlier-skill questions from groups with nonzero reward variance, up to 16 per source, with seed 7. At least four eligible questions per source are required; otherwise this experiment ends after the probe. Equal counts in the selected pool do not guarantee exactly equal counts in 32 shuffled updates, so actual sample counts are reported.

The RL arm has 32 optimizer updates, eight generations per group, learning rate 1e-5, three warmup steps, LoRA rank 64/alpha 128/dropout zero, KL coefficient 0.02, DAPO loss and group reward scaling. These loss/scaling settings were verified in the installed TRL code. Truncated completions receive zero reward and are masked from the loss. Checkpoint 16 is a recovery checkpoint; checkpoint 32 is the fixed final candidate.

The SFT control starts from the same parent and uses the exact question/completion counts sampled by RL. It replaces generated outputs with verified reference targets, then trains for 32 updates with the same learning rate and LoRA settings. This matches updates and completion counts, not token counts or GPU compute. SFT shuffles reference rows separately.

The wallet HTTP reward path calls the existing wallet grader. Wallet rewards are binary full-contract correctness and do not use legacy shaping rungs. The response's equivalence component is populated; other wallet component diagnostics are not measured. The RL wallet parser requires exactly one final `submit_descriptor` call and rejects truncated, malformed, multiple-call and plain-text-only answers. Probe and trainer share this extraction. The old task parsers remain unchanged. Earlier write tasks retain binary semantic correctness; tree tasks retain the existing equivalence-gated weight score. No reward is assigned for the text of the model's reasoning.

Before model inference, all 64 rendered reference completions must pass the actual HTTP callback. Wrong answer types and duplicate wallet calls must receive zero. Rust tests compare wallet HTTP dispatch with offline grading and check extra-key and premature-spending attacks. Python tests cover strict wallet extraction and rejection of evaluation fixtures during RL preparation/validation.

Both trained arms are evaluated in chat and submit form on `wallet-policy-v1`, `compound-v2-validation`, `compound-v3-fresh`, `human-v2`, and `wallet-sft-v1-reserved`. All are now observed development sets. In particular, the last suite was inspected after wallet SFT and is not an untouched final test. No evaluation result changes the selected training pool, number of updates or checkpoint choice in this experiment. The parent baselines come from the completed wallet SFT run with the same inference settings.

The experiment freezes code, data, baseline results and model hashes before the probe. Selection and matched control data are derived later according to the frozen rules and are recorded separately. The supervisor restores the original serving model in its final cleanup path.

Exact plan: [wallet-rl-v1-experiment.json](wallet-rl-v1-experiment.json). Progress and artifacts: `runs/rl-wallet-v1/`.

Completed: [results](wallet-rl-v1-results.md), [analysis](wallet-rl-v1-analysis.md), and [fixed spot checks](wallet-rl-v1-spot-check.md).
