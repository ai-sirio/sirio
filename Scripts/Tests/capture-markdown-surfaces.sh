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
#                    answers every prompt with showcase.md (the harness of test-ely-chat-ui-e2e.py);
#                    then the states of the richer chat messages (spec 2026-11-21-ely-chat-richer-messages-
#                    design.md): a day separator, a limit card counting, a failed turn (and a second one,
#                    and a queue behind one), a thought that settled before its reply, and a reply in
#                    mid-reveal, each in a world of its own started by the driver of
#                    test-chat-richer-messages-e2e.sh (the stub `claude` of Scripts/Tests/stubs/richer),
#                    through the socket methods a debug build of 0.47.1 already served, so the two builds
#                    that are compared are driven the same way. Run both with debug builds.
#   cr-description   the detail tab of change request #101 on the loopback fake forge
#                    (the fixtures of test-forge-ui-e2e.sh), its description being showcase.md.
#
# Each surface is captured top to bottom: the wheel is turned over the page in steps of
# $SCROLL_CLICKS clicks until a step changes nothing. A frame is a whole window, so the page's
# chrome is in every one of them; the page is the part that moves.
#
# Files written to --out-dir:
#   preview-<k>.png, chat-<k>.png, cr-description-<k>.png   the scrolled pages, k from 1
#   state-<name>.png                                        the chat's states: day-separators (and
#                                                           -top), limit-card, turn-failed, turn-failed-second,
#                                                           held-queue, thought-settled, mid-reveal-<k>
#   chat-states.json                                        what the app's readout said at each state's
#                                                           frame (a build without a field leaves it out)
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
# The mid-reveal frames are taken when the reply has arrived up to each size in $REVEAL_AT (bytes,
# comma list, default 1800,6000,12000), and fail if the reply had ended by the time the frame was shot.
#
# The state-* frames depend on the date and on timing, so a re-run differs in them: the day separators'
# labels follow the date the script runs on (Today, Yesterday, and the weekday and month of the date
# 17 days back), a limit card's reset time follows the clock, the turn footers show the time of day, and
# how far the reveal has come in a mid-reveal frame moves with the clock.
#
# Needs, from the checkout's own build: rust/target/debug/examples/ely_chat_seed (chat) and
# sirioctl when --bin-dir has none. The chat's state-* frames also need Mesa's lavapipe
# (lvp_icd*.json in /usr/share/vulkan/icd.d): the script stops without it.

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
REVEAL_AT="${REVEAL_AT:-1800,6000,12000}"
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
  # The states of the richer chat messages. Each runs in a world of its own started by the driver of
  # test-chat-richer-messages-e2e.sh (imported: its `World` is the isolated instance, the stub `claude` of
  # Scripts/Tests/stubs/richer the agent). Only what a debug build of 0.47.1 already served is called (the
  # chat's open, send, read and tabs), so a build from before this delivery is driven the same way; what a
  # state looks like on each build is what the two frames show. The window is captured the moment the state
  # is read.
  echo "== chat states"
  [ -n "$lvp" ] || fail "the chat states need Mesa's lavapipe (lvp_icd*.json in /usr/share/vulkan/icd.d)"
  case "$REVEAL_AT" in "" | *[!0-9,]*) fail "REVEAL_AT is a comma list of byte counts (got '$REVEAL_AT')" ;; esac
  STATES_DIR="$RUN_DIR/cs"
  mkdir -p "$STATES_DIR/out/frames" "$STATES_DIR/out/scenarios" "$STATES_DIR/out/logs"
  STATE_FRAMES="$OUT_DIR/.state-frames.tsv"
  : >"$STATE_FRAMES"
  env ROOT="$ROOT" RUN_DIR="$STATES_DIR" OUT="$STATES_DIR/out" DISP="$DISPLAY_TARGET" BIN="$BIN" CTL="$CTL" \
    WINDOW_SIZE="$WINDOW_SIZE" VK_DRIVER_FILES="$lvp" VK_ICD_FILENAMES="$lvp" \
    python3 -I - "$ROOT" "$OUT_DIR" "$STATE_FRAMES" "$REVEAL_AT" "$WINDOW_SIZE" <<'PY' \
    || fail "the chat states failed (see $OUT_DIR/logs/chat-states)"
import hashlib, importlib.util, json, os, sqlite3, subprocess, sys, time
from datetime import datetime, timedelta, timezone
from pathlib import Path

root, out_dir, frames_tsv, reveal_at, window_size = sys.argv[1:6]
out_dir = Path(out_dir)
spec = importlib.util.spec_from_file_location("richer", Path(root) / "Scripts/Tests/chat_richer_messages_e2e.py")
richer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(richer)
display = os.environ["DISP"]
scenario = richer.Scenario("s")
states = {}
worlds = []


def world(name):
    made = richer.World(scenario, name)
    worlds.append(made)
    return made


def user_texts(r):
    return [row.get("text") for row in r["rows"] if row.get("kind") == "user"]


def assistant_bytes(r):
    return sum(len((row.get("text") or "").encode()) for row in r["rows"] if row.get("kind") == "assistant")


def turn_over(text, rows):
    """The turn of `text` was asked and is over (a failed turn leaves at least its user row, the reply and one more)."""
    return lambda r: text in user_texts(r) and r["status"] != "streaming" and len(r["rows"]) >= rows


def shoot(made, name, read):
    """The window, right now, as state-<name>.png, with what the readout said just before it."""
    destination = out_dir / f"state-{name}.png"
    subprocess.run(["import", "-display", display, "-window", made.window, str(destination)], check=True, timeout=30)
    size = subprocess.run(["identify", "-format", "%wx%h", str(destination)], capture_output=True, text=True, check=True).stdout
    colours = int(subprocess.run(["identify", "-format", "%k", str(destination)], capture_output=True, text=True, check=True).stdout)
    if size != window_size:
        raise richer.Fail(f"{destination.name} is {size}, not {window_size}")
    if colours < 200:
        raise richer.Fail(f"{destination.name} is blank ({colours} colours)")
    sha = hashlib.sha256(destination.read_bytes()).hexdigest()
    with open(frames_tsv, "a") as handle:
        handle.write(f"chat-state\tstate-{name}.png\t{sha}\n")
    states[name] = {"file": destination.name, "readout": richer.compact(read)}
    richer.log(f"FRAME: state-{name} ({colours} colours; pid {made.proc.pid})")


def settle(seconds=2.0):
    time.sleep(seconds)


try:
    # ---- a day separator: user turns on three days, written as the app stores them
    w = world("day")
    w.start()
    w.open_chat("claude")
    surface = w.surface
    w.quit()
    now = datetime.now(timezone.utc)
    old = datetime(now.year - 1, 3, 12, 12, 0, tzinfo=timezone.utc)
    yesterday = (now - timedelta(days=1)).replace(hour=12, minute=0, second=0, microsecond=0)
    earlier = (now - timedelta(days=17)).replace(hour=12, minute=0, second=0, microsecond=0)
    turns = [("a question from last year", int(old.timestamp())), ("a question from before", int(earlier.timestamp())),
             ("yesterday's question", int(yesterday.timestamp())), ("today's question", int(now.timestamp()))]
    db = sqlite3.connect(w.dir / "session.sqlite")
    try:
        if db.execute("SELECT id FROM tab WHERE id = ?", (surface,)).fetchone() is None:
            raise richer.Fail(f"the chat tab {surface} is not in the database")
        for ordinal, (text, at) in enumerate(turns):
            payload = json.dumps({"entries": [{"UserMessage": {"text": text, "at": at}}, {"AssistantMessage": {"text": f"the answer to {text}"}},
                                              {"TurnFooter": {"text": "12:00"}}]})
            db.execute("INSERT INTO chat_turn (tab_id, ordinal, payload, updated_at) VALUES (?, ?, ?, ?)",
                       (surface, ordinal, payload.encode(), int(time.time() * 1000)))
        db.commit()
    finally:
        db.close()
    w.start()
    w.reopen_chat()
    r = w.wait("the four turns are restored", lambda r: len(user_texts(r)) == 4)
    settle()
    shoot(w, "day-separators", w.read())
    richer.move(w.window, 1090, 450)
    richer.x("click", "--repeat", "60", "--delay", "5", "4")  # the wheel, up to the first turn
    settle(1.5)
    shoot(w, "day-separators-top", w.read())
    w.keep_artifacts()
    w.quit()

    # ---- a usage limit reached: a rejected rate limit five minutes ahead, and the turn it failed
    w = world("lim")
    w.start()
    w.open_chat("claude")
    w.limit_file.write_text(str(int(time.time()) + 300))
    w.send("limit rejected")
    w.wait("the turn is over", turn_over("limit rejected", 3), timeout=40)
    settle()
    shoot(w, "limit-card", w.read())
    w.keep_artifacts()
    w.quit()

    # ---- a failed turn, then a second one (the first one's Retry is then retired)
    w = world("fail")
    w.start()
    w.open_chat("claude")
    w.send("fail")
    w.wait("the turn is over", turn_over("fail", 3), timeout=40)
    settle()
    shoot(w, "turn-failed", w.read())
    w.send("fail hold")
    w.release_gate(1)
    w.wait("the second turn is over", lambda r: "fail hold" in user_texts(r) and r["status"] != "streaming"
           and len(r["rows"]) >= 6, timeout=40)
    settle()
    shoot(w, "turn-failed-second", w.read())
    w.keep_artifacts()
    w.quit()

    # ---- two messages queued behind a turn that fails: held since this delivery, sent before it
    w = world("held")
    w.start()
    w.open_chat("claude")
    w.send("fail hold")
    w.wait("the turn streams", lambda r: r["status"] == "streaming")
    w.send("first queued", queued=True)
    w.send("second queued", queued=True)
    w.wait("two messages are queued", lambda r: r["queue"] == ["first queued", "second queued"])
    w.release_gate(1)
    w.wait("the turn is over", lambda r: r["status"] != "streaming" and "fail hold" in user_texts(r)
           and len(r["rows"]) >= 3, timeout=40)
    settle(3.0)
    shoot(w, "held-queue", w.read())
    w.keep_artifacts()
    w.quit()

    # ---- a thought whose block closed 2 s before its reply's text
    w = world("think")
    w.start()
    w.open_chat("claude")
    w.send("think slow")
    w.wait("the turn streams", lambda r: r["status"] == "streaming")
    # The stub closes its thinking block 0.45 s in and sends the text at 2.45 s. A build that reports a settled thought
    # (`durationMs`) is shot as soon as it has: it must come within 3 s, before the text. A build from before this
    # delivery reports none, and is shot at 1.2 s, where its thought is still open.
    started = time.time()
    while True:
        r = w.read()
        modern = "held" in r  # a field only a build with this delivery's readouts has
        settled = any("durationMs" in row for row in r["rows"] if row.get("kind") == "thought")
        if settled or (not modern and time.time() - started > 1.2):
            break
        if time.time() - started > 3:
            raise richer.Fail("the thought never settled (no durationMs within 3 s)")
        time.sleep(0.05)
    time.sleep(0.3)  # the frame that draws it
    shoot(w, "thought-settled", w.read())
    w.keep_artifacts()
    w.quit()

    # ---- a reply in mid-reveal, at each size it has reached
    w = world("rev")
    w.start()
    w.open_chat("claude")
    w.send("showcase slow")
    for step, wanted in enumerate((int(size) for size in reveal_at.split(",")), start=1):
        deadline = time.time() + 60
        while True:
            try:
                r = w.read()
            except richer.RpcError as stalled:  # a debug build that builds the whole reply per chunk is slow to answer
                richer.log(f"note: a read was not answered ({stalled})")
                r = None
            if r is not None and assistant_bytes(r) >= wanted:
                break
            if time.time() > deadline:
                raise richer.Fail(f"the reply never reached {wanted} bytes")
            time.sleep(0.05)
        shoot(w, f"mid-reveal-{step}", r)
        states[f"mid-reveal-{step}"]["received_bytes"] = assistant_bytes(r)
        # The frame is of a reply that was still streaming (and, where the build says so, live) when it was shot.
        after = w.read()
        last = [row for row in after["rows"] if row.get("kind") == "assistant"][-1]
        if after["status"] != "streaming" or last.get("live", "true") != "true":
            raise richer.Fail(f"mid-reveal-{step}: the reply had ended when the frame was shot (status {after['status']}, live {last.get('live')})")
    w.wait("the reply ends", lambda r: r["status"] != "streaming", timeout=90)
    w.keep_artifacts()
    w.quit()
    status = 0
except BaseException as error:
    richer.log(f"FAIL: {error!r}")
    status = 1
finally:
    for made in worlds:
        try:
            made.keep_artifacts()
        except Exception:
            pass
        made.quit()
    richer.sweep()
    (out_dir / "chat-states.json").write_text(json.dumps(states, indent=2) + "\n")
    logs = out_dir / "logs" / "chat-states"
    logs.mkdir(parents=True, exist_ok=True)
    for kept in (Path(os.environ["OUT"]) / "logs").glob("*"):
        (logs / kept.name).write_bytes(kept.read_bytes())
    for kept in (Path(os.environ["OUT"]) / "scenarios").glob("*.stub.log"):
        (logs / kept.name).write_bytes(kept.read_bytes())
sys.exit(status)
PY
  FRAMES=$((FRAMES + $(wc -l <"$STATE_FRAMES")))
  cat "$STATE_FRAMES" >>"$OUT_DIR/.frames.tsv"
  echo "OK: the chat's states were captured ($(wc -l <"$STATE_FRAMES") frames)"
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
rm -f "$OUT_DIR/.ctl.tsv" "$OUT_DIR/.frames.tsv" "$OUT_DIR/.chat-frames.tsv" "$OUT_DIR/.state-frames.tsv"

echo "artifact: $OUT_DIR"
echo "MARKDOWN SURFACES CAPTURED ($FRAMES frames)"
