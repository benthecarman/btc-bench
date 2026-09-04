# Human request transfer eval

`human-v1.json` contains 32 synthetic requests authored for this project:
24 script-writing requests and 8 Taproot design requests. These are not
collected user conversations. The set is a draft under review.
The prompts are written as complete requests rather than assembled from
policy-node verbalizers. Public keys and hash digests are filled in
deterministically per scenario group, so paired cases share key material.

The set measures whether skills learned from the generated curriculum
transfer to ordinary technical requests. It does not establish performance
on all Bitcoin development work, or guarantee that every policy structure
is absent from existing training data. The current oracle accepts the
Miniscript subset of Script and assumes signature validity in its execution
cross-check. An alternative-hashlock draft was replaced with a single-secret
case because the reference compiler could not compile that draft.

## Review before freezing

```bash
cargo build --release
./target/release/btc-bench gen-human --out datasets/human-v1
./target/release/btc-bench audit --dataset datasets/human-v1
```

Read `datasets/human-v1/prompts.md` for the exact requests sent to models.
The source catalog also contains the internal policy and a `group` for
related cases. Review the wording against that policy, especially which
signers may act together, whether a delay applies to one path or all paths,
and whether earlier paths remain available after a recovery deadline.

Fifteen requests ask for a Bitcoin script and let the model choose the
script context. The others explicitly ask for legacy Script or Taproot.
All requests supply the public keys. Missing-information conversations, impossible requests,
transaction construction and explanation quality need separate evaluations.
Keys in the generated set are test material, never wallet keys for funds.

After the joint spot check, freeze the generated fixtures. The manifest
records catalog and fixture hashes; generation refuses to overwrite an
existing fixture set. Changing a key seed would not create an independent
holdout. Keep these requests and their derived policies out of both SFT and
RL. `sft-export` and `rl_prepare.py` reject the evaluation-only manifest.
This is an accidental-use guard, not a substitute for tracking all training
inputs. If these failures are later used to tune training, call this set a
development set and reserve new requests for final evaluation.

## Run ordinary chat

```bash
./target/release/btc-bench run --dataset datasets/human-v1 \
  --config models.toml --model YOUR_SFT_MODEL --tools chat --attempts 1 \
  --out runs/human-v1-sft
./target/release/btc-bench grade --dataset datasets/human-v1 \
  --responses runs/human-v1-sft/responses.jsonl --out runs/human-v1-sft/graded
```

`--tools chat` uses only the system message `You are a helpful assistant.`
It sends no submit or diagnostic tools and no grading feedback. There is no
generation cap. Authored requests are used verbatim, including with
`--casual`; the formal Rules block is not added.

The runner accepts a raw answer or a fenced script/descriptor with
surrounding explanation. With multiple fenced blocks, syntax can separate
a script from unrelated example code; identical hex/asm representations
count as one script. Multiple distinct script candidates are recorded as
ambiguous rather than selected using their grades. Inspect these failures:
a response can be useful to a person while the automatic extraction cannot
identify one final answer. Reasoning blocks are excluded from final-answer
extraction. Raw responses are retained for review.

Script writing uses full semantic equivalence, not sparse judgment rows.
For requests marked `choose_context`, raw legacy, SegWit v0 and tapscript
answers are checked against the same spending policy. Tapscript uses the
x-only versions of the supplied full public keys. The stored `context`
identifies the reference encoding, not a required model choice. The report
excludes these requests from its per-context breakdown. These tasks
check the inner script; output wrapping and transaction construction are
not graded. Explicit-context requests retain their original context gate.
Tree tasks explicitly ask to reduce worst-case input weight, so the existing
tree score matches their requested objective. Track tree semantic validity
separately from weight improvement. Judgment contracts and their training
workflow are documented in [judgment.md](judgment.md).

Run the same fixtures with `--tools none` in a separate directory to measure
the effect of restoring the trained submit interface. That is a diagnostic
comparison; the chat run measures the ordinary-request setting.

## Compare SFT and RL

Run the RL checkpoint with identical fixtures, model inference settings and
`--tools chat`, then grade its saved responses. The existing report command
can compare the two directories:

```bash
./target/release/btc-bench report --dataset datasets/human-v1 \
  --runs runs/human-v1-sft,runs/human-v1-rl \
  --labels SFT,RL --out runs/human-v1-comparison.md
```

Review newly solved and newly broken cases in the per-task results. Use
`groups.json` to keep paired cases together: two closely related requests
are not two independent observations. With only 32 tasks this is a pilot
and a source of concrete failure examples, not a precise capability estimate.
Keep the main benchmark as a regression check. Select RL training tasks
using a separate training pool, never by their rewards on this set.
