# GRPO-family loss aggregation and advantage normalization: length biases (2025–2026)

Notation used throughout: one prompt q, G sampled responses o_1..o_G with lengths |o_i| (= L_i),
scalar rewards R_i, advantage Â_i, per-token ratio r_{i,t} = π_θ(o_{i,t}|q,o_{i,<t}) / π_old(o_{i,t}|q,o_{i,<t}).
"Our scheme" = Â_i = R_i − mean(R) (no std), zero-variance groups dropped, REINFORCE with truncated IS
weight min(π_θ/π_rollout, 2), per-token weight 1/(N_groups · T_g) with T_g = Σ_{j∈g} L_j, no KL, LoRA,
one optimizer step per batch.

## Q1. Original GRPO sequence-mean (1/|o_i|) and the Dr. GRPO analysis

### Takeaway
GRPO's per-response 1/|o_i| normalizer makes the per-token update size depend on the response's own
length: correct (Â>0) short responses get larger per-token updates, and incorrect (Â<0) long responses
get smaller per-token penalties, so length inflates, mostly in wrong answers. Dr. GRPO removes both
1/|o_i| and the std divisor (in code it divides by a constant MAX_TOKENS). It keeps accuracy and cuts the
length growth of incorrect responses.

### Cited Findings
- Paper: "Understanding R1-Zero-Like Training: A Critical Perspective", Zichen Liu, Changyu Chen, Wenjun Li,
  Penghui Qi, Tianyu Pang, Chao Du, Wee Sun Lee, Min Lin; arXiv 2503.20783, submitted 26 Mar 2025,
  revised 6 Oct 2025 — [arXiv](https://arxiv.org/abs/2503.20783)
- GRPO objective as they write it:
  J_GRPO = E[ (1/G) Σ_i (1/|o_i|) Σ_t min( r_{i,t} Â_{i,t}, clip(r_{i,t},1−ε,1+ε) Â_{i,t} ) ],
  with Â_{i,t} = (R_i − mean(R)) / std(R) — [arXiv HTML](https://arxiv.org/html/2503.20783)
- Dr. GRPO "simply remove[s] the 1/|o_i| and std({R(q,o_1),…,R(q,o_G)}) normalization terms"
  — [arXiv HTML](https://arxiv.org/html/2503.20783)
- Response-level length bias, quoted: "For positive advantages, this bias results in greater gradient updates for
  shorter responses… Conversely, for negative advantages… longer responses are penalized less… causing the
  policy to prefer lengthier responses among incorrect ones." — [arXiv HTML](https://arxiv.org/html/2503.20783)
- Question-level difficulty bias from std: "Questions with lower standard deviations… are given higher weights
  during policy updates." — [arXiv HTML](https://arxiv.org/html/2503.20783)
- Abstract: GRPO has "an optimization bias … which artificially increases response length (especially for
  incorrect outputs) during training"; Dr. GRPO is "an unbiased optimization method that improves token
  efficiency while maintaining reasoning performance"; 43.3% AIME 2024 with a 7B base model
  — [arXiv abs](https://arxiv.org/abs/2503.20783)
- Implementation: their masked-mean replacement is `(tensor * mask).sum(axis=-1) / MAX_TOKENS`. The
  normalizer is a constant generation budget, not the response's own length.
  — [arXiv HTML](https://arxiv.org/html/2503.20783)
- They point out that common PPO implementations (trl, OpenRLHF, verl at the time) "normalize the loss by
  response length… which misaligns with the PPO objective", so the bias is not specific to GRPO.
  — [arXiv HTML](https://arxiv.org/html/2503.20783)
- Empirically, the incorrect-response length is "substantially reduced" under Dr. GRPO vs GRPO, at matched
  accuracy (figure in paper; exact per-step numbers not extracted) — [arXiv HTML](https://arxiv.org/html/2503.20783)
- Independent derivation of the same mechanism: "Concise Reasoning via Reinforcement Learning", Mehdi Fatemi,
  Banafsheh Rafiee, Mingjie Tang, Kartik Talamadupula; arXiv 2504.05185, 7 Apr 2025 (rev. 21 Nov 2025). They
  argue that when the model is wrong (negative reward), per-response loss minimization pushes toward
  longer outputs "even when γ=1". The effect compounds when the data is mostly unsolvable. A short second
  RL phase on solvable problems cuts length while keeping or improving accuracy.
  — [arXiv](https://arxiv.org/abs/2504.05185)

### Inferences
- Mechanism in one line: under 1/|o_i|, the per-token weight is Â_i/|o_i|. A negative-advantage response
  can lower its per-token penalty by being longer, so the policy can reduce total loss on failures by
  getting more verbose. This is a gradient-scale artifact, not reward signal.
- The bias grows with the share of negative-advantage responses (Fatemi et al.'s point). It is worst on
  hard prompt mixes where most samples are wrong.

### Gaps
- I did not extract exact per-step length curves or numbers (e.g. tokens at step N) from the Dr. GRPO
  figures. Only the qualitative "substantially reduced" claim is cited.

## Q2. DAPO token-level policy-gradient loss

### Takeaway
DAPO normalizes by the total token count of all sampled responses, 1/Σ_i|o_i|, so every token in the
batch has equal weight whatever its sequence length. The stated motivation is the opposite of Dr. GRPO's
concern: under sample-level averaging, long samples are under-weighted, so good long reasoning is
under-reinforced and gibberish or repetition in long samples is under-penalized. DAPO reports that
token-level loss prevents entropy blow-up and "unbounded" length growth. In their ablation it is the
smallest single gain (+1 AIME point).

### Cited Findings
- Paper: "DAPO: An Open-Source LLM Reinforcement Learning System at Scale", Qiying Yu et al. (35 authors,
  ByteDance Seed / Tsinghua AIR); arXiv 2503.14476, 18 Mar 2025, rev. 20 May 2025
  — [arXiv](https://arxiv.org/abs/2503.14476)
- Objective: J_DAPO = E[ (1/Σ_{i=1}^G |o_i|) Σ_i Σ_t min( r_{i,t} Â_{i,t}, clip(r_{i,t}, 1−ε_low, 1+ε_high) Â_{i,t} ) ],
  s.t. 0 < |{o_i : is_equivalent(a, o_i)}| < G (dynamic sampling) — [arXiv HTML](https://arxiv.org/html/2503.14476)
- Rationale: under sample-level loss "tokens within longer responses (which contain more tokens) may have a
  disproportionately lower contribution to the overall loss". The result is (1) high-quality long samples do
  not reinforce reasoning patterns well, and (2) "undesirable patterns" such as gibberish and repetition in
  long samples are not penalized enough. Their Figure 4 shows token-level loss stops excess entropy rise and
  unhealthy length growth. — [arXiv HTML](https://arxiv.org/html/2503.14476)
- Overlong handling: **Overlong Filtering** masks the loss of truncated samples. **Soft Overlong Punishment**:
  R_length(y) = 0 if |y| ≤ L_max − L_cache; ((L_max − L_cache) − |y|)/L_cache if L_max − L_cache < |y| ≤ L_max;
  −1 if |y| > L_max, with L_max = 16,384 and L_cache = 4,096. — [arXiv HTML](https://arxiv.org/html/2503.14476)
- Ablation (AIME24 avg@32, Qwen2.5-32B): naive GRPO 30 → +Overlong Filtering 36 → +Clip-Higher 38 → +Soft
  Overlong Punishment 41 → +Token-level Loss 42 → +Dynamic Sampling 50. — [arXiv HTML](https://arxiv.org/html/2503.14476)
- Dynamic sampling rationale: if all outputs in a group get the same reward, the advantage is zero and
  gives no gradient, so such prompts are filtered and resampled. — [arXiv HTML](https://arxiv.org/html/2503.14476)

### Inferences
- Token-mean over the whole batch removes the *own-length* normalizer, so within a batch it is the
  Dr. GRPO-style unbiased estimator up to one shared scale 1/Σ|o_i|. Per-token weight is Â_i/T_batch for
  every token.
- The two papers do not disagree on mechanics. Dr. GRPO stresses that 1/|o_i| rewards verbose failures,
  and DAPO stresses that it under-penalizes junk in long samples. Both describe the same 1/|o_i| artifact
  and arrive at a length-independent per-token weight.

### Gaps
- DAPO does not split the token-level-loss effect on length by correct vs incorrect responses. Figure 4
  is qualitative.

## Q3. GSPO, GMPO, CISPO, verl loss_agg_mode, and comparative studies

### Takeaway
- GSPO is a *sequence-level* method: it averages over sequences (1/G) and uses a length-normalized
  sequence ratio, so its gradient carries a 1/|y_i| per-token factor like GRPO's.
- CISPO (MiniMax-M1) uses the DAPO token-level normalizer 1/Σ|o_i| and clips only a stop-gradient IS
  weight.
- verl exposes token-mean (DAPO), seq-mean-token-mean (original GRPO, length-biased), seq-mean-token-sum
  and seq-mean-token-sum-norm (Dr. GRPO-style constant normalizer).
- ScaleRL compared sample-, prompt- and token-average aggregation. **Prompt-average, which is exactly our
  scheme, had the best asymptotic performance.**
- "Tricks or Traps" finds that token-level is best for base models and sequence-level for aligned models.
- "Balanced Aggregation" (2026) analyzes our exact per-group token-mean and finds a sign–length coupling:
  when negatives are longer, token-agg puts more weight on negatives.

### Cited Findings
**GSPO**
- "Group Sequence Policy Optimization", Qwen team (Chujie Zheng et al.); arXiv 2507.18071, Jul 2025
  — [arXiv HTML](https://arxiv.org/html/2507.18071)
- s_i(θ) = (π_θ(y_i|x)/π_old(y_i|x))^{1/|y_i|} = exp( (1/|y_i|) Σ_t log r_{i,t} );
  J_GSPO = E[ (1/G) Σ_i min( s_i Â_i, clip(s_i, 1−ε, 1+ε) Â_i ) ]. The length normalization keeps
  s_i in a consistent numerical range across lengths. All tokens in a response get equal weight, unlike
  GRPO's per-token ratios. GSPO clips about two orders of magnitude more tokens than GRPO yet trains more
  efficiently, and it removes the need for Routing Replay on MoE. — [arXiv HTML](https://arxiv.org/html/2507.18071)

**GMPO**
- "Geometric-Mean Policy Optimization", Yuzhong Zhao, Yue Liu, … Furu Wei; arXiv 2507.20673, 28 Jul 2025
  (rev. 18 Oct 2025). It maximizes the geometric mean of token-level importance-weighted rewards instead of
  the arithmetic mean, to be less sensitive to outlier IS ratios. Reported up to +4.1% avg Pass@1 over GRPO
  on math (7B). I found no explicit length claim in the abstract. — [arXiv](https://arxiv.org/abs/2507.20673)

**CISPO / MiniMax-M1**
- "MiniMax-M1: Scaling Test-Time Compute Efficiently with Lightning Attention", MiniMax; arXiv 2506.13585,
  Jun 2025 — [arXiv HTML](https://arxiv.org/html/2506.13585)
- J_CISPO = E[ (1/Σ_i|o_i|) Σ_i Σ_t sg(r̂_{i,t}(θ)) Â_{i,t} log π_θ(o_{i,t}|q,o_{i,<t}) ], with
  r̂_{i,t} = clip(r_{i,t}, 1−ε^IS_low, 1+ε^IS_high). Rationale: low-probability "fork" tokens ("However",
  "Recheck", "Wait") get clipped out of PPO/GRPO updates. CISPO keeps every token's gradient and only
  bounds the IS weight. It shows a 2x speedup vs DAPO on Qwen2.5-32B. — [arXiv HTML](https://arxiv.org/html/2506.13585)
- **Directly relevant length finding:** "During output length extension, negative samples increase in length
  substantially faster than positive samples, frequently reaching the context window limit earlier." This
  led to "pattern collapse" (garbled tails). Their fixes: (1) early stop on repetitive patterns (3,000
  consecutive tokens each with p > 0.99), (2) "Adopting combined sample-level loss and token-level
  normalization to alleviate negative-positive sample imbalance", and (3) a lower grad-clip threshold and
  ε^IS_high. — [arXiv HTML](https://arxiv.org/html/2506.13585)
- AIME average response length exceeded 20k tokens during RL, and AIME24 went from 68% to 80%.
  — [arXiv HTML](https://arxiv.org/html/2506.13585)

**verl loss_agg_mode** (source: verl/trainer/ppo/core_algos.py, `agg_loss`, main branch as fetched Oct 2026)
- `token-mean`: masked_sum / batch_num_tokens (× dp_size for global-batch invariance). This is DAPO.
- `token-sum`: masked_sum.
- `seq-mean-token-sum`: sum tokens per sequence, then mean over sequences. Per-token weight is Â_i/B, with
  no own-length factor, but gradient scale grows with length.
- `seq-mean-token-mean`: mean over tokens per sequence, then mean over sequences. This is the original GRPO
  1/|o_i|, with the Dr. GRPO length bias.
- `seq-mean-token-sum-norm`: seq-mean-token-sum / loss_scale_factor (a constant). This is Dr. GRPO's
  MAX_TOKENS normalizer.
- `compute_grpo_outcome_advantage(norm_adv_by_std_in_grpo=True)`: "If False, the advantage is not scaled, as
  in Dr.GRPO (https://arxiv.org/abs/2503.20783)."
  — [verl core_algos.py](https://raw.githubusercontent.com/volcengine/verl/main/verl/trainer/ppo/core_algos.py)
- verl has no built-in mode for "token-mean within each group, then mean over groups" (our scheme).
  — [verl core_algos.py](https://raw.githubusercontent.com/volcengine/verl/main/verl/trainer/ppo/core_algos.py)

**ScaleRL ("The Art of Scaling Reinforcement Learning Compute for LLMs")**
- Devvrit Khatri, Lovish Madaan, … Rishabh Agarwal et al. (Meta); arXiv 2510.13786, 15 Oct 2025
  — [arXiv HTML](https://arxiv.org/html/2510.13786)
- Compared three aggregations. **Prompt-average:** "each prompt contributes equally" and token losses in a
  prompt are normalized by 1/Σ_{g=1}^G |y_g|. **Sample-average:** each rollout counts equally regardless of
  length. **Token-average:** all tokens in the batch are averaged directly. Prompt-average achieved the
  highest asymptotic performance and is the ScaleRL default. — [arXiv HTML](https://arxiv.org/html/2510.13786)
- CISPO form used: J = E[ (1/T) Σ_i Σ_t sg(min(ρ_{i,t}, ε_max)) Â_i log π_train(y_{i,t}|…) ]. This is a
  REINFORCE with truncated IS, the same family as our loss. "Both GSPO and CISPO substantially outperform
  DAPO" in asymptotic pass rate, and CISPO was chosen. — [arXiv HTML](https://arxiv.org/html/2510.13786)
- Advantage normalization: prompt-level std (GRPO), batch-level std (Â_i / std over batch), and none
  (Dr. GRPO) "yield similar performance". Batch-level was adopted. — [arXiv HTML](https://arxiv.org/html/2510.13786)
- Zero-variance filtering improved asymptotic performance. "No-Positive-Resampling" (drop prompts with
  pass rate ≥ 0.9 in later epochs) also helped. — [arXiv HTML](https://arxiv.org/html/2510.13786)
- Length control: forced interruptions ("Okay, time is up. Let me stop thinking and formulate a final
  answer </think>"). A DAPO-style length penalty R = clip((L_max−|y|)/L_cache − 1, −1, 0) with L_max 14k and
  L_cache 2k "does not improve performance" when swapped in. Truncation rates of 10–15% typically
  destabilized baselines. ScaleRL kept truncations below 5% for over 90% of training at scale.
  — [arXiv HTML](https://arxiv.org/html/2510.13786)
- FP32 LM head raised asymptotic A from 0.52 to 0.61. PipelineRL with max off-policyness k=8 matched
  asymptote with better efficiency. — [arXiv HTML](https://arxiv.org/html/2510.13786)

**"Part I: Tricks or Traps? A Deep Dive into RL for LLM Reasoning"** (Liu et al., Alibaba; arXiv 2508.08221,
Aug 2025, later revision dated Oct 2025)
- Token-level aggregation is best for base models, and response-level aggregation for aligned/instruct
  models. — [arXiv HTML](https://arxiv.org/html/2508.08221)
- Advantage normalization: "Calculating the mean at the local (group) level and the standard deviation at
  the global (batch) level enables more robust reward shaping". This avoids gradient amplification when
  group rewards are concentrated. — [arXiv HTML](https://arxiv.org/html/2508.08221)
- Overlong filtering helps at an 8k max length, where models become more concise, and helps little at 20k,
  where it mainly removes degenerate repetitive samples. — [arXiv HTML](https://arxiv.org/html/2508.08221)
- "Lite PPO" = group-mean + batch-std normalization + token-level aggregation. It beat GRPO and DAPO on base
  models. — [arXiv HTML](https://arxiv.org/html/2508.08221)

**"Balanced Aggregation: Understanding and Fixing Aggregation Bias in GRPO"** (arXiv 2605.04077, May 2026)
- Defines token aggregation as averaging "over all tokens in the group", which is our per-group token
  mean. It shows the objective is ∝ (T̄₊ δ̄₊^tok − T̄₋ δ̄₋^tok), a **sign–length coupling**: "when negative
  responses are systematically longer than positive responses, token-agg places disproportionate weight on
  negative samples." Sequence aggregation avoids the coupling but downweights longer responses within
  each sign group. — [arXiv HTML](https://arxiv.org/html/2605.04077)
- Fix: compute token means separately over positives and negatives, then J_BA = (k/G) L₊ + ((G−k)/G) L₋,
  where k is the number of positive responses. On DAPO-17k and Polaris, BA matches or beats peak
  performance and avoids the late-training degradation and loss drift seen with token-agg.
  — [arXiv HTML](https://arxiv.org/html/2605.04077)
- Model dependence: Qwen2.5-Math-7B (large length variance) favors token-agg, while Qwen3-1.7B (large
  pos/neg length gap) favors seq-agg. — [arXiv HTML](https://arxiv.org/html/2605.04077)

**Other related work found (not deeply read)**
- "GRPO is Secretly a Process Reward Model", Michael Sullivan; arXiv 2509.21154 (ICML 2026). It shows that
  GRPO induces an implicit PRM through shared prefixes in a group and proposes λ-GRPO, a step-aware scaling.
  — [arXiv](https://arxiv.org/abs/2509.21154)
- "λ-GRPO: Unifying the GRPO Frameworks with Learnable Token Preferences", arXiv 2510.06870, learns a λ that
  interpolates the token weighting between the GRPO, DAPO and Dr. GRPO normalizers.
  — [arXiv](https://arxiv.org/abs/2510.06870)

### Inferences
- GSPO's gradient is ∇J = E[(1/G) Σ_i s_i Â_i · (1/|y_i|) Σ_t ∇log π(y_{i,t})] (from the s_i definition).
  The per-token weight carries 1/|y_i|, the same structure Dr. GRPO identifies as length-biased. No source
  found measures GSPO's length behavior against this specific bias.
- The verl modes map to the papers as follows. seq-mean-token-mean is biased (GRPO). token-mean (DAPO)
  and seq-mean-token-sum-norm (Dr. GRPO) are free of own-length bias. seq-mean-token-sum is free of
  own-length bias but has length-scaled gradient magnitude.

### Gaps
- I found no paper titled "On the Length Bias of GRPO". The closest are Dr. GRPO, Fatemi et al., and
  Balanced Aggregation.
- ScaleRL's per-aggregation results are in Appendix A.9 figures. I did not extract numbers or any
  length/truncation curves per aggregation mode.
- I did not get the full GSPO author list or exact date, beyond "Qwen team, July 2025, 2507.18071".

## Q4. How our scheme (token-mean within group, equal weight per prompt) behaves

### Takeaway
Our aggregation is ScaleRL's "prompt-average" aggregation, which ScaleRL found best asymptotically.
Within a group it is the Dr. GRPO/DAPO-style token-uniform weight Â_i/T_g, so the GRPO "verbose failure"
artifact is absent to first order. A long correct answer gets more *total* gradient because it has more
tokens, but each token gets the same push as a short correct answer's tokens. That is the unbiased
REINFORCE gradient of log π(o_i), not an artificial length incentive. Length should then move only as far
as reward correlates with length within groups. Two second-order effects remain:
(a) the normalizer T_g includes each response's own length, which leaves a diluted, roughly 1/G version
of the GRPO bias;
(b) the sign–length coupling that Balanced Aggregation and MiniMax-M1 document.

### Cited Findings
- ScaleRL prompt-average normalizes each prompt's tokens by 1/Σ_g|y_g|, with equal weight per prompt.
  This is identical to our 1/(N_groups·T_g). ScaleRL found it the best of sample/prompt/token average.
  — [ScaleRL, arXiv 2510.13786](https://arxiv.org/html/2510.13786)
- Balanced Aggregation analyzes exactly "token aggregation… over all tokens in the group". When negatives
  are longer than positives, it places disproportionate weight on negatives, and the authors report
  late-training degradation and loss drift for token-agg. — [arXiv 2605.04077](https://arxiv.org/html/2605.04077)
- Dr. GRPO: removing the per-response 1/|o_i| (any length-independent normalizer) yields the unbiased
  objective. — [arXiv 2503.20783](https://arxiv.org/html/2503.20783)
- MiniMax-M1: when length grows, negatives grow faster than positives and hit the context limit first.
  — [arXiv 2506.13585](https://arxiv.org/html/2506.13585)

### Inferences
These derivations are mine and not taken from a source.
- **Within-group weights.** Response i's tokens each get weight w_i = Â_i/(N·T_g). The sequence-level
  coefficient on ∇log π(o_i) = Σ_t ∇log π(o_{i,t}) is Â_i/(N·T_g). Since Σ_i Â_i = 0 (mean-centering), this
  is REINFORCE with a group-mean baseline scaled by one per-group constant. If T_g were fixed, there would
  be zero length preference beyond the reward's own correlation with length.
- **Token mass by sign.** In a group with k correct responses, k·Â₊ = (G−k)·|Â₋|. So the positive:negative
  token-gradient mass ratio equals L̄₊ : L̄₋, the mean length of correct vs incorrect responses. This is
  Balanced Aggregation's coupling. It is still the exact policy gradient: longer failures get more
  "push-down" tokens because they contain more tokens. In practice this pushes *against* length when
  failures are long (truncations, loops). It can destabilize via a large negative mass, which is the
  degradation BA reports, but it does not by itself inflate length.
- **Own-length leakage through T_g.** T_g = L_i + Σ_{j≠i} L_j. A response's own length shrinks every
  weight in its group, its own included. For a correct response, being longer lowers its coefficient
  Â_i/(N·T_g). For an incorrect one, being longer lowers its penalty coefficient. This is the GRPO bias
  direction, diluted from 1/L_i to 1/(L_i + rest of group). It matters most when G is small and one
  response is much longer than the rest, e.g. one truncated or looping sample at max length beside short
  ones. With G=8 and similar lengths, it is roughly 1/8 the strength of GRPO's own-length bias. Dr.
  GRPO's constant MAX_TOKENS normalizer, or verl's seq-mean-token-sum-norm, removes it fully.
- **Cross-group weighting.** Equal weight per prompt means prompts with long responses get smaller
  per-token weights but the same total "budget". Hard or long-response prompts are not upweighted by
  token count. Batch token-mean (DAPO) would upweight them. This is a prompt-mix effect, not a
  within-group length incentive.
- **Answer to the direct question.** Yes, a long correct answer gets more total gradient (∝ L_i·Â_i/T_g)
  than a short correct one. That is inherent to the score-function gradient of a longer sequence. In
  expectation it raises length only if correct answers are longer than incorrect ones within the same
  group, which is the real reward–length correlation. If drift is seen, the likely causes are (i) a real
  "longer ⇒ more often correct" correlation on hard prompts, (ii) truncation handling (Q5), or (iii) the
  diluted own-length leakage above. Logging L̄₊ vs L̄₋ per group would show which applies. If L̄₊ > L̄₋
  consistently, the reward itself is pulling length up and the aggregation is not.

### Gaps
- I found no paper that measures the length dynamics of per-prompt token-mean (prompt-average) against
  token-mean or Dr. GRPO head-to-head on length curves. ScaleRL reports performance; whether it reports
  length per aggregation mode was not confirmed.
- The ~1/G strength of the own-length leakage is my estimate, not a measured result.

## Q5. Std normalization, dropping zero-variance groups, and truncation handling

### Takeaway
- Std normalization mainly reweights prompts by difficulty, not by length. Dr. GRPO calls it a
  difficulty bias, and ScaleRL found no-std, group-std and batch-std perform similarly.
- With mean-centered advantages, dropping zero-variance groups does not change the gradient direction,
  because those groups contribute zero gradient. It changes only the scale (via N_groups) and the
  compute spent.
- Truncation handling is the strongest length lever in this list. Scoring truncated samples 0 makes them
  negatives, an implicit length penalty. Masking them (DAPO overlong filtering) removes that pressure. The
  "Tricks or Traps" results show masking's effect depends on the length budget.

### Cited Findings
- Std normalization upweights low-variance (very easy or very hard) questions, a "question-level difficulty
  bias". — [Dr. GRPO, arXiv 2503.20783](https://arxiv.org/html/2503.20783)
- ScaleRL: prompt-level, batch-level and no normalization "yield similar performance".
  — [arXiv 2510.13786](https://arxiv.org/html/2510.13786)
- "Tricks or Traps": group mean with batch std is more robust when group rewards are concentrated, which
  avoids gradient amplification. — [arXiv 2508.08221](https://arxiv.org/html/2508.08221)
- DAPO dynamic sampling: zero-variance groups have zero advantage and give no gradient. DAPO filters and
  resamples them to keep the effective batch full, which was worth +8 AIME points in the ablation.
  — [arXiv 2503.14476](https://arxiv.org/html/2503.14476)
- ScaleRL: zero-variance filtering improved the asymptote. — [arXiv 2510.13786](https://arxiv.org/html/2510.13786)
- DAPO overlong filtering (mask truncated) was +6 AIME (30→36). DAPO's rationale is that penalizing
  truncated but possibly valid reasoning adds reward noise. Soft overlong punishment adds a graded penalty
  in the last L_cache tokens. — [arXiv 2503.14476](https://arxiv.org/html/2503.14476)
- "Tricks or Traps": overlong filtering helps substantially at an 8k cap, where models generate more
  concise responses, but little at 20k, where it mostly removes degenerate repetitive samples.
  — [arXiv 2508.08221](https://arxiv.org/html/2508.08221)
- ScaleRL uses forced interruptions instead of zero-scored truncation. A length penalty did not help, and
  truncation rates above ~10–15% destabilized training. — [arXiv 2510.13786](https://arxiv.org/html/2510.13786)
- MiniMax-M1 adds an early-stop on repetitive generation, because negatives grow faster and hit the
  context limit first. — [arXiv 2506.13585](https://arxiv.org/html/2506.13585)

### Inferences
- In our scheme (truncated ⇒ reward 0 ⇒ negative advantage when any sibling is correct), a truncated
  sample is a long negative. Under per-group token-mean it contributes large negative token mass, an
  implicit length penalty, but also own-length leakage that slightly softens its per-token penalty (Q4).
  If all G samples truncate, the group has zero variance and is dropped. Prompts where the model *always*
  overruns give no length signal at all.
- Dropping zero-variance groups changes the per-group weight 1/N_groups if N counts surviving groups. The
  effective LR per prompt then rises when many groups are dropped, e.g. late in training when easy
  prompts saturate. That is a step-size effect, not a length effect.
- Removing std normalization (as we do) avoids upweighting nearly-all-wrong groups. Those groups are
  often where long, truncated failures sit, so no-std modestly *reduces* the weight on long-failure
  groups relative to GRPO.

### Gaps
- I found no study that isolates "mask truncated vs score 0" under per-group token-mean on length curves.

## Q6. Off-policy / IS corrections (TIS, MIS, sequence-level) and length

### Takeaway
Token-level truncated IS (min(π_θ/π_rollout, C), with C≈2 the common default) has no explicit length
term. Its documented purpose is to correct trainer-vs-inference numeric mismatch. Sequence-level IS
variants use either the raw product, which explodes with length, or a length-normalized geometric mean,
which removes the length scaling. Masking variants (MIS, DeepSeek-V3.2's off-policy sequence mask, which
masks only negative-advantage sequences) remove samples rather than shrink them. I found no primary
source that directly measures TIS's effect on response length. One secondary summary says mismatch
compounds with rollout length and that collapse shows up as *shorter* responses.

### Cited Findings
- verl Rollout Correction docs: IS weights are truncated at a configurable threshold ("typically 2.0").
  Token-level thresholds run 1.5–5.0 with lower variance. Sequence-level uses the product of ratios with
  thresholds 2.0–10.0 and is "more sensitive to outliers". A geometric (mean-based) sequence mode is also
  offered. Rejection-sampling (mask) modes use k1/k2/k3 divergence bounds. "Bypass" mode
  (π_rollout = π_old) supports REINFORCE with explicit IS weights.
  — [verl docs: Rollout Correction](https://verl.readthedocs.io/en/latest/algo/rollout_corr.html)
- ms-swift docs list the formulas:
  - TIS: w = min(π_θ(y_t)/π_vLLM(y_t), τ)
  - MIS: w if w ≤ τ else 0
  - Sequence-level weight: [π_θ(y|x)/π_vLLM(y|x)]^{1/|y|}, truncated or masked
  - DeepSeek off-policy sequence mask: mask sequence i if δ_i > τ AND Â_i < 0, with
    δ_i = (1/|y_i|) Σ_t (log π_old − log π_θ)

  Default τ = 2. Sources cited are Yao et al. 2025, "Your Efficient RL Framework Secretly Brings You
  Off-Policy RL Training" (fengyao.notion.site/off-policy-rl), Liu et al., "When Speed Kills Stability"
  (yingru.notion.site), and DeepSeek-V3.2 (arXiv 2512.02556).
  — [ms-swift docs](https://swift.readthedocs.io/en/v4.4/Instruction/GRPO/AdvancedResearch/training_inference_mismatch.html)
- verl's core code multiplies the per-token PG loss by `rollout_is_weights` before aggregation. The IS
  weight is a per-token multiplier that enters before loss_agg_mode normalization.
  — [verl core_algos.py](https://raw.githubusercontent.com/volcengine/verl/main/verl/trainer/ppo/core_algos.py)
- ScaleRL's CISPO is the same structure as ours: sg(min(ρ, ε_max))·Â·log π, normalized by 1/T.
  — [arXiv 2510.13786](https://arxiv.org/html/2510.13786)
- A search-result summary of an OpenReview paper on training–inference mismatch says the mismatch
  "compounds with rollout length as the approximation errors accumulate over autoregressive steps". It
  also says rollouts "collapse to shorter, lower-diversity responses", and that response length is an
  early-warning signal. **Low confidence:** I could not open the PDF (login wall) to confirm the title or
  context. It may refer to sparse-attention rollouts rather than standard bf16 mismatch.
  — [OpenReview PDF (unverified)](https://openreview.net/pdf/5470539181a5e0ea6228e37c5bfec0dd7f6d9150.pdf)

### Inferences
- With one optimizer step per batch, π_θ = π_old at the gradient step. Our IS ratio therefore measures
  only trainer-vs-rollout numeric mismatch (vLLM kernels, LoRA merge or precision), not policy staleness.
  Ratios should be near 1 and the cap of 2 should rarely bind.
- Token-level truncation is applied per token, so it adds no 1/|o| factor and no length-dependent
  normalizer. Truncating ratios > 2 slightly *shrinks* gradients on tokens the trainer likes more than the
  sampler did, and leaves ratios < 1 untouched. If mismatch grows along long sequences, as the unverified
  source suggests, long responses would have slightly more down-weighted tokens. That is a weak bias
  *against* reinforcing long responses, not toward them.
- Sequence-level IS with a raw product would be strongly length-coupled. The geometric mean (GSPO-style)
  removes that coupling. Our token-level TIS avoids the issue.

### Gaps
- Primary sources for TIS and "When Speed Kills Stability" are Notion blogs that I could not fetch. Their
  exact dates, authors and any length plots are unverified here.
- I found no controlled study of TIS vs MIS vs no-IS on response-length growth.
