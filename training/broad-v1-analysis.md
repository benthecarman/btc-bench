# Broad SFT: interpretation and next steps

The broader SFT batch helped, but it did not solve the task. The fixed
development rule selected checkpoint 528 after two epochs. Its mean across
the eight development accuracy cells is 53.1%, against 48.3% for the parent
and 51.3% for checkpoint 264.

The reserved test was run only after that choice was saved. The parent
passed 1/128 answers across both interfaces; the selected checkpoint passed
21/128. There were no lost parent successes on this set. This is evidence
of some transfer to the reserved compositions under this test. It is not
evidence of broad Bitcoin Script competence or repeatability across seeds.

The 64 reserved questions contain 32 related script/descriptor pairs. The
two interfaces reuse those questions. Thus 128 answers are not 128
independent problems. The set is synthetic and shares primitives and
wording with training. It is now an observed test; these inspected cases
must not be described as unseen in a later experiment.

## What improved, and what regressed

| Test, both interfaces combined | Parent | Selected |
|---|---:|---:|
| Existing compound scripts | 149/192 | 144/192 |
| Human-style scripts | 67/240 | 81/240 |
| Human-style Taproot descriptors | 8/80 | 23/80 |
| Reserved scripts | 0/64 | 12/64 |
| Reserved Taproot descriptors | 1/64 | 9/64 |

The aggregate gains hide changed answers. On the existing compound tests,
the selected model gains 18 answers and loses 23. On human-style scripts it
gains 31 and loses 17. The human-style descriptor gains have no lost parent
successes. The full paired IDs are saved with the results.

The interface gap remains large. Human-style Taproot accuracy is 7/40 in
chat and 16/40 through the submit tool. In one manually reviewed case the
user explicitly requests a tr() descriptor. Chat returns raw script, while
submit returns the correct descriptor. This is an actual output error;
the request is explicit about the required answer type.

## What the intermediate expressions show

The reserved script audit checks explicit marked policy and Miniscript
expressions. It does not repair syntax or check tree reasoning. Miniscript
is checked in Segwit v0, the reference script context. The final grader
still accepts the contexts allowed by each question. Missing markers do
not prove incorrect reasoning.

| Script trace check | Parent chat | Selected chat | Parent submit | Selected submit |
|---|---:|---:|---:|---:|
| Equivalent stated policy | 5/32 | 20/32 | 3/32 | 17/32 |
| Equivalent stated Miniscript | 0/32 | 3/32 | 0/32 | 3/32 |
| Correct final script | 0/32 | 6/32 | 0/32 | 6/32 |

The model often states the right spending policy and then fails to produce
a valid construction. The failures include wrong wrapper types, incorrect
threshold child types, unbalanced expressions, and context-specific
fragments used with unsuitable keys. Of its 12 correct final script
answers, seven have invalid stated Miniscript: three in chat and four in
submit. A passing final answer must not be treated as proof that the
displayed derivation is valid.

All 32 reference script expressions pass this audit. Two negative controls
also behave as expected: a dropped-policy branch is not equivalent, and a
malformed expression is invalid. These checks leave final-answer scores
unchanged.

## Next experiment

Keep checkpoint 528 as a separate candidate. The results support a focused
construction and instruction-following batch before a long RL run:

1. Use training-only requests that require valid Miniscript composition.
   Target the observed wrapper and threshold type errors. Verify both the
   intermediate expression and its encoded final script.
2. Pair script and tr() descriptor requests for the same spending policy,
   in chat and submit form. Include short ordinary requests such as the
   reviewed owner/recovery example. Check that the response type follows
   the request.
3. Retain compound replay and report losses on the existing compound sets.
   The broader batch lost some earlier answers even as its total improved.
4. Before RL, measure repeated-sample reward variation on the broader
   training pool. This test's low single-sample accuracy does not establish
   that those training groups provide useful GRPO reward variation. Do not
   select RL questions from the reserved evaluation cases.

Do not spend another long run merely repeating these same two epochs.
The result identifies construction and response-type problems that need
direct measurement. It does not establish memorization or overfitting:
this run did not measure post-training accuracy on its training questions.

## Run checks and artifacts

Training finished in about 36 minutes. Both planned checkpoints are saved.
All 2,112 actual trainer loss masks pass. Against the frozen source targets,
1,696 completions are unchanged and 416 only gain the end token. No target
was truncated; the longest trainer example is 3,806 tokens.

All 1,280 new evaluation answers have complete coverage and normal finish
reasons. The reused parent development runs contain one older context-limit
failure, which remains a failure in the denominator. No extra generation
or time cap was added. The original qwen3-4b-think server was restored and
its model identity was checked through the API and process command.

Selected model: `runs/sft-broad-v1/merged-step528`.
Its `model.safetensors` SHA256 is
`b9669791efd6ecdf3867106bbaa29ebf539e521b6c9d6e57f46533934eeefb17`.

See [full results](broad-v1-results.md), [five spot checks](broad-v1-spot-check.md),
and [the fixed plan](broad-v1-experiment.json). Raw results, trace checks,
training logs, and models remain under `runs/`.
