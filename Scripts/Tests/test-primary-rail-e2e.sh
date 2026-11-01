#!/bin/bash
set -eEuo pipefail

# End-to-end test of the Primary rail and its peek (spec
# docs/superpowers/specs/2026-11-01-primary-rail-design.md §8): a real,
# isolated debug Sirio on a real git repository, driven by real X clicks and
# keys at the pixels the app reports, and read over the control socket. It
# proves fifteen steps, in order (§8 cases 1-11, then the plan's Review Focus
# 2-5); each failure names its step:
#    1  with no Secondary tab the strip draws no collapse button
#    2  a file opens in the Secondary; a click on collapse makes the Primary a
#       40 px rail, the Secondary takes its width, the rail lists the Primary
#       tabs in strip order
#    3  the anti-goal: a terminal and a native chat produce output and change
#       status while collapsed, the rail follows, and each one's peek holds all
#       of it
#    4  a peeked terminal has the columns it had in the half, and a real Esc
#       reaches its program with the peek still open
#    5  a real Esc in a peeked chat cancels its running turn, the peek stays
#    6  a click outside closes the peek; one icon clicked twice opens, closes
#    7  the rail's + -> New Terminal: a new Primary tab, peeked
#    8  the pin expands the half with that tab active
#    9  collapsed survives a relaunch; closing the last Secondary tab expands,
#       and a relaunch keeps it expanded
#   10  selecting a Primary tab while collapsed peeks it, and a status change
#       on the peeked tab posts no desktop notification
#   11  captures: the rail, a peeked chat, a peeked terminal, and a peek over a
#       Browser tab (the page blank while the peek is open, back after)
#   12  closing the peeked tab over the socket closes the peek
#   13  a worktree switch and back with a peek open: no peek, still collapsed
#   14  closing the last Primary tab from inside the peek leaves the rail with
#       its expand button and its +
#   15  Ctrl+Shift+B while collapsed hides the Secondary and expands the
#       Primary, and a relaunch keeps it expanded
#
# What the app must report (`surface layout read`, all values strings):
# primaryCollapsed, peekTab (a 1-based position of `surface tabs read`, or
# empty), railTabs (`position:status`, comma separated, in rail order),
# primaryWidth, secondaryWidth, and, in debug builds, railRects, a JSON object
# of window-pixel [x, y, w, h] rects or nulls: collapse, expand, new, peek,
# pin, menuTerminal (the New menu's New Terminal row while that menu is open)
# and tabs (position -> rect).
#
# Two routes are not the ones §8 names, and say so where they run:
#   - step 10 selects the tab with `surface tabs select`, a route §5 lists:
#     the socket reports no rect for a sidebar pill to click. Its status change
#     is an agent terminal's, not a chat's: a chat's status never posts a
#     desktop notification (post_activity_notification needs the pane's agent
#     id, which only terminals have), so "no line" would prove nothing for it.
#     The step first proves, on the same tab collapsed and not peeked, that the
#     change does post a line.
#   - the status-changing terminal is not stubs/hook-agents/claude: `claude`
#     on this run's PATH is the native chat's shim (stubs/trace/claude). It is
#     a `codex` this script writes into the run's bin: it answers --version,
#     and inside its pane it waits for go-files and then prints numbered lines,
#     reports `stty size`, reads one key, or runs `sirioctl notify --session
#     <its pane> --status <s>`, the command every agent hook runs.
# The chat is the native Claude chat over stubs/trace/claude (the fixture's
# trace mode): `subagents` runs a turn that waits on a permission, `cancel` a
# turn that runs until it is interrupted. The Open list opens it in the
# Secondary; a real Ctrl+Shift+M moves it into the Primary.
#
# Isolation: the app starts under `env -i` with an allowlist (launch_app),
# from a copy of its binary with no sirio-host beside it and SIRIO_HOST_BIN
# naming a missing file, so no host is found or started and terminals take the
# in-process fallback. PATH is the notify-send stub, the chat shim, the run's
# bin, then /usr/bin:/bin; HOME, XDG directories, the database, the socket and
# the credentials are the run's own. Every process the app starts carries
# SIRIO_RAIL_E2E_RUN=<run dir>; the last check quits the app and fails if any
# of them is still alive.
#
# --state-only runs with no display: nothing paints, so no rect exists and no
# click or key can be sent, and no socket verb collapses the half (§2). It
# runs step 1's reads and prints a SKIP line, with its reason, for each of
# steps 2-15. A framed run needs --display :N, a private Xvfb of at least
# 1600x1000 that this user started (never $DISPLAY), Mesa's lavapipe
# (lvp_icd*.json) and import, identify, xwininfo, xprop, xdotool and
# ImageMagick's magick or convert; step 11's Browser tab needs WebKitGTK. The
# window is moved to 0,0 and sized 1600x1000.
#
# The artifact: --out-dir DIR (default artifacts/primary-rail-e2e-<stamp>-<pid>)
# keeps transcript.log, app.log, notify.log, stub.log and -- unless
# --state-only -- PID-matched window captures in frames/. Run once with
# --appearance light and once with --appearance dark for both sets.
#
# Usage: Scripts/Tests/test-primary-rail-e2e.sh [--state-only] [--out-dir DIR]
#          [--display :N] [--appearance light|dark]
#   Xvfb :97 -screen 0 1920x1200x24 &   # a private display for a framed run

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
WINDOW_W=1600
WINDOW_H=1000
STATE_ONLY=0
OUT_DIR=""
APPEARANCE=""
DISPLAY_TARGET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

refuse() { echo "refused: $*" >&2; exit 2; }
case "$APPEARANCE" in ""|light|dark) ;; *) refuse "--appearance is light or dark, not $APPEARANCE" ;; esac
[ "$STATE_ONLY" -eq 0 ] || [ -z "$DISPLAY_TARGET" ] || refuse "--state-only runs with no display; drop --display"
# The display number of a local X display: ":97", ":0097" and ":97.0" are all 97.
display_number() { # DISPLAY-string
  [[ "$1" =~ ^([A-Za-z0-9._-]*):([0-9]+)(\.[0-9]+)?$ ]] || return 0
  [ -z "${BASH_REMATCH[1]}" ] || [ "${BASH_REMATCH[1]}" = localhost ] || return 0
  echo $((10#${BASH_REMATCH[2]}))
}
IM=""
if [ "$STATE_ONLY" -eq 0 ]; then
  for tool in import identify xwininfo xprop xdotool timeout; do
    command -v "$tool" >/dev/null || refuse "$tool is required for a framed run; pass --state-only to skip the clicks"
  done
  IM=$(command -v magick || command -v convert || true)
  [ -n "$IM" ] || refuse "ImageMagick's magick or convert is required to read step 11's captures"
  [ -n "$DISPLAY_TARGET" ] || refuse "no --display: the clicks need a display of their own; pass --display :N (a private Xvfb), or --state-only"
  target_number=$(display_number "$DISPLAY_TARGET")
  [ -n "$target_number" ] || refuse "--display takes a local X display such as :97, not '$DISPLAY_TARGET'"
  [ "$target_number" != "$(display_number "${DISPLAY:-}")" ] \
    || refuse "--display $DISPLAY_TARGET is this session's own \$DISPLAY: start a private display (Xvfb :$target_number -screen 0 1920x1200x24) and pass it"
  ps -u "$(id -u)" -o args= | python3 -c 'import re, sys; n = sys.argv[1]; sys.exit(0 if any(re.search(r"(^|/)Xvfb :" + n + r"( |$)", line) for line in sys.stdin) else 1)' "$target_number" \
    || refuse "no Xvfb :$target_number is running as $(id -un): start a private one (Xvfb :$target_number -screen 0 1920x1200x24 &)"
  DISPLAY_TARGET=":$target_number"
  geometry=$(DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool getdisplaygeometry 2>/dev/null || true)
  [ -n "$geometry" ] || refuse "no X server answers on $DISPLAY_TARGET"
  { [ "${geometry% *}" -ge "$WINDOW_W" ] && [ "${geometry#* }" -ge "$WINDOW_H" ]; } \
    || refuse "the display is ${geometry% *}x${geometry#* }, smaller than the ${WINDOW_W}x${WINDOW_H} window: start a larger Xvfb"
fi

[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/primary-rail-e2e-$(date +%Y%m%d-%H%M%S)-$$"
if [ -d "$OUT_DIR" ] && [ -n "$(ls -A "$OUT_DIR")" ]; then
  refuse "$OUT_DIR already has content (pass a new --out-dir)"
fi
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

STEP=setup
fail() { echo "FAIL: $STEP: $*" >&2; exit 1; }
# A command that fails outside a check still names the step it failed in.
trap 'echo "FAIL: $STEP: \`$BASH_COMMAND\` failed (exit $?)" >&2' ERR
# Starts a step: every failure from here on names it.
step() { # n text
  STEP="step $1"
  echo "step $1: $2"
}
[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux"
command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required"
echo "display: ${DISPLAY_TARGET:-none (--state-only)}; appearance: ${APPEARANCE:-default}"

# ---- the run's own directory and what must be stopped ---------------------
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-rail-XXXXXX")
APP_PID=""
PAGE_PID=""
# Every process the app started carries this run's marker in its environment.
run_processes() {
  python3 - "$RUN_DIR" <<'PY'
import os, sys
marker = b"SIRIO_RAIL_E2E_RUN=" + sys.argv[1].encode() + b"\0"
for entry in os.listdir("/proc"):
    if not entry.isdigit() or int(entry) == os.getpid():
        continue
    try:
        environ = open(f"/proc/{entry}/environ", "rb").read()
    except OSError:
        continue
    if marker in environ:
        print(entry)
PY
}
stop_run_processes() {
  local pids _
  for _ in $(seq 1 15); do
    pids=$(run_processes)
    [ -n "$pids" ] || return 0
    # shellcheck disable=SC2086 # one pid per word
    kill $pids 2>/dev/null || true
    sleep 0.2
  done
  # shellcheck disable=SC2086 # one pid per word
  kill -9 $(run_processes) 2>/dev/null || true
}
cleanup() {
  [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; }
  [ -z "$PAGE_PID" ] || kill "$PAGE_PID" 2>/dev/null || true
  stop_run_processes
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf --one-file-system "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl) \
  2>>"$RUN_DIR/build.log" || { tail -n 40 "$RUN_DIR/build.log" >&2; fail "the build failed"; }

# A copy of the app with no sibling sirio-host: the host lookup would otherwise
# find target/debug/sirio-host and the terminals would start a host.
mkdir -p "$RUN_DIR/alone"
cp "$BIN" "$RUN_DIR/alone/sirio"
APP_BIN="$RUN_DIR/alone/sirio"

# ---- the repository: a main worktree and a second one for step 13 ---------
WT="$RUN_DIR/repo"
OTHER="$RUN_DIR/other"
mkdir -p "$WT"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
printf '# app\n\nThe file the Secondary half shows.\n' >"$WT/README.md"
git -C "$WT" add -A
git -C "$WT" commit -q -m first
git -C "$WT" worktree add -q -b other "$OTHER"

# ---- stubs ---------------------------------------------------------------
STUB_DIR="$RUN_DIR/stub"
NOTIFY_LOG="$RUN_DIR/notify.log"
mkdir -p "$RUN_DIR/bin" "$STUB_DIR"
: >"$NOTIFY_LOG"
# The status-changing terminal's agent (see the header). Its go-files are
# $STUB_DIR/go.<pane>.<what>, so two of its panes never share one.
cat >"$RUN_DIR/bin/codex" <<'STUB_EOF'
#!/bin/bash
case "${1:-}" in --version|-V|version) echo "codex-cli 9.9.9 (primary rail e2e stub)"; exit 0 ;; esac
dir=${RAIL_STUB_DIR:?RAIL_STUB_DIR must name the stub directory}
pane=${SIRIO_PANE_ID:?codex must run inside a Sirio pane}
printf '%s\n' "$pane" >"$dir/codex.pane.$$" && mv -f "$dir/codex.pane.$$" "$dir/codex.pane"
echo "rail stub in $pane"
while :; do
  for gate in "$dir/go.$pane".*; do
    [ -e "$gate" ] || continue
    what=${gate##*.}
    rm -f "$gate"
    case "$what" in
      output)
        for n in 01 02 03 04 05 06 07 08 09 10 11 12; do echo "rail-line-$n"; sleep 0.2; done
        echo "rail-output-end"
        touch "$dir/output.$pane" ;;
      size)
        stty size >"$dir/size.$pane.$$" && mv -f "$dir/size.$pane.$$" "$dir/size.$pane" ;;
      esc)
        echo "ESC-READY"
        touch "$dir/esc-ready.$pane"
        IFS= read -rsn1 key
        printf 'GOT[%q]\n' "$key"
        printf '%q\n' "$key" >"$dir/esc.$pane" ;;
      status-*)
        status=0
        "$RAIL_SIRIOCTL" notify --session "$pane" --status "${what#status-}" || status=$?
        echo "$pane ${what#status-} exit=$status" >>"$dir/stub.log" ;;
    esac
  done
  sleep 0.2
done
STUB_EOF
chmod +x "$RUN_DIR/bin/codex"
# What opens a URL outside the app, if anything does.
for name in xdg-open browser-stub; do
  printf '#!/bin/sh\nprintf "%%s\\n" "%s $*" >>"%s/browser.log"\n' "$name" "$RUN_DIR" >"$RUN_DIR/bin/$name"
  chmod +x "$RUN_DIR/bin/$name"
done
APP_PATH="$ROOT/Scripts/Tests/stubs/notify-send:$ROOT/Scripts/Tests/stubs/trace:$RUN_DIR/bin:/usr/bin:/bin"
for name in notify-send claude codex; do
  case "$name" in
    notify-send) want="$ROOT/Scripts/Tests/stubs/notify-send/notify-send" ;;
    claude) want="$ROOT/Scripts/Tests/stubs/trace/claude" ;;
    codex) want="$RUN_DIR/bin/codex" ;;
  esac
  [ "$(PATH="$APP_PATH" command -v "$name" || true)" = "$want" ] || fail "$name does not resolve to $want under the app's PATH"
done
echo "OK: notify-send, claude and codex resolve to this run's stubs"

# ---- the page step 11's Browser tab shows: all magenta --------------------
PAGE_COLOUR='#ff00ff'
mkdir -p "$RUN_DIR/www"
printf '<!doctype html><title>rail-e2e-page</title><body style="margin:0;background:%s;height:100vh"></body>\n' "$PAGE_COLOUR" >"$RUN_DIR/www/index.html"

# ---- calling the app -----------------------------------------------------
export SIRIO_SOCKET="$RUN_DIR/c.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
[ "${#SIRIO_SOCKET}" -lt 108 ] || fail "the control socket path is too long for a unix socket: $SIRIO_SOCKET (point TMPDIR at a shorter directory)"
CALL_TIMEOUT=30
ctl() { echo "+ sirioctl $*"; timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@"; }
reply() { timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" --json 2>/dev/null; }

# The reads: the control socket's line-delimited JSON, spoken directly.
cat >"$RUN_DIR/rail.py" <<'PY'
import json, os, socket, sys, time


def rpc(method, params=None):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(os.environ["SIRIO_SOCKET"])
    s.sendall((json.dumps({"id": "rail", "method": method, "params": params or {}}) + "\n").encode())
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


def layout():
    """`surface.layout.read` with the rail's keys decoded. A missing key raises, so a
    read without it counts as "not yet", never as a match."""
    L = rpc("surface.layout.read")
    rects = json.loads(L["railRects"]) if L.get("railRects") else None
    rail = [(int(p), s) for p, s in (item.split(":", 1) for item in L["railTabs"].split(",") if item)]
    return L, rects, rail


def tabs():
    t = rpc("surface.tabs.read")
    count = int(t["count"])
    return {"count": count, "active": int(t["active"]),
            "kinds": [t[f"tab.{i}"].split("|")[0] for i in range(1, count + 1)]}


def namespace(L, R, rail):
    def num(key):
        return float(L[key])

    def secondary():
        return [int(p) for p in L["secondaryPositions"].split(",") if p]

    def primary():
        return [p for p in range(1, tabs()["count"] + 1) if p not in secondary()]

    def status(position):
        return next((s for p, s in rail if p == position), None)

    def peek():
        return int(L["peekTab"]) if L["peekTab"] else None

    return {"L": L, "R": R, "rail": rail, "num": num, "secondary": secondary,
            "primary": primary, "status": status, "peek": peek, "tabs": tabs}


def chat(surface):
    c = dict(rpc("surface.chat.read", {"surfaceId": surface}))
    c.pop("sessionPanel", None)
    c["transcript"] = json.loads(c.get("transcript") or "[]")
    return c


def tool_ids(c):
    return [r.get("id") for r in c["transcript"] if r.get("kind") == "tool"]


def pending(c):
    return [r["id"] for r in c["transcript"] if r.get("kind") == "permission" and r.get("status") == "pending"]


def until(seconds, attempt, desc):
    """Calls attempt() until it answers (True, value); returns the value. An attempt that
    raises (a failed read, a missing key) is "not yet", never a match."""
    deadline = time.time() + seconds
    error = None
    while True:
        try:
            ok, value = attempt()
            if ok:
                return value
            error = None
        except Exception as e:  # tried again until the time is up
            error = e
        if time.time() > deadline:
            print(f"  last read: {LAST.get('read')}", file=sys.stderr)
            print(f"  {desc}: not true within {seconds:.0f} s" + (f" ({error!r})" if error else ""), file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)


LAST = {}


def layout_value(expr):
    L, R, rail = layout()
    LAST["read"] = json.dumps(L)
    return eval(expr, namespace(L, R, rail))


def chat_value(surface, expr):
    c = chat(surface)
    LAST["read"] = json.dumps({k: v for k, v in c.items() if k != "transcript"} | {"tools": tool_ids(c), "pending": pending(c)})
    return eval(expr, {"c": c, "tool_ids": lambda: tool_ids(c), "pending": lambda: pending(c)})


command = sys.argv[1]
if command == "wait":
    # wait DESC SECONDS EXPR: poll the layout until EXPR is truthy.
    desc, seconds, expr = sys.argv[2], float(sys.argv[3]), sys.argv[4]

    def attempt():
        value = layout_value(expr)
        return bool(value), value
    until(seconds, attempt, f"{desc}: {expr}")
    print(f"OK: {desc}")
elif command == "get":
    # get EXPR: the value EXPR takes over the first layout read that answers.
    print(until(15, lambda: (True, layout_value(sys.argv[2])), f"get {sys.argv[2]}"))
elif command == "center":
    # center KEY [POSITION]: the window pixel at the centre of railRects[KEY], or of
    # railRects["tabs"][POSITION], once a paint has recorded it.
    key = sys.argv[2]
    expr = f'R["tabs"]["{sys.argv[3]}"]' if key == "tabs" else f'R["{key}"]'

    def attempt():
        box = layout_value(expr)
        return box is not None, box
    box = until(15, attempt, f"railRects has a {' '.join(sys.argv[2:])} rect")
    print(f"{round(box[0] + box[2] / 2)} {round(box[1] + box[3] / 2)}")
elif command == "outside":
    # outside: a window pixel on the Secondary, right of the open peek, at its middle height.
    def attempt():
        L, R, rail = layout()
        LAST["read"] = json.dumps(L)
        peek, expand = R["peek"], R["expand"]
        rail_right = expand[0] + expand[2] / 2 + 20
        secondary_right = rail_right + float(L["secondaryWidth"])
        peek_right = peek[0] + peek[2]
        return True, ((peek_right + secondary_right) / 2, peek[1] + peek[3] / 2, peek_right, secondary_right)
    x, y, peek_right, secondary_right = until(15, attempt, "the peek's rect")
    if x < peek_right + 10 or x > secondary_right - 10:
        print(f"  no room right of the peek: it ends at {peek_right}, the Secondary at {secondary_right}", file=sys.stderr)
        sys.exit(1)
    print(f"{round(x)} {round(y)}")
elif command == "positions":
    # positions KIND: the positions of `surface tabs read` holding a tab of that kind.
    print(" ".join(str(i + 1) for i, kind in enumerate(tabs()["kinds"]) if kind == sys.argv[2]))
elif command == "tabs":
    t = tabs()
    print(t["count"], t["active"])
elif command == "rpc":
    params = dict(arg.split("=", 1) for arg in sys.argv[3:])
    print(json.dumps(rpc(sys.argv[2], params)))
elif command == "chat-surface":
    print(rpc("surface.chat.open")["surfaceId"])
elif command in ("chat-wait", "chat-find"):
    # chat-wait SURFACE DESC SECONDS EXPR: poll `surface.chat.read` until EXPR is truthy;
    # chat-find prints the value instead of OK. EXPR sees c, tool_ids() and pending().
    surface, desc, seconds, expr = sys.argv[2], sys.argv[3], float(sys.argv[4]), sys.argv[5]

    def attempt():
        value = chat_value(surface, expr)
        return bool(value), value
    got = until(seconds, attempt, f"{desc}: {expr}")
    print(got if command == "chat-find" else f"OK: {desc}")
else:
    sys.exit(f"unknown command {command}")
PY
RAIL() { python3 "$RUN_DIR/rail.py" "$@"; }
# Polls `surface layout read` until EXPR holds. EXPR sees L (the read), R (railRects),
# rail ((position, status) pairs), num(key), secondary(), primary(), status(position),
# peek() and tabs().
wait_layout() { # desc expr [seconds]
  RAIL wait "$1" "${3:-30}" "$2" || fail "$1"
}
layout_get() { # expr
  RAIL get "$1" || fail "could not read $1 from the layout"
}
wait_chat() { # desc expr [seconds]
  RAIL chat-wait "$SURFACE" "$1" "${3:-30}" "$2" || fail "$1"
}
tab_count() { local t; t=$(RAIL tabs) || fail "could not read the tabs"; echo "${t% *}"; }
tab_active() { local t; t=$(RAIL tabs) || fail "could not read the tabs"; echo "${t#* }"; }

# ---- launching an isolated Sirio -----------------------------------------
if [ -n "$APPEARANCE" ]; then
  (cd "$ROOT/rust" && cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$SIRIO_DB" --appearance "$APPEARANCE") \
    || fail "could not seed the appearance"
fi
VULKAN_ENV=()
if [ "$STATE_ONLY" -eq 0 ]; then
  LVP=$(find /usr/share/vulkan/icd.d -name 'lvp_icd*.json' 2>/dev/null | head -n 1 || true)
  [ -n "$LVP" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required for a framed run on Xvfb; install vulkan-swrast or pass --state-only"
  VULKAN_ENV=("VK_DRIVER_FILES=$LVP" "VK_ICD_FILENAMES=$LVP")
fi
mkdir -p "$RUN_DIR/home" "$RUN_DIR/xdg/config" "$RUN_DIR/xdg/data" "$RUN_DIR/xdg/state" "$RUN_DIR/xdg/cache"
mkdir -m 700 "$RUN_DIR/xdg/runtime"
# `env -i` and this allowlist, so nothing of the user's reaches the app:
#   PATH          the stubs first (header), then the system directories
#   HOME, XDG_*   private; XDG_RUNTIME_DIR mode 700 with no bus under it
#   SHELL, TERM, LANG  the login shell Sirio reads its environment from; text
#   BROWSER       a stub that logs, should anything open a URL outside the app
#   SIRIO_SOCKET, SIRIO_DB, SIRIO_CREDENTIALS  the run's own
#   SIRIO_HOST_BIN  a missing file: no host is found
#   NOTIFY_LOG    the notify-send stub's log
#   RAIL_STUB_DIR, RAIL_SIRIOCTL  the codex stub's go-files and its sirioctl
#   TRACE_STEP_MS the pace of the chat fixture's scripted lines
#   SIRIO_RAIL_E2E_RUN  marks every process of this run
#   DISPLAY, GPUI_X11_SCALE_FACTOR, VK_*  the window and software Vulkan (framed runs)
APP_ENV=(
  "PATH=$APP_PATH" "HOME=$RUN_DIR/home" "SHELL=/bin/bash" "TERM=xterm-256color" "LANG=C.UTF-8"
  "XDG_CONFIG_HOME=$RUN_DIR/xdg/config" "XDG_DATA_HOME=$RUN_DIR/xdg/data"
  "XDG_STATE_HOME=$RUN_DIR/xdg/state" "XDG_CACHE_HOME=$RUN_DIR/xdg/cache" "XDG_RUNTIME_DIR=$RUN_DIR/xdg/runtime"
  "BROWSER=$RUN_DIR/bin/browser-stub"
  "SIRIO_SOCKET=$SIRIO_SOCKET" "SIRIO_DB=$SIRIO_DB" "SIRIO_CREDENTIALS=$RUN_DIR/credentials.json"
  "SIRIO_HOST_BIN=$RUN_DIR/no-sirio-host" "NOTIFY_LOG=$NOTIFY_LOG"
  "RAIL_STUB_DIR=$STUB_DIR" "RAIL_SIRIOCTL=$CTL" "TRACE_STEP_MS=${TRACE_STEP_MS:-400}"
  "SIRIO_RAIL_E2E_RUN=$RUN_DIR"
)
if [ "$STATE_ONLY" -eq 0 ]; then
  APP_ENV+=("DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1" "${VULKAN_ENV[@]}")
fi

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
app_window() {
  local window="" _
  for _ in $(seq 1 50); do
    window=$(find_window)
    [ -n "$window" ] && break
    sleep 0.2
  done
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID"
  echo "$window"
}
# Waits for a line in the app's log.
wait_app_log() { # text
  for _ in $(seq 1 50); do
    grep -Fq "$1" "$RUN_DIR/app.log" 2>/dev/null && return 0
    sleep 0.2
  done
  return 1
}
launch_app() {
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -i "${APP_ENV[@]}" "$APP_BIN" >>"$RUN_DIR/app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 75); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.2
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
  if [ "$STATE_ONLY" -eq 0 ]; then
    local window
    window=$(app_window)
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowmove "$window" 0 0 windowsize "$window" "$WINDOW_W" "$WINDOW_H" \
      || fail "could not size the window to ${WINDOW_W}x${WINDOW_H}"
    # No window manager gives the window the input focus, and a desktop
    # notification is held back only while the app is active (step 10).
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowfocus "$window" || fail "could not focus the window"
    sleep 1
  fi
}
# True while the app is alive. It is our child, so a zombie has exited.
app_running() {
  local state
  state=$(ps -o stat= -p "$APP_PID" 2>/dev/null | tr -d ' ') || true
  [ -n "$state" ] && [ "${state#Z}" = "$state" ]
}
quit_app() {
  ctl quit >/dev/null || true
  for _ in $(seq 1 75); do # up to 15 s
    app_running || break
    sleep 0.2
  done
  if app_running; then
    kill -9 "$APP_PID" 2>/dev/null || true
    fail "sirio did not quit"
  fi
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}
relaunch() {
  quit_app
  launch_app
  ctl select-workspace --workspace "$WT" >/dev/null
  RAIL wait "the layout answers after the relaunch" 30 '"primaryCollapsed" in L' >/dev/null || fail "the layout never answered after the relaunch"
}

# ---- real X input at the pixels the app reports ---------------------------
# The screen pixel of the window's top-left corner: the socket reports window pixels.
window_origin() { # prints "X Y"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$(app_window)" | awk '/Absolute upper-left X/ {x=$NF} /Absolute upper-left Y/ {y=$NF} END {print x, y}'
}
# A press and a release 0.3 s apart, as a hand's are: this window paints in
# software, and an instant click can outrun a handler armed by the press.
click_window() { # "x y" (window pixels)
  local origin x y
  origin=$(window_origin)
  x=$(( ${1% *} + ${origin% *} ))
  y=$(( ${1#* } + ${origin#* } ))
  echo "  click: window pixel $1 (screen $x $y)"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove "$x" "$y" mousedown 1 sleep 0.3 mouseup 1 \
    || fail "xdotool could not click at $x,$y"
  sleep 1
}
# A click at the centre of railRects[KEY] (or of the rail icon of POSITION).
click_rect() { # key [position]
  local xy
  xy=$(RAIL center "$@") || fail "railRects has no ${1}${2:+ $2} rect to click"
  click_window "$xy"
}
press_key() { # key
  echo "  key: $1"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool key --clearmodifiers "$1" || fail "xdotool could not press $1"
  sleep 1
}

# ---- captures --------------------------------------------------------------
capture() { # name
  sleep 2
  local window colours
  window=$(app_window)
  timeout -k 5 20 import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png" || fail "the capture of $1 failed"
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours; pid $APP_PID)"
}
# How many pixels of the page's magenta the screen shows inside the app's window.
# The screen, not the window, is read: the webview may be a window of its own.
page_pixels() { # name
  local geometry file="$OUT_DIR/frames/$1.screen.png"
  geometry=$(DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$(app_window)" \
    | awk '/Absolute upper-left X/ {x=$NF} /Absolute upper-left Y/ {y=$NF} /Width:/ {w=$NF} /Height:/ {h=$NF} END {print w "x" h "+" x "+" y}')
  timeout -k 5 20 import -display "$DISPLAY_TARGET" -window root -crop "$geometry" +repage "$file" || fail "the screen capture $1 failed"
  "$IM" "$file" -alpha off -fuzz 3% -fill black +opaque "$PAGE_COLOUR" -fill white -opaque "$PAGE_COLOUR" \
    -colorspace gray -format '%[fx:round(mean*w*h)]' info:
}

# ---- the stub terminal's go-files ------------------------------------------
stub_go() { # pane what
  touch "$STUB_DIR/go.$1.$2"
}
wait_file() { # path seconds
  local _
  for _ in $(seq 1 $(( $2 * 5 ))); do
    [ -s "$1" ] && return 0
    sleep 0.2
  done
  return 1
}
# Opens an agent terminal on the codex stub and waits for the stub to report its pane.
open_stub_terminal() {
  local pane
  rm -f "$STUB_DIR/codex.pane"
  pane=$(reply surface terminal open --agent codex | python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["pane"])') \
    || fail "the codex terminal did not open"
  wait_file "$STUB_DIR/codex.pane" 30 || fail "the codex stub never started in $pane"
  [ "$(cat "$STUB_DIR/codex.pane")" = "$pane" ] || fail "the codex stub runs in $(cat "$STUB_DIR/codex.pane"), not the new tab's $pane"
  echo "$pane"
}
# The stub's `stty size` columns, asked afresh.
stub_columns() { # pane
  rm -f "$STUB_DIR/size.$1"
  stub_go "$1" size
  wait_file "$STUB_DIR/size.$1" 15 || fail "the codex stub in $1 never reported its size"
  cut -d' ' -f2 "$STUB_DIR/size.$1"
}
# Runs `sirioctl notify --status STATUS` inside the stub's pane, and waits for the
# rail icon of POSITION to show it.
stub_status() { # pane position status
  stub_go "$1" "status-$3"
  wait_layout "the rail shows tab $2 as $3" "status($2) == '$3'"
}
notify_lines() { wc -l <"$NOTIFY_LOG" | tr -d ' '; }

# ---- the chat ---------------------------------------------------------------
SURFACE=""
send_prompt() { # text
  wait_chat "the chat is ready for a prompt" 'c["status"] in ("idle", "completed")'
  RAIL rpc surface.chat.send surfaceId="$SURFACE" text="$1" >/dev/null || fail "sending '$1' failed"
  echo "  sent '$1'"
}

# =============================================================================
launch_app
ctl project add "$WT" >/dev/null
ctl select-workspace --workspace "$WT" >/dev/null
wait_app_log "[sirio] host unavailable: sirio-host not found" \
  || fail "the app did not report that it found no sirio-host beside its copy"
echo "OK: the app found no sirio-host beside its copy"
# A fresh worktree may start with tabs of its own: close them, so every tab is this run's.
sleep 2
for position in $(seq "$(tab_count)" -1 1); do
  ctl surface tabs close "$position" --confirm >/dev/null || fail "could not close the starting tab $position"
done
if [ "$STATE_ONLY" -eq 0 ]; then
  TPANE=$(open_stub_terminal)
  TPOS=$(tab_count)
  echo "OK: the codex stub runs in $TPANE, tab $TPOS"
  # The native chat: the Open list opens it in the Secondary, Ctrl+Shift+M moves it.
  wait_layout "the Open list offers Claude" '"claude" in L["openListAgents"].split(",")' 60
  ctl surface open-list open --entry agent:claude >/dev/null || fail "the Open list did not open a Claude chat"
  wait_layout "the chat opened in the Secondary" 'tabs()["kinds"].count("chat") == 1 and len(secondary()) == 1'
  # Ctrl+Shift+M moves the focused half's active tab: select the chat first.
  ctl surface tabs select "$(RAIL positions chat)" >/dev/null
  press_key ctrl+shift+m
  wait_layout "Ctrl+Shift+M moved the chat into the Primary" \
    'L["secondaryTabs"] == "0" and all(tabs()["kinds"][p - 1] != "chat" for p in secondary())'
  CPOS=$(RAIL positions chat)
  SURFACE=$(RAIL chat-surface) || fail "no chat surface"
  wait_chat "the chat is native and connected" 'c["status"] in ("idle", "completed")' 60
  echo "OK: the chat $SURFACE is tab $CPOS in the Primary"
fi

step 1 "with no Secondary tab, no collapse button"
wait_layout "the layout reports the rail's keys, not collapsed, no peek, no Secondary tab" \
  'L["primaryCollapsed"] == "false" and L["peekTab"] == "" and L["secondaryTabs"] == "0" and "railTabs" in L and "railRects" in L'
if [ "$STATE_ONLY" -eq 0 ]; then
  wait_layout "a painted strip has its + and no collapse button" \
    'num("primaryWidth") > 0 and num("secondaryWidth") == 0 and R["new"] is not None and R["collapse"] is None'
  capture no-secondary
fi

if [ "$STATE_ONLY" -eq 1 ]; then
  SKIPS=(
    "2|clicks the collapse button"
    "3|needs the collapsed rail (a click) and clicks rail icons to peek"
    "4|clicks a rail icon and presses a real Escape"
    "5|clicks a rail icon and presses a real Escape"
    "6|clicks outside the peek and on a rail icon"
    "7|clicks the rail's + and its New Terminal row"
    "8|clicks the peek's pin"
    "9|clicks the collapse button"
    "10|clicks the collapse button"
    "11|takes window captures"
    "12|clicks a rail icon to peek"
    "13|clicks a rail icon to peek"
    "14|clicks a rail icon to peek"
    "15|presses a real Ctrl+Shift+B and clicks the collapse button"
  )
  for skip in "${SKIPS[@]}"; do
    echo "SKIP: step ${skip%%|*}: --state-only, and this step ${skip#*|}"
  done
  STEP="the last check"
  quit_app
  leftover=$(run_processes)
  [ -z "$leftover" ] || fail "processes of this run survived the quit: $leftover"
  echo "OK: no process of this run is left"
  echo "PRIMARY RAIL E2E OK"
  exit 0
fi

step 2 "a file in the Secondary; collapse makes the Primary a rail"
ctl surface editor open --path "$WT/README.md" >/dev/null || fail "the file did not open"
wait_layout "the file opened in the Secondary" 'L["secondaryVisible"] == "true" and L["secondaryKinds"] == "file"'
FILEPOS=$(layout_get 'L["secondaryPositions"]')
# The terminal's columns in the half, for step 4: drawn there, at the split's width.
ctl surface tabs select "$TPOS" >/dev/null
sleep 2
EXP_COLS=$(stub_columns "$TPANE")
[ "$(stub_columns "$TPANE")" = "$EXP_COLS" ] || fail "the terminal's columns in the half did not settle"
echo "OK: the terminal has $EXP_COLS columns in the half"
wait_layout "both halves are drawn and the collapse button with them" \
  'num("primaryWidth") > 0 and num("secondaryWidth") > 0 and R["collapse"] is not None'
P0=$(layout_get 'num("primaryWidth")')
S0=$(layout_get 'num("secondaryWidth")')
echo "  before: primaryWidth=$P0 secondaryWidth=$S0"
click_rect collapse
wait_layout "the Primary is collapsed" 'L["primaryCollapsed"] == "true" and L["peekTab"] == ""'
wait_layout "the Secondary grew by the old Primary width minus the 40 px rail (±2)" \
  "abs((num('secondaryWidth') - $S0) - ($P0 - 40)) <= 2"
wait_layout "the rail lists the Primary tabs in strip order (the list's order)" \
  '[p for p, _ in rail] == primary() and sorted(R["tabs"]) == sorted(str(p) for p in primary())'
wait_layout "the rail draws its expand button and the strip's collapse button is gone" \
  'R["expand"] is not None and R["collapse"] is None'
capture collapsed

step 3 "the anti-goal: nothing stops while collapsed"
# Everything below happens after the collapse, with no peek open.
stub_status "$TPANE" "$TPOS" running
stub_go "$TPANE" output
wait_file "$STUB_DIR/output.$TPANE" 30 || fail "the terminal never finished its output while collapsed"
stub_status "$TPANE" "$TPOS" needs-input
send_prompt subagents
wait_layout "the rail shows the chat running" "status($CPOS) == 'running'"
wait_chat "the turn waits on a permission while collapsed" 'len(pending()) == 1' 60
wait_layout "nothing was peeked while they ran" 'L["peekTab"] == "" and L["primaryCollapsed"] == "true"'
permission=$(RAIL chat-find "$SURFACE" "the pending permission" 10 'pending()[0]') || fail "no pending permission"
RAIL rpc surface.chat.permission surfaceId="$SURFACE" requestId="$permission" optionId=allow_once >/dev/null \
  || fail "answering the permission failed"
wait_chat "the turn ran to its end while collapsed" 'c["status"] == "completed"' 60
wait_layout "the rail shows the chat done" "status($CPOS) == 'done'"
click_rect tabs "$TPOS"
wait_layout "the terminal is peeked" "peek() == $TPOS"
screen=$(reply panel read --id "$TPANE") || fail "panel read of $TPANE failed"
for n in 01 02 03 04 05 06 07 08 09 10 11 12; do
  grep -Fq "rail-line-$n" <<<"$screen" || fail "the peeked terminal lacks rail-line-$n, written while collapsed: $screen"
done
grep -Fq "rail-output-end" <<<"$screen" || fail "the peeked terminal lacks the end of its output"
echo "OK: the peeked terminal holds every line written while collapsed"
click_rect tabs "$CPOS"
wait_layout "the peek switched to the chat" "peek() == $CPOS"
wait_chat "the peeked chat holds every call of the turn, the permission answered" \
  'all(t in tool_ids() for t in ("toolu_agent_a", "toolu_agent_b", "toolu_edit", "toolu_bash_fail", "toolu_bash_ok")) and not pending()'

step 4 "a peeked terminal keeps its columns, and Esc reaches its program"
click_rect tabs "$TPOS"
wait_layout "the terminal is peeked" "peek() == $TPOS"
sleep 1
cols=""
for _ in $(seq 1 10); do
  cols=$(stub_columns "$TPANE")
  [ $(( cols > EXP_COLS ? cols - EXP_COLS : EXP_COLS - cols )) -le 1 ] && break
  sleep 0.5
done
[ $(( cols > EXP_COLS ? cols - EXP_COLS : EXP_COLS - cols )) -le 1 ] \
  || fail "the peeked terminal has $cols columns, the half had $EXP_COLS"
echo "OK: the peeked terminal has $cols columns, the half had $EXP_COLS"
rm -f "$STUB_DIR/esc-ready.$TPANE" "$STUB_DIR/esc.$TPANE"
stub_go "$TPANE" esc
wait_file "$STUB_DIR/esc-ready.$TPANE" 15 || fail "the terminal never started reading a key"
press_key Escape
wait_file "$STUB_DIR/esc.$TPANE" 10 || fail "Esc never reached the peeked terminal's program"
[ "$(cat "$STUB_DIR/esc.$TPANE")" = "\$'\\E'" ] || fail "the program read $(cat "$STUB_DIR/esc.$TPANE"), not Esc"
wait_layout "the peek is still open on the terminal" "peek() == $TPOS"

step 5 "Esc in a peeked chat cancels its running turn"
send_prompt cancel
wait_chat "the turn is running" 'c["status"] == "streaming"'
wait_layout "the rail shows the chat running" "status($CPOS) == 'running'"
click_rect tabs "$CPOS"
wait_layout "the chat is peeked" "peek() == $CPOS"
sleep 2
RAIL chat-wait "$SURFACE" "the turn runs on until it is cancelled" 1 'c["status"] == "streaming"' \
  || fail "the cancel turn ended by itself"
press_key Escape
wait_chat "Esc cancelled the turn" 'c["status"] != "streaming"' 15
wait_layout "the peek is still open on the chat, no longer running" "peek() == $CPOS and status($CPOS) != 'running'"

step 6 "a click outside closes the peek; one icon twice opens and closes"
xy=$(RAIL outside) || fail "no point on the Secondary right of the peek"
click_window "$xy"
wait_layout "the click outside closed the peek" 'L["peekTab"] == ""'
[ "$(tab_active)" = "$FILEPOS" ] || fail "after the peek closed the active tab is $(tab_active), not the Secondary's file $FILEPOS"
echo "OK: the Secondary's file is the active tab again"
click_rect tabs "$TPOS"
wait_layout "the first click opened the peek" "peek() == $TPOS"
click_rect tabs "$TPOS"
wait_layout "the second click on the same icon closed it" 'L["peekTab"] == ""'

step 7 "the rail's + -> New Terminal: a new Primary tab, peeked"
before=$(tab_count)
click_rect new
xy=$(RAIL center menuTerminal) \
  || fail "railRects has no menuTerminal rect: the New menu's New Terminal row reports no rect (missing from the contract)"
click_window "$xy"
NPOS=$((before + 1))
wait_layout "a new Primary tab is peeked" \
  "tabs()['count'] == $NPOS and $NPOS not in secondary() and $NPOS in [p for p, _ in rail] and peek() == $NPOS"
[ "$(RAIL positions terminal)" = "$TPOS $NPOS" ] || fail "tab $NPOS is not a terminal"

step 8 "the pin expands the half with that tab active"
click_rect pin
wait_layout "the half is expanded and no peek is open" 'L["primaryCollapsed"] == "false" and L["peekTab"] == ""'
[ "$(tab_active)" = "$NPOS" ] || fail "the active tab is $(tab_active), not the pinned $NPOS"
echo "OK: the pinned tab $NPOS is active"

step 9 "collapsed survives a relaunch; the last Secondary tab's close expands for good"
click_rect collapse
wait_layout "the Primary is collapsed" 'L["primaryCollapsed"] == "true"'
relaunch
wait_layout "still collapsed after the relaunch" 'L["primaryCollapsed"] == "true" and L["secondaryVisible"] == "true"'
wait_layout "the Secondary holds only the file" 'L["secondaryKinds"] == "file"'
ctl surface tabs close "$(layout_get 'L["secondaryPositions"]')" >/dev/null
wait_layout "closing the last Secondary tab expanded the Primary" \
  'L["secondaryTabs"] == "0" and L["primaryCollapsed"] == "false"'
relaunch
wait_layout "still expanded after the relaunch" 'L["secondaryTabs"] == "0" and L["primaryCollapsed"] == "false"'
# With no Secondary there is no rail to draw, whatever the flag: a file shows whether it was cleared.
ctl surface editor open --path "$WT/README.md" >/dev/null || fail "the file did not open"
wait_layout "a new file shows both halves, not a rail: the flag was cleared" \
  'L["secondaryVisible"] == "true" and L["primaryCollapsed"] == "false" and R["collapse"] is not None'

step 10 "a tab selected while collapsed is peeked, and its status change posts nothing"
# Only the chat and a fresh stub terminal stay in the Primary.
CPOS=$(RAIL positions chat)
[ -n "$CPOS" ] && [ "${CPOS#* }" = "$CPOS" ] || fail "the chat did not come back as one tab after the relaunches: '$CPOS'"
for position in $(RAIL get "sorted((p for p in primary() if p != $CPOS), reverse=True)" | tr -d '[],'); do
  ctl surface tabs close "$position" --confirm >/dev/null
done
CPOS=$(RAIL positions chat)
T2PANE=$(open_stub_terminal)
T2POS=$(tab_count)
echo "OK: the codex stub runs in $T2PANE, tab $T2POS"
FILEPOS=$(layout_get 'L["secondaryPositions"]')
ctl surface tabs select "$FILEPOS" >/dev/null
click_rect collapse
wait_layout "the Primary is collapsed, nothing peeked" 'L["primaryCollapsed"] == "true" and L["peekTab"] == ""'
# The control: the tab collapsed and not peeked is off screen, so its change posts a line.
stub_status "$T2PANE" "$T2POS" running
lines=$(notify_lines)
stub_status "$T2PANE" "$T2POS" needs-input
for _ in $(seq 1 50); do
  [ "$(notify_lines)" -gt "$lines" ] && break
  sleep 0.2
done
[ "$(notify_lines)" -gt "$lines" ] \
  || fail "needs-input on the collapsed, unpeeked tab posted no notification: the check below would prove nothing (notify.log: $(cat "$NOTIFY_LOG"))"
echo "OK: off screen, needs-input posted: $(tail -n 1 "$NOTIFY_LOG")"
# The sidebar pill reports no rect, so the tab is selected the way the socket does it
# (select_tab, the hook the pill's activate_tab also ends in).
ctl surface tabs select "$T2POS" >/dev/null
wait_layout "selecting the collapsed tab peeked it" "peek() == $T2POS"
lines=$(notify_lines)
stub_status "$T2PANE" "$T2POS" running
stub_status "$T2PANE" "$T2POS" done
sleep 3
[ "$(notify_lines)" = "$lines" ] \
  || fail "done on the peeked tab posted a notification (is the window active? should_notify posts whenever it is not): $(tail -n 1 "$NOTIFY_LOG")"
echo "OK: the peeked tab's change to done posted nothing"

step 11 "captures: the rail, a peeked chat, a peeked terminal, a peek over a Browser tab"
click_rect tabs "$T2POS"
wait_layout "the peek is closed" 'L["peekTab"] == ""'
capture rail
click_rect tabs "$CPOS"
wait_layout "the chat is peeked" "peek() == $CPOS"
capture peeked-chat
click_rect tabs "$T2POS"
wait_layout "the terminal is peeked" "peek() == $T2POS"
capture peeked-terminal
click_rect tabs "$T2POS"
wait_layout "the peek is closed" 'L["peekTab"] == ""'
PAGE_PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
python3 -m http.server --bind 127.0.0.1 --directory "$RUN_DIR/www" "$PAGE_PORT" >>"$RUN_DIR/page.log" 2>&1 &
PAGE_PID=$!
ctl browser open "http://127.0.0.1:$PAGE_PORT/index.html" >/dev/null || fail "the Browser tab did not open"
wait_layout "the Browser tab is in the Secondary" '"browser" in L["secondaryKinds"].split(",")'
shown=0
for _ in $(seq 1 20); do
  shown=$(page_pixels browser-before)
  [ "$shown" -ge 20000 ] && break
  sleep 1
done
[ "$shown" -ge 20000 ] || fail "the Browser tab never painted its page ($shown magenta pixels)"
echo "OK: the Browser tab paints its page ($shown magenta pixels)"
click_rect tabs "$CPOS"
wait_layout "the chat is peeked over the Browser tab" "peek() == $CPOS"
sleep 1.5
shown=$(page_pixels browser-peeked)
[ "$shown" -eq 0 ] || fail "the page shows while the peek is open ($shown magenta pixels)"
capture peeked-chat-over-browser
echo "OK: the Browser tab is blank while the peek is open"
click_rect tabs "$CPOS"
wait_layout "the peek is closed" 'L["peekTab"] == ""'
for _ in $(seq 1 10); do
  shown=$(page_pixels browser-after)
  [ "$shown" -ge 20000 ] && break
  sleep 1
done
[ "$shown" -ge 20000 ] || fail "the page did not come back after the peek closed ($shown magenta pixels)"
capture browser-after-peek
echo "OK: the page is back after the peek closed ($shown magenta pixels)"
ctl surface tabs close "$(RAIL positions browser)" >/dev/null
kill "$PAGE_PID" 2>/dev/null || true
wait "$PAGE_PID" 2>/dev/null || true
PAGE_PID=""
wait_layout "the Browser tab is closed" 'L["secondaryKinds"] == "file"'

step 12 "closing the peeked tab over the socket closes the peek"
click_rect tabs "$T2POS"
wait_layout "the terminal is peeked" "peek() == $T2POS"
ctl surface tabs close "$T2POS" --confirm >/dev/null
wait_layout "the peek closed with its tab, the rail stays" \
  "L['peekTab'] == '' and L['primaryCollapsed'] == 'true' and $T2POS not in [p for p, _ in rail]"
ctl ping >/dev/null || fail "the app no longer answers"

step 13 "a worktree switch and back with a peek open"
CPOS=$(RAIL positions chat)
click_rect tabs "$CPOS"
wait_layout "the chat is peeked" "peek() == $CPOS"
ctl select-workspace --workspace "$OTHER" >/dev/null
wait_layout "the other worktree shows no peek" 'L["peekTab"] == ""'
ctl select-workspace --workspace "$WT" >/dev/null
wait_layout "back: still collapsed, no peek" \
  "L['primaryCollapsed'] == 'true' and L['peekTab'] == '' and $CPOS in [p for p, _ in rail]"
sleep 2
wait_layout "the restore reopened no peek" 'L["peekTab"] == ""' 1

step 14 "closing the last Primary tab from inside the peek"
LAST=$(layout_get 'rail[0][0]')
for position in $(RAIL get "sorted((p for p, _ in rail if p != $LAST), reverse=True)" | tr -d '[],'); do
  ctl surface tabs close "$position" --confirm >/dev/null
done
LAST=$(layout_get 'rail[0][0]')
wait_layout "one Primary tab is left" "[p for p, _ in rail] == [$LAST]"
click_rect tabs "$LAST"
wait_layout "the last Primary tab is peeked" "peek() == $LAST"
[ "$(tab_active)" = "$LAST" ] || fail "the peeked tab $LAST is not the active tab ($(tab_active))"
# `pane close` closes the focused pane of the active tab: the one in the peek.
ctl pane close >/dev/null
wait_layout "the peek closed and the rail is empty but for its expand button and +" \
  'L["peekTab"] == "" and rail == [] and L["primaryCollapsed"] == "true" and R["expand"] is not None and R["new"] is not None'
capture empty-rail

step 15 "Ctrl+Shift+B while collapsed hides the Secondary and expands the Primary"
press_key ctrl+shift+b
wait_layout "the Secondary is hidden and the Primary expanded" \
  'L["secondaryHidden"] == "true" and L["primaryCollapsed"] == "false" and num("secondaryWidth") == 0'
relaunch
wait_layout "still hidden and expanded after the relaunch" 'L["secondaryHidden"] == "true" and L["primaryCollapsed"] == "false"'
# A hidden Secondary draws no rail whatever the flag: showing it shows whether it was cleared.
press_key ctrl+shift+b
wait_layout "the Secondary is back beside an expanded Primary: the flag was cleared" \
  'L["secondaryVisible"] == "true" and L["primaryCollapsed"] == "false" and R["collapse"] is not None'

STEP="the last check"
echo "the last check: quitting leaves no process of this run behind"
quit_app
leftover=""
for _ in $(seq 1 25); do
  leftover=$(run_processes)
  [ -z "$leftover" ] && break
  sleep 0.2
done
[ -z "$leftover" ] || fail "processes of this run survived the quit: $(echo "$leftover" | tr '\n' ' ')"
echo "OK: no process of this run is left"

echo "PRIMARY RAIL E2E OK"
