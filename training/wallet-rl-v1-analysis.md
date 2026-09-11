The RL path works, but this small pilot did not improve development performance. Keep the parent checkpoint. The results do not justify a longer run on the same selected questions.

Both arms started from `runs/sft-wallet-v1/merged-step128` and ran for 32 updates. The SFT control used the exact question and completion counts sampled by RL. No evaluation result selected either final checkpoint.

| Test, combining chat and submit | Parent | RL | SFT control |
|---|---:|---:|---:|
| Wallet development | 52/80 | 49/80 | 52/80 |
| Later wallet set | 44/64 | 39/64 | 43/64 |
| Compound scripts | 144/192 | 145/192 | 143/192 |
| Human-v2 | 79/320 | 77/320 | 79/320 |

These are small changes from one seed on observed development sets, with related questions across representations and interfaces. They show no clear benefit from this RL pilot. The SFT control also failed to improve the combined counts. Neither recovered the broader performance lost in wallet SFT: broad checkpoint 528 had scored 104/320 on human-v2 before wallet SFT.

The probe did find a real reward signal: ten of 32 wallet questions and six of 32 earlier-skill questions had variation across eight samples. Every earlier script-writing sample was correct (128/128); the eligible earlier questions were all trees. The frozen selection rule therefore selected six wallet questions and six earlier tree questions. All were already in the training sources. This narrow pool is a material limitation, and it supplied no reward-derived learning signal for raw script writing.

During RL, 19 of 32 groups had reward variation. Sampling was evenly split between wallet and earlier tree tasks, 128 completions each. Of 256 completions, 204 were semantically correct and none was truncated. Every answer that earned reward contained a parsed submit call. The matched SFT control completed its actual loss-mask audit for all 256 reference rows. The control matches question/completion counts and optimizer settings, not token counts or GPU compute.

Spot checks show persistent problems:

- RL repaired the chat answer for `wp-native-single-0-template`, returning `wpkh(@0/**)`. The SFT control added an invalid trailing checksum marker instead.
- For `wp-joint-councils-0-template`, RL chat moved toward the intended OR rule but used ill-typed `or_b(pk(...),pk(...))`. Its submit answer still required both second-council signers. SFT chat used the correct `multi_a(1,...)` body, while its submit answer still failed.
- Both arms retained the extra closing parenthesis in the submit answer for `wr1-age-and-height-template`.
- Both retained the wrong conjunction in the chat answer for `wr1-quorum-or-joint-template`, combining independent alternatives with AND.
- Neither repaired the fixed human-v2 recovery example. RL chat still returned raw script when asked for a descriptor; the SFT control returned Python that prints raw script. Both submit answers used an invalid tree containing a bare backup key and `older(1008)`.

The explicit reasoning audit does not show a clear gain either. Across 144 wallet outputs per model, equivalent stated policies numbered 99 for the parent, 97 for RL and 97 for the SFT control. Correct final answers with wrong or invalid stated policies numbered 17/96, 13/88 and 14/95 respectively. Correct final answers with wrong or invalid stated bodies numbered 13, 11 and 10. These checks only cover explicitly marked expressions; missing markers are unmeasured, and the body check uses the reference internal key. Final-answer RL did not directly reward these explanations.

The next investment should be a broader RL training pool with independently written requests, changed signer roles, different AND/OR scope, new compositions and script-repair tasks. Include enough difficult raw-script questions to obtain mixed rewards there; 128/128 on the training probe alongside low free-form performance shows a distribution gap. Probe the new pool before choosing the next run length. Use a new sealed test before selecting a future general checkpoint; the current later wallet set has already been inspected.

If verified policy/Miniscript explanations are a product requirement, define explicit structured fields that can be checked alongside the final descriptor. Requiring and verifying those fields would be a separate output contract. Rewarding only the final answer does not establish reliable explanations.

The reward integration is now in place. Wallet rewards use the existing offline grader with no shaping, and wallet RL extraction requires one final descriptor submit call. Rust tests check dispatch parity, early spends, wrong answer types and extra-key branches. Python tests check parser parity and training/evaluation separation. The real HTTP callback passed all 64 gold completions and rejected the bad-output controls. All 1,312 new evaluation generations are accounted for; three reached the context limit and remained in the denominator. The original server was restored and verified through its process command and model API.

[Full results](wallet-rl-v1-results.md) · [Fixed examples](wallet-rl-v1-spot-check.md) · [Experiment plan](wallet-rl-v1-experiment.json)
