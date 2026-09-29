"""Multi-turn rollouts build the same tokens the chat template renders
for the whole conversation. Needs verl (btc-verl image).

    MODEL_PATH=/model python3 -m pytest rl/tests/test_turns.py
"""

import os

import pytest
from transformers import AutoProcessor, AutoTokenizer
from verl.utils.tokenizer.chat_template import apply_chat_template, initialize_turn_separator

from btc_verl.agent_loop import render_after_turn

MODEL_PATH = os.environ.get("MODEL_PATH")
pytestmark = pytest.mark.skipif(not MODEL_PATH, reason="set MODEL_PATH to a Qwen3.5-family checkpoint")
KWARGS = {"enable_thinking": True}
TOOLS = [{"type": "function", "function": {"name": "check_script", "description": "d", "parameters": {
    "type": "object", "properties": {"script": {"type": "string"}}, "required": ["script"]}}}]


@pytest.fixture(scope="module")
def classes():
    tokenizer = AutoTokenizer.from_pretrained(MODEL_PATH)
    try:
        processor = AutoProcessor.from_pretrained(MODEL_PATH)
    except Exception:
        processor = None
    return processor or tokenizer, tokenizer


def conversation(reply_role):
    head = [{"role": "system", "content": "s"}, {"role": "user", "content": "u"}]
    assistant = {"role": "assistant", "content": "", "reasoning_content": "check it",
                 "tool_calls": [{"type": "function", "function": {"name": "check_script",
                                                                  "arguments": {"script": "OP_1"}}}]}
    return head, assistant, {"role": reply_role, "content": "report text"}


def render(pc, msgs, gen):
    return apply_chat_template(pc, msgs, tools=TOOLS, tokenize=False, add_generation_prompt=gen, **KWARGS)


@pytest.mark.parametrize("reply_role", ["tool", "user"])
def test_reply_tokens_match_the_template(classes, reply_role):
    pc, tok = classes
    head, assistant, reply = conversation(reply_role)
    full = render(pc, head + [assistant, reply], True)
    # Everything after the assistant turn's close token and separator.
    reply_start = full.rindex("<|im_start|>", 0, full.rindex("<|im_start|>"))
    after_turn = full[full.rindex("<|im_end|>", 0, reply_start) + len("<|im_end|>"):]
    separator = tok.decode(initialize_turn_separator(pc, **KWARGS))
    assert after_turn.startswith(separator)
    expected = tok.encode(after_turn[len(separator):], add_special_tokens=False)
    assert render_after_turn(pc, tok, [reply], KWARGS) == expected


def test_tool_turn_matches_the_full_render(classes):
    # With a tool reply the template keeps the turn's reasoning, so the
    # generated tokens plus the rendered reply equal a render of the whole
    # conversation. (After a user reply some templates, Qwen3.5-9B's among
    # them, drop earlier reasoning; the rollout keeps what was generated.)
    pc, tok = classes
    head, assistant, reply = conversation("tool")
    prompt = render(pc, head, True)
    full = render(pc, head + [assistant, reply], True)
    after = full[len(prompt):]
    generated = after[: after.index("<|im_end|>") + len("<|im_end|>")]
    ids = (tok.encode(prompt, add_special_tokens=False) + tok.encode(generated, add_special_tokens=False)
           + initialize_turn_separator(pc, **KWARGS) + render_after_turn(pc, tok, [reply], KWARGS))
    assert ids == tok.encode(full, add_special_tokens=False)
