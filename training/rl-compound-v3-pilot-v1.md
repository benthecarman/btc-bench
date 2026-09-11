# Small RLVR and additional-SFT comparison

The parent is compound-v3 checkpoint 144. This experiment tests whether a
short RLVR run improves script construction more than additional SFT on
the same questions. It uses fixed final checkpoints, without selection
from evaluation scores.

The experiment is complete. See [results](rl-compound-v3-pilot-v1-results.md),
[analysis](rl-compound-v3-pilot-v1-analysis.md), and
[three spot checks](rl-compound-v3-pilot-v1-spot-check.md).

## Probe

The probe contains 48 training questions: 24 retained compound-v2 questions
and 24 questions added in compound v3. Each gets eight samples at temperature
0.6, top-p 0.95, top-k 20, min-p zero, and seed 7. The explicit 4,096-token
budget matches the trainer. This is a training probe, separate from the
uncapped benchmark protocol.

All 384 samples finished normally. Of 48 groups, 32 are entirely correct,
two are entirely wrong, and 14 contain both correct and incorrect answers.
Overall correctness is 322/384 (83.9%). The pilot uses the 14 mixed groups.
These include four retained questions and ten questions with conditioned
teams. The subset is selected from training data only.

The probe reward server uses the current binary with no shaping. A correct
reference and `OP_0` return one and zero, respectively. Raw generations and
regraded reward components are saved under `runs/rl-compound-v3-probe-v1`.

## Training comparison

Both arms start from the same merged parent and use 32 optimizer updates,
learning rate 1e-5, three warmup updates, cosine decay, microbatch one,
gradient accumulation eight, LoRA rank 64, alpha 128, dropout zero, and
seed 7. Neither arm has replay data or chat training rows in this small
comparison. Evaluation checks both interfaces for regressions.

The RL arm uses eight completions per question, unshaped final-answer
correctness, KL coefficient 0.02, and the installed TRL 1.12.0 GRPO trainer
with DAPO loss and group reward scaling. With this batch configuration,
one update consumes one question's eight completions. The trainer gives
truncated rollouts zero reward and masks their completion loss. Every
sampled group, final answer, and reward is saved to a local JSONL log.

The SFT control is built after RL generation. Each sampled RL completion
contributes one reference-answer row for the same question. Thus the
control has 256 rows and the exact question counts used by RL, including
repeated questions. It trains for one epoch, giving 32 updates. The
existing trainer audits complete target supervision before training.

This matches question and completion counts, optimizer updates, and the
listed optimization settings. It does not match generated token count,
target length, or GPU compute. SFT shuffles reference rows independently.
The arms use existing separate package environments, whose versions are
recorded with the run. This is a small single-seed pilot, not a conclusive
comparison of training methods.

## Evaluation

The parent and both fixed final checkpoints are compared on the existing
48 compound-v2 development questions, 48 compound-v3 transfer questions,
and 160 human-v2 questions. Both chat and submit interfaces are checked.
The parent results are reused only with matching fixture hashes and full
completion coverage. No additional generation or time caps are added.

All three suites have now been observed. This experiment does not describe
them as an untouched final test. The training subset excludes them. A new
independent test is still needed before making a broad generalization claim.
Final script correctness remains separate from intermediate trace validity.
The original model server is restored after evaluation.

## Files

- `rl-compound-v3-probe-v1.json`: fixed probe inputs and sampling settings.
- `rl-compound-v3-pilot-v1.json`: training and evaluation plan.
- `rl-compound-v3-control-v1.json`: actual SFT question counts and data hash.
- `run_compound_probe.py`: probe supervisor with server restoration.
- `report_compound_probe.py`: saved-sample grading and subset selection.
- `run_rl_compound_pilot.py`: training and evaluation supervisor.
- `prepare_rl_sft_control.py`: control data from actual sampled questions.
- `runs/rl-compound-v3-pilot-v1`: RL outputs and rollout log.
- `runs/sft-compound-v3-rl-control-v1`: SFT outputs and loss-mask audit.

The batch and reward behavior was checked against the installed source
and the [TRL GRPO documentation](https://huggingface.co/docs/trl/grpo_trainer).
