#!/bin/bash
set -eEuo pipefail

# End-to-end test of the sidebar in Codex's shell (spec
# docs/superpowers/specs/2026-11-17-sidebar-codex-shell-design.md §3, §6): a
# real, isolated debug Sirio on a real git repository under a private Xvfb,
# driven with real X clicks, wheel turns and keys at the pixels the app reports
# (`surface layout read`'s shellRects) and read back over the control socket
# (`surface attention read`'s `sidebar` object and `projects`). Each step
# asserts, writes its rows to measurements.tsv and captures frames/. It proves,
# in order:
#    1  one repo with two worktrees, added and selected: the title, bell, search,
#       New Chat row and section header are drawn; the project row is 31 px, the
#       card 53, the section header 28, the New Chat row 31
#    2  a click on the project row folds it (its cards leave the list), another
#       unfolds it; the row and the section header are captured with the pointer on
#       them and off, and the crops of their right ends are asserted to change.
#       That the project row's + and three dots and the header's expand all /
#       collapse all show only under the pointer is judged by eye in frames/
#       (a pixel difference cannot tell the buttons from the row's hover wash)
#    3  the title menu switches to Sessions (the Projects rows are not drawn) and
#       back, by real clicks on the menu's rows
#    4  the pointer on a card: after 500 ms the hover card is open beside the
#       sidebar (its left edge at least left.x + left.w + 4: 4 px past the sidebar's
#       visible right edge, border included), the hover card and the open state
#       agree, the star's rect lies inside the card. The main worktree is the
#       Primary (the served `primary` flag is read): a hover shows its filled star
#       only, a plain card its outline star and its cross (judged by eye in
#       frames/). A click on the Primary's star unsets it (the flag goes true then
#       false, and the same card's star crop differs); a second click restores it
#    5  one session made unread (a codex stub reports running then done while the
#       other worktree is selected): the bell reads "unread"; a click on the bell
#       opens its menu; a click on the entry selects the session's tab and the bell
#       reads nothing
#    6  a click on Search shows the filter field (filterShown), Esc hides it
#    7  a click on the centre pane (listFocused false), then on a card
#       (listFocused true), then on the centre pane again (false); the focus ring
#       on and off captured
#    8  a second project with 22 worktrees: the list is captured at rest, after a
#       wheel turn with the pointer off the list (idle frame, bar up) and 2.2 s
#       later (bar faded). The list body is the same in the last two and the bar's
#       strip differs, so the check fails when no bar is drawn; the body differs
#       from the rest frame (the list overflowed and moved)
# Every step ends with its captures; the run ends with
# SIDEBAR SHELL E2E OK (<appearance>).
#
# The hover card and the bell's reading are polled together with the socket:
# `hoverCardRow` (the hovered worktree's path) can be set a frame before the card
# is drawn, so a wait asks for both to agree. Closed parts read null, not absent.
#
# Two routes are not a hand's: the unread session comes from a codex stub (a
# shell running a script named codex that runs sirioctl notify --status, the
# command every agent hook runs), because no production path registers a spawned
# agent; and the unread session is left by selecting the other worktree over the
# socket (select-workspace). Everything else is a real X click or key.
#
# Isolation: the app starts under env -i with an allowlist (launch_app), from a
# copy of its binary with no sirio-host beside it and SIRIO_HOST_BIN naming a
# missing file. PATH is the notify-send stub, the run's bin (the codex stub),
# then /usr/bin:/bin; HOME, XDG directories, the database, the socket and the
# credentials are the run's own. Every process the app starts carries
# SIRIO_SIDEBAR_E2E_RUN=<run dir>; the last check quits the app and fails if any
# of them is still alive.
#
# --state-only runs with no display: nothing paints, so no rect exists and no
# click can be sent. It checks the control socket answers with the sidebar read
# and ends with SIDEBAR SHELL E2E OK (state only). A framed run needs --display
# :N, a private Xvfb of at least 1600x1000 that this user started (never
# $DISPLAY), Mesa's lavapipe (lvp_icd*.json), import, identify, xwininfo, xprop,
# xdotool and ImageMagick's magick or convert.
#
# The artifact: --out-dir DIR (must be empty; default
# artifacts/sidebar-shell-e2e-<stamp>-<pid>) keeps transcript.log, app.log,
# measurements.tsv (case, key, expected, drawn, ok), reads/ (the decoded rects
# and sidebar state of every successful read), attention-*.json and PID-matched
# window captures in frames/, named <what>-<appearance>.png. Run once with
# --appearance light and once with --appearance dark.
#
# Usage: Scripts/Tests/test-sidebar-shell-e2e.sh --display :N [--state-only]
#          [--appearance light|dark] [--out-dir DIR]
#   Xvfb :93 -screen 0 1720x1000x24 &   # a private display for a framed run

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
  [ -n "$IM" ] || refuse "ImageMagick's magick or convert is required to compare the captures"
  [ -n "$DISPLAY_TARGET" ] || refuse "no --display: the clicks need a display of their own; pass --display :N (a private Xvfb), or --state-only"
  target_number=$(display_number "$DISPLAY_TARGET")
  [ -n "$target_number" ] || refuse "--display takes a local X display such as :93, not '$DISPLAY_TARGET'"
  [ "$target_number" != "$(display_number "${DISPLAY:-}")" ] \
    || refuse "--display $DISPLAY_TARGET is this session's own \$DISPLAY: start a private display (Xvfb :$target_number -screen 0 1720x1000x24) and pass it"
  ps -u "$(id -u)" -o args= | python3 -c 'import re, sys; n = sys.argv[1]; sys.exit(0 if any(re.search(r"(^|/)Xvfb :" + n + r"( |$)", line) for line in sys.stdin) else 1)' "$target_number" \
    || refuse "no Xvfb :$target_number is running as $(id -un): start a private one (Xvfb :$target_number -screen 0 1720x1000x24 &)"
  DISPLAY_TARGET=":$target_number"
  geometry=$(DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool getdisplaygeometry 2>/dev/null || true)
  [ -n "$geometry" ] || refuse "no X server answers on $DISPLAY_TARGET"
  { [ "${geometry% *}" -ge "$WINDOW_W" ] && [ "${geometry#* }" -ge "$WINDOW_H" ]; } \
    || refuse "the display is ${geometry% *}x${geometry#* }, smaller than the ${WINDOW_W}x${WINDOW_H} window: start a larger Xvfb"
fi

[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/sidebar-shell-e2e-$(date +%Y%m%d-%H%M%S)-$$"
if [ -d "$OUT_DIR" ] && [ -n "$(ls -A "$OUT_DIR")" ]; then
  refuse "$OUT_DIR already has content (pass a new --out-dir)"
fi
mkdir -p "$OUT_DIR/frames" "$OUT_DIR/reads"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

STEP=setup
fail() { echo "FAIL: $STEP: $*" >&2; exit 1; }
trap 'echo "FAIL: $STEP: \`$BASH_COMMAND\` failed (exit $?)" >&2' ERR
step() { # n text
  STEP="step $1"
  echo "step $1: $2"
}
[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux"
command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required"
echo "display: ${DISPLAY_TARGET:-none (--state-only)}; appearance: ${APPEARANCE:-default}"
TAG="${APPEARANCE:-default}"

# The seed repositories read no git configuration of the user's.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

# ---- the run's own directory and what must be stopped ---------------------
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-sidebar-XXXXXX")
APP_PID=""
run_processes() {
  python3 - "$RUN_DIR" <<'PY'
import os, sys
marker = b"SIRIO_SIDEBAR_E2E_RUN=" + sys.argv[1].encode() + b"\0"
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
  stop_run_processes
  cp "$RUN_DIR"/*.log "$RUN_DIR"/stub/stub.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf --one-file-system "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl) \
  2>>"$RUN_DIR/build.log" || { tail -n 40 "$RUN_DIR/build.log" >&2; fail "the build failed"; }
dirty=''
git -C "$ROOT" diff --quiet HEAD -- || dirty=' (with uncommitted changes)'
echo "commit: $(git -C "$ROOT" rev-parse HEAD)$dirty"
echo "binary: $BIN sha256 $(sha256sum "$BIN" | cut -d' ' -f1)"

# A copy of the app with no sibling sirio-host.
mkdir -p "$RUN_DIR/alone"
cp "$BIN" "$RUN_DIR/alone/sirio"
APP_BIN="$RUN_DIR/alone/sirio"

# ---- the repository: a main worktree and a second one ---------------------
WT="$RUN_DIR/repo"
OTHER="$RUN_DIR/other"
mkdir -p "$WT"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
printf '# app\n\nThe file the sidebar E2E opens.\n' >"$WT/README.md"
git -C "$WT" add -A
git -C "$WT" commit -q -m first
git -C "$WT" worktree add -q -b other "$OTHER"
# The second project of step 8: a repository with 22 worktrees, so the list overflows.
BIG="$RUN_DIR/big"
mkdir -p "$BIG"
git -C "$BIG" init -q -b main
git -C "$BIG" config user.email tester@example.invalid
git -C "$BIG" config user.name Tester
printf '# big\n' >"$BIG/README.md"
git -C "$BIG" add -A
git -C "$BIG" commit -q -m first
for n in $(seq 1 22); do
  git -C "$BIG" worktree add -q -b "feature-$n" "$RUN_DIR/big-wt-$n"
done

# ---- stubs ---------------------------------------------------------------
STUB_DIR="$RUN_DIR/stub"
NOTIFY_LOG="$RUN_DIR/notify.log"
mkdir -p "$RUN_DIR/bin" "$STUB_DIR"
: >"$NOTIFY_LOG"
# The status-changing terminal's agent: it waits for go-files and runs
# sirioctl notify --status inside its pane, as an agent's hooks do.
cat >"$RUN_DIR/bin/codex" <<'STUB_EOF'
#!/bin/bash
case "${1:-}" in --version|-V|version) echo "codex-cli 9.9.9 (sidebar e2e stub)"; exit 0 ;; esac
dir=${SIDEBAR_STUB_DIR:?SIDEBAR_STUB_DIR must name the stub directory}
pane=${SIRIO_PANE_ID:?codex must run inside a Sirio pane}
printf '%s\n' "$pane" >"$dir/codex.pane.$$" && mv -f "$dir/codex.pane.$$" "$dir/codex.pane"
echo "sidebar stub in $pane"
while :; do
  for gate in "$dir/go.$pane".*; do
    [ -e "$gate" ] || continue
    what=${gate##*.}
    rm -f "$gate"
    case "$what" in
      status-*)
        status=0
        "$SIDEBAR_SIRIOCTL" notify --session "$pane" --status "${what#status-}" || status=$?
        echo "$pane ${what#status-} exit=$status" >>"$dir/stub.log" ;;
    esac
  done
  sleep 0.2
done
STUB_EOF
chmod +x "$RUN_DIR/bin/codex"
for name in xdg-open browser-stub; do
  printf '#!/bin/sh\nprintf "%%s\\n" "%s $*" >>"%s/browser.log"\n' "$name" "$RUN_DIR" >"$RUN_DIR/bin/$name"
  chmod +x "$RUN_DIR/bin/$name"
done
APP_PATH="$ROOT/Scripts/Tests/stubs/notify-send:$RUN_DIR/bin:/usr/bin:/bin"
[ "$(PATH="$APP_PATH" command -v codex || true)" = "$RUN_DIR/bin/codex" ] || fail "codex does not resolve to this run's stub under the app's PATH"
echo "OK: codex resolves to this run's stub"

# ---- calling the app -----------------------------------------------------
export SIRIO_SOCKET="$RUN_DIR/c.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
[ "${#SIRIO_SOCKET}" -lt 108 ] || fail "the control socket path is too long for a unix socket: $SIRIO_SOCKET (point TMPDIR at a shorter directory)"
CALL_TIMEOUT=30
ctl() {
  local status=0
  echo "+ sirioctl $*"
  timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" || status=$?
  echo "  -> sirioctl exit $status"
  return "$status"
}
reply() { timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" --json 2>/dev/null; }
tsv_row() { # case key expected drawn ok
  printf '%s\t%s\t%s\t%s\t%s\n' "$@" >>"$OUT_DIR/measurements.tsv"
  printf '  %-4s  %s: expected %s, drawn %s\n' "$5" "$2" "$3" "$4"
}
printf 'case\tkey\texpected\tdrawn\tok\n' >"$OUT_DIR/measurements.tsv"

# The reads: the control socket's line-delimited JSON, spoken directly.
cat >"$RUN_DIR/sb.py" <<'PY'
import json, os, socket, sys, time
from types import SimpleNamespace

TSV = os.environ["SB_TSV"]
READS = os.environ["SB_READS"]
LAST = {}


def rpc(method, params=None):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(os.environ["SIRIO_SOCKET"])
    s.sendall((json.dumps({"id": "sb", "method": method, "params": params or {}}) + "\n").encode())
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


def read():
    """One read of the layout (shellRects, decoded twice) and of the attention read's sidebar
    object and projects. A key the app does not report raises: not yet, never a match."""
    L = rpc("surface.layout.read")
    raw = json.loads(L["shellRects"])
    A = rpc("surface.attention.read")
    S = json.loads(A["sidebar"])
    P = json.loads(A["projects"])
    # The served `primary` flag of every worktree, by the last component of its path.
    W = {}
    for project in json.loads(rpc("project.list")["projects"]):
        for wt in json.loads(project["worktrees"]):
            W[os.path.basename(wt["path"].rstrip("/"))] = wt["primary"] == "true"
    LAST["state"] = {"rects": raw, "sidebar": S, "projects": P, "primary": W}
    rects = {k: (SimpleNamespace(x=v[0], y=v[1], w=v[2], h=v[3]) if v is not None else None) for k, v in raw.items()}
    return {"R": rects, "S": S, "P": P, "L": L, "A": A, "PRIM": W}


def snapshot():
    return json.dumps(LAST["state"], sort_keys=True)


def save_read(case):
    name, n = case, 1
    while os.path.exists(os.path.join(READS, name + ".json")):
        n += 1
        name = f"{case}-{n}"
    with open(os.path.join(READS, name + ".json"), "w") as f:
        json.dump(LAST["state"], f, indent=1, sort_keys=True)


def row(case, key, expected, drawn, ok):
    with open(TSV, "a") as f:
        f.write("\t".join([case, key, expected, drawn, ok]) + "\n")
    print(f"  {ok:4}  {key}: expected {expected}, drawn {drawn}")


def fmt(v):
    return f"{v:g}" if isinstance(v, (int, float)) and not isinstance(v, bool) else str(v)


def evaluate(expr, ns):
    # The expressions are this script's own (never app or user text), over the app's reads.
    return eval(expr, {"__builtins__": {}}, dict(ns, min=min, max=max, round=round, abs=abs, len=len, any=any, all=all))


def settled(case, seconds=40):
    """Reads until two reads 0.4 s apart agree and the title bar is drawn (the window paints)."""
    deadline = time.time() + seconds
    previous = None
    while True:
        try:
            ns = read()
            snap = snapshot()
            if ns["R"].get("titlebar") is not None and snap == previous:
                return ns
            previous = snap if ns["R"].get("titlebar") is not None else None
        except Exception as e:
            LAST["error"] = repr(e)
        if time.time() > deadline:
            row(case, "settled (drawn, stable)", "two equal reads", f"never; error {LAST.get('error')}", "FAIL")
            print(f"  last read: {json.dumps(LAST.get('state'))}", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.4)


command = sys.argv[1]
if command == "measure":
    # measure CASE SPEC...: settle, then one row per SPEC "label|expr|op|expected".
    # op: == (exact), >= and <= (expected is an expression too). Exits 1 at the first FAIL.
    case = sys.argv[2]
    ns = settled(case)
    save_read(case)
    for spec in sys.argv[3:]:
        label, expr, op, expected = spec.split("|")
        drawn = evaluate(expr, ns)
        want = evaluate(expected, ns)
        if op == "==":
            ok, shown = drawn == want, fmt(want)
        elif op == ">=":
            ok, shown = drawn >= want, f">= {fmt(want)}"
        elif op == "<=":
            ok, shown = drawn <= want, f"<= {fmt(want)}"
        else:
            sys.exit(f"unknown op {op}")
        row(case, label, shown, fmt(drawn), "ok" if ok else "FAIL")
        if not ok:
            print(f"  last read: {json.dumps(LAST['state'])}", file=sys.stderr)
            sys.exit(1)
elif command == "wait":
    # wait CASE SECONDS LABEL EXPR: polls until EXPR is true over two reads 0.4 s apart that agree.
    case, seconds, label, expr = sys.argv[2], float(sys.argv[3]), sys.argv[4], sys.argv[5]
    deadline = time.time() + seconds
    previous = None
    value = None
    while time.time() < deadline:
        try:
            ns = read()
            value = evaluate(expr, ns)
            snap = snapshot()
            if value is True and snap == previous:
                save_read(case)
                row(case, label, expr, "true", "ok")
                sys.exit(0)
            previous = snap if value is True else None
        except Exception as e:
            LAST["error"] = repr(e)
            previous = None
        time.sleep(0.4)
    row(case, label, expr, f"{value} within {seconds:g} s; error {LAST.get('error')}", "FAIL")
    print(f"  last read: {json.dumps(LAST.get('state'))}", file=sys.stderr)
    try:
        print(f"  sessions: {rpc('surface.attention.read')['sessions']}", file=sys.stderr)
    except Exception as e:
        print(f"  sessions: unreadable ({e!r})", file=sys.stderr)
    sys.exit(1)
elif command == "get":
    ns = settled("get")
    print(fmt(evaluate(sys.argv[2], ns)))
elif command == "center":
    # center KEY: the window pixel at the centre of the rect, once drawn.
    deadline = time.time() + 20
    while True:
        try:
            r = read()["R"].get(sys.argv[2])
            if r is not None:
                print(f"{round(r.x + r.w / 2)} {round(r.y + r.h / 2)}")
                break
        except Exception as e:
            LAST["error"] = repr(e)
        if time.time() > deadline:
            sys.exit(f"{sys.argv[2]} was not drawn within 20 s ({LAST.get('error')})")
        time.sleep(0.3)
elif command == "tabs":
    t = rpc("surface.tabs.read")
    print(t["count"], t["active"], t[f"tab.{t['active']}"].split("|", 2)[2])
elif command == "attn-save":
    A = rpc("surface.attention.read")
    json.dump({"sidebar": json.loads(A["sidebar"]), "projects": json.loads(A["projects"]),
               "sessions": json.loads(A["sessions"])}, open(sys.argv[2], "w"), indent=2)
else:
    sys.exit(f"unknown command {command}")
PY
export SB_TSV="$OUT_DIR/measurements.tsv" SB_READS="$OUT_DIR/reads"
SB() { python3 -I "$RUN_DIR/sb.py" "$@"; }
# Polls until EXPR holds. EXPR sees R (rects of the shell, an object with x y w h, or None),
# S (the sidebar's state object), P (the projects), PRIM (the served primary flag of each
# worktree, by the last component of its path) and L (the layout read).
wait_for() { # case label expr [seconds]
  SB wait "$1" "${4:-30}" "$2" "$3" || fail "$2"
}
measure() { # case spec...
  SB measure "$@" || fail "measurement of $1 failed"
}
get() { SB get "$1" || fail "could not read $1"; }

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
# env -i and this allowlist, so nothing of the user's reaches the app.
APP_ENV=(
  "PATH=$APP_PATH" "HOME=$RUN_DIR/home" "SHELL=/bin/bash" "TERM=xterm-256color" "LANG=C.UTF-8"
  "XDG_CONFIG_HOME=$RUN_DIR/xdg/config" "XDG_DATA_HOME=$RUN_DIR/xdg/data"
  "XDG_STATE_HOME=$RUN_DIR/xdg/state" "XDG_CACHE_HOME=$RUN_DIR/xdg/cache" "XDG_RUNTIME_DIR=$RUN_DIR/xdg/runtime"
  "BROWSER=$RUN_DIR/bin/browser-stub"
  "SIRIO_SOCKET=$SIRIO_SOCKET" "SIRIO_DB=$SIRIO_DB" "SIRIO_CREDENTIALS=$RUN_DIR/credentials.json"
  "SIRIO_HOST_BIN=$RUN_DIR/no-sirio-host" "NOTIFY_LOG=$NOTIFY_LOG"
  "SIDEBAR_STUB_DIR=$STUB_DIR" "SIDEBAR_SIRIOCTL=$CTL"
  "SIRIO_SIDEBAR_E2E_RUN=$RUN_DIR"
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
    local _
    for _ in $(seq 1 50); do
      DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$window" | grep -q 'Map State: IsViewable' && break
      sleep 0.2
    done
    DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool windowfocus --sync "$window" || fail "could not focus the window"
    sleep 1
  fi
}
app_running() {
  local state
  state=$(ps -o stat= -p "$APP_PID" 2>/dev/null | tr -d ' ') || true
  [ -n "$state" ] && [ "${state#Z}" = "$state" ]
}
quit_app() {
  ctl quit >/dev/null || true
  for _ in $(seq 1 75); do
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

# ---- real X input at the pixels the app reports ---------------------------
window_origin() { # prints "X Y"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$(app_window)" | awk '/Absolute upper-left X/ {x=$NF} /Absolute upper-left Y/ {y=$NF} END {print x, y}'
}
screen_xy() { # "x y" (window pixels) -> "X Y" (screen)
  local origin
  origin=$(window_origin)
  echo "$(( ${1% *} + ${origin% *} )) $(( ${1#* } + ${origin#* } ))"
}
# A press and a release 0.3 s apart, as a hand's are: this window paints in software.
click_window() { # "x y" (window pixels)
  local s
  s=$(screen_xy "$1")
  echo "  click: window pixel $1 (screen $s)"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove "${s% *}" "${s#* }" mousedown 1 sleep 0.3 mouseup 1 \
    || fail "xdotool could not click at $s"
  sleep 1
}
# The pointer to a window pixel, in two moves (a hover needs a motion after the first).
move_window() { # "x y"
  local s x y
  s=$(screen_xy "$1")
  x=${s% *}
  y=${s#* }
  echo "  pointer: window pixel $1 (screen $s)"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove "$((x - 2))" "$((y - 2))" sleep 0.15 mousemove "$x" "$y" \
    || fail "xdotool could not move to $s"
}
click_rect() { # key [dx dy]: the centre of the rect, moved by dx dy
  local xy
  xy=$(SB center "$1") || fail "the app drew no $1 rect to click"
  click_window "$(( ${xy% *} + ${2:-0} )) $(( ${xy#* } + ${3:-0} ))"
}
move_rect() { # key [dx dy]
  local xy
  xy=$(SB center "$1") || fail "the app drew no $1 rect to point at"
  move_window "$(( ${xy% *} + ${2:-0} )) $(( ${xy#* } + ${3:-0} ))"
}
# Parks the pointer on the centre pane, off every sidebar part, and waits for the hover to clear.
park_pointer() {
  move_rect center
  sleep 0.6
}
press_key() { # key
  echo "  key: $1"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool key --clearmodifiers "$1" || fail "xdotool could not press $1"
  sleep 1
}

# ---- captures --------------------------------------------------------------
capture_now() { # name: no wait, for a state that fades
  local window colours name="$1-$TAG"
  window=$(app_window)
  timeout -k 5 20 import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$name.png" || fail "the capture of $name failed"
  colours=$(identify -format '%k' "$OUT_DIR/frames/$name.png")
  [ "$colours" -ge 200 ] || fail "$name is blank ($colours colours)"
  echo "FRAME: $name ($colours colours; pid $APP_PID)"
}
capture() { # name
  sleep 2
  capture_now "$1"
}
# The mean absolute difference, in 1e-5 units of the full scale, of one crop ("WxH+X+Y") of two
# frames of this run.
frame_diff() { # a b crop
  "$IM" \( "$OUT_DIR/frames/$1-$TAG.png" -crop "$3" +repage \) \( "$OUT_DIR/frames/$2-$TAG.png" -crop "$3" +repage \) \
    -compose difference -composite -format '%[fx:round(mean*100000)]' info: || fail "could not compare $1 and $2"
}
# A row of measurements.tsv that two frames differ in a crop: the opacity-only parts are
# judged by their frames (and by these numbers), which no unit test can read.
expect_frames_differ() { # case key a b crop
  local diff
  diff=$(frame_diff "$3" "$4" "$5")
  if [ "$diff" -gt 0 ]; then
    tsv_row "$1" "$2" "$3 differs from $4 in $5" "$diff" ok
  else
    tsv_row "$1" "$2" "$3 differs from $4 in $5" "$diff" FAIL
    fail "$2: the frames $3 and $4 are identical in $5"
  fi
}
rect_crop() { # "x y w h" of a rect with its own size, grown by GROW px on each side
  python3 -c 'import sys
x, y, w, h, g = (float(v) for v in sys.argv[1:6])
print("%dx%d+%d+%d" % (round(w) + 2 * g, round(h) + 2 * g, max(0, round(x) - g), max(0, round(y) - g)))' "$1" "${2:-0}"
}
rect_of() { # key: "x y w h" window pixels of a rect
  get "(R['$1'].x, R['$1'].y, R['$1'].w, R['$1'].h)" | tr -d '(),'
}

# ---- the codex stub's go-files ---------------------------------------------
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
stub_log_lines() { if [ -f "$STUB_DIR/stub.log" ]; then wc -l <"$STUB_DIR/stub.log" | tr -d ' '; else echo 0; fi; }
stub_status() { # pane status
  local before _
  before=$(stub_log_lines)
  stub_go "$1" "status-$2"
  for _ in $(seq 1 75); do
    [ "$(stub_log_lines)" -gt "$before" ] && break
    sleep 0.2
  done
  [ "$(stub_log_lines)" -gt "$before" ] || fail "the codex stub in $1 never ran sirioctl notify --status $2"
  [ "$(tail -n 1 "$STUB_DIR/stub.log")" = "$1 $2 exit=0" ] \
    || fail "sirioctl notify --status $2 in $1 did not succeed: $(tail -n 1 "$STUB_DIR/stub.log")"
}

# =============================================================================
launch_app
ctl project add "$WT" >/dev/null
ctl select-workspace --workspace "$WT" >/dev/null

if [ "$STATE_ONLY" -eq 1 ]; then
  step 1 "state only: the control socket answers with the sidebar's read"
  # Nothing paints, so no rect exists: the read must still carry the sidebar object.
  SB wait state-only 30 "the attention read reports the sidebar's state" \
    'S["bell"] is None and S["filterShown"] is False and S["hoverCardRow"] is None and S["listFocused"] in (True, False)' \
    || fail "the sidebar's state is not reported"
  echo "SKIP: steps 2-8 need a window to paint and click"
  quit_app
  echo "SIDEBAR SHELL E2E OK (state only)"
  exit 0
fi

# ---- step 1 ----------------------------------------------------------------
step 1 "the shell's parts are drawn at their heights"
measure 1 \
  "title is drawn|R['sidebarTitle'] is not None|==|True" \
  "bell is drawn|R['sidebarBell'] is not None|==|True" \
  "search is drawn|R['sidebarSearch'] is not None|==|True" \
  "New Chat row is drawn|R['sidebarNewChat'] is not None|==|True" \
  "section header is drawn|R['sidebarSectionHeader'] is not None|==|True" \
  "section header height|R['sidebarSectionHeader'].h|==|28" \
  "New Chat row height|R['sidebarNewChat'].h|==|31" \
  "project row height|R['sidebarSection'].h|==|31" \
  "card height|R['sidebarCard'].h|==|53" \
  "closed parts read null: hover card|R['sidebarHoverCard'] is None|==|True" \
  "closed parts read null: filter|R['sidebarFilter'] is None|==|True" \
  "closed parts read null: bell menu|R['sidebarBellMenu'] is None|==|True" \
  "the bell is quiet|S['bell'] is None|==|True" \
  "the project is open|P[0]['expanded']|==|True"
park_pointer
capture shell

# ---- step 2 ----------------------------------------------------------------
step 2 "the project row folds and unfolds; the row and the header change under the pointer"
move_rect sidebarSection
sleep 0.5
capture project-row-hover
park_pointer
capture project-row-idle
move_rect sidebarSectionHeader
sleep 0.5
capture section-header-hover
park_pointer
capture section-header-idle
# The row's right end is where its + and its three dots show, and the header's right end holds
# add project (always) and expand all / collapse all (under the pointer). A pixel difference
# cannot tell those buttons from the hover wash the row gets, so the rows below only say that
# the crop changes under the pointer; that the buttons show is judged by eye in frames/
# project-row-hover-* and section-header-hover-*.
ROW=$(rect_of sidebarSection)
HEAD=$(rect_of sidebarSectionHeader)
expect_frames_differ 2 "the project row changes under the pointer (its + and dots are judged by eye in frames/)" project-row-hover project-row-idle \
  "$(python3 -c 'import sys; x, y, w, h = (float(v) for v in sys.argv[1:5]); print("%dx%d+%d+%d" % (64, round(h), round(x + w - 64), round(y)))' $ROW)"
expect_frames_differ 2 "the section header changes under the pointer (its expand all / collapse all are judged by eye in frames/)" section-header-hover section-header-idle \
  "$(python3 -c 'import sys; x, y, w, h = (float(v) for v in sys.argv[1:5]); print("%dx%d+%d+%d" % (90, round(h), round(x + w - 90), round(y)))' $HEAD)"
click_rect sidebarSection
wait_for 2 "the click folded the project" "P[0]['expanded'] is False and R['sidebarCard'] is None"
park_pointer
capture project-folded
click_rect sidebarSection
wait_for 2 "the second click unfolded it" "P[0]['expanded'] is True and R['sidebarCard'] is not None"
park_pointer
capture project-unfolded

# ---- step 3 ----------------------------------------------------------------
step 3 "the title menu switches to Sessions and back"
# The menu hangs 6 px under the title, inside a 4 px padding; a row is TITLE_ROW_H tall.
TITLE_ROW_H=${TITLE_ROW_H:-30}
TITLE=$(rect_of sidebarTitle)
TITLE_BOTTOM=$(python3 -c 'import sys; x, y, w, h = (float(v) for v in sys.argv[1:5]); print(round(y + h))' $TITLE)
click_rect sidebarTitle
sleep 0.6
capture title-menu
echo "  title menu row offsets: Projects at +$((6 + 4 + TITLE_ROW_H / 2)), Sessions at +$((6 + 4 + TITLE_ROW_H + TITLE_ROW_H / 2)) under the title"
TITLE_X=$(python3 -c 'import sys; x, y, w, h = (float(v) for v in sys.argv[1:5]); print(round(x + 60))' $TITLE)
click_window "$TITLE_X $((TITLE_BOTTOM + 6 + 4 + TITLE_ROW_H + TITLE_ROW_H / 2))"
wait_for 3 "the Sessions view does not draw the Projects rows" "R['sidebarSection'] is None and R['sidebarCard'] is None and R['sidebarSectionHeader'] is not None"
park_pointer
capture sessions-view
click_rect sidebarTitle
sleep 0.6
click_window "$TITLE_X $((TITLE_BOTTOM + 6 + 4 + TITLE_ROW_H / 2))"
wait_for 3 "choosing Projects draws the project row and the card again" "R['sidebarSection'] is not None and R['sidebarCard'] is not None"
park_pointer

# ---- step 4 ----------------------------------------------------------------
step 4 "the hover card opens beside the sidebar; a Primary shows a filled star, a plain card its star and cross"
move_rect sidebarCard 0 0
sleep 0.8
wait_for 4 "the hover card is open and the state names its row, both agreeing" \
  "S['hoverCardRow'] is not None and R['sidebarHoverCard'] is not None"
measure 4 \
  "hover card is beside the sidebar: 4 px past its visible right edge|R['sidebarHoverCard'].x|>=|R['left'].x + R['left'].w + 4" \
  "the star's rect lies inside the card (that it is drawn is judged by eye in frames/)|R['sidebarCardStar'] is not None and R['sidebarCardStar'].x >= R['sidebarCard'].x and R['sidebarCardStar'].x + R['sidebarCardStar'].w <= R['sidebarCard'].x + R['sidebarCard'].w and R['sidebarCardStar'].y >= R['sidebarCard'].y and R['sidebarCardStar'].y + R['sidebarCardStar'].h <= R['sidebarCard'].y + R['sidebarCard'].h|==|True" \
  "the hover card stays inside the window|R['sidebarHoverCard'].y + R['sidebarHoverCard'].h|<=|$WINDOW_H"
# The first card is the repository's main worktree, which Sirio makes the Primary: its star is
# the filled one and it has no cross. The card under it ("other") is a plain worktree.
capture card-hover-primary-star
STAR=$(rect_of sidebarCardStar)
CARD=$(rect_of sidebarCard)
# A crop of 24x24 px about the star's centre.
STAR_CROP=$(python3 -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
print("%dx%d+%d+%d" % (24, 24, round(x + w / 2) - 12, round(y + h / 2) - 12))' $STAR)
# Under the pointer, the card under it shows its star (outline) and its cross where the bloom was.
move_window "$(python3 -c 'import sys; print(round(float(sys.argv[1]) + 40), round(float(sys.argv[2]) + float(sys.argv[4]) + 26))' $CARD)"
sleep 0.8
capture card-hover-star-cross
# A click on the Primary's star unsets it (the same action as the menu's Unset Primary): the served
# flag goes true then false, and the same card under the same pointer shows the outline star.
wait_for 4 "the main worktree is the Primary" "PRIM['repo'] is True and PRIM['other'] is False"
park_pointer
move_rect sidebarCardStar
click_rect sidebarCardStar
wait_for 4 "the click on the Primary's star unset it" "PRIM['repo'] is False and PRIM['other'] is False"
park_pointer
move_rect sidebarCard 0 0
sleep 0.8
capture card-hover-unset-primary-star
expect_frames_differ 4 "the Primary's filled star is not the outline star of the same card once unset" \
  card-hover-primary-star card-hover-unset-primary-star "$STAR_CROP"
# The run leaves the Primary as it found it.
click_rect sidebarCardStar
wait_for 4 "a second click on the star made it the Primary again" "PRIM['repo'] is True and PRIM['other'] is False"
park_pointer
wait_for 4 "the hover card closes when the pointer leaves" "S['hoverCardRow'] is None and R['sidebarHoverCard'] is None"

# ---- step 5 ----------------------------------------------------------------
step 5 "a session left unread lights the bell; the bell's menu selects it"
rm -f "$STUB_DIR/codex.pane"
pane=$(reply surface terminal open --agent codex | python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["pane"])') \
  || fail "the codex terminal did not open"
wait_file "$STUB_DIR/codex.pane" 30 || fail "the codex stub never started in $pane"
echo "OK: the codex stub runs in $pane (its tab is titled Codex)"
ctl select-workspace --workspace "$OTHER" >/dev/null
wait_for 5 "the other worktree is shown: the stub's tab is off screen" "S['bell'] is None"
stub_status "$pane" running
stub_status "$pane" done
wait_for 5 "the bell reads unread" "S['bell'] == 'unread'" 30
SB attn-save "$OUT_DIR/attention-unread.json" || fail "could not save the attention read"
park_pointer
capture bell-unread
click_rect sidebarBell
wait_for 5 "the bell's menu is open" "R['sidebarBellMenu'] is not None"
capture bell-menu
BELL=$(rect_of sidebarBellMenu)
click_window "$(python3 -c 'import sys; print(round(float(sys.argv[1]) + float(sys.argv[3]) / 2), round(float(sys.argv[2]) + 24))' $BELL)"
wait_for 5 "the bell's menu closed and the bell is quiet" "R['sidebarBellMenu'] is None and S['bell'] is None"
tabs_now=""
for _ in $(seq 1 25); do
  tabs_now=$(SB tabs)
  [ "${tabs_now#* * }" = "Codex" ] && break
  sleep 0.4
done
[ "${tabs_now#* * }" = "Codex" ] || fail "choosing the entry did not select the session's tab: the active tab reads '$tabs_now', not the Codex tab; $(reply surface tabs read | tr '\n' ' ')"
echo "OK: the session's tab is the active tab ($tabs_now)"
park_pointer
capture bell-chosen

# ---- step 6 ----------------------------------------------------------------
step 6 "Search shows the filter field; Esc hides it"
click_rect sidebarSearch
wait_for 6 "the filter is shown and drawn" "S['filterShown'] is True and R['sidebarFilter'] is not None"
capture filter-shown
press_key Escape
wait_for 6 "Esc hid the filter" "S['filterShown'] is False and R['sidebarFilter'] is None"
capture filter-hidden

# ---- step 7 ----------------------------------------------------------------
step 7 "a click on a card focuses the list; a click on the centre pane takes the focus away"
CENTER_CLICK=$(python3 -c 'import sys; print(round(float(sys.argv[1]) + float(sys.argv[3]) / 2), round(float(sys.argv[2]) + float(sys.argv[4]) / 2))' $(rect_of center))
# The precondition: the centre pane holds the focus, so the card click below is what moves it.
click_window "$CENTER_CLICK"
wait_for 7 "the centre pane holds the focus before the card is clicked" "S['listFocused'] is False"
move_rect sidebarSection
capture focus-ring-off
click_rect sidebarCard 0 0
wait_for 7 "the card click gave the list the keyboard focus" "S['listFocused'] is True"
move_rect sidebarSection
capture focus-ring-on
click_window "$CENTER_CLICK"
wait_for 7 "a click on the centre pane took the focus away again" "S['listFocused'] is False"

# ---- step 8 ----------------------------------------------------------------
step 8 "the list's scrollbar is up after the list moves and faded once it idles"
ctl project add "$BIG" >/dev/null
wait_for 8 "the second project is listed" "len(P) == 2" 30
# 25 cards of 53 px do not fit in a 1000 px window. The scrollbar is judged on idle frames only,
# with the pointer off the list (no hover wash in the crops): one taken right after the wheel
# stopped, one 2.2 s later (the bar holds 900 ms, then fades over 300 ms). The list body must be
# the same in both (it had stopped moving), so the strip's difference is the bar and nothing else;
# and it must differ from a frame taken before the wheel (the list overflowed and moved).
LEFT=$(rect_of left)
BODY=$(python3 -c 'import sys; x, y, w, h = (float(v) for v in sys.argv[1:5]); print("%dx%d+%d+%d" % (200, round(h) - 160, round(x + 20), round(y + 120)))' $LEFT)
STRIP=$(python3 -c 'import sys; x, y, w, h = (float(v) for v in sys.argv[1:5]); print("%dx%d+%d+%d" % (14, round(h) - 160, round(x + w - 16), round(y + 120)))' $LEFT)
CENTER_XY=$(python3 -c 'import sys; print(round(float(sys.argv[1]) + float(sys.argv[3]) / 2), round(float(sys.argv[2]) + float(sys.argv[4]) / 2))' $(rect_of center))
park_pointer
capture scroll-rest
bar_seen=0
for attempt in 1 2 3; do
  move_rect sidebarCard 0 40
  for _ in 1 2 3 4 5; do
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool click 5 || fail "xdotool could not turn the wheel"
  done
  move_window "$CENTER_XY"
  capture_now scrollbar-visible
  sleep 2.2
  capture_now scrollbar-faded
  body_diff=$(frame_diff scrollbar-visible scrollbar-faded "$BODY")
  strip_diff=$(frame_diff scrollbar-visible scrollbar-faded "$STRIP")
  echo "  attempt $attempt: list body differs by $body_diff, bar strip by $strip_diff"
  if [ "$body_diff" -eq 0 ] && [ "$strip_diff" -gt 0 ]; then
    bar_seen=1
    break
  fi
done
if [ "$bar_seen" -eq 1 ]; then
  tsv_row 8 "the list body is the same in the visible and the faded frame (it had stopped)" "0" "$body_diff" ok
  tsv_row 8 "the bar strip differs between the visible and the faded frame (the bar is drawn, then gone)" "> 0" "$strip_diff" ok
else
  tsv_row 8 "the bar strip differs between the visible and the faded frame, the body not" "body 0, strip > 0" "body $body_diff, strip $strip_diff" FAIL
  fail "no attempt showed a scrollbar on an idle list: body $body_diff, strip $strip_diff"
fi
expect_frames_differ 8 "the wheel moved the list (it overflows)" scroll-rest scrollbar-faded "$BODY"
SB attn-save "$OUT_DIR/attention-final.json" || fail "could not save the attention read"

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
echo "measurements: $OUT_DIR/measurements.tsv"
if ! awk -F'\t' 'NR > 1 && $5 != "ok" {bad = 1} END {exit bad}' "$OUT_DIR/measurements.tsv"; then
  fail "measurements.tsv holds a row that is not ok"
fi

echo "SIDEBAR SHELL E2E OK (${APPEARANCE:-default})"
