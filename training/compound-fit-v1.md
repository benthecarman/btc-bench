# Three-question SFT fit diagnostic

The user approved a focused training-fit experiment after the mixed pilot
scored 0/12 on its training questions. This run deliberately fits three
questions. It is not a generalization benchmark or a deployment candidate.

The questions are `joint-seat`, `delayed-seat`, and `delivery-seat` from the
reviewed compound-approval samples. They cover a complete signature pair
counting as one vote, a delay attached to one signer, and a secret attached
to one signer's approval. Prompts, policies, Miniscript, keys, hashes, and
script targets are unchanged.

Training starts from the original `sft-qwen3-4b-think/merged` checkpoint.
It uses three rows for 40 epochs, giving each question 40 presentations.
There is no original-data replay. Batch size is one with accumulation three,
so the run has 40 optimizer updates. The learning rate is 1e-4, with three
warmup updates and a cosine schedule. LoRA rank, alpha, dropout, completion
format, and the rest of the existing SFT configuration are unchanged.
The data hashes and sampling settings are in `compound-fit-v1.json`.

This changes both exposure and learning rate compared with the failed
mixed pilot. It cannot isolate which change matters. Its purpose is to
check whether the current model and training setup can fit these examples
at all before attempting a larger curriculum.

The new optional `sft_train.py --audit-data PATH` check inspects the trainer's
prepared dataset before training. For every row, it verifies the exact prompt
token boundary, excluded prompt labels, included completion labels, and the
complete decoded answer. All three rows passed. The audit found 1,123, 898,
and 1,145 supervised tokens; their prompts have 529, 458, and 521 tokens.
Full supervised text is saved in the run's `loss-mask-audit.json`.

The diagnostic samples eight complete answers per question before and after
training: temperature 0.6, top-p 0.95, top-k 20, min-p zero, seed 20260910,
and thinking enabled. Both models use the same RTX 5090 inference setup.
No separate generation or time limit is added. The script reward is binary
semantic correctness, with all shaping settings zero. All 24 answers from
the original model failed before training.

The training run is `runs/sft-compound-fit-v1`. Its adapter, merged weights,
logs, loss-mask audit, and checkpoints are preserved. Raw probe samples are
`runs/compound-fit-v1-{before,after}.jsonl`; derived scores use the same names
with `-scores.json`. The original model server is restored after the probe.

## Completed result

| Training question | Before | After |
|---|---:|---:|
| Shared vote | 0/8 | 8/8 |
| Delayed signer | 0/8 | 8/8 |
| Inspector with a secret | 0/8 | 8/8 |
| Total sampled answers | 0/24 | 24/24 |

All 24 final answers passed semantic grading. All 24 generated traces also
contain the exact reference policy and Miniscript, and every submitted
assembly answer matches its training target. This is deliberate training
fit: 24 samples from three known questions, not 24 independent transfer
questions. It demonstrates that this setup can reproduce these targets.
It does not show that the model understands unseen requests or policies.

Training took 197.5 seconds for 40 updates. The final interval loss was
approximately 0.000058, and the mean loss across the full run was 0.01847.
No question was truncated and no completion failed during transport.
The original model server was restored after evaluation. The merged
experimental checkpoint is `runs/sft-compound-fit-v1/merged`, with SHA-256
`73b283af69c31a99debee9eaf2d22635136e5d5e10a3b024faf476ef7f371b39`.

The earlier mixed pilot was insufficient under its chosen settings. This
experiment does not identify one cause: the learning rate, per-question
exposure, number of updates, and absence of replay all changed. It rules out
a general inability of this training setup to fit these three targets.

The next useful experiment is a larger set of verified compound-approval
training examples with a separate validation split. Check training fit and
validation accuracy at checkpoints, and include original-task replay to
measure the tradeoff with forgetting. Keep the original model as the main
checkpoint until a broader test supports replacement. Do not use these
three now-saturated questions as the basis for an RL experiment.
