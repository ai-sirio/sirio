#!/usr/bin/env bash
# Does the virtual pointer die when its compositor does?
#
# It used to not. The command fifo is opened O_RDWR on purpose — so that a writer
# closing does not deliver EOF and end the session — which makes the process its
# own writer, so the fgets() loop blocks forever. Nothing else in that loop ever
# looked at the Wayland connection, so a dead compositor was simply not an event
# this program could observe.
#
# Measured 2026-08-19 on this machine: 184 virtual-pointers alive at once with a
# single sway left between them, the oldest 25 hours old — roughly one per lane
# ever driven. wayland-drive.sh's startup reap only recovers a label that gets
# used again, and most lane labels are used once, so the leak had to be closed
# in the C.
#
# This test drives it for real: a nested headless sway, a live pointer proven to
# be answering commands, then the compositor killed out from under it.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SRC="$ROOT/Scripts/wayland-virtual-pointer.c"
XML="$ROOT/Scripts/wlr-virtual-pointer-unstable-v1.xml"
[[ -r "$SRC" && -r "$XML" ]] || { echo "FAIL: missing $SRC or $XML" >&2; exit 1; }

for tool in gcc wayland-scanner sway pkg-config; do
    command -v "$tool" >/dev/null || { echo "SKIP: $tool is not installed" >&2; exit 0; }
done
pkg-config --exists wayland-client || { echo "SKIP: wayland-client headers absent" >&2; exit 0; }

LABEL="vptest-$$"
TMP="/tmp/$LABEL"
SWAYSOCK="/tmp/$LABEL-sway.sock"
SWAYCONF="$TMP/sway.conf"
SWAYLOG="$TMP/sway.log"
VP_LOG="$TMP/vp.log"
FIFO="$TMP/commands"
ACK="$TMP/ack"
READY="$TMP/ready"
SWAY_PID=""
VP_PID=""

cleanup() {
    [[ -n "$VP_PID" ]] && { kill "$VP_PID" 2>/dev/null || true; }
    [[ -n "$SWAY_PID" ]] && { kill "$SWAY_PID" 2>/dev/null || true; }
    rm -rf "$TMP" "$SWAYSOCK" || true
    return 0
}
trap cleanup EXIT

mkdir -p "$TMP"
cat > "$SWAYCONF" <<EOF
xwayland disable
default_border none
gaps inner 0
gaps outer 0
output HEADLESS-1 resolution 800x600
EOF

# --- build, with the harness's own flags so -Werror is part of the gate -------
gcc_out="$TMP/virtual-pointer"
wayland-scanner client-header "$XML" "$TMP/wlr-virtual-pointer-client-protocol.h"
wayland-scanner private-code  "$XML" "$TMP/wlr-virtual-pointer-client-protocol.c"
# shellcheck disable=SC2046
gcc -std=c11 -Wall -Wextra -Werror -O2 -I "$TMP" \
    "$SRC" "$TMP/wlr-virtual-pointer-client-protocol.c" -o "$gcc_out" \
    $(pkg-config --cflags --libs wayland-client)

# --- a nested compositor of our own ------------------------------------------
# `env -u WAYLAND_DISPLAY -u DISPLAY` is what keeps this off the operator's real
# desktop: without it a headless backend request can still land on the session
# the test was launched from.
env -u WAYLAND_DISPLAY -u DISPLAY \
    XDG_RUNTIME_DIR="/run/user/$(id -u)" \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
    SWAYSOCK="$SWAYSOCK" \
    sway -d -c "$SWAYCONF" >"$SWAYLOG" 2>&1 &
SWAY_PID=$!

WD=""
for _ in $(seq 1 100); do
    # `|| true` because this grep is *expected* to find nothing on the early
    # passes — sway has not written the line yet. Under `set -e` with pipefail an
    # empty match aborts the whole test in silence, which reads exactly like the
    # failure this file is meant to detect.
    WD="$( { grep -o 'Running compositor on wayland display .*' "$SWAYLOG" 2>/dev/null || true; } | tail -1 | awk '{print $NF}' | tr -d "'\"")"
    [[ -n "$WD" ]] && break
    kill -0 "$SWAY_PID" 2>/dev/null || { echo "FAIL: sway died during startup" >&2; tail -20 "$SWAYLOG" >&2; exit 1; }
    sleep 0.1
done
[[ -n "$WD" ]] || { echo "FAIL: sway never announced a display" >&2; tail -20 "$SWAYLOG" >&2; exit 1; }

# --- the pointer, connected to it --------------------------------------------
mkfifo "$FIFO"
env -u DISPLAY \
    XDG_RUNTIME_DIR="/run/user/$(id -u)" WAYLAND_DISPLAY="$WD" \
    "$gcc_out" "$READY" "$FIFO" "$ACK" >"$VP_LOG" 2>&1 &
VP_PID=$!

for _ in $(seq 1 100); do
    [[ -f "$READY" ]] && break
    kill -0 "$VP_PID" 2>/dev/null || { echo "FAIL: pointer exited before becoming ready" >&2; cat "$VP_LOG" >&2; exit 1; }
    sleep 0.1
done
[[ -f "$READY" ]] || { echo "FAIL: pointer never became ready" >&2; cat "$VP_LOG" >&2; exit 1; }

# Positive control. "The process exited after we killed sway" is worth nothing if
# the process was already wedged, or had never bound a pointer at all: a program
# that crashes on startup also passes that assertion. So make it do its job first
# and answer for it.
echo "move 1 400 300 800 600" > "$FIFO"
acked=0
for _ in $(seq 1 100); do
    # Braced so a not-yet-written ack is a loop iteration, not an errexit abort:
    # the `&&` chain's last command failing is exactly what `set -e` acts on.
    if [[ -f "$ACK" ]] && grep -qx '1' "$ACK"; then
        acked=1
        break
    fi
    sleep 0.1
done
[[ "$acked" == 1 ]] || { echo "FAIL: control — a live pointer did not acknowledge a move" >&2; cat "$VP_LOG" >&2; exit 1; }
kill -0 "$VP_PID" 2>/dev/null || { echo "FAIL: control — pointer died on its own before the compositor did" >&2; exit 1; }

# --- pull the compositor out from under it -----------------------------------
kill "$SWAY_PID" 2>/dev/null || true
SWAY_PID=""

exited=0
status=""
for _ in $(seq 1 100); do
    if ! kill -0 "$VP_PID" 2>/dev/null; then
        # `wait` reports the status only for a child of this shell, which it is.
        wait "$VP_PID" 2>/dev/null && status=0 || status=$?
        exited=1
        break
    fi
    sleep 0.1
done

if [[ "$exited" != 1 ]]; then
    echo "FAIL: pointer outlived its compositor — still alive 10s after sway was killed." >&2
    echo "      This is the leak: it will now sit on the fifo forever." >&2
    cat "$VP_LOG" >&2
    exit 1
fi

VP_PID=""
if [[ "$status" != 9 ]]; then
    echo "FAIL: pointer exited $status, expected 9 (compositor gone)." >&2
    echo "      Exiting 0 here is what made the orphans look like healthy idle processes." >&2
    cat "$VP_LOG" >&2
    exit 1
fi
grep -q 'compositor gone' "$VP_LOG" || {
    echo "FAIL: nothing in the log says why it exited — see $VP_LOG" >&2
    cat "$VP_LOG" >&2
    exit 1
}

echo "virtual pointer OK — acknowledged a live move, then exited 9 when its compositor died"
