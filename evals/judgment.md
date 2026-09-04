# Judgment verifier and training repairs

Judgment contract version 1 states the complete spending rules in the prompt.
Any script encoding with the required behavior receives full credit. The
verifier checks the union of the contract and candidate key/hash atoms, plus
their timelock boundaries. An extra key, hash condition, or delayed spending
branch is therefore part of the check.

The old sparse requirement rows remain diagnostic examples. They do not define
correctness or the reward denominator. Benchmark scoring is binary. RL shaping
can reward progress on a restrictive answer, but a forbidden spending path
receives zero shaped reward. Parse failures and semantic failures give mechanical
feedback without disclosing counterexamples.

This makes judgment a precise spending-policy task, similar to write. It does
**not** establish a new measure of open design ability. A future open design task
needs separate, explicit availability and safety constraints that leave some
behavior free. Sparse examples alone cannot supply that contract. The old write
oracle already accepted equivalent encodings; compiler byte equality was never
required for full credit.

## Regenerate old judgment data

Unversioned judgment fixtures are rejected with a regeneration message. Their
prompts did not state enough rules to justify applying the new verifier silently.
Keep old responses for diagnosis, but do not compare their scores directly with
version 1 results. Generate a new directory with the intended seed and counts:

```sh
cargo build --release -p bench-cli
target/release/btc-bench gen --seed 2026 --write 0 --optimize 0 --identify 0 \
  --judgment 100 --out datasets/judgment-v1
target/release/btc-bench audit --dataset datasets/judgment-v1
```

Generation and audit compile a contract witness, verify its full semantics and
sampled examples, then check execution with known preimages. Judgment generation
honors `--exclude` using the compiled witness. This excludes exact scripts;
it does not hold out entire policy families.

The reward service, ordinary runner evaluation, and script diagnostic tools now
handle judgment. `rl_prepare.py` selects `submit_script` for judgment and refuses
unknown task types, old judgment contracts, and evaluation-only datasets. It
runs the dataset audit before rendering prompts.

## Probe and training settings

Preparation records thinking mode in each row. Probe and training reject missing
or mismatched mode metadata. Re-render old RL prompt files. Select the mode for
the intended checkpoint explicitly:

```sh
python3 scripts/rl_prepare.py --pool datasets/judgment-v1 \
  --out datasets/rl-judgment-v1.jsonl --model runs/sft-qwen3-4b/merged --thinking
python3 scripts/rl_probe.py --data datasets/rl-judgment-v1.jsonl \
  --model qwen3-4b-think --thinking --out runs/judgment-v1-probe.jsonl
```

Use `--no-thinking` throughout for non-thinking experiments. Both scripts use the
same prepared prompt. Probe and trainer share defaults of temperature 0.6,
top-p 0.95, and 8 completions per group. They expose the same override flags.
The uncapped probe uses chat completions with the stored messages, tools, and
thinking mode. Serve the same tokenizer/chat template as preparation, with tool
calling enabled. The [vLLM raw completions API](https://github.com/vllm-project/vllm/blob/main/vllm/entrypoints/openai/completion/protocol.py)
defaults to 16 tokens when no limit is supplied, so it is unsuitable for that
mode. The probe adds no generation or request-time cap. To measure the trainer's
4096-token rollout budget, explicitly add `--max-completion-length 4096` to the
probe; this uses raw completions and the exact rendered training prompt. Label
that result as budgeted. This is not an uncapped capability score.
The trainer's own rollout budget defaults to 4096 tokens.

`BTCBENCH_REWARD_URL` or `--reward-url` selects the reward endpoint in both
scripts. The probe saves raw completions, finish reasons, fixtures, prompts, and
sampling settings before requesting scores. It refuses to overwrite an existing
output. Regrade saved samples without model calls:

```sh
python3 scripts/rl_probe.py --regrade runs/judgment-v1-probe.jsonl \
  --out runs/judgment-v1-probe-regraded.jsonl
```

The probe reports reward variation, which is a training-data diagnostic. It does
not prove generalization or automatically refresh a saturated pool. Use the
separate frozen human evaluation set to measure progress.

## Resume training

Both trainers accept a checkpoint path, or the flag alone to select the latest
checkpoint under `--out`. Keep the original model, data, and training settings
when resuming; a saved final adapter is not a full optimizer checkpoint.

```sh
python3 scripts/sft_train.py --out runs/sft-qwen3-4b \
  --resume-from-checkpoint runs/sft-qwen3-4b/checkpoint-200
python3 scripts/rl_train.py --data datasets/rl-judgment-v1.jsonl \
  --out runs/rl-qwen3-4b --thinking --resume-from-checkpoint
```

## Verification limits

Weight calculation is fallible, including `OP_0`. Hash atoms include their
algorithm, so SHA-256 and HASH256 of the same hex are distinct. Timelock evaluation
keeps height and time units separate and applies the CSV type/mask checks, as in
[Bitcoin Core's transaction signature checker](https://github.com/bitcoin/bitcoin/blob/master/src/script/interpreter.cpp).

The oracle still operates on Miniscript spending semantics. The execution audit
uses assumed-valid signatures. These checks do not replace transaction tests
with real signatures, witnesses, locktime/sequence settings, and output wrappers.

Run the CPU regression checks with:

```sh
cargo test --workspace
python3 -m unittest discover -s scripts -p 'test_*.py'
```
