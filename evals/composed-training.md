# Composed training pilot

See [the completed pilot results](composed-pilot-results.md) for the frozen
human-v2 comparisons and their limits.

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
4/40 groups had nonzero reward variation. A separate probe using exact
rendered prompts and the trainer's 4096-token rollout budget found 2/40.
Raw samples are retained in `runs/composed-training-v1-probe*.jsonl`.
These are training diagnostics, not held-out capability scores.

Probe and trainer explicitly set temperature 0.6, top-p 0.95, top-k 20 and
min-p 0. The budgeted probe records its seed. Its fixed seed controls both
task selection and generation requests; training has its own seeded stream.
The pilot uses the benchmark score with no syntax or agreement shaping:
binary correctness for write tasks, and continuous weight score for trees.
It uses a separate reward service on port 9901. Zero shaping does not make
tree rewards binary.

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

The assembly repeat scored 22/120 write and 0/40 tree tasks in chat. It
recovered part of the hex pilot's loss but did not beat the original model.
Its budgeted probe had 11/40 varying groups and mean write reward 0.366.
The next small comparison uses that assembly checkpoint as a common parent:
24 GRPO updates versus 24 further-SFT updates, learning rate 1e-5 and warmup
3 steps. This measures whether either update method improves transfer from
that starting point; the original checkpoint remains the baseline to beat.

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
Truncated rollouts receive zero reward even if they contain an earlier
complete tool call. The trainer checks the completion's final token against
its tokenizer end/pad tokens and masks truncated sequences from the loss.
This matches the budgeted probe's rejection of length-terminated answers.

Local vLLM and RL launches must select the installed modern toolkit:
`CUDA_HOME=/usr/local/cuda-13.1 PATH=/usr/local/cuda-13.1/bin:$PATH`.
The shell otherwise finds nvcc 12.4, which cannot build FlashInfer kernels
for the RTX 5090. A one-step GRPO smoke test completed, including checkpoint
saving; its all-zero reward group produced no learning update.

## Completed RL pilot

`runs/rl-composed-asm` completed 24 GRPO updates from the assembly checkpoint.
It sampled 192 completions from 24 question groups. Seven groups had mixed
rewards; four completions reached the training limit and received zero reward.
The trainer counted 475,439 prompt and completion tokens and ran for about
738 seconds on the RTX 5090. Mean sampled reward was 0.208; this is training
data, not an evaluation result. Checkpoints and derived metrics are retained.

`runs/sft-composed-asm-control` completed 24 further-SFT updates from the same
parent, using 192 examples from the same pool. It ran for about 476 seconds
on the RTX 5060 Ti. Equal update counts do not mean equal question exposure,
token counts, or compute. This is a small practical pilot, not a controlled
comparison of the two algorithms at scale.

Both branches are evaluated in chat and submit modes on unchanged human-v2.
The first control chat run used the RTX 5060 Ti with no speculative decoding.
A separate control chat repeat uses the RTX 5090 and the existing ngram
decoding settings, matching the original, assembly and RL runs. The first
run remains saved; the repeat does not replace its responses. Control submit
also uses the RTX 5090. All use a 32768-token server context, concurrency four,
and the same sampling settings. There is no separate generation limit or
time cap. A fixed seed does not remove every source of sampling variation.

`scripts/compare_human_runs.py` checks fixture hashes and includes unanswered
tasks in each denominator. It reports semantic correctness separately from
tree weight, and saves the question IDs gained and lost against a baseline.
Raw responses, failures, server logs and run configurations remain in `runs/`.

## Frozen composition transfer evaluation

`composition-transfer-v1.json` contains 24 synthetic requests in 12 pairs.
Each asks for two or three of four compound approvals. The training grammar
places only keys inside thresholds; these requests therefore exercise a new
composition family. The user authorized evaluation after the age wording
was clarified. The questions and model weights were pinned before generation
in `composition-transfer-v1-freeze.json`. These remain synthetic requests,
not independently authored human questions. See the
[completed comparison](composition-transfer-v1-results.md).

None of its 24 operator shapes occurs in the 14,700 recognized policy traces
of `datasets/sft-train-think.jsonl` (488 shapes), or in the 384 new assembly
examples. The original file's other 1,996 rows are identification tasks.
This checks stored inputs, not every historical training source, and the
normalization does not prove semantic novelty. The audit with file hashes is
saved in `evals/composition-transfer-v1-coverage.json`.

```bash
./target/release/btc-bench gen-human --suite composition-transfer-v1
./target/release/btc-bench audit --dataset datasets/composition-transfer-v1
```

The set is evaluation-only and cannot be exported through the normal SFT/RL
preparation paths. Keep its pairs together, and do not treat 24 related
requests as 24 independent measurements of broad Bitcoin ability.
