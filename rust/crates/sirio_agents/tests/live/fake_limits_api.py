# Stand-in for the Anthropic Messages API, for the live usage and rate-limit tests
# (acp_conformance.rs, claude_answers_usage_locally and
# claude_reports_rate_limits_through_a_gateway).
# Every /v1/messages request streams "ok", or is refused with HTTP 429 and a rate_limit_error
# body. Which one, and the response headers, come from LIMITS_FILE, re-read on every request:
#   {"status": 200 | 429, "headers": {"anthropic-ratelimit-unified-status": "allowed_warning", ...}}
# Every request, of any method, appends {"method": ..., "path": ...} to LIMITS_FILE + ".requests".
# Nothing else of a
# request is logged: no header (the bearer token lives in one) and no body.
# usage: fake_limits_api.py PORT LIMITS_FILE
# Binds 127.0.0.1 only. Nothing here is a credential.
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LIMITS = sys.argv[2]


def sse(event, data):
    return f"event: {event}\ndata: {json.dumps(data)}\n\n".encode()


def limits():
    try:
        with open(LIMITS) as f:
            return json.load(f)
    except Exception:
        return {}


def log_request(method, path):
    with open(LIMITS + ".requests", "a") as f:
        f.write(json.dumps({"method": method, "path": path}) + "\n")


def send_limit_headers(handler, config):
    for name, value in (config.get("headers") or {}).items():
        handler.send_header(name, str(value))


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def parse_request(self):
        # Runs for every request line, whatever its method, before it is dispatched.
        ok = super().parse_request()
        if ok:
            log_request(self.command, self.path)
        return ok

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
        config = limits()
        if config.get("status") == 429:
            body = json.dumps({"type": "error", "error": {
                "type": "rate_limit_error", "message": "rate limited"}}).encode()
            self.send_response(429)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            send_limit_headers(self, config)
            self.end_headers()
            self.wfile.write(body)
            return
        message = {"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
                   "content": [], "stop_reason": None, "stop_sequence": None,
                   "usage": {"input_tokens": 1, "output_tokens": 1}}
        if not request.get("stream"):
            message.update(content=[{"type": "text", "text": "ok"}], stop_reason="end_turn")
            body = json.dumps(message).encode()
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            send_limit_headers(self, config)
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        send_limit_headers(self, config)
        self.end_headers()
        try:
            for event, data in [
                ("message_start", {"type": "message_start", "message": message}),
                ("content_block_start", {"type": "content_block_start", "index": 0,
                                         "content_block": {"type": "text", "text": ""}}),
                ("content_block_delta", {"type": "content_block_delta", "index": 0,
                                         "delta": {"type": "text_delta", "text": "ok"}}),
                ("content_block_stop", {"type": "content_block_stop", "index": 0}),
                ("message_delta", {"type": "message_delta",
                                   "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                                   "usage": {"output_tokens": 1}}),
                ("message_stop", {"type": "message_stop"}),
            ]:
                self.wfile.write(sse(event, data))
                self.wfile.flush()
        except Exception:
            pass


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
