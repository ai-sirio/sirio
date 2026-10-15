#!/bin/bash
set -euo pipefail

# End-to-end test of the update card (spec 2026-10-14-update-dialog-design.md
# §5.2).
#
# Stage chain: Sirio compiled the way the release job compiles it -- Nightly, a
# loopback manifest, a key made for this run -- finds a signed update at launch
# and shows the card; on `act restart` it downloads, verifies, swaps a stand-in
# $APPIMAGE and relaunches into it. Asserted: the card at launch; Downloading ->
# Installing -> Installed in the log; the old pid exits and a new one answers;
# the new artifact's marker exists; the relaunched process was started with this
# run's environment (/proc/<pid>/environ); the project is still registered and
# the worktree selected before the restart is the restored, selected one. Chain
# types nothing, so its focus is not observable here: the focus proof is stage ui.
#
# Stage ui: every state of the card drawn -- available, the agent warning,
# ready, downloading, failed, refused, installed -- with Later, the indicator and
# hide. The focus proof: a terminal is opened from its Terminal tile, a marker is
# typed, the card is injected, a second marker is typed, and both must reach that
# terminal's scrollback in that order (the card did not take the keys). Under
# --state-only the captures are skipped, and the keyboard and focus steps are
# printed as SKIP.
#
# Display: a display is required unless --state-only, and there is no default: the
# ambient $DISPLAY is never used. Refused (exit 2, nothing created): no --display;
# a value that is not a local :N; a number that is this session's $DISPLAY or
# $WAYLAND_DISPLAY (":0.0", ":00" and ":97.0" are the numbers 0, 0 and 97); a number
# that no Xvfb started by this user serves. The keys and clicks go to the display
# through XTEST, so before every one of them (the pointer move, the click, the
# typing and Return) the focused window, the window under the pointer and the
# window aimed at must belong to the app's pid, or the run stops. Start a private
# display for a run, for example:
#   Xvfb :97 -screen 0 1600x1000x24 &
#
# Isolation: every Sirio is started under `env -i` with the allowlist written in
# launch(); each entry there says why it is needed. PATH starts with <run>/bin,
# the stub agent CLIs (claude, codex, opencode, pi, omp), then the system
# directories. Sirio puts the login shell's PATH in front of that (login_path.rs
# at every start). The login shell is SHELL=/bin/bash with the private HOME, so
# it reads only /etc/profile and the private HOME's dotfiles, never the user's.
# That was checked: a sentinel in a private .bash_profile is read, and a PATH
# entry in a private .profile is not (bash reads .bash_profile first). The stub
# directory stays first because the login shell keeps the order it was given. Each
# stage asserts from stub-calls.log that the app's `claude --version` probe
# reached the stub, so stub-first is observed rather than assumed. A display-backed
# run also gets a private session bus and a private AT-SPI bus (never the user's),
# with the two accessibility flags turned on there (under env -i they are off: no
# desktop setting reaches the run), and stage ui notes whether this app holds a
# connection on it (a note, not a check). The duplicate-id guard (the ping after
# each error capture) means something only while bezel's accessibility adapter is
# active: stage ui asserts the read's a11yActive (bezel's Window::is_a11y_active)
# before the error captures. Whether the adapter opened its own connection on the
# private AT-SPI bus is a note, not a check; docs/testing/update-card.md records
# the runs. Chain and ui note whether a started sirio-host detached (a session
# leader); the note is not asserted.
#
# The artifact: --out-dir DIR (default artifacts/update-ui-e2e-<stamp>-<pid>)
# keeps transcript.log (every card read and expect reply, as JSON), the app
# logs, build.log, stub-calls.log and -- unless --state-only -- PID-matched window
# captures in frames/. The script refuses an out dir that already has content and
# never removes one. Rerunning the script into a new out dir reproduces the run.
#
# Usage: Scripts/Tests/test-update-ui-e2e.sh [--stage chain|ui|all] [--state-only]
#          [--out-dir DIR] [--display :N] [--appearance light|dark]

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# A dedicated target: the release environment compiled in must not invalidate
# the ordinary debug build.
TARGET="$ROOT/rust/target/update-ui-e2e"
BIN="$TARGET/debug/sirio"
CTL="$TARGET/debug/sirioctl"
RELEASE_CLI="$TARGET/debug/sirio-release"
STAGE=all
STATE_ONLY=0
OUT_DIR=""
APPEARANCE=""
# No default: the ambient $DISPLAY is never used. A run names its display.
DISPLAY_TARGET=""
PORT="${SIRIO_UPDATE_UI_E2E_PORT:-38719}"
VERSION="99.0.0"
# The Terminal tile of the empty pane is clicked at these window coordinates. They
# were measured with the window at WINDOW_SIZE, which is what the app opens at on
# a 1600x1000 display; stage ui asserts the size before it clicks.
TILE_X=461
TILE_Y=418
WINDOW_SIZE="1470x833"
while [ $# -gt 0 ]; do
  case "$1" in
    --stage) STAGE="$2"; shift 2 ;;
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# Refusals come before anything is created, so a refused run leaves no artifact.
refuse() { echo "refused: $*" >&2; exit 2; }
case "$STAGE" in chain|ui|all) ;; *) refuse "unknown stage $STAGE" ;; esac
case "$APPEARANCE" in ""|light|dark) ;; *) refuse "--appearance is light or dark, not $APPEARANCE" ;; esac
# The display number of a local X display: ":97", ":0097", ":97.0" and
# "localhost:97" are all 97. Any other host is refused: it is not this machine's
# Xvfb. Prints nothing when the string is not such a display.
display_number() { # DISPLAY-string
  [[ "$1" =~ ^([A-Za-z0-9._-]*):([0-9]+)(\.[0-9]+)?$ ]] || return 0
  [ -z "${BASH_REMATCH[1]}" ] || [ "${BASH_REMATCH[1]}" = localhost ] || return 0
  echo $((10#${BASH_REMATCH[2]}))
}

if [ "$STATE_ONLY" -eq 0 ]; then
  for tool in import identify xwininfo xprop xdotool timeout gdbus busctl dbus-daemon; do
    command -v "$tool" >/dev/null || refuse "$tool is required for captures, input and the private buses; pass --state-only to skip them"
  done
fi
if [ -n "$DISPLAY_TARGET" ]; then
  target_number=$(display_number "$DISPLAY_TARGET")
  [ -n "$target_number" ] || refuse "--display takes a local X display such as :97, not '$DISPLAY_TARGET'"
  ambient_number=$(display_number "${DISPLAY:-}")
  [ "$target_number" != "$ambient_number" ] || refuse "--display $DISPLAY_TARGET is this session's own \$DISPLAY (${DISPLAY:-}): the keys and clicks would reach that desktop. Start a private display (Xvfb :$target_number -screen 0 1600x1000x24) and pass it"
  [ "$DISPLAY_TARGET" != "${WAYLAND_DISPLAY:-}" ] || refuse "--display $DISPLAY_TARGET is this session's \$WAYLAND_DISPLAY"
  # The number must be served by an Xvfb this user started. A server of another
  # number, or one that belongs to someone else, is refused whatever it is called.
  ps -u "$(id -u)" -o args= | python3 -c 'import re, sys; n = sys.argv[1]; sys.exit(0 if any(re.search(r"(^|/)Xvfb :" + n + r"( |$)", line) for line in sys.stdin) else 1)' "$target_number" \
    || refuse "no Xvfb :$target_number is running as ${USER:-$(id -un)}: start a private one (Xvfb :$target_number -screen 0 1600x1000x24 &)"
  DISPLAY_TARGET=":$target_number"
fi
if [ "$STATE_ONLY" -eq 0 ] && [ -z "$DISPLAY_TARGET" ]; then
  refuse "no --display: the captures and the keyboard need a display of their own; pass --display :N (a private Xvfb), or --state-only"
fi
if [ "$STATE_ONLY" -eq 0 ]; then
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool getdisplaygeometry >/dev/null 2>&1 \
    || refuse "no X server answers on $DISPLAY_TARGET: start a private one (Xvfb $DISPLAY_TARGET -screen 0 1600x1000x24 &)"
fi

[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/update-ui-e2e-$(date +%Y%m%d-%H%M%S)-$$"
# An out dir with content is someone's evidence: refuse it, never reuse or remove it.
if [ -d "$OUT_DIR" ] && [ -n "$(ls -A "$OUT_DIR")" ]; then
  echo "refusing to reuse $OUT_DIR: it already has content (pass a new --out-dir)" >&2
  exit 2
fi
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
# Every sirioctl call and every screen-capture tool is bounded: sirioctl waits up to
# an hour for an answer, and a wedged app must fail the run, naming the call. A
# timeout is raised inside $(...) too, where `exit` would end only the
# substitution, so it signals the script (TERM, trapped here) and the script exits
# through its EXIT cleanup.
CALL_TIMEOUT=30
IMPORT_TIMEOUT=15
trap 'exit 1' TERM
timed_out() { # what
  echo "FAIL: $* did not answer within its time limit; the run is ended" >&2
  kill -TERM $$ 2>/dev/null || true
  exit 1
}
[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux (macOS and Windows rows: docs/testing/update-card.md)"
command -v python3 >/dev/null || fail "python3 is required"
command -v curl >/dev/null || fail "curl is required"
echo "display: ${DISPLAY_TARGET:-none (--state-only)}; stage: $STAGE; state-only: $STATE_ONLY"

# Everything this run creates lives under RUN_DIR, which mktemp made just now.
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-update-ui-e2e-XXXXXX")
APP_PATH="$RUN_DIR/bin:/usr/local/bin:/usr/bin:/bin"
APP_PID=""
SERVER_PID=""
RUN_PIDS=()
SESSION_BUS=""
A11Y_BUS=""
cleanup() {
  local pid
  for pid in ${RUN_PIDS[@]+"${RUN_PIDS[@]}"}; do stop_process_group "$pid"; done
  # The document portal the app mounts under its runtime dir (FUSE) comes down
  # first, so the run dir can be removed.
  unmount_run_mounts
  stop_run_processes
  [ -z "$SERVER_PID" ] || kill "$SERVER_PID" 2>/dev/null || true
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  # A mount still under the run dir is reported and the dir is kept: removing a
  # tree with a live mount in it is not safe. Otherwise the removal stays on this
  # filesystem.
  if run_dir_mounted; then
    echo "LEFTOVER: a mount is still under $RUN_DIR; the run dir is kept" >&2
  else
    rm -rf --one-file-system "$RUN_DIR"
  fi
}
run_dir_mounted() { awk -v dir="$RUN_DIR" 'index($2, dir) == 1 { found = 1 } END { exit !found }' /proc/mounts; }
trap cleanup EXIT

# Unmounts whatever this run mounted under its own directory (FUSE mounts, which
# fusermount3 can take down without the process that serves them).
unmount_run_mounts() {
  local mount
  while read -r mount; do
    timeout 10 fusermount3 -u "$mount" 2>/dev/null || timeout 10 umount "$mount" 2>/dev/null || true
  done < <(awk -v dir="$RUN_DIR" 'index($2, dir) == 1 { print $2 }' /proc/mounts)
}

# SIGTERM to the process group `pid` leads, when it leads one that is not this
# script's own. Sirio is launched as the leader of a group of its own, and a
# relaunched Sirio leads the group of the restart script that started it.
stop_process_group() { # pid
  local group own
  group=$(ps -o pgid= -p "$1" 2>/dev/null | tr -d ' ' || true)
  own=$(ps -o pgid= -p $$ | tr -d ' ')
  if [ -n "$group" ] && [ "$group" != "$own" ]; then
    kill -TERM -- "-$group" 2>/dev/null || true
  fi
}

# Every process this run started carries SIRIO_UPDATE_E2E_RUN=<RUN_DIR> in its
# environment (the app, its helpers, the detached sirio-host and the relaunched
# app inherit it). Those are stopped even when they moved to a group or session
# of their own. SIGKILL follows after three seconds for any that ignore TERM.
stop_run_processes() {
  python3 - "$RUN_DIR" <<'PY'
import os, signal, sys, time
marker = b"SIRIO_UPDATE_E2E_RUN=" + sys.argv[1].encode() + b"\0"

def matches():
    found = []
    for entry in os.listdir("/proc"):
        if not entry.isdigit() or int(entry) == os.getpid():
            continue
        try:
            environ = open(f"/proc/{entry}/environ", "rb").read()
        except OSError:
            continue
        if marker in environ:
            found.append(int(entry))
    return found

def send(pids, sig):
    for pid in pids:
        try:
            os.kill(pid, sig)
        except OSError:
            pass

pids = matches()
send(pids, signal.SIGTERM)
deadline = time.time() + 3
while pids and time.time() < deadline:
    time.sleep(0.2)
    pids = matches()
send(pids, signal.SIGKILL)
PY
}

ctl() {
  local started status=0
  started=$(date +%s%N)
  echo "+ sirioctl $*"
  timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" || status=$?
  case "$status" in 124 | 137) timed_out "sirioctl $* (${CALL_TIMEOUT} s)" ;; esac
  [ "$status" -eq 0 ] || return "$status"
  echo "  answered in $(( ($(date +%s%N) - started) / 1000000 )) ms"
}
# sirioctl without the JSON flag, its stdout passed through: a call that does not
# answer ends the run.
call_ctl() {
  local status=0
  timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" || status=$?
  case "$status" in 124 | 137) timed_out "sirioctl $* (${CALL_TIMEOUT} s)" ;; esac
  return "$status"
}
reply() {
  local status=0
  # The timeout's own message goes to the script's stderr; only sirioctl's is hidden.
  timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" --json 2>/dev/null || status=$?
  case "$status" in 124 | 137) timed_out "sirioctl $* (${CALL_TIMEOUT} s)" ;; esac
  return "$status"
}
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
# Every read of the card is logged to the transcript as JSON (stderr, which the
# script sends to transcript.log), so the artifact shows what each check saw.
read_field() { local key=$1 json; shift; json=$(reply "$@") || fail "sirioctl $* did not answer (reading $key)"; echo "  read: $json" >&2; printf '%s' "$json" | field "$key" || fail "sirioctl $* did not answer (reading $key)"; }
wait_for() { # key value sirioctl-args...
  local key=$1 want=$2 json="" got=""
  shift 2
  for _ in $(seq 1 150); do
    json=$(reply "$@" || true)
    got=$(printf '%s' "$json" | field "$key" 2>/dev/null || true)
    if [ "$got" = "$want" ]; then
      echo "  read: $json"
      echo "OK: $key=$want"
      return 0
    fi
    sleep 0.3
  done
  echo "  read: $json" >&2
  fail "$key never became '$want' (last: '$got') for: $*"
}
expect() { # key value -- reads the card once, and logs the reply
  local json got
  json=$(reply surface update read) || fail "sirioctl surface update read did not answer"
  echo "  read: $json"
  got=$(printf '%s' "$json" | field "$1") || fail "sirioctl surface update read did not answer (reading $1)"
  [ "$got" = "$2" ] || fail "$1 is '$got', expected '$2'"
  echo "OK: $1=$2"
}

# Input goes to the private display only. Every XTEST call is bounded, and the
# DISPLAY it names is the run's own (never the ambient one).
xd() { DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool "$@"; }

find_window_once() {
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

# The app's window, found by its pid. The window may still be opening, so the
# lookup is bounded (20 seconds) and prints nothing if it never appears.
find_window() {
  local window="" _
  for _ in $(seq 1 100); do
    window=$(find_window_once)
    if [ -n "$window" ]; then
      echo "$window"
      return 0
    fi
    sleep 0.2
  done
  return 0
}

# The window must be the size the Terminal tile coordinates were measured at.
assert_window_size() { # window
  local size
  size=$(timeout 5 xwininfo -display "$DISPLAY_TARGET" -id "$1" | python3 -c '
import re, sys
text = sys.stdin.read()
w, h = re.search(r"Width:\s*(\d+)", text), re.search(r"Height:\s*(\d+)", text)
print(f"{w.group(1)}x{h.group(1)}" if w and h else "")
')
  [ "$size" = "$WINDOW_SIZE" ] || fail "the app's window is '$size', not $WINDOW_SIZE: the Terminal tile coordinates ($TILE_X,$TILE_Y) no longer name it"
  echo "OK: the window is $size, the size the tile coordinates were measured at" >&2
}

# Before any XTEST input: the focused window and the window under the pointer must
# both belong to the app's pid. A child window has no _NET_WM_PID of its own, so
# the owner is the nearest ancestor that carries one.
assert_input_target() { # window [focus-only] -- the window the input is aimed at; focus-only before the pointer moves
  WINDOW="$1" FOCUS_ONLY="${2:-}" DISPLAY="$DISPLAY_TARGET" APP_PID="$APP_PID" timeout 20 python3 - <<'PY' || fail "keyboard, pointer or target window is not the app's; no input was sent"
import os, re, subprocess, sys
want = int(os.environ["APP_PID"])
aimed = int(os.environ["WINDOW"], 0)
focus_only = os.environ.get("FOCUS_ONLY") == "focus-only"
env = dict(os.environ)

def run(*args):
    return subprocess.run(args, capture_output=True, text=True, timeout=5, env=env).stdout

def hexid(text):
    return f"0x{int(text):x}"

def owner(window):
    for _ in range(16):
        match = re.search(r"=\s*(\d+)\s*$", run("xprop", "-id", window, "_NET_WM_PID"))
        if match:
            return int(match.group(1))
        parent = re.search(r"Parent window id:\s*(0x[0-9a-fA-F]+)", run("xwininfo", "-id", window))
        if not parent or parent.group(1) == "0x0":
            return None
        window = parent.group(1)
    return None

focus = run("xdotool", "getwindowfocus").strip()
pointer = re.search(r"WINDOW=(\d+)", run("xdotool", "getmouselocation", "--shell"))
focus_owner = owner(hexid(focus)) if focus.isdigit() else None
pointer_owner = owner(hexid(pointer.group(1))) if pointer else None
aimed_owner = owner(hex(aimed))
print(f"input focus: pid {focus_owner}; pointer: pid {pointer_owner}; aimed window: pid {aimed_owner}; the app: {want}", file=sys.stderr)
pointer_ok = focus_only or pointer_owner == want
sys.exit(0 if focus_owner == want and pointer_ok and aimed_owner == want else 1)
PY
  if [ "${2:-}" = focus-only ]; then
    echo "OK: the keyboard focus is the app's window (pid $APP_PID)" >&2
  else
    echo "OK: the keyboard focus, the pointer and the target are the app's window (pid $APP_PID)" >&2
  fi
}

capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window
  window=$(find_window)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $1"
  local status=0
  timeout -k 5 "$IMPORT_TIMEOUT" import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png" || status=$?
  case "$status" in 124 | 137) fail "import of the $1 frame did not finish within ${IMPORT_TIMEOUT} s" ;; esac
  [ "$status" -eq 0 ] || fail "import of the $1 frame failed (status $status)"
  local colours
  status=0
  colours=$(timeout -k 5 "$IMPORT_TIMEOUT" identify -format '%k' "$OUT_DIR/frames/$1.png") || status=$?
  case "$status" in 124 | 137) fail "identify of the $1 frame did not finish within ${IMPORT_TIMEOUT} s" ;; esac
  [ "$status" -eq 0 ] || fail "identify of the $1 frame failed (status $status)"
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}

# The value one environment variable holds in a running process, read from its
# own /proc entry: what a (relaunched) Sirio was actually started with.
environment_of() { # pid name
  python3 - "$1" "$2" <<'PY'
import sys
pid, name = sys.argv[1], sys.argv[2].encode()
pairs = (item.split(b"=", 1) for item in open(f"/proc/{pid}/environ", "rb").read().split(b"\0") if b"=" in item)
print({key: value for key, value in pairs}.get(name, b"").decode())
PY
}

# The decoded scrollback of a terminal pane (panel read answers base64).
pane_text() { # pane
  local raw status=0
  raw=$(timeout -k 5 "$CALL_TIMEOUT" "$CTL" panel read "$1") || status=$?
  case "$status" in 124 | 137) timed_out "sirioctl panel read $1 (${CALL_TIMEOUT} s)" ;; esac
  [ "$status" -eq 0 ] || return "$status"
  printf '%s' "$raw" | python3 -c 'import base64, sys; sys.stdout.write(base64.b64decode(sys.stdin.read().strip()).decode(errors="replace"))'
}
# Polls a pane's scrollback until `needle` is in it. Bounded: a missed keystroke
# is a failure with the scrollback printed, not a fixed sleep that may be too short.
wait_pane_text() { # pane needle timeout-seconds
  local pane=$1 needle=$2 tries=$(( ${3:-15} * 5 )) text=""
  for _ in $(seq 1 "$tries"); do
    text=$(pane_text "$pane" 2>/dev/null || true)
    case "$text" in *"$needle"*) return 0 ;; esac
    sleep 0.2
  done
  printf 'scrollback of %s when the wait ended:\n%s\n' "$pane" "$text" >&2
  fail "'$needle' never reached the terminal $pane"
}

# A terminal from the empty pane's Terminal tile. Its pane id is the one that
# appears after the click: the ids seen before are taken away first.
open_terminal() { # window -> prints the new terminal's pane id
  local window=$1 before new=""
  before=$(reply panel list | python3 -c 'import json, sys; print(" ".join(row["id"] for row in json.load(sys.stdin) if row.get("tab") == "Terminal"))')
  assert_window_size "$window"
  xd windowfocus --sync "$window" || fail "could not focus the app's window (xdotool windowfocus)"
  # The move is XTEST input as well: before it, the focus must be the app's.
  assert_input_target "$window" focus-only
  xd mousemove --window "$window" "$TILE_X" "$TILE_Y" || fail "could not move the pointer onto the Terminal tile"
  assert_input_target "$window"
  xd click 1 || fail "could not click the Terminal tile"
  for _ in $(seq 1 50); do
    new=$(reply panel list | python3 -c '
import json, sys
before = set(sys.argv[1].split())
fresh = [row["id"] for row in json.load(sys.stdin) if row.get("tab") == "Terminal" and row["id"] not in before]
print(fresh[0] if fresh else "")
' "$before" 2>/dev/null || true)
    [ -n "$new" ] && break
    sleep 0.2
  done
  [ -n "$new" ] || fail "clicking the Terminal tile opened no terminal"
  echo "$new"
}

# Types a marker into the terminal: the input target is checked first, then the
# marker and Return go through XTEST, then the marker must reach the scrollback.
# The flake this replaced (the keystroke step failed in 2 of 9 runs of the card
# fix) read the scrollback once, a fixed second after typing: when the new shell
# had not yet drawn its prompt, or its echo landed late, the single read missed
# the text. The keys now wait for the prompt, and the scrollback is polled.
type_marker() { # window pane marker
  assert_input_target "$1"
  xd type --delay 30 "$3" || fail "xdotool could not type into the app's window"
  assert_input_target "$1"
  xd key Return || fail "xdotool could not press Return in the app's window"
  wait_pane_text "$2" "$3" 15
  echo "OK: '$3' reached the terminal $2"
}

# ---- the signing side, the stub agents and the server ---------------------
# cargo's warnings go to build.log in the artifact, not into the transcript.
echo "building the signing CLI"
(cd "$ROOT/rust" && CARGO_TARGET_DIR="$TARGET" cargo build --quiet -p sirio_release --bin sirio-release) >>"$RUN_DIR/build.log" 2>&1 \
  || fail "the signing CLI did not build (see build.log in $OUT_DIR)"
SERVE="$RUN_DIR/serve"
mkdir -p "$SERVE"
"$RELEASE_CLI" keygen --out "$RUN_DIR" >/dev/null
PUB_KEY=$(tr -d '\r\n' < "$RUN_DIR/sirio-release-signing.pub")
[ -n "$PUB_KEY" ] || fail "keygen produced an empty public key"
MANIFEST_URL="http://127.0.0.1:$PORT/nightly.json"

# The agent CLIs are stubs, first on the app's PATH. Each records its argv in
# stub-calls.log and answers --version at once. claude's version is above
# MIN_CLAUDE_VERSION, so Sirio chooses the native transport, not the registry
# wrapper (which would reach the network). Mirrors test-change-request-handoff-e2e.sh.
install_stubs() {
  mkdir -p "$RUN_DIR/bin"
  local name
  for name in claude codex opencode pi omp; do
    cat >"$RUN_DIR/bin/$name" <<STUB_EOF
#!/bin/sh
printf '%s\n' "$name \$*" >>"$RUN_DIR/stub-calls.log"
case "\$1" in
  --version|-V|version)
    if [ "$name" = claude ]; then echo "9.9.9 (Claude Code)"; else echo "$name 0.0.0 (e2e stub)"; fi
    exit 0 ;;
esac
exit 0
STUB_EOF
    chmod +x "$RUN_DIR/bin/$name"
  done
}
# Every agent CLI the app's PATH can reach resolves to its stub, or the run stops.
check_stub_path() {
  local name resolved
  for name in claude codex opencode pi omp; do
    resolved=$(export PATH="$APP_PATH"; command -v "$name" || true)
    [ "$resolved" = "$RUN_DIR/bin/$name" ] || fail "$name resolves to '$resolved' under the app's PATH, not its stub"
  done
  echo "OK: every agent CLI the app can reach is its stub"
}
install_stubs
check_stub_path

echo "building sirio and sirioctl with the release environment compiled in"
(cd "$ROOT/rust" && \
  CARGO_TARGET_DIR="$TARGET" \
  SIRIO_RELEASE_CHANNEL=nightly \
  SIRIO_UPDATE_MANIFEST_URL="$MANIFEST_URL" \
  SIRIO_RELEASE_ACCEPTED_KEYS="$PUB_KEY" \
  cargo build --quiet -p sirio --bin sirio && \
  CARGO_TARGET_DIR="$TARGET" cargo build --quiet -p sirio_control --bin sirioctl) >>"$RUN_DIR/build.log" 2>&1 \
  || fail "sirio did not build (see build.log in $OUT_DIR)"

# The new "AppImage": a shell script, so no appimagetool is needed. It leaves a
# marker, then runs the same debug binary.
ARTIFACT="$SERVE/Sirio-$VERSION.AppImage"
cat > "$ARTIFACT" <<'ARTIFACT_SH'
#!/bin/sh
touch "$SIRIO_UPDATE_E2E_MARKER"
exec "$SIRIO_UPDATE_E2E_BIN" "$@"
ARTIFACT_SH
chmod +x "$ARTIFACT"
"$RELEASE_CLI" sign \
  --key "$RUN_DIR/sirio-release-signing.key" \
  --channel nightly \
  --version "$VERSION" \
  --notes "Update card E2E build." \
  --url-template "http://127.0.0.1:$PORT/{file}" \
  --artifact "linux-x86_64=$ARTIFACT" \
  --out "$SERVE/nightly.json"

if curl -fsS -o /dev/null --max-time 2 "http://127.0.0.1:$PORT/" 2>/dev/null; then
  fail "port $PORT is already in use; set SIRIO_UPDATE_UI_E2E_PORT"
fi
env SIRIO_UPDATE_E2E_RUN="$RUN_DIR" python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$SERVE" >"$RUN_DIR/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 50); do curl -fsS -o /dev/null --max-time 1 "$MANIFEST_URL" 2>/dev/null && break; sleep 0.2; done
curl -fsS -o /dev/null --max-time 1 "$MANIFEST_URL" || fail "the manifest server never came up"

# ---- the session bus and the AT-SPI bus ----------------------------------
# bezel's accessibility tree (through accesskit_unix) reaches AT-SPI over the
# session bus, and its duplicate-node panic only exists where that adapter is
# active. A display-backed run therefore gets a private session bus and a private
# AT-SPI bus (at-spi-bus-launcher), and never the user's: a run has no path to
# the user's bus. Whether accesskit's adapter itself activates is not observable
# from outside here (see note_a11y_bus).
start_private_bus() {
  [ -z "$SESSION_BUS" ] || return 0
  local launcher=/usr/lib/at-spi-bus-launcher bus_home="$RUN_DIR/bus-home"
  local bus_runtime="$RUN_DIR/bus-runtime" address="" _
  [ -x "$launcher" ] || fail "at-spi-bus-launcher is required for the accessibility notes (install at-spi2-core)"
  mkdir -p "$bus_home/.config" "$RUN_DIR/bus-data"
  mkdir -m 700 "$bus_runtime"
  # Both bus processes start under env -i with nothing of the user's: the system
  # directories, the run's own HOME and XDG dirs, the system service files, and the
  # run marker. The session bus socket is the default one under this runtime dir,
  # so the launcher needs no address variable to find it.
  local bus_env=(
    PATH=/usr/local/bin:/usr/bin:/bin
    "HOME=$bus_home"
    "XDG_CONFIG_HOME=$bus_home/.config"
    "XDG_DATA_HOME=$RUN_DIR/bus-data"
    "XDG_RUNTIME_DIR=$bus_runtime"
    XDG_DATA_DIRS=/usr/local/share:/usr/share
    "SIRIO_UPDATE_E2E_RUN=$RUN_DIR"
  )
  SESSION_BUS="unix:path=$bus_runtime/bus"
  env -i "${bus_env[@]}" dbus-daemon --session --fork --address="$SESSION_BUS" >>"$RUN_DIR/bus.log" 2>&1 \
    || fail "could not start a private session bus"
  env -i "${bus_env[@]}" "$launcher" --launch-immediately >>"$RUN_DIR/a11y-launcher.log" 2>&1 &
  for _ in $(seq 1 50); do
    address=$(timeout 5 gdbus call --address "$SESSION_BUS" --dest org.a11y.Bus --object-path /org/a11y/bus \
      --method org.a11y.Bus.GetAddress 2>/dev/null | python3 -c 'import re, sys; m = re.search(r"\(\x27([^\x27]+)\x27,?\)", sys.stdin.read()); print(m.group(1) if m else "")' || true)
    [ -n "$address" ] && break
    sleep 0.2
  done
  [ -n "$address" ] || fail "the private AT-SPI bus did not answer org.a11y.Bus.GetAddress"
  A11Y_BUS="$address"
  # The launcher starts with the accessibility flags off: under this run's env -i
  # and private HOME there is no desktop setting that turns them on. The run turns
  # them on for its own bus, as a user with a screen reader has them, before any
  # app starts; an app started then holds a connection on the AT-SPI bus from its
  # start, which note_a11y_bus notes.
  local flag state
  for flag in IsEnabled ScreenReaderEnabled; do
    timeout 5 gdbus call --address "$SESSION_BUS" --dest org.a11y.Bus --object-path /org/a11y/bus \
      --method org.freedesktop.DBus.Properties.Set org.a11y.Status "$flag" "<true>" >/dev/null \
      || fail "could not turn on org.a11y.Status.$flag on the private bus"
    state=$(timeout 5 gdbus call --address "$SESSION_BUS" --dest org.a11y.Bus --object-path /org/a11y/bus \
      --method org.freedesktop.DBus.Properties.Get org.a11y.Status "$flag" 2>/dev/null || true)
    case "$state" in *"<true>"*) ;; *) fail "org.a11y.Status.$flag is not on the private bus (got '$state')" ;; esac
  done
  echo "OK: private session bus and AT-SPI bus for this run (not the user's); accessibility flags on"
}

# What the run can observe of the AT-SPI bus: a read-only busctl listing of the
# connections this app holds on the run's own AT-SPI bus. The app connects at start
# (it activates the registry on that bus), so a connection with its pid shows the
# app is on the run's bus. The registry is activated once per bus, so the bus's log
# cannot be used for this: a second app in the same run is not logged. It does not
# show that accesskit's adapter (the one the duplicate-id guard needs) is active: a
# screen-reader flag change did not open a connection in three probes, so the guard
# is not proven by this run (see the header).
note_a11y_bus() {
  local count=0
  for _ in $(seq 1 50); do
    count=$(timeout 10 busctl --address="$A11Y_BUS" list --no-pager 2>/dev/null | awk -v pid="$APP_PID" '$2 == pid' | wc -l)
    [ "$count" -ge 1 ] && break
    sleep 0.2
  done
  if [ "$count" -ge 1 ]; then
    echo "NOTE: the run's AT-SPI bus holds $count connection(s) of sirio (pid $APP_PID)"
  else
    echo "NOTE: the run's AT-SPI bus holds no connection of sirio (pid $APP_PID)"
  fi
}

# A real liveness check: the app is the script's child, so a crashed app is a
# zombie that kill -0 still reports as alive.
assert_alive() { # what
  local state
  state=$(ps -o stat= -p "$APP_PID" 2>/dev/null | tr -d ' ' || true)
  case "$state" in
    ""|Z*) fail "Sirio is not running after $1 (state '${state:-gone}')" ;;
  esac
}

# ---- launching an isolated Sirio ------------------------------------------
# The environment every Sirio is started with: `env -i` and this allowlist, so
# nothing of the user's reaches it (no ZDOTDIR, CLAUDE_CONFIG_DIR, CODEX_HOME,
# GIT_*, SSH_AUTH_SOCK, forge tokens, MISE_*, a D-Bus session bus, the user's
# XDG_RUNTIME_DIR). Each entry is needed:
#   PATH               the stub agent CLIs first, then the system directories
#   HOME, XDG_*        private: the login shell's files and Sirio's config, data,
#                      state and cache live under the run dir
#   XDG_RUNTIME_DIR    private, mode 700. With no user bus under it, `systemctl --user`
#                      does not answer, so the detach code (sirio_host_client
#                      detach.rs) does not pick systemd-run and starts sirio-host
#                      with setsid, in a session of its own, inside this run
#   SHELL              /bin/bash: the login shell Sirio reads PATH from, and the
#                      shell of the terminal panes
#   TERM               the pane's terminal type (libghostty-vt reads the terminfo name)
#   LANG (LC_ALL)      text and fonts; LC_ALL only when the user has set it
#   DISPLAY, GPUI_X11_SCALE_FACTOR   the window (captures and input only)
#   SIRIO_SOCKET, SIRIO_DB, SIRIO_CREDENTIALS, SIRIO_HOST_HOME
#                      the run's own control socket, database, credential file and
#                      host root (SIRIO_HOST_HOME is the only variable that moves
#                      the host's root)
#   SIRIO_UPDATE_E2E_BIN, SIRIO_UPDATE_E2E_MARKER, SIRIO_UPDATE_E2E_RUN
#                      the stand-in AppImage's hooks and this run's marker, which
#                      cleanup uses to find every process the run started
#   APPIMAGE           the stand-in AppImage; on Linux Sirio finds the file it
#                      replaces through this variable (sirio_apply self-locate)
# The environment is written to <name>-expected-env.log; stage chain checks the
# relaunched Sirio's /proc/<pid>/environ against it.
launch() { # name
  local name=$1 probes_before
  probes_before=$(stub_probe_count)
  local home="$RUN_DIR/$name-home" data="$RUN_DIR/$name-data" state="$RUN_DIR/$name-state"
  local cache="$RUN_DIR/$name-cache" runtime="$RUN_DIR/$name-runtime"
  local appimage="$RUN_DIR/$name-install/Sirio.AppImage" marker="$RUN_DIR/$name.marker"
  mkdir -p "$home/.config" "$state" "$cache" "$data" "$(dirname "$appimage")"
  mkdir -m 700 "$runtime"
  printf '#!/bin/sh\nexec "$SIRIO_UPDATE_E2E_BIN" "$@"\n' > "$appimage"
  chmod +x "$appimage"
  if [ -n "$APPEARANCE" ]; then
    (cd "$ROOT/rust" && CARGO_TARGET_DIR="$TARGET" cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$RUN_DIR/$name.sqlite" --appearance "$APPEARANCE") >>"$RUN_DIR/build.log" 2>&1 || fail "could not seed the appearance"
    # appearance_seed saves updates.enabled=false (the forge UI runs must not
    # poll), and with checks off the card can never show. Turn them back on in
    # this run's own database, where the setting lives in table setting(key, value).
    local updates
    updates=$(python3 - "$RUN_DIR/$name.sqlite" <<'PY'
import sqlite3, sys
con = sqlite3.connect(sys.argv[1])
con.execute("INSERT INTO setting (key, value) VALUES ('updates.enabled', 'true') ON CONFLICT(key) DO UPDATE SET value = excluded.value")
con.commit()
print(con.execute("SELECT value FROM setting WHERE key = 'updates.enabled'").fetchone()[0])
PY
)
    [ "$updates" = true ] || fail "could not turn update checks on in the run's database (updates.enabled=$updates)"
  fi
  local pairs=(
    "PATH=$APP_PATH"
    "HOME=$home"
    "XDG_CONFIG_HOME=$home/.config"
    "XDG_DATA_HOME=$data"
    "XDG_STATE_HOME=$state"
    "XDG_CACHE_HOME=$cache"
    "XDG_RUNTIME_DIR=$runtime"
    "SHELL=/bin/bash"
    "TERM=${TERM:-xterm-256color}"
    "LANG=${LANG:-C.UTF-8}"
  )
  if [ -n "${LC_ALL:-}" ]; then pairs+=("LC_ALL=$LC_ALL"); fi
  pairs+=(
    "SIRIO_SOCKET=$RUN_DIR/$name.sock"
    "SIRIO_DB=$RUN_DIR/$name.sqlite"
    "SIRIO_CREDENTIALS=$RUN_DIR/$name-credentials.json"
    "SIRIO_HOST_HOME=$RUN_DIR/$name-host"
    "SIRIO_UPDATE_E2E_BIN=$BIN"
    "SIRIO_UPDATE_E2E_MARKER=$marker"
    "SIRIO_UPDATE_E2E_RUN=$RUN_DIR"
    "APPIMAGE=$appimage"
  )
  if [ "$STATE_ONLY" -eq 0 ]; then
    start_private_bus
    # DISPLAY names the run's private Xvfb only. The bus pair is the run's own:
    # the session bus for the app's D-Bus clients, and the AT-SPI bus that
    # accesskit connects to (sirio-host detaches under setsid for want of a user
    # systemd manager on this run's XDG_RUNTIME_DIR).
    pairs+=("DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1"
            "DBUS_SESSION_BUS_ADDRESS=$SESSION_BUS" "AT_SPI_BUS_ADDRESS=$A11Y_BUS")
  fi
  export SIRIO_SOCKET="$RUN_DIR/$name.sock"
  [ "${#SIRIO_SOCKET}" -lt 108 ] || fail "the control socket path is too long for a unix socket: $SIRIO_SOCKET"
  printf '%s\n' "${pairs[@]}" > "$RUN_DIR/$name-expected-env.log"
  # python3 makes the app the leader of its own process group (pgid = pid) before
  # it execs, so the cleanup can stop the group. Started from the run's own
  # directory, not the repository this script lives in: the app's launch
  # directory becomes a worktree of its session.
  (cd "$RUN_DIR" && exec python3 -c 'import os, sys; os.setpgid(0, 0); os.execvp(sys.argv[1], sys.argv[1:])' \
    env -i "${pairs[@]}" "$appimage" >>"$RUN_DIR/$name-app.log" 2>&1) &
  APP_PID=$!
  RUN_PIDS+=("$APP_PID")
  local started
  started=$(date +%s%N)
  for _ in $(seq 1 75); do [ -S "$SIRIO_SOCKET" ] && break; sleep 0.2; done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket ($name)"
  echo "OK: the control socket of $name opened $(( ($(date +%s%N) - started) / 1000000 )) ms after the wait began"
  assert_stub_probe "$name" "$probes_before"
  check_host_detached "$name"
}

# The app's claude probe reached the stub: the stub writes stub-calls.log, so the
# app's PATH resolution picked the stub (stub-first, observed).
# The count of `claude --version` lines the stub has recorded. Each launch of Sirio
# probes claude once, so a launch shows as a line after the count taken before it.
stub_probe_count() {
  local count
  count=$(grep -c '^claude --version$' "$RUN_DIR/stub-calls.log" 2>/dev/null || true)
  echo "${count:-0}"
}

# The app's claude probe reached the stub after this launch began: the stub writes
# stub-calls.log, so a new line is the app's PATH resolution picking the stub
# (stub-first, observed). Required after every launch, in both stages.
assert_stub_probe() { # name probes-before
  for _ in $(seq 1 150); do
    if [ "$(stub_probe_count)" -gt "$2" ]; then
      echo "OK: $1: the app's claude --version probe reached the stub (a new stub-calls.log line)"
      return 0
    fi
    sleep 0.2
  done
  fail "$1: the app never asked the stub claude for its version after this launch"
}

# The packaged sirio-host starts when the app finds one beside its binary, and
# the detach code then makes it a setsid session of its own. Optional here: an
# absent host is reported, not failed. A started one must be a session leader.
check_host_detached() { # name
  local name=$1 host_dir="$RUN_DIR/$name-host" pid="" session=""
  for _ in $(seq 1 50); do
    pid=$(python3 -c '
import glob, json, sys
for path in glob.glob(sys.argv[1] + "/host-v*.json"):
    try:
        print(json.load(open(path))["pid"])
        break
    except (OSError, ValueError, KeyError):
        pass
' "$host_dir" 2>/dev/null || true)
    [ -n "$pid" ] && break
    sleep 0.2
  done
  if [ -z "$pid" ]; then
    echo "NOTE: no sirio-host seen within 10 s for $name; not checked further"
    return 0
  fi
  cp "$host_dir"/host-v*.json "$RUN_DIR/$name-host-state.log" 2>/dev/null || true
  session=$(ps -o sid= -p "$pid" 2>/dev/null | tr -d ' ' || true)
  if [ "$session" = "$pid" ]; then
    echo "NOTE: $name: sirio-host $pid is a session leader (setsid, no systemd scope)"
  else
    echo "NOTE: $name: sirio-host $pid is not a session leader (session '${session:-gone}')"
  fi
}

# Waits until a pid is gone or a zombie (the reaper is the script's shell).
wait_gone() { # pid
  local pid=$1 state=""
  for _ in $(seq 1 150); do
    state=$(awk '{print $3}' "/proc/$pid/stat" 2>/dev/null || echo gone)
    if [ "$state" = gone ] || [ "$state" = Z ]; then
      echo "OK: the old pid $pid has exited ($state)"
      return 0
    fi
    sleep 0.2
  done
  fail "the old pid $pid is still running ($state) after the restart"
}

stage_chain() {
  echo "== stage chain: the real check, download, swap and relaunch"
  launch chain
  local project="$RUN_DIR/chain-project"
  mkdir -p "$project"
  git -C "$project" init -q -b main
  ctl project add "$project"
  ctl select-workspace --workspace "$project"
  ctl panel create --worktree "$project" >/dev/null
  wait_for visible true surface update read
  expect title "Sirio $VERSION is available"
  expect buttons "later:on,restart:on"
  capture chain-available
  local before after
  before=$(read_field pid surface update read)
  local probes_before_restart
  probes_before_restart=$(stub_probe_count)
  ctl surface update act --action restart
  after=""
  for _ in $(seq 1 200); do
    after=$(reply surface update read | field pid 2>/dev/null || true)
    [ -n "$after" ] && [ "$after" != "$before" ] && break
    sleep 0.3
  done
  [ -n "$after" ] && [ "$after" != "$before" ] || fail "no new Sirio answered after the restart (before: $before)"
  echo "OK: relaunched, pid $before -> $after"
  wait_gone "$before"
  # The old pid is gone, so the cleanup no longer signals it.
  local kept=() pid
  for pid in ${RUN_PIDS[@]+"${RUN_PIDS[@]}"}; do [ "$pid" = "$before" ] || kept+=("$pid"); done
  RUN_PIDS=(${kept[@]+"${kept[@]}"})
  APP_PID=$after
  RUN_PIDS+=("$after")
  for _ in $(seq 1 150); do [ "$(stub_probe_count)" -gt "$probes_before_restart" ] && break; sleep 0.2; done
  [ "$(stub_probe_count)" -gt "$probes_before_restart" ] || fail "the relaunched Sirio never asked the stub claude for its version"
  echo "OK: the relaunched Sirio's claude --version probe reached the stub"
  [ -f "$RUN_DIR/chain.marker" ] || fail "the relaunched Sirio is not the new artifact (no marker)"
  echo "OK: the new artifact ran"
  grep -q "\[update\] status: downloading $VERSION" "$RUN_DIR/chain-app.log" || fail "no Downloading in the app log"
  grep -q "\[update\] status: installing $VERSION" "$RUN_DIR/chain-app.log" || fail "no Installing in the app log"
  grep -q "\[update\] status: installed $VERSION" "$RUN_DIR/chain-app.log" || fail "no Installed in the app log"
  echo "OK: Downloading -> Installing -> Installed"
  local key value got
  : > "$RUN_DIR/chain-relaunched-env.log"
  while IFS='=' read -r key value; do
    got=$(environment_of "$after" "$key")
    printf '%s=%s\n' "$key" "$got" >> "$RUN_DIR/chain-relaunched-env.log"
    if [ "$key" = PATH ]; then
      # Sirio merges the login shell's PATH in front of the one it was given
      # (login_path.rs, at every start), so the stub directory must lead the
      # value, not be the whole of it.
      case "$got" in
        "$RUN_DIR/bin:"*) ;;
        *) fail "the relaunched Sirio's PATH does not lead with the stub directory" ;;
      esac
    else
      [ "$got" = "$value" ] || fail "the relaunched Sirio does not carry $key"
    fi
  done < "$RUN_DIR/chain-expected-env.log"
  echo "OK: the relaunched Sirio's PATH leads with the stub directory; every other variable is the run's own (chain-relaunched-env.log)"
  reply project list | grep -q "$project" || fail "the project did not survive the restart"
  echo "OK: the project survived the restart"
  # The layout comes back: the worktree selected before the restart is the
  # restored, selected and mounted one. A terminal made over the control socket
  # is not part of the saved layout (panel list is empty after a restart, measured
  # in chain-stubs-3), so no terminal is asserted here.
  local restored="" listing=""
  for _ in $(seq 1 100); do
    listing=$(reply list-workspaces || true)
    restored=$(printf '%s' "$listing" | python3 -c '
import json, sys
path = sys.argv[1]
rows = json.load(sys.stdin)
print(any(row.get("path") == path and row.get("selected") == "true" and row.get("mounted") == "true" for row in rows))
' "$project" 2>/dev/null || echo False)
    [ "$restored" = True ] && break
    sleep 0.3
  done
  if [ "$restored" != True ]; then
    echo "workspaces after the restart: ${listing:-<no answer>}"
    fail "the layout did not come back: $project is not the restored, selected worktree"
  fi
  echo "OK: the restored layout selects the worktree that was selected before the restart"
  stop_process_group "$APP_PID"
  APP_PID=""
}

stage_ui() {
  echo "== stage ui: every state, Later, the indicator, the warning, the keyboard, the focus"
  # No server for this instance's check: it fails, and a failed check raises nothing.
  kill "$SERVER_PID" 2>/dev/null || true
  SERVER_PID=""
  launch ui
  wait_for status failed surface update read
  expect visible false
  # See note_a11y_bus: the bus's view of this app, not proof of accesskit's adapter.
  if [ "$STATE_ONLY" -eq 0 ]; then note_a11y_bus; fi

  # The terminal and the agent warning both live in a project.
  local project="$RUN_DIR/ui-project"
  mkdir -p "$project"
  git -C "$project" init -q -b main
  ctl project add "$project"
  ctl select-workspace --workspace "$project"

  local window="" terminal="" marker_one="focus-before-the-card-1" marker_two="focus-with-the-card-up-2"
  if [ "$STATE_ONLY" -eq 0 ]; then
    window=$(find_window)
    [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID"
    terminal=$(open_terminal "$window")
    echo "OK: the Terminal tile opened terminal $terminal"
    # The shell is ready when its prompt is on the screen; a key typed before that
    # can be echoed after a single read has already been taken. The prompt reads
    # "[user@host dir]$", and the scrollback drops its trailing space, so the
    # needle is the bare "$".
    wait_pane_text "$terminal" '$' 20
    type_marker "$window" "$terminal" "$marker_one"
  else
    echo "SKIP: keyboard (needs a display)"
    echo "SKIP: focus (needs a display)"
  fi

  ctl surface update inject --status available --version 99.1.0 --notes "Faster terminals."
  expect visible true
  expect title "Sirio 99.1.0 is available"
  # Nightly says what it is, as Settings does; the release notes are a Stable body.
  expect body "Nightly builds track main"
  capture ui-available

  if [ -n "$terminal" ]; then
    # The card is up. The focus must not have moved, so the next marker has to reach
    # the same terminal, after the first one, in the scrollback.
    type_marker "$window" "$terminal" "$marker_two"
    # Exit 3 is a sirioctl call that did not answer in time; 1 is the order check.
    order=0
    python3 -c '
import base64, subprocess, sys
try:
    text = subprocess.run(["'"$CTL"'", "panel", "read", sys.argv[1]], capture_output=True, text=True, timeout='"$CALL_TIMEOUT"').stdout
except subprocess.TimeoutExpired:
    sys.exit(3)
text = base64.b64decode(text.strip()).decode(errors="replace")
one, two = text.rfind(sys.argv[2]), text.rfind(sys.argv[3])
sys.exit(0 if 0 <= one < two else 1)
' "$terminal" "$marker_one" "$marker_two" || order=$?
    if [ "$order" -eq 3 ]; then
      timed_out "sirioctl panel read $terminal (${CALL_TIMEOUT} s)"
    fi
    [ "$order" -eq 0 ] || fail "the markers did not reach the terminal in order: the card may have taken the keys"
    echo "OK: focus stayed in the terminal: both markers reached it in order, with the card drawn between them"
  fi

  local agent
  agent=$(call_ctl panel create --worktree "$project" --cmd cat)
  [ -n "$agent" ] || fail "panel create returned no pane id"
  call_ctl notify --session "$agent" --status running
  wait_for agentsWorking 1 surface update read
  expect warning "1 agent is working — restarting will stop it."
  expect buttons "later:on,install:on,restart:on"
  capture ui-warning

  ctl surface update inject --status ready --version 99.1.0 --notes "Faster terminals."
  expect visible true
  expect title "Sirio 99.1.0 is ready"
  expect buttons "later:on,install:on,restart:on"
  capture ui-ready

  ctl surface update act --action later
  expect visible false
  ctl surface update inject --status available --version 99.1.0
  expect visible false
  echo "OK: Later holds for the same version"
  ctl surface update act --action open
  expect visible true
  echo "OK: the indicator brings it back"

  ctl surface update inject --status downloading --version 99.1.0
  expect buttons "later:off,restart:off"
  expect hideable true
  capture ui-downloading
  ctl surface update act --action hide
  expect visible false
  ctl surface update act --action open

  # The duplicate-id guard below means something only while bezel's accessibility
  # adapter is active, so that is asserted before the error captures, not noted.
  wait_for a11yActive true surface update read

  # A failed apply and a refusal, drawn with the agent still at work: the card
  # carries the agent warning and the error together (spec 2.2). Both were once
  # one a11y node id in a debug build, which panicked ("Duplicate a11y node id",
  # bezel-gpui window/a11y.rs); after each capture Sirio must answer.
  ctl surface update inject --status failed --version 99.1.0 --message "network unreachable"
  expect status apply-failed
  expect error "network unreachable"
  expect warning "1 agent is working — restarting will stop it."
  expect buttons "later:on,retry:on"
  capture ui-failed
  # The guard: a debug build panics on a duplicate a11y node id while the card is
  # drawn, and the panic ends the process, so the next ping has no one to answer.
  ctl ping
  assert_alive "the failed card was drawn"

  ctl surface update inject --status refused --version 99.1.0
  expect buttons "later:on"
  expect warning "1 agent is working — restarting will stop it."
  read_field error surface update read | grep -q "github.com/ai-sirio/sirio/releases" || fail "the refusal does not name the download page"
  capture ui-refused
  ctl ping
  assert_alive "the refused card was drawn"

  ctl surface update inject --status installed --version 99.1.0
  expect title "Sirio 99.1.0 is installed — restart to use it"
  expect buttons "later:on,restart:on"
  capture ui-installed
}

case "$STAGE" in
  chain) stage_chain ;;
  ui) stage_ui ;;
  all) stage_chain; stage_ui ;;
esac
echo "UPDATE UI E2E OK"
