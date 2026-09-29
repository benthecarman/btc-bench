# btc-verl runtime image

The XiaomiMiMo verl fork (commit a2ad9f6) on top of
`vllm/vllm-openai:v0.27.1`. It keeps the base image's torch 2.13.0+cu130
and vllm 0.27.1. transformers is pinned to 5.10.4 because verl's
Qwen3.5 FSDP patches fail on the base image's 5.15.0. The Dockerfile
header explains why.

## Build

```sh
docker build -t btc-verl:dev rl/docker
```

Optional build args: `CAUSAL_CONV1D_ARCHS` (default `"12.0"`),
`MAX_JOBS` (default 8), `TRANSFORMERS_VERSION`, `VERL_COMMIT`.

On the DGX Spark (arm64, GB10 = sm_121) flash-attn has no prebuilt
wheel and compiles from source (about 20 minutes):

```sh
docker build --build-arg TARGETARCH=arm64 --build-arg CAUSAL_CONV1D_ARCHS="12.1" \
  --build-arg FLASH_ATTN_CUDA_ARCHS="120" --build-arg MAX_JOBS=8 -t btc-verl:dev rl/docker
```

`patches/` is applied to the verl checkout. It makes the `FusedMoE`
import optional (vLLM 0.27 removed the name), makes sync-mode training
request its last batch (without it the final step waits forever), and
keeps LoRA adapters in fp32 on a bf16 base under FSDP2 (verl cast
them to bf16, where Adam steps near their precision round away).

## Run (GPU 1 only)

```sh
docker run --rm -it \
  --device nvidia.com/gpu=GPU-0d5ebfb4-861e-f1f8-d6be-1c4804c8ae76 \
  --shm-size=16g \
  -e HF_HOME=/hf \
  -v /mnt/llm-models/huggingface:/hf \
  -v "$PWD":/workspace/btc-bench \
  btc-verl:dev bash
```

The smallest Qwen3.5 checkpoint is already in the cache:
`Qwen/Qwen3.5-0.8B` (revision `2fc06364715b967f1860aea9cf38778875588b17`).

## Qwen3.5 notes

- verl's packed (remove-padding) Qwen3.5 path needs
  flash-linear-attention. Without it, the linear-attention state
  leaks across packed sequences, and no error is raised.
- causal-conv1d is only a speed-up. It is compiled for sm_120 by
  default.
- flash-attn 2.8.3 comes from a community prebuilt wheel on amd64
  (mjun0812/flash-attention-prebuild-wheels), pinned by sha256.
  It includes sm_80/90/100/120 kernels.
