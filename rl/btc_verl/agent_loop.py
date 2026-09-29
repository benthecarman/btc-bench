"""verl agent loops for btc-bench, scored by reward-serve.

`btc_bench` is one submit turn. `btc_bench_tools` is the `--tools basic`
conversation: the model may call check_script / check_descriptor, gets
the runner's own diagnostic replies, and ends by submitting.

The single-turn loop follows verl's SingleTurnAgentLoop, with three
differences:

- Each row brings its own submit tool (script, descriptor or identify),
  passed through vLLM's request model so the chat template sees the tool
  exactly as the eval server does.
- The response is decoded and split the way the eval server splits it
  (see parse.py), and the answer is taken by reward-serve with the
  runner's own rule.
- A response that ends without EOS used the whole budget; it scores 0,
  as an unanswered task does in `btc-bench grade`.

A reward-server outage scores INVALID_REWARD_VALUE, which GRPO excludes
from the group (algorithm.invalid_reward_value), instead of a 0 that
would train against a correct answer.
"""

import functools
import json
from typing import Any
from uuid import uuid4

from verl.experimental.agent_loop.agent_loop import AgentLoopBase, AgentLoopOutput, register
from verl.utils.profiler import simple_timer
from verl.utils.tokenizer.chat_template import apply_chat_template
from verl.utils.rollout_trace import rollout_trace_op

from btc_verl import parse, reward_client

INVALID_REWARD_VALUE = -999.0


def new_info() -> dict:
    # Every rollout reports the same keys: verl stacks them per batch.
    return {"truncated": 0.0, "reward_unavailable": 0.0, "submitted": 0.0,
            "decoded": 0.0, "equivalent": 0.0, "checks": 0.0, "turns": 0.0}


def record_reward(info: dict, result: dict) -> float:
    info["submitted"] = float(result.get("reason") != "no submit tool call")
    info["decoded"] = float(result["components"]["decoded"])
    info["equivalent"] = float(result["components"]["equivalent"])
    return float(result["score"])


def render_after_turn(processing_class, tokenizer, messages: list[dict], template_kwargs) -> list[int]:
    """Tokens the chat template puts after an assistant turn for these
    messages, through the next generation prompt, minus the turn
    separator (verl's AgentLoopBase.turn_separator) that precedes them.

    The template rejects a conversation without a user message, so the
    messages follow an empty user anchor that is then cut off.
    """
    anchor = [{"role": "user", "content": ""}]
    render = functools.partial(apply_chat_template, processing_class, tokenize=False, **template_kwargs)
    head = render(anchor, add_generation_prompt=False)
    full = render(anchor + messages, add_generation_prompt=True)
    if not full.startswith(head):
        raise ValueError("chat template renders the anchor turn differently with more messages")
    return tokenizer.encode(full[len(head):], add_special_tokens=False)


class _BtcBenchBase(AgentLoopBase):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.response_length = self.rollout_config.response_length
        tok = self.tokenizer
        self.eos_ids = {tok.eos_token_id} | {
            tok.convert_tokens_to_ids(t) for t in ("<|im_end|>", "<|endoftext|>")
        }
        self.eos_ids.discard(None)
        self.eos_ids.discard(tok.unk_token_id)

    async def _prompt(self, kwargs):
        messages = [dict(m) for m in kwargs["raw_prompt"]]
        extra = kwargs["extra_info"]
        request = parse.chat_request(messages, json.loads(extra["tools_json"]), self.apply_chat_template_kwargs)
        prompt_ids = await self.apply_chat_template(messages, tools=parse.served_tools(request))
        return json.loads(extra["fixture_json"]), request, prompt_ids


@register("btc_bench")
class BtcBenchAgentLoop(_BtcBenchBase):
    @rollout_trace_op
    async def run(self, sampling_params: dict[str, Any], priority: int = 0, **kwargs) -> AgentLoopOutput:
        priority = int(priority)
        fixture, request, prompt_ids = await self._prompt(kwargs)

        metrics = {}
        with simple_timer("generate_sequences", metrics):
            request_id = f"det-{priority}" if getattr(self.rollout_config, "full_determinism", False) else uuid4().hex
            output = await self.server_manager.generate(
                request_id=request_id, prompt_ids=prompt_ids, sampling_params=sampling_params, priority=priority,
            )
        if metrics.get("num_preempted") is None:
            metrics["num_preempted"] = output.num_preempted if output.num_preempted is not None else -1

        response_ids = output.token_ids[: self.response_length]
        truncated = not response_ids or response_ids[-1] not in self.eos_ids
        info = new_info()
        info["truncated"], info["turns"] = float(truncated), 1.0
        if truncated:
            reward = 0.0
        else:
            text = self.tokenizer.decode(response_ids, skip_special_tokens=True)
            completion = parse.parse_completion(text, self.tokenizer, request)
            try:
                reward = record_reward(info, await reward_client.score(fixture, completion))
            except reward_client.RewardUnavailable:
                reward = INVALID_REWARD_VALUE
                info["reward_unavailable"] = 1.0
        info["reward"] = reward

        out = AgentLoopOutput(
            prompt_ids=prompt_ids,
            response_ids=response_ids,
            response_mask=[1] * len(response_ids),
            response_logprobs=output.log_probs[: self.response_length] if output.log_probs else None,
            reward_score=reward,
            num_turns=2,
            metrics=metrics,
            extra_fields=output.extra_fields,
        )
        out.extra_fields.update({"turn_scores": [], "tool_rewards": [], "reward_extra_info": info})
        return out


@register("btc_bench_tools")
class BtcBenchToolsAgentLoop(_BtcBenchBase):
    """The `--tools basic` conversation, one assistant turn at a time.

    Each turn is parsed as the eval server parses it and sent to
    reward-serve's /turn, which applies the runner's own loop: a submit
    ends the rollout and is graded, check calls get the runner's replies
    and the conversation continues, a turn with neither ends it at zero.
    The policy's tokens are kept as generated (never re-rendered); the
    replies are rendered by the chat template after the turn separator
    and masked out of the loss. The whole conversation shares one
    response budget; running out scores 0, like an unanswered task.
    """

    @rollout_trace_op
    async def run(self, sampling_params: dict[str, Any], priority: int = 0, **kwargs) -> AgentLoopOutput:
        priority = int(priority)
        fixture, request, prompt_ids = await self._prompt(kwargs)
        response_ids, mask, logprobs = [], [], []
        info, metrics = new_info(), {}
        checks_used, reward = 0, 0.0
        extra_fields = {}
        while True:
            remaining = self.response_length - len(response_ids)
            if remaining < 1:
                info["truncated"] = 1.0
                break
            with simple_timer("generate_sequences", metrics):
                output = await self.server_manager.generate(
                    request_id=uuid4().hex, prompt_ids=prompt_ids + response_ids,
                    sampling_params={**sampling_params, "max_tokens": remaining}, priority=priority,
                )
            extra_fields = output.extra_fields
            turn_ids = output.token_ids[:remaining]
            response_ids += turn_ids
            mask += [1] * len(turn_ids)
            logprobs += (output.log_probs or [0.0] * len(turn_ids))[: len(turn_ids)]
            info["turns"] += 1
            if not turn_ids or turn_ids[-1] not in self.eos_ids:
                info["truncated"] = 1.0
                break
            text = self.tokenizer.decode(turn_ids, skip_special_tokens=True)
            completion = parse.parse_completion(text, self.tokenizer, request)
            try:
                result = await reward_client.turn(fixture, completion, checks_used)
            except reward_client.RewardUnavailable:
                reward = INVALID_REWARD_VALUE
                info["reward_unavailable"] = 1.0
                break
            if result["done"]:
                reward = record_reward(info, result["reward"])
                break
            checks_used = result["checks_used"]
            # The runner answers a call without an id (textual fallback)
            # with a user message instead of a tool response.
            replies = [{"role": "tool" if r["tool_call_id"] else "user", "content": r["content"]}
                       for r in result["replies"]]
            reply_ids = self.turn_separator + self._render_turn(replies)
            if len(response_ids) + len(reply_ids) >= self.response_length:
                info["truncated"] = 1.0
                break
            response_ids += reply_ids
            mask += [0] * len(reply_ids)
            logprobs += [0.0] * len(reply_ids)
        info["checks"] = float(checks_used)
        info["reward"] = reward
        out = AgentLoopOutput(
            prompt_ids=prompt_ids,
            response_ids=response_ids,
            response_mask=mask,
            response_logprobs=logprobs if logprobs else None,
            reward_score=reward,
            num_turns=int(info["turns"]) + 1,
            metrics=metrics,
            extra_fields=extra_fields,
        )
        out.extra_fields.update({"turn_scores": [], "tool_rewards": [], "reward_extra_info": info})
        return out

    def _render_turn(self, messages: list[dict]) -> list[int]:
        return render_after_turn(self.processor or self.tokenizer, self.tokenizer, messages,
                                 self.apply_chat_template_kwargs)
