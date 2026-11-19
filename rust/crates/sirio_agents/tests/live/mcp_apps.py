# A stdio MCP server for the live MCP Apps conformance test (acp_conformance.rs).
# Tools: `show` (model and app), `act` (app only), `model_only` (model only). `show` and
# `act` name the widget `ui://apps/show.html`, whose resource carries a csp with `.test`
# hosts. Every `initialize` the client sends is logged as one JSON line, so the test can read
# the capabilities the CLI declared. Speaks only over stdin/stdout, opens no socket and uses
# no credentials.
# usage: mcp_apps.py LOG_PATH
import json
import sys
import threading

LOG = sys.argv[1]
WRITE = threading.Lock()
MIME = "text/html;profile=mcp-app"
URI = "ui://apps/show.html"
SCHEMA = {"type": "object", "properties": {"text": {"type": "string"}, "step": {"type": "number"}}}


def log(obj):
    with open(LOG, "a") as f:
        f.write(json.dumps(obj) + "\n")


def send(obj):
    with WRITE:
        sys.stdout.write(json.dumps(obj) + "\n")
        sys.stdout.flush()


def tool(name, ui):
    return {"name": name, "description": "tool " + name, "inputSchema": SCHEMA, "_meta": {"ui": ui}}


TOOLS = [
    tool("show", {"resourceUri": URI, "visibility": ["model", "app"]}),
    tool("act", {"resourceUri": URI, "visibility": ["app"]}),
    tool("model_only", {"visibility": ["model"]}),
]

RESOURCE = {
    "uri": URI,
    "mimeType": MIME,
    "text": "<html><body><h1>apps widget</h1></body></html>",
    "_meta": {"ui": {"csp": {"connectDomains": ["https://api.widget.test"],
                             "resourceDomains": ["https://cdn.widget.test"]},
                     "prefersBorder": True}},
}


def handle(msg):
    mid, method, params = msg.get("id"), msg.get("method"), msg.get("params") or {}
    if method == "initialize":
        log({"initialize": params})
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "protocolVersion": params.get("protocolVersion", "2025-06-18"),
            "capabilities": {"tools": {}, "resources": {}},
            "serverInfo": {"name": "apps", "version": "0"}}})
    elif method == "tools/list":
        send({"jsonrpc": "2.0", "id": mid, "result": {"tools": TOOLS}})
    elif method == "resources/list":
        send({"jsonrpc": "2.0", "id": mid, "result": {"resources": [
            {"uri": URI, "name": "show.html", "mimeType": MIME}]}})
    elif method == "resources/read":
        if params.get("uri") == URI:
            send({"jsonrpc": "2.0", "id": mid, "result": {"contents": [RESOURCE]}})
        else:
            send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32002, "message": "not found"}})
    elif method == "tools/call":
        args = params.get("arguments") or {}
        log({"call": params.get("name"), "arguments": args})
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "content": [{"type": "text", "text": "ran " + str(params.get("name"))}],
            "structuredContent": {"tool": params.get("name"), "args": args}}})
    elif mid is not None:
        send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "no"}})


for line in sys.stdin:
    try:
        m = json.loads(line)
    except Exception:
        continue
    handle(m)
