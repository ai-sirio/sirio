#!/usr/bin/env python3
"""A deterministic fake `claude` speaking stream-json, for sirio_chat's and
sirio_ui's tests and the E2E stubs.

It is the native twin of `acp_fixture.py`: same shape, same argv contract
(mode first, optional pid file second), different protocol. Nothing here
talks to a network or reads the user's configuration.
"""

import json
import os
import queue
import sys
import threading
import time

MODE = sys.argv[1] if len(sys.argv) > 1 else "normal"
# `session_panel_refuse` is `session_panel` with `mcp_reconnect` refused for
# `codegraph`: a failed action whose sentence the panel has to show.
REFUSE_RECONNECT = MODE == "session_panel_refuse"
# `session_panel_starting` reports `codegraph` as `pending` until the third
# `mcp_status` answer, `session_panel_pending` for ever: a server that is
# still starting, and one that never gets there.
STARTS_AFTER = {"session_panel_starting": 2, "session_panel_pending": None}.get(MODE, 0)
# `session_panel_bypass` is `session_panel` with the CLI started in
# `bypassPermissions`: its init and its handshake both say so, which is what
# draws the chat's mode pill as the danger shield. Its handshake also names a
# reason fast mode cannot be used (`preference`, one of the CLI's own codes, where
# every other mode says `sdk_opt_in_required`, which Sirio does not show as one),
# so the composer menu's blocked Fast row has a reason to carry.
START_BYPASS = MODE == "session_panel_bypass"
if REFUSE_RECONNECT or MODE in ("session_panel_starting", "session_panel_pending", "session_panel_bypass"):
    MODE = "session_panel"
# `effort_ladder` gives the session's first model an effort ladder, as the
# real CLI's handshake does; `effort_refused` is the same with every change of
# `effortLevel` refused. No other mode advertises one: a chat that finds an
# effort selector asks for High at once, and that is not what the rest of the
# suite is about.
EFFORT_LADDER = MODE in ("effort_ladder", "effort_refused")
REFUSE_EFFORT = MODE == "effort_refused"
# A launch argument names the file `mcp_status` requests are counted in, so a
# test needs no environment of its own; the environment names it otherwise.
COUNT_PATH = next(
    (arg[len("--fixture-count="):] for arg in sys.argv if arg.startswith("--fixture-count=")),
    os.environ.get("SIRIO_FIXTURE_COUNT"),
)
# A launch argument names the file every control request is appended to, one
# JSON line each, before it is answered: what reached the CLI, as the CLI
# heard it. A file the fixture has not created yet holds none.
CONTROL_LOG_PATH = next(
    (arg[len("--fixture-control-log="):] for arg in sys.argv if arg.startswith("--fixture-control-log=")),
    os.environ.get("SIRIO_FIXTURE_CONTROL_LOG"),
)
# The worker appends its protocol flags after our own argv, so a second
# positional starting with "-" is one of those, not a pid file.
# Where the project's memory file is said to live: a test that wants the row
# to open a real file names its checkout here.
REPO_PATH = os.environ.get("SIRIO_FIXTURE_REPO", "/repo")
PID_PATH = (
    sys.argv[2]
    if len(sys.argv) > 2 and not sys.argv[2].startswith("-")
    else None
)
SESSION_ID = "fixture-session-1"

if PID_PATH:
    with open(PID_PATH, "w", encoding="utf-8") as pid_file:
        pid_file.write(str(os.getpid()))


# The `trace` mode writes from two threads (the turn's script and the answers
# to control requests); a line is never split between them.
SEND_LOCK = threading.Lock()


def send(message):
    with SEND_LOCK:
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
    default_model = {"value": "default", "displayName": "Default (recommended)", "description": ""}
    if EFFORT_LADDER:
        default_model["supportsEffort"] = True
        default_model["supportedEffortLevels"] = ["low", "medium", "high", "xhigh", "max"]
    return {
        "commands": [
            {"name": "usage", "description": "Show plan usage", "argumentHint": ""},
            {"name": "statusline", "description": "Configure the status line", "argumentHint": ""},
        ],
        "terminal_slash_commands": ["statusline"],
        "models": [
            default_model,
            {"value": "sonnet", "displayName": "Sonnet", "description": "Balanced"},
        ],
        "agents": [],
        "output_style": "default",
        "available_output_styles": ["default"],
        "current_permission_mode": "bypassPermissions" if START_BYPASS else "default",
        "fast_mode_state": "off",
        "fast_mode_disabled_reason": "preference" if START_BYPASS else "sdk_opt_in_required",
        "account": account,
        "hooks_applied": True,
    }


def init_line(version="2.1.273"):
    # The init list names a server and its state, never its source: that is
    # what the real CLI's `system/init` carries, and `mcp_status` is the only
    # place a connector's origin shows.
    mcp_servers = (
        [{"name": "codegraph", "status": "pending"}] if MODE == "session_panel" else []
    )
    send({
        "type": "system",
        "subtype": "init",
        "session_id": SESSION_ID,
        "claude_code_version": version,
        "cwd": os.getcwd(),
        "model": "claude-fable-5-1",
        "permissionMode": "bypassPermissions" if START_BYPASS else "default",
        "tools": ["Bash", "Read", "Edit"],
        "mcp_servers": mcp_servers,
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

# Session-panel fixture state: whether `mcp_reconnect` has brought the
# `codegraph` server up, which later `mcp_status` answers then report.
RECONNECTED = set()
# How many `mcp_status` requests have been answered so far.
STATUS_ANSWERS = 0


def control_error(request_id, error):
    send({
        "type": "control_response",
        "response": {"subtype": "error", "request_id": request_id, "error": error},
    })


def session_panel_context():
    """The shape Claude Code 2.1.304 answered `get_context_usage` with."""
    def category(cid, name, tokens, kind):
        return {"id": cid, "name": name, "tokens": tokens, "kind": kind}

    return {
        "totalTokens": 64923,
        "rawMaxTokens": 400000,
        "categories": [
            category("system_prompt", "System prompt", 2315, "used"),
            category("system_tools", "System tools", 9740, "used"),
            category("custom_agents", "Custom agents", 4947, "used"),
            category("memory_files", "Memory files", 34233, "used"),
            category("skills", "Skills", 7102, "used"),
            category("messages", "Messages", 6586, "used"),
            category("free_space", "Free space", 302077, "free"),
        ],
        "memoryFiles": [
            {"path": f"{REPO_PATH}/CLAUDE.md", "type": "Project", "tokens": 28941},
            {"path": "/home/u/.claude/CLAUDE.md", "type": "User", "tokens": 809},
        ],
        "skills": {"totalSkills": 1, "tokens": 7102, "skillFrontmatter": [
            {"name": "prompt-master", "source": "userSettings",
             "path": "/home/u/.claude/skills/prompt-master/SKILL.md", "tokens": 117},
        ]},
        "agents": [
            {"agentType": "plugin-dev:agent-creator", "source": "plugin",
             "pluginId": "plugin-dev@claude-plugins-official", "tokens": 431},
        ],
        "mcpTools": [],
    }


def session_panel_servers():
    if STARTS_AFTER is None:
        codegraph = "pending"
    elif STARTS_AFTER:
        codegraph = "pending" if STATUS_ANSWERS <= STARTS_AFTER else "connected"
    else:
        codegraph = "connected" if "codegraph" in RECONNECTED else "failed"
    return [
        {"name": "claude.ai Notion", "status": "needs-auth",
         "source": "claudeai", "scope": "claudeai"},
        {"name": "codegraph", "status": codegraph,
         "source": "user", "scope": "user"},
    ]


def assistant_tool_use(tool_use_id, tool_name, tool_input, parent=None):
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
        "parent_tool_use_id": parent,
        "session_id": SESSION_ID,
        "uuid": "perm-assistant-uuid",
    })


def ask_permission(cli_request_id, tool_name, tool_use_id, tool_input, suggestions=None,
                   agent_id=None, parent=None):
    """`agent_id` makes it a subagent's request, as the real CLI does; `parent`
    is the `parent_tool_use_id` its eventual result line carries."""
    PENDING[cli_request_id] = (tool_use_id, tool_name, parent)
    request = {
        "subtype": "can_use_tool",
        "tool_name": tool_name,
        "tool_use_id": tool_use_id,
        "input": tool_input,
        "permission_suggestions": suggestions if suggestions is not None else [],
    }
    if agent_id is not None:
        request["agent_id"] = agent_id
    send({
        "type": "control_request",
        "request_id": cli_request_id,
        "request": request,
    })


def finish_permission(cli_request_id, answer_message):
    pending = PENDING.pop(cli_request_id, None)
    if pending is None:
        return
    tool_use_id, _tool_name, parent = pending
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
        "parent_tool_use_id": parent,
        "session_id": SESSION_ID,
        "uuid": "perm-result-uuid",
    })
    result()


# ---------------------------------------------------------------------------
# `trace`: scripted turns for the trace waterfall's end-to-end test.
#
# The first word of a prompt picks the script (see TRACE_SCRIPTS). Line shapes
# copy the ones `claude` 2.1.306 wrote (rust/crates/sirio_claude/tests/fixtures/
# trace_*.json). Each step is paced by TRACE_STEP_MS (default 400), so a live
# state can be read mid-turn. A script runs on its own thread: the main loop
# keeps answering control requests (an interrupt, a context report) and records
# the answer to a permission the script is waiting for. The cost counter is the
# process's own and only ever grows, as the CLI's `total_cost_usd` does.
# ---------------------------------------------------------------------------

TRACE_MODEL = "claude-haiku-5-5"
TRACE_STEP = float(os.environ.get("TRACE_STEP_MS", "400")) / 1000.0
TRACE_COST = {"total": 0.0}
TRACE_COUNTER = {"n": 0}
TRACE_INTERRUPT = threading.Event()
TRACE_EOF = threading.Event()
TRACE_PROMPTS = queue.Queue()
TRACE_ANSWERS = {}
TRACE_ANSWERED = threading.Condition()


TRACE_UUID_LOCK = threading.Lock()


def trace_uuid():
    # A background agent's thread writes beside the turn's script.
    with TRACE_UUID_LOCK:
        TRACE_COUNTER["n"] += 1
        return f"00000000-0000-4000-8000-{TRACE_COUNTER['n']:012d}"


def pause(steps=1.0):
    time.sleep(TRACE_STEP * steps)


def trace_stream(event):
    send({
        "type": "stream_event",
        "event": event,
        "session_id": SESSION_ID,
        "parent_tool_use_id": None,
        "uuid": trace_uuid(),
    })


def requesting():
    """The line the CLI writes as a main-thread API request goes out, ahead of the
    first token (`message_start`) by however long the API takes to answer."""
    send({"type": "system", "subtype": "status", "status": "requesting",
          "session_id": SESSION_ID, "uuid": trace_uuid()})


def message_start(message_id):
    trace_stream({"type": "message_start", "message": {
        "model": TRACE_MODEL, "id": message_id, "type": "message", "role": "assistant",
        "content": [], "stop_reason": None,
        "usage": {"input_tokens": 2, "cache_creation_input_tokens": 0,
                  "cache_read_input_tokens": 1200, "output_tokens": 6},
    }})


def message_end(stop_reason="end_turn", output_tokens=120):
    trace_stream({"type": "message_delta", "delta": {"stop_reason": stop_reason},
                  "usage": {"input_tokens": 2, "cache_creation_input_tokens": 0,
                            "cache_read_input_tokens": 1200, "output_tokens": output_tokens}})
    trace_stream({"type": "message_stop"})


def tool_use(tool_use_id, name, tool_input):
    return {"type": "tool_use", "id": tool_use_id, "name": name, "input": tool_input,
            "caller": {"type": "direct"}}


def trace_assistant(message_id, block, parent=None, agent_id=None):
    """One content block of a model call. A subagent's line names its parent
    call and its agent; the main thread's names neither."""
    line = {
        "type": "assistant",
        "message": {"model": TRACE_MODEL, "id": message_id, "type": "message", "role": "assistant",
                    "content": [block], "stop_reason": None,
                    "usage": {"input_tokens": 2, "cache_creation_input_tokens": 0,
                              "cache_read_input_tokens": 1200, "output_tokens": 8}},
        "parent_tool_use_id": parent,
        "session_id": SESSION_ID,
        "uuid": trace_uuid(),
    }
    if agent_id is not None:
        line["agent_id"] = agent_id
    send(line)


def trace_result_line(tool_use_id, body, is_error=False, parent=None, totals=None):
    line = {
        "type": "user",
        "message": {"role": "user", "content": [{
            "type": "tool_result", "tool_use_id": tool_use_id,
            "content": [{"type": "text", "text": body}], "is_error": is_error,
        }]},
        "parent_tool_use_id": parent,
        "session_id": SESSION_ID,
        "uuid": trace_uuid(),
    }
    if totals is not None:
        line["tool_use_result"] = totals
    send(line)


def agent_totals(agent_id, duration_ms, tools_used):
    return {
        "status": "completed", "agentId": agent_id, "agentType": "general-purpose",
        "resolvedModel": TRACE_MODEL, "totalDurationMs": duration_ms,
        "totalTokens": 26197, "totalToolUseCount": tools_used,
        "usage": {"input_tokens": 2, "cache_creation_input_tokens": 141,
                  "cache_read_input_tokens": 25867, "output_tokens": 187},
    }


def task_started(task_id, tool_use_id, description, subagent_type, depth):
    send({"type": "system", "subtype": "task_started", "task_id": task_id,
          "run_id": f"run-{task_id}", "tool_use_id": tool_use_id,
          "description": description, "subagent_type": subagent_type,
          "is_backgrounded": False, "spawn_depth": depth, "task_type": "local_agent",
          "prompt": description, "uuid": trace_uuid(), "session_id": SESSION_ID})


def task_progress(task_id, tool_use_id, description, tokens, tools_used, last_tool, duration_ms):
    send({"type": "system", "subtype": "task_progress", "task_id": task_id,
          "run_id": f"run-{task_id}", "tool_use_id": tool_use_id,
          "description": description, "last_tool_name": last_tool,
          "usage": {"total_tokens": tokens, "tool_uses": tools_used, "duration_ms": duration_ms},
          "uuid": trace_uuid(), "session_id": SESSION_ID})


def trace_ask(cli_request_id, tool_name, tool_use_id, tool_input, agent_id=None):
    request = {"subtype": "can_use_tool", "tool_name": tool_name, "display_name": tool_name,
               "input": tool_input, "tool_use_id": tool_use_id,
               "permission_suggestions": [{"type": "addRules", "behavior": "allow",
                                           "destination": "session",
                                           "rules": [{"toolName": tool_name}]}]}
    if agent_id is not None:
        request["agent_id"] = agent_id
    send({"type": "control_request", "request_id": cli_request_id, "request": request})


def wait_answer(cli_request_id):
    """What the user answered: `allow` or `deny`. A closed stdin answers deny,
    so a piped self-check runs the whole script."""
    with TRACE_ANSWERED:
        while cli_request_id not in TRACE_ANSWERS and not TRACE_EOF.is_set():
            TRACE_ANSWERED.wait(0.2)
        return TRACE_ANSWERS.get(cli_request_id, "deny")


def trace_result(cost, started, interrupted=False):
    TRACE_COST["total"] = round(TRACE_COST["total"] + cost, 6)
    usage = {"claude-haiku-5-5": {
        "inputTokens": 2, "outputTokens": 400, "cacheReadInputTokens": 9000,
        "cacheCreationInputTokens": 300, "costUSD": TRACE_COST["total"],
        "contextWindow": 200000}}
    body = {
        "type": "result",
        "subtype": "error_during_execution" if interrupted else "success",
        "is_error": interrupted,
        "duration_ms": int((time.time() - started) * 1000),
        "num_turns": 1,
        "result": "Interrupted by user" if interrupted else "Done.",
        "stop_reason": None if interrupted else "end_turn",
        "total_cost_usd": TRACE_COST["total"],
        "modelUsage": usage,
        "permission_denials": [],
        "session_id": SESSION_ID,
        "uuid": trace_uuid(),
    }
    if interrupted:
        body["terminal_reason"] = "aborted_streaming"
    send(body)


def summary_text():
    # Long enough that the rows above it can be scrolled to the top.
    return "".join(f"{n}. A line of the summary the model writes at the end.\n\n" for n in range(1, 61))


def preamble_text():
    # Long enough that the transcript is taller than a window before the turn's first call:
    # with the list at the user's message, the card of a permission asked later is below the
    # last row the list draws, so a reveal of it has somewhere to move the list to.
    return "".join(f"Step {n} of the plan the model lays out before it starts.\n\n" for n in range(1, 31))


def script_subagents():
    started = time.time()
    # One model call, streamed, that calls two agents at once.
    message_start("msg_main_1")
    text("I will split this across two agents. ")
    text(preamble_text())
    pause()
    # The model writes between its two calls, as it does: the two Agent rows are
    # not one run of tool rows, so each lane reveals a row of its own.
    for agent, kind, description, remark in (
        ("toolu_agent_a", "Explore", "Survey the sources", "An explorer first. "),
        ("toolu_agent_b", "Plan", "Draft the plan", "And a planner. "),
    ):
        text(remark)
        trace_assistant("msg_main_1", tool_use(agent, "Agent", {
            "description": description, "subagent_type": kind, "prompt": description}))
    message_end("tool_use", 90)
    task_started("task-a", "toolu_agent_a", "Survey the sources", "Explore", 1)
    task_started("task-b", "toolu_agent_b", "Draft the plan", "Plan", 1)
    pause()
    # Both agents work; the second calls a third.
    trace_assistant("msg_a_1", tool_use("toolu_a_bash", "Bash", {
        "command": "ls", "description": "List the sources"}),
        parent="toolu_agent_a", agent_id="task-a")
    trace_assistant("msg_b_1", tool_use("toolu_agent_c", "Agent", {
        "description": "Check the tests", "subagent_type": "general-purpose",
        "prompt": "Check the tests"}),
        parent="toolu_agent_b", agent_id="task-b")
    task_started("task-c", "toolu_agent_c", "Check the tests", "general-purpose", 2)
    pause()
    task_progress("task-a", "toolu_agent_a", "Running List the sources", 19991, 1, "Bash", 1892)
    trace_assistant("msg_c_1", tool_use("toolu_c_read", "Read", {"file_path": "/repo/src/lib.rs"}),
                    parent="toolu_agent_c", agent_id="task-c")
    pause()
    trace_result_line("toolu_a_bash", "README.md\nsrc", parent="toolu_agent_a")
    trace_result_line("toolu_c_read", "fn main() {}", parent="toolu_agent_c")
    pause()
    trace_result_line("toolu_agent_c", "The tests are in place.", parent="toolu_agent_b",
                      totals=agent_totals("task-c", 1800, 1))
    pause()
    trace_result_line("toolu_agent_a", "README.md and src", totals=agent_totals("task-a", 2600, 1))
    trace_result_line("toolu_agent_b", "A two step plan.", totals=agent_totals("task-b", 3100, 1))
    pause()
    # The second model call edits a file, and asks first.
    message_start("msg_main_2")
    text("Both agents are back. Applying the fix. ")
    trace_assistant("msg_main_2", tool_use("toolu_edit", "Edit", {
        "file_path": "/repo/src/main.rs", "old_string": "fn main() {}",
        "new_string": "fn main() { run(); }"}))
    message_end("tool_use", 60)
    trace_ask("cli-edit-1", "Edit", "toolu_edit", {
        "file_path": "/repo/src/main.rs", "old_string": "fn main() {}",
        "new_string": "fn main() { run(); }"})
    allowed = wait_answer("cli-edit-1") == "allow"
    trace_result_line("toolu_edit", "edited" if allowed else "denied", is_error=not allowed)
    pause()
    # A Bash that fails, then one that works.
    message_start("msg_main_3")
    trace_assistant("msg_main_3", tool_use("toolu_bash_fail", "Bash", {
        "command": "make test", "description": "Run the tests"}))
    message_end("tool_use", 40)
    pause()
    trace_result_line("toolu_bash_fail", "make: *** No rule to make target 'test'.  Stop.",
                      is_error=True)
    message_start("msg_main_4")
    trace_assistant("msg_main_4", tool_use("toolu_bash_ok", "Bash", {
        "command": "make check", "description": "Run the checks"}))
    message_end("tool_use", 40)
    pause()
    trace_result_line("toolu_bash_ok", "all checks passed")
    message_start("msg_main_5")
    text(summary_text())
    message_end("end_turn", 300)
    pause()
    trace_result(0.31, started)


def script_second():
    started = time.time()
    message_start("msg_second_1")
    text("A second turn. ")
    pause()
    message_end("end_turn", 20)
    trace_result(0.12, started)


def script_request():
    started = time.time()
    # The request is out and the first token is not: the wait belongs to the call.
    requesting()
    pause(4)
    message_start("msg_request_1")
    text("The first token took a while. ")
    pause()
    message_end("end_turn", 20)
    trace_result(0.04, started)


def script_cancel():
    started = time.time()
    message_start("msg_cancel_1")
    text("Working on it. ")
    # Mid-call until the interrupt; the CLI then stops the call and reports it.
    while not TRACE_INTERRUPT.wait(0.2) and not TRACE_EOF.is_set():
        pass
    message_end("end_turn", 4)
    trace_result(0.02, started, interrupted=True)


def script_hang():
    started = time.time()
    message_start("msg_hang_1")
    trace_assistant("msg_hang_1", tool_use("toolu_hang", "Bash", {
        "command": "sleep 3600", "description": "A call that never reports"}))
    message_end("tool_use", 30)
    pause(3)
    trace_result(0.05, started)


def script_die():
    message_start("msg_die_1")
    text("Starting. ")
    trace_assistant("msg_die_1", tool_use("toolu_agent_d", "Agent", {
        "description": "Work that never ends", "subagent_type": "Explore",
        "prompt": "Work that never ends"}))
    message_end("tool_use", 30)
    task_started("task-d", "toolu_agent_d", "Work that never ends", "Explore", 1)
    pause(2)
    sys.stderr.write("fixture agent: fatal error, going away\n")
    sys.stderr.flush()
    os._exit(3)


def script_subagent_permission(early):
    started = time.time()
    agent, task, tool = ("toolu_agent_pe", "task-pe", "toolu_sub_pe") if early \
        else ("toolu_agent_p", "task-p", "toolu_sub_p")
    tool_input = {"command": "rm -rf build", "description": "Remove build"}
    message_start("msg_sp_1" + ("e" if early else ""))
    text("Delegating a cleanup. ")
    trace_assistant("msg_sp_1" + ("e" if early else ""), tool_use(agent, "Agent", {
        "description": "Clean the build", "subagent_type": "general-purpose",
        "prompt": "Clean the build"}))
    message_end("tool_use", 40)
    task_started(task, agent, "Clean the build", "general-purpose", 1)
    pause()
    if early:
        # The request beats the line that holds its call, the order seen live.
        trace_ask("cli-sub-" + task, "Bash", tool, tool_input, agent_id=task)
        pause()
        trace_assistant("msg_sub_" + task, tool_use(tool, "Bash", tool_input),
                        parent=agent, agent_id=task)
    else:
        trace_assistant("msg_sub_" + task, tool_use(tool, "Bash", tool_input),
                        parent=agent, agent_id=task)
        pause()
        trace_ask("cli-sub-" + task, "Bash", tool, tool_input, agent_id=task)
    allowed = wait_answer("cli-sub-" + task) == "allow"
    trace_result_line(tool, "ran" if allowed else "denied", is_error=not allowed, parent=agent)
    pause()
    trace_result_line(agent, "Cleaned." if allowed else "The cleanup was refused.",
                      totals=agent_totals(task, 1500, 1))
    message_start("msg_sp_2" + ("e" if early else ""))
    text("Finished. ")
    message_end("end_turn", 10)
    trace_result(0.07, started)


# A background agent: `task_started` says `is_backgrounded`, the `Agent` result is only its
# launch (`async_launched`), and its lines keep coming after the main thread's `result`,
# until `task_updated` and `task_notification` end it (the lines `claude` 2.1.309 wrote).
# The scripts hold between those steps for TRACE_BACKGROUND_HOLD steps of TRACE_STEP_MS,
# long enough for a check to read each state.
TRACE_BACKGROUND_HOLD = 20
# Set once the prompt after `bgprompt` has been answered: its agent ends then.
TRACE_NEXT_ANSWERED = threading.Event()


def task_started_background(task_id, tool_use_id, description):
    send({"type": "system", "subtype": "task_started", "task_id": task_id,
          "run_id": f"run-{task_id}", "tool_use_id": tool_use_id,
          "description": description, "subagent_type": "fork",
          "is_backgrounded": True, "spawn_depth": 1, "task_type": "local_agent",
          "prompt": description, "uuid": trace_uuid(), "session_id": SESSION_ID})


def async_launched(tool_use_id, task_id):
    trace_result_line(tool_use_id, "Async agent launched successfully.", totals={
        "isAsync": True, "status": "async_launched", "agentId": task_id,
        "description": "A background agent", "prompt": "Work in the background",
        "resolvedModel": TRACE_MODEL})


def task_ended(task_id, tool_use_id, duration_ms):
    send({"type": "system", "subtype": "task_updated", "task_id": task_id,
          "patch": {"status": "completed", "end_time": int(time.time() * 1000)},
          "uuid": trace_uuid(), "session_id": SESSION_ID})
    send({"type": "system", "subtype": "task_notification", "task_id": task_id,
          "tool_use_id": tool_use_id, "status": "completed",
          "summary": "Agent finished the work", "output_file": f"/tmp/{task_id}.output",
          "usage": {"total_tokens": 26197, "tool_uses": 2, "duration_ms": duration_ms},
          "uuid": trace_uuid(), "session_id": SESSION_ID})


def background_work(agent, task, remark):
    """One step of a background agent's own work: a Bash call it makes and finishes."""
    tool = f"{agent}_{remark}"
    trace_assistant(f"msg_{tool}", tool_use(tool, "Bash", {
        "command": "ls", "description": "List the files"}), parent=agent, agent_id=task)
    pause()
    trace_result_line(tool, "README.md\nsrc", parent=agent)
    task_progress(task, agent, "Working in the background", 9000, 1, "Bash", 3000)


def cli_started_turn(message_id, cost):
    """The turn the CLI starts on its own to react to a notification: no prompt of
    Sirio's, its own cumulative cost."""
    started = time.time()
    init_line()
    requesting()
    pause()
    message_start(message_id)
    text("The agent finished. ")
    message_end("end_turn", 20)
    trace_result(cost, started)


def script_background():
    started = time.time()
    requesting()
    pause()
    message_start("msg_bg_1")
    text("I will start two agents in the background. ")
    for agent, description in (("toolu_bg_a", "Survey the sources"),
                               ("toolu_bg_b", "Draft the plan")):
        trace_assistant("msg_bg_1", tool_use(agent, "Agent", {
            "description": description, "subagent_type": "fork", "prompt": description,
            "run_in_background": True}))
    message_end("tool_use", 90)
    task_started_background("task-bg-a", "toolu_bg_a", "Survey the sources")
    task_started_background("task-bg-b", "toolu_bg_b", "Draft the plan")
    pause()
    async_launched("toolu_bg_a", "task-bg-a")
    async_launched("toolu_bg_b", "task-bg-b")
    # The main thread goes on: a second call writes the answer and the turn ends, while
    # both agents work.
    requesting()
    pause()
    message_start("msg_bg_2")
    text("Both agents are running. ")
    message_end("end_turn", 30)
    trace_result(1.00, started)
    # What the agents do after the result.
    pause()
    background_work("toolu_bg_a", "task-bg-a", "bash")
    background_work("toolu_bg_b", "task-bg-b", "bash")
    for agent, task in (("toolu_bg_a", "task-bg-a"), ("toolu_bg_b", "task-bg-b")):
        trace_assistant(f"msg_{agent}_read", tool_use(f"{agent}_read", "Read", {
            "file_path": "/repo/src/lib.rs"}), parent=agent, agent_id=task)
    # Both agents run, with children, for the hold: the turn reads Live and its cost "at least".
    pause(TRACE_BACKGROUND_HOLD)
    # Agent A ends; the CLI reacts in a turn of its own ($0.20) while B still runs.
    task_ended("task-bg-a", "toolu_bg_a", 6000)
    pause()
    cli_started_turn("msg_react_a", 0.20)
    pause(TRACE_BACKGROUND_HOLD)
    # Agent B ends: the turn is Done, its cost still "at least" until the CLI reacts.
    task_ended("task-bg-b", "toolu_bg_b", 9000)
    pause(TRACE_BACKGROUND_HOLD)
    cli_started_turn("msg_react_b", 0.10)


def script_bgprompt():
    started = time.time()
    requesting()
    pause()
    message_start("msg_bp_1")
    text("I will start an agent in the background. ")
    trace_assistant("msg_bp_1", tool_use("toolu_bp", "Agent", {
        "description": "Work on its own", "subagent_type": "fork", "prompt": "Work on its own",
        "run_in_background": True}))
    message_end("tool_use", 40)
    task_started_background("task-bp", "toolu_bp", "Work on its own")
    pause()
    async_launched("toolu_bp", "task-bp")
    requesting()
    pause()
    message_start("msg_bp_2")
    text("The agent is running. ")
    message_end("end_turn", 20)
    trace_result(1.00, started)
    TRACE_NEXT_ANSWERED.clear()
    threading.Thread(target=bgprompt_agent, daemon=True).start()


def bgprompt_agent():
    """The agent keeps working, in a thread of its own, until the prompt after
    `bgprompt` has been answered and read; then it ends, with no reaction turn."""
    step = 0
    while not TRACE_NEXT_ANSWERED.is_set() and not TRACE_EOF.is_set():
        step += 1
        background_work("toolu_bp", "task-bp", f"step{step}")
    if TRACE_EOF.is_set():
        return
    pause(TRACE_BACKGROUND_HOLD)
    task_ended("task-bp", "toolu_bp", 12000)


TRACE_SCRIPTS = {
    "subagents": script_subagents,
    "second": script_second,
    "request": script_request,
    "cancel": script_cancel,
    "hang": script_hang,
    "die": script_die,
    "subperm": lambda: script_subagent_permission(False),
    "subperm_early": lambda: script_subagent_permission(True),
    "background": script_background,
    "bgprompt": script_bgprompt,
}


def script_other():
    started = time.time()
    message_start("msg_other_1")
    text("Hello. ")
    message_end("end_turn", 5)
    trace_result(0.01, started)


def trace_turns():
    while True:
        prompt = TRACE_PROMPTS.get()
        if prompt is None:
            return
        TRACE_INTERRUPT.clear()
        first = prompt.split(None, 1)[0].lower() if prompt.split() else ""
        TRACE_SCRIPTS.get(first, script_other)()
        if first != "bgprompt":
            TRACE_NEXT_ANSWERED.set()


def trace_prompt_text(message):
    content = message.get("message", {}).get("content", "")
    if isinstance(content, list):
        content = " ".join(block.get("text", "") for block in content if isinstance(block, dict))
    return content if isinstance(content, str) else ""


def trace_main():
    init_line()
    turns = threading.Thread(target=trace_turns, daemon=True)
    turns.start()
    for line in sys.stdin:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        kind = message.get("type")
        if kind == "control_request":
            request = message.get("request", {})
            subtype = request.get("subtype")
            request_id = message.get("request_id")
            if subtype == "initialize":
                control_response(request_id, initialize_payload())
            elif subtype == "interrupt":
                control_response(request_id, {"still_queued": 0})
                TRACE_INTERRUPT.set()
            elif subtype == "get_context_usage":
                control_response(request_id, {
                    "totalTokens": 48000, "rawMaxTokens": 200000, "maxTokens": 180000,
                    "percentage": 24, "model": TRACE_MODEL, "categories": []})
            elif subtype == "mcp_status":
                control_response(request_id, {"mcpServers": []})
            elif subtype == "set_permission_mode":
                control_response(request_id, {"mode": request.get("mode")})
            else:
                control_response(request_id, {})
        elif kind == "control_response":
            response = message.get("response", {})
            payload = response.get("response") or {}
            with TRACE_ANSWERED:
                TRACE_ANSWERS[response.get("request_id")] = (
                    "allow" if payload.get("behavior") == "allow" else "deny")
                TRACE_ANSWERED.notify_all()
        elif kind == "user":
            TRACE_PROMPTS.put(trace_prompt_text(message))
    # stdin closed: the script in flight runs on to its end, then the process.
    TRACE_EOF.set()
    TRACE_PROMPTS.put(None)
    turns.join()


def main():
    if MODE == "trace":
        trace_main()
        return
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
    if MODE == "session_panel":
        # The same unprompted init, carrying the server list the panel
        # shows until an `mcp_status` answer replaces it.
        init_line()
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
            if CONTROL_LOG_PATH:
                with open(CONTROL_LOG_PATH, "a", encoding="utf-8") as log:
                    log.write(json.dumps(request, separators=(",", ":")) + "\n")
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
                elif REFUSE_EFFORT and "effortLevel" in request.get("settings", {}):
                    control_error(request_id, "Effort level not available for this session")
                else:
                    control_response(request_id, {})
            elif subtype == "get_context_usage":
                if MODE == "session_panel":
                    control_response(request_id, session_panel_context())
                elif MODE == "no_context_usage":
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
            elif subtype == "mcp_status":
                # A file named by the environment counts the requests, one
                # line each, so a test can tell a poll that stopped.
                if COUNT_PATH:
                    with open(COUNT_PATH, "a") as count:
                        count.write("mcp_status\n")
                global STATUS_ANSWERS
                STATUS_ANSWERS += 1
                control_response(request_id, {"mcpServers": session_panel_servers()})
            elif subtype == "mcp_reconnect":
                name = request.get("serverName")
                if REFUSE_RECONNECT and name == "codegraph":
                    control_error(
                        request_id,
                        "Connection refused: codegraph did not start (exit 1)",
                    )
                elif name == "codegraph":
                    RECONNECTED.add(name)
                    control_response(request_id, {})
                else:
                    control_error(request_id, f"Server not found: {name}")
            elif subtype == "mcp_authenticate":
                control_response(request_id, {
                    "authUrl": "http://127.0.0.1:9/authorize",
                    "requiresUserAction": True,
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
            if MODE in ("context_usage", "no_context_usage", "session_panel"):
                init_line()
                text("Hello")
                result()
                continue
            if MODE == "file_links":
                # One answer to any prompt, whose paths and links the chat's file
                # links E2E clicks. The first path is cut in two chunks, as a
                # stream delivers it.
                init_line()
                text("Ho creato il file, l'ho messo in /docs/NOME-")
                text("FILE.md.\n"
                     "Il punto da guardare è `src/main.rs:42`, e c'è anche README.md.\n")
                text("La sezione è in docs/NOME-FILE.md:30, e l'altra nota in docs/ALTRO.md:10.\n"
                     "Non esiste: missing/nowhere.md. Versione 1.2.3. "
                     "Link web: [sito](https://example.com/docs/NOME-FILE.md).\n\n"
                     "```text\nsrc/main.rs\n```\n")
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
            if MODE == "subagent_permission_early":
                # A subagent asks before the line that holds its call arrives,
                # the order seen live. Its own lines carry the parent's id.
                init_line()
                tool_input = {"command": "rm -rf build", "description": "Remove build"}
                ask_permission(
                    "cli-sub-1", "Bash", "toolu_sub_1", tool_input,
                    agent_id="task-sub-1", parent="toolu_agent_1",
                )
                assistant_tool_use("toolu_sub_1", "Bash", tool_input, parent="toolu_agent_1")
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
