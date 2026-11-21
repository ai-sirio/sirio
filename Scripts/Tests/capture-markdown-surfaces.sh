#!/bin/bash
set -eEuo pipefail

# Captures how one Markdown document, Scripts/Tests/fixtures/markdown-documents/showcase.md,
# is drawn on the three surfaces that render Markdown: the file view's Preview, a chat reply
# and a change request's description. Real, isolated Sirio instances (a private HOME and XDG
# tree, stub agent CLIs, a scratch database, socket and host root) on a private Xvfb, driven
# over the control socket, with PID-matched X window captures. The same window size (1600x1000)
# and scale (GPUI_X11_SCALE_FACTOR=1) every run, so two runs, made with two builds, compare
# frame by frame: the baseline against the removed bezel-markdown, a later run against Ely's renderer.
#
#   preview          Preview of showcase.md in a worktree, a picture beside it, the remote
#                    picture served by a loopback `python3 -m http.server`
#                    (the harness of test-markdown-remote-image-e2e.sh).
#   chat             a native chat whose scripted agent (chat_fixture.py, mode reply-file)
#                    answers every prompt with showcase.md (the harness of test-ely-chat-ui-e2e.py).
#   cr-description   the detail tab of change request #101 on the loopback fake forge
#                    (the fixtures of test-forge-ui-e2e.sh), its description being showcase.md.
#
# Each surface is captured top to bottom: the wheel is turned over the page in steps of
# $SCROLL_CLICKS clicks until a step changes nothing. A frame is a whole window, so the page's
# chrome is in every one of them; the page is the part that moves.
#
# Files written to --out-dir:
#   preview-<k>.png, chat-<k>.png, cr-description-<k>.png   the scrolled pages, k from 1
#   transcript.json                                         the run: binaries, window size,
#                                                           every sirioctl call, every frame
#   logs/                                                   each leg's app.log and server logs
# Rerunning the script reproduces it. It prints MARKDOWN SURFACES CAPTURED (<n> frames).
#
# Usage: Scripts/Tests/capture-markdown-surfaces.sh --bin-dir DIR --out-dir DIR [--display :N]
#          [--only preview,chat,cr-description]
#   --bin-dir DIR   the directory holding sirio and sirio-host (sirioctl too when it has it;
#                   else the checkout's rust/target/debug/sirioctl, a client that speaks the
#                   same socket protocol)
#   --display :N    the Xvfb this script starts and stops, on :120 to :139 (default :120)
#
# Needs, from the checkout's own build: rust/target/debug/examples/ely_chat_seed (chat) and
# sirioctl when --bin-dir has none.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
FIXTURE_DIR="$ROOT/Scripts/Tests/fixtures/markdown-documents"

BIN_DIR=""
OUT_DIR=""
DISPLAY_TARGET=":120"
ONLY="preview,chat,cr-description"
SCROLL_CLICKS="${SCROLL_CLICKS:-8}"
MAX_FRAMES="${MAX_FRAMES:-60}"
WINDOW_SIZE="1600x1000"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin-dir) BIN_DIR="${2:?--bin-dir needs a directory}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --only) ONLY="${2:?--only needs a list}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$BIN_DIR" ] || { echo "--bin-dir is required" >&2; exit 2; }
[ -n "$OUT_DIR" ] || { echo "--out-dir is required" >&2; exit 2; }
BIN_DIR="$(cd "$BIN_DIR" && pwd)"
mkdir -p "$OUT_DIR/logs"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

for tool in python3 git xdotool xwininfo xprop xdpyinfo import identify magick compare curl Xvfb; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
[ "$(id -u)" -ne 0 ] || fail "run as a user, not root"
case "$DISPLAY_TARGET" in
  :12[0-9] | :13[0-9]) ;;
  *) fail "--display must be between :120 and :139 (got $DISPLAY_TARGET)" ;;
esac
[ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY"
case "$ONLY" in *[!a-z,-]*) fail "--only is a comma list of preview, chat, cr-description" ;; esac
wants() { case ",$ONLY," in *",$1,"*) return 0 ;; *) return 1 ;; esac }

BIN="$BIN_DIR/sirio"
[ -x "$BIN" ] || fail "no sirio in $BIN_DIR"
[ -x "$BIN_DIR/sirio-host" ] || fail "no sirio-host in $BIN_DIR"
CTL="$BIN_DIR/sirioctl"
[ -x "$CTL" ] || CTL="$TARGET_DIR/debug/sirioctl"
[ -x "$CTL" ] || fail "no sirioctl in $BIN_DIR or $TARGET_DIR/debug (cd rust && cargo build -p sirio_control --bin sirioctl)"
SEED="$TARGET_DIR/debug/examples/ely_chat_seed"
if wants chat; then
  [ -x "$SEED" ] || fail "no $SEED (cd rust && cargo build -p sirio_persistence --example ely_chat_seed)"
fi

# The run's scratch space is on disk: /tmp is a tmpfs.
BASE="${SIRIO_E2E_TMP:-${TMPDIR:-$HOME/.cache/st/rail-e2e-tmp}}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/mdsurf-XXXXXX")"
APP_PID=""
XVFB_PID=""
SERVER_PID=""
FORGE_PID=""
FRAMES=0
: >"$OUT_DIR/.ctl.tsv"
: >"$OUT_DIR/.frames.tsv"

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
stop_hosts() { # only a pid a host state file names, while that pid is still a sirio-host
  local file pid tries
  for file in "$RUN_DIR"/*/host/host-v*.json; do
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
stop_app() {
  [ -z "$APP_PID" ] || terminate "$APP_PID"
  APP_PID=""
  stop_hosts
  [ -z "$SERVER_PID" ] || terminate "$SERVER_PID"
  SERVER_PID=""
  [ -z "$FORGE_PID" ] || terminate "$FORGE_PID"
  FORGE_PID=""
}
cleanup() {
  local rc=$?
  stop_app
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  rm -rf "$RUN_DIR"
  exit "$rc"
}
trap cleanup EXIT

# ---------------------------------------------------------------- the display

number="${DISPLAY_TARGET#:}"
{ [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ]; } && fail "display $DISPLAY_TARGET is taken"
Xvfb "$DISPLAY_TARGET" -screen 0 2000x1300x24 >"$OUT_DIR/logs/xvfb.log" 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
echo "display: $DISPLAY_TARGET (Xvfb pid $XVFB_PID)"
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
VULKAN_ENV=()
if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi

# ---------------------------------------------------------------- one isolated world per leg

free_port() {
  python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}

# A private world: HOME and every XDG directory under the leg's directory, stub agent CLIs first
# on PATH (so no mise wrapper runs), a host root of its own.
new_world() { # name
  LEG="$RUN_DIR/$1"
  mkdir -p "$LEG/home" "$LEG/host" "$LEG/stubs" "$LEG/xdg/state" "$LEG/xdg/data" "$LEG/xdg/config" "$LEG/xdg/cache" "$LEG/xdg/runtime"
  chmod 700 "$LEG/xdg/runtime"
  for cli in claude codex opencode pi omp; do
    printf '#!/bin/sh\necho "2.1.257 (Claude Code)"\n' >"$LEG/stubs/$cli"
    chmod +x "$LEG/stubs/$cli"
  done
  export XDG_STATE_HOME="$LEG/xdg/state" XDG_DATA_HOME="$LEG/xdg/data" XDG_CONFIG_HOME="$LEG/xdg/config"
  export XDG_CACHE_HOME="$LEG/xdg/cache" XDG_RUNTIME_DIR="$LEG/xdg/runtime"
  export PATH="$LEG/stubs:$ORIGINAL_PATH" HOME="$LEG/home"
  export SIRIO_SOCKET="$LEG/c.sock" SIRIO_DB="$LEG/session.sqlite" SIRIO_CREDENTIALS="$LEG/credentials.json"
  export SIRIO_HOST_HOME="$LEG/host"
  export DBUS_SESSION_BUS_ADDRESS="unix:path=$LEG/no-bus"
  [ "${#SIRIO_SOCKET}" -lt 100 ] || fail "the socket path is too long for a unix socket: $SIRIO_SOCKET"
}
ORIGINAL_PATH="$PATH"

# The showcase, its picture, and (for the preview) the loopback server the remote picture comes from.
PORT="$(free_port)"
prepare_showcase() { # destination directory
  mkdir -p "$1"
  sed "s/{{REMOTE_PORT}}/$PORT/g" "$FIXTURE_DIR/showcase.md" >"$1/showcase.md"
  cp "$FIXTURE_DIR/pixel.png" "$1/pixel.png"
}
start_image_server() { # serves pixel.png on $PORT; its log is kept
  local www="$LEG/www"
  mkdir -p "$www"
  cp "$FIXTURE_DIR/pixel.png" "$www/pixel.png"
  (cd "$www" && exec python3 -I -m http.server "$PORT" --bind 127.0.0.1 >>"$LEG/http-server.log" 2>&1) &
  SERVER_PID=$!
  for _ in $(seq 1 50); do
    curl --noproxy "*" -s -o /dev/null --max-time 1 "http://127.0.0.1:$PORT/pixel.png" && break
    sleep 0.1
  done
  curl --noproxy "*" -s --max-time 2 "http://127.0.0.1:$PORT/pixel.png" | cmp -s - "$FIXTURE_DIR/pixel.png" \
    || fail "the loopback server does not serve the fixture PNG"
}

app_start() { # worktree
  rm -f "$SIRIO_SOCKET"
  (cd "$1" && exec env -u WAYLAND_DISPLAY -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u all_proxy \
    NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 \
    ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} "$BIN" >>"$LEG/app.log" 2>&1) &
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

ctl() {
  echo "+ sirioctl $*"
  printf '%s\t%s\n' "$LEG_NAME" "$*" >>"$OUT_DIR/.ctl.tsv"
  timeout -k 5 30 "$CTL" "$@"
}
reply() { timeout -k 5 30 "$CTL" "$@" --json; }
field() { python3 -I -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
wait_for() { # key value sirioctl-args...
  local key=$1 want=$2 got=""
  shift 2
  for _ in $(seq 1 150); do
    got="$(reply "$@" 2>/dev/null | field "$key" 2>/dev/null || true)"
    [ "$got" = "$want" ] && { echo "OK: $key=$want"; return 0; }
    sleep 0.3
  done
  reply "$@" || true
  fail "$key never became '$want' (last: '$got') for: $*"
}

# Opening a .md file raises a card offering to install marksman, which would sit over the
# Preview in every frame. The offer's own "Don't ask again" is the setting
# lsp.silencedLanguages, written here to the leg's scratch database; the app is restarted to read it.
silence_markdown_server_offer() { # worktree
  terminate "$APP_PID"
  APP_PID=""
  stop_hosts
  python3 -I - "$SIRIO_DB" <<'PY2'
import sqlite3, sys
connection = sqlite3.connect(sys.argv[1])
connection.execute("INSERT OR REPLACE INTO setting (key, value) VALUES ('lsp.silencedLanguages', '[\"markdown\", \"Markdown\"]')")
connection.commit()
connection.close()
PY2
  app_start "$1"
}

select_project() { # worktree
  ctl project add "$1" >/dev/null 2>&1 || true
  for _ in $(seq 1 40); do
    timeout -k 5 30 "$CTL" select-workspace --workspace "$1" >/dev/null 2>&1 && break
    sleep 0.5
  done
  local current
  current="$(timeout -k 5 30 "$CTL" current-workspace --json 2>&1 || true)"
  case "$current" in *"$1"*) ;; *) fail "the repository is not the selected workspace: $current" ;; esac
  echo "OK: the repository is the selected workspace"
}

# ---------------------------------------------------------------- capture

find_window() { # pid
  DISPLAY_TARGET="$DISPLAY_TARGET" WANT_PID="$1" python3 -I - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["WANT_PID"])
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

WINDOW=""
size_window() { # waits for the app's window and sets it to 1600x1000
  for _ in $(seq 1 100); do
    WINDOW="$(find_window "$APP_PID")"
    [ -n "$WINDOW" ] && break
    sleep 0.2
  done
  [ -n "$WINDOW" ] || fail "no window with _NET_WM_PID=$APP_PID"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowsize "$WINDOW" "${WINDOW_SIZE%x*}" "${WINDOW_SIZE#*x}" \
    || fail "could not resize the window"
  sleep 2
}

shoot() { # destination file
  local colours dims
  import -display "$DISPLAY_TARGET" -window "$WINDOW" "$1"
  [ -s "$1" ] || fail "the capture $1 is empty"
  dims="$(identify -format '%wx%h' "$1")"
  [ "$dims" = "$WINDOW_SIZE" ] || fail "$1 is $dims, not $WINDOW_SIZE"
  colours="$(identify -format '%k' "$1")"
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
}

# Top to bottom: the page is first sent back to its top, then one frame per step of the wheel until
# a step changes nothing. $2 and $3 are where the pointer rests, in percent of the window.
scroll_capture() { # prefix xpercent ypercent
  local prefix=$1 x y k=1 previous="" frame
  x=$((${WINDOW_SIZE%x*} * $2 / 100))
  y=$((${WINDOW_SIZE#*x} * $3 / 100))
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove --window "$WINDOW" "$x" "$y"
  DISPLAY="$DISPLAY_TARGET" timeout 60 xdotool click --repeat 400 --delay 5 4
  sleep 1.5
  while [ "$k" -le "$MAX_FRAMES" ]; do
    frame="$OUT_DIR/$prefix-$k.png"
    shoot "$frame"
    if [ -n "$previous" ]; then
      if [ "$(compare -metric AE -fuzz 1% "$previous" "$frame" null: 2>&1 | awk '{print $1; exit}' || true)" = "0" ]; then
        rm -f "$frame"
        break
      fi
    fi
    echo "FRAME: $prefix-$k ($(identify -format '%k' "$frame") colours)"
    printf '%s\t%s\t%s\n' "$prefix" "$prefix-$k.png" "$(sha256sum "$frame" | cut -d' ' -f1)" >>"$OUT_DIR/.frames.tsv"
    FRAMES=$((FRAMES + 1))
    previous="$frame"
    k=$((k + 1))
    DISPLAY="$DISPLAY_TARGET" timeout 30 xdotool click --repeat "$SCROLL_CLICKS" --delay 25 5
    sleep 1.5
  done
  [ "$k" -gt 3 ] || fail "$prefix: only $((k - 1)) frames: the wheel did not scroll the page"
  [ "$k" -le "$MAX_FRAMES" ] || fail "$prefix: still scrolling after $MAX_FRAMES frames"
}

# ---------------------------------------------------------------- leg 1: the file Preview

if wants preview; then
  echo "== preview"
  LEG_NAME=preview
  new_world preview
  WT="$LEG/repo"
  start_image_server
  prepare_showcase "$WT"
  git -C "$WT" init -q -b main
  git -C "$WT" config user.email tester@example.invalid
  git -C "$WT" config user.name Tester
  git -C "$WT" add -A
  git -C "$WT" commit -q -m first
  app_start "$WT"
  silence_markdown_server_offer "$WT"
  select_project "$WT"
  size_window
  ctl surface editor open --path="$WT/showcase.md" >/dev/null
  STATE=""
  for _ in $(seq 1 100); do
    STATE="$(timeout -k 5 30 "$CTL" surface editor state 2>&1 || true)"
    printf '%s\n' "$STATE" | grep -q '^state=loaded$' && break
    sleep 0.1
  done
  printf '%s\n' "$STATE" | grep -q '^state=loaded$' || fail "showcase.md never loaded: $STATE"
  ctl surface editor mode --mode=preview >/dev/null
  for _ in $(seq 1 100); do
    STATE="$(timeout -k 5 30 "$CTL" surface editor state 2>&1 || true)"
    printf '%s\n' "$STATE" | grep -q '^mode=preview$' && break
    sleep 0.1
  done
  printf '%s\n' "$STATE" | grep -q '^mode=preview$' || fail "the file never reached Preview: $STATE"
  echo "OK: showcase.md is open in Preview"
  sleep 3 # the remote picture and the diagrams arrive after the first paint
  scroll_capture preview "${PREVIEW_X:-55}" "${PREVIEW_Y:-50}"
  cp "$LEG/app.log" "$OUT_DIR/logs/preview-app.log" 2>/dev/null || true
  cp "$LEG/http-server.log" "$OUT_DIR/logs/preview-http-server.log" 2>/dev/null || true
  stop_app
fi

# ---------------------------------------------------------------- leg 2: a chat reply

if wants chat; then
  echo "== chat"
  LEG_NAME=chat
  new_world chat
  REPLY_DIR="$LEG/reply"
  prepare_showcase "$REPLY_DIR"
  start_image_server
  CHAT_FRAMES="$OUT_DIR/.chat-frames.tsv"
  : >"$CHAT_FRAMES"
  # The runner of test-ely-chat-ui-e2e.py, imported: its isolated instance, scripted agent and
  # window capture are the ones the chat E2E uses.
  python3 -I - "$ROOT" "$BIN" "$DISPLAY_TARGET" "$OUT_DIR" "$REPLY_DIR" "$CHAT_FRAMES" "$SCROLL_CLICKS" "$MAX_FRAMES" "$WINDOW_SIZE" <<'PY' \
    || fail "the chat leg failed (see $OUT_DIR/logs/chat-run)"
import argparse, hashlib, importlib.util, shutil, subprocess, sys, time
from pathlib import Path

root, sirio, display, out, reply_dir, frames_tsv, clicks, max_frames, window_size = sys.argv[1:10]
root, out, reply_dir = Path(root), Path(out), Path(reply_dir)
clicks, max_frames = int(clicks), int(max_frames)
spec = importlib.util.spec_from_file_location("chatui", root / "Scripts/Tests/test-ely-chat-ui-e2e.py")
chatui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(chatui)

args = argparse.Namespace(out_dir=str(out / "logs" / "chat-run"), state_only=False, display=display,
                          window_size=window_size, sirio_bin=sirio)
run = chatui.Run(args)
failed = None
try:
    run.preflight()
    scenario = chatui.Scenario(run, "markdown-reply", "reply-file")
    shutil.copy(reply_dir / "showcase.md", scenario.agent_dir / "reply.md")
    shutil.copy(reply_dir / "pixel.png", scenario.repo / "pixel.png")
    chat = scenario.instance
    chat.start()
    chat.send("show the showcase")
    chat.until("the turn completes with the showcase",
               lambda s: s["status"] == "completed" and "Markdown showcase" in "".join(chatui.texts(s, "assistant")))
    time.sleep(3)  # pictures and diagrams arrive after the first paint
    pid = chat.process.pid
    chatui.scroll_pane(run, pid, 400, "4")  # to the top of the transcript
    previous, k = None, 1
    while k <= max_frames:
        destination = run.out / "frames" / f"chat-{k}.png"
        run.capture(pid, destination, settle=0.5)
        if previous is not None:
            compared = subprocess.run(["compare", "-metric", "AE", "-fuzz", "1%", str(previous), str(destination), "null:"],
                                      capture_output=True, text=True)
            if compared.stderr.strip().split()[0] == "0":
                destination.unlink()
                break
        shutil.copy(destination, out / f"chat-{k}.png")
        sha = hashlib.sha256(destination.read_bytes()).hexdigest()
        with open(frames_tsv, "a") as handle:
            handle.write(f"chat\tchat-{k}.png\t{sha}\n")
        previous = destination
        k += 1
        chatui.scroll_pane(run, pid, clicks, "5")
    if k <= 3:
        raise chatui.Failure(f"chat: only {k - 1} frames: the wheel did not scroll the transcript")
    if k > max_frames:
        raise chatui.Failure(f"chat: still scrolling after {max_frames} frames")
    scenario.finish()
except BaseException as error:
    failed = error
    run.log(f"FAIL: {error}")
finally:
    for instance in run.instances:
        instance.quit()
    chatui.stop_hosts(run)
    shutil.copy(run.work / "markdown-reply" / "app.log", out / "logs" / "chat-app.log") if (run.work / "markdown-reply" / "app.log").exists() else None
if failed is not None:
    sys.exit(1)
PY
  # The chat leg's frames are counted from what it recorded.
  FRAMES=$((FRAMES + $(wc -l <"$CHAT_FRAMES")))
  cat "$CHAT_FRAMES" >>"$OUT_DIR/.frames.tsv"
  echo "OK: the chat reply was captured ($(wc -l <"$CHAT_FRAMES") frames)"
  stop_app
fi

# ---------------------------------------------------------------- leg 3: a change request description

if wants cr-description; then
  echo "== cr-description"
  LEG_NAME=cr-description
  new_world cr-description
  # The fake forge's fixtures, copied with the description of #101 replaced by the showcase.
  FIXTURES="$LEG/forge-fixtures"
  cp -r "$ROOT/Scripts/Tests/forge-fixtures" "$FIXTURES"
  prepare_showcase "$LEG/showcase"
  python3 -I - "$FIXTURES/github" "$LEG/showcase/showcase.md" <<'PY'
import json, sys
from pathlib import Path
directory, showcase = Path(sys.argv[1]), Path(sys.argv[2]).read_text()
changed = 0
for name in ("ChangeRequestHeader.json", "ChangeRequestByNumber.json"):
    path = directory / name
    document = json.loads(path.read_text())
    document["data"]["repository"]["pullRequest"]["body"] = showcase
    path.write_text(json.dumps(document))
    changed += 1
print(f"description replaced in {changed} fixtures")
PY
  FORGE_PORT="$(free_port)"
  python3 -I "$ROOT/Scripts/Tests/fake_forge.py" --flavor github --port "$FORGE_PORT" --fixtures "$FIXTURES" \
    --log "$LEG/github-requests.log" 2>"$LEG/forge-server.log" &
  FORGE_PID=$!
  for _ in $(seq 1 50); do
    curl --noproxy "*" -s -o /dev/null --max-time 1 "http://127.0.0.1:$FORGE_PORT/api/v3/meta" && break
    sleep 0.2
  done
  # The picture's loopback server too, so the remote picture of the description is reachable.
  start_image_server
  export SIRIO_FORGE_TEST_ENDPOINTS="ghe.test=http://127.0.0.1:$FORGE_PORT"
  export GH_CONFIG_DIR="$LEG/gh" GLAB_CONFIG_DIR="$LEG/glab"
  mkdir -p "$GH_CONFIG_DIR" "$GLAB_CONFIG_DIR"
  chmod 700 "$GLAB_CONFIG_DIR"
  unset GH_TOKEN GITHUB_TOKEN GITLAB_TOKEN || true
  WT="$LEG/widgets"
  mkdir -p "$WT"
  git -C "$WT" init -q -b main
  git -C "$WT" config user.email t@example.com
  git -C "$WT" config user.name Tester
  git -C "$WT" commit -q --allow-empty -m first
  git -C "$WT" checkout -q -b feat/login
  git -C "$WT" remote add origin https://ghe.test/acme/widgets.git
  app_start "$WT"
  select_project "$WT"
  size_window
  ctl surface change-requests show >/dev/null
  wait_for state not-connected surface change-requests read
  account="$(reply surface change-requests token --host ghe.test --forge github --token $'good\n' | field account)"
  [ "$account" = "fake-user" ] || fail "the token signed in as '$account'"
  wait_for state ready surface change-requests read
  ctl surface change-request open 101 >/dev/null
  wait_for state loaded surface change-request read
  wait_for title "Fix the login redirect" surface change-request read
  sleep 3
  scroll_capture cr-description "${CR_X:-55}" "${CR_Y:-50}"
  cp "$LEG/app.log" "$OUT_DIR/logs/cr-description-app.log" 2>/dev/null || true
  cp "$LEG/github-requests.log" "$OUT_DIR/logs/cr-description-forge-requests.log" 2>/dev/null || true
  stop_app
fi

# ---------------------------------------------------------------- the transcript

python3 -I - "$OUT_DIR" "$BIN_DIR" "$CTL" "$WINDOW_SIZE" "$SCROLL_CLICKS" "$DISPLAY_TARGET" <<'PY'
import hashlib, json, sys
from pathlib import Path
out, bin_dir, ctl, window_size, clicks, display = sys.argv[1:7]
out = Path(out)
def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()
calls = [dict(zip(("leg", "call"), line.rstrip("\n").split("\t", 1))) for line in (out / ".ctl.tsv").read_text().splitlines()]
frames = [dict(zip(("surface", "file", "sha256"), line.split("\t"))) for line in (out / ".frames.tsv").read_text().splitlines()]
document = {
    "binaries": {"sirio": sha(Path(bin_dir) / "sirio"), "sirio-host": sha(Path(bin_dir) / "sirio-host"),
                 "sirioctl": ctl},
    "window_size": window_size, "scale_factor": 1, "scroll_clicks_per_step": int(clicks), "display": display,
    "sirioctl_calls": calls, "frames": frames,
}
(out / "transcript.json").write_text(json.dumps(document, indent=2) + "\n")
print(len(frames))
PY
rm -f "$OUT_DIR/.ctl.tsv" "$OUT_DIR/.frames.tsv" "$OUT_DIR/.chat-frames.tsv"

echo "artifact: $OUT_DIR"
echo "MARKDOWN SURFACES CAPTURED ($FRAMES frames)"
