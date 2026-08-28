#!/usr/bin/env bash
# Boot the app under a PRIVATE, nested Xwayland (real DRI3 on the real GPU), drive it with real
# X11 input, and photograph the embedded browser's actual PAGE CONTENT — not just its chrome.
#
# This is the X11 sibling of wayland-drive.sh, built for exactly the gap that lane cannot close:
#
#     wayland-drive.sh   native Wayland client -> browser CHROME renders, the webview page does not
#     x11-nested-drive.sh private Xwayland     -> the whole app, INCLUDING the webview page, renders
#
# It does not touch DISPLAY=:1, the user's own desktop. It boots its own nested sway (wlroots
# headless backend, same trick wayland-drive.sh uses) with Xwayland ENABLED instead of disabled,
# giving Sirio a private `DISPLAY=:N` backed by a real GLES2 renderer against /dev/dri/renderD128
# — that renderer is what makes DRI3 (and therefore the browser's XCB/GLX window handle) work.
# Read docs/linux-rewrite/X11-NESTED-LANE.md before recording anything from here — especially the
# renderer trap: WLR_RENDERER=pixman (wayland-drive.sh's choice, correct for THAT lane) kills DRI3
# stone dead here with an opaque "No DRI3 support detected" from the app's own Vulkan init.
#
# Usage:
#   Scripts/x11-nested-drive.sh <outdir> '<actions>' [settle]
#
# Inside <actions> you get:
#   ctl <method> [k=v ...]   send a ControlRequest, print the JSON reply
#   click <x> <y>            left-click absolute window coordinates (real XTestFakeButtonEvent)
#   move <x> <y>             move the pointer to absolute window coordinates
#   rightclick <x> <y>       right-click absolute window coordinates
#   down <x> <y>             press and hold the left button (no release)
#   up <x> <y>               move here and release the left button — pair with `down` for a drag
#   drag <x1> <y1> <x2> <y2> [steps]   press, walk <steps> waypoints (default 4), release
#   scroll <x> <y> <steps>   move to (x,y), send <steps> wheel clicks (button 4/5); negative =
#                            opposite direction
#   type <text>              type text via XTestFakeKeyEvent (click a text target first)
#   key <name>               press a named key, e.g. key Return — X11 keysym names (man xdotool)
#   chord <mod> <key>        modifier-held named key, e.g. chord ctrl+shift p — xdotool's native
#                            `mod+mod+key` syntax, no separate press/release choreography needed
#   shot <name>              force a repaint, capture <outdir>/NN-<name>.png, print its colour count
#   $SOCK $DISPLAY $APP_LOG  socket path, the private X11 DISPLAY, the app's stdout+stderr
#
# NEVER call `shot` between a `down` and its matching `up` (or between `move` and a later
# `down`/`up` of the same gesture) -- see PLUS-MENU-INVESTIGATION.md and wayland-drive.sh's own
# usage header, which document this in full. `shot` here does the identical "resize to W2xH2 and
# back" nudge to force a repaint, and it is not a passive snapshot: resizing mid-gesture can move
# an open popup menu's anchor out from under an in-flight `down`. Capture state BEFORE a gesture
# and AFTER it completes, never in the middle of one.
#
# Example — open the browser, navigate to a local page, prove the PAGE rendered:
#   Scripts/x11-nested-drive.sh /tmp/x11-shots '
#     ctl browser.open url=about:blank
#     ctl tab.select index=3
#     ctl browser.navigate surface=surface:2 url=http://127.0.0.1:8971/probe.html
#     shot page-content
#   ' 12
#
# A real click+type gesture (not the socket) driving the same navigation:
#   Scripts/x11-nested-drive.sh /tmp/x11-shots '
#     click 584 89
#     key ctrl+a
#     type http://127.0.0.1:8971/probe.html
#     key Return
#     shot page-content-gesture
#   ' 12
#
# Env: TILLER_X11_LABEL   names this instance and all its /tmp paths (default: x11-$$).
#      TILLER_X11_BIN     drive a specific binary instead of rust/target/debug/sirio — pin a
#                          snapshot the way wayland-drive.sh's TILLER_WL_BIN does, so a builder
#                          rebuilding the shared target dir mid-drive cannot swap it under a critic.
#      TILLER_X11_KEEP=1  leave the compositor and app running after the actions finish.
#
# Exit: 0 ok · 2 no binary/tool · 3 compositor or Xwayland never came up · 4 app died ·
#       5 first frame blank
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTDIR="${1:?usage: x11-nested-drive.sh <outdir> '<actions>' [settle]}"
ACTIONS="${2:-}"
SETTLE="${3:-8}"
LABEL="${TILLER_X11_LABEL:-x11-$$}"
BIN="${TILLER_X11_BIN:-$ROOT/rust/target/debug/sirio}"
MIN_COLORS=200

SWAYSOCK="/tmp/$LABEL-sway.sock"
SWAYCONF="/tmp/$LABEL-sway.conf"
SWAYLOG="/tmp/$LABEL-sway.log"
SOCK="/tmp/$LABEL.sock"
APP_LOG="/tmp/$LABEL.log"
DB="/tmp/$LABEL.sqlite"
WINID=""

mkdir -p "$OUTDIR"
[ -x "$BIN" ] || { echo "FAIL: no binary at $BIN (cargo build -p sirio)" >&2; exit 2; }
for tool in xdotool import identify swaymsg sway; do
  command -v "$tool" >/dev/null || { echo "FAIL: $tool is not installed" >&2; exit 2; }
done

# Kill only processes whose environment names THIS instance — never match on process name alone,
# which would kill other agents' instances or (on a monitor-having box) the user's own X session.
# Linux caps `comm` at 15 chars and pgrep -x matches comm, so truncate the same way the kernel did
# (see wayland-drive.sh's kill_ours, which paid for this the hard way).
kill_ours() {
  local var="$1" want="$2" name="${3:0:15}" p
  for p in $(pgrep -x "$name" 2>/dev/null); do
    tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "$var=$want" && kill "$p" 2>/dev/null
  done
}
cleanup() {
  [ -n "${TILLER_X11_KEEP:-}" ] && {
    echo "NOTE: leaving $LABEL running (SOCK=$SOCK DISPLAY=$DISPLAY_N SOCK_SWAY=$SWAYSOCK)"
    return 0
  }
  kill_ours TILLER_SOCKET "$SOCK" "$(basename "$BIN")"
  kill_ours SWAYSOCK "$SWAYSOCK" sway
  return 0
}
trap cleanup EXIT

kill_ours TILLER_SOCKET "$SOCK" sirio
kill_ours SWAYSOCK "$SWAYSOCK" sway

W1=1280 H1=800          # sizes shot() alternates between to force a repaint (see the note below)
W2=1100 H2=700
OUTPUT_W="$W1" OUTPUT_H="$H1"

# `xwayland enable` is the whole point of this script — wayland-drive.sh disables it deliberately
# because THAT lane's app is a native Wayland client and a global Xwayland would only cost a race
# for /tmp/.X11-unix/XN between parallel instances. Here the private Xwayland IS the deliverable.
cat > "$SWAYCONF" <<EOF
xwayland enable
default_border none
default_floating_border none
gaps inner 0
gaps outer 0
output HEADLESS-1 resolution ${W1}x${H1}
EOF

# Xwayland's socket is bound close enough to instantly that it can already exist by the time a
# snapshot taken AFTER sway is confirmed up would look — that raced this script's first version
# into believing Xwayland never started (comm -13 saw the same file on both sides of the diff and
# reported no new socket). Snapshot /tmp/.X11-unix BEFORE sway launches at all, not after.
BEFORE_SOCKS="$(ls /tmp/.X11-unix/ 2>/dev/null)"

# WLR_RENDERER is intentionally left UNSET, not pixman. wayland-drive.sh sets pixman because its
# app never asks the compositor for a GPU-backed X11 pixmap. Ours does: DRI3 is served out of the
# COMPOSITOR's renderer, and pixman is software-only, so Sirio's Vulkan init fails at "No DRI3
# support detected" — a trap documented in X11-NESTED-LANE.md. Leaving WLR_RENDERER unset lets
# wlroots auto-select GLES2 against the real DRM render node, which is what DRI3 needs.
env -u WAYLAND_DISPLAY -u DISPLAY \
    XDG_RUNTIME_DIR=/run/user/"$(id -u)" \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
    SWAYSOCK="$SWAYSOCK" \
    sway -d -c "$SWAYCONF" >"$SWAYLOG" 2>&1 &

# `-d` is load-bearing (see wayland-drive.sh) — at sway's default log level nothing below INFO for
# errors is printed and this grep would silently match nothing.
WD=""
for _ in $(seq 1 60); do
  WD="$(sed -n "s/.*Running compositor on wayland display '\([^']*\)'.*/\1/p" "$SWAYLOG" | head -1)"
  [ -n "$WD" ] && break
  sleep 0.25
done
[ -n "$WD" ] || { echo "FAIL: sway never announced a display — see $SWAYLOG" >&2; tail -20 "$SWAYLOG" >&2; exit 3; }

export XDG_RUNTIME_DIR=/run/user/"$(id -u)"
export WAYLAND_DISPLAY="$WD"

# sway's Xwayland is started lazily on first client connect, but it reserves and binds its
# /tmp/.X11-unix/XN socket immediately at init — often within tens of milliseconds, well before
# any client asks for it. Diffing against the BEFORE_SOCKS snapshot taken above (before sway even
# launched) is a race-free way to learn N; waiting for the "Starting Xwayland on :N" log line
# instead only works once something has actually connected, which can be seconds late.
DISPLAY_N=""
for _ in $(seq 1 60); do
  AFTER_SOCKS="$(ls /tmp/.X11-unix/ 2>/dev/null)"
  NEW="$(comm -13 <(echo "$BEFORE_SOCKS" | sort) <(echo "$AFTER_SOCKS" | sort) | head -1)"
  if [ -n "$NEW" ]; then
    DISPLAY_N=":${NEW#X}"
    break
  fi
  sleep 0.25
done
[ -n "$DISPLAY_N" ] || { echo "FAIL: no new Xwayland socket appeared in /tmp/.X11-unix — see $SWAYLOG" >&2; exit 3; }
export DISPLAY="$DISPLAY_N" DISPLAY_N

verify_nested_x11() {
  [ "${DISPLAY:-}" = "$DISPLAY_N" ] || { echo "FAIL: DISPLAY is not this nested instance" >&2; return 1; }
  DISPLAY="$DISPLAY_N" timeout 10 xdotool getdisplaygeometry >/dev/null 2>&1 || {
    echo "FAIL: private Xwayland $DISPLAY_N is not answering — see $SWAYLOG" >&2
    return 1
  }
  swaymsg -s "$SWAYSOCK" -t get_version | EXPECTED_SWAYCONF="$SWAYCONF" python3 -c '
import json, os, sys
version = json.load(sys.stdin)
if version.get("loaded_config_file_name") != os.environ["EXPECTED_SWAYCONF"]:
    raise SystemExit("FAIL: swaymsg is not this nested compositor")
print("SAFE: sway {} on {}, Xwayland {}".format(version["human_readable"], version["loaded_config_file_name"], os.environ.get("DISPLAY_N", "")))
'
}
verify_nested_x11 || exit 3

# DISPLAY is set (deliberately, opposite of wayland-drive.sh): with it set, GPUI takes the X11
# path — that path is what gets the browser its window handle. WAYLAND_DISPLAY must NOT reach the
# app or GPUI could still prefer the Wayland backend it also sees on this same process's env.
env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_N" \
    XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
    TILLER_DB="$DB" TILLER_SOCKET="$SOCK" \
    "$BIN" >"$APP_LOG" 2>&1 &
APP_PID=$!

for _ in $(seq 1 120); do
  kill -0 "$APP_PID" 2>/dev/null || { echo "FAIL: app exited during startup — see $APP_LOG" >&2; tail -20 "$APP_LOG" >&2; exit 4; }
  [ -S "$SOCK" ] && break
  sleep 0.25
done
[ -S "$SOCK" ] || { echo "FAIL: no control socket at $SOCK in 30s" >&2; tail -20 "$APP_LOG" >&2; exit 4; }

if grep -q "No DRI3 support detected" "$APP_LOG" 2>/dev/null; then
  echo "FAIL: DRI3 unavailable — the nested compositor's renderer is not GPU-backed (check WLR_RENDERER is unset, not pixman)" >&2
  exit 3
fi

# The app's real toplevel, distinguished from GPUI's small 1x1 selection/DnD helper windows that
# also belong to this PID: --onlyvisible with a --pid filter returns exactly one window here,
# empirically, across every run this script was built and tested against.
for _ in $(seq 1 40); do
  WINID="$(DISPLAY="$DISPLAY_N" xdotool search --onlyvisible --pid "$APP_PID" 2>/dev/null | head -1)"
  [ -n "$WINID" ] && break
  sleep 0.25
done
[ -n "$WINID" ] || { echo "FAIL: no visible window found for pid $APP_PID on $DISPLAY_N" >&2; exit 3; }

export SOCK DISPLAY_N SWAYSOCK APP_LOG OUTDIR W1 H1 W2 H2 MIN_COLORS WINID

ctl() {
  local method="$1"; shift
  CTL_METHOD="$method" CTL_ARGS="$*" python3 - <<'PY'
import json, os, socket, sys
# Every param is a string — ControlRequest deserialises params as Map<String, String>.
params = {}
for pair in os.environ.get("CTL_ARGS", "").split():
    if "=" in pair:
        k, v = pair.split("=", 1)
        params[k] = v
req = {"id": "drive", "method": os.environ["CTL_METHOD"], "params": params}
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(15)
try:
    s.connect(os.environ["SOCK"])
    s.sendall((json.dumps(req) + "\n").encode())
    buf = b""
    while b"\n" not in buf:
        chunk = s.recv(65536)
        if not chunk:
            break
        buf += chunk
except Exception as e:
    print(f"CTL-FAIL {os.environ['CTL_METHOD']}: {e}", file=sys.stderr)
    sys.exit(1)
print(buf.decode(errors="replace").split("\n")[0][:3000])
PY
}

xdo() { DISPLAY="$DISPLAY_N" timeout 10 xdotool "$@"; }

move() { xdo mousemove --window "$WINID" "$1" "$2"; }
click() { xdo mousemove --window "$WINID" "$1" "$2" click 1; }
rightclick() { xdo mousemove --window "$WINID" "$1" "$2" click 3; }
down() { xdo mousemove --window "$WINID" "$1" "$2" mousedown 1; }
up() { xdo mousemove --window "$WINID" "$1" "$2" mouseup 1; }

# drag <x1> <y1> <x2> <y2> [steps] — press, walk <steps> waypoints so the target sees real
# intermediate motion (a teleport is what many drop targets ignore), release.
drag() {
  local x1="$1" y1="$2" x2="$3" y2="$4" steps="${5:-4}" i sx sy
  down "$x1" "$y1" || return 1
  sleep 0.05
  for ((i = 1; i <= steps; i++)); do
    sx=$(( x1 + (x2 - x1) * i / steps ))
    sy=$(( y1 + (y2 - y1) * i / steps ))
    move "$sx" "$sy" || return 1
    sleep 0.03
  done
  sleep 0.05
  up "$x2" "$y2"
}

# scroll <x> <y> <steps> — button 4/5 clicks, matching X11's classic wheel convention (4 = up,
# 5 = down). Negative <steps> reverses direction. Sign convention unverified against a live
# control until driven — record what you observed, same caveat as wayland-drive.sh's scroll.
scroll() {
  local x="$1" y="$2" steps="$3" btn=4 n=$3 i
  [ "$steps" -lt 0 ] && { btn=5; n=$(( -steps )); }
  move "$x" "$y" || return 1
  for ((i = 0; i < n; i++)); do
    xdo click "$btn" || return 1
  done
}

type() {
  [ "$#" -ge 1 ] || { echo "usage: type <text>" >&2; return 2; }
  xdo type --window "$WINID" --delay 20 -- "$*"
}

key() {
  [ "$#" = 1 ] || { echo "usage: key <name>" >&2; return 2; }
  xdo key --window "$WINID" "$1"
}

# chord <mod[+mod...]> <key> — xdotool's native modifier syntax takes this in one call, e.g.
# `chord ctrl+shift p` -> `xdotool key ctrl+shift+p`. No separate press/hold/release choreography
# needed the way wtype (the Wayland lane's injector) requires.
chord() {
  [ "$#" = 2 ] || { echo "usage: chord <mod[+mod...]> <key>" >&2; return 2; }
  xdo key --window "$WINID" "${1}+${2}"
}

# Repaint is force-checked here the same way wayland-drive.sh's shot() does it: nudge the output
# to a different size and back so a real configure/expose cycle happens before capture, rather than
# trusting that the socket call or gesture already produced a fresh frame. Unlike the Wayland lane,
# this app's manual testing did not reproduce a stale-frame capture without the nudge — but the
# nudge is cheap insurance against the same class of false negative, so it stays in.
SHOT_N=0
shot() {
  local name="${1:-shot}"
  SHOT_N=$((SHOT_N + 1))
  swaymsg -s "$SWAYSOCK" output HEADLESS-1 resolution "${W2}x${H2}" >/dev/null 2>&1
  sleep 0.4
  swaymsg -s "$SWAYSOCK" output HEADLESS-1 resolution "${W1}x${H1}" >/dev/null 2>&1
  sleep "${TILLER_X11_REPAINT_SETTLE:-0.8}"
  local path
  path="$(printf '%s/%02d-%s.png' "$OUTDIR" "$SHOT_N" "$name")"
  DISPLAY="$DISPLAY_N" import -window "$WINID" "$path" 2>/dev/null
  local colors
  colors="$(identify -format '%k' "$path" 2>/dev/null || echo 0)"
  if [ "${colors:-0}" -lt "$MIN_COLORS" ]; then
    echo "WARN $path is blank or near-blank ($colors colours)" >&2
    return 1
  fi
  echo "SHOT $path ($colors colours)"
}
export -f ctl xdo move click rightclick down up drag scroll type key chord shot

sleep "$SETTLE"
shot baseline || { echo "FAIL: first frame is blank — presentation is broken, not layout. See $APP_LOG" >&2; exit 5; }

eval "$ACTIONS"
rc=$?

kill -0 "$APP_PID" 2>/dev/null || { echo "FAIL: app died during the actions — see $APP_LOG" >&2; tail -20 "$APP_LOG" >&2; exit 4; }
echo "OK   $SHOT_N frames in $OUTDIR (label=$LABEL display=$DISPLAY_N socket=$SOCK)"
exit $rc
