#!/bin/bash
set -eEuo pipefail
umask 022
# Every git this script runs reads no user or system configuration.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

# The end-to-end proof of "sirioctl always there" (project 1 of the ADE
# programme, plan Task 11): a real debug `sirio` on an isolated display (or none,
# with --state-only), its real `sirioctl` and `sirio-host`, and the hook stubs
# of Task 10. It proves that the stable copy of sirioctl is the built one and is
# kept current, that its PATH link is Sirio's, that no pane inherits the
# AppImage mount, that the skill reaches every agent's worktree and user
# directory, that each agent's hook reaches the status Sirio shows, and that the
# flags sirioctl offers do what they say.
#
# Every launch has a private HOME and XDG directories, its own SIRIO_DB (so its
# host root sits beside it), a control socket under the run directory, and a
# PATH that holds only stubs and the system bin. No real agent CLI runs, and the
# user's own DISPLAY is never read: --xvfb starts a private Xvfb.
#
# The cases. Each prints `step N: ...` and then `OK: ...`. A case that cannot run
# here prints `SKIP: <reason>`, and the run still ends `SIRIOCTL E2E OK`. The
# only reasons a case may give are --state-only, a missing node or bun, and a
# platform.
#   1  first launch: the stable copy is the built one; ~/.local/bin links it;
#      `env -i ... sirioctl version` names this build; the card says the login
#      shell does not search the link
#   2  replacement: a different sirioctl beside the app replaces the stable copy;
#      a process holding the old copy still reads the old bytes; the Claude
#      worktree hook still reports its statuses through the replaced copy
#   3  a foreign ~/.local/bin/sirioctl is left as it is, and the card names it
#   4  the AppImage launcher: no mount entry in a terminal, a panel session, an
#      ACP chat or a login shell; a relaunch through a second mount keeps the
#      launch snapshot (no nesting)
#   5  quit, remove the mount, relaunch: a terminal opened before the quit runs
#      `sirioctl ping` and gets its answer
#   4c a host started with SIRIO_HOST_NO_ENV_EXACT=1 carries the launcher's
#      names present and blank
#   6  hooks: each agent's stub reports the status its adapter maps; the global
#      Claude Stop command changes a pane's status from inside that pane, and
#      outside any pane exits 0 and changes nothing
#   7  skill: every adapter's worktree copy equals skills/sirio/SKILL.md, marker
#      included, and the served-skill test passes
#   8  a debug build without the knob: no release directory, no link; the global
#      hooks name the release path; install-skill writes the three user copies
#   9  SIRIO_WORKTREE_ID: the deepest worktree's id, however a session starts
#  10  the skill's git exclude is written once by the shared-skill adapters; a
#      Claude-only repository is left alone
#  11  startup refresh: a marked user copy is brought up to date; an unmarked file
#      and a missing one are left alone; a debug launch without the knob leaves a
#      stale marked copy stale
#  12  panel wait exits with the waited child's code
#  13  --dry-run prints one request and creates no pane
#
# --out-dir DIR keeps transcript.log, app-<world>.log, host-<world>-log/ (the
# host's logs), env/<case>.env (each pane's `env -0` dump, never printed),
# hooks/*.status and hooks/global-*.rc, and copies.sha256.
#
# --state-only takes no window captures, and skips 4, 4c, 5, 6 and 10: each needs
# a pane's own shell to run a command, which a state-only run does not render for
# the case's purpose. 9, 12 and 13 run there: a session starts without a window.
# --xvfb starts a private Xvfb on a free display; --display :N uses one that is
# already there. --mutate claude-prompt-hook is the mutation check of case 6: it drops the
# UserPromptSubmit hook from the Claude worktree before that hook can fire, and
# case 6 must then fail. --only N[,N...] runs some cases and prints
# `SIRIOCTL E2E PARTIAL` instead of OK. A case that needs an earlier one's
# world (5 after 4, 4c after 5, 2 after nothing, 10 after nothing) says so when
# it cannot find it.
#
# Usage: Scripts/Tests/test-sirioctl-e2e.sh [--out-dir DIR] [--no-build]
#          [--state-only] [--xvfb | --display :N] [--only N[,N...]]
#          [--mutate claude-prompt-hook]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR=""
ONLY=""
MUTATE=""
NO_BUILD=0
STATE_ONLY=0
START_XVFB=0
DISPLAY_TARGET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --no-build) NO_BUILD=1; shift ;;
    --state-only) STATE_ONLY=1; shift ;;
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --only) ONLY=",${2:?--only needs case numbers}," ; shift 2 ;;
    --mutate) MUTATE="${2:?--mutate needs a name}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$MUTATE" in
  ""|claude-prompt-hook) ;;
  *) echo "unknown --mutate $MUTATE (the only one is claude-prompt-hook)" >&2; exit 2 ;;
esac
case "$(uname -s)" in
  Darwin) OS=macos ;;
  Linux) OS=linux ;;
  *) echo "SIRIOCTL E2E runs on Linux and macOS only" >&2; exit 2 ;;
esac
if [ "$OS" = macos ] && [ "$STATE_ONLY" -eq 0 ]; then
  echo "note: macOS run is state-only (no window capture path for macOS yet)"
  STATE_ONLY=1
fi

TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$REPO_ROOT/$TARGET_DIR" ;; esac
BIN_DIR="$TARGET_DIR/debug"
APP_BIN="$BIN_DIR/sirio"
CTL="$BIN_DIR/sirioctl"
HOST_BIN="$BIN_DIR/sirio-host"
PROBE="$BIN_DIR/examples/host_probe"
STUBS="$REPO_ROOT/Scripts/Tests/stubs/hook-agents"
SKILL="$REPO_ROOT/skills/sirio/SKILL.md"

if [ -z "$OUT_DIR" ]; then OUT_DIR="$REPO_ROOT/artifacts/sirioctl-e2e-$(date +%Y%m%d-%H%M%S)-$$"; fi
mkdir -p "$OUT_DIR"
# Nothing from an earlier run may stand in for this run's evidence.
rm -rf "$OUT_DIR/env" "$OUT_DIR/hooks" "$OUT_DIR/copies.sha256" "$OUT_DIR/ping.out"
rm -rf "$OUT_DIR"/host-*-log
rm -f "$OUT_DIR"/app-*.log "$OUT_DIR/xvfb.log"
mkdir -p "$OUT_DIR/env" "$OUT_DIR/hooks"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

APPS=()
HOLDS=()
WORLDS=()
XVFB_PID=""
RUN=""
W=""
W_HOME=""
W_REPO=""
W_SOCK=""
W_KNOB=1
W_SHELL=""
W_PATH=""
WENV=()
VULKAN_ENV=()
CASE_N=0
CASE_NAME=""
CASE_SKIPPED_NOW=0
CASES_RUN=0
CASES_SKIPPED=0
INNER_SKIPS=0
LAUNCH_N=0
CHAT_DUMP=""
APP_PID=""
WT_ID=""
NODE_BIN=""
START_ALL=$(date +%s)

fail() {
  echo "FAIL: case $CASE_N $CASE_NAME: $*" >&2
  show_evidence >&2
  exit 1
}
unexpected() {
  echo "FAIL: case $CASE_N $CASE_NAME: unexpected error at line $1 (a command the script did not expect to fail)" >&2
  show_evidence >&2
  exit 1
}
trap 'unexpected $LINENO' ERR

show_evidence() {
  local world logf
  for world in ${WORLDS[@]+"${WORLDS[@]}"}; do
    [ -d "$RUN/$world" ] || continue
    for logf in "$RUN/$world/app.log" "$RUN/$world"/host/log/*.log; do
      [ -f "$logf" ] || continue
      echo "--- $logf"
      tail -n 25 "$logf" | cut -c1-300
    done
  done
}

# ---------------------------------------------------------------- processes

alive() {
  kill -0 "$1" 2>/dev/null || return 1
  if [ -r "/proc/$1/stat" ]; then
    local stat rest
    stat="$(cat "/proc/$1/stat" 2>/dev/null)" || return 1
    rest="${stat##*) }"
    [ "${rest%% *}" != Z ]
  fi
}
dead() { ! alive "$1"; }

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

# Only a process that is a sirio-host is ever signalled by the cleanup.
is_a_host() {
  local comm
  if [ -r "/proc/$1/comm" ]; then comm="$(cat "/proc/$1/comm" 2>/dev/null)"
  else comm="$(ps -p "$1" -o comm= 2>/dev/null || true)"
  fi
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}

cleanup() {
  local rc=$?
  set +e
  trap - ERR
  local pid world
  for pid in ${HOLDS[@]+"${HOLDS[@]}"}; do kill "$pid" 2>/dev/null; done
  for world in ${WORLDS[@]+"${WORLDS[@]}"}; do
    [ -d "$RUN/$world" ] || continue
    SIRIO_HOST_HOME="$RUN/$world/host" "$PROBE" shutdown --force >/dev/null 2>&1
  done
  for pid in ${APPS[@]+"${APPS[@]}"}; do kill "$pid" 2>/dev/null; done
  for pid in $(pgrep -f "$RUN" 2>/dev/null); do
    is_a_host "$pid" && kill "$pid" 2>/dev/null
  done
  for world in ${WORLDS[@]+"${WORLDS[@]}"}; do
    [ -f "$RUN/$world/app.log" ] && cp "$RUN/$world/app.log" "$OUT_DIR/app-$world.log"
    if [ -d "$RUN/$world/host/log" ]; then
      mkdir -p "$OUT_DIR/host-$world-log"
      cp "$RUN/$world/host/log/"* "$OUT_DIR/host-$world-log/" 2>/dev/null
    fi
  done
  [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null
  [ -n "$RUN" ] && rm -rf "$RUN"
  exit "$rc"
}
trap cleanup EXIT

# ---------------------------------------------------------------- display

if [ "$STATE_ONLY" -eq 0 ]; then
  if [ "$START_XVFB" -eq 1 ]; then
    command -v Xvfb >/dev/null || { echo "Xvfb is required for --xvfb" >&2; exit 2; }
    for number in $(seq 90 99); do
      [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
    done
    [ -n "$DISPLAY_TARGET" ] || { echo "no free display between :90 and :99" >&2; exit 2; }
    Xvfb "$DISPLAY_TARGET" -screen 0 1280x800x24 >"$OUT_DIR/xvfb.log" 2>&1 &
    XVFB_PID=$!
    for _ in $(seq 1 50); do timeout 2 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 && break; sleep 0.1; done
    timeout 2 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 || { echo "Xvfb did not start on $DISPLAY_TARGET" >&2; exit 2; }
  fi
  [ -n "$DISPLAY_TARGET" ] || { echo "no display; pass --xvfb or --display :N (or --state-only)" >&2; exit 2; }
  lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
  if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi
  echo "display: $DISPLAY_TARGET"
fi

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

# ---------------------------------------------------------------- the run directory

# Short and on disk: a socket path is limited to 108 bytes, and tmpfs is small.
RUN="$(mktemp -d "${TMPDIR:-/tmp}/sirioctl-e2e-XXXXXX")"
echo "run root: $RUN"
mkdir -p "$RUN/tools" "$RUN/bin" "$RUN/hookall" "$RUN/hookclaude" "$RUN/shell"

cat > "$RUN/tools/envcheck.py" <<'PY'
import sys
# envcheck.py FILE get NAME | under NAME DIR... | first NAME DIR
# Reads an `env -0` dump. `get` prints the value, or __ABSENT__. `under` exits 1
# when a ':'-separated entry of NAME is DIR or lies below it. `first` exits 0 when
# the first entry of NAME is DIR.
path, mode, name = sys.argv[1], sys.argv[2], sys.argv[3]
env = {}
with open(path, "rb") as handle:
    for entry in handle.read().split(b"\0"):
        key, sep, value = entry.partition(b"=")
        if sep:
            env.setdefault(key.decode("utf-8", "replace"), value.decode("utf-8", "replace"))
if mode == "get":
    sys.stdout.write(env.get(name, "__ABSENT__"))
elif mode == "under":
    hits = []
    for item in env.get(name, "").split(":"):
        for prefix in sys.argv[4:]:
            if item and (item == prefix or item.startswith(prefix.rstrip("/") + "/")):
                hits.append(item)
    if hits:
        sys.stdout.write(";".join(hits))
        sys.exit(1)
elif mode == "first":
    sys.exit(0 if env.get(name, "").split(":")[0] == sys.argv[4] else 1)
else:
    sys.exit(2)
PY

PY3="$(command -v python3)"
# The stubs. A version for every CLI, so no launch reaches a real one; an
# opencode whose `acp` dumps the environment its chat was started with; the hook
# stubs as symlinks, in the directories a world chooses; and the node that the
# opencode hook runs under, found on this machine and linked alone.
for name in claude codex pi omp; do
  printf '#!/bin/sh\necho "%s 1.0.0 (stub)"\n' "$name" > "$RUN/bin/$name"
done
cat > "$RUN/bin/opencode" <<SH
#!/bin/sh
# Stand-in for opencode. \`acp\` writes the environment it was started with to
# \$ACP_ENV_DUMP and then holds its stdin, as an ACP agent does. Any other call
# prints a version.
if [ "\$1" = acp ]; then
  $PY3 -I $RUN/tools/dumpenv.py > "\$ACP_ENV_DUMP"
  pwd -P > "\${ACP_ENV_DUMP%.env}.cwd"
  exec /usr/bin/cat > /dev/null
fi
echo "opencode 1.18.33 (stub)"
SH
chmod +x "$RUN/bin/"*
for name in claude codex pi opencode omp; do
  ln -s "$STUBS/$name" "$RUN/hookall/$name"
done
ln -s "$STUBS/claude" "$RUN/hookclaude/claude"
NODE_BIN="$(command -v node 2>/dev/null || true)"
if [ -n "$NODE_BIN" ]; then
  mkdir -p "$RUN/nodebin"
  ln -s "$(python3 -I -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$NODE_BIN")" "$RUN/nodebin/node"
  NODE_DIR="$RUN/nodebin:"
else
  NODE_DIR=""
fi
# The omp hook needs bun on the PATH the stubs run with (/usr/bin and /bin here).
if PATH=/usr/bin:/bin command -v bun >/dev/null 2>&1; then
  echo "note: bun is on /usr/bin or /bin, so the omp hook runs under it"
else
  echo "note: bun is not on the run's PATH, so the omp hook is recorded as SKIP"
fi

# The shell most worlds use: a bash that reads no profile, so the login PATH it
# reports is the launch PATH (the stubs, then the system bin), which has no
# ~/.local/bin, and an agent that runs through `$SHELL -lc` still finds its stub.
cat > "$RUN/shell/shell" <<'SH'
#!/bin/sh
exec /bin/bash --noprofile --norc "$@"
SH
chmod +x "$RUN/shell/shell"

# The environment dump every pane writes: NUL-separated NAME=value entries, as
# `env -0` writes them where that flag exists (BSD env has none).
cat > "$RUN/tools/dumpenv.py" <<'EOF_DUMP'
import os, sys
sys.stdout.buffer.write(b"\0".join(
    f"{key}={value}".encode("utf-8", "surrogateescape") for key, value in os.environ.items()) + b"\0")
EOF_DUMP
DUMP="$PY3 -I $RUN/tools/dumpenv.py"

# Hard links where the filesystem allows them, so a 400 MB binary exists once.
link_or_copy() { ln -f "$1" "$2" 2>/dev/null || cp "$1" "$2"; }

# A directory an app runs from: its sirio, the matching sirio-host, and the
# sirioctl beside it, which is where the startup copy comes from.
make_app() { # dir [sirioctl-source]
  mkdir -p "$1"
  link_or_copy "$APP_BIN" "$1/sirio"
  link_or_copy "$HOST_BIN" "$1/sirio-host"
  cp "${2:-$CTL}" "$1/sirioctl"
  chmod +x "$1/sirio" "$1/sirio-host" "$1/sirioctl"
}
# An AppImage-shaped mount: AppRun as shipped, the binaries under usr/bin, and a
# hook that exports GTK_THEME the way the linuxdeploy GTK plugin does.
make_mount() { # dir
  local dir=$1
  rm -rf "$dir"
  mkdir -p "$dir/usr/bin" "$dir/apprun-hooks"
  cp "$REPO_ROOT/Scripts/appimage/AppRun" "$dir/AppRun"
  chmod +x "$dir/AppRun"
  make_app "$dir/usr/bin"
  printf 'export GTK_THEME=leak\n' > "$dir/apprun-hooks/gtk.sh"
}

make_app "$RUN/app-a"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/rust/Cargo.toml" | head -n 1)"
[ -n "$VERSION" ] || { echo "no version in rust/Cargo.toml" >&2; exit 2; }

# ---------------------------------------------------------------- sirioctl helpers

ctl() { "$CTL" "$@"; }
ctl_json() { "$CTL" "$@" --json; }
# Writes INPUT and Enter into pane PANE. The app registers a tab's pane on its
# first render, so the write waits until the app knows the pane.
pane_known() { "$CTL" panel state "$1" >/dev/null 2>&1; }
pane_input() { # pane input
  wait_until 30 pane_known "$1" || fail "the app never knew pane $1"
  "$CTL" panel write --id "$1" --input "$2" --enter >/dev/null
}

# The first row's field of a --json reply.
field() { python3 -I -c 'import json, sys
row = json.load(sys.stdin)
row = row[0] if isinstance(row, list) else row
print(row.get(sys.argv[1], ""))' "$1"; }
pane_of() { python3 -I -c 'import json, sys; print(json.load(sys.stdin)[0]["pane"])'; }
count_rows() { python3 -I -c 'import json, sys; print(len(json.load(sys.stdin)))'; }
ctl_ready() { "$CTL" surface tabs read >/dev/null 2>&1; }

# `sirioctl ARGS --json`, its first row's field KEY, or nothing when the call failed.
reply_field() { # key args...
  local key=$1
  shift
  "$CTL" "$@" --json 2>/dev/null | field "$key" 2>/dev/null || true
}

# The agent status Sirio reports for a pane (`surface.terminal.state`).
agent_status() { reply_field agentStatus surface terminal state "$1"; }
wait_status() { # pane want seconds
  local got=""
  for _ in $(seq 1 $(( $3 * 5 ))); do
    got="$(agent_status "$1")"
    [ "$got" = "$2" ] && return 0
    sleep 0.2
  done
  fail "pane $1 never reached agentStatus '$2' (last: '$got')"
}

# The worktree id of a path, from the project list; nothing when it is not listed.
worktree_id_of() { # path
  ctl_json project list | python3 -I -c '
import json, os, sys
target = os.path.realpath(sys.argv[1])
for project in json.load(sys.stdin):
    for wt in json.loads(project["worktrees"]):
        if os.path.realpath(wt["path"]) == target:
            print(wt["id"])
            raise SystemExit
' "$1"
}

# Adds the repository at $1 as a project (unless the second argument is no-add),
# and selects its worktree at $1. Sets WT_ID.
select_repo() { # path [no-add]
  local id
  if [ "${2:-add}" != no-add ]; then ctl project add "$1" >/dev/null 2>&1 || true; fi
  id="$(worktree_id_of "$1")"
  [ -n "$id" ] || fail "sirioctl does not list the worktree $1"
  ctl select-workspace --workspace "$id" >/dev/null
  sleep 2
  WT_ID="$id"
}

settings_readable() { "$CTL" surface settings read >/dev/null 2>&1; }
# Opens Settings, so its read answers. The read carries the sirioctl card.
open_settings() {
  wait_until 30 ctl_ready
  "$CTL" surface settings open >/dev/null 2>&1 || true
  wait_until 20 settings_readable || fail "the Settings read never answered"
}
card_field() { reply_field "$1" surface settings read; }
card_has() { # key text
  case "$(card_field "$1")" in *"$2"*) return 0 ;; *) return 1 ;; esac
}

# The Open list's agent ids, once the list is expanded.
open_list_has() { # id
  case "$(reply_field openListAgents surface layout read)" in *"$1"*) return 0 ;; *) return 1 ;; esac
}

# ---------------------------------------------------------------- worlds

# A world is one set of state: a HOME, XDG directories, a database (so its host
# root sits beside it), a socket, a repository, and the PATH its apps run with.
world_open() { # name
  W="$RUN/$1"
  W_HOME="$W/home"
  W_REPO="$W/repo"
  W_SOCK="$W/c.sock"
  W_KNOB=1
  W_SHELL="$RUN/shell/shell"
  W_PATH="$RUN/bin:/usr/bin:/bin"
  mkdir -p "$W_HOME" "$W/xdg/config" "$W/xdg/data" "$W/xdg/state" "$W/xdg/cache" "$W/xdg/runtime" \
    "$W/hook-out" "$W/outside"
  chmod 700 "$W/xdg/runtime"
  WORLDS+=("$1")
  export SIRIO_SOCKET="$W_SOCK"
  export SIRIO_DB="$W/s.sqlite"
  [ -d "$W_REPO/.git" ] || repo_init "$W_REPO"
}

# A repository with one commit: what a project needs.
repo_init() { # dir
  mkdir -p "$1"
  git -C "$1" init -q -b main
  git -C "$1" config user.email tester@example.invalid
  git -C "$1" config user.name Tester
  echo "# app" > "$1/README.md"
  git -C "$1" add -A
  git -C "$1" commit -q -m first
}

# The environment an app starts with: an allow-list, then this launch's extras.
world_env() {
  WENV=(
    "HOME=$W_HOME" "PATH=$W_PATH" "SHELL=$W_SHELL" "TMPDIR=${TMPDIR:-/tmp}" "LANG=C.UTF-8"
    "XDG_CONFIG_HOME=$W/xdg/config" "XDG_DATA_HOME=$W/xdg/data" "XDG_STATE_HOME=$W/xdg/state"
    "XDG_CACHE_HOME=$W/xdg/cache" "XDG_RUNTIME_DIR=$W/xdg/runtime"
    "SIRIO_DB=$W/s.sqlite" "SIRIO_SOCKET=$W_SOCK" "SIRIO_CREDENTIALS=$W/cred.json"
    "SIRIO_WORKTREE_ID=leak" "HOOK_STUB_OUT=$W/hook-out" "ACP_ENV_DUMP=$CHAT_DUMP"
  )
  if [ "$W_KNOB" -eq 1 ]; then WENV+=("SIRIO_CLI_AS_RELEASE=1"); fi
  if [ "$STATE_ONLY" -eq 0 ]; then
    WENV+=("DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1")
    WENV+=(${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"})
  fi
}

# Starts EXE in the current world with NAME=VALUE extras, and waits for its socket.
app_start() { # exe [NAME=VALUE...]
  local exe=$1
  shift
  LAUNCH_N=$((LAUNCH_N + 1))
  CHAT_DUMP="$OUT_DIR/env/chat-$(basename "$W")-$LAUNCH_N.env"
  world_env
  rm -f "$W_SOCK"
  (cd "$W_REPO" && exec env -i "${WENV[@]}" "$@" "$exe" >>"$W/app.log" 2>&1) &
  APP_PID=$!
  APPS+=("$APP_PID")
  for _ in $(seq 1 300); do
    [ -S "$W_SOCK" ] && break
    sleep 0.1
  done
  [ -S "$W_SOCK" ] || fail "sirio never opened its control socket ($exe)"
  wait_until 60 ctl_ready || fail "sirio's control actions never became ready"
}

app_quit() {
  [ -n "$APP_PID" ] || return 0
  "$CTL" quit >/dev/null 2>&1 || true
  wait_until 20 dead "$APP_PID" || fail "sirio $APP_PID did not quit after sirioctl quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}

# Stops the current world's host, sessions included, so the next app starts one.
host_shutdown() {
  SIRIO_HOST_HOME="$W/host" "$PROBE" shutdown --force >/dev/null 2>&1 || true
}

# The pid the current world's host recorded in its state file.
host_pid() {
  python3 -I -c 'import glob, json, sys
files = glob.glob(sys.argv[1] + "/host-v*.json")
print(json.load(open(files[0]))["pid"] if files else "")' "$W/host" 2>/dev/null || true
}

# The `env -0` dump's keys. The dump is an artifact: no value is ever printed.
env_get() { python3 -I "$RUN/tools/envcheck.py" "$1" get "$2"; }
env_under() { python3 -I "$RUN/tools/envcheck.py" "$1" under "$2" "${@:3}"; }
env_first() { python3 -I "$RUN/tools/envcheck.py" "$1" first "$2" "$3"; }

# The checks every launcher-world pane's dump must pass. ARGS: dump, the stable
# directory, then the mounts that must not appear.
assert_pane_env() { # dump stable-dir mount...
  local dump=$1 stable_dir=$2
  shift 2
  wait_until 30 test -s "$dump" || fail "$(basename "$dump") was never written"
  env_first "$dump" PATH "$stable_dir" || fail "$(basename "$dump"): the first PATH entry is not the stable directory"
  assert_no_launcher_names "$dump" "$@"
}
# Every check of assert_pane_env except the stable directory's place in PATH.
assert_no_launcher_names() { # dump mount...
  local dump=$1
  shift
  local name
  name="$(basename "$dump")"
  env_under "$dump" PATH "$@" || fail "$name: PATH holds a mount entry"
  env_under "$dump" LD_LIBRARY_PATH "$@" || fail "$name: LD_LIBRARY_PATH holds a mount entry"
  env_under "$dump" XDG_DATA_DIRS "$@" || fail "$name: XDG_DATA_DIRS holds a mount entry"
  env_under "$dump" PWD "$@" || fail "$name: PWD names a mount entry"
  [ "$(env_get "$dump" GTK_THEME)" = __ABSENT__ ] || fail "$name carries GTK_THEME"
  [ "$(env_get "$dump" APPIMAGE)" = __ABSENT__ ] || fail "$name carries APPIMAGE"
  [ "$(env_get "$dump" SIRIO_LAUNCH_ENV)" = __ABSENT__ ] || fail "$name carries SIRIO_LAUNCH_ENV"
  [ "$(env_get "$dump" WAYLAND_DISPLAY)" = wayland-test ] || fail "$name lost WAYLAND_DISPLAY"
  [ -n "$(env_get "$dump" SIRIO_WORKTREE_ID)" ] || fail "$name has no SIRIO_WORKTREE_ID"
  [ "$(env_get "$dump" SIRIO_WORKTREE_ID)" != leak ] || fail "$name inherited the app's leak"
}

# The status a hook stub's log line reports. The stub writes one line per step.
hook_log_has() { grep -Fq "$2" "$W/hook-out/$1.log" 2>/dev/null; }
hook_go() { touch "$W/hook-out/$1.go"; }
# The pane the stub of AGENT wrote, once it wrote one.
hook_pane() { # agent: prints the pane the stub wrote; fails quietly when it wrote none
  wait_until 30 test -s "$W/hook-out/$1.pane" || return 1
  cat "$W/hook-out/$1.pane"
}
# Claude's turn: UserPromptSubmit reads running, then Stop reads needs-input.
# The stub has logged the step's outcome: its exit, or that the worktree holds no such hook.
hook_settled() { # agent prefix
  grep -Fq "$2 exit=" "$W/hook-out/$1.log" 2>/dev/null || grep -Fq "$2 no hook" "$W/hook-out/$1.log" 2>/dev/null
}
# The mutation check (--mutate claude-prompt-hook): drops UserPromptSubmit from the
# Claude worktree's settings before the stub reads them, so no such hook can fire.
mutate_prompt_hook() {
  python3 -I -c 'import json, sys
path = sys.argv[1]
settings = json.load(open(path))
settings.get("hooks", {}).pop("UserPromptSubmit", None)
json.dump(settings, open(path, "w"), indent=2)' "$W_REPO/.claude/settings.local.json"
  echo "MUTATION: UserPromptSubmit removed from $W_REPO/.claude/settings.local.json"
}
# Claude's hooks, each one seen in Sirio's status. A pane starts running (its spawn
# state), so the first observed change is SessionStart's needs-input; then
# UserPromptSubmit's running is a change from needs-input, and Stop's needs-input a
# change from running. Each step waits for the stub's log before it reads the status.
claude_turn() { # pane
  local pane=$1 stub_pane spawn
  stub_pane="$(hook_pane claude)" || fail "the claude stub never wrote its pane"
  [ "$stub_pane" = "$pane" ] || fail "the claude stub's pane $stub_pane is not the tab's $pane"
  spawn="$(agent_status "$pane")"
  [ "$spawn" != needs-input ] || fail "the claude pane already reads needs-input before any hook: a SessionStart needs-input could not be told from the spawn"
  hook_go claude
  wait_until 30 hook_settled claude "claude SessionStart" || fail "the SessionStart step never ran"
  hook_log_has claude "claude SessionStart exit=0" || fail "the worktree's settings hold no SessionStart hook"
  wait_status "$pane" needs-input 20
  echo "OK: the SessionStart hook moved the pane from '$spawn' (before any hook) to needs-input"
  hook_go claude
  wait_until 30 hook_settled claude "claude UserPromptSubmit" || fail "the UserPromptSubmit step never ran"
  hook_log_has claude "claude UserPromptSubmit exit=0" || fail "the worktree's settings hold no UserPromptSubmit hook, so none fired"
  wait_status "$pane" running 20
  echo "OK: the UserPromptSubmit hook moved the pane from needs-input to running"
  hook_go claude
  wait_until 30 hook_settled claude "claude Stop" || fail "the Stop step never ran"
  hook_log_has claude "claude Stop exit=0" || fail "the worktree's settings hold no Stop hook"
  wait_status "$pane" needs-input 20
  echo "OK: the Stop hook moved the pane from running to needs-input"
}

append_sha() { # label file
  printf '%s  %s\n' "$(python3 -I -c 'import hashlib, sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$2")" "$1" >> "$OUT_DIR/copies.sha256"
}
sha_of() { python3 -I -c 'import hashlib, sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$1"; }
size_of() { python3 -I -c 'import os, sys; print(os.path.getsize(sys.argv[1]))' "$1"; }
stable_of() { echo "$W/xdg/data/Sirio/bin/sirioctl"; }
# The login-profile world's PATH: the stub directories, then the system bin.
path_with_stubs() { echo "$RUN/hookall:${NODE_DIR}$RUN/bin:/usr/bin:/bin"; }

# ---------------------------------------------------------------- the cases

# A step that cannot run here: printed as SKIP and counted in the summary.
inner_skip() { echo "SKIP: $1"; INNER_SKIPS=$((INNER_SKIPS + 1)); }
realpath_of() { python3 -I -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$1"; }
# Every pane's agent status, one `id status` line per pane the app lists: the
# tabs and the host's sessions alike. A session that reports no status reads empty.
pane_statuses() {
  local id
  ctl_json panel list | python3 -I -c 'import json, sys
for row in json.load(sys.stdin):
    print(row["id"])' | while IFS= read -r id; do
    printf '%s %s\n' "$id" "$(agent_status "$id")"
  done
}

want() {
  case "$ONLY" in
    "") return 0 ;;
    *",$1,"*) return 0 ;;
  esac
  return 1
}
run_case() { # N name function
  want "$1" || return 0
  CASE_N=$1
  CASE_NAME=$2
  CASE_SKIPPED_NOW=0
  echo "== case $1: $2"
  "$3"
  if [ "$CASE_SKIPPED_NOW" -eq 1 ]; then return 0; fi
  echo "CASE $1 $2 PASS"
  CASES_RUN=$((CASES_RUN + 1))
}
# A case that needs a pane's own shell: skipped under --state-only. Returns 1 then.
needs_window() {
  [ "$STATE_ONLY" -eq 0 ] && return 0
  echo "SKIP: --state-only"
  echo "CASE $CASE_N $CASE_NAME SKIP (--state-only)"
  CASE_SKIPPED_NOW=1
  CASES_SKIPPED=$((CASES_SKIPPED + 1))
  return 1
}

# ---- case 1: the first launch

case_1() {
  world_open w1
  W_PATH="$RUN/hookclaude:$RUN/bin:/usr/bin:/bin"
  echo "step 1: the first launch installs the stable copy; a bare sirioctl and the card agree with the built one"
  app_start "$RUN/app-a/sirio"
  local stable link cli_dir resolved version_out
  stable="$(stable_of)"
  wait_until 20 test -f "$stable" || fail "the stable copy was never written"
  cmp -s "$stable" "$CTL" || fail "the stable copy is not the built sirioctl"
  append_sha stable-copy-first-launch "$stable"
  echo "OK: the stable copy is the built sirioctl"
  if [ "$OS" = linux ]; then
    link="$W_HOME/.local/bin/sirioctl"
    [ -L "$link" ] || fail "~/.local/bin/sirioctl is not a link"
    [ "$(realpath_of "$link")" = "$(realpath_of "$stable")" ] || fail "the link does not point at the stable copy"
    echo "OK: ~/.local/bin/sirioctl is a link to the stable copy"
    cli_dir="$W_HOME/.local/bin"
  else
    inner_skip "platform: the link checks run on Linux; macOS places its PATH link where its login PATH searches"
    cli_dir="$(dirname "$stable")"
  fi
  resolved="$(env -i HOME="$W_HOME" PATH="$cli_dir:/usr/bin:/bin" sh -c 'command -v sirioctl')"
  [ "$resolved" = "$cli_dir/sirioctl" ] || fail "a bare sirioctl resolves to $resolved, not $cli_dir/sirioctl"
  version_out="$(env -i HOME="$W_HOME" PATH="$cli_dir:/usr/bin:/bin" SIRIO_SOCKET="$W_SOCK" sirioctl version)" || fail "a bare sirioctl version failed"
  [ "$(printf '%s\n' "$version_out" | sed -n 1p)" = "sirioctl $VERSION" ] || fail "sirioctl names another build: $version_out"
  [ "$(printf '%s\n' "$version_out" | sed -n 2p)" = "running Sirio $VERSION" ] || fail "the running app is not $VERSION: $version_out"
  echo "OK: a bare sirioctl resolves through PATH to $cli_dir/sirioctl and names $VERSION for it and for the running app"
  open_settings
  wait_until 20 card_has cliPath "$stable" || fail "the card does not name the stable copy: $(card_field cliPath)"
  card_has cliVersion "Same build as Sirio" || fail "the card does not say the copy is the same build: $(card_field cliVersion)"
  echo "OK: the card names the stable copy and says it is the same build as Sirio"
  if [ "$OS" = linux ]; then
    card_has cliLink "does not search" || fail "the card does not say the login shell does not search the link: $(card_field cliLink)"
    card_has cliLink "$W_HOME/.local/bin" || fail "the card does not name the link's directory"
    echo "OK: the card says the login shell does not search $W_HOME/.local/bin"
  fi
  app_quit
}

# ---- case 2: replacement

case_2() {
  world_open w1
  W_PATH="$RUN/hookclaude:$RUN/bin:/usr/bin:/bin"
  echo "step 2: a different sirioctl beside the app replaces the stable copy; a process holding the old copy keeps its bytes"
  local stable old_size held
  stable="$(stable_of)"
  if [ ! -f "$stable" ]; then
    app_start "$RUN/app-a/sirio"
    app_quit
  fi
  [ -f "$stable" ] || fail "no stable copy to replace"
  make_app "$RUN/app-b" "$CTL"
  printf 'x' >> "$RUN/app-b/sirioctl"
  chmod +x "$RUN/app-b/sirioctl"
  old_size="$(size_of "$stable")"
  # The holder outlives the launch: it reads the old copy until cleanup ends it.
  sleep 300 < "$stable" &
  held=$!
  HOLDS+=("$held")
  app_start "$RUN/app-b/sirio"
  select_repo "$W_REPO"
  wait_until 20 cmp -s "$stable" "$RUN/app-b/sirioctl" || fail "the stable copy still holds the old bytes after launch"
  append_sha replaced-copy "$stable"
  echo "OK: after the launch the stable copy holds the replacement's bytes"
  if [ "$OS" = linux ]; then
    alive "$held" || fail "the process holding the old copy exited before the check"
    [ "$(size_of "/proc/$held/fd/0")" = "$old_size" ] || fail "the held copy changed size"
    echo "OK: the process holding the old copy still reads its $old_size bytes"
  else
    inner_skip "platform: no /proc, so the held descriptor is not read on $OS"
  fi
  local pane
  pane="$(ctl_json surface terminal open --agent claude | pane_of)"
  claude_turn "$pane"
  app_quit
}

# ---- case 3: a foreign file

case_3() {
  if [ "$OS" != linux ]; then
    echo "SKIP: platform: the foreign-file check is the Linux link's"
    echo "CASE $CASE_N $CASE_NAME SKIP (platform)"
    CASE_SKIPPED_NOW=1
    CASES_SKIPPED=$((CASES_SKIPPED + 1))
    return 0
  fi
  world_open w1
  W_PATH="$RUN/hookclaude:$RUN/bin:/usr/bin:/bin"
  echo "step 3: a foreign ~/.local/bin/sirioctl is left as it is, and the card names it"
  local link="$W_HOME/.local/bin/sirioctl"
  mkdir -p "$W_HOME/.local/bin"
  rm -f "$link"
  printf '#!/bin/sh\necho foreign\n' > "$link"
  chmod 755 "$link"
  local before after
  before="$(sha_of "$link")"
  app_start "$RUN/app-a/sirio"
  open_settings
  wait_until 20 card_has cliLink "is not Sirio's" || fail "the card does not name the foreign file: $(card_field cliLink)"
  after="$(sha_of "$link")"
  [ "$before" = "$after" ] || fail "the foreign file changed"
  [ ! -L "$link" ] || fail "the foreign file became a link"
  echo "OK: the foreign file is unchanged, and the card says it is not Sirio's"
  app_quit
}

# ---- case 4: the launcher

case_4() {
  needs_window || return 0
  world_open w2
  W_SHELL=/bin/bash
  W_PATH="$RUN/bin:/usr/bin:/bin"
  [ ! -e /usr/bin/sirioctl ] || fail "/usr/bin/sirioctl exists, so the login-shell check could not tell the copies apart"
  # The line a distribution's skeleton profile carries: it reassigns PATH, and
  # adds ~/.local/bin only when it exists.
  cat > "$W_HOME/.profile" <<'PROFILE'
PATH=/usr/bin:/bin
[ -d "$HOME/.local/bin" ] && PATH="$HOME/.local/bin:$PATH"
export PATH
PROFILE
  make_mount "$RUN/mnt"
  local stable_dir
  stable_dir="$(dirname "$(stable_of)")"
  echo "step 4: through the mount's AppRun, a terminal, a panel session, an ACP chat and a login shell hold no mount entry"
  app_start "$RUN/mnt/AppRun" "APPIMAGE=$RUN/fake.AppImage" "APPDIR=$RUN/mnt" "ARGV0=sirio" "OWD=$RUN" "WAYLAND_DISPLAY=wayland-test"
  select_repo "$W_REPO"
  rm -f "$OUT_DIR/env/terminal.env" "$OUT_DIR/env/panel.env"
  local terminal panel
  terminal="$(ctl_json surface terminal open --cmd "$DUMP > $OUT_DIR/env/terminal.env; sleep 300" | pane_of)"
  panel="$(ctl_json panel create --worktree "$WT_ID" --cmd "$DUMP > $OUT_DIR/env/panel.env; sleep 300" | field id)"
  [ -n "$panel" ] || fail "panel create named no id"
  ctl surface open-list open --entry show >/dev/null
  ctl surface open-list open --entry chat >/dev/null
  wait_until 30 open_list_has opencode || fail "the Open list never offered the opencode stub"
  rm -f "$CHAT_DUMP" "${CHAT_DUMP%.env}.cwd"
  ctl surface open-list open --entry agent:opencode >/dev/null
  assert_pane_env "$OUT_DIR/env/terminal.env" "$stable_dir" "$RUN/mnt" "$RUN/mnt2"
  assert_pane_env "$OUT_DIR/env/panel.env" "$stable_dir" "$RUN/mnt" "$RUN/mnt2"
  assert_pane_env "$CHAT_DUMP" "$stable_dir" "$RUN/mnt" "$RUN/mnt2"
  echo "OK: the terminal, the panel session and the ACP chat hold no mount entry, and their PATH starts with the stable directory"
  wait_until 30 test -s "${CHAT_DUMP%.env}.cwd" || fail "the chat agent wrote no working directory"
  local chat_cwd chat_pwd under
  chat_cwd="$(cat "${CHAT_DUMP%.env}.cwd")"
  [ "$(realpath_of "$chat_cwd")" = "$(realpath_of "$W_REPO")" ] || fail "the chat agent's process directory is $chat_cwd, not its worktree"
  for under in "$RUN/mnt" "$RUN/mnt2"; do
    case "$chat_cwd" in "$under"|"$under"/*) fail "the chat agent's process directory is inside $under" ;; esac
  done
  chat_pwd="$(env_get "$CHAT_DUMP" PWD)"
  if [ "$chat_pwd" != __ABSENT__ ]; then
    [ "$(realpath_of "$chat_pwd")" = "$(realpath_of "$W_REPO")" ] || fail "the chat agent's PWD is $chat_pwd, not its worktree"
  fi
  echo "OK: the chat agent's process directory and its PWD are its worktree, and neither is inside a mount"
  local login
  # A host session with no command runs the user's login shell, as a tab does.
  # A tab's own pane is refused by `panel write`; a host session takes the input.
  login="$(ctl_json panel create --worktree "$WT_ID" | field id)"
  [ -n "$login" ] || fail "panel create named no login shell"
  printf '%s\n' "$login" > "$W/login-pane"
  rm -f "$OUT_DIR/env/login-shell.env" "$OUT_DIR/env/login-shell.which"
  sleep 2
  pane_input "$login" "$DUMP > $OUT_DIR/env/login-shell.env; command -v sirioctl > $OUT_DIR/env/login-shell.which"
  wait_until 30 test -s "$OUT_DIR/env/login-shell.which" || fail "the login shell never wrote its sirioctl"
  assert_no_launcher_names "$OUT_DIR/env/login-shell.env" "$RUN/mnt" "$RUN/mnt2"
  local which_path link
  which_path="$(cat "$OUT_DIR/env/login-shell.which")"
  link="$W_HOME/.local/bin/sirioctl"
  if [ "$which_path" = "$link" ]; then
    echo "OK: in the login shell 'command -v sirioctl' prints the link path $link (the profile dropped the stable directory from PATH, and the link is what keeps sirioctl in panes)"
  elif [ "$which_path" = "$(stable_of)" ]; then
    echo "OK: in the login shell 'command -v sirioctl' prints the stable copy's path (the profile kept the prefix)"
  else
    fail "in the login shell sirioctl is neither the link nor the stable copy"
  fi
  echo "OK: the login shell's environment holds no mount entry in PATH, XDG_DATA_DIRS, PWD or LD_LIBRARY_PATH, and no launcher name"
  local gtk
  gtk="$(tr '\0' '\n' < "/proc/$APP_PID/environ" 2>/dev/null | grep -x 'GTK_THEME=leak' || true)"
  [ -n "$gtk" ] || fail "the app's own environment lost GTK_THEME=leak"
  echo "OK: the app's own environment still has GTK_THEME=leak"

  echo "step 4b: an app relaunched through a second mount (as Restart to update does) keeps its launch snapshot and its terminals hold no mount entry"
  make_mount "$RUN/mnt2"
  local before_len after_len
  before_len="$(launch_snapshot_length "$APP_PID")"
  local -a relaunch_env=()
  while IFS= read -r -d '' entry; do relaunch_env+=("$entry"); done < "/proc/$APP_PID/environ"
  LAUNCH_N=$((LAUNCH_N + 1))
  CHAT_DUMP="$OUT_DIR/env/chat-$(basename "$W")-$LAUNCH_N.env"
  relaunch_env+=("ACP_ENV_DUMP=$CHAT_DUMP")
  app_quit
  rm -f "$W_SOCK"
  (cd "$W_REPO" && exec env -i "${relaunch_env[@]}" "$RUN/mnt2/AppRun" >>"$W/app.log" 2>&1) &
  APP_PID=$!
  APPS+=("$APP_PID")
  wait_until 30 test -S "$W_SOCK" || fail "the relaunched app never opened its socket"
  wait_until 60 ctl_ready || fail "the relaunched app never became ready"
  after_len="$(launch_snapshot_length "$APP_PID")"
  [ "$before_len" = "$after_len" ] || fail "SIRIO_LAUNCH_ENV changed length from $before_len to $after_len: the mount nested"
  select_repo "$W_REPO"
  rm -f "$OUT_DIR/env/restart-terminal.env"
  local restarted
  restarted="$(ctl_json surface terminal open --cmd "$DUMP > $OUT_DIR/env/restart-terminal.env; sleep 300" | pane_of)"
  [ -n "$restarted" ] || fail "the relaunched app opened no terminal"
  assert_pane_env "$OUT_DIR/env/restart-terminal.env" "$stable_dir" "$RUN/mnt" "$RUN/mnt2"
  echo "OK: the relaunched app's terminal holds no entry of mnt or mnt2, and SIRIO_LAUNCH_ENV kept its $after_len characters"
  app_quit
}

# The length of SIRIO_LAUNCH_ENV in a process's environment, read from /proc.
launch_snapshot_length() { # pid
  tr '\0' '\n' < "/proc/$1/environ" | python3 -I -c 'import re, sys
match = re.search(r"^SIRIO_LAUNCH_ENV=(.*)$", sys.stdin.read(), re.M)
print(len(match.group(1)) if match else 0)'
}

# ---- case 5: the terminal opened before the quit still runs sirioctl

case_5() {
  needs_window || return 0
  world_open w2
  W_SHELL=/bin/bash
  W_PATH="$RUN/bin:/usr/bin:/bin"
  [ -f "$W/login-pane" ] || fail "no terminal from case 4 to ping through: run case 4 first"
  local login
  login="$(cat "$W/login-pane")"
  echo "step 5: after the quit, the mount removed, and a plain relaunch, sirioctl in the terminal opened before the quit answers ping"
  rm -rf "$RUN/mnt"
  app_start "$RUN/app-a/sirio"
  select_repo "$W_REPO" no-add
  rm -f "$OUT_DIR/ping.out"
  pane_input "$login" "sirioctl ping > $OUT_DIR/ping.out"
  wait_until 30 test -s "$OUT_DIR/ping.out" || fail "the terminal wrote no ping answer"
  [ "$(cat "$OUT_DIR/ping.out")" = pong ] || fail "sirioctl ping in the old terminal did not answer pong: $(cat "$OUT_DIR/ping.out")"
  echo "OK: sirioctl in the terminal opened before the quit answers pong"
  app_quit
}

# ---- case 4c: a host started with SIRIO_HOST_NO_ENV_EXACT=1

case_4c() {
  needs_window || return 0
  world_open w2
  W_SHELL=/bin/bash
  W_PATH="$RUN/bin:/usr/bin:/bin"
  echo "step 4c: a host started with SIRIO_HOST_NO_ENV_EXACT=1 carries the launcher's names present and blank"
  local old_host new_host
  old_host="$(host_pid)"
  host_shutdown
  if [ -n "$old_host" ]; then
    wait_until 20 dead "$old_host" || fail "the old host $old_host did not stop"
  fi
  make_mount "$RUN/mnt3"
  app_start "$RUN/mnt3/AppRun" "APPIMAGE=$RUN/fake.AppImage" "APPDIR=$RUN/mnt3" "ARGV0=sirio" "OWD=$RUN" "WAYLAND_DISPLAY=wayland-test" "SIRIO_HOST_NO_ENV_EXACT=1"
  select_repo "$W_REPO" no-add
  local fresh
  rm -f "$OUT_DIR/env/noexact-terminal.env"
  fresh="$(ctl_json surface terminal open --cmd "$DUMP > $OUT_DIR/env/noexact-terminal.env; sleep 300" | pane_of)"
  [ -n "$fresh" ] || fail "no terminal under the fresh host"
  wait_until 30 test -s "$OUT_DIR/env/noexact-terminal.env" || fail "the terminal under the fresh host never wrote its dump"
  new_host="$(host_pid)"
  [ -n "$new_host" ] && [ "$new_host" != "$old_host" ] || fail "the host was not replaced (old '$old_host', new '$new_host')"
  [ "$(env_get "$OUT_DIR/env/noexact-terminal.env" GTK_THEME)" = "" ] || fail "GTK_THEME is not present and blank under the no-exact host"
  [ "$(env_get "$OUT_DIR/env/noexact-terminal.env" LD_LIBRARY_PATH)" = "" ] || fail "LD_LIBRARY_PATH is not present and blank under the no-exact host"
  echo "OK: under the no-exact host GTK_THEME and LD_LIBRARY_PATH are present and blank; the host was replaced ($old_host -> $new_host)"
  app_quit
  host_shutdown
}

# ---- case 6: hooks

case_6() {
  needs_window || return 0
  world_open w3
  W_SHELL=/bin/bash
  W_PATH="$(path_with_stubs)"
  echo "step 6: each agent's stub reports the status its adapter maps; the global Claude Stop command changes a pane's status from inside that pane and not from outside it"
  app_start "$RUN/app-a/sirio"
  select_repo "$W_REPO"
  local agent pane
  for agent in claude codex opencode omp pi; do
    pane="$(ctl_json surface terminal open --agent "$agent" | pane_of)"
    echo "$pane" > "$W/$agent.pane-id"
    if [ "$agent" = claude ] && [ "$MUTATE" = claude-prompt-hook ]; then
      mutate_prompt_hook
    fi
    [ "$(hook_pane "$agent" || true)" = "$pane" ] || fail "the $agent stub's pane is not the tab's $pane"
  done
  echo "OK: every agent tab's stub wrote the pane id the tab reports"

  claude_turn "$(cat "$W/claude.pane-id")"

  local codex_pane spawn
  codex_pane="$(cat "$W/codex.pane-id")"
  spawn="$(agent_status "$codex_pane")"
  [ "$spawn" != needs-input ] || fail "the codex pane already reads needs-input before its hook"
  hook_go codex
  wait_until 30 hook_log_has codex "codex notify exit=0" || fail "the Codex notify hook never ran"
  wait_status "$codex_pane" needs-input 20
  echo "OK: Codex's notify hook moved its pane from '$spawn' (before the hook) to needs-input"

  local opencode_pane ref
  opencode_pane="$(cat "$W/opencode.pane-id")"
  if hook_log_has opencode "SKIP: node is not on PATH"; then
    echo "opencode: SKIP: node is not on PATH" > "$OUT_DIR/hooks/opencode.status"
    inner_skip "node is not on PATH (the OpenCode plugin was not run)"
  else
    hook_go opencode
    wait_until 30 hook_log_has opencode "opencode session.updated exit=0" || fail "the OpenCode plugin never ran"
    ref="$(python3 -I -c 'import sqlite3, sys
connection = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
row = connection.execute("SELECT reference FROM session_ref WHERE session = ?", (sys.argv[2],)).fetchone()
print(row[0] if row else "")' "$W/s.sqlite" "$opencode_pane")"
    [ "$ref" = stub-opencode ] || fail "the OpenCode session ref for $opencode_pane is '$ref', not stub-opencode"
    echo "opencode: session ref stub-opencode; no status (its plugin sends only the session ref)" > "$OUT_DIR/hooks/opencode.status"
    echo "OK: OpenCode's session ref reached the database for its pane (its plugin sends no status)"
  fi

  local omp_pane
  omp_pane="$(cat "$W/omp.pane-id")"
  wait_until 15 test -s "$W/hook-out/omp.log" || true
  if hook_log_has omp "SKIP: bun is not on PATH"; then
    echo "omp: SKIP: bun is not on PATH" > "$OUT_DIR/hooks/omp.status"
    inner_skip "bun is not on PATH (the omp hook was not run)"
  else
    spawn="$(agent_status "$omp_pane")"
    [ "$spawn" != done ] || fail "the omp pane already reads done before its hooks"
    local step
    for step in 1 2 3 4; do
      hook_go omp
      wait_until 30 bash -c "[ \"\$(grep -c 'exit=' '$W/hook-out/omp.log')\" -ge $step ]" || fail "omp step $step never ran"
    done
    wait_status "$omp_pane" done 20
    echo "omp: done after its session shutdown" > "$OUT_DIR/hooks/omp.status"
    echo "OK: omp's session shutdown moved its pane from '$spawn' (before the hooks) to done"
  fi

  local pi_pane
  pi_pane="$(cat "$W/pi.pane-id")"
  # Pi sends no hook: its status is what its title says. The stub prints its idle
  # "π - <cwd>" title right after it writes its pane file, so the spawn status
  # cannot be read before the title is set; the E2E asserts only what the title
  # layer reports, which sirio_activity reads as needs-input (detect_pi_family).
  wait_status "$pi_pane" needs-input 20
  echo "pi: needs-input from its idle title (no hook; sirio_activity reads the π title as idle)" > "$OUT_DIR/hooks/pi.status"
  echo "OK: Pi's idle π title reads needs-input (the title layer; Pi sends no hook)"

  echo "step 6b: the global Claude Stop command, from ~/.claude/settings.json, changes a pane's status from inside that pane, and nothing outside it"
  local settings="$W_HOME/.claude/settings.json" running_cmd stop_cmd gate_pane before after panes
  ctl surface settings act --action install-hooks >/dev/null || fail "install-hooks did not run"
  [ -f "$settings" ] || fail "install-hooks wrote no settings file"
  running_cmd="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"])' "$settings")"
  stop_cmd="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["hooks"]["Stop"][0]["hooks"][0]["command"])' "$settings")"
  case "$stop_cmd" in *" notify --status needs-input --stdin-json") ;; *) fail "the global Stop command has an unexpected shape" ;; esac
  case "$stop_cmd" in *"--session"*) fail "the global Stop command names a session" ;; esac
  printf '#!/bin/sh\n%s <<'"'"'JSON'"'"'\n{"session_id":"global-hook"}\nJSON\n' "$running_cmd" > "$W/global-running.sh"
  printf '#!/bin/sh\n%s <<'"'"'JSON'"'"'\n{"session_id":"global-hook"}\nJSON\n' "$stop_cmd" > "$W/global-stop.sh"
  # The commands run inside a terminal tab, whose status `surface terminal state`
  # reads. The tab waits at each gate this step opens, then runs one hook.
  rm -f "$W"/global-gate-* "$OUT_DIR"/hooks/global-*.rc
  gate_pane="$(ctl_json surface terminal open --cmd "while [ ! -f $W/global-gate-1 ]; do sleep 0.2; done; sh $W/global-stop.sh; echo \$? > $OUT_DIR/hooks/global-stop-1.rc; while [ ! -f $W/global-gate-2 ]; do sleep 0.2; done; sh $W/global-running.sh; echo \$? > $OUT_DIR/hooks/global-running.rc; while [ ! -f $W/global-gate-3 ]; do sleep 0.2; done; sh $W/global-stop.sh; echo \$? > $OUT_DIR/hooks/global-stop-2.rc; sleep 300" | pane_of)"
  spawn="$(agent_status "$gate_pane")"
  [ "$spawn" != needs-input ] || fail "the global-hook pane already reads needs-input before its Stop command"
  : > "$W/global-gate-1"
  wait_until 30 test -s "$OUT_DIR/hooks/global-stop-1.rc" || fail "the global Stop command never finished in the pane"
  [ "$(cat "$OUT_DIR/hooks/global-stop-1.rc")" = 0 ] || fail "the global Stop command failed in the pane"
  wait_status "$gate_pane" needs-input 20
  echo "OK: the global Stop command, run inside the pane, moved it from '$spawn' (before the command) to needs-input"
  : > "$W/global-gate-2"
  wait_until 30 test -s "$OUT_DIR/hooks/global-running.rc" || fail "the global UserPromptSubmit command never finished in the pane"
  [ "$(cat "$OUT_DIR/hooks/global-running.rc")" = 0 ] || fail "the global UserPromptSubmit command failed in the pane"
  wait_status "$gate_pane" running 20
  echo "OK: the global UserPromptSubmit command, run inside the pane, moved it from needs-input to running"
  before="$(pane_statuses)"
  printf '%s\n' "$before" | grep -qxF "$gate_pane running" || fail "the pane is not listed with status running before the outside run: $(echo "$before" | tr '\n' ';')"
  panes="$(printf '%s\n' "$before" | grep -c .)"
  env -i HOME="$W_HOME" PATH="$(dirname "$(stable_of)"):/usr/bin:/bin" SIRIO_SOCKET="$W_SOCK" SIRIO_DB="$W/s.sqlite" \
    sh "$W/global-stop.sh" </dev/null >/dev/null 2>&1 || fail "the global Stop command exited non-zero outside a pane"
  sleep 2
  after="$(pane_statuses)"
  [ "$before" = "$after" ] || fail "the global Stop command outside a pane changed a pane's status: before [$(echo "$before" | tr '\n' ';')] after [$(echo "$after" | tr '\n' ';')]"
  echo "OK: outside any pane, with this world's socket and database, the global Stop command exits 0 and no pane's status changed ($panes panes read before and after)"
  : > "$W/global-gate-3"
  wait_until 30 test -s "$OUT_DIR/hooks/global-stop-2.rc" || fail "the global Stop command never finished in the pane"
  [ "$(cat "$OUT_DIR/hooks/global-stop-2.rc")" = 0 ] || fail "the global Stop command failed in the pane"
  wait_status "$gate_pane" needs-input 20
  echo "OK: the global Stop command, run inside the pane again, moved it from running to needs-input"
  app_quit
}

# ---- case 7: the skill

case_7() {
  world_open w7
  W_SHELL=/bin/bash
  W_PATH="$(path_with_stubs)"
  echo "step 7: every adapter's worktree copy of the skill equals skills/sirio/SKILL.md, marker included"
  app_start "$RUN/app-a/sirio"
  select_repo "$W_REPO"
  local agent copy claude_copy shared_copy marker
  for agent in claude codex pi opencode omp; do
    ctl_json surface terminal open --agent "$agent" >/dev/null
  done
  claude_copy="$W_REPO/.claude/skills/sirio/SKILL.md"
  shared_copy="$W_REPO/.agents/skills/sirio/SKILL.md"
  wait_until 30 test -f "$claude_copy" || fail "no Claude skill copy at $claude_copy"
  wait_until 30 test -f "$shared_copy" || fail "no shared skill copy at $shared_copy"
  marker="$(grep -m 1 '^<!-- Machine-managed by Sirio' "$SKILL")"
  [ -n "$marker" ] || fail "the bundled skill carries no marker"
  for copy in "$claude_copy" "$shared_copy"; do
    cmp -s "$copy" "$SKILL" || fail "$copy differs from skills/sirio/SKILL.md"
    [ "$(grep -cF "$marker" "$copy")" = 1 ] || fail "$copy does not carry the marker once"
  done
  echo "OK: the Claude copy and the shared copy both equal the bundled skill, marker included"
  echo "step 7b: the served-skill test passes"
  if cargo nextest --version >/dev/null 2>&1; then
    (cd "$REPO_ROOT/rust" && cargo nextest run -p sirio_control --test skill_is_served --no-fail-fast) || fail "the served-skill test failed"
  else
    echo "note: cargo-nextest is not installed here; running the test under cargo test"
    (cd "$REPO_ROOT/rust" && cargo test -p sirio_control --test skill_is_served) || fail "the served-skill test failed"
  fi
  echo "OK: skill_is_served passes"
  app_quit
}

# ---- case 8: a debug build without the knob

case_8() {
  world_open w5
  W_KNOB=0
  W_PATH="$RUN/bin:/usr/bin:/bin"
  echo "step 8: a debug build without the knob keeps no release copy and no link; its hooks name the release path"
  app_start "$RUN/app-a/sirio"
  wait_until 30 test -d "$W/xdg/data/Sirio/bin-debug" || fail "the debug copy directory never appeared (the startup job did not run)"
  sleep 3
  [ ! -e "$W/xdg/data/Sirio/bin" ] || fail "a debug build without the knob created the release directory"
  [ ! -e "$W_HOME/.local/bin/sirioctl" ] || fail "a debug build without the knob linked ~/.local/bin/sirioctl"
  echo "OK: no release directory and no ~/.local/bin link exist"
  ctl surface settings act --action install-hooks >/dev/null || fail "install-hooks did not run"
  python3 -I -c 'import json, sys
hook = json.load(open(sys.argv[1]))["hooks"]["Stop"][0]["hooks"][0]["command"]
assert "/Sirio/bin/sirioctl" in hook, hook
assert "bin-debug" not in hook, hook' "$W_HOME/.claude/settings.json" || fail "the global hooks do not name the release path"
  echo "OK: the global hooks name the release copy, never bin-debug"
  ctl surface settings act --action install-skill >/dev/null || fail "install-skill did not run"
  local copy
  for copy in "$W_HOME/.claude/skills/sirio/SKILL.md" "$W_HOME/.agents/skills/sirio/SKILL.md" "$W/xdg/config/opencode/skills/sirio/SKILL.md"; do
    wait_until 20 test -f "$copy" || fail "install-skill wrote no copy at $copy"
    cmp -s "$copy" "$SKILL" || fail "$copy differs from the bundled skill"
  done
  echo "OK: install-skill wrote the three user copies, each equal to the bundled skill"
  app_quit
}

# ---- case 9: SIRIO_WORKTREE_ID

case_9() {
  world_open w4
  W_PATH="$RUN/bin:/usr/bin:/bin"
  echo "step 9: each session carries the id of the deepest worktree that holds its directory, and none carries the app's leak"
  git -C "$W_REPO" worktree add -q -b nested "$W_REPO/nested-wt"
  app_start "$RUN/app-a/sirio"
  ctl project add "$W_REPO" >/dev/null 2>&1 || true
  select_repo "$W_REPO" no-add
  local main_id nested_id
  main_id="$(worktree_id_of "$W_REPO")"
  nested_id="$(worktree_id_of "$W_REPO/nested-wt")"
  [ -n "$main_id" ] && [ -n "$nested_id" ] && [ "$main_id" != "$nested_id" ] || fail "the project does not list both worktrees"
  local terminal panel split nested outside name
  rm -f "$OUT_DIR"/env/9-*.env
  terminal="$(ctl_json surface terminal open --cmd "$DUMP > $OUT_DIR/env/9-terminal.env; sleep 300" | pane_of)"
  panel="$(ctl_json panel create --cwd "$W_REPO" --cmd "$DUMP > $OUT_DIR/env/9-panel.env; sleep 300" | field id)"
  split="$(ctl_json panel split --from "$panel" --direction right --cmd "$DUMP > $OUT_DIR/env/9-split.env; sleep 300" | field id)"
  nested="$(ctl_json panel create --cwd "$W_REPO/nested-wt" --cmd "$DUMP > $OUT_DIR/env/9-nested.env; sleep 300" | field id)"
  outside="$(ctl_json panel create --cwd "$W/outside" --cmd "$DUMP > $OUT_DIR/env/9-outside.env; sleep 300" | field id)"
  [ -n "$terminal$panel$split$nested$outside" ] || fail "a session named no id"
  for name in 9-terminal 9-panel 9-split 9-nested 9-outside; do
    wait_until 30 test -s "$OUT_DIR/env/$name.env" || fail "$name never wrote its dump"
  done
  [ "$(env_get "$OUT_DIR/env/9-terminal.env" SIRIO_WORKTREE_ID)" = "$main_id" ] || fail "the terminal tab carries the wrong worktree id"
  [ "$(env_get "$OUT_DIR/env/9-panel.env" SIRIO_WORKTREE_ID)" = "$main_id" ] || fail "the panel session carries the wrong worktree id"
  [ "$(env_get "$OUT_DIR/env/9-split.env" SIRIO_WORKTREE_ID)" = "$main_id" ] || fail "the split carries the wrong worktree id"
  [ "$(env_get "$OUT_DIR/env/9-nested.env" SIRIO_WORKTREE_ID)" = "$nested_id" ] || fail "the session in the nested worktree does not carry the deepest id"
  [ "$(env_get "$OUT_DIR/env/9-outside.env" SIRIO_WORKTREE_ID)" = __ABSENT__ ] || fail "a session in no worktree carries SIRIO_WORKTREE_ID"
  echo "OK: the terminal, the panel and the split carry the main worktree's id, and the nested session carries the nested one's"
  echo "OK: a session in no worktree has no SIRIO_WORKTREE_ID, and the app's leak is not inherited"
  app_quit
}

# ---- case 10: the skill's git exclude

case_10() {
  needs_window || return 0
  world_open w3
  W_SHELL=/bin/bash
  W_PATH="$(path_with_stubs)"
  local exclude_line='.agents/skills/sirio/'
  echo "step 10: the shared-skill adapters write their exclude line once; a Claude-only repository gets none"
  app_start "$RUN/app-a/sirio"
  select_repo "$W_REPO"
  local agent
  for agent in codex pi opencode omp; do
    ctl_json surface terminal open --agent "$agent" >/dev/null
  done
  local common count
  common="$(git -C "$W_REPO" rev-parse --git-common-dir)"
  case "$common" in /*) ;; *) common="$W_REPO/$common" ;; esac
  wait_until 30 bash -c "grep -qxF '$exclude_line' '$common/info/exclude'" || fail "the exclude line never appeared in $common/info/exclude"
  count="$(grep -cxF "$exclude_line" "$common/info/exclude")"
  [ "$count" = 1 ] || fail "the exclude line appears $count times, not once"
  echo "OK: $exclude_line appears exactly once in the shared info/exclude"
  local repo_b="$W/repo2" common_b count_b
  repo_init "$repo_b"
  select_repo "$repo_b"
  ctl_json surface terminal open --agent claude >/dev/null
  wait_until 30 test -f "$repo_b/.claude/skills/sirio/SKILL.md" || fail "the Claude-only tab wrote no skill"
  common_b="$(git -C "$repo_b" rev-parse --git-common-dir)"
  case "$common_b" in /*) ;; *) common_b="$repo_b/$common_b" ;; esac
  count_b="$(grep -cxF "$exclude_line" "$common_b/info/exclude" 2>/dev/null || true)"
  [ "${count_b:-0}" = 0 ] || fail "a Claude-only repository got the exclude line"
  echo "OK: the Claude-only repository's info/exclude has no $exclude_line"
  app_quit
}

# ---- case 11: startup refresh of the user copies

case_11() {
  local marker
  marker="$(grep -m 1 '^<!-- Machine-managed by Sirio' "$SKILL")"
  echo "step 11: a launch with the knob refreshes a marked user copy and leaves an unmarked one and a missing one as they are"
  world_open w6
  W_PATH="$RUN/bin:/usr/bin:/bin"
  local marked="$W_HOME/.claude/skills/sirio/SKILL.md"
  local unmarked="$W_HOME/.agents/skills/sirio/SKILL.md"
  local missing="$W/xdg/config/opencode/skills/sirio/SKILL.md"
  mkdir -p "$(dirname "$marked")" "$(dirname "$unmarked")"
  printf '%s\nan older copy\n' "$marker" > "$marked"
  printf 'my own notes, not Sirio.\n' > "$unmarked"
  cp "$unmarked" "$RUN/unmarked.seed"
  app_start "$RUN/app-a/sirio"
  wait_until 30 cmp -s "$marked" "$SKILL" || fail "the marked copy was not refreshed to the bundled skill"
  echo "OK: the marked copy equals the bundled skill after the launch"
  wait_until 30 test -d "$W/xdg/data/Sirio/bin" || fail "the startup job never finished"
  sleep 3
  cmp -s "$unmarked" "$RUN/unmarked.seed" || fail "the unmarked user file changed"
  [ ! -e "$missing" ] || fail "a refresh created a copy where none existed"
  echo "OK: the unmarked file is byte-identical, and no copy was created where none existed"
  app_quit

  world_open w6b
  W_KNOB=0
  W_PATH="$RUN/bin:/usr/bin:/bin"
  mkdir -p "$(dirname "$W_HOME/.claude/skills/sirio/SKILL.md")"
  printf '%s\nan older copy\n' "$marker" > "$W_HOME/.claude/skills/sirio/SKILL.md"
  cp "$W_HOME/.claude/skills/sirio/SKILL.md" "$RUN/stale.seed"
  echo "step 11b: a debug launch without the knob, in a fresh HOME, leaves a stale marked copy stale"
  app_start "$RUN/app-a/sirio"
  wait_until 30 test -d "$W/xdg/data/Sirio/bin-debug" || fail "the debug launch never finished its startup job"
  sleep 3
  cmp -s "$W_HOME/.claude/skills/sirio/SKILL.md" "$RUN/stale.seed" || fail "a debug launch without the knob refreshed the stale copy"
  echo "OK: the debug launch without the knob left the stale marked copy as it was"
  app_quit
}

# ---- case 12: panel wait

case_12() {
  world_open w4
  W_PATH="$RUN/bin:/usr/bin:/bin"
  echo "step 12: panel wait exits with the waited child's code"
  app_start "$RUN/app-a/sirio"
  ctl project add "$W_REPO" >/dev/null 2>&1 || true
  select_repo "$W_REPO" no-add
  local id rc
  id="$(ctl_json panel create --cmd 'exit 3' | field id)"
  rc=0
  "$CTL" panel wait --id "$id" >/dev/null || rc=$?
  [ "$rc" = 3 ] || fail "panel wait for 'exit 3' exited $rc"
  echo "OK: panel wait for a child that exits 3 exits 3"
  id="$(ctl_json panel create --cmd 'exit 0' | field id)"
  rc=0
  "$CTL" panel wait --id "$id" >/dev/null || rc=$?
  [ "$rc" = 0 ] || fail "panel wait for 'exit 0' exited $rc"
  echo "OK: panel wait for a child that exits 0 exits 0"
  app_quit
}

# ---- case 13: --dry-run

case_13() {
  world_open w4
  W_PATH="$RUN/bin:/usr/bin:/bin"
  echo "step 13: --dry-run prints one request and creates no pane"
  app_start "$RUN/app-a/sirio"
  ctl project add "$W_REPO" >/dev/null 2>&1 || true
  select_repo "$W_REPO" no-add
  local before after lines
  before="$(ctl_json panel list | count_rows)"
  lines="$("$CTL" --dry-run panel create --cmd x)"
  [ "$(printf '%s\n' "$lines" | grep -c .)" = 1 ] || fail "--dry-run did not print exactly one line"
  printf '%s' "$lines" | python3 -I -c 'import json, sys
request = json.loads(sys.stdin.read())
assert request["method"] == "panel.create", request
assert request["params"]["cmd"] == "x", request' || fail "--dry-run did not print the panel.create request"
  after="$(ctl_json panel list | count_rows)"
  [ "$before" = "$after" ] || fail "--dry-run created a pane ($before -> $after)"
  echo "OK: --dry-run printed one panel.create request, and the pane count stayed at $after"
  app_quit
}

# ---------------------------------------------------------------- run

echo "run started"
run_case 1 "first launch: the stable copy, the link and the login PATH" case_1
run_case 2 "replacement: the stable copy is swapped and the held copy keeps its bytes" case_2
run_case 3 "a foreign ~/.local/bin/sirioctl is left alone" case_3
run_case 4 "the AppImage launcher: no mount entry in any pane, and a relaunch does not nest" case_4
run_case 5 "quit, remove the mount, relaunch: the old terminal still runs sirioctl" case_5
run_case 4 "a host started with SIRIO_HOST_NO_ENV_EXACT=1 keeps the names present and blank" case_4c
run_case 6 "hooks: each stub reports its status, and the global Stop command works in a pane" case_6
run_case 7 "skill: every worktree copy equals the bundled skill, and the served test passes" case_7
run_case 8 "a debug build without the knob: no release copy, no link, the release path in the hooks" case_8
run_case 9 "SIRIO_WORKTREE_ID is the deepest worktree's id" case_9
run_case 10 "the skill's git exclude is written once, by the shared-skill adapters" case_10
run_case 11 "startup refresh of the user copies" case_11
run_case 12 "panel wait exits with the child's code" case_12
run_case 13 "--dry-run prints one request and creates no pane" case_13

# Teardown: nothing of this run may outlive it. Every host is shut down, every
# app quit, every holder and every pane session of this run ended.
for world in ${WORLDS[@]+"${WORLDS[@]}"}; do
  SIRIO_HOST_HOME="$RUN/$world/host" "$PROBE" shutdown --force >/dev/null 2>&1 || true
done
app_quit 2>/dev/null || true
for pid in ${HOLDS[@]+"${HOLDS[@]}"}; do kill "$pid" 2>/dev/null || true; done
for pid in ${APPS[@]+"${APPS[@]}"}; do kill "$pid" 2>/dev/null || true; done
leftover=""
for _ in $(seq 1 25); do
  leftover="$(pgrep -f "$RUN" 2>/dev/null || true)$(pgrep -f "$OUT_DIR/env/" 2>/dev/null || true)"
  [ -z "$leftover" ] && break
  sleep 0.2
done
if [ -n "$leftover" ]; then
  echo "FAIL: processes of this run survived teardown: $(echo "$leftover" | tr '\n' ' ')" >&2
  exit 1
fi
echo "OK: no process of this run is left"

echo "cases: $CASES_RUN passed, $CASES_SKIPPED skipped; $INNER_SKIPS inner skip(s) printed inside the cases; $(( $(date +%s) - START_ALL ))s"
if [ -n "$ONLY" ]; then
  echo "SIRIOCTL E2E PARTIAL"
else
  echo "SIRIOCTL E2E OK"
fi
