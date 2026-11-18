#!/bin/bash
set -eEuo pipefail

# End-to-end test of a remote image in the Markdown Preview of the file view: a real,
# isolated debug Sirio on a private Xvfb opens a .md file whose image is served over
# loopback HTTP by `python3 -m http.server`, shows it in Preview, and a PID-matched
# capture of the window must hold the picture. The image is a 64x64 PNG of one colour
# the app's own palette never draws; the proof is two-fold:
#   - the capture holds a block of that colour at least 32x32 pixels (the picture was
#     fetched, decoded and painted: an engine with no HTTP client draws nothing there);
#   - the server's own log shows at least one GET of the image, answered 200 (it was asked for; a re-fetch is not a failure).
# Linux and X11 only: the capture is an X window, the app starts on a private Xvfb.
#
# The artifact: --out-dir DIR (default artifacts/markdown-remote-image-e2e-<stamp>-<pid>)
# keeps transcript.log, app.log, the server's log (http-server.log), the scan's answer
# (scan.txt) and the capture in frames/. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-markdown-remote-image-e2e.sh [--xvfb | --display :N]
#          [--out-dir DIR] [--mutate missing-image]
#
#   --xvfb        start a private Xvfb on a free display between :90 and :99
#   --display :N  use an Xvfb this user started (never your own $DISPLAY)
#   --mutate missing-image
#                 the Markdown names an image the server does not have. The script must
#                 then FAIL (no picture, a 404 in the log): the proof that it can go red.
#                 It never prints "MARKDOWN REMOTE IMAGE E2E OK" in this mode.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"

START_XVFB=0
DISPLAY_TARGET=""
OUT_DIR=""
MUTATE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --mutate) MUTATE="${2:?--mutate needs a name}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$MUTATE" in ""|missing-image) ;; *) echo "--mutate is missing-image" >&2; exit 2 ;; esac
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/markdown-remote-image-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

for tool in python3 git xdotool xwininfo xprop xdpyinfo import identify magick curl; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
[ "$(id -u)" -ne 0 ] || fail "run as a user, not root"
[ -n "$DISPLAY_TARGET" ] || [ "$START_XVFB" -eq 1 ] || fail "no display; pass --xvfb or --display :N"
[ -z "$DISPLAY_TARGET" ] || [ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY; pass --xvfb, or an Xvfb you started on another number"

# ---------------------------------------------------------------- build

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host)
for binary in "$BIN" "$CTL"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run

BASE="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/mdimg-XXXXXX")"
WT="$RUN_DIR/repo"
APP_PID=""
XVFB_PID=""
SERVER_PID=""

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
  [ -z "$SERVER_PID" ] || terminate "$SERVER_PID"
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  cp "$RUN_DIR"/app.log "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN_DIR"/http-server.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
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
  Xvfb "$DISPLAY_TARGET" -screen 0 1920x1080x24 >"$OUT_DIR/xvfb.log" 2>&1 &
  XVFB_PID=$!
  for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
  DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
fi
echo "display: $DISPLAY_TARGET (mutate: ${MUTATE:-none})"
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi

# A private world: HOME and every XDG directory under the run dir, stub agent CLIs first on
# PATH (so no mise wrapper runs), and a host root of its own.
export XDG_STATE_HOME="$RUN_DIR/xdg/state" XDG_DATA_HOME="$RUN_DIR/xdg/data"
export XDG_CONFIG_HOME="$RUN_DIR/xdg/config" XDG_CACHE_HOME="$RUN_DIR/xdg/cache"
export XDG_RUNTIME_DIR="$RUN_DIR/xdg/runtime"
mkdir -p "$RUN_DIR/home" "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR" "$RUN_DIR/host" "$RUN_DIR/stubs"
chmod 700 "$XDG_RUNTIME_DIR"
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

# ---------------------------------------------------------------- the image and its server

# The picture's colour: a magenta the theme never draws, so a block of it in the capture is
# the picture and nothing else.
IMAGE_RGB="250 40 190"
mkdir -p "$RUN_DIR/www"
python3 -I - "$RUN_DIR/www/pixel.png" $IMAGE_RGB <<'PY'
import struct, sys, zlib
path, red, green, blue = sys.argv[1], *map(int, sys.argv[2:5])
width = height = 64
row = b"\x00" + bytes((red, green, blue)) * width
raw = row * height
def chunk(kind, data):
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)
png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(raw))
png += chunk(b"IEND", b"")
open(path, "wb").write(png)
PY
[ "$(identify -format '%wx%h' "$RUN_DIR/www/pixel.png")" = 64x64 ] || fail "the fixture PNG is not 64x64"

PORT="$(python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
(cd "$RUN_DIR/www" && exec python3 -I -m http.server "$PORT" --bind 127.0.0.1 >>"$RUN_DIR/http-server.log" 2>&1) &
SERVER_PID=$!
for _ in $(seq 1 50); do
  curl --noproxy "*" -s -o /dev/null --max-time 1 "http://127.0.0.1:$PORT/pixel.png" && break
  sleep 0.1
done
curl --noproxy "*" -s --max-time 2 "http://127.0.0.1:$PORT/pixel.png" | cmp -s - "$RUN_DIR/www/pixel.png" \
  || fail "the loopback server does not serve the fixture PNG"
# The two probes above were the harness's own GETs: the log starts clean for the app.
: >"$RUN_DIR/http-server.log"

# The worktree and the Markdown file whose first block is the image.
mkdir -p "$WT"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
IMAGE_NAME="pixel.png"
[ "$MUTATE" != missing-image ] || IMAGE_NAME="no-such-image.png"
printf '![remote pixel](http://127.0.0.1:%s/%s)\n\n# Remote image\n\nThe picture above comes from loopback HTTP.\n' \
  "$PORT" "$IMAGE_NAME" >"$WT/remote.md"
git -C "$WT" add -A
git -C "$WT" commit -q -m first

# ---------------------------------------------------------------- the app

app_start() {
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -u WAYLAND_DISPLAY -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u all_proxy NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 \
    ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 300); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.1
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
  for _ in $(seq 1 600); do
    timeout -k 5 30 "$CTL" surface tabs read >/dev/null 2>&1 && return 0
    sleep 0.1
  done
  fail "sirio's control actions never became ready"
}
app_start

ctl() { echo "+ sirioctl $*"; timeout -k 5 30 "$CTL" "$@"; }
ctl project add "$WT" >/dev/null 2>&1 || true
for _ in $(seq 1 40); do
  timeout -k 5 30 "$CTL" select-workspace --workspace "$WT" >/dev/null 2>&1 && break
  sleep 0.5
done
CURRENT="$(timeout -k 5 30 "$CTL" current-workspace --json 2>&1 || true)"
case "$CURRENT" in *"$WT"*) ;; *) fail "the repository is not the selected workspace: $CURRENT" ;; esac
echo "OK: the repository is the selected workspace"

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

# Opens the file and waits for the editor to say it is loaded, then switches to Preview.
ctl surface editor open --path="$WT/remote.md" >/dev/null
STATE=""
for _ in $(seq 1 100); do
  STATE="$(timeout -k 5 30 "$CTL" surface editor state 2>&1 || true)"
  printf '%s\n' "$STATE" | grep -q '^state=loaded$' && break
  sleep 0.1
done
printf '%s\n' "$STATE" | grep -q '^state=loaded$' || fail "remote.md never loaded: $STATE"
ctl surface editor mode --mode=preview >/dev/null
for _ in $(seq 1 100); do
  STATE="$(timeout -k 5 30 "$CTL" surface editor state 2>&1 || true)"
  printf '%s\n' "$STATE" | grep -q '^mode=preview$' && break
  sleep 0.1
done
printf '%s\n' "$STATE" | grep -q '^mode=preview$' || fail "the file never reached Preview: $STATE"
echo "OK: remote.md is open in Preview"

# The picture arrives after the first paint: the capture is retaken until the block shows
# or the time is up, and the last capture is the artifact either way.
scan() { # frame: prints the side of the largest square of the picture's colour, as SxS (0x0 for none)
  local rgb="rgb(${IMAGE_RGB// /,})" width height
  read -r width height < <(identify -format '%w %h\n' "$1")
  # The mask: pixels within 2% of the picture's colour become rgb(1,2,3), every other pixel black.
  magick "$1" -fuzz 2% -fill 'rgb(1,2,3)' -opaque "$rgb" -fuzz 0 -fill black +opaque 'rgb(1,2,3)' -depth 8 gray:- \
    | python3 -I -c '
import sys
width, height = int(sys.argv[1]), int(sys.argv[2])
data = sys.stdin.buffer.read()
assert len(data) == width * height, (len(data), width, height)
best = 0
previous = [0] * (width + 1)
for y in range(height):
    row = data[y * width:(y + 1) * width]
    current = [0] * (width + 1)
    for x in range(width):
        if row[x]:
            side = min(previous[x], previous[x + 1], current[x]) + 1
            current[x + 1] = side
            if side > best:
                best = side
    previous = current
print(f"{best}x{best}")
' "$width" "$height"
}
BLOCK="0x0"
for attempt in $(seq 1 12); do
  sleep 1
  window="$(find_window)"
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/markdown-remote-image.png"
  [ -s "$OUT_DIR/frames/markdown-remote-image.png" ] || fail "the capture is empty"
  BLOCK="$(scan "$OUT_DIR/frames/markdown-remote-image.png")"
  echo "scan $attempt: largest block of rgb($IMAGE_RGB) is $BLOCK"
  [ "${BLOCK%x*}" -ge 32 ] && [ "${BLOCK#*x}" -ge 32 ] && break
done
echo "$BLOCK" >"$OUT_DIR/scan.txt"
colours="$(identify -format '%k' "$OUT_DIR/frames/markdown-remote-image.png")"
[ "$colours" -ge 200 ] || fail "the capture is blank ($colours colours)"
echo "FRAME: markdown-remote-image ($colours colours)"

[ "${BLOCK%x*}" -ge 32 ] && [ "${BLOCK#*x}" -ge 32 ] \
  || fail "the capture holds no block of rgb($IMAGE_RGB) of at least 32x32 (largest: $BLOCK): the remote image was not drawn"
echo "OK: the capture holds the picture's colour in a $BLOCK block"

# The server's own log: one GET of the image, and a success.
sleep 1
GETS="$(grep -c "\"GET /$IMAGE_NAME HTTP" "$RUN_DIR/http-server.log" || true)"
cp "$RUN_DIR/http-server.log" "$OUT_DIR/http-server.log"
[ "$GETS" -ge 1 ] || fail "the server saw no GET of /$IMAGE_NAME (log: $(tr '\n' '|' <"$RUN_DIR/http-server.log"))"
grep -q "\"GET /$IMAGE_NAME HTTP/[0-9.]*\" 200" "$RUN_DIR/http-server.log" \
  || fail "the GET of /$IMAGE_NAME was not answered 200 (log: $(tr '\n' '|' <"$RUN_DIR/http-server.log"))"
echo "OK: the server's log shows $GETS GET of /$IMAGE_NAME, one answered 200"

echo "MARKDOWN REMOTE IMAGE E2E OK (block $BLOCK, $GETS GET, display $DISPLAY_TARGET)"
