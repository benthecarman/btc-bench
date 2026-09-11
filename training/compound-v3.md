# Conditions on teams and ordinary chat answers

This run addresses two failures from compound v2: poor performance when a
condition applies to a complete team, and regression on ordinary chat
requests. The original model remains the parent and serving default.

The run is complete. See [results](compound-v3-results.md),
[error analysis and trace checks](compound-v3-analysis.md), and
[three spot checks](compound-v3-spot-check.md). The development rule selected
update 144. The original model server was restored after all evaluations.

## Curriculum

The compound pool contains the prior 192 questions plus 96 new questions:
45 delayed pairs, 34 pairs requiring a secret, and 17 delayed committees.
Each new question contains one conditioned team approval. The full pool
has 288 distinct normalized policy structures.

Replay adds 288 distinct original SFT examples: 192 script answers, 72 tree
answers, and 24 identification answers. Every compound and replay question
has a chat row and a submit row, for 1,152 rows in total. Related interfaces
stay in the same training split. Two epochs give each underlying question
four presentations: two chat and two submit.

Chat uses the benchmark's exact system prompt, `You are a helpful assistant.`,
and no tool schemas. Its final answer is a labelled script or descriptor in
a code block, or a plain identification label. For new compound tasks the
label states that the answer is a P2WSH witness script. The request itself
still asks for Bitcoin script. The policy and Miniscript trace is preserved.
Submit answers retain their existing tool-call format.

Original identification requests explicitly require a tool call. Only their
chat variants change that instruction to request the label directly. Their
submit requests and labels are unchanged. Neither script nor descriptor
answer bytes are changed by chat conversion.

## Development and fresh check

The existing 48 compound-v2 validation questions remain development data.
They now measure both chat and submit interfaces. The fixed selection rule
chooses the highest total correct count across these two interfaces, among
the original, update-144, and update-288 models. Ties prefer fewer updates.
The two interfaces share questions; 96 answers are not 96 independent tasks.

A fresh set has 48 questions: 24 new arrangements with a single conditioned
team, and 24 with two conditioned teams. Training never places two such
teams in one request. The fresh structures are absent from checked earlier
training sources, previous evaluation catalogs, and this training pool.
Structural normalization does not establish semantic novelty. These are
synthetic questions using related templates and vocabulary, not an
independent collection of human requests.

The fresh set is marked evaluation-only, has a distinct suite and role in
its manifest, and rejects training export. It is checked only for the
original model and the development-selected checkpoint. Its scores never
select a checkpoint. The selected model is also checked on unchanged
human-v2 in chat and submit modes. A fixed 24-question training-fit sample
contains 12 prior compound questions and 12 new conditioned-team questions.

## Verification and training

All 288 compound training references and 48 fresh references compile
without fallback. The 288 new chat finals pass through the benchmark's
actual chat extractor and standard-mode grader at full credit. The maximum
mixed prompt-plus-answer length is 3,850 tokens, below the 4,096-token
training context. The actual trainer's loss-mask audit passes all 1,152 rows:
prompt tokens are excluded and complete target answers are supervised.

Training uses the RTX 5090, two epochs, 288 optimizer updates, learning rate
5e-5, nine warmup updates, and gradient accumulation eight. Checkpoints are
saved at updates 144 and 288. The existing LoRA rank 64, alpha 128, dropout
0.05, bfloat16, and trainer seed 7 remain in use. The curriculum, interface
mix, replay sample, and learning rate all change from v2; this run cannot
isolate one cause of an improvement.

Inference uses the same RTX 5090 server configuration as prior comparisons:
32768-token context, ngram speculative decoding, temperature 0.6, top-p 0.95,
top-k 20, min-p zero, seed 20260904, thinking enabled, and concurrency four.
No extra benchmark generation or time limit is imposed. The original model
server is restored after evaluation. Existing original-model results are
reused only when the fixture hash and complete-response checks match.

Before training, the original model scores zero on development, fit, and
fresh questions in both interfaces. Reference verification uses Miniscript
semantics and assumed-valid-signature execution, not signed transactions.

## Files

- `build_compound_v3.py`: generator with structural exclusions.
- `compound-v3-{train,fresh,fit-sample}.json`: fixed source catalogs.
- `compound-v3-coverage.json`: counts and split limits.
- `prepare_compound_v3.py`: replay selection and matched interface conversion.
- `compound-v3-mix.json`: source hashes and every row's provenance.
- `compound-v3-experiment.json`: frozen schedule and selection rule.
- `report_compound_v3.py`: results from completed runs.
- `runs/sft-compound-v3`: model checkpoints, merged weights, audits, and logs.
- `runs/compound-v3-driver.py`: training and evaluation supervisor.
- `runs/compound-v3-status.json`: progress and original-server restoration.
