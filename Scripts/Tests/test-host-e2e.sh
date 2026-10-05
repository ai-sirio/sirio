#!/bin/bash
set -eEuo pipefail

# The live end-to-end test of the host foundation (spec §9.2): a real
# `sirio-host`, started detached by a real client (`host_probe`, which calls
# the same `ensure_host` the app does), observed only through what it leaves
# on disk, what it answers on its endpoint and which processes are alive.
#
# Why this exists on top of the unit tests. Every pure part of the host --
# the frame codec, the verdict table, the state file, the idle decision, the
# detach arms -- has its own test, and none of them can see what this script
# sees: whether a host started by one process really outlives it, whether two
# clients that start at once really end up with one host, whether a recorded
# pid that is now somebody else's process is really left alone. Those are
# properties of a real process tree on a real filesystem, and a host that
# fails them fails silently: the app would simply show "Not available".
#
# What it proves, one fresh data root per case, in this order:
#
#    1. start-and-adopt   a client starts a host; the next one adopts it (same
#                         pid, no second start)
#    2. concurrent-start  four clients at once: one host, one `start.ready`
#    3. client-killed     SIGKILL of a client -- and, on Linux, stopping the
#                         client's systemd scope -- leaves the host serving
#    4. idle-exit         with no client and no session the host leaves, in
#                         the order a client can trust, and reads as Absent
#    5. recycled-pid      a state file naming a live process that is not the
#                         host (same pid, other start time) reads as Absent,
#                         a new host is started, and nobody signals the pid
#    6. no-steal          a second host on a held lock exits 3 and the first
#                         keeps serving
#    7. drain             a v2 host starts beside a v1 host with a live
#                         session; v1 drains by itself when its session ends;
#                         an older app gets a v1 host beside the v2 one
#    8. conformance       protocol/host-v1/conformance/ against the live host
#    9. sessions-live     host.shutdown refuses while a session is live, and
#                         `force` ends it
#   10. unverifiable      a stopped (silent) host holding its lock is never
#                         replaced: Unverifiable after the wait, one host
#   11. path-too-long     an endpoint path over sun_path is named in the log
#   12. foreign-client    the control socket's NDJSON is closed on, unanswered
#
# Cases 10 and 11 are unix-only and print SKIP on Windows. The systemd scope
# arm of case 3 runs only on Linux with a user manager.
#
# Each case force-shuts every host in its own root and retires the root
# afterwards (a staged host binary is 7 MB; a dozen of them have no business
# staying in /tmp), and an exit trap does the same for whatever a failing or
# interrupted run left behind.
#
# Prints `CASE <n> <name> PASS|SKIP` per case and `HOST E2E OK` at the end.
#
# Debug builds only: the knobs it uses (SIRIO_HOST_IDLE_GRACE_MS,
# SIRIO_HOST_PROTOCOL_MAJOR, SIRIO_HOST_BIN, host.debug.hold_session) are
# compiled out of release builds.
#
# --out-dir DIR keeps every case's host logs and state files (not the staged
# binaries) plus transcript.log, the probe calls and their answers, in DIR.
# SIRIO_HOST_E2E_VERBOSE=1 prints each probe call as it is made.
#
# Usage: Scripts/Tests/test-host-e2e.sh [--out-dir DIR]

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
CASE_NAME=""
NOTE=""
ROOT=""
RC=0
OUT=""
LAST_CALL=""
ROOT_BASE=""
TRANSCRIPT=/dev/null

# ---------------------------------------------------------------- reporting

fail() {
  echo "CASE $N $CASE_NAME FAIL: $*" >&2
  show_evidence >&2
  exit 1
}

unexpected() {
  echo "CASE $N $CASE_NAME FAIL: unexpected error at line $1 (a command the script did not expect to fail)" >&2
  show_evidence >&2
}
trap 'unexpected $LINENO' ERR

# What a reader needs to see the failure without re-running: the last probe
# call and its answer, then every host log of the current case.
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
  done
}

pass() {
  local took=$(( $(date +%s) - CASE_STARTED ))
  echo "CASE $N $CASE_NAME PASS${NOTE:+ ($NOTE)} ${took}s"
}
skip() {
  echo "CASE $N $CASE_NAME SKIP ($*)"
}

# ---------------------------------------------------------------- processes

# A process of the OS, by its native pid (the pids in state files and in the
# probe's answers). A zombie is not alive.
alive() {
  if [ "$OS" = windows ]; then
    tasklist //FI "PID eq $1" //NH 2>/dev/null | grep -qw "$1"
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

# A child of this shell (a `&` job), by the shell's own pid.
job_running() {
  if [ "$OS" = windows ]; then kill -0 "$1" 2>/dev/null; else alive "$1"; fi
}

# The pid the OS knows a shell job by. Git Bash numbers its processes itself.
native_pid() {
  if [ "$OS" = windows ] && [ -r "/proc/$1/winpid" ]; then cat "/proc/$1/winpid"; else printf '%s' "$1"; fi
}

kill_hard() {
  if [ "$OS" = windows ]; then
    taskkill //F //PID "$(native_pid "$1")" >/dev/null 2>&1 || true
  else
    kill -9 "$1" 2>/dev/null || true
  fi
}

# Whether a pid is a sirio-host, for the guard before the cleanup signals
# anything: a cleanup never signals a process that is not one.
is_a_host() {
  local comm
  if [ "$OS" = windows ]; then
    tasklist //FI "PID eq $1" //FI "IMAGENAME eq sirio-host.exe" //NH 2>/dev/null | grep -qi sirio-host
    return
  fi
  if [ -r "/proc/$1/comm" ]; then comm="$(cat "/proc/$1/comm" 2>/dev/null)"
  else comm="$(ps -p "$1" -o comm= 2>/dev/null || true)"
  fi
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}

# `/proc/<pid>/stat` field 22, the same number the host writes to its state
# file on Linux.
proc_start_time() {
  local stat rest
  local -a fields
  stat="$(cat "/proc/$1/stat")"
  rest="${stat##*) }"
  read -ra fields <<<"$rest"
  printf '%s' "${fields[19]}"
}

# sirio-host processes whose working directory is the root (a detached host
# is started with the data root as its cwd). Linux only: macOS and Windows
# have no /proc to ask, and callers rely on the log there.
count_hosts_for_root() {
  local real d n=0
  real="$(cd "$1" && pwd -P)"
  for d in /proc/[0-9]*; do
    [ -r "$d/comm" ] || continue
    [ "$(cat "$d/comm" 2>/dev/null || true)" = sirio-host ] || continue
    [ "$(readlink "$d/cwd" 2>/dev/null || true)" = "$real" ] && n=$((n + 1))
  done
  echo "$n"
}
exactly_one_host_for_root() { [ "$(count_hosts_for_root "$1")" = 1 ]; }

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
  local errf="$ROOT_BASE/probe.err"
  RC=0
  OUT="$("$PROBE" "$@" 2>"$errf")" || RC=$?
  OUT="$(printf '%s' "$OUT" | tr -d '\r')"
  LAST_CALL="host_probe $* (SIRIO_HOST_PROTOCOL_MAJOR=${SIRIO_HOST_PROTOCOL_MAJOR:-unset})"
  {
    echo "[case $N $CASE_NAME] \$ $LAST_CALL -> exit $RC"
    printf '%s\n' "$OUT" | sed 's/^/    /'
    sed 's/^/    stderr: /' "$errf"
  } >>"$TRANSCRIPT"
  if [ -n "${SIRIO_HOST_E2E_VERBOSE:-}" ]; then
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
# The same, as the client of protocol major $1.
probe_as() {
  local major="$1"
  shift
  SIRIO_HOST_PROTOCOL_MAJOR="$major" run_probe "$@"
}
probe_ok_as() {
  local major="$1"
  shift
  probe_as "$major" "$@"
  [ "$RC" -eq 0 ] || fail "host_probe $* (major $major) exited $RC"
}
# A `key=value` line of the last probe's answer.
getf() { printf '%s\n' "$OUT" | sed -n "s/^$1=//p" | head -n 1; }

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
  export SIRIO_HOST_IDLE_GRACE_MS="${1:-20000}"
}

state_pid() { sed -n 's/.*"pid": *\([0-9][0-9]*\).*/\1/p' "$ROOT/host-v$1.json" 2>/dev/null | head -n 1; }

# The host log's `start.ready`, `client.refused`... count for a major.
count_event() {
  local f="$ROOT/log/host-v$1.log"
  if [ -f "$f" ]; then awk -F'\t' -v e="$2" '$3 == e { n++ } END { print n + 0 }' "$f"; else echo 0; fi
}

# A state file for a process that is not a host.
write_fake_state() { # ROOT PID START_TIME
  mkdir -p "$1"
  printf '{\n  "pid": %s,\n  "start_time": %s,\n  "version": "0.0.0",\n  "protocol": { "major": 1, "minor": 0 },\n  "mode": "on_demand",\n  "endpoint": "gone",\n  "generation": "not-a-host"\n}\n' \
    "$2" "$3" >"$1/host-v1.json"
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
archive_root() { # ROOT
  [ -n "$OUT_DIR" ] || return 0
  local root="$1" dest f
  [ -d "$root" ] || return 0
  dest="$OUT_DIR/roots/$(basename "$root" | cut -c1-40)"
  ( cd "$root" && find . -type f -not -path './bin/*' ) | while IFS= read -r f; do
    mkdir -p "$dest/$(dirname "$f")"
    cp "$root/$f" "$dest/$f"
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
    shutdown_hosts_in "$root"
    archive_root "$root"
    rm -rf "$root"
  done
  CASE_ROOTS=()
}

run_case() { # NAME FUNCTION
  N=$((N + 1))
  CASE_NAME="$1"
  NOTE=""
  LAST_CALL=""
  CASE_STARTED="$(date +%s)"
  "$2"
  end_case
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
    for root in "$ROOT_BASE"/*/; do
      [ -d "$root" ] || continue
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
( cd "$REPO_ROOT/rust" && cargo build -q -p sirio_host -p sirio_host_client --examples )
HOST_BIN="$TARGET_DIR/debug/sirio-host$EXE"
PROBE="$TARGET_DIR/debug/examples/host_probe$EXE"
[ -x "$HOST_BIN" ] || { echo "FAIL: $HOST_BIN was not built" >&2; exit 1; }
[ -x "$PROBE" ] || { echo "FAIL: $PROBE was not built" >&2; exit 1; }

# Nothing from the caller's environment may steer the hosts under test.
unset SIRIO_HOST_PROTOCOL_MAJOR SIRIO_HOST_HOME SIRIO_HOST_IDLE_GRACE_MS
export SIRIO_HOST_BIN
SIRIO_HOST_BIN="$(native "$HOST_BIN")"

# A short root: macOS's sun_path is 104 bytes.
ROOT_BASE="$(mktemp -d "${TMPDIR:-/tmp}/she.XXXX")"
TRANSCRIPT="$ROOT_BASE/transcript.log"
: >"$TRANSCRIPT"
if [ -n "$OUT_DIR" ]; then mkdir -p "$OUT_DIR"; fi
RUN_STARTED="$(date +%s)"

# ---------------------------------------------------------------- the cases

case_start_and_adopt() {
  fresh_root
  probe ensure
  local pid1 method1 pid2
  pid1="$(getf pid)"; method1="$(getf method)"
  [ -n "$pid1" ] || fail "the first ensure printed no pid"
  case "$method1" in Some\(*) ;; *) fail "the first ensure did not start a host (method=$method1)" ;; esac
  [ "$(getf major)" = 1 ] || fail "the first ensure is not on major 1: $(getf major)"
  [ "$(getf previous)" = none ] || fail "the first ensure reported a previous host: $(getf previous)"
  alive "$pid1" || fail "host $pid1 is not alive after the client that started it exited"
  probe ensure
  pid2="$(getf pid)"
  [ "$pid1" = "$pid2" ] || fail "the second ensure did not adopt the host: pid $pid1, then $pid2"
  [ "$(getf method)" = None ] || fail "the second ensure started something instead of adopting: method=$(getf method)"
  [ "$(count_event 1 start.ready)" = 1 ] || fail "expected one start.ready, saw $(count_event 1 start.ready)"
  alive "$pid1" || fail "host $pid1 died"
  [ "$(state_pid 1)" = "$pid1" ] || fail "the state file names pid $(state_pid 1), not the host $pid1"
  pass
}

case_concurrent_start() {
  fresh_root
  local i pids=() outs=() pid first=""
  # Four clients at once, each its own process, all seeing no host.
  for i in a b c d; do
    "$PROBE" ensure >"$ROOT_BASE/ens-$i.out" 2>"$ROOT_BASE/ens-$i.err" &
    pids+=("$!")
    HELPERS+=("$!")
    outs+=("$ROOT_BASE/ens-$i.out")
  done
  for pid in "${pids[@]}"; do
    wait "$pid" || fail "a concurrent ensure exited non-zero: $(cat "$ROOT_BASE"/ens-*.out "$ROOT_BASE"/ens-*.err 2>/dev/null | head -c 800)"
  done
  HELPERS=()
  for i in "${outs[@]}"; do
    pid="$(sed -n 's/^pid=//p' "$i" | head -n 1)"
    [ -n "$pid" ] || fail "$i has no pid: $(cat "$i")"
    if [ -z "$first" ]; then first="$pid"; fi
    [ "$pid" = "$first" ] || fail "concurrent clients ended with different hosts: $first and $pid"
  done
  alive "$first" || fail "host $first is not alive"
  [ "$(count_event 1 start.ready)" = 1 ] || fail "expected exactly one start.ready, saw $(count_event 1 start.ready)"
  [ "$(state_pid 1)" = "$first" ] || fail "the state file names pid $(state_pid 1), not the host $first"
  # A start that lost the race may still be in its lock retry; it leaves by itself (exit 3).
  if [ "$OS" = linux ]; then
    wait_until 5 exactly_one_host_for_root "$ROOT" || fail "expected exactly one sirio-host process for the root, found $(count_hosts_for_root "$ROOT")"
  fi
  NOTE="$(count_event 1 start.lost_lock) start(s) lost the lock"
  pass
}

case_client_killed() {
  fresh_root
  local out="$ROOT_BASE/hold.out" client host_pid
  "$PROBE" ensure --hold >"$out" 2>"$ROOT_BASE/hold.err" &
  client=$!
  HELPERS+=("$client")
  wait_until 20 has_line "$out" '^pid=' || fail "the holding client never printed a pid: $(cat "$out" "$ROOT_BASE/hold.err" 2>/dev/null | head -c 600)"
  host_pid="$(sed -n 's/^pid=//p' "$out" | head -n 1)"
  # A live session, so that an idle exit cannot be mistaken for the kill
  # having taken the host down (the grace is not what is being tested).
  probe hold on
  kill_hard "$client"
  wait "$client" 2>/dev/null || true
  sleep 1
  alive "$host_pid" || fail "host $host_pid died with its client (SIGKILL of the client that started it)"
  # ...and it noticed: the dead client is no longer counted (this info call is the one client).
  wait_until 5 info_has_one_client || fail "the host still counts the killed client: $(getf clients) clients"
  [ "$(getf pid)" = "$host_pid" ] || fail "the host that answers is pid $(getf pid), not $host_pid"
  NOTE="sigkill"

  # On Linux with a user manager the client is also stopped the way a systemd
  # scope ends -- its whole cgroup -- which is what closing a terminal does.
  if user_manager_available; then
    local unit="she-$$-$N" out2="$ROOT_BASE/scope.out" scoped host2
    shutdown_hosts_in "$ROOT"
    use_root "$ROOT_BASE/c${N}b"
    systemd-run --user --scope --quiet --collect --unit="$unit" -- "$PROBE" ensure --hold >"$out2" 2>"$ROOT_BASE/scope.err" &
    scoped=$!
    HELPERS+=("$scoped")
    wait_until 20 has_line "$out2" '^pid=' || fail "the client in scope $unit never printed a pid: $(cat "$out2" "$ROOT_BASE/scope.err" 2>/dev/null | head -c 600)"
    host2="$(sed -n 's/^pid=//p' "$out2" | head -n 1)"
    probe hold on
    grep -q "$unit" "/proc/$scoped/cgroup" || fail "the client is not in the scope $unit: $(cat "/proc/$scoped/cgroup")"
    if grep -q "$unit" "/proc/$host2/cgroup"; then fail "the host $host2 is in the client's scope $unit, so stopping it would stop the host"; fi
    systemctl --user stop "$unit.scope" || fail "could not stop the scope $unit"
    wait "$scoped" 2>/dev/null || true
    wait_until 5 dead "$scoped" || fail "the client survived its scope being stopped"
    sleep 1
    alive "$host2" || fail "host $host2 died when the scope of the client that started it was stopped"
    wait_until 5 info_has_one_client || fail "the host still counts the client of the stopped scope"
    NOTE="sigkill and systemd scope stop"
  else
    NOTE="sigkill; systemd scope arm skipped: no user manager"
  fi
  pass
}
# A systemd user manager that answers (`degraded` is a manager that answers:
# it exits non-zero). The state is on stdout, so the exit status is ignored.
user_manager_available() {
  local state
  [ "$OS" = linux ] && [ -n "${XDG_RUNTIME_DIR:-}" ] && command -v systemctl >/dev/null 2>&1 || return 1
  state="$(systemctl --user is-system-running 2>/dev/null || true)"
  case "$state" in running|degraded) return 0 ;; *) return 1 ;; esac
}
info_has_one_client() {
  run_probe info
  [ "$RC" -eq 0 ] && [ "$(getf clients)" = 1 ] && [ "$(getf sessions)" = 1 ]
}

case_idle_exit() {
  fresh_root 2000
  probe ensure
  local pid
  pid="$(getf pid)"
  [ -n "$pid" ] || fail "no pid"
  wait_until 10 absent "$ROOT/host-v1.json" || fail "the state file is still there 10 s after the last client left (grace 2 s)"
  wait_until 5 dead "$pid" || fail "host $pid is still alive after it removed its state file"
  if [ "$OS" != windows ]; then
    [ ! -e "$ROOT/host-v1.sock" ] || fail "the endpoint host-v1.sock is still there after the host left"
  fi
  run_probe observe 1
  [ "$(getf verdict)" = Absent ] || fail "observe after the idle exit is '$(getf verdict)', not Absent"
  [ "$(count_event 1 stop.idle)" = 1 ] || fail "expected one stop.idle in the log, saw $(count_event 1 stop.idle)"
  [ "$(count_event 1 stop.done)" = 1 ] || fail "expected one stop.done in the log, saw $(count_event 1 stop.done)"
  pass
}

case_recycled_pid() {
  fresh_root
  mkdir -p "$ROOT"
  sleep 300 &
  local s=$! sp
  HELPERS+=("$s")
  sp="$(native_pid "$s")"
  alive "$sp" || fail "the stand-in process $sp is not running"

  # The control: the same file with the process's real start time is a live
  # process that does not answer (Unverifiable). Without it, "Absent" below
  # could just mean the file was never read. /proc exists on Linux only.
  if [ -r "/proc/$sp/stat" ]; then
    write_fake_state "$ROOT" "$sp" "$(proc_start_time "$sp")"
    run_probe observe 1
    [ "$(getf verdict)" = Unverifiable ] || fail "control: a state file naming a live process with its true start time must read Unverifiable, not '$(getf verdict)'"
  fi
  write_fake_state "$ROOT" "$sp" 1
  run_probe observe 1
  [ "$(getf verdict)" = Absent ] || fail "a state file naming a live process of another start time must read Absent, not '$(getf verdict)'"

  probe ensure
  local pid
  pid="$(getf pid)"
  [ -n "$pid" ] || fail "no pid"
  [ "$pid" != "$sp" ] || fail "ensure adopted the recycled pid $sp"
  alive "$sp" || fail "the process $sp, whose pid the stale state file named, was signalled"
  alive "$pid" || fail "host $pid is not alive"
  [ "$(state_pid 1)" = "$pid" ] || fail "the state file still names $(state_pid 1), not the new host $pid"
  kill_hard "$s"
  wait "$s" 2>/dev/null || true
  pass
}

case_no_steal() {
  fresh_root
  probe ensure
  local pid
  pid="$(getf pid)"
  # A second host, run by hand on the same root, while the first holds the lock.
  run_bounded 20 "$HOST_BIN"
  [ "$RC" != 124 ] || fail "a second host on a held lock was still running after 20 s (it must exit 3)"
  [ "$RC" = 3 ] || fail "a second host on a held lock exited $RC, not 3"
  alive "$pid" || fail "the first host $pid died"
  probe info
  [ "$(getf pid)" = "$pid" ] || fail "after the second host's attempt the answering host is pid $(getf pid), not $pid"
  [ "$(count_event 1 start.lost_lock)" -ge 1 ] || fail "the log has no start.lost_lock"
  [ "$(count_event 1 start.ready)" = 1 ] || fail "expected one start.ready, saw $(count_event 1 start.ready)"
  [ "$(state_pid 1)" = "$pid" ] || fail "the state file names $(state_pid 1), not the first host $pid"
  pass
}
# run_bounded SECONDS CMD...: runs CMD, sets RC to its exit status, or to 124
# when it had to be killed after SECONDS.
run_bounded() {
  local limit="$1" pid n
  shift
  "$@" >/dev/null 2>&1 &
  pid=$!
  HELPERS+=("$pid")
  n=$((limit * 10))
  while [ "$n" -gt 0 ] && job_running "$pid"; do sleep 0.1; n=$((n - 1)); done
  if job_running "$pid"; then
    kill_hard "$pid"
    wait "$pid" 2>/dev/null || true
    RC=124
  else
    RC=0
    wait "$pid" || RC=$?
  fi
}

case_drain() {
  fresh_root 2000
  local v1 v2 v1b
  # 1. a v1 host with a live session (held, so the 2 s grace cannot end it)
  probe_ok_as 1 ensure
  v1="$(getf pid)"
  [ "$(getf major)" = 1 ] || fail "step 1: not major 1: $(getf major)"
  probe_ok_as 1 hold on
  # 2. the new app: a v2 host, with the v1 host adopted as its previous
  probe_ok_as 2 ensure
  v2="$(getf pid)"
  [ "$(getf major)" = 2 ] || fail "step 2: major is $(getf major), not 2"
  [ "$(getf previous)" = 1 ] || fail "step 2: previous is $(getf previous), not 1"
  [ "$v1" != "$v2" ] || fail "step 2: the v2 client ended up on the v1 host (pid $v1)"
  probe_ok_as 2 hold on
  [ -f "$ROOT/host-v1.json" ] && [ -f "$ROOT/host-v2.json" ] || fail "step 2: expected host-v1.json and host-v2.json"
  [ "$(state_pid 1)" = "$v1" ] || fail "step 2: host-v1.json names $(state_pid 1), not $v1"
  [ "$(state_pid 2)" = "$v2" ] || fail "step 2: host-v2.json names $(state_pid 2), not $v2"
  # 3. the v1 session ends: v1 drains by itself, nobody stops it
  probe_ok_as 1 hold off
  wait_until 10 absent "$ROOT/host-v1.json" || fail "step 3: the v1 host did not drain within 10 s of its last session ending"
  wait_until 5 dead "$v1" || fail "step 3: the v1 host removed its state file but is still alive"
  [ -f "$ROOT/host-v2.json" ] || fail "step 3: host-v2.json is gone"
  alive "$v2" || fail "step 3: the v2 host died when the v1 host drained"
  [ "$(count_event 1 stop.idle)" = 1 ] || fail "step 3: the v1 host did not leave by an idle exit (stop.idle: $(count_event 1 stop.idle))"
  [ "$(count_event 2 stop.idle)" = 0 ] || fail "step 3: the v2 host idled out although a session is held"
  # 4. downgrade: an older app gets its own v1 host beside the newer one
  probe_ok_as 1 ensure
  v1b="$(getf pid)"
  [ "$(getf major)" = 1 ] || fail "step 4: major is $(getf major), not 1"
  [ "$v1b" != "$v2" ] || fail "step 4: the older app was handed the v2 host"
  [ "$v1b" != "$v1" ] || fail "step 4: the drained v1 host's pid came back"
  alive "$v2" || fail "step 4: the v2 host died when an older app connected"
  [ "$(state_pid 2)" = "$v2" ] || fail "step 4: host-v2.json names $(state_pid 2), not $v2"
  # 5. every host is force-shut by end_case
  probe_ok_as 2 shutdown --force
  [ "$(getf_raw)" = ok ] || fail "step 5: shutdown --force of the v2 host answered: $OUT"
  pass
}
getf_raw() { printf '%s' "$OUT" | head -n 1; }

case_conformance() {
  fresh_root
  probe ensure
  local expected passed failed
  expected="$(find "$REPO_ROOT/protocol/host-v1/conformance" -name '*.json' | wc -l | tr -d ' ')"
  [ "$expected" -gt 0 ] || fail "no conformance cases under protocol/host-v1/conformance"
  run_probe conformance "$(native "$REPO_ROOT/protocol/host-v1/conformance")"
  passed="$(printf '%s\n' "$OUT" | grep -c '^case=[^ ]* PASS$' || true)"
  failed="$(printf '%s\n' "$OUT" | grep '^case=' | grep -vc ' PASS$' || true)"
  [ "$RC" -eq 0 ] || fail "the conformance run exited $RC ($failed failing)"
  [ "$failed" = 0 ] || fail "$failed conformance case(s) did not pass"
  [ "$passed" = "$expected" ] || fail "$passed conformance cases passed, but the directory has $expected"
  probe info
  NOTE="$passed cases"
  pass
}

case_sessions_live() {
  fresh_root
  probe ensure
  local pid
  pid="$(getf pid)"
  probe hold on
  probe shutdown
  [ "$(getf_raw)" = error=sessions_live ] || fail "shutdown with a session held answered '$OUT', not error=sessions_live"
  alive "$pid" || fail "the host left although it refused the shutdown"
  probe shutdown --force
  [ "$(getf_raw)" = ok ] || fail "shutdown --force answered '$OUT', not ok"
  wait_until 10 absent "$ROOT/host-v1.json" || fail "the state file is still there 10 s after shutdown --force"
  wait_until 5 dead "$pid" || fail "host $pid is still alive after shutdown --force"
  pass
}

case_unverifiable() {
  if [ "$OS" = windows ]; then skip "no SIGSTOP on Windows"; return 0; fi
  fresh_root
  probe ensure
  local pid before after
  pid="$(getf pid)"
  # A host that holds its lock and answers nothing.
  kill -STOP "$pid"
  before="$(date +%s)"
  run_probe ensure
  after="$(date +%s)"
  [ "$RC" -eq 1 ] || fail "ensure against a silent host exited $RC, not 1"
  case "$(getf error)" in Unverifiable*) ;; *) fail "ensure against a silent host answered '$OUT', not error=Unverifiable" ;; esac
  [ $((after - before)) -ge 10 ] || fail "ensure gave up after $((after - before)) s: the wait for a silent host is 10 s"
  [ "$(count_event 1 start.ready)" = 1 ] || fail "a second host was started beside the silent one (start.ready: $(count_event 1 start.ready))"
  [ "$(state_pid 1)" = "$pid" ] || fail "the state file now names $(state_pid 1), not the silent host $pid"
  alive "$pid" || fail "the silent host was killed"
  if [ "$OS" = linux ]; then
    [ "$(count_hosts_for_root "$ROOT")" = 1 ] || fail "expected one sirio-host process for the root, found $(count_hosts_for_root "$ROOT")"
  fi
  kill -CONT "$pid"
  probe info
  [ "$(getf pid)" = "$pid" ] || fail "after resuming, the answering host is pid $(getf pid), not $pid"
  NOTE="gave up after $((after - before)) s"
  pass
}

case_path_too_long() {
  if [ "$OS" = windows ]; then skip "named pipes have no sun_path"; return 0; fi
  local dir
  dir="$ROOT_BASE/$(printf 'd%.0s' $(seq 110))"
  use_root "$dir"
  if ! mkdir -p "$dir" 2>/dev/null; then skip "the filesystem refuses a 110-character name"; return 0; fi
  export SIRIO_HOST_IDLE_GRACE_MS=20000
  run_probe ensure
  [ "$RC" -eq 1 ] || fail "ensure on a root whose endpoint path is too long exited $RC, not 1"
  case "$(getf error)" in ?*) ;; *) fail "ensure printed no error=: '$OUT'" ;; esac
  has_line "$dir/log/host-v1.log" "$(printf 'start.bind_failed\tendpoint path too long')" \
    || fail "the log has no 'start.bind_failed<TAB>endpoint path too long': $(cat "$dir/log/host-v1.log" 2>&1)"
  [ "$(count_event 1 start.ready)" = 0 ] || fail "a host reported itself ready on an endpoint it cannot bind"
  NOTE="ensure said $(getf error | cut -c1-40)"
  pass
}

case_foreign_client() {
  fresh_root
  probe ensure
  local pid
  pid="$(getf pid)"
  probe raw-ndjson
  [ "$(getf closed)" = true ] || fail "the host did not close on a foreign protocol: '$OUT'"
  if printf '%s\n' "$OUT" | grep -q '^bytes='; then fail "the host answered a foreign protocol with bytes: '$OUT'"; fi
  probe info
  [ "$(getf pid)" = "$pid" ] || fail "after the foreign client the answering host is pid $(getf pid), not $pid"
  alive "$pid" || fail "the host died on a foreign client"
  pass
}

run_case start-and-adopt   case_start_and_adopt
run_case concurrent-start  case_concurrent_start
run_case client-killed     case_client_killed
run_case idle-exit         case_idle_exit
run_case recycled-pid      case_recycled_pid
run_case no-steal          case_no_steal
run_case drain             case_drain
run_case conformance       case_conformance
run_case sessions-live     case_sessions_live
run_case unverifiable      case_unverifiable
run_case path-too-long     case_path_too_long
run_case foreign-client    case_foreign_client

echo "HOST E2E OK ($N cases, $(( $(date +%s) - RUN_STARTED ))s)"
