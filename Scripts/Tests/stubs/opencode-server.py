#!/usr/bin/env python3
"""A stand-in for OpenCode's v2 background service (python3 standard library only).

Used by Scripts/Tests/test-hand-agents-e2e.sh. It serves the part of the service's HTTP
API that Sirio reads, with the shapes Task 2 measured against the real service:

  GET /api/info                      {"version","pid","urls","paths","capabilities"}
  GET /api/session?directory=<dir>   {"data":[Session.Info],"cursor":{...}}, an exact match
  GET /api/session/active            {"data":{"<sid>":{"type":"running"}}}
  GET /api/session/<sid>/permission  {"data":[...]}   the asks the session waits on
  GET /api/session/<sid>/form        {"data":[...]}
  GET /api/event                     SSE: `data: {json}` frames and `: heartbeat` comments;
                                     the first frame is server.connected; nothing is replayed

Every /api request needs `Authorization: Basic base64("opencode:" + password)`, else 401.
It writes the registration file the real service writes, `{id, version, url, pid,
password}` (mode 600), at --registry once it listens.

It is driven by lines appended to the --cmd file (one command per line):

  create <sid> <dir>      a session in <dir>: session.created
  busy <sid>              session.execution.started
  idle <sid>              session.execution.succeeded
  interrupt <sid>         session.execution.interrupted
  fail <sid>              session.execution.failed (its data.error text is STUB-ERROR-TEXT)
  wait <sid>              permission.asked (the session stays running)
  reply <sid>             permission.replied
  ask <sid>               form.created
  cancel <sid>            form.cancelled
  rename <sid> <title>    session.renamed
  stop                    close every connection and exit; the registration file stays

and `--ack FILE` receives the number of commands handled so far, after each one. A
request log (method, path, status; never a credential) goes to --log.
"""
import argparse
import base64
import json
import os
import queue
import secrets
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

LOCK = threading.Lock()
SESSIONS = {}  # sid -> dict
ASKS = {}  # sid -> {"permission": [...], "form": [...]}
SUBSCRIBERS = []  # queue.Queue per /api/event connection
COUNTER = [0]
# 43 characters, like the real service's: base64 of `opencode:<password>` then ends in `=`.
PASSWORD = secrets.token_urlsafe(32)
ARGS = None
SERVER = None
STOPPING = threading.Event()


def now_ms():
    return int(time.time() * 1000)


def log_line(text):
    if ARGS.log:
        with open(ARGS.log, "a") as handle:
            handle.write(text + "\n")


def emit(kind, data, directory=None):
    """One event to every connected stream (nothing is kept for a later one)."""
    COUNTER[0] += 1
    event = {"id": "evt_%d" % COUNTER[0], "created": now_ms(), "type": kind, "data": data}
    if directory is not None:
        event["location"] = {"directory": directory}
    frame = ("data: " + json.dumps(event) + "\n\n").encode()
    for subscriber in list(SUBSCRIBERS):
        subscriber.put(frame)


def session_info(sid):
    s = SESSIONS[sid]
    info = {
        "id": sid,
        "projectID": "prj_stub",
        "agent": "build",
        "model": {"id": "stub", "providerID": "stub"},
        "cost": 0,
        "tokens": {},
        "time": {"created": s["created"], "updated": s["updated"]},
        "location": {"directory": s["directory"]},
    }
    if s["title"]:
        info["title"] = s["title"]
    return info


def command(line):
    words = line.split()
    if not words:
        return
    name, rest = words[0], words[1:]
    with LOCK:
        if name == "stop":
            STOPPING.set()
            return
        sid = rest[0] if rest else ""
        if name == "create" and len(rest) >= 2:
            directory = " ".join(rest[1:])
            stamp = now_ms()
            SESSIONS[sid] = {
                "directory": directory,
                "title": "",
                "created": stamp,
                "updated": stamp,
                "busy": False,
            }
            ASKS[sid] = {"permission": [], "form": []}
            emit(
                "session.created",
                {
                    "sessionID": sid,
                    "slug": "stub",
                    "version": "2.0.29",
                    "projectID": "prj_stub",
                    "location": {"directory": directory},
                    "subpath": "",
                    "agent": "build",
                    "model": {"id": "stub", "providerID": "stub"},
                },
                directory,
            )
            return
        if sid not in SESSIONS:
            return
        s = SESSIONS[sid]
        s["updated"] = now_ms()
        # session.execution.* carry no `location`, as the real service's do not.
        if name == "busy":
            s["busy"] = True
            emit("session.execution.started", {"sessionID": sid})
        elif name == "idle":
            s["busy"] = False
            emit("session.execution.succeeded", {"sessionID": sid})
        elif name == "interrupt":
            s["busy"] = False
            emit("session.execution.interrupted", {"sessionID": sid, "reason": "shutdown"})
        elif name == "fail":
            s["busy"] = False
            emit(
                "session.execution.failed",
                {
                    "sessionID": sid,
                    "error": {
                        "type": "provider.invalid-request",
                        "message": "STUB-ERROR-TEXT",
                        "status": 400,
                    },
                },
            )
        elif name == "wait":
            ASKS[sid]["permission"].append({"id": "per_1", "sessionID": sid, "action": "shell"})
            emit(
                "permission.asked",
                {"id": "per_1", "sessionID": sid, "action": "shell", "resources": ["touch x"]},
                s["directory"],
            )
        elif name == "reply":
            ASKS[sid]["permission"] = []
            emit(
                "permission.replied",
                {"id": "per_1", "sessionID": sid, "reply": "once"},
                s["directory"],
            )
        elif name == "ask":
            ASKS[sid]["form"].append({"id": "frm_1", "sessionID": sid, "title": "Questions"})
            emit(
                "form.created",
                {"form": {"id": "frm_1", "sessionID": sid, "title": "Questions"}},
                s["directory"],
            )
        elif name == "cancel":
            ASKS[sid]["form"] = []
            emit("form.cancelled", {"id": "frm_1", "sessionID": sid}, s["directory"])
        elif name == "rename" and len(rest) >= 2:
            s["title"] = " ".join(rest[1:])
            emit("session.renamed", {"sessionID": sid, "title": s["title"]}, s["directory"])


def pump_commands():
    handled = 0
    offset = 0
    while not STOPPING.is_set():
        try:
            with open(ARGS.cmd, "rb") as handle:
                handle.seek(offset)
                chunk = handle.read()
        except FileNotFoundError:
            chunk = b""
        end = chunk.rfind(b"\n")
        if end != -1:
            offset += end + 1
            for line in chunk[: end + 1].decode().splitlines():
                command(line)
                handled += 1
                with open(ARGS.ack + ".tmp", "w") as out:
                    out.write(str(handled))
                os.replace(ARGS.ack + ".tmp", ARGS.ack)
                if STOPPING.is_set():
                    break
        time.sleep(0.05)
    # Closing the listener refuses new connections; ending each stream closes the old ones.
    for subscriber in list(SUBSCRIBERS):
        subscriber.put(None)
    time.sleep(0.3)
    SERVER.shutdown()
    SERVER.server_close()
    os._exit(0)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def authorized(self):
        want = "Basic " + base64.b64encode(("opencode:" + PASSWORD).encode()).decode()
        return self.headers.get("Authorization") == want

    def reply(self, status, body=None):
        raw = json.dumps(body).encode() if body is not None else b""
        self.send_response(status)
        if status == 401:
            self.send_header("WWW-Authenticate", 'Basic realm="opencode"')
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)
        log_line("%s %s %d" % (self.command, urlparse(self.path).path, status))

    def do_GET(self):
        url = urlparse(self.path)
        if not url.path.startswith("/api/"):
            return self.reply(404)
        if not self.authorized():
            return self.reply(401)
        query = parse_qs(url.query)
        path = url.path
        if path == "/api/info":
            return self.reply(
                200,
                {
                    "version": "2.0.29",
                    "pid": os.getpid(),
                    "urls": ["http://127.0.0.1:%d" % SERVER.server_port],
                    "paths": {"tmp": "/tmp/opencode"},
                    "capabilities": {"persistentPty": True},
                },
            )
        if path == "/api/event":
            return self.stream()
        with LOCK:
            if path == "/api/session":
                wanted = query.get("directory", [None])[0]
                rows = [
                    session_info(sid)
                    for sid, s in SESSIONS.items()
                    if wanted is None or s["directory"] == wanted
                ]
                return self.reply(200, {"data": rows, "cursor": {"previous": None, "next": None}})
            if path == "/api/session/active":
                active = {sid: {"type": "running"} for sid, s in SESSIONS.items() if s["busy"]}
                return self.reply(200, {"data": active})
            parts = path.split("/")
            # /api/session/<sid>/permission and /api/session/<sid>/form
            if len(parts) == 5 and parts[2] == "session" and parts[4] in ("permission", "form"):
                asks = ASKS.get(parts[3], {}).get(parts[4], [])
                return self.reply(200, {"data": asks})
        return self.reply(404)

    def stream(self):
        mine = queue.Queue()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        log_line("GET /api/event 200")

        def chunk(raw):
            self.wfile.write(b"%x\r\n" % len(raw) + raw + b"\r\n")
            self.wfile.flush()

        try:
            chunk(
                ("data: " + json.dumps({"id": "evt_0", "type": "server.connected", "data": {}}) + "\n\n").encode()
            )
            with LOCK:
                SUBSCRIBERS.append(mine)
            while True:
                try:
                    frame = mine.get(timeout=ARGS.heartbeat)
                except queue.Empty:
                    frame = b": heartbeat\n\n"
                if frame is None:
                    break
                chunk(frame)
            self.wfile.write(b"0\r\n\r\n")
        except (BrokenPipeError, ConnectionResetError, OSError):
            pass
        finally:
            with LOCK:
                if mine in SUBSCRIBERS:
                    SUBSCRIBERS.remove(mine)
            self.close_connection = True


def main():
    global ARGS, SERVER
    parser = argparse.ArgumentParser()
    parser.add_argument("--registry", required=True)
    parser.add_argument("--cmd", required=True)
    parser.add_argument("--ack", required=True)
    parser.add_argument("--log")
    parser.add_argument("--heartbeat", type=float, default=5.0)
    ARGS = parser.parse_args()
    SERVER = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    SERVER.daemon_threads = True
    registration = {
        "id": "00000000-0000-4000-8000-000000000011",
        "version": "2.0.29",
        "url": "http://127.0.0.1:%d" % SERVER.server_port,
        "pid": os.getpid(),
        "password": PASSWORD,
    }
    os.makedirs(os.path.dirname(ARGS.registry), exist_ok=True)
    descriptor = os.open(ARGS.registry, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w") as handle:
        json.dump(registration, handle)
    threading.Thread(target=pump_commands, daemon=True).start()
    SERVER.serve_forever()


if __name__ == "__main__":
    main()
