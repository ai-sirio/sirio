#!/bin/bash
set -eEuo pipefail

# Live proof that Sirio's Lombok agent works on the real jdtls
# (docs/superpowers/specs/2026-10-26-jdtls-lombok-design.md, section 6 "Real jdtls"):
# a real, isolated debug Sirio on a private Xvfb, the real jdtls launcher behind a
# `jdtls` stub on PATH (the built-in `java` entry still launches it), and a four-file
# Spring Boot + Lombok Maven project the script writes itself, with an EMPTY
# `$HOME/.m2` so jdtls's own Maven import downloads Lombok from Maven Central. The
# app is driven through the debug-only `sirioctl surface editor` verbs and read back
# through the editor's state (`diagnostics` for the open file, `problems` for the
# drawer, which covers files nobody opened).
#
# Run 1, the restart: AnagraficaController.java is opened. jdtls starts with no agent
# (no jar yet), the import downloads Lombok into `$HOME/.m2`, and the next time the
# server publishes diagnostics Sirio stops it and launches it again with
# `--jvm-arg=-javaagent:<that jar>`, then sends it one full build. The launch log must
# show the second line with the agent. Once that server is ready the editor's
# diagnostics and the drawer's error count (PegaService.java and LoginBody.java are
# never opened) must read 0 and hold 0 for 60 s: a Lombok error republished from the
# workspace the first launch left behind would show in that window.
# Run 2, the relaunch: the app is quit, every jdtls the run launched must be gone
# (one left over would hold the workspace lock and the fallback would hide it), the
# app is started again and the file reopened: the launch carries the agent from the
# start, over the workspace Run 1 built, and the same two values read 0 and hold.
#
# Opt-in: it needs network access to Maven Central, a Java 25 runtime and a jdtls
# install (1.63 needs Java 25), takes 10 to 20 minutes, and is never part of
# Scripts/ci.sh. Nothing is installed: the two paths are yours. Linux and X11 only.
#
# The artifact: --out-dir DIR (default artifacts/jdtls-lombok-live-<stamp>-<pid>)
# keeps transcript.log, jdtls-launches.log (one line per launch: "<pid> <project dir
# name> <argv>") and app.log, plus the jdtls workspace's own log when there is one.
#
# Usage: Scripts/Tests/test-jdtls-lombok-live.sh --jdtls PATH --java-home DIR
#          [--xvfb | --display :N] [--out-dir DIR] [--no-build]
#
#   --jdtls PATH      the jdtls launcher (the `bin/jdtls` of an unpacked release)
#   --java-home DIR   the Java 25 home it runs on
#   --xvfb            start a private Xvfb on a free display between :90 and :99
#   --display :N      use an Xvfb this user started (never your own $DISPLAY)
#   --no-build        use the binaries already built

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
SEED="$TARGET_DIR/debug/examples/appearance_seed"

START_XVFB=0
DISPLAY_TARGET=""
OUT_DIR=""
JDTLS=""
JAVA_HOME_ARG=""
BUILD=1
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --jdtls) JDTLS="${2:?--jdtls needs the path of the launcher}"; shift 2 ;;
    --java-home) JAVA_HOME_ARG="${2:?--java-home needs a directory}"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/jdtls-lombok-live-$(date +%Y%m%d-%H%M%S)-$$"
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
[ -n "$JDTLS" ] || fail "--jdtls PATH is required"
[ -n "$JAVA_HOME_ARG" ] || fail "--java-home DIR is required"
[ -x "$JDTLS" ] || fail "--jdtls $JDTLS is not an executable file"
[ -x "$JAVA_HOME_ARG/bin/java" ] || fail "--java-home $JAVA_HOME_ARG has no bin/java"
JDTLS="$(cd "$(dirname "$JDTLS")" && pwd)/$(basename "$JDTLS")"
JAVA_HOME_ARG="$(cd "$JAVA_HOME_ARG" && pwd)"
[ -n "$DISPLAY_TARGET" ] || [ "$START_XVFB" -eq 1 ] || fail "no display; pass --xvfb or --display :N (the editor draws)"
[ -z "$DISPLAY_TARGET" ] || [ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY; pass --xvfb, or an Xvfb you started on another number"

START_TIME="$(date +%s)"
elapsed() { echo "$(($(date +%s) - START_TIME)) s"; }

# ---------------------------------------------------------------- build

if [ "$BUILD" -eq 1 ]; then
  echo "building sirio, sirioctl, sirio-host and the appearance seed"
  (cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
    -p sirio_host --bin sirio-host -p sirio_persistence --example appearance_seed)
fi
for binary in "$BIN" "$CTL" "$SEED"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run

BASE="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/jdtls-lombok-live-XXXXXX")"
WT="$RUN_DIR/repo"
LAUNCH_LOG="$RUN_DIR/jdtls-launches.log"
APP_PID=""
XVFB_PID=""
SCENARIO="setup"
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
# A launch's process is alive when /proc knows it and it is not a zombie.
is_alive() { # pid
  local state
  state="$(sed -n 's/^State:[[:space:]]*\(.\).*/\1/p' "/proc/$1/status" 2>/dev/null || true)"
  [ -n "$state" ] && [ "$state" != "Z" ]
}
# Kills what a failed run left of the jdtls it launched, so no JVM outlives the run.
kill_launched_jdtls() {
  local pid
  [ -f "$LAUNCH_LOG" ] || return 0
  for pid in $(cut -d' ' -f1 "$LAUNCH_LOG"); do
    if is_alive "$pid"; then kill -KILL "$pid" 2>/dev/null || true; fi
  done
}
cleanup() {
  local rc=$?
  [ -z "$APP_PID" ] || terminate "$APP_PID"
  stop_hosts
  kill_launched_jdtls
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  cp "$RUN_DIR"/app.log "$OUT_DIR/" 2>/dev/null || true
  cp "$LAUNCH_LOG" "$OUT_DIR/" 2>/dev/null || true
  # jdtls's own log in its workspace, kept when it is there: it says why a launch failed.
  local log n=0
  for log in "$RUN_DIR"/home/.cache/jdtls/*/.metadata/.log; do
    [ -f "$log" ] || continue
    n=$((n + 1))
    tail -c 200000 "$log" >"$OUT_DIR/jdtls-workspace-$n.log" 2>/dev/null || true
  done
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
# GRADLE_USER_HOME would send the jar search to the user's own cache. HOME is the
# same for both runs: the launcher keeps jdtls's workspace under $HOME/.cache/jdtls.
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

# The `jdtls` the built-in entry finds on PATH: it logs "<pid> <project dir name>
# <argv>" and runs the real launcher on the given Java. The launcher execs the JVM, so
# the pid carries through. Java takes user.home from the passwd entry, not from $HOME,
# so the private HOME needs it said (spec section 6). `$RUN_DIR`, the jdtls path and
# the Java home are expanded here, into the stub.
mkdir -p "$HOME/.m2"
cat >"$RUN_DIR/stubs/jdtls" <<STUB
#!/bin/sh
printf '%s %s %s\n' "\$\$" "\$(basename "\$PWD")" "\$*" >> "$LAUNCH_LOG"
JAVA_HOME="$JAVA_HOME_ARG" exec "$JDTLS" "--jvm-arg=-Duser.home=\$HOME" "\$@"
STUB
chmod +x "$RUN_DIR/stubs/jdtls"

# The project, in a fresh git repository inside the run dir: the four files of
# spec section 7. Lombok 1.18.40 comes from Spring Boot's dependency management, and
# `$HOME/.m2` is empty, so the import downloads it.
mkdir -p "$WT/src/main/java/demo/model" "$WT/src/main/java/demo/service" "$WT/src/main/java/demo/controller"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
cat >"$WT/pom.xml" <<'POM'
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
    <version>3.5.6</version>
    <relativePath/>
  </parent>
  <groupId>demo</groupId>
  <artifactId>demo</artifactId>
  <version>0.0.1</version>
  <properties><java.version>21</java.version></properties>
  <dependencies>
    <dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId></dependency>
    <dependency><groupId>org.projectlombok</groupId><artifactId>lombok</artifactId><optional>true</optional></dependency>
  </dependencies>
</project>
POM
cat >"$WT/src/main/java/demo/Application.java" <<'JAVA'
package demo;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class Application {
    public static void main(String[] args) {
        SpringApplication.run(Application.class, args);
    }
}
JAVA
cat >"$WT/src/main/java/demo/model/LoginBody.java" <<'JAVA'
package demo.model;

import lombok.Data;

@Data
public class LoginBody {
    private String targa;
    private String tipoOperazione;
}
JAVA
cat >"$WT/src/main/java/demo/service/PegaService.java" <<'JAVA'
package demo.service;

import demo.model.LoginBody;
import lombok.extern.slf4j.Slf4j;
import org.springframework.stereotype.Service;

@Slf4j
@Service
public class PegaService {
    public String describe(LoginBody body) {
        log.info("describe");
        return body.getTarga() + body.getTipoOperazione();
    }
}
JAVA
cat >"$WT/src/main/java/demo/controller/AnagraficaController.java" <<'JAVA'
package demo.controller;

import demo.model.LoginBody;
import demo.service.PegaService;
import lombok.RequiredArgsConstructor;
import lombok.extern.slf4j.Slf4j;
import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RestController;

@RestController
@Slf4j
@RequiredArgsConstructor
public class AnagraficaController {
    private final PegaService pegaService;

    @PostMapping(path = "/utenti", consumes = MediaType.APPLICATION_JSON_VALUE)
    public String associa(@RequestBody LoginBody loginBody, String targa) {
        log.info("INIZIO");
        LoginBody temp = loginBody == null ? new LoginBody() : loginBody;
        temp.setTarga(targa);
        return pegaService.describe(temp);
    }
}
JAVA
git -C "$WT" add -A
git -C "$WT" commit -q -m "fixtures"
CONTROLLER="src/main/java/demo/controller/AnagraficaController.java"
LOMBOK_DIR="$HOME/.m2/repository/org/projectlombok/lombok/"
[ -z "$(ls -A "$HOME/.m2")" ] || fail "\$HOME/.m2 should start empty"

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

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
say() { echo "--- $*"; }

# The workspace the editor verbs act in: the repository, selected.
select_workspace() {
  ctl project add "$WT" >/dev/null 2>&1 || true
  for _ in $(seq 1 40); do
    "$CTL" select-workspace --workspace "$WT" >/dev/null 2>&1 && break
    sleep 0.5
  done
  local current
  current="$("$CTL" current-workspace --json 2>&1 || true)"
  case "$current" in *"$WT"*) ;; *) fail "the repository is not the selected workspace: $current" ;; esac
  echo "OK: the repository is the selected workspace"
}

# ---------------------------------------------------------------- the editor's state

read_state() {
  STATE="$("$CTL" surface editor state 2>&1)" || fail "$SCENARIO: surface editor state did not answer: $STATE"
}
sfield() { printf '%s\n' "$STATE" | sed -n "s/^$1=//p" | head -n 1; }
# One field of the status row (`status`: `lsp=jdtls:ready;branch=main;...`).
status_field() { # key
  sfield status | tr ';' '\n' | sed -n "s/^$1=//p" | head -n 1
}
# The drawer's error count: the first field of `problems` ("<errors>/<warnings>/<notes>"),
# over every file of the worktree, open or not. An empty field (no drawer yet) reads 0.
problem_errors() {
  local value
  value="$(sfield problems)"
  value="${value%%/*}"
  echo "${value:-0}"
}

# The launch log: one line per launch, "<pid> <project dir name> <argv>".
launch_count() {
  local n
  n="$(grep -c . "$LAUNCH_LOG" 2>/dev/null || true)"
  echo "${n:-0}"
}
launch_line() { sed -n "$1p" "$LAUNCH_LOG"; }

# Records the largest error counts seen, for the report: what the first launch showed
# before the restart. Informational: a poll can miss a short-lived value.
MAX_DIAGNOSTICS=0
MAX_PROBLEM_ERRORS=0
sample_errors() {
  local diagnostics errors
  read_state
  diagnostics="$(sfield diagnostics)"
  errors="$(problem_errors)"
  [ "${diagnostics:-0}" -le "$MAX_DIAGNOSTICS" ] || MAX_DIAGNOSTICS="$diagnostics"
  [ "$errors" -le "$MAX_PROBLEM_ERRORS" ] || MAX_PROBLEM_ERRORS="$errors"
}

# Waits up to SECONDS for the launch log to hold COUNT lines, printing progress.
wait_launches() { # count seconds
  local deadline=$((SECONDS + $2)) next=$((SECONDS + 30))
  while [ "$SECONDS" -lt "$deadline" ]; do
    [ "$(launch_count)" -ge "$1" ] && return 0
    sample_errors
    if [ "$SECONDS" -ge "$next" ]; then
      echo "... $(elapsed): $(launch_count) launch(es); status $(sfield status); diagnostics=$(sfield diagnostics) problems=$(sfield problems)"
      next=$((SECONDS + 30))
    fi
    sleep 2
  done
  fail "$SCENARIO: expected $1 launches within $2 s, got $(launch_count) ($(tr '\n' '|' <"$LAUNCH_LOG" 2>/dev/null))"
}
# Launch N carries the Lombok agent from the private Maven repository, as one argument.
expect_agent() { # launch number
  local line
  line="$(launch_line "$1")"
  case "$line" in
    *"--jvm-arg=-javaagent:$LOMBOK_DIR"*"/lombok-"*".jar"*) echo "OK: launch $1 carries the agent: ${line#* repo }" ;;
    *) fail "$SCENARIO: launch $1 expected --jvm-arg=-javaagent:$LOMBOK_DIR..., got '$line'" ;;
  esac
}
# Waits up to SECONDS for the editor's language server to read jdtls:ready.
expect_ready() { # seconds
  local deadline=$((SECONDS + $1)) next=$((SECONDS + 30)) got=""
  while [ "$SECONDS" -lt "$deadline" ]; do
    read_state
    got="$(status_field lsp)"
    if [ "$got" = "jdtls:ready" ]; then
      echo "OK: lsp=jdtls:ready at $(elapsed)"
      return 0
    fi
    if [ "$SECONDS" -ge "$next" ]; then
      echo "... $(elapsed): lsp=$got"
      next=$((SECONDS + 30))
    fi
    sleep 2
  done
  fail "$SCENARIO: lsp expected jdtls:ready within $1 s, got '$got' (status: $(sfield status))"
}
# Both values must read 0 and hold 0 for HOLD seconds: the open file's `diagnostics`
# and the drawer's error count over every file. A zero that is only the window's first
# instant (the restart emptied the stores) would not survive a Lombok error republished
# from the workspace on disk, which is what the window is for. Polled up to LIMIT seconds
# for the first zero; any non-zero after it restarts the window.
expect_clean() { # limit hold
  local deadline=$((SECONDS + $1)) hold_until=0 diagnostics errors next=$((SECONDS + 30))
  while [ "$SECONDS" -lt "$deadline" ] || [ "$hold_until" -gt 0 ]; do
    read_state
    diagnostics="$(sfield diagnostics)"
    errors="$(problem_errors)"
    if [ "$diagnostics" = 0 ] && [ "$errors" = 0 ]; then
      [ "$hold_until" -gt 0 ] || { hold_until=$((SECONDS + $2)); echo "clean at $(elapsed): holding for $2 s"; }
      if [ "$SECONDS" -ge "$hold_until" ]; then
        echo "OK: diagnostics=0 problems=$(sfield problems) held for $2 s (problem_files: '$(sfield problem_files)')"
        return 0
      fi
    else
      [ "$hold_until" -eq 0 ] || echo "not clean again at $(elapsed): diagnostics=$diagnostics problems=$(sfield problems) problem_files='$(sfield problem_files)'"
      hold_until=0
      [ "$SECONDS" -lt "$deadline" ] || break
    fi
    if [ "$SECONDS" -ge "$next" ]; then
      echo "... $(elapsed): diagnostics=$diagnostics problems=$(sfield problems) files='$(sfield problem_files)'"
      next=$((SECONDS + 30))
    fi
    sleep 1
  done
  fail "$SCENARIO: expected diagnostics=0 and no error in the drawer, got diagnostics=$diagnostics problems=$(sfield problems) problem_files='$(sfield problem_files)'"
}

# ---------------------------------------------------------------- run 1: the restart

SCENARIO="run1_restart"
say "run 1: the restart (empty \$HOME/.m2, jdtls downloads Lombok during its import)"
app_start
select_workspace
ctl surface editor open --path="$WT/$CONTROLLER" >/dev/null
# Two launches within 600 s: the first with no agent, the second the restart's.
wait_launches 2 600
echo "OK: two launches by $(elapsed)"
case "$(launch_line 1)" in
  *"-javaagent:"*) fail "$SCENARIO: the first launch already carried an agent, but no jar could exist yet: '$(launch_line 1)'" ;;
esac
expect_agent 2
echo "before the restart the poll saw at most diagnostics=$MAX_DIAGNOSTICS, drawer errors=$MAX_PROBLEM_ERRORS"
expect_ready 300
expect_clean 300 60
[ "$(launch_count)" = 2 ] || fail "$SCENARIO: expected exactly 2 launches, got $(launch_count) ($(tr '\n' '|' <"$LAUNCH_LOG"))"
echo "OK: run 1 had two launches, and Lombok's errors are gone from the open file and from files nobody opened"
RUN1_PROBLEMS="$(sfield problems)"
RUN1_DIAGNOSTICS="$(sfield diagnostics)"
RUN1_MAX_DIAGNOSTICS="$MAX_DIAGNOSTICS"
RUN1_MAX_PROBLEM_ERRORS="$MAX_PROBLEM_ERRORS"

# ---------------------------------------------------------------- run 2: the relaunch

SCENARIO="run2_relaunch"
say "run 2: quit, every jdtls gone, relaunch over the workspace run 1 left"
terminate "$APP_PID"
APP_PID=""
deadline=$((SECONDS + 30))
alive=""
while :; do
  alive=""
  for pid in $(cut -d' ' -f1 "$LAUNCH_LOG"); do
    if is_alive "$pid"; then alive="$alive $pid"; fi
  done
  [ -n "$alive" ] || break
  [ "$SECONDS" -lt "$deadline" ] || fail "$SCENARIO: jdtls still running 30 s after the app quit (pid$alive): it would hold the workspace lock, and the fallback would hide the cause"
  sleep 0.5
done
echo "OK: every jdtls of run 1 is gone at $(elapsed)"
app_start
select_workspace
ctl surface editor open --path="$WT/$CONTROLLER" >/dev/null
wait_launches 3 60
expect_agent 3
expect_ready 300
expect_clean 300 60
[ "$(launch_count)" = 3 ] || fail "$SCENARIO: expected exactly 3 launches, got $(launch_count) ($(tr '\n' '|' <"$LAUNCH_LOG"))"
echo "OK: run 2 had one launch, with the agent from the start"
RUN2_PROBLEMS="$(sfield problems)"
RUN2_DIAGNOSTICS="$(sfield diagnostics)"

stop_hosts
echo "run 1: diagnostics=$RUN1_DIAGNOSTICS problems=$RUN1_PROBLEMS (before the restart the poll saw at most diagnostics=$RUN1_MAX_DIAGNOSTICS, drawer errors=$RUN1_MAX_PROBLEM_ERRORS)"
echo "run 2: diagnostics=$RUN2_DIAGNOSTICS problems=$RUN2_PROBLEMS"
echo "launches:"
sed 's/^/  /' "$LAUNCH_LOG"
echo "total $(elapsed)"
echo "JDTLS LOMBOK LIVE OK"
