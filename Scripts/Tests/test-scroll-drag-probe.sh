#!/bin/bash
set -euo pipefail

# Native check that a scrollbar thumb follows the pointer when it is dragged, for the three bars
# the app draws (file_view::scrollbar, the transcript's list_scrollbar and horizontal_scroll::bar),
# side by side in rust/crates/sirio_ui/examples/scroll_drag_probe.rs on an isolated X display.
# It runs before a PR moves those bars off bezel and again after, so the new bars are held to the
# behaviour the old ones have: this is the pointer's view of a drag, which a gpui test cannot give.
#
# For each bar the script captures the window, finds the thumb as the run of pixels in the bar's
# column (its row, for the horizontal bar) that differ from the column's most common colour,
# presses the pointer at the thumb's centre and moves it 3 px, then 1 px along the bar, then 120
# px in six steps, captures again and finds the thumb. The thumb must have moved as far as the
# pointer did after its second move, give or take 4 px: gpui starts the drag on the move that
# crosses 2 px and delivers the first drag event on the next, where every bar anchors its grab, so
# that move is where the pointer's travel is counted from. A second drag of +2000 px (the pointer
# stops at the screen's edge, past the track's end) must leave the thumb flush with the track's
# end.
#
# The layout is the probe's contract: a 900x600 window, three panes of 300x600 from the left, a
# vertical bar at the right edge of panes 1 and 2, a horizontal one along the bottom of pane 3.
#
# The artifact: --out-dir DIR (default artifacts/scroll-drag-probe-<stamp>-<pid>) keeps
# transcript.log, probe.log and frames/*.png (the window before the first drag and, per bar, while
# dragged and at the end). Rerunning the script reproduces it. To prove the check can fail, set
# SCROLL_DRAG_PROBE_SKEW=40: the expected thumb travel grows by 40 px and the run must exit
# nonzero.
#
# Usage: Scripts/Tests/test-scroll-drag-probe.sh [--xvfb | --display :N]
#          [--no-software-vulkan] [--out-dir DIR] [--no-build]
#   --xvfb                start a private Xvfb on a free display and stop it on exit
#   --no-software-vulkan  do not force Mesa's lavapipe (on Xvfb, frames are then blank)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROBE="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/examples/scroll_drag_probe"
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
    -h|--help) sed -n '3,32p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
# Scratch never goes to /tmp (a small tmpfs).
export TMPDIR="${TMPDIR:-$HOME/.cache/sirio-e2e-tmp}"
mkdir -p "$TMPDIR"
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/scroll-drag-probe-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
for tool in python3 xwininfo xprop xdpyinfo import identify magick xdotool; do
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
  echo "building scroll_drag_probe"
  (cd "$ROOT/rust" && cargo build --quiet -p sirio_ui --example scroll_drag_probe)
fi
[ -x "$PROBE" ] || fail "no probe binary at $PROBE"

# env execs the probe, so $! is the probe's own pid, the one in _NET_WM_PID.
env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" "${VULKAN_ENV[@]}" SIRIO_REDUCED_MOTION=1 \
  "$PROBE" >"$OUT_DIR/probe.log" 2>&1 &
PROBE_PID=$!

DISPLAY_TARGET="$DISPLAY_TARGET" PROBE_PID="$PROBE_PID" OUT_DIR="$OUT_DIR" \
SKEW="${SCROLL_DRAG_PROBE_SKEW:-0}" python3 - <<'PY' || fail "the scroll drag check failed (see $OUT_DIR/transcript.log)"
import collections, os, re, subprocess, sys, time

display = os.environ["DISPLAY_TARGET"]
pid = int(os.environ["PROBE_PID"])
out = os.environ["OUT_DIR"]
skew = int(os.environ["SKEW"])
env = dict(os.environ, DISPLAY=display)

WINDOW_W, WINDOW_H, PANE_W, TRACK = 900, 600, 300, 10
TOLERANCE = 4
MIN_THUMB = 25


def fail(message):
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)


def run(*command, timeout=30):
    return subprocess.run(command, env=env, capture_output=True, text=True, timeout=timeout, check=False)


def find_window():
    listing = run("xwininfo", "-display", display, "-root", "-children").stdout
    best = None
    for line in listing.splitlines():
        match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
        if not match:
            continue
        window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
        prop = run("xprop", "-display", display, "-id", window, "_NET_WM_PID").stdout
        owner = re.search(r"=\s*(\d+)\s*$", prop)
        if owner and int(owner.group(1)) == pid and (best is None or width * height > best[0]):
            best = (width * height, window)
    return best[1] if best else None


window = None
for _ in range(100):
    try:
        os.kill(pid, 0)
    except OSError:
        fail(f"the probe exited early; see {out}/probe.log")
    window = find_window()
    if window:
        break
    time.sleep(0.2)
if not window:
    fail(f"no window with _NET_WM_PID={pid}")
info = run("xwininfo", "-display", display, "-id", window).stdout
geometry = {k: int(re.search(rf"{k}:\s*(-?\d+)", info).group(1)) for k in ("Absolute upper-left X", "Absolute upper-left Y", "Width", "Height")}
win_x, win_y = geometry["Absolute upper-left X"], geometry["Absolute upper-left Y"]
if (geometry["Width"], geometry["Height"]) != (WINDOW_W, WINDOW_H):
    fail(f"the probe window is {geometry['Width']}x{geometry['Height']}, not {WINDOW_W}x{WINDOW_H}")
screen = re.search(r"dimensions:\s*(\d+)x(\d+)", run("xdpyinfo", "-display", display).stdout)
screen_w, screen_h = int(screen.group(1)), int(screen.group(2))
# The +2000 px drag ends where the pointer stops, at the screen's edge: that must lie past every
# track's end, or the drag would stop short of it.
if win_x < 0 or win_y < 0 or win_x + WINDOW_W >= screen_w or win_y + WINDOW_H >= screen_h:
    fail(f"the window ({win_x},{win_y} {WINDOW_W}x{WINDOW_H}) must lie inside the {screen_w}x{screen_h} screen with room past its far edges")
print(f"probe: pid {pid}, window {window} at {win_x},{win_y}, screen {screen_w}x{screen_h}", flush=True)


def capture(name):
    path = f"{out}/frames/{name}.png"
    result = run("import", "-display", display, "-window", window, path)
    if result.returncode != 0:
        fail(f"capture {name} failed: {result.stderr.strip()}")
    return path


def pixels(frame, crop):
    data = subprocess.run(["magick", frame, "-crop", crop, "+repage", "-depth", "8", "rgb:-"], capture_output=True, check=True).stdout
    return [tuple(data[i:i + 3]) for i in range(0, len(data), 3)]


class Bar:
    """A bar's scan line in the window: where it is, and which way the thumb runs."""

    def __init__(self, number, vertical):
        self.number, self.vertical = number, vertical
        if vertical:
            # The track is the pane's right-hand 10 px; its centre line is 5 px in.
            self.cross = number * PANE_W - TRACK // 2
            self.length = WINDOW_H
            self.crop = f"1x{WINDOW_H}+{self.cross}+0"
        else:
            self.cross = WINDOW_H - TRACK // 2
            self.length = PANE_W
            self.crop = f"{PANE_W}x1+{(number - 1) * PANE_W}+{self.cross}"
            self.offset = (number - 1) * PANE_W

    def thumb(self, frame):
        """(start, end) along the track, from the longest run that is not the column's mode colour."""
        line = pixels(frame, self.crop)
        mode = collections.Counter(line).most_common(1)[0][0]
        runs, start = [], None
        for index, pixel in enumerate(line + [mode]):
            differs = max(abs(a - b) for a, b in zip(pixel, mode)) > 6
            if differs and start is None:
                start = index
            elif not differs and start is not None:
                runs.append((start, index))
                start = None
        if not runs:
            fail(f"bar {self.number}: no thumb in {frame} (every pixel of {self.crop} is {mode})")
        run_ = max(runs, key=lambda r: r[1] - r[0])
        if not MIN_THUMB <= run_[1] - run_[0] < self.length:
            fail(f"bar {self.number}: the run {run_} in {frame} is not a thumb (track {self.length} px)")
        return run_

    def point(self, along):
        """Window coordinates, in screen terms, of `along` px down (or across) the track."""
        if self.vertical:
            return win_x + self.cross, win_y + along
        return win_x + self.offset + along, win_y + self.cross

    def move(self, distance):
        step = ("0", str(distance)) if self.vertical else (str(distance), "0")
        run("xdotool", "mousemove_relative", "--", *step)

    def pointer_along(self):
        location = run("xdotool", "getmouselocation", "--shell").stdout
        x = int(re.search(r"X=(-?\d+)", location).group(1))
        y = int(re.search(r"Y=(-?\d+)", location).group(1))
        return (y - win_y) if self.vertical else (x - win_x - self.offset)


def settled(bar, name):
    """The thumb once two captures a moment apart agree: a bar needs a frame after the first to draw."""
    previous = None
    for attempt in range(20):
        time.sleep(0.5)
        found = bar.thumb(capture(name))
        if found == previous:
            return found
        previous = found
    fail(f"bar {bar.number}: the thumb never settled (last {previous})")


def press(bar, thumb):
    centre = (thumb[0] + thumb[1]) // 2
    x, y = bar.point(centre)
    run("xdotool", "mousemove", str(x), str(y))
    time.sleep(0.3)
    run("xdotool", "mousedown", "1")
    time.sleep(0.2)
    # 3 px crosses gpui's 2 px drag threshold; the 1 px after it is the move whose event is the
    # first drag event, where the bar anchors its grab.
    bar.move(3)
    time.sleep(0.15)
    bar.move(1)
    time.sleep(0.15)
    return bar.pointer_along()


bars = [Bar(1, True), Bar(2, True), Bar(3, False)]
time.sleep(2)
before_thumbs = {bar.number: settled(bar, "before") for bar in bars}
for bar in bars:
    print(f"bar {bar.number}: thumb {before_thumbs[bar.number]} before any drag", flush=True)

for bar in bars:
    thumb = before_thumbs[bar.number]
    anchored = press(bar, thumb)
    for _ in range(6):
        bar.move(20)
        time.sleep(0.1)
    time.sleep(0.5)
    pointer = bar.pointer_along()
    moved = bar.thumb(capture(f"drag-bar-{bar.number}"))
    run("xdotool", "mouseup", "1")
    travel, expected = moved[0] - thumb[0], (pointer - anchored) + skew
    print(f"bar {bar.number}: pointer moved {pointer - anchored} px after its second move, thumb {travel} px (expected {expected}, tolerance {TOLERANCE})", flush=True)
    if abs(travel - expected) > TOLERANCE:
        fail(f"bar {bar.number}: the thumb moved {travel} px for {expected} px of pointer travel")
    time.sleep(0.5)

    rest = settled(bar, f"after-drag-bar-{bar.number}")
    press(bar, rest)
    for _ in range(6):
        bar.move(334)
        time.sleep(0.1)
    time.sleep(0.5)
    end = bar.thumb(capture(f"end-bar-{bar.number}"))
    run("xdotool", "mouseup", "1")
    print(f"bar {bar.number}: after a drag of +2000 px the thumb is {end} on a {bar.length} px track", flush=True)
    if end[1] < bar.length - 2:
        fail(f"bar {bar.number}: the thumb stops at {end[1]}, {bar.length - end[1]} px short of the track's end")
    time.sleep(0.3)

print("drag checks passed for 3 bars")
PY
echo "frames: $OUT_DIR/frames"
echo "SCROLL DRAG PROBE OK (3 bars)"
