Wallet SFT v1 teaches the descriptor output contract introduced by the BIP-388 pilot. It starts from `runs/sft-broad-v1/merged-step528` and uses one fixed pass: 1,024 rows, 128 updates, learning rate 2e-5, six warmup steps, accumulation eight, LoRA rank 64/alpha 128/dropout 0.05. Checkpoint 64 is for recovery; checkpoint 128 is the predetermined evaluation candidate. No test result selects a checkpoint.

The new data contains 128 generated scenarios across 16 families. Each scenario has template and concrete targets, each in chat and submit form: 512 rows. Targets contain a separately stated policy and descriptor bodies extracted after type, sanity, and complete spending-contract checks. Template trace bodies use symbolic key expressions; checks expand those expressions to real public keys. P2WPKH has no custom Miniscript. Taproot traces include its internal-key route in the full policy. These are checked teaching explanations, not measurements of the model's reasoning.

The other 512 rows replay 256 earlier questions in both interfaces: 128 broad questions (64 write and 64 tree), 64 compound questions, and 64 original questions (32 script, 16 tree, 16 identify). One replay candidate was excluded for matching a reserved composition. All 1,024 targets pass tokenizer round trips and fit within 4,096 tokens. The actual trainer must also verify completion loss masks before it trains.

The current 40 wallet questions and three observed suites (`compound-v2-validation`, `compound-v3-fresh`, `human-v2`) are development/retention data. They remain unchanged. Existing checkpoint-528 runs provide their parent baselines. Both chat and submit interfaces are measured with the same inference settings as before.

The reserved check has 16 separately authored scenarios, paired into 32 questions. Eight scenarios use familiar patterns; eight have policy compositions absent from this wallet training mix under normalized policy comparison. The normalization removes key identities and time constants, preserves clock domains and thresholds, sorts commutative children, and flattens AND/OR. It is not a proof of complete semantic novelty, and does not establish absence from all historical parent training. The prose was authored by the coding assistant, not collected independently from people. Counts across forms/interfaces are correlated, not independent samples.

The reserved source, fixtures, training data, code and model hashes are frozen before training. Both the parent and the predetermined final checkpoint are evaluated on the reserved set. Results from that set do not alter training in this experiment.

Some reserved compositions exceed BIP-388's English renderer or reverse decoder. Each fixture records renderer support and reverse round-trip success separately. These diagnostics do not replace typed descriptor validation and full semantic comparison with the separately stated policy. The original pilot and training builders still require renderer support and successful reverse decoding.

The new `build-training` and `grade-training` commands accept only training manifests. The existing evaluation runner, grader, and audit continue to reject training datasets. The original `build` command retains its 20-scenario pilot guard; `build-evaluation` permits a separately sized evaluation catalog.

Examples: [wallet-sft-v1-examples.md](wallet-sft-v1-examples.md). Exact mix and row provenance: [wallet-sft-v1-mix.json](wallet-sft-v1-mix.json). Experiment plan: [wallet-sft-v1-experiment.json](wallet-sft-v1-experiment.json).

Completed results: [wallet-sft-v1-results.md](wallet-sft-v1-results.md). Interpretation and next experiment: [wallet-sft-v1-analysis.md](wallet-sft-v1-analysis.md).
