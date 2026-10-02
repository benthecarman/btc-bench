"""The trainer's long-sequence path (two-level checkpointing, chunked
MLPs) gives the stock forward's gradients, and a rollout that runs out
of memory is left out of the step whole. Runs on CPU with a tiny
Qwen3.5 text model; needs the btc-verl image.

    CUDA_VISIBLE_DEVICES= python3 -m pytest rl/tests/test_train_long.py
"""

import argparse
import copy
import os
import random
import sys
import types

import torch
import transformers.models.qwen3_5.modeling_qwen3_5 as qwen
from transformers.models.qwen3_5.configuration_qwen3_5 import Qwen3_5TextConfig

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "local"))
import train  # noqa: E402

qwen.is_fast_path_available = False  # torch reference kernels for the linear-attention layers
ARGS = argparse.Namespace(temperature=1.0, top_k=20, top_p=0.95, loss="tis", is_cap=2.0, clip=0.2, grad_clip=1.0)


class Model(torch.nn.Module):
    """A tiny text model behind the attribute path train.py walks on the PeftModel."""

    def __init__(self):
        super().__init__()
        cfg = Qwen3_5TextConfig(vocab_size=97, hidden_size=64, intermediate_size=96, num_hidden_layers=10,
                                num_attention_heads=4, num_key_value_heads=2, head_dim=16, linear_num_key_heads=2,
                                linear_num_value_heads=4, linear_key_head_dim=16, linear_value_head_dim=16,
                                full_attention_interval=4)
        cfg._attn_implementation = "sdpa"
        self.body = qwen.Qwen3_5TextModel(cfg).float()
        self.head = torch.nn.Linear(64, 97, bias=False)
        self.body.gradient_checkpointing_enable(gradient_checkpointing_kwargs={"use_reentrant": False})
        self.link()

    def link(self):
        self.base_model = types.SimpleNamespace(model=types.SimpleNamespace(
            model=types.SimpleNamespace(language_model=self.body), lm_head=self.head))


class Record:
    """Stands in for the optimizer: keeps the gradients it is stepped with."""

    def __init__(self, model):
        self.model, self.grads = model, None

    def zero_grad(self, set_to_none=True):
        for p in self.model.parameters():
            p.grad = None

    def step(self):
        self.grads = {n: p.grad.clone() for n, p in self.model.named_parameters() if p.grad is not None}


def batch():
    rng = random.Random(1)

    def rollout(g, n_prompt, n_resp, reward):
        return {"group": g, "reward": reward, "prompt_ids": [rng.randrange(97) for _ in range(n_prompt)],
                "response_ids": [rng.randrange(97) for _ in range(n_resp)], "response_mask": [1] * n_resp,
                "rollout_logprobs": [-3.0] * n_resp}

    return [rollout(0, 8, 45, 1.0), rollout(0, 8, 12, 0.0), rollout(0, 8, 60, 0.5),
            rollout(1, 5, 10, 0.0), rollout(1, 5, 40, 1.0)]


def step(model, monkeypatch):
    real = torch.tensor
    monkeypatch.setattr(torch, "tensor", lambda data, device=None, **kw: real(data, **kw))  # CPU, not cuda
    opt = Record(model)
    return train.train_on(model, opt, batch(), ARGS), opt.grads


def test_long_path_matches_stock_forward(monkeypatch):
    torch.manual_seed(0)
    stock = Model()
    long = copy.deepcopy(stock)
    long.link()
    train.chunk_long_mlps(long.body)
    monkeypatch.setattr(train, "TWO_LEVEL_FROM", 10**9)
    s_stock, g_stock = step(stock, monkeypatch)
    # Every sequence over 30 tokens: 4-layer segments, 16-token MLP chunks.
    monkeypatch.setattr(train, "TWO_LEVEL_FROM", 30)
    monkeypatch.setattr(train, "SEGMENT", 4)
    monkeypatch.setattr(train, "MLP_CHUNK", 16)
    s_long, g_long = step(long, monkeypatch)
    assert s_long["tokens"] == s_stock["tokens"] and s_long["skipped_out_of_memory"] == []
    assert abs(s_long["grad_norm"] - s_stock["grad_norm"]) < 1e-6
    assert g_long.keys() == g_stock.keys()
    # Chunked matmuls only reorder float sums.
    scale = max(g.abs().max().item() for g in g_stock.values())
    assert max((g_long[n] - g_stock[n]).abs().max().item() for n in g_stock) < 1e-5 * scale


def test_out_of_memory_rollout_is_left_out_whole(monkeypatch):
    torch.manual_seed(0)
    model = Model()
    s_all, _ = step(model, monkeypatch)
    real = train.sequence_logprobs

    def fails_on_68(model_, ids, n_prompt, args):
        out = real(model_, ids, n_prompt, args)
        if len(ids) == 68:  # the 60-token rollout, after its forward pass
            raise torch.OutOfMemoryError("test")
        return out

    monkeypatch.setattr(train, "sequence_logprobs", fails_on_68)
    s, grads = step(model, monkeypatch)
    assert s["skipped_out_of_memory"] == [68]
    assert 0 < s["tokens"] < s_all["tokens"] and grads
