#!/bin/bash
set -euo pipefail
# Proves spec §5.4 row by row: a detached child keeps beating after its
# parent's world is killed. Prints one `ROW <name> PASS|FAIL|SKIP` per row and
# `DETACH PROBE OK` when no row failed. --out-dir DIR keeps the evidence.
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

beats_after_kill() { # dir -> 0 when the heartbeat advanced 1.5 s after the kill
  local dir="$1" before after
  before="$(cat "$dir/heartbeat" 2>/dev/null || echo -1)"; sleep 1.5
  after="$(cat "$dir/heartbeat" 2>/dev/null || echo -1)"
  [ "$after" -gt "$before" ]
}
row() { echo "ROW $1 $2"; [ "$2" = FAIL ] && FAILED=1; [ -n "$OUT_DIR" ] && cp -r "$3" "$OUT_DIR/$1" 2>/dev/null || true; }
cleanup_child() { [ -f "$1/child.pid" ] && kill "$(cat "$1/child.pid")" 2>/dev/null || true; }

# Row: SIGKILL of the parent (every unix platform).
if [ "$OS" != windows ]; then
  D="$WORK/sigkill"; mkdir -p "$D"
  "$PROBE" parent "$D" > "$D/parent.out" & P=$!
  for _ in $(seq 50); do [ -f "$D/child.pid" ] && break; sleep 0.1; done
  kill -9 "$P"; wait "$P" 2>/dev/null || true
  if beats_after_kill "$D"; then row sigkill PASS "$D"; else row sigkill FAIL "$D"; fi
  cleanup_child "$D"
fi

# Row: the parent's systemd scope is stopped (Linux with a user manager).
if [ "$OS" = linux ]; then
  if systemctl --user is-system-running >/dev/null 2>&1 || [ "$(systemctl --user is-system-running 2>/dev/null)" = degraded ]; then
    D="$WORK/scope"; mkdir -p "$D"; UNIT="sirio-probe-parent-$$"
    systemd-run --user --scope --quiet --unit="$UNIT" -- "$PROBE" parent "$D" > "$D/parent.out" &
    for _ in $(seq 50); do [ -f "$D/child.pid" ] && break; sleep 0.1; done
    systemctl --user stop "$UNIT.scope"
    if beats_after_kill "$D"; then row scope PASS "$D"; else row scope FAIL "$D"; fi
    cleanup_child "$D"
  else
    echo "ROW scope SKIP (no systemd user manager)"
  fi
fi

# Row: the parent's job object is closed with kill-on-close (Windows).
if [ "$OS" = windows ]; then
  D="$WORK/job"; mkdir -p "$D"
  "$PROBE" job "$D" > "$D/job.out"
  if beats_after_kill "$D"; then row job PASS "$D"; else row job FAIL "$D"; fi
  grep -q WindowsNoBreakaway "$D/job.out" 2>/dev/null && echo "note: breakaway refused by the job"
  cleanup_child "$D"
fi

# Row: Force Quit (macOS) cannot be driven from a script -- see docs/testing/host-detach-probe.md.
[ "$OS" = macos ] && echo "ROW force-quit SKIP (manual, docs/testing/host-detach-probe.md)"

rm -rf "$WORK"
[ "$FAILED" = 0 ] && echo "DETACH PROBE OK" || { echo "DETACH PROBE FAILED"; exit 1; }
