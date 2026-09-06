"""Local deterministic ACP stream: no tools, network, or user files.

First prompt seeds 500 entries (250 assistant/tool pairs). Each prompt then
waits for <SIRIO_PERF_FIXTURE_DIR>/go-<prompt ordinal>, streams 1500 chunks at
20 ms intervals, and completes. A prompt containing 'thought' streams reasoning;
otherwise it streams markdown. The go gate excludes seed/startup from sampling.
"""
import json
import os
from pathlib import Path
import sys
import time

SESSION = "sirio-perf-fixture"


def send(message):
    print(json.dumps(message, separators=(",", ":")), flush=True)


def response(identifier, result):
    send({"jsonrpc": "2.0", "id": identifier, "result": result})


def update(value):
    send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": SESSION, "update": value}})


def chunk(text, thought=False):
    update({"sessionUpdate": "agent_thought_chunk" if thought else "agent_message_chunk", "content": {"type": "text", "text": text}})


def main():
    ordinal = 0
    run = Path(os.environ["SIRIO_PERF_FIXTURE_DIR"])
    for line in sys.stdin:
        request = json.loads(line)
        method = request.get("method")
        if method == "initialize":
            response(request["id"], {"protocolVersion": 1, "agentCapabilities": {}})
        elif method == "session/new":
            response(request["id"], {"sessionId": SESSION})
        elif method == "session/prompt":
            ordinal += 1
            if ordinal == 1:
                for index in range(250):
                    chunk(f"Fixture entry {index}: **markdown**, `code`, and plain text.\n\n")
                    update({"sessionUpdate": "tool_call", "toolCallId": f"seed-{index}", "title": "Fixture read", "kind": "read", "status": "completed"})
            thought = "thought" in json.dumps(request.get("params", {}))
            chunk("Measurement begins after the gate.\n", thought)
            (run / f"ready-{ordinal}").write_text(str(time.time_ns()))
            deadline = time.monotonic() + 300
            while not (run / f"go-{ordinal}").exists():
                if time.monotonic() > deadline:
                    raise RuntimeError("Fixture gate timed out")
                time.sleep(0.02)
            for index in range(1500):
                chunk(f"Line {index}: **steady stream** with `inline code` and a measured rendering path.\n\n", thought)
                time.sleep(0.02)
            response(request["id"], {"stopReason": "end_turn"})
            (run / f"done-{ordinal}").write_text(str(time.time_ns()))


if __name__ == "__main__":
    main()
