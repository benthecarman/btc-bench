"""Rollout parsing matches the eval server's parsers. Needs vllm (btc-verl image).

    MODEL_PATH=/model python3 -m pytest rl/tests/test_parse.py
"""

import json
import os

import pytest
from transformers import AutoTokenizer

from btc_verl import parse

MODEL_PATH = os.environ.get("MODEL_PATH")
pytestmark = pytest.mark.skipif(not MODEL_PATH, reason="set MODEL_PATH to a Qwen3.5-family checkpoint")

SUBMIT_SCRIPT = {
    "function": {
        "description": "Submit your final Bitcoin Script answer.",
        "name": "submit_script",
        "parameters": {
            "properties": {"script": {"description": "The Bitcoin Script as a hex string or Bitcoin Core asm",
                                      "type": "string"}},
            "required": ["script"],
            "type": "object",
        },
    },
    "type": "function",
}
MESSAGES = [{"role": "system", "content": "s"}, {"role": "user", "content": "u"}]


@pytest.fixture(scope="module")
def tokenizer():
    return AutoTokenizer.from_pretrained(MODEL_PATH)


def call(script):
    return f"<tool_call>\n<function=submit_script>\n<parameter=script>\n{script}\n</parameter>\n</function>\n</tool_call>"


def parsed(text, tokenizer):
    return parse.parse_completion(text, tokenizer, parse.chat_request(MESSAGES, [SUBMIT_SCRIPT], {"enable_thinking": True}))


def test_reasoning_text_and_call_are_split(tokenizer):
    out = parsed(f"Think it through.\n</think>\n\nHere it is.\n\n{call('OP_1 OP_EQUAL')}", tokenizer)
    assert out["reasoning"].strip() == "Think it through."
    assert out["content"].strip() == "Here it is."
    assert out["tool_calls"] == [{"name": "submit_script", "arguments": {"script": "OP_1 OP_EQUAL"}}]


def test_every_call_is_kept_in_order(tokenizer):
    out = parsed(f"r\n</think>\n\n{call('OP_1')}\n{call('OP_2')}", tokenizer)
    assert [c["arguments"]["script"] for c in out["tool_calls"]] == ["OP_1", "OP_2"]


def test_a_call_ends_reasoning_as_the_server_reads_it(tokenizer):
    # vLLM 0.27's parser engine treats <tool_call> as an implicit end of
    # reasoning, so a call written before </think> is a submitted answer
    # at eval time too. Pinned so a vLLM upgrade that changes it shows up.
    out = parsed(f"I will submit {call('OP_1')} once sure", tokenizer)
    assert out["reasoning"] == "I will submit "
    assert out["tool_calls"] == [{"name": "submit_script", "arguments": {"script": "OP_1"}}]


def test_served_tools_are_what_the_template_sees(tokenizer):
    tools = parse.served_tools(parse.chat_request(MESSAGES, [SUBMIT_SCRIPT], {"enable_thinking": True}))
    assert tools[0]["function"]["name"] == "submit_script"
    assert json.loads(json.dumps(tools))  # plain JSON, safe to template
