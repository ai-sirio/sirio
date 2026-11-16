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
#   owner_resolved    the state shows the client's connection owned by the pane it runs in
#                     (the pane id panel create returned) and by that pane's worktree
#   selection_utf16  opening a file sends its selection once (a focused file says it); in a
#                    file whose line 3 is `let s = "<emoji>x";` the `x`, selected with
#                    surface.editor.select, arrives as selection_changed with start {2, 11},
#                    end {2, 12} (UTF-16 columns), isEmpty false, text "x"
#   selection_coalesced
#                    20 surface.editor.select calls 25 ms apart: at most one notification per
#                    100 ms of the burst's duration, plus one, and the last one is the final
#                    selection (the 100 ms coalescing itself is proved by sirio_ide's unit
#                    tests; the control poll alone keeps this burst under the bound)
#   focus_sends_selection
#                    selecting a terminal tab sends nothing; selecting the file tab again sends
#                    exactly one notification, the current selection
#   at_mention       two Claude terminals: the first one's pane is focused before its client
#                    starts, the second is created after it, the file is opened, both clients
#                    connect and lines 5 to 7 are sent with "Add to Agent Thread"
#                    (surface.editor.context ... send-to-agent): at_mentioned lineStart 4,
#                    lineEnd 6 reaches the first (the one used last), not the second; its tab is
#                    focused and nothing is written into the pane
#   selection_while_terminal_active
#                    with the file in the Secondary half and the Claude terminal the active tab,
#                    a click and Shift+Down (xdotool) in the file reach the client as
#                    selection_changed for the new lines
#   at_mention_to_new_terminal
#                    a terminal created after the others, focused when its client connects, gets
#                    the next at-mention
#   no_eligible_agent_toast
#                    with every terminal and the chat closed, the same command leaves the toast
#                    "No Claude terminal or chat in this worktree" in the state's toasts
#   at_mention_without_focus_history
#                    in a fresh worktree, a terminal nobody focused (another tab was active when
#                    its client connected) still gets the at-mention
#   owner_after_restart
#                    the client's pane is in a worktree that is not selected; Sirio quits and
#                    relaunches and the client reconnects: the state shows it ownerless, in
#                    that worktree (its cwd); six seconds later (the reconnect's retries are
#                    over) selecting the worktree gives it an owner; one at-mention from a file
#                    of that worktree then reaches it (nothing was focused since the relaunch),
#                    focuses its tab and leaves the toast count as it was
#
# The artifact: --out-dir DIR (default artifacts/ide-bridge-e2e-<stamp>-<pid>) keeps
# transcript.log, app.log, client-*.jsonl (every frame a client sent and received), the lockfile
# with its authToken redacted, the PID-matched window captures in frames/ and socket.jsonl: one
# JSON line per request this script sent to the app's control socket, with the app's complete
# answer. The state, settings, switch and selection-burst calls go straight to the socket; every sirioctl call
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
CLIENT_TAB=""

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
# N `surface.editor.select` calls 25 ms apart, each on a connection of its own opened beforehand.
# Sent without a gap they would be one pass of the app's control poll and one change of the
# editor's selection (gpui merges the notifies of a pass), so they are spread over several
# passes. Be clear about what that proves: the app takes about 100 ms per pass here, so the
# bound the case asserts also holds for a Sirio that does not coalesce. The case proves the
# contract (the bound and the last notification being the final selection); the 100 ms coalescing
# is proved by the unit tests of sirio_ide::coalesce. Logged like sockcall's, in request order; the ranges are the arguments, the
# elapsed milliseconds from the first send to the last answer are printed.
cat >"$RUN_DIR/selburst.py" <<'SELBURST'
import json, os, socket, sys, time

log_path = os.path.join(os.environ["RUN_DIR"], "socket.jsonl")
requests = [{"id": f"selburst-{i}-{time.time_ns()}", "method": "surface.editor.select",
             "params": {"ranges": ranges}} for i, ranges in enumerate(sys.argv[1:])]


def write_line(entry):
    with open(log_path, "a", encoding="utf-8") as log:
        log.write(json.dumps(entry, sort_keys=True) + "\n")


socks = []
try:
    for _ in requests:
        sock = socket.socket(socket.AF_UNIX)
        sock.settimeout(20)
        sock.connect(os.environ["SIRIO_SOCKET"])
        socks.append(sock)
    started = time.monotonic()
    for sock, request in zip(socks, requests):
        sock.sendall((json.dumps(request) + "\n").encode())
        time.sleep(0.025)
    answers = []
    for sock in socks:
        data = b""
        while not data.endswith(b"\n"):
            chunk = sock.recv(65536)
            if not chunk:
                break
            data += chunk
        if not data:
            raise EOFError("no answer")
        answers.append(json.loads(data))
    elapsed = int((time.monotonic() - started) * 1000)
except (OSError, EOFError, ValueError) as error:
    for request in requests:
        write_line({"via": "selburst", "request": request, "error": type(error).__name__})
    raise
finally:
    for sock in socks:
        sock.close()
failed = None
for request, response in zip(requests, answers):
    write_line({"via": "selburst", "request": request, "response": response})
    if not response.get("ok") and failed is None:
        failed = f"select {request['params']['ranges']} failed: {response.get('error')}"
if failed:
    sys.exit(failed)
print(elapsed)
SELBURST
selburst() { python3 -I "$RUN_DIR/selburst.py" "$@"; }
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
# The client in an application terminal tab of the selected worktree (`surface.terminal.open`:
# a real pane Sirio holds a view of, which `panel create`'s host-only session is not), run by
# `sh -c` as the shell's child. Sets CLIENT_FIFO, CLIENT_LOG, CLIENT_PANE and CLIENT_TAB.
start_app_client() { # [gate file: the client starts when it exists]
  CLIENT_N=$((CLIENT_N + 1))
  CLIENT_FIFO="$RUN_DIR/client-$CLIENT_N.fifo"
  CLIENT_LOG="$RUN_DIR/client-$CLIENT_N.jsonl"
  rm -f "$CLIENT_FIFO" "$CLIENT_LOG"
  mkfifo "$CLIENT_FIFO"
  local wait_for=""
  [ -z "${1:-}" ] || wait_for="while [ ! -e $1 ]; do sleep 0.1; done; "
  CLIENT_PANE="$(sockcall surface.terminal.open "cmd=${wait_for}$(command -v python3) $CLIENT --fifo $CLIENT_FIFO --transcript $CLIENT_LOG; :" | jq_py 'd["pane"]')"
  [ -n "$CLIENT_PANE" ] || fail "$CASE: surface.terminal.open named no pane"
  CLIENT_TAB="$(tabs_field count)"
  [ -z "${1:-}" ] || return 0
  wait_client_ready
}
wait_client_ready() {
  local tries=0
  while ! grep -q '"event": "ready"' "$CLIENT_LOG" 2>/dev/null && [ "$tries" -lt 150 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  grep -q '"event": "ready"' "$CLIENT_LOG" 2>/dev/null || fail "$CASE: the client never started in $CLIENT_PANE (tab $CLIENT_TAB)"
}
end_app_client() {
  client_do quit
  sockcall surface.tabs.close "index=$CLIENT_TAB" confirm=yes >/dev/null
}
# Closes every tab of a kind (file, chat, terminal): their clients end with them.
close_tabs_of_kind() { # kind
  local index
  while index="$(tab_index "$1")" && [ "$index" != 0 ]; do
    sockcall surface.tabs.close "index=$index" confirm=yes >/dev/null
  done
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

# The params of every notification METHOD the client received, one JSON object per line.
client_notes() { # method
  python3 -I - "$CLIENT_LOG" "$1" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    frame = r.get("frame", {})
    if r.get("dir") == "in" and frame.get("method") == sys.argv[2]:
        print(json.dumps(frame["params"], sort_keys=True))
PY
}
note_count() { client_notes "$1" | wc -l | tr -d ' '; }
# Polls until the client holds COUNT notifications of METHOD.
wait_notes() { # method count
  local tries=0 got=""
  while [ "$tries" -lt 100 ]; do
    got="$(note_count "$1")"
    [ "$got" = "$2" ] && return 0
    tries=$((tries + 1))
    sleep 0.1
  done
  fail "$CASE: the client holds $got $1 notifications, expected $2 (last: $(client_notes "$1" | tail -n 2 | cut -c1-300))"
}
# The position of the last tab of a kind (file, chat, terminal).
tab_index() { # kind
  "$CTL" surface tabs read --json | python3 -I -c '
import json, sys
d = json.load(sys.stdin)[0]
print(max([int(k[4:]) for k, v in d.items() if k.startswith("tab.") and v.startswith(sys.argv[1] + "|")] or [0]))' "$1"
}
file_tab_index() { tab_index file; }
tabs_field() { "$CTL" surface tabs read --json | jq_py "d[0][\"$1\"]"; }
# The file the selection cases use: line 3 is `let s = "<emoji>x";` (the x is column 11 in UTF-16
# units, 13 bytes in). Prints the byte offsets: x, start of line 5, end of line 5, a point inside
# line 6, end of line 7.
write_sel_file() {
  python3 -I - "$1" <<'PY'
import sys
lines = ["fn main() {", "// keep", 'let s = "\U0001F600x";', "// four", "five();", "six();", "seven();", "}"]
text = "\n".join(lines) + "\n"
open(sys.argv[1], "w", encoding="utf-8").write(text)
data = text.encode("utf-8")
starts = [0]
for i, byte in enumerate(data):
    if byte == 0x0A:
        starts.append(i + 1)
x = starts[2] + len('let s = "'.encode()) + len("\U0001F600".encode())
assert data[x:x + 1] == b"x"
print(x, starts[4], starts[4] + len("five();"), starts[5] + 2, starts[6] + len("seven();"))
PY
}
# Selects RANGES on the active file tab, waiting for a file that is still loading.
select_ranges() { # ranges
  local tries=0
  while ! sockcall surface.editor.select "ranges=$1" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -lt 50 ] || fail "$CASE: surface.editor.select $1 never succeeded: $(sockcall surface.editor.select "ranges=$1" 2>&1 | tail -n 1)"
    sleep 0.2
  done
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

SEL_FILE="$WT/sel.rs"
SEL_X=""; SEL_L5S=""; SEL_L5E=""; SEL_L6=""; SEL_L7E=""
FILE_TAB=""

case_owner_resolved() {
  # The switch was turned back on by the last case: a terminal made now needs the port.
  wait_lockfile >/dev/null
  expect_state "the bridge is back on its port" 'd["port"]' "$PORT"
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "one connection" 'len(d["connections"])' 1
  expect_state "the connection is owned by the pane the client runs in" 'd["connections"][0]["owner_pane"]' "$CLIENT_PANE"
  expect_state "the owner's worktree is the repository" 'd["connections"][0]["worktree"]' "$WT"
  HELD_CLIENT_FIFO="$CLIENT_FIFO"
  HELD_CLIENT_LOG="$CLIENT_LOG"
  HELD_CLIENT_PANE="$CLIENT_PANE"
}

case_selection_utf16() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  read -r SEL_X SEL_L5S SEL_L5E SEL_L6 SEL_L7E < <(write_sel_file "$SEL_FILE")
  local before
  before="$(note_count selection_changed)"
  ctl surface editor open --path "$SEL_FILE" >/dev/null
  FILE_TAB="$(tabs_field active)"
  # Opening a file focuses its tab, and focusing a file tab says its selection (spec 4.4).
  wait_notes selection_changed "$((before + 1))"
  sleep 0.5
  [ "$(note_count selection_changed)" = "$((before + 1))" ] || fail "$CASE: opening the file said its selection more than once"
  echo "OK: opening the file sent its selection once"
  before="$(note_count selection_changed)"
  select_ranges "$SEL_X..$((SEL_X + 1))"
  wait_notes selection_changed "$((before + 1))"
  local got
  got="$(client_notes selection_changed | tail -n 1)"
  python3 -I - "$got" "$SEL_FILE" <<'PY' || fail "$CASE: the notification is not the selected x: $got"
import json, sys
n = json.loads(sys.argv[1])
s = n["selection"]
ok = (n["text"] == "x" and n["filePath"] == sys.argv[2] and n["fileUrl"] == "file://" + sys.argv[2]
      and s["start"] == {"line": 2, "character": 11} and s["end"] == {"line": 2, "character": 12}
      and s["isEmpty"] is False)
sys.exit(0 if ok else 1)
PY
  echo "OK: selection_changed: start {2,11}, end {2,12}, isEmpty false, text x"
}

case_selection_coalesced() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  local before elapsed ranges="" i got bound
  before="$(note_count selection_changed)"
  for i in $(seq 1 19); do ranges="$ranges 0..$i"; done
  # shellcheck disable=SC2086
  elapsed="$(selburst $ranges "$SEL_L5S..$SEL_L5E")"
  bound=$((elapsed / 100 + 1))
  echo "the burst of 20 selections took ${elapsed} ms: at most $bound notifications"
  local tries=0
  while [ "$tries" -lt 100 ]; do
    got="$(client_notes selection_changed | tail -n 1)"
    case "$got" in *'"text": "five();"'*|*'"text":"five();"'*) break ;; esac
    tries=$((tries + 1))
    sleep 0.1
  done
  python3 -I - "$got" <<'PY' || fail "$CASE: the last notification is not the final selection: $got"
import json, sys
n = json.loads(sys.argv[1])
s = n["selection"]
sys.exit(0 if (n["text"] == "five();" and s["start"] == {"line": 4, "character": 0}
               and s["end"] == {"line": 4, "character": 7}) else 1)
PY
  sleep 0.5
  got="$(($(note_count selection_changed) - before))"
  [ "$got" -ge 1 ] && [ "$got" -le "$bound" ] || fail "$CASE: $got notifications for a ${elapsed} ms burst, expected 1 to $bound"
  echo "OK: $got notifications for the burst (bound $bound), the last is the final selection"
}

case_focus_sends_selection() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  local before
  before="$(note_count selection_changed)"
  ctl surface tabs select "$CLIENT_TAB" >/dev/null
  sleep 0.6
  [ "$(note_count selection_changed)" = "$before" ] || fail "$CASE: focusing a terminal tab sent a selection"
  echo "OK: focusing a terminal tab sends nothing"
  ctl surface tabs select "$FILE_TAB" >/dev/null
  wait_notes selection_changed "$((before + 1))"
  sleep 0.6
  [ "$(note_count selection_changed)" = "$((before + 1))" ] || fail "$CASE: focusing the file tab sent more than one notification"
  case "$(client_notes selection_changed | tail -n 1)" in *five*) ;; *) fail "$CASE: the focus notification is not the current selection" ;; esac
  echo "OK: focusing the file tab sends exactly one notification, the current selection"
}

# Two Claude terminals: the first one's pane is focused while its client has not started yet (so
# the focus is recorded before the pane is eligible), the second is created after it (active,
# never focused by a click), the file is opened (so neither terminal is focused when its client
# connects) and both clients start; then lines 5 to 7 are selected and sent: the first terminal gets the at-mention (the one used
# last), the second does not, though it connected later.
case_at_mention() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  close_tabs_of_kind file
  end_app_client
  local gate_a="$RUN_DIR/gate-a" gate_b="$RUN_DIR/gate-b"
  rm -f "$gate_a" "$gate_b"
  start_app_client "$gate_a"
  A_FIFO="$CLIENT_FIFO" A_LOG="$CLIENT_LOG" A_PANE="$CLIENT_PANE" A_TAB="$CLIENT_TAB"
  ctl surface tabs select "$A_TAB" >/dev/null
  start_app_client "$gate_b"
  B_FIFO="$CLIENT_FIFO" B_LOG="$CLIENT_LOG" B_PANE="$CLIENT_PANE" B_TAB="$CLIENT_TAB"
  # The file becomes the active tab: neither terminal is focused when its client connects.
  ctl surface editor open --path "$SEL_FILE" >/dev/null
  FILE_TAB="$(tabs_field active)"
  touch "$gate_a"
  CLIENT_FIFO="$A_FIFO" CLIENT_LOG="$A_LOG" CLIENT_PANE="$A_PANE" CLIENT_TAB="$A_TAB"
  wait_client_ready
  wait_client "A connected" 'r.get("event") == "connected"' 1
  touch "$gate_b"
  CLIENT_FIFO="$B_FIFO" CLIENT_LOG="$B_LOG" CLIENT_PANE="$B_PANE" CLIENT_TAB="$B_TAB"
  wait_client_ready
  wait_client "B connected" 'r.get("event") == "connected"' 1
  expect_state "A and B are owned by their panes" 'sorted(c["owner_pane"] for c in d["connections"])' "$(python3 -I -c 'import sys; print(sorted(sys.argv[1:]))' "$A_PANE" "$B_PANE")"
  CLIENT_FIFO="$A_FIFO" CLIENT_LOG="$A_LOG" CLIENT_PANE="$A_PANE" CLIENT_TAB="$A_TAB"
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  wait_notes at_mentioned 1
  python3 -I - "$(client_notes at_mentioned | tail -n 1)" "$SEL_FILE" <<'PY' || fail "$CASE: the at_mentioned params are wrong: $(client_notes at_mentioned | tail -n 1)"
import json, sys
n = json.loads(sys.argv[1])
sys.exit(0 if (n == {"filePath": sys.argv[2], "lineStart": 4, "lineEnd": 6}) else 1)
PY
  echo "OK: the terminal used last got at_mentioned lineStart 4, lineEnd 6"
  [ "$(CLIENT_LOG="$B_LOG" note_count at_mentioned)" = 0 ] || fail "$CASE: the other terminal got the at-mention too"
  local tries=0
  while [ "$(tabs_field active)" != "$A_TAB" ] && [ "$tries" -lt 50 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ "$(tabs_field active)" = "$A_TAB" ] || fail "$CASE: the terminal's tab ($A_TAB) is not focused (active: $(tabs_field active))"
  echo "OK: the terminal's tab is focused"
  case "$("$CTL" panel read "$A_PANE" 2>&1)" in *sel.rs*) fail "$CASE: something was written into the pane" ;; esac
  echo "OK: nothing was written into the pane"
  capture ide-at-mention
}

# The file sits in the Secondary half, the Claude terminal of the Primary half is the active tab
# (as after an at-mention), and the user selects new lines in the file with the mouse and keys:
# the selection must reach Claude though the file's tab is not the active one.
case_selection_while_terminal_active() {
  CLIENT_FIFO="$A_FIFO" CLIENT_LOG="$A_LOG" CLIENT_PANE="$A_PANE" CLIENT_TAB="$A_TAB"
  [ "$(tabs_field active)" = "$A_TAB" ] || fail "$CASE: the terminal is not the active tab"
  local window geometry width height before
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window for pid $APP_PID"
  geometry="$(DISPLAY="$DISPLAY_TARGET" xdotool getwindowgeometry "$window" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')"
  read -r width height <<<"$geometry"
  before="$(note_count selection_changed)"
  # Line 3 of the file, in the middle of the Secondary half (measured on ide-at-mention.png).
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$((width * 62 / 100))" "$((height * 18 / 100))" click 1
  sleep 0.3
  DISPLAY="$DISPLAY_TARGET" xdotool key shift+Down
  [ "$(tabs_field active)" = "$A_TAB" ] || fail "$CASE: the click made the file's tab the active one; the layout under test is gone"
  local tries=0 got=""
  while [ "$tries" -lt 50 ]; do
    got="$(client_notes selection_changed | tail -n 1)"
    python3 -I - "$got" <<'PY' && break
import json, sys
try:
    s = json.loads(sys.argv[1])["selection"]
except (ValueError, KeyError, IndexError):
    sys.exit(1)
sys.exit(0 if (s["start"]["line"] == 2 and s["end"]["line"] == 3 and s["isEmpty"] is False) else 1)
PY
    tries=$((tries + 1))
    sleep 0.2
  done
  [ "$tries" -lt 50 ] || fail "$CASE: no selection_changed for the new lines reached the client (last: $got; $before before)"
  echo "OK: the selection of the visible, inactive file reached the client: $got"
}

# A terminal created after another one is the active tab and has had no click, so nothing
# recorded it: its client connecting while it is focused makes it the one used last.
case_at_mention_to_new_terminal() {
  start_app_client
  wait_client "C connected" 'r.get("event") == "connected"' 1
  expect_state "C is owned by its pane" '"'"$CLIENT_PANE"'" in [c["owner_pane"] for c in d["connections"]]' True
  ctl surface tabs select "$FILE_TAB" >/dev/null
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  wait_notes at_mentioned 1
  [ "$(CLIENT_LOG="$A_LOG" note_count at_mentioned)" = 1 ] || fail "$CASE: the earlier terminal got a second at-mention"
  echo "OK: the new terminal, focused when it connected, got the at-mention"
}

case_no_eligible_agent_toast() {
  close_tabs_of_kind terminal
  expect_state "no connection once the clients are gone" 'len(d["connections"])' 0
  # A new worktree opens with a chat tab, which can take an at-mention: close it.
  close_tabs_of_kind chat
  ctl surface tabs select "$(file_tab_index)" >/dev/null
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  expect_state "the toast is in the state" '"No Claude terminal or chat in this worktree" in d["toasts"]' True
}

# A terminal nobody focused (created, then another tab made active before its client connects) is
# owned but is in no focus history: the at-mention reaches it, and leaves no toast.
case_at_mention_without_focus_history() {
  local project third third_id gate="$RUN_DIR/gate-third"
  project="$(project_id "$WT")"
  ctl new-workspace --project "$project" --branch ide-third >/dev/null
  third="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-third"]
print(paths[0] if paths else "")')"
  [ -n "$third" ] || fail "$CASE: sirioctl does not list the third worktree"
  third_id="$(worktree_id "$third")"
  write_sel_file "$third/sel.rs" >/dev/null
  ctl select-workspace --workspace "$third_id" >/dev/null
  rm -f "$gate"
  start_app_client "$gate"
  ctl surface editor open --path "$third/sel.rs" >/dev/null
  touch "$gate"
  wait_client_ready
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "owned by its pane" '"'"$CLIENT_PANE"'" in [c["owner_pane"] for c in d["connections"]]' True
  read -r SEL_X SEL_L5S SEL_L5E SEL_L6 SEL_L7E < <(write_sel_file "$third/sel.rs")
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  wait_notes at_mentioned 1
  echo "OK: the owned terminal nobody focused got the at-mention"
  end_app_client
  expect_state "no connection once that client is gone" 'len(d["connections"])' 0
  ctl select-workspace --workspace "$WT_ID" >/dev/null
}

case_owner_after_restart() {
  local second second_id tries=0
  second="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-second"]
print(paths[0] if paths else "")')"
  [ -n "$second" ] || fail "$CASE: sirioctl does not list the second worktree"
  second_id="$(worktree_id "$second")"
  ctl select-workspace --workspace "$second_id" >/dev/null
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "owned while its worktree is selected" 'd["connections"][0]["owner_pane"]' "$CLIENT_PANE"
  # A file is the active tab when Sirio quits: after the relaunch nothing focuses the terminal.
  read -r SEL_X SEL_L5S SEL_L5E SEL_L6 SEL_L7E < <(write_sel_file "$second/sel.rs")
  ctl surface editor open --path "$second/sel.rs" >/dev/null
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  app_quit
  wait_client "the old connection closed" 'r.get("event") == "closed"' 1
  app_start
  select_repo
  WT_ID="$(worktree_id "$WT")"
  client_do "token $TOKEN"
  wait_client "reconnected" 'r.get("event") == "connected"' 2
  expect_state "the reconnected client is a connection" 'len(d["connections"])' 1
  expect_state "ownerless while its worktree is not mounted" 'd["connections"][0]["owner_pane"]' None
  expect_state "its worktree is found from its cwd" 'd["connections"][0]["worktree"]' "$second"
  # The reconnect's own retries end within five seconds: select the worktree after that, so only
  # the selection can resolve the owner.
  sleep 6
  ctl select-workspace --workspace "$second_id" >/dev/null
  expect_state "selecting the worktree gives it an owner" 'd["connections"][0]["owner_pane"] is not None' True
  expect_state "the owner's worktree is the second one" 'd["connections"][0]["worktree"]' "$second"
  # After the relaunch nothing has been focused: one at-mention still reaches the owned terminal,
  # focuses its tab and raises no toast.
  local before toasts_before toasts_after client_tab
  before="$(note_count at_mentioned)"
  toasts_before="$(state_expr 'len(d["toasts"])')"
  client_tab="$(tab_index terminal)"
  [ "$(tabs_field active)" != "$client_tab" ] || fail "$CASE: the terminal is already the active tab"
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  wait_notes at_mentioned "$((before + 1))"
  tries=0
  while [ "$(tabs_field active)" != "$client_tab" ] && [ "$tries" -lt 50 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ "$(tabs_field active)" = "$client_tab" ] || fail "$CASE: the client's tab ($client_tab) is not focused (active: $(tabs_field active))"
  sleep 0.5
  toasts_after="$(state_expr 'len(d["toasts"])')"
  [ "$toasts_after" = "$toasts_before" ] || fail "$CASE: the at-mention raised a toast ($toasts_before then $toasts_after)"
  echo "OK: after the relaunch the at-mention reached the client, focused its tab and raised no toast ($toasts_before toasts both times)"
  end_app_client
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
case_run owner_resolved case_owner_resolved
case_run selection_utf16 case_selection_utf16
case_run selection_coalesced case_selection_coalesced
case_run focus_sends_selection case_focus_sends_selection
case_run at_mention case_at_mention
case_run selection_while_terminal_active case_selection_while_terminal_active
case_run at_mention_to_new_terminal case_at_mention_to_new_terminal
case_run no_eligible_agent_toast case_no_eligible_agent_toast
case_run at_mention_without_focus_history case_at_mention_without_focus_history
case_run owner_after_restart case_owner_after_restart

stop_hosts
echo "IDE BRIDGE E2E OK ($CASES_PASSED cases, state-only $STATE_ONLY)"
