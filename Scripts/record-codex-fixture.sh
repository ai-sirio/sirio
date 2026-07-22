#!/bin/bash
# Records one Codex app-server conversation turn into a fixture file.
# Requires: codex 0.145.0 installed and authenticated. Run from repo root.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/Packages/TillerACP/Tests/TillerACPTests/Fixtures/codex-init-turn.ndjson"
mkdir -p "$(dirname "$out")"

# The Codex turn runs in a disposable directory so the agent cannot modify the
# checkout. The Python here-document drives Codex over its stdin and writes
# every server-to-client JSON-RPC line to the fixture as it arrives.
scratch="$(mktemp -d)"
python3 - "$out" "$scratch" <<'PY'
import json
import subprocess
import sys
import time

out_path, cwd = sys.argv[1:]
process = subprocess.Popen(
    ["/bin/zsh", "-lc", "exec codex app-server"],
    cwd=cwd,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=1,
)

def send(message):
    process.stdin.write(json.dumps(message, separators=(",", ":")) + "\n")
    process.stdin.flush()

def read_until(predicate, timeout=240):
    deadline = time.monotonic() + timeout
    with open(out_path, "a", encoding="utf-8") as fixture:
        while time.monotonic() < deadline:
            line = process.stdout.readline()
            if not line:
                raise RuntimeError("codex app-server closed stdout before the turn completed")
            line = line.rstrip("\n")
            fixture.write(line + "\n")
            fixture.flush()
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if predicate(message):
                return message
    raise TimeoutError("timed out waiting for codex app-server")

open(out_path, "w", encoding="utf-8").close()
send({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "clientInfo": {"name": "tiller-fixture", "title": "Tiller fixture", "version": "0.1.0"},
        "capabilities": {"experimentalApi": True},
    },
})
read_until(lambda message: message.get("id") == 1)
send({"jsonrpc": "2.0", "method": "initialized", "params": {}})

send({
    "jsonrpc": "2.0",
    "id": 2,
    "method": "thread/start",
    "params": {"cwd": cwd, "approvalPolicy": "never", "sandbox": "workspace-write"},
})
thread_response = read_until(lambda message: message.get("id") == 2)
thread_id = thread_response["result"]["thread"]["id"]

send({
    "jsonrpc": "2.0",
    "id": 3,
    "method": "turn/start",
    "params": {
        "threadId": thread_id,
        "input": [{"type": "text", "text": "Run: echo hello then reply done"}],
        "approvalPolicy": "never",
        "sandboxPolicy": {"type": "workspaceWrite"},
    },
})
read_until(lambda message: message.get("id") == 3)
read_until(lambda message: message.get("method") in {"turn/completed", "turn/aborted"})

process.stdin.close()
process.terminate()
process.wait(timeout=10)
PY

echo "captured $(wc -l < "$out" | tr -d ' ') lines -> $out"
