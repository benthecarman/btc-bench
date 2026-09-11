# Expanded compound-approval curriculum

Training and evaluation are complete. See [the results](compound-v2-results.md).

This experiment follows the successful three-question fit diagnostic. It
asks whether training can transfer to separate compound-approval questions
while retaining performance on the existing human-style benchmark.

## Data and split

There are 192 new training questions and 48 validation questions, with one
question per normalized policy structure. The training mix adds 192 distinct
rows from the original SFT data: 128 script, 48 tree, and 16 identification
answers. New tasks therefore make up half of every epoch. The original
checkpoint is the parent; the memorization diagnostic is not the parent.

The new training grammar includes single signers, complete pairs,
either-person approvals, two-of-three committees, delayed signers,
deadline-bound signers, and signers with SHA-256 or HASH160 secrets. Each
question requires two or three of three or four approvals. A compound item
counts as one approval. Requests ask for Bitcoin script; references choose
P2WSH witness scripts.

Validation has two fixed subsets:

- 24 new arrangements of the approval types present in training.
- 24 compositions with delayed pairs, pairs requiring a secret, or delayed
  committees. These gated team approvals are absent from the new training
  grammar. Their normalized complete policies are also absent from the
  checked earlier training data.

Both splits exclude normalized structures from human-v2, composition
transfer-v1, the 12 reviewed samples, the prior 384-task composition pool,
and all 14,700 recognized policy traces in the original SFT file. The
remaining 1,996 original rows are identification examples. There are 994
excluded structures in total. This checks available stored sources, not
all historical inputs, and does not prove semantic novelty.

Prompts are synthetic and generated from authored approval descriptions.
Validation uses separate request templates but shares that vocabulary.
This is a test of transfer within the specified task distribution, not an
independently authored human test of general Bitcoin ability. Validation
selects checkpoints, so it is development data rather than a final test.

The validation manifest sets `evaluation_only=true`. Both RL preparation
and SFT export reject it. Policies, references, and group IDs are retained
for review. The split and inference settings were fixed before any model
answers on these questions were generated.

## Reference and training checks

All 240 fixtures pass the repository audit without failures, warnings, or
compiler fallback. All 192 answers extracted from the exported SFT
completions pass standard-mode grading. The longest new example has 3,131
tokens. With original-task replay, the maximum is 3,794; no example needs
truncation in the 4,096-token training context.

The actual trainer's loss-mask audit passes for all 384 mixed rows: prompt
labels are excluded, every completion token is included, and the supervised
text reproduces the full target. The trace format remains policy →
Miniscript → assembly → submit call. Reference checks use Miniscript
semantics and assumed-valid-signature execution, not fully signed Bitcoin
transactions.

## Training and selection

Training starts from `runs/sft-qwen3-4b-think/merged` on the RTX 5090.
It uses four epochs, 192 optimizer updates, learning rate 1e-4, six warmup
updates, batch size one, and gradient accumulation eight. LoRA settings
remain rank 64, alpha 128, dropout 0.05; the trainer uses bfloat16 and seed 7.
Checkpoints at updates 96 and 192 correspond to two and four epochs.

Before training, the original model scores 0/48 on validation and 0/24 on a
fixed random sample of training questions. The same fit sample and validation
set are checked for both saved checkpoints in submit mode, with one sampled
answer per question. Sampling matches the prior benchmark: temperature 0.6,
top-p 0.95, top-k 20, min-p zero, seed 20260904, and thinking enabled. The
server context is 32768 with the existing ngram decoding configuration;
there is no added benchmark generation or time limit.

The selection rule is fixed: choose the most correct validation answers
among the original, update-96, and update-192 models. Ties prefer fewer
updates. The fit sample is diagnostic only. If a new checkpoint wins,
evaluate it on unchanged human-v2 in chat and submit modes to check for
regression. Human-v2 results do not choose the checkpoint. The original
server is restored afterward; selection alone does not replace it.

This changes data breadth, mixture, exposure, and the training schedule
compared with the three-question diagnostic. It is a practical next-stage
experiment, not an isolated comparison of those individual choices.

## Artifacts

- `build_compound_v2.py`: source generator and structural exclusions.
- `compound-v2-{train,validation}.json`: authored catalogs and group IDs.
- `compound-v2-coverage.json`: structure counts and coverage limits.
- `compound-v2-examples.md`: readable requests, policies, and Miniscript.
- `prepare_compound_v2.py`: fixed replay selection and token-length checks.
- `compound-v2-mix.json`: all mixed source rows and data hashes.
- `compound-v2-experiment.json`: frozen training, inference, and selection settings.
- `compound-v2-fit-sample.json`: fixed 24-question training-fit sample.
- `runs/sft-compound-v2`: adapters, checkpoints, audit, merged models, and logs.
- `runs/compound-v2-{original,step96,step192}-{validation,fit}`: saved model answers and grades.
- `runs/compound-v2-driver.py` and `runs/compound-v2-status.json`: experiment supervision and status.
