#!/bin/bash
set -euo pipefail

# End-to-end test of the chat's session panel (spec
# 2026-10-20-chat-session-panel-design.md §9): a real, isolated debug Sirio, its
# Claude chat talking to a stub `claude` (Scripts/Tests/stubs/session-panel/claude,
# which runs rust/crates/sirio_acp/tests/fixtures/claude_fixture.py in its
# session_panel mode) and its change request card reading the loopback fake forge
# (Scripts/Tests/fake_forge.py). Everything is asserted on the `sessionPanel` JSON
# of `surface.chat.read` -- the column's sections, rows, details and actions --
# and driven through the debug-only `surface chat panel` verb, so no pixel is read.
# Framed runs add PID-matched window captures to look at.
#
# The change request card is read only while a frame paints the column, so a run
# with no display (--state-only alone) has nothing that paints and skips the two
# scenarios that read the forge (change_request, rate_limit_shared); --state-only
# with --display :N paints on that display without taking captures and runs all six.
#
# Scenarios (each starts a fresh Sirio with its own database, host and HOME):
#   environment        the project, the chat's directory (the worktree's root) and the branch, tagged worktree
#   context            asked once on open with no prompt yet, then after a prompt: the first
#                      reducible group, a plugin node unfolding to its agents, the project
#                      CLAUDE.md row opening a file tab
#   connectors         a failed server reconnecting to connected, a server that needs
#                      sign-in waiting for it (the opened URL is looked for in a browser stub)
#   change_request     the branch's pull request as a card, its row opening the tab
#   rate_limit_shared  a forge host paused by the right panel's list is paused for the card
#                      too: the row says so and the forge sees no read of it during the pause
#   restore            the column is open, Sirio quits and starts again: it is still open
#
# Isolation: every Sirio starts under `env -i` with an allowlist (see launch()), the
# stub agent CLIs first on PATH, a private HOME, XDG directories, database, socket,
# credentials and host root. The run records the hash of the user's
# ~/.config/mise/config.toml before and after and fails if it changed (an
# unisolated `claude --version` goes through the user's mise wrapper). A display is
# only ever one the user started for this: --display :N, never $DISPLAY.
#
# The artifact: --out-dir DIR (default artifacts/chat-session-panel-e2e-<stamp>-<pid>)
# keeps transcript.log (every call and every read of the column), the app logs, the
# fake forge's request log, stub-calls.log and -- unless --state-only -- PID-matched
# window captures in frames/. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-chat-session-panel-e2e.sh [--state-only] [--out-dir DIR]
#          [--display :N] [--appearance light|dark] [--window-size WxH] [--only NAME[,NAME...]]
#   --window-size WxH (framed runs only; default none) resizes the app's window. Without
#     it the window keeps Sirio's own default size, which leaves the chat about 430 px
#     wide, so the column is laid over the transcript (the snapshot says `overlay`);
#     a window of 2200x1000 leaves the chat wide enough for the column to dock.
#   --only runs some scenarios and prints CHAT SESSION PANEL E2E PARTIAL instead; the
#     scenarios that read the forge are refused without a display.
#   A framed run needs a private display, for example:
#     Xvfb :97 -screen 0 2400x1200x24 &
#   and Mesa's lavapipe (software Vulkan) for the window to draw on it.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
ALL_SCENARIOS=(environment context connectors change_request rate_limit_shared restore)
STATE_ONLY=0
OUT_DIR=""
DISPLAY_TARGET=""
APPEARANCE=""
WINDOW_SIZE=""
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    --window-size) WINDOW_SIZE="$2"; shift 2 ;;
    --only) ONLY="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

refuse() { echo "refused: $*" >&2; exit 2; }
case "$APPEARANCE" in ""|light|dark) ;; *) refuse "--appearance is light or dark, not $APPEARANCE" ;; esac
[ -z "$WINDOW_SIZE" ] || [[ "$WINDOW_SIZE" =~ ^[0-9]+x[0-9]+$ ]] || refuse "--window-size is WxH, not $WINDOW_SIZE"
# The scenarios that read the forge through the change request card, which reads only
# while a frame paints the column.
PAINTED_SCENARIOS=" change_request rate_limit_shared "
SCENARIOS=()
SKIPPED=()
for name in "${ALL_SCENARIOS[@]}"; do
  if [ -z "$DISPLAY_TARGET" ] && [[ "$PAINTED_SCENARIOS" == *" $name "* ]]; then
    SKIPPED+=("$name")
  else
    SCENARIOS+=("$name")
  fi
done
if [ -n "$ONLY" ]; then
  IFS=',' read -r -a SCENARIOS <<<"$ONLY"
  for name in "${SCENARIOS[@]}"; do
    case " ${ALL_SCENARIOS[*]} " in *" $name "*) ;; *) refuse "unknown scenario $name (${ALL_SCENARIOS[*]})" ;; esac
    [ -n "$DISPLAY_TARGET" ] || [[ "$PAINTED_SCENARIOS" != *" $name "* ]] || refuse "scenario $name needs a painted column: pass --display :N"
  done
fi
# The display number of a local X display: ":97", ":0097" and ":97.0" are all 97.
display_number() { # DISPLAY-string
  [[ "$1" =~ ^([A-Za-z0-9._-]*):([0-9]+)(\.[0-9]+)?$ ]] || return 0
  [ -z "${BASH_REMATCH[1]}" ] || [ "${BASH_REMATCH[1]}" = localhost ] || return 0
  echo $((10#${BASH_REMATCH[2]}))
}
if [ "$STATE_ONLY" -eq 0 ]; then
  for tool in import identify xwininfo xprop xdotool timeout; do
    command -v "$tool" >/dev/null || refuse "$tool is required for captures; pass --state-only to skip them"
  done
  [ -n "$DISPLAY_TARGET" ] || refuse "no --display: the captures need a display of their own; pass --display :N (a private Xvfb), or --state-only"
fi
if [ -n "$DISPLAY_TARGET" ]; then
  target_number=$(display_number "$DISPLAY_TARGET")
  [ -n "$target_number" ] || refuse "--display takes a local X display such as :97, not '$DISPLAY_TARGET'"
  ambient_number=$(display_number "${DISPLAY:-}")
  [ "$target_number" != "$ambient_number" ] || refuse "--display $DISPLAY_TARGET is this session's own \$DISPLAY: start a private display (Xvfb :$target_number -screen 0 1600x1000x24) and pass it"
  [ "$DISPLAY_TARGET" != "${WAYLAND_DISPLAY:-}" ] || refuse "--display $DISPLAY_TARGET is this session's \$WAYLAND_DISPLAY"
  ps -u "$(id -u)" -o args= | python3 -c 'import re, sys; n = sys.argv[1]; sys.exit(0 if any(re.search(r"(^|/)Xvfb :" + n + r"( |$)", line) for line in sys.stdin) else 1)' "$target_number" \
    || refuse "no Xvfb :$target_number is running as $(id -un): start a private one (Xvfb :$target_number -screen 0 1600x1000x24 &)"
  DISPLAY_TARGET=":$target_number"
  if [ "$STATE_ONLY" -eq 0 ] || [ -n "$WINDOW_SIZE" ]; then
    command -v xdotool >/dev/null || refuse "xdotool is required to resize the window"
    geometry=$(DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool getdisplaygeometry 2>/dev/null || true)
    [ -n "$geometry" ] || refuse "no X server answers on $DISPLAY_TARGET"
    [ -z "$WINDOW_SIZE" ] || { [ "${geometry% *}" -ge "${WINDOW_SIZE%x*}" ] && [ "${geometry#* }" -ge "${WINDOW_SIZE#*x}" ]; } \
      || refuse "the display is ${geometry% *}x${geometry#* }, smaller than --window-size $WINDOW_SIZE: start a larger Xvfb (Xvfb $DISPLAY_TARGET -screen 0 2400x1200x24 &)"
  fi
fi

[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/chat-session-panel-e2e-$(date +%Y%m%d-%H%M%S)-$$"
if [ -d "$OUT_DIR" ] && [ -n "$(ls -A "$OUT_DIR")" ]; then
  echo "refusing to reuse $OUT_DIR: it already has content (pass a new --out-dir)" >&2
  exit 2
fi
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux"
command -v python3 >/dev/null || fail "python3 is required"
command -v curl >/dev/null || fail "curl is required"
echo "display: ${DISPLAY_TARGET:-none (--state-only)}; scenarios: ${SCENARIOS[*]}; appearance: ${APPEARANCE:-default}"
for name in "${SKIPPED[@]:-}"; do
  [ -z "$name" ] || [ -n "$ONLY" ] || echo "SKIP: scenario $name reads a column that only a frame paints, and there is no --display"
done

# ---- the user's mise config must not move ---------------------------------
MISE_CONFIG="$HOME/.config/mise/config.toml"
mise_hash() { sha256sum "$MISE_CONFIG" 2>/dev/null | cut -d' ' -f1 || true; }
MISE_BEFORE=$(mise_hash)
check_mise() { # when
  [ "$(mise_hash)" = "$MISE_BEFORE" ] || fail "$MISE_CONFIG changed $1 (was ${MISE_BEFORE:-absent}, is $(mise_hash))"
}
echo "mise config: ${MISE_BEFORE:-absent}"

# ---- the run's own directory and what must be stopped ---------------------
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-csp-XXXXXX")
APP_PID=""
FORGE_PID=""
stop_hosts() {
  local state pid tries
  for state in "$RUN_DIR"/*-host/host-v*.json; do
    [ -f "$state" ] || continue
    pid=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("pid", ""))' "$state" 2>/dev/null) || continue
    [ -n "$pid" ] || continue
    case "$(cat "/proc/$pid/comm" 2>/dev/null || true)" in
      *sirio-host*) ;;
      *) continue ;;
    esac
    kill "$pid" 2>/dev/null || true
    tries=0
    while kill -0 "$pid" 2>/dev/null && [ "$tries" -lt 50 ]; do tries=$((tries + 1)); sleep 0.1; done
    kill -KILL "$pid" 2>/dev/null || true
  done
}
# Every process this run started carries SIRIO_CSP_E2E_RUN=<RUN_DIR>: the app, the
# stub claude, the host. Those still alive at the end are stopped, TERM then KILL.
stop_run_processes() {
  python3 - "$RUN_DIR" <<'PY'
import os, signal, sys, time
marker = b"SIRIO_CSP_E2E_RUN=" + sys.argv[1].encode() + b"\0"

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
cleanup() {
  [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; }
  [ -z "$FORGE_PID" ] || kill "$FORGE_PID" 2>/dev/null || true
  stop_hosts
  stop_run_processes
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf --one-file-system "$RUN_DIR"
}
trap cleanup EXIT

# ---- build -----------------------------------------------------------------
echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl) 2>>"$RUN_DIR/build.log" \
  || { tail -n 40 "$RUN_DIR/build.log" >&2; fail "the build failed"; }

# ---- the fake forge --------------------------------------------------------
FORGE_PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
python3 "$ROOT/Scripts/Tests/fake_forge.py" --flavor github --port "$FORGE_PORT" --log "$RUN_DIR/github-requests.log" &
FORGE_PID=$!
for _ in $(seq 1 50); do
  curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$FORGE_PORT/api/v3/meta" && break
  sleep 0.2
done
forge_post() { curl -s -o /dev/null -X POST "http://127.0.0.1:$FORGE_PORT$1"; }
# How many reads of the branch's change request the forge has seen.
card_reads() { grep -cE " (ChangeRequestForBranch|ChangeRequestByNumber) " "$RUN_DIR/github-requests.log" || true; }

# ---- the fixture repository ------------------------------------------------
# A primary checkout on main and a linked worktree on feat/login, whose origin is
# the host the fake forge answers for. The chat runs in the worktree's src/.
PRIMARY="$RUN_DIR/widgets"
LINKED="$RUN_DIR/login"
mkdir -p "$PRIMARY"
git -C "$PRIMARY" init -q -b main
git -C "$PRIMARY" config user.email t@example.com
git -C "$PRIMARY" config user.name Tester
git -C "$PRIMARY" commit -q --allow-empty -m first
git -C "$PRIMARY" remote add origin https://ghe.test/acme/widgets.git
git -C "$PRIMARY" worktree add -q -b feat/login "$LINKED"
mkdir -p "$LINKED/src"
printf '# Widgets\n\nThe project memory the stub claude reports.\n' > "$LINKED/CLAUDE.md"
printf 'fn main() {}\n' > "$LINKED/src/main.rs"
CHAT_CWD="$LINKED/src"

# ---- stubs: the claude shim first, the other agents, and a browser ---------
STUBS="$ROOT/Scripts/Tests/stubs/session-panel"
mkdir -p "$RUN_DIR/bin"
for name in codex opencode pi omp; do
  cat >"$RUN_DIR/bin/$name" <<STUB_EOF
#!/bin/sh
printf '%s\n' "$name \$*" >>"$RUN_DIR/stub-calls.log"
case "\$1" in --version|-V|version) echo "$name 0.0.0 (e2e stub)" ;; esac
exit 0
STUB_EOF
  chmod +x "$RUN_DIR/bin/$name"
done
# What opens a URL. gpui may hand it to the desktop portal instead, which a private
# run has no bus for; the scenario that needs it says whether it was seen.
for name in xdg-open browser-stub; do
  cat >"$RUN_DIR/bin/$name" <<STUB_EOF
#!/bin/sh
printf '%s\n' "$name \$*" >>"$RUN_DIR/browser.log"
exit 0
STUB_EOF
  chmod +x "$RUN_DIR/bin/$name"
done
APP_PATH="$STUBS:$RUN_DIR/bin:/usr/local/bin:/usr/bin:/bin"
resolved=$(PATH="$APP_PATH" command -v claude || true)
[ "$resolved" = "$STUBS/claude" ] || fail "claude resolves to '$resolved' under the app's PATH, not the shim"
echo "OK: claude resolves to the shim"

VULKAN_ENV=()
if [ -n "$DISPLAY_TARGET" ]; then
  LVP=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
  [ -n "$LVP" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required for a framed run on Xvfb; install vulkan-swrast or drop --display"
  VULKAN_ENV=("VK_DRIVER_FILES=$LVP" "VK_ICD_FILENAMES=$LVP")
  echo "vulkan: lavapipe"
fi

# ---- calling the app --------------------------------------------------------
CALL_TIMEOUT=30
ctl() { echo "+ sirioctl $*"; timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@"; }
reply() { timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" --json 2>/dev/null; }

# One helper for what sirioctl has no verb for (surface.chat.open|send|read, which
# every build serves) and for reading the column: it speaks the control socket's
# line-delimited JSON itself.
cat >"$RUN_DIR/e2e.py" <<'PY'
import json, os, socket, sys, time


def rpc(method, params):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(os.environ["SIRIO_SOCKET"])
    s.sendall((json.dumps({"id": "e2e", "method": method, "params": params}) + "\n").encode())
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = s.recv(1 << 20)
        if not chunk:
            break
        buf += chunk
    s.close()
    response = json.loads(buf)
    if not response.get("ok"):
        raise RuntimeError(response.get("error") or method)
    return response.get("result") or {}


def panel(surface):
    return json.loads(rpc("surface.chat.read", {"surfaceId": surface})["sessionPanel"])


class Helpers:
    def __init__(self, p):
        self.p = p

    def S(self, key):
        return next((s for s in self.p["sections"] if s["key"] == key), None)

    def R(self, section, key):
        s = self.S(section)
        return next((r for r in (s["rows"] if s else []) if r["key"] == key), None)

    def rows(self, section):
        s = self.S(section)
        return s["rows"] if s else []


def evaluate(expr, p):
    # The expressions are this script's own, written next to their scenario; the
    # only data they see is the column's JSON.
    h = Helpers(p)
    return eval(expr, {"p": p, "S": h.S, "R": h.R, "rows": h.rows, "json": json})


command = sys.argv[1]
if command == "rpc":
    params = dict(arg.split("=", 1) for arg in sys.argv[3:])
    print(json.dumps(rpc(sys.argv[2], params)))
elif command == "chat-surface":
    print(rpc("surface.chat.open", {})["surfaceId"])
elif command in ("wait", "find"):
    # wait SURFACE DESC SECONDS EXPR: poll until EXPR over the column's JSON is truthy.
    # find SURFACE DESC SECONDS EXPR: the same, printing what EXPR returned.
    surface, desc, seconds, expr = sys.argv[2], sys.argv[3], float(sys.argv[4]), sys.argv[5]
    deadline = time.time() + seconds
    last, error = None, None
    while True:
        try:
            last = panel(surface)
            got = evaluate(expr, last)
            if got:
                print("  read: " + json.dumps(last, separators=(",", ":")), file=sys.stderr)
                print(got if command == "find" else f"OK: {desc}")
                sys.exit(0)
        except Exception as e:  # a read that fails is tried again until the time is up
            error = e
        if time.time() > deadline:
            print("  read: " + json.dumps(last, separators=(",", ":")), file=sys.stderr)
            try:
                chat = rpc("surface.chat.read", {"surfaceId": surface})
                chat.pop("sessionPanel", None)
                print("  chat: " + json.dumps(chat, separators=(",", ":")), file=sys.stderr)
            except Exception as e:
                print(f"  chat: unreadable ({e})", file=sys.stderr)
            print(f"FAIL: {desc}: not true within {seconds:.0f} s: {expr}" + (f" ({error})" if error else ""), file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
elif command == "check":
    # check SURFACE DESC EXPR: one read, no waiting.
    surface, desc, expr = sys.argv[2], sys.argv[3], sys.argv[4]
    p = panel(surface)
    print("  read: " + json.dumps(p, separators=(",", ":")), file=sys.stderr)
    if not evaluate(expr, p):
        print(f"FAIL: {desc}: {expr}", file=sys.stderr)
        sys.exit(1)
    print(f"OK: {desc}")
PY
E2E() { python3 "$RUN_DIR/e2e.py" "$@"; }
SURFACE=""
# wait DESC EXPR [SECONDS]: the column's JSON as `p`, with S(section), R(section, row), rows(section).
wait_panel() { E2E wait "$SURFACE" "$1" "${3:-30}" "$2" || fail "$1"; }
find_panel() { E2E find "$SURFACE" "$1" "${3:-30}" "$2" || fail "$1"; }
check_panel() { E2E check "$SURFACE" "$1" "$2" || fail "$1"; }
panel_ctl() { ctl surface chat panel "$SURFACE" "$@"; }
# The column opens; on a display it is also drawn: over the transcript of a chat narrower
# than 640 px (Sirio's default window), beside it in a window resized wide (--window-size).
open_column() {
  panel_ctl open >/dev/null
  wait_panel "the column is open" 'p["open"]'
  if [ -n "$DISPLAY_TARGET" ]; then
    if [ -n "$WINDOW_SIZE" ]; then
      wait_panel "the column is drawn beside the transcript" 'p["drawn"] and not p["overlay"]'
    else
      wait_panel "the column is laid over the narrow chat's transcript" 'p["drawn"] and p["overlay"]'
    fi
  fi
}
# Folds the cards above the one a capture is of, so a narrow chat's column shows it whole.
fold_cards() { # key...
  local key
  for key in "$@"; do
    panel_ctl toggle "$key" >/dev/null
    wait_panel "the $key card is folded" "S('$key')['expanded'] is False"
  done
}
# Polls a sirioctl read until one of its fields is `want`; the reads are logged.
wait_field() { # desc key want seconds sirioctl-args...
  local desc=$1 key=$2 want=$3 seconds=$4 json="" got="" _
  shift 4
  for _ in $(seq 1 $((seconds * 3))); do
    json=$(reply "$@" || true)
    got=$(printf '%s' "$json" | python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$key" 2>/dev/null || true)
    if [ "$got" = "$want" ]; then echo "  read: $json"; echo "OK: $desc"; return 0; fi
    sleep 0.3
  done
  echo "  read: $json" >&2
  fail "$desc: $key is '$got', not '$want'"
}

# ---- window captures -----------------------------------------------------------
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
  local window="" _
  for _ in $(seq 1 50); do
    window=$(find_window)
    [ -n "$window" ] && break
    sleep 0.2
  done
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $1"
  timeout -k 5 20 import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png" || fail "capture of $1 failed"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours; pid $APP_PID)"
}

# ---- launching an isolated Sirio ---------------------------------------------------
# `env -i` and this allowlist, so nothing of the user's reaches the app (no
# CLAUDE_CONFIG_DIR, MISE_*, GH_TOKEN, GIT_*, a D-Bus session bus, the user's
# XDG_RUNTIME_DIR). Each entry is needed:
#   PATH          the claude shim and the other agent stubs first, then the system directories
#   HOME, XDG_*   private: the login shell's files, Sirio's config, data, state and cache
#   XDG_RUNTIME_DIR private, mode 700: no user bus under it, so the host starts with setsid
#   SHELL, TERM, LANG  the login shell Sirio reads PATH from; text
#   BROWSER       the stub that logs the sign-in URL, if gpui hands it to a browser at all
#   DISPLAY, GPUI_X11_SCALE_FACTOR, VK_*  the window and software Vulkan (framed runs only)
#   SIRIO_SOCKET, SIRIO_DB, SIRIO_CREDENTIALS, SIRIO_HOST_HOME  the run's own
#   SIRIO_FORGE_TEST_ENDPOINTS  ghe.test -> the loopback fake forge (debug builds only)
#   SIRIO_FIXTURE_REPO  where the stub claude says the project memory lives
#   SIRIO_STUB_LOG  the shim's call log; SIRIO_CSP_E2E_RUN marks this run's processes
# How many `claude --version` probes the shim has recorded.
probe_count() { local n; n=$(grep -c '^claude --version$' "$RUN_DIR/stub-calls.log" 2>/dev/null || true); echo "${n:-0}"; }
launch() { # name (a relaunch under the same name keeps its database and host)
  local name=$1 before
  local home="$RUN_DIR/$name-home" data="$RUN_DIR/$name-data" state="$RUN_DIR/$name-state"
  local cache="$RUN_DIR/$name-cache" runtime="$RUN_DIR/$name-runtime"
  mkdir -p "$home/.config" "$state" "$cache" "$data"
  [ -d "$runtime" ] || mkdir -m 700 "$runtime"
  if [ -n "$APPEARANCE" ] && [ ! -e "$RUN_DIR/$name.sqlite" ]; then
    (cd "$ROOT/rust" && cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$RUN_DIR/$name.sqlite" --appearance "$APPEARANCE") || fail "could not seed the appearance"
  fi
  local pairs=(
    "PATH=$APP_PATH" "HOME=$home" "XDG_CONFIG_HOME=$home/.config" "XDG_DATA_HOME=$data"
    "XDG_STATE_HOME=$state" "XDG_CACHE_HOME=$cache" "XDG_RUNTIME_DIR=$runtime"
    "SHELL=/bin/bash" "TERM=${TERM:-xterm-256color}" "LANG=${LANG:-C.UTF-8}"
    "BROWSER=$RUN_DIR/bin/browser-stub"
    "SIRIO_SOCKET=$RUN_DIR/$name.sock" "SIRIO_DB=$RUN_DIR/$name.sqlite"
    "SIRIO_CREDENTIALS=$RUN_DIR/$name-credentials.json" "SIRIO_HOST_HOME=$RUN_DIR/$name-host"
    "SIRIO_FORGE_TEST_ENDPOINTS=ghe.test=http://127.0.0.1:$FORGE_PORT"
    "GH_CONFIG_DIR=$home/gh" "GLAB_CONFIG_DIR=$home/glab"
    "SIRIO_FIXTURE_REPO=$LINKED" "SIRIO_STUB_LOG=$RUN_DIR/stub-calls.log"
    "SIRIO_CSP_E2E_RUN=$RUN_DIR"
  )
  [ -z "${LC_ALL:-}" ] || pairs+=("LC_ALL=$LC_ALL")
  if [ -n "$DISPLAY_TARGET" ]; then
    pairs+=("DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1" "${VULKAN_ENV[@]}")
  fi
  export SIRIO_SOCKET="$RUN_DIR/$name.sock"
  [ "${#SIRIO_SOCKET}" -lt 108 ] || fail "the control socket path is too long for a unix socket: $SIRIO_SOCKET"
  printf '%s\n' "${pairs[@]}" > "$RUN_DIR/$name-expected-env.log"
  before=$(probe_count)
  # Started from the chat's own directory: it is the launch directory that becomes
  # the chat's cwd.
  (cd "$CHAT_CWD" && exec python3 -c 'import os, sys; os.setpgid(0, 0); os.execvp(sys.argv[1], sys.argv[1:])' \
    env -i "${pairs[@]}" "$BIN" >>"$RUN_DIR/$name-app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 75); do [ -S "$SIRIO_SOCKET" ] && break; sleep 0.2; done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket ($name)"
  for _ in $(seq 1 150); do
    [ "$(probe_count)" -gt "$before" ] && break
    sleep 0.2
  done
  [ "$(probe_count)" -gt "$before" ] \
    || fail "$name: the app's claude --version probe never reached the shim"
  echo "OK: $name: pid $APP_PID answers; its claude --version probe reached the shim"
  if [ -n "$DISPLAY_TARGET" ] && [ -n "$WINDOW_SIZE" ]; then
    local window="" _
    for _ in $(seq 1 100); do
      window=$(find_window)
      [ -n "$window" ] && break
      sleep 0.2
    done
    [ -n "$window" ] || fail "$name: no window with _NET_WM_PID=$APP_PID"
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowmove "$window" 0 0 windowsize "$window" "${WINDOW_SIZE%x*}" "${WINDOW_SIZE#*x}" \
      || fail "$name: could not resize the window to $WINDOW_SIZE"
    sleep 1
    echo "OK: $name: the window is $WINDOW_SIZE"
  fi
}
stop_app() {
  ctl quit >/dev/null 2>&1 || true
  for _ in $(seq 1 100); do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.2
  done
  kill -0 "$APP_PID" 2>/dev/null && fail "sirio did not quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
  stop_hosts
  check_mise "after the app stopped"
}

# The positions, highest first, of the tabs of the given kinds that `surface tabs read` lists.
tab_positions() { # kind...
  reply surface tabs read | python3 -c '
import json, sys
kinds = sys.argv[1:]
tabs = json.load(sys.stdin)[0]
print(*sorted((int(k.split(".")[1]) for k, v in tabs.items() if k.startswith("tab.") and v.split("|")[0] in kinds), reverse=True))' "$@"
}

# A fresh Sirio on the fixture, with a token for the fake forge and a Claude chat open.
chat_app() { # name
  launch "$1"
  forge_post /__reset
  ctl project add "$PRIMARY" >/dev/null
  ctl surface change-requests show >/dev/null
  local account
  account=$(reply surface change-requests token --host ghe.test --forge github --token $'good\n' | python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get("account", ""))')
  [ "$account" = "fake-user" ] || fail "the token signed in as '$account'"
  # The Open list offers an agent once its CLI has been looked for.
  for _ in $(seq 1 50); do
    "$CTL" surface open-list open --entry agent:claude >/dev/null 2>&1 && break
    sleep 0.3
  done
  # `surface.chat.open` answers the first chat tab, and a fresh Sirio starts with a
  # default one that belongs to no agent, beside a terminal that would share the
  # width with the chat (the column goes over the transcript under 640 px): close both, so the
  # chat opened here is the first and the only tab.
  local old_tabs
  old_tabs=$(tab_positions chat terminal)
  ctl surface open-list open --entry agent:claude >/dev/null || fail "the Open list never offered a Claude chat"
  local position
  for position in $old_tabs; do ctl surface tabs close "$position" --confirm >/dev/null; done
  SURFACE=""
  for _ in $(seq 1 50); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "no chat tab opened"
  echo "OK: chat surface $SURFACE"
}

# ---- scenarios ---------------------------------------------------------------------
scenario_environment() {
  chat_app environment
  check_panel "the column starts closed" 'p["open"] is False'
  open_column
  wait_panel "the project is named" "R('environment', 'environment.project') and R('environment', 'environment.project')['label'] == 'widgets'"
  # The app starts a chat in the worktree's root even when it is launched from
  # src/ (`initial_working_directory` walks up to the repository), so the card's
  # "the chat's own directory, not the root" is a unit-tested distinction here and
  # the directory this run can reach is the root.
  wait_panel "the directory is the chat's own, the linked worktree" \
    "(R('environment', 'environment.directory') or {}).get('label', '').endswith('/login')"
  wait_panel "the branch is feat/login, tagged worktree" \
    "(R('environment', 'environment.branch') or {}).get('label') == 'feat/login' and R('environment', 'environment.branch')['detail'] == 'worktree'"
  capture environment
  stop_app
}

scenario_context() {
  chat_app context
  open_column
  # Opened before any prompt: the report is asked for once, on open.
  wait_panel "a context section appears with no prompt sent" "S('context') is not None and len(rows('context')) > 0"
  wait_panel "the first reducible group is Memory files, 34.2k" \
    "rows('context')[0]['label'] == 'You can reduce it' and rows('context')[1]['label'] == 'Memory files' and rows('context')[1]['detail'] == '34.2k'"
  echo "step: one prompt"
  E2E rpc surface.chat.send surfaceId="$SURFACE" text="hello" >/dev/null
  for _ in $(seq 1 100); do
    [ "$(E2E rpc surface.chat.read surfaceId="$SURFACE" | python3 -c 'import json, sys; print(json.load(sys.stdin)["status"])')" = completed ] && break
    sleep 0.3
  done
  [ "$(E2E rpc surface.chat.read surfaceId="$SURFACE" | python3 -c 'import json, sys; print(json.load(sys.stdin)["status"])')" = completed ] || fail "the prompt's turn never completed"
  echo "OK: the turn completed"
  wait_panel "the context total is the stub's" "'16%' in S('context')['summary'] and '64.9k' in S('context')['summary']"
  wait_panel "Memory files is still the first reducible group" \
    "rows('context')[1]['label'] == 'Memory files' and rows('context')[1]['detail'] == '34.2k'"
  check_panel "free space closes the card, after Claude Code" \
    "rows('context')[-1]['key'] == 'context/free_space' and any(r['key'] == 'context/claude_code' for r in rows('context')[:-1])"
  check_panel "the memory group's remainder sits after its files" \
    "[r['label'] for r in rows('context') if r['key'].startswith('context/memory/')][-1] == 'Other'"
  echo "step: a plugin node unfolds to its agents"
  local plugin="context/plugins/plugin-dev"
  check_panel "the plugin node is folded" "R('context', '$plugin') is not None and R('context', '$plugin')['expanded'] is False"
  panel_ctl toggle "$plugin" >/dev/null
  wait_panel "the plugin's agent appears" "R('context', '$plugin/agent:agent-creator') is not None"
  capture context
  echo "step: the groups fold, so the Fixed group shows in the capture"
  fold_cards environment
  local node
  for node in context/memory context/plugins context/conversation; do
    panel_ctl toggle "$node" >/dev/null
    wait_panel "$node is folded" "R('context', '$node')['expanded'] is False"
  done
  capture context-fixed
  panel_ctl toggle context/memory >/dev/null
  wait_panel "Memory files is open again" "R('context', 'context/memory')['expanded'] is True"
  echo "step: the project CLAUDE.md opens as a file tab"
  local memory
  memory=$(find_panel "the project memory row" "next(r['key'] for r in rows('context') if r['key'].endswith('/CLAUDE.md') and r['tag'] == 'Project')")
  panel_ctl activate "$memory" >/dev/null
  wait_field "the active tab is a file tab on the project CLAUDE.md" path "$LINKED/CLAUDE.md" 20 surface file read
  wait_field "that file tab has loaded the file" state loaded 20 surface file read
  stop_app
}

scenario_connectors() {
  chat_app connectors
  open_column
  local codegraph="connectors/user/codegraph" notion="connectors/claude.ai/claude.ai Notion"
  wait_panel "codegraph failed, offering Reconnect" \
    "R('connectors', '$codegraph') and R('connectors', '$codegraph')['detail'] == 'failed' and R('connectors', '$codegraph')['action'] == 'Reconnect'"
  wait_panel "Notion needs sign-in, offering Sign in" \
    "R('connectors', '$notion') and R('connectors', '$notion')['detail'] == 'needs sign-in' and R('connectors', '$notion')['action'] == 'Sign in'"
  fold_cards environment context
  capture connectors-before
  panel_ctl activate "$codegraph" >/dev/null
  wait_panel "codegraph reconnected" "R('connectors', '$codegraph')['detail'] == 'connected'"
  panel_ctl activate "$notion" >/dev/null
  wait_panel "Notion waits for the sign-in" "R('connectors', '$notion')['detail'] == 'waiting for sign-in…'"
  capture connectors-after
  sleep 1
  if grep -q '127.0.0.1:9/authorize' "$RUN_DIR/browser.log" 2>/dev/null; then
    echo "OK: the sign-in URL reached the browser stub: $(grep '127.0.0.1:9/authorize' "$RUN_DIR/browser.log" | head -n 1)"
  else
    echo "NOTE: the sign-in URL was not observed through the BROWSER/xdg-open stubs (gpui opens URLs through the desktop portal when it can); the row's waiting state is what is asserted"
  fi
  stop_app
}

scenario_change_request() {
  chat_app change_request
  open_column
  wait_panel "the card is #112" "R('change_request', 'change_request/card') and R('change_request', 'change_request/card')['label'].startswith('#112')"
  fold_cards environment context
  capture change_request
  panel_ctl activate change_request/card >/dev/null
  wait_field "a change request tab for #112 is open" label '#112' 20 surface change-request read
  wait_field "and it has loaded" state loaded 20 surface change-request read
  local tabs
  tabs=$(reply surface tabs read)
  echo "  read: $tabs"
  case "$tabs" in *'change_request|'*) echo "OK: the tab list names a change_request tab" ;; *) fail "the tab list has no change_request tab" ;; esac
  capture change_request-tab
  stop_app
}

scenario_rate_limit_shared() {
  chat_app rate_limit_shared
  echo "step: the right panel's list is rate limited, and pauses the host"
  forge_post "/__ratelimit?seconds=40"
  ctl surface change-requests show >/dev/null
  ctl surface change-requests filter mine >/dev/null
  for _ in $(seq 1 100); do
    [ "$(reply surface change-requests read | python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get("state", ""))')" = error ] && break
    sleep 0.3
  done
  local before
  before=$(card_reads)
  echo "step: the column opens during the pause (card reads so far: $before)"
  panel_ctl open >/dev/null
  wait_panel "the column is open" 'p["open"]'
  wait_panel "the card row says it is rate limited" \
    "'rate limited' in (R('change_request', 'change_request/error') or {}).get('label', '').lower()" 10
  capture rate-limited
  sleep 2
  [ "$(card_reads)" = "$before" ] || fail "the forge saw a read of the branch's change request during the pause ($(card_reads) > $before)"
  echo "OK: the forge saw no ChangeRequestForBranch or ChangeRequestByNumber during the pause"
  forge_post /__reset
  stop_app
}

scenario_restore() {
  chat_app restore
  open_column
  # A saved layout is written shortly after a change; a quit flushes it as well.
  sleep 1
  local first_surface=$SURFACE
  stop_app
  echo "step: Sirio starts again on the same database"
  launch restore
  SURFACE=""
  for _ in $(seq 1 100); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "the chat was not restored"
  echo "OK: the restored chat is $SURFACE (was $first_surface)"
  wait_panel "the restored chat's column is open" 'p["open"]'
  if [ -n "$DISPLAY_TARGET" ]; then
    wait_panel "and drawn" 'p["drawn"]'
  fi
  wait_panel "and its Environment is drawn again" "R('environment', 'environment.project') is not None"
  capture restore
  stop_app
}

ran=0
for name in "${SCENARIOS[@]}"; do
  echo "== scenario $name"
  "scenario_$name"
  check_mise "after scenario $name"
  ran=$((ran + 1))
done
check_mise "at the end"
echo "mise config unchanged: ${MISE_BEFORE:-absent}"
cp "$RUN_DIR/github-requests.log" "$OUT_DIR/" 2>/dev/null || true
echo "artifact: $OUT_DIR"
if [ -n "$ONLY" ]; then
  echo "CHAT SESSION PANEL E2E PARTIAL ($ran of ${#ALL_SCENARIOS[@]} scenarios: ${SCENARIOS[*]})"
else
  skipped=$(( ${#ALL_SCENARIOS[@]} - ran ))
  echo "CHAT SESSION PANEL E2E OK ($ran scenarios, $skipped skipped without a display, appearance ${APPEARANCE:-default}, state-only $STATE_ONLY, window ${WINDOW_SIZE:-default})"
fi
