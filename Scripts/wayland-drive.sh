#!/usr/bin/env bash
# Boot the app under a nested headless Wayland compositor, DRIVE it over the control socket,
# and photograph the result — with no X display and no drive lock.
#
# This is the sibling of linux-drive.sh. The difference that matters:
#
#     linux-drive.sh   one shared X pointer  -> a global mutex, real clicks, one driver at a time
#     wayland-drive.sh virtual pointer       -> no lock, socket calls and real input, any number in parallel
#
# Pointer and keyboard devices are created BEFORE Tiller connects, then kept alive for the whole
# drive. A transient wlrctl/wtype client can send its first event before GPUI has bound the matching
# wl_pointer/wl_keyboard. Read WAYLAND-LANE.md before recording anything from here — especially the
# paragraph about which half of a row you proved.
#
# Usage:
#   Scripts/wayland-drive.sh <outdir> '<actions>' [settle]
#
# Inside <actions> you get:
#   ctl <method> [k=v ...]   send a ControlRequest, print the JSON reply
#   click <x> <y>            left-click absolute nested-output coordinates
#   move <x> <y>             move to absolute nested-output coordinates
#   rightclick <x> <y>       right-click absolute nested-output coordinates (opens context menus)
#   down <x> <y>             press and hold the left button at this position (no release)
#   up <x> <y>               move here and release the left button — pair with `down` for a drag
#   drag <x1> <y1> <x2> <y2> [steps]   press at (x1,y1), move through <steps> waypoints (default
#                            4), release at (x2,y2) — composes down/move/up for you
#   scroll <x> <y> <steps>  move to (x,y), then send <steps> wheel notches (negative = opposite
#                            direction) as a real axis event, not a synthesized keypress
#   xdnd <x1> <y1> <x2> <y2> <file...> [--delay-ms N]   a REAL compositor-delivered XDND file
#                            drag — a second Wayland client (Scripts/xdnd-source) offers
#                            text/uri-list as a genuine wl_data_device_manager drag source, this
#                            function presses the persistent virtual pointer at (x1,y1) to hand it
#                            a real button serial, then walks to (x2,y2) and releases to drop.
#                            Distinct from `drag`, which stays entirely inside this app's own
#                            GPUI on_drag/on_drop and never touches wl_data_device at all.
#   type <text>              type text through wtype (click a text target first)
#   key <name>               type a named key through wtype, e.g. key Tab
#   chord <mod> <key>        modifier-held named key, e.g. chord shift F10 (mod: shift, ctrl,
#                            alt, logo, altgr, capslock — see `man wtype`; `+`-join for more than
#                            one, e.g. chord ctrl+shift p for the command palette)
#   modclick <mod> <x> <y>   hold <mod> while left-clicking (x,y) — e.g. modclick ctrl 400 300
#                            for the platform-modifier+click convention on terminal links
#   title <text>             set the focused terminal pane's title via OSC 0 (click the pane
#                            first). Use this rather than writing the escape sequence inline —
#                            its bare `;` splits the eval'd action block in half.
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
#      TILLER_WL_PROTOCOL_LOG=1 adds WAYLAND_DEBUG=1 to the app's own environment, so APP_LOG
#      carries the app's ONE Wayland connection's wire trace — every wl_keyboard/wl_pointer event
#      in true arrival order. Off by default: it is verbose and only worth paying for when a
#      question is specifically about event ORDER (e.g. was a modifier applied before a click's
#      button event reached the app), which no screenshot can answer.
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
# Standalone crate (deliberately outside rust/'s workspace — see Scripts/xdnd-source/Cargo.toml)
# providing the `xdnd` action's real wl_data_device_manager drag SOURCE. Built on first use, not
# eagerly, so instances that never call `xdnd` pay nothing for it.
XDND_SOURCE_DIR="$ROOT/Scripts/xdnd-source"
XDND_SOURCE_BIN="$XDND_SOURCE_DIR/target/debug/xdnd-source"

# Sockets live under /tmp because sockaddr_un caps a path at 108 bytes and the agents' scratchpad
# paths overflow it — the failure is an opaque bind error a long way from the cause.
SWAYSOCK="/tmp/$LABEL-sway.sock"
SWAYCONF="/tmp/$LABEL-sway.conf"
SWAYLOG="/tmp/$LABEL-sway.log"
SOCK="/tmp/$LABEL.sock"
APP_LOG="/tmp/$LABEL.log"
DB="/tmp/$LABEL.sqlite"
INPUT_DIR="/tmp/$LABEL-input"
VP_FIFO="$INPUT_DIR/commands"
VP_ACK="$INPUT_DIR/ack"
VP_READY="$INPUT_DIR/ready"
VP_LOG="$INPUT_DIR/virtual-pointer.log"
VK_LOG="$INPUT_DIR/virtual-keyboard.log"
VP_PID=""
VK_PID=""
POINTER_ENABLED=0
POINTER_COMMAND_ID=0

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
  [ -n "${TILLER_WL_KEEP:-}" ] && {
    echo "NOTE: leaving $LABEL running (SOCK=$SOCK WAYLAND_DISPLAY=$WD VP_FIFO=$VP_FIFO)"
    return 0
  }
  [ -n "${VP_PID:-}" ] && kill "$VP_PID" 2>/dev/null || true
  [ -n "${VK_PID:-}" ] && kill "$VK_PID" 2>/dev/null || true
  kill_ours TILLER_SOCKET "$SOCK" tiller
  kill_ours SWAYSOCK "$SWAYSOCK" sway
  return 0
}
trap cleanup EXIT

kill_ours TILLER_SOCKET "$SOCK" tiller
kill_ours SWAYSOCK "$SWAYSOCK" sway

W1=1715 H1=972          # the two sizes shot() alternates between; see the repaint note below
W2=1400 H2=900
OUTPUT_W="$W1" OUTPUT_H="$H1"
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

# Every injector in this script is guarded against the operator's real compositor. The compositor
# config path is unique to this run, so it is a stronger check than trusting a wayland-N name.
export XDG_RUNTIME_DIR=/run/user/"$(id -u)"
export WAYLAND_DISPLAY="$WD"
verify_nested_sway() {
  [ "${WAYLAND_DISPLAY:-}" = "$WD" ] || {
    echo "FAIL: WAYLAND_DISPLAY is not this nested instance" >&2
    return 1
  }
  swaymsg -s "$SWAYSOCK" -t get_version | EXPECTED_SWAYCONF="$SWAYCONF" python3 -c '
import json, os, sys
version = json.load(sys.stdin)
if version.get("loaded_config_file_name") != os.environ["EXPECTED_SWAYCONF"]:
    raise SystemExit("FAIL: swaymsg is not this nested compositor")
print("SAFE: sway {} on {}".format(version["human_readable"], version["loaded_config_file_name"]))
'
}

# A pointer must exist before GPUI connects: wlrctl's one-shot client creates the device, sends its
# event, then destroys it before the app can bind wl_pointer. Build a tiny persistent client only
# when the action block asks for pointer input, preserving the visual-only lane's startup cost.
start_virtual_pointer() {
  command -v gcc >/dev/null || { echo "FAIL: gcc is required for Wayland pointer input" >&2; return 1; }
  command -v wayland-scanner >/dev/null || { echo "FAIL: wayland-scanner is required for Wayland pointer input" >&2; return 1; }
  pkg-config --exists wayland-client || { echo "FAIL: wayland-client development files are required" >&2; return 1; }
  verify_nested_sway || return 1
  mkdir -p "$INPUT_DIR"
  rm -f "$VP_FIFO" "$VP_ACK" "$VP_READY"
  mkfifo "$VP_FIFO"
  local header="$INPUT_DIR/wlr-virtual-pointer-client-protocol.h"
  local protocol_c="$INPUT_DIR/wlr-virtual-pointer-client-protocol.c"
  local binary="$INPUT_DIR/virtual-pointer"
  wayland-scanner client-header "$ROOT/Scripts/wlr-virtual-pointer-unstable-v1.xml" "$header" || return 1
  wayland-scanner private-code "$ROOT/Scripts/wlr-virtual-pointer-unstable-v1.xml" "$protocol_c" || return 1
  gcc -std=c11 -Wall -Wextra -Werror -O2 -I "$INPUT_DIR" \
    "$ROOT/Scripts/wayland-virtual-pointer.c" "$protocol_c" -o "$binary" \
    $(pkg-config --cflags --libs wayland-client) || return 1
  "$binary" "$VP_READY" "$VP_FIFO" "$VP_ACK" >"$VP_LOG" 2>&1 &
  VP_PID=$!
  for _ in $(seq 1 40); do
    [ -f "$VP_READY" ] && break
    kill -0 "$VP_PID" 2>/dev/null || { cat "$VP_LOG" >&2; return 1; }
    sleep 0.1
  done
  [ -f "$VP_READY" ] || { echo "FAIL: virtual pointer did not become ready — see $VP_LOG" >&2; return 1; }
  POINTER_ENABLED=1
}

start_virtual_keyboard() {
  command -v wtype >/dev/null || { echo "FAIL: wtype is required for Wayland keyboard input" >&2; return 1; }
  verify_nested_sway || return 1
  # The press/release happens before Tiller starts; -s then keeps wtype (and its virtual keyboard)
  # connected without leaving Shift held. Later type/key commands use the already-advertised seat.
  wtype -M shift -m shift -s 600000 -k Shift_L >"$VK_LOG" 2>&1 &
  VK_PID=$!
  sleep 0.1
  kill -0 "$VK_PID" 2>/dev/null || { cat "$VK_LOG" >&2; return 1; }
}

# modclick needs BOTH devices pre-created: the pointer for the click itself, the keyboard because
# it holds a modifier around it. It appears in both guards below.
if grep -Eq '(^|[;[:space:]])(click|move|rightclick|down|up|drag|scroll|modclick|xdnd)([;[:space:]]|$)' <<<"$ACTIONS"; then
  start_virtual_pointer || exit 3
fi
if grep -Eq '(^|[;[:space:]])(type|key|title|chord|modclick)([;[:space:]]|$)' <<<"$ACTIONS"; then
  start_virtual_keyboard || exit 3
fi

# DISPLAY must be UNSET, not empty: with it set at all, GPUI takes the X11 path, which under
# Xvfb/Xephyr has no DRI3 route and presents nothing. A blank frame here is almost always this.
#
# TILLER_WL_PROTOCOL_LOG=1 turns on libwayland's own wire tracer (WAYLAND_DEBUG=1) for the app
# process only (never the injectors — their traces would dwarf the app's and bury the question
# this exists to answer). It interleaves every event the app's ONE Wayland connection receives —
# wl_keyboard.modifiers, wl_keyboard.key, wl_pointer.button, all of it — in true wire order, in
# APP_LOG. That is the only ground truth for "was the modifier already applied when the click's
# button event arrived", which no screenshot or ctl call can answer: a screenshot shows the
# *result* of event processing, never the order events were delivered in. Built for P130's modclick
# investigation; leave it available for the next primitive that needs the same question answered.
env -u DISPLAY \
    XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
    WAYLAND_DISPLAY="$WD" \
    VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
    TILLER_DB="$DB" TILLER_SOCKET="$SOCK" \
    ${TILLER_WL_PROTOCOL_LOG:+WAYLAND_DEBUG=1} \
    "$BIN" >"$APP_LOG" 2>&1 &
APP_PID=$!

for _ in $(seq 1 120); do
  kill -0 "$APP_PID" 2>/dev/null || { echo "FAIL: app exited during startup — see $APP_LOG" >&2; tail -20 "$APP_LOG" >&2; exit 4; }
  [ -S "$SOCK" ] && break
  sleep 0.25
done
[ -S "$SOCK" ] || { echo "FAIL: no control socket at $SOCK in 30s" >&2; tail -20 "$APP_LOG" >&2; exit 4; }

export SOCK WD SWAYSOCK APP_LOG OUTDIR W1 H1 W2 H2 MIN_COLORS VP_FIFO VP_ACK POINTER_ENABLED
# grim reads WAYLAND_DISPLAY, and on a machine where the operator is logged into a Wayland session
# that variable is already set to THEIR compositor. Left alone, grim looks for HEADLESS-1 on the
# user's desktop, finds nothing and writes no file — and on a compositor that did have a matching
# output it would photograph the operator's actual screen. Point it at ours explicitly.
# WAYLAND_DISPLAY was exported above, before the virtual pointer and the app were launched.

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

pointer_command() {
  # $4 (extra) is optional and signed — only `scroll` uses it, as wheel-notch count.
  local operation="$1" x="$2" y="$3" extra="${4:-}" id
  [ "$POINTER_ENABLED" = 1 ] || { echo "FAIL: $operation needs the pre-app virtual pointer" >&2; return 1; }
  [[ "$x" =~ ^[0-9]+$ && "$y" =~ ^[0-9]+$ ]] || { echo "FAIL: coordinates must be non-negative integers" >&2; return 1; }
  if [ -n "$extra" ]; then
    [[ "$extra" =~ ^-?[0-9]+$ ]] || { echo "FAIL: $operation's extra value must be an integer" >&2; return 1; }
  fi
  verify_nested_sway || return 1
  POINTER_COMMAND_ID=$((POINTER_COMMAND_ID + 1))
  id="$POINTER_COMMAND_ID"
  printf '%s %s %s %s %s %s %s\n' "$operation" "$id" "$x" "$y" "$OUTPUT_W" "$OUTPUT_H" "${extra:-0}" >"$VP_FIFO"
  for _ in $(seq 1 50); do
    grep -qx "$id" "$VP_ACK" 2>/dev/null && return 0
    sleep 0.02
  done
  echo "FAIL: virtual pointer did not acknowledge $operation — see $VP_LOG" >&2
  return 1
}

move() { pointer_command move "$1" "$2"; }
click() { pointer_command click "$1" "$2"; }

# Right-click: same persistent device as click/move, BTN_RIGHT (0x111) instead of BTN_LEFT.
# See wayland-virtual-pointer.c's `rightclick` branch.
rightclick() { pointer_command rightclick "$1" "$2"; }

# Button-held drag, composed from three virtual-pointer primitives that did not exist before:
# down (press, no release), plain move (motion only, already existed), up (release, no press-first).
down() { pointer_command down "$1" "$2"; }
up() { pointer_command up "$1" "$2"; }

# drag <x1> <y1> <x2> <y2> [steps] — press at the start, walk <steps> waypoints (default 4) to the
# end so the target sees real intermediate motion (not a teleport many drop targets ignore), then
# release. This is the "compose one by hand from move" the pointer primitives could not do before.
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

# scroll <x> <y> <steps> — move to (x,y) then send <steps> wheel notches as a real
# zwlr_virtual_pointer axis+axis_discrete event (15 libinput units/notch, matching a physical
# wheel), not a synthesized PageUp/Down keypress. Negative <steps> scrolls the other way; sign
# convention is unverified until driven against a live control — record what you observed.
scroll() { pointer_command scroll "$1" "$2" "$3"; }

# xdnd <x1> <y1> <x2> <y2> <file1> [file2 ...] [--delay-ms N] — a REAL compositor-delivered XDND
# drag, not GPUI's own in-process simulated drag (that path was already proven by
# a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files in
# tiller_terminal/src/lib.rs; F-CORE-FILE-03A was open on exactly the gap this closes). Launches
# Scripts/xdnd-source as a second Wayland client on this SAME compositor connection — a real
# wl_data_device_manager drag SOURCE offering text/uri-list for every <file> (turned into a
# file:// URI each). It maps a tiny zwlr_layer_shell_v1 overlay surface at (x1,y1) — a layer-shell
# surface, deliberately not an xdg_toplevel, so sway's tiling never touches Tiller's own window or
# the coordinate space the rest of this script's actions use.
#
# The handshake: xdnd-source prints READY once that overlay is mapped and eligible for pointer
# focus; THEN this function presses the persistent virtual pointer's button at (x1,y1) — the same
# proven-live pointer client `click`/`drag` already use — which is what gives xdnd-source the real
# wl_pointer.button serial it needs to call wl_data_device.start_drag (a serial from an
# out-of-process synthetic click cannot be forged; it has to come from an actual button-down the
# compositor delivered). xdnd-source prints DRAG_STARTED once that request went out. Only then does
# this function walk the pointer to (x2,y2) with real intermediate motion and release — the
# compositor delivers wl_data_device.enter/motion/drop to whatever surface is under the pointer at
# that point, i.e. Tiller's own window, exactly as dragging out of a real file manager would.
#
# `--delay-ms N` simulates a slow-resolving provider by delaying xdnd-source's write into the
# offer pipe once the target's `receive()` request triggers its `send` event — see
# docs/linux-rewrite/WAYLAND-LANE.md's XDND section for why that is the closest analogue
# `text/uri-list` has to the macOS NSItemProvider clause, and why it is not proven equivalent to it.
xdnd() {
  [ "$#" -ge 5 ] || { echo "usage: xdnd <x1> <y1> <x2> <y2> <file1> [file2 ...] [--delay-ms N]" >&2; return 2; }
  [ "$POINTER_ENABLED" = 1 ] || { echo "FAIL: xdnd needs the pre-app virtual pointer" >&2; return 1; }
  verify_nested_sway || return 1
  local x1="$1" y1="$2" x2="$3" y2="$4"
  shift 4
  if [ ! -x "$XDND_SOURCE_BIN" ]; then
    echo "Building Scripts/xdnd-source (first use in this checkout)..." >&2
    ( cd "$XDND_SOURCE_DIR" && cargo build -q ) || {
      echo "FAIL: Scripts/xdnd-source did not build — see above" >&2
      return 1
    }
  fi
  local -a files=() delay_args=()
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --delay-ms) delay_args=(--delay-ms "$2"); shift 2 ;;
      *) files+=("$1"); shift ;;
    esac
  done
  [ "${#files[@]}" -ge 1 ] || { echo "usage: xdnd needs at least one file path" >&2; return 2; }
  local -a uri_args=() f
  for f in "${files[@]}"; do uri_args+=(--file "$f"); done

  local xlog="$INPUT_DIR/xdnd-source.log"
  : > "$xlog"
  env -u DISPLAY XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" WAYLAND_DISPLAY="$WD" \
    "$XDND_SOURCE_BIN" --anchor-x "$x1" --anchor-y "$y1" "${uri_args[@]}" "${delay_args[@]}" \
    >"$xlog" 2>&1 &
  local xpid=$! i

  for i in $(seq 1 100); do
    grep -qx READY "$xlog" 2>/dev/null && break
    kill -0 "$xpid" 2>/dev/null || {
      echo "FAIL: xdnd-source exited before READY — see $xlog" >&2
      cat "$xlog" >&2
      return 1
    }
    sleep 0.05
  done
  grep -qx READY "$xlog" 2>/dev/null || {
    echo "FAIL: xdnd-source never printed READY within 5s" >&2
    kill "$xpid" 2>/dev/null
    cat "$xlog" >&2
    return 1
  }

  down "$x1" "$y1" || { kill "$xpid" 2>/dev/null; return 1; }

  for i in $(seq 1 60); do
    grep -qx DRAG_STARTED "$xlog" 2>/dev/null && break
    sleep 0.05
  done
  if ! grep -qx DRAG_STARTED "$xlog" 2>/dev/null; then
    echo "FAIL: the button press over xdnd-source's surface never reached start_drag — see $xlog" >&2
    up "$x1" "$y1" >/dev/null 2>&1
    kill "$xpid" 2>/dev/null
    cat "$xlog" >&2
    return 1
  fi

  local steps=6 sx sy
  for ((i = 1; i <= steps; i++)); do
    sx=$(( x1 + (x2 - x1) * i / steps ))
    sy=$(( y1 + (y2 - y1) * i / steps ))
    move "$sx" "$sy" || true
    sleep 0.05
  done
  up "$x2" "$y2"

  wait "$xpid"
  local rc=$?
  cat "$xlog"
  return $rc
}

type() {
  verify_nested_sway || return 1
  command -v wtype >/dev/null || { echo "FAIL: wtype is not installed" >&2; return 1; }
  wtype "$@"
}

key() {
  [ "$#" = 1 ] || { echo "usage: key <name>" >&2; return 2; }
  verify_nested_sway || return 1
  command -v wtype >/dev/null || { echo "FAIL: wtype is not installed" >&2; return 1; }
  wtype -k "$1"
}

# chord <mod> <key> — modifier-held named key, e.g. `chord shift F10`. One wtype invocation
# (-M press, -k the key while held, -m release) is enough: the keyboard's wl_keyboard binding
# already happened during start_virtual_keyboard's pre-app dance, so this one-shot process is not
# racing a first bind (see trap 3) — the same reason plain `key`/`type` are safe as one-shot calls.
# <mod> is whatever `man wtype` accepts: shift, ctrl, alt, logo, altgr, capslock — or a `+`-joined
# combination, e.g. `chord ctrl+shift p` for the command palette's Ctrl+Shift+P. Each `-M` press is
# issued in the order given, and `-m` release in reverse order, so a genuinely-held multi-modifier
# chord reaches the app the same way a real keyboard would deliver it (all modifiers already down
# before the key event), not two separate single-modifier chords that could never register as one.
chord() {
  [ "$#" = 2 ] || { echo "usage: chord <mod[+mod...]> <key>" >&2; return 2; }
  verify_nested_sway || return 1
  command -v wtype >/dev/null || { echo "FAIL: wtype is not installed" >&2; return 1; }
  local mods="$1" key="$2"
  local -a mod_arr press_args release_args
  IFS='+' read -r -a mod_arr <<<"$mods"
  press_args=()
  for m in "${mod_arr[@]}"; do press_args+=(-M "$m"); done
  release_args=()
  for ((i = ${#mod_arr[@]} - 1; i >= 0; i--)); do release_args+=(-m "${mod_arr[$i]}"); done
  wtype "${press_args[@]}" -k "$key" "${release_args[@]}"
}

# modclick <mod> <x> <y> — hold <mod> across a left-click, e.g. `modclick ctrl 400 300` for the
# platform-modifier+click convention on a terminal hyperlink. wtype only holds a modifier for as
# long as its own process lives ("modifiers get released automatically once the program
# terminates" — man wtype), so the click must happen DURING that process's lifetime: -M presses
# and holds, -s sleeps 400ms holding it, -m releases and the process exits. We background it, give
# the press 50ms to land before clicking (same settle reasoning as trap 3), then wait for the
# release so a second modclick never overlaps this one's hold.
modclick() {
  [ "$#" = 3 ] || { echo "usage: modclick <mod> <x> <y>" >&2; return 2; }
  local mod="$1" x="$2" y="$3" wpid
  verify_nested_sway || return 1
  command -v wtype >/dev/null || { echo "FAIL: wtype is not installed" >&2; return 1; }
  wtype -M "$mod" -s 400 -m "$mod" &
  wpid=$!
  sleep 0.05
  if ! click "$x" "$y"; then
    wait "$wpid" 2>/dev/null
    return 1
  fi
  wait "$wpid"
}

# Set the FOCUSED TERMINAL PANE's title, by typing a printf that emits OSC 0 into its shell.
#
# This exists because writing the escape sequence inline does not work and fails in a way that
# looks like the app ignoring it. The whole action block reaches `eval` (see the bottom of this
# file), and an OSC sequence carries a bare `;` — `type printf '\033]0;X\007'` therefore parses
# as two commands, types half a sequence, and leaves the title untouched. Four F-CORE-ACT rows
# (02, 06, 07, 11) sat unexercised on exactly that mistake.
#
# Click a terminal pane first so the shell has focus; this types a real command a user could
# have typed, which is the input path those rows are supposed to be verified through.
title() {
  [ "$#" -ge 1 ] || { echo "usage: title <text>" >&2; return 2; }
  verify_nested_sway || return 1
  command -v wtype >/dev/null || { echo "FAIL: wtype is not installed" >&2; return 1; }
  local text="$*"
  case "$text" in
    *"'"*) echo "FAIL: title text may not contain a single quote" >&2; return 2 ;;
  esac
  wtype "printf '\\033]0;${text}\\007'" || return 1
  wtype -k Return
}

# Repaint is lazy. After the first frame the app sits still and grim keeps returning that frame
# BYTE FOR BYTE — a socket call that demonstrably succeeded can produce an identical PNG. Setting
# the output to the size it already has is a no-op, so a configure has to be forced. Never read an
# unchanged capture as "the feature did nothing": verify the frame is fresh first, or the lane
# manufactures false negatives.
#
# This used to force the configure by ALTERNATING between two sizes, leaving each capture at
# whichever size came next. That made the lane a measuring instrument whose scale changed between
# observation and action: coordinates read off one frame were sent while the output was at the
# other size, the pointer scaled them to the wrong space (see pointer_command's OUTPUT_W/OUTPUT_H),
# the click landed in dead space, and the row was written up as a missing feature. It cost at least
# two false negatives in wave F (F-EDIT-10, F-EDIT-11) before a critic noticed, and it silently
# spanned invocations, because CUR resets to 1 on every run while the compositor keeps the size the
# previous run left behind.
#
# Now the configure is forced by nudging to the off-size and immediately returning, so EVERY
# captured frame — and every click, in this run or the next — is in one fixed coordinate space.
SHOT_N=0
shot() {
  local name="${1:-shot}"
  SHOT_N=$((SHOT_N + 1))
  # Nudge away and back: two real configure events, ending at the canonical size.
  swaymsg -s "$SWAYSOCK" output HEADLESS-1 resolution "${W2}x${H2}" >/dev/null 2>&1
  sleep 0.4
  swaymsg -s "$SWAYSOCK" output HEADLESS-1 resolution "${W1}x${H1}" >/dev/null 2>&1
  OUTPUT_W="$W1"; OUTPUT_H="$H1"
  local res="${W1}x${H1}"
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
export -f ctl pointer_command move click rightclick down up drag scroll xdnd type key chord modclick shot verify_nested_sway

sleep "$SETTLE"
shot baseline || { echo "FAIL: first frame is blank — presentation is broken, not layout. See $APP_LOG" >&2; exit 5; }

eval "$ACTIONS"
rc=$?

kill -0 "$APP_PID" 2>/dev/null || { echo "FAIL: app died during the actions — see $APP_LOG" >&2; tail -20 "$APP_LOG" >&2; exit 4; }
echo "OK   $SHOT_N frames in $OUTDIR (label=$LABEL display=$WD socket=$SOCK)"
exit $rc
