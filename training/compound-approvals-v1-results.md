# Compound approval pilot results

The 24-update SFT pilot completed, but it did not produce a correct answer
on any of the 12 training questions or the 24 composition questions.
The original checkpoint remains the model to use. Small changes on human-v2
do not establish an improvement from this single sampled comparison.

| Check | Original SFT | Compound pilot |
|---|---:|---:|
| Training questions, submit | 0/12 | 0/12 |
| Composition questions, chat | 0/24 | 0/24 |
| Composition questions, submit | 0/24 | 0/24 |
| Human-v2 write, chat | 32/120 | 33/120 |
| Human-v2 tree, chat | 1/40 | 1/40 |
| Human-v2 write, submit | 34/120 | 32/120 |
| Human-v2 tree, submit | 4/40 | 5/40 |

Counts measure semantic correctness. All questions remain in each
denominator, including answers that could not be extracted. In chat, write
tasks gained three correct answers and lost two. In submit mode, write tasks
gained one and lost three; trees gained one and lost none.

## Training and verification

Training started from `runs/sft-qwen3-4b-think/merged`, using 48 presentations
of the 12 new examples plus 144 distinct original SFT rows. It ran for
416.2 seconds on the RTX 5060 Ti. The learning rate was 1e-5 with three warmup
updates, gradient accumulation eight, and one epoch. No example exceeded
the 4,096-token training context. The final reported loss was 0.0441; this
low loss did not translate to successful generation on the new questions.

The adapter and separate merged model are in
`runs/sft-compound-approvals-v1-pilot/final` and
`runs/sft-compound-approvals-v1-pilot/merged`. Checkpoints at updates 10, 20,
and 24 are retained. The merged model SHA-256 is
`92bc1d092235fe3bb5cb33a571a6b1368efdfb4f0b980a4b2ec293390a4748f4`.
A check of three projection tensors confirmed that the merged model contains
updated weights; it does not establish how useful those updates are.

All 12 reference answers passed the standard-mode grader before training.
Evaluation used pinned fixture hashes, the same inference settings as the
original comparison, and the RTX 5090 for every measured generation. No
extra generation or time limit was added. Four pilot generations reached
the server context limit: one composition submit answer, two human-v2 chat
answers, and one human-v2 submit answer. Every expected question has a
recorded completion; no transport failures were found. The original model
server is restored after evaluation.

## What failed

The model still fails to translate approval groups into the intended policy.
For the shared-vote example, both saved generations use:

```text
and(thresh(2,pk(Nora),pk(Eli),pk(Sam),pk(Tess)),and(pk(Sam),pk(Tess)))
```

The required policy is:

```text
thresh(2,pk(Nora),pk(Eli),and(pk(Sam),pk(Tess)))
```

The generated policy blocks the authorized Nora-plus-Eli spend and allows
Sam and Tess alone. Their joint signature should count as only one vote.
Both runs submitted the same incorrect assembly for this example. Across
the training-fit check, decoded answers fell from 11/12 to 6/12; all decoded
answers had wrong semantics.

This is a failed small pilot, not evidence that SFT cannot teach the skill.
It also does not isolate data volume, repetition, or learning rate as the
cause. Before a larger curriculum or RL run, use a focused training-fit
experiment on a few examples to check that training can reliably produce
their correct policies and scripts. That experiment would be a training
diagnostic, not an evaluation of generalization.

## Scope and artifacts

The 12 questions are training data. The composition set is now a development
test for the family taught by these examples; human-v2 is also an existing
development set. None of these scores establishes transfer to fresh tasks.

- `compound-approvals-v1-pilot.md`: training design and commands.
- `compound-approvals-v1-pilot.json`: source hashes, row selections, and pinned evaluation settings.
- `runs/compound-pilot-results.json`: aggregate results and finish reasons.
- `runs/compound-pilot-comparison-{chat,submit}`: paired human-v2 outcomes and question IDs.
- `runs/compound-pilot-fit-check.md`: per-training-question failures and the shared-vote spot check.
- `runs/compound-pilot-status.json`: evaluation completion and restored server PID.
- `runs/{compound-pilot-after-submit,transfer-v1-compound-pilot-chat,transfer-v1-compound-pilot-submit,human-v2-compound-pilot-chat,human-v2-compound-pilot-submit}`: raw responses, configs, and grading results.
