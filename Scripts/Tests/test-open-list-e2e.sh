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
# agent CLI resolves and no user configuration is read, and with
# SIRIO_HOST_BIN naming a missing file, so its terminals use the in-process
# fallback and no detached host outlives the run.
#
# The artifact: --out-dir DIR (default artifacts/open-list-e2e-<stamp>-<pid>)
# keeps transcript.log, app.log and -- unless --state-only -- PID-matched
# window captures in frames/. Run once with --appearance light and once with
# --appearance dark for both captures.
#
# Usage: Scripts/Tests/test-open-list-e2e.sh [--state-only] [--out-dir DIR] [--display :N]
#          [--appearance light|dark]

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
STATE_ONLY=0
OUT_DIR=""
APPEARANCE=""
DISPLAY_TARGET="${DISPLAY:-}"
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
cleanup() {
  [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; }
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl)

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
reply() { "$CTL" "$@" --json; }
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
read_field() { # key sirioctl-args...
  local key=$1
  shift
  reply "$@" | field "$key" || fail "sirioctl $* did not answer (reading $key)"
}
wait_for() { # key value sirioctl-args...
  local key=$1 want=$2
  shift 2
  local got=""
  for _ in $(seq 1 100); do
    got=$(reply "$@" | field "$key" || true)
    [ "$got" = "$want" ] && { echo "OK: $key=$want"; return 0; }
    sleep 0.3
  done
  reply "$@" || true
  fail "$key never became '$want' (last: '$got') for: $*"
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
    (cd "$WT" && exec env -u DISPLAY -u WAYLAND_DISPLAY "${APP_ENV[@]}" "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
  else
    (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 "${APP_ENV[@]}" "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
  fi
  APP_PID=$!
  for _ in $(seq 1 75); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.2
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
}
quit_app() {
  ctl quit
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}
first_secondary() { read_field secondaryPositions surface layout read | cut -d, -f1; }

launch_app
ctl project add "$WT"
ctl select-workspace --workspace "$WT"

echo "step 1: at launch the centre is one pane"
wait_for secondaryTabs 0 surface layout read
wait_for secondaryVisible false surface layout read
capture launch

echo "step 2: the Open list, and Chat with no agent on PATH"
ctl surface open-list open --entry show
wait_for rightPanelView open surface layout read
capture open-list
ctl surface open-list open --entry chat
wait_for openListChatExpanded true surface layout read
wait_for openListAgents "" surface layout read
capture open-list-chat
ctl surface open-list open --entry chat
wait_for openListChatExpanded false surface layout read

echo "step 3: a terminal opens a Secondary pane, and takes it away when closed"
ctl surface open-list open --entry terminal
wait_for secondaryVisible true surface layout read
wait_for secondaryKinds terminal surface layout read
capture with-terminal
ctl surface tabs close "$(first_secondary)" --confirm
wait_for secondaryTabs 0 surface layout read
wait_for secondaryVisible false surface layout read

echo "step 4: a Secondary tab survives quit and relaunch"
ctl surface open-list open --entry changes
wait_for secondaryKinds diff surface layout read
quit_app
launch_app
ctl select-workspace --workspace "$WT"
wait_for secondaryKinds diff surface layout read
wait_for secondaryVisible true surface layout read
capture relaunched-with-changes

echo "step 5: an emptied pane stays away after quit and relaunch"
ctl surface tabs close "$(first_secondary)"
wait_for secondaryTabs 0 surface layout read
quit_app
launch_app
ctl select-workspace --workspace "$WT"
wait_for secondaryTabs 0 surface layout read
wait_for secondaryVisible false surface layout read
capture relaunched-empty

echo "step 6: an agent the list does not offer is refused"
"$CTL" surface open-list open --entry agent:nonexistent >/dev/null 2>&1 \
  && fail "an unknown agent was accepted" || echo "OK: an unknown agent is refused"

echo "OPEN LIST E2E OK"
