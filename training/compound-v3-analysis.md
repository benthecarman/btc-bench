# Interpretation and trace checks

Training completed all 288 updates in 1,191 seconds (19 minutes 51 seconds). The final training loss was 0.006809. The complete loss-mask audit passed 1,152 rows, with no target truncation.

The fixed development rule selected update 144. Update 288 fit 23/24 training-sample questions in each interface, but its development total fell from 78 to 76. This small, single-run difference does not establish overfitting by itself.

## What changed

The selected checkpoint solves 35/48 fresh questions in chat and 36/48 with the submit tool. The original solved zero in both interfaces. On questions with two conditioned teams, which never occurred together during this training, it solves 20/24 in chat and 18/24 with the tool. This supports transfer within the generated task family. It does not establish broad generalization to independent human requests.

The earlier compound-v2 model scored 22/120 on human-v2 chat write questions. This run scores 34/120, compared with the original model's 32/120. The aggregate regression is gone, but old and new errors differ: chat write gains 10 and loses 8 relative to the original. Submit write gains 9 and loses 10. The scores remain low on this wider benchmark.

## Intermediate expressions

All 48 reference-policy and reference-Miniscript controls pass. Explicit model expressions are parsed as written, without syntax repair. Miniscript is checked in Segwit v0 context. Unrecognized expression markers are reported as missing, rather than presumed semantically wrong. The extractor includes the observed “The target is a threshold signature scheme” marker.

| Fresh questions | Policy equivalent | Miniscript equivalent | Final script correct |
|---|---:|---:|---:|
| chat: new single gated-team arrangement | 22/24 | 13/24 | 15/24 |
| chat: two gated-team approvals | 20/24 | 18/24 | 20/24 |
| submit: new single gated-team arrangement | 21/24 | 18/24 | 18/24 |
| submit: two gated-team approvals | 21/24 | 15/24 | 18/24 |

The selected model states an equivalent policy on 42/48 questions in each interface. Its Miniscript is equivalent on 31/48 chat and 33/48 submit answers. Four passing chat finals and five passing submit finals have invalid intermediate Miniscript. These intermediate failures do not change final-script scores.

For example, question 020 requires a 48-block delay on Cleo and Nora's joint approval. The model states that delay in its policy, then omits it from its Miniscript and final script. Question 000 has a correct final script but an invalid `c:` wrapper in its stated Miniscript. See [three complete spot checks](compound-v3-spot-check.md).

Fresh chat failures comprise 10 decoder rejections and three semantic mismatches. Submit has nine decoder rejections and three semantic mismatches. A decoder rejection does not independently prove that all possible script execution is invalid; it means the current verifier cannot accept the candidate. The choose-context decoder sometimes reports only its last Tapscript error for a P2WSH-labelled answer. That diagnostic issue is recorded as a papercut.

Every fresh answer finished normally: `stop` for chat and `tool_calls` for submit. In human-v2 submit, tree-backup-two-clocks-either reached the available model context after 32,321 output tokens without a tool call. It remains in the denominator as a failure. No additional benchmark generation cap was applied.

## Next experiment

Use update 144 as a candidate for a small RLVR pilot after measuring sampled reward variation on training-only questions. Keep a matched additional-SFT control and evaluate both interfaces. The fresh questions above are now observed evaluation data: do not reuse their exact failures for training while describing them as an untouched final test. Reserve another independent test before the next curriculum change.

The main visible gap is now policy-to-Miniscript/script construction. The reward must still check the complete spending policy. Keep intermediate validity as a separate diagnostic unless the task explicitly requires a valid intermediate expression. A compiler-assisted mode can be a useful separate experiment, but it changes what the benchmark measures.

## Artifacts

Selected weights: `runs/sft-compound-v3/merged-step144`.

Selected weight SHA256: `b77ef92444aa63e0f8a4eca7d5d1088aec6350bb5fa04cceabc259ae8b05f579`.

Training data SHA256: `2a4213dcfcfa564567a9b78bcb247fc3cb429fbacda685ac9ddedaaf6fd24cdf`.

Exact settings, source hashes, and run paths are in `compound-v3-experiment.json` and `compound-v3-mix.json`. Full paired outcomes are in `runs/compound-v3-comparison-{chat,submit}/comparison.json`. Trace inputs, checker output, and counts are in `runs/compound-v3-trace-{input,checks}.jsonl` and `runs/compound-v3-trace-summary.json`.

The run checks semantics with the current Miniscript oracle. It is not a signed-transaction integration test. The original model remains the serving default.
