#!/bin/bash
set -euo pipefail

# Native check of the code editor's hosting hooks: a per-line highlighter, git
# marks, a diagnostic, view options, no breakpoints and the events a host
# listens to. rust/crates/sirio_ui/examples/ely_editor_probe.rs draws them with
# Sirio's theme on an isolated X display; this script aims the pointer from the
# geometry the probe prints.
#
# It proves real pixels (a frame needs at least 200 colours and an exact
# #ff00ff, the highlighter's colour, on line 3), the right window (input and
# captures go only to the window whose _NET_WM_PID is the probe's), and these
# events, each from a real xdotool gesture: Pointer over line 3 with an offset
# inside that line; GitMarkPressed for the git bar of line 5; no breakpoint
# toggle from a click in the breakpoint slot of line 7 (the breakpoint set
# stays empty); SecondaryClick from a right click on line 3; PlatformClick from
# a Super-held click on line 3 at its own offset. Not covered here: `reset`
# and the view options' effect on the frame (see docs/testing/).
#
# The artifact: --out-dir DIR (default ${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}/ely-editor-probe)
# keeps transcript.log, probe-main.log and probe-breakpoints-on.log (the
# control run, which keeps the breakpoint slot live), xvfb.log and frames/*.png.
# Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-ely-editor-probe.sh [--xvfb | --display :N]
#          [--no-software-vulkan] [--out-dir DIR] [--no-build]
#   --xvfb                start a private Xvfb on a free display and stop it on exit
#   --no-software-vulkan  do not force Mesa's lavapipe (on Xvfb, frames are then blank)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROBE="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/examples/ely_editor_probe"
LVP=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
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
[ -n "$OUT_DIR" ] || OUT_DIR="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}/ely-editor-probe"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
for tool in python3 xwininfo xprop xdotool import identify magick; do
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
  echo "building ely_editor_probe"
  (cd "$ROOT/rust" && cargo build --quiet -p sirio_ui --example ely_editor_probe)
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
LOG=""
launch() { # run-name breakpoints (off, or on for the control run)
  LOG="$OUT_DIR/probe-$1.log"
  : >"$LOG"
  # env execs the probe, so $! is the probe's own pid, the one in _NET_WM_PID.
  env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" "${VULKAN_ENV[@]}" \
    ELY_PROBE_APPEARANCE=dark ELY_PROBE_UI_SIZE=13 ELY_PROBE_BREAKPOINTS="$2" "$PROBE" >"$LOG" 2>&1 &
  PROBE_PID=$!
  WINDOW=""
  for _ in $(seq 1 100); do
    kill -0 "$PROBE_PID" 2>/dev/null || fail "probe exited early; see $LOG"
    WINDOW=$(find_window)
    [ -n "$WINDOW" ] && break
    sleep 0.2
  done
  [ -n "$WINDOW" ] || fail "no window with _NET_WM_PID=$PROBE_PID"
  for _ in $(seq 1 100); do grep -q '^geometry ' "$LOG" && break; sleep 0.1; done
  grep -q '^geometry ' "$LOG" || fail "the probe printed no geometry; see $LOG"
  echo "probe: pid $PROBE_PID, window $WINDOW"
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
xd() { DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool "$@"; }

# The targets, from the geometry the probe printed (see its header).
# Prints KEY=VALUE lines for eval: the pixel targets and the byte range of line 3.
targets() {
  python3 - "$LOG" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
geometry = re.search(r"^geometry advance=([\d.]+) line_height=([\d.]+) digits=(\d+) gutter_columns=(\d+)$", text, re.M)
line3 = re.search(r"^line 3 bytes (\d+)\.\.(\d+)$", text, re.M)
if not (geometry and line3):
    sys.exit("the probe's geometry or line 3 is missing")
advance, line_height = float(geometry.group(1)), float(geometry.group(2))
columns = int(geometry.group(4))
row = lambda n: round(n * line_height + line_height / 2)
# gutter.rs: slot [0, 2a), digits, chevron [4a, 6a), git bar at 6a (a 4px hit area).
print(f"SLOT_X={round(advance)}")
print(f"BAR_X={round(6 * advance + 2)}")
# The text starts one advance past the gutter, so column c is centred at (columns + 1 + c + 0.5) advances.
print(f"TEXT_X={round((columns + 1 + 12 + 0.5) * advance)}")
print(f"ROW3={row(3)}")
print(f"ROW5={row(5)}")
print(f"ROW7={row(7)}")
print(f"LINE3_START={line3.group(1)}")
print(f"LINE3_END={line3.group(2)}")
PY
}

# Waits for a new transcript line matching a regex whose first group is a number
# in LO..HI: BEFORE counts the matches already in the transcript. Exits 0 on a match.
EVENT_PY='
import re, sys, time
log, pattern, before, lo, hi, timeout = sys.argv[1], re.compile(sys.argv[2], re.M), int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5]), float(sys.argv[6])
end = time.time() + timeout
while time.time() < end:
    hits = pattern.findall(open(log).read())
    if len(hits) > before:
        value = int(hits[-1])
        if lo <= value <= hi:
            print(f"EVENT: {value} within {lo}..{hi}")
            sys.exit(0)
        print(f"EVENT: {value} outside {lo}..{hi}")
        sys.exit(1)
    time.sleep(0.2)
print("EVENT: none arrived")
sys.exit(1)
'
echo "step 1: as drawn, with the highlighter's colour on line 3"
launch main off
eval "$(targets)"
echo "targets: slot x=$SLOT_X, git bar x=$BAR_X, text x=$TEXT_X, rows 3/5/7 at y=$ROW3/$ROW5/$ROW7, line 3 bytes $LINE3_START..$LINE3_END"
xd mousemove --window "$WINDOW" 600 580   # a neutral spot, away from every target
capture initial
timeout 30 magick "$OUT_DIR/frames/initial.png" -format '%c' histogram:info:"$OUT_DIR/histogram.txt" || fail "no histogram of the frame"
grep -qi '#FF00FF' "$OUT_DIR/histogram.txt" || fail "the frame has no #ff00ff pixel: the highlighter's colour did not draw"
echo "COLOUR: #ff00ff is in the frame"

echo "step 2: a pointer over line 3 reports an offset inside that line"
before=$(grep -c '^event Pointer ' "$LOG" || true)
xd mousemove --window "$WINDOW" "$TEXT_X" "$ROW3"
python3 -c "$EVENT_PY" "$LOG" '^event Pointer offset=(?:Some\()?(\d+)' "$before" "$LINE3_START" "$LINE3_END" 5 \
  || fail "Pointer over line 3 did not report an offset inside $LINE3_START..$LINE3_END"

echo "step 3: a click on line 5's git bar reports GitMarkPressed"
before=$(grep -c '^event GitMarkPressed ' "$LOG" || true)
xd mousemove --window "$WINDOW" "$BAR_X" "$ROW5" click 1
python3 -c "$EVENT_PY" "$LOG" '^event GitMarkPressed line=(\d+)$' "$before" 5 5 5 \
  || fail "the git bar of line 5 did not report GitMarkPressed line=5"

echo "step 4: a click in line 7's breakpoint slot toggles nothing"
xd mousemove --window "$WINDOW" "$SLOT_X" "$ROW7" click 1
sleep 1
[ "$(grep -c '^breakpoints ' "$LOG" || true)" -eq 1 ] || fail "a breakpoint was toggled: $(grep '^breakpoints ' "$LOG" | tail -n 1)"
grep -qx 'breakpoints {}' "$LOG" || fail "the breakpoint set is not empty"
echo "BREAKPOINTS: none toggled, set is {}"

echo "step 5: a right click on line 3 reports SecondaryClick"
before=$(grep -c '^event SecondaryClick ' "$LOG" || true)
xd mousemove --window "$WINDOW" "$TEXT_X" "$ROW3" click 3
python3 -c "$EVENT_PY" "$LOG" '^event SecondaryClick offset=(?:Some\()?(\d+)' "$before" "$LINE3_START" "$LINE3_END" 5 \
  || fail "the right click on line 3 did not report SecondaryClick inside line 3"

echo "step 6: a Super-held click on line 3 reports PlatformClick at its own offset"
before=$(grep -c '^event PlatformClick ' "$LOG" || true)
xd keydown Super_L
xd mousemove --window "$WINDOW" "$TEXT_X" "$ROW3" click 1
xd keyup Super_L
python3 -c "$EVENT_PY" "$LOG" '^event PlatformClick offset=(?:Some\()?(\d+)' "$before" "$LINE3_START" "$LINE3_END" 5 \
  || fail "the Super-held click on line 3 did not report PlatformClick inside line 3"
stop

echo "step 7: control: with breakpoints on, the click in line 7's slot toggles line 7"
launch breakpoints-on on
eval "$(targets)"
xd mousemove --window "$WINDOW" "$SLOT_X" "$ROW7" click 1
for _ in $(seq 1 25); do grep -qx 'breakpoints {7}' "$LOG" && break; sleep 0.2; done
grep -qx 'breakpoints {7}' "$LOG" || fail "the same click in line 7's slot did not toggle line 7 with breakpoints on: the negative check in step 4 would prove nothing"
echo "CONTROL: the slot click toggled line 7 with breakpoints on"
stop

echo "ELY EDITOR PROBE OK"
echo "artifacts: $OUT_DIR"
