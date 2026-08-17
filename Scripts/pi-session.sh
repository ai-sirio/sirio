#!/usr/bin/env bash
# Bring up (or check, or tear down) the PARENT compositor that every drive lane nests inside on a
# monitor-less box. Nothing renders here without it.
#
#   Scripts/pi-session.sh start     idempotent: starts what is missing, leaves what is up
#   Scripts/pi-session.sh status    prints what is running, exits 1 if the parent is down
#   Scripts/pi-session.sh stop      tears the parent down (does NOT touch nested lanes)
#   Scripts/pi-session.sh env       prints the exports a drive needs, for eval
#
# ## Why this file exists
#
# On 2026-08-17 an agent finished a piece and cleaned up with `pkill -x sway`. That is broader
# than the lane's own `kill_ours`, and it killed the parent — the one process on this box that
# makes anything visible at all. What made it expensive was the silent part: a *nested* sway
# leaked from a different agent then took the freed `wayland-1` name, so `WAYLAND_DISPLAY=wayland-1`
# still resolved, drives still started, and every frame came back black with
# `MESA: error: ZINK: failed to choose pdev` buried in the app log. The lane looked alive and was
# not, which is the failure mode that costs the most.
#
# Two things follow, and both are implemented here:
#
#   1. The parent must be restorable by anyone, in one command, without archaeology.
#   2. The parent's socket must have a name a nested compositor cannot take. Nested sways get
#      wayland-N from a counter and sway offers no way to name its own socket, so the parent
#      PUBLISHES the name it got (an exec line in the config writes /tmp/tiller-parent.display)
#      and is identified by its config path, never by "whatever holds wayland-1".
#
# `pkill -x sway` is still wrong — it kills every agent's nested compositor too. Use the lane's own
# cleanup, or `Scripts/pi-session.sh stop` for the parent alone.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONF="$ROOT/Scripts/pi-sway-headless.conf"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

PARENT_DISPLAY_FILE=/tmp/tiller-parent.display
PARENT_SOCK="/tmp/tiller-parent-sway.sock"
PARENT_LOG="/tmp/tiller-parent-sway.log"
VNC_LOG="/tmp/tiller-parent-wayvnc.log"
VNC_ADDR="${TILLER_VNC_ADDR:-127.0.0.1}"
VNC_PORT="${TILLER_VNC_PORT:-5900}"

parent_pid() {
  # The parent is identified by its config path in the command line, never by the name `sway` —
  # nested lanes are also called sway and must never be caught by this.
  pgrep -f "sway .*${CONF//\//\\/}" 2>/dev/null | head -1
}

parent_display() { cat "$PARENT_DISPLAY_FILE" 2>/dev/null; }

parent_alive() {
  local pid disp; pid="$(parent_pid)"; disp="$(parent_display)"
  [ -n "$pid" ] && [ -n "$disp" ] && [ -S "$XDG_RUNTIME_DIR/$disp" ]
}

vnc_pid() { pgrep -f "wayvnc -o HEADLESS-1 $VNC_ADDR $VNC_PORT" 2>/dev/null | head -1; }

start_parent() {
  if parent_alive; then
    echo "parent: already up (pid $(parent_pid), display $(parent_display))"
    return 0
  fi
  [ -r "$CONF" ] || { echo "FAIL: no config at $CONF" >&2; return 2; }
  command -v sway >/dev/null || { echo "FAIL: sway is not installed" >&2; return 2; }

  rm -f "$PARENT_DISPLAY_FILE"

  # The headless backend still renders on the real GPU, so sway must be able to open the render
  # node. /dev/dri/renderD128 is mode 0660 root:render, and this box's login sessions predate the
  # user being added to `render` — /etc/group says yes while the running session's credentials say
  # no, and the only symptom is `Failed to open '/dev/dri/renderD128': Permission denied` followed
  # by a compositor that starts and paints nothing. A fresh login picks the group up; re-exec
  # through one when, and only when, the current session is missing it.
  local launcher=(env)
  if ! id -nG | tr ' ' '\n' | grep -qx render; then
    if sudo -n true 2>/dev/null; then
      echo "parent: session lacks the 'render' group; re-execing through a fresh login" >&2
      launcher=(sudo -n -u "$(id -un)" -i env)
    else
      echo "WARN: not in the 'render' group and sudo needs a password — sway will fail to open" >&2
      echo "      /dev/dri/renderD128. Log out and back in, or run: sg render -c 'Scripts/pi-session.sh start'" >&2
    fi
  fi

  nohup "${launcher[@]}" \
    XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
    SWAYSOCK="$PARENT_SOCK" \
    sway -c "$CONF" >"$PARENT_LOG" 2>&1 &

  local deadline=$((SECONDS + 15))
  while [ "$SECONDS" -lt "$deadline" ]; do
    parent_alive && { echo "parent: up (pid $(parent_pid), display $(parent_display))"; return 0; }
    sleep 0.3
  done
  echo "FAIL: parent never came up; see $PARENT_LOG" >&2
  tail -20 "$PARENT_LOG" >&2
  return 3
}

start_vnc() {
  command -v wayvnc >/dev/null || { echo "vnc:    wayvnc not installed, skipping"; return 0; }
  if [ -n "$(vnc_pid)" ]; then echo "vnc:    already up (pid $(vnc_pid), $VNC_ADDR:$VNC_PORT)"; return 0; fi
  WAYLAND_DISPLAY="$(parent_display)" nohup wayvnc -o HEADLESS-1 "$VNC_ADDR" "$VNC_PORT" \
    >"$VNC_LOG" 2>&1 &
  sleep 1
  if [ -n "$(vnc_pid)" ]; then echo "vnc:    up (pid $(vnc_pid), $VNC_ADDR:$VNC_PORT)"
  else echo "vnc:    failed to start; see $VNC_LOG" >&2; fi
}

case "${1:-status}" in
  start)
    start_parent || exit $?
    start_vnc
    echo
    echo "A drive now needs:"
    echo "  export XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$(parent_display)"
    ;;
  status)
    if parent_alive; then echo "parent: up (pid $(parent_pid), display $(parent_display))"
    else echo "parent: DOWN — run Scripts/pi-session.sh start"; exit 1; fi
    [ -n "$(vnc_pid)" ] && echo "vnc:    up (pid $(vnc_pid), $VNC_ADDR:$VNC_PORT)" || echo "vnc:    down"
    echo "nested lanes currently up:"
    pgrep -af "sway .*/tmp/.*-sway.conf" | sed 's/^/  /' || echo "  (none)"
    ;;
  stop)
    pid="$(parent_pid)"; [ -n "$pid" ] && kill "$pid" && echo "parent: stopped ($pid)" || echo "parent: was not running"
    vpid="$(vnc_pid)";   [ -n "$vpid" ] && kill "$vpid" && echo "vnc:    stopped ($vpid)" || true
    ;;
  env)
    echo "export XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$(parent_display)"
    ;;
  *)
    echo "usage: Scripts/pi-session.sh [start|status|stop|env]" >&2; exit 64;;
esac
