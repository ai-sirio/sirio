#!/usr/bin/env bash
# Launch the app, DRIVE it, then photograph the result.
#
# linux-shot.sh only ever captures the app's opening state, which is not enough to judge
# anything: the app restores its last session, so a screenshot named "chat" can easily show a
# terminal. To exercise a feature you have to operate the program — click the tab, type in the
# composer, press the key — and only then take the picture.
#
# You supply a snippet of shell run against the live window. Inside it you get:
#     $WID    the app window id        $DISP   the X display
#     key / type / click / rclick / drag / shot   helpers, defined below
#                                          (rclick = right-click, button 3)
#                                          (drag x1 y1 x2 y2 [steps] = mousedown, several
#                                           intermediate mousemoves, mouseup)
#
# Example — open the second tab, type a message, send it, wait, and capture:
#   Scripts/linux-drive.sh out.png '
#     click 470 60          # the Chat tab
#     sleep 1
#     click 800 700         # the composer
#     type "hello, are you there?"
#     key Return
#     sleep 20              # let the agent answer
#   '
#
# Only one agent may drive at a time, and this script now enforces that with a real lock —
# see the block below. Set SIRIO_DRIVE_LABEL so a waiter can see who is holding it.
#
# Exit: 0 captured · 2 no binary · 3 no window/display · 4 app died · 5 frame blank
#       6 gave up waiting for the drive lock
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:?usage: linux-drive.sh <out.png> '<actions>' [settle] [display]}"
ACTIONS="${2:-}"
SETTLE="${3:-6}"
DISP="${4:-:1}"
BIN="$ROOT/rust/target/debug/sirio"
LOG="${OUT%.png}.log"
MIN_COLORS=200

mkdir -p "$(dirname "$OUT")"
[ -x "$BIN" ] || { echo "FAIL: no binary at $BIN" >&2; exit 2; }
command -v xdotool >/dev/null || { echo "FAIL: xdotool is not installed" >&2; exit 3; }
timeout 10 env DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 || { echo "FAIL: no X server on $DISP" >&2; exit 3; }

# ---------------------------------------------------------------------------
# The drive lock. There is one X pointer on the display and it is global.
#
# This script already goes to some trouble to photograph the right window
# (_NET_WM_PID, below) — but that only settles which window is PHOTOGRAPHED,
# never which window RECEIVES THE CLICK. xdotool moves the one shared pointer,
# and the click lands on whatever window is under it. So two agents driving at
# once corrupt each other's input while each still captures its own window
# perfectly: the frame shows a control that did nothing, about code that is
# fine. That is a false negative, and it is indistinguishable by inspection
# from a real one.
#
# This was not hypothetical. On 2026-08-14 the orchestrator drove six captures
# while the critic was mid-batch; the same right-click-then-Open sequence
# opened an editor in one frame and left the menu sitting open in the next.
#
# A documented convention would not have prevented that, because the agent
# breaking it did not know the rule existed. So it is a mutex.
#
# It is NOT flock. The first version of this was `flock` on fd 9, and within
# the hour it deadlocked the whole roster: fd 9 was inherited by a long-lived,
# reparented `claude` process, so the kernel held the lock for a run that had
# finished, and no diagnosis from outside could release it. Agent CLIs fork and
# reparent constantly, which makes any fd-inheritance-based lock fragile here.
#
# **A stuck lock is worse than no lock.** No lock costs you the occasional
# corrupted capture; a stuck lock blocks every drive, and driving is the
# project's rate limit. So this lock is self-healing: it records its holder and
# breaks itself if that holder is gone or has held it implausibly long.
# ---------------------------------------------------------------------------
LOCKDIR="${SIRIO_DRIVE_LOCK:-/tmp/sirio-drive$(printf '%s' "$DISP" | tr -c 'a-zA-Z0-9' '-').lockd}"
LOCK_WAIT="${SIRIO_DRIVE_LOCK_WAIT:-900}"
LOCK_STALE="${SIRIO_DRIVE_LOCK_STALE:-1800}"
WE_HOLD_LOCK=""

cleanup() {
  [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
  [ -n "$WE_HOLD_LOCK" ] && rm -rf "$LOCKDIR"
  return 0
}
trap cleanup EXIT

lock_deadline=$(( SECONDS + LOCK_WAIT ))
while :; do
  # mkdir is atomic and owns no file descriptor, so nothing can inherit it.
  if mkdir "$LOCKDIR" 2>/dev/null; then
    WE_HOLD_LOCK=1
    printf 'pid=%s label=%s since=%s out=%s\n' \
      "$$" "${SIRIO_DRIVE_LABEL:-unlabelled}" "$(date -Is)" "$OUT" >"$LOCKDIR/holder"
    break
  fi

  holder="$(cat "$LOCKDIR/holder" 2>/dev/null || echo 'unknown')"
  hpid="$(sed -n 's/^pid=\([0-9]*\).*/\1/p' "$LOCKDIR/holder" 2>/dev/null)"

  # Self-heal 1: the recorded holder is gone. Its drive died without cleaning up.
  if [ -n "$hpid" ] && ! kill -0 "$hpid" 2>/dev/null; then
    echo "NOTE: breaking a drive lock whose holder (pid $hpid) no longer exists" >&2
    rm -rf "$LOCKDIR"; continue
  fi

  # Self-heal 2: held implausibly long. No legitimate drive batch runs this far.
  held_since="$(stat -c %Y "$LOCKDIR" 2>/dev/null || date +%s)"
  held_for=$(( $(date +%s) - held_since ))
  if [ "$held_for" -gt "$LOCK_STALE" ]; then
    echo "NOTE: breaking a drive lock held ${held_for}s (stale after ${LOCK_STALE}s): $holder" >&2
    rm -rf "$LOCKDIR"; continue
  fi

  if [ "$SECONDS" -ge "$lock_deadline" ]; then
    echo "FAIL: waited ${LOCK_WAIT}s for the drive lock on $DISP and gave up." >&2
    echo "      Holder: $holder" >&2
    echo "      This is a mutex, not a hang — two drivers share one X pointer and" >&2
    echo "      corrupt each other's clicks. Wait, or ask the holder to yield." >&2
    exit 6
  fi
  sleep 3
done

env -u WAYLAND_DISPLAY DISPLAY="$DISP" "$BIN" >"$LOG" 2>&1 &
APP_PID=$!
# The EXIT trap is already installed above (cleanup) and kills APP_PID as well
# as releasing the lock — it must not be replaced here, or the lock leaks.

# The app sets no WM_NAME, so it cannot be found by title — but "the largest window on the display"
# is worse than useless here, because several agents run this app on :1 simultaneously. Driving a
# stranger's instance is the quiet failure this caused: keystrokes go to somebody else's build, your
# own window never changes, and you report "input does not work" about code that is fine. So match
# _NET_WM_PID against the process we launched, and only fall back to geometry with a loud warning.
export APP_PID DISP
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
    if fallback is None or w*h > fallback[1]: fallback = (wid, w*h)
    try:
        out = subprocess.run(['xprop', '-display', disp, '-id', wid, '_NET_WM_PID'],
                             capture_output=True, text=True, timeout=5).stdout
        pid = int(out.rsplit('=', 1)[-1].strip())
    except Exception:
        continue
    if pid == want and (best is None or w*h > best[1]): best = (wid, w*h)
print(best[0] if best else ('' if os.environ.get('STRICT_PID') else (fallback or ('',))[0]))
"
}

# The app sets no WM_NAME, so find it by geometry: the largest child of the root.
WID=""
for _ in $(seq 1 120); do
  kill -0 "$APP_PID" 2>/dev/null || { echo "FAIL: app exited during startup — see $LOG" >&2; tail -20 "$LOG" >&2; exit 4; }
  WID="$(STRICT_PID=1 find_window)"
  [ -n "$WID" ] && break
  sleep 0.25
done
if [ -z "$WID" ]; then
  WID="$(find_window)"
  [ -n "$WID" ] && echo "WARN: no window advertised our pid ($APP_PID); driving the largest window" >&2
  [ -n "$WID" ] && echo "      on $DISP — it may belong to another running instance." >&2
fi
[ -n "$WID" ] || { echo "FAIL: mapped no window in 30s" >&2; tail -20 "$LOG" >&2; exit 3; }

export WID DISP OUT
sleep "$SETTLE"                                   # first paint before we touch anything
timeout 10 env DISPLAY="$DISP" xdotool windowactivate --sync "$WID" 2>/dev/null
timeout 10 env DISPLAY="$DISP" xdotool windowfocus --sync "$WID" 2>/dev/null

# You give window-relative coordinates — that is what you can read off a screenshot — and these
# helpers translate to absolute screen coordinates before injecting.
#
# Why not xdotool's own --window targeting: it delivers through XSendEvent and warps relative to
# the window, and neither survives here. This app is an X11 guest of a Wayland compositor, so X
# focus cannot be forced onto it and synthetic events aimed at the window id are dropped. Aiming
# at absolute screen position and letting XTEST deliver it is the only route that lands. The cost
# is real: this moves the operator's actual pointer, so keep runs short.
# Resolve the window's screen origin AFTER the settle, and refuse to continue without it.
#
# This is a safety gate, not a nicety. The click helper computes $(( WIN_X + x )), and in shell an
# empty variable is zero — so a failed lookup silently turns every window-relative click into a
# click measured from the top-left of the *screen*, landing on whatever else the operator has
# open. It happened: the app replaced its window between discovery and inspection, xwininfo
# answered "No such window", and the run clicked its way across someone else's desktop while
# reporting success. Injecting input at coordinates we cannot vouch for is worse than not running.
WIN_X="" WIN_Y=""
for _ in $(seq 1 12); do
  geom="$(timeout 10 env DISPLAY="$DISP" xwininfo -id "$WID" 2>/dev/null)" || geom=""
  WIN_X="$(printf '%s' "$geom" | awk '/Absolute upper-left X/ {print $NF}')"
  WIN_Y="$(printf '%s' "$geom" | awk '/Absolute upper-left Y/ {print $NF}')"
  [ -n "$WIN_X" ] && [ -n "$WIN_Y" ] && break
  WID="$(find_window)"          # the app may have replaced its window; re-discover and retry
  sleep 0.5
done
case "$WIN_X$WIN_Y" in
  *[!0-9-]* | "") echo "FAIL: could not resolve window origin — refusing to inject clicks blind" >&2; exit 3 ;;
esac

# LOG must be exported too: shot() redirects stderr into it, and an unexported LOG expands to an
# empty filename inside the actions subshell, which fails every intermediate capture while the
# final one — taken by the parent — still succeeds. That asymmetry looks like "only some shots
# work" and wastes a run to diagnose.
export WIN_X WIN_Y LOG WID

key()   { timeout 10 env DISPLAY="$DISP" xdotool key --clearmodifiers "$@"; sleep 0.2; }
type()  { timeout 10 env DISPLAY="$DISP" xdotool type --clearmodifiers --delay 25 "$1"; sleep 0.2; }
click() { timeout 10 env DISPLAY="$DISP" xdotool mousemove --sync $(( WIN_X + $1 )) $(( WIN_Y + $2 )); sleep 0.2; timeout 10 env DISPLAY="$DISP" xdotool click 1; sleep 0.4; }
# Right-click. The harness had no button-3 path at all until now, and its
# absence was being read as "XTEST cannot deliver a right-click under
# XWayland" — a much more pessimistic conclusion, which would have made the
# context-menu rows permanently unverifiable. Button 1 travels this exact
# route and lands, and XWayland does not discriminate by button for a focused
# X client, so the same mousemove-then-XTEST sequence is used verbatim.
rclick() { timeout 10 env DISPLAY="$DISP" xdotool mousemove --sync $(( WIN_X + $1 )) $(( WIN_Y + $2 )); sleep 0.2; timeout 10 env DISPLAY="$DISP" xdotool click 3; sleep 0.4; }
# Drag from (x1,y1) to (x2,y2), window-relative, same coordinate convention as click/rclick.
# GPUI (and most toolkits) tell a real drag from a click by motion while the button stays down,
# so this is button-down, several intermediate mousemove --sync steps (not one jump — a
# same-frame down-then-up-elsewhere is indistinguishable from a click-then-click to the
# recipient), then button-up. Steps default to 8, which was enough to carry a drag payload
# (e.g. a Files-panel row's path) onto a drop target in manual verification; override with a
# 5th argument if a particular drag needs finer-grained motion.
drag()  {
  local x1=$1 y1=$2 x2=$3 y2=$4 steps="${5:-8}"
  timeout 10 env DISPLAY="$DISP" xdotool mousemove --sync $(( WIN_X + x1 )) $(( WIN_Y + y1 )); sleep 0.2
  timeout 10 env DISPLAY="$DISP" xdotool mousedown 1; sleep 0.2
  local i sx sy
  for i in $(seq 1 "$steps"); do
    sx=$(( x1 + (x2 - x1) * i / steps ))
    sy=$(( y1 + (y2 - y1) * i / steps ))
    timeout 10 env DISPLAY="$DISP" xdotool mousemove --sync $(( WIN_X + sx )) $(( WIN_Y + sy )); sleep 0.05
  done
  sleep 0.2
  timeout 10 env DISPLAY="$DISP" xdotool mouseup 1; sleep 0.4
}
shot()  { timeout 30 env DISPLAY="$DISP" import -window "$WID" "${1:-$OUT}" 2>>"$LOG"; }
export -f key type click rclick drag shot

if [ -n "$ACTIONS" ]; then
  echo "--- driving ---"
  bash -c "$ACTIONS" || echo "WARN: an action returned non-zero; continuing to capture anyway" >&2
fi

shot "$OUT"
[ -s "$OUT" ] || { echo "FAIL: capture produced nothing" >&2; exit 3; }
COLORS="$(identify -format '%k' "$OUT" 2>/dev/null || echo 0)"
[ "${COLORS:-0}" -ge "$MIN_COLORS" ] || { echo "FAIL: frame is blank ($COLORS colours) — presentation is broken, not layout" >&2; exit 5; }

echo "OK   $OUT  ($(identify -format '%wx%h' "$OUT") · $COLORS colours · window $WID)"
echo "log  $LOG"
