# BIP-388 wallet-policy pilot

This pilot adds 40 evaluation questions: 20 authored scenarios, each with a
BIP-388 template request and a concrete descriptor request. It measures
construction with placeholders versus full public keys. Run both forms in
ordinary chat and through `submit_descriptor` before training on this task.

The initial checkpoint-528 run is complete. See [results](wallet-policy-v1-results.md)
and [four reference/model spot checks](wallet-policy-v1-examples.md).

The source is [wallet-policy-v1.json](wallet-policy-v1.json). Its ten family
groups contain both scenario variants and both output forms. All groups
have the evaluation split. They must stay together if future data adds
paraphrases, derivation indices, or training counterparts. The manifest is
evaluation-only, so the existing SFT and RL preparation guards reject it.

These are synthetic requests authored separately from the BIP-388 English
renderer. They are not collected human conversations. The policy families
are familiar wallet patterns; this test does not claim structural novelty
against earlier training. It is a representation diagnostic, and the two
formats differ in their key appendix and explicit answer instructions.

## Integration

`crates/bench-wallet` supplies `btc-wallet-bench build`, `audit`, and `grade`.
It is a separate pilot command with a wallet fixture schema. Its scores are
not added to the existing write/tree benchmark totals. The usual benchmark
and reward server keep their existing task contract. There is no wallet
training export or wallet RL reward endpoint in this pilot.

The BIP-388 crate is pinned to revision
`b80f6288afc7b7c2de5e3b307db35b250c21e941`. Its upstream repository is
[bigspider/bip388](https://github.com/bigspider/bip388). It is experimental.
This adapter uses it for template parsing, key information, expansion, and
canonical English round trips. The existing rust-miniscript dependency and
bench-core semantic oracle check the actual spending behavior.

The pilot supports individual extended public keys, the standard receive
and change branches `<0;1>/*` (also written `/**`), and the top-level forms
`wpkh`, `wsh`, and `tr`. MuSig, arbitrary derivation paths, hashlocks, and
general compound-threshold cleartext are outside this first catalog.

## Reference and answer checks

Each scenario has a reference descriptor template and a separately authored
high-level spending policy. Both use indexed keys. The builder creates
deterministic public test keys, expands the template, and derives the actual
keys with rust-miniscript. No private key is sent to the model.

Reference audits check:

1. BIP-388 validation and a supported canonical English rendering.
2. Reconstruction of the original template from that canonical English.
3. Typed, sane descriptors at receive/change indices 0 and 7.
4. Semantic equivalence to the separately authored spending policy at all
   four derivations.

The round trip checks the library's generated English. It does not prove
that the separately authored human request says the same thing; review the
catalog's request, policy, template, and canonical English together.

Template answers must preserve the supplied key identities and standard
receive/change paths. This path rule is checked structurally. Concrete
answers must contain public keys, rather than extended keys or wildcards.
Both answer forms must use the requested descriptor type and pass Miniscript
type and sanity checks. The semantic oracle checks all relevant signer and
clock states, including atoms introduced by a candidate. It does not compare
only a few permitted spends or require compiler-identical descriptor text.

The grader checks the spending behavior of the complete descriptor,
including the Taproot key path. It does not prove optimal weight, execute
signed transactions, or assess arbitrary English explanations. The confusion
score is stored as metadata; it never affects a correctness score.

Every expected question stays in the grading denominator. Missing answers
score zero; unknown or duplicate response IDs are rejected. Chat extraction
uses answer syntax and explicit labels, without consulting a reference or
selecting whichever candidate earns the best grade.

## Commands

```bash
cargo build --release -p bench-wallet
target/release/btc-wallet-bench build \
  --catalog evals/wallet-policy-v1.json --out datasets/wallet-policy-v1
target/release/btc-wallet-bench audit --dataset datasets/wallet-policy-v1
target/release/btc-wallet-bench grade --dataset datasets/wallet-policy-v1 \
  --responses datasets/wallet-policy-v1/references.jsonl \
  --out runs/wallet-policy-v1-reference-check
```

The builder and grader refuse to overwrite earlier outputs. The dataset
manifest records fixture, catalog, dependency, and revision information.

To test a model already served at a local OpenAI-compatible endpoint:

```bash
python3 scripts/run_wallet_bench.py --dataset datasets/wallet-policy-v1 \
  --out runs/wallet-model-chat --model SERVED_MODEL_NAME --mode chat
target/release/btc-wallet-bench grade --dataset datasets/wallet-policy-v1 \
  --responses runs/wallet-model-chat/responses.jsonl \
  --out runs/wallet-model-chat/graded
```

Repeat with `--mode submit` and a new output directory. The runner saves the
exact requests and raw responses. It uses concurrency 4, temperature 0.6,
top-p 0.95, top-k 20, min-p 0, seed 20260904, and thinking enabled. It adds no
output-token or request-time cap. The server context remains 32,768 for the
baseline experiment. There are no diagnostic tools or grader retries.

This local pilot uses non-streaming JSON responses and requires structured
tool calls in submit mode. It does not use the main runner's textual tool
fallback. Compare the two representations within this pilot; do not treat
its absolute scores as an unchanged-interface comparison with older suites.

The fixed checkpoint-528 baseline uses
`training/run_wallet_policy_v1.py`, which checks the frozen inputs, loads
that checkpoint, evaluates both interfaces, and restores the original
server even if evaluation fails. Model weights are not trained or changed.

## Tests

```bash
cargo test -p bench-wallet
python3 -m unittest discover -s scripts -p 'test_run_wallet_bench.py'
```

The tests cover equivalent encodings, role swaps, clock boundaries, changed
derivation paths, extra spending branches, wrong output types, malformed
answers, reference drift, ambiguous extraction, and prompt isolation.
