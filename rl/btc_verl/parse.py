"""Split a rollout into reasoning, text and tool calls with vLLM's parsers.

The eval server runs `--reasoning-parser qwen3 --tool-call-parser
qwen3_coder`; the same parser classes run here, looked up by those
names, so a rollout is read exactly as `btc-bench run` would receive it.
"""

import json
from functools import lru_cache

REASONING_PARSER = "qwen3"
TOOL_PARSER = "qwen3_coder"


def _managers():
    try:
        from vllm.reasoning import ReasoningParserManager
    except ImportError:  # older layout
        from vllm.reasoning.abs_reasoning_parsers import ReasoningParserManager
    try:
        from vllm.tool_parsers import ToolParserManager
    except ImportError:  # older layout
        from vllm.entrypoints.openai.tool_parsers import ToolParserManager
    return ReasoningParserManager, ToolParserManager


def _request_cls():
    try:
        from vllm.entrypoints.openai.chat_completion.protocol import ChatCompletionRequest
    except ImportError:  # older layout
        from vllm.entrypoints.openai.protocol import ChatCompletionRequest
    return ChatCompletionRequest


@lru_cache(maxsize=4)
def _parsers(tokenizer):
    reasoning_mgr, tool_mgr = _managers()
    return (reasoning_mgr.get_reasoning_parser(REASONING_PARSER)(tokenizer),
            tool_mgr.get_tool_parser(TOOL_PARSER)(tokenizer))


def chat_request(messages, tools, chat_template_kwargs):
    """The request vLLM's OpenAI server would build for these messages and tools."""
    return _request_cls()(model="rollout", messages=messages, tools=tools,
                          chat_template_kwargs=dict(chat_template_kwargs))


def served_tools(request):
    """Tool dicts as the server hands them to the chat template."""
    return [tool.model_dump() for tool in request.tools] if request.tools else None


def parse_completion(text: str, tokenizer, request) -> dict:
    """reasoning / content / tool_calls, as the served chat completion reports them."""
    reasoning_parser, tool_parser = _parsers(tokenizer)
    extract = getattr(reasoning_parser, "extract_reasoning", None) or reasoning_parser.extract_reasoning_content
    reasoning, content = extract(text, request=request)
    calls = []
    if content:
        info = tool_parser.extract_tool_calls(content, request=request)
        if info.tools_called:
            for call in info.tool_calls:
                try:
                    arguments = json.loads(call.function.arguments)
                except json.JSONDecodeError:
                    arguments = None
                # The runner's client drops a call whose arguments are not
                # a JSON object, so it can never be the answer.
                if isinstance(arguments, dict):
                    calls.append({"name": call.function.name, "arguments": arguments})
            content = info.content or ""
    return {"reasoning": reasoning or "", "content": content or "", "tool_calls": calls}
