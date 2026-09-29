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


def _post(url: str, body: bytes) -> dict:
    req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        return json.load(resp)


async def _call(path: str, payload: dict) -> dict:
    url = REWARD_URL.rsplit("/", 1)[0] + path
    body = json.dumps(payload).encode()
    for backoff in (*RETRY_BACKOFF_SECS, None):
        try:
            return await asyncio.to_thread(_post, url, body)
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


async def score(fixture: dict, completion: dict) -> dict:
    """The server's response: score (bench grade), shaped, reason, components."""
    return await _call("/reward", {"task": fixture, "completion": completion})


async def turn(fixture: dict, completion: dict, checks_used: int) -> dict:
    """One `--tools basic` turn: {done, reward} or {done: false, replies, checks_used}."""
    return await _call("/turn", {"task": fixture, "completion": completion, "checks_used": checks_used})
