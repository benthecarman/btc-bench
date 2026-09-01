# btc-bench — project instructions

## Error messages come from the real world

Every error, diagnostic, or finding shown to a model must be the text
a real tool prints, whenever such a tool exists:

- rust-miniscript parse/decode/analysis errors and `AnalysisError`
  strings pass through **verbatim** — never paraphrased, typos
  included.
- Consensus findings use Bitcoin Core's canonical `script_error.cpp`
  strings ("Attempted to use a disabled opcode", "Invalid OP_IF
  construction", "OP_CHECKMULTISIG(VERIFY) is not available in
  tapscript", ...), with at most a short context clarifier appended.
- Bench-invented prose is allowed only where no ecosystem text exists
  to borrow (the asm answer parser, the equivalence verdict).

Why: models trained against this feedback meet the same words in real
work, so the learned repair loop transfers out of the bench. Invented
phrasing trains bench-specific reflexes.

Two corollaries, both test-pinned:

- **Report violations, never certify validity.** A "consensus: OK"
  line next to a decode failure hands the model ammunition for the
  "my script is still consensus-valid" dismissal. Cleanliness is
  silence.
- **Facts, not strategy.** Mechanical facts about the model's own
  submission are fair feedback; anything that reveals the reference,
  the distinguishing assignment, or the implicit decode-gate grammar
  is not.

## No generation caps

Do not set `max_tokens` (or any generation/time limit) on bench or
RL rollout runs, and do not recommend it to speed runs up. Slow
runs are the cost of honest measurement, not a problem to fix.

Why: caps silently censor the solvable tail. Measured on real runs
(2026-09-01): solved-task output p99 was 39k tokens on the hard-tail
retry and one genuine solve took 57k; an 8k cap would have zeroed
those and reported "model can't" where the truth was "harness gave
up". The upstream 120s request timeout inflicted exactly this bias
until streaming removed it — a cap is the same bug reintroduced on
purpose. Rollout budgets are the RL trainer's config, not the
bench's; a truncated rollout scoring zero is already handled by the
reward. An opt-in flag for throwaway smoke runs is acceptable;
default and headline numbers stay uncapped.

## Other standing rules

- Identify (t3) is label-only and binary, by decision (see git log
  df3ff3a); fixture params are ungraded metadata. Do not reintroduce
  parameter grading.
- Before "improving" vestigial-looking code, check `git log` for the
  commit that started removing it.
- Prompt-surface edits cut framing and grammar noise but keep
  informational qualifiers; no grading or benchmark language anywhere
  a model can see.
- Model responses are the only expensive artifact. Grading, reports,
  and pass@k must always be re-derivable offline from responses.jsonl;
  `report` re-grades live and never trusts stored results.
