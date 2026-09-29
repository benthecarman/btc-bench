#!/usr/bin/env bash
# Launch btc-bench GRPO (rl/config/btc_bench.yaml) inside the btc-verl image.
#
# Needs a reachable `btc-bench reward-serve` (BTCBENCH_REWARD_URL, default
# http://127.0.0.1:9900/reward). Sizes come from the environment; the
# defaults are a single-GPU smoke run. Extra arguments are Hydra overrides.
#
#   MODEL_PATH=/models/Qwen3.5-2B DATA_DIR=/data/rl-pool-1/verl rl/run_train.sh
#
# Rollout budget: a rollout that fills RESPONSE_LENGTH scores 0, like an
# unanswered task. That is a training setting; the benchmark stays uncapped.
set -euo pipefail

RL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
: "${MODEL_PATH:?MODEL_PATH must point to the policy checkpoint}"
: "${DATA_DIR:?DATA_DIR must hold train.parquet and val.parquet from rl/build_parquet.py}"

EXP_NAME="${EXP_NAME:-btc-bench-smoke}"
RUN_DIR="${RUN_DIR:-${RL_DIR}/../runs/rl/${EXP_NAME}}"
PROMPT_LENGTH="${PROMPT_LENGTH:-4096}"
RESPONSE_LENGTH="${RESPONSE_LENGTH:-65536}"
TRAIN_BATCH_SIZE="${TRAIN_BATCH_SIZE:-32}"
N="${N:-8}"
NGPUS="${NGPUS:-1}"
ROLLOUT_TP="${ROLLOUT_TP:-1}"
ROLLOUT_GPU_MEM_UTIL="${ROLLOUT_GPU_MEM_UTIL:-0.5}"
ROLLOUT_MAX_NUM_SEQS="${ROLLOUT_MAX_NUM_SEQS:-64}"
LORA_RANK="${LORA_RANK:-32}"
LORA_ALPHA="${LORA_ALPHA:-32}"
LR="${LR:-1e-4}"
# Match the model's eval sampling in models.toml (27B: 1.0, 9B: 0.6).
TEMPERATURE="${TEMPERATURE:?set TEMPERATURE to the eval temperature in models.toml}"
TOTAL_EPOCHS="${TOTAL_EPOCHS:-1}"
SAVE_FREQ="${SAVE_FREQ:-10}"
TEST_FREQ="${TEST_FREQ:-10}"
export BTCBENCH_REWARD_URL="${BTCBENCH_REWARD_URL:-http://127.0.0.1:9900/reward}"
export TENSORBOARD_DIR="${RUN_DIR}/tensorboard"
export PYTHONPATH="${RL_DIR}:${PYTHONPATH:-}"

if ! curl -sf -m 5 "${BTCBENCH_REWARD_URL%/reward}/health" >/dev/null; then
  echo "reward server not reachable at ${BTCBENCH_REWARD_URL}; start btc-bench reward-serve first" >&2
  exit 2
fi
MAXLEN=$((PROMPT_LENGTH + RESPONSE_LENGTH))
mkdir -p "${RUN_DIR}"

exec python3 -m verl.trainer.main_ppo \
  --config-path="${RL_DIR}/config" --config-name=btc_bench \
  actor_rollout_ref.model.path="${MODEL_PATH}" \
  actor_rollout_ref.model.lora_rank="${LORA_RANK}" \
  actor_rollout_ref.model.lora_alpha="${LORA_ALPHA}" \
  actor_rollout_ref.actor.optim.lr="${LR}" \
  actor_rollout_ref.actor.ppo_mini_batch_size="${TRAIN_BATCH_SIZE}" \
  actor_rollout_ref.actor.ppo_max_token_len_per_gpu="${MAXLEN}" \
  actor_rollout_ref.rollout.n="${N}" \
  actor_rollout_ref.rollout.temperature="${TEMPERATURE}" \
  actor_rollout_ref.rollout.val_kwargs.temperature="${TEMPERATURE}" \
  actor_rollout_ref.rollout.prompt_length="${PROMPT_LENGTH}" \
  actor_rollout_ref.rollout.response_length="${RESPONSE_LENGTH}" \
  actor_rollout_ref.rollout.max_model_len="${MAXLEN}" \
  actor_rollout_ref.rollout.tensor_model_parallel_size="${ROLLOUT_TP}" \
  actor_rollout_ref.rollout.gpu_memory_utilization="${ROLLOUT_GPU_MEM_UTIL}" \
  actor_rollout_ref.rollout.max_num_seqs="${ROLLOUT_MAX_NUM_SEQS}" \
  actor_rollout_ref.rollout.agent.agent_loop_config_path="${RL_DIR}/config/agent_loop.yaml" \
  data.train_files="['${DATA_DIR}/train.parquet']" \
  data.val_files="['${DATA_DIR}/val.parquet']" \
  data.train_batch_size="${TRAIN_BATCH_SIZE}" \
  data.max_prompt_length="${PROMPT_LENGTH}" \
  data.max_response_length="${RESPONSE_LENGTH}" \
  trainer.nnodes=1 \
  trainer.n_gpus_per_node="${NGPUS}" \
  trainer.total_epochs="${TOTAL_EPOCHS}" \
  trainer.save_freq="${SAVE_FREQ}" \
  trainer.test_freq="${TEST_FREQ}" \
  trainer.project_name=btc-bench \
  trainer.experiment_name="${EXP_NAME}" \
  trainer.default_local_dir="${RUN_DIR}/ckpt" \
  trainer.rollout_data_dir="${RUN_DIR}/rollout" \
  trainer.validation_data_dir="${RUN_DIR}/validation" \
  hydra.run.dir="${RUN_DIR}/hydra" \
  +ray_kwargs.ray_init.runtime_env.env_vars.PYTHONPATH="${PYTHONPATH}" \
  +ray_kwargs.ray_init.runtime_env.env_vars.BTCBENCH_REWARD_URL="${BTCBENCH_REWARD_URL}" \
  +ray_kwargs.ray_init.runtime_env.env_vars.TENSORBOARD_DIR="${TENSORBOARD_DIR}" \
  "$@"
