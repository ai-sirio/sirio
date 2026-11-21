#!/bin/bash
set -eEuo pipefail

# End-to-end test of Markdown on ely-documents (spec 2026-11-20-ely-markdown-documents-design.md
# §6.2): the real, isolated debug Sirio on a private Xvfb, opening one document that holds every
# kept and added feature (Scripts/Tests/fixtures/markdown-documents/showcase.md, plus a link to
# a late section and that section), driven by real X input and read through the debug state
# (`sirioctl surface editor state`: the Preview's node counts, scroll, drawn texts, drawn links,
# outline and diagram renders) and the display's real clipboard (GTK, python3-gi).
#
# What it proves, in order (the name before each OK line is the check's name):
#   draws             the Preview's tree holds math, five alerts, a footnote, pictures, a table, a
#                     code block and a table of contents (node counts); the page scrolls
#   anchor-toc        a click on a `[[toc]]` entry brings the second "Second section" (slug
#                     second-section-1) to the top of the page, not the first
#   anchor-slug       a click on a `#late-section` link brings that heading to the top
#   anchor-outline    the outline toggle opens the panel; from the bottom of the page a click on
#                     its row for the second "Second section" brings that heading to the top
#   progress          the reading-progress share grows with the scroll
#   picture-width     `<img width="120">` draws as a 120 logical pixel square (a capture scan)
#   diagrams          the Mermaid fence is a picture rendered into the cache directory; PlantUML
#                     is one when `plantuml` is on PATH, else `SKIP plantuml (not on PATH)`
#   copy-drag         a real drag across rows and Ctrl+C put the expected text on the clipboard:
#                     the emoji turned, the TeX of the math, the alert titles, the footnote
#   copy-menu         the right-click menu's Copy puts the same text on the clipboard
#   terminal-ctrl-c   with that selection made, Ctrl+C in a terminal pane still reaches the PTY
#                     (its screen shows ^C) and leaves the clipboard alone
#   chat-select-all   a chat reply of at least 40 KiB, drawn only near the window (the early blocks
#                     are culled at the tail), copied whole through the transcript menu's Select
#                     All and Copy: the clipboard equals the transcript text; then a jump to the
#                     top of the reply draws its first blocks, none of them blank
#
# --mutate no-sections starts the app with SIRIO_DEBUG_MARKDOWN_NO_SECTIONS (a debug build's
# knob: the Preview is drawn without `sections`, so no anchor registers). The run must FAIL at
# an anchor check.
#
# The artifact: --out-dir DIR (default artifacts/ely-md/e2e) keeps transcript.json (the checks,
# the sirioctl calls, the frames), frames/*.png (PID-matched window captures), the clipboards and
# the expected texts (clipboard-*.txt, expected-*.txt) and logs/. Rerunning the script
# reproduces it. It prints MARKDOWN DOCUMENTS E2E OK (<n> checks, ...).
#
# Usage: Scripts/Tests/test-markdown-documents-e2e.sh [--bin-dir DIR] [--out-dir DIR]
#          [--display :N] [--mutate no-sections]
#   --bin-dir DIR   the directory holding sirio and sirio-host (default rust/target/debug)
#   --display :N    the Xvfb this script starts and stops, on :120 to :139 (default :120)
#
# Needs: python3 with gi (GTK 3) for the clipboard, xdotool, xwininfo, xprop, xdpyinfo, import,
# identify, magick, curl, Xvfb, and from the checkout's own build rust/target/debug/sirioctl and
# rust/target/debug/examples/ely_chat_seed (cd rust && cargo build -p sirio -p sirio_control
# --bin sirioctl && cargo build -p sirio_persistence --example ely_chat_seed).
# Linux and X11 only: its input and its captures are X's.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
FIXTURE_DIR="$ROOT/Scripts/Tests/fixtures/markdown-documents"

BIN_DIR="$TARGET_DIR/debug"
OUT_DIR="$ROOT/artifacts/ely-md/e2e"
DISPLAY_TARGET=":120"
MUTATE=""
WINDOW_SIZE="1600x1000"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin-dir) BIN_DIR="${2:?--bin-dir needs a directory}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --mutate) MUTATE="${2:?--mutate needs a name}"; shift 2 ;;
    -h|--help) sed -n '3,60p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$MUTATE" in ""|no-sections) ;; *) echo "--mutate is no-sections" >&2; exit 2 ;; esac
BIN_DIR="$(cd "$BIN_DIR" && pwd)"
mkdir -p "$OUT_DIR/logs" "$OUT_DIR/frames"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

for tool in python3 git xdotool xwininfo xprop xdpyinfo import identify magick curl Xvfb; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
[ "$(id -u)" -ne 0 ] || fail "run as a user, not root"
case "$DISPLAY_TARGET" in
  :12[0-9] | :13[0-9]) ;;
  *) fail "--display must be between :120 and :139 (got $DISPLAY_TARGET)" ;;
esac
[ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY"
python3 -I -c 'import gi; gi.require_version("Gtk", "3.0"); from gi.repository import Gtk' 2>/dev/null \
  || fail "python3 with gi (GTK 3) is required to read and own the clipboard"

BIN="$BIN_DIR/sirio"
[ -x "$BIN" ] || fail "no sirio in $BIN_DIR"
[ -x "$BIN_DIR/sirio-host" ] || fail "no sirio-host in $BIN_DIR"
CTL="$BIN_DIR/sirioctl"
[ -x "$CTL" ] || CTL="$TARGET_DIR/debug/sirioctl"
[ -x "$CTL" ] || fail "no sirioctl in $BIN_DIR or $TARGET_DIR/debug (cd rust && cargo build -p sirio_control --bin sirioctl)"
SEED="$TARGET_DIR/debug/examples/ely_chat_seed"
[ -x "$SEED" ] || fail "no $SEED (cd rust && cargo build -p sirio_persistence --example ely_chat_seed)"

# The run's scratch space is on disk: /tmp is a tmpfs.
BASE="${SIRIO_E2E_TMP:-${TMPDIR:-$HOME/.cache/st/rail-e2e-tmp}}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/mddocs-XXXXXX")"
APP_PID=""
XVFB_PID=""
SERVER_PID=""
: >"$OUT_DIR/.ctl.tsv"
: >"$OUT_DIR/.checks.jsonl"
LEG_NAME=setup
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
Xvfb "$DISPLAY_TARGET" -screen 0 2400x1200x24 >"$OUT_DIR/logs/xvfb.log" 2>&1 &
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
    ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} ${APP_ENV[@]+"${APP_ENV[@]}"} "$BIN" >>"$LEG/app.log" 2>&1) &
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


# ---------------------------------------------------------------- the document

# The showcase, as every surface is compared on it, plus what the anchor checks need: a link to a
# section far below the top, and that section followed by a page of text so that it can reach the top.
build_document() { # destination directory
  mkdir -p "$1"
  python3 -I - "$FIXTURE_DIR" "$1" "$PORT" <<'PY'
import sys
from pathlib import Path
fixture, destination, port = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
text = (fixture / "showcase.md").read_text().replace("{{REMOTE_PORT}}", port)
assert text.count("(#second-section) below.") == 1
text = text.replace("(#second-section) below.", "(#second-section) below. Or [jump to the late section](#late-section).")
text += "\n## Late section\n\n" + "".join(f"Late line {n}: the section sits far below the top.\n\n" for n in range(1, 61))
(destination / "documents.md").write_text(text)
PY
  cp "$FIXTURE_DIR/pixel.png" "$1/pixel.png"
}

APP_ENV=()
[ "$MUTATE" != no-sections ] || APP_ENV=(SIRIO_DEBUG_MARKDOWN_NO_SECTIONS=1)

# ---------------------------------------------------------------- leg 1: the file Preview

LEGS="${MARKDOWN_E2E_LEGS:-preview chat}" # a development knob: one leg alone
if [[ " $LEGS " == *" preview "* ]]; then
echo "== preview (mutate: ${MUTATE:-none})"
LEG_NAME=preview
new_world preview
WT="$LEG/repo"
start_image_server
build_document "$WT"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
git -C "$WT" add -A
git -C "$WT" commit -q -m first
app_start "$WT"
silence_markdown_server_offer "$WT"
select_project "$WT"
size_window
ctl surface editor open --path="$WT/documents.md" >/dev/null
STATE=""
for _ in $(seq 1 100); do
  STATE="$(timeout -k 5 30 "$CTL" surface editor state 2>&1 || true)"
  grep -q '^state=loaded$' <<<"$STATE" && break
  sleep 0.1
done
grep -q '^state=loaded$' <<<"$STATE" || fail "documents.md never loaded: ${STATE:0:300}"
ctl surface editor mode --mode=preview >/dev/null
for _ in $(seq 1 100); do
  STATE="$(timeout -k 5 30 "$CTL" surface editor state 2>&1 || true)"
  grep -q '^mode=preview$' <<<"$STATE" && break
  sleep 0.1
done
grep -q '^mode=preview$' <<<"$STATE" || fail "the file never reached Preview: ${STATE:0:300}"
echo "OK: documents.md is open in Preview"
sleep 3 # the remote picture and the diagrams arrive after the first paint

LEG_ENV=(CTL="$CTL" DISP="$DISPLAY_TARGET" WIN="$WINDOW" APP_PID="$APP_PID" OUT="$OUT_DIR" WT="$WT" RUN_DIR="$RUN_DIR"
  XDG_CACHE="$XDG_CACHE_HOME" MUTATE="$MUTATE" WINDOW_SIZE="$WINDOW_SIZE" CHECKS="$OUT_DIR/.checks.jsonl")
env "${LEG_ENV[@]}" python3 -I "$ROOT/Scripts/Tests/markdown_documents_e2e.py" preview || fail "the preview leg failed (see $OUT_DIR/logs)"
stop_app
fi

# ---------------------------------------------------------------- leg 2: a long chat reply

if [[ " $LEGS " == *" chat "* ]]; then
echo "== chat"
LEG_NAME=chat
new_world chat
start_image_server
env CTL="$CTL" DISP="$DISPLAY_TARGET" OUT="$OUT_DIR" RUN_DIR="$RUN_DIR" ROOT="$ROOT" SIRIO="$BIN" PORT="$PORT" \
  WINDOW_SIZE="$WINDOW_SIZE" CHECKS="$OUT_DIR/.checks.jsonl" \
  python3 -I "$ROOT/Scripts/Tests/markdown_documents_e2e.py" chat || fail "the chat leg failed (see $OUT_DIR/logs/chat-run)"
stop_app
fi

# ---------------------------------------------------------------- the transcript

python3 -I - "$OUT_DIR" "$BIN_DIR" "$CTL" "$WINDOW_SIZE" "$DISPLAY_TARGET" "$MUTATE" <<'PY'
import hashlib, json, sys
from pathlib import Path
out, bin_dir, ctl, window_size, display, mutate = sys.argv[1:7]
out = Path(out)
def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()
checks = [json.loads(line) for line in (out / ".checks.jsonl").read_text().splitlines()]
legs = {path.stem[5:]: json.loads(path.read_text()) for path in sorted(out.glob(".leg-*.json"))}
document = {
    "binaries": {"sirio": sha(Path(bin_dir) / "sirio"), "sirio-host": sha(Path(bin_dir) / "sirio-host"), "sirioctl": ctl},
    "window_size": window_size, "display": display, "mutate": mutate or None,
    "checks": checks,
    "setup_sirioctl_calls": [dict(zip(("leg", "call"), line.split("\t", 1))) for line in (out / ".ctl.tsv").read_text().splitlines()],
    "legs": legs,
}
(out / "transcript.json").write_text(json.dumps(document, indent=2) + "\n")
PY
read -r CHECKS_OK CHECKS_SKIPPED < <(python3 -I -c 'import json, sys; c = [json.loads(l) for l in open(sys.argv[1])]; print(sum(x["status"] == "OK" for x in c), sum(x["status"] == "SKIP" for x in c))' "$OUT_DIR/.checks.jsonl")
rm -f "$OUT_DIR/.ctl.tsv" "$OUT_DIR/.checks.jsonl" "$OUT_DIR"/.leg-*.json
for needed in draws anchor-toc anchor-slug anchor-outline progress picture-width diagrams copy-drag copy-menu terminal-ctrl-c chat-select-all chat-jump; do
  grep -q "\"check\": \"$needed\"" "$OUT_DIR/transcript.json" || fail "the run holds no result for the check $needed"
done
echo "artifact: $OUT_DIR"
echo "MARKDOWN DOCUMENTS E2E OK ($CHECKS_OK checks, $CHECKS_SKIPPED skipped, display $DISPLAY_TARGET)"
