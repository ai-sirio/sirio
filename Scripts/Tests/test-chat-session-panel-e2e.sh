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
# Every card but Environment starts folded, and the change request card is read only
# while it is unfolded and a frame paints the column, so a run with no display
# (--state-only alone) has nothing that paints and skips the scenarios that need a
# frame (default_folds, change_request, rate_limit_shared, trace_tab, trace_idle);
# --state-only with --display :N paints on that display without taking captures and
# runs all of them.
# A scenario that works inside a card unfolds it first (unfold_cards).
#
# Scenarios (each starts a fresh Sirio with its own database, host and HOME):
#   environment        the project, the chat's directory (the worktree's root) and the branch, tagged worktree
#   default_folds      Environment open and the other cards folded, Context and Connectors with their
#                      summaries, the folded change request card drawn while the forge sees no request
#                      at all, and the card read once it is unfolded; then a chat on a host nothing names
#                      (no setting, no token), where opening the column runs neither `gh` nor `glab`
#                      (stubs that record every call)
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
# The trace scenarios (spec 2026-10-21-chat-trace-waterfall-design.md §8) use another stub,
# Scripts/Tests/stubs/trace/claude, the same fixture in its trace mode: the first word of
# a prompt picks a scripted turn (subagents, second, cancel, hang, die, subperm,
# subperm_early) and TRACE_STEP_MS (400 by default) paces its lines, so a live state can be
# read mid-turn. They are asserted on the card's rows in `sessionPanel`, on
# `surface.trace.read` (the Trace tab) and on `firstVisibleEntry` and `lastVisibleEntry` of
# `surface.chat.read`.
#   trace_live         a turn with two parallel agents (one nesting a third), an Edit that asks and
#                      a Bash that fails and then works: the lanes, the waiting tag and its row, the
#                      reveal of the permission (the list's top at it unpainted, the card between the
#                      first and last row drawn painted), the failed and done calls, Done with $0.31
#   trace_cost         a second turn's cost is the difference of the process's totals: $0.12
#   trace_interrupt    Stop ends the turn Interrupted, its cost and the next turn's are unknown
#   trace_death        the CLI dying mid-call ends the turn Failed, every lane failed or interrupted
#   trace_hang         a call that never reports ends interrupted with the turn
#   trace_subagent_permission  a subagent's request is a waiting lane and one Permission card, never a
#                      tool row left pending, whether it comes after its call's line or before
#   trace_reveal       a lane reveals its Agent row, the root the user's message
#   trace_tab          the Trace tab: its tree, one per chat, kept across a worktree switch, never
#                      restored, closed with its chat
#   trace_idle         SIRIO_PERF_TRACE counts the frames the trace asks for (trace.frame): some while
#                      a turn runs and the card or the tab is drawn, none with it folded, with the tab
#                      or a third tab selected in its place, with the worktree parked or after the turn
#   trace_keys         real X input on the Trace tab: a click on a row and Enter on the selected row
#                      reveal its Agent row, a click on the Follow checkbox toggles it
# The file_links scenario (spec 2026-10-24-chat-file-links-design.md §6) uses a third stub,
# Scripts/Tests/stubs/file-links/claude, the fixture in its file_links mode: every prompt is
# answered with one text holding paths that exist in the worktree, ones that do not, a
# version, a fenced path and a web link. It is asserted on `transcriptLinks` of
# `surface.chat.read` (the links of the last paint, with the pixel of each), on
# `surface.editor.state`, on `surface tabs read` and on the browser stub's log.
#   file_links         real X clicks and drags on a chat response: its file paths and the
#                      web link are the links painted, a click opens the file (a Markdown
#                      one in Preview, or in the source at its line when the path names
#                      one), a second click reuses the tab, a Super+Shift click still goes
#                      to the editor, a drag selects and opens nothing, and the web link
#                      still reaches the browser stub
# trace_tab and trace_idle need a painted column (--display :N): the tab closes when its chat is
# released (observe_release, then TraceTabEvent::Close), which in a window comes after the paint
# that replaces the chat's last frame, and a frame that is never painted asks for nothing
# whatever the code does.
# trace_keys and file_links need a framed run at --window-size 2200x1000 (no --state-only): they
# click at pixels.
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
# fake forge's request log, stub-calls.log, the performance traces of trace_idle
# (trace_idle-*.tsv) and -- unless --state-only -- PID-matched window captures in
# frames/. Rerunning the script reproduces it. The run's own directory is made under
# $TMPDIR (a unix socket path must stay under 108 bytes, so point TMPDIR at a short
# directory if /tmp is not wanted).
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
ALL_SCENARIOS=(environment default_folds context connectors change_request rate_limit_shared restore
  trace_live trace_cost trace_interrupt trace_death trace_hang trace_subagent_permission trace_reveal
  trace_tab trace_idle trace_keys file_links)
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
# while a frame paints the column, and the two that need a frame for what they assert:
# a Trace tab closes when its chat is released, which in a window comes after the paint
# that replaces the chat's last frame, and a trace that asks for no frame proves nothing
# when nothing is ever painted.
PAINTED_SCENARIOS=" default_folds change_request rate_limit_shared trace_tab trace_idle trace_keys file_links "
# trace_keys and file_links send real X clicks, keys and drags at pixels, so they need a
# framed run (no --state-only) at the 2200x1000 window size.
FRAMED_SCENARIOS=" trace_keys file_links "
framed_run() { [ "$STATE_ONLY" -eq 0 ] && [ "$WINDOW_SIZE" = 2200x1000 ]; }
SCENARIOS=()
SKIPPED=()
for name in "${ALL_SCENARIOS[@]}"; do
  if [ -z "$DISPLAY_TARGET" ] && [[ "$PAINTED_SCENARIOS" == *" $name "* ]]; then
    SKIPPED+=("$name")
  elif [[ "$FRAMED_SCENARIOS" == *" $name "* ]] && ! framed_run; then
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
    [[ "$FRAMED_SCENARIOS" != *" $name "* ]] || framed_run || refuse "scenario $name sends real clicks at the 2200x1000 layout: pass --display :N --window-size 2200x1000 and no --state-only"
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
  [ -z "$name" ] || [ -n "$ONLY" ] || echo "SKIP: scenario $name needs a framed run (a painted column; trace_keys and file_links also the 2200x1000 window and real X input)"
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
  cp "$RUN_DIR"/*.tsv "$OUT_DIR/" 2>/dev/null || true
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
# A repository whose remote is on a host nothing names: no setting, no token. default_folds
# opens a chat on it with recording `gh` and `glab` (below).
STRANGER="$RUN_DIR/stranger"
mkdir -p "$STRANGER"
git -C "$STRANGER" init -q -b main
git -C "$STRANGER" config user.email t@example.com
git -C "$STRANGER" config user.name Tester
git -C "$STRANGER" commit -q --allow-empty -m first
git -C "$STRANGER" remote add origin https://git.example.invalid/acme/stranger.git

# ---- stubs: the claude shim first, the other agents, and a browser ---------
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
# `gh` and `glab` that record their arguments and say nobody is signed in. They are on the
# PATH of the one launch that looks for them (default_folds), never of the others.
CLI_STUBS="$RUN_DIR/clistubs"
mkdir -p "$CLI_STUBS"
for name in gh glab; do
  cat >"$CLI_STUBS/$name" <<STUB_EOF
#!/bin/sh
printf '%s\n' "$name \$*" >>"$RUN_DIR/forge-cli.log"
exit 1
STUB_EOF
  chmod +x "$CLI_STUBS/$name"
done
# Which claude shim the next launch puts first on the app's PATH: session-panel (the
# column's own scenarios), trace (the trace scenarios) or file-links (file_links). A scenario picks its stub by
# naming it in SIRIO_E2E_CLAUDE_STUB through use_stub, and the run loop puts the default
# back before the next scenario.
use_stub() { # session-panel|trace|file-links
  SIRIO_E2E_CLAUDE_STUB=$1
  local stubs="$ROOT/Scripts/Tests/stubs/$SIRIO_E2E_CLAUDE_STUB" resolved
  APP_PATH="$stubs:$RUN_DIR/bin:/usr/local/bin:/usr/bin:/bin"
  resolved=$(PATH="$APP_PATH" command -v claude || true)
  [ "$resolved" = "$stubs/claude" ] || fail "claude resolves to '$resolved' under the app's PATH, not the $SIRIO_E2E_CLAUDE_STUB shim"
}
for stub in session-panel trace file-links; do
  use_stub "$stub"
  echo "OK: claude resolves to the $stub shim"
done
use_stub session-panel

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
import json, os, re, socket, sys, time


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


def state(surface):
    """The column's JSON and the rest of the chat's own read: its status, its transcript
    (the rows decoded) and the first and last transcript row its list drew (`first`, `last`)."""
    result = rpc("surface.chat.read", {"surfaceId": surface})
    panel = json.loads(result["sessionPanel"])
    chat = {k: v for k, v in result.items() if k != "sessionPanel"}
    chat["transcript"] = json.loads(chat.get("transcript") or "[]")
    chat["first"] = int(chat.get("firstVisibleEntry") or 0)
    chat["last"] = int(chat.get("lastVisibleEntry") or 0)
    return panel, chat


def panel(surface):
    return state(surface)[0]


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


def evaluate(expr, surface, p, c):
    # The expressions are this script's own, written next to their scenario; the
    # only data they see is the chat's own reads: `p` the column's JSON, `c` the rest of
    # `surface.chat.read`, and `tab()` the Trace tab's `surface.trace.read`.
    h = Helpers(p)

    def lanes_of():
        return {r["key"][len("trace/lane/"):]: r for r in h.rows("trace") if r["key"].startswith("trace/lane/")}

    def wait_rows():
        return [r for r in h.rows("trace") if r["key"].startswith("trace/wait/")]

    def row_index(row_id):
        # The transcript's own index of a tool or Agent row.
        return next((i for i, r in enumerate(c["transcript"]) if r.get("kind") == "tool" and r.get("id") == row_id), None)

    def drawn_index(row_id):
        # The index the transcript draws a row at: consecutive tool rows are one box, drawn at
        # the run's last row, so that is where a reveal of any of them lands.
        at = row_index(row_id)
        while at is not None and at + 1 < len(c["transcript"]) and c["transcript"][at + 1].get("kind") == "tool":
            at += 1
        return at

    def pending():
        return [(i, r["id"]) for i, r in enumerate(c["transcript"]) if r.get("kind") == "permission" and r.get("status") == "pending"]

    def tab():
        result = dict(rpc("surface.trace.read", {"surfaceId": surface}))
        result["spans"] = json.loads(result.get("spans") or "[]")
        return result

    def T(span_id):
        return next((s for s in tab()["spans"] if s["id"] == span_id), None)

    namespace = {
        "p": p, "c": c, "S": h.S, "R": h.R, "rows": h.rows, "json": json, "re": re,
        "lane": lambda key: h.R("trace", "trace/lane/" + key), "lanes_of": lanes_of,
        "wait_rows": wait_rows, "row_index": row_index, "drawn_index": drawn_index, "pending": pending,
        "tab": tab, "T": T,
    }
    return eval(expr, namespace)


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
            last, chat = state(surface)
            got = evaluate(expr, surface, last, chat)
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
    p, chat = state(surface)
    print("  read: " + json.dumps(p, separators=(",", ":")), file=sys.stderr)
    if not evaluate(expr, surface, p, chat):
        print(f"FAIL: {desc}: {expr}", file=sys.stderr)
        sys.exit(1)
    print(f"OK: {desc}")
elif command in ("links-check", "link", "link-width"):
    # The links `surface.chat.read` reports for the chat's last paint: rows with kind (file|web),
    # label, target, the window pixel (x, y) a few pixels into the label at its vertical middle, and w,
    # its painted width.
    # links-check SURFACE SECONDS WEB_LABEL FILE_LABEL...: poll until the file links are exactly
    #   FILE_LABEL... (in any order) and the web links exactly WEB_LABEL.
    # link SURFACE KIND LABEL: poll until a link of that kind and label is reported, print "x y".
    # link-width SURFACE KIND LABEL: the same link, print `w`, how many pixels wide its label painted.
    surface = sys.argv[2]

    def read_links():
        result = rpc("surface.chat.read", {"surfaceId": surface})
        if "transcriptLinks" not in result:
            raise RuntimeError("surface.chat.read has no transcriptLinks, so the chat reports no link of its last paint")
        return json.loads(result["transcriptLinks"] or "[]")

    deadline = time.time() + (float(sys.argv[3]) if command == "links-check" else 15)
    rows, error = [], None
    while True:
        try:
            rows = read_links()
            error = None
            if command == "links-check":
                files = sorted(r["label"] for r in rows if r["kind"] == "file")
                web = sorted(r["label"] for r in rows if r["kind"] == "web")
                if files == sorted(sys.argv[5:]) and web == [sys.argv[4]]:
                    print("  links: " + json.dumps(rows, ensure_ascii=False), file=sys.stderr)
                    print(f"OK: the file links are {sorted(sys.argv[5:])} and the web link is {sys.argv[4]!r}")
                    sys.exit(0)
            else:
                row = next((r for r in rows if r["kind"] == sys.argv[3] and r["label"] == sys.argv[4]), None)
                if row:
                    print(int(float(row["w"])) if command == "link-width" else f"{int(float(row['x']))} {int(float(row['y']))}")
                    sys.exit(0)
        except SystemExit:
            raise
        except Exception as e:  # a read that fails is tried again until the time is up
            error = e
        if time.time() > deadline:
            print("  links: " + json.dumps(rows, ensure_ascii=False), file=sys.stderr)
            if command == "links-check":
                print(f"FAIL: the links are not the file links {sorted(sys.argv[5:])} and the web link {sys.argv[4]!r}"
                      + (f" ({error})" if error else ""), file=sys.stderr)
            else:
                print(f"FAIL: no {sys.argv[3]} link labelled {sys.argv[4]!r}" + (f" ({error})" if error else ""), file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
elif command == "perf-window":
    # perf-window FILE SECONDS: how many trace.frame events the app recorded in the next
    # SECONDS, by wall clock (the file's header names the instant its own clock began),
    # once its writer has flushed past the window, which it does about once a second. The
    # writer drops events rather than block and says so in its health rows: the total it
    # reports is part of the answer.
    path, seconds = sys.argv[2], float(sys.argv[3])

    def read_perf():
        epoch, rows = None, []
        with open(path) as handle:
            for line in handle:
                line = line.rstrip("\n")
                if line.startswith("# sirio-perf-v1"):
                    epoch = int(next(f for f in line.split("\t") if f.startswith("epoch_ns=")).split("=")[1])
                elif line and not line.startswith("#"):
                    start, _, kind, name, entity, _ = line.split("\t")[:6]
                    rows.append((epoch + int(start), kind, name, int(entity)))
        return rows

    t0 = time.time_ns()
    time.sleep(seconds)
    t1 = time.time_ns()
    deadline = time.time() + 10
    while True:
        rows = read_perf()
        if any(name == "trace.dropped" and wall > t1 for wall, _, name, _ in rows):
            break
        if time.time() > deadline:
            print("FAIL: the performance trace was not flushed past the window", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.25)
    frames = {}
    for wall, kind, name, entity in rows:
        if name == "trace.frame" and t0 <= wall <= t1:
            frames[entity] = frames.get(entity, 0) + 1
    dropped = sum(entity for _, _, name, entity in rows if name == "trace.dropped")
    print(json.dumps({"frames": sum(frames.values()), "by_entity": frames, "dropped": dropped,
                      "frames_in_file": sum(1 for _, _, name, _ in rows if name == "trace.frame")}))
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
# Unfolds the cards a scenario works inside: every card but Environment starts folded,
# and a card that needs the agent or the forge is drawn, folded, only once it has found
# what it needs.
unfold_cards() { # key...
  local key
  for key in "$@"; do
    wait_panel "the $key card is drawn" "S('$key') is not None"
    panel_ctl toggle "$key" >/dev/null
    wait_panel "the $key card is unfolded" "S('$key')['expanded'] is True"
  done
}
# Unfolds a card unless it is open already, where unfold_cards would toggle it shut.
ensure_unfolded() { # key
  wait_panel "the $1 card is drawn" "S('$1') is not None"
  if ! E2E check "$SURFACE" "the $1 card is unfolded" "S('$1')['expanded'] is True" >/dev/null 2>&1; then
    unfold_cards "$1"
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
#   TRACE_STEP_MS  the pace of the trace fixture's scripted lines
#   SIRIO_PERF_TRACE  trace_idle's performance trace (a file that does not exist yet)
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
    "SIRIO_CSP_E2E_RUN=$RUN_DIR" "TRACE_STEP_MS=${TRACE_STEP_MS:-400}"
  )
  # A performance trace names a file that does not exist yet, one per launch.
  [ -z "${PERF_TRACE:-}" ] || pairs+=("SIRIO_PERF_TRACE=$PERF_TRACE")
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
  open_chat
}

# Opens a Claude chat as the first and only tab of a fresh Sirio.
open_chat() {
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

# ---- the trace scenarios' helpers -----------------------------------------------------
# A fresh Sirio on the fixture with the trace stub and a Claude chat, its column open.
# TRACE_STEP_MS (the optional second argument, in ms) paces the fixture's scripted lines;
# a scenario that reads no live state runs them fast.
trace_chat() { # name [step-ms]
  use_stub trace
  TRACE_STEP_MS=${2:-400}
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  open_column
}

# Sends a prompt once the chat is neither connecting nor streaming (a prompt sent while the
# agent connects is held back, and one sent while a turn streams is queued).
send_prompt() { # text
  wait_panel "the chat is ready for a prompt" 'c["status"] in ("idle", "completed")' 30
  E2E rpc surface.chat.send surfaceId="$SURFACE" text="$1" >/dev/null || fail "sending '$1' failed"
  echo "step: sent '$1'"
}

# Answers the permission card the turn waits on: allow_once or reject_once.
answer_permission() { # option
  local id
  id=$(find_panel "a permission card is pending" 'next((r["id"] for r in c["transcript"] if r.get("kind") == "permission" and r.get("status") == "pending"), "")' 30)
  E2E rpc surface.chat.permission surfaceId="$SURFACE" requestId="$id" optionId="$1" >/dev/null || fail "answering request $id with $1 failed"
  echo "OK: request $id answered with $1"
}

# The Secondary half's tab kinds, comma separated, as `surface layout read` lists them. A read
# that failed, or one with no `secondaryKinds`, is an error and prints nothing: it is never an
# empty list, which would read as "no trace tab".
secondary_kinds() {
  local out
  out=$(reply surface layout read) || return 1
  python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["secondaryKinds"])' <<<"$out" 2>/dev/null
}
# How many trace tabs the Secondary half has (the chat's own tab is in it too). An error when
# the layout could not be read.
trace_tab_count() {
  local kinds
  kinds=$(secondary_kinds) || return 1
  printf '%s\n' "$kinds" | tr ',' '\n' | grep -c '^trace$' || true
}
# Fails unless the Secondary half has exactly WANT trace tabs, or the layout cannot be read.
expect_trace_tabs() { # desc want
  local got
  got=$(trace_tab_count) || fail "$1: the layout could not be read"
  [ "$got" = "$2" ] || fail "$1: $got trace tab(s), expected $2 ($(secondary_kinds || echo unreadable))"
  echo "OK: $1 ($got trace tab(s))"
}
# Polls the layout until the Secondary half has (present) or has not (absent) a trace tab.
# The tab closes one paint after its chat is released, so the wait is bounded and the
# window is painted. A read that fails is "not yet", never "absent": the wait can end in
# a match only on a layout it read.
wait_trace_tab() { # desc present|absent seconds
  local desc=$1 want=$2 seconds=${3:-20} kinds="" has="" read_ok=0 _
  for _ in $(seq 1 $((seconds * 3))); do
    if kinds=$(secondary_kinds); then
      read_ok=1
      case ",$kinds," in *,trace,*) has=present ;; *) has=absent ;; esac
      if [ "$has" = "$want" ]; then echo "  read: secondaryKinds=$kinds"; echo "OK: $desc"; return 0; fi
    else
      read_ok=0
    fi
    sleep 0.3
  done
  [ "$read_ok" = 1 ] || fail "$desc: the layout could not be read at the end of $seconds s"
  fail "$desc: secondaryKinds is '$kinds', a trace tab is not $want"
}

# How many trace.frame events the app recorded in the next SECONDS, `zero` or `some` of
# them expected. The performance trace is a file the run named for this launch.
frames_in_window() { # desc seconds zero|some
  local desc=$1 seconds=$2 want=$3 out frames dropped
  out=$(E2E perf-window "$PERF_TRACE" "$seconds") || fail "$desc: the performance trace could not be read"
  echo "  perf: $out"
  frames=$(printf '%s' "$out" | python3 -c 'import json, sys; print(json.load(sys.stdin)["frames"])')
  dropped=$(printf '%s' "$out" | python3 -c 'import json, sys; print(json.load(sys.stdin)["dropped"])')
  [ "$dropped" = 0 ] || fail "$desc: the trace dropped $dropped event(s), so no count can be trusted"
  case "$want" in
    zero) [ "$frames" = 0 ] || fail "$desc: $frames trace.frame event(s) in $seconds s, expected none" ;;
    # Software Vulkan paints a frame in well over the loop's 33 ms, so a drawn turn asks for
    # a handful in three seconds, not ninety.
    some) [ "$frames" -ge 3 ] || fail "$desc: $frames trace.frame event(s) in $seconds s, expected the animation's own (3 or more)" ;;
  esac
  # Who asked: the owners' entity ids of the window's frames, space separated, sorted.
  LAST_ENTITIES=$(printf '%s' "$out" | python3 -c 'import json, sys; print(*sorted(json.load(sys.stdin)["by_entity"]))')
  echo "OK: $desc: $frames trace.frame event(s) in $seconds s from entit(ies) [${LAST_ENTITIES:-none}] (dropped $dropped)"
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

scenario_default_folds() {
  chat_app default_folds
  open_column
  wait_panel "the change request card is drawn folded, with no row" "S('change_request') is not None and rows('change_request') == []"
  wait_panel "Context shows its summary" "S('context') is not None and S('context')['summary'] != ''"
  wait_panel "Connectors shows its summary" "S('connectors') is not None and S('connectors')['summary'] != ''"
  check_panel "Environment is open and Context, Connectors and Change request are folded" \
    "S('environment')['expanded'] is True and all(S(k)['expanded'] is False for k in ('context', 'connectors', 'change_request'))"
  capture default-folds
  # `chat_app` left the right panel on the change request list, which asks the forge
  # while it is shown: move it off, or the log would be the list's and not the card's.
  echo "step: the right panel leaves the change request list"
  ctl surface open-list open --entry show >/dev/null
  wait_field "the right panel shows the Open list" rightPanelView open 10 surface layout read
  sleep 2
  local before after
  before=$(wc -l < "$RUN_DIR/github-requests.log")
  echo "step: five seconds with the column open (forge requests so far: $before)"
  sleep 5
  after=$(wc -l < "$RUN_DIR/github-requests.log")
  [ "$after" = "$before" ] || fail "the forge saw $((after - before)) request(s) while the card was folded ($before -> $after)"
  check_panel "the card is still folded and unread" "S('change_request')['expanded'] is False and rows('change_request') == []"
  echo "OK: the forge's whole request log did not move with the column open and the card folded ($before lines)"
  echo "step: unfolding the card reads it"
  unfold_cards change_request
  wait_panel "the card is #112" "R('change_request', 'change_request/card') and R('change_request', 'change_request/card')['label'].startswith('#112')"
  [ "$(card_reads)" -ge 1 ] || fail "the card shows #112 but the forge saw no read of the branch's change request"
  echo "OK: the forge saw $(card_reads) read(s) of the branch's change request after the unfold"
  capture default-folds-unfolded
  stop_app

  # The probe asks nothing of a host: `gh auth status` would contact it. A worktree whose
  # remote is on a host with no setting and no token has gh and glab recording every call.
  echo "step: a host nothing names, with gh and glab that record every call"
  local stubbed="$CLI_STUBS:$APP_PATH"
  [ "$(PATH="$stubbed" command -v gh)" = "$CLI_STUBS/gh" ] || fail "gh does not resolve to the recording stub"
  [ "$(PATH="$stubbed" command -v glab)" = "$CLI_STUBS/glab" ] || fail "glab does not resolve to the recording stub"
  CHAT_CWD="$STRANGER" APP_PATH="$stubbed" launch default_folds_stranger
  ctl project add "$STRANGER" >/dev/null
  open_chat
  open_column
  sleep 5
  check_panel "no change request card is drawn for a host nothing names" "S('change_request') is None"
  [ ! -s "$RUN_DIR/forge-cli.log" ] || fail "opening the column ran gh or glab: $(tr '\n' ';' < "$RUN_DIR/forge-cli.log")"
  echo "OK: opening the column on git.example.invalid ran neither gh nor glab"
  stop_app
}

scenario_context() {
  chat_app context
  open_column
  unfold_cards context
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
  unfold_cards connectors
  local codegraph="connectors/user/codegraph" notion="connectors/claude.ai/claude.ai Notion"
  wait_panel "codegraph failed, offering Reconnect" \
    "R('connectors', '$codegraph') and R('connectors', '$codegraph')['detail'] == 'failed' and R('connectors', '$codegraph')['action'] == 'Reconnect'"
  wait_panel "Notion needs sign-in, offering Sign in" \
    "R('connectors', '$notion') and R('connectors', '$notion')['detail'] == 'needs sign-in' and R('connectors', '$notion')['action'] == 'Sign in'"
  fold_cards environment
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
  unfold_cards change_request
  wait_panel "the card is #112" "R('change_request', 'change_request/card') and R('change_request', 'change_request/card')['label'].startswith('#112')"
  fold_cards environment
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
  unfold_cards change_request
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

# ---- the trace scenarios -----------------------------------------------------------
# The latest turn is Done at $0.31: the summary of the card, and the turn's own number.
DONE_031='re.match(r"^Done · .+ · \$0\.31$", S("trace")["summary"])'

scenario_trace_live() {
  trace_chat trace_live
  send_prompt subagents
  wait_panel "the Trace card appears with a live turn" 'S("trace") is not None and S("trace")["summary"].startswith("Live · ")' 30
  check_panel "the card starts folded" 'S("trace")["expanded"] is False'
  unfold_cards trace
  wait_panel "the lanes are Claude Code, two agents and the third one of them started (depth 2)" \
    '{k: (v["label"], v["depth"]) for k, v in lanes_of().items()} == {"root": ("Claude Code", 0), "toolu_agent_a": ("Explore", 1), "toolu_agent_b": ("Plan", 1), "toolu_agent_c": ("general-purpose", 2)}' 30
  wait_panel "the Edit asks: the title, the root lane and one wait row say the turn waits for the user" \
    'S("trace")["summary"].startswith("Waiting for you · ") and lane("root")["tag"] == "waiting" and len(wait_rows()) == 1' 30
  local wait_key permission
  wait_key=$(find_panel "the wait row" 'wait_rows()[0]["key"]')
  permission=$(find_panel "the transcript's pending Permission entry" 'str(pending()[0][0])')
  check_panel "the wait row and the Permission entry are the same request" "pending()[0][1] == '${wait_key#trace/wait/}' and pending()[0][0] > 0"
  capture trace-live
  echo "step: the root reveals the user's message, then the wait reveals the Permission entry ($permission)"
  panel_ctl activate trace/lane/root >/dev/null
  wait_panel "the root lane reveals the turn's user message" 'c["first"] == next(i for i, r in enumerate(c["transcript"]) if r.get("kind") == "user")' 10
  # The fixture's long first reply makes the transcript taller than a window, so with the list
  # at the user's message the card is below the last row the list drew (unpainted nothing is
  # drawn, and the last row is the first).
  check_panel "with the list at the user's message the Permission entry is below its last row" "c['last'] < $permission"
  panel_ctl activate "$wait_key" >/dev/null
  # The Permission card is the transcript's last entry while it waits, so painted it cannot be the
  # list's top: gpui backs the top up from the end of the content until the pane is full. Painted,
  # the reveal is read as the card on screen: between the first and last row the list drew, the
  # list moved there from the user's message. Unpainted the list's top sits at the card.
  wait_panel "the wait reveals the Permission entry" "c['first'] == $permission or (p['drawn'] and c['first'] <= $permission <= c['last'])" 10
  echo "step: the user allows the Edit"
  E2E rpc surface.chat.permission surfaceId="$SURFACE" requestId="${wait_key#trace/wait/}" optionId=allow_once >/dev/null || fail "allowing the Edit failed"
  wait_panel "the turn is Done at \$0.31" "$DONE_031" 40
  check_panel "no wait is left, the root lane is done and the failed Bash is the one error" \
    'len(wait_rows()) == 0 and lane("root")["tag"] == "done" and R("trace", "trace/stat/errors")["detail"] == "1" and R("trace", "trace/stat/cost")["detail"] == "$0.31"'
  check_panel "the numbers are the script's: 11 model calls, 5 tool calls, the root's own 8 calls" \
    'R("trace", "trace/stat/llm_calls")["detail"] == "11" and R("trace", "trace/stat/tool_calls")["detail"] == "5" and re.match(r"^8 calls · ", lane("root")["detail"])'
  echo "step: the tab holds each call: the Bash that failed, then the one that worked"
  panel_ctl activate trace/open >/dev/null
  wait_panel "the Trace tab is open" 'tab()["open"] == "1"' 20
  check_panel "the first Bash failed and ended, the second is done, in that order" \
    'T("toolu_bash_fail")["state"] == "failed" and T("toolu_bash_fail")["ended"] == "1" and T("toolu_bash_ok")["state"] == "done" and [s["id"] for s in tab()["spans"] if s["id"] in ("toolu_bash_fail", "toolu_bash_ok")] == ["toolu_bash_fail", "toolu_bash_ok"]'
  stop_app
}

scenario_trace_cost() {
  trace_chat trace_cost 30
  send_prompt subagents
  answer_permission allow_once
  wait_panel "the first turn is Done at \$0.31" "$DONE_031" 40
  send_prompt second
  wait_panel "the second turn's cost is the difference of the process's totals: \$0.12" \
    're.match(r"^Done · .+ · \$0\.12$", S("trace")["summary"])' 30
  unfold_cards trace
  check_panel "the Cost number says \$0.12 too" 'R("trace", "trace/stat/cost")["detail"] == "$0.12"'
  stop_app
}

scenario_trace_interrupt() {
  trace_chat trace_interrupt
  send_prompt cancel
  wait_panel "the turn is live" 'S("trace") is not None and S("trace")["summary"].startswith("Live · ")' 30
  echo "step: Stop"
  E2E rpc surface.chat.stop surfaceId="$SURFACE" >/dev/null || fail "stopping the chat failed"
  wait_panel "Stop ends the turn Interrupted" 'S("trace")["summary"].startswith("Interrupted · ")' 30
  unfold_cards trace
  check_panel "an interrupted turn's cost is unknown" 'R("trace", "trace/stat/cost")["detail"] == "—" and lane("root")["tag"] == "interrupted"'
  send_prompt second
  wait_panel "the next turn is Done, and its cost is unknown too" \
    'S("trace")["summary"].startswith("Done · ") and "$" not in S("trace")["summary"] and R("trace", "trace/stat/cost")["detail"] == "—"' 30
  stop_app
}

scenario_trace_death() {
  trace_chat trace_death
  send_prompt die
  wait_panel "the CLI dying mid-call ends the turn Failed" 'S("trace") is not None and S("trace")["summary"].startswith("Failed · ")' 40
  unfold_cards trace
  wait_panel "the root and the agent it was waiting on are lanes" 'len(lanes_of()) == 2' 10
  check_panel "every lane is failed or interrupted" 'all(v["tag"] in ("failed", "interrupted") for v in lanes_of().values())'
  stop_app
}

scenario_trace_hang() {
  trace_chat trace_hang
  send_prompt hang
  wait_panel "the turn is Done though its call never reported" 'S("trace") is not None and S("trace")["summary"].startswith("Done · ")' 40
  unfold_cards trace
  panel_ctl activate trace/open >/dev/null
  wait_panel "the Trace tab is open" 'tab()["open"] == "1"' 20
  check_panel "the call that never reported is interrupted, and ended" \
    'T("toolu_hang") is not None and T("toolu_hang")["state"] == "interrupted" and T("toolu_hang")["ended"] == "1"'
  stop_app
}

# One subagent's request: DECISION is the option to answer with, AGENT the Agent call that
# owns the subagent and TOOL the call it asks about.
subagent_permission() { # prompt agent tool
  local prompt=$1 agent=$2 tool=$3 before
  before=$(find_panel "the Permission cards the transcript has so far" 'str(len([r for r in c["transcript"] if r.get("kind") == "permission"]))' 10)
  send_prompt "$prompt"
  wait_panel "the Trace card appears" 'S("trace") is not None' 30
  ensure_unfolded trace
  wait_panel "the subagent's lane waits for the user, with one wait row" \
    "lane('$agent') is not None and lane('$agent')['tag'] == 'waiting' and len(wait_rows()) == 1" 30
  check_panel "the transcript has exactly one more Permission entry and no row for the subagent's call" \
    "len([r for r in c['transcript'] if r.get('kind') == 'permission']) == $before + 1 and row_index('$tool') is None"
  answer_permission reject_once
  wait_panel "the turn is over, nothing waits and no card is pending" \
    'c["status"] == "completed" and len(wait_rows()) == 0 and len(pending()) == 0 and S("trace")["summary"].startswith("Done · ")' 40
  check_panel "no transcript row is left pending, and the call still has no row" \
    "all(str(r.get('status', '')).lower() != 'pending' for r in c['transcript']) and row_index('$tool') is None and len([r for r in c['transcript'] if r.get('kind') == 'permission']) == $before + 1"
}

scenario_trace_subagent_permission() {
  trace_chat trace_subagent_permission
  echo "step: the request comes after the line that holds its call"
  subagent_permission subperm toolu_agent_p toolu_sub_p
  echo "step: the request comes before that line (the order seen live)"
  subagent_permission subperm_early toolu_agent_pe toolu_sub_pe
  stop_app
}

scenario_trace_reveal() {
  trace_chat trace_reveal 30
  send_prompt subagents
  answer_permission allow_once
  wait_panel "the turn is Done at \$0.31" "$DONE_031" 40
  unfold_cards trace
  echo "step: each agent's lane reveals its Agent row (at the index the transcript draws it), the root the user's message"
  local user_row
  user_row=$(find_panel "the user's message" 'str(next(i for i, r in enumerate(c["transcript"]) if r.get("kind") == "user"))' 10)
  check_panel "the two Agent rows are rows of their own, below the user's message" \
    'row_index("toolu_agent_a") is not None and drawn_index("toolu_agent_a") == row_index("toolu_agent_a") and drawn_index("toolu_agent_b") == row_index("toolu_agent_b") and row_index("toolu_agent_a") < row_index("toolu_agent_b")'
  panel_ctl activate trace/lane/root >/dev/null
  wait_panel "the root lane reveals the user's message" "c['first'] == $user_row" 10
  panel_ctl activate trace/lane/toolu_agent_b >/dev/null
  wait_panel "the Plan lane reveals the Plan Agent row" 'c["first"] == drawn_index("toolu_agent_b")' 10
  panel_ctl activate trace/lane/toolu_agent_a >/dev/null
  wait_panel "the Explore lane reveals the Explore Agent row, which is above" \
    'c["first"] == drawn_index("toolu_agent_a") and drawn_index("toolu_agent_a") < drawn_index("toolu_agent_b")' 10
  stop_app
}

scenario_trace_tab() {
  trace_chat trace_tab 30
  send_prompt subagents
  answer_permission allow_once
  wait_panel "the turn is Done at \$0.31" "$DONE_031" 40
  unfold_cards trace
  expect_trace_tabs "no trace tab before one was opened" 0
  echo "step: the card opens the tab"
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the layout lists a trace tab in the Secondary half" present 20
  wait_panel "the tab reads turn 1 of 1, following the latest, with no span left out" 'tab()["open"] == "1" and tab()["turn"] == "1" and tab()["turns"] == "1" and tab()["follow"] == "1" and tab()["dropped"] == "0"' 20
  check_panel "the tab holds the whole tree, each span under its parent" \
    '{s["id"]: (s["parent"], s["kind"], s["depth"]) for s in tab()["spans"] if s["kind"] in ("root", "subagent", "tool")} == {"root": ("", "root", "0"), "toolu_agent_a": ("root", "subagent", "1"), "toolu_agent_b": ("root", "subagent", "1"), "toolu_agent_c": ("toolu_agent_b", "subagent", "2"), "toolu_a_bash": ("toolu_agent_a", "tool", "2"), "toolu_c_read": ("toolu_agent_c", "tool", "3"), "toolu_edit": ("root", "tool", "1"), "toolu_bash_fail": ("root", "tool", "1"), "toolu_bash_ok": ("root", "tool", "1")}'
  check_panel "it also holds the model calls, each under the agent that made it" \
    '{s["id"]: s["parent"] for s in tab()["spans"] if s["kind"] == "llm"} == {"msg_main_1": "root", "msg_main_2": "root", "msg_main_3": "root", "msg_main_4": "root", "msg_main_5": "root", "msg_a_1": "toolu_agent_a", "toolu_agent_a/final": "toolu_agent_a", "msg_b_1": "toolu_agent_b", "toolu_agent_b/final": "toolu_agent_b", "msg_c_1": "toolu_agent_c", "toolu_agent_c/final": "toolu_agent_c"}'
  capture trace-tab
  echo "step: a second open keeps one tab"
  panel_ctl activate trace/open >/dev/null
  sleep 1
  expect_trace_tabs "one trace tab after two opens" 1
  echo "step: another worktree, and back"
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  sleep 1
  ctl select-workspace --workspace "$LINKED" >/dev/null
  wait_trace_tab "the trace tab is still there after a worktree switch" present 20
  wait_panel "and still reads the chat's turn" 'tab()["open"] == "1" and tab()["turns"] == "1"' 20
  echo "step: Sirio quits and starts again"
  sleep 1
  local first_surface=$SURFACE
  stop_app
  launch trace_tab
  SURFACE=""
  for _ in $(seq 1 100); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "the chat was not restored"
  echo "OK: the restored chat is $SURFACE (was $first_surface)"
  sleep 3
  expect_trace_tabs "the relaunch restored no trace tab" 0
  check_panel "no trace tab came back, and the chat has no trace of the turn before" 'tab()["open"] == "0" and S("trace") is None'
  echo "OK: no trace tab is restored"
  echo "step: a turn after the relaunch gives the chat a trace again; closing the chat closes its tab"
  send_prompt second
  wait_panel "the turn is Done" 'S("trace") is not None and S("trace")["summary"].startswith("Done · ")' 40
  unfold_cards trace
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the trace tab is open again" present 20
  local position
  position=$(tab_positions chat)
  [ -n "$position" ] || fail "the chat tab is not in the tab list"
  ctl surface tabs close "$position" --confirm >/dev/null
  wait_trace_tab "closing the chat closes its trace tab" absent 20
  stop_app
}

scenario_trace_idle() {
  PERF_TRACE="$RUN_DIR/trace_idle-perf.tsv"
  trace_chat trace_idle
  [ -s "$PERF_TRACE" ] || fail "the app wrote no performance trace at $PERF_TRACE"
  send_prompt cancel
  wait_panel "the turn is live and will stay so" 'S("trace") is not None and S("trace")["summary"].startswith("Live · ")' 30
  unfold_cards trace
  sleep 1
  frames_in_window "the card unfolded and drawn, mid-turn (the control: the animation does ask)" 3 some
  local card_entity=$LAST_ENTITIES
  [ -n "$card_entity" ] && [[ "$card_entity" != *" "* ]] || fail "the card's frames come from [$card_entity], not one owner"
  fold_cards trace
  sleep 1
  frames_in_window "the card folded and no tab, mid-turn" 3 zero
  unfold_cards trace
  echo "step: the tab opens and is selected: the chat's tab is hidden"
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the trace tab is open" present 20
  sleep 1
  frames_in_window "the tab drawn, the chat's card hidden behind it, mid-turn (the control for the tab's loop)" 3 some
  local tab_entity=$LAST_ENTITIES
  [ -n "$tab_entity" ] && [[ "$tab_entity" != *" "* ]] && [ "$tab_entity" != "$card_entity" ] \
    || fail "the tab's frames come from [$tab_entity], and the card's owner is [$card_entity]: the hidden chat asked for frames, or the owners are not told apart"
  echo "step: the chat's tab is selected again: the trace tab is hidden"
  ctl surface tabs select "$(tab_positions chat)" >/dev/null
  sleep 1
  frames_in_window "the card drawn, the trace tab hidden behind the chat, mid-turn" 3 some
  [ "$LAST_ENTITIES" = "$card_entity" ] || fail "with the tab hidden the frames came from [$LAST_ENTITIES], not from the card alone [$card_entity]"
  echo "step: the trace tab open, and a third tab selected in the Secondary half"
  ctl surface open-list open --entry changes >/dev/null
  local kinds=""
  for _ in $(seq 1 60); do
    kinds=$(secondary_kinds || true)
    case ",$kinds," in *,diff,*) break ;; esac
    sleep 0.3
  done
  # The Changes tab's kind is `diff`.
  case ",$kinds," in *,diff,*) ;; *) fail "the Changes tab did not open in the Secondary half beside the chat and the trace: [$kinds]" ;; esac
  echo "OK: the Secondary half holds [$kinds], the Changes tab selected"
  sleep 1
  frames_in_window "the trace tab open but another tab selected beside it: neither the card nor the tab is drawn, mid-turn" 3 zero
  ctl surface tabs select "$(tab_positions chat)" >/dev/null
  sleep 1
  echo "step: the worktree parked mid-turn, with the card unfolded and the tab open"
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  sleep 1
  frames_in_window "the card and the tab both open but their worktree parked, mid-turn" 3 zero
  ctl select-workspace --workspace "$LINKED" >/dev/null
  sleep 1
  ctl surface tabs select "$(tab_positions chat)" >/dev/null
  echo "step: the turn ends"
  E2E rpc surface.chat.stop surfaceId="$SURFACE" >/dev/null || fail "stopping the chat failed"
  wait_panel "the chat is no longer streaming and the turn is over" 'c["status"] != "streaming" and S("trace")["summary"].startswith("Interrupted · ")' 30
  sleep 1
  frames_in_window "the turn over, the card drawn" 3 zero
  ctl surface tabs select "$(tab_positions trace)" >/dev/null
  sleep 1
  frames_in_window "the turn over, the tab drawn" 3 zero
  cp "$PERF_TRACE" "$OUT_DIR/" 2>/dev/null || true
  stop_app
}

# The tab's keys and mouse, with real X events (xdotool): a click on a row selects it and
# reveals its Agent row, Enter on the selected row reveals it again, and a click on the
# "Follow the latest turn" checkbox toggles it. The clicks are at pixels of the 2200x1000
# window's layout (PLAN_ROW_X/Y and FOLLOW_X/Y override them), so the scenario runs only
# framed at that size. Enter *while the checkbox has focus* is not reachable from a real
# keyboard in this build, and so is not asserted: the checkbox's mouse-down prevents the
# default, which keeps a click from focusing it, and the app binds no Tab to FocusNext.
click_at() { # x y
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove "$1" "$2" click 1 || fail "xdotool could not click at $1,$2"
  sleep 1
}
press_key() { # key
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool key "$1" || fail "xdotool could not press $1"
  sleep 1
}
scenario_trace_keys() {
  trace_chat trace_keys 30
  send_prompt subagents
  answer_permission allow_once
  wait_panel "the turn is Done at \$0.31" "$DONE_031" 40
  ensure_unfolded trace
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the trace tab is open" present 20
  wait_panel "the tab shows the turn, following the latest" 'tab()["open"] == "1" and tab()["follow"] == "1"' 20
  capture trace-keys-before
  echo "step: a real click on the Plan row reveals its Agent row"
  panel_ctl activate trace/lane/root >/dev/null
  wait_panel "the list is at the user's message" 'c["first"] == 0' 10
  click_at "${PLAN_ROW_X:-1157}" "${PLAN_ROW_Y:-323}"
  wait_panel "the click revealed the Plan Agent row" 'c["first"] == drawn_index("toolu_agent_b")' 10
  echo "step: the list goes back to the top, then a real Enter on the selected row reveals it again"
  panel_ctl activate trace/lane/root >/dev/null
  wait_panel "the list is at the user's message again" 'c["first"] == 0' 10
  press_key Return
  wait_panel "Enter on the selected row revealed the Plan Agent row" 'c["first"] == drawn_index("toolu_agent_b")' 10
  capture trace-keys-enter
  echo "step: a real click on the Follow checkbox toggles it, and another toggles it back"
  click_at "${FOLLOW_X:-1702}" "${FOLLOW_Y:-97}"
  wait_panel "the click toggled Follow off" 'tab()["follow"] == "0"' 10
  capture trace-keys-follow-off
  click_at "${FOLLOW_X:-1702}" "${FOLLOW_Y:-97}"
  wait_panel "the second click toggled Follow on" 'tab()["follow"] == "1"' 10
  stop_app
}

# A chat response's file links (spec 2026-10-24-chat-file-links-design.md §6), with real X
# input at the pixels `transcriptLinks` reports for the last paint. Opening a file selects its
# tab, which hides the chat, so each click is preceded by showing the chat tab again. The
# files are written into the worktree before the app starts; the stub's one answer names
# them, names one that is not there, a version, a fenced path and a web link.
# A row of `transcriptLinks` also carries `w`, the label's painted width: the second drag ends
# 10 px past the label's end (x + w + 10), where plain text follows it on the line.
# The tabs: `surface tabs read` lists them (`count`, and `active` is the selected one's
# position); `surface editor state` reads the active tab's file, so it only answers while a
# file tab is selected. The steps that must open nothing therefore compare `count` and `active`
# with what they were, the chat tab being the one selected.
tabs_state() { # prints "COUNT ACTIVE"
  reply surface tabs read | python3 -c 'import json, sys; t = json.load(sys.stdin)[0]; print(t["count"], t["active"])'
}
tab_count() { tabs_state | cut -d' ' -f1; }
show_chat() {
  local position
  position=$(tab_positions chat)
  [ -n "$position" ] || fail "no chat tab to show"
  reply surface tabs select "$position" >/dev/null || fail "could not select the chat tab $position"
  sleep 1.5
}
# The pixel of a link of KIND and LABEL in the chat's last paint.
link_xy() { # kind label
  local xy
  xy=$(E2E link "$SURFACE" "$1" "$2") || fail "no $1 link '$2' to point at"
  echo "  link: $1 '$2' at $xy" >&2
  echo "$xy"
}
# A click on the link of KIND and LABEL, the chat shown first. The press and the release are
# 0.3 s apart, as a hand's are: a web link's text only listens for the release in the frame
# after the press, and this window paints in software, so an instant `click 1` outruns it.
click_link() { # kind label [modifiers...]
  local kind=$1 label=$2 xy
  shift 2
  show_chat
  xy=$(link_xy "$kind" "$label")
  if [ $# -gt 0 ]; then
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool keydown "$@" mousemove ${xy% *} ${xy#* } mousedown 1 sleep 0.3 mouseup 1 keyup "$@" || fail "xdotool could not click at $xy with $*"
  else
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove ${xy% *} ${xy#* } mousedown 1 sleep 0.3 mouseup 1 || fail "xdotool could not click at $xy"
  fi
  sleep 1
}
# A drag from (X1, Y1) to (X2, Y2): a press, two moves, a release.
drag_at() { # x1 y1 x2 y2
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool mousemove "$1" "$2" mousedown 1 sleep 0.3 \
    mousemove $((($1 + $3) / 2)) "$4" sleep 0.3 mousemove "$3" "$4" sleep 0.3 mouseup 1 \
    || fail "xdotool could not drag from $1,$2 to $3,$4"
  sleep 1
}
# The byte offset of the start of line N of FILE.
line_offset() { # file n
  python3 -c 'import sys; print(sum(len(l) for l in open(sys.argv[1], "rb").readlines()[:int(sys.argv[2]) - 1]))' "$1" "$2"
}
# Polls the active file tab until it is PATH, in MODE (any mode when it is empty) and with
# SELECTIONS, when given.
expect_editor() { # desc path mode [selections]
  local desc=$1 want_path=$2 want_mode=$3 want_selections=${4:-} out="" _
  for _ in $(seq 1 50); do
    out=$(reply surface editor state || true)
    if printf '%s' "$out" | python3 -c '
import json, sys
d = json.load(sys.stdin)[0]
ok = d.get("path") == sys.argv[1] and (not sys.argv[2] or d.get("mode") == sys.argv[2]) and d.get("state") == "loaded"
ok = ok and (not sys.argv[3] or d.get("selections") == sys.argv[3])
sys.exit(0 if ok else 1)' "$want_path" "$want_mode" "$want_selections" 2>/dev/null; then
      echo "  read: $(printf '%s' "$out" | python3 -c 'import json, sys; d = json.load(sys.stdin)[0]; print({k: d.get(k) for k in ("path", "mode", "state", "selections")})')"
      echo "OK: $desc"
      return 0
    fi
    sleep 0.3
  done
  echo "  read: $(printf '%s' "$out" | cut -c1-400)" >&2
  fail "$desc: the editor is not on $want_path in $want_mode${want_selections:+ with selections $want_selections}"
}
# The browser stub's log: how many calls it has recorded. gpui starts `xdg-open` without
# waiting for it, so a check that nothing was recorded lets it run first.
browser_calls() {
  sleep 1
  if [ -f "$RUN_DIR/browser.log" ]; then wc -l <"$RUN_DIR/browser.log"; else echo 0; fi
}
scenario_file_links() {
  use_stub file-links
  local docs_main="$LINKED/docs/NOME-FILE.md" docs_other="$LINKED/docs/ALTRO.md" main_rs="$LINKED/src/main.rs" readme="$LINKED/README.md"
  mkdir -p "$LINKED/docs"
  python3 - "$docs_main" "$docs_other" "$main_rs" "$readme" <<'PY'
import sys
docs_main, docs_other, main_rs, readme = sys.argv[1:]
for path in (docs_main, docs_other):
    open(path, "w").write("".join(f"Riga {n} di {path.rsplit('/', 1)[1]}\n" for n in range(1, 61)))
open(main_rs, "w").write("".join(f"// riga {n}\n" if n != 1 else "fn main() {}\n" for n in range(1, 61)))
open(readme, "w").write("# Widgets\n\nIl file che il progetto ha già.\n")
PY
  # What opens a URL is gpui's `xdg-open`, started in `sirio_env::for_child`'s PATH: the login
  # shell's (`$SHELL -lc`, run in this private HOME with the launch PATH, which it extends and
  # never reorders) before the launch's. The harness's `xdg-open` stub leads both, and this
  # profile keeps it first even if a system profile prepended directories, so any `xdg-open`
  # the app runs (a file:// as much as the web link) lands in $RUN_DIR/browser.log and never
  # starts a real browser on the display.
  mkdir -p "$RUN_DIR/file_links-home"
  printf 'export PATH="%s/bin:$PATH"\n' "$RUN_DIR" >"$RUN_DIR/file_links-home/.bash_profile"
  launch file_links
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  send_prompt "mostrami i file"
  wait_panel "the turn is over" 'c["status"] == "completed"' 40
  echo "step: the links of the response"
  E2E links-check "$SURFACE" 30 sito '/docs/NOME-FILE.md' 'src/main.rs:42' 'README.md' 'docs/NOME-FILE.md:30' 'docs/ALTRO.md:10' \
    || fail "the response's links are not the expected ones"
  capture file-links-response
  local before browser xy line _
  before=$(tab_count)
  [ "$before" = 1 ] || fail "the chat is not the only tab: $(tab_count) tabs"

  echo "step: a click on /docs/NOME-FILE.md opens the Markdown file in Preview"
  click_link file '/docs/NOME-FILE.md'
  expect_editor "/docs/NOME-FILE.md opened in Preview" "$docs_main" preview
  [ "$(tab_count)" = $((before + 1)) ] || fail "the click did not open exactly one more tab: $(tab_count) tabs, was $before"
  echo "OK: one more tab"

  echo "step: a click on src/main.rs:42 opens the file in code at its line 42"
  click_link file 'src/main.rs:42'
  expect_editor "src/main.rs:42 opened in code at line 42" "$main_rs" code "$(line_offset "$main_rs" 42)..$(line_offset "$main_rs" 42)"
  local after_main
  after_main=$(tab_count)
  [ "$after_main" = $((before + 2)) ] || fail "src/main.rs:42 did not open exactly one more tab: $after_main tabs"

  echo "step: a click on docs/NOME-FILE.md:30 reuses the tab of step 2, in code at line 30"
  click_link file 'docs/NOME-FILE.md:30'
  expect_editor "docs/NOME-FILE.md:30 switched its open tab to code at line 30" "$docs_main" code "$(line_offset "$docs_main" 30)..$(line_offset "$docs_main" 30)"
  [ "$(tab_count)" = "$after_main" ] || fail "the second click on the Markdown file opened a tab: $(tab_count) tabs, was $after_main"
  echo "OK: the tab was reused"

  echo "step: a click on docs/ALTRO.md:10, never opened before, opens it in code at line 10"
  click_link file 'docs/ALTRO.md:10'
  expect_editor "docs/ALTRO.md:10 opened in code at line 10, not in Preview" "$docs_other" code "$(line_offset "$docs_other" 10)..$(line_offset "$docs_other" 10)"
  [ "$(tab_count)" = $((after_main + 1)) ] || fail "docs/ALTRO.md:10 did not open exactly one more tab: $(tab_count) tabs"

  echo "step: Super+Shift+click on README.md still opens the editor, not the browser"
  browser=$(browser_calls)
  click_link file 'README.md' super shift
  expect_editor "README.md opened in the editor" "$readme" ""
  [ "$(browser_calls)" = "$browser" ] || fail "Super+Shift+click on README.md called the browser: $(tail -n 3 "$RUN_DIR/browser.log")"
  echo "OK: the browser stub recorded no call"

  echo "step: a drag inside src/main.rs:42 selects and opens nothing"
  show_chat
  local count_active
  count_active=$(tabs_state)
  xy=$(link_xy file 'src/main.rs:42')
  drag_at "${xy% *}" "${xy#* }" $((${xy% *} + 17)) "${xy#* }"
  [ "$(tabs_state)" = "$count_active" ] || fail "a drag inside src/main.rs:42 changed the tabs: '$(tabs_state)', was '$count_active'"
  [ "$(browser_calls)" = "$browser" ] || fail "a drag inside src/main.rs:42 called the browser: $(tail -n 3 "$RUN_DIR/browser.log")"
  echo "OK: nothing opened (tabs '$count_active')"

  echo "step: a drag from inside src/main.rs:42 to just past its end opens nothing either"
  xy=$(link_xy file 'src/main.rs:42')
  local width
  width=$(E2E link-width "$SURFACE" file 'src/main.rs:42') || fail "no painted width for src/main.rs:42"
  [ "$width" -gt 20 ] || fail "src/main.rs:42 reports a painted width of $width px"
  drag_at "${xy% *}" "${xy#* }" $((${xy% *} + width + 10)) "${xy#* }"
  [ "$(tabs_state)" = "$count_active" ] || fail "a drag across src/main.rs:42 changed the tabs: '$(tabs_state)', was '$count_active'"
  [ "$(browser_calls)" = "$browser" ] || fail "a drag across src/main.rs:42 called the browser: $(tail -n 3 "$RUN_DIR/browser.log")"
  echo "OK: nothing opened (tabs '$count_active')"
  capture file-links-drag

  echo "step: a click on the web link still reaches the browser"
  click_link web sito
  [ "$(tabs_state)" = "$count_active" ] || fail "the web link opened a tab: '$(tabs_state)', was '$count_active'"
  sleep 1
  line=""
  for _ in $(seq 1 50); do
    line=$(grep -Fx 'xdg-open https://example.com/docs/NOME-FILE.md' "$RUN_DIR/browser.log" 2>/dev/null | head -n 1 || true)
    [ -n "$line" ] && break
    sleep 0.3
  done
  [ -n "$line" ] || fail "the harness's xdg-open stub never recorded 'xdg-open https://example.com/docs/NOME-FILE.md' (it recorded: $(cat "$RUN_DIR/browser.log" 2>/dev/null || echo nothing))"
  echo "OK: the xdg-open stub recorded: $line"
  echo "OK: file_links: the response's paths open their files, a drag opens nothing, and the web link reaches the browser"
  stop_app
  # The fixture worktree is shared with the scenarios that run after this one.
  rm -rf "$LINKED/docs" "$LINKED/README.md"
  printf 'fn main() {}\n' >"$main_rs"
}

ran=0
for name in "${SCENARIOS[@]}"; do
  echo "== scenario $name"
  use_stub session-panel
  TRACE_STEP_MS=400
  PERF_TRACE=""
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
  echo "CHAT SESSION PANEL E2E OK ($ran scenarios, $skipped skipped, appearance ${APPEARANCE:-default}, state-only $STATE_ONLY, window ${WINDOW_SIZE:-default})"
fi
