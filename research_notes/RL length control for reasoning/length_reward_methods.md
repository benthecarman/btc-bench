# Length-aware reward / length-penalty methods for RLVR (GRPO/PPO-style), 2025–2026

Scope: methods that control response length through the reward or advantage, not through hard generation caps. Each entry lists the formula, hyperparameters, where the length signal is computed (per answer, relative to the group, or relative to the batch), the models tested, and the reported effects. Sources were fetched from arXiv in October 2026. Paper dates use the arXiv version I fetched.

Caveat on sources: the WebFetch summaries come from a small model reading the paper. Formulas that look garbled or that I could not check against the paper are flagged.

---

## Q1. Kimi k1.5 group-relative length penalty (and later Kimi updates)

### Takeaway
Kimi k1.5 computes a min-max-normalized length reward inside each prompt's sample group. It ranges from +0.5 for the shortest sample to −0.5 for the longest. Correct answers get the full value; incorrect answers get only the non-positive part, so long wrong answers are penalized and short wrong answers are not rewarded. The penalty is warmed up: training starts with no length penalty and then switches to a constant one. Later Kimi models (K2, K2.5) moved to per-task token budgets with a penalty on truncation, plus a phase-alternating "Toggle" heuristic in K2.5.

### Cited Findings
- **Formula (Kimi k1.5).** Sample k responses with lengths len(i), and take min_len and max_len over the group. Then λ = 0.5 − (len(i) − min_len)/(max_len − min_len). len_reward(i) = λ if the answer is correct (r = 1), and min(0, λ) if it is incorrect (r = 0). — [Kimi Team, "Kimi k1.5: Scaling Reinforcement Learning with LLMs", arXiv:2501.12599 (v1 Jan 2025; v4 Jun 3 2025)](https://arxiv.org/html/2501.12599)
- **Stated intent.** The penalty is meant to "promote shorter responses and penalize longer responses among correct ones, while explicitly penalizing long responses with incorrect answers." It targets the "overthinking phenomenon", in which response length grows a lot during RL. — [arXiv:2501.12599](https://arxiv.org/html/2501.12599)
- **Warmup.** "In preliminary experiments, length penalty may slow down training during initial phases… we propose to gradually warm up the length penalty." In practice: standard policy optimization with no length penalty first, then "a constant length penalty for the rest" of training. — [arXiv:2501.12599](https://arxiv.org/html/2501.12599)
- **Long2short methods in the same report.** (1) Model merging: average the weights of the long-CoT and short-CoT models. (2) Shortest rejection sampling: sample n = 8 and keep the shortest correct response. (3) DPO with the shortest correct response as the positive and longer responses as negatives. (4) Long2short RL: the length penalty plus a "significantly reduce[d] maximum rollout length". — [arXiv:2501.12599](https://arxiv.org/html/2501.12599)
- **Reported results (Fig. 7).** k1.5-short with RL scores 60.8 on AIME 2024 at an average of 3,272 tokens. k1.5-shortest scores 88.2 on MATH500. The report says all k1.5 short variants have "superior token efficiency compared to other models". — [arXiv:2501.12599](https://arxiv.org/html/2501.12599)
- **Kimi K2 (budget control).** "We enforce a per-sample maximum token budget throughout RL training, where the budget is determined based on the type of task." Responses over the budget are "truncated and assigned a penalty". The report says this "significantly enhances the model's token efficiency, encouraging concise yet effective solutions across all domains." The motivation given: longer outputs help reasoning tasks but do not justify the inference cost in non-reasoning domains. K2 also uses temperature decay and a PTX (SFT-data) auxiliary loss. — [Kimi Team, "Kimi K2: Open Agentic Intelligence", arXiv:2507.20534 (v2 Feb 3 2026)](https://arxiv.org/html/2507.20534)
- **Kimi K2.5 "Toggle" (secondary source only).** A heuristic that alternates RL between budget-constrained phases and standard (inference-time-scaling) phases. Reported to cut output tokens by 25–30% "with negligible performance impact". — [DAIR.AI summary of the Kimi K2.5 report](https://academy.dair.ai/papers/kimi-k25-visual-agentic-intelligence); [centron.de K2.5 explainer](https://www.centron.de/en/tutorial/kimi-k2-5-explained-architecture-agent-swarm-deployment-guide/)
- **Kimi K2 Thinking evaluation budgets.** 96k thinking tokens for HLE (no tools), AIME25, HMMT25, and GPQA; 128k for IMO-AnswerBench, LiveCodeBench, and OJ-Bench. These are evaluation caps, not a training reward. — [Friendli model card for Kimi-K2-Thinking](https://friendli.ai/models/moonshotai/Kimi-K2-Thinking)
- **Follow-up critique.** Later work says Kimi's length reward "cannot be directly applied during the early stages" of RL and is only introduced in a post-RL phase. — reported in search results citing ["Efficient RL Training for Reasoning Models via Length-Aware Optimization", arXiv:2505.12284](https://arxiv.org/html/2505.12284v1)

### Inferences
- The Kimi penalty is group-relative and scale-free: only the length rank or position inside the group matters. So it keeps pushing toward shorter outputs even after the outputs are already short. That is likely why it needs a warmup and why Kimi paired it with a shorter max rollout length in long2short RL.
- Kimi's own trajectory (k1.5 soft group penalty, then K2 task-specific hard budgets with a truncation penalty, then K2.5 phase toggling) suggests that a frontier lab found budget or truncation-style signals easier to operate at scale than a continuous length reward. DLER (Q4) reaches a similar conclusion independently.

### Gaps
- The k1.5 report says the length reward is combined with the task reward. I did not confirm the weighting coefficient or the warmup step count; neither appeared in the fetched text.
- I found no primary-source (arXiv) text for the K2.5 Toggle details. The 25–30% figure comes from secondary summaries.
- The K2 report does not give the size of the truncation penalty or the per-task budget values in the section I fetched.

---

## Q2. DAPO overlong reward shaping and overlong filtering

### Takeaway
DAPO handles truncated samples in two ways. The first is "overlong filtering", which masks the loss of truncated samples so that a good-but-unfinished response is not punished. The second is "soft overlong punishment", a linear ramp from 0 to −1 over the last L_cache tokens before L_max. The ramp is a per-response penalty added to the correctness reward, and it does not depend on correctness.

### Cited Findings
- **Soft overlong punishment formula.**
  - R_length(y) = 0 if |y| ≤ L_max − L_cache
  - R_length(y) = ((L_max − L_cache) − |y|)/L_cache if L_max − L_cache < |y| ≤ L_max
  - R_length(y) = −1 if |y| > L_max

  The penalty is added to the rule-based correctness reward. — [Yu et al. (ByteDance Seed / Tsinghua AIR), "DAPO: An Open-Source LLM Reinforcement Learning System at Scale", arXiv:2503.14476, Mar 17 2025](https://arxiv.org/html/2503.14476)
- **Settings.** Qwen2.5-32B base. Maximum generation length 20,480 tokens, made up of an "expected maximum length" of 16,384 plus a 4,096-token soft punishment cache. So the ramp spans 16,384 to 20,480 tokens. — [arXiv:2503.14476](https://arxiv.org/html/2503.14476)
- **Rationale.** "Improper reward shaping for truncated samples can introduce reward noise" and destabilize training. Overlong filtering masks the gradient of truncated samples. — [arXiv:2503.14476](https://arxiv.org/html/2503.14476)
- **Ablation (AIME 2024, avg@32, progressive additions).** The fetch confirmed 36 after overlong filtering and 41 after adding soft overlong punishment. From my reading of the same Table 1 (I did not re-verify the intermediate cells this session), the full sequence is: naive GRPO 30 → + overlong filtering 36 → + clip-higher 38 → + soft overlong punishment 41 → + token-level loss 42 → + dynamic sampling (full DAPO) 50. — [arXiv:2503.14476](https://arxiv.org/html/2503.14476)

### Inferences
- DAPO's shaping is per-answer and absolute, not group-relative. It acts only near the context limit, so it is a soft cap substitute rather than an efficiency objective. It does nothing to responses below 16k tokens.
- Overlong filtering and soft punishment pull in opposite directions. Filtering removes the penalty signal from truncated samples; soft punishment adds a graded one before truncation. DAPO uses both: the ramp warns the model before the hard limit, and filtering stops the hard limit from injecting a −1 that does not depend on reasoning quality.

### Gaps
- The fetched summary said "+5 points from filtering alone". That conflicts with the progressive table (30→36 is +6 for filtering; 38→41 is +3 for the soft punishment). The intermediate values should be checked against Table 1 before anyone quotes them.

---

## Q3. L1 / LCPO (Length Controlled Policy Optimization)

### Takeaway
L1 puts a target token count in the prompt ("Think for n_gold tokens.") and trains with RL, using a reward of correctness minus α times the deviation from the target (Exact), or correctness gated by a clipped budget term (Max). This gives a 1.5B model about 3% mean length deviation, and it beats S1 budget forcing by 20–25 points absolute at matched budgets.

### Cited Findings
- **L1-Exact reward.** r(y, y_gold, n_gold) = 𝟙(y = y_gold) − α·|n_gold − n_y|. — [Aggarwal & Welleck (CMU), "L1: Controlling How Long A Reasoning Model Thinks With Reinforcement Learning", arXiv:2503.04697 (v1 Mar 2025; v2 Oct 3 2025)](https://arxiv.org/html/2503.04697)
- **L1-Max reward.** r = 𝟙(y = y_gold)·clip(α·(n_gold − n_y) + δ, 0, 1), with α = 0.0003 and δ = 0.5. In L1-Max a wrong answer always gets 0, and a correct answer gets less credit the further it runs over budget. In L1-Exact a wrong answer still receives the length-deviation penalty. — [arXiv:2503.04697](https://arxiv.org/html/2503.04697)
- **Setup.** Base model DeepScaleR-1.5B-Preview (from R1-Distill-Qwen-1.5B). Training data: DeepScaleR-Preview dataset, 40K math QA pairs (AIME, AMC, Omni-Math, STILL). Context length 4K for training and 8K for evaluation. 700 steps of LCPO-Exact, then 120 steps of LCPO-Max. The target n_gold is sampled per prompt. — [arXiv:2503.04697](https://arxiv.org/html/2503.04697)
- **Results.**
  - Beats S1 budget forcing by "100–150% relative and 20–25% absolute" on math across budgets of 512–3,600 tokens.
  - Mean length deviation is about 3%.
  - L1-Max budget-violation rate is below 2.5% (500-token threshold).
  - The 1.5B L1 model matches GPT-4o at equal generation length, and is 5% better than the non-reasoning base model at equal tokens. — [arXiv:2503.04697](https://arxiv.org/html/2503.04697)

### Inferences
- L1 changes the problem from "be short" to "follow a budget", so the length signal is per-answer against a user-specified target and is not group-relative. That makes it the method most like a soft, learned budget at inference time, as opposed to a fixed compression.
- ALP (Q4) reports that L1-Exact uses about 3,000 tokens regardless of difficulty. So budget conditioning by itself does not allocate compute adaptively.

### Gaps
- I did not extract per-benchmark accuracy tables for L1 at each budget.

---

## Q4. Catalogue of other 2025–2026 methods

### Takeaway
Most methods are one of five families:
1. Per-prompt normalized penalties among correct samples (Arora & Zanette, GRPO-LEAD, Acoer).
2. Group-relative or group-optimal targets (Kimi, ShorterBetter, GR³, DRPO).
3. Budget, step, or truncation rewards (ThinkPrune, Laser, DLER, L1).
4. Difficulty-adaptive penalties (ALP, Laser-D/DE, DA-DLER, GRPO-λ, AdaptThink).
5. Sample filtering or data selection that adds no reward term (GFPO, Fatemi two-phase).

A 2025–2026 finding that recurs: the optimizer setup (normalization, clipping, dynamic sampling) matters as much as the penalty shape. The DLER paper finds that plain truncation does as well as more elaborate penalties once the optimizer setup is fixed.

### Cited Findings

**Training Language Models to Reason Efficiently** — Daman Arora & Andrea Zanette (CMU), arXiv:2502.04463 (Feb 2025; v4 Nov 2025)
- Reward: correctness times (1 − α·σ(normalized length)). The length is normalized per prompt using the mean and std of **correct** responses only. α ∈ [0, 1) sets the compression strength. — [arXiv:2502.04463](https://arxiv.org/html/2502.04463)
- Setup: PPO with an RLOO advantage, 8 samples per prompt, 3.2k Numina Math prompts, about 100 RL steps (about 200 gradient updates). Models: R1-Distill-Qwen-1.5B and 7B. — [arXiv:2502.04463](https://arxiv.org/html/2502.04463)
- 7B results:
  - MATH500 at α = 0.1: −36% tokens, −2.2% accuracy.
  - AIME2024 at α = 0.2: −27% tokens, −4.0% accuracy.
  - GSM8K at α = 0.2: −83% tokens, −1.7% accuracy.

  Easy problems compress far more than competition problems. — [arXiv:2502.04463](https://arxiv.org/html/2502.04463)
- Flag: the fetch confirmed the sigmoid and the correct-only normalization. The exact multiplicative form above is my reading of the method, not text I re-read in this session.

**ShorterBetter** — Jingyang Yi, Jiazheng Wang, Sida Li, arXiv:2504.21370 (Apr 30 2025; v4 Dec 2 2025)
- Reward: r(y_j) = α·𝟙(correct) − β·|ℓ(y_j) − ℓ_SOL|.
  - SOL ("Sample Optimal Length") is the length of the shortest correct response in the group.
  - If no response in the group is correct, SOL is the group's average length.
  - α = 1 or 2 (α = 2 for the main 7B results) and β = 0.001.
  - Incorrect responses also get the |ℓ − SOL| penalty. — [arXiv:2504.21370](https://arxiv.org/html/2504.21370)
- Models: R1-Distill-Qwen-1.5B and 7B. Output length falls 50–80% in-domain and out-of-domain "while maintaining accuracy". On AIME (7B): 53.3% accuracy at 5,288 tokens versus the base model's 36.7% at 11,382 tokens. — [arXiv:2504.21370](https://arxiv.org/html/2504.21370)

**ThinkPrune** — Bairu Hou, Yang Zhang, Jiabao Ji, Yujian Liu, Kaizhi Qian, Jacob Andreas, Shiyu Chang, arXiv:2504.01296 (Apr 2 2025)
- Method: several rounds of RL, each with a stricter token limit. Responses over the limit are truncated and get zero reward. — [arXiv:2504.01296](https://arxiv.org/abs/2504.01296)
- R1-Distill-Qwen-1.5B: AIME24 reasoning length −50% for −2% accuracy. — [arXiv:2504.01296](https://arxiv.org/abs/2504.01296)
- Laser places ThinkPrune in the "truncation" family with adaptive limits across stages. — [arXiv:2505.15612](https://arxiv.org/html/2505.15612)

**Laser / Laser-D / Laser-DE** — Wei Liu, Ruochen Zhou, Yiyun Deng, Yuzhen Huang, Junteng Liu, Yuntian Deng, Yizhe Zhang, Junxian He, "Learn to Reason Efficiently with Adaptive Length-based Reward Shaping", arXiv:2505.15612 (May 21 2025)
- Unified framework: R̂(x, y) = C(y) + λ(y)·S(y), with a correctness term C, a length term S, and a control variable λ. Truncation, ThinkPrune, Kimi/Arora-style group rewards, and L1-Exact/Max are all special cases. — [arXiv:2505.15612](https://arxiv.org/html/2505.15612)
- Laser: S = α·𝟙(L(y) ≤ L_T), a step bonus for correct responses under a target length L_T, with α = 0.5. — [arXiv:2505.15612](https://arxiv.org/html/2505.15612)
- Laser-D: sets the target length L_A adaptively for three difficulty tiers. The metric is Expected Correct Responses, ECR_d = P_{l,d}·|C_d|, recomputed every N steps (3.5% overhead). — [arXiv:2505.15612](https://arxiv.org/html/2505.15612)
- Laser-DE: α·𝟙(R)·𝟙(L ≤ L_A) + α·(1 − 𝟙(R))·𝟙(L > L_A). This **rewards incorrect responses for being longer** than L_A, to encourage exploration. — [arXiv:2505.15612](https://arxiv.org/html/2505.15612)
- Models: R1-Distill-Qwen 1.5B, 7B, and 32B. On the 1.5B, Laser-DE scores **+6.1 points on AIME2024 with 63% fewer tokens** (about 16K → 5.8K). Evaluated out of domain on GPQA, LSAT, and MMLU. — [arXiv:2505.15612](https://arxiv.org/html/2505.15612)

**Cosine length-scaling reward** — Edward Yeo, Yuxuan Tong, Morry Niu, Graham Neubig, Xiang Yue, "Demystifying Long Chain-of-Thought Reasoning in LLMs", arXiv:2502.03373 (Feb 5 2025)
- CosFn(t, T, η_min, η_max) = η_min + ½(η_max − η_min)(1 + cos(tπ/T)), where t is the generated length and T = L_max = 14,336.
  - Correct answers: from r0^c = +2 at length 0 down to rL^c = +1 at L_max.
  - Wrong answers: from r0^w = −10 at length 0 up to rL^w = 0 at L_max.
  - Exceeding the context window: r_exceed = −10. — [arXiv:2502.03373](https://arxiv.org/html/2502.03373)
- So short correct answers are preferred, while short wrong answers are punished hardest. This encourages longer thinking when the model is wrong. The authors report more stable training accuracy and length than a plain correctness reward. — [arXiv:2502.03373](https://arxiv.org/html/2502.03373)
- Reward hacking: "with enough training compute, the model started to show signs of reward hacking, where it increased the lengths of its CoTs on hard questions using repetition." The fix is an n-gram repetition penalty (N = 40, P = −0.05, applied per token). — [arXiv:2502.03373](https://arxiv.org/html/2502.03373)

**GRPO-LEAD** — Jixiao Zhang, Chunsheng Zuo, arXiv:2504.09696 (Apr 13 2025; rev. Sep 19 2025; EMNLP 2025 main)
- Three changes to GRPO:
  - A length-regularized accuracy reward using a z-scored length and an α weight.
  - Explicit penalties for incorrect solutions.
  - Difficulty-aware advantage reweighting.

  Targets 14B models and reports SOTA at the 14B scale. — [arXiv:2504.09696](https://arxiv.org/abs/2504.09696)
- Acoer (below) classifies GRPO-LEAD as an incorrect-penalizing method and reports a 100% collapse rate in its setting. — [arXiv:2606.22716](https://arxiv.org/html/2606.22716)

**O1-Pruner (Length-Harmonizing Fine-Tuning)** — Haotian Luo, Li Shen, Haiying He, Yibo Wang, Shiwei Liu, Wei Li, Naiqiang Tan, Xiaochun Cao, Dacheng Tao, arXiv:2501.12570 (Jan 22 2025)
- RL-style fine-tuning that encourages shorter reasoning "under accuracy constraints". It pre-samples the reference model to get baseline length and accuracy. The abstract says it "not only significantly reduces inference overhead but also achieves higher accuracy". — [arXiv:2501.12570](https://arxiv.org/abs/2501.12570)

**Concise Reasoning via RL** — Mehdi Fatemi, Banafsheh Rafiee, Mingjie Tang, Kartik Talamadupula, arXiv:2504.05185 (Apr 7 2025; rev. Nov 21 2025)
- No length term at all. The paper argues that "incorrect answers inherently drive policies toward verbosity even when γ = 1", because a negative reward is minimized by spreading it over more tokens. Remedy: a brief second RL phase on a small set of solvable problems (8 MATH problems). — [arXiv:2504.05185](https://arxiv.org/html/2504.05185)
- PPO settings: λ (GAE) = 0.95 ("λ < 1 is critical") and γ = 1. Reward +1 for a correct boxed answer, −0.5 for an incorrect boxed answer, −1 for an unboxed answer. — [arXiv:2504.05185](https://arxiv.org/html/2504.05185)
- Results:
  - 1.5B on AIME'24: 12,104 → 6,752 tokens (−44%) at 30–32.5% accuracy.
  - 7B on AIME'24: 10,510 → 6,632 tokens (−37%) at 51–53% accuracy.
  - 1.5B on MATH500: 4,842 → 1,965 tokens (−59%), with accuracy going from 84.2% to 81%. — [arXiv:2504.05185](https://arxiv.org/html/2504.05185)
- GRPO "collapse modes": when a group is all-correct or all-wrong, the advantage is zero, the policy loss vanishes, KL dominates, and the conciseness pressure disappears. The authors prefer PPO, where length affects the value-based advantage. — [arXiv:2504.05185](https://arxiv.org/html/2504.05185)

**LC-R1** — Zhengxiang Cheng, Dongping Chen, Mingyang Fu, Tianyi Zhou, "Optimizing Length Compression in Large Reasoning Models", arXiv:2506.14755 (Jun 17 2025; rev. Sep 11 2025)
- GRPO with two rewards: a Length Reward for overall conciseness, and a Compress Reward that removes "invalid thinking", meaning re-verification after the answer has already been reached. Guided by two principles, Brevity and Sufficiency. — [arXiv:2506.14755](https://arxiv.org/abs/2506.14755)
- About −50% sequence length for about −2% accuracy. — [arXiv:2506.14755](https://arxiv.org/abs/2506.14755)

**AdaptThink** — Jiajie Zhang, Nianyi Lin, Lei Hou, Ling Feng, Juanzi Li, arXiv:2505.13417 (May 19 2025)
- Constrained-optimization RL that encourages skipping thinking (the "NoThinking" mode) as long as accuracy does not fall below the reference. Importance sampling balances thinking and no-thinking samples. — [arXiv:2505.13417](https://arxiv.org/abs/2505.13417)
- R1-Distill-Qwen-1.5B on three math sets: **−53% length and +2.4% accuracy**. — [arXiv:2505.13417](https://arxiv.org/abs/2505.13417)

**ALP (Adaptive Length Penalty)** — Violet Xiang, Chase Blagden, Rafael Rafailov, Nathan Lile, Sang Truong, Chelsea Finn, Nick Haber, "Just Enough Thinking", arXiv:2506.05256 (Jun 5 2025)
- Reward: r(y, q) = 𝟙[correct] − β·N·max(p_solved(q), 1/K), where p_solved is the online solve rate over K rollouts. Flag: N is described as a "normalization constant (maximum trace length)". The length term probably uses the response's own token count; the fetched formula may have lost that factor, so check the paper.
- Easy prompts pay a high token cost. Unsolved prompts still pay at least the 1/K floor. The penalty applies to correct and incorrect responses alike. — [arXiv:2506.05256](https://arxiv.org/html/2506.05256)
- Hyperparameters: β = 1e-7, K = 16–32, context 4,096–8,192, 100 gradient steps, batch size 512, LR 1e-6. Works with GRPO, RLOO, and Reinforce++. — [arXiv:2506.05256](https://arxiv.org/html/2506.05256)
- DeepScaleR-1.5B: about −50% average tokens. At a 4,096 budget: MATH-500 0.80 (vs 0.81), AIME 2024–25 0.24 (vs 0.22), OlympiadBench 0.51 (vs 0.47). It spends 21% of tokens on the easiest 50% of problems, with a 5.35× hard-to-easy token ratio. The paper says L1-Exact stays near 3,000 tokens regardless of difficulty. — [arXiv:2506.05256](https://arxiv.org/html/2506.05256)

**GRPO-λ (Stable RL for Efficient Reasoning)** — Muzhi Dai, Shixuan Liu, Qingyi Si, arXiv:2505.18086 (May 23 2025)
- Monitors the correctness ratio within each group. Low-accuracy groups get no length penalty; high-accuracy groups get one. This addresses "premature accuracy collapse" under length penalties. — [arXiv:2505.18086](https://arxiv.org/abs/2505.18086)
- +1.48% average accuracy and −47.3% CoT length on GSM8K, GPQA, MATH-500, AMC23, and AIME24. The abstract does not name the base model. — [arXiv:2505.18086](https://arxiv.org/abs/2505.18086)

**DLER (Doing Length pEnalty Right)** — Shih-Yang Liu, Xin Dong, Ximing Lu, Shizhe Diao, Mingjie Liu, Min-Hung Chen, Hongxu Yin, Yu-Chiang Frank Wang, Kwang-Ting Cheng, Yejin Choi, Jan Kautz, Pavlo Molchanov (NVIDIA), arXiv:2510.15110 (Oct 16 2025)
- Penalty: plain truncation, meaning zero reward above a target length (default 4,000 tokens). The paper's contribution is the optimizer setup around it:
  - batch-wise reward normalization instead of group-relative normalization
  - a higher clip (clip_ratio_high = 0.28)
  - dynamic sampling that drops all-0 and all-1 prompts
  - batch size 512 and 16 rollouts per prompt — [arXiv:2510.15110](https://arxiv.org/html/2510.15110)
- Diagnosis of why prior penalties fail:
  - Truncation zeros inflate reward variance and bias group advantages.
  - Clipping suppresses high-entropy "Wait" / "Alternatively" tokens, which leads to entropy collapse.
  - Rewards are sparse: about 50% of prompts are all-zero early in training, and easy prompts dominate later. — [arXiv:2510.15110](https://arxiv.org/html/2510.15110)
- Results:
  - DLER-R1-1.5B: −77% length. DLER-R1-7B: −69% length (2,405 average tokens).
  - DA-DLER (difficulty-aware truncation, e.g. 2,000 vs 4,000 tokens at a 0.5 correctness threshold) cuts a further 15% (1.5B) and 12% (7B).
  - About 4× single-response latency improvement.
  - With the same DLER setup, truncation, cosine, L1-Max, and Laser penalties all improve, but they "shift performance along the efficiency frontier rather than beyond it". Truncation is competitive and cheaper to train.
  - Update-selective merging (keep the top 25% of parameter deltas, scaled by 0.7) recovers accuracy for Llama-3.1-Nemotron-8B at −46% length.
  - Baselines: Laser, AdaptThink, LC-R1, VeriThinker. — [arXiv:2510.15110](https://arxiv.org/html/2510.15110)

**DRPO (Decoupled Reward Policy Optimization)** — Gang Li, Yan Chen, Ming Lin, Tianbao Yang, arXiv:2510.04474 (Oct 6 2025)
- Problem: in GRPO with a length penalty, a correct but long rollout can get a negative advantage once mixed with zero-reward incorrect ones. Fix: normalize the length signal for correct rollouts only within the positive group, using weights ω(o|q) = exp(r_l(o)/λ)/E[exp(r_l(o)/λ)]. — [arXiv:2510.04474](https://arxiv.org/html/2510.04474v1)
- 1.5B on GSM8K: −77% length (1,563 → 356 tokens) for −1.1% accuracy, versus RLOO-LP at −68% length and −4.3% accuracy. — [arXiv:2510.04474](https://arxiv.org/html/2510.04474v1)

**GFPO (Group Filtered Policy Optimization)** — Microsoft, "Sample More to Think Less", arXiv:2508.09726 (Aug 2025; ICLR 2026)
- No reward term. GFPO samples larger groups and trains only on the subset filtered by length or by token efficiency (reward per token). This acts as implicit reward shaping. There is also an Adaptive Difficulty variant. — [arXiv:2508.09726](https://arxiv.org/pdf/2508.09726); [Microsoft Research page](https://www.microsoft.com/en-us/research/publication/sample-more-to-think-less-group-filtered-policy-optimization-for-concise-reasoning/)
- Phi-4-reasoning: cuts GRPO's length inflation by up to 85% on AIME24/25, GPQA, Omni-MATH, and LiveCodeBench "while preserving accuracy", for 7% more training time and about 30% lower end-to-end latency. — [arXiv:2508.09726](https://arxiv.org/pdf/2508.09726)

**GR³ (Group Relative Reward Rescaling)** — Zichao Li, Jie Lou, Fangchen Dong, Zhiyuan Fan, Mengjie Ren, Hongyu Lin, Xianpei Han, Debing Zhang, Le Sun, Yaojie Lu, Xing Yu, arXiv:2603.10535 (Mar 11 2026)
- Multiplicative instead of additive: R̂ = R·1/(1 + α·ℓ/ℓ̄), where ℓ̄ is the group mean length and α = 0.33 (calibrated). Because R = 0 for wrong answers, it is effectively correct-only. GRPO with 16 samples per prompt. — [arXiv:2603.10535](https://arxiv.org/html/2603.10535)
- Claim: "additive penalties introduce a compensatory effect" that the policy can exploit, while multiplicative rescaling ties the length signal to task reward. — [arXiv:2603.10535](https://arxiv.org/html/2603.10535)
- R1-Distill-7B on AIME24: 13,213 → 7,923 tokens (−40%), with accuracy 57.1% (GRPO) → 60.1%. In RLHF (Qwen3-8B, Arena-Hard) the score goes 77.2 → 92.8 while length stays flat (1,171 → 1,178). Also tested on R1-Distill-1.5B and Qwen3-4B/8B. — [arXiv:2603.10535](https://arxiv.org/html/2603.10535)

**Acoer (Adaptive Correct-Only Rewards)** — Jungseob Lee, Seungyoon Lee, Seongtae Hong, Minhyuk Kim, Chanjun Park, Heuiseok Lim, "Beyond Penalizing Mistakes", arXiv:2606.22716 (Jun 21 2026)
- Reward: r = 1 + α_t·g(ℓ/B_t) if correct, and 0 + r_format if incorrect. Here g(x) = log(1 + k(1 − x))/log(1 + k) with k = 5.
  - B_t = max(B_min, 0.85·EMA of correct-answer lengths).
  - α_t is a control loop: +2% when accuracy holds, −5% if accuracy drops by more than 2% over 100 steps. — [arXiv:2606.22716](https://arxiv.org/html/2606.22716)
- Qwen3-1.7B on NuminaMath-TIR:
  - MATH-500: 88.4% (base level), with tokens 5,553 → 2,134 (−62%).
  - MATH-Hard: −56% tokens at 78.1%.
  - AIME25: −33% at 36.7%.
  - OlympiadBench: −46% at 55.3%. — [arXiv:2606.22716](https://arxiv.org/html/2606.22716)

**Other 2026 items found but not read in depth**
- Group Prioritized Off-Policy Optimization (POPO), [arXiv:2606.01281](https://arxiv.org/html/2606.01281).
- BPPO (Binary Prefix Policy Optimization, concise responses), [arXiv:2605.28028](https://arxiv.org/pdf/2605.28028).
- ERR+ (sequential entropy resolution), [arXiv:2608.28771](https://arxiv.org/pdf/2608.28771).
- Dynamic Rollout Editing for overthinking, [arXiv:2606.17890](https://arxiv.org/pdf/2606.17890).
- Survey, "Towards Concise and Adaptive Thinking in LRMs", [arXiv:2507.09662](https://arxiv.org/pdf/2507.09662).

### Inferences
- Where the length signal is computed matters:
  - Per-answer absolute: DAPO, L1, Laser, ThinkPrune/DLER truncation, cosine.
  - Group-relative: Kimi, Arora (per-prompt stats over correct samples), ShorterBetter (group shortest correct), GR³ (group mean).
  - Difficulty-adaptive through group solve rate: ALP, GRPO-λ, Laser-D, DA-DLER.
  - Batch-relative: DLER, deliberately, to reduce variance.
- Group-relative penalties combined with GRPO's group normalization interact in ways that DRPO, Acoer, and DLER each identify as a failure source. This is the main methodological lesson of late 2025 to 2026.

### Gaps
- GRPO-LEAD's exact α and z-score formula and its numeric results were not in the fetched abstract.
- O1-Pruner's reward formula and numbers were not in the abstract.
- LC-R1's exact reward weights were not extracted.
- AdaptThink's δ was not extracted.
- GRPO-λ's base model and thresholds were not extracted.
- I found no primary source for "Don't Overthink It" or the "Short is better" papers in this pass. They are not covered.

---

## Q5. Correct-only vs. penalizing incorrect answers; reward hacking

### Takeaway
Penalizing incorrect answers' length is now the main suspected cause of training instability under GRPO. Short-wrong reward hacking can only arise when a short wrong answer is not penalized relative to a long wrong answer; Kimi's design deliberately avoids this. The opposite hack, padding length with repetition, appears when wrong answers are rewarded for length (cosine). Later methods increasingly use correct-only signals (Arora, DRPO, GR³ implicitly, Acoer) or turn the penalty off on hard prompts (GRPO-λ, ALP's solve-rate scaling, Laser-D).

### Cited Findings
- **Classification of the methods in Q4.**
  - Correct-only (wrong answers get no length term): Arora & Zanette (normalized over correct samples); L1-Max (wrong = 0); GR³ (multiplicative, so wrong = 0); DRPO (normalized within the positive group); Acoer (β = 0); Laser base (the bonus only goes to correct responses under L_T).
  - Penalize wrong answers' length: Kimi k1.5 (min(0, λ), so wrong answers only lose reward for being long); ShorterBetter (|ℓ − SOL| applies to all); L1-Exact; ALP (uniform); GRPO-LEAD ("explicit penalties"); DAPO's overlong ramp.
  - Reward wrong answers' length: cosine (wrong answers get −10 when short, rising to 0 at L_max); Laser-DE (bonus for wrong answers over L_A).

  Sources: [Kimi](https://arxiv.org/html/2501.12599), [Arora](https://arxiv.org/html/2502.04463), [L1](https://arxiv.org/html/2503.04697), [ShorterBetter](https://arxiv.org/html/2504.21370), [ALP](https://arxiv.org/html/2506.05256), [cosine](https://arxiv.org/html/2502.03373), [Laser](https://arxiv.org/html/2505.15612), [GR³](https://arxiv.org/html/2603.10535), [DRPO](https://arxiv.org/html/2510.04474v1), [Acoer](https://arxiv.org/html/2606.22716)
- **Collapse from penalizing incorrect length (Acoer).**
  - Structural pathway: in mixed groups, a continuous length penalty on wrong answers creates "a constant downward pressure" on their length. In all-wrong groups the reward std approaches 0, so the normalized advantage magnitudes "diverge".
  - In their tests, GRPO+LP, GRPO-LEAD, and ReCut had a **100% collapse rate**.
  - Correct-only penalties can still over-compress through the same divergence inside the correct subgroup (the stochastic pathway), which motivates the adaptive budget and α.
  - Binary-threshold (step/truncation) penalties are "immune" because their gradients are zero almost everywhere. — [arXiv:2606.22716](https://arxiv.org/html/2606.22716)
- **Correct long answers pushed negative.** With GRPO and an additive penalty, a correct but long rollout can end up below the group mean when mixed with zero-reward wrong ones. — [DRPO, arXiv:2510.04474](https://arxiv.org/html/2510.04474v1)
- **Additive penalties are exploitable.** "Additive penalties introduce a compensatory effect" that enables shortcuts; multiplicative rescaling removes it. — [GR³, arXiv:2603.10535](https://arxiv.org/html/2603.10535)
- **Lengthening hack.** When wrong answers are rewarded for length (cosine), the model "increased the lengths of its CoTs on hard questions using repetition rather than learning to solve them", which required an n-gram repetition penalty. — [arXiv:2502.03373](https://arxiv.org/html/2502.03373)
- **Why wrong answers lengthen in the first place.** Negative rewards drive verbosity even at γ = 1. Training on unsolvable problems is the source. — [Fatemi et al., arXiv:2504.05185](https://arxiv.org/html/2504.05185)
- **Accuracy-gated penalties.** The penalty is applied only when group accuracy is high, to avoid "premature accuracy collapse". — [GRPO-λ, arXiv:2505.18086](https://arxiv.org/abs/2505.18086)
- **Exploration on wrong answers.** Laser-DE explicitly rewards long incorrect responses to encourage exploration on hard prompts, and reports the best AIME result in that paper (+6.1 points). — [arXiv:2505.15612](https://arxiv.org/html/2505.15612)

### Inferences
- Short-wrong hacking needs short wrong answers to beat long wrong answers or beat failure. Kimi's min(0, λ) only ever lowers the reward of wrong answers, and only for being long. So the short-wrong incentive there is indirect: within a group of wrong answers, shorter ones get relatively higher advantage. This is the GRPO normalization pathway that Acoer formalizes.
- For a binary verifiable-reward setup like this one, the safest designs in the 2025–2026 literature have three properties: (a) correct-only or multiplicative (GR³, Acoer, DRPO, L1-Max); (b) gated or scaled by group solve rate (ALP, GRPO-λ); (c) normalized at the batch level or with care for all-wrong groups (DLER, Acoer).

### Gaps
- I found no head-to-head paper that measures the rate of "short wrong answer" hacking under each design. The evidence is indirect (collapse rates, accuracy drops).

---

## Q6. Accuracy cost vs. token savings; evidence that length penalties improve accuracy

### Takeaway
Typical reported trade-offs are 35–80% fewer tokens for −0 to −4 points of accuracy. Several papers report accuracy gains: Laser-DE +6.1 on AIME24, AdaptThink +2.4, GRPO-λ +1.48, GR³ +3.0 on AIME24, ALP +2 to +4 on hard sets, ShorterBetter 7B on AIME, and O1-Pruner qualitatively. A careful 2026 study, however, finds that no length-control method gives a uniform accuracy improvement. Accuracy follows a peak-then-plateau-or-decline curve in length, which suggests that gains come from trimming past the peak and from better optimization, not from the penalty itself.

### Cited Findings
Trade-off table (as reported in each paper):

| Method | Model | Token change | Accuracy change | Source |
|---|---|---|---|---|
| Arora & Zanette α = 0.1/0.2 | R1-Distill-7B | −36% MATH500 / −27% AIME / −83% GSM8K | −2.2 / −4.0 / −1.7 | [2502.04463](https://arxiv.org/html/2502.04463) |
| ThinkPrune | R1-Distill-1.5B | −50% AIME24 | −2 | [2504.01296](https://arxiv.org/abs/2504.01296) |
| LC-R1 | (multiple) | ~−50% | ~−2 | [2506.14755](https://arxiv.org/abs/2506.14755) |
| Fatemi two-phase | R1-Distill-1.5B | −44% AIME24; −59% MATH500 | ~flat AIME; 84.2 → 81 MATH500 | [2504.05185](https://arxiv.org/html/2504.05185) |
| ShorterBetter (α = 2, β = 0.001) | R1-Distill-7B | 11,382 → 5,288 AIME (~−54%) | 36.7 → 53.3 (as reported) | [2504.21370](https://arxiv.org/html/2504.21370) |
| Laser-DE | R1-Distill-1.5B | −63% AIME24 | +6.1 | [2505.15612](https://arxiv.org/html/2505.15612) |
| AdaptThink | R1-Distill-1.5B | −53% | +2.4 | [2505.13417](https://arxiv.org/abs/2505.13417) |
| GRPO-λ | n/a | −47.3% | +1.48 avg | [2505.18086](https://arxiv.org/abs/2505.18086) |
| ALP (β = 1e-7) | DeepScaleR-1.5B | ~−50% | −1 MATH500, +2 AIME, +4 Olympiad | [2506.05256](https://arxiv.org/html/2506.05256) |
| DRPO | 1.5B | −77% GSM8K | −1.1 (vs RLOO-LP −68% / −4.3) | [2510.04474](https://arxiv.org/html/2510.04474v1) |
| DLER | R1-Distill-1.5B / 7B | −77% / −69% | "maintained" | [2510.15110](https://arxiv.org/html/2510.15110) |
| GFPO | Phi-4-reasoning | up to −85% of GRPO's inflation | "preserved" | [2508.09726](https://arxiv.org/pdf/2508.09726) |
| GR³ (α = 0.33) | R1-Distill-7B | −40% AIME24 | 57.1 → 60.1 vs GRPO | [2603.10535](https://arxiv.org/html/2603.10535) |
| Acoer | Qwen3-1.7B | −62% MATH500, −33% AIME25 | MATH500 held at 88.4 | [2606.22716](https://arxiv.org/html/2606.22716) |
| Kimi k1.5 long2short RL | k1.5 | 3,272 tokens avg on AIME24 | 60.8 AIME24 | [2501.12599](https://arxiv.org/html/2501.12599) |
| DAPO soft overlong punishment | Qwen2.5-32B | n/a (acts only at 16–20k) | +3 AIME24 in the progressive ablation (38 → 41) | [2503.14476](https://arxiv.org/html/2503.14476) |

- **Counter-evidence (2026).** "On the Optimal Reasoning Length for RL-Trained Language Models" — Daisuke Nohara, Taishi Nakamura, Rio Yokota (Institute of Science Tokyo), arXiv:2602.09591 (v3 Jun 10 2026).
  - Compared GRPO sample-average, DAPO token-average, RLOO-LP, ALP, and DRPO (GFPO could not be reproduced).
  - Models: R1-Distill-Qwen-1.5B, Qwen3-1.7B, and Qwen3-4B-Base. Benchmarks: AIME24/25, AMC, MATH-500, HumanEval, and MBPP+. At least 576 GPU-hours per configuration.
  - "None show uniform accuracy improvements." Accuracy "rises with length, reaches a peak, and then plateaus or declines".
  - The optimal length is about 6–8K tokens for several math tasks, versus 12K+ unconstrained.
  - Proposed mechanism: "mode leakage". With longer outputs the mode becomes more correct, but individual samples scatter further from it. — [arXiv:2602.09591](https://arxiv.org/html/2602.09591v3)
- DLER's view is that different penalty shapes move along the same efficiency frontier, and that the optimizer setup (not the penalty) moves the frontier. — [arXiv:2510.15110](https://arxiv.org/html/2510.15110)

### Inferences
- Reported accuracy gains are confounded. Many baselines are R1-Distill models evaluated with tight context budgets, where the base model often truncates and fails, so shortening the output recovers accuracy mechanically (for example, the ShorterBetter 7B AIME baseline at 36.7%). Gains also come together with other optimizer changes (DLER's clip-higher and dynamic sampling; GR³ compared against plain GRPO).
- The most defensible accuracy-improvement claim is difficulty-aware reallocation (ALP, Laser-DE). These spend fewer tokens on easy prompts and the same or more on hard ones, which raises hard-set accuracy. Uniform penalties mostly trade accuracy for tokens.
- For a project that bans generation caps: reward-based length control is the sanctioned alternative. The methods that leave the solvable tail intact are those that do not penalize length on hard or unsolved prompts:
  - ALP: the penalty scales with solve rate, though the 1/K floor still charges unsolved prompts.
  - GRPO-λ: the penalty switches off when group accuracy is low.
  - Laser-DE: wrong answers get a length bonus.
  - Correct-only multiplicative schemes (GR³, Acoer): wrong or unsolved samples get no length term.

  Truncation-reward methods (ThinkPrune, DLER) behave like hard caps on the reward side: long correct solutions get zero reward, so they share the censoring bias that the project's CLAUDE.md warns about.

### Gaps
- Few papers test on models above 32B or on non-math verifiable domains. I found no 2025–2026 study of length rewards on code or formal-verification-style tasks beyond HumanEval/MBPP+ and LiveCodeBench.
- The cited accuracy numbers are mostly single-seed as reported. I did not find variance or seed counts for most of them.
