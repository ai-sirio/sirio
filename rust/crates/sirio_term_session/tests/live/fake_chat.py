# Stand-in for an OpenAI chat-completions API (omp, Pi, OpenCode).
# usage: fake_chat.py PORT HOLD_SECONDS
# Holds each request HOLD seconds, then streams "ok" or, when the latest message is a user turn
# saying "run it" and the request offers tools, a call to the tool named `shell` (OpenCode's;
# its permission ask is what "waiting" is). Binds 127.0.0.1 only. Nothing here is a credential.
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, HOLD = int(sys.argv[1]), float(sys.argv[2])


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        print("GET", self.path, flush=True)
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers.get("content-length") or 0)) or b"{}")
        messages = request.get("messages", [])
        tools = [t.get("function", {}).get("name") for t in request.get("tools", []) or []]
        last = messages[-1] if messages else {}
        print("POST", self.path, "messages", len(messages), "last role", last.get("role"), flush=True)
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()

        def chunk(delta, finish=None):
            obj = {"id": "c1", "object": "chat.completion.chunk", "created": 1, "model": "m",
                   "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
            self.wfile.write(f"data: {json.dumps(obj)}\n\n".encode())
            self.wfile.flush()

        chunk({"role": "assistant", "content": ""})
        time.sleep(HOLD)
        if "run it" in json.dumps(last.get("content", "")) and last.get("role") == "user" and tools:
            print("  -> shell tool call", flush=True)
            chunk({"tool_calls": [{"index": 0, "id": "call_1", "type": "function",
                   "function": {"name": "shell", "arguments": json.dumps(
                       {"command": "touch never-run-by-probe", "description": "probe"})}}]})
            chunk({}, "tool_calls")
        else:
            chunk({"content": "ok"})
            chunk({}, "stop")
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
