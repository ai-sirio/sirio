#!/usr/bin/env python3
"""A deterministic, fast-streaming ACP v1 agent for driving Chat transcript
scroll/follow behaviour live (F-CHAT-20) without depending on a real LLM's
uncontrolled response cadence.

Point `TILLER_ACP_PROGRAM` at this file (it must be executable) and launch a
Chat tab: every `session/prompt` streams `LINE_COUNT` predictable
`agent_message_chunk` lines (`LINE 0001`, `LINE 0002`, ...) spaced
`INTERVAL_S` apart, then ends the turn. Both are overridable via
`ACP_TICKER_LINES` / `ACP_TICKER_INTERVAL_S` env vars so a drive can size the
stream to the pane's real height and to however much wall-clock budget it
has.

Why a fixture instead of the real agent: P131 established that a live
agent's own event cadence (not the capture pipeline) is what made prior
`F-CHAT-20` drives look "stale" — there is no ground truth for what a real
model's streaming reply *should* contain at a given wall-clock moment. This
fixture removes that ambiguity the same way P131's own terminal-counter
experiment did: the expected line number at any capture time is
`floor((now - start) / INTERVAL_S)`, computable from timestamps bracketing
the capture, so a screenshot can be graded against a known-correct value
instead of "does this look plausible".

Not a general-purpose ACP mock — kept deliberately smaller than
`rust/crates/sirio_acp/tests/fixtures/acp_fixture.py`, which several
integration tests already depend on for its exact MODE surface; this is a
separate, single-purpose file so neither has to grow to cover the other's
job.
"""

import json
import os
import sys
import time

SESSION_ID = "ticker-session"
LINE_COUNT = int(os.environ.get("ACP_TICKER_LINES", "60"))
INTERVAL_S = float(os.environ.get("ACP_TICKER_INTERVAL_S", "0.3"))


def send(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def response(request_id, result):
    send({"jsonrpc": "2.0", "id": request_id, "result": result})


def notify_chunk(text):
    send(
        {
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": SESSION_ID,
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": text},
                },
            },
        }
    )


def main():
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            return
        method = request.get("method")
        if method == "initialize":
            response(request["id"], {"protocolVersion": 1, "agentCapabilities": {}})
        elif method == "session/new":
            response(request["id"], {"sessionId": SESSION_ID})
        elif method == "session/prompt":
            start = time.time()
            for n in range(1, LINE_COUNT + 1):
                notify_chunk(f"LINE {n:04d} t={time.time() - start:.3f}\n")
                time.sleep(INTERVAL_S)
            response(request["id"], {"stopReason": "end_turn"})
            return
        elif method == "session/cancel":
            return


if __name__ == "__main__":
    main()
