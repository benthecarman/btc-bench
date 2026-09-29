"""verl agent loop for btc-bench: one submit turn, scored by reward-serve.

Follows verl's SingleTurnAgentLoop, with three differences:

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

import json
from typing import Any
from uuid import uuid4

from verl.experimental.agent_loop.agent_loop import AgentLoopBase, AgentLoopOutput, register
from verl.utils.profiler import simple_timer
from verl.utils.rollout_trace import rollout_trace_op

from btc_verl import parse, reward_client

INVALID_REWARD_VALUE = -999.0


@register("btc_bench")
class BtcBenchAgentLoop(AgentLoopBase):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.response_length = self.rollout_config.response_length
        tok = self.tokenizer
        self.eos_ids = {tok.eos_token_id} | {
            tok.convert_tokens_to_ids(t) for t in ("<|im_end|>", "<|endoftext|>")
        }
        self.eos_ids.discard(None)
        self.eos_ids.discard(tok.unk_token_id)

    @rollout_trace_op
    async def run(self, sampling_params: dict[str, Any], priority: int = 0, **kwargs) -> AgentLoopOutput:
        priority = int(priority)
        messages = [dict(m) for m in kwargs["raw_prompt"]]
        extra = kwargs["extra_info"]
        fixture = json.loads(extra["fixture_json"])
        request = parse.chat_request(messages, json.loads(extra["tools_json"]), self.apply_chat_template_kwargs)
        prompt_ids = await self.apply_chat_template(messages, tools=parse.served_tools(request))

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
        info = {"truncated": float(truncated), "reward_unavailable": 0.0, "submitted": 0.0,
                "decoded": 0.0, "equivalent": 0.0}
        if truncated:
            reward = 0.0
        else:
            text = self.tokenizer.decode(response_ids, skip_special_tokens=True)
            completion = parse.parse_completion(text, self.tokenizer, request)
            try:
                result = await reward_client.score(fixture, completion)
                reward = float(result["score"])
                info["submitted"] = float(result.get("reason") != "no submit tool call")
                info["decoded"] = float(result["components"]["decoded"])
                info["equivalent"] = float(result["components"]["equivalent"])
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
