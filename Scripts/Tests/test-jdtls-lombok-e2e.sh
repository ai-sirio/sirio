#!/bin/bash
set -eEuo pipefail

# End-to-end test of how Sirio launches the built-in `jdtls` around Lombok
# (docs/superpowers/specs/2026-10-26-jdtls-lombok-design.md): a real, isolated
# debug Sirio on a private Xvfb, a fake `jdtls` on PATH that logs each launch
# (its argv and the project directory it was started in) and then speaks LSP
# through Scripts/Tests/fake_lsp.py, and fake Lombok jars in a private HOME. The
# app is driven through the debug-only `sirioctl surface editor` verbs and read
# back through the editor's status row. Linux and X11 only.
#
# Scenarios, in the order they run:
#   restart              a project that mentions Lombok, opened with no jar anywhere,
#                        launches with no argument; once Maven's 1.18.30 lands and the
#                        server next publishes diagnostics (the app looks at most every
#                        2 s), it is stopped (`shutdown`, then `exit`) and launched again
#                        with that jar's agent, sent the open file and one full build, and
#                        it is ready, and still ready 3 s later; a newer Gradle jar placed
#                        afterwards causes no third launch
#   restart_held         the same restart, from a launch that carries Maven's 1.18.30
#                        agent and has not been answered its full build (the fake holds
#                        the answer): Gradle's newer 1.18.36 lands and the next publish
#                        stops it and launches again with the 1.18.36 agent, not with no
#                        agent. That is the old router being cancelled: left running, it
#                        would see the stream close before the build was answered, blame
#                        the agent and send the relaunch out without it
#   agent_at_launch      a project whose parent POM alone mentions Lombok
#                        (lombok-parent/app) launches with the newest jar's agent: the
#                        Gradle cache's 1.18.36 over Maven's 1.18.30, as one argv
#                        element (checked from the argv itself, not the joined log line),
#                        and sends one full build (`java/buildWorkspace`) after the
#                        server's ServiceReady, after `initialized`
#   agent_from_settings  with the Gradle jar gone, the agent is Maven's, found through
#                        ~/.m2/settings.xml's localRepository, `${user.home}/m2 custom`:
#                        the space in the path reaches the launch intact
#   no_lombok            a project that does not mention Lombok launches with no argument
#                        and is sent no full build
#   fallback_init        a jdtls that exits at once when it is given the agent (a launch
#                        that fails) is launched once more without it: two launches, the
#                        first with the agent and the second with no argument, which is
#                        ready and sent the open file, and no third launch
#   fallback_early       a jdtls that ends right after `initialized`, before it has
#                        answered the full build, is launched once more without the agent,
#                        with the same checks
#   stderr_flood         a jdtls that writes 200 KB to stderr before it answers is
#                        ready within 10 s: the app's pipe to it is never left unread
#   hand_written         a java entry in languages.toml is launched with exactly the
#                        arguments written there, in a project that mentions Lombok,
#                        and is sent no full build
#
# The artifact: --out-dir DIR (default artifacts/jdtls-lombok-e2e-<stamp>-<pid>) keeps
# transcript.log, app.log and jdtls/, the fake server's launch log (launches.log, one
# line per launch: "<n> <project dir> <argv>") and one request log per launch. Rerunning
# the script reproduces it.
#
# Usage: Scripts/Tests/test-jdtls-lombok-e2e.sh [--xvfb | --display :N]
#          [--out-dir DIR] [--only NAME[,NAME...]]
#
#   --xvfb        start a private Xvfb on a free display between :90 and :99
#   --display :N  use an Xvfb this user started (never your own $DISPLAY)
#   --only        run the named scenarios alone and print "JDTLS LOMBOK E2E PARTIAL"
#                 instead of "JDTLS LOMBOK E2E OK"; only the whole run is the artifact

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
SEED="$TARGET_DIR/debug/examples/appearance_seed"

START_XVFB=0
DISPLAY_TARGET=""
OUT_DIR=""
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --only) ONLY=",${2:?--only needs scenario names},"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/jdtls-lombok-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

for tool in python3 git xdotool xwininfo xprop xdpyinfo; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
[ "$(id -u)" -ne 0 ] || fail "run as a user, not root"
[ -n "$DISPLAY_TARGET" ] || [ "$START_XVFB" -eq 1 ] || fail "no display; pass --xvfb or --display :N (the editor draws)"
[ -z "$DISPLAY_TARGET" ] || [ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY; pass --xvfb, or an Xvfb you started on another number"

# ---------------------------------------------------------------- build

echo "building sirio, sirioctl, sirio-host and the appearance seed"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host -p sirio_persistence --example appearance_seed)
for binary in "$BIN" "$CTL" "$SEED"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run

BASE="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/jdtls-lombok-XXXXXX")"
WT="$RUN_DIR/repo"
APP_PID=""
XVFB_PID=""
SCENARIO="setup"
SCENARIOS_PASSED=0
STATE=""

is_a_host() {
  local comm
  comm="$(cat "/proc/$1/comm" 2>/dev/null || true)"
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
# Ends a child of this script: TERM, then KILL if it has not exited within 5 s.
terminate() { # pid
  kill -TERM "$1" 2>/dev/null || return 0
  (sleep 5; kill -KILL "$1" 2>/dev/null) >/dev/null 2>&1 &
  local watchdog=$!
  wait "$1" 2>/dev/null || true
  kill "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
}
stop_hosts() {
  # Only the hosts whose state files are in this run's own host root.
  local file pid tries
  for file in "$RUN_DIR"/host/host-v*.json; do
    [ -f "$file" ] || continue
    pid="$(sed -n 's/.*"pid": *\([0-9][0-9]*\).*/\1/p' "$file" | head -n 1)"
    [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && is_a_host "$pid" || continue
    kill -TERM "$pid" 2>/dev/null || true
    tries=0
    while kill -0 "$pid" 2>/dev/null && [ "$tries" -lt 50 ]; do
      tries=$((tries + 1))
      sleep 0.1
    done
    kill -KILL "$pid" 2>/dev/null || true
  done
}
cleanup() {
  local rc=$?
  [ -z "$APP_PID" ] || terminate "$APP_PID"
  stop_hosts
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  cp "$RUN_DIR"/app.log "$OUT_DIR/" 2>/dev/null || true
  # The fake server's launch and request logs: the artifact, kept on failure too.
  [ -d "$RUN_DIR/jdtls" ] && cp -R "$RUN_DIR/jdtls" "$OUT_DIR/" 2>/dev/null || true
  [ -d "$RUN_DIR/host/log" ] && cp -R "$RUN_DIR/host/log" "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
  exit "$rc"
}
trap cleanup EXIT

# A display: a private Xvfb on a free number, or the one given.
VULKAN_ENV=()
if [ "$START_XVFB" -eq 1 ]; then
  command -v Xvfb >/dev/null || fail "Xvfb is required for --xvfb"
  for number in $(seq 90 99); do
    [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
  done
  [ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
  Xvfb "$DISPLAY_TARGET" -screen 0 1920x1080x24 >"$OUT_DIR/xvfb.log" 2>&1 &
  XVFB_PID=$!
  for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
  DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
fi
echo "display: $DISPLAY_TARGET"
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi

# A private world: HOME and every XDG directory under the run dir, stub agent
# CLIs first on PATH (so no mise wrapper runs), and a host root of its own.
# GRADLE_USER_HOME would send the jar search to the user's own cache.
unset GRADLE_USER_HOME
export XDG_STATE_HOME="$RUN_DIR/xdg/state" XDG_DATA_HOME="$RUN_DIR/xdg/data"
export XDG_CONFIG_HOME="$RUN_DIR/xdg/config" XDG_CACHE_HOME="$RUN_DIR/xdg/cache"
export XDG_RUNTIME_DIR="$RUN_DIR/xdg/runtime"
mkdir -p "$RUN_DIR/home" "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR" "$RUN_DIR/host"
chmod 700 "$XDG_RUNTIME_DIR"
mkdir -p "$XDG_CONFIG_HOME/sirio" "$RUN_DIR/stubs"
for cli in claude codex opencode pi omp; do
  printf '#!/bin/sh\necho "2.1.257 (Claude Code)"\n' >"$RUN_DIR/stubs/$cli"
  chmod +x "$RUN_DIR/stubs/$cli"
done
export PATH="$RUN_DIR/stubs:$PATH"
export HOME="$RUN_DIR/home"
export SIRIO_SOCKET="$RUN_DIR/c.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
export SIRIO_CREDENTIALS="$RUN_DIR/credentials.json"
export SIRIO_HOST_HOME="$RUN_DIR/host"
[ "${#SIRIO_SOCKET}" -lt 100 ] || fail "the socket path is too long for a unix socket: $SIRIO_SOCKET"

# The database the app opens, seeded with an appearance.
"$SEED" --database "$SIRIO_DB" --appearance dark

# The fake jdtls: a program named `jdtls` on the app's PATH, so the built-in
# entry finds it. It logs "<n> <project dir name> <argv>" to launches.log, then
# behaves by the project's directory name, then becomes the fake language server
# (Scripts/Tests/fake_lsp.py) with one request log per launch. `$RUN_DIR` and
# `$ROOT` are expanded here, into the stub.
JDTLS_DIR="$RUN_DIR/jdtls"
mkdir -p "$JDTLS_DIR"
cat >"$JDTLS_DIR/answers.json" <<JSON
{"service_ready": true, "publish": {}, "nonce": 0}
JSON
# The same answers for a launch that ends right after `initialized`.
cat >"$JDTLS_DIR/answers-dieearly.json" <<JSON
{"service_ready": true, "publish": {}, "nonce": 0, "exit_after_initialized": true}
JSON
# The same answers for a launch that is never answered its full build.
cat >"$JDTLS_DIR/answers-holdbuild.json" <<JSON
{"service_ready": true, "publish": {}, "nonce": 0, "hold_build": true}
JSON
cat >"$RUN_DIR/stubs/jdtls" <<STUB
#!/bin/sh
# Fake jdtls: logs "<n> <project dir name> <argv>" then behaves by the dir name.
dir="$RUN_DIR/jdtls"
n=\$(( \$(wc -l < "\$dir/launches.log" 2>/dev/null || echo 0) + 1 ))
# The argv itself, one element per line after its count: launches.log joins it with
# spaces, which cannot tell one element holding a space from two. Written first, so
# a launch in launches.log always has its argv file.
{ echo "\$#"; printf '%s\n' "\$@"; } > "\$dir/argv-\$n.log"
printf '%s %s %s\n' "\$n" "\$(basename "\$PWD")" "\$*" >> "\$dir/launches.log"
agent=0; case " \$* " in *" --jvm-arg=-javaagent:"*) agent=1 ;; esac
answers="\$dir/answers.json"
case "\$(basename "\$PWD")" in
  *-failinit) [ "\$agent" = 1 ] && exit 3 ;;
  *-dieearly) [ "\$agent" = 1 ] && answers="\$dir/answers-dieearly.json" ;;
  *-holdbuild) [ "\$agent" = 1 ] && answers="\$dir/answers-holdbuild.json" ;;
  *-stderr) head -c 204800 /dev/zero | tr '\0' x >&2 ;;
esac
exec python3 -I "$ROOT/Scripts/Tests/fake_lsp.py" "\$answers" "\$dir/requests-\$n.log"
STUB
chmod +x "$RUN_DIR/stubs/jdtls"

# The fixtures, in a fresh git repository inside the run dir: one project per
# case, each with a pom.xml and one Java file.
mkdir -p "$WT"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
make_project() { # dir pom-text
  mkdir -p "$WT/$1/src/main/java/demo"
  printf '%s\n' "$2" >"$WT/$1/pom.xml"
  printf 'package demo; class A {}\n' >"$WT/$1/src/main/java/demo/A.java"
}
# Lombok is declared in the parent POM only; the module that is opened does not
# mention it, so the server's root (lombok-parent/app) is not where it is found.
mkdir -p "$WT/lombok-parent"
printf '%s\n' '<project><dependencies><dependency><artifactId>lombok</artifactId></dependency></dependencies></project>' >"$WT/lombok-parent/pom.xml"
make_project lombok-parent/app '<project><artifactId>app</artifactId></project>'
make_project restart '<project><artifactId>lombok</artifactId></project>'
make_project plain '<project><artifactId>plain</artifactId></project>'
make_project plain-stderr '<project><artifactId>plain-stderr</artifactId></project>'
make_project lombok-m2 '<project><artifactId>lombok</artifactId></project>'
make_project lombok-failinit '<project><artifactId>lombok</artifactId></project>'
make_project lombok-dieearly '<project><artifactId>lombok</artifactId></project>'
make_project lombok-holdbuild '<project><artifactId>lombok</artifactId></project>'
make_project lombok-hand '<project><artifactId>lombok</artifactId></project>'
git -C "$WT" add -A
git -C "$WT" commit -q -m "fixtures"

# Lombok's jars are text files: only their path and their length matter to the
# launch, and nothing here starts a JVM. Maven's local repository is moved by
# settings.xml to a folder with a space in its name.
place_jar() { # dir version
  mkdir -p "$1"
  printf 'not a jar' >"$1/lombok-$2.jar"
}
MAVEN_REPO="$HOME/m2 custom"
GRADLE_CACHE="$HOME/.gradle/caches/modules-2/files-2.1/org.projectlombok/lombok"
mkdir -p "$HOME/.m2"
printf '%s\n' '<settings><localRepository>${user.home}/m2 custom</localRepository></settings>' >"$HOME/.m2/settings.xml"
ensure_jars() {
  place_jar "$MAVEN_REPO/org/projectlombok/lombok/1.18.30" 1.18.30
  place_jar "$GRADLE_CACHE/1.18.36/abc123" 1.18.36
}

# ---------------------------------------------------------------- the app

app_start() {
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 \
    ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 300); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.1
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
  for _ in $(seq 1 600); do
    "$CTL" surface tabs read >/dev/null 2>&1 && return 0
    sleep 0.1
  done
  fail "sirio's control actions never became ready"
}
app_start

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
say() { echo "--- $*"; }

# The workspace the editor verbs act in: the repository, selected.
ctl project add "$WT" >/dev/null 2>&1 || true
for _ in $(seq 1 40); do
  "$CTL" select-workspace --workspace "$WT" >/dev/null 2>&1 && break
  sleep 0.5
done
CURRENT="$("$CTL" current-workspace --json 2>&1 || true)"
case "$CURRENT" in *"$WT"*) ;; *) fail "the repository is not the selected workspace: $CURRENT" ;; esac
echo "OK: the repository is the selected workspace"

# ---------------------------------------------------------------- the editor's state

read_state() {
  STATE="$("$CTL" surface editor state 2>&1)" || fail "$SCENARIO: surface editor state did not answer: $STATE"
}
sfield() { printf '%s\n' "$STATE" | sed -n "s/^$1=//p" | head -n 1; }
# One field of the status row (`status`: `branch=main;ahead=1;...`).
status_field() { # key
  sfield status | tr ';' '\n' | sed -n "s/^$1=//p" | head -n 1
}
# Polls the status row until KEY reads VALUE, for SECONDS (default 30).
expect_status() { # key value [seconds]
  local got="" tries=0 limit=$((${3:-30} * 5))
  while [ "$tries" -lt "$limit" ]; do
    read_state
    got="$(status_field "$1")"
    if [ "$got" = "$2" ]; then
      echo "OK: status $1=$2"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: status $1 expected $2, got $got (status: $(sfield status))"
}

# The fake server's launch log: one line per launch, "<n> <project dir> <argv>".
launches_of() { # project
  grep -E "^[0-9]+ $1( |\$)" "$JDTLS_DIR/launches.log" 2>/dev/null || true
}
# The argv of a project's one launch: its log line without the number and the name.
launch_args_of() { # project
  launches_of "$1" | sed -E 's/^[0-9]+ [^ ]+ ?//'
}
# The launch's argv as the fake jdtls received it, from its argv file: the element
# count, and for a count of 1 the element, compared whole. The project's first launch,
# or its ORDINAL-th (a fallback is the second).
expect_argv() { # project count [element] [ordinal]
  local n count first
  n="$(launches_of "$1" | sed -n "${4:-1}p" | cut -d' ' -f1)"
  [ -f "$JDTLS_DIR/argv-$n.log" ] || fail "$SCENARIO: $1 has no argv file for launch ${4:-1} ('$n')"
  count="$(sed -n 1p "$JDTLS_DIR/argv-$n.log")"
  [ "$count" = "$2" ] || fail "$SCENARIO: $1 expected $2 argv elements, got $count ($(tr '\n' '|' <"$JDTLS_DIR/argv-$n.log"))"
  if [ "$2" -ge 1 ]; then
    first="$(sed -n 2p "$JDTLS_DIR/argv-$n.log")"
    [ "$first" = "$3" ] || fail "$SCENARIO: $1 expected the one element '$3', got '$first'"
  fi
  echo "OK: $1 argv of launch ${4:-1}: $count element(s)"
}
wait_launches() { # project count seconds
  local tries=0 limit=$(($3 * 5)) got=0
  while [ "$tries" -lt "$limit" ]; do
    got="$(launches_of "$1" | grep -c . || true)"
    [ "$got" -ge "$2" ] && return 0
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: $1 expected $2 launches within $3 s, got $got ($(cat "$JDTLS_DIR/launches.log" 2>/dev/null | tr '\n' '|'))"
}
# The request log of a project's FIRST launch: the fake server's record of every
# message the app sent it, one line per message.
requests_log_of() { # project
  local n
  n="$(launches_of "$1" | head -n 1 | cut -d' ' -f1)"
  echo "$JDTLS_DIR/requests-$n.log"
}
# Waits up to SECONDS for the project's first launch to have been sent exactly one
# full build, and that after `initialized`.
expect_one_full_build() { # project seconds
  local log tries=0 limit=$(($2 * 5)) builds=0
  log="$(requests_log_of "$1")"
  while [ "$tries" -lt "$limit" ]; do
    builds="$(grep -c '^request java/buildWorkspace$' "$log" 2>/dev/null || true)"
    [ "$builds" -ge 1 ] && break
    tries=$((tries + 1))
    sleep 0.2
  done
  [ "$builds" = 1 ] || fail "$SCENARIO: $1 expected one java/buildWorkspace within $2 s, got $builds ($(tr '\n' '|' <"$log" 2>/dev/null))"
  # Anything more arrives within a moment of the first.
  sleep 1
  builds="$(grep -c '^request java/buildWorkspace$' "$log")"
  [ "$builds" = 1 ] || fail "$SCENARIO: $1 expected exactly one java/buildWorkspace, got $builds"
  [ "$(grep -n '^notification initialized$' "$log" | head -n 1 | cut -d: -f1)" -lt \
    "$(grep -n '^request java/buildWorkspace$' "$log" | head -n 1 | cut -d: -f1)" ] \
    || fail "$SCENARIO: $1 sent the full build before initialized ($(tr '\n' '|' <"$log"))"
  echo "OK: $1 was sent one full build after initialized"
}
# After SECONDS, the project's first launch has been sent no full build.
expect_no_full_build() { # project seconds
  local log
  log="$(requests_log_of "$1")"
  sleep "$2"
  [ -f "$log" ] || fail "$SCENARIO: $1 has no request log"
  if grep -q '^request java/buildWorkspace$' "$log"; then
    fail "$SCENARIO: $1 was sent a full build though it launched without the agent ($(tr '\n' '|' <"$log"))"
  fi
  echo "OK: $1 was sent no full build"
}
open_java() { # path relative to the repository
  ctl surface editor open --path="$WT/$1" >/dev/null
}

# ---------------------------------------------------------------- scenarios

scenario() { # name function
  SCENARIO="$1"
  if [ -n "$ONLY" ]; then
    case "$ONLY" in *",$1,"*) ;; *) return 0 ;; esac
  fi
  say "scenario $1"
  "$2"
  SCENARIOS_PASSED=$((SCENARIOS_PASSED + 1))
  echo "PASS $1"
}

# Rewrites an answers file (default answers.json) with the next nonce, plus EXTRA
# members: the fake server looks at its file every 200 ms and, `publish` being there,
# publishes every open document when it changes.
NONCE=0
bump_nonce() { # [file [extra members, with a leading comma]]
  local file="${1:-answers.json}"
  NONCE=$((NONCE + 1))
  printf '{"service_ready": true, "publish": {}, "nonce": %s%s}\n' "$NONCE" "${2:-}" >"$JDTLS_DIR/$file.new"
  mv "$JDTLS_DIR/$file.new" "$JDTLS_DIR/$file"
}
# Waits up to SECONDS for a launch's request log to hold LINE (a whole line).
wait_request() { # log line seconds
  local tries=0 limit=$(($3 * 5))
  while [ "$tries" -lt "$limit" ]; do
    grep -qx "$2" "$1" 2>/dev/null && return 0
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: $(basename "$1") never held '$2' within $3 s ($(tr '\n' '|' <"$1" 2>/dev/null))"
}

scenario_restart() {
  # No jar anywhere: the first launch has no agent, and is ready.
  rm -rf "$HOME/m2 custom" "$HOME/.gradle"
  open_java restart/src/main/java/demo/A.java
  expect_status lsp jdtls:ready
  wait_launches restart 1 10
  expect_argv restart 0
  local first="$JDTLS_DIR/requests-$(launches_of restart | sed -n 1p | cut -d' ' -f1).log"
  wait_request "$first" "notification textDocument/didOpen" 10
  # The jar lands, as a Maven import would put it; the next publish after the
  # re-check interval makes the app look.
  place_jar "$MAVEN_REPO/org/projectlombok/lombok/1.18.30" 1.18.30
  sleep 2.5
  bump_nonce
  wait_launches restart 2 15
  local expected="--jvm-arg=-javaagent:$MAVEN_REPO/org/projectlombok/lombok/1.18.30/lombok-1.18.30.jar"
  expect_argv restart 1 "$expected" 2
  # The old server was stopped as a server is: `shutdown`, then `exit`.
  wait_request "$first" "notification exit" 10
  [ "$(grep -n '^request shutdown$' "$first" | head -n 1 | cut -d: -f1)" -lt \
    "$(grep -n '^notification exit$' "$first" | head -n 1 | cut -d: -f1)" ] \
    || fail "$SCENARIO: the old server was not sent shutdown before exit ($(tr '\n' '|' <"$first"))"
  echo "OK: the old server was sent shutdown, then exit"
  # The new one is sent the open file and one full build, and is ready.
  local second="$JDTLS_DIR/requests-$(launches_of restart | sed -n 2p | cut -d' ' -f1).log"
  wait_request "$second" "notification textDocument/didOpen" 10
  wait_request "$second" "request java/buildWorkspace" 10
  sleep 1
  [ "$(grep -c '^request java/buildWorkspace$' "$second")" = 1 ] \
    || fail "$SCENARIO: the second launch expected one java/buildWorkspace ($(tr '\n' '|' <"$second"))"
  echo "OK: the second launch was sent the open file and one full build"
  expect_status lsp jdtls:ready
  sleep 3
  expect_status lsp jdtls:ready 5
  # A newer jar later does not restart it again: the one restart is spent.
  place_jar "$GRADLE_CACHE/1.18.36/abc123" 1.18.36
  sleep 2.5
  bump_nonce
  sleep 5
  [ "$(launches_of restart | grep -c .)" = 2 ] \
    || fail "$SCENARIO: restart expected exactly 2 launches, got $(launches_of restart | grep -c .)"
  echo "OK: a newer jar caused no second restart"
}

# The restart from a launch whose full build is still unanswered: the old router must
# be cancelled, or the old stream's end is blamed on its (older) agent and the
# relaunch goes out with no agent at all.
scenario_restart_held() {
  # Maven's 1.18.30 only: the restart scenario left Gradle's 1.18.36 behind.
  rm -rf "$GRADLE_CACHE/1.18.36"
  place_jar "$MAVEN_REPO/org/projectlombok/lombok/1.18.30" 1.18.30
  open_java lombok-holdbuild/src/main/java/demo/A.java
  expect_status lsp jdtls:ready
  wait_launches lombok-holdbuild 1 10
  local maven="--jvm-arg=-javaagent:$MAVEN_REPO/org/projectlombok/lombok/1.18.30/lombok-1.18.30.jar"
  local gradle="--jvm-arg=-javaagent:$GRADLE_CACHE/1.18.36/abc123/lombok-1.18.36.jar"
  expect_argv lombok-holdbuild 1 "$maven" 1
  local first="$JDTLS_DIR/requests-$(launches_of lombok-holdbuild | sed -n 1p | cut -d' ' -f1).log"
  # The build is asked for and never answered.
  wait_request "$first" "request java/buildWorkspace" 10
  place_jar "$GRADLE_CACHE/1.18.36/abc123" 1.18.36
  sleep 2.5
  bump_nonce answers-holdbuild.json ', "hold_build": true'
  wait_launches lombok-holdbuild 2 15
  expect_argv lombok-holdbuild 1 "$gradle" 2
  wait_request "$first" "notification exit" 10
  [ "$(grep -n '^request shutdown$' "$first" | head -n 1 | cut -d: -f1)" -lt \
    "$(grep -n '^notification exit$' "$first" | head -n 1 | cut -d: -f1)" ] \
    || fail "$SCENARIO: the old server was not sent shutdown before exit ($(tr '\n' '|' <"$first"))"
  echo "OK: the old server was sent shutdown, then exit"
  expect_status lsp jdtls:ready
  sleep 3
  [ "$(launches_of lombok-holdbuild | grep -c .)" = 2 ] \
    || fail "$SCENARIO: lombok-holdbuild expected exactly 2 launches, got $(launches_of lombok-holdbuild | grep -c .)"
  echo "OK: the relaunch carried the newer agent, and there was no third launch"
}

scenario_agent_at_launch() {
  ensure_jars
  open_java lombok-parent/app/src/main/java/demo/A.java
  expect_status lsp jdtls:ready
  wait_launches app 1 10
  local expected line
  expected="--jvm-arg=-javaagent:$GRADLE_CACHE/1.18.36/abc123/lombok-1.18.36.jar"
  line="$(launch_args_of app)"
  [ "$line" = "$expected" ] \
    || fail "$SCENARIO: the launch of lombok-parent/app expected '$expected', got '$line'"
  echo "OK: launch args: $line"
  expect_argv app 1 "$expected"
  expect_one_full_build app 5
}

scenario_agent_from_settings() {
  ensure_jars
  # The Gradle jar stays removed for the rest of the run: were it to come back, a
  # later publish from lombok-m2's server would see a jar newer than its agent.
  rm -rf "$GRADLE_CACHE/1.18.36"
  open_java lombok-m2/src/main/java/demo/A.java
  expect_status lsp jdtls:ready
  wait_launches lombok-m2 1 10
  local expected line
  expected="--jvm-arg=-javaagent:$MAVEN_REPO/org/projectlombok/lombok/1.18.30/lombok-1.18.30.jar"
  line="$(launch_args_of lombok-m2)"
  [ "$line" = "$expected" ] \
    || fail "$SCENARIO: the launch of lombok-m2 expected '$expected', got '$line'"
  echo "OK: launch args: $line"
  expect_argv lombok-m2 1 "$expected"
}

scenario_no_lombok() {
  open_java plain/src/main/java/demo/A.java
  expect_status lsp jdtls:ready
  wait_launches plain 1 10
  local line
  line="$(launch_args_of plain)"
  [ -z "$line" ] || fail "$SCENARIO: plain expected a launch with no argument, got '$line'"
  echo "OK: launch args: none"
  expect_argv plain 0
  expect_no_full_build plain 3
}

# After the agent broke the first launch: two launches, the first with the agent (one
# argv element) and the second with none, and the second sent the open file. Then no
# third launch, whatever else happens in 3 s.
expect_fallback() { # project
  local expected second tries=0 log=""
  expected="--jvm-arg=-javaagent:$MAVEN_REPO/org/projectlombok/lombok/1.18.30/lombok-1.18.30.jar"
  wait_launches "$1" 2 20
  expect_argv "$1" 1 "$expected" 1
  expect_argv "$1" 0 "" 2
  expect_status lsp jdtls:ready
  second="$(launches_of "$1" | sed -n 2p | cut -d' ' -f1)"
  log="$JDTLS_DIR/requests-$second.log"
  while [ "$tries" -lt 50 ] && ! grep -q '^notification textDocument/didOpen$' "$log" 2>/dev/null; do
    tries=$((tries + 1))
    sleep 0.2
  done
  grep -q '^notification textDocument/didOpen$' "$log" \
    || fail "$SCENARIO: $1's second launch was never sent the open file ($(tr '\n' '|' <"$log" 2>/dev/null))"
  echo "OK: $1's second launch was sent the open file"
  sleep 3
  [ "$(launches_of "$1" | grep -c .)" = 2 ] \
    || fail "$SCENARIO: $1 expected exactly 2 launches, got $(launches_of "$1" | grep -c .)"
  echo "OK: $1 had no third launch"
}
# The agent is Maven's from here on, as in agent_from_settings.
use_maven_agent_only() {
  ensure_jars
  rm -rf "$GRADLE_CACHE/1.18.36"
}

scenario_fallback_init() {
  use_maven_agent_only
  open_java lombok-failinit/src/main/java/demo/A.java
  expect_status lsp jdtls:ready
  expect_fallback lombok-failinit
}

scenario_fallback_early() {
  use_maven_agent_only
  open_java lombok-dieearly/src/main/java/demo/A.java
  wait_launches lombok-dieearly 2 20
  expect_fallback lombok-dieearly
}

scenario_stderr_flood() {
  # 200 KB on stderr is more than a pipe holds: a server whose stderr nobody reads
  # blocks before its first answer, and the editor never leaves "starting".
  open_java plain-stderr/src/main/java/demo/A.java
  expect_status lsp jdtls:ready 10
  wait_launches plain-stderr 1 10
}

scenario_hand_written() {
  cat >"$XDG_CONFIG_HOME/sirio/languages.toml" <<'TOML'
[[language]]
name = "java"
extensions = ["java"]
command = "jdtls"
args = ["--hand-written"]
roots = ["pom.xml"]
TOML
  open_java lombok-hand/src/main/java/demo/A.java
  expect_status lsp jdtls:ready
  wait_launches lombok-hand 1 10
  local line
  line="$(launch_args_of lombok-hand)"
  [ "$line" = "--hand-written" ] \
    || fail "$SCENARIO: lombok-hand expected exactly --hand-written, got '$line'"
  echo "OK: launch args: $line"
  expect_argv lombok-hand 1 "--hand-written"
  expect_no_full_build lombok-hand 3
}

scenario restart scenario_restart
scenario restart_held scenario_restart_held
scenario agent_at_launch scenario_agent_at_launch
scenario agent_from_settings scenario_agent_from_settings
scenario no_lombok scenario_no_lombok
scenario fallback_init scenario_fallback_init
scenario fallback_early scenario_fallback_early
scenario stderr_flood scenario_stderr_flood
# Last: the table it writes applies to every .java file from then on.
scenario hand_written scenario_hand_written

stop_hosts
if [ -n "$ONLY" ]; then
  [ "$SCENARIOS_PASSED" -gt 0 ] || fail "--only named no scenario of this script: ${ONLY//,/ }"
  echo "JDTLS LOMBOK E2E PARTIAL ($SCENARIOS_PASSED scenarios of --only)"
  exit 0
fi
echo "JDTLS LOMBOK E2E OK ($SCENARIOS_PASSED scenarios)"
