# A stdio MCP server for the live elicitation conformance test (acp_conformance.rs).
# Its one tool, `ask`, sends the client an `elicitation/create` form request and returns
# whatever answer came back as the tool's text, so the test can read it in the tool_result.
# Speaks only over stdin/stdout, opens no socket and uses no credentials.
# usage: elicit_mcp.py LOG_PATH
import json, sys, threading

LOG = open(sys.argv[1], "a")


def log(*a):
    print(*a, file=LOG, flush=True)


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()
    log("SEND", json.dumps(obj))


pending = {}


def handle_call(req):
    eid = "e-" + str(req["id"])
    params = {
        "mode": "form",
        "message": "Pick a colour for the probe",
        "requestedSchema": {
            "type": "object",
            "properties": {
                "colour": {"type": "string", "title": "Colour", "enum": ["red", "blue"]},
                "count": {"type": "integer", "title": "How many", "minimum": 1},
                "note": {"type": "string", "description": "free text", "format": "email"},
                "ok": {"type": "boolean", "default": True},
            },
            "required": ["colour"],
        },
    }
    ev = threading.Event()
    pending[eid] = [ev, None]
    send({"jsonrpc": "2.0", "id": eid, "method": "elicitation/create", "params": params})
    ev.wait(600)
    answer = pending.pop(eid)[1]
    send({"jsonrpc": "2.0", "id": req["id"],
          "result": {"content": [{"type": "text", "text": "elicitation answer: " + json.dumps(answer)}]}})


for line in sys.stdin:
    log("RECV", line.strip())
    try:
        msg = json.loads(line)
    except Exception:
        continue
    if "method" in msg and "id" in msg:
        m = msg["method"]
        if m == "initialize":
            send({"jsonrpc": "2.0", "id": msg["id"], "result": {
                "protocolVersion": msg["params"]["protocolVersion"],
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "elic", "version": "0.0.1"}}})
        elif m == "tools/list":
            send({"jsonrpc": "2.0", "id": msg["id"], "result": {"tools": [{
                "name": "ask", "description": "Ask the user something",
                "inputSchema": {"type": "object", "properties": {}}}]}})
        elif m == "tools/call":
            threading.Thread(target=handle_call, args=(msg,), daemon=True).start()
        else:
            send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
    elif "id" in msg and ("result" in msg or "error" in msg):
        p = pending.get(msg["id"])
        if p:
            p[1] = msg
            p[0].set()
