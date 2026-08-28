#!/usr/bin/env bash
# Launch the Linux build and photograph it. This is the critic's instrument.
#
# It must answer one question honestly — did the app put real pixels on a screen? — and it must
# not be fooled by the three ways this app has already failed to do so while looking healthy:
#
#   1. No windowing backend compiled in. gpui's "wayland"/"x11" features are on by default, so
#      `default-features = false` silently removes them; gpui::guess_compositor() then reads
#      neither $DISPLAY nor $WAYLAND_DISPLAY, answers "Headless", and the app runs a perfectly
#      healthy event loop forever with no window. Nothing is logged. Nothing crashes.
#   2. A display without DRI3. Xvfb has no DRI3, and Vulkan requires it to present on X11, so
#      the window maps and every frame is dropped — you capture a uniform black rectangle and,
#      unless you check, you call it a pass. Hence the flatness guard below. Xwayland is a real
#      X server backed by the compositor, so it has DRI3 and the GPU; that is why we use it.
#   3. An unnamed window. The app sets no WM_NAME, so searching by title finds nothing even
#      though the window is right there. We match on geometry instead: the largest child.
#
# Every X client below runs under `timeout`, and that is not defensive padding.
#
# A wedged X server — one whose Xwayland is alive and sitting in ep_poll while its compositor can no
# longer import its buffers — still ACCEPTS the connection and then never answers. So xdpyinfo,
# xwininfo, xprop, xrandr, xdotool and import all block forever, with no timeout of their own and no
# output to say why. On 13/08 this cost the critic 1h26m: it sat inside a single `import -window`
# call, made no progress, and looked from the outside exactly like an agent thinking hard.
#
# A hang is the worst failure mode a tool can have, because unlike an error it produces no evidence
# and no deadline. Anything that talks to an X server here gets a deadline.
#
# Usage:  Scripts/linux-shot.sh <out.png> [seconds-to-settle] [display]
# Exit:   0 shot taken · 2 binary missing · 3 no window · 4 app died · 5 frame is blank
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/artifacts/shot.png}"
SETTLE="${2:-6}"
DISP="${3:-:1}"
BIN="$ROOT/rust/target/debug/tiller"
LOG="${OUT%.png}.log"
MIN_COLORS=200          # a real UI frame has thousands; a dropped-frame capture has 1

mkdir -p "$(dirname "$OUT")"
[ -x "$BIN" ] || { echo "FAIL: no binary at $BIN — build it first" >&2; exit 2; }

# Reuse an EXISTING display by default. Do not create one.
#
# This script used to spin up its own Xwayland whenever no display was named, for isolation. On this
# machine that now guarantees a blank frame, and the reason is worth writing down because it is not
# guessable from the symptom:
#
#   cosmic-comp: Failed to render texture … import for wrong devices DrmNode { dev: 57984 } …
#                modifier: Unrecognized(144115188076389125)
#
# 57984 is renderD128, the only render node; 0x0200000000000005 is an AMD tiling modifier the
# compositor's renderer does not recognise, so it cannot import the buffer the app rendered into.
# Xwayland negotiates its dmabuf modifier set once, at startup, and keeps it — so displays created
# BEFORE the mismatch appeared still present, and every display created after it never will.
#
# Measured 13/08 ~10:00: three consecutive fresh Xwaylands → 1 colour each. `:1`, alive since the
# session began → 8787 colours. Forcing software Vulkan (VK_DRIVER_FILES=…/lvp_icd.json) does not
# help; neither does Xvfb, which additionally has no DRI3 at all.
#
# So: default to `:1`, and only create a private server if the caller explicitly asks, which is
# worth keeping for the day the compositor is restarted and fresh displays work again.
#
# `xrandr --output XWAYLAND0 --mode` stays essential either way: a nested Xwayland sits at an
# unconfigured 640x480 until told otherwise. `:1` was found at 640x480 and enlarged with exactly
# this call, after which it still presented — resizing does not lose the good modifier set.
OWN_DISPLAY=""
if [ -n "${TILLER_NEW_DISPLAY:-}" ]; then
  echo "note: creating a private X server because TILLER_NEW_DISPLAY is set. On this machine" >&2
  echo "      freshly created Xwaylands do not present — expect a blank frame and exit 5." >&2
  for n in $(seq 60 79); do [ -e "/tmp/.X11-unix/X$n" ] || { DISP=":$n"; break; }; done
  WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}" Xwayland "$DISP" -ac -noreset -nolisten tcp \
    >"${OUT%.png}.xserver.log" 2>&1 &
  OWN_DISPLAY=$!
  for _ in $(seq 1 60); do timeout 10 env DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 && break; sleep 0.25; done
fi
# Whatever display we ended up with, make sure its output is a usable size.
timeout 10 env DISPLAY="$DISP" xrandr --output XWAYLAND0 --mode "${GEOMETRY:-1440x900}" >/dev/null 2>&1 || true
trap 'kill "${APP_PID:-}" "${OWN_DISPLAY:-}" 2>/dev/null' EXIT

timeout 10 env DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 || { echo "FAIL: no X server on $DISP" >&2; exit 3; }

# WAYLAND_DISPLAY must be absent or GPUI prefers Wayland and ignores $DISPLAY entirely.
#
# GPUI_X11_SCALE_FACTOR=1 pins the scale. Without it the app inherits whatever the current Xwayland
# advertises, which after the session's X server restarted came back as a fractional factor: the
# window then reported geometry like 3798x2152+-937+-536 — negative origins and double the real
# size — so a root crop of that rectangle lands off-screen and returns black. Pinning the factor
# also keeps captures comparable between runs, which matters when the whole method is putting two
# frames side by side.
env -u WAYLAND_DISPLAY DISPLAY="$DISP" GPUI_X11_SCALE_FACTOR=1 "$BIN" >"$LOG" 2>&1 &
APP_PID=$!
export APP_PID DISP
trap 'kill "${APP_PID:-}" "${OWN_DISPLAY:-}" 2>/dev/null' EXIT

# Find OUR window, not merely a big one.
#
# Several agents run this app on the shared :1 display at the same time, so "largest child of the
# root" happily returns somebody else's instance — and then you photograph their build, compare it
# against your change, and conclude something false about code you never ran. It happened twice
# before this filter existed: an identical window id came back across separate launches, which is
# the tell. So candidates are matched against our own process id via _NET_WM_PID first, and the
# geometry heuristic is only a fallback for when the property is missing.
find_window() {
  timeout 10 env DISPLAY="$DISP" xwininfo -root -children 2>/dev/null | python3 -c "
import re, subprocess, sys, os
want = int(os.environ.get('APP_PID', '0'))
disp = os.environ.get('DISP', ':1')
best = fallback = None
for line in sys.stdin:
    m = re.match(r'\s+(0x[0-9a-f]+).*?\s(\d+)x(\d+)\+', line)
    if not m: continue
    wid, w, h = m.group(1), int(m.group(2)), int(m.group(3))
    if w <= 800 or h <= 500: continue
    if fallback is None or w * h > fallback[1]: fallback = (wid, w * h)
    try:
        out = subprocess.run(['xprop', '-display', disp, '-id', wid, '_NET_WM_PID'],
                             capture_output=True, text=True, timeout=5).stdout
        pid = int(out.rsplit('=', 1)[-1].strip())
    except Exception:
        continue
    if pid == want and (best is None or w * h > best[1]): best = (wid, w * h)
# Strict mode prints only a PID-matched window; the geometry fallback is offered separately so the
# caller can decide, rather than silently accepting a stranger's window on the first poll.
print(best[0] if best else ('' if os.environ.get('STRICT_PID') else (fallback or ('',))[0]))
"
}

WID=""
for _ in $(seq 1 120); do
  kill -0 "$APP_PID" 2>/dev/null || { echo "FAIL: app exited during startup — see $LOG" >&2; tail -20 "$LOG" >&2; exit 4; }
  WID="$(STRICT_PID=1 find_window)"
  [ -n "$WID" ] && break
  sleep 0.25
done
if [ -z "$WID" ]; then
  # No window carried our pid. Before declaring the gap, allow the geometry match — some setups do
  # not set _NET_WM_PID at all — but say so out loud, because on this machine several instances
  # share the display and an unverified window may belong to someone else's build.
  WID="$(find_window)"
  [ -n "$WID" ] && echo "WARN: no window advertised our pid ($APP_PID); falling back to the largest" >&2
  [ -n "$WID" ] && echo "      window on $DISP — this frame may belong to another running instance." >&2
fi
[ -n "$WID" ] || { echo "FAIL: alive but mapped no window in 30s — this IS the gap" >&2; tail -20 "$LOG" >&2; exit 3; }

sleep "$SETTLE"   # first paint and async layout

# Capture, with a fallback that matters.
#
# `import -window <id>` reads the window's own pixmap, and on some Xwayland instances the app
# presents directly to the compositor without keeping one — so the call succeeds and returns a
# uniformly black image. That is indistinguishable from "the UI failed to draw" unless you look.
# When it happens, crop the window's rectangle out of the root instead, which reads what is
# actually on screen. Try the cheap path first, check flatness, then fall back.
capture() { timeout 30 env DISPLAY="$DISP" import "$@" 2>>"$LOG"; }
flat() { [ "$(identify -format '%k' "$1" 2>/dev/null || echo 0)" -lt "$MIN_COLORS" ]; }

capture -window "$WID" "$OUT"
if [ ! -s "$OUT" ] || flat "$OUT"; then
  geom="$(timeout 10 env DISPLAY="$DISP" xwininfo -id "$WID" 2>/dev/null)"
  gx="$(printf '%s' "$geom" | awk '/Absolute upper-left X/ {print $NF}')"
  gy="$(printf '%s' "$geom" | awk '/Absolute upper-left Y/ {print $NF}')"
  gw="$(printf '%s' "$geom" | awk '/Width:/ {print $NF}')"
  gh="$(printf '%s' "$geom" | awk '/Height:/ {print $NF}')"
  if [ -n "$gx$gy$gw$gh" ]; then
    echo "note: window pixmap was blank; cropping ${gw}x${gh}+${gx}+${gy} out of the root instead" >&2
    capture -window root -crop "${gw}x${gh}+${gx}+${gy}" "$OUT"
  fi
fi

[ -s "$OUT" ] || { echo "FAIL: capture produced nothing" >&2; exit 3; }
COLORS="$(identify -format '%k' "$OUT" 2>/dev/null || echo 0)"
if [ "${COLORS:-0}" -lt "$MIN_COLORS" ]; then
  echo "FAIL: both the window pixmap and the root crop came back blank ($COLORS distinct" >&2
  echo "      colours) — presentation is broken, not layout. Check DRI3 and the Vulkan" >&2
  echo "      driver in $LOG." >&2
  exit 5
fi

echo "OK   $OUT  ($(identify -format '%wx%h' "$OUT") · $COLORS colours · window $WID)"
echo "log  $LOG"
