#!/bin/bash
set -euo pipefail

# Native check of the horizontal scroll bezel-markdown gives a wide fenced
# block and a wide table under a rendered document (the painter a chat
# response and the file view's Preview share), drawn by
# rust/crates/sirio_ui/examples/markdown_scroll_probe.rs with Sirio's theme,
# on an isolated X display.
#
# Both blocks are `overflow_x_scroll` containers, and a container that
# scrolls without drawing a bar hides whatever is past its edge. The probe
# draws document A (a `json` fence of one very long line, then an eight-column
# table) and document B (the same fence alone), and the script proves, in
# dark and light:
#   - a horizontal scrollbar thumb is drawn under the fence and under the
#     table, starting at the left edge (the content begins unscrolled);
#   - dragging the thumb with the real pointer moves it to the end, and the
#     press on the thumb never reaches the response's own press handler (the
#     probe's `PRESS doc-a` canary, which a press on text does reach);
#   - at the end of the fence the right padding shows after the last glyph:
#     the last characters are reachable, not hidden under the edge;
#   - document B's fence did not move when document A's did (scroll state
#     belongs to its block, not to the block's index).
# The frame is read by pixel: the bar strip is the bottom 10 px inside a
# block, and the thumb is the longest run of columns in it that differs from
# the block's own background.
#
# The artifact: --out-dir DIR (default artifacts/markdown-scroll-probe-<stamp>-<pid>)
# keeps transcript.log, probe-<appearance>.log and frames/*.png. Rerunning
# the script reproduces it.
#
# Usage: Scripts/Tests/test-markdown-scroll-probe.sh [--xvfb | --display :N]
#          [--no-software-vulkan] [--out-dir DIR] [--no-build]
#   --xvfb                start a private Xvfb on a free display and stop it on exit
#   --no-software-vulkan  do not force Mesa's lavapipe (on Xvfb, frames are then blank)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROBE="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/examples/markdown_scroll_probe"
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
    -h|--help) sed -n '3,35p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/markdown-scroll-probe-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
for tool in python3 xwininfo xprop import identify magick xdotool; do
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
  echo "building markdown_scroll_probe"
  (cd "$ROOT/rust" && cargo build --quiet -p sirio_ui --example markdown_scroll_probe)
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

# Pixel readers over one captured frame.
#   thumb FRAME X Y W H   -> "start end" (columns) of the longest run of the bar
#                            strip that differs from the block's background, or "none"
#   reach FRAME X Y W H   -> the right-most column of the fence's code area that
#                            differs from the code area's background, or "none"
PIXELS=$(cat <<'PY'
import subprocess, sys

mode, frame = sys.argv[1], sys.argv[2]
x, y, w, h = (int(v) for v in sys.argv[3:7])
width, height = (int(v) for v in subprocess.run(
    ["identify", "-format", "%w %h", frame], capture_output=True, text=True, check=True).stdout.split())
raw = subprocess.run(["magick", frame, "-depth", "8", "rgb:-"], capture_output=True, check=True).stdout
assert len(raw) == width * height * 3, "unexpected frame size"

def pixel(px, py):
    i = (py * width + px) * 3
    return raw[i], raw[i + 1], raw[i + 2]

def median(colours):
    return tuple(sorted(c[k] for c in colours)[len(colours) // 2] for k in range(3))

def differs(a, b):
    return max(abs(p - q) for p, q in zip(a, b)) > 12

if mode == "thumb":
    columns = range(x + 3, x + w - 3)
    rows = range(y + h - 10, y + h - 1)
    # The strip's own background is read just above it, where no bar is drawn:
    # a thumb can cover most of the strip's row, so the row cannot name it.
    background = median([pixel(c, y + h - 12) for c in columns])
    painted = [any(differs(pixel(c, r), background) for r in rows) for c in columns]
    # The block's rounded bottom corners (10 px) paint a few columns at each end
    # of the strip; a thumb is never that short, so a run under 8 px is a corner.
    best, start = (7, None), None
    for i, on in enumerate(painted + [False]):
        if on and start is None:
            start = i
        elif not on and start is not None:
            if i - start > best[0]:
                best = (i - start, (x + 3 + start, x + 3 + i - 1))
            start = None
    print("none" if best[1] is None else f"{best[1][0]} {best[1][1]}")
elif mode == "reach":
    background = pixel(x + 4, y + h - 14)
    last = None
    for c in range(x + 1, x + w - 1):
        if any(differs(pixel(c, r), background) for r in range(y + 32, y + h - 11)):
            last = c
    print("none" if last is None else last)
PY
)
pixels() { python3 -c "$PIXELS" "$@"; }

block_of() { # log doc ix -> "x y w h"
  grep "^BLOCK doc $2 $3 " "$1" | tail -n 1 | cut -d' ' -f5-8
}

shoot() { # window name
  timeout 30 import -display "$DISPLAY_TARGET" -window "$1" "$OUT_DIR/frames/$2.png" || fail "capture of $2 failed"
  echo "$OUT_DIR/frames/$2.png"
}

xd() { env DISPLAY="$DISPLAY_TARGET" xdotool "$@"; }

drag() { # window from_x from_y to_x
  local window="$1" fx="$2" fy="$3" tx="$4" step cx
  xd mousemove --window "$window" "$fx" "$fy"
  sleep 0.2
  xd mousedown 1
  sleep 0.05
  cx="$fx"
  # gpui's on_drag_move needs two moves before it believes in a drag.
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    cx=$((cx + (tx - fx) / 8))
    [ "$cx" -lt "$tx" ] || cx="$tx"
    xd mousemove --window "$window" "$cx" "$fy"
    sleep 0.05
    [ "$cx" -lt "$tx" ] || break
  done
  xd mousemove --window "$window" "$tx" "$fy"
  sleep 0.05
  xd mouseup 1
  sleep 0.6
}

presses() { grep -c '^PRESS doc-a ' "$1" || true; }

check() { # appearance
  local appearance="$1" log="$OUT_DIR/probe-$1.log" window="" frame target name
  # env execs the probe, so $! is the probe's own pid, the one in _NET_WM_PID.
  env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" "${VULKAN_ENV[@]}" \
    MD_PROBE_APPEARANCE="$appearance" "$PROBE" >"$log" 2>&1 &
  PROBE_PID=$!
  for _ in $(seq 1 100); do
    kill -0 "$PROBE_PID" 2>/dev/null || fail "probe exited early ($appearance); see $log"
    window=$(find_window)
    [ -n "$window" ] && break
    sleep 0.2
  done
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$PROBE_PID ($appearance)"
  echo "probe $appearance: pid $PROBE_PID, window $window"
  local ready=0
  for _ in $(seq 1 100); do
    if grep -q '^BLOCK doc a 0 ' "$log" && grep -q '^BLOCK doc a 1 ' "$log" && grep -q '^BLOCK doc b 0 ' "$log"; then
      ready=1; break
    fi
    sleep 0.2
  done
  [ "$ready" -eq 1 ] || fail "$appearance: the probe never reported its blocks; see $log"
  sleep 1.5
  [ "$(find_window)" = "$window" ] || fail "window $window no longer belongs to pid $PROBE_PID"
  local initial
  initial=$(shoot "$window" "$appearance-0-initial")

  local summary=""
  for target in code table; do
    local ix=0
    [ "$target" = table ] && ix=1
    local x y w h
    read -r x y w h <<<"$(block_of "$log" a "$ix")"
    [ -n "${w:-}" ] || fail "$appearance: no bounds for doc a block $ix ($log)"
    [ "$target" = code ] && name="code block" || name="table"
    echo "$appearance: $name at x $x y $y w $w h $h"

    local thumb start end length
    thumb=$(pixels thumb "$initial" "$x" "$y" "$w" "$h") || fail "could not read $initial"
    [ "$thumb" != none ] || fail "$appearance: no horizontal scrollbar under $name ($initial)"
    read -r start end <<<"$thumb"
    length=$((end - start + 1))
    [ "$length" -ge 20 ] && [ "$length" -le $((w - 20)) ] \
      || fail "$appearance: the scrollbar under $name is $length px long (x $start..$end), want 20..$((w - 20)) ($initial)"
    [ "$start" -le $((x + 10)) ] \
      || fail "$appearance: the scrollbar under $name starts at x $start, past x $((x + 10)): the content does not begin unscrolled ($initial)"

    local before after
    before=$(presses "$log")
    drag "$window" $(((start + end) / 2)) $((y + h - 6)) $((x + w + 60))
    frame=$(shoot "$window" "$appearance-1-$target-dragged")
    thumb=$(pixels thumb "$frame" "$x" "$y" "$w" "$h") || fail "could not read $frame"
    [ "$thumb" != none ] || fail "$appearance: the scrollbar under $name vanished after the drag ($frame)"
    read -r start end <<<"$thumb"
    [ "$end" -ge $((x + w - 10)) ] \
      || fail "$appearance: dragging the scrollbar under $name left its thumb at x $start..$end, short of x $((x + w - 10)) ($frame)"
    after=$(presses "$log")
    [ "$after" -eq "$before" ] \
      || fail "$appearance: a press on the scrollbar under $name reached the response's press handler ($((after - before)) PRESS lines in $log)"
    echo "FRAME: $appearance $name -- thumb $start..$end of x $x..$((x + w - 1)), no press reached the response ($frame)"
    summary="$summary $frame"

    if [ "$target" = code ]; then
      local last bx by bw bh
      last=$(pixels reach "$frame" "$x" "$y" "$w" "$h") || fail "could not read $frame"
      [ "$last" != none ] || fail "$appearance: no code painted in the $name at the end of its scroll ($frame)"
      [ "$last" -le $((x + w - 10)) ] \
        || fail "$appearance: the $name at the end of its scroll paints to x $last, flush with its edge x $((x + w - 1)): the last characters are not padded ($frame)"
      [ "$last" -ge $((x + w - 60)) ] \
        || fail "$appearance: the $name at the end of its scroll stops at x $last, far from its edge x $((x + w - 1)): not scrolled to the end ($frame)"
      echo "FRAME: $appearance $name -- last glyph at x $last, edge x $((x + w - 1)) ($frame)"

      read -r bx by bw bh <<<"$(block_of "$log" b 0)"
      [ -n "${bw:-}" ] || fail "$appearance: no bounds for doc b block 0 ($log)"
      thumb=$(pixels thumb "$frame" "$bx" "$by" "$bw" "$bh") || fail "could not read $frame"
      [ "$thumb" != none ] || fail "$appearance: no horizontal scrollbar under the second document's code block ($frame)"
      read -r start end <<<"$thumb"
      [ "$start" -le $((bx + 10)) ] \
        || fail "$appearance: the second document's code block scrolled with the first (thumb x $start..$end) ($frame)"
      echo "FRAME: $appearance second document -- thumb $start..$end, still at its start ($frame)"
    fi
  done

  # The canary must be able to fire, or its silence above proves nothing.
  local x y w h before
  read -r x y w h <<<"$(block_of "$log" a 0)"
  before=$(presses "$log")
  xd mousemove --window "$window" $((x + 40)) $((y + 40))
  sleep 0.2
  xd click 1
  sleep 0.5
  [ "$(presses "$log")" -gt "$before" ] \
    || fail "$appearance: a press on the code text did not reach the response's press handler: the canary is dead ($log)"
  echo "FRAME: $appearance canary -- a press on text reaches the response"

  kill "$PROBE_PID" 2>/dev/null || true
  wait "$PROBE_PID" 2>/dev/null || true
  PROBE_PID=""
  FRAMES="$FRAMES$summary"
}

FRAMES=""
check dark
check light
echo "MARKDOWN SCROLL PROBE OK (frames: $OUT_DIR/frames)"
