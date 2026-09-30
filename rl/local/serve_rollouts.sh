#!/usr/bin/env bash
# vLLM rollout server for local 27B RL on the RTX 5090: the NVFP4
# checkpoint the benchmark serves, with runtime LoRA loading so each
# training step's adapter can be swapped in. Same serving flags as the
# benchmark's local-model, minus MTP speculative decoding (kept off so
# the returned sampling log-probs are the target model's own), plus
# processed log-probs (after temperature/top-k/top-p, as sampled).
set -euo pipefail
ADAPTERS="${ADAPTERS:?directory holding step-N adapters}"
H=/mnt/llm-models/huggingface/hub/models--nvidia--Qwen3.8-27B-NVFP4
docker rm -f vllm-rl >/dev/null 2>&1 || true
exec docker run --name vllm-rl --rm --shm-size=8g -e HF_HUB_OFFLINE=1 -e PYTORCH_CUDA_ALLOC_CONF=expandable_segments:True \
  -e VLLM_USE_FLASHINFER_SAMPLER=1 -e VLLM_ALLOW_RUNTIME_LORA_UPDATING=true \
  -p 0.0.0.0:18010:8000 --device nvidia.com/gpu=GPU-c9619b6c-484a-b172-b6c7-afa7d12273f6 \
  --mount type=bind,src=$H,dst=/model,readonly -v "$ADAPTERS":/adapters:ro --entrypoint vllm \
  vllm/vllm-openai:v0.27.1@sha256:0a51ea5b4ae2dc5d81890e5173f54203d2a3ae0cfffe51b8fd2afd4391bfd967 \
  serve /model/snapshots/482ca0f3832238542f8f5295dde86b5f22711d80 --served-model-name qwen3.8:27b \
  --host 0.0.0.0 --port 8000 --dtype bfloat16 --generation-config auto --seed 0 --attention-backend flashinfer \
  --kv-cache-dtype fp8_e4m3 --mamba-cache-dtype float32 --max-num-seqs 4 --max-num-batched-tokens 8192 \
  --enable-chunked-prefill --enable-prefix-caching --gpu-memory-utilization 0.95 --max-model-len 131072 \
  --enable-lora --max-lora-rank 32 --max-loras 2 --logprobs-mode processed_logprobs \
  --enable-auto-tool-choice --tool-call-parser qwen3_coder --reasoning-parser qwen3
