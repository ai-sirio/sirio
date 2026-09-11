#!/usr/bin/env python3
"""Deterministic ACP v1 agent for the sirio_ui chat-surface drawn tests.

Modes (argv[1], optional argv[2] is a scratch directory):

  staged <dir>     Stream chunk "first ", block until <dir>/go exists, then
                   stream the rest of a rich turn (second chunk, thought,
                   tool call, usage update, end_turn). The go-file gate makes
                   the "transcript grows in place" assertion deterministic:
                   the fixture cannot emit chunk two before the test allows it.
  permission       On every prompt, request a permission with Allow/Deny
                   options, read whichever answer the client sends, and end
                   the turn.
  permission-unrenderable
                   Request a permission with no options and no structured
                   question, read the client's cancellation, and end the turn.
  subagent         Emit a Task tool call followed by a nested Read call. One process handles several prompts, so both
                   answers can be exercised against one connection.
  question         On every prompt, ask a structured question with no wire
                   options (F-CHAT-25 text answers), read the answer, echo
                   it back as an assistant chunk, and end the turn.
  question-options On every prompt, ask a structured question WITH wire
                   options (F-CHAT-25 listed-option answers): the client
                   must render clickable pills instead of a text field.
                   Read the chosen option id, echo it back, end the turn.
  question-expire  Ask a question and end the turn without reading an
                   answer: the client must expire the card (F-CHAT-27).
  plan             Publish a plan, request approval with Approve/Keep
                   options, read the answer, advance the plan entries, and
                   end the turn (F-CHAT-24).
  cancel           The first prompt streams a partial reply then waits for a
                   session/cancel notification and answers the prompt with
                   stopReason "cancelled". Later prompts complete normally.
  plain            Every prompt completes immediately: one chunk, end_turn.
  death-then-ok <dir>  On the first invocation (no <dir>/died marker) stream a
                   partial reply and die; later invocations stream normally.
                   Lets the drawn Retry click be followed by a real recovery.
  auth-required    initialize advertises one login auth method, then every
                   session/new is rejected with wire code -32000
                   "Authentication required" — ACP's auth_required shape, so
                   the client must draw its dedicated auth banner (naming the
                   advertised method) instead of the generic connection card.
                   Stateless: a Retry launches a fresh process that rejects
                   the same way, which is what makes a second attempt
                   observable.
  broken-transport initialize and session/new succeed, then the first
                   session/prompt answers with a line that is not JSON at all
                   and exits non-zero: the connection itself dies rather than
                   one request being refused, which is the shape that lands
                   the "Agent disconnected" banner. Every invocation starts
                   clean, so a Restart click reconnects and then sits idle.

Wire shapes mirror the sirio_acp integration fixture (acp_fixture.py).
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


def error(request_id, message):
    send(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32000, "message": message},
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


def tool_call(tool_id, title, status, raw_input=None):
    update = {
        "sessionUpdate": "tool_call",
        "toolCallId": tool_id,
        "title": title,
        "status": status,
    }
    if raw_input is not None:
        update["rawInput"] = raw_input
    notification(update)


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


def advertise():
    """Advertise slash commands, model/effort config options, and an 85%
    context fill, the way a real agent announces its composer affordances."""
    notification(
        {
            "sessionUpdate": "available_commands_update",
            "availableCommands": [
                {"name": "cr", "description": "Code review the diff"},
                {"name": "create-plan", "description": "Draft an implementation plan"},
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
                    "options": [
                        {"value": "sonnet", "name": "Sonnet"},
                        {"value": "opus", "name": "Opus"},
                    ],
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
    usage_update(170000, 200000)


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


def request_unrenderable_permission():
    send(
        {
            "jsonrpc": "2.0",
            "id": 9001,
            "method": "session/request_permission",
            "params": {
                "sessionId": SESSION_ID,
                "toolCall": {
                    "toolCallId": "tool-unrenderable",
                    "title": "Unknown permission",
                    "status": "pending",
                },
                "options": [],
            },
        }
    )


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


def plan_update(entries):
    notification(
        {
            "sessionUpdate": "plan",
            "entries": [
                {"content": content, "status": status, "priority": "medium"}
                for content, status in entries
            ],
        }
    )


def request_question():
    """A structured question with no wire options: the client must offer a
    free-text answer field (F-CHAT-25) and the answer travels back as the
    selected option id."""
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
                        "_sirioTextInput": {
                            "placeholder": "Type a color",
                            "prefill": "",
                        },
                    },
                },
                "options": [],
            },
        }
    )


def request_question_with_options():
    """A structured question WITH wire options: unlike `request_question`
    (top-level `options: []`, forcing a free-text field), this populates the
    top-level `options` the client renders as clickable pills -- the "listed
    option" arm of F-CHAT-25 that `request_question` deliberately does not
    exercise. Same rendering path as a plain permission request's options,
    just carrying the "Ask user question" title and a `rawInput.questions`
    payload like a real structured question would."""
    send(
        {
            "jsonrpc": "2.0",
            "id": 9001,
            "method": "session/request_permission",
            "params": {
                "sessionId": SESSION_ID,
                "toolCall": {
                    "toolCallId": "tool-q-opts",
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
                        ]
                    },
                },
                "options": [
                    {"optionId": "blue", "name": "Blue", "kind": "allow_once"},
                    {"optionId": "green", "name": "Green", "kind": "allow_once"},
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


def subagent_turn(request):
    tool_call(
        "task-1",
        "Task",
        "in_progress",
        {
            "description": "Inspect the chat transcript",
            "subagent_type": "explorer",
        },
    )
    time.sleep(0.02)
    tool_call("child-1", "Read chat.rs", "in_progress")
    time.sleep(0.02)
    tool_call_update("child-1", "completed")
    tool_call_update("task-1", "completed")
    response(request["id"], {"stopReason": "end_turn"})


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
            result = {"protocolVersion": 1, "agentCapabilities": {}}
            if mode == "auth-required":
                # The banner must be able to name the method the agent
                # itself advertised, so the login guidance is the agent's,
                # not invented by the client.
                result["authMethods"] = [
                    {"id": "login", "name": "Login", "description": "agent auth login"}
                ]
            response(request["id"], result)
        elif method == "session/new":
            if mode == "auth-required":
                # -32000 is the wire code sirio_acp maps to a typed
                # auth_required failure (see the sirio_acp fixture's own
                # session_creation_auth_required_error_becomes_typed_auth_required).
                error(request["id"], "Authentication required")
                continue
            response(request["id"], {"sessionId": SESSION_ID})
            if mode == "composer":
                advertise()
        elif method == "session/prompt":
            if mode == "staged":
                # The first prompt is the go-gated rich turn; any later
                # prompt (e.g. a queued item draining at turn end) is
                # answered like `plain` so a queue test can watch the
                # whole completion path.
                if first_prompt:
                    first_prompt = False
                    rich_turn(request)
                else:
                    message_chunk("reply ")
                    response(request["id"], {"stopReason": "end_turn"})
            if mode == "plain":
                message_chunk("reply ")
                response(request["id"], {"stopReason": "end_turn"})
            if mode == "composer":
                message_chunk("reply ")
                response(request["id"], {"stopReason": "end_turn"})
            if mode == "echo-blocks":
                # Name every content block of the prompt back, so a test
                # can prove what actually reached the agent: an image block
                # by its media type, everything else by its type alone.
                names = []
                for block in request.get("params", {}).get("prompt", []):
                    kind = block.get("type", "?")
                    if kind == "image":
                        names.append("image(" + block.get("mimeType", "?") + ")")
                    else:
                        names.append(kind)
                message_chunk("blocks: " + ",".join(names))
                response(request["id"], {"stopReason": "end_turn"})
            if mode == "permission":
                request_permission()
                sys.stdin.readline()  # the client's answer to the permission
                response(request["id"], {"stopReason": "end_turn"})
            elif mode == "permission-unrenderable":
                request_unrenderable_permission()
                answer_line = sys.stdin.readline()
                if not answer_line:
                    return
                answer = json.loads(answer_line)
                outcome = answer.get("result", {}).get("outcome", {})
                if outcome.get("outcome") != "cancelled":
                    error(request["id"], message="expected cancelled dismissal")
                    return
                response(request["id"], {"stopReason": "end_turn"})
            elif mode == "subagent":
                subagent_turn(request)
            elif mode == "question":
                # A structured question with no wire options: the client
                # must offer a free-text answer, and the answer comes back
                # as the selected option id. Echo the answer so the test
                # can assert the round trip end to end. A cancelled
                # question (F-CHAT-25 Cancel) is also a valid outcome.
                request_question()
                answer_line = sys.stdin.readline()
                if not answer_line:
                    return
                answer = json.loads(answer_line)
                outcome = answer.get("result", {}).get("outcome", {})
                if outcome.get("outcome") == "cancelled":
                    response(request["id"], {"stopReason": "end_turn"})
                    return
                if outcome.get("outcome") != "selected":
                    error(request["id"], message="expected selected answer")
                    return
                message_chunk("You chose: " + outcome.get("optionId", ""))
                response(request["id"], {"stopReason": "end_turn"})
            elif mode == "question-options":
                # F-CHAT-25's "listed option" arm: wire options are
                # populated, so the client must render clickable pills
                # (not a free-text field). Echo the chosen option id.
                request_question_with_options()
                answer_line = sys.stdin.readline()
                if not answer_line:
                    return
                answer = json.loads(answer_line)
                outcome = answer.get("result", {}).get("outcome", {})
                if outcome.get("outcome") == "cancelled":
                    response(request["id"], {"stopReason": "end_turn"})
                    return
                if outcome.get("outcome") != "selected":
                    error(request["id"], message="expected selected answer")
                    return
                message_chunk("You picked: " + outcome.get("optionId", ""))
                response(request["id"], {"stopReason": "end_turn"})
            elif mode == "question-expire":
                # First prompt: ask a question, then wait for the client to
                # withdraw it — Escape cancels the turn, which answers the
                # pending permission with `cancelled` (the protocol requires
                # it) and ends the turn. The client must expire the card
                # instead of waiting forever (F-CHAT-27). Later prompts
                # complete normally so a follow-up turn can be exercised.
                if first_prompt:
                    first_prompt = False
                    request_question()
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
                else:
                    message_chunk("reply ")
                    response(request["id"], {"stopReason": "end_turn"})
            elif mode == "plan":
                # A plan that advances, then asks for approval: the Plan
                # card must show the entries, attach the approval buttons,
                # and reflect the entries advancing (F-CHAT-24).
                plan_update([("Read the design", "pending"), ("Implement it", "pending")])
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
                                {
                                    "optionId": "approve",
                                    "name": "Approve plan",
                                    "kind": "allow_once",
                                },
                                {
                                    "optionId": "keep",
                                    "name": "Keep planning",
                                    "kind": "reject_once",
                                },
                            ],
                        },
                    }
                )
                answer_line = sys.stdin.readline()
                if not answer_line:
                    return
                answer = json.loads(answer_line)
                outcome = answer.get("result", {}).get("outcome", {})
                if outcome.get("outcome") != "selected" or outcome.get("optionId") != "approve":
                    error(request["id"], message="expected approve selection")
                    return
                plan_update(
                    [("Read the design", "completed"), ("Implement it", "in_progress")]
                )
                message_chunk("approved")
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
            elif mode == "broken-transport":
                # Deliberately not `error(...)`: a refused request would
                # leave the agent alive and land a per-request "prompt
                # failed" message. This writes something the protocol layer
                # cannot parse as a response to anything and exits non-zero,
                # so the connection itself dies. `os._exit` skips
                # interpreter teardown, so nothing else can reach stdout
                # after the garbage line.
                sys.stdout.write("not json at all\n")
                sys.stdout.flush()
                os._exit(1)


if __name__ == "__main__":
    main()
