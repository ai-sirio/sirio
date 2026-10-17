#!/bin/bash
set -euo pipefail

# End-to-end test of a project whose disk is unplugged between two launches:
# a real, isolated Sirio, a repository with a linked worktree inside a
# "volume" directory, and the volume moved away the way unplugging a USB disk
# takes /Volumes/<disk> away. The launch without the volume must keep the
# project, both worktrees (labelled "(missing)") and the linked worktree's
# tabs, and the launch after it is plugged back in must show them whole again.
# (The control socket reports branches without the sidebar's "(missing)"
# label; the capture shows it, and session.rs's unit test pins it.)
# Before the fix, the first of those launches deleted the project from the
# database, with its worktrees and tabs by cascade.
#
# The artifact: --out-dir DIR (default artifacts/unplugged-volume-e2e-<stamp>-<pid>)
# keeps transcript.log, app.log, each launch's `project list` answer and --
# with --display -- PID-matched captures of the window while the volume is
# away and after it is back. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-unplugged-volume-e2e.sh [--out-dir DIR] [--display :N]

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
HOST_BIN="$TARGET_DIR/debug/sirio-host"
PROBE="$TARGET_DIR/debug/examples/host_probe"
OUT_DIR=""
DISPLAY_TARGET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/unplugged-volume-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
if [ -n "$DISPLAY_TARGET" ]; then
  for tool in import identify xwininfo xprop; do
    command -v "$tool" >/dev/null || fail "$tool is required for the capture; drop --display to skip it"
  done
fi
command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required"

RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-unplugged-e2e-XXXXXX")
APP_PID=""
alive() { kill -0 "$1" 2>/dev/null; }
cleanup() {
  if [ -n "$APP_PID" ] && alive "$APP_PID"; then
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  fi
  # The host outlives the app by design; end this run's own.
  if [ -x "$PROBE" ]; then
    for f in "$RUN_DIR"/host/host-v*.json; do
      [ -f "$f" ] || continue
      major="${f##*host-v}"
      SIRIO_HOST_HOME="$RUN_DIR/host" SIRIO_HOST_PROTOCOL_MAJOR="${major%.json}" \
        "$PROBE" shutdown --force >/dev/null 2>&1 || true
    done
  fi
  cp "$RUN_DIR"/app.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio, sirioctl, sirio-host and host_probe"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host -p sirio_host_client --example host_probe)

# ---- the world: every path the app or a tool may write to is this run's own
export SIRIO_SOCKET="$RUN_DIR/c.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
export SIRIO_CREDENTIALS="$RUN_DIR/credentials.json"
export SIRIO_HOST_HOME="$RUN_DIR/host"
export SIRIO_HOST_BIN="$HOST_BIN"
export HOME="$RUN_DIR/home"
export XDG_STATE_HOME="$RUN_DIR/xdg/state" XDG_DATA_HOME="$RUN_DIR/xdg/data"
export XDG_CONFIG_HOME="$RUN_DIR/xdg/config" XDG_CACHE_HOME="$RUN_DIR/xdg/cache"
export XDG_RUNTIME_DIR="$RUN_DIR/xdg/runtime"
mkdir -p "$HOME" "$SIRIO_HOST_HOME" "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" \
  "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# ---- the volume: a repository and its linked worktree beside it
VOLUME="$RUN_DIR/volume"
UNPLUGGED="$RUN_DIR/volume-unplugged"
REPO="$VOLUME/repo"
LINKED="$VOLUME/repo-feature"
ELSEWHERE="$RUN_DIR/elsewhere"
mkdir -p "$REPO" "$ELSEWHERE"
for dir in "$REPO" "$ELSEWHERE"; do
  git -C "$dir" init -q -b main
  git -C "$dir" -c user.email=t@example.invalid -c user.name=Tester commit -q --allow-empty -m first
done
git -C "$REPO" worktree add -q -b feature "$LINKED"

LAUNCH=0
app_start() {
  LAUNCH=$((LAUNCH + 1))
  rm -f "$SIRIO_SOCKET"
  echo "--- launch $LAUNCH" >>"$RUN_DIR/app.log"
  if [ -n "$DISPLAY_TARGET" ]; then
    (cd "$ELSEWHERE" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 \
      "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
  else
    (cd "$ELSEWHERE" && exec env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
  fi
  APP_PID=$!
  for _ in $(seq 1 300); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.1
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket (launch $LAUNCH)"
  for _ in $(seq 1 300); do
    "$CTL" surface tabs read >/dev/null 2>&1 && return 0
    sleep 0.2
  done
  fail "sirio's control actions never became ready (launch $LAUNCH)"
}
app_quit() {
  "$CTL" quit >/dev/null 2>&1 || true
  for _ in $(seq 1 100); do
    alive "$APP_PID" || { wait "$APP_PID" 2>/dev/null || true; APP_PID=""; return 0; }
    sleep 0.2
  done
  fail "sirio $APP_PID did not quit after sirioctl quit (launch $LAUNCH)"
}
ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }

# The catalog as `project list` reports it, saved for the artifact, then
# reduced to one line per worktree of the volume's repository:
# "<path> <branch label>". Prints nothing when the project is gone.
volume_rows() { # name
  "$CTL" project list --json >"$OUT_DIR/project-list-$1.json" || fail "project list did not answer ($1)"
  python3 - "$OUT_DIR/project-list-$1.json" "$REPO" <<'PY'
import json, os, sys
answer = json.load(open(sys.argv[1]))
repo = os.path.realpath(sys.argv[2])
for project in answer:
    if os.path.realpath(project["path"]) != repo:
        continue
    for worktree in json.loads(project["worktrees"]):
        print(os.path.realpath(worktree["path"]), worktree["branch"])
PY
}
# The ids of the linked worktree's persisted tabs, read from the database
# (read-only, so a running app's writes are never disturbed). A deleted and re-created strip has new ids.
linked_tab_ids() {
  python3 - "$SIRIO_DB" "$LINKED" <<'PY'
import os, sqlite3, sys
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
# The database holds canonical paths (/private/var on macOS, not /var).
rows = db.execute(
    "SELECT tab.id FROM tab JOIN worktree ON worktree.id = tab.worktree_id"
    " WHERE worktree.path = ? ORDER BY tab.order_idx",
    (os.path.realpath(sys.argv[2]),),
).fetchall()
print(" ".join(row[0] for row in rows))
PY
}
dump_db() {
  python3 - "$SIRIO_DB" <<'PY'
import sqlite3, sys
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
for table, columns in (("worktree", "id, path, branch"), ("tab", "id, worktree_id, kind, title")):
    for row in db.execute(f"SELECT {columns} FROM {table}"):
        print(f"{table}: {row}")
PY
}
tab_count() {
  "$CTL" surface tabs read --json | python3 -c 'import json, sys; a = json.load(sys.stdin); print(len(a if isinstance(a, list) else a.get("tabs", [])))'
}
capture() { # name
  [ -n "$DISPLAY_TARGET" ] || return 0
  sleep 2
  local window
  window=$(DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"], capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"], capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $1"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/$1.png"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}

REPO_REAL="$(cd "$REPO" && pwd -P)"
LINKED_REAL="$(cd "$LINKED" && pwd -P)"
EXPECTED_ROWS="$REPO_REAL main
$LINKED_REAL feature"

echo "step 1: both projects added, the linked worktree given tabs"
app_start
ctl project add "$REPO" >/dev/null
ctl project add "$ELSEWHERE" >/dev/null
ctl select-workspace --workspace "$LINKED"
TABS=""
for _ in $(seq 1 50); do
  TABS=$(tab_count)
  [ "$TABS" -gt 0 ] && break
  sleep 0.2
done
[ "$TABS" -gt 0 ] || fail "the linked worktree opened no tabs"
echo "OK: the linked worktree has $TABS tabs"
# A strip that is still the untouched default is not written; a terminal the
# user opened is. Wait for the database to hold it rather than assume.
ctl surface terminal open --cmd "sleep 600" >/dev/null
TAB_IDS=""
for _ in $(seq 1 75); do
  TAB_IDS=$(linked_tab_ids)
  [ -n "$TAB_IDS" ] && break
  sleep 0.2
done
[ -n "$TAB_IDS" ] || { dump_db; fail "the linked worktree's tabs were never persisted"; }
ROWS=$(volume_rows plugged-before)
[ "$ROWS" = "$EXPECTED_ROWS" ] || fail "unexpected rows before unplugging:
$ROWS"
echo "OK: the repository lists both worktrees"
app_quit
TAB_IDS=$(linked_tab_ids)
[ -n "$TAB_IDS" ] || { dump_db; fail "the linked worktree's tabs were gone after quitting"; }
echo "OK: the linked worktree's tabs are persisted: $TAB_IDS"

echo "step 2: the volume is unplugged; a launch keeps the project and both worktrees"
mv "$VOLUME" "$UNPLUGGED"
app_start
ROWS=$(volume_rows unplugged)
[ -n "$ROWS" ] || fail "the launch without the volume dropped the project"
[ "$ROWS" = "$EXPECTED_ROWS" ] || fail "unexpected rows while unplugged:
$ROWS"
echo "OK: both worktrees kept"
grep -q "is not reachable, keeping its last-known state" "$RUN_DIR/app.log" \
  || fail "app.log does not say why the project was kept"
echo "OK: app.log says the project was not reachable"
capture unplugged
app_quit
[ "$(linked_tab_ids)" = "$TAB_IDS" ] || fail "the launch without the volume changed the linked worktree's tabs: $(linked_tab_ids)"
echo "OK: the launch without the volume left the linked worktree's tabs as they were"

echo "step 3: plugged back in; a launch shows the project whole, tabs included"
mv "$UNPLUGGED" "$VOLUME"
app_start
ROWS=$(volume_rows plugged-after)
[ "$ROWS" = "$EXPECTED_ROWS" ] || fail "unexpected rows after plugging back in:
$ROWS"
echo "OK: both worktrees back"
capture plugged-back
ctl select-workspace --workspace "$LINKED"
AFTER=""
for _ in $(seq 1 50); do
  AFTER=$(tab_count)
  [ "$AFTER" = "$TABS" ] && break
  sleep 0.2
done
[ "$AFTER" = "$TABS" ] || fail "the linked worktree came back with $AFTER tabs, not $TABS"
echo "OK: the linked worktree shows its $TABS tabs"
app_quit
[ "$(linked_tab_ids)" = "$TAB_IDS" ] || fail "the linked worktree came back with other tabs: $(linked_tab_ids)"
echo "OK: the linked worktree came back with the same tabs"

echo "UNPLUGGED VOLUME E2E OK"
