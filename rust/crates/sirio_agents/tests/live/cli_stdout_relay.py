# A transparent stdout relay for the live MCP Apps conformance test (acp_conformance.rs).
# It runs the CLI with stdin inherited (the client's pipe goes straight to the CLI) and stdout
# piped, relays every stdout line unchanged and flushed, and logs only structure as JSON lines:
# the capabilities of `system/init`, for each `can_use_tool` request the tool-use id's
# prefix, the tool name and the input, and for each tool result its error flag and the type of
# the line's `tool_use_result`. Opens no socket and uses no credentials.
# usage: python -I cli_stdout_relay.py LOG_PATH CLI [ARGS...]
import json
import signal
import subprocess
import sys

LOG = sys.argv[1]
PREFIX = "toolu_mcp_call_"


def log(obj):
    with open(LOG, "a") as f:
        f.write(json.dumps(obj) + "\n")


child = subprocess.Popen(sys.argv[2:], stdout=subprocess.PIPE)


def forward(signum, _frame):
    child.send_signal(signum)


signal.signal(signal.SIGTERM, forward)
signal.signal(signal.SIGINT, forward)

for raw in child.stdout:
    sys.stdout.buffer.write(raw)
    sys.stdout.buffer.flush()
    try:
        msg = json.loads(raw)
    except Exception:
        continue
    if msg.get("type") == "system" and msg.get("subtype") == "init":
        log({"init_capabilities": msg.get("capabilities") or []})
    elif msg.get("type") == "user":
        # Structure only, never a value: for each tool result, its error flag and the type of the
        # line's `tool_use_result`.
        blocks = (msg.get("message") or {}).get("content")
        if isinstance(blocks, list):
            for block in blocks:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    log({"result": {
                        "tool_use_id": block.get("tool_use_id"),
                        "is_error": block.get("is_error"),
                        "tool_use_result_type": type(msg.get("tool_use_result")).__name__}})
    elif msg.get("type") == "control_request":
        req = msg.get("request") or {}
        if req.get("subtype") == "can_use_tool":
            use_id = req.get("tool_use_id") or ""
            log({"ask": {"id_prefix": PREFIX if use_id.startswith(PREFIX) else "other",
                         "tool_name": req.get("tool_name"), "input": req.get("input")}})

sys.exit(child.wait())
