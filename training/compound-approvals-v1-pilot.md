# Compound approval SFT pilot

The run and evaluation are complete. See [the results](compound-approvals-v1-results.md).

The user approved training on the 12 reviewed compound-approval examples.
This pilot starts from the original `sft-qwen3-4b-think/merged` checkpoint.
The earlier composition SFT and RL checkpoints are not parents of this run.

The dataset has 192 rows: four copies of each new example, plus 144 distinct
rows from the original SFT data. Replay includes 88 script answers, 48 tree
answers, and eight identification answers. These repetitions are deliberate;
they are not 48 independent new questions. The two five-vote variants stay
in the training split together.

The data builder records every source row and its file hash in
`compound-approvals-v1-pilot.json`. It selects replay rows that fit the
4,096-token training context. The longest combined prompt and completion is
3,299 tokens. Targets retain the original policy → Miniscript → assembly
trace and submit-tool format. The 12 new reference targets pass the standard
mode grader before training.

The run uses one epoch, 24 optimizer updates, batch size one, accumulation
eight, learning rate 1e-5, three warmup updates, and checkpoints every ten
updates. The existing trainer uses LoRA rank 64, alpha 128, dropout 0.05,
bfloat16, completion-only loss, and trainer seed 7. Training uses GPU 1, the
RTX 5060 Ti. The model output is `runs/sft-compound-approvals-v1-pilot`.

Before training, the original model solves 0/12 new examples in submit mode.
Eleven answers decode but have wrong semantics; one fails decoding. This
baseline is saved in `runs/compound-pilot-before-submit`.

Evaluation uses the unchanged human-v2 and composition-transfer-v1 fixtures,
in chat and submit modes. Their hashes and inference settings were recorded
before pilot evaluation. The inference server uses GPU 0, the RTX 5090,
with the same sampling and speculative-decoding settings as the original
comparisons. There is no added benchmark generation or time limit. The
12 training examples are checked again in submit mode as a fit diagnostic.
The original model server is restored after evaluation.

Scores on the 12 examples measure training fit, not generalization.
The composition transfer set is now a development test for this taught
skill family. Better scores there would not establish unseen-family
transfer. Human-v2 is also an existing development set. A later claim of
broader transfer needs a fresh held-out test.

## Reproduce

Run the sample compiler and SFT preparation described in
`compound-approvals-v1-samples.md` first. Preserve existing experiment outputs;
the data builder refuses to overwrite them.

```sh
/home/ben/.local/share/venvs/sft/bin/python training/prepare_compound_pilot.py
CUDA_VISIBLE_DEVICES=1 /home/ben/.local/share/venvs/sft/bin/python scripts/sft_train.py \
  --data datasets/sft-compound-approvals-v1-pilot.jsonl \
  --model runs/sft-qwen3-4b-think/merged \
  --out runs/sft-compound-approvals-v1-pilot \
  --epochs 1 --learning-rate 1e-5 --warmup-steps 3 \
  --gradient-accumulation-steps 8 --save-steps 10
```

Evaluation supervision and raw logs are saved in `runs/compound-pilot-driver.py`
and `runs/compound-pilot-driver.log`; progress is in
`runs/compound-pilot-status.json`.
