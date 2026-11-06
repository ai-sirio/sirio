# Stand-in for the Anthropic Messages API, enough for a Claude Code turn.
# usage: fake_messages.py PORT HOLD_SECONDS
# Holds each /v1/messages request HOLD seconds (so "working" can be observed), then streams
# either "ok" or, when the latest user turn says "run tool" and carries no tool result yet,
# a Bash tool call that sleeps 3 s and touches `probe-file` in Claude's cwd.
# Binds 127.0.0.1 only. Nothing here is a credential.
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HOLD = float(sys.argv[2])
TOOL_INPUT = {"command": "sleep 3; touch probe-file", "description": "Create a probe file"}


def sse(event, data):
    return f"event: {event}\ndata: {json.dumps(data)}\n\n".encode()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        length = int(self.headers.get("content-length") or 0)
        try:
            request = json.loads(self.rfile.read(length) or b"{}")
        except Exception:
            request = {}
        if not self.path.startswith("/v1/messages") or "count_tokens" in self.path:
            self.send_response(404)
            self.end_headers()
            return
        messages = request.get("messages") or []
        # The latest user turn (Claude appends system messages after it).
        users = [m for m in messages if m.get("role") == "user"]
        last = json.dumps(users[-1]) if users else ""
        want_tool = bool(request.get("tools")) and "run tool" in last and "tool_result" not in last
        print("POST", self.path, "messages", len(messages), "tool" if want_tool else "text", flush=True)
        time.sleep(HOLD)
        message = {"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
                   "content": [], "stop_reason": None, "stop_sequence": None,
                   "usage": {"input_tokens": 1, "output_tokens": 1}}
        if not request.get("stream"):
            message.update(content=[{"type": "text", "text": "ok"}], stop_reason="end_turn")
            body = json.dumps(message).encode()
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        if want_tool:
            block = {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {}}
            delta = {"type": "input_json_delta", "partial_json": json.dumps(TOOL_INPUT)}
            stop = "tool_use"
        else:
            block = {"type": "text", "text": ""}
            delta = {"type": "text_delta", "text": "ok"}
            stop = "end_turn"
        for event, data in [
            ("message_start", {"type": "message_start", "message": message}),
            ("content_block_start", {"type": "content_block_start", "index": 0, "content_block": block}),
            ("content_block_delta", {"type": "content_block_delta", "index": 0, "delta": delta}),
            ("content_block_stop", {"type": "content_block_stop", "index": 0}),
            ("message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None},
                               "usage": {"output_tokens": 1}}),
            ("message_stop", {"type": "message_stop"}),
        ]:
            self.wfile.write(sse(event, data))
            self.wfile.flush()


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
