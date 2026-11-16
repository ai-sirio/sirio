#!/bin/bash
set -eEuo pipefail

# End-to-end test of Sirio as Claude Code's IDE (/ide): a real, isolated debug Sirio on a
# private Xvfb, a stand-in client (Scripts/Tests/ide_client.py) running in one of its terminal
# panes the way `claude` would, and the files on disk. Everything is private: HOME (so the
# lockfile goes to $HOME/.claude/ide/ inside the run), the database, the control socket, the host
# root and TMPDIR (on disk under ~/.cache/st, never the small /tmp tmpfs). Nothing of the real
# ~/.claude, the installed Sirio or a running Sirio is touched: every sirioctl and socket call
# below names this run's own socket. Linux and X11 only.
#
# Cases, in the order they run:
#   lockfile_and_env  the lockfile has exactly six keys, mode 0600 in a 0700 directory, ideName
#                     "Sirio (debug)", the app's pid and the repository as a workspace folder; a
#                     new pane's environment holds the port the state reports
#   token_refused     a client that offers a wrong token is refused (401) and is no connection
#   handshake         a client with the right token is accepted, the server echoes the mcp
#                     subprotocol, tools/list names exactly the four tools, the state lists the
#                     connection with the client's pid
#   switch_off        `surface.ide.switch on=false`: the lockfile is gone, the client's connection
#                     is closed, the state says enabled:false with no port; on=true: the same
#                     port and token are back
#   switch_flap       the switch sent off, on, off, on from one process, back to back, while a
#                     start is held in flight (the debug knob SIRIO_DEBUG_IDE_START_DELAY_MS, set
#                     for the whole run): the port is the original, the state agrees, and exactly
#                     one lockfile exists
#   port_survives_catalog_change
#                     a worktree added with workspace.create joins the lockfile's workspace
#                     folders, and a pane in it still gets the port
#   restart_keeps_port
#                     quit (the lockfile goes) and relaunch: the same port and token, the
#                     lockfile is back, and the old pane's client reconnects with that token
#   inherited_port    Sirio launched with CLAUDE_CODE_SSE_PORT=1 hands its own port to panes;
#                     with the switch off a new pane has none
#
# The artifact: --out-dir DIR (default artifacts/ide-bridge-e2e-<stamp>-<pid>) keeps
# transcript.log, app.log, client-*.jsonl (every frame a client sent and received), the lockfile
# with its authToken redacted, the PID-matched window captures in frames/ and socket.jsonl: one
# JSON line per request this script sent to the app's control socket, with the app's complete
# answer. The state, settings and switch calls go straight to the socket; every sirioctl call
# (the app-ready poll's attempts included) goes through a recording proxy that forwards it
# unchanged. A request that got no answer (a connect error, an EOF, a timeout, an answer that is
# not JSON) is a line with the request and an `error` and no `response`. NOT recorded: the
# `system.ping` route probe sirioctl sends by itself, and only before `panel.*` calls (the proxy
# forwards it and leaves it out of the log). Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-ide-bridge-e2e.sh [--xvfb | --display :N] [--state-only] [--out-dir DIR]
#
#   --xvfb        start a private Xvfb on a free display between :90 and :99
#   --display :N  use an Xvfb this user started (never your own $DISPLAY)
#   --state-only  take no window captures; the window still draws

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
SEED="$TARGET_DIR/debug/examples/appearance_seed"
CLIENT="$ROOT/Scripts/Tests/ide_client.py"

STATE_ONLY=0
START_XVFB=0
DISPLAY_TARGET=""
OUT_DIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/ide-bridge-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

for tool in python3 git xdotool xwininfo xprop xdpyinfo; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
if [ "$STATE_ONLY" -eq 0 ]; then
  for tool in import identify; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi
[ -n "$DISPLAY_TARGET" ] || [ "$START_XVFB" -eq 1 ] || fail "no display; pass --xvfb or --display :N"
[ -z "$DISPLAY_TARGET" ] || [ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY; pass --xvfb, or an Xvfb you started on another number"

# ---------------------------------------------------------------- build

echo "building sirio, sirioctl, sirio-host and the appearance seed"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host -p sirio_persistence --example appearance_seed)
for binary in "$BIN" "$CTL" "$SEED"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run

BASE="${SIRIO_E2E_TMP:-$HOME/.cache/st/ide-e2e-tmp}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/ide-XXXXXX")"
WT="$RUN_DIR/repo"
APP_PID=""
XVFB_PID=""
CASE="setup"
CASES_PASSED=0
CLIENT_N=0

is_a_host() {
  local comm
  comm="$(cat "/proc/$1/comm" 2>/dev/null || true)"
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
terminate() { # pid: TERM, then KILL after 5 s
  kill -TERM "$1" 2>/dev/null || return 0
  (sleep 5; kill -KILL "$1" 2>/dev/null) >/dev/null 2>&1 &
  local watchdog=$!
  wait "$1" 2>/dev/null || true
  kill "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
}
stop_hosts() {
  local file pid tries
  for file in "$RUN_DIR"/host/host-v*.json; do
    [ -f "$file" ] || continue
    pid="$(sed -n 's/.*"pid": *\([0-9][0-9]*\).*/\1/p' "$file" | head -n 1)"
    [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && is_a_host "$pid" || continue
    kill -TERM "$pid" 2>/dev/null || true
    tries=0
    while kill -0 "$pid" 2>/dev/null && [ "$tries" -lt 50 ]; do
      tries=$((tries + 1))
      sleep 0.1
    done
    kill -KILL "$pid" 2>/dev/null || true
  done
}
cleanup() {
  local rc=$?
  [ -z "$APP_PID" ] || terminate "$APP_PID"
  stop_hosts
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  [ -z "${PROXY_PID:-}" ] || kill "$PROXY_PID" 2>/dev/null || true
  cp "$RUN_DIR"/app.log "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN_DIR"/socket.jsonl "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN_DIR"/client-*.jsonl "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
  exit "$rc"
}
trap cleanup EXIT

VULKAN_ENV=()
if [ "$START_XVFB" -eq 1 ]; then
  command -v Xvfb >/dev/null || fail "Xvfb is required for --xvfb"
  for number in $(seq 90 99); do
    [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
  done
  [ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
  Xvfb "$DISPLAY_TARGET" -screen 0 1920x1080x24 >"$OUT_DIR/xvfb.log" 2>&1 &
  XVFB_PID=$!
  for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
  DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
fi
echo "display: $DISPLAY_TARGET (state-only $STATE_ONLY)"
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi

# A private world: HOME and every XDG directory under the run dir, stub agent CLIs first on
# PATH (so no mise wrapper runs), a host root and a TMPDIR of its own on disk.
export XDG_STATE_HOME="$RUN_DIR/xdg/state" XDG_DATA_HOME="$RUN_DIR/xdg/data"
export XDG_CONFIG_HOME="$RUN_DIR/xdg/config" XDG_CACHE_HOME="$RUN_DIR/xdg/cache"
export XDG_RUNTIME_DIR="$RUN_DIR/xdg/runtime"
mkdir -p "$RUN_DIR/home" "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" \
  "$XDG_RUNTIME_DIR" "$RUN_DIR/host" "$RUN_DIR/tmp"
chmod 700 "$XDG_RUNTIME_DIR"
mkdir -p "$RUN_DIR/stubs"
for cli in claude codex opencode pi omp; do
  printf '#!/bin/sh\necho "2.1.257 (Claude Code)"\n' >"$RUN_DIR/stubs/$cli"
  chmod +x "$RUN_DIR/stubs/$cli"
done
export PATH="$RUN_DIR/stubs:$PATH"
export HOME="$RUN_DIR/home"
export TMPDIR="$RUN_DIR/tmp"
# A pane's shell: no rc file of the user's matters, whatever their login shell is.
export SHELL=/bin/sh
export SIRIO_SOCKET="$RUN_DIR/c.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
export SIRIO_CREDENTIALS="$RUN_DIR/credentials.json"
export SIRIO_HOST_HOME="$RUN_DIR/host"
unset CLAUDE_CODE_SSE_PORT
[ "${#SIRIO_SOCKET}" -lt 100 ] || fail "the socket path is too long for a unix socket: $SIRIO_SOCKET"
LOCK_DIR="$HOME/.claude/ide"

"$SEED" --database "$SIRIO_DB" --appearance dark

mkdir -p "$WT"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
printf 'fn main() {}\n' >"$WT/main.rs"
git -C "$WT" add main.rs
git -C "$WT" commit -q -m initial

# ---------------------------------------------------------------- helpers

# One control-socket call, logged to socket.jsonl, answering the result map as JSON (the
# state verb's own JSON is unwrapped from its `state` key).
cat >"$RUN_DIR/sockcall.py" <<'PY'
import json, os, socket, sys, time

method, params = sys.argv[1], dict(arg.split("=", 1) for arg in sys.argv[2:])
request = {"id": f"e2e-{os.getpid()}-{time.time_ns()}", "method": method, "params": params}
log_path = os.path.join(os.environ["RUN_DIR"], "socket.jsonl")


def write_line(entry):
    with open(log_path, "a", encoding="utf-8") as log:
        log.write(json.dumps(entry, sort_keys=True) + "\n")


# A connect error, an EOF before an answer, a timeout or an answer that is not JSON leaves a
# line with the request and an `error` and no `response`, then fails as it always did.
try:
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(20)
        sock.connect(os.environ["SIRIO_SOCKET"])
        sock.sendall((json.dumps(request) + "\n").encode())
        data = b""
        while not data.endswith(b"\n"):
            chunk = sock.recv(65536)
            if not chunk:
                break
            data += chunk
    if not data:
        raise EOFError("no answer")
    response = json.loads(data)
except (OSError, EOFError, ValueError) as error:
    write_line({"request": request, "error": type(error).__name__})
    raise
write_line({"request": request, "response": response})
if not response.get("ok"):
    sys.stderr.write(f"{method} failed: {response.get('error')}\n")
    sys.exit(1)
result = response.get("result") or {}
if "state" in result:
    print(result["state"])
else:
    print(json.dumps(result))
PY
export RUN_DIR
sockcall() { python3 -I "$RUN_DIR/sockcall.py" "$@"; }
# Several `surface.ide.switch` calls from one process, back to back, each on its own connection
# and logged like sockcall's: SWITCHES are on=true or on=false.
cat >"$RUN_DIR/sockburst.py" <<'BURST'
import json, os, socket, sys, time

with open(os.path.join(os.environ["RUN_DIR"], "socket.jsonl"), "a", encoding="utf-8") as log:
    for value in sys.argv[1:]:
        request = {"id": f"burst-{time.time_ns()}", "method": "surface.ide.switch",
                   "params": {"on": value}}
        try:
            with socket.socket(socket.AF_UNIX) as sock:
                sock.settimeout(20)
                sock.connect(os.environ["SIRIO_SOCKET"])
                sock.sendall((json.dumps(request) + "\n").encode())
                data = b""
                while not data.endswith(b"\n"):
                    chunk = sock.recv(65536)
                    if not chunk:
                        break
                    data += chunk
            if not data:
                raise EOFError("no answer")
            response = json.loads(data)
        except (OSError, EOFError, ValueError) as error:
            log.write(json.dumps({"via": "burst", "request": request, "error": type(error).__name__}, sort_keys=True) + "\n")
            raise
        log.write(json.dumps({"via": "burst", "request": request, "response": response}, sort_keys=True) + "\n")
        if not response.get("ok"):
            sys.exit(f"switch {value} failed: {response.get('error')}")
BURST
sockburst() { python3 -I "$RUN_DIR/sockburst.py" "$@"; }
# Every sirioctl call goes through a recording proxy: it listens on its own socket, forwards
# each request line to the app's socket and the answer back, and appends both, complete, to
# socket.jsonl. The `system.ping` route probe sirioctl sends before a `panel.*` call is forwarded
# and not recorded.
cat >"$RUN_DIR/proxy.py" <<'PROXY'
import json, os, socket, socketserver, sys, threading

listen, upstream, log_path = sys.argv[1:4]
log_lock = threading.Lock()


def record(entry):
    with log_lock:
        with open(log_path, "a", encoding="utf-8") as log:
            log.write(json.dumps(entry, sort_keys=True) + "\n")


class Handler(socketserver.StreamRequestHandler):
    def handle(self):
        up = None
        try:
            for line in self.rfile:
                try:
                    request = json.loads(line)
                except ValueError:
                    request = line.decode("utf-8", "replace")
                entry = {"via": "sirioctl", "request": request}
                try:
                    if up is None:
                        up = socket.socket(socket.AF_UNIX)
                        up.connect(upstream)
                        up_file = up.makefile("rwb")
                    up_file.write(line)
                    up_file.flush()
                    answer = up_file.readline()
                    if not answer:
                        raise OSError("the app closed the connection")
                    entry["response"] = json.loads(answer)
                    self.wfile.write(answer)
                    self.wfile.flush()
                except (OSError, ValueError) as error:
                    entry["error"] = f"{type(error).__name__}"
                    if not (isinstance(request, dict) and request.get("method") == "system.ping"):
                        record(entry)
                    return
                if not (isinstance(request, dict) and request.get("method") == "system.ping"):
                    record(entry)
        finally:
            if up is not None:
                up.close()


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


with Server(listen, Handler) as server:
    server.serve_forever()
PROXY
PROXY_SOCKET="$RUN_DIR/p.sock"
python3 -I "$RUN_DIR/proxy.py" "$PROXY_SOCKET" "$SIRIO_SOCKET" "$RUN_DIR/socket.jsonl" &
PROXY_PID=$!
for _ in $(seq 1 50); do [ -S "$PROXY_SOCKET" ] && break; sleep 0.1; done
[ -S "$PROXY_SOCKET" ] || fail "the recording proxy did not start"
printf '#!/bin/sh\nSIRIO_SOCKET="%s" exec "%s" "$@"\n' "$PROXY_SOCKET" "$CTL" >"$RUN_DIR/ctl-proxied"
chmod +x "$RUN_DIR/ctl-proxied"
CTL="$RUN_DIR/ctl-proxied"
ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
say() { echo "--- $*"; }

# A value out of JSON on stdin: a Python expression over `d`.
jq_py() { python3 -I -c 'import json, sys; d = json.load(sys.stdin); print(eval(sys.argv[1]))' "$1"; }
ide_state() { sockcall surface.ide.state; }
state_expr() { ide_state | jq_py "$1"; }

# Polls `surface.settings.read` until it reports claudeIdeBridge=WANT (the Settings switch).
expect_settings_switch() { # want
  local tries=0 got=""
  "$CTL" surface settings open >/dev/null 2>&1 || true
  while [ "$tries" -lt 50 ]; do
    got="$(sockcall surface.settings.read 2>/dev/null | jq_py 'd.get("claudeIdeBridge")' 2>/dev/null || true)"
    [ "$got" = "$1" ] && { echo "OK: surface.settings.read: claudeIdeBridge=$1"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: surface.settings.read never reported claudeIdeBridge=$1 (last: ${got:-none})"
}

# Polls until `state_expr EXPR` prints WANT.
expect_state() { # label expr want
  local got="" tries=0
  while [ "$tries" -lt 100 ]; do
    got="$(state_expr "$2" 2>/dev/null || true)"
    if [ "$got" = "$3" ]; then
      echo "OK: $1 ($2 = $3)"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: $1: state $2 expected $3, got $got (state: $(ide_state 2>&1 | cut -c1-600))"
}

app_start() { # [NAME=value...]: extra variables for the app
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 \
    SIRIO_DEBUG_IDE_START_DELAY_MS=600 \
    ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} "$@" "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 300); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.1
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
  for _ in $(seq 1 600); do
    "$CTL" surface tabs read >/dev/null 2>&1 && return 0
    sleep 0.1
  done
  fail "sirio's control actions never became ready"
}
app_quit() {
  "$CTL" quit >/dev/null 2>&1 || true
  local tries=0
  while kill -0 "$APP_PID" 2>/dev/null && [ "$tries" -lt 200 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  kill -0 "$APP_PID" 2>/dev/null && fail "sirio did not quit after sirioctl quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}
select_repo() {
  ctl project add "$WT" >/dev/null 2>&1 || true
  local tries=0
  while [ "$tries" -lt 40 ]; do
    "$CTL" select-workspace --workspace "$WT" >/dev/null 2>&1 && break
    tries=$((tries + 1))
    sleep 0.5
  done
  case "$("$CTL" current-workspace --json 2>&1 || true)" in *"$WT"*) ;; *) fail "the repository is not the selected workspace" ;; esac
}
# The id of the worktree whose path is $1.
worktree_id() { # path
  "$CTL" list-workspaces --json | python3 -I -c '
import json, sys
for row in json.load(sys.stdin):
    if row["path"] == sys.argv[1]:
        print(row["id"])
        break' "$1"
}
project_id() { # path of one of its worktrees
  "$CTL" list-workspaces --json | python3 -I -c '
import json, sys
for row in json.load(sys.stdin):
    if row["path"] == sys.argv[1]:
        print(row["project"])
        break' "$1"
}

lock_file() { ls "$LOCK_DIR"/*.lock 2>/dev/null | head -n 1; }
lock_field() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
# Waits for a lockfile and prints its path.
wait_lockfile() {
  local tries=0 file=""
  while [ "$tries" -lt 100 ]; do
    file="$(lock_file || true)"
    [ -n "$file" ] && { printf '%s' "$file"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: no lockfile appeared in $LOCK_DIR"
}
wait_no_lockfile() {
  local tries=0
  while [ "$tries" -lt 100 ]; do
    [ -z "$(lock_file || true)" ] && return 0
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: the lockfile is still there: $(lock_file)"
}

# A pane in worktree $1 that writes the port it was given to a file, then answers it. A pane
# with none (or a blank one) answers "-".
pane_port() { # worktree-id
  local out="$RUN_DIR/port-$RANDOM.txt" id tries=0 got
  id="$("$CTL" panel create --worktree "$1" --cmd "printf '%s' \"\${CLAUDE_CODE_SSE_PORT-}\" > $out; sleep 300")"
  [ -n "$id" ] || fail "$CASE: panel create named no pane"
  while [ ! -f "$out" ] && [ "$tries" -lt 100 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ -f "$out" ] || fail "$CASE: the pane $id never wrote its environment"
  got="$(cat "$out")"
  "$CTL" panel close "$id" >/dev/null 2>&1 || true
  printf '%s' "${got:--}"
}

# The client in a pane of worktree $1, driven through a FIFO; its transcript is
# $RUN_DIR/client-N.jsonl. Sets CLIENT_FIFO, CLIENT_LOG and CLIENT_PANE.
start_client() { # worktree-id
  CLIENT_N=$((CLIENT_N + 1))
  CLIENT_FIFO="$RUN_DIR/client-$CLIENT_N.fifo"
  CLIENT_LOG="$RUN_DIR/client-$CLIENT_N.jsonl"
  rm -f "$CLIENT_FIFO" "$CLIENT_LOG"
  mkfifo "$CLIENT_FIFO"
  CLIENT_PANE="$("$CTL" panel create --worktree "$1")"
  [ -n "$CLIENT_PANE" ] || fail "$CASE: panel create named no pane"
  local tries=0
  while ! "$CTL" panel state "$CLIENT_PANE" >/dev/null 2>&1 && [ "$tries" -lt 100 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  "$CTL" panel write "$CLIENT_PANE" --enter --input "$(command -v python3) $CLIENT --fifo $CLIENT_FIFO --transcript $CLIENT_LOG" >/dev/null
  tries=0
  while ! grep -q '"event": "ready"' "$CLIENT_LOG" 2>/dev/null && [ "$tries" -lt 150 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  grep -q '"event": "ready"' "$CLIENT_LOG" 2>/dev/null || fail "$CASE: the client never started in pane $CLIENT_PANE (pane: $("$CTL" panel read "$CLIENT_PANE" 2>&1 | head -c 600))"
}
client_do() { printf '%s\n' "$*" >"$CLIENT_FIFO"; }
# How many transcript records match a Python expression over `r`.
client_count() { # expr
  python3 -I - "$CLIENT_LOG" "$1" <<'PY'
import json, sys
n = 0
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    if eval(sys.argv[2]):
        n += 1
print(n)
PY
}
wait_client() { # label expr count
  local tries=0 got=""
  while [ "$tries" -lt 100 ]; do
    got="$(client_count "$2")"
    [ "$got" = "$3" ] && { echo "OK: client $1 ($3)"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: client $1: expected $3 records of $2, got $got ($(tail -n 5 "$CLIENT_LOG" | cut -c1-300))"
}
end_client() {
  client_do quit
  "$CTL" panel close "$CLIENT_PANE" >/dev/null 2>&1 || true
}

find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"],
                         capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"],
                          capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
}
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window colours
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window with _NET_WM_PID=$APP_PID for $1"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png"
  [ -s "$OUT_DIR/frames/$1.png" ] || fail "$CASE: $1.png is empty"
  colours="$(identify -format '%k' "$OUT_DIR/frames/$1.png")"
  [ "$colours" -ge 200 ] || fail "$CASE: $1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}

# ---------------------------------------------------------------- the cases

case_run() { # name function
  CASE="$1"
  say "case $1"
  "$2"
  CASES_PASSED=$((CASES_PASSED + 1))
  echo "PASS $1"
}

WT_ID=""
PORT=""
TOKEN=""

case_lockfile_and_env() {
  local file mode dir_mode keys port folders
  file="$(wait_lockfile)"
  port="$(basename "$file" .lock)"
  case "$port" in ''|*[!0-9]*) fail "$CASE: the lockfile is not named by a port: $file" ;; esac
  keys="$(python3 -I -c 'import json, sys; print(",".join(sorted(json.load(open(sys.argv[1])))))' "$file")"
  [ "$keys" = "authToken,ideName,pid,runningInWindows,transport,workspaceFolders" ] \
    || fail "$CASE: the lockfile's keys are $keys"
  mode="$(stat -c %a "$file")"
  [ "$mode" = 600 ] || fail "$CASE: the lockfile's mode is $mode, not 600"
  dir_mode="$(stat -c %a "$LOCK_DIR")"
  [ "$dir_mode" = 700 ] || fail "$CASE: the lockfile directory's mode is $dir_mode, not 700"
  [ "$(lock_field "$file" ideName)" = "Sirio (debug)" ] || fail "$CASE: ideName is $(lock_field "$file" ideName)"
  [ "$(lock_field "$file" pid)" = "$APP_PID" ] || fail "$CASE: pid is $(lock_field "$file" pid), the app is $APP_PID"
  [ "$(lock_field "$file" transport)" = ws ] || fail "$CASE: transport is $(lock_field "$file" transport)"
  [ "$(lock_field "$file" runningInWindows)" = False ] || fail "$CASE: runningInWindows is $(lock_field "$file" runningInWindows)"
  folders="$(python3 -I -c 'import json, sys; print("\n".join(json.load(open(sys.argv[1]))["workspaceFolders"]))' "$file")"
  printf '%s\n' "$folders" | grep -Fxq "$WT" || fail "$CASE: workspaceFolders ($folders) lack $WT"
  PORT="$port"
  TOKEN="$(lock_field "$file" authToken)"
  [ "${#TOKEN}" = 32 ] || fail "$CASE: the token is ${#TOKEN} characters, not 32 hex"
  python3 -I - "$file" "$OUT_DIR/lockfile.redacted.json" <<'PY'
import json, sys
body = json.load(open(sys.argv[1]))
body["authToken"] = "<redacted>"
json.dump(body, open(sys.argv[2], "w"), indent=2, sort_keys=True)
PY
  echo "OK: lockfile $file: six keys, 0600 in a 0700 directory, Sirio (debug), pid $APP_PID, $WT"
  expect_state "the state's port is the lockfile's" 'd["port"]' "$port"
  expect_state "the state is enabled" 'd["enabled"]' True
  expect_state "the state names the lockfile" 'd["lockfile"]' "$port.lock"
  [ "$(ide_state | jq_py '"authToken" in json.dumps(d) or "'"$TOKEN"'" in json.dumps(d)')" = False ] \
    || fail "$CASE: the state holds the token"
  [ "$(pane_port "$WT_ID")" = "$port" ] || fail "$CASE: a new pane's CLAUDE_CODE_SSE_PORT is not $port"
  echo "OK: a new pane's environment holds the port $port"
}

case_token_refused() {
  start_client "$WT_ID"
  client_do "token wrong"
  wait_client "refused with 401" 'r.get("event") == "refused" and r.get("status") == 401' 1
  expect_state "no connection after a refused handshake" 'len([c for c in d["connections"]])' 0
  end_client
}

case_handshake() {
  start_client "$WT_ID"
  wait_client "connected, the server echoes mcp" 'r.get("event") == "connected" and r.get("subprotocol") == "mcp"' 1
  client_do "call tools/list {}"
  wait_client "tools/list answered" 'r.get("dir") == "in" and r.get("frame", {}).get("id") == "call-1"' 1
  local tools
  tools="$(python3 -I - "$CLIENT_LOG" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    if r.get("dir") == "in" and r.get("frame", {}).get("id") == "call-1":
        print(",".join(sorted(t["name"] for t in r["frame"]["result"]["tools"])))
PY
)"
  [ "$tools" = "close_tab,closeAllDiffTabs,getDiagnostics,openDiff" ] || [ "$tools" = "closeAllDiffTabs,close_tab,getDiagnostics,openDiff" ] \
    || fail "$CASE: tools/list is $tools"
  echo "OK: tools/list names exactly $tools"
  local pid
  pid="$(python3 -I - "$CLIENT_LOG" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    if r.get("dir") == "out" and r.get("frame", {}).get("method") == "ide_connected":
        print(r["frame"]["params"]["pid"])
PY
)"
  expect_state "one connection, the client's pid" '[c["pid"] for c in d["connections"]]' "[$pid]"
  # The held client stays for the next case.
  HELD_CLIENT_FIFO="$CLIENT_FIFO"
  HELD_CLIENT_LOG="$CLIENT_LOG"
  HELD_CLIENT_PANE="$CLIENT_PANE"
}

case_switch_off() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  sockcall surface.ide.switch on=false >/dev/null
  wait_no_lockfile
  echo "OK: the lockfile is gone"
  wait_client "the connection closed" 'r.get("event") == "closed"' 1
  expect_state "the state says off" 'd["enabled"]' False
  expect_settings_switch false
  expect_state "no port while off" 'd["port"]' None
  expect_state "no connections while off" 'len(d["connections"])' 0
  [ "$(pane_port "$WT_ID")" = "-" ] || fail "$CASE: a pane started with the switch off has a port"
  echo "OK: a pane started with the switch off has none"
  sockcall surface.ide.switch on=true >/dev/null
  local file
  file="$(wait_lockfile)"
  [ "$(basename "$file" .lock)" = "$PORT" ] || fail "$CASE: the port changed from $PORT to $(basename "$file" .lock)"
  [ "$(lock_field "$file" authToken)" = "$TOKEN" ] || fail "$CASE: the token changed with the switch"
  expect_state "back on, the same port" 'd["port"]' "$PORT"
  expect_settings_switch true
  [ "$(pane_port "$WT_ID")" = "$PORT" ] || fail "$CASE: a pane started after the switch came back lacks the port"
  echo "OK: on again: port $PORT and the token are the same, panes get the port"
  end_client
}

case_switch_flap() {
  local file count
  sockburst false true false true
  file="$(wait_lockfile)"
  expect_state "the flapped switch is on" 'd["enabled"]' True
  expect_state "the flapped bridge has a port" 'd["port"] is not None' True
  [ "$(basename "$file" .lock)" = "$PORT" ] || fail "$CASE: the port moved from $PORT to $(basename "$file" .lock)"
  expect_state "the state's port is still the original" 'd["port"]' "$PORT"
  sleep 1
  count="$(ls "$LOCK_DIR"/*.lock | wc -l)"
  [ "$count" = 1 ] || fail "$CASE: $count lockfiles exist: $(ls "$LOCK_DIR")"
  [ "$(lock_field "$(lock_file)" authToken)" = "$TOKEN" ] || fail "$CASE: the token changed"
  echo "OK: after off, on, off, on: port $PORT, one lockfile"
}

case_port_survives_catalog_change() {
  local project before after second second_id
  project="$(project_id "$WT")"
  [ -n "$project" ] || fail "$CASE: no project for $WT"
  before="$(lock_field "$(wait_lockfile)" workspaceFolders)"
  ctl new-workspace --project "$project" --branch ide-second >/dev/null
  local tries=0 file
  file="$(wait_lockfile)"
  while [ "$tries" -lt 100 ]; do
    after="$(lock_field "$file" workspaceFolders)"
    [ "$after" != "$before" ] && break
    tries=$((tries + 1))
    sleep 0.2
  done
  [ "$after" != "$before" ] || fail "$CASE: the lockfile's workspaceFolders never changed: $after"
  second="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-second"]
print(paths[0] if paths else "")')"
  [ -n "$second" ] || fail "$CASE: sirioctl does not list the new worktree"
  case "$after" in *"$second"*) ;; *) fail "$CASE: workspaceFolders ($after) lack $second" ;; esac
  echo "OK: workspaceFolders now names $second"
  [ "$(basename "$(wait_lockfile)" .lock)" = "$PORT" ] || fail "$CASE: the port changed with the catalog"
  second_id="$(worktree_id "$second")"
  # A pane can start only in a worktree that is mounted; selecting it mounts it.
  ctl select-workspace --workspace "$second_id" >/dev/null
  [ "$(pane_port "$second_id")" = "$PORT" ] || fail "$CASE: a pane in the new worktree lacks the port $PORT"
  echo "OK: a pane in the new worktree still gets $PORT"
}

case_restart_keeps_port() {
  start_client "$WT_ID"
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "one connection" 'len(d["connections"])' 1
  app_quit
  wait_no_lockfile
  echo "OK: quitting removed the lockfile"
  wait_client "the old connection closed" 'r.get("event") == "closed"' 1
  app_start
  select_repo
  local file
  file="$(wait_lockfile)"
  [ "$(basename "$file" .lock)" = "$PORT" ] || fail "$CASE: the port changed across a restart: $PORT then $(basename "$file" .lock)"
  [ "$(lock_field "$file" authToken)" = "$TOKEN" ] || fail "$CASE: the token changed across a restart"
  expect_state "the relaunched state's port" 'd["port"]' "$PORT"
  WT_ID="$(worktree_id "$WT")"
  client_do "token $TOKEN"
  wait_client "reconnected with the same token" 'r.get("event") == "connected"' 2
  expect_state "the reconnected client is a connection" 'len(d["connections"])' 1
  end_client
}

case_inherited_port() {
  app_quit
  app_start CLAUDE_CODE_SSE_PORT=1
  select_repo
  WT_ID="$(worktree_id "$WT")"
  wait_lockfile >/dev/null
  expect_state "the inherited port is not Sirio's" 'd["port"]' "$PORT"
  [ "$(pane_port "$WT_ID")" = "$PORT" ] || fail "$CASE: a pane got the launch's port, not Sirio's $PORT"
  echo "OK: a pane gets Sirio's $PORT, not the launch's 1"
  sockcall surface.ide.switch on=false >/dev/null
  wait_no_lockfile
  [ "$(pane_port "$WT_ID")" = "-" ] || fail "$CASE: with the switch off a pane kept the launch's port"
  echo "OK: with the switch off a pane has no port"
  sockcall surface.ide.switch on=true >/dev/null
  capture ide-final
}

app_start
select_repo
WT_ID="$(worktree_id "$WT")"
[ -n "$WT_ID" ] || fail "sirioctl does not list the worktree $WT"
echo "OK: the repository is the selected workspace ($WT_ID)"

case_run lockfile_and_env case_lockfile_and_env
case_run token_refused case_token_refused
case_run handshake case_handshake
case_run switch_off case_switch_off
case_run switch_flap case_switch_flap
case_run port_survives_catalog_change case_port_survives_catalog_change
case_run restart_keeps_port case_restart_keeps_port
case_run inherited_port case_inherited_port

stop_hosts
echo "IDE BRIDGE E2E OK ($CASES_PASSED cases, state-only $STATE_ONLY)"
