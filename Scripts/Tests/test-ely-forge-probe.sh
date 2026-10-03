#!/bin/bash
set -euo pipefail

# Native check of the Ely components the change-request surfaces use: Tabs,
# GitStatusBadge and DiffStat, drawn by rust/crates/sirio_ui/examples/
# ely_forge_probe.rs with Sirio's theme, on an isolated X display.
#
# It proves real pixels (a frame needs at least 200 colours), the right
# window (input and captures go only to the window whose _NET_WM_PID is the
# probe's), a click on a tab reaching its on_change with that tab's value, a
# disabled tab ignoring a click, the badge tooltip on hover, and dark and
# light both drawn and different. It does not prove keyboard use of the tabs
# (see docs/testing/ely-forge-probe.md).
#
# The artifact: --out-dir DIR (default artifacts/ely-forge-probe-<stamp>-<pid>)
# keeps transcript.log, probe-<appearance>.log and frames/*.png. Rerunning
# the script reproduces it.
#
# Usage: Scripts/Tests/test-ely-forge-probe.sh [--xvfb | --display :N]
#          [--no-software-vulkan] [--out-dir DIR] [--no-build]
#   --xvfb                start a private Xvfb on a free display and stop it on exit
#   --no-software-vulkan  do not force Mesa's lavapipe (on Xvfb, frames are then blank)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROBE="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/examples/ely_forge_probe"
LVP=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1)
OUT_DIR=""
DISPLAY_TARGET=""
START_XVFB=0
SOFTWARE_VULKAN=1
BUILD=1
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --no-software-vulkan) SOFTWARE_VULKAN=0; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/ely-forge-probe-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
for tool in xwininfo xprop xdotool import identify compare; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done

PROBE_PID=""
XVFB_PID=""
cleanup() {
  [ -z "$PROBE_PID" ] || kill "$PROBE_PID" 2>/dev/null || true
  [ -z "$XVFB_PID" ] || kill "$XVFB_PID" 2>/dev/null || true
}
trap cleanup EXIT

if [ "$START_XVFB" -eq 1 ]; then
  command -v Xvfb >/dev/null || fail "Xvfb is required for --xvfb"
  for n in $(seq 90 99); do
    [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ] || { DISPLAY_TARGET=":$n"; break; }
  done
  [ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
  Xvfb "$DISPLAY_TARGET" -screen 0 1280x800x24 >"$OUT_DIR/xvfb.log" 2>&1 &
  XVFB_PID=$!
  for _ in $(seq 1 50); do timeout 2 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 && break; sleep 0.1; done
  timeout 2 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
fi
[ -n "$DISPLAY_TARGET" ] || fail "no display; pass --xvfb or --display :N"
echo "display: $DISPLAY_TARGET"

VULKAN_ENV=()
if [ "$SOFTWARE_VULKAN" -eq 1 ]; then
  [ -n "$LVP" ] || fail "software Vulkan driver missing (lvp_icd*.json in /usr/share/vulkan/icd.d); install vulkan-swrast or pass --no-software-vulkan"
  VULKAN_ENV=("VK_DRIVER_FILES=$LVP" "VK_ICD_FILENAMES=$LVP")
  echo "vulkan: lavapipe"
fi

if [ "$BUILD" -eq 1 ]; then
  echo "building ely_forge_probe"
  (cd "$ROOT/rust" && cargo build --quiet -p sirio_ui --example ely_forge_probe)
fi
[ -x "$PROBE" ] || fail "no probe binary at $PROBE"

find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" PROBE_PID="$PROBE_PID" python3 - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["PROBE_PID"])
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

WINDOW=""
launch() { # appearance
  local log="$OUT_DIR/probe-$1.log"
  # env execs the probe, so $! is the probe's own pid, the one in _NET_WM_PID.
  env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" "${VULKAN_ENV[@]}" \
    ELY_PROBE_APPEARANCE="$1" ELY_PROBE_UI_SIZE=13 "$PROBE" >"$log" 2>&1 &
  PROBE_PID=$!
  WINDOW=""
  for _ in $(seq 1 100); do
    kill -0 "$PROBE_PID" 2>/dev/null || fail "probe exited early ($1); see $log"
    WINDOW=$(find_window)
    [ -n "$WINDOW" ] && break
    sleep 0.2
  done
  [ -n "$WINDOW" ] || fail "no window with _NET_WM_PID=$PROBE_PID ($1)"
  echo "probe $1: pid $PROBE_PID, window $WINDOW"
}
stop() {
  kill "$PROBE_PID" 2>/dev/null || true
  wait "$PROBE_PID" 2>/dev/null || true
  PROBE_PID=""
}
capture() { # name
  sleep 2
  [ "$(find_window)" = "$WINDOW" ] || fail "window $WINDOW no longer belongs to pid $PROBE_PID"
  timeout 30 import -display "$DISPLAY_TARGET" -window "$WINDOW" "$OUT_DIR/frames/$1.png" || fail "capture of $1 failed"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}
differs() { # a b what
  # compare prints the count first, sometimes in scientific notation; awk reads both.
  local pixels
  pixels=$(compare -metric AE "$OUT_DIR/frames/$1.png" "$OUT_DIR/frames/$2.png" null: 2>&1 | awk '{printf "%d", $1}' || true)
  [ "${pixels:-0}" -gt 0 ] || fail "$3: $1 and $2 are identical"
  echo "DIFFERS: $1 vs $2 ($pixels pixels) — $3"
}
xd() { DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool "$@"; }
click() { # x y
  xd mousemove --window "$WINDOW" "$1" "$2" click 1
}
tab_changes() { grep -c '^probe tab: ' "$OUT_DIR/probe-dark.log" || true; }
expect_tab() { # value
  local log="$OUT_DIR/probe-dark.log"
  for _ in $(seq 1 25); do
    [ "$(grep '^probe tab: ' "$log" | tail -n 1)" = "probe tab: $1" ] && { echo "TAB: $1"; return 0; }
    sleep 0.2
  done
  fail "after the click the last on_change is '$(grep '^probe tab: ' "$log" | tail -n 1)', expected 'probe tab: $1'"
}

# Layout contract with ely_forge_probe.rs: 16 px padding and gaps; badges
# y 16-56, diff stats y 72-112; strips at y 128 (main), 300 (Checks first)
# and 400 (Files first, disabled); the first tab of a strip contains x 30.
echo "step 1: dark, as drawn"
launch dark
capture dark-initial

echo "step 2: hovering the first badge shows its tooltip"
xd mousemove --window "$WINDOW" 24 36
capture dark-badge-hover
differs dark-initial dark-badge-hover "the badge tooltip"
xd mousemove --window "$WINDOW" 600 560

echo "step 3: a click on the strip whose first tab is Checks selects checks"
click 30 318
expect_tab checks
capture dark-checks
differs dark-initial dark-checks "selecting Checks"

echo "step 4: a click on the main strip's first tab selects conversation again"
click 30 146
expect_tab conversation
capture dark-conversation-again
differs dark-checks dark-conversation-again "returning to Conversation"

echo "step 5: a click on the disabled Files tab does nothing"
click 30 418
sleep 1
[ "$(tab_changes)" -eq 2 ] || fail "a click on the disabled tab reached on_change ($(tab_changes) changes)"
echo "DISABLED: files ignored"
stop

echo "step 6: light, as drawn, differs from dark"
launch light
capture light-initial
differs dark-initial light-initial "light against dark"
stop

echo "ELY FORGE PROBE OK"
echo "artifacts: $OUT_DIR"
