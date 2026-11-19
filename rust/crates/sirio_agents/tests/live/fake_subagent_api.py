"""Fake Messages API for the subagent-text live test (acp_conformance.rs).

Shaped like fake_tasks_api.py. argv: port. Scripted by the first user prompt:
  "run sub false"   -> an `Agent` tool_use (toolu_sub1) with `run_in_background: false`.
  "run sub omitted" -> the same without the `run_in_background` field.
The subagent's request (its first user text is the Agent prompt "sub task") answers the text
"Sub here. Done."; any follow-up answers "ok". Uses no credential and no network beyond
127.0.0.1.
"""
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def sse(event, data):
    return f"event: {event}\ndata: {json.dumps(data)}\n\n".encode()


def text_of(message):
    content = message.get("content")
    return content if isinstance(content, str) else json.dumps(content)


def tool_use(index, tool_id, name, tool_input):
    return [
        ("content_block_start", {"type": "content_block_start", "index": index,
                                 "content_block": {"type": "tool_use", "id": tool_id,
                                                   "name": name, "input": {}}}),
        ("content_block_delta", {"type": "content_block_delta", "index": index,
                                 "delta": {"type": "input_json_delta",
                                           "partial_json": json.dumps(tool_input)}}),
        ("content_block_stop", {"type": "content_block_stop", "index": index}),
    ]


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
        # Version 2.1.257 appends a trailing system-role message; ignore those.
        messages = [m for m in (request.get("messages") or []) if m.get("role") != "system"]
        users = [m for m in messages if m.get("role") == "user"]
        first = text_of(users[0]) if users else ""
        history = json.dumps(messages)
        message = {"id": "msg_1", "type": "message", "role": "assistant",
                   "model": "claude-opus-5-5",
                   "content": [], "stop_reason": None, "stop_sequence": None,
                   "usage": {"input_tokens": 1, "output_tokens": 1}}
        delay, blocks, text, stop = 0, [], "ok", "end_turn"
        if "sub task" in first:
            # The subagent's own first request; its follow-ups (none expected) say "ok".
            if "tool_result" not in history:
                text = "Sub here. Done."
        elif "toolu_sub1" not in history and (
                "run sub false" in first or "run sub omitted" in first):
            tool_input = {"description": "sub", "prompt": "sub task",
                          "subagent_type": "general-purpose"}
            if "run sub false" in first:
                tool_input["run_in_background"] = False
            blocks = tool_use(0, "toolu_sub1", "Agent", tool_input)
            stop = "tool_use"

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
        try:
            self.wfile.write(sse("message_start", {"type": "message_start", "message": message}))
            self.wfile.flush()
            time.sleep(delay)
            if not blocks:
                blocks = [
                    ("content_block_start", {"type": "content_block_start", "index": 0,
                                             "content_block": {"type": "text", "text": ""}}),
                    ("content_block_delta", {"type": "content_block_delta", "index": 0,
                                             "delta": {"type": "text_delta", "text": text}}),
                    ("content_block_stop", {"type": "content_block_stop", "index": 0}),
                ]
            tail = [("message_delta", {"type": "message_delta",
                                       "delta": {"stop_reason": stop, "stop_sequence": None},
                                       "usage": {"output_tokens": 1}}),
                    ("message_stop", {"type": "message_stop"})]
            for event, data in blocks + tail:
                self.wfile.write(sse(event, data))
                self.wfile.flush()
        except Exception:
            pass


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
