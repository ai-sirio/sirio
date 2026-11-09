#!/bin/bash
set -eEuo pipefail

# End-to-end test of folded directories in the Files tree
# (docs/superpowers/specs/2026-11-08-folded-directories-design.md §6): a real, isolated debug
# Sirio on a private Xvfb, driven with real X clicks and keys at the bounds the debug-only
# `sirioctl surface files read` reports for the rows of the last drawn frame.
#
# One app, one fixture repository (a Maven-like tree with single-child chains and an unstaged
# deletion inside one of them). Each check prints `ok <n> <name>`:
#   1 nested listings stay current         a file made under the third open level shows, and goes
#   2 chains are joined, never unfolded    the joined rows exist at their depths; no link of a chain
#                                          was ever built as a row of its own (seen.log)
#   3 the joined row carries the change    the git status of the outermost link, not the leaf's
#   4 opening a joined row                 its children are joined too; the row is expanded, selected
#   5 the row splits and rejoins           a sibling file splits the chain, Up moves to the middle
#                                          link, removing the file rejoins it, still selected
#   6 a closed row stays closed           Space closes the joined row; a refresh does not reopen it
#   7 hidden files decide the chain        a dotfile does not split it until hidden files are shown
#                                          and a row whose links disagree, either way, draws closed
#                                          and opens with one click
#   8 a worktree switch keeps the rows     away and back, the opened rows are as they were
#
# `seen.log` is the union of every label the app built since the previous read (a destructive
# read the script drains on every poll); `never_built` and `ever_built` ask it. The artifact:
# --out-dir DIR (default artifacts/files-fold-e2e-<stamp>-<pid>) keeps transcript.log (the
# script's notes and every logged sirioctl call with its reply; the reads and the quit sent
# through the quiet ctlq are not logged), captures/light-<n>.png, light-5-split.png and
# light-7-disagree.png (PID-matched window captures), seen-<n>.log snapshots, git-status-<n>.txt,
# light-app.log and xvfb.log, and from any run seen.log and last-state.txt (the last Files read),
# written on exit. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-files-fold-e2e.sh --xvfb [--out-dir DIR]
#
#   --xvfb     start a private Xvfb on a free display between :90 and :99
#   --out-dir  where the artifact goes

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
SEED="$TARGET_DIR/debug/examples/appearance_seed"
PROBE="$TARGET_DIR/debug/examples/host_probe"

[ "$(uname -s)" = Linux ] || { echo "FILES FOLD E2E runs on Linux only (it needs Xvfb)" >&2; exit 2; }

START_XVFB=0
OUT_DIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ "$START_XVFB" -eq 1 ] || { echo "pass --xvfb: the run needs its own display" >&2; exit 2; }
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/files-fold-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/captures"
OUT_DIR="$(cd "$OUT_DIR" && pwd -P)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

SCENARIO="setup"
fail() {
  echo "FAIL: $*" >&2
  # A capture of what the window showed at the failure, best effort.
  if [ -n "${WIN:-}" ] && [ -n "${DISPLAY_TARGET:-}" ] && [ "${FAILING:-0}" = 0 ]; then
    FAILING=1
    timeout 20 import -display "$DISPLAY_TARGET" -window "$WIN" "$OUT_DIR/captures/failure.png" 2>/dev/null \
      && echo "failure capture: $OUT_DIR/captures/failure.png" >&2
  fi
  exit 1
}
trap 'echo "FAIL: $SCENARIO: unexpected error at line $LINENO" >&2' ERR
# Notes go to stderr: a helper whose answer is read with $(...) still logs.
note() { echo "[$SCENARIO] $*" >&2; }
ok() { # n name
  echo "ok $1 $2"
}

for tool in python3 git Xvfb xdpyinfo xdotool xwininfo xprop import identify; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
TIMEOUT_BIN="$(command -v timeout)"
REAL_GIT="$(command -v git)"

# ---------------------------------------------------------------- build

echo "building sirio, sirioctl, sirio-host, the host probe and the appearance seed"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host -p sirio_host_client --example host_probe \
  -p sirio_persistence --example appearance_seed)
for binary in "$BIN" "$CTL" "$SEED" "$PROBE"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run's world

# Short, because the control socket lives in it.
BASE="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}"
mkdir -p "$BASE"
WORK="$(python3 -I -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$(mktemp -d "$BASE/files-fold-XXXXXX")")"
SD=""
APP_PID=""
XVFB_PID=""
WIN=""
DISPLAY_TARGET=""

alive() { kill -0 "$1" 2>/dev/null; }
terminate() { # pid
  kill -TERM "$1" 2>/dev/null || return 0
  local n=0
  while alive "$1" && [ "$n" -lt 50 ]; do n=$((n + 1)); sleep 0.1; done
  kill -KILL "$1" 2>/dev/null || true
  wait "$1" 2>/dev/null || true
}
is_a_host() {
  case "$(cat "/proc/$1/comm" 2>/dev/null || true)" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
stop_hosts() {
  local dir pid
  for dir in "$WORK"/*/host; do
    [ -d "$dir" ] || continue
    SIRIO_HOST_HOME="$dir" "$PROBE" shutdown --force >/dev/null 2>&1 || true
  done
  for pid in $(pgrep -f "$WORK" 2>/dev/null || true); do
    is_a_host "$pid" && kill "$pid" 2>/dev/null || true
  done
}
collect_logs() {
  local dir
  for dir in "$WORK"/*/; do
    [ -f "$dir/app.log" ] && cp "$dir/app.log" "$OUT_DIR/$(basename "$dir")-app.log" 2>/dev/null || true
  done
}
cleanup() {
  local rc=$?
  set +e
  trap - ERR
  [ -z "$APP_PID" ] || terminate "$APP_PID"
  # What a failed run needs to be diagnosed, before $WORK goes.
  [ ! -f "$WORK/seen.log" ] || cp "$WORK/seen.log" "$OUT_DIR/seen.log"
  printf '%s\n' "${STATE:-}" >"$OUT_DIR/last-state.txt"
  collect_logs
  stop_hosts
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  rm -rf "$WORK"
  exit "$rc"
}
trap cleanup EXIT

for number in $(seq 90 99); do
  [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
done
[ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
Xvfb "$DISPLAY_TARGET" -screen 0 1920x1080x24 >"$OUT_DIR/xvfb.log" 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
lvp="$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)"
[ -n "$lvp" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required on Xvfb"
echo "display: $DISPLAY_TARGET"

# Stub agent CLIs (without them the app runs `claude --version` through the
# user's own wrapper) and a shell that reads no profile.
mkdir -p "$WORK/stubs" "$WORK/shell"
for cli in claude codex opencode pi omp; do
  printf '#!/bin/sh\necho "2.1.257 (Claude Code)"\n' >"$WORK/stubs/$cli"
  chmod +x "$WORK/stubs/$cli"
done
printf '#!/bin/sh\nexec /bin/bash --noprofile --norc "$@"\n' >"$WORK/shell/shell"
chmod +x "$WORK/shell/shell"


# The project: a Maven-like tree with single-child chains (java/it/acme/app/v2, domain/model,
# test/java/..., docs/adr, config/pmd, tools/pkg) and one real branching (src/main, src/test).
REPO="$WORK/repo"
fixture_git() {
  env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    "$REAL_GIT" "$@"
}
mkdir -p "$REPO"
fixture_git -C "$REPO" init -q -b main
fixture_git -C "$REPO" config user.email tester@example.invalid
fixture_git -C "$REPO" config user.name Tester
for file in \
  source/pom.xml \
  source/src/main/java/it/acme/app/v2/App.java \
  source/src/main/java/it/acme/app/v2/domain/model/Order.java \
  source/src/main/java/it/Old.java \
  source/src/main/resources/application.yml \
  source/src/test/java/it/acme/app/v2/AppTest.java \
  source/docs/adr/0001.md \
  source/config/pmd/rules.xml \
  source/tools/pkg/__init__.py \
  source/tools/pkg/sub/__init__.py \
  source/tools/pkg/sub/mod.py; do
  mkdir -p "$REPO/$(dirname "$file")"
  printf '%s\n' "$file" >"$REPO/$file"
done
fixture_git -C "$REPO" add -A
fixture_git -C "$REPO" commit -q -m first
# An unstaged deletion inside the chain: the middle link `it` carries it, the leaf `v2` does not.
rm "$REPO/source/src/main/java/it/Old.java"
# A second project, only to switch worktree to (check 8).
OTHER_REPO="$WORK/other"
mkdir -p "$OTHER_REPO"
fixture_git -C "$OTHER_REPO" init -q -b main
fixture_git -C "$OTHER_REPO" config user.email tester@example.invalid
fixture_git -C "$OTHER_REPO" config user.name Tester
printf 'Another project.\n' >"$OTHER_REPO/README"
fixture_git -C "$OTHER_REPO" add -A
fixture_git -C "$OTHER_REPO" commit -q -m first

# ---------------------------------------------------------------- calling the app

with_timeout() { # seconds command...
  local seconds=$1
  shift
  "$TIMEOUT_BIN" -k 5 "$seconds" "$@"
}
ctlq() { # args...: quiet, for polled reads
  with_timeout 30 env -i PATH=/usr/bin:/bin HOME="$SD/home" SIRIO_SOCKET="$SIRIO_SOCKET" "$CTL" "$@"
}
ctl() { # args...: logged, and a failing call fails the scenario
  local out rc=0
  echo "[$SCENARIO] + sirioctl $*" >&2
  out="$(ctlq "$@" 2>&1)" || rc=$?
  if [ -n "$out" ]; then printf '%s\n' "$out" | sed "s/^/[$SCENARIO]   /" >&2; fi
  [ "$rc" -eq 0 ] || fail "$SCENARIO: sirioctl $* exited $rc"
  printf '%s\n' "$out"
}


# A verb's reply: `key=value` lines, matched whole unless the pair is `key~=text`, which asks the
# value to contain text. Three more pair forms read the rows of the Files read, by label:
#   row:LABEL:Key=Value   the row labelled LABEL has row<i>Key equal to Value
#   norow:LABEL           no row is labelled LABEL
#   after:A:B             a row labelled B is listed below the row labelled A
STATE=""
: >"$WORK/seen.log"
read_verb() { # files
  local out
  case "$1" in
    files)
      out="$(ctlq surface files read)" || return 1
      # seen<i> is a destructive read: every value of every read joins seen.log.
      sed -n 's/^seen[0-9][0-9]*=//p' <<<"$out" >>"$WORK/seen.log"
      printf '%s\n' "$out"
      ;;
  esac
}
value() { sed -n "s/^$1=//p" <<<"$STATE" | head -n 1; }
row_index_of() { # label: the index of the row with that label in the last read
  local i=0 count
  count="$(value count)"
  while [ "$i" -lt "${count:-0}" ]; do
    [ "$(value "row${i}Label")" = "$1" ] && { echo "$i"; return 0; }
    i=$((i + 1))
  done
  return 1
}
row_pair_holds() { # LABEL:Key=Value
  local label=${1%%:*} kv=${1#*:} i
  i="$(row_index_of "$label")" || return 1
  [ "$(value "row${i}${kv%%=*}")" = "${kv#*=}" ]
}
pair_holds() { # pair
  local a b
  case "$1" in
    row:*) row_pair_holds "${1#row:}" ;;
    norow:*) ! row_index_of "${1#norow:}" >/dev/null ;;
    after:*)
      a="$(row_index_of "$(sed 's/^after:\([^:]*\):.*/\1/' <<<"$1")")" || return 1
      b="$(row_index_of "${1##*:}")" || return 1
      [ "$b" -gt "$a" ]
      ;;
    *'~='*) [[ "$(value "${1%%~=*}")" == *"${1#*~=}"* ]] ;;
    *) grep -Fxq -- "$1" <<<"$STATE" ;;
  esac
}
last_read() { printf '%s' "$STATE" | tr '\n' ' ' | cut -c1-1500; }
read_state() { # verb
  STATE="$(read_verb "$1" 2>&1)" || fail "$SCENARIO: the $1 verb did not answer: $STATE"
}
DEADLINE_SECS=30
# Polls the verb until one read holds every pair; the deadline is the failure.
wait_on() { # verb pair...
  local verb=$1 deadline pair good
  shift
  deadline=$(( $(date +%s) + DEADLINE_SECS ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    STATE="$(read_verb "$verb" 2>&1)" || { sleep 0.2; continue; }
    good=1
    for pair in "$@"; do pair_holds "$pair" || { good=0; break; }; done
    if [ "$good" -eq 1 ]; then
      note "OK: $verb: $*"
      return 0
    fi
    sleep 0.2
  done
  fail "$SCENARIO: $verb never read: $* -- last read: $(last_read)"
}
# Every read for SECONDS holds every pair. Only meaningful after a completion
# signal, as a "stays" check.
hold_on() { # seconds verb pair...
  local end verb=$2 pair
  end=$(( $(date +%s) + $1 ))
  shift 2
  while :; do
    read_state "$verb"
    for pair in "$@"; do pair_holds "$pair" || fail "$SCENARIO: $verb: $pair stopped holding -- read: $(last_read)"; done
    [ "$(date +%s)" -lt "$end" ] || break
    sleep 0.2
  done
  note "OK: held for $1 s: $verb: $*"
}
# Polls a shell condition until it holds.
wait_until() { # description command...
  local description=$1 deadline
  shift
  deadline=$(( $(date +%s) + DEADLINE_SECS ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if "$@"; then
      note "OK: $description"
      return 0
    fi
    sleep 0.2
  done
  fail "$SCENARIO: never: $description"
}

# ---------------------------------------------------------------- the rows, the log, the disk

# Drains the app's record of built labels into seen.log, then empties it: what is built from now
# on is all the log holds.
seen_reset() {
  read_state files
  : >"$WORK/seen.log"
}
# Fails when a row was ever built with one of these exact labels since the last seen_reset.
never_built() { # label...
  local label
  for label in "$@"; do
    if grep -Fxq -- "$label" "$WORK/seen.log"; then
      fail "$SCENARIO: a row labelled '$label' was built unfolded -- seen.log: $(tr '\n' '|' <"$WORK/seen.log" | cut -c1-600)"
    fi
  done
  note "OK: never built: $*"
}
# Fails unless a row with exactly this label was built since the last seen_reset: the positive
# control of never_built, which would also pass if the record were empty.
ever_built() { # label...
  local label
  for label in "$@"; do
    grep -Fxq -- "$label" "$WORK/seen.log" \
      || fail "$SCENARIO: no row labelled '$label' was ever built -- seen.log: $(tr '\n' '|' <"$WORK/seen.log" | cut -c1-600)"
  done
  note "OK: built: $*"
}
# Milliseconds since the epoch; EPOCHREALTIME's separator follows the locale.
now_ms() { local us=${EPOCHREALTIME/[.,]/}; printf '%s' $(( us / 1000 )); }
# Waits until `refreshes` has not moved for MS milliseconds, starting the count over each time it
# moves; the deadline is the failure. A late refresh is waited out, never taken as a failure.
settle_refreshes() { # ms
  local deadline last since now
  deadline=$(( $(now_ms) + DEADLINE_SECS * 1000 ))
  read_state files
  last="$(value refreshes)"
  since=$(now_ms)
  while :; do
    now=$(now_ms)
    [ $(( now - since )) -lt "$1" ] || break
    [ "$now" -lt "$deadline" ] || fail "$SCENARIO: refreshes never settled -- last read: $(last_read)"
    sleep 0.2
    read_state files
    if [ "$(value refreshes)" != "$last" ]; then
      last="$(value refreshes)"
      since=$(now_ms)
    fi
  done
  note "OK: refreshes settled at $last"
}
drawn_refreshes_over() { # n
  STATE="$(read_verb files 2>&1)" || return 1
  [ "$(value drawnRefreshes)" -gt "$1" ]
}
# Clicks the row labelled LABEL once, at the bounds of the read just taken (a single mouse-down
# toggles a directory and selects it).
click_row() { # label
  local i
  i="$(row_index_of "$1")" || fail "$SCENARIO: no row labelled '$1' -- read: $(last_read)"
  click_at_bounds "$(value "row${i}Bounds")"
}
# Waits for the directory row, clicks it, and waits for it to read open.
open_dir() { # label
  wait_on files "row:$1:Dir=1"
  click_row "$1"
  wait_on files "row:$1:Expanded=1"
}
# What a check leaves: a capture, the seen.log so far and git's own view of the fixture.
snapshot() { # n
  capture "$1"
  cp "$WORK/seen.log" "$OUT_DIR/seen-$1.log"
  fixture_git --no-optional-locks -C "$REPO" status --short >"$OUT_DIR/git-status-$1.txt"
}

# ---------------------------------------------------------------- the display

find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 -I - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"],
                         capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"],
                          capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
}
# captures/<theme>-<name>.png of this app's window, once it has drawn (after
# SETTLE seconds, 2 by default).
capture() { # name [settle]
  sleep "${2:-2}"
  local colours size file="$OUT_DIR/captures/$THEME-$1.png"
  with_timeout 20 import -display "$DISPLAY_TARGET" -window "$WIN" "$file" || fail "$SCENARIO: capture of $1 failed"
  [ -s "$file" ] || fail "$SCENARIO: $1.png is empty"
  colours="$(identify -format '%k' "$file")"
  [ "$colours" -ge 200 ] || fail "$SCENARIO: $1 is blank ($colours colours)"
  size="$(xwininfo -display "$DISPLAY_TARGET" -id "$WIN" | awk '/Width:/ {w = $2} /Height:/ {h = $2} END {print w "x" h}')"
  [ "$(identify -format '%wx%h' "$file")" = "$size" ] \
    || fail "$SCENARIO: $1.png is $(identify -format '%wx%h' "$file"), the window is $size"
  note "CAPTURE: captures/$THEME-$1.png ($colours colours, $size; pid $APP_PID)"
}
# Xvfb has no window manager, so the window is focused before every key.
focus_app() { DISPLAY="$DISPLAY_TARGET" with_timeout 10 xdotool windowfocus --sync "$WIN"; }
xkey() { # key names...
  focus_app
  DISPLAY="$DISPLAY_TARGET" xdotool key "$@"
}
# The centre of an `x,y,w,h` bounds value, in window pixels
# (GPUI_X11_SCALE_FACTOR=1 makes a window pixel an X pixel).
center_of() { # bounds
  local x y w h
  IFS=, read -r x y w h <<<"$1"
  echo "$((x + w / 2)) $((y + h / 2))"
}
point_at() { # bounds
  local point
  [ -n "$1" ] || fail "$SCENARIO: no bounds to point at -- read: $(last_read)"
  point="$(center_of "$1")"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$WIN" ${point% *} ${point#* }
}
click_at_bounds() { # bounds [button]
  focus_app
  point_at "$1"
  DISPLAY="$DISPLAY_TARGET" xdotool click "${2:-1}"
}

# ---------------------------------------------------------------- an app

app_start() { # theme
  local n=0
  THEME="$1"
  SD="$WORK/$THEME"
  mkdir -p "$SD/home" "$SD/tmp" "$SD/cwd" "$SD/host" "$SD/xdg" "$SD/xdg-config" "$SD/xdg-state" \
    "$SD/xdg-cache" "$SD/xdg-runtime" "$SD/projects"
  chmod 700 "$SD/xdg-runtime"
  SIRIO_SOCKET="$SD/c.sock"
  SIRIO_DB="$SD/session.sqlite"
  [ "${#SIRIO_SOCKET}" -lt 100 ] || fail "the socket path is too long for a unix socket: $SIRIO_SOCKET (set SIRIO_E2E_TMP to a shorter directory)"
  "$SEED" --database "$SIRIO_DB" --appearance "$THEME"
  WENV=(
    "HOME=$SD/home" "PATH=$WORK/stubs:/usr/bin:/bin" "SHELL=$WORK/shell/shell" "TMPDIR=$SD/tmp" "LANG=C.UTF-8"
    "XDG_CONFIG_HOME=$SD/xdg-config" "XDG_DATA_HOME=$SD/xdg" "XDG_STATE_HOME=$SD/xdg-state"
    "XDG_CACHE_HOME=$SD/xdg-cache" "XDG_RUNTIME_DIR=$SD/xdg-runtime"
    "SIRIO_DB=$SIRIO_DB" "SIRIO_SOCKET=$SIRIO_SOCKET" "SIRIO_CREDENTIALS=$SD/credentials.json"
    "SIRIO_HOST_HOME=$SD/host" "SIRIO_PROJECTS_DIR=$SD/projects"
    "DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1" "VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"
  )
  # `exec` keeps the subshell's PID: $! is the app, which the captures match.
  (cd "$SD/cwd" && exec env -i "${WENV[@]}" "$BIN" >>"$SD/app.log" 2>&1) &
  APP_PID=$!
  while [ ! -S "$SIRIO_SOCKET" ] && [ "$n" -lt 300 ]; do n=$((n + 1)); sleep 0.1; done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket (see $SD/app.log)"
  n=0
  while ! ctlq surface tabs read >/dev/null 2>&1 && [ "$n" -lt 600 ]; do n=$((n + 1)); sleep 0.1; done
  ctlq surface tabs read >/dev/null 2>&1 || fail "sirio's control actions never became ready"
  WIN=""
  for _ in $(seq 1 50); do
    WIN="$(find_window)"
    [ -n "$WIN" ] && break
    sleep 0.2
  done
  [ -n "$WIN" ] || fail "no window with _NET_WM_PID=$APP_PID"
  note "app up (pid $APP_PID, window $WIN, $THEME)"
}
app_quit() {
  [ -n "$APP_PID" ] || return 0
  ctlq quit >/dev/null 2>&1 || true
  local n=0
  while alive "$APP_PID" && [ "$n" -lt 200 ]; do n=$((n + 1)); sleep 0.1; done
  alive "$APP_PID" && fail "$SCENARIO: sirio $APP_PID did not quit after sirioctl quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
  SIRIO_HOST_HOME="$SD/host" "$PROBE" shutdown --force >/dev/null 2>&1 || true
}

# Adds both projects and selects the repository's worktree; the Files view is the panel's default.
select_repo() {
  local id
  ctl project add "$REPO" >/dev/null
  ctl project add "$OTHER_REPO" >/dev/null
  wait_until "both repositories are listed" both_listed
  id="$(worktree_id)"
  ctl select-workspace --workspace "$id" >/dev/null
  wait_on files settled=1 rightPanelView=files
}
both_listed() { [ -n "$(worktree_id)" ] && [ -n "$(worktree_id_of "$OTHER_REPO")" ]; }
worktree_id() { worktree_id_of "$REPO"; }
worktree_id_of() { # repository
  # The whole listing first: awk quitting early would break sirioctl's pipe (a panic, and with
  # pipefail a failed call).
  local listing
  listing="$(ctlq list-workspaces 2>/dev/null)" || return 0
  awk -F '\t' -v repo="$1" '$4 == repo {print $1; exit}' <<<"$listing"
}

# ---------------------------------------------------------------- checks

JAVA='java / it / acme / app / v2'
THEME="light"
SCENARIO="launch"
app_start light
select_repo
wait_on files 'row:source:Dir=1' 'row:source:Depth=0'
seen_reset

SCENARIO="nested_listings"
open_dir source
open_dir src
open_dir main
# main's own listing has landed once its children show.
wait_on files 'row:resources:Depth=3'
printf 'notes\n' >"$REPO/source/src/main/NOTES.md"
wait_on files 'row:NOTES.md:Depth=3'
rm "$REPO/source/src/main/NOTES.md"
# Paired with rows that must still be there, so an empty read cannot satisfy norow.
wait_on files shown=1 'row:resources:Depth=3' 'norow:NOTES.md'
snapshot 1
ok 1 "nested listings stay current"

SCENARIO="chains_joined"
wait_on files "row:$JAVA:Depth=3" \
  'row:test / java / it / acme / app / v2:Depth=2' \
  'row:docs / adr:Depth=1' 'row:config / pmd:Depth=1' 'row:tools / pkg:Depth=1'
wait_on files "row:$JAVA:Path=source/src/main/java/it/acme/app/v2"
ever_built "$JAVA" 'docs / adr' 'test / java / it / acme / app / v2'
never_built java docs config tools test \
  'java / it' 'java / it / acme' 'java / it / acme / app' \
  'test / java' 'test / java / it' 'test / java / it / acme' 'test / java / it / acme / app'
snapshot 2
ok 2 "chains are joined and never built unfolded"

SCENARIO="middle_link_status"
# `it` holds the deletion; the leaf `v2` is clean, so a leaf-status rule fails here.
wait_on files "row:$JAVA:Status=changed"
snapshot 3
ok 3 "the joined row carries the middle link's change"

SCENARIO="open_joined"
seen_reset
read_state files
click_row "$JAVA"
wait_on files "row:$JAVA:Expanded=1" "row:$JAVA:Selected=1" \
  'row:domain / model:Depth=4' 'row:domain / model:Dir=1' 'row:App.java:Depth=4'
ever_built 'domain / model'
never_built domain
snapshot 4
ok 4 "opening a joined row shows its children joined"

SCENARIO="split_and_rejoin"
touch "$REPO/source/src/main/java/it/New.java"
wait_on files 'row:java / it:Depth=3' 'row:java / it:Expanded=1' \
  'row:acme / app / v2:Depth=4' 'row:acme / app / v2:Expanded=1' 'row:acme / app / v2:Selected=1' \
  'row:New.java:Depth=4' 'after:acme / app / v2:New.java'
xkey Up
wait_on files 'row:java / it:Selected=1' 'row:java / it:Path=source/src/main/java/it' \
  'row:acme / app / v2:Selected=0'
# The split state, on screen: `java / it` above its own open `acme / app / v2` and New.java.
capture 5-split
rm "$REPO/source/src/main/java/it/New.java"
# Spec §6 #14: the selected path `it` is now a middle link of the rejoined row.
wait_on files "row:$JAVA:Expanded=1" "row:$JAVA:Selected=1" 'row:App.java:Selected=0'
snapshot 5
ok 5 "the row splits and rejoins with the disk"

SCENARIO="closed_stays_closed"
# The rejoined row is still selected: Space, a real key, closes it (every link).
xkey space
wait_on files "row:$JAVA:Expanded=0" "row:$JAVA:Selected=1" 'norow:domain / model' 'norow:App.java'
settle_refreshes 2000
read_state files
before="$(value drawnRefreshes)"
touch "$REPO/source/src/main/Tick.md"
wait_until "a frame drew a refresh after Tick.md" drawn_refreshes_over "$before"
wait_on files 'row:Tick.md:Depth=3'
# A refresh landed and was drawn: the closed row must not have reopened itself.
hold_on 2 files "row:$JAVA:Expanded=0" 'norow:domain / model' 'norow:App.java'
snapshot 6
rm "$REPO/source/src/main/Tick.md"
xkey space
wait_on files "row:$JAVA:Expanded=1" 'row:domain / model:Depth=4' 'row:App.java:Depth=4' 'norow:Tick.md'
ok 6 "a closed row stays closed through a refresh"

SCENARIO="hidden_files"
# A precache walk of this worktree can still land after check 6 and refresh once more: let the
# count settle before taking it as the baseline.
wait_on files stale=0 updating=0
settle_refreshes 2000
read_state files
before="$(value drawnRefreshes)"
touch "$REPO/source/src/main/java/it/.DS_Store"
wait_until "a frame drew a refresh after the dotfile" drawn_refreshes_over "$before"
hold_on 2 files "row:$JAVA:Depth=3" "row:$JAVA:Expanded=1" showHidden=0
read_state files
click_at_bounds "$(value hiddenSwitchBounds)"
wait_on files showHidden=1 'row:java / it:Depth=3' 'row:.DS_Store:Depth=4'
read_state files
click_at_bounds "$(value hiddenSwitchBounds)"
wait_on files showHidden=0 "row:$JAVA:Depth=3" "row:$JAVA:Expanded=1"
# Spec §3.4: a row is drawn open only when every link is. With hidden files on, close `java / it`
# while `acme / app / v2` stays open; hidden files off joins them again, and the joined row must
# draw closed, and one click must open it. A row or a click that took its state from the leaf
# alone would draw it open, or close it on that click. A refresh that starts after the close forgets
# the open links under the closed `java` and closes every link, which proves nothing, so no refresh
# may land from before the close until the row has opened again.
settle_refreshes 2000
read_state files
quiet="$(value refreshes)"
click_at_bounds "$(value hiddenSwitchBounds)"
wait_on files showHidden=1 'row:java / it:Expanded=1' 'row:acme / app / v2:Expanded=1'
click_row 'java / it'
wait_on files 'row:java / it:Expanded=0' 'norow:acme / app / v2'
read_state files
click_at_bounds "$(value hiddenSwitchBounds)"
wait_on files showHidden=0 "row:$JAVA:Depth=3" "row:$JAVA:Expanded=0" 'norow:domain / model'
capture 7-disagree
click_row "$JAVA"
wait_on files "row:$JAVA:Expanded=1" 'row:domain / model:Depth=4' 'row:App.java:Depth=4'
[ "$(value refreshes)" = "$quiet" ] \
  || fail "$SCENARIO: a refresh landed while the links disagreed, so the step proved nothing -- last read: $(last_read)"
note "no refresh landed while the links disagreed (refreshes=$quiet)"
# The other way round: close `acme / app / v2` while `java / it` stays open. The joined row must
# draw closed again, and one click must open it. A row or a click that read the first link alone
# would draw it open, or close it on that click. This way needs no guard: a refresh keeps the open
# `java / it` above the closed `acme`, so the links still disagree.
read_state files
click_at_bounds "$(value hiddenSwitchBounds)"
wait_on files showHidden=1 'row:java / it:Expanded=1' 'row:acme / app / v2:Expanded=1'
click_row 'acme / app / v2'
wait_on files 'row:acme / app / v2:Expanded=0' 'row:java / it:Expanded=1' 'norow:domain / model'
read_state files
click_at_bounds "$(value hiddenSwitchBounds)"
wait_on files showHidden=0 "row:$JAVA:Depth=3" "row:$JAVA:Expanded=0" 'norow:domain / model'
click_row "$JAVA"
wait_on files "row:$JAVA:Expanded=1" 'row:domain / model:Depth=4' 'row:App.java:Depth=4'
snapshot 7
ok 7 "hidden files decide the chain"

SCENARIO="worktree_switch"
wait_on files stale=0 updating=0
ctl select-workspace --workspace "$(worktree_id_of "$OTHER_REPO")" >/dev/null
wait_on files 'row:README:Depth=0'
# Spec 3.6: while the repository is not shown, its watcher marks its cached snapshot dirty (a
# 300 ms poll, main.rs start_files_watcher_poll -> mark_files_snapshot_dirty) and the precache
# rebuilds it with build_files_snapshot, which keeps the open paths and re-reads them. Either
# snapshot (the rebuilt one, or the dirty one it replaces) restores the open rows. No verb signals
# the precache, so the wait is a bounded pause, after which the note below logs whether the precache
# had rebuilt the snapshot (stale=0) or not (stale=1); neither outcome is a failure.
printf 'late\n' >"$REPO/source/src/main/Precache.md"
sleep 3
seen_reset
ctl select-workspace --workspace "$(worktree_id)" >/dev/null
# A hint for the artifact, not a check: stale=0 on the first read of the repository's panel
# means the switch back restored the snapshot the precache rebuilt; stale=1, the dirty one.
wait_on files 'row:source:Depth=0'
note "first read after the switch back: $(grep -E '^(stale|updating)=' <<<"$STATE" | tr '\n' ' ')"
wait_on files "row:$JAVA:Expanded=1" 'row:domain / model:Depth=4' 'row:Precache.md:Depth=3'
never_built java domain
snapshot 8
ok 8 "a worktree switch keeps the rows"
app_quit

# ---------------------------------------------------------------- teardown

SCENARIO="teardown"
collect_logs
stop_hosts
terminate "$XVFB_PID"
XVFB_PID=""
leftover=""
for _ in $(seq 1 25); do
  leftover="$(pgrep -f "$WORK" 2>/dev/null || true)"
  [ -z "$leftover" ] && break
  sleep 0.2
done
[ -z "$leftover" ] || fail "processes of this run survived teardown: $(echo "$leftover" | tr '\n' ' ')"
echo "OK: no process of this run is left"
echo "artifact: $OUT_DIR"
echo "FILES FOLD E2E OK"
