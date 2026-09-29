# syntax=docker/dockerfile:1
#
# RL training runtime: XiaomiMiMo verl fork on top of the upstream vLLM
# image. torch and vllm come from the base image and are frozen: every
# pip/uv step runs against a constraints file generated from the base
# image's own package list, and the last step fails the build if any
# base package other than transformers moved.
#
# transformers is swapped from the base's 5.15.0 to 5.10.4 (verl pins
# <5.11; vllm 0.27.1 declares >=5.5.3). verl's Qwen3.5 FSDP patches
# (verl/models/transformers/qwen3_5.py) read
# Qwen3_5DecoderLayer.layer_type and GatedDeltaNet.causal_conv1d_fn /
# .chunk_gated_delta_rule; 5.15 renamed the first to block_type and
# dropped the per-module kernel attributes, so verl's actor forward
# raises AttributeError on 5.15. vLLM's own Qwen3.5 serving was checked
# on 5.10.4 and works.

FROM vllm/vllm-openai:v0.27.1@sha256:0a51ea5b4ae2dc5d81890e5173f54203d2a3ae0cfffe51b8fd2afd4391bfd967

ARG VERL_REPO=https://github.com/XiaomiMiMo/verl
ARG VERL_COMMIT=a2ad9f6160b03ff2d47e59832bfb6b289f37c917
ARG TRANSFORMERS_VERSION=5.10.4
ARG FLA_VERSION=0.5.2
ARG CAUSAL_CONV1D_URL=https://files.pythonhosted.org/packages/f4/d1/eba27735f31bd5527d39a3c7f693e6ded7b844cab275db719a15ad37b6cd/causal_conv1d-1.7.0.tar.gz
ARG CAUSAL_CONV1D_SHA256=3202758494eaa7b597ce1c282dfa188889506bfcb92cad3c407d26736bfcd32b
# No official flash-attn wheel exists for torch 2.13 + cu130. amd64 uses
# the community prebuilt from mjun0812/flash-attention-prebuild-wheels,
# pinned by sha256; arm64 (DGX Spark) has no prebuilt and compiles
# FLASH_ATTN_SOURCE_VERSION for FLASH_ATTN_CUDA_ARCHS.
ARG TARGETARCH
ARG FLASH_ATTN_SOURCE_VERSION=2.8.3
ARG FLASH_ATTN_CUDA_ARCHS="120"
ARG FLASH_ATTN_WHEEL=flash_attn-2.8.3+cu130torch2.13-cp312-cp312-linux_x86_64.whl
ARG FLASH_ATTN_URL=https://github.com/mjun0812/flash-attention-prebuild-wheels/releases/download/v0.9.47/flash_attn-2.8.3%2Bcu130torch2.13-cp312-cp312-linux_x86_64.whl
ARG FLASH_ATTN_SHA256=c65d762d24b7dd1ea8dbd9f5a98708fa8abbfd8f33f46bfc6123e76f1e174d3f
# causal-conv1d compiles from source. Default is the RTX 50xx arch;
# add more (space separated, e.g. "9.0 12.0") for other GPUs.
ARG CAUSAL_CONV1D_ARCHS="12.0"
ARG MAX_JOBS=8

ENV PIP_NO_CACHE_DIR=1 \
    PIP_BREAK_SYSTEM_PACKAGES=1 \
    UV_NO_CACHE=1 \
    UV_BREAK_SYSTEM_PACKAGES=1 \
    PYTHONUNBUFFERED=1

RUN apt-get update \
    && apt-get install -y --no-install-recommends git \
    && rm -rf /var/lib/apt/lists/*

# Freeze the base image's environment. Anything that would need to
# upgrade or downgrade a base package fails the build instead.
RUN mkdir -p /etc/btc-verl \
    && pip list --format=freeze > /etc/btc-verl/base-constraints.txt \
    && { cat /etc/uv-overrides.txt; grep -iE '^(packaging|fsspec)==' /etc/btc-verl/base-constraints.txt; } \
       > /etc/btc-verl/overrides.txt

# verl runtime deps (requirements.txt minus transformers, pre-commit),
# plus peft/ray/http clients/pandas/pyarrow and the GDN kernels.
# tensordict: verl wants <=0.10.0, TransferQueue 0.1.8 wants >=0.10.0.
# math-verify/latex2sympy2 use the antlr 4.9.3 extra so they share
# antlr4-python3-runtime with hydra-core/omegaconf.
# Two upper caps are overridden rather than downgrading base packages:
# pyvers 0.1.0 (a tensordict dep) caps packaging<26 (base: 26.3), and
# datasets 5.0.1 caps fsspec<=2026.6.0 (base: 2026.7.0). Without the
# fsspec override the resolver silently falls back to datasets 1.1.1.
RUN uv pip install --system -c /etc/btc-verl/base-constraints.txt \
        --override /etc/btc-verl/overrides.txt \
        accelerate \
        codetiming \
        "datasets>=5" \
        dill \
        hydra-core \
        liger-kernel \
        "numpy>=2.0.0" \
        pandas \
        peft \
        "pyarrow>=19.0.0" \
        pybind11 \
        pylatexenc \
        "ray[default]>=2.41.0" \
        "tensordict==0.10.0" \
        torchdata \
        wandb \
        "packaging>=20.0" \
        uvicorn \
        fastapi \
        aiohttp \
        httpx \
        "latex2sympy2_extended[antlr4_9_3]" \
        "math-verify[antlr4_9_3]" \
        tensorboard \
        "TransferQueue==0.1.8" \
        "flash-linear-attention==${FLA_VERSION}"

# All of transformers 5.10.4's deps are already satisfied by the base.
RUN uv pip install --system --no-deps "transformers==${TRANSFORMERS_VERSION}"

# flash-attn 2: verl's remove-padding path imports
# flash_attn.bert_padding, and the FSDP actor defaults to
# attn_implementation=flash_attention_2.
RUN if [ "${TARGETARCH:-amd64}" = "amd64" ]; then \
        curl -fsSL -o "/tmp/${FLASH_ATTN_WHEEL}" "${FLASH_ATTN_URL}" \
        && echo "${FLASH_ATTN_SHA256}  /tmp/${FLASH_ATTN_WHEEL}" | sha256sum -c - \
        && uv pip install --system --no-deps "/tmp/${FLASH_ATTN_WHEEL}" \
        && rm -f "/tmp/${FLASH_ATTN_WHEEL}"; \
    else \
        FLASH_ATTN_CUDA_ARCHS="${FLASH_ATTN_CUDA_ARCHS}" FLASH_ATTENTION_FORCE_BUILD=TRUE MAX_JOBS=${MAX_JOBS} \
        pip install --no-build-isolation --no-deps -c /etc/btc-verl/base-constraints.txt \
            "flash-attn==${FLASH_ATTN_SOURCE_VERSION}"; \
    fi

# flash-linear-attention (installed above) is required, not optional:
# without it the packed (remove-padding) GDN path silently mixes state
# across packed sequences. causal-conv1d is a speed-up only; verl has a
# per-sequence torch fallback for the conv when it is missing.
# setup.py hardcodes its gencode list; narrow it to CAUSAL_CONV1D_ARCHS.
RUN mkdir /tmp/cc && cd /tmp/cc \
    && curl -fsSL -o cc.tar.gz "${CAUSAL_CONV1D_URL}" \
    && echo "${CAUSAL_CONV1D_SHA256}  cc.tar.gz" | sha256sum -c - \
    && tar xzf cc.tar.gz && cd causal_conv1d-*/ \
    && python3 - "${CAUSAL_CONV1D_ARCHS}" <<'EOF'
import sys
archs = [a.replace(".", "") for a in sys.argv[1].split()]
src = open("setup.py").read()
anchor = "    # HACK: The compiler flag -D_GLIBCXX_USE_CXX11_ABI"
assert src.count(anchor) == 1, "causal-conv1d setup.py layout changed"
patch = (
    "    if not HIP_BUILD:\n"
    f"        cc_flag = [f for a in {archs!r} for f in ('-gencode', f'arch=compute_{{a}},code=sm_{{a}}')]\n"
    "        print('causal-conv1d gencode:', cc_flag)\n\n"
)
open("setup.py", "w").write(src.replace(anchor, patch + anchor))
EOF

RUN cd /tmp/cc/causal_conv1d-*/ \
    && CAUSAL_CONV1D_FORCE_BUILD=TRUE MAX_JOBS=${MAX_JOBS} \
       pip install --no-build-isolation --no-deps -c /etc/btc-verl/base-constraints.txt . \
    && cd / && rm -rf /tmp/cc

# verl fork at a pinned commit, editable so examples/recipes stay
# next to the package.
# patches/: vLLM 0.27 dropped the FusedMoE name that verl imports at
# rollout startup; sync-mode training never requested its last batch.
COPY patches /tmp/verl-patches
RUN git clone --filter=blob:none "${VERL_REPO}" /opt/verl \
    && git -C /opt/verl checkout --detach "${VERL_COMMIT}" \
    && for p in /tmp/verl-patches/*.patch; do git -C /opt/verl apply "$p"; done \
    && uv pip install --system --no-deps -e /opt/verl

# Fail the build if a base package other than transformers moved.
RUN pip list --format=freeze > /tmp/after.txt \
    && python3 - <<'EOF'
base = dict(l.strip().split("==", 1) for l in open("/etc/btc-verl/base-constraints.txt") if "==" in l)
after = dict(l.strip().split("==", 1) for l in open("/tmp/after.txt") if "==" in l)
moved = {k: (v, after.get(k)) for k, v in base.items() if after.get(k) != v and k != "transformers"}
assert not moved, f"base packages changed: {moved}"
print("base packages unchanged:", len(base) - 1, "| transformers:", base["transformers"], "->", after["transformers"])
EOF

WORKDIR /workspace
ENTRYPOINT []
CMD ["bash"]
