This experiment compares 256 updates of RL with 256 updates of RL plus
completion-only SFT replay. Both start from wallet SFT checkpoint 128. The
32-update pilot was too short to establish a learning curve.

The RL pool is the same frozen 12-question selection from wallet RL v1:
six wallet questions and six earlier tree questions. Each update samples
eight answers. Both arms use seed 7, temperature 0.6, top-p 0.95, top-k 20,
min-p 0, thinking, and a 4,096-token completion budget. Truncated answers
receive zero reward and are masked. The reward server uses no shaping.

Both arms use LoRA rank 64, alpha 128, dropout zero, learning rate 1e-5,
eight warmup updates, a cosine schedule over 256 updates, KL coefficient
0.02, DAPO loss and group reward scaling. The optimizer starts fresh from
the parent. This is a longer schedule, not a resume of the decayed pilot.

The mixed arm adds 0.1 times the mean completion-token cross-entropy for
one reference example per optimizer update. It adds that gradient after
the eight RL microbatches, before the common gradient clipping and optimizer
step. The separate backward frees the RL graph before SFT. Its accumulation
scaling is checked against an independent masked-loss gradient calculation.
The coefficient is an initial choice, not a claim of an optimal mixture.

Replay has 256 rows: 64 earlier write questions and 64 earlier tree questions,
each in chat and submit form. All were in the parent's SFT data. Their order
is frozen with seed 7. Stated policies match reference policies; write
Miniscript is checked independently; tree trace descriptors match strictly
graded reference descriptors. Final chat and submit targets agree. Actual
completion masks are audited before training. No RL-generated trace is
reused as an SFT target.

Both arms have 2,048 RL completions and 256 optimizer updates. The mixed
arm also has 256 supervised examples. This compares adding SFT replay;
it is not a match by compute or data exposure. Wall time, supervised tokens,
RL completion lengths, losses, truncations and reward variation are saved.
One seed cannot establish a robust method ranking.

A separate two-update mixed preflight tests the actual model and trainer.
Its weights are discarded. The two measured arms start from the unchanged
parent. Checkpoints 64, 128 and 256 are evaluated in chat and submit form
on all five existing development suites. Training completes before these
evaluations; no intermediate result changes either arm. Step 256 is the
fixed final comparison. Intermediate checkpoints describe the curve.

All five suites are observed development data, including the formerly
reserved wallet set. No new claim of untouched-test transfer is made.
No model is promoted automatically. The supervisor preserves the original
model and restores its server after completion or failure.

Run artifacts: `runs/rl-wallet-v2/`. Frozen code, data, model and baseline
hashes: `training/wallet-rl-v2-experiment.json`.
