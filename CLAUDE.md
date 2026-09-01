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
