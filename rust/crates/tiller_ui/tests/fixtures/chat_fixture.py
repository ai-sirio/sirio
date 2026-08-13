#!/usr/bin/env python3
"""Deterministic ACP v1 agent for the tiller_ui chat-surface drawn tests.

Modes (argv[1], optional argv[2] is a scratch directory):

  staged <dir>     Stream chunk "first ", block until <dir>/go exists, then
                   stream the rest of a rich turn (second chunk, thought,
                   tool call, usage update, end_turn). The go-file gate makes
                   the "transcript grows in place" assertion deterministic:
                   the fixture cannot emit chunk two before the test allows it.
  permission       On every prompt, request a permission with Allow/Deny
                   options, read whichever answer the client sends, and end
                   the turn. One process handles several prompts, so both
                   answers can be exercised against one connection.
  cancel           The first prompt streams a partial reply then waits for a
                   session/cancel notification and answers the prompt with
                   stopReason "cancelled". Later prompts complete normally.
  plain            Every prompt completes immediately: one chunk, end_turn.
  death-then-ok <dir>  On the first invocation (no <dir>/died marker) stream a
                   partial reply and die; later invocations stream normally.
                   Lets the drawn Retry click be followed by a real recovery.

Wire shapes mirror the tiller_acp integration fixture (acp_fixture.py).
"""

import json
import os
import sys
import time


SESSION_ID = "chat-fixture-session"


def send(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def response(request_id, result):
    send({"jsonrpc": "2.0", "id": request_id, "result": result})


def notification(update):
    send(
        {
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {"sessionId": SESSION_ID, "update": update},
        }
    )


def message_chunk(text):
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": text},
        }
    )


def thought_chunk(text):
    notification(
        {
            "sessionUpdate": "agent_thought_chunk",
            "content": {"type": "text", "text": text},
        }
    )


def tool_call(tool_id, title, status):
    notification(
        {
            "sessionUpdate": "tool_call",
            "toolCallId": tool_id,
            "title": title,
            "status": status,
        }
    )


def tool_call_update(tool_id, status):
    notification(
        {
            "sessionUpdate": "tool_call_update",
            "toolCallId": tool_id,
            "status": status,
        }
    )


def usage_update(used, size):
    notification({"sessionUpdate": "usage_update", "used": used, "size": size})


def rich_turn(request):
    """The full anatomy of one streamed turn."""
    message_chunk("first ")
    if wait_for_go(sys.argv[2]):
        message_chunk("streamed")
    thought_chunk("thinking hard")
    tool_call("tool-1", "write nonce", "pending")
    time.sleep(0.02)
    tool_call_update("tool-1", "completed")
    usage_update(53000, 200000)
    response(request["id"], {"stopReason": "end_turn"})


def request_permission():
    send(
        {
            "jsonrpc": "2.0",
            "id": 9001,
            "method": "session/request_permission",
            "params": {
                "sessionId": SESSION_ID,
                "toolCall": {
                    "toolCallId": "tool-1",
                    "title": "write nonce",
                    "status": "pending",
                },
                "options": [
                    {"optionId": "allow", "name": "Allow once", "kind": "allow_once"},
                    {"optionId": "deny", "name": "Deny once", "kind": "reject_once"},
                ],
            },
        }
    )


def wait_for_go(dir_path):
    deadline = time.time() + 120
    while time.time() < deadline:
        if os.path.exists(os.path.join(dir_path, "go")):
            return True
        time.sleep(0.02)
    return False


def main():
    mode = sys.argv[1]
    extra = sys.argv[2] if len(sys.argv) > 2 else None
    first_prompt = True

    for line in sys.stdin:
        if not line:
            return
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
            if mode == "staged":
                rich_turn(request)
                return
            if mode == "plain":
                message_chunk("reply ")
                response(request["id"], {"stopReason": "end_turn"})
            if mode == "permission":
                request_permission()
                sys.stdin.readline()  # the client's answer to the permission
                response(request["id"], {"stopReason": "end_turn"})
            elif mode == "cancel":
                if first_prompt:
                    first_prompt = False
                    message_chunk("partial ")
                    while True:
                        line = sys.stdin.readline()
                        if not line:
                            return
                        message = json.loads(line)
                        if message.get("method") == "session/cancel":
                            response(request["id"], {"stopReason": "cancelled"})
                            break
                else:
                    message_chunk("done ")
                    response(request["id"], {"stopReason": "end_turn"})
            elif mode == "death-then-ok":
                marker = os.path.join(extra, "died")
                if os.path.exists(marker):
                    message_chunk("alive ")
                    response(request["id"], {"stopReason": "end_turn"})
                else:
                    with open(marker, "w") as f:
                        f.write("1")
                    message_chunk("partial ")
                    os._exit(17)


if __name__ == "__main__":
    main()
