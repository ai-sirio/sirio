#!/usr/bin/env bash
# P119's red-capable probe: live controls, settings selection, and a native
# session reference must survive a quit/relaunch on the same SQLite fixture.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
CAPTURE_DIR="$ROOT/reference/linux-progress/p119"
SOCK=/tmp/pi.sock
DB=/tmp/pi.sqlite
APP="$ROOT/rust/target/debug/tiller"
CTL="$ROOT/rust/target/debug/tillerctl"
LOG1="$CAPTURE_DIR/03-launch-1.log"
LOG2="$CAPTURE_DIR/05-launch-2.log"

if [[ -S "$SOCK" ]]; then
  echo "refusing to reuse a live $SOCK" >&2
  exit 2
fi

run_ctl() { TILLER_SOCKET="$SOCK" "$CTL" "$@"; }
wait_for_socket() {
  for _ in $(seq 1 100); do
    [[ -S "$SOCK" ]] && return 0
    sleep 0.1
  done
  return 1
}
wait_for_no_socket() {
  for _ in $(seq 1 100); do
    [[ ! -S "$SOCK" ]] && return 0
    sleep 0.1
  done
  return 1
}
db_snapshot() {
  local label=$1
  python3 - "$DB" "$label" <<'PY'
import json, sqlite3, sys
path, label = sys.argv[1:]
con = sqlite3.connect(f'file:{path}?mode=ro', uri=True)
con.row_factory = sqlite3.Row
print(f'[{label}] user_version={con.execute("pragma user_version").fetchone()[0]}')
for table in ('tab', 'tab_state', 'session_ref', 'setting', 'sidebar_state'):
    rows = [dict(row) for row in con.execute(f'select * from {table} order by 1')]
    print(f'[{table}] rows={len(rows)}')
    print(json.dumps(rows, sort_keys=True, default=str))
PY
}

rm -f "$SOCK"
(env -u DISPLAY -u WAYLAND_DISPLAY TILLER_SOCKET="$SOCK" TILLER_DB="$DB" "$APP" >"$LOG1" 2>&1 & echo $! >"$CAPTURE_DIR/03-app-1.pid")
wait_for_socket
printf 'launch-1 pid=%s\n' "$(cat "$CAPTURE_DIR/03-app-1.pid")"
run_ctl ping
WORKTREE=$(run_ctl current-workspace --json | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["id"])')
printf 'worktree=%s\n' "$WORKTREE"
PANE=$(run_ctl panel create)
printf 'created-pane=%s\n' "$PANE"
run_ctl session-ref --session "$PANE" --ref p119-native-ref
run_ctl surface settings open --section general
run_ctl surface settings select appearance
run_ctl surface settings read
run_ctl restore-session
printf '%s\n' 'pre-restart panels:'
run_ctl panel list --worktree "$WORKTREE"
db_snapshot pre-restart

run_ctl quit
wait_for_no_socket
printf '%s\n' 'socket absent after quit'
db_snapshot after-quit

(env -u DISPLAY -u WAYLAND_DISPLAY TILLER_SOCKET="$SOCK" TILLER_DB="$DB" "$APP" >"$LOG2" 2>&1 & echo $! >"$CAPTURE_DIR/05-app-2.pid")
wait_for_socket
printf 'launch-2 pid=%s\n' "$(cat "$CAPTURE_DIR/05-app-2.pid")"
run_ctl ping
printf '%s\n' 'post-restart panels:'
run_ctl panel list --worktree "$WORKTREE"
printf '%s\n' 'post-restart settings read:'
if run_ctl surface settings read; then
  SETTINGS_RESULT=restored
else
  SETTINGS_RESULT=not-restored
fi
db_snapshot after-relaunch

PRE_COUNT=$(awk '/^pre-restart panels:/{take=1; next} /^\[pre-restart\]/{take=0} take && /^pane-/{n++} END{print n+0}' "$CAPTURE_DIR/04-restart-probe.txt")
POST_COUNT=$(awk '/^post-restart panels:/{take=1; next} /^post-restart settings read:/{take=0} take && /^pane-/{n++} END{print n+0}' "$CAPTURE_DIR/04-restart-probe.txt")
printf 'assertion pre_panels=%s post_panels=%s settings=%s\n' "$PRE_COUNT" "$POST_COUNT" "$SETTINGS_RESULT"
[[ "$PRE_COUNT" -gt 0 && "$POST_COUNT" -eq 0 && "$SETTINGS_RESULT" == not-restored ]]

run_ctl quit
wait_for_no_socket
rm -f "$SOCK"
