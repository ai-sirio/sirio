#!/bin/bash
set -eEuo pipefail
umask 022
# Every git this script runs reads no user or system configuration.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

# The end-to-end proof that an agent typed by hand into a Sirio terminal shows its
# mark: a real debug `sirio` on an isolated display, its real `sirioctl` and
# `sirio-host`, and a stub (Scripts/Tests/stubs/hand-agents/agent.py) that plays
# claude, codex, omp and pi. Nothing here launches a real agent CLI.
#
# Each case opens a terminal with `sirioctl surface terminal open` (no --cmd, so the
# pane is a job-control shell and the host names its foreground program), types into
# it through the world's host (`host_probe term-write`, as a user types), and reads
# the `agent` field of the pane's row in `sirioctl panel list --json`.
#
# The cases:
#   typed_agents_show_their_mark   claude, codex, omp and pi typed by name: the pane's
#                                  agent is that name within 3 s; a frame of each
#   wrapped_claude_is_claude       `node .../node_modules/@anthropic-ai/claude-code/cli.js`
#                                  is claude
#   leaving_clears_the_mark        `exit` to the codex stub: the agent is "" within 2 s
#   layer_d_is_off_for_named_programs
#                                  in a pane that has named a program, a background
#                                  `codex` (a copy of python3) is not the foreground: the
#                                  agent stays "" -- Layer D would have named it
#   omp_title_is_omp               an OSC title `π > /x` from the plain shell is omp
#
# The world is isolated: `env -i` with an allow-list (a private HOME, every XDG
# directory, SIRIO_DB, SIRIO_SOCKET, SIRIO_HOST_HOME and no session bus), its own
# OpenCode service config pointing at nothing, and a PATH that holds the stubs and
# the system bin. No file of the user's own ~/.config, ~/.claude or ~/.codex is read.
#
# --out-dir DIR keeps transcript.log, steps.log (each step's `panel list --json` and
# `surface attention read`), frames/ (PID-matched window captures) and the app and
# host logs. --only a,b runs some cases and prints `HAND AGENTS E2E PARTIAL`.
#
# Usage: Scripts/Tests/test-hand-agents-e2e.sh [--out-dir DIR] [--no-build]
#          [--xvfb | --display :N] [--only case[,case...]]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR=""
ONLY=""
NO_BUILD=0
START_XVFB=0
DISPLAY_TARGET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --no-build) NO_BUILD=1; shift ;;
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --only) ONLY=",${2:?--only needs case names}," ; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$DISPLAY_TARGET" ] || START_XVFB=1
case "$(uname -s)" in
  Linux) ;;
  *) echo "HAND AGENTS E2E runs on Linux only (its window captures use X11)" >&2; exit 2 ;;
esac

TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$REPO_ROOT/$TARGET_DIR" ;; esac
BIN_DIR="$TARGET_DIR/debug"
APP_BIN="$BIN_DIR/sirio"
CTL="$BIN_DIR/sirioctl"
HOST_BIN="$BIN_DIR/sirio-host"
PROBE="$BIN_DIR/examples/host_probe"
STUB="$REPO_ROOT/Scripts/Tests/stubs/hand-agents/agent.py"

# /tmp is a small tmpfs: the run lives on disk, and short (a socket path is limited
# to 108 bytes).
: "${TMPDIR:=$HOME/.cache/st/hand-agents-tmp}"
export TMPDIR
mkdir -p "$TMPDIR"
if [ -z "$OUT_DIR" ]; then OUT_DIR="$HOME/.cache/st/hand-agents-e2e-$(date +%Y%m%d-%H%M%S)"; fi
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
# Nothing from an earlier run may stand in for this run's evidence.
rm -rf "$OUT_DIR/frames" "$OUT_DIR"/host-*-log
rm -f "$OUT_DIR/steps.log" "$OUT_DIR"/app-*.log "$OUT_DIR/xvfb.log"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

APP_PID=""
XVFB_PID=""
RUN=""
W=""
VULKAN_ENV=()
CASE_N=0
CASE_NAME=""
CASES_RUN=0
FRAME_NO=0
START_ALL=$(date +%s)

show_evidence() {
  local logf
  for logf in "$W/app.log" "$W"/host/log/*.log; do
    [ -f "$logf" ] || continue
    echo "--- $logf"
    tail -n 25 "$logf" | cut -c1-300
  done
}
fail() {
  echo "FAIL: case $CASE_N $CASE_NAME: $*" >&2
  [ -z "$W" ] || show_evidence >&2
  exit 1
}
unexpected() {
  echo "FAIL: case $CASE_N $CASE_NAME: unexpected error at line $1 (a command the script did not expect to fail)" >&2
  [ -z "$W" ] || show_evidence >&2
  exit 1
}
trap 'unexpected $LINENO' ERR

alive() {
  kill -0 "$1" 2>/dev/null || return 1
  local stat rest
  stat="$(cat "/proc/$1/stat" 2>/dev/null)" || return 1
  rest="${stat##*) }"
  [ "${rest%% *}" != Z ]
}
is_a_host() {
  local comm
  comm="$(cat "/proc/$1/comm" 2>/dev/null || true)"
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}

cleanup() {
  local rc=$?
  set +e
  trap - ERR
  local pid
  if [ -n "$W" ] && [ -d "$W" ]; then
    SIRIO_HOST_HOME="$W/host" "$PROBE" shutdown --force >/dev/null 2>&1
  fi
  [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null
  if [ -n "$RUN" ]; then
    # By path: only a process of this run's own directory is ever signalled. A stub
    # the app started itself on a pty (it waits for typed lines) and the background
    # codex both outlive the world otherwise.
    for pid in $(pgrep -f "$RUN/(bin|bg)/" 2>/dev/null); do kill "$pid" 2>/dev/null; done
    for pid in $(pgrep -f "$RUN" 2>/dev/null); do
      is_a_host "$pid" && kill "$pid" 2>/dev/null
    done
  fi
  if [ -n "$W" ]; then
    [ -f "$W/app.log" ] && cp "$W/app.log" "$OUT_DIR/app-w.log"
    if [ -d "$W/host/log" ]; then
      mkdir -p "$OUT_DIR/host-w-log"
      cp "$W/host/log/"* "$OUT_DIR/host-w-log/" 2>/dev/null
    fi
  fi
  [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null
  [ -n "$RUN" ] && rm -rf "$RUN"
  exit "$rc"
}
trap cleanup EXIT

# wait_until SECONDS CMD...: polls CMD every 0.2 s; succeeds as soon as it does.
wait_until() {
  local n=$(( $1 * 5 ))
  shift
  while [ "$n" -gt 0 ]; do
    if "$@"; then return 0; fi
    sleep 0.2
    n=$((n - 1))
  done
  "$@"
}

# ---------------------------------------------------------------- display

if [ "$START_XVFB" -eq 1 ]; then
  command -v Xvfb >/dev/null || { echo "Xvfb is required (or pass --display :N)" >&2; exit 2; }
  for number in $(seq 90 99); do
    [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
  done
  [ -n "$DISPLAY_TARGET" ] || { echo "no free display between :90 and :99" >&2; exit 2; }
  Xvfb "$DISPLAY_TARGET" -screen 0 1280x800x24 >"$OUT_DIR/xvfb.log" 2>&1 &
  XVFB_PID=$!
  for _ in $(seq 1 50); do timeout 2 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 && break; sleep 0.1; done
  timeout 2 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 || { echo "Xvfb did not start on $DISPLAY_TARGET" >&2; exit 2; }
fi
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi
echo "display: $DISPLAY_TARGET"

# ---------------------------------------------------------------- build

if [ "$NO_BUILD" -eq 0 ]; then
  echo "building sirio, sirioctl, sirio-host and the host probe"
  (cd "$REPO_ROOT/rust" && cargo build --quiet -p sirio --bin sirio \
    && cargo build --quiet -p sirio_control --bin sirioctl \
    && cargo build --quiet -p sirio_host --bin sirio-host \
    && cargo build --quiet -p sirio_host_client --example host_probe)
fi
for binary in "$APP_BIN" "$CTL" "$HOST_BIN" "$PROBE"; do
  [ -x "$binary" ] || { echo "missing $binary: build it or drop --no-build" >&2; exit 2; }
done
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 2; }

# ---------------------------------------------------------------- the run directory

RUN="$(mktemp -d "$TMPDIR/hand-XXXXXX")"
echo "run root: $RUN"
PY3="$(python3 -I -c 'import os, sys; print(os.path.realpath(sys.executable))')"
mkdir -p "$RUN/bin" "$RUN/bg" "$RUN/lib/node_modules/@anthropic-ai/claude-code" "$RUN/shell"
# The stubs, and nothing else, ahead of the system bin: a symlink per agent, `node`
# (which is python3 here), the package script `node cli.js` runs, and a background
# `codex` that is a copy of python3 (the process Layer D would have named).
for name in claude codex omp pi opencode; do ln -s "$STUB" "$RUN/bin/$name"; done
ln -s "$PY3" "$RUN/bin/node"
cp "$STUB" "$RUN/lib/node_modules/@anthropic-ai/claude-code/cli.js"
cp "$PY3" "$RUN/bg/codex"
chmod +x "$RUN/bg/codex"
# A bash that reads no profile: the PATH it reports is the launch PATH.
cat > "$RUN/shell/shell" <<'SH'
#!/bin/sh
exec /bin/bash --noprofile --norc "$@"
SH
chmod +x "$RUN/shell/shell"
# The app, with the sirioctl beside it, as the app's own directory.
mkdir -p "$RUN/app"
ln -f "$APP_BIN" "$RUN/app/sirio" 2>/dev/null || cp "$APP_BIN" "$RUN/app/sirio"
ln -f "$HOST_BIN" "$RUN/app/sirio-host" 2>/dev/null || cp "$HOST_BIN" "$RUN/app/sirio-host"
cp "$CTL" "$RUN/app/sirioctl"

# ---------------------------------------------------------------- the world

W="$RUN/w"
W_HOME="$W/home"
W_REPO="$W/repo"
W_SOCK="$W/c.sock"
mkdir -p "$W_HOME" "$W/xdg/config/opencode" "$W/xdg/data" "$W/xdg/state" "$W/xdg/cache" "$W/xdg/runtime" "$W/host"
chmod 700 "$W/xdg/runtime"
# The world's own OpenCode service configuration, pointing at nothing: this world
# can never reach the service a person runs on port 49374.
printf '{"service":{"port":1,"host":"127.0.0.1"}}\n' > "$W/xdg/config/opencode/service.json"
export SIRIO_SOCKET="$W_SOCK" SIRIO_DB="$W/s.sqlite" SIRIO_HOST_HOME="$W/host"
mkdir -p "$W_REPO"
git -C "$W_REPO" init -q -b main
git -C "$W_REPO" config user.email tester@example.invalid
git -C "$W_REPO" config user.name Tester
echo "# app" > "$W_REPO/README.md"
git -C "$W_REPO" add -A
git -C "$W_REPO" commit -q -m first

ctl() { "$CTL" "$@"; }
ctl_ready() { "$CTL" surface tabs read >/dev/null 2>&1; }

app_start() {
  local wenv=(
    "HOME=$W_HOME" "PATH=$RUN/bin:/usr/bin:/bin" "SHELL=$RUN/shell/shell" "TMPDIR=$TMPDIR" "LANG=C.UTF-8"
    "XDG_CONFIG_HOME=$W/xdg/config" "XDG_DATA_HOME=$W/xdg/data" "XDG_STATE_HOME=$W/xdg/state"
    "XDG_CACHE_HOME=$W/xdg/cache" "XDG_RUNTIME_DIR=$W/xdg/runtime"
    "SIRIO_DB=$W/s.sqlite" "SIRIO_SOCKET=$W_SOCK" "SIRIO_HOST_HOME=$W/host"
    "SIRIO_DEBUG_WINDOW_ACTIVE=inactive"
    "DBUS_SESSION_BUS_ADDRESS=unix:path=$RUN/no-bus"
    "DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1"
  )
  wenv+=(${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"})
  rm -f "$W_SOCK"
  (cd "$W_REPO" && exec env -i "${wenv[@]}" "$RUN/app/sirio" >>"$W/app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 300); do
    [ -S "$W_SOCK" ] && break
    sleep 0.1
  done
  [ -S "$W_SOCK" ] || fail "sirio never opened its control socket"
  wait_until 60 ctl_ready || fail "sirio's control actions never became ready"
}

# ---------------------------------------------------------------- helpers

# The id of the worktree at $1, from the project list.
worktree_id_of() {
  ctl project list --json | python3 -I -c '
import json, os, sys
target = os.path.realpath(sys.argv[1])
for project in json.load(sys.stdin):
    for wt in json.loads(project["worktrees"]):
        if os.path.realpath(wt["path"]) == target:
            print(wt["id"])
            raise SystemExit
' "$1"
}

# A step's evidence: the pane list and the attention read, appended to steps.log.
step() { # label
  {
    echo "=== $1"
    ctl panel list --json 2>&1 || true
    echo
    ctl surface attention read 2>&1 || true
    echo
  } >> "$OUT_DIR/steps.log"
}

# The `agent` of the row of pane $1 in `panel list`; "__ABSENT__" when there is no row.
agent_of() {
  ctl panel list --json 2>/dev/null | python3 -I -c '
import json, sys
try:
    rows = json.load(sys.stdin)
except Exception:
    print("__ABSENT__")
    raise SystemExit
for row in rows:
    if row.get("id") == sys.argv[1]:
        print(row.get("agent", ""))
        raise SystemExit
print("__ABSENT__")' "$1"
}

# Waits until pane $1 reports agent $2 (possibly empty), for $3 seconds.
wait_agent() { # pane want seconds
  local got=""
  for _ in $(seq 1 $(( $3 * 5 ))); do
    got="$(agent_of "$1")"
    [ "$got" = "$2" ] && return 0
    sleep 0.2
  done
  step "FAILED: $1 wanted agent '$2', last '$got'"
  fail "pane $1: the agent never read '$2' within $3 s (last: '$got')"
}
# Holds for $3 seconds that pane $1's agent stays $2.
hold_agent() { # pane want seconds
  local got=""
  for _ in $(seq 1 $(( $3 * 5 ))); do
    got="$(agent_of "$1")"
    [ "$got" = "$2" ] || fail "pane $1: the agent read '$got' while it had to stay '$2'"
    sleep 0.2
  done
}

# Opens a terminal (no --cmd: a job-control shell) and prints its pane id.
open_pane() {
  local before after pane
  before="$(ctl panel list --json 2>/dev/null | python3 -I -c 'import json, sys; print(" ".join(r["id"] for r in json.load(sys.stdin)))')"
  ctl surface terminal open >/dev/null
  for _ in $(seq 1 100); do
    after="$(ctl panel list --json 2>/dev/null | python3 -I -c 'import json, sys; print(" ".join(r["id"] for r in json.load(sys.stdin)))')"
    pane="$(python3 -I -c 'import sys
old = set(sys.argv[1].split())
new = [p for p in sys.argv[2].split() if p not in old]
print(new[0] if new else "")' "$before" "$after")"
    [ -z "$pane" ] || { printf '%s' "$pane"; return 0; }
    sleep 0.2
  done
  fail "no new pane appeared after surface terminal open"
}

# Types $2 and Enter into pane $1, through the world's host, as a user types.
type_line() { # pane text
  "$PROBE" term-write "$1" "$2"$'\r' >/dev/null
}

# A capture of the app's own window, found by its _NET_WM_PID. The frame must not be blank.
capture() { # name
  FRAME_NO=$((FRAME_NO + 1))
  local name window colours
  name=$(printf '%02d-%s' "$FRAME_NO" "$1")
  sleep 2
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
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $name"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$name.png"
  colours=$(identify -format '%k' "$OUT_DIR/frames/$name.png")
  [ "$colours" -ge 200 ] || fail "$name is blank ($colours colours)"
  echo "FRAME: frames/$name.png ($colours colours)"
}

# ---------------------------------------------------------------- the cases

# A pane that has named a program and gone back to the shell: what the Layer D case needs.
prime_named_pane() { # pane
  type_line "$1" codex
  wait_agent "$1" codex 3
  type_line "$1" exit
  wait_agent "$1" "" 3
}

case_typed_agents_show_their_mark() {
  local name pane
  for name in claude codex omp pi; do
    pane="$(open_pane)"
    echo "  $name: pane $pane"
    type_line "$pane" "$name"
    wait_agent "$pane" "$name" 3
    step "typed $name in $pane"
    capture "typed-$name"
  done
}

case_wrapped_claude_is_claude() {
  local pane
  pane="$(open_pane)"
  # --no-title: the stub sets no `✳ Claude Code` title, so only the program names it.
  type_line "$pane" "node $RUN/lib/node_modules/@anthropic-ai/claude-code/cli.js --no-title"
  wait_agent "$pane" claude 3
  step "wrapped claude in $pane"
}

case_leaving_clears_the_mark() {
  local pane
  pane="$(open_pane)"
  type_line "$pane" codex
  wait_agent "$pane" codex 3
  type_line "$pane" exit
  wait_agent "$pane" "" 2
  step "left codex in $pane"
}

case_layer_d_is_off_for_named_programs() {
  local pane pids pid
  pane="$(open_pane)"
  prime_named_pane "$pane"
  type_line "$pane" "$RUN/bg/codex -c 'import time; time.sleep(600)' &"
  # The process exists, and its comm is codex: what Layer D reads.
  for _ in $(seq 1 50); do
    pids="$(pgrep -f "$RUN/bg/codex -c" 2>/dev/null || true)"
    [ -z "$pids" ] || break
    sleep 0.2
  done
  [ -n "$pids" ] || fail "the background codex never started"
  for pid in $pids; do
    [ "$(cat "/proc/$pid/comm" 2>/dev/null)" = codex ] && break
    pid=""
  done
  [ -n "${pid:-}" ] || fail "no process of this run has the comm codex (pids: $pids)"
  # Layer D polls every 500 ms: 2 s is four ticks. The foreground is the shell.
  sleep 2
  hold_agent "$pane" "" 2
  step "background codex under $pane, foreground the shell"
  capture "layer-d-off"
}

case_omp_title_is_omp() {
  local pane
  pane="$(open_pane)"
  type_line "$pane" "printf '\\033]2;π > /x\\007'"
  wait_agent "$pane" omp 3
  step "omp title in $pane"
}

run_case() { # name
  CASE_N=$((CASE_N + 1))
  CASE_NAME="$1"
  if [ -n "$ONLY" ]; then
    case "$ONLY" in *",$1,"*) ;; *) echo "case $CASE_N $1: skipped (--only)"; return 0 ;; esac
  fi
  echo "case $CASE_N: $1"
  "case_$1"
  CASES_RUN=$((CASES_RUN + 1))
  echo "OK: $1"
}

app_start
wt="$(worktree_id_of "$W_REPO" || true)"
if [ -z "$wt" ]; then
  ctl project add "$W_REPO" >/dev/null 2>&1 || true
  for _ in $(seq 1 50); do wt="$(worktree_id_of "$W_REPO" || true)"; [ -z "$wt" ] || break; sleep 0.2; done
fi
[ -n "$wt" ] || fail "sirioctl does not list the worktree $W_REPO"
ctl select-workspace --workspace "$wt" >/dev/null
sleep 2
step "start"

run_case typed_agents_show_their_mark
run_case wrapped_claude_is_claude
run_case leaving_clears_the_mark
run_case layer_d_is_off_for_named_programs
run_case omp_title_is_omp

[ "$CASES_RUN" -gt 0 ] || { echo "FAIL: --only named no case" >&2; exit 1; }
if [ -n "$ONLY" ]; then label=PARTIAL; else label=OK; fi
echo "HAND AGENTS E2E $label ($CASES_RUN cases, $(( $(date +%s) - START_ALL ))s)"
