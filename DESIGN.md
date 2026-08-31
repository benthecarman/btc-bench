# btc-bench — an LLM benchmark for Bitcoin Script

First benchmark for AI models that write, optimize, and identify Bitcoin
Script. No prior art exists as of 2026-08 (nearest neighbors: Solidity
decompilation benches such as SCDBench). Recall benchmark: standard and
protocol templates are public knowledge, and we accept that models have
memorized them.

Rust workspace, three crates:

- `bench-core` — task types, fixture schemas, graders, equivalence oracle.
  No network I/O.
- `bench-gen` — seeded policy sampler, English verbalizer, naive
  de-optimizer, identification corpus, fixture writer.
- `bench-cli` — the `btc-bench` binary: fixture generation, offline
  grading, and (next phase) the live model runner.

## Task types

### Task 1 — write a script that does X

1. Generator samples a concrete miniscript policy from a seeded grammar
   (tiers below). Guardrails: satisfiable, `Policy::compile()` succeeds,
   201-opcode and stack limits hold, atom count small enough for the
   truth-table oracle (see below).
2. Reference = compiled miniscript and its script bytes. Fixtures store
   the compiled script, weights, and the policy string. Compiler output
   is not stable across rust-miniscript versions, so the compiled bytes —
   not the policy — are the answer key.
3. Prompt = deterministic English template walk of the policy AST. Fixed,
   distinct vocabulary for relative vs absolute timelocks. Keys appear in
   the prompt as labeled variables ("Alice's key: 02…"), context-correct:
   33-byte compressed for legacy/segwit, 32-byte x-only for taproot.
   The verbalizer has multiple hand-written template families per AST
   node (`verbal::FAMILIES`); family 0 is the canonical benchmark
   phrasing and stays byte-stable (test-pinned). `gen
   --verbal-families <id,id>` draws per task from an explicit family
   list, chosen by a per-task seed salt (never the main rng stream, so
   policy sampling is unaffected). The split is a generation-time
   guarantee: family 0 is bench-only; training sets list only non-eval
   families. When a new family is authored, it starts held-out — never
   trained on — until deliberately released into the training list, so
   there is always at least one never-trained family to measure
   phrasing transfer against (the report's spec-family cut).
   Word swaps alone leave the shared template skeleton learnable, so
   `--vary-structure` additionally varies the clause tree (seeded, per
   task): and/or/thresh children are permuted in the prose — they are
   commutative, so the policy, reference, and oracle are untouched —
   and the root list renders inline, numbered, or as lettered spending
   paths. Eval structure stays canonical. No LLM paraphrasing
   anywhere: every variant is authored per node, so prose can never
   drift from policy semantics, and timelock/hash vocabulary stays
   distinct in every family and shape (test-pinned).
4. Model returns one script (hex or asm). Grading:
   - Parse answer to bytes; malformed = 0.
   - Gate: `Miniscript::decode_consensus` with the task's context.
     Not-a-miniscript or type-invalid = 0.
   - Oracle: semantic equivalence to the reference (below). Pass = 1.

Contexts (all three, v1): legacy (model writes the P2SH redeemScript),
segwit v0 (P2WSH witnessScript), taproot (the tapleaf script; key-path
design is out of scope, prompt states the target). The model always
writes the inner script.

Tiers: easy ≤ 2 atoms, no timelocks; medium 2–6 atoms with a timelock
or hash always present (the 2-atom band — two keys plus one non-key
atom — is the deliberate calibration step above easy; synthetic
two-level shapes plus the MINT-001/002 timelock-in-thresh structure at
one draw in four); hard 7–12 atoms, timelocks + hashes + `thresh`,
every shape census-verified shippable. Split 40/40/20. Write/optimize
prompts state the asm notation rule (opcode names carry the OP_
prefix). The Miniscript decode-gate requirement is deliberately NOT
stated in prompts: producing a script that composes into valid
Miniscript is part of what the benchmark measures.

### Task 2 — write a more optimized script

1. Same generator and oracle as Task 1.
2. Baseline = the systematic de-optimizer; encoding details and the
   or/thresh shape requirement are under "Task 2 (refinements from
   implementation)" below.
3. Prompt hands the model the baseline script and states the metric:
   input weight primary, script size secondary.
4. Candidate must remain semantically equivalent (Task 1 oracle).
   Score per metric = clamp((base − cand) / (base − optimal), 0, 1);
   headline = weight score; size score reported.

### Task 3 — identify what this script does

1. Item shows the raw output script (scriptPubKey) plus the
   redeemScript/witnessScript where the family has one. No sample
   witnesses; P2TR items are labeled plain `p2tr`.
2. Families v2 (datasets/v2): the ten textbook standards plus a
   Lightning corpus covering every commitment era — P2WSH to_local,
   to_remote under anchors, keyed anchors, offered/received HTLC with
   and without the anchors CSV clause, and the taproot era
   (bolt-simple-taproot PR #1330): TR to_local delay, TR to_remote,
   TR anchor, and TR offered/accepted HTLC timeout tapleaves — plus a
   Liquid federation peg item (N-of-M with CSV-gated 2-of-3 emergency
   backup). Excluded as byte-indistinguishable from existing families:
   LN funding output (= bare_multisig), legacy to_remote (= P2WPKH),
   HTLC-success/timeout second stage (= to_local), shared P2A (= p2a),
   and TR-ZFC variants that collapse onto the above. Zero-fee
   commitments reuse the no-anchors HTLC scripts (verified in
   rust-lightning's test_anchors).
   Pins: lightning/bolts master 152897261850 (P2WSH families,
   cross-checked against rust-lightning chan_utils.rs); bolts PR #1330
   (taproot). Liquid is constructed from the documented structure and
   is NOT byte-pinned to the production fedpegscript. Coinswap and
   Revault are pending: their canonical sources moved or vanished
   (coinswap-mmcs/spec 404s; teleport-transactions renamed; no
   revault script repo found).
   Protocol items rotate 4 of 11 families per identify group (~70/30
   standard/protocol overall).
3. No near-miss distractors, no `unknown` class (decision: do not
   punish models for knowing the templates).
4. Answer = single flat label + parameters extracted mechanically from
   the script (`k`, `n`, `delay`, `timeout`, `hash_type`, anchors
   flag). Per-parameter credit: a wrong label = 0; a correct label =
   `partial_credit + (1 − partial_credit) × param_fraction`, where
   `param_fraction` is correct params over claimed-plus-expected
   params (Jaccard-style, so invented params dilute — spamming keys
   never pays). Exact params still = 1.0; `partial_credit` defaults
   to 0.5. Denser than the old all-or-nothing params bonus, which
   scored one-wrong-param the same as no params at all — and a denser
   RLVR signal. No LLM judge anywhere.

### Task 4 — design a taproot tree

1. Generator samples a root-level disjunction: one bare-key branch
   (the key-path candidate) plus 1-5 compound branches (key+key,
   key+timelock, key+hash, one thresh at most), tiered by branch
   count (easy 2, medium 3-4, hard 5-6) inside the oracle's atom
   budget. Every branch requires a signature.
2. The answer is a `tr(INTERNAL_KEY,TREE)` descriptor string — the
   model chooses the key path and the leaf split, which a single-leaf
   tapscript task cannot measure. The prompt supplies the BIP-341
   NUMS point for policies where no key deserves the key path.
3. Reference = hand-built: the bare-key branch becomes the internal
   key, every other branch compiles to its own leaf
   (`Concrete::compile::<Tap>`), leaves form a balanced binary tree
   (equal odds make that the Huffman shape, and it minimizes
   worst-case control-block depth — the scored metric). NOT
   `compile_tr`: miniscript 13.1's TapTree Display emits closing
   braces after the next leaf instead of before it, so any tree with
   a depth decrease between consecutive leaves prints as a malformed
   string its own parser rejects; answer keys must round-trip as
   strings because model answers are strings. Baseline = the whole
   policy as one leaf under the NUMS key; the generator requires
   baseline weight strictly above reference weight, so the task is
   never vacuous.
4. Grading: parse (must be `tr()`), lift both sides
   (`Tr::lift` = or(internal key, leaves)), truth-table equivalence
   with the fixture's unspendable key *pinned false* on both sides —
   dropped from the enumerated atom set, so it evaluates
   unsatisfied. Pinning is what makes NUMS-vs-extracted-key designs
   comparable: a NUMS atom is genuinely unspendable, so formula
   equivalence after pinning is real-world equivalence. Equivalent
   answers score on the optimize curve between baseline and
   reference weight; beating the reference (possible — worst-case
   and expected-cost optima differ) clamps to 1.0. Per-leaf lint is
   reported; every reference leaf passes the execution oracle; the
   audit re-derives descriptors, weights, and the strictly-heavier
   gate from the stored policy.
5. Runner: a `submit_descriptor` tool; multi-turn feedback mirrors
   optimize (weight gap for correct designs, verbatim parse reasons,
   never the reference). Reward service: same shaping rungs, with
   agreement computed on the lifted policies.

## Correctness oracle

Judge-free, complete for our task distribution:

1. Gate: `decode_consensus` into `Miniscript<Pk, Ctx>` (per-context:
   `PublicKey` for legacy/segwit, `XOnlyPublicKey` for tap).
2. Fast path: lift both to `semantic::Policy`, `sorted()`, `PartialEq`.
3. Full oracle: exhaustive truth-table equivalence. Every generated
   task has a closed atom set: keys (satisfied / not), preimages
   (known / not), absolute and relative timelocks. Keys and preimages
   are boolean; timelocks are monotone, so testing at each distinct
   atom value and one below it (union of both policies' breakpoints)
   plus the extremes is complete. Two monotone step functions equal on
   every breakpoint are equal everywhere. Atom counts are bounded by
   the tiers (≤ 12 atoms), so the table is at most ~2^12 × a few
   timelock points × a small AST — milliseconds in Rust.

rust-miniscript provides parsing, type-checking, context validity, and
lifting; only the truth-table walk is ours (~100 lines over the
crate's `semantic::Policy` enum). The crate's own lift+compare is
documented incomplete (Gröbner bases), hence step 3 — required anyway
for Task 2, where structural difference is guaranteed by design.

Two generated shapes are ungradable and resampled at fixture-build
time: compiled references that the Legacy optimizer renders as `pk_h`
(the script bytes carry only a 20-byte hash, which decodes to `RawPkH`
and cannot lift), and policies mixing height and relative timelocks in
a single spending path (the lifter rejects those by design).

### Execution cross-check (dual oracle)

After rust-miniscript's bitcoind integration tests, every reference
and baseline is also proven *spendable* at fixture-build time: the
semantic policy is searched for one satisfying assignment, the crate
satisfier builds a concrete witness (dummy signatures; hash preimages
and timelocks are real — hashes are sampled as image-of-known-preimage
for this), and the crate interpreter executes the witness under the
output's natural wrapping (P2SH scriptSig, P2WSH witness, P2TR
script-path with a real commitment). The execution path shares no code
with the truth-table walk, so each oracle cross-checks the other;
`oracle::tests::agrees_with_mutual_entailment` additionally pins
agreement with the crate's bounded entailment on both polarities.
Fixtures carry the sampled preimages (`hash_preimages`) so the audit
can re-run this check; they leak nothing the embedded answer key does
not already contain.

## Task 2 (refinements from implementation)

The naive baseline is a *sampled* de-optimizer (after rust-miniscript's
insane-fragment corpus, `bitcoind-tests/data/random_ms.txt`): each
policy node is rendered through a randomly chosen type-valid encoding —
`and_v(v:…)` chains vs `and_b(…, s:…)` altstack conjunctions vs
`andor(…, 0)`; `or_d` vs `or_b` vs `t:or_i` vs `t:or_c` vs `andor(…,1,…)`;
`thresh` as k-subset enumeration (or_d arms, t:or_i arms, andor folds)
or a `thresh(k, pk, a:pk, …)` node with no `multi`. Candidates are
validated by `from_str_insane` parsing before use; the fixture oracle
remains the final arbiter. A single deterministic style (the original
encoder, kept as the reference form) is learnable as a pattern; the
sampled space presents the full variety of hand-written bloat.

Optimize tasks sample policies containing an `or` or `thresh`, where
the naive encoding is guaranteed non-optimal: plain 2-key `and_v`
chains are already the compiled optimum and would give the optimizer
nothing to do. The generator asserts baseline weight > compiled weight;
shapes that fail the assertion are skipped.

Weights come from `Descriptor::max_weight_to_satisfy()` (the
non-deprecated weight API; it supersedes `max_satisfaction_weight`)
on `sh(ms)` / `wsh(ms)` / `tr(<dummy key>, leaf)` wrappers.

### Hard-tier shapes and the shape census

The four MINT vault structures (MINT-005..009: vault_full,
vault_simplified, timelock_gated_recovery, vault_single_principal) and
the original synthetic hard sampler were **removed**: a shape census
(parse → compile → lift self-check → execution oracle, per context)
showed all five died at the pk_h/RawPkH gate in every context — thresh
groups under or-branches make the compiler emit `pk_h`, whose script
bytes decode as `RawPkH` and cannot lift — so they never shipped a
single fixture in any dataset and only burned retries. MINT-001/002
(timelock_in_thresh, 3–5 atoms) survives and is classified as a
medium-tier shape, matching its atom budget.

The hard tier now samples four census-verified shapes, each held to
the full 7..=12 atom budget (test-enforced): absolute timelocks hugging
the height/time encoding boundary from below (499999996–499999999 —
499999999 is the last height-encoded CLTV value; the BIP65 threshold
500000000 is inclusive, so values ≥ it are UNIX timestamps and stay out
of this shape), a custody-style recovery structure (instant path behind
a relative timelock, committee path behind an absolute one; thresh only
under and-branches), deep and-nesting over mixed atom kinds with one
embedded key-or, and k-of-n at the subset-expansion cap (C(n,k) ≤ 12).
The census runs as a permanent test: every dispatched shape must ship
in ≥ 10/30 trials, so a future shape that cannot ship fails CI instead
of silently skewing the distribution. Duplicate-key sampling was also
removed — the sane compiler rejects repeated keys in every context, so
every reused-key draw was dead rng burn.

### Lint (insanity) reporting

Every decoded write/optimize answer is analyzed with miniscript's own
predicates (`requires_sig`, `is_non_malleable`, `within_resource_limits`,
`has_repeated_keys`, `has_mixed_timelocks`, `contains_raw_pkh`). The
findings are mechanical facts about the submitted script, so they
appear in graded output (`lint` on each task score) and in multi-turn
feedback, without affecting scores. Finding texts are rust-miniscript's
own `AnalysisError` Display strings, verbatim (typos included): the
feedback a model trains against here is the same text real tooling
prints, so the learned repair loop transfers out of the bench. The
same principle holds across diagnostics — decode errors and compiler
errors pass through untouched; bench-invented prose exists only where
the ecosystem has no text to borrow (the answer parser, the
equivalence verdict). Task scores also carry a structural `failure`
class ("parse error" / "decode reject" / "wrong semantics" / "gated")
set from the verdict, so the report taxonomy never string-matches
library error text (the old substring matching miscounted
optimize/tree decode rejects as parse errors). `grade --standard-mode` opts into
gating: answers with findings score 0 and the findings become the
reason. Type-correct-but-insane answers (e.g. malleable rewrites) are
equivalence-legal by design; the lint makes the distinction visible.

## Dataset audit

`btc-bench audit --dataset <dir>` re-derives every answer key from
first principles (after rust-miniscript's differential
`regression_compiler` fuzz methodology): stored scripts must decode,
oracle-verify against themselves, and pass the execution oracle;
policies must recompile (byte drift that stays oracle-equivalent is a
warning, non-equivalence is a hard failure); stored weights must match
freshly computed values; optimize baselines must stay equivalent and
strictly heavier; manifest pins must match the declared dependency
versions. Run it in CI whenever a dataset is regenerated for release
or a dependency bumps. A golden grader test (`bench-core/tests/golden`) pins grading
behavior — scores, reasons, and lint output — against committed
fixtures and answers.

## Runner

- Default mode is multi-turn (attempts = 3): after a graded failure
  the model receives mechanical feedback (parse errors and decode-gate
  rejections verbatim, lint findings, the optimize weight/size gap;
  never the reference, its keys, or the distinguishing assignment) and
  may retry. Single-shot (attempts = 1) measures unaided fluency;
  multi-turn measures feedback-driven recovery. The regression gate
  runs single-shot for speed and noise.
- Tool-assisted mode (`run --tools basic`): beside the submit tool the
  model gets `check_script` (write/optimize: parse, decode gate, lint,
  weight/size) or `check_descriptor` (tree), executed locally from
  `bench_core::toolbox`. The inviolable rule: diagnostics are pure
  functions of model-supplied input — no fixture parameter exists, so
  no reference can leak, by construction. check_* calls loop within
  the same graded attempt (budget 16 per task; exhaustion tells the
  model to submit); only a submit or a no-call response ends the
  turn. Identify stays tool-less (the asm decode is already the
  prompt). `tool_calls` lands on each response record and summaries
  report call efficiency. What it measures: the none-to-basic delta
  per model is the mechanical-formatting deficit (recoverable with a
  compiler loop); the residual at basic is the semantic gap. Keep
  tool-assisted runs single-attempt — tools-within-a-turn and graded
  feedback between turns measure overlapping recovery. The reward
  service mirrors the diagnostics at POST /tool for RL trainers
  driving their own rollouts.
- Sampling: temperature 0, n = 1, pass@1 by default; n, temperature,
  top-p configurable. Raw responses are always stored, so pass^k is
  computable post-hoc without re-running.
- Embedded scripts (optimize baselines, identify scriptPubKeys) render as
  decoded Bitcoin Core asm by default; `--display hex` switches to raw
  hex. Answers are accepted in either notation regardless.
- Display and answer parsing are ONE dialect, test-enforced: every
  script the bench displays must re-parse byte-identically
  (to_human_asm -> parse_script_answer). All-digit push tokens are
  ambiguous between hex and decimal; they resolve by position exactly
  as the renderer emits them — decimal directly before
  OP_CLTV/OP_CSV, raw hex everywhere else. The asymmetry this fixed
  (`36 OP_CSV` parsing as hex 0x36 = 54) silently misgraded models
  echoing displayed notation on 21% of displayed scripts. Guarded at
  three layers: parser unit tests, a generated-fixture round-trip
  property test (roundtrip.rs, including grade-level assertions that
  displayed reference asm earns full marks), and the dataset audit,
  which round-trips every displayed script including identify
  spk/inner and tree leaves.
- Providers via `goose-providers` (team choice; alpha, pinned exact):
  `openai` (Responses API), `openai_compatible` (chat/completions against
  any base URL, non-streaming), `anthropic` (Messages API, streaming;
  tool calls arrive complete on the stream). Models are listed in a
  `models.toml` (see `models.example.toml`): `[[model.<name>]]` tables
  with `provider`, `model`, optional `base_url`, `api_key_env`,
  `temperature` (default 0.0); generation length is never capped. Tools are
  `rmcp::model::Tool` values: `submit_script{script}` for write/optimize
  tasks and `submit_identify{label, params}` for identify tasks, one
  presented per task. Runs are sequential. A task whose response carries
  no tool call (or errors) goes to `failures.jsonl` with the raw text;
  it counts as unanswered at grading time.
  If goose-providers embedding ever becomes genuinely blocked, we
  surface exactly what is missing and decide — no silent fallback.

## Reporting statistics

- Grade summaries and the sweep report's headline means carry 95%
  percentile-bootstrap CIs (1000 resamples over tasks, seeded and
  deterministic). A regression is only a regression when the CIs say
  so.
- Format vs reasoning split: an answer is *well-formed* when it
  cleared the parse and decode gates (its failure, if any, is
  semantic). Summaries report the well-formed count and "semantic
  accuracy given well-formed" — sweeps showed parse/decode failures
  were ~2/3 of zero scores, a different capability than wrong
  semantics.
- Write/optimize fixtures record the policy's boolean atom count
  (`atoms`), the continuous difficulty axis under the tiers; the
  report breaks scores down by atom count. `gen --tiers` overrides the
  default 40/40/20 cycle with an explicit tier round-robin (repeat a
  tier to weight it) for curriculum sets.

## Train/eval contamination controls

- `gen --exclude <dataset>` collects the excluded set's answer keys
  (write reference / optimize optimal script hexes) and resamples any
  sampled task that lands on one. Random keys make cross-seed
  collisions astronomically unlikely; the realistic path is same-seed
  reuse, which exclusion kills.
- Every manifest embeds a BIG-bench-style canary GUID
  (`bench_cli::CANARY`). Identify templates are public knowledge (this
  is a recall benchmark for them), so identify items are not deduped.

## RL reward shaping

The reward service serves two scores per answer: `score` (the
benchmark score, unchanged) and `shaped` (the training reward). All
shaping lives in the service layer — the graders and the benchmark
never change, so training reward experiments cannot drift the
leaderboard metric.

Shaping components, each a server flag with a per-request override:

- Parse and decode rungs: small fixed credit for clearing the answer
  parser and the miniscript decode gate. Gets syntax learned fast at
  the start of training; anneal toward zero from the trainer side by
  overriding per request.
- Agreement band: scaled by *balanced* truth-table agreement — the
  mean of agreement rates on reference-true and reference-false rows,
  computed by the same exhaustive walk as the oracle. Balanced is the
  hack-resistance property: a constant script (OP_1 / always-false)
  agrees perfectly on one side and never on the other, so it caps at
  exactly 0.5 however skewed the table is; the band normalizes 0.5 to
  zero, so constants earn none of it. 1.0 iff equivalent
  (test-pinned).
- Equivalence floor (optimize): the weight curve scores an
  equivalent-but-unimproved rewrite 0; during training, reaching
  equivalence at all deserves reward.
- Lint penalty / gate: the training analog of `grade
  --standard-mode`, so RL cannot converge to malleable-but-equivalent
  encodings unnoticed.

Guardrail: the service rejects configs where parse + decode +
agreement exceeds 0.5 — a non-equivalent answer must never approach
full credit, or the shaping itself becomes the reward hack.

## Fixtures and artifacts

`datasets/` is gitignored; the headline benchmark is a *pinned seed*,
not a committed file. The same seed and the same dependency pins
regenerate byte-identical fixtures, `audit` re-verifies the answer
keys, and the fixture files ship alongside published results (a
dependency bump can shift compiler output, and the bytes — not the
policies — are the answer keys). The generator supports `--seed` for
fresh sets. Sizes v1: 300 write, 300 optimize, 250 identify (~70%
standard / 30% protocol). Task IDs are stable and append-only across
revisions.

Manifest pins: schema version, generator git hash, seed, parameters,
miniscript / bitcoin versions, bolts commit (task 3). Per-run
artifacts in `runs/<timestamp>-<model>/`: raw request/response JSONL,
graded per-task JSON, markdown summary, and `run.json` — the run's
provenance (model entry name, dataset path with its manifest embedded,
attempts/concurrency/display knobs, bench version, start time). The
manifest is embedded, not referenced, because datasets/ is not in git:
a run must stay interpretable after the dataset directory is
regenerated. Grading is always re-derivable offline from
responses.jsonl, so grader changes never require re-running a model.

## Stack

| Crate           | Version         | Note                                  |
|-----------------|-----------------|---------------------------------------|
| miniscript      | 13.1.0          | latest stable; MSRV 1.63              |
| bitcoin         | 0.32.102        | latest stable; ceiling from miniscript `^0.32.6` |
| secp256k1       | ^0.29           | via miniscript                        |
| goose-providers | 0.1.0-alpha.7   | alpha — pin exact version             |
| rmcp            | 3.x             | model types for tools, via goose      |
| Workspace MSRV  | 1.94.1          | governed by goose-providers           |

## Roadmap

1. ✅ Design (this document).
2. ✅ Scaffold + oracle + generator + offline grading: `btc-bench
   gen|prompts|grade`, judge-free end to end.
3. ✅ Live runner on goose-providers: `btc-bench run` (single-shot,
   submit tools, responses/failures JSONL); verified against a local
   OpenAI-compatible mock server.
4. First model sweep against the pinned dataset.
5. ✅ Protocol identification corpus: Lightning across all eras
   (P2WSH + taproot) and a structural Liquid peg item, in
   datasets/v2; coinswap and Revault still pending sources.
6. ✅ Taproot tree-tier tasks (t4), pass^k reporting, and
   tool-assisted mode (--tools basic).
7. Extension tier: arbitrary (non-miniscript) scripts — needs an
   execution engine (bitcoin-scriptexec / bitcoin-circle-stf /
   bitcoind regtest) since the decode gate no longer applies.
