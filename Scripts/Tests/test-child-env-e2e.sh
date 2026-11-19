#!/bin/bash
set -eEuo pipefail
umask 022
# Every git this script runs reads no user or system configuration.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

# End-to-end test of the environment a link is opened in. gpui starts `xdg-open` for every
# link the app opens; Sirio hands it the user's environment (`set_child_environment` in
# `sirio/src/main.rs`, from `sirio_env::for_child`), never the AppImage mount's variables.
# A real debug Sirio is started the way an AppImage starts it, through a fake mount's
# AppRun (the shape Scripts/Tests/test-sirioctl-e2e.sh builds: AppRun as shipped, the
# binaries under usr/bin, a hook that exports GTK_THEME), on a private Xvfb, with a Claude
# chat on the file-links stub (Scripts/Tests/stubs/file-links/claude). A real X click on the
# web link the chat draws opens it; an `xdg-open` stub on the login PATH writes its own
# environment to a file, and that dump must hold the user's values and nothing of the mount:
#   - PATH, LD_LIBRARY_PATH and XDG_DATA_DIRS name no entry of the mount;
#   - APPIMAGE, APPDIR (the AppImage runtime's own, given to AppRun here), GTK_THEME (the
#     hook's) and SIRIO_LAUNCH_ENV (AppRun's snapshot) are absent;
#   - the login shell's PATH entries (the private ~/.profile puts the stub directory first)
#     come before the launch PATH's, and HOME is the world's;
#   - WAYLAND_DISPLAY is the user's own (`wayland-test`) although the app removed it from
#     itself: with both WAYLAND_DISPLAY and a DISPLAY set the app forces X11 by default
#     (`display_backend.rs`, `SIRIO_FORCE_X11` unset), and a link must not lose the user's
#     Wayland session because Sirio runs on X11.
# Spawn sites: the vendored engine (rust/vendor/gpui_linux/src/linux/platform.rs) applies the
# child environment at three places: open_uri_internal (:1212, `open::commands`, reached by every
# `cx.open_url`, which is what a web link click calls), reveal_path_internal (:1247) and
# open_with_system (:951, `xdg-open`). Only the first is reachable from Sirio: nothing in
# rust/crates or the vendored Ely calls `reveal_path` or `open_with_system` (the file views'
# "Reveal in file manager" runs its own `sirio_env::command("xdg-open")`, sirio_ui/src/editor.rs:1288),
# so this one click covers every link or file opener the app can start through the engine (the
# restart path spawns without the child environment, outside the spec's §5.3.2). A re-applied
# set_child_environment that misses the two unreachable sites cannot be seen by any E2E.
# With `set_child_environment` removed from main.rs the script must go red on a mount entry;
# that mutation is made by hand (rebuild, run, restore), not by a flag.
#
# The artifact: --out-dir DIR (default artifacts/child-env-e2e-<stamp>-<pid>) keeps
# transcript.log, app.log, xdg-open.env (the dump itself, NUL-separated NAME=value), the
# stub's call log and the PID-matched window capture in frames/. Rerunning the script
# reproduces it.
#
# Usage: Scripts/Tests/test-child-env-e2e.sh [--xvfb | --display :N] [--out-dir DIR]
#   --xvfb        start a private Xvfb (2400x1200) on a free display between :90 and :99
#   --display :N  use an Xvfb this user started (never your own $DISPLAY); the window is
#                 resized to 2200x1000, so the screen must be at least 2400x1200

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
HOST_BIN="$TARGET_DIR/debug/sirio-host"
CTL="$TARGET_DIR/debug/sirioctl"

START_XVFB=0
DISPLAY_TARGET=""
OUT_DIR=""
WINDOW_SIZE=2200x1000
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/child-env-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

for tool in python3 git xdotool xwininfo xprop xdpyinfo import identify; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
[ "$(id -u)" -ne 0 ] || fail "run as a user, not root"
[ -n "$DISPLAY_TARGET" ] || [ "$START_XVFB" -eq 1 ] || fail "no display; pass --xvfb or --display :N"
[ -z "$DISPLAY_TARGET" ] || [ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY; pass --xvfb, or an Xvfb you started on another number"

# ---------------------------------------------------------------- build

echo "building sirio, sirioctl and sirio-host"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host)
for binary in "$BIN" "$HOST_BIN" "$CTL"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run

# Short and on disk: a socket path is limited to 108 bytes, and tmpfs is small.
RUN="$(mktemp -d "${TMPDIR:-/tmp}/child-env-e2e-XXXXXX")"
echo "run root: $RUN"
APP_PID=""
XVFB_PID=""

is_a_host() {
  local comm
  comm="$(cat "/proc/$1/comm" 2>/dev/null || true)"
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
terminate() { # pid: TERM, then KILL if it has not exited within 5 s
  kill -TERM "$1" 2>/dev/null || return 0
  (sleep 5; kill -KILL "$1" 2>/dev/null) >/dev/null 2>&1 &
  local watchdog=$!
  wait "$1" 2>/dev/null || true
  kill "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
}
stop_hosts() {
  local file pid tries
  for file in "$RUN"/host/host-v*.json; do
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
  cp "$RUN"/app.log "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN"/xdg-open.log "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN"/xdg-open.env "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN"
  exit "$rc"
}
trap cleanup EXIT

VULKAN_ENV=()
if [ "$START_XVFB" -eq 1 ]; then
  command -v Xvfb >/dev/null || fail "Xvfb is required for --xvfb"
  for number in $(seq 90 99); do
    [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
  done
  [ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
  Xvfb "$DISPLAY_TARGET" -screen 0 2400x1200x24 >"$OUT_DIR/xvfb.log" 2>&1 &
  XVFB_PID=$!
  for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
  DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
fi
echo "display: $DISPLAY_TARGET"
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
[ -n "$lvp" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required for the window to draw on Xvfb"
VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp")

# ---------------------------------------------------------------- the world

HOME_DIR="$RUN/home"
MOUNT="$RUN/mnt"
REPO="$RUN/repo"
LOGIN_BIN="$RUN/loginbin"
mkdir -p "$HOME_DIR" "$RUN/xdg/config" "$RUN/xdg/data" "$RUN/xdg/state" "$RUN/xdg/cache" "$RUN/xdg/runtime" \
  "$RUN/host" "$LOGIN_BIN" "$REPO"
chmod 700 "$RUN/xdg/runtime"
export SIRIO_SOCKET="$RUN/c.sock"
[ "${#SIRIO_SOCKET}" -lt 100 ] || fail "the socket path is too long for a unix socket: $SIRIO_SOCKET"

git -C "$REPO" init -q -b main
git -C "$REPO" config user.email tester@example.invalid
git -C "$REPO" config user.name Tester
printf '# app\n' >"$REPO/README.md"
git -C "$REPO" add -A
git -C "$REPO" commit -q -m first

# The opener the app's link reaches: first on the login PATH, which the private ~/.profile sets
# (the app runs `$SHELL -lc` in this HOME and puts its entries before the launch PATH's). It
# writes its own environment, NUL-separated as `env -0` does, and exits 0, so no real browser
# starts. The launch PATH names the same directory too, so that an app that kept the mount's
# environment still finds this stub: the mutation then fails on what the dump holds, never on
# a missing dump.
cat >"$LOGIN_BIN/xdg-open" <<STUB
#!/bin/sh
/usr/bin/env -0 >"$RUN/xdg-open.env.part"
printf 'xdg-open %s\n' "\$*" >>"$RUN/xdg-open.log"
mv "$RUN/xdg-open.env.part" "$RUN/xdg-open.env"
exit 0
STUB
chmod +x "$LOGIN_BIN/xdg-open"
printf 'export PATH="%s:$PATH"\n' "$LOGIN_BIN" >"$HOME_DIR/.profile"

# The mount: AppRun as shipped, the binaries under usr/bin and the hook of the GTK plugin.
link_or_copy() { ln -f "$1" "$2" 2>/dev/null || cp "$1" "$2"; }
mkdir -p "$MOUNT/usr/bin" "$MOUNT/apprun-hooks"
cp "$ROOT/Scripts/appimage/AppRun" "$MOUNT/AppRun"
chmod +x "$MOUNT/AppRun"
link_or_copy "$BIN" "$MOUNT/usr/bin/sirio"
link_or_copy "$HOST_BIN" "$MOUNT/usr/bin/sirio-host"
cp "$CTL" "$MOUNT/usr/bin/sirioctl"
chmod +x "$MOUNT/usr/bin/sirio" "$MOUNT/usr/bin/sirio-host" "$MOUNT/usr/bin/sirioctl"
printf 'export GTK_THEME=leak\n' >"$MOUNT/apprun-hooks/gtk.sh"
MOUNT_REAL="$(cd "$MOUNT" && pwd -P)"

# The helper the harness speaks the control socket with (surface.chat.* have no sirioctl verb).
cat >"$RUN/e2e.py" <<'PY'
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


command = sys.argv[1]
if command == "chat-surface":
    print(rpc("surface.chat.open", {})["surfaceId"])
elif command == "send":
    rpc("surface.chat.send", {"surfaceId": sys.argv[2], "text": sys.argv[3]})
elif command == "wait-status":
    # wait-status SURFACE SECONDS STATUS...: poll until the chat's status is one of them.
    deadline, last = time.time() + float(sys.argv[3]), None
    while time.time() < deadline:
        try:
            last = rpc("surface.chat.read", {"surfaceId": sys.argv[2]}).get("status")
        except Exception as error:  # a read that fails is tried again
            last = str(error)
        if last in sys.argv[4:]:
            sys.exit(0)
        time.sleep(0.3)
    print(f"the chat's status is {last!r}", file=sys.stderr)
    sys.exit(1)
elif command == "link":
    # link SURFACE KIND LABEL: poll until the last paint reports that link, print "x y" (window pixels).
    deadline, rows = time.time() + 20, []
    while time.time() < deadline:
        try:
            result = rpc("surface.chat.read", {"surfaceId": sys.argv[2]})
            rows = json.loads(result.get("transcriptLinks") or "[]")
            row = next((r for r in rows if r["kind"] == sys.argv[3] and r["label"] == sys.argv[4]), None)
            if row:
                print(f"{int(float(row['x']))} {int(float(row['y']))}")
                sys.exit(0)
        except SystemExit:
            raise
        except Exception:
            pass
        time.sleep(0.3)
    print("links: " + json.dumps(rows, ensure_ascii=False), file=sys.stderr)
    sys.exit(1)
else:
    sys.exit(f"unknown command {command}")
PY
E2E() { python3 -I "$RUN/e2e.py" "$@"; }

# The dump reader of test-sirioctl-e2e.sh: `get` prints a value or __ABSENT__, `under` exits 1
# when a ':'-separated entry of NAME is DIR or lies below it, `first` exits 0 when the first
# entry of NAME is DIR, `before` exits 0 when entry A comes before entry B.
cat >"$RUN/envcheck.py" <<'PY'
import sys
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
elif mode == "before":
    items = env.get(name, "").split(":")
    sys.exit(0 if sys.argv[4] in items and sys.argv[5] in items and items.index(sys.argv[4]) < items.index(sys.argv[5]) else 1)
else:
    sys.exit(2)
PY
env_get() { python3 -I "$RUN/envcheck.py" "$1" get "$2"; }
env_under() { python3 -I "$RUN/envcheck.py" "$1" under "$2" "${@:3}"; }
env_before() { python3 -I "$RUN/envcheck.py" "$1" before "$2" "$3" "$4"; }

# ---------------------------------------------------------------- the app

STUBS="$ROOT/Scripts/Tests/stubs/file-links"
APP_PATH="$STUBS:$LOGIN_BIN:/usr/bin:/bin"
[ "$(PATH="$APP_PATH" command -v claude)" = "$STUBS/claude" ] || fail "claude does not resolve to the file-links stub under $APP_PATH"

# The launch environment, as a user's session hands it to the AppImage: a Wayland session
# (WAYLAND_DISPLAY) and an X display both set, which makes the app force X11 for itself, and the
# AppImage runtime's own APPIMAGE and APPDIR.
PAIRS=(
  "PATH=$APP_PATH" "HOME=$HOME_DIR" "SHELL=/bin/bash" "LANG=C.UTF-8" "TERM=xterm-256color"
  "XDG_CONFIG_HOME=$RUN/xdg/config" "XDG_DATA_HOME=$RUN/xdg/data" "XDG_STATE_HOME=$RUN/xdg/state"
  "XDG_CACHE_HOME=$RUN/xdg/cache" "XDG_RUNTIME_DIR=$RUN/xdg/runtime"
  "SIRIO_SOCKET=$SIRIO_SOCKET" "SIRIO_DB=$RUN/s.sqlite" "SIRIO_CREDENTIALS=$RUN/cred.json"
  "SIRIO_HOST_HOME=$RUN/host" "SIRIO_FIXTURE_REPO=$REPO"
  "DBUS_SESSION_BUS_ADDRESS=unix:path=$RUN/no-bus"
  "WAYLAND_DISPLAY=wayland-test" "DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1"
  "APPIMAGE=$MOUNT_REAL/fake.AppImage" "APPDIR=$MOUNT_REAL"
  "${VULKAN_ENV[@]}"
)
# SIRIO_FORCE_X11 is deliberately unset: the default is the case under test.
(cd "$REPO" && exec env -i "${PAIRS[@]}" "$MOUNT/AppRun" >>"$RUN/app.log" 2>&1) &
APP_PID=$!
for _ in $(seq 1 300); do
  [ -S "$SIRIO_SOCKET" ] && break
  sleep 0.1
done
[ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket (log: $(tail -n 5 "$RUN/app.log" 2>/dev/null))"
for _ in $(seq 1 600); do
  "$CTL" surface tabs read >/dev/null 2>&1 && break
  sleep 0.1
done
"$CTL" surface tabs read >/dev/null 2>&1 || fail "sirio's control actions never became ready"
echo "OK: sirio (pid $APP_PID) started through the mount's AppRun"

# The app's own environment, from /proc (the environment it started with: a later remove_var
# does not show there): the mount's variables are in it, so the dump below is not trivially clean.
python3 -I - "$APP_PID" "$MOUNT_REAL" <<'PY' || fail "the app does not look like an AppImage launch"
import sys
pid, mount = sys.argv[1:3]
env = {}
for entry in open(f"/proc/{pid}/environ", "rb").read().split(b"\0"):
    key, sep, value = entry.partition(b"=")
    if sep:
        env[key.decode()] = value.decode("utf-8", "replace")
problems = []
if not any(item.startswith(mount) for item in env.get("PATH", "").split(":")):
    problems.append("the app's PATH has no mount entry")
if "GTK_THEME" not in env:
    problems.append("the app has no GTK_THEME from the hook")
if problems:
    print("; ".join(problems), file=sys.stderr)
    sys.exit(1)
PY
echo "OK: the app's own environment has the mount's entries and GTK_THEME"
# The app removed WAYLAND_DISPLAY from itself and runs on X11: its own startup line says so.
for _ in $(seq 1 50); do
  grep -q '^\[display\] Wayland session detected; running on X11' "$RUN/app.log" && break
  sleep 0.1
done
grep -q '^\[display\] Wayland session detected; running on X11' "$RUN/app.log" \
  || fail "the app did not force X11 for itself (log: $(head -n 5 "$RUN/app.log"))"
echo "OK: the app forced X11 for itself (WAYLAND_DISPLAY and a DISPLAY were both set)"

find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 -I - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"],
                         capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"],
                          capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
}

ctl() { echo "+ sirioctl $*"; timeout -k 5 30 "$CTL" "$@"; }
ctl surface attention act --focus active >/dev/null || fail "surface attention act --focus active was refused"
WINDOW=""
for _ in $(seq 1 100); do
  WINDOW="$(find_window)"
  [ -n "$WINDOW" ] && break
  sleep 0.2
done
[ -n "$WINDOW" ] || fail "no window with _NET_WM_PID=$APP_PID"
DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowmove "$WINDOW" 0 0 windowsize "$WINDOW" "${WINDOW_SIZE%x*}" "${WINDOW_SIZE#*x}" \
  || fail "could not resize the window to $WINDOW_SIZE"
sleep 1
echo "OK: the window is $WINDOW_SIZE"

# ---------------------------------------------------------------- the chat and the link

# The tab positions of the kinds `surface tabs read` lists, highest first.
tab_positions() { # kind...
  "$CTL" surface tabs read --json 2>/dev/null | python3 -I -c '
import json, sys
kinds = sys.argv[1:]
tabs = json.load(sys.stdin)[0]
print(*sorted((int(k.split(".")[1]) for k, v in tabs.items() if k.startswith("tab.") and v.split("|")[0] in kinds), reverse=True))' "$@"
}
# AppRun starts the app in the mount, so no worktree is selected until the project is.
ctl project add "$REPO" >/dev/null 2>&1 || true
for _ in $(seq 1 40); do
  "$CTL" select-workspace --workspace "$REPO" >/dev/null 2>&1 && break
  sleep 0.5
done
case "$("$CTL" current-workspace --json 2>&1 || true)" in *"$REPO"*) ;; *) fail "the repository is not the selected workspace" ;; esac
for _ in $(seq 1 50); do
  "$CTL" surface open-list open --entry agent:claude >/dev/null 2>&1 && break
  sleep 0.3
done
OLD_TABS="$(tab_positions chat terminal)"
ctl surface open-list open --entry agent:claude >/dev/null || fail "the Open list never offered a Claude chat"
for position in $OLD_TABS; do ctl surface tabs close "$position" --confirm >/dev/null; done
SURFACE=""
for _ in $(seq 1 50); do
  SURFACE="$(E2E chat-surface 2>/dev/null || true)"
  [ -n "$SURFACE" ] && break
  sleep 0.3
done
[ -n "$SURFACE" ] || fail "no chat tab opened"
echo "OK: chat surface $SURFACE"

E2E wait-status "$SURFACE" 40 idle completed || fail "the chat never became ready for a prompt"
E2E send "$SURFACE" "mostrami i link"
echo "step: sent a prompt; the stub answers with a web link labelled 'sito'"
sleep 1
E2E wait-status "$SURFACE" 40 completed || fail "the turn never completed"
POSITION="$(tab_positions chat)"
[ -n "$POSITION" ] || fail "no chat tab to show"
ctl surface tabs select "$POSITION" >/dev/null || fail "could not select the chat tab $POSITION"
sleep 1.5
XY="$(E2E link "$SURFACE" web sito)" || fail "the response has no web link 'sito' to click"
echo "step: a real click on the web link 'sito' at window pixel $XY"
[ ! -e "$RUN/xdg-open.env" ] || fail "the dump exists before the click"
sleep 2
import -display "$DISPLAY_TARGET" -window "$WINDOW" "$OUT_DIR/frames/child-env-chat.png"
[ -s "$OUT_DIR/frames/child-env-chat.png" ] || fail "the capture is empty"
# The press and the release are 0.3 s apart, as a hand's are: a web link's text only listens
# for the release in the frame after the press, and this window paints in software.
DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove ${XY% *} ${XY#* } mousedown 1 sleep 0.3 mouseup 1 \
  || fail "xdotool could not click at $XY"

DUMP="$RUN/xdg-open.env"
for _ in $(seq 1 100); do
  [ -s "$DUMP" ] && break
  sleep 0.2
done
[ -s "$DUMP" ] || fail "the xdg-open stub never wrote its environment (stub log: $(cat "$RUN/xdg-open.log" 2>/dev/null || echo none); app log tail: $(tail -n 5 "$RUN/app.log" 2>/dev/null))"
grep -Fx 'xdg-open https://example.com/docs/NOME-FILE.md' "$RUN/xdg-open.log" >/dev/null \
  || fail "the stub was not asked for the link (it recorded: $(cat "$RUN/xdg-open.log"))"
echo "OK: the click reached the xdg-open stub, and its environment was dumped"

# ---------------------------------------------------------------- the dump

name="xdg-open dump"
FAILED=()
check() { # description command...: a failed check is recorded, every one is run
  local description=$1
  shift
  "$@" >/dev/null 2>&1 || FAILED+=("$name: $description")
}
absent() { [ "$(env_get "$DUMP" "$1")" = __ABSENT__ ]; }
is_value() { [ "$(env_get "$DUMP" "$1")" = "$2" ]; }
check "PATH holds a mount entry" env_under "$DUMP" PATH "$MOUNT" "$MOUNT_REAL"
check "LD_LIBRARY_PATH holds a mount entry" env_under "$DUMP" LD_LIBRARY_PATH "$MOUNT" "$MOUNT_REAL"
check "XDG_DATA_DIRS holds a mount entry" env_under "$DUMP" XDG_DATA_DIRS "$MOUNT" "$MOUNT_REAL"
for var in APPIMAGE APPDIR GTK_THEME SIRIO_LAUNCH_ENV; do
  check "carries $var" absent "$var"
done
check "WAYLAND_DISPLAY is '$(env_get "$DUMP" WAYLAND_DISPLAY)', not the user's wayland-test" is_value WAYLAND_DISPLAY wayland-test
# The launch PATH lists the file-links stubs before the login directory; the user environment
# puts the login shell's entries first, so the login directory leads the stubs in the dump.
check "the login shell's PATH entries do not come before the launch PATH's" env_before "$DUMP" PATH "$LOGIN_BIN" "$STUBS"
check "HOME is not the world's" is_value HOME "$HOME_DIR"
if [ "${#FAILED[@]}" -gt 0 ]; then
  for failure in "${FAILED[@]}"; do echo "FAIL: $failure" >&2; done
  echo "FAIL: ${#FAILED[@]} assertion(s) on the opener's environment failed" >&2
  exit 1
fi
echo "OK: the opener's environment has no mount entry in PATH, LD_LIBRARY_PATH or XDG_DATA_DIRS; no APPIMAGE, APPDIR, GTK_THEME or SIRIO_LAUNCH_ENV; the user's WAYLAND_DISPLAY and login PATH"

echo "CHILD ENV E2E OK (display $DISPLAY_TARGET, dump $OUT_DIR/xdg-open.env)"
