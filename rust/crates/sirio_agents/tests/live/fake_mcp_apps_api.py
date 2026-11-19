# Stand-in for the Anthropic Messages API, for the live MCP Apps conformance test
# (acp_conformance.rs). When the latest user turn says "run apps" and carries no tool
# result yet, it streams a call to the MCP tool `mcp__apps__show` with {"text":"hi"};
# otherwise it streams "ok".
# usage: fake_mcp_apps_api.py PORT
# Binds 127.0.0.1 only. Nothing here is a credential.
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def sse(e, d):
    return f"event: {e}\ndata: {json.dumps(d)}\n\n".encode()


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        n = int(self.headers.get("content-length") or 0)
        try:
            req = json.loads(self.rfile.read(n) or b"{}")
        except Exception:
            req = {}
        if not self.path.startswith("/v1/messages") or "count_tokens" in self.path:
            self.send_response(404)
            self.end_headers()
            return
        users = [m for m in req.get("messages") or [] if m.get("role") == "user"]
        last = json.dumps(users[-1]) if users else ""
        want = "run apps" in last and "tool_result" not in last
        msg = {"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
               "content": [], "stop_reason": None, "stop_sequence": None,
               "usage": {"input_tokens": 1, "output_tokens": 1}}
        if not req.get("stream"):
            msg.update(content=[{"type": "text", "text": "ok"}], stop_reason="end_turn")
            b = json.dumps(msg).encode()
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(b)))
            self.end_headers()
            self.wfile.write(b)
            return
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        if want:
            block = {"type": "tool_use", "id": "toolu_a1", "name": "mcp__apps__show", "input": {}}
            delta = {"type": "input_json_delta", "partial_json": "{\"text\": \"hi\"}"}
            stop = "tool_use"
        else:
            block = {"type": "text", "text": ""}
            delta = {"type": "text_delta", "text": "ok"}
            stop = "end_turn"
        for e, d in [
            ("message_start", {"type": "message_start", "message": msg}),
            ("content_block_start", {"type": "content_block_start", "index": 0, "content_block": block}),
            ("content_block_delta", {"type": "content_block_delta", "index": 0, "delta": delta}),
            ("content_block_stop", {"type": "content_block_stop", "index": 0}),
            ("message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None}, "usage": {"output_tokens": 1}}),
            ("message_stop", {"type": "message_stop"}),
        ]:
            self.wfile.write(sse(e, d))
            self.wfile.flush()


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
