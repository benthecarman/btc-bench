"""Score a parsed completion with `btc-bench reward-serve`."""

import asyncio
import json
import os
import urllib.error
import urllib.request

REWARD_URL = os.environ.get("BTCBENCH_REWARD_URL", "http://127.0.0.1:9900/reward")
RETRY_BACKOFF_SECS = (2, 8, 30)


class RewardUnavailable(RuntimeError):
    """The reward server could not score a rollout; not a model failure."""


def _post(body: bytes) -> dict:
    req = urllib.request.Request(REWARD_URL, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        return json.load(resp)


async def score(fixture: dict, completion: dict) -> dict:
    """The server's response: score (bench grade), shaped, reason, components."""
    body = json.dumps({"task": fixture, "completion": completion}).encode()
    for attempt, backoff in enumerate((*RETRY_BACKOFF_SECS, None)):
        try:
            return await asyncio.to_thread(_post, body)
        except urllib.error.HTTPError as e:
            # 4xx is a malformed request: a bug to fix, so fail loudly.
            if e.code < 500:
                raise ValueError(f"reward server rejected the request: {e.read()[:300]!r}") from e
            if backoff is None:
                raise RewardUnavailable(f"reward server HTTP {e.code}") from e
        except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
            if backoff is None:
                raise RewardUnavailable(f"reward server unreachable: {e}") from e
        await asyncio.sleep(backoff)
