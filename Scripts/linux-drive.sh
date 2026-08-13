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
#     key / type / click / shot        helpers, defined below
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
# Exit: 0 captured · 2 no binary · 3 no window/display · 4 app died · 5 frame blank
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:?usage: linux-drive.sh <out.png> '<actions>' [settle] [display]}"
ACTIONS="${2:-}"
SETTLE="${3:-6}"
DISP="${4:-:1}"
BIN="$ROOT/rust/target/debug/tiller"
LOG="${OUT%.png}.log"
MIN_COLORS=200

mkdir -p "$(dirname "$OUT")"
[ -x "$BIN" ] || { echo "FAIL: no binary at $BIN" >&2; exit 2; }
command -v xdotool >/dev/null || { echo "FAIL: xdotool is not installed" >&2; exit 3; }
timeout 10 env DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 || { echo "FAIL: no X server on $DISP" >&2; exit 3; }

env -u WAYLAND_DISPLAY DISPLAY="$DISP" "$BIN" >"$LOG" 2>&1 &
APP_PID=$!
trap 'kill "$APP_PID" 2>/dev/null' EXIT

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
shot()  { timeout 30 env DISPLAY="$DISP" import -window "$WID" "${1:-$OUT}" 2>>"$LOG"; }
export -f key type click shot

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
