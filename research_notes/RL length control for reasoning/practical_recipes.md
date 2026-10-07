# Practical recipes for response-length control in open RL pipelines (2025-2026)

Scope: what open frameworks implement for length control (exact knobs and values) and what open reasoning-model reports used, aimed at a LoRA GRPO run on a ~27B reasoning model with binary verifiable rewards, responses of tens of thousands of tokens, and no hard generation cap.

Research date: 2026-10-07. All fetched pages were read through a summarizing fetch tool. Values quoted below are what that tool returned from the primary page. Treat any value marked "(not re-verified)" with care.

---

## 1. Framework implementations: exact knobs and defaults

### Takeaway
Every major framework ships the same small toolkit. (a) DAPO "soft overlong punishment": a linear penalty that starts at `L_max - L_cache` and reaches `-penalty_factor` at `L_max`. (b) Masking or zero-rewarding of truncated (no-EOS) samples. (c) A choice of loss aggregation, where token-level ("token-mean"/"dapo") is the common default. No framework found ships a correctness-conditional length penalty as a first-class knob, except TRL's cosine-scaled reward. All overlong mechanisms are defined relative to a max length, so a run with no cap must pick a soft budget that triggers the penalty but does not stop generation.

### Cited Findings

**verl (volcengine / verl-project)**
- The DAPO recipe's overlong buffer has the keys `enable`, `len`, `penalty_factor` (plus `log`). The docs say "the penalty increases linearly from `0` to `overlong_buffer.penalty_factor` when the length of the output exceeds the `max_response_length - overlong_buffer.len` by `0` to `overlong_buffer.len` tokens." The example config uses `max_response_length: 20480`, `len` 4096, and `penalty_factor` 1.0. — [verl docs, "DAPO" recipe page](https://verl.readthedocs.io/en/latest/algo/dapo.html) (accessed 2026-10-07)
- In the code (`verl/workers/reward_manager/dapo.py`): `expected_len = self.max_resp_len - overlong_buffer_len`, `exceed_len = valid_response_length - expected_len`, `overlong_reward = min(-exceed_len / overlong_buffer_len * overlong_penalty_factor, 0)`, then `reward += overlong_reward`. The penalty is added after the base score for **all** responses, correct or not. It is capped at 0, so it never rewards brevity. When `log` is on, it logs `overlong_reward` and an `overlong` boolean. — [verl `reward_manager/dapo.py`](https://github.com/volcengine/verl/blob/main/verl/workers/reward_manager/dapo.py) (main, accessed 2026-10-07)
- `loss_agg_mode` values listed in the DAPO docs: `"token-mean"` (default), `"seq-mean-token-sum"`, and `"seq-mean-token-mean"`. Clip-higher uses `clip_ratio_low` (example 0.2) and `clip_ratio_high` (example 0.28). Dynamic sampling uses `filter_groups.enable: True`, `metric` (e.g. `"acc"`), and `max_num_gen_batches` (non-positive means unlimited). The recipe scripts are in `recipe/dapo/` (e.g. `run_dapo_qwen2.5_32b.sh`) in the [verl-project/verl](https://github.com/verl-project/verl) repo. — [verl DAPO docs](https://verl.readthedocs.io/en/latest/algo/dapo.html)
- `algorithm.norm_adv_by_std_in_grpo` defaults to `True` ("Whether to normalize advantages by std (specific to GRPO)"). The `filter_groups` block has `enable`, `metric`, `max_num_gen_batches`, and `max_inflight_gen_batches`. — [verl `ppo_trainer.yaml`](https://raw.githubusercontent.com/volcengine/verl/main/verl/trainer/config/ppo_trainer.yaml) (main, accessed 2026-10-07)
- `use_kl_loss` defaults to False. `kl_loss_coef` defaults to 0.001. `entropy_coeff` "default value is changed to 0.0 since v0.3.x". `reward_model.reward_manager` defaults to `naive` (`prime` for parallel verification; the DAPO recipe uses the `dapo` manager). — [verl config docs](https://verl.readthedocs.io/en/latest/examples/config.html)

**TRL GRPOTrainer (Hugging Face, `main` docs)**
- `loss_type` defaults to **`"dapo"`** (token-level normalization, `1/Σ|o_i|`). The other options are `"grpo"` (per-sequence `1/|o_i|`, which has a length bias), `"dr_grpo"` (divide by the constant `L·G`, "recommended: max completion length"), `"sapo"`, `"cispo"`, and `"vespo"`. — [TRL GRPO Trainer docs](https://huggingface.co/docs/trl/main/en/grpo_trainer) (accessed 2026-10-07)
- `mask_truncated_completions` defaults to `False` ("exclude tokens from truncated/clipped completions from the loss"). `scale_rewards` defaults to `"group"`. `"batch"` means "Mean at group level, std at batch level", citing the Lite PPO paper, arXiv 2508.08221. `False` disables std scaling and cites the Dr. GRPO finding that std scaling "may cause a question-level difficulty bias". `max_completion_length` defaults to 512. `beta` defaults to 0.0. `epsilon` defaults to 0.2, and `epsilon_high` defaults to `epsilon` (DAPO recommends 0.28). `importance_sampling_level` is `"token"` or `"sequence"` (GSPO). — [TRL GRPO Trainer docs](https://huggingface.co/docs/trl/main/en/grpo_trainer)
- `trl.rewards.get_soft_overlong_punishment(max_completion_len, soft_punish_cache)` implements DAPO Eq. 13. The reward is 0 if `|y| ≤ L_max − L_cache`, `((L_max − L_cache) − |y|)/L_cache` in the buffer, and −1 beyond `L_max`. Docs example: `max_completion_len=100, soft_punish_cache=20`, and a 90-token completion gets −0.5. — [TRL Reward Functions docs](https://huggingface.co/docs/trl/main/en/rewards)
- `trl.rewards.get_cosine_scaled_reward(max_len, min_value_wrong=-1.0, max_value_wrong=-0.5, min_value_correct=0.5, max_value_correct=1.0)` is a correctness-conditional cosine schedule from "Demystifying Long CoT" (arXiv 2502.03373), App. C.1. Correct answers get more reward when shorter. For wrong answers the bounds are swapped, so "a longer wrong completion is penalized less, preserving exploration". `get_repetition_penalty_reward(ngram_size=3, max_penalty=-1.0)` is recommended against "degenerate, repetitive text (a common failure mode and reward-hacking strategy when length- or format-shaping rewards are used)". — [TRL Reward Functions docs](https://huggingface.co/docs/trl/main/en/rewards)

**NeMo-RL (NVIDIA), ProRLv2 guide**
- `stop_properly_penalty_coef` is in [0.0, 1.0] and "scales the reward for truncated samples". 0.0 gives truncated (no-EOS) generations zero reward; 1.0 keeps the original reward. The example config uses `0.0`. To use DAPO overlong shaping instead, set `stop_properly_penalty_coef` to null and set `overlong_buffer_length: 4096`, `overlong_buffer_penalty: 1.0`, and `max_response_length: 20480`. — [NeMo RL docs, "An In-Depth Walkthrough of ProRLv2 in NeMo RL"](https://docs.nvidia.com/nemo/rl/latest/guides/prorlv2.html) (latest; also versioned 0.6.0/0.7.0)
- `overlong_filtering` is an optional bool that "excludes sequences that hit max length without EOS from loss computation". — [NeMo RL GRPO API docs](https://docs.nvidia.com/nemo/rl/latest/apidocs/nemo_rl/nemo_rl.algorithms.grpo.html) (via search snippet)
- Other ProRLv2 knobs: `token_level_loss: true`, `ratio_clip_min: 0.2`, `ratio_clip_max: 0.27`, `grpo.use_dynamic_sampling: true` (with `batch_multiplier` and `dynamic_sampling_max_gen_batches`), `grpo.adv_estimator.name: "reinforce_plus_plus"` with `minus_baseline: true`, `normalize_rewards: true`, and `use_leave_one_out_baseline: false`. MoE IS-correction: `truncated_importance_sampling_type: "icepop"` (ratio 5.0 / min 0.5) or `"seq-mask-tis"` (1.002 / 0.999, recommended). — [NeMo RL ProRLv2 guide](https://docs.nvidia.com/nemo/rl/latest/guides/prorlv2.html)

### Inferences
- verl's DAPO penalty with `penalty_factor 1.0` and a {0,1} reward means a correct answer at `L_max` scores 0, the same as a wrong answer. For a binary-reward run that wants length to be a tiebreaker rather than a veto, a factor of about 0.1-0.5 is more in line with Magistral's 0.1 (see §2).
- No cap: the verl/TRL/NeMo penalties need only the measured response length, so you can set the "max length" in the penalty as a soft budget and leave the inference request uncapped. The penalty saturates at `-penalty_factor` beyond the budget (TRL clamps to −1; verl's code as quoted is `min(-exceed/len*factor, 0)`, which does **not** clamp at −factor for lengths beyond `max_resp_len`, because verl normally truncates there). If you reuse verl's formula without truncation, add your own floor.
- The "mask truncated" knobs (`mask_truncated_completions`, `overlong_filtering`) only mean something with a hard cap. With no cap, nothing is truncated, and these knobs do nothing unless the server has its own context limit.
- Interaction with dynamic sampling (from verl's design, not verified in code this session): with `filter_groups.metric: acc`, groups where every sample is correct or every sample is wrong are dropped by accuracy variance. So a length term inside an all-correct group never yields a gradient. Use `metric: score` (or similar) if you want length-only signal from solved prompts.

### Gaps
- OpenRLHF, slime, AReaL, and ROLL: I did not fetch their length-control knobs because of the tool-call budget. I have no verified parameter names for them.
- I did not confirm the full verl `loss_agg_mode` list in current main (for example, whether `seq-mean-token-sum-norm` exists). The DAPO doc lists three values.
- I did not fetch the verl DAPO shell script (the raw URL returned 404, probably because the recipe moved to `verl-project/verl` or a separate recipe repo).

---

## 2. What open training reports used, with numbers

### Takeaway
Most large open reasoning RL runs used **no explicit length penalty** during capability RL. They controlled length with a staged max-length curriculum, token-level loss, and a choice about truncated samples. Explicit penalties appear when a lab wants efficiency: Kimi k1.5 (group-relative, correct-only positive part, warmed up), Magistral (soft overlong, max −0.1 against a 1.0 reward), and Phi-4-reasoning (cosine length-aware reward, correctness-conditional).

### Cited Findings
- **Kimi k1.5** (arXiv 2501.12599, v4 dated 2025-06-03): `λ = 0.5 − (len(i) − min_len)/(max_len − min_len)`, where min/max are the shortest and longest of the k samples for that prompt. The length reward is `λ` if correct and `min(0, λ)` if incorrect. Long wrong answers are penalized, but short wrong answers get no bonus. On warmup: the length penalty "may slow down training during initial phases", so they ran standard policy optimization without it, then "a constant length penalty for the rest". The fetched text gave no weight value. Long2short methods: model merging (weight averaging), shortest rejection sampling (n=8, keep the shortest correct), DPO (shortest correct as positive), and a separate long2short RL phase with a reduced max rollout length. — [Kimi k1.5 paper](https://arxiv.org/html/2501.12599)
- **Magistral** (Mistral, arXiv 2506.10910, 2025-06-12): soft length penalty `R_length = 0` if `|y| ≤ l_max − l_cache`, `−0.1·(|y| − l_max + l_cache)/l_cache` in the buffer, and `−0.1` beyond `l_max`. The reward is 0 for a format violation, 0.1 for correct format, plus 0.9 for correctness, plus 0.1 for language consistency. So the length penalty is at most 10% of the full reward. `l_max − l_cache` was raised in stages: 16k→24k, then 24k→32k, while batch sizes dropped 8k→4k→2k. Loss was summed over all tokens of all generations and divided by total length. Advantages were normalized at minibatch level. They "remove the KL penalty entirely". `ε_high` was 0.26-0.28 for math (0.3 for smaller models). — [Magistral paper](https://arxiv.org/html/2506.10910)
- **Skywork-OR1** (arXiv 2505.22312, 2025-05-29): context grew in stages, 8K→16K→32K, which was cheaper than fixed long context. No length penalty. They used token-level loss "by removing the length normalization term 1/|y_ij|". They tried advantage masking for truncated responses and dropped it: "assigning negative advantages to truncated samples not only improves token efficiency but also preserves the model's scaling ability." No KL. Adaptive entropy control with target entropy 0.2. Group size 16. Temperature τ=1.0. — [Skywork-OR1 report](https://arxiv.org/html/2505.22312)
- **POLARIS** (HKU NLP blog, 2025): max training length was 52K for Qwen3-4B, with shorter windows in early stages. At inference they used YaRN with factor 1.5 to go past the training length. The blog describes no length penalty or overlong filtering. Temperature per stage: 7B 0.7→1.0→1.1, 4B 1.4→1.45→1.5. Prompts with accuracy > 0.9 are dropped after each stage. Rollout n=8. The truncation `clip_ratio` "remained below 10%". — [POLARIS blog](https://hkunlp.github.io/blog/2025/Polaris/)
- **AceReason-Nemotron** (NVIDIA, arXiv 2505.16400, dated 2025-06-05 by fetch): response-length curriculum 8K→16K→24K→32K. Token-level GRPO loss, β=0, strictly on-policy ("exactly one gradient update after model generation"), 8 rollouts at 8K then 16. No explicit length penalty. The fetch reported temperature 0.6 for early stages (not re-verified; treat with caution). — [AceReason-Nemotron paper](https://arxiv.org/html/2505.16400)
- **MiniMax-M1** (arXiv 2506.13585, 2025-06-16): generation length grew 40K→48K→56K→64K→72K→80K. They moved to the next stage on two signals: "convergence of perplexity on the generated sequences and whether the 99th percentile of the output lengths is approaching the current context window limit." Long generations degraded into garbled text, which they traced to large negative gradients in later segments. Early-stop heuristic: "generation is halted if 3,000 consecutive tokens each have a probability above 0.99". The fetched text mentions no length penalty. The GenRM "preferred longer outputs", which they handled by monitoring and recalibrating the reward model. The algorithm is CISPO (clips IS weights instead of dropping tokens). — [MiniMax-M1 paper](https://arxiv.org/html/2506.13585)
- **Phi-4-reasoning** (Microsoft, arXiv 2504.21318, 2025-04-30): length-aware accuracy reward with `L_max = 31,744`, `L_pos_control = 25,600` (correct answers are not penalized below this), and `L_neg_control = 3,702` (wrong answers are not penalized above this). Correct answers get R in [0.5, 1.0] and wrong answers R in [−1.0, −0.5], on a cosine schedule. Incomplete responses get −0.5 and invalid think blocks −1.0. A repetition penalty is mixed in with `w_acc = 8/13` and `w_rep = 1/13`. G=8, 32k max, β=0.001, lr 5×10⁻⁸ (as returned by the fetch, not re-verified), 90 RL steps for the chosen checkpoint. — [Phi-4-reasoning report](https://arxiv.org/html/2504.21318)
- **GFPO** (Microsoft, arXiv 2508.09726; ICLR 2026 poster): sample a larger group, keep the top-k by length or reward-per-token, and train only on those. On Phi-4-reasoning this cut GRPO length inflation by 46-71%, or 71-85% when filtering by token efficiency, "while maintaining accuracy". — [GFPO paper](https://arxiv.org/pdf/2508.09726); [MSR page](https://www.microsoft.com/en-us/research/publication/sample-more-to-think-less-group-filtered-policy-optimization-for-concise-reasoning/)
- **Qwen3** (arXiv 2505.09388, 2025-05-14): reasoning RL used 3,995 query-verifier pairs with GRPO. Keeping entropy "to increase steadily or remain stable" was "crucial". AIME'24 went from 70.1 to 85.1 over 170 steps (235B-A22B). The fetched text did not state a length penalty or a max generation length. The thinking budget is enforced at inference: at the threshold they insert "Considering the limited time by the user, I have to give the solution based on the thinking directly now.\n</think>.\n\n". This ability "is not explicitly trained but emerges naturally" from Thinking Mode Fusion. — [Qwen3 Technical Report](https://arxiv.org/html/2505.09388)
- **Optimal length** (Nohara, Nakamura, Yokota, arXiv 2602.09591, Feb 2026, revised June 2026): "accuracy is non-monotonic in output length, peaking at an intermediate value". Mode accuracy keeps improving with length while per-sample accuracy plateaus or falls. — [arXiv 2602.09591](https://arxiv.org/abs/2602.09591) (abstract only; the methods compared were not visible)

### Inferences
- The common pattern for capability RL (Skywork-OR1, AceReason, POLARIS, MiniMax-M1, Magistral) is a staged length budget plus token-level loss, with at most a small soft overlong penalty. Explicit "shorter is better" pressure (Kimi λ, Phi-4 cosine, GFPO) is used when efficiency is a goal, and Kimi warms it up only after capability training.
- MiniMax-M1's stage-advance rule (p99 length nearing the budget) carries over directly to a soft-budget, no-cap setup. Raise the soft-penalty threshold when p99 of correct responses approaches it.

### Gaps
- Not covered because of the tool budget: DeepSeek-R1/V3.x, Kimi K2, GLM-4.5, Seed-Thinking, Llama-Nemotron, OpenThoughts/OpenR1, Klear, Archer, JustRL, and ProRL v1. I have no verified length-control numbers for these.
- I could not confirm whether AceReason-Nemotron ablated overlong filtering per stage. The fetch did not find it.

---

## 3. Group-relative advantages and length penalties

### Takeaway
Every recipe found adds the length term to the scalar reward **before** the group baseline and normalization are computed. A small term (0.1) can therefore dominate after std normalization in groups with uniform correctness. Correctness-conditional designs (Kimi `min(0, λ)` for wrong answers, Phi-4/TRL cosine with swapped bounds for wrong answers) exist to keep short wrong answers from being rewarded, which is the guard against collapse to short wrong answers.

### Cited Findings
- verl DAPO: `reward += overlong_reward` is added to the score before advantage estimation and applies to all responses. — [verl dapo.py](https://github.com/volcengine/verl/blob/main/verl/workers/reward_manager/dapo.py)
- Kimi k1.5 gives the length reward group-relatively (min/max within the k samples). Wrong answers get only `min(0, λ)`, so a short wrong answer gets no positive bonus. — [Kimi k1.5](https://arxiv.org/html/2501.12599)
- In TRL's cosine reward, wrong-answer bounds are swapped so that "a longer wrong completion is penalized less, preserving exploration". — [TRL rewards docs](https://huggingface.co/docs/trl/main/en/rewards)
- Phi-4-reasoning: correct answers keep at least +0.5 and wrong answers stay at or below −0.5. With `L_neg_control = 3,702`, short wrong answers get the harshest penalty. — [Phi-4-reasoning](https://arxiv.org/html/2504.21318)
- Magistral caps the length penalty at 0.1 against a correctness reward of 0.9 (+0.1 format). — [Magistral](https://arxiv.org/html/2506.10910)
- TRL `scale_rewards="batch"` (group mean, batch std) "enables more robust reward shaping" (Lite PPO, arXiv 2508.08221). `scale_rewards=False` removes std scaling, which Dr. GRPO links to difficulty bias. — [TRL GRPO docs](https://huggingface.co/docs/trl/main/en/grpo_trainer)
- Warmup: Kimi used no penalty at first, then a constant one. Magistral and Skywork-OR1 raised the budget in stages. — [Kimi k1.5](https://arxiv.org/html/2501.12599); [Magistral](https://arxiv.org/html/2506.10910); [Skywork-OR1](https://arxiv.org/html/2505.22312)
- Skywork-OR1: giving negative advantage to truncated samples (instead of masking them) improved token efficiency and kept the model's ability to scale length. — [Skywork-OR1](https://arxiv.org/html/2505.22312)

### Inferences
- With per-group std normalization, if all G samples are correct, the correctness variance is 0 and the advantages are driven only by the length term, rescaled to unit variance. A 0.1-magnitude penalty then produces gradients as large as correctness does in mixed groups. Choose one of these deliberately: drop such groups (verl `filter_groups` on acc), use batch-level std (TRL `scale_rewards="batch"`), or disable std scaling.
- For binary rewards the safe shapes are: (a) penalize only over a soft budget (DAPO/Magistral) with max magnitude ≤ about 0.1-0.5 of the correct reward, or (b) a correct-only brevity bonus with no positive term for short wrong answers (Kimi/Phi-4). Shape (a) does not push toward short answers below the budget, which lowers collapse risk. The guards in the reports are: warmup with no penalty (Kimi), a correctness floor where correct ≥ 0.5 always beats wrong ≤ −0.5 (Phi-4), and a repetition penalty (Phi-4, TRL).

### Gaps
- I found no report with a controlled comparison of adding the length term before vs after the group baseline. All implementations seen add it to the reward before the baseline.

---

## 4. LoRA-specific guidance

### Takeaway
Thinking Machines found LoRA matches full fine-tuning for policy-gradient RL even at rank 1, provided LoRA covers the MLP layers and the learning rate is about 10x the full-FT rate. They give no length-specific LoRA guidance.

### Cited Findings
- "LoRA fully matches the learning performance of FullFT when running policy gradient algorithms for reinforcement learning, even with ranks as low as 1." The policy gradient gives "O(1) bits per episode". — [Thinking Machines, "LoRA Without Regret" (2025-09-29)](https://thinkingmachines.ai/blog/lora/)
- "The optimal learning rate for FullFT is lower by a factor of 10 than for high-rank LoRAs." "Attention-only LoRA significantly underperforms MLP-only LoRA". "LoRA is less tolerant of large batch sizes than FullFT". Experiments used MATH, GSM8K, and DeepMath. — [LoRA Without Regret](https://thinkingmachines.ai/blog/lora/)

### Inferences
- Because LoRA is less tolerant of large batches, the long-response, small-batch regime is not a disadvantage for LoRA. The length-control choices above are reward and loss-aggregation choices and work the same with LoRA.

### Gaps
- I found no LoRA-RL report that studies response-length dynamics specifically.

---

## 5. Evaluation and monitoring

### Takeaway
Track the length distribution (p99, not just the mean) against the budget, the truncation/overlong rate, perplexity or entropy, and the share of length-shaped reward. Advance budgets when p99 nears the limit. Watch for repetition and garbling at long lengths.

### Cited Findings
- MiniMax-M1 advanced the budget on perplexity convergence plus "whether the 99th percentile of the output lengths is approaching the current context window limit". It stopped generation after 3,000 consecutive tokens with p > 0.99. — [MiniMax-M1](https://arxiv.org/html/2506.13585)
- POLARIS tracked `clip_ratio` (share of samples at max length) and kept it below 10%. It reported a sharp accuracy drop past the pre-training length without extrapolation (26% accuracy for responses > 32K). — [POLARIS blog](https://hkunlp.github.io/blog/2025/Polaris/)
- verl logs `overlong_reward` and `overlong` per sample when `overlong_buffer.log` is on. — [verl dapo.py](https://github.com/volcengine/verl/blob/main/verl/workers/reward_manager/dapo.py)
- Qwen3 and Skywork-OR1 both treat entropy as a key stability signal (Qwen3: keep it stable or rising; Skywork: adaptive target 0.2). — [Qwen3](https://arxiv.org/html/2505.09388); [Skywork-OR1](https://arxiv.org/html/2505.22312)
- Accuracy can be non-monotonic in length, so length alone is not a quality proxy. — [arXiv 2602.09591](https://arxiv.org/abs/2602.09591)

### Inferences
- Log per step: mean, p50, and p99 length split by correct vs incorrect; the share of samples past the soft budget; the mean length-term contribution vs the mean correctness reward; entropy; and the share of groups with zero accuracy variance. If correct and incorrect lengths both rise while accuracy is flat, that is length inflation (GFPO's target). If incorrect-response length collapses toward the minimum, that is the short-wrong-answer failure mode.

### Gaps
- I found no published checkpoint-eval cadence for these recipes in the sources fetched.
