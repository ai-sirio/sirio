#!/bin/bash
set -euo pipefail

# End-to-end test of the Secondary pane on demand and the right panel's Open
# list (spec 2026-10-15-secondary-on-demand-design.md §4): a real, isolated
# debug Sirio on a real git repository, driven over the control socket. At
# launch the centre is one pane; the Open list draws, Chat expands to say no
# agent is on PATH; a terminal and Changes open in a Secondary pane that
# appears with them and goes with its last tab; a Secondary tab survives quit
# and relaunch, and an emptied pane stays away. Agent chats are proven
# in-process (`drawn_open_list_chat_opens_the_picked_agent_on_the_right`).
#
# The app runs with PATH=/usr/bin:/bin and a private HOME and XDG dirs, so no
# agent CLI resolves and no user configuration is read. It runs from a copy of
# its binary at $RUN_DIR/alone/sirio, because the host lookup takes a
# sirio-host beside the running binary (target/debug has one), and SIRIO_HOST_BIN
# names a missing file. So no host is found, started or adopted. Step 1 asserts
# that the app logged that it found no sirio-host. A captured run renders, so
# its terminal starts and must log the in-process fallback (step 3). A
# state-only app never renders, so its terminal never starts and never reaches
# the fallback: there the isolation rests on the host lookup and step 7.
# Step 7 quits the app and fails if any process naming $RUN_DIR is still alive;
# cleanup stops any sirio-host that names it before removing it.
#
# Every layout read is strict: a failed read or a missing key counts as "not
# yet", never as a match, and step 6 requires the handler's own refusal text.
#
# The artifact: --out-dir DIR (default artifacts/open-list-e2e-<stamp>-<pid>)
# keeps transcript.log, app.log and -- unless --state-only -- PID-matched
# window captures in frames/. Run once with --appearance light and once with
# --appearance dark for both captures.
#
# Unlike test-changes-e2e.sh, DISPLAY is not read from the environment: a
# captured run needs --display :N, and --state-only needs no display.
#
# Usage: Scripts/Tests/test-open-list-e2e.sh [--state-only] [--out-dir DIR] [--display :N]
#          [--appearance light|dark]

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
STATE_ONLY=0
OUT_DIR=""
APPEARANCE=""
DISPLAY_TARGET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/open-list-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
if [ "$STATE_ONLY" -eq 0 ]; then
  [ -n "$DISPLAY_TARGET" ] || fail "no DISPLAY; pass --display :N or --state-only"
  for tool in import identify xwininfo xprop; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi
command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required"

RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-open-list-e2e-XXXXXX")
APP_PID=""

# Only a process that is a sirio-host is ever signalled, as in
# test-host-sessions-e2e.sh.
is_a_host() {
  local comm
  if [ -r "/proc/$1/comm" ]; then comm="$(cat "/proc/$1/comm" 2>/dev/null)"
  else comm="$(ps -p "$1" -o comm= 2>/dev/null || true)"
  fi
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
# The sirio-host processes whose command line names this run's directory.
run_hosts() {
  local pid
  for pid in $(pgrep -f "$RUN_DIR" 2>/dev/null || true); do
    is_a_host "$pid" && echo "$pid"
  done
  return 0
}
cleanup() {
  [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; }
  # A host that names this run was started by the app, which this run never
  # expects: stop it before its data root is removed.
  local pid
  for pid in $(run_hosts); do
    kill "$pid" 2>/dev/null || true
  done
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl)

# A copy of the app with no sibling sirio-host. The host lookup would otherwise
# find target/debug/sirio-host, and the terminal would start a host.
mkdir -p "$RUN_DIR/alone"
cp "$BIN" "$RUN_DIR/alone/sirio"
APP_BIN="$RUN_DIR/alone/sirio"

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
reply() { "$CTL" "$@" --json; }
# One key of `surface layout read`. Fails unless the read answered and holds the key.
layout_field() {
  local out
  out=$(reply surface layout read 2>/dev/null) || return 1
  python3 -c 'import json, sys; print(json.load(sys.stdin)[0][sys.argv[1]])' "$1" <<<"$out" 2>/dev/null
}
# Polls one key of `surface layout read` until it equals the value. A failed
# read, or a missing key, is "not yet".
wait_for() { # key value
  local key=$1 want=$2 got=""
  for _ in $(seq 1 100); do
    if got=$(layout_field "$key"); then
      [ "$got" = "$want" ] && { echo "OK: $key=$want"; return 0; }
    fi
    sleep 0.3
  done
  reply surface layout read || true
  fail "$key never became '$want' (last: '$got')"
}
# Waits for a line in the app's log.
wait_app_log() { # text
  for _ in $(seq 1 50); do
    grep -Fq "$1" "$RUN_DIR/app.log" 2>/dev/null && return 0
    sleep 0.2
  done
  return 1
}

find_window() {
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
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window
  window=$(find_window)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $1"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}

# ---- the repository: one commit, nothing else; the Open list needs a git repository
WT="$RUN_DIR/repo"
mkdir -p "$WT" "$RUN_DIR/home" "$RUN_DIR/xdg"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
echo "# app" >"$WT/README.md"
git -C "$WT" add -A
git -C "$WT" commit -q -m first

export SIRIO_SOCKET="$RUN_DIR/control.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
export SIRIO_CREDENTIALS="$RUN_DIR/credentials.json"
if [ -n "$APPEARANCE" ]; then
  (cd "$ROOT/rust" && cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$SIRIO_DB" --appearance "$APPEARANCE") || fail "could not seed the appearance"
fi
APP_ENV=(PATH=/usr/bin:/bin HOME="$RUN_DIR/home" SHELL=/bin/bash
  XDG_CONFIG_HOME="$RUN_DIR/xdg/config" XDG_DATA_HOME="$RUN_DIR/xdg/data"
  XDG_STATE_HOME="$RUN_DIR/xdg/state" XDG_CACHE_HOME="$RUN_DIR/xdg/cache"
  SIRIO_HOST_BIN="$RUN_DIR/no-sirio-host")

launch_app() {
  rm -f "$SIRIO_SOCKET"
  if [ "$STATE_ONLY" -eq 1 ]; then
    (cd "$WT" && exec env -u DISPLAY -u WAYLAND_DISPLAY "${APP_ENV[@]}" "$APP_BIN" >>"$RUN_DIR/app.log" 2>&1) &
  else
    (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 "${APP_ENV[@]}" "$APP_BIN" >>"$RUN_DIR/app.log" 2>&1) &
  fi
  APP_PID=$!
  for _ in $(seq 1 75); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.2
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
}
# True while the app is alive. It is our child, so a zombie has exited.
app_running() {
  local state
  state=$(ps -o stat= -p "$APP_PID" 2>/dev/null | tr -d ' ') || true
  [ -n "$state" ] && [ "${state#Z}" = "$state" ]
}
quit_app() {
  ctl quit
  for _ in $(seq 1 75); do # up to 15 s
    app_running || break
    sleep 0.2
  done
  if app_running; then
    kill -9 "$APP_PID" 2>/dev/null || true
    fail "sirio did not quit"
  fi
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}
# The 1-based position of the first Secondary tab.
first_secondary() {
  local positions
  positions=$(layout_field secondaryPositions) || fail "could not read secondaryPositions"
  [ -n "$positions" ] || fail "no Secondary tab to close"
  echo "${positions%%,*}"
}

launch_app
ctl project add "$WT"
ctl select-workspace --workspace "$WT"

echo "step 1: at launch the centre is one pane, and the app found no sirio-host beside its copy"
wait_app_log "[sirio] host unavailable: sirio-host not found" \
  || fail "the app did not report that it found no sirio-host beside its copy"
echo "OK: the app found no sirio-host beside its copy"
wait_for secondaryTabs 0
wait_for secondaryVisible false
capture launch

echo "step 2: the Open list, and Chat with no agent on PATH"
ctl surface open-list open --entry show
wait_for rightPanelView open
capture open-list
ctl surface open-list open --entry chat
wait_for openListChatExpanded true
wait_for openListAgents ""
capture open-list-chat
ctl surface open-list open --entry chat
wait_for openListChatExpanded false

echo "step 3: a terminal opens a Secondary pane, and takes it away when closed"
ctl surface open-list open --entry terminal
wait_for secondaryVisible true
wait_for secondaryKinds terminal
if [ "$STATE_ONLY" -eq 0 ]; then
  # The terminal starts on its first render, which only a captured run does.
  wait_app_log "[sirio] terminal on the in-process fallback" \
    || fail "the terminal did not take the in-process fallback: no such line in app.log"
  echo "OK: the terminal took the in-process fallback"
fi
capture with-terminal
secondary=$(first_secondary)
ctl surface tabs close "$secondary" --confirm
wait_for secondaryTabs 0
wait_for secondaryVisible false

echo "step 4: a Secondary tab survives quit and relaunch"
ctl surface open-list open --entry changes
wait_for secondaryKinds diff
quit_app
launch_app
ctl select-workspace --workspace "$WT"
wait_for secondaryKinds diff
wait_for secondaryVisible true
capture relaunched-with-changes

echo "step 5: an emptied pane stays away after quit and relaunch"
secondary=$(first_secondary)
ctl surface tabs close "$secondary"
wait_for secondaryTabs 0
quit_app
launch_app
ctl select-workspace --workspace "$WT"
wait_for secondaryTabs 0
wait_for secondaryVisible false
capture relaunched-empty

echo "step 6: an agent the list does not offer is refused, with the handler's own refusal"
refusal=$("$CTL" surface open-list open --entry agent:nonexistent 2>&1 >/dev/null) \
  && fail "an unknown agent was accepted"
grep -Fq "unknown Open list entry" <<<"$refusal" || fail "the refusal was not the unknown-entry one: $refusal"
echo "OK: an unknown agent is refused"

echo "step 7: quitting leaves no process of this run behind"
quit_app
leftover=$(pgrep -f "$RUN_DIR" || true)
[ -z "$leftover" ] || fail "processes of this run survived the quit: $leftover"
echo "OK: no process of this run is left"

echo "OPEN LIST E2E OK"
