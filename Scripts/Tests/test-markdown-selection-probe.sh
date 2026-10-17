#!/bin/bash
set -euo pipefail

# Native check of the selection bezel-markdown paints under a rendered
# document (the painter a chat response and the file view's Preview share),
# drawn by rust/crates/sirio_ui/examples/markdown_selection_probe.rs with
# Sirio's theme, on an isolated X display.
#
# The probe selects its whole document, so every row is washed. It proves
# that each row's wash begins at the row's first glyph: for every pixel row
# of the frame, wherever a reference column inside the text (x 84) is not
# background, the column just inside the text's left edge (x 26) is not
# background either. A soft-wrapped row whose wash began one glyph late --
# the defect rust/vendor/README.md's `bezel_markdown` section describes --
# leaves that edge bare for a whole line height. The probe's text soft-wraps,
# carries a markdown line break inside one paragraph, wraps an inline code
# span, and ends with a second paragraph; dark and light are both checked.
#
# The artifact: --out-dir DIR (default artifacts/markdown-selection-probe-<stamp>-<pid>)
# keeps transcript.log, probe-<appearance>.log and frames/*.png. Rerunning
# the script reproduces it.
#
# Usage: Scripts/Tests/test-markdown-selection-probe.sh [--xvfb | --display :N]
#          [--no-software-vulkan] [--out-dir DIR] [--no-build]
#   --xvfb                start a private Xvfb on a free display and stop it on exit
#   --no-software-vulkan  do not force Mesa's lavapipe (on Xvfb, frames are then blank)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROBE="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/examples/markdown_selection_probe"
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
    -h|--help) sed -n '3,26p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/markdown-selection-probe-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
for tool in python3 xwininfo xprop import identify magick; do
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
  echo "building markdown_selection_probe"
  (cd "$ROOT/rust" && cargo build --quiet -p sirio_ui --example markdown_selection_probe)
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

check() { # appearance
  local log="$OUT_DIR/probe-$1.log" frame="$OUT_DIR/frames/selection-$1.png" window=""
  # env execs the probe, so $! is the probe's own pid, the one in _NET_WM_PID.
  env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" "${VULKAN_ENV[@]}" \
    MD_PROBE_APPEARANCE="$1" "$PROBE" >"$log" 2>&1 &
  PROBE_PID=$!
  for _ in $(seq 1 100); do
    kill -0 "$PROBE_PID" 2>/dev/null || fail "probe exited early ($1); see $log"
    window=$(find_window)
    [ -n "$window" ] && break
    sleep 0.2
  done
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$PROBE_PID ($1)"
  echo "probe $1: pid $PROBE_PID, window $window"
  sleep 2
  [ "$(find_window)" = "$window" ] || fail "window $window no longer belongs to pid $PROBE_PID"
  timeout 30 import -display "$DISPLAY_TARGET" -window "$window" "$frame" || fail "capture of $1 failed"
  kill "$PROBE_PID" 2>/dev/null || true
  wait "$PROBE_PID" 2>/dev/null || true
  PROBE_PID=""

  # Each column as one text line per pixel; background is the padding's corner.
  local verdict
  verdict=$(FRAME="$frame" python3 - <<'PY'
import os, re, subprocess
frame = os.environ["FRAME"]
EDGE, REFERENCE = 26, 84

def column(x):
    out = subprocess.run(["magick", frame, "-crop", f"1x100000+{x}+0", "-depth", "8", "txt:-"],
                         capture_output=True, text=True, check=True).stdout
    pixels = {}
    for line in out.splitlines():
        m = re.match(r"\s*\d+,(\d+):\s*\((\d+),(\d+),(\d+)", line)
        if m:
            pixels[int(m.group(1))] = tuple(int(v) for v in m.group(2, 3, 4))
    return pixels

corner = subprocess.run(["magick", frame, "-crop", "1x1+4+4", "-depth", "8", "txt:-"],
                        capture_output=True, text=True, check=True).stdout
background = tuple(int(v) for v in re.search(r"\((\d+),(\d+),(\d+)", corner).groups())
painted = lambda rgb: max(abs(a - b) for a, b in zip(rgb, background)) > 12
edge, reference = column(EDGE), column(REFERENCE)
washed = [y for y, rgb in reference.items() if painted(rgb)]
bare = [y for y in washed if not painted(edge.get(y, background))]
print(f"{len(washed)} {len(bare)} {','.join(map(str, bare[:12]))}")
PY
) || fail "could not read $frame"
  read -r washed bare first_bare <<<"$verdict"
  # Six rows of text, each a full line height: anything less is not the probe's frame.
  [ "$washed" -ge 100 ] || fail "$1: only $washed painted pixel rows at x 84 -- blank or wrong frame ($frame)"
  # A rounded corner may leave a pixel or two; a row that starts late leaves a line height.
  [ "$bare" -le 3 ] || fail "$1: $bare of $washed washed pixel rows are bare at the text's left edge (y ${first_bare}...) -- a row's selection starts late ($frame)"
  echo "FRAME: $1 -- $washed washed pixel rows, $bare bare at the left edge"
}

check dark
check light
echo "MARKDOWN SELECTION PROBE OK (frames: $OUT_DIR/frames)"
