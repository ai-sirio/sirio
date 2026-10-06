#!/bin/bash
set -euo pipefail

# End-to-end test of the change request surfaces: a real, isolated Sirio
# against the loopback fake forge (Scripts/Tests/fake_forge.py), driven over
# the control socket -- the right panel's view first not signed in, a token
# saved through the same verification the view's own field uses (sent with a
# trailing newline, which must be trimmed), a filter, the detail tab and one
# of its inner tabs. Then search and clear, a filter during a rate-limit
# pause and its automatic resume, a pause with no reset, and an unknown
# forge. The fake's /__ratelimit?seconds=N, /__throttle and /__reset hooks
# control the two pauses; a second fake (--flavor none) cannot be identified.
#
# The artifact: --out-dir DIR (default artifacts/forge-ui-e2e-<stamp>-<pid>)
# keeps transcript.log, app.log, the fake forge's request log and -- unless
# --state-only -- PID-matched window captures in frames/. Rerunning the
# script reproduces it.
#
# Usage: Scripts/Tests/test-forge-ui-e2e.sh [--state-only] [--out-dir DIR] [--display :N]
#          [--appearance light|dark]   draw in that mode (default: the app's own)
#          [--window-size WxH]         resize the app's window before each capture (needs xdotool)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
STATE_ONLY=0
OUT_DIR=""
DISPLAY_TARGET="${DISPLAY:-}"
APPEARANCE=""
WINDOW_SIZE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    --window-size) WINDOW_SIZE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/forge-ui-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
if [ "$STATE_ONLY" -eq 0 ]; then
  [ -n "$DISPLAY_TARGET" ] || fail "no DISPLAY; pass --display :N or --state-only"
  for tool in import identify xwininfo xprop; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi

RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-forge-ui-XXXXXX")
APP_PID=""
FORGE_PID=""
NONE_PID=""
cleanup() {
  [ -z "$APP_PID" ] || kill "$APP_PID" 2>/dev/null || true
  [ -z "$FORGE_PID" ] || kill "$FORGE_PID" 2>/dev/null || true
  [ -z "$NONE_PID" ] || kill "$NONE_PID" 2>/dev/null || true
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl)

PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
python3 "$ROOT/Scripts/Tests/fake_forge.py" --flavor github --port "$PORT" --log "$RUN_DIR/github-requests.log" &
FORGE_PID=$!
NONE_PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
python3 "$ROOT/Scripts/Tests/fake_forge.py" --flavor none --port "$NONE_PORT" --log "$RUN_DIR/none-requests.log" &
NONE_PID=$!
for _ in $(seq 1 50); do
  curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$PORT/api/v3/meta" && break
  sleep 0.2
done

FIXTURE="$RUN_DIR/widgets"
mkdir -p "$FIXTURE"
git -C "$FIXTURE" init -q -b main
git -C "$FIXTURE" config user.email t@example.com
git -C "$FIXTURE" config user.name Tester
git -C "$FIXTURE" commit -q --allow-empty -m first
git -C "$FIXTURE" checkout -q -b feat/login
git -C "$FIXTURE" remote add origin https://ghe.test/acme/widgets.git

export SIRIO_SOCKET="$RUN_DIR/control.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
if [ -n "$APPEARANCE" ]; then
  (cd "$ROOT/rust" && cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$SIRIO_DB" --appearance "$APPEARANCE") || fail "could not seed the appearance"
fi
export SIRIO_CREDENTIALS="$RUN_DIR/credentials.json"
export SIRIO_FORGE_TEST_ENDPOINTS="ghe.test=http://127.0.0.1:$PORT,git.unknown.test=http://127.0.0.1:$NONE_PORT"
export GH_CONFIG_DIR="$RUN_DIR/gh" GLAB_CONFIG_DIR="$RUN_DIR/glab"
mkdir -p "$GH_CONFIG_DIR" "$GLAB_CONFIG_DIR"
chmod 700 "$GLAB_CONFIG_DIR"
unset GH_TOKEN GITHUB_TOKEN GITLAB_TOKEN HTTP_PROXY HTTPS_PROXY http_proxy https_proxy || true

if [ "$STATE_ONLY" -eq 1 ]; then
  (cd "$FIXTURE" && exec env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" >"$RUN_DIR/app.log" 2>&1) &
else
  (cd "$FIXTURE" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 "$BIN" >"$RUN_DIR/app.log" 2>&1) &
fi
APP_PID=$!
for _ in $(seq 1 75); do
  [ -S "$SIRIO_SOCKET" ] && break
  sleep 0.2
done
[ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
reply() { "$CTL" "$@" --json; }
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
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

asked() { # how many list reads the github fake forge has seen
  grep -cE " (ChangeRequestList|ChangeRequestSearch|ChangeRequestMine) " "$RUN_DIR/github-requests.log" || true
}
contains() { # key needle sirioctl-args...
  local key=$1 needle=$2
  shift 2
  local got
  got=$(reply "$@" | field "$key")
  case "$got" in *"$needle"*) echo "OK: $key ~ $needle" ;; *) fail "$key '$got' does not contain '$needle'" ;; esac
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
  if [ -n "$WINDOW_SIZE" ]; then
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowsize "$window" "${WINDOW_SIZE%x*}" "${WINDOW_SIZE#*x}" || fail "could not resize the window"
    sleep 2
  fi
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}

ctl project add "$FIXTURE"
ctl select-workspace --workspace "$FIXTURE"

echo "step 1: the view on an unknown enterprise host, with no CLI and no token, asks for a sign-in"
ctl surface change-requests show
wait_for state not-connected surface change-requests read
wait_for host ghe.test surface change-requests read
capture not-connected

echo "step 2: a token (sent with a trailing newline) is verified, kept, and the list loads"
account=$(reply surface change-requests token --host ghe.test --forge github --token $'good\n' | field account)
[ "$account" = "fake-user" ] || fail "the token signed in as '$account'"
# The store maps "forge:<host>" to a JSON string that itself holds
# {"forge", "token"}: decode both levels rather than grep escaped text.
python3 - "$SIRIO_CREDENTIALS" <<'PY' || fail "the token was not kept, or kept untrimmed"
import json, sys
stored = json.loads(json.load(open(sys.argv[1]))["forge:ghe.test"])
assert stored == {"forge": "github", "token": "good"}, stored
PY
wait_for state ready surface change-requests read
wait_for labels "#101,#102" surface change-requests read
wait_for card "#112" surface change-requests read
capture list

echo "step 3: a filter lists its own change requests"
ctl surface change-requests filter mine
wait_for labels "#105,#103,#106" surface change-requests read
capture mine

echo "step 4: the detail tab loads the conversation, then the checks"
ctl surface change-request open 101
wait_for state loaded surface change-request read
wait_for title "Fix the login redirect" surface change-request read
wait_for conversation_threads 6 surface change-request read
wait_for rows 11 surface change-request read
capture detail-conversation
ctl surface change-request tab checks
wait_for state loaded surface change-request read
wait_for rows 4 surface change-request read
capture detail-checks

echo "step 5: a search that matches nothing says so, and clearing it lists again"
ctl surface change-requests filter all-open
wait_for labels "#101,#102" surface change-requests read
ctl surface change-requests search --text "zzzz-nothing"
wait_for rows 0 surface change-requests read
wait_for state ready surface change-requests read
capture empty
ctl surface change-requests search --text ""
wait_for labels "#101,#102" surface change-requests read

echo "step 6: a filter chosen while rate limited says so, and the list looks again when the pause ends"
curl -s -o /dev/null -X POST "http://127.0.0.1:$PORT/__ratelimit?seconds=6"
ctl surface change-requests filter to-review
wait_for state error surface change-requests read
contains error "rate limited" surface change-requests read
before=$(asked)
ctl surface change-requests filter all-open
wait_for state error surface change-requests read
[ "$(asked)" = "$before" ] || fail "the list asked the forge while paused"
capture rate-limited
wait_for labels "#101,#102" surface change-requests read

echo "step 7: a rate limit with no reset time pauses too"
curl -s -o /dev/null -X POST "http://127.0.0.1:$PORT/__throttle"
ctl surface change-requests filter mine
wait_for state error surface change-requests read
before=$(asked)
ctl surface change-requests filter all-open
sleep 1
[ "$(asked)" = "$before" ] || fail "a rate limit without a reset did not pause the list"
curl -s -o /dev/null -X POST "http://127.0.0.1:$PORT/__reset"

echo "step 8: a host Sirio cannot identify asks which forge it is"
UNKNOWN="$RUN_DIR/tools"
mkdir -p "$UNKNOWN"
git -C "$UNKNOWN" init -q -b main
git -C "$UNKNOWN" -c user.email=t@example.com -c user.name=Tester commit -q --allow-empty -m first
git -C "$UNKNOWN" remote add origin https://git.unknown.test/acme/tools.git
ctl project add "$UNKNOWN"
ctl select-workspace --workspace "$UNKNOWN"
ctl surface change-requests show
wait_for state unknown-forge surface change-requests read
wait_for host git.unknown.test surface change-requests read
capture unknown-host

echo "artifact: $OUT_DIR"
echo "FORGE UI E2E OK"
