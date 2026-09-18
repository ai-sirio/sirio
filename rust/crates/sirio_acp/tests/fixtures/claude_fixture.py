#!/usr/bin/env python3
"""A deterministic fake `claude` speaking stream-json, for sirio_acp tests.

It is the native twin of `acp_fixture.py`: same shape, same argv contract
(mode first, optional pid file second), different protocol. Nothing here
talks to a network or reads the user's configuration.
"""

import json
import os
import sys
import time

MODE = sys.argv[1] if len(sys.argv) > 1 else "normal"
# The worker appends its protocol flags after our own argv, so a second
# positional starting with "-" is one of those, not a pid file.
PID_PATH = (
    sys.argv[2]
    if len(sys.argv) > 2 and not sys.argv[2].startswith("-")
    else None
)
SESSION_ID = "fixture-session-1"

if PID_PATH:
    with open(PID_PATH, "w", encoding="utf-8") as pid_file:
        pid_file.write(str(os.getpid()))


def send(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def control_response(request_id, payload):
    send({
        "type": "control_response",
        "response": {"subtype": "success", "request_id": request_id, "response": payload},
    })


def initialize_payload():
    account = (
        {"tokenSource": "none", "apiProvider": "firstParty"}
        if MODE == "logged_out"
        else {"email": "dev@example.com", "subscriptionType": "max", "apiProvider": "firstParty"}
    )
    return {
        "commands": [
            {"name": "usage", "description": "Show plan usage", "argumentHint": ""},
            {"name": "statusline", "description": "Configure the status line", "argumentHint": ""},
        ],
        "terminal_slash_commands": ["statusline"],
        "models": [
            {"value": "default", "displayName": "Default (recommended)", "description": ""},
            {"value": "sonnet", "displayName": "Sonnet", "description": "Balanced"},
        ],
        "agents": [],
        "output_style": "default",
        "available_output_styles": ["default"],
        "current_permission_mode": "default",
        "account": account,
        "hooks_applied": True,
    }


def init_line():
    send({
        "type": "system",
        "subtype": "init",
        "session_id": SESSION_ID,
        "claude_code_version": "2.1.273",
        "cwd": os.getcwd(),
        "model": "claude-fable-5-1",
        "permissionMode": "default",
        "tools": ["Bash", "Read", "Edit"],
        "mcp_servers": [],
        "slash_commands": ["usage"],
        "uuid": "init-uuid",
    })


def text(chunk):
    send({
        "type": "stream_event",
        "event": {
            "type": "content_block_delta",
            "delta": {"type": "text_delta", "text": chunk},
        },
        "session_id": SESSION_ID,
    })


def result(subtype="success", is_error=False, text_body="Done."):
    send({
        "type": "result",
        "subtype": subtype,
        "is_error": is_error,
        "result": text_body,
        "stop_reason": "end_turn" if not is_error else "cancelled",
        "total_cost_usd": 0.01,
        "modelUsage": {
            "claude-fable-5-1": {
                "inputTokens": 100,
                "outputTokens": 20,
                "cacheReadInputTokens": 0,
                "cacheCreationInputTokens": 0,
                "costUSD": 0.01,
                "contextWindow": 200000,
            }
        },
        "session_id": SESSION_ID,
        "uuid": "result-uuid",
    })


def normal_turn():
    init_line()
    text("Hello")
    text(" there")
    result()


def send_flush_and_exit():
    sys.stdout.flush()
    os._exit(0)


def main():
    if MODE == "silent":
        # Answers nothing, ever: the startup-timeout case.
        time.sleep(600)
        return
    for line in sys.stdin:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            return
        kind = message.get("type")
        if kind == "control_request":
            request = message.get("request", {})
            subtype = request.get("subtype")
            request_id = message.get("request_id")
            if subtype == "initialize":
                if MODE == "malformed":
                    sys.stdout.write("not-json\n")
                    sys.stdout.flush()
                control_response(request_id, initialize_payload())
            elif subtype == "interrupt":
                control_response(request_id, {"still_queued": 0})
                if MODE in ("slow_turn", "silent_turn", "chatty_slow_turn"):
                    result(subtype="error_during_execution", is_error=True,
                           text_body="Interrupted by user")
                    send_flush_and_exit()
            elif subtype == "set_permission_mode":
                control_response(request_id, {"mode": request.get("mode")})
                send({
                    "type": "system",
                    "subtype": "status",
                    "permissionMode": request.get("mode"),
                    "session_id": SESSION_ID,
                    "uuid": "status-uuid",
                })
            else:
                control_response(request_id, {})
        elif kind == "user":
            if MODE == "logged_out":
                init_line()
                result(subtype="error_during_execution", is_error=True,
                       text_body="Not logged in · Please run /login")
                return
            if MODE == "death":
                init_line()
                text("starting")
                sys.stderr.write("fixture agent: fatal error, going away\n")
                sys.stderr.flush()
                os._exit(3)
            if MODE == "echo_control":
                # Stays up answering control requests; the turn is not the
                # subject of these tests.
                continue
            if MODE == "slow_turn":
                # Emits one chunk and waits: the test interrupts the turn,
                # so blocking here would swallow the interrupt and end the
                # turn normally instead of cancelled.
                init_line()
                text("working")
                continue
            if MODE == "silent_turn":
                init_line()
                time.sleep(600)
                return
            if MODE == "chatty_slow_turn":
                init_line()
                # Six reports across a window the test sets to 400 ms: the
                # turn outlives the window without ever being silent for one.
                for index in range(6):
                    text(f"step {index} ")
                    time.sleep(0.15)
                result()
                return
            # Stay up serving turns until stdin closes: the worker owns our
            # lifetime, and exiting after one turn would read as a death.
            normal_turn()


main()
