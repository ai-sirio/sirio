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
# and, from what the agents report over OSC 7501, the status the tab shows and the
# stops it makes (each step takes its own `posted_count` baseline and logs the pane's
# attention row in steps.log):
#   startup_is_silent              claude, codex and omp typed: no status, nothing posted
#   claude_states                  work -> running; idle after >= 1 s -> done (+1, unread);
#                                  block -> needs-input (+0: done -> needs-input is no stop)
#   first_report_blocked           a first `block` is needs-input and a stop (+1)
#   turn_end_notifies_once         codex work, 1.3 s, done: done, +1; its idle right after keeps done
#   blocked_inside_turn_then_idle  work, block, idle: needs-input +1, then done +0
#   short_turn_is_silent           work, idle 0.2 s later: no status, +0, running false
#   exit_mid_turn_is_finished      work, 1.3 s, exit: a stop (+1), then no agent, no status
#   clear_mid_turn_is_silent       claude work, 1.3 s, clear: no status, +0
#   hooks_yield_to_reports         a hook after an idle changes nothing
#   pi_reports_once_answered       pi, as Pi 1.4.0 does, reports once the probe is answered: work ->
#                                  running; idle after >= 1 s -> done (+1, unread)
#   hooks_still_rule_without_reports
#                                  `pi --no-reports` sends nothing: its hooks set the status (and
#                                  prove a hook from a pane reaches the app)
#   spawned_agent_reports          `surface terminal open --agent claude`: reports show, the
#                                  Stop hook that follows changes nothing
#   relaunch_mid_turn              codex mid-turn, `system.quit`, restart: the startup posts nothing,
#                                  the turn is still open and its idle is a stop (+1); an idle tab
#                                  stays silent
#   agent_hands_the_terminal_to_a_child
#                                  an agent stub gives the terminal to a child (`fgchild`): claude, with
#                                  a live record, keeps its mark; `pi --no-reports`, with no record, loses it
#                                  while the child is the foreground and has it back after
#   argv_stays_out_of_app_stderr   `claude SIRIO-ARGV-MARKER`: the marker is not in the app's stderr
#
# and OpenCode, which reports nothing in the terminal: its status is read from the service
# it talks to, played by Scripts/Tests/stubs/opencode-server.py (the same endpoints, auth
# and events as the real v2 service; driven by lines in $RUN/oc.cmd; registered in the
# world's own $XDG_STATE_HOME/opencode/service.json):
#   opencode_three_states          a session that exists before the pane starts, linked by its
#                                  title: busy -> running; a permission ask -> needs-input (+1);
#                                  replied -> running; idle after >= 1 s -> done (+1); a typed
#                                  `opencode` never gets a session ref, however it is linked
#   opencode_first_turn_links      one OpenCode pane and a session born after it: linked with no
#                                  title, busy -> running, idle -> done (+1); the TUI's title
#                                  before the service's rename keeps Done and adds no stop; no
#                                  session ref is written, before or after the title
#   opencode_duplicate_title_keeps_link
#                                  an older session of the folder has the title the service later
#                                  gives the pane's first-turn session, the TUI's title coming
#                                  first: the link stays, and the next turn shows running, then
#                                  done (+1); no session ref is written
#   opencode_short_turn_is_finished
#                                  a turn the service ends under a second after it began: done, +1
#   opencode_resumed_finish_is_not_replayed
#                                  a turn ends, the TUI quits, a new `opencode` in the tab links to
#                                  that session by title: no status; its next turn then shows
#                                  and posts
#   opencode_sibling_finish_is_not_replayed
#                                  two TUIs in one folder, B runs a turn and quits: A, now alone,
#                                  links to B's finished session by the first-turn rule and shows
#                                  no status, no unread mark and posts nothing; its next turn shows
#                                  on A and posts once
#   opencode_own_finish_links_late
#                                  two TUIs in one folder, A's first turn ends before its title and
#                                  the rename arrive: the title link forms at Done, and A shows
#                                  finished and posts once
#   two_opencode_panes_one_folder  two panes in one folder, one then two sessions: no status on
#                                  either; with titles `OC | a`, `OC | b`: each follows its own
#                                  and no session ref is written for either
#   opencode_failed_turn_is_error  a failed turn -> error; the service's error text and its
#                                  password stay out of the app's stderr
#   opencode_after_relaunch        a titled session, the app quits and starts again: the pane's
#                                  title comes from the host's seed, so busy -> running and idle
#                                  after >= 1 s -> done (+1); still no session ref
#   opencode_agent_tab_resumes     two tabs from `surface terminal open --agent opencode` in one
#                                  folder (OpenCode 2): each tab's own session is created by Sirio
#                                  (the stub's `POST /api/session 200 <id> <worktree>`), the pane's
#                                  ref is that id before any title exists, the tab's process runs
#                                  `opencode --session <id>`, and `system.identify` returns it; the
#                                  first turn shows its status with no title (the pane's created
#                                  session is linked, +1 at the end), and a session opened inside
#                                  the TUI (`create ses_tui`) takes the link away; a relaunch with
#                                  the host alive keeps the refs and the hidden tab linked
#   opencode_unavailable_service_launches_plain
#                                  `create fail` (POST answered 500), `create delay 8` (the tab
#                                  starts plain at the 3 s deadline while the control socket still
#                                  answers in under a second; the late reply writes no ref) and an
#                                  OpenCode 1 (`oc-version` 1.18.33: no POST at all, the v1 plugin
#                                  is written, reading its pane from the environment): plain
#                                  `opencode`, no ref
#   opencode_v2_writes_no_plugin   OpenCode 2 and a worktree plugin: the real v1 plugin an
#                                  OpenCode 1 tab made Sirio write (`make_real_v1_plugin`) and
#                                  the legacy one that names a pane (what older builds wrote)
#                                  are removed at the next fresh tab; a copy of it whose sirioctl
#                                  line is no longer one JSON string literal, a user's file with
#                                  no Sirio header, and one with Sirio's header line over the
#                                  user's own body, stay byte for byte
#   opencode_unknown_version_writes_no_plugin
#                                  `oc-version` holds `garbage` (no readable version) and the app
#                                  is relaunched: an OpenCode tab runs a plain `opencode`, no POST
#                                  /api/session, no ref, no plugin written and the real v1
#                                  plugin an earlier OpenCode 1 tab wrote, and the legacy one,
#                                  removed; then 2.3.0 is back and the app relaunched
#   fresh_tab_drops_stale_ref      a row left under the key the next fresh tab takes (a crash, an
#                                  older build, a closed pane) is dropped as that tab is made: a
#                                  codex tab opened after a relaunch has no ref
#   no_service_degrades            the service stops: the mark stays, no status, and the app's
#                                  stderr names the failure kind once and nothing else
#
# and the tabs no terminal holds: after a relaunch only the selected tab has a terminal, and a
# worktree never selected since has none; the app's listener on the host's `term.sessions`
# topic reads those tabs' programs and reports (docs/agents/architecture/activity-detection.md):
#   background_tab_notifies        two codex tabs, a relaunch: B, never shown since, runs work, 1.3 s,
#                                  done: done, +1, unread
#   showing_the_tab_adds_no_stop   continued: showing B adds no stop and clears its mark; a full turn
#                                  in B while it is shown makes exactly one
#   background_finish_at_relaunch_is_silent
#                                  a claude tab whose turn ended and was read: after a relaunch, hidden,
#                                  nothing is posted and its mark is unchanged
#   background_exit_mid_turn_is_one_stop
#                                  hidden codex tab: work, 1.3 s, exit: +1, the agent and status clear
#   background_opencode_notifies   a hidden OpenCode tab after a relaunch: busy -> running, idle -> done (+1)
#   control_session_gets_no_state  a `panel create` session that belongs to no tab: codex typed, a turn:
#                                  nothing posted and no attention row
#   other_worktree_notifies        a codex tab of a worktree never selected since the relaunch: a turn
#                                  makes done, +1, unread
#   background_agent_tab_ends_mid_turn / _fails_mid_turn / background_exec_agent_ends_mid_turn
#                                  a hidden Sirio-launched agent tab (exit 0, exit 3) or a hand-typed
#                                  `exec codex`: work, 1.3 s, the stub exits so the session ends: +1,
#                                  unread, not running; showing the tab adds none
#   show_session_of_hidden_tab_is_one_stop
#                                  `panel show` of a hidden tab's session from another worktree opens a
#                                  Show-session tab; a turn begun before it and ended while it is open is
#                                  one stop, the hidden tab's own (its mark, its notification, whose body
#                                  names its branch), and after the Show tab closes the hidden tab still
#                                  notifies
#   show_session_on_screen_is_silent
#                                  the same Show tab selected, the window active: the turn is watched, so
#                                  nothing is posted and the hidden tab is not unread
#   restart_agent_tab_mid_turn_is_silent / restart_hand_typed_agent_tab_mid_turn_is_silent
#                                  `surface terminal restart` of a shown agent tab (Sirio-launched, or
#                                  typed) mid-turn: nothing posted, no status, not running, no agent on a
#                                  typed one; the restarted launched tab's next full turn is one stop
#   one_end_per_session            a hidden agent tab exits, the host publishes a title after the exit and
#                                  then closes the session: one stop, not a second end
#   close_hidden_worktree_mid_turn_is_silent
#                                  closing a worktree nobody selected since the relaunch, its agent
#                                  mid-turn: nothing posted, nothing left running
#   background_exit_while_app_down_is_silent
#                                  a session that exited while the app was down posts nothing at the
#                                  relaunch, hidden or attached
#   background_host_lost           the world's host killed under an attached tab and a hidden one, both
#                                  mid-turn: one stop each, then a turn on the fresh host is one stop
#   end_all_sessions_parity        `host.shutdown {force}` (End all sessions) under both: one stop each,
#                                  the same notification; it shuts the host down
#   opencode_resumes_without_host  last, because it stops the host itself: an OpenCode tab with its
#                                  created session, the app quits and the host goes, the relaunch
#                                  starts `opencode --session <id>` only after Sirio's v1 plugin is
#                                  removed, and a hidden second tab only once it is shown; the
#                                  world is left with the app and a host running
#
# The world is isolated: `env -i` with an allow-list (a private HOME, every XDG
# directory, SIRIO_DB, SIRIO_SOCKET, SIRIO_HOST_HOME and no session bus), its own
# OpenCode service config pointing at nothing, and a PATH that holds the stubs and
# the system bin. No file of the user's own ~/.config, ~/.claude or ~/.codex is read, and
# the OpenCode service it finds is the stub's: never the one a person runs on port 49374.
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
OC_STUB="$REPO_ROOT/Scripts/Tests/stubs/opencode-server.py"

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
OC_PID=""
OC_SENT=0
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
  [ -n "$OC_PID" ] && kill "$OC_PID" 2>/dev/null
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
# The OpenCode service of this world: a stub that registers itself where OpenCode's own
# service does (`$XDG_STATE_HOME/opencode/service.json`, mode 600) and takes its commands
# from $RUN/oc.cmd. It is up before the app starts.
: > "$RUN/oc.cmd"
python3 -I "$OC_STUB" --registry "$W/xdg/state/opencode/service.json" --cmd "$RUN/oc.cmd" \
  --ack "$RUN/oc.ack" --log "$RUN/oc.log" >"$RUN/oc-stub.out" 2>&1 &
OC_PID=$!
for _ in $(seq 1 100); do
  [ -s "$W/xdg/state/opencode/service.json" ] && break
  sleep 0.1
done
[ -s "$W/xdg/state/opencode/service.json" ] || { echo "the OpenCode stub never registered" >&2; exit 2; }
mkdir -p "$W_REPO"
git -C "$W_REPO" init -q -b main
git -C "$W_REPO" config user.email tester@example.invalid
git -C "$W_REPO" config user.name Tester
echo "# app" > "$W_REPO/README.md"
git -C "$W_REPO" add -A
git -C "$W_REPO" commit -q -m first
# Two more worktrees, for the cases about tabs of a worktree nobody selected since the
# relaunch (repo-c is the one a case closes).
git -C "$W_REPO" worktree add -q -b wt-b "$W/repo-b"
git -C "$W_REPO" worktree add -q -b wt-c "$W/repo-c"

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

# `surface attention read`'s sessions, one JSON object per line (the attention rows
# are keyed by the tab's persistence id, not by the pane id).
attention_rows() {
  ctl surface attention read --json | python3 -I -c 'import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
for row in json.loads(reply["sessions"]):
    print(json.dumps(row))'
}
# One field of one session row: strings as they are, everything else as JSON.
attention_get() { # id field
  attention_rows | python3 -I -c 'import json, sys
for line in sys.stdin:
    row = json.loads(line)
    if row["id"] == sys.argv[1]:
        value = row[sys.argv[2]]
        print(value if isinstance(value, str) else json.dumps(value))
        break' "$1" "$2"
}
# The notifications the app decided to post, one JSON object per line, oldest first
# (`posted` is kept by a debug build only).
posted_rows() {
  ctl surface attention read --json | python3 -I -c 'import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
if "posted" not in reply:
    sys.exit("the attention read has no posted key")
for row in json.loads(reply["posted"]):
    print(json.dumps(row))'
}
# Every assertion about a step compares against the count taken just before the step.
posted_count() { posted_rows | wc -l | tr -d ' '; }
posted_since() { posted_rows | tail -n +"$(( $1 + 1 ))"; }

# Opens a terminal (`surface terminal open` plus any arguments; no --cmd by default: a
# job-control shell) and sets PANE to its pane id and SESS to its attention session.
# Not called in a subshell: it sets globals.
PANE=""
SESS=""
open_pane() {
  local before after known
  before="$(ctl panel list --json 2>/dev/null | python3 -I -c 'import json, sys; print(" ".join(r["id"] for r in json.load(sys.stdin)))')"
  known="$(attention_rows | python3 -I -c 'import json, sys
print(" ".join(json.loads(line)["id"] for line in sys.stdin))')"
  ctl surface terminal open "$@" >/dev/null
  PANE=""
  for _ in $(seq 1 100); do
    after="$(ctl panel list --json 2>/dev/null | python3 -I -c 'import json, sys; print(" ".join(r["id"] for r in json.load(sys.stdin)))')"
    PANE="$(python3 -I -c 'import sys
old = set(sys.argv[1].split())
new = [p for p in sys.argv[2].split() if p not in old]
print(new[0] if new else "")' "$before" "$after")"
    [ -z "$PANE" ] || break
    sleep 0.2
  done
  [ -n "$PANE" ] || fail "no new pane appeared after surface terminal open"
  SESS=""
  for _ in $(seq 1 100); do
    SESS="$(attention_rows | python3 -I -c 'import json, os, sys
known = set(sys.argv[2].split())
for line in sys.stdin:
    row = json.loads(line)
    if row["kind"] == "terminal" and row["id"] not in known \
            and os.path.realpath(row["worktree"]) == os.path.realpath(sys.argv[1]):
        print(row["id"])
        break' "${OPEN_DIR:-$W_REPO}" "$known")"
    [ -z "$SESS" ] || break
    sleep 0.2
  done
  [ -n "$SESS" ] || fail "no attention session appeared for pane $PANE"
}

# The 1-based position of pane $1 in the tab strip: one pane per tab, tabs in the order
# the panes were opened (`panel list` sorts its ids as text, so pane-10 precedes pane-2, and
# lists a control session, which is no tab, under an id of its own).
tab_index_of() { # pane
  ctl panel list --json | python3 -I -c 'import json, re, sys
ids = sorted((row["id"] for row in json.load(sys.stdin) if re.fullmatch(r"pane-[0-9]+", row["id"])), key=lambda i: int(i.rsplit("-", 1)[1]))
print(ids.index(sys.argv[1]) + 1)' "$1"
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

# A step's evidence for one session: its status, unread mark and running flag, and the
# posted count, then the pane list and the attention read (`step`).
stepn() { # label session
  {
    echo "=== $1 -- session $2: status=$(attention_get "$2" status) unread=$(attention_get "$2" unread) running=$(attention_get "$2" running) posted=$(posted_count) tags=$(posted_rows | python3 -I -c 'import json, sys; print(",".join(json.loads(l)["tag"][-6:] for l in sys.stdin))')"
  } >> "$OUT_DIR/steps.log"
  step "$1"
}

# Waits until session $1's status reads $2 (null, running, needs-input, done, error) within $3 s.
wait_status() { # session want seconds
  local got=""
  for _ in $(seq 1 $(( $3 * 5 ))); do
    got="$(attention_get "$1" status)"
    [ "$got" = "$2" ] && return 0
    sleep 0.2
  done
  step "FAILED: session $1 wanted status '$2', last '$got'"
  fail "session $1: the status never read '$2' within $3 s (last: '$got')"
}
# Holds for $3 s that session $1's status stays $2.
hold_status() { # session want seconds
  local got=""
  for _ in $(seq 1 $(( $3 * 5 ))); do
    got="$(attention_get "$1" status)"
    [ "$got" = "$2" ] || { step "FAILED: session $1 read '$got' while it had to stay '$2'"; fail "session $1: the status read '$got' while it had to stay '$2'"; }
    sleep 0.2
  done
}
# After a settling second, the app has posted exactly $2 notifications since baseline $1.
expect_posted() { # baseline want what
  local got
  sleep 1.5
  got="$(( $(posted_count) - $1 ))"
  [ "$got" = "$2" ] || fail "$3: the app posted $got notification(s), wanted $2; posted since: $(posted_since "$1" | tr '\n' ' ')"
}
expect_attention() { # session field want what
  local got
  got="$(attention_get "$1" "$2")"
  [ "$got" = "$3" ] || fail "$4: $2 reads '$got', wanted '$3'"
}

# A terminal with agent $1 typed and its mark shown (sets PANE and SESS), then 2.5 s:
# past the 2 s the app keeps a startup title silent, and the probe's answer is in.
start_agent() { # name [arguments]
  open_pane
  type_line "$PANE" "$1${2:+ $2}"
  wait_agent "$PANE" "$1" 3
  sleep 2.5
}
# A turn of at least a second: work, then 1.3 s.
long_work() { # pane session
  type_line "$1" work
  wait_status "$2" running 3
  sleep 1.3
}

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
    open_pane; pane="$PANE"
    echo "  $name: pane $pane"
    type_line "$pane" "$name"
    wait_agent "$pane" "$name" 3
    step "typed $name in $pane"
    capture "typed-$name"
  done
}

case_wrapped_claude_is_claude() {
  local pane
  open_pane; pane="$PANE"
  # --no-title: the stub sets no `✳ Claude Code` title, so only the program names it.
  type_line "$pane" "node $RUN/lib/node_modules/@anthropic-ai/claude-code/cli.js --no-title"
  wait_agent "$pane" claude 3
  step "wrapped claude in $pane"
}

case_leaving_clears_the_mark() {
  local pane
  open_pane; pane="$PANE"
  type_line "$pane" codex
  wait_agent "$pane" codex 3
  type_line "$pane" exit
  wait_agent "$pane" "" 2
  step "left codex in $pane"
}

case_layer_d_is_off_for_named_programs() {
  local pane pids pid
  open_pane; pane="$PANE"
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
  open_pane; pane="$PANE"
  type_line "$pane" "printf '\\033]2;π > /x\\007'"
  wait_agent "$pane" omp 3
  step "omp title in $pane"
}

case_startup_is_silent() {
  local name base
  for name in claude codex omp; do
    base="$(posted_count)"
    open_pane
    type_line "$PANE" "$name"
    wait_agent "$PANE" "$name" 3
    sleep 3
    stepn "startup $name in $PANE" "$SESS"
    expect_attention "$SESS" status null "$name at startup"
    expect_attention "$SESS" running false "$name at startup"
    expect_attention "$SESS" unread false "$name at startup"
    expect_posted "$base" 0 "$name at startup"
  done
}

case_claude_states() {
  local base
  start_agent claude
  long_work "$PANE" "$SESS"
  stepn "claude working" "$SESS"
  capture "claude-running"
  base="$(posted_count)"
  type_line "$PANE" idle
  wait_status "$SESS" done 3
  expect_posted "$base" 1 "claude's idle after a turn"
  expect_attention "$SESS" unread true "claude's idle after a turn"
  stepn "claude idle after a turn" "$SESS"
  capture "claude-done"
  base="$(posted_count)"
  type_line "$PANE" block
  wait_status "$SESS" needs-input 3
  expect_posted "$base" 0 "done -> needs-input is no stop"
  stepn "claude blocked after done" "$SESS"
  capture "claude-needs-input"
}

case_first_report_blocked() {
  local base
  start_agent claude
  base="$(posted_count)"
  type_line "$PANE" block
  wait_status "$SESS" needs-input 3
  expect_posted "$base" 1 "a first blocked"
  expect_attention "$SESS" unread true "a first blocked"
  stepn "first report blocked" "$SESS"
}

case_turn_end_notifies_once() {
  local base
  start_agent codex
  long_work "$PANE" "$SESS"
  base="$(posted_count)"
  type_line "$PANE" done
  wait_status "$SESS" done 3
  expect_posted "$base" 1 "codex's done"
  expect_attention "$SESS" unread true "codex's done"
  # The stub sends the idle Codex 0.166 sends right after every done: the finish stays.
  hold_status "$SESS" done 2
  expect_posted "$base" 1 "the idle after codex's done"
  stepn "codex done, then its idle" "$SESS"
}

case_blocked_inside_turn_then_idle() {
  local base
  start_agent codex
  type_line "$PANE" work
  wait_status "$SESS" running 3
  base="$(posted_count)"
  type_line "$PANE" block
  wait_status "$SESS" needs-input 3
  expect_posted "$base" 1 "a block inside a turn"
  stepn "blocked inside a turn" "$SESS"
  base="$(posted_count)"
  type_line "$PANE" idle
  wait_status "$SESS" done 3
  expect_posted "$base" 0 "the idle after a block (needs-input -> done)"
  stepn "idle after a block" "$SESS"
}

# Waits until pane $1's screen shows $2, for $3 seconds.
wait_screen() { # pane text seconds
  for _ in $(seq 1 $(( $3 * 5 ))); do
    "$PROBE" term-read "$1" text 2>/dev/null | grep -qF "$2" && return 0
    sleep 0.2
  done
  fail "pane $1: the screen never showed '$2' within $3 s"
}

case_short_turn_is_silent() {
  local base
  start_agent codex
  base="$(posted_count)"
  # Nothing between the two lines may take long: the turn is measured on the host's clock
  # between the two reports, and a poll of the app in between would make it a long one.
  type_line "$PANE" work
  sleep 0.2
  type_line "$PANE" idle
  wait_screen "$PANE" "ack idle" 5
  sleep 1
  expect_attention "$SESS" status null "a short turn"
  hold_status "$SESS" null 1
  expect_attention "$SESS" running false "a short turn"
  expect_posted "$base" 0 "a short turn"
  stepn "short turn" "$SESS"
}

case_exit_mid_turn_is_finished() {
  local base
  start_agent codex
  long_work "$PANE" "$SESS"
  base="$(posted_count)"
  type_line "$PANE" exit
  wait_agent "$PANE" "" 3
  wait_status "$SESS" null 3
  expect_posted "$base" 1 "an exit mid-turn"
  expect_attention "$SESS" unread true "an exit mid-turn"
  expect_attention "$SESS" running false "an exit mid-turn"
  stepn "exit mid-turn" "$SESS"
}

case_clear_mid_turn_is_silent() {
  local base
  start_agent claude
  # A status under the record: past the startup grace a title is the layer below it, and
  # the clear must take it away with the record, or it would come back as the status.
  type_line "$PANE" "title . thinking"
  wait_status "$SESS" running 3
  long_work "$PANE" "$SESS"
  base="$(posted_count)"
  type_line "$PANE" clear
  wait_status "$SESS" null 3
  hold_status "$SESS" null 1
  expect_attention "$SESS" running false "a clear mid-turn"
  expect_posted "$base" 0 "a clear mid-turn"
  stepn "clear mid-turn" "$SESS"
}

case_hooks_yield_to_reports() {
  local base
  start_agent claude
  long_work "$PANE" "$SESS"
  type_line "$PANE" idle
  wait_status "$SESS" done 3
  sleep 1
  base="$(posted_count)"
  type_line "$PANE" "hook needs-input"
  hold_status "$SESS" done 2
  expect_posted "$base" 0 "a hook while a record is live"
  stepn "hook after a report" "$SESS"
}

case_pi_reports_once_answered() {
  local base
  start_agent pi
  long_work "$PANE" "$SESS"
  stepn "pi working" "$SESS"
  base="$(posted_count)"
  type_line "$PANE" idle
  wait_status "$SESS" done 3
  expect_posted "$base" 1 "pi's idle after a turn"
  expect_attention "$SESS" unread true "pi's idle after a turn"
  stepn "pi idle after a turn" "$SESS"
}

case_hooks_still_rule_without_reports() {
  local base
  # `--no-reports`: an agent that sends no OSC 7501 at all (here Pi, with no probe sent either).
  start_agent pi --no-reports
  base="$(posted_count)"
  type_line "$PANE" "hook needs-input"
  wait_status "$SESS" needs-input 3
  expect_posted "$base" 1 "a silent agent's first hook"
  stepn "silent agent: hook needs-input" "$SESS"
  base="$(posted_count)"
  type_line "$PANE" "hook running"
  wait_status "$SESS" running 3
  type_line "$PANE" "hook needs-input"
  wait_status "$SESS" needs-input 3
  expect_posted "$base" 1 "a silent agent's running then needs-input"
  stepn "silent agent: hook running then needs-input" "$SESS"
}

case_spawned_agent_reports() {
  local base
  # The stub sets no title when a `<argv0>.no-title` file sits beside it: the pane's agent
  # can then come from the report's `app=` alone, which is what a Sirio-spawned tab needs.
  : > "$RUN/bin/claude.no-title"
  open_pane --agent claude
  sleep 3
  rm -f "$RUN/bin/claude.no-title"
  stepn "spawned claude before its first report" "$SESS"
  # Nothing has told the app what this tab runs or does: the report is the only evidence.
  expect_attention "$SESS" status null "a spawned claude before it reports"
  type_line "$PANE" work
  wait_status "$SESS" running 5
  sleep 1.3
  stepn "spawned claude working" "$SESS"
  base="$(posted_count)"
  type_line "$PANE" idle
  wait_status "$SESS" done 3
  expect_posted "$base" 1 "a spawned claude's idle"
  expect_attention "$SESS" unread true "a spawned claude's idle"
  base="$(posted_count)"
  # The Stop hook, which fires when a turn ends: the report already said finished.
  type_line "$PANE" "hook needs-input"
  hold_status "$SESS" done 2
  expect_posted "$base" 0 "the Stop hook after the report"
  stepn "spawned claude: Stop hook after idle" "$SESS"
}

case_relaunch_mid_turn() {
  local pane_a sess_a pane_b sess_b base
  start_agent codex
  pane_b="$PANE"; sess_b="$SESS"
  type_line "$pane_b" idle
  start_agent codex
  pane_a="$PANE"; sess_a="$SESS"
  type_line "$pane_a" work
  wait_status "$sess_a" running 3
  sleep 1.5
  stepn "before the quit: A mid-turn, B idle" "$sess_a"
  ctl quit >/dev/null 2>&1 || true
  for _ in $(seq 1 100); do alive "$APP_PID" || break; sleep 0.2; done
  alive "$APP_PID" && fail "sirio did not quit after sirioctl quit"
  APP_PID=""
  app_start
  ctl select-workspace --workspace "$wt" >/dev/null
  sleep 2
  # Only the selected tab (A, the last opened) is mounted: its terminal attaches to the
  # host's live session, and the host's record seeds the open turn.
  wait_agent "$pane_a" codex 15
  wait_status "$sess_a" running 10
  stepn "after the relaunch: A mid-turn" "$sess_a"
  # The new app starts with an empty `posted` list: whatever it holds now, its seed made.
  expect_posted 0 0 "the relaunch's own startup (no report was typed)"
  # A seed that settled a restored tab would mark it unread without posting (a restored first
  # decision): tab A never ran to an end, so it may not read unread.
  expect_attention "$sess_a" unread false "tab A, seeded mid-turn by the relaunch"
  # Tab B is selected: it attaches and its host record (idle) seeds no status.
  ctl tab select "$(tab_index_of "$pane_b")" >/dev/null
  wait_agent "$pane_b" codex 15
  sleep 1
  expect_attention "$sess_b" status null "tab B selected after the relaunch"
  stepn "after the relaunch: B selected, at idle" "$sess_b"
  ctl tab select "$(tab_index_of "$pane_a")" >/dev/null
  sleep 1
  wait_status "$sess_a" running 5
  base="$(posted_count)"
  type_line "$pane_a" idle
  wait_status "$sess_a" done 5
  expect_posted "$base" 1 "the idle of a turn the relaunch found open"
  expect_attention "$sess_a" unread true "the idle after the relaunch"
  stepn "after the relaunch: A's idle" "$sess_a"
  expect_attention "$sess_b" status null "tab B after A's idle"
}

# The pid of the `sleep 6.123` an agent stub handed the terminal to, once it is the
# terminal's foreground group (its process group is the terminal's: fields 3 and 6 of
# /proc/<pid>/stat after the command name). Sets CHILD. A pass that never saw the child
# in the foreground would prove nothing.
wait_foreground_child() {
  local pid stat rest
  CHILD=""
  for _ in $(seq 1 50); do
    for pid in $(pgrep -f 'sleep 6.123' 2>/dev/null || true); do
      stat="$(cat "/proc/$pid/stat" 2>/dev/null || true)"
      [ -n "$stat" ] || continue
      rest="${stat##*) }"
      set -- $rest
      if [ "$3" = "$6" ] && [ "$6" != 0 ] && [ "$6" != -1 ]; then CHILD="$pid"; return 0; fi
    done
    sleep 0.2
  done
  fail "the stub's child never became the terminal's foreground group"
}
child_alive() { kill -0 "$CHILD" 2>/dev/null; }
child_gone() { ! child_alive; }

case_agent_hands_the_terminal_to_a_child() {
  # A reporting agent keeps its mark and its turn while a child holds the terminal and
  # after the child gives it back (Refinement 6): the terminal returning to the live agent
  # is not a replacement, so there is no finished stop. One that never reports loses the
  # mark for as long, and has it back when it takes the terminal.
  local base
  open_pane
  type_line "$PANE" claude
  wait_agent "$PANE" claude 3
  sleep 2.5
  type_line "$PANE" work
  wait_status "$SESS" running 3
  base="$(posted_count)"
  type_line "$PANE" fgchild
  wait_foreground_child
  sleep 1.5
  child_alive || fail "the child is gone before the mark could be checked"
  [ "$(agent_of "$PANE")" = claude ] || fail "claude lost its mark while its child held the terminal"
  expect_attention "$SESS" status running "claude with a live record, a child in the foreground"
  stepn "claude: child holds the terminal" "$SESS"
  wait_until 10 child_gone || fail "the child never ended"
  sleep 1.5
  [ "$(agent_of "$PANE")" = claude ] || fail "claude lost its mark after its child ended"
  expect_attention "$SESS" status running "claude after its child gave the terminal back"
  expect_posted "$base" 0 "the child's group ending and the terminal coming back"
  stepn "claude: terminal taken back" "$SESS"
  type_line "$PANE" idle
  wait_status "$SESS" done 3
  expect_posted "$base" 1 "claude's idle after the child"
  stepn "claude: idle after the child" "$SESS"

  open_pane
  type_line "$PANE" "pi --no-reports"
  wait_agent "$PANE" pi 3
  sleep 2.5
  type_line "$PANE" fgchild
  wait_foreground_child
  wait_agent "$PANE" "" 4
  child_alive || fail "the child ended before the silent pi's mark cleared, so the check proves nothing"
  stepn "silent pi: child holds the terminal, mark cleared" "$SESS"
  wait_until 10 child_gone || fail "the child never ended"
  wait_agent "$PANE" pi 5
  stepn "silent pi: terminal taken back, mark back" "$SESS"
}

case_argv_stays_out_of_app_stderr() {
  local count
  open_pane
  type_line "$PANE" "claude SIRIO-ARGV-MARKER"
  wait_agent "$PANE" claude 3
  sleep 2
  count="$(grep -c SIRIO-ARGV-MARKER "$W/app.log" || true)"
  [ "$count" = 0 ] || fail "the app's stderr holds the argv marker $count time(s)"
  echo "  marker in the app's stderr: $count"
}

# ---------------------------------------------------------------- OpenCode

# The folder every terminal of this world opens in, as the service names it (a session's
# directory is the exact cwd its TUI started in).
OC_DIR="$(realpath "$W_REPO")"

oc_acked() { [ "$(cat "$RUN/oc.ack" 2>/dev/null || echo 0)" -ge "$OC_SENT" ]; }
# One command to the stub service ($RUN/oc.cmd), returned once the stub has handled it.
oc() {
  OC_SENT=$((OC_SENT + 1))
  echo "$*" >> "$RUN/oc.cmd"
  wait_until 5 oc_acked || fail "the OpenCode stub never took '$*'"
}
# The session ref of pane $1 in the world's database (the row a relaunch resumes with
# `opencode --session <ref>`), or nothing when the pane has none; `__ERROR__` when the
# database cannot be read, so a check for no row never passes on a failed read.
session_ref_of() { # pane
  python3 -I -c '
import sqlite3, sys
try:
    con = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
    row = con.execute("SELECT reference FROM session_ref WHERE session = ?", (sys.argv[2],)).fetchone()
except Exception:
    print("__ERROR__")
else:
    print(row[0] if row and row[0] is not None else "")
' "$W/s.sqlite" "$1" 2>/dev/null || echo __ERROR__
}
# Waits until pane $1's session ref reads $2 ("" for none) within $3 s.
wait_ref() { # pane want seconds
  local got=""
  for _ in $(seq 1 $(( $3 * 5 ))); do
    got="$(session_ref_of "$1")"
    if [ "$got" = "$2" ]; then
      echo "OK: $1 session ref = ${2:-(none)}"
      return 0
    fi
    sleep 0.2
  done
  step "FAILED: pane $1 wanted session ref '$2', last '$got'"
  fail "pane $1: the session ref never read '$2' within $3 s (last: '$got')"
}
# Holds for $3 s that pane $1's session ref stays $2 ("" for none).
hold_ref() { # pane want seconds
  local got=""
  for _ in $(seq 1 $(( $3 * 5 ))); do
    got="$(session_ref_of "$1")"
    [ "$got" = "$2" ] || { step "FAILED: pane $1 session ref read '$got' while it had to stay '$2'"; fail "pane $1: the session ref read '$got' while it had to stay '$2'"; }
    sleep 0.2
  done
  echo "OK: $1 session ref stays ${2:-(none)}"
}
# Stops the world's host and waits until its process is gone: the relaunch then has no host
# session to attach to.
host_down() {
  local pid
  pid="$(host_pid)"
  SIRIO_HOST_HOME="$W/host" "$PROBE" shutdown --force >/dev/null 2>&1 || true
  [ -z "$pid" ] && return 0
  wait_until 20 not_alive "$pid" || fail "the host (pid $pid) did not stop after shutdown --force"
}
not_alive() { ! alive "$1"; }
# The arguments after the program of the OpenCode process of pane $1, on one line (empty for a
# plain `opencode`); fails when the pane has none. A `--version` probe is not the tab's process.
oc_argv_of() { # pane
  python3 -I -c '
import os, sys
pane, exe = sys.argv[1:3]
for pid in filter(str.isdigit, os.listdir("/proc")):
    try:
        with open("/proc/" + pid + "/environ", "rb") as f:
            env = f.read().split(b"\0")
        if ("SIRIO_PANE_ID=" + pane).encode() not in env:
            continue
        with open("/proc/" + pid + "/cmdline", "rb") as f:
            argv = [a.decode(errors="replace") for a in f.read().split(b"\0")]
    except OSError:
        continue
    for i, word in enumerate(argv):
        if exe in word:
            rest = [a for a in argv[i + 1:] if a]
            if "--version" in rest:
                break
            print(" ".join(rest))
            sys.exit(0)
sys.exit(1)
' "$1" "$RUN/bin/opencode"
}
# Whether pane $1 runs `opencode --session <ref $2>`: what a relaunch, or a fresh tab whose
# session Sirio created, starts.
runs_resumed() { # pane ref
  local argv
  argv="$(oc_argv_of "$1")" || return 1
  case " $argv " in *" --session $2 "*) return 0 ;; esac
  return 1
}
# Whether, past the first $1 lines of `oc-starts`, a fake opencode noted a start in folder $2
# (`plugin <dir>` or `clean <dir>`, see the stub's header).
oc_started_since() { # lines dir
  tail -n +"$(( $1 + 1 ))" "$RUN/oc-starts" 2>/dev/null | grep -qE "^(plugin|clean) $2\$"
}
# Whether pane $1 runs a plain `opencode`: a process, and no `--session` among its arguments.
runs_plain() { # pane
  local argv
  argv="$(oc_argv_of "$1")" || return 1
  case " $argv " in *" --session "*) return 1 ;; esac
  return 0
}
# The `system.identify` sessionRef of pane $1 (empty when there is none).
identify_ref() { # pane
  SIRIO_PANE_ID="$1" SIRIO_WORKTREE_ID="$wt" ctl identify --json | python3 -I -c '
import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
print(reply.get("sessionRef", ""))'
}
identify_is() { [ "$(identify_ref "$1")" = "$2" ]; }
# How many lines of the stub's log match the pattern $1 (a grep pattern).
oc_log_count() { # pattern
  local n
  n="$(grep -c -e "$1" "$RUN/oc.log" 2>/dev/null || true)"
  echo "${n:-0}"
}
oc_log_more() { [ "$(oc_log_count "$1")" -gt "$2" ]; }
# Holds for $3 s that the stub's log keeps exactly $2 lines matching $1.
hold_log_count() { # pattern count seconds
  local got
  for _ in $(seq 1 $(( $3 * 5 ))); do
    got="$(oc_log_count "$1")"
    [ "$got" = "$2" ] || fail "the stub's log holds $got line(s) of '$1' while it had to stay at $2"
    sleep 0.2
  done
}
# Waits for the stub's successful POST /api/session after the first $1 of them, and sets
# OC_POST_ID and OC_POST_DIR from its log line (the directory must be this world's worktree).
OC_POST_ID=""
OC_POST_DIR=""
wait_post_ok() { # already-seen count
  local line
  wait_until 15 oc_log_more '^POST /api/session 200 ' "$1" || fail "the stub saw no successful POST /api/session after the first $1"
  line="$(grep '^POST /api/session 200 ' "$RUN/oc.log" | sed -n "$(( $1 + 1 ))p")"
  OC_POST_ID="$(echo "$line" | cut -d' ' -f4)"
  OC_POST_DIR="$(echo "$line" | cut -d' ' -f5-)"
  [ -n "$OC_POST_ID" ] || fail "the stub's POST line carries no session id: $line"
  [ "$(realpath "$OC_POST_DIR")" = "$OC_DIR" ] || fail "the session was created in '$OC_POST_DIR', not in the tab's worktree $OC_DIR"
}
# Opens an OpenCode 2 tab (`surface terminal open --agent opencode` plus any arguments) and
# waits until Sirio created its session: sets PANE, SESS and OC_TAB_ID. The pane's ref is that
# session's id and the tab's process resumes it, with no title sent anywhere.
OC_TAB_ID=""
open_oc_tab() {
  local seen
  seen="$(oc_log_count '^POST /api/session 200 ')"
  open_pane --agent opencode "$@"
  wait_post_ok "$seen"
  OC_TAB_ID="$OC_POST_ID"
  wait_ref "$PANE" "$OC_TAB_ID" 10
  wait_until 15 runs_resumed "$PANE" "$OC_TAB_ID" || fail "the OpenCode tab $PANE did not start opencode --session $OC_TAB_ID (argv: $(oc_argv_of "$PANE" || echo none))"
}
# Closes the tab of pane $1 (the debug build's --confirm skips the dialog for a live agent).
close_tab() { # pane
  ctl surface tabs close "$(tab_index_of "$1")" --confirm >/dev/null
}
# A typed `opencode` is linked by the watch and never gets a session ref: this fails on a ref
# (or on a database that cannot be read) at the moment it is called, which is after the link.
expect_no_ref() { # pane what
  local got
  got="$(session_ref_of "$1")"
  [ -z "$got" ] || fail "$2: pane $1 has the session ref '$got', a typed opencode must have none"
}
# The pid of the world's host (empty when none is registered).
host_pid() {
  python3 -I -c 'import glob, json, sys
files = glob.glob(sys.argv[1] + "/host-v*.json")
print(json.load(open(files[0]))["pid"] if files else "")' "$W/host" 2>/dev/null || true
}
host_up() {
  local pid
  pid="$(host_pid)"
  [ -n "$pid" ] && alive "$pid"
}
# Whether $2 ms have passed since the nanosecond clock reading $1.
elapsed_since() { # start_ns ms
  [ $(( ($(date +%s%N) - $1) / 1000000 )) -ge "$2" ]
}
# The number of `[opencode-watch] <kind>` lines in the app's stderr.
watch_lines() { grep -c '^\[opencode-watch\] ' "$W/app.log" || true; }
# Leaves the OpenCode stub in pane $1 and waits for the mark to go: a case's last step, so
# that the next one starts with no OpenCode pane in the folder.
leave_opencode() { # pane
  type_line "$1" exit
  wait_agent "$1" "" 3
}

# Writes byte for byte what older Sirio builds generated as a worktree OpenCode plugin:
# LEGACY_PANE_PLUGIN_TEMPLATE of rust/crates/sirio_agents/src/opencode.rs with `__SIRIOCTL__`
# as the JSON string literal of the path and `__PANE__` as the pane key (the header's dash is
# U+2014). A current build writes a plugin that reads its pane from the environment instead;
# both are still Sirio's to remove. Sirio removes a file
# only when it matches this text exactly, so a plant with the header and any other body would
# be the user's file to it, and would prove nothing about the removal.
write_generated_oc_plugin() { # file pane-key sirioctl-path
  local tpl lit
  IFS= read -r -d '' tpl <<'TEMPLATE' || true
// Generated by Sirio — reports the OpenCode session id for restore.
export const SirioSession = async ({ $ }) => {
  let reported = ""
  const sirioctl = __SIRIOCTL__
  const pane = "__PANE__"
  return {
    event: async ({ event }) => {
      const id = event?.properties?.info?.id
      if (event?.type === "session.updated" && id && id !== reported) {
        reported = id
        await $`${sirioctl} session-ref --session ${pane} --ref ${id}`.quiet().nothrow()
      }
    },
  }
}
TEMPLATE
  tpl="${tpl//__SIRIOCTL__/\"$3\"}"
  tpl="${tpl//__PANE__/"$2"}"
  mkdir -p "$(dirname "$1")"
  printf '%s' "$tpl" > "$1"
}

case_opencode_three_states() {
  local base
  # A session that exists before the pane starts: the watch reads it from the service's
  # list, and the pane's title (`OC | alpha`) is what links it. No event announces it.
  oc "create ses_t $OC_DIR"
  oc "rename ses_t alpha"
  start_agent opencode
  type_line "$PANE" "title OC | alpha"
  sleep 3
  stepn "opencode: linked by title, idle" "$SESS"
  expect_attention "$SESS" status null "an idle OpenCode"
  # A plain turn, no ask in it: running, then done after more than a second (+1).
  oc "busy ses_t"
  wait_status "$SESS" running 4
  # Running proves the title link holds: a title-based ref writer would have written by now.
  expect_no_ref "$PANE" "a typed opencode linked by title and running"
  sleep 1.3
  base="$(posted_count)"
  oc "idle ses_t"
  wait_status "$SESS" done 4
  expect_posted "$base" 1 "the end of a plain turn"
  stepn "opencode: a plain turn done" "$SESS"
  oc "busy ses_t"
  wait_status "$SESS" running 4
  sleep 1.3
  stepn "opencode working" "$SESS"
  capture "opencode-running"
  base="$(posted_count)"
  oc "wait ses_t"
  wait_status "$SESS" needs-input 4
  expect_posted "$base" 1 "a permission ask inside a turn"
  stepn "opencode asks for a permission" "$SESS"
  oc "reply ses_t"
  wait_status "$SESS" running 4
  base="$(posted_count)"
  oc "idle ses_t"
  wait_status "$SESS" done 4
  expect_posted "$base" 1 "the end of a turn"
  expect_attention "$SESS" unread true "the end of a turn"
  hold_ref "$PANE" "" 1
  stepn "opencode done" "$SESS"
  capture "opencode-done"
  # The service saw the app authenticate every call.
  grep -q '^GET /api/info 200$' "$RUN/oc.log" || fail "the stub never answered an authenticated /api/info"
  grep -q '^GET /api/event 200$' "$RUN/oc.log" || fail "the stub never opened an event stream"
  ! grep -q ' 401$' "$RUN/oc.log" || fail "the app made a call the stub refused (401)"
  leave_opencode "$PANE"
}

case_opencode_first_turn_links() {
  # The title is `OpenCode` until the first turn ends: the session born after the pane,
  # the only one in the folder and the only OpenCode pane there, is the pane's.
  local base
  start_agent opencode
  oc "create ses_f $OC_DIR"
  oc "busy ses_f"
  wait_status "$SESS" running 4
  sleep 1.3
  stepn "opencode: first turn, linked with no title" "$SESS"
  expect_no_ref "$PANE" "a typed opencode linked by its first turn"
  base="$(posted_count)"
  oc "idle ses_f"
  wait_status "$SESS" done 4
  expect_posted "$base" 1 "the end of the first turn"
  stepn "opencode: first turn done" "$SESS"
  # The TUI's title (`OC | <t>`) can reach the app before the service's rename does: the
  # link, and the turn's Done with it, must not go in between, and the rename adds no stop.
  base="$(posted_count)"
  type_line "$PANE" "title OC | firstturn"
  sleep 1.5
  hold_status "$SESS" done 1
  oc "rename ses_f firstturn"
  sleep 1.5
  hold_status "$SESS" done 1
  # The title and the rename have met: a title-based ref writer would have written by now.
  hold_ref "$PANE" "" 2
  expect_posted "$base" 0 "a title that came before the rename"
  stepn "opencode: first turn kept across the title and the rename" "$SESS"
  leave_opencode "$PANE"
}

case_opencode_duplicate_title_keeps_link() {
  # An older session of the folder already has the title the service gives this pane's
  # session (a generic one, "Greeting"): the pane was linked by the first-turn rule, and
  # the link must survive the rename and the TUI's title, which now match two sessions.
  local base
  oc "create ses_old $OC_DIR"
  oc "rename ses_old Greeting"
  start_agent opencode
  oc "create ses_d $OC_DIR"
  oc "busy ses_d"
  wait_status "$SESS" running 4
  sleep 1.3
  base="$(posted_count)"
  oc "idle ses_d"
  wait_status "$SESS" done 4
  expect_posted "$base" 1 "the end of the first turn"
  base="$(posted_count)"
  # The TUI's title lands before the service's rename (the order that happens): the held
  # session is untitled, and only the older session carries the title.
  type_line "$PANE" "title OC | Greeting"
  sleep 1.5
  hold_status "$SESS" done 2
  oc "rename ses_d Greeting"
  sleep 1.5
  hold_status "$SESS" done 2
  expect_posted "$base" 0 "a title, then a rename, to a title another session has"
  stepn "opencode: first turn kept across a duplicate title" "$SESS"
  hold_ref "$PANE" "" 1
  oc "busy ses_d"
  wait_status "$SESS" running 4
  sleep 1.3
  base="$(posted_count)"
  oc "idle ses_d"
  wait_status "$SESS" done 4
  expect_posted "$base" 1 "the end of a turn under a duplicate title"
  hold_ref "$PANE" "" 1
  stepn "opencode: a later turn under a duplicate title" "$SESS"
  leave_opencode "$PANE"
}

case_opencode_short_turn_is_finished() {
  # The service's own end of a turn is a finish whatever its length: the one-second floor
  # that hides the blip Codex sends does not apply to it.
  local base
  start_agent opencode
  base="$(posted_count)"
  oc "create ses_q $OC_DIR"
  oc "busy ses_q"
  oc "idle ses_q"
  wait_status "$SESS" done 4
  expect_posted "$base" 1 "the end of a turn under a second"
  expect_attention "$SESS" unread true "the end of a turn under a second"
  stepn "opencode: a short turn done" "$SESS"
  leave_opencode "$PANE"
}

case_opencode_resumed_finish_is_not_replayed() {
  # A turn ends, the TUI quits, and a new `opencode` in the same tab links to that session by
  # its title: the session's last turn ended before this pane's `opencode` started, so it is
  # no news: no finished status. (The tab already ran a turn, so a stop could not be seen here:
  # the notification is checked by the sibling case, in a tab that never did.)
  local base keeper
  # Another OpenCode pane in the folder keeps the watch (and what it saw) alive while the
  # tab's TUI is away: with none, the watch stops and reads the list again, which has no outcome.
  start_agent opencode
  keeper="$PANE"
  oc "create ses_p $OC_DIR"
  oc "rename ses_p resumed"
  start_agent opencode
  type_line "$PANE" "title OC | resumed"
  sleep 2
  oc "busy ses_p"
  wait_status "$SESS" running 4
  sleep 1.3
  oc "idle ses_p"
  wait_status "$SESS" done 4
  leave_opencode "$PANE"
  wait_status "$SESS" null 4
  sleep 1.5
  base="$(posted_count)"
  type_line "$PANE" opencode
  wait_agent "$PANE" opencode 3
  sleep 2.5
  type_line "$PANE" "title OC | resumed"
  sleep 3
  stepn "opencode: resumed a session whose turn ended before this pane started" "$SESS"
  expect_attention "$SESS" status null "a resumed session whose last turn is older than the pane"
  # The link is real: the session's next turn shows.
  oc "busy ses_p"
  wait_status "$SESS" running 4
  sleep 1.3
  base="$(posted_count)"
  oc "idle ses_p"
  wait_status "$SESS" done 4
  expect_posted "$base" 1 "the end of the next turn"
  leave_opencode "$PANE"
  leave_opencode "$keeper"
}

case_opencode_sibling_finish_is_not_replayed() {
  # Two TUIs in one folder, each in a new tab: B runs a turn and quits. A, still at its start
  # screen, is now alone, and the session B created after A started is the one the first-turn
  # rule links it to: a turn A never ran. It shows no finished status and posts nothing.
  local pane_a sess_a pane_b sess_b base
  start_agent opencode
  pane_a="$PANE"; sess_a="$SESS"
  start_agent opencode
  pane_b="$PANE"; sess_b="$SESS"
  oc "create ses_sib $OC_DIR"
  oc "rename ses_sib sibling"
  type_line "$pane_b" "title OC | sibling"
  sleep 2
  oc "busy ses_sib"
  wait_status "$sess_b" running 4
  sleep 1.3
  oc "idle ses_sib"
  wait_status "$sess_b" done 4
  sleep 1.5
  # Before B leaves: A links in the very event that ends B's program, so a later baseline
  # would miss a post made by the link.
  base="$(posted_count)"
  leave_opencode "$pane_b"
  wait_status "$sess_b" null 4
  sleep 3
  stepn "opencode: A alone, linked to a session B finished" "$sess_a"
  expect_attention "$sess_a" status null "a session another pane finished"
  expect_attention "$sess_a" unread false "a session another pane finished"
  expect_posted "$base" 0 "A linking to a session B finished"
  # A is linked: the session's next turn shows on A, and ends in one notification.
  oc "busy ses_sib"
  wait_status "$sess_a" running 4
  sleep 1.3
  base="$(posted_count)"
  oc "idle ses_sib"
  wait_status "$sess_a" done 4
  expect_posted "$base" 1 "the end of a turn on A, linked after B quit"
  leave_opencode "$pane_a"
}

case_opencode_own_finish_links_late() {
  # Two TUIs in one folder (the first-turn rule never applies): A's first turn ends before its
  # `OC | <t>` title and the service's rename arrive, so the title link forms at Done. That
  # finish is A's own, from a turn that began after A started: it shows, and A, off screen
  # (B is the attached tab), posts one notification.
  local pane_a sess_a pane_b sess_b base
  start_agent opencode
  pane_a="$PANE"; sess_a="$SESS"
  start_agent opencode
  pane_b="$PANE"; sess_b="$SESS"
  oc "create ses_late $OC_DIR"
  oc "busy ses_late"
  sleep 1.3
  oc "idle ses_late"
  sleep 1
  expect_attention "$sess_a" status null "A before its title and the rename"
  base="$(posted_count)"
  type_line "$pane_a" "title OC | late"
  oc "rename ses_late late"
  wait_status "$sess_a" done 4
  expect_posted "$base" 1 "A's own finish, linked after the turn ended"
  expect_attention "$sess_b" status null "B, which ran nothing"
  stepn "opencode: A's own finish, linked late" "$sess_a"
  leave_opencode "$pane_a"
  leave_opencode "$pane_b"
}

case_two_opencode_panes_one_folder() {
  local pane_a sess_a pane_b sess_b base
  start_agent opencode
  pane_a="$PANE"; sess_a="$SESS"
  start_agent opencode
  pane_b="$PANE"; sess_b="$SESS"
  # One new session, two panes that could own it: it is nobody's, and no status is guessed.
  oc "create ses_a $OC_DIR"
  oc "busy ses_a"
  sleep 2
  hold_status "$sess_a" null 1
  hold_status "$sess_b" null 1
  stepn "two opencode panes, one session: no guess (A)" "$sess_a"
  stepn "two opencode panes, one session: no guess (B)" "$sess_b"
  oc "idle ses_a"
  # A second session: both are new, still nobody's.
  oc "create ses_b $OC_DIR"
  oc "rename ses_a a"
  oc "rename ses_b b"
  sleep 1.5
  hold_status "$sess_a" null 1
  hold_status "$sess_b" null 1
  # The titles say which is which.
  type_line "$pane_a" "title OC | a"
  type_line "$pane_b" "title OC | b"
  sleep 1.5
  oc "busy ses_a"
  wait_status "$sess_a" running 4
  hold_status "$sess_b" null 1
  oc "busy ses_b"
  wait_status "$sess_b" running 4
  # Both are linked by their titles: neither typed pane has a ref.
  expect_no_ref "$pane_a" "A linked by its title"
  expect_no_ref "$pane_b" "B linked by its title"
  sleep 1.3
  base="$(posted_count)"
  oc "idle ses_a"
  wait_status "$sess_a" done 4
  expect_posted "$base" 1 "the end of A's turn"
  expect_attention "$sess_b" status running "B while A finished"
  stepn "opencode A done, B running" "$sess_a"
  base="$(posted_count)"
  oc "idle ses_b"
  wait_status "$sess_b" done 4
  expect_posted "$base" 1 "the end of B's turn"
  stepn "opencode B done" "$sess_b"
  leave_opencode "$pane_a"
  leave_opencode "$pane_b"
}

case_opencode_failed_turn_is_error() {
  local password base
  start_agent opencode
  oc "create ses_e $OC_DIR"
  oc "busy ses_e"
  wait_status "$SESS" running 4
  sleep 1.3
  base="$(posted_count)"
  oc "fail ses_e"
  wait_status "$SESS" error 4
  expect_posted "$base" 1 "a failed turn"
  stepn "opencode failed" "$SESS"
  # The error text is the provider's, and the password is a secret: neither is logged.
  password="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["password"])' "$W/xdg/state/opencode/service.json")"
  [ -n "$password" ] || fail "the stub's registration has no password"
  [ "$(grep -c 'STUB-ERROR-TEXT' "$W/app.log" || true)" = 0 ] || fail "the app's stderr holds the service's error text"
  [ "$(grep -cF "$password" "$W/app.log" || true)" = 0 ] || fail "the app's stderr holds the service's password"
  leave_opencode "$PANE"
}

case_opencode_after_relaunch() {
  local base
  start_agent opencode
  oc "create ses_r $OC_DIR"
  oc "rename ses_r relaunched"
  type_line "$PANE" "title OC | relaunched"
  oc "busy ses_r"
  wait_status "$SESS" running 4
  sleep 1.3
  oc "idle ses_r"
  wait_status "$SESS" done 4
  stepn "opencode before the quit: done" "$SESS"
  ctl quit >/dev/null 2>&1 || true
  for _ in $(seq 1 100); do alive "$APP_PID" || break; sleep 0.2; done
  alive "$APP_PID" && fail "sirio did not quit after sirioctl quit"
  APP_PID=""
  app_start
  ctl select-workspace --workspace "$wt" >/dev/null
  sleep 2
  # The new app learns the program from the host, and the pane's title from the host's
  # seed of the session it attaches (the title does not change again). The session is older
  # than this app's `opencode`, so only the title can link it.
  wait_agent "$PANE" opencode 15
  sleep 2
  stepn "opencode after the relaunch: the mark" "$SESS"
  # The new app starts with an empty `posted` list: whatever it holds now, its seed made.
  expect_posted 0 0 "the relaunch's own startup (the service said nothing new)"
  oc "busy ses_r"
  wait_status "$SESS" running 5
  sleep 1.3
  stepn "opencode after the relaunch: running" "$SESS"
  base="$(posted_count)"
  oc "idle ses_r"
  wait_status "$SESS" done 5
  expect_posted "$base" 1 "the end of a turn after the relaunch"
  stepn "opencode after the relaunch: done" "$SESS"
  # Linked again by its title after the relaunch: still no ref.
  hold_ref "$PANE" "" 1
  leave_opencode "$PANE"
}

case_opencode_agent_tab_resumes() {
  # Two tabs Sirio launched as OpenCode 2 in one folder (`surface terminal open --agent
  # opencode`), B opened first and A last. Sirio creates each tab's session (POST /api/session,
  # location = the worktree) and starts `opencode --session <id>`; the pane's ref is that id
  # from the launch on, before any title exists, and the first turn of a tab shows its status
  # at once (the watch links the pane's own created session while the title is `OpenCode`);
  # a session opened inside the TUI that no pane created takes that link away until the title
  # renames it. The watch only reads status: a relaunch with
  # the host alive keeps both refs and the hidden tab linked (B, hidden, is fed by the app's
  # listener, whose seed carries a `Program(None)`).
  local pane_a sess_a pane_b sess_b id_a id_b base
  open_oc_tab
  pane_b="$PANE"; sess_b="$SESS"; id_b="$OC_TAB_ID"
  open_oc_tab
  pane_a="$PANE"; sess_a="$SESS"; id_a="$OC_TAB_ID"
  [ "$(session_ref_of "$pane_b")" = "$id_b" ] || fail "B's ref changed when A's tab opened: '$(session_ref_of "$pane_b")', wanted $id_b"
  wait_until 5 identify_is "$pane_a" "$id_a" || fail "system.identify for $pane_a returns sessionRef '$(identify_ref "$pane_a")', wanted $id_a"
  wait_until 5 identify_is "$pane_b" "$id_b" || fail "system.identify for $pane_b returns sessionRef '$(identify_ref "$pane_b")', wanted $id_b"
  echo "OK: system.identify returns each tab's own session id ($id_a, $id_b)"
  # Two panes, two sessions with no title yet: each pane's TUI title is still `OpenCode`, and
  # the watch links a pane to the session Sirio created for it (the `created` rule), even with
  # another OpenCode tab in the folder. So A's first turn shows its status with no title at all,
  # and B, linked to its own idle session, shows none.
  oc "busy $id_a"
  wait_status "$sess_a" running 4
  hold_status "$sess_b" null 1
  sleep 1.3
  base="$(posted_count)"
  oc "idle $id_a"
  wait_status "$sess_a" done 4
  expect_posted "$base" 1 "the end of the first turn of an OpenCode tab Sirio launched"
  stepn "opencode agent tab A: first turn done, linked by its created session" "$sess_a"
  # A session opened inside the TUI (`/new`) that no pane created, born after A's: A cannot tell
  # which session its TUI shows any more, so nothing is linked. The stub has no delete, so
  # `ses_tui` stays in the folder; it is older than every session and pane the later cases make,
  # and the rule only looks at sessions born at or after a pane's own, so it disturbs none of them.
  oc "create ses_tui $OC_DIR"
  wait_status "$sess_a" null 4
  stepn "opencode agent tab A: a session opened in the TUI, no link" "$sess_a"
  oc "rename $id_a agenttab-a"
  oc "rename $id_b agenttab-b"
  type_line "$pane_a" "title OC | agenttab-a"
  type_line "$pane_b" "title OC | agenttab-b"
  oc "busy $id_a"
  wait_status "$sess_a" running 5
  hold_status "$sess_b" null 1
  sleep 1.3
  base="$(posted_count)"
  oc "idle $id_a"
  wait_status "$sess_a" done 5
  expect_posted "$base" 1 "the end of a turn in an OpenCode tab Sirio launched"
  stepn "opencode agent tabs: A done, B linked and idle" "$sess_a"
  # A relaunch with the host alive: A attaches, B stays hidden and the listener feeds it.
  relaunch_app
  wait_ref "$pane_a" "$id_a" 5
  wait_ref "$pane_b" "$id_b" 5
  oc "busy $id_b"
  wait_status "$sess_b" running 6
  stepn "opencode agent tab B, hidden after a relaunch: running" "$sess_b"
  oc "idle $id_b"
  wait_status "$sess_b" done 5
  # The higher position first, so each tab's index is still the one read.
  close_tab "$pane_a"
  close_tab "$pane_b"
}

case_opencode_unavailable_service_launches_plain() {
  local pane seen_ok seen_all seen_500 t0 t1 t2 ms
  # (1) The service answers the POST with 500: plain opencode, and no ref.
  oc "create fail"
  seen_ok="$(oc_log_count '^POST /api/session 200 ')"
  seen_500="$(oc_log_count '^POST /api/session 500$')"
  open_pane --agent opencode
  pane="$PANE"
  wait_until 15 oc_log_more '^POST /api/session 500$' "$seen_500" || fail "the app never sent its POST /api/session to the failing service"
  wait_until 15 runs_plain "$pane" || fail "the tab did not start a plain opencode after the POST failed (argv: $(oc_argv_of "$pane" || echo none))"
  expect_no_ref "$pane" "a tab whose session could not be created"
  hold_ref "$pane" "" 1
  [ "$(oc_log_count '^POST /api/session 200 ')" = "$seen_ok" ] || fail "a session was created though the stub was set to fail"
  echo "OK: POST answered 500: plain opencode, no ref"
  close_tab "$pane"

  # (2) The service answers after 8 s: the tab starts plain at the 3 s deadline, and the UI
  # thread is free meanwhile.
  oc "create delay 8"
  seen_ok="$(oc_log_count '^POST /api/session 200 ')"
  t0="$(date +%s%N)"
  open_pane --agent opencode
  pane="$PANE"
  t1="$(date +%s%N)"
  ctl surface tabs read >/dev/null
  t2="$(date +%s%N)"
  ms=$(( (t2 - t1) / 1000000 ))
  # The open itself must not wait for the service either: a launch that called the service on
  # the UI thread would hold the open's reply for the whole 3 s deadline.
  [ $(( (t1 - t0) / 1000000 )) -lt 2500 ] || fail "opening the tab took $(( (t1 - t0) / 1000000 )) ms while the service delayed: the open waited for it"
  elapsed_since "$t0" 7000 && fail "the control call ran $(( (t2 - t0) / 1000000 )) ms after the open: past the stub's delay, so it proves nothing"
  [ "$ms" -lt 1000 ] || fail "a control call took $ms ms while the app waited for the service: the UI thread is blocked"
  echo "OK: the open answered in $(( (t1 - t0) / 1000000 )) ms and a control call in $ms ms while the service delayed"
  wait_until 8 runs_plain "$pane" || fail "the tab never started a plain opencode within the 3 s deadline plus margin (argv: $(oc_argv_of "$pane" || echo none))"
  elapsed_since "$t0" 7500 && fail "the plain opencode appeared after the service's own 8 s delay: the deadline did not cut it"
  [ "$(oc_log_count '^POST /api/session 200 ')" = "$seen_ok" ] || fail "the service had answered before the tab started: the deadline proves nothing"
  expect_no_ref "$pane" "a tab that started at the deadline"
  echo "OK: the tab started plain before the service answered"
  # The late answer is no news to a tab that already started: it writes no ref.
  wait_until 12 elapsed_since "$t0" 9500 || fail "the clock did not reach the end of the service's delay"
  hold_ref "$pane" "" 2
  oc "create ok"
  close_tab "$pane"

  # (3) OpenCode 1 on the PATH: no POST at all, a plain opencode, and the v1 plugin.
  rm -f "$W_REPO/.opencode/plugin/sirio-session.js"
  echo 1.18.33 > "$RUN/oc-version"
  seen_all="$(oc_log_count '^POST /api/session ')"
  open_pane --agent opencode
  pane="$PANE"
  wait_until 15 runs_plain "$pane" || { echo 2.3.0 > "$RUN/oc-version"; fail "the tab did not start a plain opencode under OpenCode 1"; }
  echo 2.3.0 > "$RUN/oc-version"
  wait_until 5 test -f "$W_REPO/.opencode/plugin/sirio-session.js" || fail "no v1 plugin was written for an OpenCode 1 tab"
  [ "$(head -n 1 "$W_REPO/.opencode/plugin/sirio-session.js")" = '// Generated by Sirio — reports the OpenCode session id for restore.' ] || fail "the v1 plugin does not start with Sirio's generated header"
  grep -qF 'process.env.SIRIO_PANE_ID' "$W_REPO/.opencode/plugin/sirio-session.js" || fail "the v1 plugin does not read its pane from the environment"
  ! grep -qE 'pane-[0-9]' "$W_REPO/.opencode/plugin/sirio-session.js" || fail "the v1 plugin names a pane: one worktree file serves every tab"
  hold_log_count '^POST /api/session ' "$seen_all" 2
  expect_no_ref "$pane" "an OpenCode 1 tab"
  echo "OK: OpenCode 1: no POST, plain opencode, no ref, the plugin is written"
  close_tab "$pane"
  rm -f "$W_REPO/.opencode/plugin/sirio-session.js"
}

# Has Sirio itself write the real v1 plugin: an OpenCode 1 tab (`oc-version` 1.18.33) is opened,
# the file it writes into the worktree is waited for and copied to $RUN/real-v1-plugin.js, and
# the tab is closed. The plugin stays in the worktree and 2.3.0 is back, on every exit path.
make_real_v1_plugin() {
  local plugin="$W_REPO/.opencode/plugin/sirio-session.js" pane
  rm -f "$plugin"
  echo 1.18.33 > "$RUN/oc-version"
  open_pane --agent opencode
  pane="$PANE"
  wait_until 15 runs_plain "$pane" || { echo 2.3.0 > "$RUN/oc-version"; fail "the tab did not start a plain opencode under OpenCode 1"; }
  wait_until 5 test -f "$plugin" || { echo 2.3.0 > "$RUN/oc-version"; fail "no v1 plugin was written for an OpenCode 1 tab"; }
  cp "$plugin" "$RUN/real-v1-plugin.js"
  echo 2.3.0 > "$RUN/oc-version"
  close_tab "$pane"
}

case_opencode_v2_writes_no_plugin() {
  local plugin="$W_REPO/.opencode/plugin/sirio-session.js" saved="$RUN/user-plugin.js" first
  first='// Generated by Sirio — reports the OpenCode session id for restore.'
  # Sirio's own v1 plugin, exactly as a 1.x build writes it: OpenCode 2's TUI shows "Plugin
  # failed" for it, so the next fresh tab removes it.
  make_real_v1_plugin
  open_oc_tab
  wait_until 10 test ! -e "$plugin" || fail "Sirio's generated plugin is still there after an OpenCode 2 tab was created"
  echo "OK: Sirio's real v1 plugin was removed for an OpenCode 2 tab"
  close_tab "$PANE"
  # The legacy text older builds wrote, which names one pane, is Sirio's too: removed the same.
  write_generated_oc_plugin "$plugin" pane-1 "$CTL"
  open_oc_tab
  wait_until 10 test ! -e "$plugin" || fail "Sirio's legacy pane-naming plugin is still there after an OpenCode 2 tab was created"
  echo "OK: Sirio's legacy pane-naming plugin was removed for an OpenCode 2 tab"
  close_tab "$PANE"
  # The same file with its `const sirioctl` line turned into an expression is no longer one
  # JSON string literal there: it is the user's file, and stays byte for byte.
  python3 -I -c 'import sys
out = []
for line in open(sys.argv[1], encoding="utf-8").read().split("\n"):
    out.append("  const sirioctl = \"a\" + x() + \"b\"" if line.startswith("  const sirioctl = ") else line)
open(sys.argv[2], "w", encoding="utf-8").write("\n".join(out))' "$RUN/real-v1-plugin.js" "$plugin"
  cp "$plugin" "$saved"
  cmp -s "$plugin" "$RUN/real-v1-plugin.js" && fail "the edited copy of the v1 plugin is identical to the original: the edit proves nothing"
  open_oc_tab
  cmp -s "$plugin" "$saved" || fail "a copy of Sirio's plugin whose sirioctl line is not a string literal was changed or removed by an OpenCode 2 tab"
  echo "OK: a copy of the generated plugin with an edited sirioctl line was left byte for byte"
  close_tab "$PANE"
  # A plugin the user wrote, with no Sirio header, stays byte for byte.
  printf '%s\n%s\n' "// my own plugin" "export default async () => ({ name: 'mine' })" > "$plugin"
  cp "$plugin" "$saved"
  open_oc_tab
  cmp -s "$plugin" "$saved" || fail "a plugin without Sirio's header was changed or removed by an OpenCode 2 tab"
  echo "OK: a user's plugin was left byte for byte"
  close_tab "$PANE"
  # A file with Sirio's header and a body of the user's own is the user's too: only the exact
  # generated text is Sirio's to remove. The tab's `opencode --session` start (open_oc_tab
  # waits for it) comes after the plugin step ran, as the removal above relies on.
  printf '%s\n%s\n' "$first" "export default async () => ({ name: 'mine' })" > "$plugin"
  cp "$plugin" "$saved"
  open_oc_tab
  cmp -s "$plugin" "$saved" || fail "a file with Sirio's header line and the user's own body was changed or removed by an OpenCode 2 tab"
  echo "OK: a user's file under Sirio's header line was left byte for byte"
  close_tab "$PANE"
  rm -f "$plugin" "$saved"
}

case_opencode_unknown_version_writes_no_plugin() {
  local plugin="$W_REPO/.opencode/plugin/sirio-session.js" pane seen_all
  # `garbage` makes the fake print `opencode vgarbage`, which no probe can read. The app is
  # relaunched so its startup probe reads nothing too (the version memo keeps a version read
  # earlier in a run, so only a fresh process proves the unreadable case).
  make_real_v1_plugin
  [ -e "$plugin" ] || fail "the OpenCode 1 tab did not leave Sirio's v1 plugin in the worktree"
  echo garbage > "$RUN/oc-version"
  relaunch_app
  seen_all="$(oc_log_count '^POST /api/session ')"
  open_pane --agent opencode
  pane="$PANE"
  wait_until 15 runs_plain "$pane" || { echo 2.3.0 > "$RUN/oc-version"; fail "the tab did not start a plain opencode when the version could not be read (argv: $(oc_argv_of "$pane" || echo none))"; }
  hold_log_count '^POST /api/session ' "$seen_all" 2
  expect_no_ref "$pane" "an OpenCode tab with no readable version"
  # Under the old rule an unreadable version counted as 1.x and the plugin was written again.
  wait_until 10 test ! -e "$plugin" || { echo 2.3.0 > "$RUN/oc-version"; fail "Sirio's generated plugin is still there (or was rewritten) though OpenCode gave no readable version"; }
  echo "OK: no readable version: plain opencode, no POST, no ref, no plugin written, the generated one removed"
  close_tab "$pane"
  # The legacy pane-naming text is removed under an unreadable version too.
  write_generated_oc_plugin "$plugin" pane-1 "$CTL"
  open_pane --agent opencode
  pane="$PANE"
  wait_until 15 runs_plain "$pane" || { echo 2.3.0 > "$RUN/oc-version"; fail "the second tab did not start a plain opencode when the version could not be read"; }
  wait_until 10 test ! -e "$plugin" || { echo 2.3.0 > "$RUN/oc-version"; fail "Sirio's legacy pane-naming plugin is still there though OpenCode gave no readable version"; }
  echo "OK: no readable version: the legacy pane-naming plugin was removed too"
  close_tab "$pane"
  echo 2.3.0 > "$RUN/oc-version"
  relaunch_app
  rm -f "$plugin"
}

case_fresh_tab_drops_stale_ref() {
  local old_pane codex
  # Pane ids are reused after a relaunch: the first free id is one past the largest a persisted
  # layout names. A row left under the key the next tab takes (a crash, an older build, a closed
  # pane) is dropped as the fresh tab is made, whatever its agent, before anything could resume
  # it. A tab whose OpenCode 2 session could not be created writes none and would inherit it,
  # and so would any other agent.
  relaunch_app
  open_oc_tab
  old_pane="$PANE"
  close_tab "$old_pane"
  relaunch_app
  # Written through the control socket, so the app's mirror of the rows holds it, as it holds
  # every row it loaded at startup (a crash's, an older build's): a fresh tab looks there, and
  # deletes nothing in the database when the mirror has no row for its key. A row another
  # connection writes while the app runs is not seen (a known limit).
  ctl session-ref --session "$old_pane" --ref stale-ref-from-a-crash >/dev/null
  [ "$(session_ref_of "$old_pane")" = stale-ref-from-a-crash ] || fail "the stale row for $old_pane could not be written"
  open_pane --agent codex
  codex="$PANE"
  [ "$codex" = "$old_pane" ] || fail "the codex tab got the pane key $codex, not $old_pane: the stale row proves nothing"
  [ -z "$(session_ref_of "$codex")" ] || fail "the fresh codex tab $codex kept the stale ref '$(session_ref_of "$codex")'"
  echo "OK: a fresh tab under $codex dropped the stale row it would have inherited"
}

case_no_service_degrades() {
  local kinds
  # A healthy service logs nothing.
  [ "$(watch_lines)" = 0 ] || fail "the app logged a watch failure while the service was up: $(grep '^\[opencode-watch\] ' "$W/app.log" | head -n 3 | tr '\n' ' ')"
  start_agent opencode
  oc "create ses_n $OC_DIR"
  oc "busy ses_n"
  wait_status "$SESS" running 4
  oc "stop"
  # The status goes with the link, the mark does not, and the retries (2 s, 4 s, ...) name
  # the failure once.
  wait_status "$SESS" null 5
  sleep 8
  [ "$(agent_of "$PANE")" = opencode ] || fail "the OpenCode mark went with the service"
  expect_attention "$SESS" status null "an OpenCode whose service is gone"
  kinds="$(grep '^\[opencode-watch\] ' "$W/app.log" | sort | uniq -c | tr -s ' ' | tr '\n' ';')"
  echo "  watch failures logged: $kinds"
  [ "$(grep -c '^\[opencode-watch\] unreachable$' "$W/app.log" || true)" = 1 ] || fail "the failure kind 'unreachable' was not logged exactly once: $kinds"
  [ "$(grep '^\[opencode-watch\] ' "$W/app.log" | sort | uniq -d | wc -l | tr -d ' ')" = 0 ] || fail "a failure kind was logged twice: $kinds"
  [ "$(grep '^\[opencode-watch\] ' "$W/app.log" | grep -vc '^\[opencode-watch\] [a-z_]*$' || true)" = 0 ] || fail "a watch line holds more than a kind name"
  stepn "opencode: the service is gone" "$SESS"
  leave_opencode "$PANE"
}

# ---------------------------------------------------------------- tabs no terminal holds

# Quits the app and starts it again, then selects the worktree: only the tab that was
# selected at the quit attaches; every other tab stays unbound until it is shown.
relaunch_app() {
  ctl quit >/dev/null 2>&1 || true
  for _ in $(seq 1 100); do alive "$APP_PID" || break; sleep 0.2; done
  alive "$APP_PID" && fail "sirio did not quit after sirioctl quit"
  APP_PID=""
  app_start
  ctl select-workspace --workspace "$wt" >/dev/null
  sleep 2
}

# Two tabs of agent $1, B opened first and A last, then a quit and a relaunch: A is the
# attached tab (the selected one) and B has not been shown since. Sets PANE_A, SESS_A, PANE_B and SESS_B.
PANE_A=""; SESS_A=""; PANE_B=""; SESS_B=""
relaunch_with_a_hidden_tab() { # agent
  start_agent "$1"
  PANE_B="$PANE"; SESS_B="$SESS"
  start_agent "$1"
  PANE_A="$PANE"; SESS_A="$SESS"
  relaunch_app
  wait_agent "$PANE_A" "$1" 15
}

# A turn typed into the hidden tab B (host_probe writes to its host session): work, 1.3 s,
# done. Nothing selects the tab, so no terminal holds it.
case_background_tab_notifies() {
  local base
  relaunch_with_a_hidden_tab codex
  stepn "after the relaunch: A attached, B hidden" "$SESS_B"
  expect_posted 0 0 "the relaunch's own startup"
  base="$(posted_count)"
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  sleep 1.3
  stepn "hidden tab B: working" "$SESS_B"
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 5
  expect_posted "$base" 1 "the end of a turn in a tab no terminal holds"
  expect_attention "$SESS_B" unread true "the end of a turn in a tab no terminal holds"
  expect_attention "$SESS_B" running false "the end of a turn in a tab no terminal holds"
  stepn "hidden tab B: done, never shown" "$SESS_B"
}

case_showing_the_tab_adds_no_stop() {
  local base
  relaunch_with_a_hidden_tab codex
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  sleep 1.3
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 5
  expect_attention "$SESS_B" unread true "B's turn before it is shown"
  # Showing B attaches a terminal to it: the link seeds the same state, and the stop that
  # the listener already made is not made again. Being shown clears the mark.
  # The app's window reads inactive in this world (SIRIO_DEBUG_WINDOW_ACTIVE), so a tab on
  # screen is only seen while the window is told it is active.
  base="$(posted_count)"
  ctl surface attention act --focus active >/dev/null
  ctl tab select "$(tab_index_of "$PANE_B")" >/dev/null
  wait_agent "$PANE_B" codex 10
  sleep 2
  expect_posted "$base" 0 "showing a tab whose turn already ended"
  expect_attention "$SESS_B" unread false "B shown"
  expect_attention "$SESS_B" status done "B shown"
  stepn "B shown: no second stop" "$SESS_B"
  ctl surface attention act --focus inactive >/dev/null
  # The tab is bound now: its terminal's link is the route, the listener feeds nothing, and
  # a turn makes exactly one stop.
  base="$(posted_count)"
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  sleep 1.3
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 5
  expect_posted "$base" 1 "a turn in the tab that is shown"
  stepn "B shown: one stop for a full turn" "$SESS_B"
}

case_background_finish_at_relaunch_is_silent() {
  start_agent claude
  PANE_B="$PANE"; SESS_B="$SESS"
  long_work "$PANE_B" "$SESS_B"
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 5
  start_agent claude
  PANE_A="$PANE"; SESS_A="$SESS"
  # B is read: shown, with the window active, while its turn is over; then A is the selected tab again.
  ctl surface attention act --focus active >/dev/null
  ctl tab select "$(tab_index_of "$PANE_B")" >/dev/null
  sleep 2
  ctl tab select "$(tab_index_of "$PANE_A")" >/dev/null
  sleep 1
  ctl surface attention act --focus inactive >/dev/null
  expect_attention "$SESS_B" unread false "B read before the quit"
  stepn "before the quit: B done and read, A attached" "$SESS_B"
  relaunch_app
  wait_agent "$PANE_A" claude 15
  sleep 3
  stepn "after the relaunch: B hidden, its record is done" "$SESS_B"
  expect_posted 0 0 "the relaunch's own startup, B's finished turn in the host's record"
  expect_attention "$SESS_B" unread false "B hidden after the relaunch"
}

case_background_exit_mid_turn_is_one_stop() {
  local base
  relaunch_with_a_hidden_tab codex
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  wait_agent "$PANE_B" codex 5
  sleep 1.3
  base="$(posted_count)"
  type_line "$PANE_B" exit
  wait_status "$SESS_B" null 5
  # A hidden tab's row may be gone from `panel list` or list no agent: either is no agent.
  for _ in $(seq 1 25); do
    case "$(agent_of "$PANE_B")" in ""|__ABSENT__) break ;; esac
    sleep 0.2
  done
  case "$(agent_of "$PANE_B")" in ""|__ABSENT__) ;; *) fail "pane $PANE_B still lists agent '$(agent_of "$PANE_B")' after its agent exited" ;; esac
  expect_posted "$base" 1 "an exit mid-turn in a tab no terminal holds"
  expect_attention "$SESS_B" unread true "an exit mid-turn in a tab no terminal holds"
  expect_attention "$SESS_B" running false "an exit mid-turn in a tab no terminal holds"
  stepn "hidden tab B: exit mid-turn" "$SESS_B"
}

case_background_opencode_notifies() {
  local base
  start_agent opencode
  PANE_B="$PANE"; SESS_B="$SESS"
  oc "create ses_h $OC_DIR"
  oc "rename ses_h hidden"
  type_line "$PANE_B" "title OC | hidden"
  oc "busy ses_h"
  wait_status "$SESS_B" running 4
  oc "idle ses_h"
  wait_status "$SESS_B" done 4
  # A codex tab opened last is the one attached after the relaunch.
  start_agent codex
  PANE_A="$PANE"; SESS_A="$SESS"
  relaunch_app
  wait_agent "$PANE_A" codex 15
  # B is hidden: its program and its title come from the host through the app's listener.
  wait_agent "$PANE_B" opencode 15
  sleep 2
  stepn "opencode hidden after the relaunch: the mark" "$SESS_B"
  expect_posted 0 0 "the relaunch's own startup (the service said nothing new)"
  oc "busy ses_h"
  wait_status "$SESS_B" running 5
  sleep 1.3
  base="$(posted_count)"
  oc "idle ses_h"
  wait_status "$SESS_B" done 5
  expect_posted "$base" 1 "the end of an OpenCode turn in a hidden tab"
  expect_attention "$SESS_B" unread true "the end of an OpenCode turn in a hidden tab"
  stepn "opencode hidden: done" "$SESS_B"
  leave_opencode "$PANE_B"
}

case_control_session_gets_no_state() {
  local base rows id
  base="$(posted_count)"
  rows="$(attention_rows | wc -l | tr -d ' ')"
  # A session that belongs to no tab: `panel create` makes a login shell the app lists
  # as a control panel.
  id="$(ctl panel create --worktree "$wt" --json | python3 -I -c 'import json, sys
row = json.load(sys.stdin)
row = row[0] if isinstance(row, list) else row
print(row.get("id", ""))')"
  [ -n "$id" ] || fail "panel create named no id"
  sleep 1
  type_line "$id" codex
  sleep 2.5
  type_line "$id" work
  sleep 1.5
  type_line "$id" done
  sleep 2
  step "control session $id after a turn"
  expect_posted "$base" 0 "a turn in a session that belongs to no tab"
  case "$(agent_of "$id")" in ""|__ABSENT__) ;; *) fail "the control session $id lists agent '$(agent_of "$id")': the listener fed a session no tab names" ;; esac
  [ "$(attention_rows | wc -l | tr -d ' ')" = "$rows" ] || fail "a control session added an attention row: $(attention_rows | tr '\n' ' ')"
  type_line "$id" exit
}

# A worktree the app has not selected since the relaunch: no terminal of it is mounted, and
# its tab is named only by the layout the database keeps. (Evicting a mounted worktree
# from the pane cache would be the other way to get here, but no terminal is dropped
# today: `subscribe_terminal` keeps a strong handle to its terminal in `TerminalBindings`,
# which is never emptied, so a worktree's views outlive their eviction.)
case_other_worktree_notifies() {
  local wt_b base
  wt_b="$(worktree_id_of "$W/repo-b")"
  [ -n "$wt_b" ] || fail "the project does not list the worktree repo-b"
  ctl select-workspace --workspace "$wt_b" >/dev/null
  sleep 2
  OPEN_DIR="$W/repo-b" start_agent codex
  PANE_B="$PANE"; SESS_B="$SESS"
  ctl select-workspace --workspace "$wt" >/dev/null
  sleep 2
  relaunch_app
  # The worktree repo-b is not selected, so none of its terminals is mounted.
  step "after the relaunch: repo-b not selected, A selected"
  expect_posted 0 0 "the relaunch's own startup"
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  sleep 1.3
  base="$(posted_count)"
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 5
  expect_posted "$base" 1 "the end of a turn in a worktree not selected since the relaunch"
  expect_attention "$SESS_B" unread true "the end of a turn in a worktree not selected since the relaunch"
  stepn "worktree repo-b, not selected: done" "$SESS_B"
}


# Kills this world's host, by pid: only a process of this run's own directory is signalled
# (the way `cleanup` finds the hosts).
kill_world_hosts() {
  local pid
  for pid in $(pgrep -f "$RUN" 2>/dev/null); do
    is_a_host "$pid" && kill -9 "$pid" 2>/dev/null
  done
  return 0
}

# The number of sessions the attention read reports as running.
running_rows() {
  attention_rows | python3 -I -c 'import json, sys
print(sum(1 for line in sys.stdin if json.loads(line).get("running") is True))'
}

# The activity model's own count of panes running, which the update card reads (`surface
# update read`): closing a worktree's attention records does not reset it.
agents_working() {
  ctl surface update read --json | python3 -I -c 'import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
print(reply["agentsWorking"])'
}

# Waits until the app has posted $2 notifications since baseline $1, for $3 s.
posted_reached() { [ "$(( $(posted_count) - $1 ))" -ge "$2" ]; }

# A hidden agent tab whose session ends mid-turn. Mode "spawn" is a Sirio-launched agent
# (`surface terminal open --agent`): restored by the relaunch, spawn-owned, and the PTY child is the
# stub itself, so the host names no program. Mode "exec" is a hand-typed `exec codex`: the stub
# takes the shell's place, the host names no program either, and the report's `app=` gives the
# identity. The stub exits with code $2; the session's end is the host's `Exited` with that code.
hidden_agent_ends_mid_turn() { # mode code
  local mode="$1" code="$2" base
  case "$mode" in
    spawn) open_pane --agent codex ;;
    exec)
      open_pane
      type_line "$PANE" "exec codex"
      wait_agent "$PANE" codex 5 ;;
  esac
  PANE_B="$PANE"; SESS_B="$SESS"
  sleep 3
  start_agent codex
  PANE_A="$PANE"; SESS_A="$SESS"
  relaunch_app
  wait_agent "$PANE_A" codex 15
  sleep 2
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  sleep 1.3
  stepn "hidden $mode agent tab B, mid-turn" "$SESS_B"
  base="$(posted_count)"
  type_line "$PANE_B" "exit $code"
  wait_until 8 posted_reached "$base" 1 || true
  sleep 1.5
  stepn "hidden $mode agent tab B, its session ended with code $code" "$SESS_B"
  expect_posted "$base" 1 "the end of a hidden $mode agent tab's session mid-turn (exit $code)"
  expect_attention "$SESS_B" unread true "the end of a hidden $mode agent tab mid-turn (exit $code)"
  expect_attention "$SESS_B" running false "the end of a hidden $mode agent tab mid-turn (exit $code)"
  # Showing the tab attaches a link to the exited session: its own end is the same end, and
  # makes no second stop.
  base="$(posted_count)"
  ctl tab select "$(tab_index_of "$PANE_B")" >/dev/null
  sleep 4
  expect_posted "$base" 0 "showing a tab whose session already ended (exit $code)"
  stepn "hidden $mode agent tab B shown after its end" "$SESS_B"
}

case_background_agent_tab_ends_mid_turn() { hidden_agent_ends_mid_turn spawn 0; }
# A non-zero exit takes the path that skips the settle and goes straight to the exit result.
case_background_agent_tab_fails_mid_turn() { hidden_agent_ends_mid_turn spawn 3; }
case_background_exec_agent_ends_mid_turn() { hidden_agent_ends_mid_turn exec 0; }

# The host dies under two tabs mid-turn, the attached one and a hidden one: both end the same
# way, and a fresh host is then used by the hidden tab, whose turn makes one stop.
case_background_host_lost() {
  local base
  relaunch_with_a_hidden_tab codex
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  type_line "$PANE_A" work
  wait_status "$SESS_A" running 5
  sleep 1.3
  stepn "before the host dies: A and B mid-turn" "$SESS_B"
  base="$(posted_count)"
  kill_world_hosts
  # The links' grace is 10 s; the listener's the same.
  wait_until 25 posted_reached "$base" 2 || true
  sleep 2
  stepn "after the host died: B" "$SESS_B"
  stepn "after the host died: A" "$SESS_A"
  expect_attention "$SESS_B" running false "the hidden tab B after the host was lost"
  expect_attention "$SESS_A" running false "the attached tab A after the host was lost"
  expect_attention "$SESS_B" unread true "the hidden tab B after the host was lost"
  expect_attention "$SESS_A" unread true "the attached tab A after the host was lost"
  expect_posted "$base" 2 "the host lost under two tabs mid-turn (one stop each)"
  [ "$(posted_since "$base" | python3 -I -c 'import json, sys
print(len({(json.loads(l)["title"], json.loads(l)["body"]) for l in sys.stdin}))')" = 1 ] || fail "the two tabs were not told the same thing: $(posted_since "$base" | tr '\n' ' ')"
  # Showing B ensures a new host; B is a plain shell there. A full turn through its link, and
  # the listener that follows the new adoption, make one stop.
  ctl tab select "$(tab_index_of "$PANE_B")" >/dev/null
  sleep 8
  type_line "$PANE_B" codex
  wait_agent "$PANE_B" codex 15
  sleep 2.5
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  sleep 1.3
  base="$(posted_count)"
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 5
  expect_posted "$base" 1 "a turn on the fresh host"
  stepn "B on the fresh host: one stop" "$SESS_B"
}

# A worktree nobody selected since the relaunch is closed while its agent is mid-turn: the end
# of its session is the app's own doing and posts nothing, and nothing of it stays running.
case_close_hidden_worktree_mid_turn_is_silent() {
  local wt_c base running_before working_before
  wt_c="$(worktree_id_of "$W/repo-c")"
  [ -n "$wt_c" ] || fail "the project does not list the worktree repo-c"
  ctl select-workspace --workspace "$wt_c" >/dev/null
  sleep 2
  OPEN_DIR="$W/repo-c" start_agent codex
  PANE_B="$PANE"; SESS_B="$SESS"
  ctl select-workspace --workspace "$wt" >/dev/null
  sleep 2
  relaunch_app
  running_before="$(running_rows)"
  working_before="$(agents_working)"
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  # A spinner title every 100 ms: events of the session are in flight when the worktree closes.
  type_line "$PANE_B" spin
  sleep 1.3
  [ "$(running_rows)" = "$(( running_before + 1 ))" ] || fail "the running count did not count the hidden turn: $(attention_rows | tr '\n' ' ')"
  [ "$(agents_working)" = "$(( working_before + 1 ))" ] || fail "the activity model's working count is $(agents_working), it was $working_before before the turn"
  base="$(posted_count)"
  ctl close-workspace --workspace "$wt_c" >/dev/null
  sleep 6
  stepn "repo-c closed mid-turn" "$SESS_B"
  expect_posted "$base" 0 "closing a worktree whose hidden tab is mid-turn"
  [ -z "$(attention_get "$SESS_B" status)" ] || [ "$(attention_get "$SESS_B" status)" = null ] || fail "the closed worktree's session still has status '$(attention_get "$SESS_B" status)'"
  [ "$(running_rows)" = "$running_before" ] || fail "the running count is $(running_rows), it was $running_before before the turn: $(attention_rows | tr '\n' ' ')"
  # The activity model's own count (what the update card reads), which closing a worktree's
  # attention records does not reset.
  [ "$(agents_working)" = "$working_before" ] || fail "the activity model still counts $(agents_working) agents working, it was $working_before before the turn"
}

# A Sirio-launched agent tab whose session exits (code 3) while the app is down, hidden or
# attached after the relaunch. Sets X_STATUS, the session's status once the relaunch has settled.
exit_while_the_app_is_down() { # hidden|shown
  if [ "$1" = shown ]; then
    start_agent codex
    open_pane --agent codex
    PANE_X="$PANE"; SESS_X="$SESS"
    sleep 3
  else
    open_pane --agent codex
    PANE_X="$PANE"; SESS_X="$SESS"
    sleep 3
    start_agent codex
  fi
  ctl quit >/dev/null 2>&1 || true
  for _ in $(seq 1 100); do alive "$APP_PID" || break; sleep 0.2; done
  alive "$APP_PID" && fail "sirio did not quit after sirioctl quit"
  APP_PID=""
  type_line "$PANE_X" "exit 3"
  sleep 2
  app_start
  ctl select-workspace --workspace "$wt" >/dev/null
  sleep 6
  X_STATUS="$(attention_get "$SESS_X" status)"
  stepn "$1 agent tab X after its session exited while the app was down" "$SESS_X"
}

# The notifications of a process are its own (`posted` starts empty at every launch), so each
# relaunch's count is read as it is. A session that exited while the app was down is seeded as
# the link of an attached tab seeds it: the same status, and nothing posted.
case_background_exit_while_app_down_is_silent() {
  local hidden shown hidden_status
  exit_while_the_app_is_down hidden
  hidden="$(posted_count)"; hidden_status="$X_STATUS"
  echo "  the relaunch posted $hidden for the hidden tab, whose status reads '$hidden_status': $(posted_rows | tr '\n' ' ')"
  exit_while_the_app_is_down shown
  shown="$(posted_count)"
  echo "  the relaunch posted $shown for the attached tab, whose status reads '$X_STATUS': $(posted_rows | tr '\n' ' ')"
  [ "$hidden" = "$shown" ] || fail "a hidden tab's session that exited while the app was down posted $hidden, the attached tab $shown"
  [ "$hidden" = 0 ] || fail "a session that exited while the app was down posted $hidden at the relaunch"
  [ "$hidden_status" = "$X_STATUS" ] || fail "a hidden tab's session that exited while the app was down reads '$hidden_status', the attached tab's '$X_STATUS'"
}

# The panel ids `panel list` names for tabs of the selected worktree, one per line.
tab_pane_ids() {
  ctl panel list --json | python3 -I -c 'import json, re, sys
for row in json.load(sys.stdin):
    if re.fullmatch(r"pane-[0-9]+", row["id"]):
        print(row["id"])'
}

# A Show-session tab of a hidden tab's session: the stop of a turn is the hidden tab's own (its
# mark, its notification), and only one. The Show tab makes none, and closing it leaves the hidden
# tab fed.
case_show_session_of_hidden_tab_is_one_stop() {
  local wt_b base running_before working_before before shown
  wt_b="$(worktree_id_of "$W/repo-b")"
  [ -n "$wt_b" ] || fail "the project does not list the worktree repo-b"
  ctl select-workspace --workspace "$wt_b" >/dev/null
  sleep 2
  OPEN_DIR="$W/repo-b" start_agent codex
  PANE_B="$PANE"; SESS_B="$SESS"
  ctl select-workspace --workspace "$wt" >/dev/null
  sleep 2
  relaunch_app
  running_before="$(running_rows)"
  working_before="$(agents_working)"
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  sleep 1.3
  # From the selected worktree: the session is not a tab of it, so a tab that shows it opens,
  # in the middle of the turn.
  before="$(tab_pane_ids)"
  ctl panel show "$PANE_B" >/dev/null
  sleep 4
  shown="$(comm -13 <(echo "$before" | sort) <(tab_pane_ids | sort) | head -n 1)"
  [ -n "$shown" ] || fail "panel show opened no tab for $PANE_B"
  step "session $PANE_B shown by $shown, mid-turn"
  base="$(posted_count)"
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 6
  expect_posted "$base" 1 "a turn in a hidden tab whose session a Show-session tab also shows"
  expect_attention "$SESS_B" running false "the hidden tab whose turn a Show-session tab also shows"
  case "$(posted_since "$base")" in *'"body": "wt-b'*) ;; *) fail "the notification's body does not name the hidden tab's branch (wt-b): $(posted_since "$base")" ;; esac
  [ "$(running_rows)" = "$running_before" ] || fail "the running count is $(running_rows), it was $running_before before the turn"
  [ "$(agents_working)" = "$working_before" ] || fail "the activity model counts $(agents_working) agents working, it was $working_before"
  stepn "show session: one stop for one turn, the hidden tab's" "$SESS_B"
  # Closing the Show tab leaves its view behind: it must not silence the hidden tab.
  ctl surface tabs close "$(tab_index_of "$shown")" >/dev/null
  sleep 3
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 6
  sleep 1.3
  base="$(posted_count)"
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 6
  expect_posted "$base" 1 "a turn in a hidden tab after the Show-session tab of its session closed"
  case "$(posted_since "$base")" in *'"body": "wt-b'*) ;; *) fail "the notification's body does not name the hidden tab's branch (wt-b): $(posted_since "$base")" ;; esac
  stepn "show session closed: the hidden tab still notifies" "$SESS_B"
}

# A Show-session tab on screen, the window active: the turn of the hidden tab it shows is watched,
# so it is no news (no notification, no unread mark), as it would be in the tab itself.
case_show_session_on_screen_is_silent() {
  local wt_b base
  wt_b="$(worktree_id_of "$W/repo-b")"
  [ -n "$wt_b" ] || fail "the project does not list the worktree repo-b"
  ctl select-workspace --workspace "$wt_b" >/dev/null
  sleep 2
  OPEN_DIR="$W/repo-b" start_agent codex
  PANE_B="$PANE"; SESS_B="$SESS"
  ctl select-workspace --workspace "$wt" >/dev/null
  sleep 2
  relaunch_app
  ctl panel show "$PANE_B" >/dev/null
  sleep 4
  ctl surface attention act --focus active >/dev/null
  sleep 1
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 6
  sleep 1.3
  base="$(posted_count)"
  type_line "$PANE_B" done
  wait_status "$SESS_B" done 6
  sleep 1.5
  stepn "show session on screen: the turn is watched" "$SESS_B"
  ctl surface attention act --focus inactive >/dev/null
  expect_posted "$base" 0 "a turn in a hidden tab whose Show-session tab is on screen"
  expect_attention "$SESS_B" unread false "the hidden tab whose Show-session tab is on screen"
}

# A Restart is the user's own act: the session it ends and the one it opens make no stop.
case_restart_agent_tab_mid_turn_is_silent() {
  local base
  open_pane --agent codex
  PANE_R="$PANE"; SESS_R="$SESS"
  sleep 3
  type_line "$PANE_R" work
  wait_status "$SESS_R" running 5
  sleep 1.3
  base="$(posted_count)"
  ctl surface terminal restart "$PANE_R" >/dev/null
  sleep 6
  stepn "restarted agent tab R" "$SESS_R"
  expect_posted "$base" 0 "a Restart of an agent tab mid-turn"
  expect_attention "$SESS_R" status null "the restarted agent tab"
  expect_attention "$SESS_R" running false "the restarted agent tab"
  # The new session's reports are accepted: a full turn is one stop.
  type_line "$PANE_R" work
  wait_status "$SESS_R" running 5
  sleep 1.3
  base="$(posted_count)"
  type_line "$PANE_R" done
  wait_status "$SESS_R" done 5
  expect_posted "$base" 1 "a turn in the restarted agent tab"
  stepn "restarted agent tab R: a full turn" "$SESS_R"
}

# The same for a hand-typed agent: the restart opens a plain shell, so the pane names no agent.
case_restart_hand_typed_agent_tab_mid_turn_is_silent() {
  local base
  start_agent codex
  PANE_R="$PANE"; SESS_R="$SESS"
  type_line "$PANE_R" work
  wait_status "$SESS_R" running 5
  sleep 1.3
  base="$(posted_count)"
  ctl surface terminal restart "$PANE_R" >/dev/null
  sleep 6
  stepn "restarted hand-typed agent tab R" "$SESS_R"
  expect_posted "$base" 0 "a Restart of a hand-typed agent tab mid-turn"
  expect_attention "$SESS_R" status null "the restarted hand-typed agent tab"
  expect_attention "$SESS_R" running false "the restarted hand-typed agent tab"
  [ "$(agent_of "$PANE_R")" = "" ] || fail "the restarted pane still lists agent '$(agent_of "$PANE_R")'"
}

# One end per session: a title the host publishes after the exit does not make the session a new
# one, so its `Closed` is no second end (which would be a second exit result, here an Unknown one).
case_one_end_per_session() {
  local base status
  open_pane --agent codex
  PANE_B="$PANE"; SESS_B="$SESS"
  sleep 3
  start_agent codex
  PANE_A="$PANE"; SESS_A="$SESS"
  relaunch_app
  wait_agent "$PANE_A" codex 15
  sleep 2
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  sleep 1.3
  base="$(posted_count)"
  type_line "$PANE_B" "exit 0"
  sleep 4
  expect_posted "$base" 1 "the exit of a hidden agent tab"
  status="$(attention_get "$SESS_B" status)"
  "$PROBE" term-feed "$PANE_B" $'\033]2;after the exit\007' >/dev/null || true
  sleep 1.5
  "$PROBE" term-close "$PANE_B" >/dev/null || true
  sleep 4
  stepn "hidden agent tab B: exited, a title after it, then closed by the host" "$SESS_B"
  expect_posted "$base" 1 "the host closing a session whose end was already sent"
  [ "$(attention_get "$SESS_B" status)" = "$status" ] || fail "the status went from '$status' to '$(attention_get "$SESS_B" status)' when the exited session was closed"
}

# End all sessions (`host.shutdown {force}`, the call `perform_end_all_sessions` makes, here from
# another client of the same host): the attached tab and a hidden one, both mid-turn, are told the
# same. It shuts the host down, so it runs last.
case_end_all_sessions_parity() {
  local base
  relaunch_with_a_hidden_tab codex
  type_line "$PANE_B" work
  wait_status "$SESS_B" running 5
  type_line "$PANE_A" work
  wait_status "$SESS_A" running 5
  sleep 1.3
  base="$(posted_count)"
  SIRIO_HOST_HOME="$W/host" "$PROBE" shutdown --force >/dev/null
  wait_until 25 posted_reached "$base" 2 || true
  sleep 3
  stepn "after End all sessions: B" "$SESS_B"
  stepn "after End all sessions: A" "$SESS_A"
  echo "  posted: $(posted_since "$base" | tr '\n' ' ')"
  expect_posted "$base" 2 "End all sessions under an attached tab and a hidden one (one each)"
  [ "$(posted_since "$base" | python3 -I -c 'import json, sys
print(len({(json.loads(l)["title"], json.loads(l)["body"]) for l in sys.stdin}))')" = 1 ] || fail "the hidden tab was not told what the attached tab was: $(posted_since "$base" | tr '\n' ' ')"
  expect_attention "$SESS_B" running false "the hidden tab after End all sessions"
  expect_attention "$SESS_A" running false "the attached tab after End all sessions"
  echo "  statuses after End all sessions: hidden '$(attention_get "$SESS_B" status)', attached '$(attention_get "$SESS_A" status)'"
  [ "$(attention_get "$SESS_B" status)" = "$(attention_get "$SESS_A" status)" ] || fail "the hidden tab reads '$(attention_get "$SESS_B" status)' after End all sessions, the attached tab '$(attention_get "$SESS_A" status)'"
}

# The OpenCode stub, started again when an earlier case stopped it (`no_service_degrades`):
# the new process registers a new port and password where the service does, and reads
# $RUN/oc.cmd from its start.
oc_stub_restart() {
  : > "$RUN/oc.cmd"
  rm -f "$RUN/oc.ack"
  OC_SENT=0
  rm -f "$W/xdg/state/opencode/service.json"
  python3 -I "$OC_STUB" --registry "$W/xdg/state/opencode/service.json" --cmd "$RUN/oc.cmd" \
    --ack "$RUN/oc.ack" --log "$RUN/oc.log" >>"$RUN/oc-stub.out" 2>&1 &
  OC_PID=$!
  wait_until 10 test -s "$W/xdg/state/opencode/service.json" || fail "the OpenCode stub never registered again"
}

case_opencode_resumes_without_host() {
  # Last: it stops the host. An OpenCode tab with its created session, the app quits and the
  # host goes with every session, and the relaunch has nothing to attach to: the selected tab
  # starts its agent again on the saved session. A second, hidden tab starts only when it is
  # shown, as any restored tab does. The world is left with the app and a host up.
  local pane sess id plugin starts hidden hidden_id
  alive "$OC_PID" || oc_stub_restart
  # A fresh app, whatever the earlier cases (End all sessions) left of the host.
  relaunch_app
  open_oc_tab
  hidden="$PANE"; hidden_id="$OC_TAB_ID"
  open_oc_tab
  pane="$PANE"; sess="$SESS"; id="$OC_TAB_ID"
  # Sirio's generated v1 plugin in the tab's worktree (an older build wrote it): a restored
  # OpenCode pane waits for the first version probe of the relaunch, whose plugin step removes
  # it under OpenCode 2, before its `opencode` starts. A pane that started at once would find it
  # (OpenCode 2's TUI shows "Plugin failed" for it) and nothing would take it away.
  plugin="$W_REPO/.opencode/plugin/sirio-session.js"
  mkdir -p "$(dirname "$plugin")"
  write_generated_oc_plugin "$plugin" pane-1 "$CTL"
  ctl quit >/dev/null 2>&1 || true
  for _ in $(seq 1 100); do alive "$APP_PID" || break; sleep 0.2; done
  alive "$APP_PID" && fail "sirio did not quit after sirioctl quit"
  APP_PID=""
  host_down
  # The fake opencode notes, as it starts, whether the plugin was in its folder (`oc-starts`).
  starts="$(wc -l < "$RUN/oc-starts" 2>/dev/null || echo 0)"
  app_start
  ctl select-workspace --workspace "$wt" >/dev/null
  wait_until 20 runs_resumed "$pane" "$id" || fail "the relaunched OpenCode tab $pane did not start opencode --session $id"
  [ ! -e "$plugin" ] || fail "Sirio's generated v1 plugin is still in the worktree after the restored OpenCode pane started"
  # The process can be there before it wrote its line: wait for the line, then read them all.
  wait_until 5 oc_started_since "$starts" "$(cd "$W_REPO" && pwd -P)" || fail "no opencode noted its start in $W_REPO after the relaunch: $(tail -n +"$((starts + 1))" "$RUN/oc-starts" 2>/dev/null | tr '\n' ';')"
  ! tail -n +"$((starts + 1))" "$RUN/oc-starts" | grep -q '^plugin ' || fail "a restored OpenCode pane started while Sirio's generated v1 plugin was still in its folder: $(tail -n +"$((starts + 1))" "$RUN/oc-starts" | tr '\n' ';')"
  echo "OK: $pane was started again as opencode --session $id, and no restored opencode found Sirio's v1 plugin as it started"
  wait_ref "$pane" "$id" 5
  wait_until 5 identify_is "$pane" "$id" || fail "system.identify for $pane returns '$(identify_ref "$pane")' after the relaunch, wanted $id"
  # The release of the held launches came before the shown tab started: a hidden tab it had
  # opened would be running by now. It opens when it is shown, on its own session.
  wait_until 2 oc_argv_of "$hidden" >/dev/null && fail "the hidden OpenCode tab $hidden started before it was shown (argv: $(oc_argv_of "$hidden"))"
  ctl surface tabs select "$(tab_index_of "$hidden")" >/dev/null
  wait_until 20 runs_resumed "$hidden" "$hidden_id" || fail "the hidden OpenCode tab $hidden, once shown, did not start opencode --session $hidden_id (argv: $(oc_argv_of "$hidden" || echo none))"
  echo "OK: the hidden tab $hidden stayed unopened until shown, then resumed $hidden_id"
  ctl surface tabs select "$(tab_index_of "$pane")" >/dev/null
  stepn "opencode agent tab after a relaunch with no host: resumed" "$sess"
  oc "rename $id resumed-tab"
  type_line "$pane" "title OC | resumed-tab"
  oc "busy $id"
  wait_status "$sess" running 5
  oc "idle $id"
  wait_status "$sess" done 5
  stepn "opencode agent tab after the resume: done" "$sess"
  # The higher position first, so each tab's index is still the one read.
  close_tab "$pane"
  close_tab "$hidden"
  ctl_ready || fail "the app does not answer after the case"
  wait_until 20 host_up || fail "no host is running after the case"
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
run_case startup_is_silent
run_case claude_states
run_case first_report_blocked
run_case turn_end_notifies_once
run_case blocked_inside_turn_then_idle
run_case short_turn_is_silent
run_case exit_mid_turn_is_finished
run_case clear_mid_turn_is_silent
run_case hooks_yield_to_reports
run_case pi_reports_once_answered
run_case hooks_still_rule_without_reports
run_case spawned_agent_reports
run_case relaunch_mid_turn
run_case agent_hands_the_terminal_to_a_child
run_case argv_stays_out_of_app_stderr
run_case opencode_three_states
run_case opencode_first_turn_links
run_case opencode_duplicate_title_keeps_link
run_case opencode_short_turn_is_finished
run_case opencode_resumed_finish_is_not_replayed
run_case opencode_sibling_finish_is_not_replayed
run_case opencode_own_finish_links_late
run_case two_opencode_panes_one_folder
run_case opencode_failed_turn_is_error
run_case opencode_after_relaunch
run_case opencode_agent_tab_resumes
run_case opencode_unavailable_service_launches_plain
run_case opencode_v2_writes_no_plugin
run_case opencode_unknown_version_writes_no_plugin
run_case fresh_tab_drops_stale_ref
run_case background_opencode_notifies
run_case no_service_degrades
run_case background_tab_notifies
run_case showing_the_tab_adds_no_stop
run_case background_finish_at_relaunch_is_silent
run_case background_exit_mid_turn_is_one_stop
run_case control_session_gets_no_state
run_case other_worktree_notifies
run_case background_agent_tab_ends_mid_turn
run_case background_agent_tab_fails_mid_turn
run_case background_exec_agent_ends_mid_turn
run_case show_session_of_hidden_tab_is_one_stop
run_case show_session_on_screen_is_silent
run_case restart_agent_tab_mid_turn_is_silent
run_case restart_hand_typed_agent_tab_mid_turn_is_silent
run_case one_end_per_session
run_case close_hidden_worktree_mid_turn_is_silent
run_case background_exit_while_app_down_is_silent
run_case background_host_lost
run_case end_all_sessions_parity
run_case opencode_resumes_without_host

[ "$CASES_RUN" -gt 0 ] || { echo "FAIL: --only named no case" >&2; exit 1; }
if [ -n "$ONLY" ]; then label=PARTIAL; else label=OK; fi
echo "HAND AGENTS E2E $label ($CASES_RUN cases, $(( $(date +%s) - START_ALL ))s)"
