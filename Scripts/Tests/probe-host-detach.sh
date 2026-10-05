#!/bin/bash
set -euo pipefail
# Proves spec §5.4 row by row: a detached child keeps beating after its
# parent's world is killed. Prints one `ROW <name> PASS|FAIL|SKIP` per row and
# `DETACH PROBE OK` when no row failed. --out-dir DIR keeps the evidence.
#
# A row only counts when the kill really happened: the parent's pid must be
# gone before the heartbeat is judged, and the sigkill row kills the parent's
# whole process group, so a child that never left the group cannot survive it.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR=""
while [ $# -gt 0 ]; do case "$1" in --out-dir) OUT_DIR="$2"; shift 2 ;; *) echo "unknown $1" >&2; exit 2 ;; esac; done
WORK="$(mktemp -d "${TMPDIR:-/tmp}/sirio-detach.XXXXXX")"
[ -n "$OUT_DIR" ] && mkdir -p "$OUT_DIR"
( cd "$REPO_ROOT/rust" && cargo build -q -p sirio_host_client --example detach_probe )
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) EXE=.exe; OS=windows ;; Darwin) EXE=; OS=macos ;; *) EXE=; OS=linux ;; esac
PROBE="$REPO_ROOT/rust/target/debug/examples/detach_probe$EXE"
FAILED=0

pid_alive() { # pid -> 0 while the process exists and is not a zombie
  if [ "$OS" = windows ]; then
    tasklist //FI "PID eq $1" //NH 2>/dev/null | grep -Eq "^[^ ]+[[:space:]]+$1[[:space:]]"
  else
    kill -0 "$1" 2>/dev/null && [ "$(ps -o stat= -p "$1" 2>/dev/null | cut -c1)" != Z ]
  fi
}
wait_dead() { # pid -> 0 once it is gone (3 s)
  for _ in $(seq 30); do pid_alive "$1" || return 0; sleep 0.1; done
  return 1
}
wait_started() { # dir -> 0 once the parent has detached its child and said so
  for _ in $(seq 50); do [ -f "$1/child.pid" ] && [ -f "$1/parent.pid" ] && return 0; sleep 0.1; done
  return 1
}
beat() { # dir -> the heartbeat counter, -1 when absent or caught mid-write
  local value
  value="$(cat "$1/heartbeat" 2>/dev/null || true)"
  case "$value" in ''|*[!0-9]*) echo -1 ;; *) echo "$value" ;; esac
}
beats_after_kill() { # dir -> 0 when the heartbeat advanced 1.5 s after the kill
  local before after
  before="$(beat "$1")"; sleep 1.5
  after="$(beat "$1")"
  [ "$after" -gt "$before" ]
}
row() { echo "ROW $1 $2"; [ "$2" = FAIL ] && FAILED=1; [ -n "$OUT_DIR" ] && { rm -rf "${OUT_DIR:?}/$1"; cp -r "$3" "$OUT_DIR/$1"; } 2>/dev/null || true; }
cleanup_child() {
  [ -f "$1/child.pid" ] || return 0
  if [ "$OS" = windows ]; then taskkill //F //PID "$(cat "$1/child.pid")" >/dev/null 2>&1 || true
  else kill "$(cat "$1/child.pid")" 2>/dev/null || true; fi
}
# After the kill: the parent must be dead, and only then does the heartbeat mean anything.
judge() { # name dir
  local name="$1" dir="$2" parent
  parent="$(cat "$dir/parent.pid")"
  if ! wait_dead "$parent"; then
    echo "  $name: parent $parent survived the kill, so the row proves nothing"
    row "$name" FAIL "$dir"; return 0
  fi
  if beats_after_kill "$dir"; then row "$name" PASS "$dir"; else row "$name" FAIL "$dir"; fi
}

# Row: SIGKILL of the parent's whole process group (every unix platform). The
# parent leads a group of its own (job control gives a background job one), so
# a child that stayed in it dies with it; one that left it (setsid, a systemd
# scope, a launchd job) survives.
if [ "$OS" != windows ]; then
  D="$WORK/sigkill"; mkdir -p "$D"
  set -m
  "$PROBE" parent "$D" > "$D/parent.out" & P=$!
  set +m
  PGID="$(ps -o pgid= -p "$P" | tr -d ' ')"
  if [ -z "$PGID" ] || [ "$PGID" = "$(ps -o pgid= -p $$ | tr -d ' ')" ]; then
    kill -9 "$P" 2>/dev/null || true
    echo "  sigkill: could not give the parent a process group of its own"
    row sigkill FAIL "$D"
  elif ! wait_started "$D"; then
    kill -9 -- "-$PGID" 2>/dev/null || true
    echo "  sigkill: the probe never reported a detached child"
    row sigkill FAIL "$D"
  else
    kill -9 -- "-$PGID"; wait "$P" 2>/dev/null || true
    judge sigkill "$D"
  fi
  cleanup_child "$D"
fi

# Row: the parent's systemd scope is stopped (Linux with a user manager).
if [ "$OS" = linux ]; then
  if systemctl --user is-system-running >/dev/null 2>&1 || [ "$(systemctl --user is-system-running 2>/dev/null)" = degraded ]; then
    D="$WORK/scope"; mkdir -p "$D"; UNIT="sirio-probe-parent-$$"
    systemd-run --user --scope --quiet --unit="$UNIT" -- "$PROBE" parent "$D" > "$D/parent.out" &
    if ! wait_started "$D"; then
      systemctl --user stop "$UNIT.scope" 2>/dev/null || true
      echo "  scope: the probe never reported a detached child"
      row scope FAIL "$D"
    else
      systemctl --user stop "$UNIT.scope"
      judge scope "$D"
    fi
    cleanup_child "$D"
  else
    echo "ROW scope SKIP (no systemd user manager)"
  fi
fi

# Row: the parent's job object is closed with kill-on-close (Windows).
if [ "$OS" = windows ]; then
  D="$WORK/job"; mkdir -p "$D"
  "$PROBE" job "$D" > "$D/job.out"
  if [ ! -f "$D/parent.pid" ]; then
    echo "  job: the probe never reported a detached child"
    row job FAIL "$D"
  else
    # A job without BREAKAWAY_OK refuses breakaway and the host stays inside
    # it: the child then dies with the job and the heartbeat stops.
    judge job "$D"
    if grep -q WindowsNoBreakaway "$D/job.out" 2>/dev/null; then
      echo "note: breakaway was refused by the job (WindowsNoBreakaway); the host ran inside it"
    fi
  fi
  cleanup_child "$D"
fi

# Row: Force Quit (macOS) cannot be driven from a script -- see docs/testing/host-detach-probe.md.
[ "$OS" = macos ] && echo "ROW force-quit SKIP (manual, docs/testing/host-detach-probe.md)"

rm -rf "$WORK"
[ "$FAILED" = 0 ] && echo "DETACH PROBE OK" || { echo "DETACH PROBE FAILED"; exit 1; }
