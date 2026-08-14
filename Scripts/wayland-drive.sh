#!/usr/bin/env bash
# Boot the app under a nested headless Wayland compositor, DRIVE it over the control socket,
# and photograph the result — with no X display and no drive lock.
#
# This is the sibling of linux-drive.sh. The difference that matters:
#
#     linux-drive.sh   one shared X pointer  -> a global mutex, real clicks, one driver at a time
#     wayland-drive.sh no pointer at all     -> no lock, socket calls only, any number in parallel
#
# So this lane is for rows whose evidence is "the app entered this state and rendered it", and the
# X lane is for rows whose evidence is "a person clicked this". Read WAYLAND-LANE.md before
# recording anything from here — especially the paragraph about which half of a row you proved.
#
# Usage:
#   Scripts/wayland-drive.sh <outdir> '<actions>' [settle]
#
# Inside <actions> you get:
#   ctl <method> [k=v ...]   send a ControlRequest, print the JSON reply
#   shot <name>              force a repaint, capture <outdir>/NN-<name>.png, print its colour count
#   $SOCK $WD $APP_LOG       socket path, wayland display, the app's stdout+stderr
#
# Example — open Settings, switch section, and photograph both:
#   Scripts/wayland-drive.sh /tmp/shots '
#     ctl surface.settings.open
#     shot settings-appearance
#     ctl surface.settings.select section=general
#     shot settings-general
#   '
#
# Env: TILLER_WL_LABEL  names this instance and all its /tmp paths (default: wl-$$).
#      TILLER_WL_KEEP=1 leaves the compositor and app running after the actions finish.
#
# Exit: 0 ok · 2 no binary · 3 compositor never came up · 4 app died · 5 first frame blank
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTDIR="${1:?usage: wayland-drive.sh <outdir> '<actions>' [settle]}"
ACTIONS="${2:-}"
SETTLE="${3:-6}"
LABEL="${TILLER_WL_LABEL:-wl-$$}"
BIN="$ROOT/rust/target/debug/tiller"
MIN_COLORS=200

# Sockets live under /tmp because sockaddr_un caps a path at 108 bytes and the agents' scratchpad
# paths overflow it — the failure is an opaque bind error a long way from the cause.
SWAYSOCK="/tmp/$LABEL-sway.sock"
SWAYCONF="/tmp/$LABEL-sway.conf"
SWAYLOG="/tmp/$LABEL-sway.log"
SOCK="/tmp/$LABEL.sock"
APP_LOG="/tmp/$LABEL.log"
DB="/tmp/$LABEL.sqlite"

mkdir -p "$OUTDIR"
[ -x "$BIN" ] || { echo "FAIL: no binary at $BIN (cargo build -p tiller)" >&2; exit 2; }
command -v grim >/dev/null || { echo "FAIL: grim is not installed" >&2; exit 3; }

# Kill only processes whose environment names THIS instance. Matching on the process name would
# kill other agents' instances — and, on a machine where the user runs a Wayland session, theirs.
kill_ours() {
  local var="$1" want="$2" name="$3" p
  for p in $(pgrep -x "$name" 2>/dev/null); do
    tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "$var=$want" && kill "$p" 2>/dev/null
  done
}
cleanup() {
  [ -n "${TILLER_WL_KEEP:-}" ] && { echo "NOTE: leaving $LABEL running (SOCK=$SOCK WAYLAND_DISPLAY=$WD)"; return 0; }
  kill_ours TILLER_SOCKET "$SOCK" tiller
  kill_ours SWAYSOCK "$SWAYSOCK" sway
  return 0
}
trap cleanup EXIT

kill_ours TILLER_SOCKET "$SOCK" tiller
kill_ours SWAYSOCK "$SWAYSOCK" sway

W1=1715 H1=972          # the two sizes shot() alternates between; see the repaint note below
W2=1400 H2=900
# Xwayland off: the app is a native Wayland client here, and Xwayland claims a global
# /tmp/.X11-unix/XN. Two instances of this script race for the same number and the loser refuses
# to start at all — which is fatal to the one property this lane is for, running in parallel.
cat > "$SWAYCONF" <<EOF
xwayland disable
default_border none
default_floating_border none
gaps inner 0
gaps outer 0
output HEADLESS-1 resolution ${W1}x${H1}
EOF

env -u WAYLAND_DISPLAY -u DISPLAY \
    XDG_RUNTIME_DIR=/run/user/"$(id -u)" \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
    SWAYSOCK="$SWAYSOCK" \
    sway -d -c "$SWAYCONF" >"$SWAYLOG" 2>&1 &

# `-d` is load-bearing, not diagnostic noise: "Running compositor on wayland display" is an INFO
# line, and at sway's default level the log carries errors only. Without it the log is silent and
# there is no way to learn which display this instance took.
#
# sway picks the first free wayland-N — it skips any whose lockfile is held, so parallel instances
# land on different numbers. Assuming wayland-1 is how you end up driving another agent's
# compositor, so read back the one it actually chose.
WD=""
for _ in $(seq 1 60); do
  WD="$(sed -n "s/.*Running compositor on wayland display '\([^']*\)'.*/\1/p" "$SWAYLOG" | head -1)"
  [ -n "$WD" ] && break
  sleep 0.25
done
[ -n "$WD" ] || { echo "FAIL: sway never announced a display — see $SWAYLOG" >&2; tail -20 "$SWAYLOG" >&2; exit 3; }

# DISPLAY must be UNSET, not empty: with it set at all, GPUI takes the X11 path, which under
# Xvfb/Xephyr has no DRI3 route and presents nothing. A blank frame here is almost always this.
env -u DISPLAY \
    XDG_RUNTIME_DIR=/run/user/"$(id -u)" \
    WAYLAND_DISPLAY="$WD" \
    VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
    TILLER_DB="$DB" TILLER_SOCKET="$SOCK" \
    "$BIN" >"$APP_LOG" 2>&1 &
APP_PID=$!

for _ in $(seq 1 120); do
  kill -0 "$APP_PID" 2>/dev/null || { echo "FAIL: app exited during startup — see $APP_LOG" >&2; tail -20 "$APP_LOG" >&2; exit 4; }
  [ -S "$SOCK" ] && break
  sleep 0.25
done
[ -S "$SOCK" ] || { echo "FAIL: no control socket at $SOCK in 30s" >&2; tail -20 "$APP_LOG" >&2; exit 4; }

export SOCK WD SWAYSOCK APP_LOG OUTDIR W1 H1 W2 H2 MIN_COLORS
export XDG_RUNTIME_DIR=/run/user/"$(id -u)"
# grim reads WAYLAND_DISPLAY, and on a machine where the operator is logged into a Wayland session
# that variable is already set to THEIR compositor. Left alone, grim looks for HEADLESS-1 on the
# user's desktop, finds nothing and writes no file — and on a compositor that did have a matching
# output it would photograph the operator's actual screen. Point it at ours explicitly.
export WAYLAND_DISPLAY="$WD"

ctl() {
  local method="$1"; shift
  CTL_METHOD="$method" CTL_ARGS="$*" python3 - <<'PY'
import json, os, socket, sys
# Every param is a string. ControlRequest deserialises params as a map of String to String, so
# sending `index=0` as a JSON integer is rejected outright:
#   invalid type: integer `0`, expected a string
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

# Repaint is lazy. After the first frame the app sits still and grim keeps returning that frame
# BYTE FOR BYTE — a socket call that demonstrably succeeded can produce an identical PNG. Setting
# the output to the size it already has is a no-op, so this alternates between two sizes to force
# a configure. Never read an unchanged capture as "the feature did nothing": verify the frame is
# fresh first, or the lane manufactures false negatives.
SHOT_N=0
CUR=1
shot() {
  local name="${1:-shot}" res
  SHOT_N=$((SHOT_N + 1))
  if [ "$CUR" = 1 ]; then res="${W2}x${H2}"; CUR=2; else res="${W1}x${H1}"; CUR=1; fi
  swaymsg -s "$SWAYSOCK" output HEADLESS-1 resolution "$res" >/dev/null 2>&1
  sleep 1
  local path
  path="$(printf '%s/%02d-%s.png' "$OUTDIR" "$SHOT_N" "$name")"
  grim -o HEADLESS-1 "$path" 2>/dev/null
  local colors
  colors="$(identify -format '%k' "$path" 2>/dev/null || echo 0)"
  if [ "${colors:-0}" -lt "$MIN_COLORS" ]; then
    echo "WARN $path is blank or near-blank ($colors colours at $res)" >&2
    return 1
  fi
  echo "SHOT $path ($res · $colors colours)"
}
export -f ctl shot

sleep "$SETTLE"
shot baseline || { echo "FAIL: first frame is blank — presentation is broken, not layout. See $APP_LOG" >&2; exit 5; }

eval "$ACTIONS"
rc=$?

kill -0 "$APP_PID" 2>/dev/null || { echo "FAIL: app died during the actions — see $APP_LOG" >&2; tail -20 "$APP_LOG" >&2; exit 4; }
echo "OK   $SHOT_N frames in $OUTDIR (label=$LABEL display=$WD socket=$SOCK)"
exit $rc
