# Stand-in for the OpenAI Responses API, enough for a Codex turn.
# usage: fake_responses.py PORT HOLD_SECONDS
# Holds each request HOLD seconds (so "working" can be observed), then streams "ok" or, by
# what the latest input says, an `exec_command` tool call:
#   "run it"   -> `touch never-run-by-probe` asking for escalation (Codex asks for approval)
#   "sleep it" -> `sleep 3; touch probe-file` inside the sandbox (no approval)
# Binds 127.0.0.1 only. Nothing here is a credential.
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, HOLD = int(sys.argv[1]), float(sys.argv[2])
calls = {"n": 0}


def sse(w, obj):
    w.write(f"event: {obj['type']}\ndata: {json.dumps(obj)}\n\n".encode())
    w.flush()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        print("GET", self.path, flush=True)
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length") or 0))
        try:
            request = json.loads(body)
        except Exception:
            request = {}
        text = json.dumps(request.get("input", []))[-400:]
        calls["n"] += 1
        print("POST", self.path, "call", calls["n"], flush=True)
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        sse(self.wfile, {"type": "response.created", "response": {"id": f"resp_{calls['n']}"}})
        time.sleep(HOLD)
        after_tool = '"function_call_output"' in text or '"custom_tool_call_output"' in text
        args = None
        if not after_tool and "run it" in text:
            args = {"cmd": "touch never-run-by-probe", "sandbox_permissions": "require_escalated",
                    "justification": "probe needs to write"}
        elif not after_tool and "sleep it" in text:
            args = {"cmd": "sleep 3; touch probe-file"}
        if args is not None:
            item = {"type": "function_call", "id": "fc_1", "call_id": f"call_{calls['n']}",
                    "name": "exec_command", "arguments": json.dumps(args)}
            print("  -> exec_command", flush=True)
        else:
            item = {"type": "message", "id": f"msg_{calls['n']}", "role": "assistant",
                    "content": [{"type": "output_text", "text": "ok", "annotations": []}]}
            sse(self.wfile, {"type": "response.output_text.delta", "delta": "ok", "item_id": item["id"],
                             "output_index": 0, "content_index": 0})
        sse(self.wfile, {"type": "response.output_item.done", "output_index": 0, "item": item})
        sse(self.wfile, {"type": "response.completed", "response": {"id": f"resp_{calls['n']}", "usage": {
            "input_tokens": 1, "input_tokens_details": {"cached_tokens": 0}, "output_tokens": 1,
            "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 2}}})


ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
