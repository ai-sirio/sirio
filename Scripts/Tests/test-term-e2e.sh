#!/bin/bash
set -eEuo pipefail

# The live end-to-end test of the host's terminal sessions (spec §9.2): a real
# `sirio-host`, held by a real client (`host_probe ensure --hold`, which calls
# the same `ensure_host` the app does), driven only through the `term.*`
# verbs a remote client would use.
#
# Why this exists on top of the engine's integration tests. The engine tests
# (`sirio_term_session/tests/engine.rs`) prove each property against a
# session in the same process; none of them can see what this script sees:
# whether a session outlives the client that attached to it, whether a
# stalled network client is detached with `lagged` while the session keeps
# printing, whether `term.close` really ends a backgrounded grandchild, or
# whether a host with no live session and no client really idles out. Those
# are properties of real processes on a real host, and a host that fails them
# fails silently: the app would simply show a dead tab.
#
# What it proves, one fresh data root per case, in this order (the numbering
# is spec §9.2 plus the review-focus cases):
#
#    1. survives-client-kill  a counter session outlives SIGKILL of the client
#                             attached to it: the counter keeps rising and the
#                             session counts no clients
#    2. resume                re-attach at the recorded offset → `resume`
#    3. snapshot-fidelity     re-attach outside the ring → `snapshot`, and a
#                             fresh client-side VT fed only the snapshot
#                             renders the same text as the host's VT (60
#                             coloured lines, bracketed paste on, scrollback
#                             beyond one screen)
#    4. query-one-reply       the engine test's DSR script, with no client
#                             attached, gets exactly one cursor-position reply
#    5. lagged                a client that never reads is detached with
#                             `lagged` while a 20 MB flood runs to `FLOOD-DONE`
#                             without blocking the session
#    6. close-kills-tree      `term.close` ends the shell and a backgrounded
#                             grandchild (on Windows, through the session's
#                             job object)
#    7. sirioctl-direct       with no app (the control socket path does not
#                             exist), `sirioctl panel` answers from the host:
#                             create, read, write, wait, list, state, close,
#                             SIRIO_PANE_ID read back from the session, and the
#                             session leaves term-list
#    8. exited-then-idle      a session that exits with no client is kept as
#                             exited, and with no live session and no client
#                             the host idles out
#    9. conformance           already run by `test-host-e2e.sh` case 8 — this
#                             script does not repeat it
#   10. flood                 a 50 MB flood with a client reading normally:
#                             the replica and the host's text agree afterwards
#   11. resize-detached       `term.resize` with no client attached takes, at
#                             both sizes, and the last screen shows `90x20`
#   12. alt-screen            attaching during the alternate screen resyncs
#                             once back to the primary screen when it is left
#   13. no-ctty               the host holds no controlling terminal while a
#                             session exists, and the session's shell holds one
#   14. echo-latency          keystroke echo latency through the host, measured
#                             only, never a FAIL
#   15. bounded-read           a `term.read` bigger than one response frame is
#                             answered, not dropped: `raw` is capped at 512 KiB,
#                             newest bytes first
#   16. wait-does-not-stall   a term.wait held open on one session does not stop
#                             another session's echo on the same connection
#
# Each case force-shuts every host in its own root and retires the root
# afterwards, and an exit trap does the same for whatever a failing or
# interrupted run left behind.
#
# Prints `CASE <n> <name> PASS|SKIP` per case and `TERM E2E OK` at the end.
#
# Debug builds only: the knobs it uses (SIRIO_HOST_IDLE_GRACE_MS,
# SIRIO_HOST_BIN) are compiled out of release builds.
#
# --out-dir DIR keeps every case's host logs and state files plus
# transcript.log, the probe calls and their answers, in DIR.
# SIRIO_TERM_E2E_VERBOSE=1 prints each probe call as it is made.
#
# Usage: Scripts/Tests/test-term-e2e.sh [--out-dir DIR]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) EXE=.exe; OS=windows ;;
  Darwin)               EXE=;     OS=macos ;;
  *)                    EXE=;     OS=linux ;;
esac

# Native binaries take native paths. Git Bash converts neither environment
# variables nor `key=value` arguments reliably, and a Rust PathBuf made from
# "/tmp/x" on Windows silently means C:\tmp\x. Shell-side checks keep using
# the shell's own paths (ROOT); what a native process reads is native().
native() {
  if [ "$OS" = windows ]; then cygpath -w "$1"; else printf '%s' "$1"; fi
}

# What this script itself starts that must not outlive it, and every data
# root it made. Declared before anything can fail so the trap can read them.
HELPERS=()
CASE_ROOTS=()
N=0
CASE_ID=""
CASE_NAME=""
NOTE=""
ROOT=""
RC=0
OUT=""
LAST_CALL=""
ROOT_BASE=""
TRANSCRIPT=/dev/null
NEW_ID=""
LAST_COUNTER=""

# ---------------------------------------------------------------- reporting

fail() {
  echo "CASE ${CASE_ID:-$N} $CASE_NAME FAIL: $*" >&2
  show_evidence >&2
  exit 1
}

unexpected() {
  echo "CASE ${CASE_ID:-$N} $CASE_NAME FAIL: unexpected error at line $1 (a command the script did not expect to fail)" >&2
  show_evidence >&2
}
trap 'unexpected $LINENO' ERR

# What a reader needs to see the failure without re-running: the last probe
# call and its answer, every host log of the current case, and the case's own
# small files (attach outputs, replicas, pid files).
show_evidence() {
  [ -n "$LAST_CALL" ] && { echo "--- last probe call: $LAST_CALL (exit $RC)"; printf '%s\n' "$OUT"; }
  local root f
  for root in ${CASE_ROOTS[@]+"${CASE_ROOTS[@]}"}; do
    [ -d "$root" ] || continue
    echo "--- root $root"
    ls -la "$root" 2>/dev/null | cut -c1-200
    for f in "$root"/log/*.log "$root"/host-v*.json; do
      [ -f "$f" ] || continue
      echo "--- $f"
      cat "$f"
    done
    for f in "$root"/*.out "$root"/*.err; do
      [ -f "$f" ] || continue
      echo "--- $f"
      cat "$f"
    done
    for f in "$root"/*.txt; do
      [ -f "$f" ] || continue
      echo "--- $f ($(wc -c <"$f" | tr -d ' ') bytes)"
      head -c 3000 "$f"
      echo
    done
  done
}

pass() {
  local took=$(( $(date +%s) - CASE_STARTED ))
  echo "CASE ${CASE_ID:-$N} $CASE_NAME PASS${NOTE:+ ($NOTE)} ${took}s"
}
skip() {
  echo "CASE ${CASE_ID:-$N} $CASE_NAME SKIP ($*)"
}

# ---------------------------------------------------------------- processes

# A process of the OS, by its native pid (the pids in state files and in the
# probe's answers). A zombie is not alive.
alive() {
  if [ "$OS" = windows ]; then
    # Native, the same check the client's verdict uses: tasklist piped into
    # grep read every live host as gone on the hosted runner.
    "$PROBE" alive "$1"
    return
  fi
  kill -0 "$1" 2>/dev/null || return 1
  if [ -r "/proc/$1/stat" ]; then
    local stat rest
    stat="$(cat "/proc/$1/stat" 2>/dev/null)" || return 1
    rest="${stat##*) }"
    [ "${rest%% *}" != Z ]
  fi
}
dead() { ! alive "$1"; }

# `host_probe alive` directly: the brief names it for the tree-kill checks.
# It needs no data root.
probe_alive() { "$PROBE" alive "$1" >/dev/null 2>&1; }
probe_dead() { ! probe_alive "$1"; }

# A child of this shell (a `&` job), by the shell's own pid.
job_running() {
  if [ "$OS" = windows ]; then kill -0 "$1" 2>/dev/null; else alive "$1"; fi
}

kill_hard() {
  if [ "$OS" = windows ]; then
    taskkill //F //PID "$1" >/dev/null 2>&1 || true
  else
    kill -9 "$1" 2>/dev/null || true
  fi
}

# Whether a pid is a sirio-host, for the guard before the cleanup signals
# anything: a cleanup never signals a process that is not one.
is_a_host() {
  local comm
  if [ "$OS" = windows ]; then
    "$PROBE" alive "$1" || return 1
    powershell.exe -NoProfile -NonInteractive -Command \
      "exit [int]((Get-Process -Id $1 -ErrorAction SilentlyContinue).ProcessName -ne 'sirio-host')" \
      >/dev/null 2>&1
    return
  fi
  if [ -r "/proc/$1/comm" ]; then comm="$(cat "/proc/$1/comm" 2>/dev/null)"
  else comm="$(ps -p "$1" -o comm= 2>/dev/null || true)"
  fi
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}

# wait_until SECONDS CMD ARGS...: polls CMD every 0.1 s; succeeds as soon as
# CMD does, and fails (with CMD's last answer) when the time is up.
wait_until() {
  local n=$(( $1 * 10 ))
  shift
  while [ "$n" -gt 0 ]; do
    if "$@"; then return 0; fi
    sleep 0.1
    n=$((n - 1))
  done
  "$@"
}
absent() { [ ! -e "$1" ]; }
has_line() { grep -q "$2" "$1" 2>/dev/null; }

# ---------------------------------------------------------------- the probe

# run_probe ARGS...: runs host_probe against the current root and records
# its stdout in OUT and its exit status in RC. Never fails the case on its
# own; `probe` is the one that does.
run_probe() {
  local errf="$ROOT_BASE/probe.err" outf="$ROOT_BASE/probe.out"
  RC=0
  # Into a file, not $(...): bash 3.2 runs the inherited ERR trap inside a
  # command substitution whose command fails, `|| RC=$?` outside or not.
  "$PROBE" "$@" >"$outf" 2>"$errf" || RC=$?
  OUT="$(tr -d '\r' <"$outf")"
  LAST_CALL="host_probe $* (SIRIO_HOST_PROTOCOL_MAJOR=${SIRIO_HOST_PROTOCOL_MAJOR:-unset})"
  {
    echo "[case $N $CASE_NAME] \$ $LAST_CALL -> exit $RC"
    printf '%s\n' "$OUT" | sed 's/^/    /'
    sed 's/^/    stderr: /' "$errf"
  } >>"$TRANSCRIPT"
  if [ -n "${SIRIO_TERM_E2E_VERBOSE:-}" ]; then
    echo "  $LAST_CALL -> exit $RC" >&2
    printf '%s\n' "$OUT" | sed 's/^/    /' >&2
  fi
  if [ "$RC" -ne 0 ] && [ -s "$errf" ]; then
    OUT="$OUT"$'\n'"stderr: $(head -c 600 "$errf")"
  fi
}
probe() {
  run_probe "$@"
  [ "$RC" -eq 0 ] || fail "host_probe $* exited $RC"
}
# A `key=value` line of the last probe's answer.
getf() { printf '%s\n' "$OUT" | sed -n "s/^$1=//p" | head -n 1; }

# A direct host_probe call whose stdout must reach a file byte-for-byte
# (`term-read text` for a later `cmp`: OUT has its carriage returns stripped).
# Logged to the transcript by hand.
log_direct() { echo "[case $N $CASE_NAME] direct: $*" >>"$TRANSCRIPT"; }

# ---------------------------------------------------------------- data roots

use_root() {
  ROOT="$1"
  CASE_ROOTS+=("$ROOT")
  export SIRIO_HOST_HOME
  SIRIO_HOST_HOME="$(native "$ROOT")"
}
# Every case gets a root of its own, named after its number. The default
# idle grace is long on purpose: a host that idled out between two steps of a
# case would be a flake of the test; only the cases about idling use a short
# one, and every host is force-shut by its case anyway.
fresh_root() {
  use_root "$ROOT_BASE/c$N"
  # The redirects of the background jobs below (`ensure --hold`, the stalled
  # and reading attaches) are created before the host makes the directory.
  mkdir -p "$ROOT"
  export SIRIO_HOST_IDLE_GRACE_MS="${1:-20000}"
}

# Force-shuts every host a root's state files name. With `mode` = cleanup it
# is for a failed or interrupted run: it also resumes a stopped host and, as
# the last resort, kills one that will not leave -- but only a process that is
# a sirio-host, and only one named by a state file in a root this script made.
# Otherwise a host that is still there afterwards is a failure of the case.
shutdown_hosts_in() { # ROOT [cleanup]
  local root="$1" mode="${2:-}" f major pid
  for f in "$root"/host-v*.json; do
    [ -f "$f" ] || continue
    major="${f##*host-v}"
    major="${major%.json}"
    pid="$(sed -n 's/.*"pid": *\([0-9][0-9]*\).*/\1/p' "$f" | head -n 1)"
    [ -n "$pid" ] || continue
    if [ "$mode" = cleanup ] && [ "$OS" != windows ] && is_a_host "$pid"; then
      kill -CONT "$pid" 2>/dev/null || true # a host the case left stopped
    fi
    SIRIO_HOST_HOME="$(native "$root")" SIRIO_HOST_PROTOCOL_MAJOR="$major" \
      "$PROBE" shutdown --force >/dev/null 2>&1 || true
    if ! wait_until 5 dead "$pid"; then
      if [ "$mode" = cleanup ]; then
        if is_a_host "$pid"; then kill_hard "$pid"; fi
      else
        fail "host $pid (major $major) did not leave after shutdown --force"
      fi
    fi
  done
}

# Keeps a root's logs and state files (not the staged binary) under --out-dir.
# Called before a root's hosts are shut down (a host that leaves removes its
# state file) and again after (the logs then hold its last lines); a copy
# never deletes, so the second call adds to the first.
archive_root() { # ROOT
  [ -n "$OUT_DIR" ] || return 0
  local root="$1" dest f
  [ -d "$root" ] || return 0
  dest="$OUT_DIR/roots/$(basename "$root" | cut -c1-40)"
  ( cd "$root" && find . -type f -not -path './bin/*' ) | while IFS= read -r f; do
    mkdir -p "$dest/$(dirname "$f")"
    # A leaving host may remove its state file between find and cp.
    cp "$root/$f" "$dest/$f" 2>/dev/null || true
  done
}

# The end of a passing case: every host of its roots leaves, the roots go,
# whatever the case started in the background is reaped.
end_case() {
  local root pid
  for pid in ${HELPERS[@]+"${HELPERS[@]}"}; do
    if job_running "$pid"; then kill_hard "$pid"; fi
  done
  HELPERS=()
  for root in ${CASE_ROOTS[@]+"${CASE_ROOTS[@]}"}; do
    archive_root "$root"
    shutdown_hosts_in "$root"
    archive_root "$root"
    rm -rf "$root"
  done
  CASE_ROOTS=()
}

run_case() { # NAME FUNCTION [DISPLAY_ID]
  N=$((N + 1))
  CASE_NAME="$1"
  CASE_ID="${3:-$N}"
  NOTE=""
  LAST_CALL=""
  CASE_STARTED="$(date +%s)"
  "$2"
  end_case
  CASE_ID=""
}

# The exit trap: nothing this script started survives it, a pass or a fail.
cleanup() {
  local status=$? root pid
  trap - EXIT INT TERM ERR
  set +e
  for pid in ${HELPERS[@]+"${HELPERS[@]}"}; do
    if job_running "$pid"; then kill_hard "$pid"; fi
  done
  if [ -n "$ROOT_BASE" ] && [ -d "$ROOT_BASE" ]; then
    for root in ${CASE_ROOTS[@]+"${CASE_ROOTS[@]}"}; do
      [ -d "$root" ] || continue
      archive_root "$root"
      shutdown_hosts_in "$root" cleanup
      archive_root "$root"
    done
    for root in "$ROOT_BASE"/*/; do
      [ -d "$root" ] || continue
      archive_root "${root%/}"
      shutdown_hosts_in "${root%/}" cleanup
      archive_root "${root%/}"
    done
    if [ -n "$OUT_DIR" ] && [ -f "$ROOT_BASE/transcript.log" ]; then
      cp "$ROOT_BASE/transcript.log" "$OUT_DIR/transcript.log"
    fi
    rm -rf "$ROOT_BASE"
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ---------------------------------------------------------------- build

TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/rust/target}"
( cd "$REPO_ROOT/rust" && cargo build -q -p sirio_host --bin sirio-host -p sirio_host_client --example host_probe -p sirio_control --bin sirioctl )
HOST_BIN="$TARGET_DIR/debug/sirio-host$EXE"
PROBE="$TARGET_DIR/debug/examples/host_probe$EXE"
SIRIOCTL="$TARGET_DIR/debug/sirioctl$EXE"
[ -x "$HOST_BIN" ] || { echo "FAIL: $HOST_BIN was not built" >&2; exit 1; }
[ -x "$PROBE" ] || { echo "FAIL: $PROBE was not built" >&2; exit 1; }
[ -x "$SIRIOCTL" ] || { echo "FAIL: $SIRIOCTL was not built" >&2; exit 1; }

# Nothing from the caller's environment may steer the hosts under test.
unset SIRIO_HOST_PROTOCOL_MAJOR SIRIO_HOST_HOME SIRIO_HOST_IDLE_GRACE_MS
export SIRIO_HOST_BIN
SIRIO_HOST_BIN="$(native "$HOST_BIN")"

# A short root: macOS's sun_path is 104 bytes.
ROOT_BASE="$(mktemp -d "${TMPDIR:-/tmp}/ste.XXXX")"
TRANSCRIPT="$ROOT_BASE/transcript.log"
: >"$TRANSCRIPT"
if [ -n "$OUT_DIR" ]; then mkdir -p "$OUT_DIR"; fi
RUN_STARTED="$(date +%s)"

# ---------------------------------------------------------------- terminal sessions

# Every case (except the one about idling) holds its host with a client that
# stays connected, so the host cannot idle out between two steps.
start_held_host() {
  "$PROBE" ensure --hold >"$ROOT/hold.out" 2>"$ROOT/hold.err" &
  HELPERS+=("$!")
  wait_until 20 has_line "$ROOT/hold.out" '^pid=' \
    || fail "the holding ensure never printed a pid: $(cat "$ROOT/hold.out" "$ROOT/hold.err" 2>/dev/null | head -c 600)"
}

# new_session ARGS...: `term-create` with the case root as the cwd and the
# rest as the command (`-- sh -c '...'`); the id lands in NEW_ID.
new_session() {
  probe term-create "$(native "$ROOT")" "$@"
  NEW_ID="$(printf '%s\n' "$OUT" | sed -n 's/^session=//p' | head -n 1)"
  [ -n "$NEW_ID" ] || fail "term-create printed no session id: $OUT"
}

# term_read_has ID FIXED_STRING: polls `term-read` until the text contains it.
term_read_has() {
  run_probe term-read "$1" text
  [ "$RC" -eq 0 ] || return 1
  printf '%s\n' "$OUT" | grep -qF "$2"
}

# session_line ID: prints the `term-list` line of the session.
session_line() {
  run_probe term-list
  [ "$RC" -eq 0 ] || return 1
  printf '%s\n' "$OUT" | grep "^session=$1 " | head -n 1
}

# session_has_no_clients ID: for the poll after a client is killed.
session_has_no_clients() {
  local line
  line="$(session_line "$1" || true)" || return 1
  [ -n "$line" ] || return 1
  case "$line" in *clients=0*) return 0 ;; *) return 1 ;; esac
}

# read_last_counter ID: the last `n=` of a counter session, in LAST_COUNTER.
read_last_counter() {
  run_probe term-read "$1" text
  [ "$RC" -eq 0 ] || return 1
  if LAST_COUNTER="$(printf '%s\n' "$OUT" | grep -o 'n=[0-9][0-9]*' | tail -n 1 | sed 's/^n=//')"; then
    [ -n "$LAST_COUNTER" ]
  else
    return 1
  fi
}

case_survives_client_kill() {
  fresh_root
  start_held_host
  new_session -- sh -c 'i=0; while :; do echo n=$i; i=$((i+1)); sleep 0.1; done'
  local id="$NEW_ID" victim a b
  "$PROBE" term-attach "$id" --for 60000 >"$ROOT/victim.out" 2>"$ROOT/victim.err" &
  victim=$!
  HELPERS+=("$victim")
  log_direct "term-attach $id --for 60000 & (victim client, pid $victim)"
  sleep 1
  # kill -9 of the attached client (kill_hard is taskkill on Windows).
  kill_hard "$victim"
  wait "$victim" 2>/dev/null || true
  wait_until 10 read_last_counter "$id" || fail "the counter never printed"
  a="$LAST_COUNTER"
  sleep 1
  read_last_counter "$id" || fail "could not read the counter a second time"
  b="$LAST_COUNTER"
  [ "$b" -gt "$a" ] || fail "the counter stopped with its client ($a, then $b)"
  wait_until 5 session_has_no_clients "$id" \
    || fail "term-list still counts the killed client: $OUT"
  NOTE="n=$a->$b"
  pass
}

case_resume() {
  fresh_root
  start_held_host
  new_session -- sh -c 'i=0; while :; do echo n=$i; i=$((i+1)); sleep 0.1; done'
  local id="$NEW_ID" off
  probe term-attach "$id" --for 1000 --last-offset "$(native "$ROOT/off.txt")"
  [ "$(getf mode)" = snapshot ] || fail "the first attach is '$(getf mode)', not snapshot"
  off="$(cat "$ROOT/off.txt")"
  [ -n "$off" ] || fail "the first attach recorded no offset"
  probe term-attach "$id" --resume "$off" --for 500
  [ "$(getf mode)" = resume ] || fail "re-attach at the recorded offset is '$(getf mode)', not resume"
  pass
}

case_snapshot_fidelity() {
  fresh_root
  start_held_host
  # The engine test's script, with a shell loop instead of `seq`: 60 coloured
  # lines (more than one screen of scrollback), bracketed paste on, then sleep.
  new_session -- sh -c 'i=1; while [ $i -le 60 ]; do printf "\033[3%dmline %d\033[0m\n" $((i % 7)) $i; i=$((i+1)); done; printf "\033[?2004h"; sleep 60'
  local id="$NEW_ID"
  wait_until 10 term_read_has "$id" "line 60" || fail "the 60 lines never reached the host VT"
  probe term-attach "$id" --resume 999999999999 --for 1500 --replica-text "$(native "$ROOT/r.txt")"
  [ "$(getf mode)" = snapshot ] || fail "an offset outside the ring attached as '$(getf mode)', not snapshot"
  "$PROBE" term-read "$id" text >"$ROOT/h.txt" 2>"$ROOT/h-read.err" \
    || fail "term-read failed after the snapshot"
  log_direct "term-read $id text -> h.txt"
  cmp "$ROOT/r.txt" "$ROOT/h.txt" \
    || fail "the snapshot replica and the host text differ"
  pass
}

case_query_one_reply() {
  fresh_root
  start_held_host
  # The engine test's DSR script (`a_terminal_query_with_no_client_gets_exactly_one_reply`),
  # with its single quotes traded for double quotes so the whole script stays one
  # bash word: ask for a cursor-position report and read the one reply from the tty.
  new_session -- sh -c 'stty -echo raw; printf "\033[6n"; r=$(dd bs=1 < /dev/tty 2>/dev/null & reader=$!; sleep 1; kill "$reader" 2>/dev/null; wait "$reader" 2>/dev/null); stty sane; printf "reply=%s\n" "$(printf "%s" "$r" | od -An -c | tr -d " \n")"; sleep 5'
  local id="$NEW_ID" count
  wait_until 10 term_read_has "$id" "reply=" || fail "the DSR reply line never appeared"
  run_probe term-read "$id" text
  count="$(printf '%s\n' "$OUT" | grep -o '033\[' | wc -l | tr -d ' ' || true)"
  [ "$count" = 1 ] || fail "expected exactly one cursor-position report, saw $count"
  pass
}

case_lagged() {
  fresh_root
  start_held_host
  new_session -- sh -c 'head -c 20000000 /dev/zero | tr "\0" x; echo; echo FLOOD-DONE; sleep 60'
  local id="$NEW_ID" stalled
  "$PROBE" term-attach "$id" --no-read --for 20000 >"$ROOT/stalled.out" 2>"$ROOT/stalled.err" &
  stalled=$!
  HELPERS+=("$stalled")
  log_direct "term-attach $id --no-read --for 20000 & (stalled client, pid $stalled)"
  wait_until 60 term_read_has "$id" "FLOOD-DONE" \
    || fail "the flood never finished: the session blocked on its stalled client"
  wait "$stalled" || fail "the stalled attach failed"
  grep -q '^lagged=1$' "$ROOT/stalled.out" \
    || fail "the stalled client never lagged: $(cat "$ROOT/stalled.out" "$ROOT/stalled.err" 2>/dev/null | head -c 600)"
  { echo "--- stalled attach.out"; cat "$ROOT/stalled.out"; } >>"$TRANSCRIPT"
  NOTE="the session outran its stalled client"
  pass
}

case_close_kills_tree() {
  fresh_root
  start_held_host
  local id="$NEW_ID" line shell gc
  if [ "$OS" = windows ]; then
    # A shell with a child (start /b) and a foreground grandchild-equivalent:
    # closing the session must take the whole tree through the job object.
    # The check below looks for any PING.EXE that was ours — parented to the
    # session's shell, or carrying our command line — rather than trusting one
    # pid, which the job may already have recycled.
    new_session -- cmd /c "start /b ping -n 300 127.0.0.1 > nul & ping -n 300 127.0.0.1 > nul"
    id="$NEW_ID"
    sleep 3 # let both pings start
  else
    new_session -- sh -c 'sleep 300 & echo $! > gc.pid; wait'
    id="$NEW_ID"
    wait_until 5 has_line "$ROOT/gc.pid" '[0-9]' \
      || fail "the grandchild pid file never appeared"
    gc="$(tr -d ' \r\n\t' <"$ROOT/gc.pid")"
    [ -n "$gc" ] || fail "the grandchild pid file is empty"
  fi
  run_probe term-list
  line="$(printf '%s\n' "$OUT" | grep "^session=$id " | head -n 1 || true)"
  shell="$(printf '%s\n' "$line" | sed -n 's/.* pid=\([0-9][0-9]*\).*/\1/p')"
  [ -n "$shell" ] || fail "term-list names no shell pid for $id: $OUT"
  probe term-close "$id"
  if [ "$OS" = windows ]; then
    wait_until 8 no_session_ping "$shell" \
      || fail "a PING.EXE of the session survived term-close"
  else
    wait_until 3 probe_dead "$shell" \
      || fail "the shell $shell survived term-close"
    wait_until 3 probe_dead "$gc" \
      || fail "the grandchild $gc survived term-close"
  fi
  pass
}
# No PING.EXE parented to the session's shell, and none carrying our command
# line (both the start /b child and the foreground ping have it).
no_session_ping() {
  local out
  out="$(powershell.exe -NoProfile -NonInteractive -Command \
    "Get-CimInstance Win32_Process | Where-Object { \$_.Name -eq 'PING.EXE' -and (\$_.ParentProcessId -eq $1 -or \$_.CommandLine -like '*300 127.0.0.1*') } | Select-Object -ExpandProperty ProcessId" 2>/dev/null || true)"
  [ -z "$(printf '%s' "$out" | tr -d ' \r\n\t')" ]
}

# sirioctl with the app down: the socket path does not exist, so every
# `panel` verb is answered by the host of this case's root.
sirioctl_panel() {
  SIRIO_SOCKET="$(native "$ROOT/none.sock")" "$SIRIOCTL" panel "$@"
}
# The base64 `panel read` answers, decoded to stdout.
decode_base64() {
  base64 -d 2>/dev/null || base64 -D
}
session_listed() { # ID: term-list still names it
  run_probe term-list
  [ "$RC" -eq 0 ] && printf '%s\n' "$OUT" | grep -q "^session=$1 "
}
session_gone() { ! session_listed "$1"; }

case_sirioctl_direct() {
  fresh_root
  start_held_host
  local id text worker keyed_cmd
  # The command runs through the platform's shell: cmd.exe on Windows has no
  # `sleep` and reads variables as %NAME%.
  if [ "$OS" = windows ]; then
    worker='echo worker-ok & ping -n 60 127.0.0.1 > nul'
    keyed_cmd='echo id=%SIRIO_PANE_ID% & ping -n 60 127.0.0.1 > nul'
  else
    worker='echo worker-ok; sleep 60'
    keyed_cmd='echo id=$SIRIO_PANE_ID; sleep 60'
  fi
  id="$(sirioctl_panel create --cwd "$(native "$ROOT")" --cmd "$worker")" \
    || fail "sirioctl panel create failed with no app"
  case "$id" in pane-c[0-9a-f]*) ;; *) fail "panel create printed no host id: $id" ;; esac
  text=""
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
    text="$(sirioctl_panel read "$id" | decode_base64)" || fail "sirioctl panel read failed"
    case "$text" in *worker-ok*) break ;; esac
    sleep 0.1
  done
  case "$text" in *worker-ok*) ;; *) fail "panel read never showed worker-ok" ;; esac
  sirioctl_panel write "$id" --input 'x' || fail "sirioctl panel write failed"
  local waited
  if waited="$(sirioctl_panel wait "$id" --timeout-ms 200 2>&1)"; then
    fail "panel wait reported an exit for a session still sleeping: $waited"
  fi
  case "$waited" in *"timed out"*) ;; *) fail "panel wait did not time out: $waited" ;; esac
  # `panel list` names the session with an empty tab and agent (the host has
  # neither: the app adds them when it answers).
  local listed
  listed="$(sirioctl_panel list)" || fail "sirioctl panel list failed"
  printf '%s\n' "$listed" | awk -F'\t' -v id="$id" \
    '$1 == id && $2 == "" && $4 == "" { found = 1 } END { exit !found }' \
    || fail "panel list does not name $id with an empty tab and agent: $listed"
  # `panel state` on the host route: alive, and it says the host keeps it.
  local state
  state="$(sirioctl_panel state "$id" --json)" || fail "sirioctl panel state failed"
  case "$state" in *'"survives":"true"'*) ;; *) fail "panel state lacks survives=true: $state" ;; esac
  case "$state" in *'"exitStatus":"running"'*) ;; *) fail "panel state says the session is not running: $state" ;; esac
  # `SIRIO_PANE_ID` is the session's id, read back from inside the session (spec §3.1).
  local keyed
  keyed="$(sirioctl_panel create --cwd "$(native "$ROOT")" --cmd "$keyed_cmd")" \
    || fail "sirioctl panel create (SIRIO_PANE_ID) failed"
  text=""
  for _ in $(seq 1 60); do
    text="$(sirioctl_panel read "$keyed" | decode_base64)" || fail "sirioctl panel read failed"
    case "$text" in *"id=$keyed"*) break ;; esac
    sleep 0.1
  done
  case "$text" in *"id=$keyed"*) ;; *) fail "SIRIO_PANE_ID did not read back as $keyed: $text" ;; esac
  # Show session needs the app: with none, the host route says so.
  local shown
  if shown="$(sirioctl_panel show "$id" 2>&1)"; then
    fail "panel show succeeded with no app: $shown"
  fi
  case "$shown" in *"Show session needs Sirio running"*) ;; *) fail "panel show said: $shown" ;; esac
  sirioctl_panel close "$keyed" || fail "sirioctl panel close (keyed) failed"
  sirioctl_panel close "$id" || fail "sirioctl panel close failed"
  wait_until 5 session_gone "$id" || fail "term-list still lists $id after close: $OUT"
  NOTE="id=$id"
  pass
}

case_exited_then_idle() {
  # No --hold here on purpose: the host must idle out on its own once its
  # last session has exited and its last client has left.
  fresh_root 1500
  probe ensure
  new_session -- sh -c 'echo bye; exit 4'
  local id="$NEW_ID" line
  probe term-wait "$id" 5000
  [ "$(getf exit)" = "code:4" ] || fail "term-wait answered '$(getf exit)', not exit=code:4"
  run_probe term-list
  line="$(printf '%s\n' "$OUT" | grep "^session=$id " | head -n 1 || true)"
  case "$line" in
    *exit=code:4*) ;;
    *) fail "term-list lost the exited session: $OUT" ;;
  esac
  # The state file, not `observe`: every `observe` connects, and a connection
  # is activity — polling it would keep the host alive on its own.
  wait_until 6 absent "$ROOT/host-v1.json" \
    || fail "the state file is still there 6 s after the last session exited (grace 1.5 s)"
  if [ "$OS" != windows ]; then
    [ ! -e "$ROOT/host-v1.sock" ] || fail "the endpoint is still there after the host left"
  fi
  run_probe observe 1
  [ "$(getf verdict)" = Absent ] || fail "observe after the idle exit is '$(getf verdict)', not Absent"
  pass
}

case_flood() {
  fresh_root
  start_held_host
  # The attach lands while 50 MB are flowing, so the snapshot is taken
  # mid-flow: the replica is rebuilt from it and then reads normally to the
  # end of the flood. Afterwards it must agree byte-for-byte with the host's
  # text; `lagged` may be >= 0 either way and is recorded, not asserted.
  # ConPTY passes the flood on far slower (50 MB did not finish in 270 s on a
  # GitHub runner), so Windows floods 8 MB: still twice the 4 MiB journal,
  # which is what the case needs to outrun.
  local bytes=50000000 secs=90
  [ "$OS" = windows ] && { bytes=8000000; secs=240; }
  new_session -- sh -c "yes | head -c $bytes; echo FLOOD-DONE; sleep 60"
  local id="$NEW_ID" reader lagged
  "$PROBE" term-attach "$id" --for "$((secs * 1000))" --replica-text "$(native "$ROOT/r.txt")" \
    >"$ROOT/attach.out" 2>"$ROOT/attach.err" &
  reader=$!
  HELPERS+=("$reader")
  log_direct "term-attach $id --for $((secs * 1000)) --replica-text r.txt & (reader, pid $reader)"
  wait_until "$secs" term_read_has "$id" "FLOOD-DONE" \
    || fail "the $bytes-byte flood never finished"
  sleep 1
  wait "$reader" || fail "the reading attach failed"
  { echo "--- reader attach.out"; cat "$ROOT/attach.out"; } >>"$TRANSCRIPT"
  "$PROBE" term-read "$id" text >"$ROOT/h.txt" 2>"$ROOT/h-read.err" \
    || fail "term-read failed after the flood"
  log_direct "term-read $id text -> h.txt"
  cmp "$ROOT/r.txt" "$ROOT/h.txt" \
    || fail "the replica and the host text differ after the flood"
  lagged="$(sed -n 's/^lagged=//p' "$ROOT/attach.out" | head -n 1)"
  NOTE="lagged=${lagged:-?}, measurement only"
  pass
}

# attach_read_cmp REPLICA HOST_TEXT: one attach plus the host text right
# after, passing when the two agree. Retried: the loop paints every 0.2 s, so
# a frame landing between the attach and the read is not a defect.
attach_read_cmp() {
  local i
  for i in 1 2 3 4 5 6 7 8 9 10; do
    probe term-attach "$ATTACH_ID" --for 800 --replica-text "$(native "$ROOT/$1")"
    "$PROBE" term-read "$ATTACH_ID" text >"$ROOT/$2" 2>"$ROOT/read-$i.err" || return 1
    log_direct "term-attach $ATTACH_ID --for 800 (try $i) + term-read -> $2"
    if cmp -s "$ROOT/$1" "$ROOT/$2"; then return 0; fi
    sleep 0.2
  done
  return 1
}

case_resize_detached() {
  fresh_root
  start_held_host
  new_session -- sh -c 'while :; do printf "\033[H\033[2J%sx%s\n" $(tput cols) $(tput lines); sleep 0.2; done'
  local id="$NEW_ID"
  ATTACH_ID="$id"
  probe term-resize "$id" 100 30
  sleep 0.5
  attach_read_cmp r.txt h.txt \
    || fail "no attach agreed with the host text at 100x30"
  probe term-resize "$id" 90 20
  sleep 0.5
  attach_read_cmp r2.txt h2.txt \
    || fail "no attach agreed with the host text at 90x20"
  grep -qF '90x20' "$ROOT/r2.txt" \
    || fail "the resized replica shows no 90x20"
  pass
}
ATTACH_ID=""

case_alt_screen() {
  fresh_root
  start_held_host
  # The engine test's script (`alternate_screen_exit_resyncs_a_stream_that_attached_during_it`).
  new_session -- sh -c 'echo PRIMARY-HISTORY; sleep 0.5; printf "\033[?1049hALT-SCREEN"; sleep 1.5; printf "\033[?1049l"; echo BACK; sleep 30'
  local id="$NEW_ID"
  wait_until 5 term_read_has "$id" "ALT-SCREEN" \
    || fail "the session never entered its alternate screen"
  probe term-attach "$id" --for 4000 --replica-text "$(native "$ROOT/r.txt")"
  [ "$(getf resyncs)" = 1 ] \
    || fail "expected one resync when the alternate screen was left, saw '$(getf resyncs)'"
  grep -qF 'PRIMARY-HISTORY' "$ROOT/r.txt" \
    || fail "the replica lost PRIMARY-HISTORY"
  grep -qF 'BACK' "$ROOT/r.txt" \
    || fail "the replica never saw BACK"
  "$PROBE" term-read "$id" text >"$ROOT/h.txt" 2>"$ROOT/h-read.err" \
    || fail "term-read failed after the resync"
  log_direct "term-read $id text -> h.txt"
  cmp "$ROOT/r.txt" "$ROOT/h.txt" \
    || fail "the replica and the host text differ after the resync"
  pass
}

case_no_ctty() {
  if [ "$OS" = windows ]; then skip "unix only"; return 0; fi
  fresh_root
  start_held_host
  # A session the way case 1 has one: every case runs its own host (end_case
  # shuts it down), so this recreates case 1's session for the host-side check.
  new_session -- sh -c 'i=0; while :; do echo n=$i; i=$((i+1)); sleep 0.1; done'
  probe info
  local hostpid
  hostpid="$(getf pid)"
  [ -n "$hostpid" ] || fail "info printed no pid"
  if [ "$OS" = linux ]; then
    [ "$(awk '{print $7}' /proc/"$hostpid"/stat)" = 0 ] \
      || fail "the host holds a controlling terminal"
  else
    [ "$(ps -o tty= -p "$hostpid" | tr -d ' ')" = "??" ] \
      || fail "the host holds a controlling terminal"
  fi
  # ...while the session's own shell does: a shell that evaluates what it is
  # sent, so `tty` runs on the pty and names it.
  new_session -- sh -c 'while read -r line; do eval "$line"; done'
  local shid="$NEW_ID" want
  probe term-write "$shid" 'tty\r'
  if [ "$OS" = linux ]; then want="/dev/pts/"; else want="/dev/ttys"; fi
  wait_until 10 term_read_has "$shid" "$want" \
    || fail "the session shell has no controlling terminal of its own"
  pass
}

case_echo_latency() {
  fresh_root
  start_held_host
  new_session -- cat
  local id="$NEW_ID"
  probe term-echo-latency "$id" 50
  # Measurement only, never a FAIL: the values go to the transcript (via
  # run_probe) and to this case's summary.
  NOTE="p50=$(getf p50_ms)ms p95=$(getf p95_ms)ms max=$(getf max_ms)ms"
  pass
}

case_bounded_read() {
  fresh_root
  start_held_host
  # A read bigger than one response frame (1 MiB payload, spec §6.1) must be
  # answered, not dropped: `raw` is capped at 512 KiB (base64 ≈ 683 KiB) of the
  # newest bytes, so this returns at once instead of timing out.
  new_session -- sh -c 'head -c 3000000 /dev/zero | tr "\0" y; echo READY-MARKER; sleep 30'
  local id="$NEW_ID" started took size
  wait_until 30 term_read_has "$id" "READY-MARKER" \
    || fail "the 3 MB flood never finished"
  started="$(date +%s)"
  "$PROBE" term-read "$id" raw >"$ROOT/capped.bin" 2>"$ROOT/capped.err" \
    || fail "term-read raw of a 3 MB journal failed"
  log_direct "term-read $id raw -> capped.bin"
  took=$(( $(date +%s) - started ))
  size="$(wc -c <"$ROOT/capped.bin" | tr -d ' ')"
  [ "$took" -le 2 ] || fail "term-read raw took ${took}s (it must answer within 2 s)"
  [ "$size" -le 524288 ] || fail "term-read raw returned $size bytes (cap 512 KiB)"
  NOTE="$size bytes in ${took}s"
  pass
}

# Finding 2 (one slow host request must not stall every terminal): a term.wait
# held open on one session must not stop another session's echo on the same
# connection. The probe measures the echo while the wait is still pending.
case_wait_does_not_stall() {
  fresh_root
  start_held_host
  new_session --origin control -- sh -c 'sleep 60'
  local waiter="$NEW_ID"
  new_session -- cat
  local echo_id="$NEW_ID"
  probe term-wait-echo "$waiter" "$echo_id"
  [ "$(getf echo_seen)" = true ] || fail "the echo never arrived while term.wait was pending (echo_ms=$(getf echo_ms))"
  [ "$(getf echo_ms)" -le 2000 ] || fail "the echo took $(getf echo_ms) ms while term.wait was pending (limit 2000 ms)"
  [ "$(getf wait_running)" = true ] || fail "the term.wait ended before the echo was measured"
  NOTE="echo $(getf echo_ms) ms with a term.wait pending on another session"
  pass
}

run_case survives-client-kill case_survives_client_kill
run_case resume               case_resume
run_case snapshot-fidelity    case_snapshot_fidelity
run_case query-one-reply      case_query_one_reply
run_case lagged               case_lagged
run_case close-kills-tree     case_close_kills_tree
run_case sirioctl-direct      case_sirioctl_direct      7
run_case exited-then-idle     case_exited_then_idle     8
run_case flood                case_flood                10
run_case resize-detached      case_resize_detached      11
run_case alt-screen           case_alt_screen           12
run_case no-ctty              case_no_ctty              13
run_case echo-latency         case_echo_latency         14
run_case bounded-read           case_bounded_read           15
run_case wait-does-not-stall    case_wait_does_not_stall    16

echo "TERM E2E OK ($N cases, $(( $(date +%s) - RUN_STARTED ))s)"
