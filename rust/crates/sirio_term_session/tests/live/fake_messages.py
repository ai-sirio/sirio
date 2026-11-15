# Stand-in for the Anthropic Messages API, enough for a Claude Code turn.
# usage: fake_messages.py PORT HOLD_SECONDS
# Holds each /v1/messages request HOLD seconds (so "working" can be observed), then streams
# either "ok" or, when the latest user turn says "run tool" and carries no tool result yet,
# a Bash tool call that sleeps 3 s and touches `probe-file` in Claude's cwd. When it says
# "run edit" (an Edit not yet answered): an Edit of `<cwd>/probe.txt` ("alpha" -> "beta");
# "run edit twice" streams that Edit in two successive turns (each Edit asked for and not yet answered).
# The cwd is the "Primary working directory" Claude reports in its request, else this process's.
# Binds 127.0.0.1 only. Nothing here is a credential.
import json, os, re, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HOLD = float(sys.argv[2])
TOOL_INPUT = {"command": "sleep 3; touch probe-file", "description": "Create a probe file"}
EDIT_OLD, EDIT_NEW = "alpha", "beta"


def edit_input(request):
    found = re.search(r"Primary working directory: ([^\\\n]+)", json.dumps(request))
    cwd = found.group(1).strip() if found else os.getcwd()
    return {"file_path": cwd + "/probe.txt", "old_string": EDIT_OLD, "new_string": EDIT_NEW}


def wants_edit(users):
    """Whether this turn streams an Edit. The content blocks of the user turns are read in order,
    because Claude merges a new prompt into the user message that holds the previous tool result:
    the last block saying "run edit" decides, and the Edit results after it are the Edits already
    answered (one for "run edit", two for "run edit twice")."""
    blocks = []
    for user in users:
        content = user.get("content")
        blocks += content if isinstance(content, list) else [content]
    said = None
    for index, block in enumerate(blocks):
        text = block.get("text", "") if isinstance(block, dict) else str(block or "")
        if "run edit" in text and not (isinstance(block, dict) and block.get("type") == "tool_result"):
            said = index
    if said is None:
        return False
    text = blocks[said].get("text", "") if isinstance(blocks[said], dict) else str(blocks[said])
    answered = sum(
        1 for block in blocks[said:]
        if isinstance(block, dict) and block.get("type") == "tool_result"
        and str(block.get("tool_use_id", "")).startswith("toolu_edit_")
    )
    return answered < (2 if "run edit twice" in text else 1)


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
        want_edit = bool(request.get("tools")) and wants_edit(users)
        print("POST", self.path, "messages", len(messages), "edit" if want_edit else "tool" if want_tool else "text", flush=True)
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
        if want_edit:
            block = {"type": "tool_use", "id": f"toolu_edit_{len(messages)}", "name": "Edit", "input": {}}
            delta = {"type": "input_json_delta", "partial_json": json.dumps(edit_input(request))}
            stop = "tool_use"
        elif want_tool:
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
