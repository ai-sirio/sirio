#!/usr/bin/env python3
"""A stand-in for Claude Code's IDE client, used by test-ide-bridge-e2e.sh.

It runs inside a Sirio terminal pane, so it reads what a real Claude reads there:
`CLAUDE_CODE_SSE_PORT` from the environment and `$HOME/.claude/ide/<port>.lock` for the
token. It speaks RFC 6455 with the standard library only: it offers the `mcp`
subprotocol, sends the token in `x-claude-code-ide-authorization`, checks that the server
echoes `mcp`, then sends `initialize`, `notifications/initialized` and
`ide_connected {pid: os.getpid()}`.

Commands arrive as lines on a FIFO:
  call <method> <json>    a JSON-RPC request; waits for its response (10 s)
  notify <method> <json>  a JSON-RPC notification
  token <value>           close, then connect again with this token
  close                   close the connection (the client stays up)
  quit                    close and exit

Every frame in and out, and every connection event, is one JSON line in the transcript:
  {"dir": "out"|"in", "frame": <the JSON-RPC message>}
  {"event": "connected", "subprotocol": "mcp"}
  {"event": "refused", "status": 401}
  {"event": "closed"}
"""

import argparse
import base64
import hashlib
import json
import os
import socket
import struct
import sys
import threading
import time

GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
AUTH_HEADER = "x-claude-code-ide-authorization"


class Transcript:
    def __init__(self, path):
        self._path = path
        self._lock = threading.Lock()

    def write(self, record):
        line = json.dumps(dict(record, t=time.time()), sort_keys=True)
        with self._lock:
            with open(self._path, "a", encoding="utf-8") as handle:
                handle.write(line + "\n")
                handle.flush()


class Refused(Exception):
    def __init__(self, status):
        super().__init__(f"handshake refused with {status}")
        self.status = status


def read_exact(sock, count):
    data = b""
    while len(data) < count:
        chunk = sock.recv(count - len(data))
        if not chunk:
            raise EOFError("connection closed")
        data += chunk
    return data


class Connection:
    """One WebSocket. A reader thread logs every inbound frame and wakes the waiters."""

    def __init__(self, sock, transcript):
        self.sock = sock
        self.transcript = transcript
        self.send_lock = threading.Lock()
        self.responses = {}
        self.cond = threading.Condition()
        self.closed = False
        self.reader = threading.Thread(target=self._read_loop, daemon=True)
        self.reader.start()

    def _send_frame(self, opcode, payload):
        header = bytes([0x80 | opcode])
        length = len(payload)
        if length < 126:
            header += bytes([0x80 | length])
        elif length < 65536:
            header += bytes([0x80 | 126]) + struct.pack(">H", length)
        else:
            header += bytes([0x80 | 127]) + struct.pack(">Q", length)
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        with self.send_lock:
            self.sock.sendall(header + mask + masked)

    def send_json(self, message):
        self.transcript.write({"dir": "out", "frame": message})
        self._send_frame(0x1, json.dumps(message).encode("utf-8"))

    def _read_frame(self):
        first, second = read_exact(self.sock, 2)
        opcode = first & 0x0F
        length = second & 0x7F
        if length == 126:
            length = struct.unpack(">H", read_exact(self.sock, 2))[0]
        elif length == 127:
            length = struct.unpack(">Q", read_exact(self.sock, 8))[0]
        if second & 0x80:
            mask = read_exact(self.sock, 4)
            payload = bytes(b ^ mask[i % 4] for i, b in enumerate(read_exact(self.sock, length)))
        else:
            payload = read_exact(self.sock, length)
        return first & 0x80, opcode, payload

    def _read_loop(self):
        message = b""
        try:
            while True:
                fin, opcode, payload = self._read_frame()
                if opcode == 0x8:
                    break
                if opcode == 0x9:
                    self._send_frame(0xA, payload)
                    continue
                if opcode in (0x1, 0x0):
                    message += payload
                    if fin:
                        self._deliver(message)
                        message = b""
        except (EOFError, OSError):
            pass
        with self.cond:
            self.closed = True
            self.cond.notify_all()
        self.transcript.write({"event": "closed"})

    def _deliver(self, raw):
        try:
            frame = json.loads(raw.decode("utf-8"))
        except ValueError:
            self.transcript.write({"dir": "in", "raw": raw.decode("utf-8", "replace")})
            return
        self.transcript.write({"dir": "in", "frame": frame})
        if isinstance(frame, dict) and "id" in frame and "method" not in frame:
            with self.cond:
                self.responses[frame["id"]] = frame
                self.cond.notify_all()

    def call(self, request_id, method, params, timeout=10.0):
        self.send_json({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        deadline = time.time() + timeout
        with self.cond:
            while request_id not in self.responses:
                left = deadline - time.time()
                if left <= 0 or self.closed:
                    return None
                self.cond.wait(left)
            return self.responses.pop(request_id)

    def close(self):
        try:
            self._send_frame(0x8, struct.pack(">H", 1000))
        except OSError:
            pass
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()


def handshake(port, token):
    sock = socket.create_connection(("127.0.0.1", port), timeout=10)
    key = base64.b64encode(os.urandom(16)).decode()
    request = (
        f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n"
        f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
        f"Sec-WebSocket-Protocol: mcp\r\n{AUTH_HEADER}: {token}\r\n\r\n"
    )
    sock.sendall(request.encode())
    head = b""
    while b"\r\n\r\n" not in head:
        chunk = sock.recv(4096)
        if not chunk:
            sock.close()
            raise Refused("closed before a response")
        head += chunk
    status_line, _, rest = head.partition(b"\r\n")
    status = int(status_line.split()[1])
    if status != 101:
        sock.close()
        raise Refused(status)
    headers = {}
    for line in rest.split(b"\r\n\r\n")[0].split(b"\r\n"):
        name, _, value = line.decode("latin-1").partition(":")
        headers[name.strip().lower()] = value.strip()
    accept = base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()
    if headers.get("sec-websocket-accept") != accept:
        sock.close()
        raise Refused("bad Sec-WebSocket-Accept")
    # Ruling 7: the server must echo the subprotocol Claude's client offered.
    subprotocol = headers.get("sec-websocket-protocol")
    if subprotocol != "mcp":
        # Node's `ws` fails a handshake that drops a subprotocol it offered; so does this client.
        sock.close()
        raise Refused(f"the server did not echo mcp (got {subprotocol!r})")
    sock.settimeout(None)
    return sock, subprotocol


def lockfile_token(port):
    path = os.path.join(os.environ["HOME"], ".claude", "ide", f"{port}.lock")
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)["authToken"]


def connect(port, token, transcript):
    """Connect and announce; returns the Connection, or None when the server refuses."""
    try:
        sock, subprotocol = handshake(port, token)
    except Refused as refusal:
        transcript.write({"event": "refused", "status": refusal.status})
        return None
    transcript.write({"event": "connected", "subprotocol": subprotocol})
    connection = Connection(sock, transcript)
    connection.call(
        "init-1",
        "initialize",
        {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "ide_client.py", "version": "0"}},
    )
    connection.send_json({"jsonrpc": "2.0", "method": "notifications/initialized"})
    connection.send_json({"jsonrpc": "2.0", "method": "ide_connected", "params": {"pid": os.getpid()}})
    return connection


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fifo", required=True)
    parser.add_argument("--transcript", required=True)
    args = parser.parse_args()
    transcript = Transcript(args.transcript)
    port = int(os.environ["CLAUDE_CODE_SSE_PORT"])
    connection = connect(port, lockfile_token(port), transcript)
    next_id = 0
    # Opened read-write so the FIFO never reports end-of-file between writers.
    fifo = os.open(args.fifo, os.O_RDWR)
    with os.fdopen(fifo, "r", encoding="utf-8") as lines:
        transcript.write({"event": "ready"})
        for line in lines:
            line = line.strip()
            if not line:
                continue
            verb, _, rest = line.partition(" ")
            if verb == "token":
                if connection:
                    connection.close()
                connection = connect(port, rest.strip(), transcript)
            elif verb in ("call", "notify"):
                method, _, payload = rest.partition(" ")
                params = json.loads(payload) if payload.strip() else {}
                if connection is None:
                    transcript.write({"event": "no connection", "verb": verb})
                elif verb == "call":
                    next_id += 1
                    connection.call(f"call-{next_id}", method, params)
                else:
                    connection.send_json({"jsonrpc": "2.0", "method": method, "params": params})
            elif verb == "close":
                if connection:
                    connection.close()
                    connection = None
            elif verb == "quit":
                if connection:
                    connection.close()
                return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
