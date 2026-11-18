# Stand-in for the Anthropic Messages API, for the live session-state conformance test
# (acp_conformance.rs, claude_reports_its_session_state_under_the_variable).
# The first user message picks the scenario:
#   "run bash" -> a Bash call that needs a permission, then "ok"
#   "run bg"   -> a background Agent whose own requests (first message contains
#                 SUBPROMPT) answer after 3 s; the turn the agent's completion starts says "done"
#   anything else -> "ok"
# Two scenarios are picked by the LAST user text instead (A7, errors and notices), since the
# second turn of a session still has the first turn's text first:
#   "overload twice" -> the first two streaming requests answer HTTP 529 (overloaded_error),
#                       the next one "ok"
#   "fail midway"    -> a streaming request gets message_start (msg_m), one text delta and an
#                       SSE error event; a non-streaming request gets a JSON message msg_1 "ok"
# usage: fake_state_api.py PORT
# Binds 127.0.0.1 only. Nothing here is a credential.
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

OVERLOADED = {}


def sse(e, d):
    return f"event: {e}\ndata: {json.dumps(d)}\n\n".encode()


def last_text_of(message):
    """The last text block of a message: the CLI puts its system reminder before the prompt."""
    content = message.get("content")
    if isinstance(content, str):
        return content
    texts = [b.get("text", "") for b in content or [] if isinstance(b, dict) and b.get("type") == "text"]
    return texts[-1] if texts else ""


def text_of(message):
    content = message.get("content")
    return content if isinstance(content, str) else json.dumps(content)


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
        msgs = req.get("messages") or []
        users = [m for m in msgs if m.get("role") == "user"]
        first = text_of(users[0]) if users else ""
        last = text_of(msgs[-1]) if msgs else ""
        last_user = last_text_of(users[-1]) if users else ""
        if req.get("stream") and "overload twice" in last_user:
            OVERLOADED["n"] = OVERLOADED.get("n", 0) + 1
            if OVERLOADED["n"] <= 2:
                body = json.dumps({"type": "error", "error": {
                    "type": "overloaded_error", "message": "Overloaded"}}).encode()
                self.send_response(529)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
        if req.get("stream") and "fail midway" in last_user:
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.end_headers()
            try:
                self.wfile.write(sse("message_start", {"type": "message_start", "message": {
                    "id": "msg_m", "type": "message", "role": "assistant", "model": req.get("model"),
                    "content": [], "stop_reason": None, "stop_sequence": None,
                    "usage": {"input_tokens": 1, "output_tokens": 1}}}))
                self.wfile.write(sse("content_block_start", {"type": "content_block_start", "index": 0,
                                                             "content_block": {"type": "text", "text": ""}}))
                self.wfile.write(sse("content_block_delta", {"type": "content_block_delta", "index": 0,
                                                             "delta": {"type": "text_delta", "text": "partial-before-error"}}))
                self.wfile.flush()
                time.sleep(0.5)
                self.wfile.write(sse("error", {"type": "error", "error": {
                    "type": "api_error", "message": "boom"}}))
                self.wfile.flush()
            except Exception:
                pass
            return
        followup = '"tool_result"' in last
        notified = "task-notification" in last or "task_notification" in last
        msg = {"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
               "content": [], "stop_reason": None, "stop_sequence": None,
               "usage": {"input_tokens": 1, "output_tokens": 1}}
        delay = 0
        block = {"type": "text", "text": ""}
        delta = {"type": "text_delta", "text": "ok"}
        stop = "end_turn"
        if "SUBPROMPT" in first:
            delay = 3
            delta["text"] = "subagent finished"
        elif "run bash" in first and not followup:
            block = {"type": "tool_use", "id": "toolu_b1", "name": "Bash", "input": {}}
            delta = {"type": "input_json_delta", "partial_json": json.dumps(
                {"command": "touch probe-perm.txt", "description": "touch"})}
            stop = "tool_use"
        elif "run bg" in first and not followup and not notified:
            block = {"type": "tool_use", "id": "toolu_a1", "name": "Agent", "input": {}}
            delta = {"type": "input_json_delta", "partial_json": json.dumps(
                {"description": "bg task", "prompt": "SUBPROMPT do the thing",
                 "subagent_type": "general-purpose", "run_in_background": True})}
            stop = "tool_use"
        elif notified:
            delta["text"] = "agent reported done"
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
        try:
            self.wfile.write(sse("message_start", {"type": "message_start", "message": msg}))
            self.wfile.flush()
            time.sleep(delay)
            for e, d in [
                ("content_block_start", {"type": "content_block_start", "index": 0, "content_block": block}),
                ("content_block_delta", {"type": "content_block_delta", "index": 0, "delta": delta}),
                ("content_block_stop", {"type": "content_block_stop", "index": 0}),
                ("message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None},
                                   "usage": {"output_tokens": 1}}),
                ("message_stop", {"type": "message_stop"}),
            ]:
                self.wfile.write(sse(e, d))
                self.wfile.flush()
        except Exception:
            pass


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
