#!/usr/bin/env python3
"""A deterministic fake `claude` speaking stream-json, for sirio_chat's and
sirio_ui's tests and the E2E stubs.

It is the native twin of `acp_fixture.py`: same shape, same argv contract
(mode first, optional pid file second), different protocol. Nothing here
talks to a network or reads the user's configuration.
"""

import json
import os
import unicodedata
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
START_BYPASS = MODE == "session_panel_bypass" or "--fixture-start-bypass" in sys.argv
if REFUSE_RECONNECT or MODE in ("session_panel_starting", "session_panel_pending", "session_panel_bypass"):
    MODE = "session_panel"
# `effort_ladder` gives the session's first model an effort ladder, as the
# real CLI's handshake does; `effort_refused` is the same with every change of
# `effortLevel` refused. No other mode advertises one: a chat that finds an
# effort selector asks for High at once, and that is not what the rest of the
# suite is about.
EFFORT_LADDER = MODE in ("effort_ladder", "effort_refused") or "--fixture-effort-ladder" in sys.argv
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
# `elicitation` mode: a launch argument names the file the host's answers to
# the fixture's own requests are appended to, verbatim, one JSON line each, and
# every prompt as `{"prompt": <text>}`. A launch argument, not the environment,
# so tests running side by side each keep their own log.
RESPONSE_LOG_PATH = next(
    (arg[len("--fixture-response-log="):] for arg in sys.argv if arg.startswith("--fixture-response-log=")),
    None,
)
# A launch argument names the file each process appends one JSON line to at
# startup, `{"resume": <the --resume id or null>, "session_id": <the
# --session-id argument or null>}`: the session every launch and relaunch
# asked for, as the CLI heard it.
LAUNCH_LOG_PATH = next(
    (arg[len("--fixture-launch-log="):] for arg in sys.argv if arg.startswith("--fixture-launch-log=")),
    None,
)
# The file every `user` line (a prompt, as the CLI heard it) is appended to,
# one JSON line each, before the turn runs: what a mention became on the wire.
# A launch argument names it, so tests running side by side each keep their own;
# the environment names it otherwise.
USER_LOG_PATH = next(
    (arg[len("--fixture-user-log="):] for arg in sys.argv if arg.startswith("--fixture-user-log=")),
    os.environ.get("SIRIO_FIXTURE_USER_LOG"),
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


def flag_value(name):
    """The value of `--<name>=<value>`; an empty value counts as absent."""
    prefix = f"--{name}="
    value = next((arg[len(prefix):] for arg in sys.argv if arg.startswith(prefix)), "")
    return value or None


def argv_value(name):
    """The argument after `<name>`, or None when absent or empty."""
    if name not in sys.argv:
        return None
    at = sys.argv.index(name)
    return (sys.argv[at + 1] if at + 1 < len(sys.argv) else "") or None


# `rename_session`: `--fixture-rename-error=<text>` answers every rename with
# that error; `--fixture-title-store=<path>` is a JSON object
# `{session_id: title}` read at start and rewritten on each accepted rename;
# `--fixture-handshake-gate=<path>` holds the `initialize` answer of a session
# launch (`--session-id` or `--resume`, never the usage probe) until the file
# exists; `--fixture-rename-gate=<path>` holds the answer (and the echo) of every
# rename, in order, until the file exists, while later requests are still read.
# These flags apply only to the main-loop modes: the `trace`,
# `session_state` and `background_tasks` modes answer a rename with their
# default `success {}` (no echo, no store, no error, no gate).
RENAME_ERROR = flag_value("fixture-rename-error")
TITLE_STORE_PATH = flag_value("fixture-title-store")
HANDSHAKE_GATE_PATH = flag_value("fixture-handshake-gate")
RENAME_GATE_PATH = flag_value("fixture-rename-gate")
HELD_RENAMES = []
HELD_RENAMES_LOCK = threading.Lock()
# True from the first held rename until the release thread has answered them all.
RENAMES_DRAINING = False
# `list_permission_rules`, in all four control loops. Without
# `--fixture-permission-rules=<path>` the fixture refuses it as the 2.1.257 floor
# does; with it, the file's JSON object is the success payload, re-read on each
# request. `--fixture-rules-log=<path>` gets one `{"t": <monotonic seconds>}`
# line per request received; `--fixture-rules-gate=<path>` holds every answer,
# in order, until the file exists (as `--fixture-rename-gate` does).
# `--fixture-permission-fields=<path>` is a JSON object merged into every
# `can_use_tool`, re-read on each use; `--fixture-result-gate=<path>` holds the
# `result` that ends a `permission`-mode turn until the file exists.
PERMISSION_RULES_PATH = flag_value("fixture-permission-rules")
RULES_LOG_PATH = flag_value("fixture-rules-log")
RULES_GATE_PATH = flag_value("fixture-rules-gate")
PERMISSION_FIELDS_PATH = flag_value("fixture-permission-fields")
RESULT_GATE_PATH = flag_value("fixture-result-gate")
# `get_hooks_listing`, in all four control loops. Without
# `--fixture-hooks=<path>` every loop refuses it as an unsupported subtype (never
# the catch-all success `{}`); with it, the file's JSON is the success payload,
# verbatim, re-read on each request. `--fixture-hooks-gate=<path>` holds every
# answer, in order, until the file exists (as `--fixture-rules-gate` does). Every
# request is in the control log like any other.
HOOKS_PATH = flag_value("fixture-hooks")
HOOKS_GATE_PATH = flag_value("fixture-hooks-gate")
HELD_HOOKS = []
HELD_HOOKS_LOCK = threading.Lock()
# True from the first held hooks request until the release thread has answered them all.
HOOKS_DRAINING = False
# The connector switch and the reloads, in the main-loop modes only; the other
# three loops refuse `mcp_toggle`, `reload_plugins` and `reload_skills` as
# unsupported subtypes. `--fixture-floor` (the flag `background_tasks` and the
# trace mode `subagent_text` read too) is the 2.1.257 floor: no `capabilities` in
# the handshake, no `held` and no `refuse_if_busy` handling; in `subagent_text`
# no line and no ask carries `agent_id`. `--fixture-reload=<kind>` picks the
# `reload_plugins` answer (apply, held, busy, busy-text, refused, errors);
# `--fixture-toggle=<kind>` makes every `mcp_toggle` fail (`refused` answers an
# unsupported subtype, `missing` a server not found), except `unread`: the toggle
# applies, and the first `mcp_status` after it answers a control error;
# `--fixture-reload-gate`
# and `--fixture-toggle-gate` hold each answer until the file exists, checked
# when the request arrives, while later requests are still read;
# `--fixture-cli-busy-gate=<path>` has the CLI report a running turn after the
# handshake and an idle one once the file exists.
RELOAD_KIND = flag_value("fixture-reload") or "apply"
RELOAD_GATE_PATH = flag_value("fixture-reload-gate")
TOGGLE_KIND = flag_value("fixture-toggle")
TOGGLE_GATE_PATH = flag_value("fixture-toggle-gate")
CLI_BUSY_GATE_PATH = flag_value("fixture-cli-busy-gate")
# `--fixture-skills=refused` answers `reload_skills` as an unsupported subtype.
SKILLS_KIND = flag_value("fixture-skills")
HELD_RULES = []
HELD_RULES_LOCK = threading.Lock()
# True from the first held rules request until the release thread has answered them all.
RULES_DRAINING = False
# Set by `init_line`: from then on the CLI's current session is SESSION_ID.
INIT_PRINTED = False

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


def main_initialize_payload():
    """The handshake answer of a main-loop mode: the floor says no capabilities."""
    payload = initialize_payload()
    if not BG_FLOOR:
        payload["capabilities"] = ["reload_plugins_refuse_if_busy_v1"]
    return payload


def init_line(version="2.1.273"):
    # The init list names a server and its state, never its source: that is
    # what the real CLI's `system/init` carries, and `mcp_status` is the only
    # place a connector's origin shows.
    global INIT_PRINTED
    INIT_PRINTED = True
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
        **mcp_apps_init_fields(),
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


def result(subtype="success", is_error=False, text_body="Done.", queued=0, stop_reason=None, **extra):
    """A turn's `result`. `queued` is its `queued_turn_count`, which the real CLI always writes:
    the user's sends it has read and not yet run. This fixture answers a prompt before it reads
    the next, so that is 0 unless a mode says otherwise; `None` leaves the field out. `stop_reason`
    is the result's own (`max_tokens`, say); left out, it is `end_turn`, or `cancelled` for an error."""
    if queued is not None:
        extra = {**extra, "queued_turn_count": queued}
    send({
        **extra,
        "type": "result",
        "subtype": subtype,
        "is_error": is_error,
        "result": text_body,
        "stop_reason": stop_reason or ("end_turn" if not is_error else "cancelled"),
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


# The connector switch and the reloads: which servers `mcp_toggle` has turned
# off, which it has turned on (their next status read says `pending`, as a
# server starting again does), whether a reload has applied the plugin, and the
# slash list the CLI would send now.
PANEL_LOCK = threading.Lock()
DISABLED = set()
RESTARTING = set()
PLUGIN_APPLIED = False
PLUGIN_READ = False
CURRENT_COMMANDS = []
PLUGIN_SERVER = "plugin:pa:m"
RLO = "\u202e"
# A plugin-authored name with a bidi override, listed once the plugin applies.
PLUGIN_RLO_SERVER = "plugin:pa:" + RLO + "x"
# `--fixture-toggle=unread`: set by an applied toggle, spent by the next `mcp_status`.
UNREAD_NEXT = [False]


def session_panel_servers():
    global PLUGIN_READ
    if STARTS_AFTER is None:
        codegraph = "pending"
    elif STARTS_AFTER:
        codegraph = "pending" if STATUS_ANSWERS <= STARTS_AFTER else "connected"
    else:
        codegraph = "connected" if "codegraph" in RECONNECTED else "failed"
    servers = [
        {"name": "claude.ai Notion", "status": "needs-auth",
         "source": "claudeai", "scope": "claudeai"},
        {"name": "codegraph", "status": codegraph,
         "source": "user", "scope": "user"},
    ]
    with PANEL_LOCK:
        if PLUGIN_APPLIED:
            servers.append({"name": PLUGIN_SERVER,
                            "status": "connected" if PLUGIN_READ else "pending",
                            "source": "plugin", "scope": "plugin"})
            servers.append({"name": PLUGIN_RLO_SERVER,
                            "status": "connected",
                            "source": "plugin", "scope": "plugin"})
            PLUGIN_READ = True
        shown = []
        for server in servers:
            name = server["name"]
            if name in DISABLED:
                server = {**server, "status": "disabled", "tools": []}
            elif name in RESTARTING:
                RESTARTING.discard(name)
                server = {**server, "status": "pending"}
            shown.append(server)
    return shown


def control_ok(request_id):
    """A success with no body, as the real CLI answers `mcp_toggle`."""
    send({
        "type": "control_response",
        "response": {"subtype": "success", "request_id": request_id},
    })


def control_error_code(request_id, error, code=None):
    response = {"subtype": "error", "request_id": request_id, "error": error}
    if code:
        response["error_code"] = code
    send({"type": "control_response", "response": response})


def toggle_when_gated(request_id, request):
    while not os.path.exists(TOGGLE_GATE_PATH):
        time.sleep(0.02)
    answer_toggle(request_id, request)


def answer_toggle(request_id, request):
    name = request.get("serverName")
    if TOGGLE_KIND == "refused":
        control_error(request_id, "Unsupported control request subtype: mcp_toggle")
        return
    if not isinstance(request.get("enabled"), bool):
        control_error(request_id, "mcp_toggle: enabled must be a boolean")
        return
    with PANEL_LOCK:
        known = ["claude.ai Notion", "codegraph"] + ([PLUGIN_SERVER, PLUGIN_RLO_SERVER] if PLUGIN_APPLIED else [])
        if TOGGLE_KIND == "missing" or name not in known:
            control_error(request_id, f"Server not found: {name}")
            return
        if request.get("enabled") is True:
            DISABLED.discard(name)
            RESTARTING.add(name)
        else:
            DISABLED.add(name)
        if TOGGLE_KIND == "unread":
            UNREAD_NEXT[0] = True
    control_ok(request_id)


def reload_when_gated(request_id, request):
    while not os.path.exists(RELOAD_GATE_PATH):
        time.sleep(0.02)
    answer_reload_plugins(request_id, request)


def answer_reload_plugins(request_id, request):
    global PLUGIN_APPLIED
    hold = request.get("hold_on_cache_impact") is True and not BG_FLOOR
    refuse_if_busy = request.get("refuse_if_busy") is True and not BG_FLOOR
    busy_text = "The session is busy, so plugins were not reloaded. Ask again once it is idle."
    if RELOAD_KIND == "refused":
        # The floor sends the refusal text and no code (FINDINGS Q4).
        control_error_code(request_id,
                           "Unsupported control request subtype: reload_plugins",
                           None if BG_FLOOR else "unsupported_request")
        return
    if RELOAD_KIND == "busy-text":
        control_error_code(request_id, busy_text)
        return
    if RELOAD_KIND == "busy" and refuse_if_busy:
        control_error_code(request_id, busy_text, "reload_plugins_busy")
        return
    with PANEL_LOCK:
        if RELOAD_KIND == "held" and hold:
            control_response(request_id, {
                "commands": list(CURRENT_COMMANDS), "agents": [], "plugins": [],
                "mcpServers": [], "error_count": 0, "held": True,
                "cache_impact": {
                    "mcp_servers_added": [PLUGIN_SERVER, "plugin:pa:" + RLO + "x"],
                    "mcp_servers_removed": ["old-server"],
                    "lsp_tool_change": "may-add",
                },
            })
            return
        if not any(command["name"] == "pa:hello" for command in CURRENT_COMMANDS):
            CURRENT_COMMANDS.append({"name": "pa:hello",
                                     "description": "probe plugin command",
                                     "argumentHint": ""})
        PLUGIN_APPLIED = True
        payload = {
            "commands": list(CURRENT_COMMANDS), "agents": [],
            "plugins": [{"name": "pa"}], "mcpServers": [],
            "error_count": 2 if RELOAD_KIND == "errors" else 0,
        }
        if hold:
            payload["held"] = False
    control_response(request_id, payload)


def answer_reload_skills(request_id):
    if SKILLS_KIND == "refused":
        control_error(request_id, "Unsupported control request subtype: reload_skills")
        return
    with PANEL_LOCK:
        added = not any(command["name"] == "lateskill" for command in CURRENT_COMMANDS)
        if added:
            CURRENT_COMMANDS.append({"name": "lateskill", "description": "late skill",
                                     "argumentHint": ""})
        commands = list(CURRENT_COMMANDS)
    control_response(request_id, {"skills": [{"name": "lateskill",
                                              "description": "late skill"}]})
    # The CLI sends the full list only when it changed.
    if added:
        send({"type": "system", "subtype": "commands_changed", "commands": commands,
              "session_id": SESSION_ID, "uuid": trace_uuid()})


def cli_busy_until_gate():
    send({"type": "system", "subtype": "session_state_changed", "state": "running",
          "uuid": trace_uuid(), "session_id": SESSION_ID})
    while not os.path.exists(CLI_BUSY_GATE_PATH):
        time.sleep(0.02)
    send({"type": "system", "subtype": "session_state_changed", "state": "idle",
          "uuid": trace_uuid(), "session_id": SESSION_ID})


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


def permission_fields():
    """The extra `can_use_tool` fields the launch asked for, read on each use."""
    if not PERMISSION_FIELDS_PATH:
        return {}
    try:
        with open(PERMISSION_FIELDS_PATH, encoding="utf-8") as fields:
            data = json.load(fields)
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def answer_permission_rules(request_id):
    """One `list_permission_rules` request, any control loop."""
    global RULES_DRAINING
    if RULES_LOG_PATH:
        with open(RULES_LOG_PATH, "a", encoding="utf-8") as log:
            log.write(json.dumps({"t": time.monotonic()}) + "\n")
    if RULES_GATE_PATH:
        # Held in order, while later requests are still read: a request that
        # arrives during the drain queues behind the rest.
        with HELD_RULES_LOCK:
            if RULES_DRAINING or not os.path.exists(RULES_GATE_PATH):
                HELD_RULES.append(request_id)
                if not RULES_DRAINING:
                    RULES_DRAINING = True
                    threading.Thread(target=release_held_rules, daemon=True).start()
                return
    answer_permission_rules_now(request_id)


def release_held_rules():
    global RULES_DRAINING
    while True:
        # Checked before each answer: a gate removed mid-drain holds the rest.
        while not os.path.exists(RULES_GATE_PATH):
            time.sleep(0.05)
        with HELD_RULES_LOCK:
            if not HELD_RULES:
                RULES_DRAINING = False
                return
            request_id = HELD_RULES.pop(0)
        answer_permission_rules_now(request_id)


def answer_permission_rules_now(request_id):
    if not PERMISSION_RULES_PATH:
        control_error(request_id,
                      "Unsupported control request subtype: list_permission_rules")
        return
    try:
        with open(PERMISSION_RULES_PATH, encoding="utf-8") as rules:
            payload = json.load(rules)
    except (OSError, ValueError):
        control_error(request_id, "fixture: unreadable rules file")
        return
    control_response(request_id, payload)


def answer_hooks_listing(request_id):
    """One `get_hooks_listing` request, any control loop."""
    global HOOKS_DRAINING
    if HOOKS_GATE_PATH:
        with HELD_HOOKS_LOCK:
            if HOOKS_DRAINING or not os.path.exists(HOOKS_GATE_PATH):
                HELD_HOOKS.append(request_id)
                if not HOOKS_DRAINING:
                    HOOKS_DRAINING = True
                    threading.Thread(target=release_held_hooks, daemon=True).start()
                return
    answer_hooks_listing_now(request_id)


def release_held_hooks():
    global HOOKS_DRAINING
    while True:
        while not os.path.exists(HOOKS_GATE_PATH):
            time.sleep(0.05)
        with HELD_HOOKS_LOCK:
            if not HELD_HOOKS:
                HOOKS_DRAINING = False
                return
            request_id = HELD_HOOKS.pop(0)
        answer_hooks_listing_now(request_id)


def answer_hooks_listing_now(request_id):
    if not HOOKS_PATH:
        control_error(request_id,
                      "Unsupported control request subtype: get_hooks_listing")
        return
    try:
        with open(HOOKS_PATH, encoding="utf-8") as hooks:
            payload = json.load(hooks)
    except (OSError, ValueError):
        control_error(request_id, "fixture: unreadable hooks file")
        return
    control_response(request_id, payload)


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
    request.update(permission_fields())
    send({
        "type": "control_request",
        "request_id": cli_request_id,
        "request": request,
    })


def finish_permission(cli_request_id, answer_message, end_turn=True):
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
    if end_turn:
        if MODE == "permission" and RESULT_GATE_PATH:
            threading.Thread(target=result_when_gated, daemon=True).start()
        else:
            result()


def result_when_gated():
    while not os.path.exists(RESULT_GATE_PATH):
        time.sleep(0.02)
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
# `--fixture-ask-gate=<path>` holds `subperm`'s permission request until the file exists.
ASK_GATE_PATH = flag_value("fixture-ask-gate")
TRACE_COST = {"total": 0.0}
TRACE_COUNTER = {"n": 0}
TRACE_INTERRUPT = threading.Event()
TRACE_EOF = threading.Event()
TRACE_PROMPTS = queue.Queue()
TRACE_ANSWERS = {}
TRACE_ANSWERED = threading.Condition()
# Whether the session's `initialize` carried `forwardSubagentText: true`.
TRACE_FORWARD = {"on": False}


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
    request.update(permission_fields())
    send({"type": "control_request", "request_id": cli_request_id, "request": request})


def wait_answer(cli_request_id):
    """What the user answered: `allow` or `deny`. A closed stdin answers deny,
    so a piped self-check runs the whole script."""
    with TRACE_ANSWERED:
        while cli_request_id not in TRACE_ANSWERS and not TRACE_EOF.is_set():
            TRACE_ANSWERED.wait(0.2)
        return TRACE_ANSWERS.get(cli_request_id, "deny")


# ---------------------------------------------------------------------------
# MCP Apps: `--fixture-mcp-apps` makes `system/init` carry the four capabilities
# (`--fixture-mcp-apps-caps=<a,b>` replaces the list with those names, empty for
# none; `--fixture-floor` carries none at all), `mcp_status` list the servers
# `apps` and `other`, and, in the trace mode, `mcp_read_resource` and `mcp_call`
# answer. `--fixture-widget-html=<path>` is the document `ui://apps/show.html`
# serves. `--fixture-stray-mcp-ask` has each trace turn end with a `can_use_tool`
# for `toolu_mcp_call_deadbeef`, as a CLI ask nobody expects would be.
# ---------------------------------------------------------------------------

MCP_APPS = "--fixture-mcp-apps" in sys.argv
STRAY_MCP_ASK = "--fixture-stray-mcp-ask" in sys.argv
# Not `flag_value`: an empty list is an answer here.
MCP_APPS_CAPS = next(
    (arg.split("=", 1)[1] for arg in sys.argv if arg.startswith("--fixture-mcp-apps-caps=")),
    None,
)
MCP_APPS_ALL_CAPS = [
    "mcp_tool_ui_meta_v1",
    "mcp_read_resource_v1",
    "mcp_call_result_not_for_model_v1",
    "mcp_call_check_permissions_v1",
]
WIDGET_MIME = "text/html;profile=mcp-app"
WIDGET_URI = "ui://apps/show.html"
CONTROL_LOG_LOCK = threading.Lock()
# The permission mode in force, which decides whether an `mcp_call` asks: the real CLI
# asks nothing in `bypassPermissions`.
MCP_MODE = {"mode": "bypassPermissions" if START_BYPASS else "default"}


def control_log(entry):
    if not CONTROL_LOG_PATH:
        return
    with CONTROL_LOG_LOCK:
        with open(CONTROL_LOG_PATH, "a", encoding="utf-8") as log:
            log.write(json.dumps(entry, separators=(",", ":")) + "\n")


def mcp_apps_init_fields():
    if not MCP_APPS or BG_FLOOR:
        return {}
    if MCP_APPS_CAPS is None:
        return {"capabilities": list(MCP_APPS_ALL_CAPS)}
    return {"capabilities": [name for name in MCP_APPS_CAPS.split(",") if name]}


def mcp_apps_servers():
    def tool(name, visibility):
        entry = {"name": name, "annotations": {}}
        if not BG_FLOOR:
            entry["_meta"] = {"ui": {"resourceUri": WIDGET_URI, "visibility": visibility}}
        return entry

    def server(name, tools):
        entry = {"name": name, "status": "connected", "tools": tools}
        if not BG_FLOOR:
            entry["source"] = "dynamic"
        return entry

    return [
        server("apps", [
            tool("show", ["model", "app"]),
            tool("act", ["app"]),
            tool("report", ["app"]),
            tool("model_only", ["model"]),
        ]),
        server("other", [{"name": "ping", "annotations": {}}]),
    ]


def mcp_status_payload(default):
    return {"mcpServers": mcp_apps_servers()} if MCP_APPS else default


def answer_read_resource(request_id, request):
    if BG_FLOOR:
        control_error(request_id, "Unsupported control request subtype: mcp_read_resource")
    elif request.get("serverName") == "apps" and request.get("uri") == WIDGET_URI:
        path = flag_value("fixture-widget-html")
        if path:
            with open(path, encoding="utf-8") as document:
                html = document.read()
        else:
            html = "<!doctype html><p>widget</p>"
        control_response(request_id, {"contents": [{
            "uri": WIDGET_URI,
            "mimeType": WIDGET_MIME,
            "text": html,
            "_meta": {"ui": {
                "csp": {"connectDomains": ["https://api.widget.test"],
                        "resourceDomains": ["https://cdn.widget.test",
                                            "http://bad.widget.test"]},
                "prefersBorder": True,
            }},
        }]})
    else:
        control_error(request_id, "mcp_read_resource: resource not found")


def answer_mcp_call(request_id, request):
    """Asks first when the request says to check permissions, the way the CLI does
    outside `bypassPermissions`. On a thread of its own: the answer to the ask is read by
    the main loop, which must stay free to read it."""
    tool = request.get("tool")
    arguments = request.get("arguments")

    def run():
        if request.get("check_permissions") is True and MCP_MODE["mode"] != "bypassPermissions":
            ask_id = "toolu_mcp_call_" + os.urandom(4).hex()
            cli_id = "cli-" + ask_id
            trace_ask(cli_id, tool, ask_id, arguments)
            answer = wait_answer(cli_id)
            control_log({"mcp_call_ask": answer})
            if answer != "allow":
                control_error(request_id, "The tool did not run: denied")
                return
        compact = json.dumps(arguments, separators=(",", ":"))
        control_response(request_id, {
            "content": [{"type": "text", "text": f"apps:{tool}:{compact}"}],
            "structuredContent": {"tool": tool, "args": arguments, "n": 42},
            "_meta": {"com.anthropic/x": 1, "other/key": "v"},
        })

    threading.Thread(target=run, daemon=True).start()


def stray_mcp_ask():
    """An ask under an `mcp_call` id that no `mcp_call` of Sirio's explains."""
    trace_ask("cli-mcp-stray", "mcp__apps__act", "toolu_mcp_call_deadbeef", {"stray": True})
    control_log({"stray_mcp_ask": wait_answer("cli-mcp-stray")})


def mcp_widget_result(tool_use_id, parent=None):
    body = '{"n":42}'
    trace_result_line(tool_use_id, body, parent=parent, totals={
        "content": body, "structuredContent": {"n": 42},
        "_meta": {"other/key": "v", "com.anthropic/hidden": 1}})


def mcp_widget_result_in(parent):
    mcp_widget_result("toolu_sub_show", parent=parent)


def script_mcp_widget():
    """Two `show` calls in a row with no text between, then the turn's text."""
    started = time.time()
    for number, text_in in ((1, "hi"), (2, "again")):
        message_start(f"msg_mcp_{number}")
        trace_assistant(f"msg_mcp_{number}", tool_use(
            f"toolu_show_{number}", "mcp__apps__show", {"text": text_in}))
        message_end("tool_use", 40)
        pause()
        mcp_widget_result(f"toolu_show_{number}")
    message_start("msg_mcp_3")
    text("Shown.")
    message_end("end_turn", 10)
    trace_result(0.05, started)


def script_mcp_subagent():
    """A subagent's `mcp__` call: its lines name their parent, so the call has no row of
    its own on the main thread."""
    started = time.time()
    agent = "toolu_agent_mcp"
    message_start("msg_msub_1")
    trace_assistant("msg_msub_1", tool_use(agent, "Agent", {
        "description": "Show it", "prompt": "Show it",
        "subagent_type": "general-purpose", "run_in_background": False}))
    message_end("tool_use", 40)
    task_started("task-msub", agent, "Show it", "general-purpose", 1)
    trace_assistant("msg_msub_s1", tool_use(
        "toolu_sub_show", "mcp__apps__show", {"text": "sub"}),
        parent=agent, agent_id="task-msub")
    pause()
    mcp_widget_result_in(agent)
    pause()
    trace_result_line(agent, "[Subagent hand-back]\nDone.",
                      totals=agent_totals("task-msub", 900, 1))
    message_start("msg_msub_2")
    text("ok")
    message_end("end_turn", 10)
    trace_result(0.05, started)


def script_mcp_flood():
    started = time.time()
    message_start("msg_flood_1")
    trace_assistant("msg_flood_1", tool_use(
        "toolu_flood_1", "mcp__apps__show", {"text": "flood"}))
    message_end("tool_use", 40)
    pause()
    mcp_widget_result("toolu_flood_1")
    trace_result(0.05, started)


def script_mcp_forge():
    """A `show` call whose widget posts straight to the IPC handler, outside its relay."""
    started = time.time()
    message_start("msg_forge_1")
    trace_assistant("msg_forge_1", tool_use(
        "toolu_forge_1", "mcp__apps__show", {"text": "forge"}))
    message_end("tool_use", 40)
    pause()
    mcp_widget_result("toolu_forge_1")
    trace_result(0.05, started)


def script_mcp_idle():
    """A `show` call whose widget does nothing by itself but offers a button (text "idle")."""
    started = time.time()
    message_start("msg_idle_1")
    trace_assistant("msg_idle_1", tool_use(
        "toolu_idle_1", "mcp__apps__show", {"text": "idle"}))
    message_end("tool_use", 40)
    pause()
    mcp_widget_result("toolu_idle_1")
    trace_result(0.05, started)


def script_mcp_paste():
    """A `show` call whose widget is a textarea (text "paste")."""
    started = time.time()
    message_start("msg_paste_1")
    trace_assistant("msg_paste_1", tool_use(
        "toolu_paste_1", "mcp__apps__show", {"text": "paste"}))
    message_end("tool_use", 40)
    pause()
    mcp_widget_result("toolu_paste_1")
    trace_result(0.05, started)


def script_mcp_denied():
    """A `show` call that did not run: its `tool_use_result` is a string, as for a denial."""
    started = time.time()
    message_start("msg_denied_1")
    trace_assistant("msg_denied_1", tool_use(
        "toolu_denied_1", "mcp__apps__show", {"text": "secret-input"}))
    message_end("tool_use", 40)
    pause()
    trace_result_line("toolu_denied_1", "Permission to use mcp__apps__show was denied",
                      is_error=True, totals="Permission to use mcp__apps__show was denied")
    trace_result(0.05, started)


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
        "stop_reason": None if interrupted else "end_turn",
        "total_cost_usd": TRACE_COST["total"],
        "modelUsage": usage,
        "permission_denials": [],
        "session_id": SESSION_ID,
        "uuid": trace_uuid(),
    }
    if interrupted:
        # The shape the real CLI captured for a Stop: no `result` text at all.
        body["terminal_reason"] = "aborted_streaming"
    else:
        body["result"] = "Done."
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
        # A closed stdin ends the wait, as it ends `wait_answer`'s, and so does an interrupt.
        while (ASK_GATE_PATH and not os.path.exists(ASK_GATE_PATH)
               and not TRACE_EOF.is_set() and not TRACE_INTERRUPT.is_set()):
            time.sleep(0.02)
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


def task_ended(task_id, tool_use_id, duration_ms, summary="Agent finished the work"):
    send({"type": "system", "subtype": "task_updated", "task_id": task_id,
          "patch": {"status": "completed", "end_time": int(time.time() * 1000)},
          "uuid": trace_uuid(), "session_id": SESSION_ID})
    send({"type": "system", "subtype": "task_notification", "task_id": task_id,
          "tool_use_id": tool_use_id, "status": "completed",
          "summary": summary, "output_file": f"/tmp/{task_id}.output",
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


def script_subagent_text():
    """A foreground agent whose text, thinking and tool calls the CLI forwards (one
    stdout line per content block, sharing the message's id), a grandchild, and a
    background agent whose lines come after the `result`. The floor (2.1.257) names no
    `agent_id` on a line or an ask. A foreground agent's text and thinking are written only
    when `initialize` asked for them; its tool lines always; a background agent's always."""
    started = time.time()
    forward = TRACE_FORWARD["on"]
    fg, grand, bg = "toolu_agent_text", "toolu_agent_grand", "toolu_agent_bg"

    def aid(task):
        return None if BG_FLOOR else task

    def sub(message_id, block, parent, task, foreground=False):
        if foreground and not forward:
            return
        trace_assistant(message_id, block, parent=parent, agent_id=aid(task))

    odd = "I will look. First I read the file " + chr(0x202E) + "txt."
    message_start("msg_main_1")
    trace_assistant("msg_main_1", tool_use(fg, "Agent", {
        "description": "Read the notes", "prompt": "Read the notes and report.",
        "subagent_type": "general-purpose", "run_in_background": False}))
    trace_assistant("msg_main_1", tool_use("toolu_main_ls", "Bash", {
        "command": "ls", "description": "List the files"}))
    trace_assistant("msg_main_1", tool_use(bg, "Agent", {
        "description": "Watch the logs", "prompt": "Watch the logs",
        "subagent_type": "fork", "run_in_background": True}))
    message_end("tool_use", 90)
    task_started("task-text", fg, "Read the notes", "general-purpose", 1)
    task_started_background("task-bg-t", bg, "Watch the logs")
    async_launched(bg, "task-bg-t")
    trace_result_line("toolu_main_ls", "README.md\nnotes.txt")
    # The subagent's first line follows its call at once.
    sub("msg_s1", {"type": "thinking", "thinking": "Let me look first.", "signature": ""},
        fg, "task-text", True)
    sub("msg_s1", {"type": "text", "text": odd}, fg, "task-text", True)
    sub("msg_s1", tool_use("toolu_sub_read", "Read", {"file_path": "/repo/notes.txt"}),
        fg, "task-text")
    pause()
    trace_result_line("toolu_sub_read", "hi", parent=fg)
    bash_input = {"command": "cat notes.txt", "description": "Print the notes"}
    sub("msg_s2", tool_use("toolu_sub_bash", "Bash", bash_input), fg, "task-text")
    pause()
    trace_ask("cli-sub-text", "Bash", "toolu_sub_bash", bash_input, agent_id=aid("task-text"))
    allowed = wait_answer("cli-sub-text") == "allow"
    trace_result_line("toolu_sub_bash", "hi" if allowed else "denied",
                      is_error=not allowed, parent=fg)
    sub("msg_s3", tool_use(grand, "Agent", {
        "description": "Check one thing", "prompt": "Check one thing",
        "subagent_type": "general-purpose"}), fg, "task-text")
    task_started("task-grand", grand, "Check one thing", "general-purpose", 2)
    sub("msg_g1", {"type": "text", "text": "Grandchild here. All clear."},
        grand, "task-grand", True)
    pause()
    trace_result_line(grand, "[Subagent hand-back]\nAll clear.", parent=fg,
                      totals=agent_totals("task-grand", 900, 0))
    sub("msg_s4", {"type": "text", "text": "Done. The file says hi."}, fg, "task-text", True)
    pause()
    trace_result_line(fg, "[Subagent hand-back]\nThe file says hi.",
                      totals=agent_totals("task-text", 2400, 2))
    message_start("msg_main_2")
    text("ok")
    message_end("end_turn", 10)
    trace_result(0.08, started)
    # The background agent keeps writing after the result, on a thread of its own: it waits
    # for the next prompt's turn to be answered, so its lines reach a card a new turn began after.
    TRACE_NEXT_ANSWERED.clear()

    def background():
        while not TRACE_NEXT_ANSWERED.is_set() and not TRACE_EOF.is_set():
            time.sleep(0.05)
        if TRACE_EOF.is_set():
            return
        pause(8)
        trace_assistant("msg_bgt_1", {"type": "text", "text": "Background start. Still going."},
                        parent=bg, agent_id=aid("task-bg-t"))
        # Long enough for a check to read the card at work, whatever the checks before it cost.
        pause(TRACE_BACKGROUND_HOLD * 2)
        task_ended("task-bg-t", bg, 5000, summary="Watched the logs.")

    threading.Thread(target=background, daemon=True).start()


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
    "subagent_text": script_subagent_text,
    "mcp_widget": script_mcp_widget,
    "mcp_flood": script_mcp_flood,
    "mcp_denied": script_mcp_denied,
    "mcp_forge": script_mcp_forge,
    "mcp_idle": script_mcp_idle,
    "mcp_paste": script_mcp_paste,
    "mcp_subagent": script_mcp_subagent,
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
        if STRAY_MCP_ASK:
            stray_mcp_ask()
        if first not in ("bgprompt", "subagent_text"):
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
            control_log(request)
            if subtype == "initialize":
                TRACE_FORWARD["on"] = request.get("forwardSubagentText") is True
                control_response(request_id, initialize_payload())
            elif subtype == "interrupt":
                control_response(request_id, {"still_queued": 0})
                TRACE_INTERRUPT.set()
            elif subtype == "get_context_usage":
                control_response(request_id, {
                    "totalTokens": 48000, "rawMaxTokens": 200000, "maxTokens": 180000,
                    "percentage": 24, "model": TRACE_MODEL, "categories": []})
            elif subtype == "mcp_status":
                control_response(request_id, mcp_status_payload({"mcpServers": []}))
            elif subtype == "mcp_read_resource" and MCP_APPS:
                answer_read_resource(request_id, request)
            elif subtype == "mcp_call" and MCP_APPS:
                answer_mcp_call(request_id, request)
            elif subtype == "list_permission_rules":
                answer_permission_rules(request_id)
            elif subtype == "get_hooks_listing":
                answer_hooks_listing(request_id)
            elif subtype in ("mcp_toggle", "reload_plugins", "reload_skills"):
                control_error(request_id, f"Unsupported control request subtype: {subtype}")
            elif subtype == "set_permission_mode":
                MCP_MODE["mode"] = request.get("mode")
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


# ---------------------------------------------------------------------------
# `elicitation`: MCP elicitation, withdrawal and refusal. The prompt's exact
# text (stripped) picks the script; `ELIC_SCRIPTS` lists them. A turn ends with
# a success `result` once every request its script made is answered or
# withdrawn, except `silent form`, which never ends by itself.
# ---------------------------------------------------------------------------

# The schema's own order is the reverse of `property_order`, so a host that
# loses `property_order` shows the fields backwards.
ELIC_FORM_SCHEMA = {
    "type": "object",
    "properties": {
        "ok": {"type": "boolean", "default": True},
        "note": {"type": "string", "description": "free text", "format": "email"},
        "count": {"type": "integer", "title": "How many", "minimum": 1},
        "colour": {"type": "string", "title": "Colour", "enum": ["red", "blue"]},
    },
    "required": ["colour"],
}
# CLI request id -> "elicitation" | "permission" | "unknown", for the requests
# the current script is still waiting on.
ELIC_OUTSTANDING = {}
# Every id the fixture ever issued, answered, withdrawn or not: an answer to
# one of them is logged whatever became of it, so a host that answers a
# withdrawn request is seen doing so.
ELIC_ISSUED = set()
ELIC_ENDS_TURN = True
# Request id -> (server, elicitation id) to announce once the user accepts.
ELIC_COMPLETES = {}


def log_response(message):
    if RESPONSE_LOG_PATH:
        with open(RESPONSE_LOG_PATH, "a", encoding="utf-8") as log:
            log.write(json.dumps(message, separators=(",", ":")) + "\n")


def ask_elicitation(request_id, **fields):
    ELIC_OUTSTANDING[request_id] = "elicitation"
    ELIC_ISSUED.add(request_id)
    request = {"subtype": "elicitation", "mcp_server_name": "elic"}
    request.update(fields)
    send({"type": "control_request", "request_id": request_id, "request": request})


def ask_form(request_id, schema=None):
    ask_elicitation(
        request_id,
        message="Pick a colour for the probe",
        mode="form",
        requested_schema=schema if schema is not None else ELIC_FORM_SCHEMA,
        property_order=["colour", "count", "note", "ok"] if schema is None else None,
    )


def ask_bash_permission(request_id, tool_use_id):
    tool_input = {"command": "rm -rf build", "description": "Remove build"}
    assistant_tool_use(tool_use_id, "Bash", tool_input)
    ELIC_OUTSTANDING[request_id] = "permission"
    ELIC_ISSUED.add(request_id)
    ask_permission(request_id, "Bash", tool_use_id, tool_input)


def withdraw(request_id):
    ELIC_OUTSTANDING.pop(request_id, None)
    PENDING.pop(request_id, None)
    send({"type": "control_cancel_request", "request_id": request_id})


def script_elic_form():
    ask_form("elic-form")


def script_elic_url():
    ELIC_COMPLETES["elic-url"] = ("elic", "el-2")
    ask_elicitation(
        "elic-url",
        message="Sign in to the probe service",
        mode="url",
        url="https://example.com/auth?x=1",
        elicitation_id="el-2",
    )


def script_elic_url_early():
    # The user finished the flow by hand: the completion arrives before any answer.
    ask_elicitation(
        "elic-url-early",
        message="Sign in to the probe service",
        mode="url",
        url="https://example.com/auth?x=1",
        elicitation_id="el-2",
    )
    send({"type": "system", "subtype": "elicitation_complete",
          "mcp_server_name": "elic", "elicitation_id": "el-2",
          "session_id": SESSION_ID, "uuid": "elicitation-complete-early-uuid"})


def script_elic_bad_schema():
    ask_form("elic-bad", {"type": "object", "properties": {"nested": {"type": "object"}}})


def script_elic_withdraw():
    ask_form("elic-withdraw")
    time.sleep(0.5)
    withdraw("elic-withdraw")
    result()


def script_elic_withdraw_permission():
    ask_bash_permission("cli-perm-withdraw", "toolu_withdraw_1")
    time.sleep(0.5)
    withdraw("cli-perm-withdraw")
    result()


def script_elic_unknown():
    ELIC_OUTSTANDING["u1"] = "unknown"
    ELIC_ISSUED.add("u1")
    send({"type": "control_request", "request_id": "u1", "request": {"subtype": "future_thing"}})


def script_elic_both():
    ask_bash_permission("cli-perm-both", "toolu_both_1")
    ask_form("elic-both")


def script_elic_silent_form():
    global ELIC_ENDS_TURN
    ELIC_ENDS_TURN = False
    ask_form("elic-silent")


ELIC_SCRIPTS = {
    "form": script_elic_form,
    "url": script_elic_url,
    "url early": script_elic_url_early,
    "bad schema": script_elic_bad_schema,
    "withdraw": script_elic_withdraw,
    "withdraw permission": script_elic_withdraw_permission,
    "unknown": script_elic_unknown,
    "both": script_elic_both,
    "silent form": script_elic_silent_form,
}


def elicitation_prompt(message):
    global ELIC_ENDS_TURN
    prompt = trace_prompt_text(message)
    log_response({"prompt": prompt})
    init_line()
    ELIC_ENDS_TURN = True
    script = ELIC_SCRIPTS.get(prompt.strip())
    if script is None:
        text("Hello")
        result()
        return
    script()


def elicitation_answered(message):
    response = message.get("response", {})
    request_id = response.get("request_id")
    if request_id not in ELIC_ISSUED:
        return
    log_response(message)
    kind = ELIC_OUTSTANDING.pop(request_id, None)
    if kind is None:
        return
    if kind == "permission":
        finish_permission(request_id, message, end_turn=False)
    elif kind == "elicitation":
        action = response.get("response", {}).get("action", "?")
        if action == "accept" and request_id in ELIC_COMPLETES:
            server, elicitation_id = ELIC_COMPLETES.pop(request_id)
            send({"type": "system", "subtype": "elicitation_complete",
                  "mcp_server_name": server, "elicitation_id": elicitation_id,
                  "session_id": SESSION_ID, "uuid": "elicitation-complete-uuid"})
        text(f"elicitation: {action}")
    if not ELIC_OUTSTANDING and ELIC_ENDS_TURN:
        result()


def resume_arg():
    """The id after `--resume`, or None for a launch that resumes nothing."""
    if "--resume" not in sys.argv:
        return None
    at = sys.argv.index("--resume")
    return sys.argv[at + 1] if at + 1 < len(sys.argv) else ""


# ---------------------------------------------------------------------------
# `session_state`: the CLI's own `session_state_changed` lines. The real CLI
# writes them only when CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS is set, so this
# mode does too: a host that does not set the variable sees none. Line shapes
# copy a live probe (plain, a permission, a background agent). `requires_action`
# is written BEFORE the `can_use_tool` request it announces, and the last
# `idle` after the last `result`.
#
# The first word of a prompt picks the script: `plain`, `ask`, `background`,
# `monitor` (three turns of its own after the first) or `silent` (writes nothing).
# `--fixture-gate=<path>` holds `background` after its main `result` until
# `<path>` exists, and again before its second `result` until `<path>.2`
# does; an empty or missing value means do not wait. A script runs on its own
# thread so the main thread keeps answering control requests.
# ---------------------------------------------------------------------------

GATE_PATH = next(
    (arg[len("--fixture-gate="):] for arg in sys.argv if arg.startswith("--fixture-gate=")),
    "",
)
DONE_PATH = next(
    (arg[len("--fixture-done="):] for arg in sys.argv if arg.startswith("--fixture-done=")),
    "",
)
STATE_EMITS = bool(os.environ.get("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS"))
STATE_PROMPTS = queue.Queue()
STATE_ANSWERED = threading.Event()
STATE_EOF = threading.Event()


def state_line(state):
    if STATE_EMITS:
        send({"type": "system", "subtype": "session_state_changed", "state": state,
              "uuid": trace_uuid(), "session_id": SESSION_ID})


def state_text(body):
    # The fold reads text from the stream deltas; the `assistant` line is the
    # complete message that follows them.
    text(body)
    send({"type": "assistant",
          "message": {"id": "msg-state", "type": "message", "role": "assistant",
                      "model": "claude-fable-5-1", "content": [{"type": "text", "text": body}],
                      "stop_reason": None},
          "parent_tool_use_id": None, "session_id": SESSION_ID, "uuid": trace_uuid()})


def state_tool_result(tool_use_id, body):
    send({"type": "user",
          "message": {"role": "user", "content": [
              {"tool_use_id": tool_use_id, "type": "tool_result", "content": body}]},
          "parent_tool_use_id": None, "session_id": SESSION_ID, "uuid": trace_uuid()})


def state_gate(suffix):
    """True once the gate is open (or there is none); False when stdin closed
    first, and the script must then write nothing more."""
    if not GATE_PATH:
        return True
    path = GATE_PATH + suffix
    while not os.path.exists(path):
        if STATE_EOF.is_set():
            return False
        time.sleep(0.05)
    return True


def state_plain():
    state_line("running")
    init_line()
    state_text("Hello there")
    result()
    state_line("idle")


def state_ask():
    state_line("running")
    init_line()
    tool_input = {"command": "ls", "description": "List"}
    assistant_tool_use("toolu_state_1", "Bash", tool_input)
    state_line("requires_action")
    ask_permission("cli-state-1", "Bash", "toolu_state_1", tool_input)
    while not STATE_ANSWERED.wait(0.2):
        if STATE_EOF.is_set():
            return
    state_line("running")
    state_tool_result("toolu_state_1", "ran")
    state_text("Listed")
    result()
    state_line("idle")


def state_background():
    state_line("running")
    init_line()
    assistant_tool_use("toolu_bg_1", "Agent", {
        "description": "bg task", "prompt": "do the thing",
        "subagent_type": "general-purpose", "run_in_background": True})
    send({"type": "system", "subtype": "background_tasks_changed",
          "tasks": [{"task_id": "bg-task-1", "run_id": "run-bg-task-1",
                     "task_type": "local_agent", "subagent_type": "general-purpose",
                     "description": "bg task"}],
          "uuid": trace_uuid(), "session_id": SESSION_ID})
    task_started_background("bg-task-1", "toolu_bg_1", "bg task")
    state_tool_result("toolu_bg_1", "Async agent launched successfully.")
    state_text("ok")
    result()
    if not state_gate(""):
        return
    task_ended("bg-task-1", "toolu_bg_1", 1200)
    send({"type": "system", "subtype": "background_tasks_changed", "tasks": [],
          "uuid": trace_uuid(), "session_id": SESSION_ID})
    init_line()
    state_text("agent reported done")
    if not state_gate(".2"):
        return
    result()
    state_line("idle")


def state_monitor():
    # A Monitor task: the prompt gets its own turn, then the CLI starts three
    # turns by itself, one per watched line (live probe: `running`, `system`
    # init, `assistant`, `result`, `idle`, with no `task_*` lines).
    state_line("running")
    init_line()
    state_text("Watching")
    result()
    state_line("idle")
    for number in (1, 2, 3):
        time.sleep(2.0 if number == 1 else 1.0)
        if STATE_EOF.is_set():
            return
        state_line("running")
        init_line()
        # The real CLI takes a while to answer an event: a sampler can see it.
        time.sleep(0.6)
        state_text(f"monitor event {number}")
        result()
        state_line("idle")
    if DONE_PATH:
        open(DONE_PATH, "w").close()


def state_silent():
    # Writes nothing: a host test that applies the CLI's lines itself, with a
    # live client, uses it so the fixture adds none of its own.
    pass


STATE_SCRIPTS = {"plain": state_plain, "ask": state_ask, "background": state_background,
                 "monitor": state_monitor, "silent": state_silent}


def state_turns():
    while True:
        prompt = STATE_PROMPTS.get()
        if prompt is None:
            return
        STATE_ANSWERED.clear()
        first = prompt.split(None, 1)[0].lower() if prompt.split() else ""
        STATE_SCRIPTS.get(first, state_plain)()


def state_main():
    init_line()
    turns = threading.Thread(target=state_turns, daemon=True)
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
            elif subtype == "get_context_usage":
                control_response(request_id, {
                    "totalTokens": 48000, "rawMaxTokens": 200000, "maxTokens": 180000,
                    "percentage": 24, "model": TRACE_MODEL, "categories": []})
            elif subtype == "mcp_status":
                control_response(request_id, mcp_status_payload({"mcpServers": []}))
            elif subtype == "list_permission_rules":
                answer_permission_rules(request_id)
            elif subtype == "get_hooks_listing":
                answer_hooks_listing(request_id)
            elif subtype in ("mcp_toggle", "reload_plugins", "reload_skills"):
                control_error(request_id, f"Unsupported control request subtype: {subtype}")
            elif subtype == "set_permission_mode":
                control_response(request_id, {"mode": request.get("mode")})
            else:
                control_response(request_id, {})
        elif kind == "control_response":
            cli_request_id = message.get("response", {}).get("request_id")
            if cli_request_id in PENDING:
                # The script writes what follows the answer, in its own order.
                PENDING.pop(cli_request_id, None)
                STATE_ANSWERED.set()
        elif kind == "user":
            if USER_LOG_PATH:
                with open(USER_LOG_PATH, "a", encoding="utf-8") as log:
                    log.write(json.dumps(message, separators=(",", ":")) + "\n")
            STATE_PROMPTS.put(trace_prompt_text(message))
    STATE_EOF.set()
    STATE_PROMPTS.put(None)
    turns.join()


# ---------------------------------------------------------------------------
# `background_tasks`: a background shell or agent, `get_task_output` and
# `stop_task`. Line shapes and orders copy the A3 live probe (claude 2.1.326,
# and the 2.1.257 floor for `--fixture-floor`).
#
# The first word of a prompt picks the script; any other word is a plain turn
# (init, text, `result`, `idle`) that starts nothing:
#   `shell`  a backgrounded Bash. The first is `bshell001`, a later one
#            `bshell002`, and so on: a task's state is never reused. Its output
#            is time-based: one `\u001b[32mline N\u001b[0m\n` a second for 6 s,
#            then quiet. The turn ends (`result`, `idle`), as the CLI's does
#            while the shell runs.
#   `agent`  a background Agent, `aagent001`, alive until `stop_task`: the turn's
#            `result` comes and its `idle` does not. `get_task_output` refuses it.
#   `hold`   `shell`'s lines up to the `tool_result`, then nothing more for the
#            turn: no `result`, no `idle`. It still answers `get_task_output`.
#   `slow`   `shell`, but a `get_task_output` is not answered until the file
#            named by `--fixture-gate=<path>` exists, so a read stays pending
#            for exactly as long as a test wants.
# A `stop_task` for a live task is answered about a second later, with the
# task's end in the probe's order: `task_updated` (`killed`), `task_notification`
# (`stopped`), `background_tasks_changed []`, the `success {}`, then `idle`
# (a stopped Agent also gets the follow-up turn the CLI starts). `\n[killed]\n`
# ends the output then. A stop for an ended or unknown task is `success {}` at
# once, with no lines.
# `--fixture-floor` is the floor (2.1.257): no `run_id` anywhere,
# `get_task_output` refused, and on a stop the rows drain before the `task_*`
# lines. In the trace mode `subagent_text` it means no line and no ask carries
# `agent_id`.
# `--fixture-request-log=<path>` appends every control request received, one
# JSON line each, as received.
# ---------------------------------------------------------------------------

BG_FLOOR = "--fixture-floor" in sys.argv
# `--fixture-rowless`: every non-empty list also carries a row with no `task_id`.
BG_ROWLESS = "--fixture-rowless" in sys.argv
BG_REQUEST_LOG = next(
    (arg[len("--fixture-request-log="):] for arg in sys.argv
     if arg.startswith("--fixture-request-log=")),
    "",
)
# `SIRIO_FIXTURE_STOP_DELAY` (seconds) lengthens it: a framed run's slow frames can keep a control read
# waiting past a second, and the `stopping` state a test looks for would be over by then.
BG_STOP_DELAY = float(os.environ.get("SIRIO_FIXTURE_STOP_DELAY") or 1.0)
BG_OUTPUT_SECONDS = 6
BG_PROMPTS = queue.Queue()
# Every task the session has started, by id: its type, when it began, when its
# process was stopped, and whether `[killed]` ends its output yet.
BG_TASKS = {}
BG_SLOW = threading.Event()
BG_LOCK = threading.Lock()


def bg_init():
    init_line("2.1.257" if BG_FLOOR else "2.1.273")


def bg_system(subtype, **fields):
    if BG_FLOOR:
        fields.pop("run_id", None)
    send({"type": "system", "subtype": subtype, **fields,
          "uuid": trace_uuid(), "session_id": SESSION_ID})


def bg_state(state):
    bg_system("session_state_changed", state=state)


def bg_row(task_id, task_type, description, subagent_type=None):
    row = {"task_id": task_id, "run_id": f"run-{task_id}", "task_type": task_type}
    if subagent_type:
        row["subagent_type"] = subagent_type
    row["description"] = description
    if BG_FLOOR:
        row.pop("run_id")
    return row


def bg_rows(rows):
    if BG_ROWLESS and rows:
        rows = [{"task_type": "local_bash", "description": "no id"}] + rows
    bg_system("background_tasks_changed", tasks=rows)


def bg_output(task_id):
    """A shell's output file, as of now."""
    with BG_LOCK:
        task = dict(BG_TASKS[task_id])
    elapsed = (task["ended_at"] or time.time()) - task["started"]
    count = min(int(elapsed), BG_OUTPUT_SECONDS)
    body = "".join(f"\u001b[32mline {n}\u001b[0m\n" for n in range(1, count + 1))
    if task["killed"]:
        body += "\n[killed]\n"
    return {"output": body, "total_bytes": len(body.encode("utf-8")), "truncated": False}


def bg_next_id(prefix):
    with BG_LOCK:
        taken = sum(1 for task_id in BG_TASKS if task_id.startswith(prefix))
    return f"{prefix}{taken + 1:03d}"


def bg_start(task_id, task_type, tool_use_id, tool_name, tool_input, description,
             subagent_type, launched):
    bg_state("running")
    bg_init()
    assistant_tool_use(tool_use_id, tool_name, tool_input)
    bg_rows([bg_row(task_id, task_type, description, subagent_type)])
    started = {"task_id": task_id, "run_id": f"run-{task_id}", "tool_use_id": tool_use_id,
               "description": description, "is_backgrounded": True, "task_type": task_type}
    if subagent_type:
        started["subagent_type"] = subagent_type
        started["spawn_depth"] = 1
        started["prompt"] = tool_input.get("prompt", description)
    bg_system("task_started", **started)
    with BG_LOCK:
        BG_TASKS[task_id] = {"type": task_type, "started": time.time(),
                             "ended_at": None, "killed": False,
                             "tool_use_id": tool_use_id, "summary": description}
    state_tool_result(tool_use_id, launched)


def bg_shell(hold=False, slow=False):
    if slow:
        BG_SLOW.set()
    task_id = bg_next_id("bshell")
    bg_start(task_id, "local_bash", f"toolu_s{int(task_id[-3:])}", "Bash",
             {"command": "bash shell40.sh", "run_in_background": True}, "shell40", None,
             f"Command running in background with ID: {task_id}. Output is being "
             f"written to: /tmp/{task_id}.output. You will be notified when it completes.")
    if hold:
        return
    result()
    bg_state("idle")


def bg_agent():
    task_id = bg_next_id("aagent")
    bg_start(task_id, "local_agent", f"toolu_a{int(task_id[-3:])}", "Agent",
             {"description": "bg task", "prompt": "do the thing",
              "subagent_type": "general-purpose", "run_in_background": True},
             "bg task", "general-purpose",
             [{"type": "text", "text": "Async agent launched successfully.\n"
               f"agentId: {task_id} (internal ID - do not mention to user.)\n"
               "The agent is working in the background. You will be notified "
               "automatically when it completes."}])
    result()


def bg_plain():
    bg_state("running")
    bg_init()
    state_text("Hello there")
    result()
    bg_state("idle")


def bg_end(task_id, request_id):
    """What a stop leaves, after the delay a real stop takes: the probe's lines
    in the probe's order, the stop's own answer among them."""
    time.sleep(BG_STOP_DELAY)
    with BG_LOCK:
        task = BG_TASKS[task_id]
        task["killed"] = True
        task_type, tool_use_id, summary = task["type"], task["tool_use_id"], task["summary"]
    if BG_FLOOR:
        bg_rows([])
    bg_system("task_updated", task_id=task_id, run_id=f"run-{task_id}",
              patch={"status": "killed", "end_time": int(time.time() * 1000)})
    bg_system("task_notification", task_id=task_id, run_id=f"run-{task_id}",
              tool_use_id=tool_use_id, status="stopped",
              output_file=f"/tmp/{task_id}.output", summary=summary)
    if not BG_FLOOR:
        bg_rows([])
    control_response(request_id, {})
    if task_type == "local_agent":
        # A stopped Agent makes the CLI start a turn of its own; a shell does not.
        send({"type": "user",
              "message": {"role": "user", "content": [
                  {"type": "text", "text": "[Request interrupted by user]"}]},
              "parent_tool_use_id": tool_use_id, "agent_id": task_id,
              "session_id": SESSION_ID, "uuid": trace_uuid(),
              "subagent_type": "general-purpose", "task_description": summary})
        bg_init()
        state_text("The agent was stopped.")
        result()
    bg_state("idle")


BG_SCRIPTS = {"shell": lambda: bg_shell(), "hold": lambda: bg_shell(hold=True),
              "slow": lambda: bg_shell(slow=True), "agent": bg_agent}


def bg_turns():
    while True:
        prompt = BG_PROMPTS.get()
        if prompt is None:
            return
        first = prompt.split(None, 1)[0].lower() if prompt.split() else ""
        BG_SCRIPTS.get(first, bg_plain)()


def bg_answer_output(request_id, task_id):
    with BG_LOCK:
        task = BG_TASKS.get(task_id)
    if BG_FLOOR:
        control_error(request_id, "Unsupported control request subtype: get_task_output")
    elif task is not None and task["type"] == "local_bash":
        control_response(request_id, bg_output(task_id))
    elif task is None and task_id.startswith("b"):
        # The probe: an unknown id shaped like a shell's is an empty success.
        control_response(request_id, {"output": "", "total_bytes": 0, "truncated": False})
    else:
        control_error(request_id,
                      "get_task_output: no shell or Monitor task with that task_id in this session")


def bg_answer_when_gated(request_id, task_id):
    while GATE_PATH and not os.path.exists(GATE_PATH):
        time.sleep(0.02)
    bg_answer_output(request_id, task_id)


def bg_main():
    bg_init()
    turns = threading.Thread(target=bg_turns, daemon=True)
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
            if BG_REQUEST_LOG:
                with open(BG_REQUEST_LOG, "a", encoding="utf-8") as log:
                    log.write(json.dumps(message, separators=(",", ":")) + "\n")
            if subtype == "initialize":
                control_response(request_id, initialize_payload())
            elif subtype == "interrupt":
                control_response(request_id, {"still_queued": []})
            elif subtype == "get_task_output":
                task_id = request.get("task_id", "")
                if BG_SLOW.is_set() and not BG_FLOOR:
                    threading.Thread(target=bg_answer_when_gated,
                                     args=(request_id, task_id), daemon=True).start()
                else:
                    bg_answer_output(request_id, task_id)
            elif subtype == "stop_task":
                task_id = request.get("task_id", "")
                with BG_LOCK:
                    task = BG_TASKS.get(task_id)
                    live = task is not None and task["ended_at"] is None
                    if live:
                        task["ended_at"] = time.time()
                if live:
                    threading.Thread(target=bg_end, args=(task_id, request_id),
                                     daemon=True).start()
                else:
                    control_response(request_id, {})
            elif subtype == "list_permission_rules":
                answer_permission_rules(request_id)
            elif subtype == "get_hooks_listing":
                answer_hooks_listing(request_id)
            elif subtype in ("mcp_toggle", "reload_plugins", "reload_skills"):
                control_error(request_id, f"Unsupported control request subtype: {subtype}")
            else:
                control_response(request_id, {})
        elif kind == "user":
            BG_PROMPTS.put(trace_prompt_text(message))
    BG_PROMPTS.put(None)
    turns.join()


def current_session():
    """The session a rename addresses: the launch's own id until this process
    has printed an `init` line, the fixture's `SESSION_ID` after."""
    if INIT_PRINTED:
        return SESSION_ID
    return argv_value("--session-id") or argv_value("--resume") or SESSION_ID


def read_title_store():
    if not TITLE_STORE_PATH:
        return {}
    try:
        with open(TITLE_STORE_PATH, encoding="utf-8") as store:
            data = json.load(store)
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def title_echo(title, session_id):
    send({
        "type": "system",
        "subtype": "session_title_changed",
        "title": title,
        "uuid": trace_uuid(),
        "session_id": session_id,
    })


def sanitise_title(title):
    """As the real CLI does: control characters become spaces, trim, 200 code points."""
    cleaned = "".join(" " if unicodedata.category(ch) == "Cc" else ch for ch in title)
    return cleaned.strip()[:200]


def answer_rename(request_id, request):
    global RENAMES_DRAINING
    if RENAME_GATE_PATH:
        # Held: the main loop goes on reading, so a later rename is heard (and
        # logged) while this one waits. They are answered in the order they came:
        # while the release thread drains, a new rename queues behind the rest.
        with HELD_RENAMES_LOCK:
            if RENAMES_DRAINING or not os.path.exists(RENAME_GATE_PATH):
                HELD_RENAMES.append((request_id, request))
                if not RENAMES_DRAINING:
                    RENAMES_DRAINING = True
                    threading.Thread(target=release_held_renames, daemon=True).start()
                return
    answer_rename_now(request_id, request)


def release_held_renames():
    global RENAMES_DRAINING
    while True:
        # Checked before each answer: a gate removed mid-drain holds the rest.
        while not os.path.exists(RENAME_GATE_PATH):
            time.sleep(0.05)
        with HELD_RENAMES_LOCK:
            if not HELD_RENAMES:
                # Every held rename is answered; the next one goes straight out.
                RENAMES_DRAINING = False
                return
            request_id, request = HELD_RENAMES.pop(0)
        answer_rename_now(request_id, request)


def answer_rename_now(request_id, request):
    if RENAME_ERROR:
        control_error(request_id, RENAME_ERROR)
        return
    current = current_session()
    given = request.get("session_id")
    if given is not None and given != current:
        control_error(request_id, "session_id is not the current session")
        return
    title = sanitise_title(str(request.get("title", "")))
    if not title:
        control_error(request_id, "title must be non-empty")
        return
    if TITLE_STORE_PATH:
        store = read_title_store()
        store[current] = title
        with open(TITLE_STORE_PATH, "w", encoding="utf-8") as out:
            json.dump(store, out)
    title_echo(title, current)
    control_response(request_id, {})


def wait_for_handshake_gate():
    if not HANDSHAKE_GATE_PATH:
        return
    if "--session-id" not in sys.argv and "--resume" not in sys.argv:
        return
    while not os.path.exists(HANDSHAKE_GATE_PATH):
        time.sleep(0.05)


# ---------------------------------------------------------------------------
# `cli_notices`: the frames the CLI sends when a request is retried, when it
# raises a notification, refuses, or says something informational, and an
# `assistant` answer it sent with no stream before it. The prompt text picks the
# turn (see `cli_notices_turn`); each ends with a `result`. Line shapes copy the
# live probe of the real CLI. `--fixture-retry-gate=<path>` holds `retry twice`
# after both `api_retry` frames, before its reply, until `<path>` exists; that turn runs on its own
# thread, so control requests are still answered meanwhile.
# ---------------------------------------------------------------------------

RETRY_GATE_PATH = flag_value("fixture-retry-gate")
# `--fixture-limits=<file>`: JSON `{"soon": epoch, "later": epoch}`, the reset times the
# `limit ...` prompts report (read when the prompt arrives, so the file may be written late).
LIMITS_PATH = flag_value("fixture-limits")


def cli_limit_resets():
    with open(LIMITS_PATH, encoding="utf-8") as handle:
        limits = json.load(handle)
    return limits["soon"], limits["later"]


def cli_limit_event(info):
    send({"type": "rate_limit_event", "rate_limit_info": info,
          "session_id": SESSION_ID, "uuid": trace_uuid()})


def cli_limit_refused(text_body, info):
    """A turn the plan refused: the event, the CLI's synthetic error message and a
    failed `result`, with no model reply (written by hand like `retry out`)."""
    cli_limit_event(info)
    send({"type": "assistant",
          "message": {"id": trace_uuid(), "type": "message", "role": "assistant",
                      "model": "<synthetic>", "content": [{"type": "text", "text": text_body}],
                      "stop_reason": "stop_sequence"},
          "error": "rate_limit", "is_api_error_message": True, "api_error_status": 429,
          "parent_tool_use_id": None, "session_id": SESSION_ID, "uuid": trace_uuid()})
    send({"type": "result", "subtype": "success", "is_error": True, "result": text_body,
          "terminal_reason": "api_error", "api_error_status": 429,
          "stop_reason": "stop_sequence", "session_id": SESSION_ID, "uuid": trace_uuid()})


def cli_system(subtype, **fields):
    line = {"type": "system", "subtype": subtype, "session_id": SESSION_ID,
            "uuid": trace_uuid()}
    line.update(fields)
    send(line)


def cli_retry(attempt, delay_ms, status, error, **extra):
    cli_system("api_retry", attempt=attempt, max_retries=10, retry_delay_ms=delay_ms,
               error_status=status, error=error, **extra)


def cli_streamed(body, message_id):
    """A reply with its stream: the start, the text, the block's end, the
    complete `assistant` line, then the message's end."""
    message_start(message_id)
    text(body)
    trace_stream({"type": "content_block_stop", "index": 0})
    trace_assistant(message_id, {"type": "text", "text": body})
    message_end()


def cli_notify(key, body, priority, timeout_ms, color=None):
    fields = {"key": key, "text": body, "priority": priority, "timeout_ms": timeout_ms}
    if color is not None:
        fields["color"] = color
    cli_system("notification", **fields)


def cli_inform(content, level):
    cli_system("informational", content=content, level=level)


# `--fixture-usage=<file>`: JSON `{"marker": bool, "report": <value>, "fails": bool, "text": str}`
# for the local commands `/usage` and `/context`, re-read on every one. A missing key means:
# `marker` true, no `usage_report` field written, `fails` false, the command's own text (the
# blocks below). `"report": null` writes `usage_report: null`.
USAGE_PATH = flag_value("fixture-usage")
USAGE_TEXT = ("Estimated cost:        $0.0000\nTotal duration (API):  0s\n"
              "Total duration (wall): 3s\n"
              "Total code changes:    0 lines added, 0 lines removed\n"
              "Usage:                 0 input, 0 output, 0 cache read, 0 cache write")
CONTEXT_TEXT = "Context: 1k of 200k tokens"


def cli_local_command(command):
    """A command the CLI answers itself, as the real one does (probe a9, Q1): no model call,
    so no `requesting` line. A synthetic assistant message and a `result`; with `marker` false,
    the floor's shape, both without the field that names the command."""
    options = {}
    if USAGE_PATH:
        with open(USAGE_PATH, encoding="utf-8") as handle:
            options = json.load(handle)
    marker = options.get("marker", True)
    body = options.get("text", USAGE_TEXT if command == "usage" else CONTEXT_TEXT)
    assistant = {"type": "assistant",
                 "message": {"id": trace_uuid(), "type": "message", "role": "assistant",
                             "model": "<synthetic>", "content": [{"type": "text", "text": body}],
                             "stop_reason": None},
                 "parent_tool_use_id": None, "session_id": SESSION_ID, "uuid": trace_uuid()}
    if marker:
        assistant["local_command_run"] = {"command": command, "args": ""}
    if "report" in options:
        assistant["usage_report"] = options["report"]
    send(assistant)
    result_line = {"type": "result", "subtype": "success", "is_error": bool(options.get("fails")),
                   "num_turns": 0, "total_cost_usd": 0, "duration_ms": 0, "duration_api_ms": 0,
                   "stop_reason": None, "result": body,
                   "session_id": SESSION_ID, "uuid": trace_uuid()}
    if marker:
        result_line["local_command"] = command
    send(result_line)


def cli_notices_turn(prompt):
    if prompt in ("/usage", "/context"):
        cli_local_command(prompt[1:])
        return
    requesting()
    if prompt == "retry twice":
        cli_retry(1, 617, 529, "overloaded")
        cli_retry(2, 1101, 529, "overloaded")
        if RETRY_GATE_PATH:
            while not os.path.exists(RETRY_GATE_PATH):
                time.sleep(0.02)
        cli_streamed("ok", "msg_rt")
    elif prompt == "retry kinds":
        cli_retry(1, 617, 500, "server_error")
        cli_retry(2, 617, 429, "rate_limit")
        cli_retry(3, 617, None, "unknown", no_response=None)
        cli_retry(4, 617, None, "unknown", no_response={"waited_ms": 2500, "retry_wait_ms": 0})
        # A wrongly typed optional field costs only itself, never the line.
        cli_retry(5, 617, "x", "server_error")
        # A wrongly typed required field drops the notice.
        cli_retry("five", 617, 500, "server_error")
        cli_streamed("ok", "msg_rk")
    elif prompt == "retry out":
        sentence = "API Error: Repeated 529 Overloaded errors."
        cli_retry(1, 617, 529, "overloaded")
        cli_retry(2, 1101, 529, "overloaded")
        send({"type": "assistant",
              "message": {"id": trace_uuid(), "type": "message", "role": "assistant",
                          "model": "<synthetic>", "content": [{"type": "text", "text": sentence}],
                          "stop_reason": "stop_sequence"},
              "error": "server_error", "parent_tool_use_id": None,
              "session_id": SESSION_ID, "uuid": trace_uuid()})
        send({"type": "result", "subtype": "success", "is_error": True, "result": sentence,
              "stop_reason": None, "session_id": SESSION_ID, "uuid": trace_uuid()})
        return
    elif prompt in ("limit warning", "limit warning again", "limit allowed"):
        soon, _ = cli_limit_resets()
        cli_streamed("ok", "msg_lw")
        info = {"status": "allowed", "resetsAt": soon, "rateLimitType": "five_hour"}
        if prompt != "limit allowed":
            info.update({"status": "allowed_warning", "utilization": 0.92,
                         "surpassedThreshold": 0.9})
        if prompt == "limit warning":
            info["unifiedWindows"] = {"five_hour": {"utilization": 0.92, "resetsAt": soon}}
        cli_limit_event(info)
    elif prompt == "limit rejected":
        soon, _ = cli_limit_resets()
        cli_limit_refused(
            "You've hit your limit \u00b7 resets 3:15pm (UTC)",
            {"status": "rejected", "rateLimitType": "five_hour", "resetsAt": soon,
             "unifiedWindows": {"five_hour": {"utilization": 1, "resetsAt": soon}}})
        return
    elif prompt == "limit pooled":
        _, later = cli_limit_resets()
        cli_limit_refused(
            "You've hit your team's pooled group budget",
            {"status": "rejected", "rateLimitType": "overage", "resetsAt": later,
             "overageDisabledReason": "org_level_disabled_until",
             "limitScope": "group_pool"})
        return
    elif prompt == "notify":
        cli_streamed("ok", "msg_n")
        cli_notify("stop-hook-error", "Stop hook error occurred \u00b7 ctrl+o to see",
                   "immediate", 120000)
        cli_notify("auto-mode-gate-plan-exit-fallback", "Plan mode exit fell back",
                   "high", 120000, color="warning")
        cli_notify("lsp-server-start-failed:demo", "LSP server demo failed to start",
                   "low", 120000)
        cli_notify("lsp-server-start-failed:demo", "LSP server demo failed again",
                   "low", 120000)
    elif prompt == "notify short":
        cli_streamed("ok", "msg_ns")
        cli_notify("short", "Compaction failed", "immediate", 8000, color="error")
    elif prompt == "notify typed":
        cli_streamed("ok", "msg_nt")
        cli_system("notification", key="typed", text="Typed badly", priority="low",
                   timeout_ms="x")
    elif prompt == "unstreamed":
        trace_assistant("msg_u", {"type": "text", "text": "ok"})
    elif prompt == "notify short again":
        cli_streamed("ok", "msg_nsa")
        cli_notify("short", "Compaction failed again", "immediate", 15000, color="error")
    elif prompt == "refuse":
        cli_streamed("refused-partial", "msg_r")
        cli_inform("Opus 5.5's safeguards stopped the response above \u00b7 "
                   "continuing once with that noted", "notice")
        send({"type": "user",
              "message": {"role": "user", "content": [
                  {"type": "text",
                   "text": "Your response above was stopped by a safety classifier."}]},
              "isSynthetic": True, "refused_message_id": "msg_r",
              "parent_tool_use_id": None, "session_id": SESSION_ID, "uuid": trace_uuid()})
        cli_streamed("ok", "msg_r2")
    elif prompt == "refuse fallback":
        cli_streamed("refused-partial", "msg_rf")
        cli_system("model_refusal_fallback", trigger="refusal", direction="retry",
                   scope="session", original_model="claude-opus-5",
                   fallback_model="claude-opus-4-8", request_id=None,
                   content="Opus 5's safeguards flagged this message. Switched to Opus 4.8."
                           "\n\nDetails: `[cyber]` \N{RIGHT-TO-LEFT OVERRIDE}evil")
        cli_streamed("ok", "msg_rf2")
    elif prompt == "refuse none":
        cli_streamed("refused-partial", "msg_rn")
        cli_system("model_refusal_no_fallback", original_model="claude-opus-5-5",
                   request_id=None, content="Opus 5.5's safeguards stopped this response.")
    elif prompt == "inform levels":
        cli_inform("Started in the background: demo", "info")
        cli_inform("Try /compact", "suggestion")
        cli_inform("A hook gave no usable answer, so it didn't check this action.", "warning")
        cli_inform("", "notice")
        cli_streamed("ok", "msg_il")
    elif prompt == "subagent text":
        trace_assistant(trace_uuid(), {"type": "text", "text": "subagent words"},
                        parent="toolu_x", agent_id="agent_x")
        cli_streamed("ok", "msg_sa")
    elif prompt in ("midstream", "midstream drop"):
        message_start("msg_m")
        text("partial-before-error")
        trace_stream({"type": "content_block_stop", "index": 0})
        if prompt == "midstream":
            trace_stream({"type": "message_stop"})
        trace_assistant("msg_1", {"type": "text", "text": "ok"})
    else:
        cli_streamed("ok", "msg_any")
    result()


def cli_notices_prompt(message):
    return trace_prompt_text(message).strip()


# ---------------------------------------------------------------------------
# `failed_turns`: a turn the CLI ends with an `is_error` result, chosen by the
# prompt: `overloaded` is a `success` subtype that says it failed (the API gave
# up), `turn limit` and `budget limit` are the two limits a resend would hit
# again. `unasked interrupt` is a result that reads like a Stop's (`error_during_execution`,
# "Interrupted by user") that nobody asked for. `hold` streams a word and waits for the
# `interrupt` request, then answers it the way the real CLI does: `error_during_execution`,
# `is_error`, no stop reason and no text. `hold late` is `hold` with the answer owed: the CLI
# winds the stopped turn down after Stop freed the composer, so the stopped turn's result is
# written only when the next prompt arrives, ahead of that prompt's own result. Any other
# prompt is an ordinary answer, so a test can tell the client is still usable after a failure.
# The results carry no `stop_reason`, as the CLI's own failures do.
# Every result carries `queued_turn_count`, the prompts read and not yet run, as the real CLI's
# do: `hold late`'s stopped result says 1 (the prompt that made it due is read and waits).
# `hold late bare` is `hold late` with that field left out, the shape of an older or synthetic
# result.
# `fold first` is answered by nothing and `fold second` by one result that closes both
# prompts (`queued_turn_count: 0`): the CLI folds a queued prompt into the running turn.
# `then wake` is answered, and then the CLI starts a turn of its own (no prompt of Sirio's
# behind it, as after a background agent ends) that fails.
# `wake held` is answered, and then the CLI starts a turn of its own and holds it: the next
# prompt is read and queued behind it, so that turn's failing result says 1 is queued, and
# only then is the prompt run.
# ---------------------------------------------------------------------------

FAILED_TURNS = {
    "overloaded": ("success", "API Error: 529 overloaded"),
    "turn limit": ("error_max_turns", "Reached the turn limit"),
    "budget limit": ("error_max_budget_usd", "Reached the budget limit"),
    "unasked interrupt": ("error_during_execution", "Interrupted by user"),
}
HOLDING = threading.Event()
HOLDING_LATE = threading.Event()
STOPPED_RESULT_OWED = threading.Event()
WAKE_RESULT_OWED = threading.Event()
# The `queued_turn_count` of the stopped result `hold late` owes: 1, or None for `hold late bare`.
LATE_QUEUED = {"count": 1}


def with_queued(line, queued):
    if queued is not None:
        line["queued_turn_count"] = queued
    return line


def interrupted_result(queued=0):
    """The answer to a Stop, in the shape the real CLI captured: no `result` text, no stop reason,
    `terminal_reason` `aborted_streaming`."""
    send(with_queued({"type": "result", "subtype": "error_during_execution", "is_error": True,
                      "stop_reason": None, "terminal_reason": "aborted_streaming",
                      "session_id": SESSION_ID, "uuid": trace_uuid()}, queued))


def failure_result(subtype, body, queued=0):
    send(with_queued({"type": "result", "subtype": subtype, "is_error": True, "result": body,
                      "stop_reason": None, "session_id": SESSION_ID, "uuid": trace_uuid()},
                     queued))


def failed_turn(prompt):
    if not INIT_PRINTED:
        init_line()
    if STOPPED_RESULT_OWED.is_set():
        STOPPED_RESULT_OWED.clear()
        interrupted_result(queued=LATE_QUEUED["count"])
    if WAKE_RESULT_OWED.is_set():
        WAKE_RESULT_OWED.clear()
        failure_result(*FAILED_TURNS["overloaded"], queued=1)
    if prompt in ("hold", "hold late", "hold late bare"):
        text("working")
        if prompt == "hold":
            HOLDING.set()
        else:
            LATE_QUEUED["count"] = None if prompt == "hold late bare" else 1
            HOLDING_LATE.set()
        return
    if prompt == "fold first":
        text("first ")
        return
    if prompt == "fold second":
        text("second")
        result(queued=0)
        return
    if prompt == "then wake":
        text("ok")
        result()
        text("The agent finished. ")
        failure_result(*FAILED_TURNS["overloaded"])
        return
    if prompt == "wake held":
        text("ok")
        result()
        text("The agent finished. ")
        WAKE_RESULT_OWED.set()
        return
    if prompt not in FAILED_TURNS:
        text("ok")
        result()
        return
    failure_result(*FAILED_TURNS[prompt])


# ---------------------------------------------------------------------------
# `thinking_blocks`: a reply streamed block by block, as the Messages API does:
# a thinking block (index 0) and a text block (index 1) of the first message, then a second
# message whose text block reuses index 0. Then two messages that leave a thinking block open
# (index 2 in one that ends, index 3 in one that is cut off by the next message's start) and
# stops that index outside them: a stop whose block belongs to a message that is over is not
# a thought's end. That is what emptying the map at a message's end and start is for.
# ---------------------------------------------------------------------------


def block_event(kind, index, **fields):
    trace_stream({"type": kind, "index": index, **fields})


def thinking_blocks_turn():
    if not INIT_PRINTED:
        init_line()
    message_start("msg_thinking")
    block_event("content_block_start", 0,
                content_block={"type": "thinking", "thinking": "", "signature": ""})
    for piece in ("weighing ", "the options"):
        block_event("content_block_delta", 0,
                    delta={"type": "thinking_delta", "thinking": piece})
    block_event("content_block_stop", 0)
    block_event("content_block_start", 1, content_block={"type": "text", "text": ""})
    block_event("content_block_delta", 1, delta={"type": "text_delta", "text": "first answer"})
    block_event("content_block_stop", 1)
    trace_assistant("msg_thinking", {"type": "text", "text": "first answer"})
    message_end()
    message_start("msg_second")
    block_event("content_block_start", 0, content_block={"type": "text", "text": ""})
    block_event("content_block_delta", 0, delta={"type": "text_delta", "text": " second answer"})
    block_event("content_block_stop", 0)
    trace_assistant("msg_second", {"type": "text", "text": " second answer"})
    message_end()
    message_start("msg_left_ended")
    block_event("content_block_start", 2,
                content_block={"type": "thinking", "thinking": "", "signature": ""})
    message_end()
    block_event("content_block_stop", 2)
    message_start("msg_left_cut_off")
    block_event("content_block_start", 3,
                content_block={"type": "thinking", "thinking": "", "signature": ""})
    message_start("msg_after_cut_off")
    block_event("content_block_stop", 3)
    message_end()
    result()


# ---------------------------------------------------------------------------
# `--fixture-startup=<kind>`: a CLI that cannot start. Unless a kind says
# otherwise it writes at once, never reads stdin, and exits 1. Shapes copy the
# live probe (`.superpowers/probe-a7`, Q5).
# ---------------------------------------------------------------------------


def startup_failure_result(reason, errors, **extra):
    line = {"type": "result", "subtype": "error_during_execution", "is_error": True,
            "startup_failure_reason": reason, "duration_ms": 0, "errors": errors,
            "session_id": SESSION_ID, "uuid": "startup-uuid"}
    line.update(extra)
    send(line)


def startup_main(kind):
    """Returns only for a kind that carries on as a normal session."""
    opted_in = os.environ.get("CLAUDE_CODE_STARTUP_FAILURE_RESULTS") == "1"
    if kind == "config_invalid":
        startup_failure_result(
            "config_file_invalid",
            ["Configuration error in /fixture/config/.claude.json: JSON Parse error: "
             "Expected '}'\nFix the JSON, or move the file aside.",
             "bidi \N{RIGHT-TO-LEFT OVERRIDE}here"],
            invalid_config_file={"path": "/fixture/config/.claude.json", "is_global": True,
                                 "backup_searched": True,
                                 "backup_path": "/fixture/config/backups/.claude.json.bak"})
    elif kind in ("cwd_unavailable", "future_reason"):
        reason, line = (
            ("cwd_unavailable", "The current directory no longer exists (it was deleted or moved).")
            if kind == "cwd_unavailable"
            else ("some_future_reason", "A reason this build does not know."))
        if opted_in:
            startup_failure_result(reason, [line])
        else:
            sys.stderr.write(line + "\n")
            sys.stderr.flush()
    elif kind == "no_result":
        sys.stderr.write("fixture: first line\nfixture: last line\n")
        sys.stderr.flush()
    elif kind == "resume_reason":
        if "--resume" not in sys.argv:
            return
        startup_failure_result("session_held_by_background",
                               ["Session is running in the background."])
    elif kind in ("resume_no_reason", "no_reason"):
        if kind == "resume_no_reason" and "--resume" not in sys.argv:
            return
        # By hand, as live (`run-q5-cur-resume-missing`): no `result` key and
        # no reason, with `modelUsage`, `total_cost_usd` and `num_turns`.
        errors = (["No conversation found with session ID: fixture"]
                  if kind == "resume_no_reason"
                  else ["Something went wrong before the session."])
        send({"type": "result", "subtype": "error_during_execution", "is_error": True,
              "duration_ms": 0, "num_turns": 0, "stop_reason": None,
              "session_id": SESSION_ID, "total_cost_usd": 0, "modelUsage": {},
              "errors": errors, "uuid": "startup-uuid"})
    else:
        return
    sys.stdout.flush()
    os._exit(1)


# ---------------------------------------------------------------------------
# `richer`: the turns of Scripts/Tests/test-chat-richer-messages-e2e.sh. The prompt picks the turn (the
# stub's header, Scripts/Tests/stubs/richer/claude, lists them). A turn runs on its own thread, so control
# requests (the `interrupt` of Stop above all) are answered while it streams, and a Stop ends it at once with
# the result the real CLI writes: `error_during_execution`, `is_error`, no text.
#
# What a retry resends is the same prompt, so a turn that fails does so on the FIRST prompt of that text only,
# counted over every process in $SIRIO_STUB_LOG (`prompt\t<pid>\t<json text>` lines the fixture appends as it
# reads each prompt): the second one answers. `<prompt> hold` waits for the file `$SIRIO_FIXTURE_GATE.<n>`
# (n the prompt's number) before it fails or answers, so a scenario can queue messages behind a running turn.
# ---------------------------------------------------------------------------

RICHER_STOP = threading.Event()
RICHER_CHUNKS = 200
RICHER_CHUNK_SECONDS = 0.02
RICHER_PARTIAL = (
    "I started on it. Here is **what I found** so far, with `code` and a list:\n\n"
    "- the first point, written out\n- the second point\n\n"
    "The next step is"
)
# What `max tokens` writes before the model runs out of room, and the chunks of `slow` (40 of them, 0.2 s apart).
RICHER_CUT = "The answer is long, so it is cut off right in the middle of a sen"
RICHER_SLOW_CHUNKS = [f"slow reply part {number}. " for number in range(1, 41)]
RICHER_SLOW_PAUSE = 0.2


def richer_log(kind, text=""):
    path = os.environ.get("SIRIO_STUB_LOG")
    if path:
        with open(path, "a", encoding="utf-8") as log:
            log.write(f"{kind}\t{os.getpid()}\t{json.dumps(text)}\n")


def richer_seen(text):
    """How many prompts of exactly this text the log already holds (every process)."""
    path = os.environ.get("SIRIO_STUB_LOG")
    if not path or not os.path.exists(path):
        return 0
    wanted = json.dumps(text)
    count = 0
    with open(path, encoding="utf-8") as log:
        for line in log:
            parts = line.rstrip("\n").split("\t", 2)
            if len(parts) == 3 and parts[0] == "prompt" and parts[2] == wanted:
                count += 1
    return count


def richer_showcase():
    """The SHOWCASE document, then parts of a few blocks each (they freeze as they arrive, so the reply
    reveals) until the reply is 40 KiB."""
    here = os.path.dirname(os.path.realpath(__file__))
    path = os.path.join(here, *([".."] * 5), "Scripts", "Tests", "fixtures", "markdown-documents", "showcase.md")
    with open(path, encoding="utf-8") as handle:
        text = handle.read().replace("{{REMOTE_PORT}}", "9")
    part = 0
    while len(text.encode("utf-8")) < 40 * 1024:
        part += 1
        text += (
            f"\n\n## Part {part}\n\nA paragraph of part {part}, with **bold**, `code` and a "
            f"[link](https://example.com/{part}). Its marker is part-{part:03d}-end.\n\n"
            "- the first point\n- the second\n"
        )
    return text


def richer_showcase_x8():
    """SHOWCASE repeated eight times: every copy mentions `[^1]` again, so the reply cannot freeze."""
    here = os.path.dirname(os.path.realpath(__file__))
    path = os.path.join(here, *([".."] * 5), "Scripts", "Tests", "fixtures", "markdown-documents", "showcase.md")
    with open(path, encoding="utf-8") as handle:
        text = handle.read().replace("{{REMOTE_PORT}}", "9")
    return (text + "\n\n") * 8


def richer_compact():
    """28 KiB with no blank line (headings on their text, `Run:` on its fence): the stream never freezes a block of
    it, so past the reveal's 7 KiB tail budget the reveal hands over to drawing the reply as it arrives. (A debug
    build builds the whole reply for every chunk of such a reply: this one arrives in 70 chunks 150 ms apart, so that
    the UI thread has idle time between chunks and the control socket is answered while it streams.)"""
    text, step = "", 0
    while len(text.encode("utf-8")) < 28 * 1024:
        step += 1
        text += (
            f"## Step {step}\nRead `src/lib.rs` and **check** the [notes](https://example.com/{step}).\n"
            "The second line carries on, with _emphasis_ and a list under it.\n"
            f"- the first point of step {step}\n- the second point, with `inline code`\n"
            f"Run:\n```sh\ncargo test -p sirio_ui step_{step}\n```\n"
        )
    return text


def richer_wait(seconds):
    """Sleeps up to `seconds`; False when a Stop came in meanwhile."""
    return not RICHER_STOP.wait(seconds)


def richer_gate_file(name):
    """The path of the gate file `name` ($SIRIO_FIXTURE_GATE.<name>), None when the run names no gate."""
    base = os.environ.get("SIRIO_FIXTURE_GATE")
    return f"{base}.{name}" if base else None


def richer_wait_for_file(path, seconds=120):
    """Holds until `path` exists, however a Stop came in meanwhile (`richer_wait` returns at once after one, which is
    no wait for the file); gives up after `seconds`."""
    deadline = time.time() + seconds
    while path and not os.path.exists(path) and time.time() < deadline:
        time.sleep(0.05)


def richer_gate(number):
    """Holds until the gate file of this prompt's number exists; False when a Stop came in."""
    path = richer_gate_file(number)
    while path and not os.path.exists(path):
        if not richer_wait(0.05):
            return False
    return True


def richer_block(index, kind, **fields):
    trace_stream({"type": kind, "index": index, **fields})


def richer_open_text(message_id, index=0):
    message_start(message_id)
    richer_block(index, "content_block_start", content_block={"type": "text", "text": ""})


def richer_delta(piece, index=0):
    richer_block(index, "content_block_delta", delta={"type": "text_delta", "text": piece})


def richer_close_text(message_id, body, index=0):
    richer_block(index, "content_block_stop")
    trace_assistant(message_id, {"type": "text", "text": body})
    message_end()


def richer_stream(message_id, chunks, pause):
    """A text block delivered chunk by chunk; False when a Stop cut it short (nothing is closed then)."""
    richer_open_text(message_id)
    for chunk in chunks:
        richer_delta(chunk)
        if not richer_wait(pause):
            return False
    richer_close_text(message_id, "".join(chunks))
    return True


def richer_chunks(text, count):
    size = max(1, -(-len(text) // count))
    return [text[at:at + size] for at in range(0, len(text), size)]


def richer_interrupted():
    interrupted_result()


def richer_failed(text_body):
    failure_result("success", text_body)


def richer_reply(message_id, body):
    if richer_stream(message_id, [body], 0):
        result()


def richer_limit_epoch():
    path = os.environ.get("SIRIO_FIXTURE_LIMIT_FILE")
    with open(path, encoding="utf-8") as handle:
        return int(handle.read().strip())


def richer_turn(prompt):
    number = richer_seen(prompt) + 1
    richer_log("prompt", prompt)
    RICHER_STOP.clear()
    # `<prompt> hold` waits for its gate; so does a `Continue` while the file $SIRIO_FIXTURE_GATE.hold-continue exists
    # (the turn a Continue starts has the text `Continue`, which the E2E cannot suffix).
    held_continue = prompt == "Continue" and os.path.exists(richer_gate_file("hold-continue") or "")
    hold = prompt.endswith(" hold") or held_continue
    base = prompt[: -len(" hold")] if prompt.endswith(" hold") else prompt
    requesting()
    if base in ("showcase", "showcase slow", "showcase x8", "compact"):
        body = {"showcase x8": richer_showcase_x8, "compact": richer_compact}.get(base, richer_showcase)()
        # `showcase slow` is the same reply at 40 ms a chunk (8 s), long enough to select and copy while it streams;
        # `compact` is 70 chunks 150 ms apart.
        pause = {"showcase slow": RICHER_CHUNK_SECONDS * 2, "compact": 0.15}.get(base, RICHER_CHUNK_SECONDS)
        if not richer_stream("msg_showcase", richer_chunks(body, 70 if base == "compact" else RICHER_CHUNKS), pause):
            richer_interrupted()
            return
        result()
    elif base == "fail":
        if number == 1:
            richer_open_text("msg_fail")
            for piece in richer_chunks(RICHER_PARTIAL, 3):
                richer_delta(piece)
            if hold and not richer_gate(number):
                richer_interrupted()
                return
            richer_failed("API Error: 529 overloaded")
        else:
            if hold and not richer_gate(number):
                richer_interrupted()
                return
            richer_reply("msg_ok", "recovered")
    elif base == "fail max turns":
        richer_open_text("msg_max")
        richer_delta("Working on it.")
        failure_result("error_max_turns", "Reached the turn limit")
    elif base == "max tokens":
        # The reply is cut where the model ran out of room: a closed text block, a message that stopped at the limit
        # and a result that says so (not an error: the turn ended, short).
        richer_open_text("msg_cut")
        for piece in richer_chunks(RICHER_CUT, 3):
            richer_delta(piece)
        richer_block(0, "content_block_stop")
        trace_assistant("msg_cut", {"type": "text", "text": RICHER_CUT})
        message_end(stop_reason="max_tokens")
        result(text_body=RICHER_CUT, stop_reason="max_tokens")
    elif base in ("slow", "slow late"):
        # A reply that streams for 8 s, so that a Stop lands in the middle of it. `slow late` answers the Stop only once
        # the file $SIRIO_FIXTURE_GATE.late exists: the stopped turn's late end, which a scenario holds back to send
        # another prompt in between.
        if not richer_stream("msg_slow", RICHER_SLOW_CHUNKS, RICHER_SLOW_PAUSE):
            if base == "slow late":
                richer_wait_for_file(richer_gate_file("late"))
            richer_interrupted()
            return
        result()
    elif base == "think slow":
        message_start("msg_think")
        richer_block(0, "content_block_start", content_block={"type": "thinking", "thinking": "", "signature": ""})
        for piece in ("weighing ", "the options ", "once more"):
            richer_block(0, "content_block_delta", delta={"type": "thinking_delta", "thinking": piece})
            if not richer_wait(0.15):
                richer_interrupted()
                return
        richer_block(0, "content_block_stop")
        # The thought is over; the reply's first text is 2 s away.
        if not richer_wait(2.0):
            richer_interrupted()
            return
        richer_block(1, "content_block_start", content_block={"type": "text", "text": ""})
        richer_delta("done thinking", 1)
        richer_block(1, "content_block_stop")
        trace_assistant("msg_think", {"type": "text", "text": "done thinking"})
        message_end()
        result()
    elif base == "die":
        if number == 1:
            richer_open_text("msg_die")
            for piece in richer_chunks(RICHER_PARTIAL, 3):
                richer_delta(piece)
            sys.stdout.flush()
            sys.stderr.write("richer fixture: dying mid-stream\n")
            sys.stderr.flush()
            os._exit(3)
        richer_reply("msg_ok", "recovered")
    elif base in ("limit rejected", "limit event"):
        # The reset is read when the event is written (after the gate), so a scenario sets it just before.
        if hold and not richer_gate(number):
            richer_interrupted()
            return
        if base == "limit event" or number == 1:
            epoch = richer_limit_epoch()
            info = {"status": "rejected", "rateLimitType": "five_hour", "resetsAt": epoch,
                    "unifiedWindows": {"five_hour": {"utilization": 1, "resetsAt": epoch}}}
        if base == "limit event":
            # The plan reached its limit but the turn went on: a rejected event and no failed turn.
            cli_limit_event(info)
            richer_reply("msg_ok", "ok")
        elif number == 1:
            cli_limit_refused("You've hit your limit \u00b7 resets 3:15pm (UTC)", info)
        else:
            richer_reply("msg_ok", "recovered")
    elif base == "replyfile":
        with open(os.environ["SIRIO_FIXTURE_REPLY_FILE"], encoding="utf-8") as handle:
            body = handle.read()
        richer_reply("msg_replyfile", body)
    else:
        if hold and not richer_gate(number):
            richer_interrupted()
            return
        richer_reply("msg_ok", "ok")


def richer_turns(prompts):
    while True:
        prompt = prompts.get()
        if prompt is None:
            return
        richer_turn(prompt)


def richer_main():
    richer_log("start", " ".join(sys.argv[1:]))
    init_line()
    prompts = queue.Queue()
    worker = threading.Thread(target=richer_turns, args=(prompts,), daemon=True)
    worker.start()
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
                control_response(request_id, main_initialize_payload())
            elif subtype == "interrupt":
                control_response(request_id, {"still_queued": 0})
                RICHER_STOP.set()
            elif subtype == "get_context_usage":
                control_response(request_id, {
                    "totalTokens": 48000, "rawMaxTokens": 200000, "maxTokens": 180000,
                    "percentage": 24, "model": TRACE_MODEL, "categories": []})
            elif subtype == "mcp_status":
                control_response(request_id, {"mcpServers": []})
            elif subtype in ("mcp_toggle", "reload_plugins", "reload_skills"):
                control_error(request_id, f"Unsupported control request subtype: {subtype}")
            elif subtype == "set_permission_mode":
                control_response(request_id, {"mode": request.get("mode")})
            else:
                control_response(request_id, {})
        elif kind == "user":
            prompts.put(trace_prompt_text(message).strip())
    prompts.put(None)
    worker.join()


def main():
    if LAUNCH_LOG_PATH:
        with open(LAUNCH_LOG_PATH, "a", encoding="utf-8") as log:
            log.write(json.dumps({
                "resume": resume_arg(),
                "session_id": argv_value("--session-id"),
                "mcp_apps_host": os.environ.get("CLAUDE_CODE_MCP_APPS_HOST"),
            }) + "\n")
    if flag_value("fixture-startup"):
        startup_main(flag_value("fixture-startup"))
    if MODE == "richer":
        richer_main()
        return
    if MODE == "trace":
        trace_main()
        return
    if MODE == "session_state":
        state_main()
        return
    if MODE == "background_tasks":
        bg_main()
        return
    if MODE == "refuse_unknown_resume" and resume_arg() not in (None, SESSION_ID):
        # Only this fixture's own session exists: the resume of any other id
        # is refused, one of its own is continued.
        sys.stderr.write("fixture: unknown session\n")
        sys.stderr.flush()
        os._exit(1)
    if MODE in ("refuse_resume", "refuse_resume_early_init") and "--resume" in sys.argv:
        # The session file is gone: refuse the resume the way the real CLI
        # does, so the worker must come back without the id.
        sys.stderr.write("fixture: unknown session\n")
        sys.stderr.flush()
        os._exit(1)
    CURRENT_COMMANDS.extend(initialize_payload()["commands"])
    resumed = resume_arg()
    if resumed and resumed in read_title_store():
        # A resumed session that has a stored title says so first (Q7).
        title_echo(read_title_store()[resumed], resumed)
    if MODE == "refuse_resume_early_init":
        # The fresh session the worker comes back with names itself in an
        # unprompted init, before the handshake is answered.
        init_line()
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
                if MODE == "early_request":
                    # Requests the CLI sends before the host's handshake is
                    # answered: the host must refuse each, never drop it.
                    ELIC_ISSUED.update(["early-e", "early-p"])
                    send({"type": "control_request", "request_id": "early-e",
                          "request": {"subtype": "elicitation", "mcp_server_name": "elic",
                                      "mode": "form", "message": "too early",
                                      "requested_schema": {"type": "object", "properties": {}}}})
                    send({"type": "control_request", "request_id": "early-p",
                          "request": {"subtype": "can_use_tool", "tool_name": "Bash",
                                      "tool_use_id": "early-t", "input": {},
                                      "permission_suggestions": []}})
                if MODE == "malformed":
                    sys.stdout.write("not-json\n")
                    sys.stdout.flush()
                wait_for_handshake_gate()
                control_response(request_id, main_initialize_payload())
                if CLI_BUSY_GATE_PATH:
                    threading.Thread(target=cli_busy_until_gate, daemon=True).start()
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
                if MODE == "failed_turns" and HOLDING.is_set():
                    HOLDING.clear()
                    interrupted_result()
                elif MODE == "failed_turns" and HOLDING_LATE.is_set():
                    HOLDING_LATE.clear()
                    STOPPED_RESULT_OWED.set()
                if MODE in ("slow_turn", "silent_turn", "chatty_slow_turn"):
                    interrupted_result()
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
                with PANEL_LOCK:
                    unread = UNREAD_NEXT[0]
                    UNREAD_NEXT[0] = False
                if unread:
                    control_error(request_id, "mcp_status: the servers could not be read")
                    continue
                control_response(
                    request_id, mcp_status_payload({"mcpServers": session_panel_servers()}))
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
            elif subtype == "rename_session":
                answer_rename(request_id, request)
            elif subtype == "list_permission_rules":
                answer_permission_rules(request_id)
            elif subtype == "get_hooks_listing":
                answer_hooks_listing(request_id)
            elif subtype == "mcp_toggle":
                if TOGGLE_GATE_PATH and not os.path.exists(TOGGLE_GATE_PATH):
                    threading.Thread(target=toggle_when_gated,
                                     args=(request_id, request), daemon=True).start()
                else:
                    answer_toggle(request_id, request)
            elif subtype == "reload_plugins":
                if RELOAD_GATE_PATH and not os.path.exists(RELOAD_GATE_PATH):
                    threading.Thread(target=reload_when_gated,
                                     args=(request_id, request), daemon=True).start()
                else:
                    answer_reload_plugins(request_id, request)
            elif subtype == "reload_skills":
                answer_reload_skills(request_id)
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
            if MODE in ("elicitation", "early_request"):
                elicitation_answered(message)
            elif cli_request_id in PENDING:
                finish_permission(cli_request_id, message)
        elif kind == "user":
            if USER_LOG_PATH:
                with open(USER_LOG_PATH, "a", encoding="utf-8") as log:
                    log.write(json.dumps(message, separators=(",", ":")) + "\n")
            if MODE == "elicitation":
                elicitation_prompt(message)
                continue
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
                if MODE == "session_panel" and RESULT_GATE_PATH:
                    threading.Thread(target=result_when_gated, daemon=True).start()
                else:
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
            if MODE == "font_sheet":
                # One fixed answer to any prompt, for the font setting's E2E
                # (Scripts/Tests/test-font-setting-e2e.sh): prose with emphasis and
                # inline code, a list, a Rust block with the characters a face must
                # tell apart, and a text block with a file tree, arrows, a check, a
                # bullet, a lambda and a greater-or-equal sign.
                init_line()
                text(
                    "Here is a **fixed answer** with *emphasis*, `inline code` and a "
                    "paragraph long enough to wrap onto a second line, so that every "
                    "face is judged on running text as well as on single words.\n\n"
                    "- first item, with **bold**\n"
                    "- second item, with *italic*\n"
                    "- third item, with `src/main.rs`\n\n"
                    "```rust\n"
                    "0O 1lI {}[]() -> != ==\n"
                    "fn main() { let total = 0; }\n"
                    "```\n\n"
                    "```text\n"
                    "├── src\n"
                    "│   └── main.rs\n"
                    "└── README.md\n"
                    "→ ← ✓ ● λ ≥\n"
                    "```\n"
                )
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
            if MODE == "failed_turns":
                failed_turn(cli_notices_prompt(message))
                continue
            if MODE == "thinking_blocks":
                thinking_blocks_turn()
                continue
            if MODE == "cli_notices":
                if not INIT_PRINTED:
                    init_line()
                prompt = cli_notices_prompt(message)
                if RETRY_GATE_PATH and prompt == "retry twice":
                    threading.Thread(target=cli_notices_turn, args=(prompt,),
                                     daemon=True).start()
                else:
                    cli_notices_turn(prompt)
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
