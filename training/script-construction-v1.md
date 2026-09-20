# Script construction and repair SFT

This pilot tests whether checked construction and repair examples improve
raw-script answers from broad SFT checkpoint 528. It uses the existing
Miniscript verifier. Signed-transaction tests and coverage of arbitrary
Bitcoin Script remain separate work; this run does not claim either.

The final model is fixed at 96 optimizer updates before any new model
evaluation. Checkpoint 48 is for recovery only. There is no checkpoint
selection or automatic replacement of the serving model.

The 768 training rows contain:

- 384 new rows: 12 construction families, eight parameter/key variants,
  each with write and repair requests in chat and submit form.
- 384 replay rows: 128 script, 48 tree, and 16 identification questions
  from the verified broad-v1 mix, each in both interfaces.

New targets choose P2WSH and contain raw ASM, a policy, a typed Miniscript,
and a short explanation of branch placement or stack use. All 192 training
answers pass strict grading; all 192 faulty drafts fail. Compiler checks
also cover ASM round trips, reference interpreter execution with assumed
signatures, and chat answer extraction. All 768 tokenized targets remain
complete; the longest prompt/target pair has 3,805 tokens. The actual SFT
trainer must check every completion loss mask before its first update.

The reserved set contains 16 scenarios from four separate composition
families, each as write and repair questions: 32 questions total. These
normalized structures are absent from both the new curriculum and selected
replay. Absence from all parent training is not established. Names and
parameters vary, but the questions are synthetic and share primitives.
Related task and interface variants are not independent observations.
Reserved questions are never exported as training rows.

One epoch uses learning rate 1e-5, batch size one, eight accumulation steps,
six warmup updates, seed 7, and the existing rank-64 LoRA configuration.
The parent, data, executable, tokenizer, and relevant code hashes are frozen
in `script-construction-v1-experiment.json`. The new data SHA-256 is
`d2a7a6b13ae5743a65e5cbe2a17e3279956597fb0fdd69433acff2c313502aaa`.

After training, the controller evaluates the final model, broad parent,
original SFT, and stock Qwen3-4B on the reserved set. It evaluates the final
model on human-v2 and the two existing compound development suites. Saved
parent responses on those suites are regraded with the same executable.
Every run uses one attempt in both chat and submit modes, the same RTX 5090,
thinking enabled, temperature 0.6, top-p 0.95, top-k 20, and inference seed
20260904. The server context is 32,768 tokens. No extra generation or time
cap is added. Context-limit failures remain in the denominator.

The primary comparison is reserved write accuracy in ordinary chat against
the broad parent. Report repair accuracy, paired gains/losses, existing
script retention, tree retention, finish reasons, and explicit intermediate
policy/Miniscript checks separately. This single pass cannot isolate the
effects of new examples from replay, or establish repeatability.

Preparation and launch commands, run from the repository root:

```bash
/home/ben/.local/share/venvs/sft/bin/python training/build_script_construction_v1.py
python3 scripts/sft_experiment.py training/script-construction-v1-experiment.json --check
python3 scripts/sft_experiment.py training/script-construction-v1-experiment.json
```

Preparation refuses existing output. The frozen plan includes the original
server PID and local storage paths; a new launch needs a new plan with
verified paths and process identity. Do not reuse it for a different run.

Run state is saved in `runs/sft-script-construction-v1/status.json` and
training output in `train.log`. Results are written to `results.md`,
`results.json`, and `trace-summary.json` in that directory. The controller
restores the original `qwen3-4b-think` server after success or failure.
Raw responses, checkpoints, frozen inputs, and commands stay on the models
drive through the existing `runs` symlink.
