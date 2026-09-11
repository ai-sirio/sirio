#!/usr/bin/env python3
"""Tiny deterministic ACP v1 agent used by sirio_acp integration tests."""

import json
import os
import sys
import time


MODE = sys.argv[1]
PID_PATH = sys.argv[2] if len(sys.argv) > 2 else None
SESSION_ID = "fixture-session"

if PID_PATH:
    with open(PID_PATH, "w", encoding="utf-8") as pid_file:
        pid_file.write(str(os.getpid()))


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


def advertise():
    notification(
        {
            "sessionUpdate": "available_commands_update",
            "availableCommands": [
                {"name": "cr", "description": "Code review the diff"},
                {"name": "research", "description": "Research a topic"},
            ],
        }
    )
    notification(
        {
            "sessionUpdate": "config_option_update",
            "configOptions": [
                {
                    "id": "model",
                    "name": "Model",
                    "category": "model",
                    "type": "select",
                    "currentValue": "sonnet",
                    "options": [{"value": "sonnet", "name": "Sonnet"}],
                },
                {
                    "id": "effort",
                    "name": "Reasoning effort",
                    "category": "effort",
                    "type": "select",
                    "currentValue": "medium",
                    "options": [
                        {"value": "low", "name": "Low"},
                        {"value": "medium", "name": "Medium"},
                        {"value": "high", "name": "High"},
                    ],
                },
            ],
        }
    )


def blocks_prompt(request):
    summary = []
    for block in request.get("params", {}).get("prompt", []):
        kind = block.get("type")
        if kind == "text":
            summary.append("text:" + block.get("text", ""))
        elif kind == "resource_link":
            summary.append(
                "link:" + block.get("uri", "") + ":" + block.get("name", "")
            )
        elif kind == "image":
            summary.append("image:" + block.get("mimeType", ""))
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "|".join(summary)},
        }
    )
    response(request["id"], {"stopReason": "end_turn"})


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
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "partial"},
        }
    )
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


def cancel_permission_direct_prompt(request):
    """Ask, then expect the client to withdraw the permission on its own
    (F-CHAT-25 cancel) and end the turn normally."""
    send(
        {
            "jsonrpc": "2.0",
            "id": 9001,
            "method": "session/request_permission",
            "params": {
                "sessionId": SESSION_ID,
                "toolCall": {
                    "toolCallId": "tool-cancel-direct",
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
    line = sys.stdin.readline()
    if not line:
        return
    message = json.loads(line)
    outcome = message.get("result", {}).get("outcome", {})
    if outcome.get("outcome") != "cancelled":
        error(request["id"], message="expected cancelled outcome")
        return
    response(request["id"], {"stopReason": "end_turn"})


def question_death_prompt(request):
    """Ask a question and die without reading an answer: the transport
    death must expire the pending permission instead of leaving it open."""
    send(
        {
            "jsonrpc": "2.0",
            "id": 9001,
            "method": "session/request_permission",
            "params": {
                "sessionId": SESSION_ID,
                "toolCall": {
                    "toolCallId": "tool-q-death",
                    "title": "Ask user question",
                    "status": "pending",
                },
                "options": [],
            },
        }
    )
    os._exit(17)


def queue_prompt(request, number):
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "first" if number == 1 else "second"},
        }
    )
    if number == 1:
        time.sleep(0.15)
    response(request["id"], {"stopReason": "end_turn"})


def plan_prompt(request):
    """Publish a plan, ask for approval, advance the plan after the answer."""
    notification(
        {
            "sessionUpdate": "plan",
            "entries": [
                {"content": "Read the design", "status": "pending", "priority": "medium"},
                {"content": "Implement it", "status": "pending", "priority": "medium"},
            ],
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
                    "toolCallId": "tool-plan",
                    "title": "Exit plan mode",
                    "status": "pending",
                },
                "options": [
                    {"optionId": "approve", "name": "Approve plan", "kind": "allow_once"},
                    {"optionId": "keep", "name": "Keep planning", "kind": "reject_once"},
                ],
            },
        }
    )
    permission_response = json.loads(sys.stdin.readline())
    outcome = permission_response.get("result", {}).get("outcome", {})
    if outcome.get("outcome") != "selected" or outcome.get("optionId") != "approve":
        error(request["id"], message="expected approve selection")
        return
    notification(
        {
            "sessionUpdate": "plan",
            "entries": [
                {"content": "Read the design", "status": "completed", "priority": "medium"},
                {"content": "Implement it", "status": "in_progress", "priority": "medium"},
            ],
        }
    )
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "approved"},
        }
    )
    response(request["id"], {"stopReason": "end_turn"})


def question_prompt(request, text_input_key="_sirioTextInput"):
    """A structured question with no wire options, answered with free text.

    The key is parameterised because agents built before the Sirio rename
    declare the affordance as `_tillerTextInput`, and that spelling must keep
    working.
    """
    send(
        {
            "jsonrpc": "2.0",
            "id": 9001,
            "method": "session/request_permission",
            "params": {
                "sessionId": SESSION_ID,
                "toolCall": {
                    "toolCallId": "tool-q",
                    "title": "Ask user question",
                    "status": "pending",
                    "rawInput": {
                        "questions": [
                            {
                                "header": "Which color?",
                                "question": "Which color should the button be?",
                                "options": [
                                    {"label": "Blue", "description": "The ocean"},
                                    {"label": "Green", "description": "The grass"},
                                ],
                            }
                        ],
                        text_input_key: {"placeholder": "Type a color", "prefill": ""},
                    },
                },
                "options": [],
            },
        }
    )
    permission_response = json.loads(sys.stdin.readline())
    outcome = permission_response.get("result", {}).get("outcome", {})
    if outcome.get("outcome") != "selected" or outcome.get("optionId") != "Blue":
        error(request["id"], message="expected the typed answer")
        return
    notification(
        {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "You chose: Blue"},
        }
    )
    response(request["id"], {"stopReason": "end_turn"})


def main():
    prompt_request = None
    prompt_count = 0
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
            if MODE == "blocks":
                advertise()
        elif method == "session/prompt":
            prompt_request = request
            prompt_count += 1
            if MODE == "normal":
                normal_prompt(request)
                return
            if MODE == "blocks":
                blocks_prompt(request)
                return
            if MODE == "multi":
                blocks_prompt(request)
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
                sys.stderr.write("fixture last words before dying\n")
                sys.stderr.flush()
                os._exit(17)
            if MODE == "cancel":
                cancel_prompt(request)
                return
            if MODE == "cancel_permission":
                cancel_permission_prompt(request)
                return
            if MODE == "cancel_permission_direct":
                cancel_permission_direct_prompt(request)
                return
            if MODE == "question_death":
                question_death_prompt(request)
                return
            if MODE == "queue":
                queue_prompt(request, prompt_count)
            if MODE == "plan":
                plan_prompt(request)
                return
            if MODE == "question":
                question_prompt(request)
                return
            if MODE == "question_legacy":
                question_prompt(request, text_input_key="_tillerTextInput")
                return
        elif method == "session/cancel" and prompt_request is not None:
            response(prompt_request["id"], {"stopReason": "cancelled"})
            return


if __name__ == "__main__":
    main()
