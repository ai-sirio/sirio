#!/bin/bash
set -eEuo pipefail

# End-to-end test of the Zed layout parity (spec
# docs/superpowers/specs/2026-11-08-zed-layout-parity-design.md §2, §3, §6.1): a
# real, isolated debug Sirio on a real git repository under Xvfb, measured over
# the control socket from the rects the app drew (`surface layout read`'s
# `shellRects`, debug builds), and driven with real X input for the divider. It
# proves seven cases, in order; every assertion is a row of measurements.tsv
# and the run stops at the first wrong row, after printing it:
#    1  a fresh database: at 1470x833 and at 1710x987 the sidebar is 300 and the
#       right panel 240, the two centre halves 456.5 / 576.5 wide, the title
#       bar 34, the tab strips 32, the status bar 30, the right header 32, a
#       sidebar section header 31 and a two-line card 53 high, the right
#       header's last icon and the Secondary's close button inside their
#       parents; at 1100x800 the halves stay at least 320 wide and the columns
#       do not overlap. Spec §9's `secondaryInsideCenter` is report-only
#       (`ok` is `warn`, never a failure).
#    2  migration: a stored 280 / 320 at schema 26 draws 300 / 240 and is
#       stored as such, at schema 27
#    3  a stored 350 / 400 at schema 26 is kept, drawn and stored
#    4  a stored 280 at schema 27 (a deliberate drag after the migration) is kept
#    5  an unparseable width falls back to 300
#    6  the divider: a real drag moves the halves, the ratio is saved, and a
#       relaunch draws the same halves (the case that fails when the drag does
#       not schedule the save)
#    7  light theme, interface font 18: the exact heights at 18 (the card is 58),
#       the light background really drawn (and the dark one in dark-1470), every
#       key of shellRects drawn
#    7b interface font back to 13, light, at 1710x987: the case 1 heights and
#       widths again
#
# The Secondary pane is opened with `surface open-list open --entry changes`
# after every launch that does not restore one. The sidebar's and the right
# panel's probes fill on their first paint, so every read polls until the keys
# it asserts are drawn, the layout reflects the window's size, and two reads
# 0.4 s apart agree. No fixed sleep stands in for a paint: the only fixed waits
# are the 1.5 s the divider's debounced (500 ms) save is given (counted from
# the real mouseup), the 2 s before each capture and the three 0.3 s pauses of
# the divider's X drag.
#
# Isolation: the app starts under `env -i` with an allowlist (launch_app), from
# a copy of its binary with no sirio-host beside it, SIRIO_HOST_BIN naming a
# missing file. PATH is the notify-send stub, the chat shim, then /usr/bin:/bin;
# HOME, XDG directories, the database, the socket and the credentials are the
# run's own. Every process the app starts carries SIRIO_ZED_E2E_RUN=<run dir>.
#
# Needs --display :N, a private Xvfb of at least 1720x1000 that this user
# started (never $DISPLAY), Mesa's lavapipe (lvp_icd*.json), import, identify,
# xwininfo, xprop, xdotool and ImageMagick's magick or convert.
#
# The artifact: --out-dir DIR (default artifacts/zed-layout-e2e-<stamp>-<pid>)
# keeps transcript.log (it starts with the commit and the binary's sha256 and
# logs every sirioctl call with its exit status), app.log, measurements.tsv
# (case, key, expected, drawn, ok), reads/ (the decoded shellRects of every
# successful assertion read: <case>.json, then <case>-2.json, -3 ...) and PID-matched window captures in frames/.
#
# Usage: Scripts/Tests/test-zed-layout-e2e.sh --display :N [--out-dir DIR]
#   Xvfb :97 -screen 0 1720x1000x24 &   # a private display

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
MIN_W=1720
MIN_H=1000
OUT_DIR=""
DISPLAY_TARGET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

refuse() { echo "refused: $*" >&2; exit 2; }
display_number() { # DISPLAY-string
  [[ "$1" =~ ^([A-Za-z0-9._-]*):([0-9]+)(\.[0-9]+)?$ ]] || return 0
  [ -z "${BASH_REMATCH[1]}" ] || [ "${BASH_REMATCH[1]}" = localhost ] || return 0
  echo $((10#${BASH_REMATCH[2]}))
}
for tool in import identify xwininfo xprop xdotool timeout; do
  command -v "$tool" >/dev/null || refuse "$tool is required"
done
IM=$(command -v magick || command -v convert || true)
[ -n "$IM" ] || refuse "ImageMagick's magick or convert is required"
[ -n "$DISPLAY_TARGET" ] || refuse "no --display: pass --display :N (a private Xvfb of at least ${MIN_W}x${MIN_H})"
target_number=$(display_number "$DISPLAY_TARGET")
[ -n "$target_number" ] || refuse "--display takes a local X display such as :97, not '$DISPLAY_TARGET'"
[ "$target_number" != "$(display_number "${DISPLAY:-}")" ] \
  || refuse "--display $DISPLAY_TARGET is this session's own \$DISPLAY: start a private display and pass it"
ps -u "$(id -u)" -o args= | python3 -c 'import re, sys; n = sys.argv[1]; sys.exit(0 if any(re.search(r"(^|/)Xvfb :" + n + r"( |$)", line) for line in sys.stdin) else 1)' "$target_number" \
  || refuse "no Xvfb :$target_number is running as $(id -un): start a private one (Xvfb :$target_number -screen 0 ${MIN_W}x${MIN_H}x24 &)"
DISPLAY_TARGET=":$target_number"
geometry=$(DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool getdisplaygeometry 2>/dev/null || true)
[ -n "$geometry" ] || refuse "no X server answers on $DISPLAY_TARGET"
{ [ "${geometry% *}" -ge "$MIN_W" ] && [ "${geometry#* }" -ge "$MIN_H" ]; } \
  || refuse "the display is ${geometry% *}x${geometry#* }, smaller than ${MIN_W}x${MIN_H}: start a larger Xvfb"

[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/zed-layout-e2e-$(date +%Y%m%d-%H%M%S)-$$"
if [ -d "$OUT_DIR" ] && [ -n "$(ls -A "$OUT_DIR")" ]; then
  refuse "$OUT_DIR already has content (pass a new --out-dir)"
fi
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

STEP=setup
fail() { echo "FAIL: $STEP: $*" >&2; exit 1; }
trap 'echo "FAIL: $STEP: \`$BASH_COMMAND\` failed (exit $?)" >&2' ERR
step() { # text
  STEP="$1"
  echo "case $1"
}
[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux"
command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required"
echo "display: $DISPLAY_TARGET"

# ---- the run's own directory and what must be stopped ---------------------
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-zed-XXXXXX")
APP_PID=""
run_processes() {
  python3 -I - "$RUN_DIR" <<'PY'
import os, sys
marker = b"SIRIO_ZED_E2E_RUN=" + sys.argv[1].encode() + b"\0"
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
  cp "$RUN_DIR"/app.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf --one-file-system "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio, sirioctl and the appearance seed"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl \
  && cargo build --quiet -p sirio_persistence --example appearance_seed) \
  2>>"$RUN_DIR/build.log" || { tail -n 40 "$RUN_DIR/build.log" >&2; fail "the build failed"; }
SEED_BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/examples/appearance_seed"
[ -x "$SEED_BIN" ] || fail "the appearance seed was not built at $SEED_BIN"

# A copy of the app with no sibling sirio-host.
mkdir -p "$RUN_DIR/alone"
cp "$BIN" "$RUN_DIR/alone/sirio"
APP_BIN="$RUN_DIR/alone/sirio"

# ---- the repository: a project with a main worktree and a second one -------
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
printf 'changed\n' >>"$WT/README.md" # a change for the Changes tab to list

APP_PATH="$ROOT/Scripts/Tests/stubs/notify-send:$ROOT/Scripts/Tests/stubs/trace:/usr/bin:/bin"

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
echo "commit: $(git -C "$ROOT" rev-parse HEAD)"
echo "binary: $BIN sha256 $(sha256sum "$BIN" | cut -d' ' -f1)"
mkdir -p "$OUT_DIR/reads"
# A row of measurements.tsv written by the script itself.
tsv_row() { # case key expected drawn ok
  printf '%s\t%s\t%s\t%s\t%s\n' "$@" >>"$OUT_DIR/measurements.tsv"
  printf '  %-4s  %s: expected %s, drawn %s\n' "$5" "$2" "$3" "$4"
}

printf 'case\tkey\texpected\tdrawn\tok\n' >"$OUT_DIR/measurements.tsv"
cat >"$RUN_DIR/zl.py" <<'PY'
import json, os, socket, sys, time
from types import SimpleNamespace

TSV = os.environ["ZL_TSV"]
KEYS = ["titlebar", "left", "center", "right", "primary", "secondary", "primaryStrip", "secondaryStrip",
        "secondaryClose", "status", "rightHeader", "rightHeaderLastIcon", "sidebarSection", "sidebarCard",
        "dividerHandle"]
LAST = {}
READS = os.environ["ZL_READS"]


def rpc(method, params=None):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(os.environ["SIRIO_SOCKET"])
    s.sendall((json.dumps({"id": "zl", "method": method, "params": params or {}}) + "\n").encode())
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
    """The layout read and its shellRects, decoded twice (the value is a JSON string)."""
    L = rpc("surface.layout.read")
    LAST["L"] = {k: v for k, v in L.items() if k != "railRects"}
    raw = json.loads(L["shellRects"])
    LAST["rects"] = raw
    rects = {k: (SimpleNamespace(x=v[0], y=v[1], w=v[2], h=v[3]) if v is not None else None) for k, v in raw.items()}
    return L, rects


def fail_row(case, key, expected, drawn):
    line = "\t".join([case, key, expected, drawn, "FAIL"])
    with open(TSV, "a") as f:
        f.write(line + "\n")
    print(f"  FAIL  {key}: expected {expected}, drawn {drawn}", file=sys.stderr)


def save_read(case):
    """The decoded shellRects of a successful read: reads/<case>.json, then -2, -3 ... (the next
    free name on disk, since every call of this script is a new process)."""
    name, n = case, 1
    while os.path.exists(os.path.join(READS, name + ".json")):
        n += 1
        name = f"{case}-{n}"
    with open(os.path.join(READS, name + ".json"), "w") as f:
        json.dump(LAST["rects"], f, indent=1, sort_keys=True)


def settled(case, keys, width):
    """Reads until every key is drawn, the title bar spans the window (the resize happened) and
    two reads 0.4 s apart agree. Exits 1 with the last read when that never happens."""
    deadline = time.time() + float(os.environ.get("ZL_SETTLE_SECONDS", "40"))
    previous = None
    while True:
        try:
            L, rects = read()
            absent = [k for k in KEYS if k not in rects]
            if absent:
                fail_row(case, "shellRects keys", f"each of {len(KEYS)} expected keys present", f"absent {absent}")
                sys.exit(1)
            ok = all(rects.get(k) is not None for k in keys) and rects.get("titlebar") is not None \
                and abs(rects["titlebar"].w - width) <= 1
            snap = json.dumps(LAST["rects"], sort_keys=True)
            if ok and snap == previous:
                save_read(case)
                return rects
            previous = snap if ok else None
        except Exception as e:
            LAST["error"] = repr(e)
        if time.time() > deadline:
            missing = [k for k in keys if (LAST.get("rects") or {}).get(k) is None]
            fail_row(case, "settled (drawn, window width, stable)", f"keys {keys} at width {width:g}",
                     f"undrawn {missing}; error {LAST.get('error')}")
            print(f"  never settled at width {width}; undrawn: {missing}; error: {LAST.get('error')}", file=sys.stderr)
            print(f"  last shellRects: {json.dumps(LAST.get('rects'))}", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.4)


def fmt(v):
    return f"{v:g}" if isinstance(v, (int, float)) else str(v)


def row(case, key, expected, drawn, ok):
    line = "\t".join([case, key, expected, drawn, ok])
    with open(TSV, "a") as f:
        f.write(line + "\n")
    print(f"  {ok:4}  {key}: expected {expected}, drawn {drawn}")


def evaluate(expr, rects):
    ns = {k: v for k, v in rects.items()}
    ns["min"] = min
    ns["max"] = max
    ns["round"] = round
    return eval(expr, {"__builtins__": {}}, ns)


command = sys.argv[1]
if command == "measure":
    # measure CASE WIDTH KEYS SPEC...: settle, then one row per SPEC "label|expr|op|expected".
    # op: == (exact), ~ (within 1), >= and <= (expected is an expression too), warn<= (report only).
    case, width, keys = sys.argv[2], float(sys.argv[3]), [k for k in sys.argv[4].split(",") if k]
    rects = settled(case, KEYS if keys == ["ALL"] else keys, width)
    for spec in sys.argv[5:]:
        label, expr, op, expected = spec.split("|")
        drawn = evaluate(expr, rects)
        want = evaluate(expected, rects)
        if op == "==":
            ok, shown = drawn == want, f"{fmt(want)}"
        elif op == "~":
            ok, shown = abs(drawn - want) <= 1, f"{fmt(want)} +-1"
        elif op == ">=":
            ok, shown = drawn >= want, f">= {fmt(want)}"
        elif op == "<=":
            ok, shown = drawn <= want, f"<= {fmt(want)}"
        elif op == "warn<=":
            ok, shown = drawn <= want, f"<= {fmt(want)}"
            row(case, label, shown, fmt(drawn), "ok" if ok else "warn")
            continue
        else:
            sys.exit(f"unknown op {op}")
        row(case, label, shown, fmt(drawn), "ok" if ok else "FAIL")
        if not ok:
            print(f"  shellRects: {json.dumps(LAST['rects'])}", file=sys.stderr)
            sys.exit(1)
elif command == "get":
    # get CASE WIDTH KEYS EXPR: the value of EXPR over the settled rects.
    keys = [k for k in sys.argv[4].split(",") if k]
    rects = settled(sys.argv[2], KEYS if keys == ["ALL"] else keys, float(sys.argv[3]))
    print(fmt(evaluate(sys.argv[5], rects)))
elif command == "wait":
    # wait CASE SECONDS LABEL EXPR: polls until EXPR (over the drawn rects) is true. A poll
    # that never settles writes a FAIL row and exits 1.
    case, seconds, label, expr = sys.argv[2], float(sys.argv[3]), sys.argv[4], sys.argv[5]
    deadline = time.time() + seconds
    value = None
    while time.time() < deadline:
        try:
            L, rects = read()
            if all(rects.get(k) is not None for k in KEYS):
                value = evaluate(expr, rects)
                if value is True:
                    save_read(case)
                    row(case, label, expr, "true", "ok")
                    sys.exit(0)
        except Exception as e:
            LAST["error"] = repr(e)
        time.sleep(0.2)
    fail_row(case, label, expr, f"{value} within {seconds:g} s; error {LAST.get('error')}")
    sys.exit(1)
elif command == "center":
    # center WIDTH KEY CASE: the window pixel at the centre of the rect.
    rects = settled(sys.argv[4], [sys.argv[3]], float(sys.argv[2]))
    r = rects[sys.argv[3]]
    print(f"{round(r.x + r.w / 2)} {round(r.y + r.h / 2)}")
elif command == "secondary-tabs":
    print(rpc("surface.layout.read").get("secondaryTabs", "0"))
elif command == "answers":
    rpc("surface.layout.read")
else:
    sys.exit(f"unknown command {command}")
PY
export ZL_TSV="$OUT_DIR/measurements.tsv" ZL_READS="$OUT_DIR/reads"
ZL() { python3 -I "$RUN_DIR/zl.py" "$@"; }

# ---- the database, edited while the app is quit ---------------------------
cat >"$RUN_DIR/db.py" <<'PY'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
command = sys.argv[2]
if command == "seed":
    # seed VERSION KEY=VALUE...
    for pair in sys.argv[4:]:
        key, value = pair.split("=", 1)
        db.execute("INSERT INTO setting (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value", (key, value))
    db.execute(f"PRAGMA user_version = {int(sys.argv[3])}")
    db.commit()
elif command == "get":
    row = db.execute("SELECT value FROM setting WHERE key = ?", (sys.argv[3],)).fetchone()
    print(row[0] if row else "<none>")
elif command == "version":
    print(db.execute("PRAGMA user_version").fetchone()[0])
PY
db_seed() { python3 -I "$RUN_DIR/db.py" "$SIRIO_DB" seed "$@"; }
db_get() { python3 -I "$RUN_DIR/db.py" "$SIRIO_DB" get "$1"; }
db_version() { python3 -I "$RUN_DIR/db.py" "$SIRIO_DB" version; }
# A stored-value row: the database is read after the app quit.
expect_db() { # case key expected
  local drawn
  drawn=$(db_get "$2")
  if [ "$drawn" = "$3" ]; then
    printf '%s\t%s\t%s\t%s\tok\n' "$1" "$2" "$3" "$drawn" >>"$OUT_DIR/measurements.tsv"
    echo "  ok    $2 stored: expected $3, read $drawn"
  else
    printf '%s\t%s\t%s\t%s\tFAIL\n' "$1" "$2" "$3" "$drawn" >>"$OUT_DIR/measurements.tsv"
    fail "$2 is stored as $drawn, expected $3"
  fi
}
expect_version() { # case expected
  local drawn
  drawn=$(db_version)
  if [ "$drawn" = "$2" ]; then
    printf '%s\tuser_version\t%s\t%s\tok\n' "$1" "$2" "$drawn" >>"$OUT_DIR/measurements.tsv"
    echo "  ok    user_version: expected $2, read $drawn"
  else
    printf '%s\tuser_version\t%s\t%s\tFAIL\n' "$1" "$2" "$drawn" >>"$OUT_DIR/measurements.tsv"
    fail "user_version is $drawn, expected $2"
  fi
}

# ---- launching an isolated Sirio -----------------------------------------
LVP=$(find /usr/share/vulkan/icd.d -name 'lvp_icd*.json' 2>/dev/null | head -n 1 || true)
[ -n "$LVP" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required on Xvfb"
mkdir -p "$RUN_DIR/home" "$RUN_DIR/xdg/config" "$RUN_DIR/xdg/data" "$RUN_DIR/xdg/state" "$RUN_DIR/xdg/cache"
mkdir -m 700 "$RUN_DIR/xdg/runtime"
# `env -i` and this allowlist, so nothing of the user's reaches the app.
APP_ENV=(
  "PATH=$APP_PATH" "HOME=$RUN_DIR/home" "SHELL=/bin/bash" "TERM=xterm-256color" "LANG=C.UTF-8"
  "XDG_CONFIG_HOME=$RUN_DIR/xdg/config" "XDG_DATA_HOME=$RUN_DIR/xdg/data"
  "XDG_STATE_HOME=$RUN_DIR/xdg/state" "XDG_CACHE_HOME=$RUN_DIR/xdg/cache" "XDG_RUNTIME_DIR=$RUN_DIR/xdg/runtime"
  "SIRIO_SOCKET=$SIRIO_SOCKET" "SIRIO_DB=$SIRIO_DB" "SIRIO_CREDENTIALS=$RUN_DIR/credentials.json"
  "SIRIO_HOST_BIN=$RUN_DIR/no-sirio-host" "SIRIO_ZED_E2E_RUN=$RUN_DIR"
  "DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1" "VK_DRIVER_FILES=$LVP" "VK_ICD_FILENAMES=$LVP"
)

find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 -I - <<'PY'
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
# Moves the window to 0,0 and sizes it; the layout read then follows (measure polls for it).
resize_window() { # w h
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowmove "$(app_window)" 0 0 windowsize "$(app_window)" "$1" "$2" \
    || fail "could not size the window to $1x$2"
}
WIDTH=0
launch_app() { # w h
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -i "${APP_ENV[@]}" "$APP_BIN" >>"$RUN_DIR/app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 75); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.2
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
  local window _
  window=$(app_window)
  for _ in $(seq 1 50); do
    DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$window" | grep -q 'Map State: IsViewable' && break
    sleep 0.2
  done
  resize_window "$1" "$2"
  WIDTH=$1
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool windowfocus --sync "$window" || fail "could not focus the window"
  ZL answers || fail "the layout read never answered"
}
app_running() {
  local state
  state=$(ps -o stat= -p "$APP_PID" 2>/dev/null | tr -d ' ') || true
  [ -n "$state" ] && [ "${state#Z}" = "$state" ]
}
quit_app() {
  ctl quit || true
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
# Opens one Secondary tab (the Changes tab) unless the relaunch restored one.
ensure_secondary() {
  local _ n=0
  for _ in $(seq 1 20); do
    n=$(ZL secondary-tabs || echo 0)
    [ "$n" -ge 1 ] && return 0
    sleep 0.3
  done
  for _ in $(seq 1 30); do
    ctl surface open-list open --entry changes && break
    sleep 1
  done
  for _ in $(seq 1 30); do
    n=$(ZL secondary-tabs || echo 0)
    [ "$n" -ge 1 ] && return 0
    sleep 0.5
  done
  fail "no Secondary tab opened"
}
# A launch with a Secondary tab open, ready to measure.
start() { # w h
  launch_app "$1" "$2"
  ctl select-workspace --workspace "$WT"
  ensure_secondary
}

# ---- real X input and captures ---------------------------------------------
window_origin() { # prints "X Y"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$(app_window)" | awk '/Absolute upper-left X/ {x=$NF} /Absolute upper-left Y/ {y=$NF} END {print x, y}'
}
# A drag from window pixel (X1, Y1) to (X2, Y2): a press, two moves, a release.
drag_window() { # x1 y1 x2 y2
  local origin ox oy
  origin=$(window_origin)
  ox=${origin% *}
  oy=${origin#* }
  echo "  drag: window pixel $1,$2 to $3,$4"
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool mousemove $(($1 + ox)) $(($2 + oy)) mousedown 1 sleep 0.3 \
    mousemove $((($1 + $3) / 2 + ox)) $(($4 + oy)) sleep 0.3 mousemove $(($3 + ox)) $(($4 + oy)) sleep 0.3 mouseup 1 \
    || fail "xdotool could not drag from $1,$2 to $3,$4"
}
capture() { # name
  sleep 2
  local window colours
  window=$(app_window)
  timeout -k 5 20 import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png" || fail "the capture of $1 failed"
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours; pid $APP_PID)"
}

# The mean luminance (0-255) of a 40x40 patch of a capture, in window pixels, as a plain number.
LUMA_THRESHOLD=128 # the midpoint of 0-255: a light surface sits far above it, a dark one far below
luma() { # frame x y: prints the mean; fails on a missing frame, a patch outside it or a non-number
  local frame="$OUT_DIR/frames/$1.png" size mean
  size=$(identify -format '%w %h' "$frame") || return 1
  [ -n "$size" ] && [ $(($2 + 40)) -le "${size% *}" ] && [ $(($3 + 40)) -le "${size#* }" ] && [ "$2" -ge 0 ] && [ "$3" -ge 0 ] \
    || { echo "the 40x40 patch at $2,$3 is outside the ${size:-unknown} frame $1" >&2; return 1; }
  mean=$("$IM" "$frame" -crop 40x40+"$2"+"$3" +repage -colorspace Gray -format '%[fx:mean*255]' info:) || return 1
  [[ "$mean" =~ ^[0-9]+(\.[0-9]+)?$ ]] || { echo "luminance of $1 is not a number: '$mean'" >&2; return 1; }
  echo "$mean"
}
# A luminance row: CASE FRAME OP (< or >). Fails the run on a wrong or unreadable value.
check_luma() { # case frame op
  local mean
  mean=$(luma "$2" $PATCH) || { tsv_row "$1" "$2 patch luminance" "$3 $LUMA_THRESHOLD" "unreadable" FAIL; fail "could not read the luminance of $2"; }
  echo "luminance patch $PATCH: $2 mean $mean"
  if awk -v v="$mean" -v t="$LUMA_THRESHOLD" -v op="$3" 'BEGIN {exit !((op == "<" && v < t) || (op == ">" && v > t))}'; then
    tsv_row "$1" "$2 patch luminance" "$3 $LUMA_THRESHOLD" "$mean" ok
  else
    tsv_row "$1" "$2 patch luminance" "$3 $LUMA_THRESHOLD" "$mean" FAIL
    fail "the patch of $2 is on the wrong side of $LUMA_THRESHOLD"
  fi
}
ALL=ALL # the one list of shellRects keys lives in zl.py (KEYS)
COLUMNS_ONLY=titlebar,left,right
# The assertions of case 1 at one window size; $1 case, $2 width, $3 each half's width.
HEIGHTS=(
  'titlebar.h|titlebar.h|==|34'
  'primaryStrip.h|primaryStrip.h|==|32'
  'secondaryStrip.h|secondaryStrip.h|==|32'
  'status.h|status.h|==|30'
  'rightHeader.h|rightHeader.h|==|32'
  'sidebarSection.h|sidebarSection.h|==|31'
  'sidebarCard.h|sidebarCard.h|==|53'
)
INSIDE=(
  'rightHeaderLastIcon inside right (px to spare)|right.x + right.w - 1 - (rightHeaderLastIcon.x + rightHeaderLastIcon.w)|>=|0'
  'secondaryClose inside secondaryStrip less its bottom px (min px to spare)|min(secondaryClose.x - secondaryStrip.x, secondaryClose.y - secondaryStrip.y, secondaryStrip.x + secondaryStrip.w - secondaryClose.x - secondaryClose.w, secondaryStrip.y + secondaryStrip.h - 1 - secondaryClose.y - secondaryClose.h)|>=|0'
  'secondaryInsideCenter (spec §9, report only; overhang px past center right edge - 1)|secondary.x + secondary.w - (center.x + center.w - 1)|warn<=|0'
)
measure_case1() { # case width half
  ZL measure "$1" "$2" "$ALL" \
    "left.w|left.w|~|300" "right.w|right.w|~|240" \
    "primary.w|primary.w|~|$3" "secondary.w|secondary.w|~|$3" \
    "${HEIGHTS[@]}" "${INSIDE[@]}" || fail "a measurement of $1 is wrong (see the FAIL row above)"
}

# ---- case 1: a fresh database ------------------------------------------------
launch_app 1470 833
ctl project add "$WT"
ctl select-workspace --workspace "$WT"
ensure_secondary
step "1a: fresh database at 1470x833"
measure_case1 1a 1470 456.5
capture dark-1470
# A patch of the right panel, near its bottom (empty background), whose luminance tells dark from light.
PATCH=$(ZL get 1a 1470 ALL 'round(right.x + right.w / 2 - 20)') || fail "could not place the luminance patch"
PATCH="$PATCH $(ZL get 1a 1470 ALL 'round(right.y + right.h - 80)')" || fail "could not place the luminance patch"
check_luma 1a dark-1470 "<"

step "1b: fresh database at 1710x987"
resize_window 1710 987
WIDTH=1710
measure_case1 1b 1710 576.5
capture dark-1710

step "1c: fresh database at 1100x800"
resize_window 1100 800
WIDTH=1100
ZL measure 1c 1100 "$ALL" \
  'primary.w|primary.w|>=|320' 'secondary.w|secondary.w|>=|320' \
  'right.x vs center end|right.x|>=|center.x + center.w' \
  'center.x vs left end|center.x|>=|left.x + left.w' \
  || fail "a measurement of 1c is wrong (see the FAIL row above)"
quit_app

# ---- case 2: the migration --------------------------------------------------
step "2: a stored 280 / 320 at schema 26 draws 300 / 240"
db_seed 26 appearance.sidebarWidth=280 appearance.rightPanelWidth=320
start 1710 987
ZL measure 2 1710 "$COLUMNS_ONLY" 'left.w|left.w|~|300' 'right.w|right.w|~|240' || fail "a measurement of 2 is wrong"
quit_app
expect_db 2 appearance.sidebarWidth 300
expect_db 2 appearance.rightPanelWidth 240
expect_version 2 27

# ---- case 3: dragged widths are kept ------------------------------------------
step "3: a stored 350 / 400 at schema 26 is kept"
db_seed 26 appearance.sidebarWidth=350 appearance.rightPanelWidth=400
start 1710 987
ZL measure 3 1710 "$COLUMNS_ONLY" 'left.w|left.w|~|350' 'right.w|right.w|~|400' || fail "a measurement of 3 is wrong"
quit_app
expect_db 3 appearance.sidebarWidth 350
expect_db 3 appearance.rightPanelWidth 400
expect_version 3 27

# ---- case 4: a deliberate 280 after the migration -----------------------------
step "4: a stored 280 at schema 27 is kept"
db_seed 27 appearance.sidebarWidth=280 appearance.rightPanelWidth=300
start 1710 987
ZL measure 4 1710 "$COLUMNS_ONLY" 'left.w|left.w|~|280' || fail "a measurement of 4 is wrong"
quit_app

# ---- case 5: an unparseable width -----------------------------------------------
step "5: an unparseable sidebar width falls back to 300"
db_seed 26 appearance.sidebarWidth=abc appearance.rightPanelWidth=240
start 1710 987
ZL measure 5 1710 "$COLUMNS_ONLY" 'left.w|left.w|~|300' || fail "a measurement of 5 is wrong"
quit_app
expect_version 5 27

# ---- case 6: the divider -----------------------------------------------------
step "6: a dragged divider is saved and drawn again after a relaunch"
db_seed 27 appearance.sidebarWidth=300 appearance.rightPanelWidth=240 appearance.centerSplitRatio=500
start 1710 987
ZL measure 6 1710 "$ALL" 'primary.w before the drag|primary.w|~|576.5' || fail "a measurement of 6 is wrong"
HANDLE=$(ZL center 1710 dividerHandle 6)
echo "divider handle at window pixel $HANDLE"
drag_window "${HANDLE% *}" "${HANDLE#* }" $((${HANDLE% *} + 120)) "${HANDLE#* }"
MOUSEUP_AT=$(date +%s.%N)
echo "mouseup at $(date -d "@$MOUSEUP_AT" +%T.%N)"
# The halves move by about 120 px; wait for the paint that shows it.
ZL wait 6 20 'primary.w after the drag exceeds 600 (poll)' 'primary.w > 600' || fail "the halves never moved with the drag"
PRIMARY=$(ZL get 6 1710 "$ALL" 'primary.w')
SECONDARY=$(ZL get 6 1710 "$ALL" 'secondary.w')
echo "after the drag: primary.w $PRIMARY, secondary.w $SECONDARY"
ZL measure 6 1710 "$ALL" 'primary.w grew with the drag|primary.w|>=|600' 'secondary.w shrank with the drag|secondary.w|<=|550' \
  || fail "the drag did not move the halves"
# The save is debounced 500 ms: quit 1.5 s after the real mouseup, counting the reads above.
SPARE=$(awk -v t0="$MOUSEUP_AT" -v t1="$(date +%s.%N)" 'BEGIN {d = 1.5 - (t1 - t0); print (d > 0 ? d : 0)}')
sleep "$SPARE"
QUIT_AT=$(date +%s.%N)
echo "quitting at $(date -d "@$QUIT_AT" +%T.%N); mouseup to quit: $(awk -v a="$MOUSEUP_AT" -v b="$QUIT_AT" 'BEGIN {printf "%.3f", b - a}') s"
quit_app
RATIO=$(db_get appearance.centerSplitRatio)
if [ "$RATIO" != 500 ] && [ "$RATIO" != "<none>" ]; then
  printf '6\tcenterSplitRatio\t!= 500\t%s\tok\n' "$RATIO" >>"$OUT_DIR/measurements.tsv"
  echo "  ok    centerSplitRatio stored: $RATIO (not 500)"
else
  printf '6\tcenterSplitRatio\t!= 500\t%s\tFAIL\n' "$RATIO" >>"$OUT_DIR/measurements.tsv"
  fail "centerSplitRatio is stored as $RATIO after a drag and a 1.5 s wait: the drag did not schedule its save"
fi
start 1710 987
ZL measure 6 1710 "$ALL" "primary.w after the relaunch|primary.w|~|$PRIMARY" "secondary.w after the relaunch|secondary.w|~|$SECONDARY" \
  || fail "the relaunch did not draw the halves the drag left"
capture divider-relaunched
quit_app

# ---- case 7: light theme, interface font 18 -----------------------------------
step "7: light theme at interface font 18"
"$SEED_BIN" --database "$SIRIO_DB" --appearance light || fail "could not seed the light appearance"
db_seed 27 appearance.uiFontSize=18
start 1470 833
ZL measure 7 1470 "$ALL" \
  'titlebar.h|titlebar.h|==|34' 'primaryStrip.h|primaryStrip.h|==|32' 'secondaryStrip.h|secondaryStrip.h|==|32' \
  'status.h|status.h|==|30' 'rightHeader.h|rightHeader.h|==|32' \
  'sidebarSection.h at font 18|sidebarSection.h|==|31' 'sidebarCard.h at font 18 (53 + 5 for the title line)|sidebarCard.h|==|58' \
  || fail "a measurement of 7 is wrong"
capture light-1470-font18
check_luma 7 light-1470-font18 ">"
quit_app

# ---- case 7b: interface font back to 13, light, at 1710x987 --------------------
step "7b: light theme at interface font 13, 1710x987"
db_seed 27 appearance.uiFontSize=13
start 1710 987
measure_case1 7b 1710 576.5
capture light-1710
check_luma 7b light-1710 ">"
quit_app

echo "measurements: $OUT_DIR/measurements.tsv"
if awk -F'\t' 'NR > 1 && $5 != "ok" && $5 != "warn" {bad = 1} END {exit bad}' "$OUT_DIR/measurements.tsv"; then
  echo "ZED LAYOUT E2E OK"
else
  fail "a measurements row is not ok"
fi
