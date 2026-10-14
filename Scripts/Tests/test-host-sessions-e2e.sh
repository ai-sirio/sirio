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
#   23. close-unvisited-worktree  closing a worktree this run never mounted ends its
#                       session on the host; the other worktree's session lives (finding 8)
#   31. agent-rebind-reports-under-its-id  a new agent tab takes a fresh id when its id is
#                       held: its codex notify names that fresh id, not the held one (I1)
#   32. restored-agent-rebind-resumes-under-its-id  a restored agent whose id is held by
#                       another worktree resumes its saved session under the fresh id (I1)
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
SKIPPED=0
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
survives_is() { # pane value: re-read on every poll
  [ "$(pane_state "$1" survives 2>/dev/null)" = "$2" ]
}
tab_count_is() { # count: re-read on every poll
  [ "$(tab_count 2>/dev/null)" = "$1" ]
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
  wait_until 10 clients_are 1 "$pane" || fail "the host counts $(session_clients "$pane") clients, expected 1"
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
  wait_until 10 survives_is "$pane" true || fail "the adopted $pane is not attached to the host (survives $(pane_state "$pane" survives), clients $(session_clients "$pane"))"
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
  wait_until 5 tab_count_is "$((before + 1))" || fail "panel show did not open one tab"
  ctl panel show "$id" >/dev/null
  sleep 1
  [ "$(tab_count)" = "$((before + 1))" ] || fail "a second panel show opened another tab"
  shown_count=$(tab_count)
  pid=$(session_pid "$id")
  [ -n "$pid" ] || fail "the shown session $id is not on the host"
  ctl surface tabs close "$shown_count" --confirm >/dev/null
  wait_until 5 tab_count_is "$before" || fail "closing the shown tab did not remove it"
  session_listed "$id" || fail "closing the shown tab ended $id on the host"
  pid_alive "$pid" || fail "closing the shown tab killed pid $pid"
  ctl panel show "$id" >/dev/null
  wait_until 5 tab_count_is "$((before + 1))" || fail "the second show did not open a tab"
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
  wait_until 10 survives_is "$pane" true || fail "the restored $pane never reached the host"
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
  ! printf '%s\n' "$TERM_LIST" | grep -q "pid=$pid_b " || fail "world A's host lists world B's pid $pid_b"
  use_world "$b_dir"
  session_listed "$b_pane" || fail "world B's host does not list its own $b_pane"
  term_list || fail "world B's host did not answer term-list"
  ! printf '%s\n' "$TERM_LIST" | grep -q "pid=$pid_a " || fail "world B's host lists world A's pid $pid_a"
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
# The shells among PID's children that lead their own process group, as a
# fallback PTY's shell does; comma-separated pids, empty when none. pgrep -P and
# ps -o pid=,pgid=,comm= read the same on Linux and macOS.
fallback_shells() {
  local child pid pgid comm found=""
  for child in $(pgrep -P "$1" || true); do
    read -r pid pgid comm < <(ps -o pid=,pgid=,comm= -p "$child" 2>/dev/null || true) || continue
    case "${comm##*/}" in sh|bash|zsh|fish|dash|-sh|-bash|-zsh) ;; *) continue ;; esac
    [ "$pid" = "$pgid" ] && found="${found:+$found,}$pid"
  done
  printf '%s' "$found"
}

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
    # A local shell is what a fallback would start: a PTY's shell leads its own
    # session, so it is its own process-group leader. The app's other shell
    # children (the tab summarizer, a usage request) stay in the app's group.
    kids=$(fallback_shells "$APP_PID")
    [ -z "$kids" ] || fail "the app runs local shell(s) while the host is unverifiable: $(ps -o pid,pgid,tty,args -p "$kids" 2>/dev/null | tr '\n' ' ')"
    ! grep -q 'terminal on the in-process fallback' "$W/app.log" \
      || fail "the app logged an in-process fallback while the host is unverifiable"
    [ "$(pane_state "$pane" survives 2>/dev/null)" = false ] && fail "pane $pane reports survives=false while the host is unverifiable"
    sleep 0.5
  done
  kill -CONT "$host_pid" || fail "could not resume the host $host_pid"
  wait_until 60 survives_is "$pane" true || fail "the restored tab $pane is not attached after SIGCONT (survives $(pane_state "$pane" survives 2>/dev/null))"
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
  # Unmounting B (selecting A) detaches its terminal and ends nothing: B's
  # session is still listed and its pid still runs.
  session_listed "$pb" || fail "B's session $pb is no longer listed after B was unmounted"
  pid_alive "$pb_pid" || fail "pid $pb_pid of B's session $pb died when B was unmounted"
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
  wait_until 10 survives_is "$pb" true || fail "B's tab $pb is not attached to its session after select-workspace B"
  [ "$(session_pid "$pb")" = "$pb_pid" ] || fail "B's session $pb is another session (pid $(session_pid "$pb"), expected $pb_pid)"
  NOTE="A strip unchanged at $count_before; $pa (pid $pa_pid) and $pb (pid $pb_pid) alive; B attached"
}

# A fresh terminal never attaches to a session that already holds its id: the
# control session holding the next app id stays untouched, with no client and
# its own pid. The new terminal moves to a fresh id and runs on its own session
# (fix batch 5, N5 and #11: a refused id is never a dead end).
case_duplicate_id() {
  new_world c12
  app_start
  select_repo "$W/repo"
  local first next pidfile control_pid control_clients pid fresh
  first=$(open_terminal "sleep 300")
  [ -n "$first" ] || fail "the first terminal has no pane id"
  next="pane-$(( ${first#pane-} + 1 ))"
  probe term-create "$W/repo" --id "$next" -- sleep 300
  [ "$RC" -eq 0 ] || fail "host_probe term-create $next failed: $OUT"
  wait_until 10 session_listed "$next" || fail "the control session $next is not listed"
  control_pid=$(session_pid "$next")
  control_clients=$(session_clients "$next")
  pidfile="$W/pid.txt"
  open_terminal "$(heartbeat "$pidfile")" >/dev/null
  wait_until 30 pid_written "$pidfile" || fail "the new terminal (held id $next) never ran its command"
  pid=$(pid_in "$pidfile")
  term_list || true
  fresh=$(printf '%s\n' "$TERM_LIST" | grep -E " pid=$pid( |\$)" | sed -n 's/^session=\([^ ]*\) .*/\1/p' | head -1)
  [ -n "$fresh" ] && [ "$fresh" != "$next" ] || fail "the new terminal (pid $pid) did not get its own session (held $next, listed as ${fresh:-nothing})"
  session_listed "$next" || fail "the control session $next was ended"
  [ "$(session_clients "$next")" = "$control_clients" ] || fail "the new terminal attached to the control session $next (clients $(session_clients "$next"), expected $control_clients)"
  [ "$(session_pid "$next")" = "$control_pid" ] || fail "the held id $next now names another session"
  pid_alive "$control_pid" || fail "the control session pid $control_pid is not alive"
  [ "$(session_origin "$next")" = control ] || fail "the control session $next changed origin"
  NOTE="held $next (pid $control_pid, clients $control_clients) untouched; the new terminal took $fresh (pid $pid)"
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

# Whether a terminal's text (its screen or its replica) contains MARKER.
shows_text() { # pane marker
  pane_text "$1" 2>/dev/null | grep -q -- "$2"
}
# Whether the host lists PANE as exited with EXIT (exit=code:3 and the like).
session_exited_with() { # pane label
  term_list && session_line "$1" | grep -q -- "exit=$2"
}

# Finding 9 (attaching to an exited session): a session that exited while no
# client was attached shows its exit when its tab is restored, with the last
# screen above it (spec §4.2, row "Session exits while no client is attached").
case_exited_attach() {
  new_world c21
  app_start
  select_repo "$W/repo"
  local pane release="$W/release"
  pane=$(open_terminal "echo done-marker; while [ ! -f $release ]; do sleep 0.2; done; exit 3")
  wait_until 10 shows_text "$pane" done-marker || fail "the terminal never printed its marker"
  app_quit
  touch "$release"
  wait_until 10 session_exited_with "$pane" "code:3" \
    || fail "the host does not list $pane as exited with code 3 (line: $(session_line "$pane"))"
  app_start
  wait_until 30 pane_known "$pane" || fail "the relaunched app does not show $pane"
  wait_until 10 not_running "$pane" \
    || fail "the restored $pane reads running, though its session exited while the app was gone (exitStatus $(pane_state "$pane" exitStatus))"
  [ "$(pane_state "$pane" exitStatus)" = "code:3" ] \
    || fail "the restored $pane reads exitStatus $(pane_state "$pane" exitStatus), expected code:3"
  pane_text "$pane" | grep -q done-marker || fail "the restored $pane does not show its last screen"
  NOTE="$pane exited with code 3 while the app was gone; restored tab shows exitStatus code:3 and its last screen"
}
# Finding 3 (an SP1 host is replaced, never held): v0.31.0 shipped the SP1 host,
# which has no `terminal` capability. An app that finds one with no session
# asks it to leave (non-force), starts the packaged host, and runs its terminal
# on that. The old host's pid is gone; the new host lists the session. Gated on
# SIRIO_SP1_HOST_BIN (a sirio-host built from tag v0.31.0); skipped without it.
case_sp1_host_upgrade() {
  if [ -z "${SIRIO_SP1_HOST_BIN:-}" ]; then
    SKIPPED=1
    NOTE="SKIP: SIRIO_SP1_HOST_BIN is not set (a sirio-host built from tag v0.31.0)"
    return 0
  fi
  [ -x "$SIRIO_SP1_HOST_BIN" ] || fail "SIRIO_SP1_HOST_BIN is not executable: $SIRIO_SP1_HOST_BIN"
  new_world c22
  mkdir -p "$W/host"
  local old_host pidfile pane pid new_host
  # The old host, started as an SP1 app would leave it: on-demand, detached
  # from this shell, on this world's root.
  SIRIO_HOST_HOME="$W/host" setsid "$SIRIO_SP1_HOST_BIN" --mode on-demand >"$W/sp1-host.log" 2>&1 </dev/null &
  wait_until 10 test -s "$W/host/host-v1.json" || fail "the SP1 host did not record itself"
  old_host=$(current_host_pid)
  alive "$old_host" || fail "the SP1 host $old_host is not alive"
  app_start
  select_repo "$W/repo"
  pidfile="$W/pid.txt"
  pane=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 30 pid_written "$pidfile" || fail "the terminal opened over the SP1 host never wrote its pid"
  pid=$(pid_in "$pidfile")
  wait_until 10 beat_above "$pane" 1 || fail "the terminal opened over the SP1 host does not print"
  wait_until 10 dead "$old_host" || fail "the SP1 host $old_host is still alive: the app held it"
  new_host=$(current_host_pid)
  { [ "$new_host" != "$old_host" ] && alive "$new_host"; } \
    || fail "the host is $new_host, expected a new live host in place of the SP1 host $old_host"
  probe term-list
  [ "$RC" -eq 0 ] || fail "the new host does not advertise the terminal capability: $(head -c 200 "$W/probe.err")"
  wait_until 30 session_listed "$pane" || fail "the new host does not list $pane"
  [ "$(session_pid "$pane")" = "$pid" ] || fail "the new host lists pid $(session_pid "$pane") for $pane, expected $pid"
  wait_until 10 clients_are 1 "$pane" || fail "the new host counts $(session_clients "$pane") clients for $pane, expected 1"
  [ "$(pane_state "$pane" survives)" = true ] || fail "$pane does not report survives=true"
  NOTE="SP1 host $old_host gone; $pane (pid $pid) on new host $new_host with the terminal capability"
}

# Finding 8 (closing a worktree that this run never visited): B's terminal is on
# the host, the app is killed and relaunched on A, so B is never mounted. Closing
# B must still end its session, because the close reads the host's listing and
# not only the terminal views of this run (spec §4.2: "its sessions"). A's
# session is untouched.
case_close_unvisited_worktree() {
  new_world c23
  git -C "$W/repo" worktree add -q "$W/wt-b" -b feature-b
  app_start
  select_repo "$W/repo"
  local wt_a_id wt_b_id pa pb pa_pid pb_pid
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
  app_kill9
  app_start
  wait_until 30 pane_known "$pa" || fail "the relaunched app does not know $pa"
  # The relaunch restores A, so B has no terminal view in this run.
  wt_b_id=$(worktree_id_at "$W/wt-b")
  [ -n "$wt_b_id" ] || fail "sirioctl does not list the second worktree after the relaunch"
  ctl close-workspace --workspace "$wt_b_id" >/dev/null \
    || fail "closing the unvisited worktree B was refused"
  wait_until 10 session_gone "$pb" \
    || fail "the session $pb of the closed, unvisited worktree B is still listed (pid $pb_pid)"
  wait_until 10 pid_dead "$pb_pid" || fail "pid $pb_pid of the closed worktree B survived"
  session_listed "$pa" || fail "A's session $pa was ended by the close of B"
  [ "$(session_pid "$pa")" = "$pa_pid" ] || fail "A's session $pa changed pid (now $(session_pid "$pa"), was $pa_pid)"
  pid_alive "$pa_pid" || fail "pid $pa_pid of A's session $pa died with the close of B"
  NOTE="B's $pb (pid $pb_pid) ended though B was never mounted this run; A's $pa (pid $pa_pid) untouched"
}

# Finding N2 (closing a worktree ends another worktree's agent): B's layout
# names the ids its terminals took in this run, and after a relaunch on an
# empty host A's next id is one of them. Closing B must not end A's new
# terminal, whose cwd is A.
case_close_worktree_id_collision() {
  new_world c24
  git -C "$W/repo" worktree add -q "$W/wt-b" -b feature-b
  app_start
  select_repo "$W/repo"
  local wt_a_id wt_b_id pa1 pa2 pb1 pb2 pa3 pa3_pid
  wt_a_id="$WT_ID"
  pa1=$(open_terminal "sleep 300")
  pa2=$(open_terminal "sleep 300")
  ctl project add "$W/wt-b" >/dev/null 2>&1 || true
  wt_b_id=$(worktree_id_at "$W/wt-b")
  [ -n "$wt_b_id" ] || fail "sirioctl does not list the second worktree"
  ctl select-workspace --workspace "$wt_b_id" >/dev/null
  sleep 2
  pb1=$(open_terminal "sleep 300")
  pb2=$(open_terminal "sleep 300")
  ctl select-workspace --workspace "$wt_a_id" >/dev/null
  sleep 2
  ctl surface host end-all >/dev/null || fail "End All was refused"
  wait_until 20 host_gone || fail "the host did not end after End All"
  app_quit
  app_start
  wait_until 30 pane_known "$pa1" || fail "the relaunched app does not know $pa1"
  pa3=$(open_terminal "sleep 300")
  wait_until 10 session_listed "$pa3" || fail "the new terminal $pa3 in A is not on the host"
  pa3_pid=$(session_pid "$pa3")
  ctl close-workspace --workspace "$wt_b_id" >/dev/null || fail "closing B was refused"
  sleep 3
  session_listed "$pa3" || fail "A's new terminal $pa3 was ended by the close of B (B held $pb1 and $pb2)"
  pid_alive "$pa3_pid" || fail "pid $pa3_pid of A's new terminal $pa3 died with the close of B"
  NOTE="A's new terminal $pa3 (pid $pa3_pid) survives the close of B, which held $pb1 and $pb2"
}

# Finding N5 and #11 (a fresh terminal never dead-ends on a held id): the app
# starts while the host is unverifiable and does not adopt it. Once the host
# answers, a control session holds the id the next app terminal would take. The
# new terminal gets its own session and runs its command; the control session
# is untouched.
case_fresh_tab_never_dead_ends() {
  new_world c25
  app_start
  select_repo "$W/repo"
  local first t1 next control_pid control_clients t2 pidfile pid t2_now host_pid
  first=$(open_terminal "sleep 300")
  wait_until 10 session_listed "$first" || fail "the first terminal $first is not on the host"
  app_quit
  host_pid=$(current_host_pid)
  kill -STOP "$host_pid" || fail "could not stop the host $host_pid"
  STOPPED+=("$host_pid")
  app_start
  # The startup ensure gives up on the stopped host (its own log line says so),
  # so the app starts with no host adopted and the floor not raised.
  wait_until 90 grep -q "host unavailable" "$W/app.log" || fail "the startup ensure never reported the stopped host"
  kill -CONT "$host_pid" || fail "could not resume the host $host_pid"
  STOPPED=()
  t1=$(open_terminal "sleep 300")
  wait_until 60 session_listed "$t1" || fail "the terminal $t1 opened after SIGCONT is not on the host"
  next="pane-$(( ${t1#pane-} + 1 ))"
  probe term-create "$W/repo" --id "$next" -- sleep 300
  [ "$RC" -eq 0 ] || fail "host_probe term-create $next failed: $OUT"
  wait_until 10 session_listed "$next" || fail "the control session $next is not listed"
  control_pid=$(session_pid "$next")
  control_clients=$(session_clients "$next")
  pidfile="$W/pid.txt"
  t2=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 30 pid_written "$pidfile" || fail "the fresh terminal (held $next, opened as $t2) never ran its command"
  pid=$(pid_in "$pidfile")
  term_list || true
  t2_now=$(printf '%s\n' "$TERM_LIST" | grep -E " pid=$pid( |\$)" | sed -n 's/^session=\([^ ]*\) .*/\1/p' | head -1)
  [ -n "$t2_now" ] || fail "no host session runs the fresh terminal's pid $pid"
  [ "$t2_now" != "$next" ] || fail "the fresh terminal attached to the control session $next"
  session_listed "$next" || fail "the control session $next was ended"
  [ "$(session_clients "$next")" = "$control_clients" ] || fail "the control session $next gained a client (clients $(session_clients "$next"), expected $control_clients)"
  [ "$(session_pid "$next")" = "$control_pid" ] || fail "the held id $next now names another session"
  pid_alive "$control_pid" || fail "the control session pid $control_pid is not alive"
  NOTE="held $next (pid $control_pid) untouched; the fresh terminal runs its own session $t2_now (pid $pid)"
}

# Finding N5 (Restart on a Failed pane must never close a session): a restored
# tab fails while its session is live (the host is stopped). Restart, then the
# host resumes: the reopen must attach to the original session, not close it
# and create another under the same id.
case_restart_on_failed_is_safe() {
  new_world c26
  local pane pidfile pid host_pid
  app_start
  select_repo "$W/repo"
  pidfile="$W/pid.txt"
  pane=$(open_terminal "$(heartbeat "$pidfile")")
  wait_until 10 pid_written "$pidfile" || fail "the terminal never wrote its pid"
  pid=$(pid_in "$pidfile")
  app_quit
  host_pid=$(current_host_pid)
  kill -STOP "$host_pid" || fail "could not stop the host $host_pid"
  STOPPED+=("$host_pid")
  app_start
  wait_until 150 lost_flagged "$pane" || fail "the restored tab $pane never failed while the host was stopped"
  "$CTL" surface terminal restart "$pane" >/dev/null 2>&1 || fail "Restart on the failed tab $pane was refused"
  sleep 2
  kill -CONT "$host_pid" || fail "could not resume the host $host_pid"
  STOPPED=()
  wait_until 90 clients_are 1 "$pane" || fail "the reopened tab $pane never attached (clients $(session_clients "$pane"))"
  pid_alive "$pid" || fail "Restart on the failed tab closed its live session: pid $pid is dead"
  [ "$(session_pid "$pane")" = "$pid" ] || fail "the reopened tab $pane runs pid $(session_pid "$pane"), expected the original $pid"
  NOTE="restarted $pane attached to its original pid $pid after SIGCONT"
}

# Finding 6 and N5 (the startup work runs on the first adoption, not only at
# the startup's): the app starts while the host is unverifiable, so no startup
# work runs. An orphan session of the app's own (no tab names it) stands on the
# host. Once the host answers, the first terminal's open adopts the host, and
# the orphan is adopted with it: it becomes a tab, attached to its own pid.
case_adopt_after_late_host() {
  new_world c27
  local pane pidfile pid host_pid first
  pane=pane-7
  pidfile="$W/pid.txt"
  app_start
  select_repo "$W/repo"
  app_quit
  probe ensure
  [ "$RC" -eq 0 ] || fail "host_probe ensure failed: $OUT"
  probe term-create "$W/repo" --id "$pane" --origin app -- /bin/sh -c "$(heartbeat "$pidfile")"
  [ "$RC" -eq 0 ] || fail "host_probe term-create failed: $OUT"
  wait_until 10 pid_written "$pidfile" || fail "the orphan session never wrote its pid"
  pid=$(pid_in "$pidfile")
  host_pid=$(current_host_pid)
  kill -STOP "$host_pid" || fail "could not stop the host $host_pid"
  STOPPED+=("$host_pid")
  app_start
  wait_until 90 grep -q "host unavailable" "$W/app.log" || fail "the startup ensure never reported the stopped host"
  kill -CONT "$host_pid" || fail "could not resume the host $host_pid"
  STOPPED=()
  # The reconnect the app makes once the host answers (its initial terminal is
  # waiting on the host) adopts the orphan. No terminal is opened here: a new
  # tab made in the same instant would take the active tab from the adopted one,
  # which is not drawn, so it never opens and never attaches until it is selected.
  wait_until 60 pane_known "$pane" || fail "the reconnect after SIGCONT did not adopt the orphan $pane"
  wait_until 20 survives_is "$pane" true || fail "the adopted $pane is not attached to the host (survives $(pane_state "$pane" survives 2>/dev/null))"
  [ "$(session_pid "$pane")" = "$pid" ] || fail "the adopted $pane is another session (pid $(session_pid "$pane"), expected $pid)"
  wait_until 10 clients_are 1 "$pane" || fail "the adopted $pane has $(session_clients "$pane") clients, expected 1"
  NOTE="orphan $pane (pid $pid) adopted by the reconnect after SIGCONT, attached with one client"
}

# Finding N3 (a restore attaches only to a session its own worktree holds): A's
# layout names pane-N, and the host holds a live session with that id in B's
# cwd. Restoring A must not attach to it: A's tab takes a fresh id and runs its
# own session, and B's session keeps its pid with no client.
case_restore_stale_id_of_another_worktree() {
  new_world c28
  git -C "$W/repo" worktree add -q "$W/wt-b" -b feature-b
  app_start
  select_repo "$W/repo"
  local pane_a control_pid app_session
  ctl project add "$W/wt-b" >/dev/null 2>&1 || true
  pane_a=$(open_terminal "sleep 300")
  wait_until 10 session_listed "$pane_a" || fail "the terminal $pane_a is not on the host"
  app_quit
  probe shutdown --force >/dev/null 2>&1 || true
  wait_until 20 host_gone || fail "the host did not end before the control session was made"
  probe ensure
  [ "$RC" -eq 0 ] || fail "host_probe ensure failed: $OUT"
  probe term-create "$W/wt-b" --id "$pane_a" -- sleep 300
  [ "$RC" -eq 0 ] || fail "host_probe term-create $pane_a in B failed: $OUT"
  wait_until 10 session_listed "$pane_a" || fail "B's session $pane_a is not listed"
  control_pid=$(session_pid "$pane_a")
  app_start
  app_session=""
  for _ in $(seq 1 300); do
    term_list || true
    app_session=$(printf '%s\n' "$TERM_LIST" | grep 'origin=app' | grep -v " pid=$control_pid " | head -1 || true)
    [ -n "$app_session" ] && break
    sleep 0.2
  done
  [ -n "$app_session" ] || fail "A's restored tab did not get a session of its own (the host lists $(printf '%s' "$TERM_LIST" | tr '\n' ' '))"
  [ "$(session_clients "$pane_a")" = 0 ] || fail "A's restore attached to B's session $pane_a (clients $(session_clients "$pane_a"), expected 0)"
  [ "$(session_pid "$pane_a")" = "$control_pid" ] || fail "B's session $pane_a changed pid"
  pid_alive "$control_pid" || fail "B's session pid $control_pid is not alive"
  NOTE="B's $pane_a (pid $control_pid) untouched with no client; A's restored tab runs its own session"
}

# Fix batch 7, I1 (a pane that moves to a fresh id launches its agent under that
# id): a new agent tab opens on an id a control session holds. The agent's
# command must name the id the tab really took, in its notify (--session) and in
# SIRIO_PANE_ID, and the notify must save its ref under that id, not the held one.
# The codex stub stands for codex: it records its argv and SIRIO_PANE_ID, then runs
# the notify its -c override names, with an agent ref derived from the pane.
install_codex_stub() {
  mkdir -p "$W/bin" "$W/stub"
  cat >"$W/bin/codex" <<'STUB'
#!/usr/bin/env python3
# A stand-in for codex (e2e). Only an agent launch (a -c notify override) stays
# as the pane; every other call (a version probe) answers at once.
import json, os, subprocess, sys, time
args = sys.argv[1:]
if not args or not args[0].startswith("-c") or len(args) < 2 or not args[1].startswith("notify="):
    sys.exit(0)
stub = os.environ["E2E_STUB_DIR"]
os.makedirs(stub, exist_ok=True)
base = os.path.join(stub, "codex-%d" % os.getpid())
with open(base + ".argv", "wb") as f:
    f.write(b"\0".join(a.encode() for a in args))
pane = os.environ.get("SIRIO_PANE_ID", "")
notify = json.loads(args[1][len("notify="):])
subprocess.run(notify + ["--agent-session", "e2e-" + pane], check=False)
with open(base + ".env", "w") as f:
    f.write(pane)
time.sleep(300)
STUB
  chmod +x "$W/bin/codex"
  # The shell the world's terminals start (`$SHELL -lc CMD`): a login profile would
  # put a real codex back ahead of the stub, so this one runs CMD with the stub's PATH.
  cat >"$W/bin/guard-shell" <<GUARD
#!/bin/sh
PATH="$W/bin:/usr/bin:/bin"
export PATH
while [ \$# -gt 0 ]; do
  case "\$1" in
    -*c*) shift; exec /bin/sh -c "\$1" ;;
    *) shift ;;
  esac
done
exec /bin/sh
GUARD
  chmod +x "$W/bin/guard-shell"
}

# The newest codex launch of this world: its file (argv or env) and, from the
# argv, the --session its notify names.
newest_codex() { ls -t "$W"/stub/codex-*."$1" 2>/dev/null | head -n 1; }
codex_launched() { [ -n "$(newest_codex env)" ]; }
codex_notify_session() {
  python3 -c '
import json, sys
args = open(sys.argv[1], "rb").read().split(b"\0")
notify = next(a.decode() for a in args if a.decode().startswith("notify="))
parts = json.loads(notify[len("notify="):])
print(parts[parts.index("--session") + 1])
' "$1"
}
# The ref the store holds for a pane key ("" when none): read from the world's db.
saved_ref() {
  python3 -c '
import sqlite3, sys
row = sqlite3.connect(sys.argv[1]).execute("SELECT reference FROM session_ref WHERE session = ?", (sys.argv[2],)).fetchone()
print(row[0] if row else "")
' "$W/s.sqlite" "$1"
}
saved_ref_is() { [ "$(saved_ref "$1")" = "$2" ]; }

case_agent_rebind_reports_under_its_id() {
  new_world c31
  install_codex_stub
  app_start "PATH=$W/bin:$PATH" "SHELL=$W/bin/guard-shell" "E2E_STUB_DIR=$W/stub"
  select_repo "$W/repo"
  local first next control_pid control_clients new_pane session_arg
  first=$(open_terminal "sleep 300")
  wait_until 10 session_listed "$first" || fail "the first terminal $first is not on the host"
  next="pane-$(( ${first#pane-} + 1 ))"
  probe term-create "$W/repo" --id "$next" -- sleep 300
  [ "$RC" -eq 0 ] || fail "host_probe term-create $next failed: $OUT"
  wait_until 10 session_listed "$next" || fail "the control session $next is not listed"
  control_pid=$(session_pid "$next")
  control_clients=$(session_clients "$next")
  ctl surface terminal open --agent codex >/dev/null || fail "the agent terminal was refused"
  wait_until 60 codex_launched || fail "the codex agent never started (its tab held $next)"
  new_pane=$(cat "$(newest_codex env)")
  [ -n "$new_pane" ] && [ "$new_pane" != "$next" ] || fail "the agent tab kept the held id $next (SIRIO_PANE_ID=$new_pane)"
  session_arg=$(codex_notify_session "$(newest_codex argv)")
  [ "$session_arg" = "$new_pane" ] || fail "codex reports under --session $session_arg, but its pane is $new_pane (the held id was $next)"
  wait_until 10 session_listed "$new_pane" || fail "the agent's pane $new_pane is not on the host"
  wait_until 10 saved_ref_is "$new_pane" "e2e-$new_pane" || fail "the agent's notify saved no ref under $new_pane (saved: '$(saved_ref "$new_pane")')"
  saved_ref_is "$next" "e2e-$new_pane" && fail "the agent's notify saved its ref under the held id $next"
  [ "$(session_pid "$next")" = "$control_pid" ] || fail "the held id $next now names another session"
  [ "$(session_clients "$next")" = "$control_clients" ] || fail "the control session $next gained a client"
  NOTE="held $next (pid $control_pid) untouched; the agent took $new_pane and reported under it"
}

# Fix batch 7, I1 for a restored agent: its saved session ref moves with it, so
# the pane that takes a fresh id after the held one resumes the same session.
# App A's codex pane is saved and resumable (its rollout file is on disk); the
# host is ended, another worktree's control session takes its id, and the
# relaunch refuses that id (N3) and reopens the agent under a fresh one.
codex_launches() { ls "$W"/stub/codex-*.env 2>/dev/null | wc -l; }
codex_launched_twice() { [ "$(codex_launches)" -ge 2 ]; }
codex_resume_arg() {
  python3 -c '
import sys
args = open(sys.argv[1], "rb").read().split(b"\0")
print(args[args.index(b"resume") + 1].decode() if b"resume" in args else "")
' "$1"
}

case_restored_agent_rebind_resumes_under_its_id() {
  new_world c32
  git -C "$W/repo" worktree add -q "$W/wt-b" -b feature-b
  install_codex_stub
  mkdir -p "$W/codex-home/sessions"
  local -a agent_env=("PATH=$W/bin:$PATH" "SHELL=$W/bin/guard-shell" "E2E_STUB_DIR=$W/stub" "CODEX_HOME=$W/codex-home")
  app_start "${agent_env[@]}"
  select_repo "$W/repo"
  ctl project add "$W/wt-b" >/dev/null 2>&1 || true
  local pane_a ref control_pid argv_new pane_new
  ctl surface terminal open --agent codex >/dev/null || fail "the agent terminal was refused"
  wait_until 60 codex_launched || fail "the codex agent never started"
  pane_a=$(cat "$(newest_codex env)")
  ref="e2e-$pane_a"
  wait_until 10 saved_ref_is "$pane_a" "$ref" || fail "the agent saved no ref under $pane_a (saved: '$(saved_ref "$pane_a")')"
  : >"$W/codex-home/sessions/rollout-$ref.jsonl"
  app_quit
  probe shutdown --force >/dev/null 2>&1 || true
  wait_until 20 host_gone || fail "the host did not end before B's control session was made"
  probe ensure
  [ "$RC" -eq 0 ] || fail "host_probe ensure failed: $OUT"
  probe term-create "$W/wt-b" --id "$pane_a" -- sleep 300
  [ "$RC" -eq 0 ] || fail "host_probe term-create $pane_a in B failed: $OUT"
  wait_until 10 session_listed "$pane_a" || fail "B's session $pane_a is not listed"
  control_pid=$(session_pid "$pane_a")
  app_start "${agent_env[@]}"
  wait_until 60 codex_launched_twice || fail "the restored agent of A never relaunched (its tab held $pane_a)"
  pane_new=$(cat "$(newest_codex env)")
  [ -n "$pane_new" ] && [ "$pane_new" != "$pane_a" ] || fail "the restored agent kept the held id $pane_a (SIRIO_PANE_ID=$pane_new)"
  argv_new=$(newest_codex argv)
  [ "$(codex_notify_session "$argv_new")" = "$pane_new" ] || fail "codex reports under $(codex_notify_session "$argv_new"), but its pane is $pane_new (the held id was $pane_a)"
  [ "$(codex_resume_arg "$argv_new")" = "$ref" ] || fail "the restored agent does not resume $ref under $pane_new (resume: '$(codex_resume_arg "$argv_new")')"
  wait_until 10 session_listed "$pane_new" || fail "the restored agent's pane $pane_new is not on the host"
  [ "$(session_pid "$pane_a")" = "$control_pid" ] || fail "B's session $pane_a changed pid"
  pid_alive "$control_pid" || fail "B's session pid $control_pid is not alive"
  NOTE="B's $pane_a (pid $control_pid) untouched; A's restored agent took $pane_new and resumed $ref under it"
}

# ---------------------------------------------------------------- the run

# Fix batch 6, N1 (a nested Sirio does not inherit the outer app's host root):
# app 2, launched from a pane of app 1 with only its own SIRIO_DB and
# SIRIO_SOCKET, has a host root of its own. It does not adopt app 1's host or
# list app 1's sessions, and a sirioctl in app 1's pane still reaches app 1's
# host when the app's socket is not there to answer. App 1 takes its root from
# its database, not from an explicit SIRIO_HOST_HOME, which a child would inherit.
case_nested_app_own_host() {
  new_world c29
  unset SIRIO_HOST_HOME
  app_start
  select_repo "$W/repo"
  local pane1 pid1 host1_pid nested host2_pid nested_pid pane3
  nested="$W/nested"
  mkdir -p "$nested"
  host1_pid=$(current_host_pid)
  pane1=$(open_terminal "$(heartbeat "$W/pid1.txt")")
  wait_until 10 pid_written "$W/pid1.txt" || fail "app 1's terminal never wrote its pid"
  pid1=$(pid_in "$W/pid1.txt")
  # Typed into a pane of app 1: the nested app inherits that pane's environment.
  open_terminal "env SIRIO_DB=$nested/s.sqlite SIRIO_SOCKET=$nested/c.sock $APP_EXE >>$nested/app.log 2>&1 & echo \$! >$nested/app.pid; sleep 600" >/dev/null
  wait_until 60 pid_written "$nested/app.pid" || fail "the nested app was never launched from app 1's pane"
  nested_pid=$(pid_in "$nested/app.pid")
  APPS+=("$nested_pid")
  WORLDS+=("$nested")
  wait_until 60 test -s "$nested/host/host-v1.json" || fail "the nested app has no host of its own under $nested/host"
  host2_pid=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["pid"])' "$nested/host/host-v1.json")
  [ "$host2_pid" != "$host1_pid" ] || fail "the nested app adopted app 1's host (pid $host1_pid)"
  alive "$host1_pid" || fail "app 1's host $host1_pid is gone"
  # App 2's host lists none of app 1's sessions.
  local listing rc=0
  listing=$(env -u SIRIO_HOST_HOME SIRIO_DB="$nested/s.sqlite" "$PROBE" term-list 2>/dev/null) || rc=$?
  [ "$rc" -eq 0 ] || fail "the nested app's host did not answer term-list (exit $rc)"
  if printf '%s\n' "$listing" | grep -q "^session=$pane1 "; then
    fail "the nested app's host lists app 1's session $pane1"
  fi
  # A sirioctl in app 1's pane, with no socket to answer and no database
  # variable to lead anywhere, reaches app 1's host: panel state of app 1's
  # pane is answered by the host, and only the pane's own root holds it.
  pane3=$(open_terminal "env -u SIRIO_DB SIRIO_SOCKET=$W/none.sock $CTL panel state $pane1 >$W/panel-in-pane.txt 2>&1; sleep 600")
  wait_until 30 grep -q "running" "$W/panel-in-pane.txt" \
    || fail "a sirioctl in app 1's pane did not reach app 1's host: $(head -c 300 "$W/panel-in-pane.txt" 2>/dev/null)"
  NOTE="app 2 host $host2_pid, app 1 host $host1_pid; app 1's $pane1 (pid $pid1) not on app 2's host; sirioctl in $pane3 reached app 1's host"
}

# Fix batch 6, N4 (no re-ensure storm): the host binary starts and never
# answers, so every ensure waits out its start and finds no host. The app's
# startup ensure is the one attempt the storm may cost; three terminals opened
# in quick succession share its answer. All three fall back to LocalLink within
# 15 s of being opened, and the host is started once in all. The stub records
# each start, so the count is the spawns themselves, not a log line.
case_ensure_storm() {
  new_world c30
  local stub="$W/stub-host" starts="$W/host-starts.txt" first second third count
  printf '#!/bin/sh\necho "$$" >> %s\nexit 0\n' "$starts" >"$stub"
  chmod +x "$stub"
  export SIRIO_HOST_BIN="$stub"
  app_start
  select_repo "$W/repo"
  first=$(open_terminal "sleep 300")
  second=$(open_terminal "sleep 300")
  third=$(open_terminal "sleep 300")
  wait_until 15 all_fallen_back "$first" "$second" "$third" \
    || fail "the three terminals ($first, $second, $third) did not all fall back to LocalLink within 15 s (survives: $(pane_state "$first" survives 2>/dev/null)/$(pane_state "$second" survives 2>/dev/null)/$(pane_state "$third" survives 2>/dev/null))"
  count=0
  if [ -f "$starts" ]; then count=$(wc -l <"$starts" | tr -d ' '); fi
  [ "$count" -le 1 ] || fail "the host was started $count times for three terminals; at most the startup's own start is allowed"
  NOTE="three terminals on LocalLink within 15 s; host started $count time(s)"
}
all_fallen_back() { # panes: each reports survives=false
  local p
  for p in "$@"; do
    [ "$(pane_state "$p" survives 2>/dev/null)" = false ] || return 1
  done
}

run_case() { # number name function
  N="$1"; CASE_NAME="$2"; CASE_STARTED=$(date +%s); NOTE=""; SKIPPED=0
  if [ -n "$ONLY" ] && [ "${ONLY#*,$N,}" = "$ONLY" ]; then return 0; fi
  "$3"
  end_case
  # A case that could not run here says so, and is not counted as passed.
  if [ "$SKIPPED" -eq 1 ]; then
    echo "CASE $N $CASE_NAME SKIP${NOTE:+ ($NOTE)}"
    return 0
  fi
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
run_case 21 exited-attach case_exited_attach
run_case 22 sp1-host-upgrade case_sp1_host_upgrade
run_case 23 close-unvisited-worktree case_close_unvisited_worktree
run_case 24 close-worktree-id-collision case_close_worktree_id_collision
run_case 25 fresh-tab-never-dead-ends case_fresh_tab_never_dead_ends
run_case 26 restart-on-failed-is-safe case_restart_on_failed_is_safe
run_case 27 adopt-after-late-host case_adopt_after_late_host
run_case 28 restore-stale-id-of-another-worktree case_restore_stale_id_of_another_worktree
run_case 29 nested-app-own-host case_nested_app_own_host
run_case 30 ensure-storm case_ensure_storm
run_case 31 agent-rebind-reports-under-its-id case_agent_rebind_reports_under_its_id
run_case 32 restored-agent-rebind-resumes-under-its-id case_restored_agent_rebind_resumes_under_its_id

if [ -n "$ONLY" ]; then
  echo "HOST SESSIONS E2E PARTIAL ($CASES_PASSED cases passed)"
else
  echo "HOST SESSIONS E2E OK ($CASES_PASSED cases, $(( $(date +%s) - START_ALL ))s)"
fi
