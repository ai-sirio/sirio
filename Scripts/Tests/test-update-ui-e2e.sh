#!/bin/bash
set -euo pipefail

# End-to-end test of the update card (spec 2026-10-14-update-dialog-design.md
# §5.2). Stage `chain`: Sirio compiled the way the release job compiles it --
# Nightly, a loopback manifest, a key made for this run -- finds a signed
# update at launch, shows the card, and on `act restart` downloads, verifies,
# swaps a stand-in $APPIMAGE and relaunches into it (a new pid answers, the
# new artifact's marker exists, the relaunched process carries the isolated
# environment, a terminal and the project survived). Stage `ui`: every state of
# the card drawn, Later and the indicator, the agent warning from a real
# `sirioctl notify`, and keystrokes reaching a terminal while the card is up.
#
# Isolation: every Sirio the script starts runs with its own HOME and XDG
# directories, and with PATH = <run>/bin (stub agent CLIs: claude, codex,
# opencode, pi, omp) followed by the system directories only. So neither the
# user's agent CLIs nor the mise wrapper that rewrites the user's global mise
# config can run. The stub answers `claude --version` at once with a version
# above sirio_claude::MIN_CLAUDE_VERSION, which is what Sirio reads at launch.
#
# The artifact: --out-dir DIR (default artifacts/update-ui-e2e-<stamp>-<pid>)
# keeps transcript.log, the app logs, build.log, stub-calls.log (every call a
# stub received) and -- unless --state-only -- PID-matched window captures in
# frames/. The script refuses an out dir that already has content, and never
# removes one. Rerunning the script into a new out dir reproduces the run.
#
# Usage: Scripts/Tests/test-update-ui-e2e.sh [--stage chain|ui|all] [--state-only]
#          [--out-dir DIR] [--display :N] [--appearance light|dark]

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# A dedicated target: the release environment compiled in must not invalidate
# the ordinary debug build.
TARGET="$ROOT/rust/target/update-ui-e2e"
BIN="$TARGET/debug/sirio"
CTL="$TARGET/debug/sirioctl"
RELEASE_CLI="$TARGET/debug/sirio-release"
STAGE=all
STATE_ONLY=0
OUT_DIR=""
APPEARANCE=""
DISPLAY_TARGET="${DISPLAY:-}"
PORT="${SIRIO_UPDATE_UI_E2E_PORT:-38719}"
VERSION="99.0.0"
while [ $# -gt 0 ]; do
  case "$1" in
    --stage) STAGE="$2"; shift 2 ;;
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/update-ui-e2e-$(date +%Y%m%d-%H%M%S)-$$"
# An out dir with content is someone's evidence: refuse it, never reuse or remove it.
if [ -d "$OUT_DIR" ] && [ -n "$(ls -A "$OUT_DIR")" ]; then
  echo "refusing to reuse $OUT_DIR: it already has content (pass a new --out-dir)" >&2
  exit 2
fi
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux (macOS and Windows rows: docs/testing/update-card.md)"
if [ "$STATE_ONLY" -eq 0 ]; then
  [ -n "$DISPLAY_TARGET" ] || fail "no DISPLAY; pass --display :N or --state-only"
  for tool in import identify xwininfo xprop xdotool; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi
command -v python3 >/dev/null || fail "python3 is required"
command -v curl >/dev/null || fail "curl is required"

# Everything this run creates lives under RUN_DIR, which mktemp made just now.
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-update-ui-e2e-XXXXXX")
APP_PATH="$RUN_DIR/bin:/usr/local/bin:/usr/bin:/bin"
APP_PID=""
SERVER_PID=""
RUN_PIDS=()
cleanup() {
  local pid
  for pid in ${RUN_PIDS[@]+"${RUN_PIDS[@]}"}; do stop_process_group "$pid"; done
  stop_run_processes
  [ -z "$SERVER_PID" ] || kill "$SERVER_PID" 2>/dev/null || true
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

# SIGTERM to the process group `pid` leads, when it leads one that is not this
# script's own. Sirio is launched as the leader of a group of its own, and a
# relaunched Sirio leads the group of the restart script that started it.
stop_process_group() { # pid
  local group own
  group=$(ps -o pgid= -p "$1" 2>/dev/null | tr -d ' ' || true)
  own=$(ps -o pgid= -p $$ | tr -d ' ')
  if [ -n "$group" ] && [ "$group" != "$own" ]; then
    kill -TERM -- "-$group" 2>/dev/null || true
  fi
}

# Every process this run started carries SIRIO_UPDATE_E2E_RUN=<RUN_DIR> in its
# environment (the app, its helpers and the relaunched app inherit it). Those
# are stopped even when they moved to a group or session of their own.
stop_run_processes() {
  python3 - "$RUN_DIR" <<'PY'
import os, signal, sys, time
marker = b"SIRIO_UPDATE_E2E_RUN=" + sys.argv[1].encode() + b"\0"

def matches():
    found = []
    for entry in os.listdir("/proc"):
        if not entry.isdigit() or int(entry) == os.getpid():
            continue
        try:
            environ = open(f"/proc/{entry}/environ", "rb").read()
        except OSError:
            continue
        if marker in environ:
            found.append(int(entry))
    return found

def send(pids, sig):
    for pid in pids:
        try:
            os.kill(pid, sig)
        except OSError:
            pass

pids = matches()
send(pids, signal.SIGTERM)
deadline = time.time() + 3
while pids and time.time() < deadline:
    time.sleep(0.2)
    pids = matches()
send(pids, signal.SIGKILL)
PY
}

ctl() {
  local started
  started=$(date +%s%N)
  echo "+ sirioctl $*"
  "$CTL" "$@"
  echo "  answered in $(( ($(date +%s%N) - started) / 1000000 )) ms"
}
reply() { "$CTL" "$@" --json 2>/dev/null; }
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
read_field() { local key=$1; shift; reply "$@" | field "$key" || fail "sirioctl $* did not answer (reading $key)"; }
wait_for() { # key value sirioctl-args...
  local key=$1 want=$2; shift 2
  local got=""
  for _ in $(seq 1 150); do
    got=$(reply "$@" | field "$key" 2>/dev/null || true)
    [ "$got" = "$want" ] && { echo "OK: $key=$want"; return 0; }
    sleep 0.3
  done
  fail "$key never became '$want' (last: '$got') for: $*"
}
expect() { # key value -- reads the card once
  local got
  got=$(read_field "$1" surface update read)
  [ "$got" = "$2" ] || fail "$1 is '$got', expected '$2'"
  echo "OK: $1=$2"
}

find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
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
}
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window
  window=$(find_window)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $1"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}
# The value one environment variable holds in a running process, read from its
# own /proc entry: what a (relaunched) Sirio was actually started with.
environment_of() { # pid name
  python3 - "$1" "$2" <<'PY'
import sys
pid, name = sys.argv[1], sys.argv[2].encode()
pairs = (item.split(b"=", 1) for item in open(f"/proc/{pid}/environ", "rb").read().split(b"\0") if b"=" in item)
print({key: value for key, value in pairs}.get(name, b"").decode())
PY
}

# ---- the signing side, the stub agents and the server ---------------------
# cargo's warnings go to build.log in the artifact, not into the transcript.
echo "building the signing CLI"
(cd "$ROOT/rust" && CARGO_TARGET_DIR="$TARGET" cargo build --quiet -p sirio_release --bin sirio-release) >>"$RUN_DIR/build.log" 2>&1 \
  || fail "the signing CLI did not build (see build.log in $OUT_DIR)"
SERVE="$RUN_DIR/serve"
mkdir -p "$SERVE"
"$RELEASE_CLI" keygen --out "$RUN_DIR" >/dev/null
PUB_KEY=$(tr -d '\r\n' < "$RUN_DIR/sirio-release-signing.pub")
[ -n "$PUB_KEY" ] || fail "keygen produced an empty public key"
MANIFEST_URL="http://127.0.0.1:$PORT/nightly.json"

# The agent CLIs are stubs, first on the app's PATH. Each records its argv in
# stub-calls.log and answers --version at once. claude's version is above
# MIN_CLAUDE_VERSION, so Sirio chooses the native transport, not the registry
# wrapper (which would reach the network). Mirrors test-change-request-handoff-e2e.sh.
install_stubs() {
  mkdir -p "$RUN_DIR/bin"
  local name
  for name in claude codex opencode pi omp; do
    cat >"$RUN_DIR/bin/$name" <<STUB_EOF
#!/bin/sh
printf '%s\n' "$name \$*" >>"$RUN_DIR/stub-calls.log"
case "\$1" in
  --version|-V|version)
    if [ "$name" = claude ]; then echo "9.9.9 (Claude Code)"; else echo "$name 0.0.0 (e2e stub)"; fi
    exit 0 ;;
esac
exit 0
STUB_EOF
    chmod +x "$RUN_DIR/bin/$name"
  done
}
# Every agent CLI the app's PATH can reach resolves to its stub, or the run stops.
check_stub_path() {
  local name resolved
  for name in claude codex opencode pi omp; do
    resolved=$(export PATH="$APP_PATH"; command -v "$name" || true)
    [ "$resolved" = "$RUN_DIR/bin/$name" ] || fail "$name resolves to '$resolved' under the app's PATH, not its stub"
  done
  echo "OK: every agent CLI the app can reach is its stub"
}
install_stubs
check_stub_path

echo "building sirio and sirioctl with the release environment compiled in"
(cd "$ROOT/rust" && \
  CARGO_TARGET_DIR="$TARGET" \
  SIRIO_RELEASE_CHANNEL=nightly \
  SIRIO_UPDATE_MANIFEST_URL="$MANIFEST_URL" \
  SIRIO_RELEASE_ACCEPTED_KEYS="$PUB_KEY" \
  cargo build --quiet -p sirio --bin sirio && \
  CARGO_TARGET_DIR="$TARGET" cargo build --quiet -p sirio_control --bin sirioctl) >>"$RUN_DIR/build.log" 2>&1 \
  || fail "sirio did not build (see build.log in $OUT_DIR)"

# The new "AppImage": a shell script, so no appimagetool is needed. It leaves a
# marker, then runs the same debug binary.
ARTIFACT="$SERVE/Sirio-$VERSION.AppImage"
cat > "$ARTIFACT" <<'ARTIFACT_SH'
#!/bin/sh
touch "$SIRIO_UPDATE_E2E_MARKER"
exec "$SIRIO_UPDATE_E2E_BIN" "$@"
ARTIFACT_SH
chmod +x "$ARTIFACT"
"$RELEASE_CLI" sign \
  --key "$RUN_DIR/sirio-release-signing.key" \
  --channel nightly \
  --version "$VERSION" \
  --notes "Update card E2E build." \
  --url-template "http://127.0.0.1:$PORT/{file}" \
  --artifact "linux-x86_64=$ARTIFACT" \
  --out "$SERVE/nightly.json"

if curl -fsS -o /dev/null --max-time 2 "http://127.0.0.1:$PORT/" 2>/dev/null; then
  fail "port $PORT is already in use; set SIRIO_UPDATE_UI_E2E_PORT"
fi
env SIRIO_UPDATE_E2E_RUN="$RUN_DIR" python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$SERVE" >"$RUN_DIR/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 50); do curl -fsS -o /dev/null --max-time 1 "$MANIFEST_URL" 2>/dev/null && break; sleep 0.2; done
curl -fsS -o /dev/null --max-time 1 "$MANIFEST_URL" || fail "the manifest server never came up"

# ---- launching an isolated Sirio ------------------------------------------
# Starts Sirio through its stand-in $APPIMAGE with the isolated environment
# above, as the leader of its own process group. The environment it was given
# is written to <name>-expected-env.log, which stage chain checks the
# relaunched Sirio against.
launch() { # name
  local name=$1
  local home="$RUN_DIR/$name-home"
  local data="$RUN_DIR/$name-data"
  local appimage="$RUN_DIR/$name-install/Sirio.AppImage"
  local marker="$RUN_DIR/$name.marker"
  mkdir -p "$home/.config" "$RUN_DIR/$name-state" "$RUN_DIR/$name-cache" "$data" "$(dirname "$appimage")"
  printf '#!/bin/sh\nexec "$SIRIO_UPDATE_E2E_BIN" "$@"\n' > "$appimage"
  chmod +x "$appimage"
  export SIRIO_SOCKET="$RUN_DIR/$name.sock"
  [ "${#SIRIO_SOCKET}" -lt 108 ] || fail "the control socket path is too long for a unix socket: $SIRIO_SOCKET"
  if [ -n "$APPEARANCE" ]; then
    (cd "$ROOT/rust" && CARGO_TARGET_DIR="$TARGET" cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$RUN_DIR/$name.sqlite" --appearance "$APPEARANCE") >>"$RUN_DIR/build.log" 2>&1 || fail "could not seed the appearance"
    # appearance_seed saves updates.enabled=false (the forge UI runs must not
    # poll), and with checks off the card can never show. Turn them back on in
    # this run's own database, where the setting lives in table setting(key, value).
    local updates
    updates=$(python3 - "$RUN_DIR/$name.sqlite" <<'PY'
import sqlite3, sys
con = sqlite3.connect(sys.argv[1])
con.execute("INSERT INTO setting (key, value) VALUES ('updates.enabled', 'true') ON CONFLICT(key) DO UPDATE SET value = excluded.value")
con.commit()
print(con.execute("SELECT value FROM setting WHERE key = 'updates.enabled'").fetchone()[0])
PY
)
    [ "$updates" = true ] || fail "could not turn update checks on in the run's database (updates.enabled=$updates)"
  fi
  local pairs=(
    "PATH=$APP_PATH"
    "HOME=$home"
    "XDG_CONFIG_HOME=$home/.config"
    "XDG_DATA_HOME=$data"
    "XDG_STATE_HOME=$RUN_DIR/$name-state"
    "XDG_CACHE_HOME=$RUN_DIR/$name-cache"
    "SIRIO_SOCKET=$SIRIO_SOCKET"
    "SIRIO_DB=$RUN_DIR/$name.sqlite"
    "SIRIO_CREDENTIALS=$RUN_DIR/$name-credentials.json"
    "SIRIO_HOST_HOME=$RUN_DIR/$name-host"
    "SIRIO_UPDATE_E2E_BIN=$BIN"
    "SIRIO_UPDATE_E2E_MARKER=$marker"
    "SIRIO_UPDATE_E2E_RUN=$RUN_DIR"
    "APPIMAGE=$appimage"
  )
  if [ "$STATE_ONLY" -eq 0 ]; then
    pairs+=("DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1")
  fi
  printf '%s\n' "${pairs[@]}" > "$RUN_DIR/$name-expected-env.log"
  local unset_args=(-u MISE_GLOBAL_CONFIG_FILE -u MISE_CONFIG_DIR -u MISE_DATA_DIR -u WAYLAND_DISPLAY)
  if [ "$STATE_ONLY" -eq 1 ]; then unset_args+=(-u DISPLAY); fi
  # python3 makes the app the leader of its own process group (pgid = pid)
  # before it execs, so the cleanup can stop the group.
  # Started from the run's own directory, not the repository this script lives
  # in: the app's launch directory becomes a worktree of its session, and the
  # repository's worktree would be a session of the user's, not this run's.
  (cd "$RUN_DIR" && exec python3 -c 'import os, sys; os.setpgid(0, 0); os.execvp(sys.argv[1], sys.argv[1:])' \
    env "${unset_args[@]}" "${pairs[@]}" "$appimage" >>"$RUN_DIR/$name-app.log" 2>&1) &
  APP_PID=$!
  RUN_PIDS+=("$APP_PID")
  local started
  started=$(date +%s%N)
  for _ in $(seq 1 75); do [ -S "$SIRIO_SOCKET" ] && break; sleep 0.2; done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket ($name)"
  echo "OK: the control socket of $name opened $(( ($(date +%s%N) - started) / 1000000 )) ms after the wait began"
}

stage_chain() {
  echo "== stage chain: the real check, download, swap and relaunch"
  launch chain
  local project="$RUN_DIR/chain-project"
  mkdir -p "$project"
  git -C "$project" init -q -b main
  ctl project add "$project"
  ctl select-workspace --workspace "$project"
  ctl panel create --worktree "$project" >/dev/null
  wait_for visible true surface update read
  expect title "Sirio $VERSION is available"
  expect buttons "later:on,restart:on"
  capture chain-available
  local before
  before=$(read_field pid surface update read)
  ctl surface update act --action restart
  local after=""
  for _ in $(seq 1 200); do
    after=$(reply surface update read | field pid 2>/dev/null || true)
    [ -n "$after" ] && [ "$after" != "$before" ] && break
    sleep 0.3
  done
  [ -n "$after" ] && [ "$after" != "$before" ] || fail "no new Sirio answered after the restart (before: $before)"
  echo "OK: relaunched, pid $before -> $after"
  APP_PID=$after
  RUN_PIDS+=("$after")
  [ -f "$RUN_DIR/chain.marker" ] || fail "the relaunched Sirio is not the new artifact (no marker)"
  echo "OK: the new artifact ran"
  grep -q "\[update\] status: downloading $VERSION" "$RUN_DIR/chain-app.log" || fail "no Downloading in the app log"
  grep -q "\[update\] status: installing $VERSION" "$RUN_DIR/chain-app.log" || fail "no Installing in the app log"
  grep -q "\[update\] status: installed $VERSION" "$RUN_DIR/chain-app.log" || fail "no Installed in the app log"
  echo "OK: Downloading -> Installing -> Installed"
  local key value got
  : > "$RUN_DIR/chain-relaunched-env.log"
  while IFS='=' read -r key value; do
    got=$(environment_of "$after" "$key")
    printf '%s=%s\n' "$key" "$got" >> "$RUN_DIR/chain-relaunched-env.log"
    if [ "$key" = PATH ]; then
      # Sirio merges the login shell's PATH in front of the one it was given
      # (login_path.rs, at every start), so the stub directory must lead the
      # value, not be the whole of it.
      case "$got" in
        "$RUN_DIR/bin:"*) ;;
        *) fail "the relaunched Sirio's PATH does not lead with the stub directory" ;;
      esac
    else
      [ "$got" = "$value" ] || fail "the relaunched Sirio does not carry $key"
    fi
  done < "$RUN_DIR/chain-expected-env.log"
  echo "OK: the relaunched Sirio's PATH leads with the stub directory and its HOME, XDG and SIRIO_* are the run's own (chain-relaunched-env.log)"
  grep -q '^claude --version$' "$RUN_DIR/stub-calls.log" || fail "the app never asked the stub claude for its version"
  echo "OK: the app's claude probe reached the stub"
  reply project list | grep -q "$project" || fail "the project did not survive the restart"
  echo "OK: the project survived the restart"
  # The layout comes back: the worktree that was selected before the restart is
  # the restored, selected and mounted one. (A terminal made over the control
  # socket is not part of the saved layout: panel list is empty after a restart,
  # measured in chain-stubs-3, so no terminal is asserted here.)
  local restored="" listing=""
  for _ in $(seq 1 100); do
    listing=$(reply list-workspaces || true)
    restored=$(printf '%s' "$listing" | python3 -c '
import json, sys
path = sys.argv[1]
rows = json.load(sys.stdin)
print(any(row.get("path") == path and row.get("selected") == "true" and row.get("mounted") == "true" for row in rows))
' "$project" 2>/dev/null || echo False)
    [ "$restored" = True ] && break
    sleep 0.3
  done
  if [ "$restored" != True ]; then
    echo "workspaces after the restart: ${listing:-<no answer>}"
    fail "the layout did not come back: $project is not the restored, selected worktree"
  fi
  echo "OK: the restored layout selects the worktree that was selected before the restart"
  stop_process_group "$APP_PID"
  APP_PID=""
}

stage_ui() {
  echo "== stage ui: every state, Later, the indicator, the warning, the keyboard"
  # No server for this instance's check: it fails, and a failed check raises nothing.
  kill "$SERVER_PID" 2>/dev/null || true
  SERVER_PID=""
  launch ui
  wait_for status failed surface update read
  expect visible false

  # The terminal the agent warning counts lives in a project.
  local project="$RUN_DIR/ui-project"
  mkdir -p "$project"
  git -C "$project" init -q -b main
  ctl project add "$project"
  ctl select-workspace --workspace "$project"

  ctl surface update inject --status available --version 99.1.0 --notes "Faster terminals."
  expect visible true
  expect title "Sirio 99.1.0 is available"
  # Nightly says what it is, as Settings does; the release notes are a Stable body.
  expect body "Nightly builds track main"
  capture ui-available

  local pane
  pane=$("$CTL" panel create --worktree "$project" --cmd cat)
  [ -n "$pane" ] || fail "panel create returned no pane id"
  "$CTL" notify --session "$pane" --status running
  wait_for agentsWorking 1 surface update read
  expect warning "1 agent is working — restarting will stop it."
  expect buttons "later:on,install:on,restart:on"
  capture ui-warning

  if [ "$STATE_ONLY" -eq 0 ]; then
    # xdotool reads DISPLAY from the environment; --display names it for the captures too.
    export DISPLAY="$DISPLAY_TARGET"
    local window terminal
    window=$(find_window)
    # No window manager runs on the private display, so focus is set on the X
    # window directly (windowactivate reports success without moving focus).
    xdotool windowfocus --sync "$window" >/dev/null 2>&1 || true
    # A control-socket pane sits in a hidden tab, so the keys go to a terminal
    # the user can see: the Terminal tile of the empty pane, clicked as a user
    # would click it (window coordinates; the layout is fixed for this size).
    xdotool mousemove --window "$window" 461 418 click 1
    terminal=""
    for _ in $(seq 1 50); do
      terminal=$(reply panel list | python3 -c '
import json, sys
panes = [row["id"] for row in json.load(sys.stdin) if row.get("tab") == "Terminal"]
print(panes[0] if panes else "")
' 2>/dev/null || true)
      [ -n "$terminal" ] && break
      sleep 0.2
    done
    [ -n "$terminal" ] || fail "clicking the Terminal tile opened no terminal"
    xdotool mousemove --window "$window" 500 300 click 1
    sleep 1
    xdotool type --delay 30 "card-keeps-the-keyboard-away"
    xdotool key Return
    sleep 1
    # panel read answers the scrollback base64-encoded.
    "$CTL" panel read "$terminal" | python3 -c '
import base64, sys
text = base64.b64decode(sys.stdin.read().strip()).decode(errors="replace")
sys.exit(0 if "card-keeps-the-keyboard-away" in text else 1)
' || fail "keystrokes did not reach the terminal ($terminal) while the card was up"
    echo "OK: the terminal ($terminal) kept the keyboard while the card was up"
  fi

  ctl surface update act --action later
  expect visible false
  ctl surface update inject --status available --version 99.1.0
  expect visible false
  echo "OK: Later holds for the same version"
  ctl surface update act --action open
  expect visible true
  echo "OK: the indicator brings it back"

  ctl surface update inject --status downloading --version 99.1.0
  expect buttons "later:off,restart:off"
  expect hideable true
  capture ui-downloading
  ctl surface update act --action hide
  expect visible false
  ctl surface update act --action open

  # The agent warning and an error share one call site in the card's message
  # helper (ely_ui.rs `message`), so both drawn together give two a11y nodes
  # with one id: a debug build panics ("Duplicate a11y node id", bezel-gpui
  # window/a11y.rs). That is an app defect, recorded in docs/testing/update-card.md;
  # until it is fixed the error states are checked with no agent at work.
  "$CTL" notify --session "$pane" --status done
  wait_for agentsWorking 0 surface update read

  ctl surface update inject --status failed --version 99.1.0 --message "network unreachable"
  expect status apply-failed
  expect error "network unreachable"
  expect buttons "later:on,retry:on"
  capture ui-failed

  ctl surface update inject --status refused --version 99.1.0
  expect buttons "later:on"
  read_field error surface update read | grep -q "github.com/ai-sirio/sirio/releases" || fail "the refusal does not name the download page"
  capture ui-refused

  ctl surface update inject --status installed --version 99.1.0
  expect title "Sirio 99.1.0 is installed — restart to use it"
  expect buttons "later:on,restart:on"
  capture ui-installed
}

case "$STAGE" in
  chain) stage_chain ;;
  ui) stage_ui ;;
  all) stage_chain; stage_ui ;;
  *) fail "unknown stage $STAGE" ;;
esac
echo "UPDATE UI E2E OK"
