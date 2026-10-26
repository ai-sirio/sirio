#!/bin/bash
set -euo pipefail

# Native capture of the surfaces the app's own E2E tests do not reach: the eight Settings pages
# (and Git Hosting again with a hand-written forge source, `settings-git-hosting-accounts`), a tab
# context menu and a tooltip, drawn by rust/crates/sirio_ui/examples/surfaces_probe.rs with
# Sirio's theme, on an isolated X display. They are the frames of the bezel-off capture
# set (docs/testing/bezel-off.md) that nothing else takes; Scripts/bezel-off-captures.sh runs
# this script once per appearance and keeps DIR/frames/*.png.
#
# One probe process per frame and appearance, each one the PID-matched window of its own
# process, captured once two consecutive captures one second apart are the same pixels (compared
# by pixel signature, `identify -format %#`, so file metadata never decides). The
# pointer is parked outside the centred window before every launch and, once the window is up,
# taken into the window and out again (see clear_hover), so no row is hovered; for
# `tooltip` it is moved onto the 120x28 box at (200, 200), and the frame is kept once gpui's
# 500 ms hover delay has passed and the tooltip is on screen. Every probe runs with PATH unset
# (see the probe's header: it is how the Agents page reaches its registry warning strip, and it
# keeps the Language Servers rows and the PlantUML line from reading the machine).
#
# A Settings page longer than the window is captured in pages: after the first frame, the
# script puts the pointer on the detail column's blank margin, turns the wheel SCROLL_NOTCHES
# notches (one notch is 78 px, so a step is 702 px of the 792 px viewport and consecutive pages
# share 90 px), moves the pointer out of the window again and takes
# `<frame>-p<N>-<appearance>.png` (N = 2, 3, ...) until a step moves nothing: that capture is
# pixel-identical to the last page and is discarded. A page that fits takes no second frame. The
# cap is MAX_PAGES pages a frame (8, or $SURFACES_MAX_PAGES); a step after the last allowed page
# that still moves fails the script. The pages each frame took are printed as `PAGES`.
#
# Asserted per frame: at least 200 distinct colours (the visual sweep's MIN_COLORS), except
# `tooltip`, which must differ from its own pre-hover capture by at least 150 pixels, all of
# them near the box. A state the probe's public API cannot reach prints `GAP <frame> <state>`
# and is counted, never faked.
#
# The artifact: --out-dir DIR (default artifacts/surfaces-probe-<stamp>-<pid>) keeps
# transcript.log, probe-<frame>-<appearance>.log, tooltip-before-<appearance>.png and
# frames/<frame>-<appearance>.png (plus -p<N> pages). Rerunning the script reproduces it.
#
# Environment: SURFACES_MAX_PAGES=N sets the page cap (default 8). Setting it below a page count
# the frames really have shows the cap's FAIL line (N=2 fails on the first 3-page frame, AI
# Providers). N must be at least 2: the cap is checked once a second page has been taken.
#
# Usage: Scripts/Tests/test-surfaces-probe.sh [--xvfb | --display :N] [--appearance dark|light]
#          [--no-software-vulkan] [--out-dir DIR] [--no-build]
#   --xvfb                start a private Xvfb (1600x1000x24, room for the 1470x833 window) on a
#                         free display and stop it on exit
#   --appearance A        only this appearance; without it both
#   --no-software-vulkan  do not force Mesa's lavapipe (on Xvfb, frames are then blank)
# Prints "SURFACES PROBE OK (<n> frames, <g> gaps)" last, n counting every frame taken.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROBE="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/examples/surfaces_probe"
LVP=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
MIN_COLORS=200
TOOLTIP_MIN_PIXELS=150
SCROLL_NOTCHES=9
MAX_PAGES="${SURFACES_MAX_PAGES:-8}"
OUT_DIR=""
DISPLAY_TARGET=""
START_XVFB=0
SOFTWARE_VULKAN=1
BUILD=1
APPEARANCES=(dark light)
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) [ $# -ge 2 ] || { echo "--display needs a display like :99" >&2; exit 2; }; DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance)
      [ $# -ge 2 ] || { echo "--appearance needs dark or light" >&2; exit 2; }
      case "$2" in dark|light) APPEARANCES=("$2") ;; *) echo "--appearance takes dark or light, not '$2'" >&2; exit 2 ;; esac
      shift 2 ;;
    --no-software-vulkan) SOFTWARE_VULKAN=0; shift ;;
    --out-dir) [ $# -ge 2 ] || { echo "--out-dir needs a directory" >&2; exit 2; }; OUT_DIR="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    -h|--help) sed -n '3,/^# Prints/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$MAX_PAGES" in
  ''|*[!0-9]*|0|1) echo "SURFACES_MAX_PAGES must be a whole number of at least 2 (the cap is checked after a second page), not '$MAX_PAGES'" >&2; exit 2 ;;
esac
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/surfaces-probe-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames" "$OUT_DIR/work"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
if [ "$START_XVFB" -eq 1 ] && [ -n "$DISPLAY_TARGET" ]; then fail "pass --xvfb or --display :N, not both"; fi
for tool in python3 xwininfo xprop xdotool import identify magick; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done

PROBE_PID=""
XVFB_PID=""
cleanup() {
  [ -z "$PROBE_PID" ] || kill "$PROBE_PID" 2>/dev/null || true
  [ -z "$XVFB_PID" ] || kill "$XVFB_PID" 2>/dev/null || true
  rm -rf "$OUT_DIR/work"
}
trap cleanup EXIT

if [ "$START_XVFB" -eq 1 ]; then
  command -v Xvfb >/dev/null || fail "Xvfb is required for --xvfb"
  command -v xdpyinfo >/dev/null || fail "xdpyinfo is required for --xvfb"
  for n in $(seq 90 99); do
    [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ] || { DISPLAY_TARGET=":$n"; break; }
  done
  [ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
  Xvfb "$DISPLAY_TARGET" -screen 0 1600x1000x24 -nolisten tcp >"$OUT_DIR/xvfb.log" 2>&1 &
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
  echo "building surfaces_probe"
  (cd "$ROOT/rust" && cargo build --quiet -p sirio_ui --example surfaces_probe)
fi
[ -x "$PROBE" ] || fail "no probe binary at $PROBE"
FRAMES=()
while IFS= read -r name; do FRAMES+=("$name"); done < <("$PROBE" --list)
[ "${#FRAMES[@]}" -gt 0 ] || fail "the probe lists no frame"
echo "frames: ${FRAMES[*]}"

xd() { DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool "$@"; }

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

# A window that maps with the pointer outside it is drawn with the row at its centre hovered, and
# stays so until the pointer has been in the window and left it again. Enter the blank margin of
# the Settings detail column (x 330, y 700) and leave, so no frame starts with a phantom hover.
clear_hover() {
  xd mousemove --window "$WINDOW" 330 700
  xd mousemove 0 0
}

# The pointer is parked at the display's (0,0), which must be outside the window: fail unless the
# window's origin is greater than 0 on both axes (a display barely larger than the 1470x833
# window would centre it on the park and the hover clearing would silently not happen).
require_pointer_park_outside() {
  local geometry x y
  geometry=$(xwininfo -display "$DISPLAY_TARGET" -id "$WINDOW") || fail "xwininfo cannot read window $WINDOW"
  x=$(printf '%s\n' "$geometry" | awk '/Absolute upper-left X:/ { print $NF }')
  y=$(printf '%s\n' "$geometry" | awk '/Absolute upper-left Y:/ { print $NF }')
  [[ "$x" =~ ^-?[0-9]+$ && "$y" =~ ^-?[0-9]+$ ]] || fail "cannot read the origin of window $WINDOW from xwininfo"
  if [ "$x" -le 0 ] || [ "$y" -le 0 ]; then
    fail "window $WINDOW is at $x,$y on $DISPLAY_TARGET: the pointer is parked at 0,0, which must be outside it; use a display larger than the 1470x833 window (--xvfb is 1600x1000)"
  fi
}

WINDOW=""
launch() { # frame appearance
  local log="$OUT_DIR/probe-$1-$2.log"
  # The pointer starts outside the window, which maps at the middle of the display.
  xd mousemove 0 0
  # env execs the probe, so $! is the probe's own pid, the one in _NET_WM_PID. PATH is unset on
  # purpose (the header says why); the probe is named by its absolute path.
  env -u WAYLAND_DISPLAY -u PATH DISPLAY="$DISPLAY_TARGET" ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} \
    SURFACES_PROBE_SCRATCH="$OUT_DIR/work/scratch" \
    "$PROBE" --frame "$1" --appearance "$2" >"$log" 2>&1 &
  PROBE_PID=$!
  WINDOW=""
  for _ in $(seq 1 100); do
    kill -0 "$PROBE_PID" 2>/dev/null || fail "probe exited early ($1 $2); see $log"
    WINDOW=$(find_window)
    [ -n "$WINDOW" ] && break
    sleep 0.2
  done
  [ -n "$WINDOW" ] || fail "no window with _NET_WM_PID=$PROBE_PID ($1 $2)"
  echo "probe $1 $2: pid $PROBE_PID, window $WINDOW"
  require_pointer_park_outside
  clear_hover
}
stop() {
  kill "$PROBE_PID" 2>/dev/null || true
  wait "$PROBE_PID" 2>/dev/null || true
  PROBE_PID=""
}

grab() { # dest
  [ "$(find_window)" = "$WINDOW" ] || fail "window $WINDOW no longer belongs to pid $PROBE_PID"
  timeout 30 import -display "$DISPLAY_TARGET" -window "$WINDOW" "$1" || fail "capture to $1 failed"
  [ -s "$1" ] || fail "import produced no pixels for $1"
}

# Whether two captures are the same pixels. The signature is a hash of the pixel data, so the PNG
# chunks `import` happens to write (none today) cannot make two identical frames differ.
same_pixels() { [ "$(identify -format '%#' "$1")" = "$(identify -format '%#' "$2")" ]; }

# settle DEST ACCEPT: capture the window until two captures one second apart are the same pixels
# and `ACCEPT capture` agrees; DEST then holds the last one. Under load the first frames of a
# lavapipe window can arrive late, and a blank window that is the same twice is not settled.
settle() {
  local dest="$1" accept="$2" previous="$OUT_DIR/work/previous.png" current="$OUT_DIR/work/current.png" attempt
  rm -f "$previous" "$current"
  sleep 1.5
  for attempt in $(seq 1 20); do
    grab "$current"
    if [ -f "$previous" ] && same_pixels "$previous" "$current" && "$accept" "$current"; then
      mv "$current" "$dest"
      return 0
    fi
    mv "$current" "$previous"
    sleep 1
  done
  return 1
}

colors_of() { identify -format '%k' "$1" 2>/dev/null || echo 0; }
enough_colors() { [ "$(colors_of "$1")" -ge "$MIN_COLORS" ]; }
any_pixels() { [ -s "$1" ]; }

# The pixels (and their bounding box, as WxH+X+Y) in which two captures differ by more than 2% in
# some channel. Counted from a difference image, never from `compare -metric AE`.
changed_pixels() { # a b
  magick "$1" "$2" -compose Difference -composite -colorspace Gray -threshold 2% \
    -format '%[fx:round(mean*w*h)]' info:
}
changed_box() { # a b
  magick "$1" "$2" -compose Difference -composite -colorspace Gray -threshold 2% -format '%@' info:
}

# One wheel step over the settings detail column. The wheel turns with the pointer on the blank
# margin left of the 720 px content (x 330, y 420), so no row is under it, and the pointer then
# leaves the window: a row a pointer sat on stays drawn hovered until the pointer is seen to
# leave it, and one that stayed on a row would open its tooltip.
scroll_page() {
  xd mousemove --window "$WINDOW" 330 420
  xd click --repeat "$SCROLL_NOTCHES" --delay 25 5
  xd mousemove 0 0
}

PAGE_LINES=()
# take_pages FRAME APPEARANCE: the pages after the first, each kept while the wheel still moves
# the page. Sets PAGES to the number of pages the frame took (the first included).
take_pages() {
  local frame="$1" appearance="$2" page=1 previous="$OUT_DIR/frames/$1-$2.png" next probe_step
  PAGES=1
  while :; do
    scroll_page
    next="$OUT_DIR/frames/$frame-p$((page + 1))-$appearance.png"
    settle "$next" enough_colors || fail "$frame $appearance: page $((page + 1)) never settled with at least $MIN_COLORS colours; see $OUT_DIR/probe-$frame-$appearance.log"
    if same_pixels "$previous" "$next"; then
      rm -f "$next"
      break
    fi
    page=$((page + 1))
    PAGES=$page
    previous="$next"
    TAKEN=$((TAKEN + 1))
    echo "FRAME: $frame-p$page-$appearance ($(identify -format '%wx%h' "$next") · $(colors_of "$next") colours)"
    if [ "$page" -ge "$MAX_PAGES" ]; then
      # The cap: the step after the last allowed page must move nothing.
      scroll_page
      probe_step="$OUT_DIR/work/cap-check.png"
      settle "$probe_step" enough_colors || fail "$frame $appearance: never settled after page $page"
      same_pixels "$previous" "$probe_step" || fail "$frame $appearance: the page still scrolls after $MAX_PAGES pages (a step of $SCROLL_NOTCHES notches moved it); raise SURFACES_MAX_PAGES on purpose, do not drop frames"
      break
    fi
  done
}

TAKEN=0
take() { # frame appearance
  local frame="$1" appearance="$2" dest="$OUT_DIR/frames/$1-$2.png" colors
  launch "$frame" "$appearance"
  if [ "$frame" = "tooltip" ]; then
    local before="$OUT_DIR/tooltip-before-$appearance.png" pixels box x y
    settle "$before" any_pixels || fail "$frame $appearance: the window never settled before the hover"
    # The box is 120x28 at (200, 200) of the window; gpui shows a tooltip after 500 ms of hover.
    xd mousemove --window "$WINDOW" 260 214
    sleep 1
    tooltip_shown() { [ "$(changed_pixels "$before" "$1")" -ge "$TOOLTIP_MIN_PIXELS" ]; }
    settle "$dest" tooltip_shown || fail "$frame $appearance: no tooltip appeared within the hover (fewer than $TOOLTIP_MIN_PIXELS pixels differ from $before)"
    pixels=$(changed_pixels "$before" "$dest")
    box=$(changed_box "$before" "$dest")
    x=${box#*+}; x=${x%%+*}; y=${box##*+}
    # The tooltip is drawn at the pointer, which is inside the box: well inside the window's top
    # left.
    [ "$x" -ge 190 ] && [ "$y" -ge 190 ] && [ "$x" -le 520 ] && [ "$y" -le 340 ] \
      || fail "$frame $appearance: the changed pixels ($box) are not near the box at 200,200; see $dest"
    echo "FRAME: $frame-$appearance ($(identify -format '%wx%h' "$dest") · tooltip $pixels pixels at $box)"
  else
    settle "$dest" enough_colors || fail "$frame $appearance: never settled with at least $MIN_COLORS colours ($(colors_of "$OUT_DIR/work/previous.png") colours at the end); see $OUT_DIR/probe-$frame-$appearance.log"
    echo "FRAME: $frame-$appearance ($(identify -format '%wx%h' "$dest") · $(colors_of "$dest") colours)"
  fi
  [ "$(identify -format '%wx%h' "$dest")" = "1470x833" ] || fail "$frame $appearance: the window is $(identify -format '%wx%h' "$dest"), not 1470x833"
  TAKEN=$((TAKEN + 1))
  case "$frame" in
    settings-*)
      take_pages "$frame" "$appearance"
      PAGE_LINES+=("PAGES: $frame-$appearance $PAGES")
      ;;
  esac
  stop
}

for appearance in "${APPEARANCES[@]}"; do
  for frame in "${FRAMES[@]}"; do
    take "$frame" "$appearance"
  done
done

# A GAP is a state the probe could not reach; the same line from both appearances counts once.
GAPS=$(cat "$OUT_DIR"/probe-*.log | grep '^GAP ' | sort -u || true)
GAP_COUNT=0
if [ -n "$GAPS" ]; then
  GAP_COUNT=$(printf '%s\n' "$GAPS" | wc -l | tr -d ' ')
  printf '%s\n' "$GAPS"
fi
FRAME_COUNT=$(find "$OUT_DIR/frames" -name '*.png' | wc -l | tr -d ' ')
[ "$FRAME_COUNT" -eq "$TAKEN" ] || fail "took $TAKEN frames but frames/ holds $FRAME_COUNT"
# Every frame's first page was taken, one per frame and appearance; pages come on top of those.
FIRST_PAGES=$(( ${#FRAMES[@]} * ${#APPEARANCES[@]} ))
[ "$TAKEN" -ge "$FIRST_PAGES" ] || fail "took $TAKEN frames, fewer than the $FIRST_PAGES first pages"
printf '%s\n' "${PAGE_LINES[@]}"
echo "SURFACES PROBE OK ($TAKEN frames, $GAP_COUNT gaps)"
