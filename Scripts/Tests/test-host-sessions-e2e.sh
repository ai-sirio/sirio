#!/bin/bash
set -eEuo pipefail

# The app-level end-to-end test of SP2's terminal sessions (spec §9.3): a real
# debug `sirio` on an isolated display, a real `sirio-host` that the app adopts
# or starts, and the control socket (`sirioctl`) plus the host's own probe
# (`host_probe`) to observe both sides. What a user would see is captured by
# PID of the app under test.
#
# Each case gets a world of its own: a repository, a SIRIO_DB, a control socket
# under a short path, a SIRIO_HOST_HOME and a SIRIO_CREDENTIALS. Nothing touches
# the user's ~/.local/share/Sirio, and nothing is killed by name: only the PIDs
# this script started, and the hosts named by their own state files.
#
# The cases, in order (spec §9.3 plus the review-focus cases):
#    1. survival-kill   a terminal's session outlives kill -9 of the app: the
#                       same pid keeps running, the relaunched app re-attaches
#    2. survival-quit   the same with a graceful `sirioctl quit`
#    3. close-ends      closing the tab ends its session and its pid
#    4. end-all         End All sessions ends both pids and the host
#    5. fallback        with no host binary the terminal still opens, on
#                       LocalLink, and says so (survives=false)
#    6. orphan          a tab not yet saved when the app dies is adopted from
#                       the host on relaunch (repeated until reproduced)
#    7. show-session    `panel show` opens one tab; closing it keeps the session;
#                       a relaunch does not restore the shown tab
#    8. slow-host       the host starts late; the restored tab's session is
#                       created on the host, never on LocalLink
#    9. isolation       two worlds: each app and each host see only their own
#   10. echo-latency    keystroke echo through the host and through LocalLink,
#                       recorded beside each other, never a FAIL
#   11. close-worktree  closing a worktree ends the control sessions inside it;
#                       a session in a directory outside it survives
#   12. duplicate-id    a control session holds the id the app will use next;
#                       a fresh app terminal never attaches to it (finding 11)
#   13. adopt-two-worktrees  a terminal in each of two worktrees survives kill -9;
#                       the relaunch adds no tab and both pids keep running
#   14. unverifiable-no-fallback  the host holds its lock but does not answer: a
#                       restored tab runs no local shell and reports no
#                       survives=false; SIGCONT, and it attaches (finding 7)
#
# Prints `CASE <n> <name> PASS|FAIL` per case, `HOST SESSIONS E2E OK (<n> cases,
# <s>s)` at the end.
#
# --out-dir DIR keeps transcript.log, probe-calls.log, every app and host log,
# the host state files and the PID-matched window captures in frames/.
# --no-build skips the cargo build. --state-only takes no captures (macOS takes
# none yet). --xvfb starts a private Xvfb on a free display; --display :N uses
# one that is already there.
#
# Usage: Scripts/Tests/test-host-sessions-e2e.sh [--out-dir DIR] [--no-build]
#          [--state-only] [--xvfb | --display :N] [--only N[,N...]]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR=""
ONLY=""
NO_BUILD=0
STATE_ONLY=0
START_XVFB=0
DISPLAY_TARGET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --no-build) NO_BUILD=1; shift ;;
    --state-only) STATE_ONLY=1; shift ;;
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --only) ONLY=",${2:?--only needs case numbers},"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$(uname -s)" in
  Darwin) OS=macos ;;
  Linux) OS=linux ;;
  *) echo "HOST SESSIONS E2E runs on Linux and macOS only" >&2; exit 2 ;;
esac
if [ "$OS" = macos ] && [ "$STATE_ONLY" -eq 0 ]; then
  # Ruling: no PID-matched window capture exists for macOS in Scripts/ yet, so
  # the macOS run is state-only rather than an untested capture path.
  echo "note: macOS run is state-only (no window capture path for macOS yet)"
  STATE_ONLY=1
fi

TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$REPO_ROOT/$TARGET_DIR" ;; esac
BIN_DIR="$TARGET_DIR/debug"
APP_BIN="$BIN_DIR/sirio"
CTL="$BIN_DIR/sirioctl"
HOST_BIN="$BIN_DIR/sirio-host"
PROBE="$BIN_DIR/examples/host_probe"

if [ -z "$OUT_DIR" ]; then OUT_DIR="$REPO_ROOT/artifacts/host-sessions-e2e-$(date +%Y%m%d-%H%M%S)-$$"; fi
mkdir -p "$OUT_DIR/frames"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "CASE $N $CASE_NAME FAIL: $*" >&2
  show_evidence >&2
  exit 1
}
unexpected() {
  echo "CASE $N $CASE_NAME FAIL: unexpected error at line $1 (a command the script did not expect to fail)" >&2
  show_evidence >&2
}
trap 'unexpected $LINENO' ERR

# Everything this script starts, so the exit trap can end it.
APPS=()
WORLDS=()
XVFB_PID=""
RUN=""
N=0
CASE_NAME=""
CASE_STARTED=0
NOTE=""
CASES_PASSED=0
W=""
APP_PID=""
APP_EXE="$APP_BIN"
RC=0
OUT=""
LAST_CALL=""
TERM_LIST=""
LOCAL_ECHO=""
HOST_ECHO=""
FRAME_NO=0
WT_ID=""
# Hosts this script stopped with SIGSTOP: resumed before anything ends them.
STOPPED=()

show_evidence() {
  [ -n "$LAST_CALL" ] && { echo "--- last call: $LAST_CALL (exit $RC)"; printf '%s\n' "$OUT"; }
  local w f
  for w in ${WORLDS[@]+"${WORLDS[@]}"}; do
    [ -d "$w" ] || continue
    for f in "$w"/app.log "$w"/host/log/*.log "$w"/host/host-v*.json; do
      [ -f "$f" ] || continue
      echo "--- $f"
      tail -n 30 "$f"
    done
  done
}

pass() {
  local took=$(( $(date +%s) - CASE_STARTED ))
  echo "CASE $N $CASE_NAME PASS${NOTE:+ ($NOTE)} ${took}s"
  CASES_PASSED=$((CASES_PASSED + 1))
}

# ---------------------------------------------------------------- processes

alive() { # a native pid that runs and is not a zombie
  kill -0 "$1" 2>/dev/null || return 1
  if [ -r "/proc/$1/stat" ]; then
    local stat rest
    stat="$(cat "/proc/$1/stat" 2>/dev/null)" || return 1
    rest="${stat##*) }"
    [ "${rest%% *}" != Z ]
  fi
}
dead() { ! alive "$1"; }

# wait_until SECONDS CMD...: polls CMD every 0.1 s; succeeds as soon as it does.
wait_until() {
  local n=$(( $1 * 10 ))
  shift
  while [ "$n" -gt 0 ]; do
    if "$@"; then return 0; fi
    sleep 0.1
    n=$((n - 1))
  done
  "$@"
}

# Only a process that is a sirio-host is ever signalled by the cleanup.
is_a_host() {
  local comm
  if [ -r "/proc/$1/comm" ]; then comm="$(cat "/proc/$1/comm" 2>/dev/null)"
  else comm="$(ps -p "$1" -o comm= 2>/dev/null || true)"
  fi
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}

pid_alive() { "$PROBE" alive "$1" >/dev/null 2>&1; }
pid_dead() { ! pid_alive "$1"; }

# ---------------------------------------------------------------- worlds

# Points every SIRIO_* variable and the probe at a world's own directory.
use_world() {
  W="$1"
  export SIRIO_SOCKET="$W/c.sock"
  export SIRIO_DB="$W/s.sqlite"
  export SIRIO_CREDENTIALS="$W/cred.json"
  export SIRIO_HOST_HOME="$W/host"
  export SIRIO_HOST_BIN="$HOST_BIN"
  # Every XDG directory the app or a tool may write to is this world's own, so
  # no run can touch the user's real state, data, config or cache.
  export XDG_STATE_HOME="$W/xdg/state" XDG_DATA_HOME="$W/xdg/data"
  export XDG_CONFIG_HOME="$W/xdg/config" XDG_CACHE_HOME="$W/xdg/cache"
  export XDG_RUNTIME_DIR="$W/xdg/runtime"
  mkdir -p "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR"
  chmod 700 "$XDG_RUNTIME_DIR"
  unset SIRIO_HOST_START_DELAY_MS SIRIO_HOST_IDLE_GRACE_MS SIRIO_HOST_PROTOCOL_MAJOR
}

# A new world: a repository on its own branch and an empty host root.
new_world() { # name
  local name="$1"
  local dir="$RUN/$name"
  mkdir -p "$dir/repo" "$dir/host"
  git -C "$dir/repo" init -q -b main
  git -C "$dir/repo" -c user.email=t@example.com -c user.name=Tester commit -q --allow-empty -m first
  WORLDS+=("$dir")
  use_world "$dir"
}

# Starts the app of the current world. Extra NAME=value arguments go to its
# environment. Waits for its control socket.
app_start() {
  local extra=("$@")
  rm -f "$SIRIO_SOCKET"
  if [ "$STATE_ONLY" -eq 1 ] || [ -z "$DISPLAY_TARGET" ]; then
    (cd "$W/repo" && exec env -u DISPLAY -u WAYLAND_DISPLAY ${extra[@]+"${extra[@]}"} "$APP_EXE" >>"$W/app.log" 2>&1) &
  else
    (cd "$W/repo" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 \
      ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} ${extra[@]+"${extra[@]}"} "$APP_EXE" >>"$W/app.log" 2>&1) &
  fi
  APP_PID=$!
  APPS+=("$APP_PID")
  for _ in $(seq 1 300); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.1
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
  # The socket answers before the window is drawn; an action that needs the
  # window (a tab read is one) answers once the app is ready for them.
  wait_until 60 ctl_ready || fail "sirio's control actions never became ready"
}
ctl_ready() { "$CTL" surface tabs read >/dev/null 2>&1; }

app_kill9() {
  kill -9 "$APP_PID" 2>/dev/null || true
  wait_until 10 dead "$APP_PID" || fail "app $APP_PID did not die after kill -9"
}

app_quit() {
  "$CTL" quit >/dev/null 2>&1 || true
  wait_until 20 dead "$APP_PID" || fail "app $APP_PID did not quit after sirioctl quit"
}

# ---------------------------------------------------------------- the probe

# A host_probe call against the current world's host root. Its answer goes
# to OUT and its exit status to RC; never fails the case by itself.
probe() {
  local outf="$W/probe.out" errf="$W/probe.err"
  RC=0
  "$PROBE" "$@" >"$outf" 2>"$errf" || RC=$?
  OUT="$(tr -d '\r' <"$outf")"
  LAST_CALL="host_probe $*"
  {
    echo "[case $N $CASE_NAME] \$ $LAST_CALL -> exit $RC"
    printf '%s\n' "$OUT" | sed 's/^/    /'
    sed 's/^/    stderr: /' "$errf"
  } >>"$OUT_DIR/probe-calls.log"
}

# The host's session list (term-list) of the current world, in TERM_LIST.
# Succeeds only when the host answered. A probe that failed leaves TERM_LIST
# empty and says so by its status: it is never read as "no sessions".
term_list() {
  probe term-list
  TERM_LIST="$OUT"
  [ "$RC" -eq 0 ]
}
session_line() { # id -> its term-list line, if any
  printf '%s\n' "$TERM_LIST" | grep "^session=$1 " || true
}
session_listed() { term_list && [ -n "$(session_line "$1")" ]; }
session_gone() { term_list && [ -z "$(session_line "$1")" ]; }
session_pid() { term_list || true; session_line "$1" | sed -n 's/.* pid=\([0-9][0-9]*\).*/\1/p'; }
session_clients() { term_list || true; session_line "$1" | sed -n 's/.* clients=\([0-9][0-9]*\).*/\1/p'; }
clients_are() { # count id: re-read on every poll
  [ "$(session_clients "$2")" = "$1" ]
}
session_origin() { term_list || true; session_line "$1" | sed -n 's/.* origin=\([a-z]*\).*/\1/p'; }

# The host of the current world has left once its state file is gone.
host_gone() { ! ls "$W"/host/host-v*.json >/dev/null 2>&1; }
host_absent() { probe observe 1; [ "$OUT" = "verdict=Absent" ]; }

# ---------------------------------------------------------------- sirioctl

ctl() { "$CTL" "$@"; }
ctl_json() { "$CTL" "$@" --json; }
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
pane_of() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["pane"])'; }

# Adds the repository at $1 as a project and selects its only worktree. The
# worktree is selected (and given a moment to settle) before any terminal is
# opened in it: the control socket answers "unknown worktree" until then.
select_repo() { # path
  local id
  ctl project add "$1" >/dev/null
  id=$(ctl_json project list | python3 -c '
import json, os, sys
target = os.path.realpath(sys.argv[1])
for project in json.load(sys.stdin):
    for wt in json.loads(project["worktrees"]):
        if os.path.realpath(wt["path"]) == target:
            print(wt["id"])
            raise SystemExit
' "$1")
  [ -n "$id" ] || fail "sirioctl does not list the worktree $1"
  ctl select-workspace --workspace "$id" >/dev/null
  sleep 2
  WT_ID="$id"
}

# An app terminal running CMD, opened the way "new terminal" opens one. Prints
# its pane id (the session id the host lists it under).
open_terminal() { # CMD
  ctl_json surface terminal open --cmd "$1" | pane_of
}

tab_count() { ctl_json surface tabs read | field count; }

# The newest output of a pane, decoded (the visible screen of an app pane).
pane_text() { # pane
  ctl_json panel read "$1" | python3 -c '
import base64, json, sys
row = json.load(sys.stdin)[0]
sys.stdout.write(base64.b64decode(row.get("data", "")).decode("utf-8", "replace"))'
}
pane_state() { # pane key
  ctl_json panel state "$1" | field "$2"
}
pane_known() { ctl_json panel state "$1" >/dev/null 2>&1; }
# Whether a terminal tab shows *Session lost* (surface.terminal.state, debug only).
session_lost_of() { # pane
  ctl_json surface terminal state "$1" 2>/dev/null | field sessionLost
}
lost_flagged() { [ "$(session_lost_of "$1")" = true ]; }
# A pane whose exit the view has reported (any status but running).
not_running() { [ "$(pane_state "$1" exitStatus)" != running ]; }

# The largest beat=N a pane prints; -1 while none is shown.
max_beat() { # pane
  pane_text "$1" 2>/dev/null | python3 -c '
import re, sys
values = [int(m) for m in re.findall(r"beat=(\d+)", sys.stdin.read())]
print(max(values) if values else -1)'
}
beat_above() { # pane, value
  [ "$(max_beat "$1")" -gt "$2" ]
}

# The heartbeat command: records its own pid in FILE, then prints a counter.
heartbeat() { # file
  printf 'echo $$ > %s; i=0; while :; do echo beat=$i; i=$((i + 1)); sleep 0.2; done' "$1"
}
# The pid a heartbeat wrote to its file.
pid_in() { # file
  tr -dc '0-9' <"$1"
}
pid_written() { [ -s "$1" ] && [ -n "$(tr -dc '0-9' <"$1")" ]; }

# ---------------------------------------------------------------- captures

# A capture of the app's own window, found by its _NET_WM_PID, as the forge
# and ely probes take theirs. The frame must not be blank.
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  FRAME_NO=$((FRAME_NO + 1))
  local name
  name=$(printf '%02d-%s' "$FRAME_NO" "$1")
  sleep 2
  local window
  window=$(DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"], capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"], capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $name"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$name.png"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$name.png")
  [ "$colours" -ge 200 ] || fail "$name is blank ($colours colours)"
  echo "FRAME: frames/$name.png ($colours colours)"
}

# ---------------------------------------------------------------- echo

# The window of the current app, by its _NET_WM_PID (the capture's finder).
app_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"], capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"], capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
}

# Keystroke echo through the app's own window, typed the way a user types it
# (XTEST, into the focused window) and read back from the pane's screen. It
# works for any app terminal, LocalLink or host. Prints "mean median" in ms,
# "none none" when no echo showed, and "n/a n/a" where no display is driven.
echo_typed() { # pane samples
  if [ "$OS" != linux ] || [ -z "$DISPLAY_TARGET" ] || [ "$STATE_ONLY" -eq 1 ]; then
    echo "n/a n/a"
    return 0
  fi
  local window
  window=$(app_window)
  if [ -z "$window" ]; then
    echo "none none"
    return 0
  fi
  # Focus the window, then click into its middle: the terminal takes the keys
  # only once the pointer has pressed in it.
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowfocus --sync "$window" >/dev/null 2>&1 || true
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove --window "$window" 400 300 click 1 >/dev/null 2>&1 || true
  sleep 0.5
  PANE="$1" SAMPLES="$2" CTL="$CTL" DISPLAY="$DISPLAY_TARGET" python3 - <<'PY'
import base64, json, os, statistics, subprocess, time
ctl, pane, samples = os.environ["CTL"], os.environ["PANE"], int(os.environ["SAMPLES"])

def screen():
    out = subprocess.run([ctl, "panel", "read", pane, "--json"], capture_output=True, text=True).stdout
    try:
        return base64.b64decode(json.loads(out)[0].get("data", "")).decode("utf-8", "replace")
    except (ValueError, IndexError, KeyError):
        return ""

times = []
for i in range(samples):
    marker = f"echo{i:03d}x"
    started = time.monotonic()
    subprocess.run(["xdotool", "type", "--delay", "0", marker], capture_output=True)
    subprocess.run(["xdotool", "key", "Return"], capture_output=True)
    while time.monotonic() - started < 5:
        if marker in screen():
            times.append((time.monotonic() - started) * 1000)
            break
        time.sleep(0.005)
print("none none" if not times else f"{statistics.mean(times):.1f} {statistics.median(times):.1f}")
PY
}

# ---------------------------------------------------------------- cases

# Survival: the terminal's session outlives the app (MODE kill: kill -9; quit:
# a graceful sirioctl quit). The same pid goes on printing after the relaunch,
# and the host counts the relaunched app as its one client.
survive() { # kill|quit
  local mode="$1" pane pidfile pid before
  select_repo "$W/repo"
  pidfile="$W/pid-$mode.txt"
  pane=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 10 pid_written "$pidfile" || fail "the terminal never wrote its pid"
  pid=$(pid_in "$pidfile")
  wait_until 10 beat_above "$pane" 1 || fail "the terminal never printed beat=2"
  sleep 2
  before=$(max_beat "$pane")
  session_listed "$pane" || fail "the host does not list $pane before the $mode"
  [ "$(session_pid "$pane")" = "$pid" ] || fail "the host lists another pid for $pane"
  capture "$mode-before"
  if [ "$mode" = kill ]; then app_kill9; else app_quit; fi
  pid_alive "$pid" || fail "the session's pid $pid died with the app"
  [ "$(session_pid "$pane")" = "$pid" ] || fail "the host lost $pane across the $mode"
  app_start
  wait_until 30 pane_known "$pane" || fail "the relaunched app does not show $pane"
  wait_until 20 beat_above "$pane" "$before" || fail "$pane did not print past beat=$before after the $mode"
  wait_until 10 [ "$(session_clients "$pane")" = 1 ] || fail "the host counts $(session_clients "$pane") clients, expected 1"
  [ "$(session_pid "$pane")" = "$pid" ] || fail "the pid of $pane changed across the $mode"
  NOTE="pid $pid kept printing from beat $before"
  capture "$mode-after"
}
case_survival_kill() {
  new_world c1
  app_start
  survive kill
}
case_survival_quit() {
  new_world c2
  app_start
  survive quit
}

# Closing the tab ends the session and its pid.
case_close_ends() {
  new_world c3
  app_start
  select_repo "$W/repo"
  local pane pidfile pid count
  pidfile="$W/pid.txt"
  pane=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 10 pid_written "$pidfile" || fail "the terminal never wrote its pid"
  pid=$(pid_in "$pidfile")
  count=$(tab_count)
  ctl surface tabs close "$count" --confirm >/dev/null
  wait_until 3 pid_dead "$pid" || fail "pid $pid is alive 3 s after the tab closed"
  wait_until 3 session_gone "$pane" || fail "the host still lists $pane after the tab closed"
  NOTE="pid $pid gone"
}

# End All ends every session and the host.
case_end_all() {
  new_world c4
  app_start
  select_repo "$W/repo"
  local pid1 pid2
  open_terminal "$(heartbeat "$W/pid-1.txt")" >/dev/null
  open_terminal "$(heartbeat "$W/pid-2.txt")" >/dev/null
  wait_until 10 pid_written "$W/pid-1.txt" || fail "the first terminal never wrote its pid"
  wait_until 10 pid_written "$W/pid-2.txt" || fail "the second terminal never wrote its pid"
  pid1=$(pid_in "$W/pid-1.txt")
  pid2=$(pid_in "$W/pid-2.txt")
  ctl surface host end-all >/dev/null
  wait_until 10 pid_dead "$pid1" || fail "pid $pid1 survived End All"
  wait_until 10 pid_dead "$pid2" || fail "pid $pid2 survived End All"
  # The host answers Unverifiable while it is still leaving; it must be Absent
  # shortly after its last session has gone.
  wait_until 10 host_absent || fail "the host after End All is '$OUT', expected verdict=Absent"
  NOTE="pids $pid1 $pid2 gone; host absent"
}

# With no host binary the terminal still opens, on LocalLink, and says so.
case_fallback() {
  new_world c5
  # A copy of the app with no sibling sirio-host, and no SIRIO_HOST_BIN: the
  # app finds no host to adopt or start, so its terminal falls back.
  mkdir -p "$W/alone"
  cp "$APP_BIN" "$W/alone/sirio"
  APP_EXE="$W/alone/sirio"
  app_start SIRIO_HOST_BIN="$W/missing-sirio-host"
  select_repo "$W/repo"
  local pane pidfile pid
  pidfile="$W/pid.txt"
  pane=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 10 pid_written "$pidfile" || fail "the fallback terminal never wrote its pid"
  pid=$(pid_in "$pidfile")
  wait_until 10 beat_above "$pane" 1 || fail "the fallback terminal does not print"
  [ "$(pane_state "$pane" survives)" = false ] || fail "a LocalLink pane claims to survive the app"
  capture fallback-banner
  export SIRIO_HOST_BIN="$W/missing-sirio-host"
  probe term-list
  export SIRIO_HOST_BIN="$HOST_BIN"
  [ "$RC" -ne 0 ] || fail "term-list reached a host with no host binary and no host running"
  grep -q 'terminal host connection' "$W/probe.err" \
    || fail "term-list failed, but not for want of a host: $(head -c 200 "$W/probe.err")"
  APP_EXE="$APP_BIN"
  # A keystroke echo on the same path, for case 10's comparison.
  local shell_pane
  shell_pane=$(open_terminal "cat")
  wait_until 10 pane_known "$shell_pane" || fail "the cat terminal is unknown"
  sleep 1
  LOCAL_ECHO=$(echo_typed "$shell_pane" 10)
  if [ "$LOCAL_ECHO" = "none none" ]; then
    NOTE="local echo NOT MEASURED (no echo showed); local pid $pid"
  else
    NOTE="local echo mean/median ms: $LOCAL_ECHO; local pid $pid"
  fi
  kill -9 "$pid" 2>/dev/null || true
}

# A host session with no tab (an orphan: origin app, not in the saved layout)
# is adopted by the app on launch: a tab for it appears, attached to the
# session, which keeps its pid. The orphan is made on the host directly, with
# host_probe. Spec §4.1 deliberately shrinks the kill-before-save window: a new
# terminal tab is saved at once, so an app killed between opening a tab and its
# save is rare by design, and killing the app at that instant cannot be made to
# happen reliably. What this case proves is the adoption path (§4.2): a host
# session with no saved tab gets one on launch (Ruling, accepted in review).
case_orphan() {
  new_world c6
  local pane pidfile pid
  pane=pane-7
  pidfile="$W/pid.txt"
  # The project and its worktree are known and selected, as a user's are when
  # the app comes back: the adopted tab lives in that worktree and is drawn.
  app_start
  select_repo "$W/repo"
  app_quit
  probe ensure
  [ "$RC" -eq 0 ] || fail "host_probe ensure failed: $OUT"
  probe term-create "$W/repo" --id "$pane" --origin app -- /bin/sh -c "$(heartbeat "$pidfile")"
  [ "$RC" -eq 0 ] || fail "host_probe term-create failed: $OUT"
  wait_until 10 pid_written "$pidfile" || fail "the orphan session never wrote its pid"
  pid=$(pid_in "$pidfile")
  session_listed "$pane" || fail "the host does not list the orphan $pane"
  [ "$(session_origin "$pane")" = app ] || fail "the orphan $pane is not origin app"
  app_start
  wait_until 30 pane_known "$pane" || fail "the relaunched app did not adopt $pane"
  wait_until 10 [ "$(pane_state "$pane" survives)" = true ] || fail "the adopted $pane is not attached to the host (survives $(pane_state "$pane" survives), clients $(session_clients "$pane"))"
  wait_until 20 beat_above "$pane" 0 || fail "the adopted $pane does not print"
  [ "$(session_pid "$pane")" = "$pid" ] || fail "the adopted $pane is another session"
  wait_until 10 clients_are 1 "$pane" || fail "the adopted $pane has $(session_clients "$pane") clients, expected 1; tabs $(ctl_json surface tabs read); state $(ctl_json panel state "$pane")"
  NOTE="adopted $pane (pid $pid), attached, one client"
  capture orphan-adopted
}

# `panel show` opens one tab for a session, closing it keeps the session, and
# the shown tab is not restored by a relaunch.
case_show_session() {
  new_world c7
  app_start
  select_repo "$W/repo"
  local created id before shown_count pid
  created=$(ctl_json panel create --cmd 'sleep 300' | field id)
  id="$created"
  [ -n "$id" ] || fail "panel create answered no id"
  before=$(tab_count)
  ctl panel show "$id" >/dev/null
  wait_until 5 [ "$(tab_count)" = "$((before + 1))" ] || fail "panel show did not open one tab"
  ctl panel show "$id" >/dev/null
  sleep 1
  [ "$(tab_count)" = "$((before + 1))" ] || fail "a second panel show opened another tab"
  shown_count=$(tab_count)
  pid=$(session_pid "$id")
  [ -n "$pid" ] || fail "the shown session $id is not on the host"
  ctl surface tabs close "$shown_count" --confirm >/dev/null
  wait_until 5 [ "$(tab_count)" = "$before" ] || fail "closing the shown tab did not remove it"
  session_listed "$id" || fail "closing the shown tab ended $id on the host"
  pid_alive "$pid" || fail "closing the shown tab killed pid $pid"
  ctl panel show "$id" >/dev/null
  wait_until 5 [ "$(tab_count)" = "$((before + 1))" ] || fail "the second show did not open a tab"
  app_kill9
  app_start
  wait_until 30 pane_known "$id" || fail "the relaunched app does not know $id"
  [ "$(tab_count)" = "$before" ] || fail "a relaunch restored the shown tab (count $(tab_count), expected $before): $(ctl_json surface tabs read)"
  session_listed "$id" || fail "the relaunch lost $id on the host"
  pid_alive "$pid" || fail "the relaunch killed pid $pid"
  NOTE="one tab per session; shown tab not restored; pid $pid alive"
}

# The host starts late. The restored tab's session is then created on the
# host, and the pane never falls back to LocalLink.
case_slow_host() {
  new_world c8
  app_start
  select_repo "$W/repo"
  local pane pidfile pid delay_ms
  pidfile="$W/pid.txt"
  pane=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 10 pid_written "$pidfile" || fail "the terminal never wrote its pid"
  pid=$(pid_in "$pidfile")
  app_quit
  probe shutdown --force
  wait_until 10 host_gone || fail "the host did not leave"
  wait_until 5 dead "$pid" || fail "pid $pid survived the host's shutdown"
  # The lock of the host that just left is stale: removed, so its mtime below
  # is the new host's own.
  rm -f "$W/host/host-v1.lock"
  export SIRIO_HOST_START_DELAY_MS=3000
  app_start
  for _ in $(seq 1 600); do
    probe info
    [ "$RC" -eq 0 ] && break
    sleep 0.1
  done
  [ "$RC" -eq 0 ] || fail "the delayed host never answered"
  # The host takes its lock, waits the delay, then binds its socket: the two
  # files' modification times are the delay the host actually observed.
  delay_ms=$(python3 -c '
import os, sys
lock, sock = sys.argv[1], sys.argv[2]
print(int((os.stat(sock).st_mtime - os.stat(lock).st_mtime) * 1000))' "$W/host/host-v1.lock" "$W/host/host-v1.sock")
  [ "$delay_ms" -ge 2500 ] || fail "the host bound its socket ${delay_ms} ms after its lock; the 3 s start delay did not apply"
  wait_until 30 pane_known "$pane" || fail "the restored $pane is unknown"
  wait_until 30 session_listed "$pane" || fail "the restored $pane was not created on the host"
  [ "$(session_origin "$pane")" = app ] || fail "the restored $pane is not an app session on the host"
  # Pending is false by design until the session is attached, so the check
  # starts once the host lists the pane: from then on survives must reach
  # true and then stay true for three seconds of samples. A LocalLink fallback
  # at any sample fails the case, not only the end state.
  wait_until 10 [ "$(pane_state "$pane" survives)" = true ] || fail "the restored $pane never reached the host"
  for _ in $(seq 1 30); do
    [ "$(pane_state "$pane" survives)" = true ] || fail "the restored $pane fell back to LocalLink after the host listed it"
    sleep 0.1
  done
  unset SIRIO_HOST_START_DELAY_MS
  NOTE="host bound ${delay_ms} ms after its lock; $pane created on the host with origin=app"
  capture slow-host
}

# Two worlds, two apps, two hosts: each sees only its own sessions.
case_isolation() {
  new_world c9a
  local a_dir b_dir pid_a pid_b pid_file_a pid_file_b a_pane b_pane
  a_dir="$W"
  app_start
  select_repo "$W/repo"
  pid_file_a="$W/pid.txt"
  a_pane=$(open_terminal "$(heartbeat "$pid_file_a")")
  wait_until 10 pid_written "$pid_file_a" || fail "world A never wrote its pid"
  pid_a=$(pid_in "$pid_file_a")
  new_world c9b
  b_dir="$W"
  app_start
  select_repo "$W/repo"
  pid_file_b="$W/pid.txt"
  b_pane=$(open_terminal "$(heartbeat "$pid_file_b")")
  wait_until 10 pid_written "$pid_file_b" || fail "world B never wrote its pid"
  pid_b=$(pid_in "$pid_file_b")
  use_world "$a_dir"
  session_listed "$a_pane" || fail "world A's host does not list its own $a_pane"
  [ "$(session_pid "$a_pane")" = "$pid_a" ] || fail "world A's host lists another pid for $a_pane"
  term_list || fail "world A's host did not answer term-list"
  [ -z "$(printf '%s\n' "$TERM_LIST" | grep "pid=$pid_b ")" ] || fail "world A's host lists world B's pid $pid_b"
  use_world "$b_dir"
  session_listed "$b_pane" || fail "world B's host does not list its own $b_pane"
  term_list || fail "world B's host did not answer term-list"
  [ -z "$(printf '%s\n' "$TERM_LIST" | grep "pid=$pid_a ")" ] || fail "world B's host lists world A's pid $pid_a"
  [ "$(pane_state "$b_pane" survives)" = true ] || fail "world B's $b_pane is not on its own host"
  use_world "$a_dir"
  [ "$(pane_state "$a_pane" survives)" = true ] || fail "world A's $a_pane is not on its own host"
  NOTE="A pid $pid_a only on A's host; B pid $pid_b only on B's"
}

# Keystroke echo through the host, recorded beside the LocalLink figure of
# case 5. Measured only: never a failure.
case_echo_latency() {
  new_world c10
  app_start
  select_repo "$W/repo"
  local pane probe_out
  pane=$(open_terminal "cat")
  wait_until 10 pane_known "$pane" || fail "the cat terminal is unknown"
  sleep 1
  HOST_ECHO=$(echo_typed "$pane" 10)
  probe term-echo-latency "$pane" 10
  probe_out="$OUT"
  echo "echo-latency: host (sirioctl loop, mean median ms) $HOST_ECHO; host_probe term-echo-latency: $(printf '%s' "$probe_out" | tr '\n' ' ')"
  echo "echo-latency: LocalLink (case 5, mean median ms) ${LOCAL_ECHO:-not measured}"
  NOTE="host ${HOST_ECHO} ms, LocalLink ${LOCAL_ECHO:-n/a} ms (mean median; 'none' = NOT MEASURED, no echo showed); recorded, not judged"
}

# Closing a worktree ends the control sessions inside it. A control session in
# a directory outside it survives.
# Finding 7 (spec §8): a host that holds its lock and does not answer is
# Unverifiable, never Absent. A restored tab must not run a local shell beside
# the session, must not report survives=false, and attaches once the host
# answers again.
case_unverifiable_no_fallback() {
  new_world c14
  local pane pidfile pid host_pid i kids
  app_start
  select_repo "$W/repo"
  pidfile="$W/pid.txt"
  pane=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 10 pid_written "$pidfile" || fail "the terminal never wrote its pid"
  pid=$(pid_in "$pidfile")
  app_quit
  host_pid=$(python3 -c 'import glob, json, sys; print(json.load(open(glob.glob(sys.argv[1] + "/host-v*.json")[0]))["pid"])' "$W/host")
  kill -STOP "$host_pid" || fail "could not stop the host $host_pid"
  app_start
  for i in $(seq 1 60); do
    # A local shell is what a fallback would start; other children (the
    # app's own usage requests, say) are not the session's business.
    kids=$(ps -o comm= --ppid "$APP_PID" | grep -cE '^(sh|bash|zsh|fish|dash)$' || true)
    [ "$kids" -eq 0 ] || fail "the app runs $kids local shell(s) while the host is unverifiable"
    [ "$(pane_state "$pane" survives 2>/dev/null)" = false ] && fail "pane $pane reports survives=false while the host is unverifiable"
    sleep 0.5
  done
  kill -CONT "$host_pid" || fail "could not resume the host $host_pid"
  wait_until 60 [ "$(pane_state "$pane" survives 2>/dev/null)" = true ] || fail "the restored tab $pane is not attached after SIGCONT (survives $(pane_state "$pane" survives 2>/dev/null))"
  [ "$(session_pid "$pane")" = "$pid" ] || fail "the restored tab $pane is another session (pid $(session_pid "$pane"), expected $pid)"
  wait_until 20 clients_are 1 "$pane" || fail "the restored tab $pane has $(session_clients "$pane") clients, expected 1"
  NOTE="restored $pane (pid $pid): no local child while unverifiable; attached after SIGCONT"
}

# Finding 4 (End All never touches the socket on the GPUI thread): with the
# host stopped, End All must not stall the app. `surface tabs read` (app-local)
# answers within a second on every sample while the shutdown is in flight; the
# host is continued inside the call's own 5 s window, and End All then completes.
case_end_all_host_stopped() {
  new_world c16
  app_start
  select_repo "$W/repo"
  local pid1 host_pid i took worst
  open_terminal "$(heartbeat "$W/pid-1.txt")" >/dev/null
  wait_until 10 pid_written "$W/pid-1.txt" || fail "the terminal never wrote its pid"
  pid1=$(pid_in "$W/pid-1.txt")
  host_pid=$(python3 -c 'import glob, json, sys; print(json.load(open(glob.glob(sys.argv[1] + "/host-v*.json")[0]))["pid"])' "$W/host")
  STOPPED+=("$host_pid")
  kill -STOP "$host_pid" || fail "could not stop the host $host_pid"
  local end_out
  end_out=$("$CTL" surface host end-all 2>&1) || fail "surface host end-all was refused while the host is stopped: $end_out"
  worst=0
  for i in $(seq 1 8); do
    local start
    start=$(date +%s%N)
    ctl_json surface tabs read >/dev/null 2>&1 || fail "surface tabs read failed while End All waits on the host"
    took=$(( ($(date +%s%N) - start) / 1000000 ))
    [ "$took" -gt "$worst" ] && worst=$took
    sleep 0.25
  done
  kill -CONT "$host_pid" || fail "could not resume the host $host_pid"
  STOPPED=()
  [ "$worst" -lt 1000 ] || fail "the app took ${worst} ms to answer surface tabs read while End All waited on a stopped host (expected under 1000 ms)"
  wait_until 10 pid_dead "$pid1" || fail "pid $pid1 survived End All after the host was resumed"
  wait_until 10 host_absent || fail "the host after End All is '$OUT', expected verdict=Absent"
  NOTE="worst surface tabs read while End All waited on the stopped host ${worst} ms; pid $pid1 gone after SIGCONT; host absent"
}

# Finding 2, app side (one slow host request must not stall every terminal):
# `sirioctl panel wait` blocked through the app on a control session must not
# stall a tab's keystrokes. The tab's program appends each line it reads to a
# file, and the echo is timed on that file: panel.* is forwarded to the host,
# the very path under test, so the screen cannot be the witness.
case_panel_wait_does_not_stall() {
  new_world c17
  app_start
  select_repo "$W/repo"
  if [ "$STATE_ONLY" -eq 1 ] || [ -z "$DISPLAY_TARGET" ]; then
    NOTE="not measured: keystrokes need the display (--xvfb)"
    return 0
  fi
  local sink="$W/typed.txt" sinkscript="$W/sink.sh" window pane i start took worst
  printf '%s\n' "while IFS= read -r line; do printf '%s\\n' \"\$line\" >> $sink; done" >"$sinkscript"
  pane=$(open_terminal "sh $sinkscript")
  wait_until 10 pane_known "$pane" || fail "the tab $pane is unknown"
  probe term-create "$W/repo" --id pane-9001 --origin control -- sleep 300
  "$CTL" panel wait pane-9001 >"$W/wait.out" 2>&1 &
  local wait_pid=$!
  APPS+=("$wait_pid")
  sleep 1
  alive "$wait_pid" || fail "sirioctl panel wait exited before the echo was measured: $(cat "$W/wait.out")"
  sleep 1
  window=$(app_window)
  [ -n "$window" ] || fail "no window to type into"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowfocus --sync "$window" >/dev/null 2>&1 || true
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove --window "$window" 400 300 click 1 >/dev/null 2>&1 || true
  sleep 0.5
  worst=0
  for i in 1 2 3; do
    start=$(date +%s%N)
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool type --delay 0 "probe-$i" >/dev/null 2>&1
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool key Return >/dev/null 2>&1
    wait_until 5 grep -qx "probe-$i" "$sink" 2>/dev/null \
      || fail "typed probe-$i did not reach the tab's program within 5 s while sirioctl panel wait was pending (echo arrived: $(cat "$sink" 2>/dev/null || true))"
    took=$(( ($(date +%s%N) - start) / 1000000 ))
    [ "$took" -gt "$worst" ] && worst=$took
  done
  alive "$wait_pid" || fail "sirioctl panel wait ended during the measurement, so the echo proves nothing"
  [ "$worst" -le 2000 ] || fail "the slowest keystroke echo took ${worst} ms while sirioctl panel wait was pending (limit 2000 ms)"
  kill "$wait_pid" 2>/dev/null || true
  NOTE="worst keystroke echo ${worst} ms with sirioctl panel wait pending on a control session"
}

# Finding 5 (the owner thread never waits on the host for resize or feed): with
# the host stopped, a window resize of the visible terminal must leave the app
# answering. The app-local verb `surface tabs read` is timed, never a panel verb
# (panel.* is forwarded to the host). After SIGCONT the host-side session must
# have taken the new size.
case_stopped_host_resize() {
  new_world c15
  app_start
  select_repo "$W/repo"
  local pane host_pid window i start took worst before after geometry_before geometry_after
  if [ "$STATE_ONLY" -eq 1 ] || [ -z "$DISPLAY_TARGET" ]; then
    NOTE="not measured: a window resize needs the display (--xvfb)"
    return 0
  fi
  pane=$(open_terminal "while :; do stty size; sleep 0.5; done")
  wait_until 10 pane_known "$pane" || fail "the terminal $pane is unknown"
  host_pid=$(python3 -c 'import glob, json, sys; print(json.load(open(glob.glob(sys.argv[1] + "/host-v*.json")[0]))["pid"])' "$W/host")
  sleep 1
  window=$(app_window)
  [ -n "$window" ] || fail "no window for the app to resize"
  before=$(stty_last "$pane")
  geometry_before=$(DISPLAY="$DISPLAY_TARGET" xwininfo -id "$window" | grep -E 'Width|Height' | tr -d ' \n')
  STOPPED+=("$host_pid")
  kill -STOP "$host_pid" || fail "could not stop the host $host_pid"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowsize "$window" 1100 700 >/dev/null 2>&1 || fail "could not resize the window"
  sleep 1
  geometry_after=$(DISPLAY="$DISPLAY_TARGET" xwininfo -id "$window" | grep -E 'Width|Height' | tr -d ' \n')
  [ "$geometry_before" != "$geometry_after" ] || fail "the window did not change size ($geometry_before)"
  worst=0
  for i in $(seq 1 8); do
    start=$(date +%s%N)
    ctl_json surface tabs read >/dev/null 2>&1 || fail "surface tabs read failed while the host is stopped"
    took=$(( ($(date +%s%N) - start) / 1000000 ))
    [ "$took" -gt "$worst" ] && worst=$took
    sleep 0.25
  done
  kill -CONT "$host_pid" || fail "could not resume the host $host_pid"
  STOPPED=()
  [ "$worst" -lt 1000 ] || fail "the app took ${worst} ms to answer surface tabs read while its terminal's host was stopped (expected under 1000 ms); the owner thread waited on the host for a resize"
  wait_until 30 bool_stty_changed "$pane" "$before" || fail "the host-side session $pane never took the new size after SIGCONT (was '$before', still '$(stty_last "$pane")')"
  NOTE="worst surface tabs read while the host was stopped ${worst} ms; host-side size $before -> $(stty_last "$pane")"
}
stty_last() { # pane -> the newest 'rows cols' line in the host's screen
  probe term-read "$1" text --max 4096 || true
  printf '%s' "$OUT" | grep -oE '^[0-9]+ [0-9]+$' | tail -1
}
bool_stty_changed() { # pane before
  local now
  now=$(stty_last "$1")
  [ -n "$now" ] && [ "$now" != "$2" ]
}

# The id of the worktree at path $1 in the project catalog.
worktree_id_at() { # path
  ctl_json project list | python3 -c '
import json, os, sys
target = os.path.realpath(sys.argv[1])
for project in json.load(sys.stdin):
    for wt in json.loads(project["worktrees"]):
        if os.path.realpath(wt["path"]) == target:
            print(wt["id"])
            raise SystemExit
' "$1"
}

# Finding 1: every worktree's persisted terminals are referenced. A terminal in
# each of two worktrees survives kill -9; the relaunch (active: A) adds no tab
# to A's strip, and selecting B attaches B's tab to B's own session.
case_adopt_two_worktrees() {
  new_world c13
  local wt_b wt_a_id wt_b_id pb pa pa_pid pb_pid count_before
  git -C "$W/repo" worktree add -q "$W/wt-b" -b feature-b
  app_start
  select_repo "$W/repo"
  wt_a_id="$WT_ID"
  ctl project add "$W/wt-b" >/dev/null 2>&1 || true
  wt_b_id=$(worktree_id_at "$W/wt-b")
  [ -n "$wt_b_id" ] || fail "sirioctl does not list the second worktree"
  ctl select-workspace --workspace "$wt_b_id" >/dev/null
  sleep 2
  pb=$(open_terminal "sleep 300")
  wait_until 10 session_listed "$pb" || fail "the terminal in B ($pb) is not on the host"
  pb_pid=$(session_pid "$pb")
  ctl select-workspace --workspace "$wt_a_id" >/dev/null
  sleep 2
  pa=$(open_terminal "sleep 300")
  wait_until 10 session_listed "$pa" || fail "the terminal in A ($pa) is not on the host"
  pa_pid=$(session_pid "$pa")
  count_before=$(tab_count)
  app_kill9
  app_start
  wait_until 30 pane_known "$pa" || fail "the relaunched app does not know $pa"
  sleep 3
  [ "$(tab_count)" = "$count_before" ] || fail "the relaunch added a tab to the active worktree (count $(tab_count), expected $count_before): $(ctl_json surface tabs read)"
  pid_alive "$pa_pid" || fail "pid $pa_pid of $pa is not alive after the relaunch"
  pid_alive "$pb_pid" || fail "pid $pb_pid of $pb is not alive after the relaunch"
  ctl select-workspace --workspace "$wt_b_id" >/dev/null
  sleep 2
  wait_until 10 [ "$(pane_state "$pb" survives)" = true ] || fail "B's tab $pb is not attached to its session after select-workspace B"
  [ "$(session_pid "$pb")" = "$pb_pid" ] || fail "B's session $pb is another session (pid $(session_pid "$pb"), expected $pb_pid)"
  NOTE="A strip unchanged at $count_before; $pa (pid $pa_pid) and $pb (pid $pb_pid) alive; B attached"
}

# A fresh terminal never attaches to a session that already holds its id: the
# control session holding the next app id stays untouched, with no client and
# its own pid, and the new terminal is not on that session.
case_duplicate_id() {
  new_world c12
  app_start
  select_repo "$W/repo"
  local first next pidfile control_pid control_clients dup
  first=$(open_terminal "sleep 300")
  [ -n "$first" ] || fail "the first terminal has no pane id"
  next="pane-$(( ${first#pane-} + 1 ))"
  probe term-create "$W/repo" --id "$next" -- sleep 300
  [ "$RC" -eq 0 ] || fail "host_probe term-create $next failed: $OUT"
  wait_until 10 session_listed "$next" || fail "the control session $next is not listed"
  control_pid=$(session_pid "$next")
  control_clients=$(session_clients "$next")
  pidfile="$W/pid.txt"
  dup=$(open_terminal "$(heartbeat "$pidfile")")
  [ "$dup" = "$next" ] || fail "the new terminal got $dup, not the held id $next: the case proves nothing"
  sleep 2
  session_listed "$next" || fail "the control session $next was ended"
  [ "$(session_clients "$next")" = "$control_clients" ] || fail "the new terminal attached to the control session $next (clients $(session_clients "$next"), expected $control_clients)"
  [ "$(session_pid "$next")" = "$control_pid" ] || fail "the held id $next now names another session"
  pid_alive "$control_pid" || fail "the control session pid $control_pid is not alive"
  # The duplicate open fails and shows why; it runs no command at all, on the
  # held session or on a local shell (finding 7: no fallback for a host open).
  ! pid_written "$pidfile" || fail "the new terminal ran its command beside the control session"
  [ "$(session_origin "$next")" = control ] || fail "the control session $next changed origin"
  NOTE="held $next (pid $control_pid, clients $control_clients) untouched; the new terminal failed, ran nothing"
}

case_close_worktree() {
  new_world c11
  app_start
  mkdir -p "$W/other/sub" "$W/outside"
  git -C "$W/other" init -q -b main
  git -C "$W/other" -c user.email=t@example.com -c user.name=Tester commit -q --allow-empty -m first
  select_repo "$W/other"
  local other_wt
  other_wt="$WT_ID"
  local inside outside pid_in_session pid_out
  inside=$(ctl_json panel create --cwd "$W/other/sub" --cmd 'sleep 300' | field id)
  outside=$(ctl_json panel create --cwd "$W/outside" --cmd 'sleep 300' | field id)
  pid_in_session=$(session_pid "$inside")
  pid_out=$(session_pid "$outside")
  [ -n "$pid_in_session" ] && [ -n "$pid_out" ] || fail "the control sessions are not on the host"
  ctl close-workspace --workspace "$other_wt" >/dev/null
  wait_until 10 session_gone "$inside" || fail "the closed worktree's session $inside is still listed"
  wait_until 10 pid_dead "$pid_in_session" || fail "pid $pid_in_session survived the worktree's close"
  session_listed "$outside" || fail "a session outside the closed worktree was ended"
  pid_alive "$pid_out" || fail "pid $pid_out outside the worktree died"
  NOTE="pid $pid_in_session ended with its worktree; pid $pid_out outside survives"
}

# The pid the current world's host recorded in its state file.
current_host_pid() {
  python3 -c 'import glob, json, sys; print(json.load(open(glob.glob(sys.argv[1] + "/host-v*.json")[0]))["pid"])' "$W/host"
}

# Finding 6 (recovery after End All): End All ends the host by design. The tab
# of a session it closed shows that session's exit, and the next terminal
# starts a new host and runs on it (spec §3.1, §4.2 row 7). The check is a
# working session on the new host: it lists the pane under the new pid, the
# pid prints, and the host the app uses is not the old one.
case_recover_after_end_all() {
  new_world c18
  app_start
  select_repo "$W/repo"
  local old_pane pidfile old_pid old_host pane pidfile2 new_pid new_host
  pidfile="$W/pid-1.txt"
  old_pane=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 10 pid_written "$pidfile" || fail "the first terminal never wrote its pid"
  wait_until 10 beat_above "$old_pane" 1 || fail "the first terminal does not print"
  old_pid=$(pid_in "$pidfile")
  old_host=$(current_host_pid)
  ctl surface host end-all >/dev/null
  wait_until 10 pid_dead "$old_pid" || fail "pid $old_pid survived End All"
  wait_until 10 host_absent || fail "the host after End All is '$OUT', expected verdict=Absent"
  wait_until 10 not_running "$old_pane" \
    || fail "the tab of the session End All closed still reads running (exitStatus $(pane_state "$old_pane" exitStatus))"
  pidfile2="$W/pid-2.txt"
  pane=$(open_terminal "$(heartbeat "$pidfile2")")
  wait_until 30 pid_written "$pidfile2" || fail "the terminal opened after End All never wrote its pid"
  new_pid=$(pid_in "$pidfile2")
  wait_until 10 beat_above "$pane" 1 || fail "the terminal opened after End All does not print"
  new_host=$(current_host_pid)
  { [ "$new_host" != "$old_host" ] && alive "$new_host"; } \
    || fail "the host is $new_host after End All, expected a new live host (the old one was $old_host)"
  wait_until 30 session_listed "$pane" || fail "the new host does not list $pane"
  [ "$(session_pid "$pane")" = "$new_pid" ] || fail "the new host lists pid $(session_pid "$pane") for $pane, expected $new_pid"
  wait_until 10 clients_are 1 "$pane" || fail "the new host counts $(session_clients "$pane") clients for $pane, expected 1"
  [ "$(pane_state "$pane" survives)" = true ] || fail "$pane does not report survives=true"
  NOTE="old tab exit $(pane_state "$old_pane" exitStatus); $pane (pid $new_pid) on new host $new_host, was $old_host"
}

# Finding 6 (recovery after host death): a host killed with kill -9 leaves its
# tab Session lost. A new terminal starts a host and runs on it, and Restart on
# the lost tab goes through that host too (spec §4.2 rows 6 and 7).
case_recover_after_host_death() {
  new_world c19
  app_start
  select_repo "$W/repo"
  local lost pidfile old_host pane2 pidfile2 pid2 new_host restarted_pid
  pidfile="$W/pid-1.txt"
  lost=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 10 pid_written "$pidfile" || fail "the terminal never wrote its pid"
  wait_until 10 beat_above "$lost" 1 || fail "the terminal does not print"
  old_host=$(current_host_pid)
  kill -9 "$old_host" || fail "could not kill the host $old_host"
  wait_until 10 dead "$old_host" || fail "the host $old_host did not die"
  wait_until 30 lost_flagged "$lost" \
    || fail "the tab $lost does not show Session lost after its host died (sessionLost $(session_lost_of "$lost"))"
  pidfile2="$W/pid-2.txt"
  pane2=$(open_terminal "$(heartbeat "$pidfile2")")
  wait_until 30 pid_written "$pidfile2" || fail "the terminal opened after the host died never wrote its pid"
  pid2=$(pid_in "$pidfile2")
  wait_until 10 beat_above "$pane2" 1 || fail "the terminal opened after the host died does not print"
  new_host=$(current_host_pid)
  { [ "$new_host" != "$old_host" ] && alive "$new_host"; } \
    || fail "the host is $new_host after the death, expected a new live host (the old one was $old_host)"
  wait_until 30 session_listed "$pane2" || fail "the new host does not list $pane2"
  [ "$(session_pid "$pane2")" = "$pid2" ] || fail "the new host lists pid $(session_pid "$pane2") for $pane2, expected $pid2"
  wait_until 10 clients_are 1 "$pane2" || fail "the new host counts $(session_clients "$pane2") clients for $pane2, expected 1"
  # Restart on the lost tab: the same command runs again, on the new host.
  rm -f "$pidfile"
  ctl surface terminal restart "$lost" >/dev/null || fail "surface terminal restart $lost was refused"
  wait_until 30 pid_written "$pidfile" || fail "Restart did not start the lost tab's terminal"
  restarted_pid=$(pid_in "$pidfile")
  wait_until 30 session_listed "$lost" || fail "the new host does not list the restarted $lost"
  [ "$(session_pid "$lost")" = "$restarted_pid" ] || fail "the new host lists pid $(session_pid "$lost") for $lost, expected $restarted_pid"
  wait_until 10 clients_are 1 "$lost" || fail "the new host counts $(session_clients "$lost") clients for $lost, expected 1"
  [ "$(pane_state "$lost" survives)" = true ] || fail "the restarted $lost does not report survives=true"
  NOTE="host $old_host died; $pane2 (pid $pid2) and restarted $lost (pid $restarted_pid) on new host $new_host"
}

# Finding 12 (closing a tab while it connects): the host is stopped before the
# tab opens, so the open waits in its first host call. The tab is closed in that
# wait, and the host is continued inside the open's own window. The closed
# tab's session must never be left listed, and nothing may stay attached to it.
case_close_connecting() {
  new_world c20
  app_start
  select_repo "$W/repo"
  local host_pid pidfile pane count
  host_pid=$(current_host_pid)
  pidfile="$W/pid.txt"
  kill -STOP "$host_pid" || fail "could not stop the host $host_pid"
  STOPPED+=("$host_pid")
  pane=$(open_terminal "$(heartbeat "$pidfile")")
  sleep 1
  count=$(tab_count)
  ctl surface tabs close "$count" --confirm >/dev/null || fail "closing the connecting tab was refused"
  sleep 1
  kill -CONT "$host_pid" || fail "could not resume the host $host_pid"
  STOPPED=()
  sleep 2
  if [ -s "$pidfile" ]; then
    local pid
    pid=$(pid_in "$pidfile")
    wait_until 5 dead "$pid" || fail "the session of the closed tab $pane ran (pid $pid is alive)"
  fi
  wait_until 10 session_gone "$pane" \
    || fail "the closed tab's session $pane is still on the host (clients $(session_clients "$pane"))"
  NOTE="$pane closed while connecting: not listed on the host after the host was resumed"
}

# ---------------------------------------------------------------- the run

run_case() { # number name function
  N="$1"; CASE_NAME="$2"; CASE_STARTED=$(date +%s); NOTE=""
  if [ -n "$ONLY" ] && [ "${ONLY#*,$N,}" = "$ONLY" ]; then return 0; fi
  "$3"
  end_case
  pass
}

# Ends everything a case started: the apps it launched, then the hosts of its
# worlds (shut down through the probe; a stuck one is killed by pid, and only
# if it is a sirio-host). Every log is archived under OUT_DIR/worlds/<world>.
end_case() {
  local pid w name f hostpid major
  for pid in ${STOPPED[@]+"${STOPPED[@]}"}; do
    kill -CONT "$pid" 2>/dev/null || true
  done
  STOPPED=()
  for pid in ${APPS[@]+"${APPS[@]}"}; do
    if alive "$pid"; then
      kill "$pid" 2>/dev/null || true
      wait_until 10 dead "$pid" || kill -9 "$pid" 2>/dev/null || true
    fi
  done
  APPS=()
  for w in ${WORLDS[@]+"${WORLDS[@]}"}; do
    name="$(basename "$w")"
    for f in "$w"/host/host-v*.json; do
      [ -f "$f" ] || continue
      major="${f##*host-v}"
      major="${major%.json}"
      hostpid="$(sed -n 's/.*"pid": *\([0-9][0-9]*\).*/\1/p' "$f" | head -n 1)"
      if [ -n "$hostpid" ] && alive "$hostpid" && is_a_host "$hostpid"; then
        kill -CONT "$hostpid" 2>/dev/null || true
        SIRIO_HOST_HOME="$w/host" SIRIO_HOST_PROTOCOL_MAJOR="$major" "$PROBE" shutdown --force >/dev/null 2>&1 || true
        wait_until 5 dead "$hostpid" || kill -9 "$hostpid" 2>/dev/null || true
      fi
    done
    mkdir -p "$OUT_DIR/worlds/$name"
    cp "$w"/app*.log "$OUT_DIR/worlds/$name/" 2>/dev/null || true
    cp "$w"/*.txt "$OUT_DIR/worlds/$name/" 2>/dev/null || true
    [ -d "$w/host/log" ] && cp -R "$w/host/log" "$OUT_DIR/worlds/$name/" 2>/dev/null || true
    cp "$w"/host/host-v*.json "$OUT_DIR/worlds/$name/" 2>/dev/null || true
  done
  WORLDS=()
}

cleanup() {
  local rc=$?
  end_case 2>/dev/null || true
  [ -z "$XVFB_PID" ] || kill "$XVFB_PID" 2>/dev/null || true
  [ -z "$RUN" ] || rm -rf "$RUN"
  exit "$rc"
}
trap cleanup EXIT

[ "$NO_BUILD" -eq 1 ] || {
  echo "building sirio, sirioctl, sirio-host and host_probe"
  (cd "$REPO_ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
    -p sirio_host --bin sirio-host -p sirio_host_client --example host_probe)
}
for binary in "$APP_BIN" "$CTL" "$HOST_BIN" "$PROBE"; do
  [ -x "$binary" ] || { echo "missing binary: $binary" >&2; exit 1; }
done

# A display: a private Xvfb on a free number (lavapipe when present, so the
# window draws), or the one given.
VULKAN_ENV=()
if [ "$STATE_ONLY" -eq 0 ] && [ "$OS" = linux ]; then
  if [ "$START_XVFB" -eq 1 ]; then
    command -v Xvfb >/dev/null || { echo "Xvfb is required for --xvfb" >&2; exit 2; }
    for number in $(seq 90 99); do
      [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
    done
    [ -n "$DISPLAY_TARGET" ] || { echo "no free display between :90 and :99" >&2; exit 2; }
    Xvfb "$DISPLAY_TARGET" -screen 0 1280x800x24 >"$OUT_DIR/xvfb.log" 2>&1 &
    XVFB_PID=$!
    for _ in $(seq 1 50); do timeout 2 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 && break; sleep 0.1; done
    timeout 2 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 || { echo "Xvfb did not start on $DISPLAY_TARGET" >&2; exit 2; }
  fi
  [ -n "$DISPLAY_TARGET" ] || { echo "no display; pass --xvfb or --display :N (or --state-only)" >&2; exit 2; }
  lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
  if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi
  echo "display: $DISPLAY_TARGET"
fi

RUN="$(mktemp -d "${TMPDIR:-/tmp}/hs-XXXXXX")"
echo "run root: $RUN"
START_ALL=$(date +%s)

run_case 1 survival-kill case_survival_kill
run_case 2 survival-quit case_survival_quit
run_case 3 close-ends case_close_ends
run_case 4 end-all case_end_all
run_case 5 fallback case_fallback
run_case 6 orphan case_orphan
run_case 7 show-session case_show_session
run_case 8 slow-host case_slow_host
run_case 9 isolation case_isolation
run_case 10 echo-latency case_echo_latency
run_case 11 close-worktree case_close_worktree
run_case 12 duplicate-id case_duplicate_id
run_case 13 adopt-two-worktrees case_adopt_two_worktrees
run_case 14 unverifiable-no-fallback case_unverifiable_no_fallback
run_case 15 stopped-host-resize case_stopped_host_resize
run_case 16 end-all-host-stopped case_end_all_host_stopped
run_case 17 panel-wait-no-stall case_panel_wait_does_not_stall
run_case 18 recover-after-end-all case_recover_after_end_all
run_case 19 recover-after-host-death case_recover_after_host_death
run_case 20 close-connecting case_close_connecting

if [ -n "$ONLY" ]; then
  echo "HOST SESSIONS E2E PARTIAL ($CASES_PASSED of 20 cases)"
else
  echo "HOST SESSIONS E2E OK ($CASES_PASSED cases, $(( $(date +%s) - START_ALL ))s)"
fi
