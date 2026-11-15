# Stand-in for the Anthropic Messages API, enough for a Claude Code turn.
# usage: fake_messages.py PORT HOLD_SECONDS
# Holds each /v1/messages request HOLD seconds (so "working" can be observed), then streams
# either "ok" or, when the latest user turn says "run tool" and carries no tool result yet,
# a Bash tool call that sleeps 3 s and touches `probe-file` in Claude's cwd. When it says
# "run edit" (an Edit not yet answered): an Edit of `<cwd>/probe.txt` ("alpha" -> "beta");
# "run edit twice" streams it in two successive turns (beta -> alpha the second time, so both apply).
# Only the latest prompt decides; every response has its own message id, so Claude keeps the turns apart.
# The cwd is the "Primary working directory" Claude reports in its request, else this process's.
# Binds 127.0.0.1 only. Nothing here is a credential.
import itertools, json, os, re, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HOLD = float(sys.argv[2])
TOOL_INPUT = {"command": "sleep 3; touch probe-file", "description": "Create a probe file"}
EDIT_OLD, EDIT_NEW = "alpha", "beta"
COUNTER = itertools.count(1)


def edit_input(request):
    """The Edit of `<cwd>/probe.txt`: alpha -> beta, and beta -> alpha once the file holds beta, so
    successive Edits keep applying."""
    found = re.search(r"Primary working directory: ([^\\\n]+)", json.dumps(request, ensure_ascii=False))
    cwd = found.group(1).strip() if found else os.getcwd()
    path = cwd + "/probe.txt"
    try:
        text = open(path).read()
    except OSError:
        text = EDIT_OLD
    old, new = (EDIT_NEW, EDIT_OLD) if EDIT_OLD not in text and EDIT_NEW in text else (EDIT_OLD, EDIT_NEW)
    return {"file_path": path, "old_string": old, "new_string": new}


def wants_edit(users):
    """Whether this turn streams an Edit. Only the latest prompt (a text block that is not a system
    reminder or an interruption note) decides the scenario. "run edit" streams one Edit while
    nothing follows the prompt; "run edit twice" streams a second one when the latest user turn is
    the tool result of the first."""
    blocks = []
    for user in users:
        content = user.get("content")
        blocks += content if isinstance(content, list) else [content]

    def text_of(block):
        if isinstance(block, dict):
            return "" if block.get("type") == "tool_result" else str(block.get("text", ""))
        return str(block or "")

    def noise(block):
        if isinstance(block, dict) and block.get("type") == "tool_result":
            return False
        text = text_of(block).lstrip()
        return not text or text.startswith("<system-reminder>") or text.startswith("[Request interrupted")

    prompt = None
    for index, block in enumerate(blocks):
        if not noise(block) and not (isinstance(block, dict) and block.get("type") == "tool_result"):
            prompt = index
    if prompt is None or "run edit" not in text_of(blocks[prompt]):
        return False
    after = [block for block in blocks[prompt + 1:] if not noise(block)]
    if not after:
        return True
    if "run edit twice" not in text_of(blocks[prompt]):
        return False
    results = [block for block in after if isinstance(block, dict) and block.get("type") == "tool_result"]
    last = after[-1]
    return (
        len(results) == 1
        and last is results[0]
        and str(last.get("tool_use_id", "")).startswith("toolu_edit_")
    )


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
        message = {"id": f"msg_{next(COUNTER)}", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
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
            block = {"type": "tool_use", "id": f"toolu_edit_{next(COUNTER)}", "name": "Edit", "input": {}}
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
