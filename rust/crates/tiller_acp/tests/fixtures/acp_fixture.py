#!/usr/bin/env python3
"""Tiny deterministic ACP v1 agent used by tiller_acp integration tests."""

import json
import os
import sys
import time


MODE = sys.argv[1]
SESSION_ID = "fixture-session"


def send(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def response(request_id, result):
    send({"jsonrpc": "2.0", "id": request_id, "result": result})


def error(request_id, code=-32600, message="fixture error"):
    send(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": code, "message": message},
        }
    )


def notification(update):
    send(
        {
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {"sessionId": SESSION_ID, "update": update},
        }
    )


def initialize(request):
    response(
        request["id"],
        {
            "protocolVersion": 1,
            "agentCapabilities": {},
        },
    )


def new_session(request):
    response(request["id"], {"sessionId": SESSION_ID})


def normal_prompt(request):
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "first "},
        }
    )
    time.sleep(0.03)
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "streamed"},
        }
    )
    notification(
        {
            "sessionUpdate": "tool_call",
            "toolCallId": "tool-1",
            "title": "write nonce",
            "status": "pending",
        }
    )
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
                    {
                        "optionId": "allow",
                        "name": "Allow once",
                        "kind": "allow_once",
                    },
                    {
                        "optionId": "deny",
                        "name": "Deny once",
                        "kind": "reject_once",
                    },
                ],
            },
        }
    )

    permission_response = json.loads(sys.stdin.readline())
    outcome = permission_response.get("result", {}).get("outcome", {})
    if outcome.get("outcome") != "selected" or outcome.get("optionId") != "deny":
        error(request["id"], message="expected deny selection")
        return

    notification(
        {
            "sessionUpdate": "tool_call_update",
            "toolCallId": "tool-1",
            "status": "completed",
        }
    )
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": " denied"},
        }
    )
    response(request["id"], {"stopReason": "end_turn"})


def cancel_prompt(request):
    while True:
        line = sys.stdin.readline()
        if not line:
            return
        message = json.loads(line)
        if message.get("method") == "session/cancel":
            response(request["id"], {"stopReason": "cancelled"})
            return


def cancel_permission_prompt(request):
    send(
        {
            "jsonrpc": "2.0",
            "id": 9001,
            "method": "session/request_permission",
            "params": {
                "sessionId": SESSION_ID,
                "toolCall": {
                    "toolCallId": "tool-cancel",
                    "title": "cancel this tool",
                    "status": "pending",
                },
                "options": [
                    {
                        "optionId": "allow",
                        "name": "Allow once",
                        "kind": "allow_once",
                    }
                ],
            },
        }
    )
    permission_was_cancelled = False
    cancel_was_received = False
    while not (permission_was_cancelled and cancel_was_received):
        line = sys.stdin.readline()
        if not line:
            return
        message = json.loads(line)
        if message.get("id") == 9001:
            outcome = message.get("result", {}).get("outcome", {})
            permission_was_cancelled = outcome.get("outcome") == "cancelled"
        elif message.get("method") == "session/cancel":
            cancel_was_received = True
    response(request["id"], {"stopReason": "cancelled"})


def main():
    prompt_request = None
    for line in sys.stdin:
        if not line:
            return
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            return

        method = request.get("method")
        if method == "initialize":
            initialize(request)
        elif method == "session/new":
            new_session(request)
        elif method == "session/prompt":
            prompt_request = request
            if MODE == "normal":
                normal_prompt(request)
                return
            if MODE == "malformed":
                sys.stdout.write("not-json\n")
                sys.stdout.flush()
                rejection = json.loads(sys.stdin.readline())
                if (
                    "id" not in rejection
                    or rejection.get("error", {}).get("code") != -32700
                ):
                    return
                notification(
                    {
                        "sessionUpdate": "agent_message_chunk",
                        "content": {"type": "text", "text": "after malformed"},
                    }
                )
                response(request["id"], {"stopReason": "end_turn"})
                time.sleep(60)
                return
            if MODE == "death":
                notification(
                    {
                        "sessionUpdate": "agent_message_chunk",
                        "content": {"type": "text", "text": "partial"},
                    }
                )
                os._exit(17)
            if MODE == "cancel":
                cancel_prompt(request)
                return
            if MODE == "cancel_permission":
                cancel_permission_prompt(request)
                return
        elif method == "session/cancel" and prompt_request is not None:
            response(prompt_request["id"], {"stopReason": "cancelled"})
            return


if __name__ == "__main__":
    main()
