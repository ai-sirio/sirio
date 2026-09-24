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
        "fast_mode_state": "off",
        "fast_mode_disabled_reason": "sdk_opt_in_required",
        "account": account,
        "hooks_applied": True,
    }


def init_line(version="2.1.273"):
    send({
        "type": "system",
        "subtype": "init",
        "session_id": SESSION_ID,
        "claude_code_version": version,
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


# Permission-fixture state: CLI request id -> (tool_use_id, tool_name).
PENDING = {}


def assistant_tool_use(tool_use_id, tool_name, tool_input):
    send({
        "type": "assistant",
        "message": {
            "id": "msg-perm",
            "role": "assistant",
            "model": "claude-fable-5-1",
            "content": [{
                "type": "tool_use",
                "id": tool_use_id,
                "name": tool_name,
                "input": tool_input,
            }],
            "stop_reason": None,
        },
        "parent_tool_use_id": None,
        "session_id": SESSION_ID,
        "uuid": "perm-assistant-uuid",
    })


def ask_permission(cli_request_id, tool_name, tool_use_id, tool_input, suggestions=None):
    PENDING[cli_request_id] = (tool_use_id, tool_name)
    send({
        "type": "control_request",
        "request_id": cli_request_id,
        "request": {
            "subtype": "can_use_tool",
            "tool_name": tool_name,
            "tool_use_id": tool_use_id,
            "input": tool_input,
            "permission_suggestions": suggestions if suggestions is not None else [],
        },
    })


def finish_permission(cli_request_id, answer_message):
    pending = PENDING.pop(cli_request_id, None)
    if pending is None:
        return
    tool_use_id, _tool_name = pending
    try:
        payload = answer_message.get("response", {}).get("response", {})
        behavior = payload.get("behavior", "deny")
    except AttributeError:
        behavior = "deny"
    allowed = behavior == "allow"
    send({
        "type": "user",
        "message": {"content": [{
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "is_error": not allowed,
            "content": "ran" if allowed else "denied",
        }]},
        "parent_tool_use_id": None,
        "session_id": SESSION_ID,
        "uuid": "perm-result-uuid",
    })
    result()


def main():
    if MODE == "refuse_resume" and "--resume" in sys.argv:
        # The session file is gone: refuse the resume the way the real CLI
        # does, so the worker must come back without the id.
        sys.stderr.write("fixture: unknown session\n")
        sys.stderr.flush()
        os._exit(1)
    if MODE == "echo_argv":
        # The real CLI prints its init line unprompted at startup, before
        # answering the handshake; the argv it reports is what the test
        # reads back through the client's version cell.
        init_line(" ".join(sys.argv))
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
                if MODE == "commands_changed":
                    # A plugin or skill loaded mid-session. The CLI resends
                    # the whole list — terminal-bound commands included,
                    # because it does not resend the terminal filter.
                    send({
                        "type": "system",
                        "subtype": "commands_changed",
                        "session_id": SESSION_ID,
                        "uuid": "commands-changed-uuid",
                        "commands": [
                            {"name": "statusline",
                             "description": "Configure the status line",
                             "argumentHint": ""},
                            {"name": "deep-research",
                             "description": "Fan out web searches",
                             "argumentHint": "<question>"},
                        ],
                    })
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
            elif subtype == "set_max_thinking_tokens":
                if MODE == "no_thinking":
                    send({
                        "type": "control_response",
                        "response": {
                            "subtype": "error",
                            "request_id": request_id,
                            "error": "Unknown control request subtype",
                        },
                    })
                else:
                    control_response(request_id, {})
            elif subtype == "apply_flag_settings":
                if MODE == "no_fast_mode":
                    send({
                        "type": "control_response",
                        "response": {
                            "subtype": "error",
                            "request_id": request_id,
                            "error": "Unknown setting: fastMode",
                        },
                    })
                else:
                    control_response(request_id, {})
            elif subtype == "get_context_usage":
                if MODE == "no_context_usage":
                    # Answers nothing: the turn still has to end, on the
                    # numbers the result already carried.
                    pass
                else:
                    control_response(request_id, {
                        "totalTokens": 48000,
                        "rawMaxTokens": 200000,
                        "maxTokens": 180000,
                        "percentage": 24,
                        "model": "claude-fable-5-1",
                        "categories": [],
                    })
            elif subtype == "rewind_files":
                if MODE == "no_rewind":
                    send({
                        "type": "control_response",
                        "response": {
                            "subtype": "error",
                            "request_id": request_id,
                            "error": "Unknown control request subtype: rewind_files",
                        },
                    })
                else:
                    control_response(request_id, {
                        "canRewind": True,
                        "filesChanged": ["/repo/a.rs"],
                        "insertions": 12,
                        "deletions": 4,
                    })
            else:
                control_response(request_id, {})
        elif kind == "control_response":
            response = message.get("response", {})
            cli_request_id = response.get("request_id")
            if cli_request_id in PENDING:
                finish_permission(cli_request_id, message)
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
                # Sixteen reports at 250 ms — 4 s — across a window the test
                # sets to 2 s: the turn outlives the window twice over without
                # ever being silent for an eighth of one, so a loaded machine
                # cannot turn this into a measurement of its own scheduler.
                for index in range(16):
                    text(f"step {index} ")
                    time.sleep(0.25)
                result()
                return
            if MODE in ("context_usage", "no_context_usage"):
                init_line()
                text("Hello")
                result()
                continue
            if MODE == "permission":
                init_line()
                tool_input = {"command": "rm -rf build", "description": "Remove build"}
                assistant_tool_use("toolu_perm_1", "Bash", tool_input)
                ask_permission(
                    "cli-perm-1", "Bash", "toolu_perm_1", tool_input,
                    [{"type": "addRules", "behavior": "allow", "destination": "session"}],
                )
                continue
            if MODE == "permission_then_text":
                init_line()
                tool_input = {"command": "rm -rf build", "description": "Remove build"}
                assistant_tool_use("toolu_perm_1", "Bash", tool_input)
                ask_permission("cli-perm-1", "Bash", "toolu_perm_1", tool_input)
                text("still here")
                text(" still working")
                continue
            if MODE == "question":
                init_line()
                tool_input = {"questions": [{
                    "header": "Pick a branch",
                    "question": "Which branch should this target?",
                    "options": [{"label": "main", "description": "The default branch"},
                                {"label": "dev"}],
                }]}
                assistant_tool_use("toolu_q_1", "AskUserQuestion", tool_input)
                ask_permission("cli-q-1", "AskUserQuestion", "toolu_q_1", tool_input)
                continue
            if MODE == "exit_plan_mode":
                init_line()
                tool_input = {"plan": "1. Read\n2. Write"}
                assistant_tool_use("toolu_plan_1", "ExitPlanMode", tool_input)
                ask_permission("cli-plan-1", "ExitPlanMode", "toolu_plan_1", tool_input)
                continue
            if MODE == "notices":
                init_line()
                # A foreground call and a backgrounded one raise the same
                # pair of messages; only the start says which is which.
                for task_id, backgrounded in (("fg1", False), ("bg1", True)):
                    send({"type": "system", "subtype": "task_started",
                          "session_id": SESSION_ID, "uuid": f"start-{task_id}",
                          "task_id": task_id, "is_backgrounded": backgrounded,
                          "task_type": "local_bash", "description": "work"})
                send({"type": "system", "subtype": "background_tasks_changed",
                      "session_id": SESSION_ID, "uuid": "tasks-1",
                      "tasks": [{"task_id": "bg1", "task_type": "local_bash",
                                 "description": "work"}]})
                send({"type": "system", "subtype": "task_notification",
                      "session_id": SESSION_ID, "uuid": "note-fg",
                      "task_id": "fg1", "status": "completed",
                      "summary": "a foreground call nobody asked about"})
                send({"type": "system", "subtype": "background_tasks_changed",
                      "session_id": SESSION_ID, "uuid": "tasks-2", "tasks": []})
                send({"type": "system", "subtype": "task_notification",
                      "session_id": SESSION_ID, "uuid": "note-bg",
                      "task_id": "bg1", "status": "completed",
                      "summary": "Background command \"work\" completed (exit code 0)"})
                send({"type": "system", "subtype": "compact_boundary",
                      "session_id": SESSION_ID, "uuid": "compact-1",
                      "compact_metadata": {"trigger": "auto",
                                           "pre_tokens": 43134,
                                           "post_tokens": 11574}})
                send({"type": "rate_limit_event", "session_id": SESSION_ID,
                      "uuid": "limit-1",
                      "rate_limit_info": {"status": "rejected",
                                          "rateLimitType": "five_hour",
                                          "resetsAt": 1789990800,
                                          "unifiedWindows": {
                                              "five_hour": {"utilization": 1.0}}}})
                result()
                continue
            # Stay up serving turns until stdin closes: the worker owns our
            # lifetime, and exiting after one turn would read as a death.
            normal_turn()


main()
