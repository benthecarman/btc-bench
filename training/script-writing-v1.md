# Script writing only

The active objective is to write a complete Bitcoin Script from an English
spending request. Each answer contains raw ASM for the requested context.
Training and evaluation use ordinary chat. Repair, optimization,
identification, and descriptor/tree tasks are excluded.

The starting checkpoint is broad SFT step 528. The repair-trained child is
not used as a parent. This pilot has one fixed epoch: 512 examples and 64
optimizer updates at learning rate 1e-5. Checkpoint 32 is for recovery;
checkpoint 64 is the fixed final model. No evaluation selects a checkpoint
and no model is promoted automatically.

The training mix has 288 new writing examples and 224 earlier writing
examples. New examples cover signatures, varied quorums, mandatory signers,
separate teams, absolute and relative locks, combined locks, recovery
branches, secret claims, and delays shared across branches. They cover 96
spending scenarios in three script contexts. Parameter choices include
small-integer boundaries and block/time lock units.

| Context | Examples |
|---|---:|
| P2SH redeem script | 96 |
| P2WSH witness script | 320 |
| Tapscript leaf | 96 |

Replay comes only from the write fixtures in the broad-v1 and compound-v3
training pools. It contains 112 examples from each pool. Every prompt asks
for construction from requirements and supplies no draft script. The final
answer is ASM; the checked policy and Miniscript appear in the training
trace. Context variants retain their common scenario group.

All 512 training targets and 36 reserved targets passed strict equivalence
in the specified context, reference interpreter execution, ASM round trips,
and the actual chat answer extractor. Reference execution assumes valid
signatures. All 512 complete tokenized targets fit within the trainer's
4,096-token limit; the longest pair is 2,525 tokens. The actual trainer
must also audit every loss mask before updating weights.

The main development measure is single-attempt writing accuracy on the
120 writing questions from human-v2. The writing subset preserves the
original requests and IDs; its 40 descriptor questions are excluded.
These questions have been observed before and are development data.

A separate reserved set has 36 writing requests: 12 spending scenarios,
each expressed in three script contexts. Its four composition families
are absent from the new training mix under normalized operator comparison,
including replay. Absence from all parent training is not established.
The related context variants are not independent observations.

The fixed final model and unchanged broad parent will both receive the same
writing-only evaluations on the RTX 5090. Inference uses ordinary chat,
one attempt, no grader feedback, temperature 0.6, top-p 0.95, top-k 20, and
seed 20260904. Thinking is enabled. The server context is 32,768 tokens;
there is no extra output or generation-time cap. Missing answers and
context-limit failures stay in the denominator.

Report semantic correctness, paired gains/losses, and per-context results.
The existing grader is limited to Miniscript. A passing result is not a
full signed-transaction test or a proof of arbitrary Bitcoin Script
correctness. This is a small single-seed writing pilot.

Preparation:

```bash
cargo build --release -p bench-cli --example compile_script_targets
/home/ben/.local/share/venvs/sft/bin/python training/build_script_writing_v1.py
```

Preparation refuses to overwrite prior outputs. The experiment plan freezes
the data, model, tokenizer, code, executable, and evaluation fixture hashes.
Launch with:

```bash
python3 scripts/sft_experiment.py training/script-writing-v1-experiment.json --check
python3 scripts/sft_experiment.py training/script-writing-v1-experiment.json
```

The plan records local paths and the verified original server process;
prepare a new plan for a new run. Status, training logs, checkpoints, raw
answers, and reports are stored under `runs/sft-script-writing-v1/` on the
models drive. The controller restores the original serving model after
completion or failure. The prior mixed construction/repair run remains
preserved as historical evidence.
