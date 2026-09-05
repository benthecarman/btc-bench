# Composed training pilot

The human-v2 interface check used the same SFT checkpoint, sampling seed and
four concurrent requests. Chat solved 32/120 write and 1/40 tree tasks. Submit
mode solved 34/120 write and 4/40 tree tasks. The submit interface also changes
the system instruction, so this is not an isolated tool-schema experiment.
The small write gain and remaining semantic errors justify a broader training
pool. Original responses remain in `runs/human-v2-sft` and
`runs/human-v2-sft-submit`.

## Separate training data

`scripts/compose_training.py` samples a recursive grammar of signed branches,
quorums, AND/OR compositions, hashlocks and timelocks. It does not read human
evaluation requests as templates. It reads evaluation policies only to reject
their operator shapes, using the documented normalization from
`audit_human_catalog.py`. It keeps one training request per operator shape.
Each policy uses one domain for relative locks and one for absolute locks,
so a path cannot demand incompatible block/time units from the same clock.

The first pool uses seed 20260906: 256 write and 128 tree tasks, with 384
distinct operator shapes. All 141 human-v2 shapes were excluded. It also has
zero operator-shape overlap with the later composition-transfer-v1 draft.
This is not a proof that no training policy is logically equivalent to an
evaluation policy: normalization does not perform every Boolean reduction.

Compilation checks the authored policy against decoded references, checks
execution with assumed-valid signatures, and requires full-credit grading.
An unsupported proposal fails the entire compilation; it is not silently
dropped. Fallback reference encodings and tree baselines are recorded in
`reference-notes.json`. See the human-eval README for their limits.

```bash
python scripts/compose_training.py
cargo run --release -p bench-cli --example compile_training_catalog -- \
  datasets/composed-training-v1-source.json datasets/composed-training-v1
python scripts/rl_prepare.py --pool datasets/composed-training-v1 \
  --out datasets/rl-composed-training-v1.jsonl \
  --model runs/sft-qwen3-4b-think/merged --thinking
python scripts/prepare_composed_sft.py
```

The SFT builder uses exactly those training tasks and their verified targets.
It checks the tokenizer's thinking prefix and refuses to truncate a target.
Its assembly targets and trace format match the established SFT curriculum;
the current output is `datasets/sft-composed-training-v1-asm.jsonl`.
The 384 examples have a maximum length of 3927 tokens. All completion targets
were extracted and scored through the reward service at full credit.

## Probe before spending more on RL

The initial uncapped probe sampled eight answers on 40 training tasks. Only
4/40 groups had nonzero binary reward variation. A separate probe using exact
rendered prompts and the trainer's 4096-token rollout budget found 2/40.
Raw samples are retained in `runs/composed-training-v1-probe*.jsonl`.
These are training diagnostics, not held-out capability scores.

Probe and trainer explicitly set temperature 0.6, top-p 0.95, top-k 20 and
min-p 0. The budgeted probe records its seed. Its fixed seed controls both
task selection and generation requests; training has its own seeded stream.
Reward defaults are binary, with no syntax or agreement shaping. The pilot
uses a separate reward service on port 9901.

```bash
python scripts/rl_probe.py --data datasets/rl-composed-training-v1.jsonl \
  --tasks 40 --seed 7 --k 8 --max-completion-length 4096 --thinking \
  --reward-url http://127.0.0.1:9901/reward/batch \
  --out runs/NEW-budgeted-probe.jsonl
```

## Staged experiment

The first additional-SFT pilot starts from `sft-qwen3-4b-think/merged` and
runs one epoch over the 384 examples: learning rate 2e-5, warmup 3 steps,
batch size 1, gradient accumulation 8, and checkpoints every 10 steps.
Its output is `runs/sft-composed-v1`. Training loss is not the outcome:
repeat the training probe and run the unchanged human-v2 evaluation.

The initial pilot used hex script targets and a shorter trace format. It
regressed to 8/120 write and 0/40 tree tasks on human-v2 chat, versus the
original 32/120 and 1/40. Its budgeted training probe had 12/40 varying groups,
but mean write reward fell from 0.281 to 0.129. Reward variation alone did not
show improved capability. The further-SFT control from that parent was stopped.

That target-format change was an avoidable confound. The assembly repeat in
`runs/sft-composed-v1-asm` starts from the original checkpoint, with the same
384 tasks, learning rate and one-epoch schedule. It uses the larger GPU.
The failed hex run, its raw generations and the interrupted control remain
available. Neither is silently replaced by the corrected run.

If a warmed checkpoint has enough reward variation, compare an RL branch
with further SFT from that same checkpoint. Record steps, sampled tokens and
data exposure; a small pilot is not a compute-matched algorithm comparison.
For GRPO background and API details, see the
[TRL documentation](https://huggingface.co/docs/trl/grpo_trainer).

Training now loads bfloat16 explicitly with `dtype`, with no automatic CPU
dispatch. The older `torch_dtype` setting let TRL default to float32, which
caused offload hooks and a startup failure on the 16 GB GPU.
GRPO uses microbatches of one, accumulates eight batches and checkpoints every
ten steps. Its colocated vLLM engine uses sleep mode and an 8192-token training
context; the completion budget is 4096. These training budgets do not change
the uncapped benchmark configuration.

Local vLLM and RL launches must select the installed modern toolkit:
`CUDA_HOME=/usr/local/cuda-13.1 PATH=/usr/local/cuda-13.1/bin:$PATH`.
The shell otherwise finds nvcc 12.4, which cannot build FlashInfer kernels
for the RTX 5090. A one-step GRPO smoke test completed, including checkpoint
saving; its all-zero reward group produced no learning update.

## Fresh transfer draft

`composition-transfer-v1.json` contains 24 synthetic requests in 12 pairs.
Each asks for two or three of four compound approvals. The training grammar
places only keys inside thresholds; these requests therefore exercise a new
composition family. The references are verified, but the wording still
needs independent review before this is called a final holdout.

```bash
./target/release/btc-bench gen-human --suite composition-transfer-v1
./target/release/btc-bench audit --dataset datasets/composition-transfer-v1
```

The set is evaluation-only and cannot be exported through the normal SFT/RL
preparation paths. Keep its pairs together, and do not treat 24 related
requests as 24 independent measurements of broad Bitcoin ability.
