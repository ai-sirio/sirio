#!/bin/bash
set -euo pipefail

# Native check of the code editor's hosting hooks: a per-line highlighter, git
# marks, a diagnostic, view options, no breakpoints and the events a host
# listens to. rust/crates/sirio_ui/examples/ely_editor_probe.rs draws them with
# Sirio's theme on an isolated X display; this script aims the pointer from the
# geometry the probe prints.
#
# It proves real pixels (a frame needs at least 200 colours and an exact
# #ff00ff, the highlighter's colour, on line 3), that a highlighter keeps Ely's
# lexer out: no exact keyword colour, and the bracket run on line 4 drawn
# differently from the lexer's own run (the lexer paints brackets in Ely's
# punctuation colour; exact pixels of that grey never appear, so the two runs
# are compared), the right window (input and captures go only to the window
# whose _NET_WM_PID is the probe's), and these events, each from a real xdotool
# gesture: Pointer over line 3 with an offset inside that line; Pointer over
# line 5's git bar with no offset (the gutter is not the code); GitMarkPressed
# for that git bar; no breakpoint toggle from a click in the breakpoint slot
# of line 7 (the breakpoint set stays empty), with a control run that does
# toggle when breakpoints are on; SecondaryClick from a right click on line 3;
# no PlatformClick from a plain click on line 3, but one from a Super-held
# click on it at its own offset. Line 12, far wider than the window, scrolls
# sideways: a wheel to the right (X button 7) and Shift with the wheel down
# move what a pointer at one place over line 12 lands on further along that
# same line (the rows stay where they were), the git bar still has no offset
# while scrolled (the gutter stays put), a wheel to the left brings the line's
# start back, and End then Home move the view with the caret, so the line's
# tail and then its start sit under the pointer. Not covered here: `reset`, the view options'
# effect on the frame, and a pointer past the last row (see docs/testing/).
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
DEFAULT_OUT_DIR="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}/ely-editor-probe"
[ -n "$OUT_DIR" ] || OUT_DIR="$DEFAULT_OUT_DIR"
# A run starts from an empty out dir. The default one is removed whole; any other
# dir loses only the files this script writes, so nothing outside OUT_DIR goes.
if [ "$OUT_DIR" = "$DEFAULT_OUT_DIR" ] && [ "$(basename "$OUT_DIR")" = "ely-editor-probe" ]; then
  rm -rf -- "$OUT_DIR"
else
  rm -f -- "$OUT_DIR/transcript.log" "$OUT_DIR/probe.log" "$OUT_DIR"/probe-*.log \
    "$OUT_DIR/xvfb.log" "$OUT_DIR/histogram.txt" "$OUT_DIR"/frames/*.png
fi
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
launch() { # run-name [VAR=value ...]: a run of the probe; breakpoints stay off unless a run sets them
  local name="$1"
  shift
  LOG="$OUT_DIR/probe-$name.log"
  : >"$LOG"
  # env execs the probe, so $! is the probe's own pid, the one in _NET_WM_PID.
  env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" "${VULKAN_ENV[@]}" \
    ELY_PROBE_APPEARANCE=dark ELY_PROBE_UI_SIZE=13 ELY_PROBE_BREAKPOINTS=off "$@" "$PROBE" >"$LOG" 2>&1 &
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
# Pixels that differ between two frames inside a crop (WxH+X+Y), printed as a count.
crop_pixels_differ() { # a b geometry
  magick "$OUT_DIR/frames/$1.png" -crop "$3" +repage "$OUT_DIR/scratch-a.png"
  magick "$OUT_DIR/frames/$2.png" -crop "$3" +repage "$OUT_DIR/scratch-b.png"
  local pixels
  # compare prints the count first, sometimes in scientific notation; awk reads both.
  pixels=$(compare -metric AE "$OUT_DIR/scratch-a.png" "$OUT_DIR/scratch-b.png" null: 2>&1 | awk '{printf "%d", $1}' || true)
  rm -f "$OUT_DIR/scratch-a.png" "$OUT_DIR/scratch-b.png"
  echo "${pixels:-0}"
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
  local width height
  width=$(xwininfo -display "$DISPLAY_TARGET" -id "$WINDOW" | awk '/Width:/ {print $2}')
  height=$(xwininfo -display "$DISPLAY_TARGET" -id "$WINDOW" | awk '/Height:/ {print $2}')
  python3 - "$LOG" "$width" "$height" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
window_width = int(sys.argv[2])
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
wide = re.search(r"^line 12 bytes (\d+)\.\.(\d+)$", text, re.M)
if not wide:
    sys.exit("the probe printed no line 12")
print(f"ROW12={row(12)}")
print(f"WIDE_START={wide.group(1)}")
print(f"WIDE_END={wide.group(2)}")
# Column 20 of line 12, unscrolled; and six advances short of the minimap (12 advances, view.rs) at the right edge.
print(f"SAMPLE_X={round((columns + 1 + 20 + 0.5) * advance)}")
print(f"RIGHT_X={round(window_width - (12 + 6) * advance)}")
# The horizontal bar's thumb starts at the code cell's left edge (the gutter's right) while
# unscrolled and is far longer than 40 px here; its 6 px stroke sits 9..3 px above the window's
# bottom (the body ends there: no banner, no status row), drawn 588..594 on this Xvfb.
height = int(sys.argv[3])
print(f"THUMB_X={round(columns * advance + 40)}")
print(f"THUMB_Y={height - 9}")
hues = re.search(r"^hues keyword=(#[0-9A-Fa-f]{6}) punctuation=(#[0-9A-Fa-f]{6})$", text, re.M)
if not hues:
    sys.exit("the probe printed no hues")
print(f"KEYWORD_HEX={hues.group(1)}")
bracket = re.search(r"^brackets line=4 columns=(\d+)\.\.(\d+)$", text, re.M)
if not bracket:
    sys.exit("the probe printed no bracket run")
start, end = int(bracket.group(1)), int(bracket.group(2))
# The bracket run on line 4 sits between text columns start and end.
print(f"BRACKET_X={round((columns + 1 + start) * advance)}")
print(f"BRACKET_Y={round(4 * line_height)}")
print(f"BRACKET_W={round((end - start) * advance)}")
print(f"BRACKET_H={round(line_height)}")
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
echo "step 1: as drawn, with the highlighter's colour on line 3, and the lexer's colours kept out"
# The control run keeps code's own lexer, which paints the brackets in Ely's punctuation colour.
launch lexer-on ELY_PROBE_LEXER=on
capture lexer-on
stop
launch main
eval "$(targets)"
echo "targets: slot x=$SLOT_X, git bar x=$BAR_X, text x=$TEXT_X, rows 3/5/7 at y=$ROW3/$ROW5/$ROW7, line 3 bytes $LINE3_START..$LINE3_END"
xd mousemove --window "$WINDOW" 600 580   # a neutral spot, away from every target
capture initial
timeout 30 magick "$OUT_DIR/frames/initial.png" -format '%c' histogram:info:"$OUT_DIR/histogram.txt" || fail "no histogram of the frame"
grep -qi '#FF00FF' "$OUT_DIR/histogram.txt" || fail "the frame has no #ff00ff pixel: the highlighter's colour did not draw"
echo "COLOUR: #ff00ff is in the frame"
grep -qi "$KEYWORD_HEX" "$OUT_DIR/histogram.txt" && fail "Ely's keyword colour $KEYWORD_HEX is in the frame: a highlighter set leaves the lexer's keyword colour out"
echo "KEYWORD: no exact keyword colour ($KEYWORD_HEX) in the frame"
# Exact pixels of Ely's punctuation grey never appear here: its strokes are anti-aliased. So the bracket run
# is compared with the lexer's own run instead. With a highlighter set, the brackets must not keep the lexer's colour.
brackets=$(crop_pixels_differ lexer-on initial "${BRACKET_W}x${BRACKET_H}+${BRACKET_X}+${BRACKET_Y}")
[ "$brackets" -gt 0 ] || fail "the bracket run on line 4 is drawn exactly as the lexer draws it: Ely's punctuation colour is still applied with a highlighter set"
echo "BRACKETS: the bracket run on line 4 differs from the lexer's run ($brackets pixels)"

echo "step 2: a pointer over line 3 reports an offset inside that line"
before=$(grep -c '^event Pointer ' "$LOG" || true)
xd mousemove --window "$WINDOW" "$TEXT_X" "$ROW3"
python3 -c "$EVENT_PY" "$LOG" '^event Pointer offset=(?:Some\()?(\d+)' "$before" "$LINE3_START" "$LINE3_END" 5 \
  || fail "Pointer over line 3 did not report an offset inside $LINE3_START..$LINE3_END"

echo "step 3: a click on line 5's git bar reports GitMarkPressed"
before=$(grep -c '^event Pointer offset=None$' "$LOG" || true)
xd mousemove --window "$WINDOW" "$BAR_X" "$ROW5"
for _ in $(seq 1 25); do
  [ "$(grep -c '^event Pointer offset=None$' "$LOG" || true)" -gt "$before" ] && break
  sleep 0.2
done
[ "$(grep -c '^event Pointer offset=None$' "$LOG" || true)" -gt "$before" ] \
  || fail "the pointer over line 5's git bar reported an offset: the gutter is not the code"
echo "GUTTER: the pointer over the git bar has no offset"
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

echo "step 6: a plain click on line 3 reports no PlatformClick; a Super-held one does, at its own offset"
before=$(grep -c '^event PlatformClick ' "$LOG" || true)
xd mousemove --window "$WINDOW" "$TEXT_X" "$ROW3" click 1
sleep 1
[ "$(grep -c '^event PlatformClick ' "$LOG" || true)" -eq "$before" ] \
  || fail "a plain click on line 3 reported PlatformClick"
echo "PLAIN: a plain click on line 3 reported no PlatformClick"
before=$(grep -c '^event PlatformClick ' "$LOG" || true)
xd keydown Super_L
xd mousemove --window "$WINDOW" "$TEXT_X" "$ROW3" click 1
xd keyup Super_L
python3 -c "$EVENT_PY" "$LOG" '^event PlatformClick offset=(?:Some\()?(\d+)' "$before" "$LINE3_START" "$LINE3_END" 5 \
  || fail "the Super-held click on line 3 did not report PlatformClick inside line 3"

echo "step 7: line 12, wider than the window, scrolls sideways under a gutter that stays put"
# The offset a pointer at x y reports: it rests at a neutral spot first, so arriving fires a Pointer event.
pointer_on() { # x y lo hi why
  xd mousemove --window "$WINDOW" 600 580
  sleep 0.5
  local before
  before=$(grep -c '^event Pointer offset=Some' "$LOG" || true)
  xd mousemove --window "$WINDOW" "$1" "$2"
  python3 -c "$EVENT_PY" "$LOG" '^event Pointer offset=Some\((\d+)\)' "$before" "$3" "$4" 5 || fail "$5"
}
pointer_on "$SAMPLE_X" "$ROW12" $((WIDE_START + 19)) $((WIDE_START + 22)) \
  "unscrolled, the pointer at column 20 of line 12 did not land on it"
xd mousemove --window "$WINDOW" "$SAMPLE_X" "$ROW12" click --repeat 8 --delay 80 7
capture wheel-right
pointer_on "$SAMPLE_X" "$ROW12" $((WIDE_START + 30)) "$WIDE_END" \
  "a wheel to the right did not move line 12 sideways (or moved the rows instead)"
before=$(grep -c '^event Pointer offset=None$' "$LOG" || true)
xd mousemove --window "$WINDOW" "$BAR_X" "$ROW5"
for _ in $(seq 1 25); do
  [ "$(grep -c '^event Pointer offset=None$' "$LOG" || true)" -gt "$before" ] && break
  sleep 0.2
done
[ "$(grep -c '^event Pointer offset=None$' "$LOG" || true)" -gt "$before" ] \
  || fail "scrolled sideways, the pointer over line 5's git bar reported an offset: the gutter moved with the code"
echo "GUTTER: scrolled sideways, the git bar still has no offset"
xd mousemove --window "$WINDOW" "$SAMPLE_X" "$ROW12" click --repeat 40 --delay 30 6
pointer_on "$SAMPLE_X" "$ROW12" $((WIDE_START + 19)) $((WIDE_START + 22)) \
  "a wheel to the left did not bring line 12's start back"
xd mousemove --window "$WINDOW" "$SAMPLE_X" "$ROW12"
xd keydown Shift_L
xd click --repeat 8 --delay 80 5
xd keyup Shift_L
pointer_on "$SAMPLE_X" "$ROW12" $((WIDE_START + 30)) "$WIDE_END" \
  "Shift with the wheel down did not move line 12 sideways"
xd mousemove --window "$WINDOW" "$SAMPLE_X" "$ROW12" click --repeat 40 --delay 30 6
echo "WHEEL: the wheel sideways and Shift with the wheel move line 12, and the rows stay"
# A real drag of the bar's thumb: press, two moves (gpui starts a drag on the first), release.
xd mousemove --window "$WINDOW" "$THUMB_X" "$THUMB_Y"
sleep 0.3
xd mousedown 1
sleep 0.3
xd mousemove --window "$WINDOW" $((THUMB_X + 20)) "$THUMB_Y"
sleep 0.3
xd mousemove --window "$WINDOW" $((THUMB_X + 300)) "$THUMB_Y"
capture bar-dragging
xd mouseup 1
capture bar-dragged
pointer_on "$SAMPLE_X" "$ROW12" $((WIDE_START + 30)) "$WIDE_END" \
  "a drag of the horizontal bar's thumb did not move line 12 sideways"
echo "BAR: a drag of the thumb moves line 12"
xd mousemove --window "$WINDOW" "$SAMPLE_X" "$ROW12" click --repeat 40 --delay 30 6
pointer_on "$SAMPLE_X" "$ROW12" $((WIDE_START + 19)) $((WIDE_START + 22)) \
  "a wheel to the left did not bring line 12's start back after the drag"
# The caret goes to line 12 with a click; End and Home are real keys at the window that has X focus.
xd mousemove --window "$WINDOW" "$SAMPLE_X" "$ROW12" click 1
xd windowfocus --sync "$WINDOW"
xd key End
capture end-of-line
pointer_on "$RIGHT_X" "$ROW12" $((WIDE_END - 40)) "$WIDE_END" \
  "End left the view where it was: line 12's tail is not under the pointer near the right edge"
xd key Home
pointer_on "$SAMPLE_X" "$ROW12" $((WIDE_START + 19)) $((WIDE_START + 25)) \
  "Home did not bring the view back to line 12's start"
echo "CARET: End and Home move the view with the caret"
stop

echo "step 8: control: with breakpoints on, the click in line 7's slot toggles line 7"
launch breakpoints-on ELY_PROBE_BREAKPOINTS=on
eval "$(targets)"
xd mousemove --window "$WINDOW" "$SLOT_X" "$ROW7" click 1
for _ in $(seq 1 25); do grep -qx 'breakpoints {7}' "$LOG" && break; sleep 0.2; done
grep -qx 'breakpoints {7}' "$LOG" || fail "the same click in line 7's slot did not toggle line 7 with breakpoints on: the negative check in step 4 would prove nothing"
echo "CONTROL: the slot click toggled line 7 with breakpoints on"
stop

echo "ELY EDITOR PROBE OK"
echo "artifacts: $OUT_DIR"
