#!/bin/bash
set -eEuo pipefail

# End-to-end test of saved prompts (docs/superpowers/specs/2026-11-01-prompt-list-design.md
# §7): a real, isolated debug Sirio on a private Xvfb, driven with real X clicks
# and keys at the bounds the debug-only `sirioctl surface prompts read` reports,
# and checked against the prompts folder itself, the X CLIPBOARD, the tab strip
# (`surface tabs read`), the editor (`surface editor state`) and the workspace
# toast (`surface project-identity read`, the verb that already exposes it).
#
# One app runs scenarios 1-10 in order, each building on the prompts the one
# before left. A second app, on a fresh database seeded dark and the same
# prompts folder, then shows the same three prompts after the restart.
#
# Scenarios (each prints `ok <n> <name>`):
#    1 empty_state               the view is empty and creates no folder; the rail opens it
#    2 new_and_type              New, a typed title, Tab, three typed lines: saved with no Ctrl+S
#    3 copy                      Copy puts the file's bytes on the CLIPBOARD and shows its check
#    4 copy_right_after_typing   Edit focuses the text; a key then Copy at once copies that key
#    5 edit_dedupes              Edit twice leaves one tab for the prompt
#    6 new_then_close_untouched  New then Ctrl+W leaves no file and no index entry
#    7 delete_with_tab_open      a key, then Delete: the file, the entry and the tab are gone for good
#    8 plain_md_unchanged        a plain .md opens in Preview and stays dirty until Ctrl+S
#    9 no_worktree               without a worktree New is off and Copy still works
#   10 unwritable                an unwritable folder: Save failed, the text stays, New raises
#                                the toast and writes nothing; the next key after the fix saves
# Then light and dark captures of the view with three prompts and of the Delete
# dialog (Escape cancels it and the prompt stays).
#
# Two traps this script avoids on purpose:
#   - The CLIPBOARD is read through GTK forced onto X11 with nothing of the
#     caller's environment: with WAYLAND_DISPLAY set, GTK would read the
#     desktop's own clipboard instead of this display's.
#   - A character the keymap lacks (“ and ” here) is typed by xtext.py, not
#     by `xdotool type`: it binds the character to a spare keycode, waits for
#     the app to take the new mapping, then presses the key through XTest.
#     xdotool binds and presses at once, and the app sometimes reads the key
#     before the mapping and drops the character.
#
# The artifact: --out-dir DIR (default artifacts/prompts-e2e-<stamp>-<pid>)
# keeps transcript.log (every sirioctl call and its reply), prompts/ (the
# prompts folder the app used: SIRIO_PROMPTS_DIR, as the run left it),
# captures/{light,dark}-*.png (PID-matched window captures) and the two
# app logs. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-prompts-e2e.sh --xvfb [--out-dir DIR]
#
#   --xvfb     start a private Xvfb on a free display between :90 and :99
#   --out-dir  where the artifact goes; it must not hold a prompts folder yet

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
SEED="$TARGET_DIR/debug/examples/appearance_seed"
PROBE="$TARGET_DIR/debug/examples/host_probe"

[ "$(uname -s)" = Linux ] || { echo "PROMPTS E2E runs on Linux only (it needs Xvfb)" >&2; exit 2; }

START_XVFB=0
OUT_DIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ "$START_XVFB" -eq 1 ] || { echo "pass --xvfb: the run needs its own display" >&2; exit 2; }
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/prompts-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/captures"
OUT_DIR="$(cd "$OUT_DIR" && pwd -P)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

SCENARIO="setup"
fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: $SCENARIO: unexpected error at line $LINENO" >&2' ERR
# Notes go to stderr: a helper whose answer is read with $(...) still logs.
note() { echo "[$SCENARIO] $*" >&2; }
ok() { # n name
  echo "ok $1 $2"
}

# The prompts folder: set here, never taken from the caller, and refused unless
# it lies under the run directory, so no run can touch real prompts.
export SIRIO_PROMPTS_DIR="$OUT_DIR/prompts"
real_prompts="$(python3 -I -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$SIRIO_PROMPTS_DIR")"
case "$real_prompts" in
  "$OUT_DIR"/*) ;;
  *) fail "SIRIO_PROMPTS_DIR ($real_prompts) is not under the run directory $OUT_DIR" ;;
esac
[ ! -e "$SIRIO_PROMPTS_DIR" ] || fail "$OUT_DIR already holds a prompts folder; pass a fresh --out-dir"
echo "prompts folder: $SIRIO_PROMPTS_DIR"

for tool in python3 git Xvfb xdpyinfo xdotool xwininfo xprop import identify; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
python3 -I -c 'import gi; gi.require_version("Gtk", "3.0")' 2>/dev/null || fail "python3-gi with GTK 3 is required (it reads the CLIPBOARD)"
python3 -I -c 'import ctypes; ctypes.CDLL("libX11.so.6"); ctypes.CDLL("libXtst.so.6")' 2>/dev/null \
  || fail "libX11 and libXtst are required (they type the characters the keymap lacks)"
TIMEOUT_BIN="$(command -v timeout)"
REAL_GIT="$(command -v git)"

# ---------------------------------------------------------------- build

echo "building sirio, sirioctl, sirio-host, the host probe and the appearance seed"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host -p sirio_host_client --example host_probe \
  -p sirio_persistence --example appearance_seed)
for binary in "$BIN" "$CTL" "$SEED" "$PROBE"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run's world

# Short, because the control socket lives in it.
BASE="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}"
mkdir -p "$BASE"
WORK="$(python3 -I -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$(mktemp -d "$BASE/prompts-XXXXXX")")"
SD=""
APP_PID=""
XVFB_PID=""
WIN=""
DISPLAY_TARGET=""

alive() { kill -0 "$1" 2>/dev/null; }
terminate() { # pid
  kill -TERM "$1" 2>/dev/null || return 0
  local n=0
  while alive "$1" && [ "$n" -lt 50 ]; do n=$((n + 1)); sleep 0.1; done
  kill -KILL "$1" 2>/dev/null || true
  wait "$1" 2>/dev/null || true
}
is_a_host() {
  case "$(cat "/proc/$1/comm" 2>/dev/null || true)" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
stop_hosts() {
  local dir pid
  for dir in "$WORK"/*/host; do
    [ -d "$dir" ] || continue
    SIRIO_HOST_HOME="$dir" "$PROBE" shutdown --force >/dev/null 2>&1 || true
  done
  for pid in $(pgrep -f "$WORK" 2>/dev/null || true); do
    is_a_host "$pid" && kill "$pid" 2>/dev/null || true
  done
}
collect_logs() {
  local dir
  for dir in "$WORK"/*/; do
    [ -f "$dir/app.log" ] && cp "$dir/app.log" "$OUT_DIR/$(basename "$dir")-app.log" 2>/dev/null || true
  done
}
cleanup() {
  local rc=$?
  set +e
  trap - ERR
  # Scenario 10 takes the folder's write bit away; the artifact keeps it.
  [ ! -d "$SIRIO_PROMPTS_DIR" ] || chmod u+w "$SIRIO_PROMPTS_DIR"
  [ -z "$APP_PID" ] || terminate "$APP_PID"
  collect_logs
  stop_hosts
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  rm -rf "$WORK"
  exit "$rc"
}
trap cleanup EXIT

for number in $(seq 90 99); do
  [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
done
[ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
Xvfb "$DISPLAY_TARGET" -screen 0 1920x1080x24 >"$OUT_DIR/xvfb.log" 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
lvp="$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)"
[ -n "$lvp" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required on Xvfb"
echo "display: $DISPLAY_TARGET"

# Stub agent CLIs (without them the app runs `claude --version` through the
# user's own wrapper) and a shell that reads no profile.
mkdir -p "$WORK/stubs" "$WORK/shell"
for cli in claude codex opencode pi omp; do
  printf '#!/bin/sh\necho "2.1.257 (Claude Code)"\n' >"$WORK/stubs/$cli"
  chmod +x "$WORK/stubs/$cli"
done
printf '#!/bin/sh\nexec /bin/bash --noprofile --norc "$@"\n' >"$WORK/shell/shell"
chmod +x "$WORK/shell/shell"

# Typing: ASCII runs through `xdotool type`, every other character through a
# spare keycode bound first and pressed once the app has had the mapping.
cat >"$WORK/xtext.py" <<'PY'
import ctypes, os, subprocess, sys, time
from ctypes import POINTER, byref, c_char_p, c_int, c_ubyte, c_uint, c_ulong, c_void_p
x = ctypes.CDLL("libX11.so.6")
xtest = ctypes.CDLL("libXtst.so.6")
x.XOpenDisplay.restype = c_void_p
x.XOpenDisplay.argtypes = [c_char_p]
x.XGetKeyboardMapping.restype = POINTER(c_ulong)
x.XGetKeyboardMapping.argtypes = [c_void_p, c_ubyte, c_int, POINTER(c_int)]
x.XChangeKeyboardMapping.argtypes = [c_void_p, c_int, c_int, POINTER(c_ulong), c_int]
x.XDisplayKeycodes.argtypes = [c_void_p, POINTER(c_int), POINTER(c_int)]
x.XSync.argtypes = [c_void_p, c_int]
x.XCloseDisplay.argtypes = [c_void_p]
xtest.XTestFakeKeyEvent.argtypes = [c_void_p, c_uint, c_int, c_ulong]
name, text = sys.argv[1], sys.argv[2]
display = x.XOpenDisplay(name.encode())
if not display:
    sys.exit("cannot open " + name)
low, high, per = c_int(), c_int(), c_int()
x.XDisplayKeycodes(display, byref(low), byref(high))
count = high.value - low.value + 1

def glyph(char):
    keysym = 0x01000000 + ord(char)
    syms = x.XGetKeyboardMapping(display, low.value, count, byref(per))
    def first(code):
        return syms[(code - low.value) * per.value]
    code = next((c for c in range(low.value, high.value + 1) if first(c) == keysym), None)
    if code is None:
        code = next(c for c in range(high.value, low.value, -1)
                    if all(syms[(c - low.value) * per.value + i] == 0 for i in range(per.value)))
        x.XChangeKeyboardMapping(display, code, 1, (c_ulong * 1)(keysym), 1)
        x.XSync(display, 0)
        # The app reloads its keymap on the mapping notify; the key waits for it.
        time.sleep(0.5)
    xtest.XTestFakeKeyEvent(display, code, 1, 0)
    xtest.XTestFakeKeyEvent(display, code, 0, 0)
    x.XSync(display, 0)
    time.sleep(0.1)

run = ""
for char in text + "\0":
    if char != "\0" and char.isascii():
        run += char
        continue
    if run:
        subprocess.run(["xdotool", "type", "--delay", "60", "--", run],
                       check=True, env={**os.environ, "DISPLAY": name})
        run = ""
    if char != "\0":
        glyph(char)
x.XCloseDisplay(display)
PY

# The project the prompts are used beside: a git repository with a plain note.
REPO="$WORK/repo"
fixture_git() {
  env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    "$REAL_GIT" "$@"
}
mkdir -p "$REPO"
fixture_git -C "$REPO" init -q -b main
fixture_git -C "$REPO" config user.email tester@example.invalid
fixture_git -C "$REPO" config user.name Tester
printf '# Notes\n\nA plain Markdown file.\n' >"$REPO/notes.md"
fixture_git -C "$REPO" add -A
fixture_git -C "$REPO" commit -q -m first

# ---------------------------------------------------------------- calling the app

with_timeout() { # seconds command...
  local seconds=$1
  shift
  "$TIMEOUT_BIN" -k 5 "$seconds" "$@"
}
ctlq() { # args...: quiet, for polled reads
  with_timeout 30 env -i PATH=/usr/bin:/bin HOME="$SD/home" SIRIO_SOCKET="$SIRIO_SOCKET" "$CTL" "$@"
}
ctl() { # args...: logged, and a failing call fails the scenario
  local out rc=0
  echo "[$SCENARIO] + sirioctl $*" >&2
  out="$(ctlq "$@" 2>&1)" || rc=$?
  if [ -n "$out" ]; then printf '%s\n' "$out" | sed "s/^/[$SCENARIO]   /" >&2; fi
  [ "$rc" -eq 0 ] || fail "$SCENARIO: sirioctl $* exited $rc"
  printf '%s\n' "$out"
}

# A verb's reply: `key=value` lines, matched whole unless the pair is
# `key~=text`, which asks the value to contain text.
STATE=""
read_verb() { # prompts|editor|identity
  case "$1" in
    prompts) ctlq surface prompts read ;;
    editor) ctlq surface editor state ;;
    identity) ctlq surface project-identity read ;;
  esac
}
value() { sed -n "s/^$1=//p" <<<"$STATE" | head -n 1; }
pair_holds() { # pair
  case "$1" in
    *'~='*) [[ "$(value "${1%%~=*}")" == *"${1#*~=}"* ]] ;;
    *) grep -Fxq -- "$1" <<<"$STATE" ;;
  esac
}
last_read() { printf '%s' "$STATE" | grep -v '^text=' | tr '\n' ' ' | cut -c1-900; }
read_state() { # verb
  STATE="$(read_verb "$1" 2>&1)" || fail "$SCENARIO: the $1 verb did not answer: $STATE"
}
DEADLINE_SECS=30
# Polls the verb until one read holds every pair; the deadline is the failure.
wait_on() { # verb pair...
  local verb=$1 deadline pair good
  shift
  deadline=$(( $(date +%s) + DEADLINE_SECS ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    STATE="$(read_verb "$verb" 2>&1)" || { sleep 0.2; continue; }
    good=1
    for pair in "$@"; do pair_holds "$pair" || { good=0; break; }; done
    if [ "$good" -eq 1 ]; then
      note "OK: $verb: $*"
      return 0
    fi
    sleep 0.2
  done
  fail "$SCENARIO: $verb never read: $* -- last read: $(last_read)"
}
# Every read for SECONDS holds every pair. Only meaningful after a completion
# signal, as a "stays" check.
hold_on() { # seconds verb pair...
  local end verb=$2 pair
  end=$(( $(date +%s) + $1 ))
  shift 2
  while :; do
    read_state "$verb"
    for pair in "$@"; do pair_holds "$pair" || fail "$SCENARIO: $verb: $pair stopped holding -- read: $(last_read)"; done
    [ "$(date +%s)" -lt "$end" ] || break
    sleep 0.2
  done
  note "OK: held for $1 s: $verb: $*"
}
# Polls a shell condition until it holds.
wait_until() { # description command...
  local description=$1 deadline
  shift
  deadline=$(( $(date +%s) + DEADLINE_SECS ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if "$@"; then
      note "OK: $description"
      return 0
    fi
    sleep 0.2
  done
  fail "$SCENARIO: never: $description"
}
# A shell condition holds on every check for SECONDS.
hold_until() { # seconds description command...
  local end description=$2
  end=$(( $(date +%s) + $1 ))
  shift 2
  while :; do
    "$@" || fail "$SCENARIO: stopped holding: $description"
    [ "$(date +%s)" -lt "$end" ] || break
    sleep 0.2
  done
  note "OK: held: $description"
}

# ---------------------------------------------------------------- the prompts, on disk and on screen

md_list() { (cd "$SIRIO_PROMPTS_DIR" 2>/dev/null && ls -1 -- *.md 2>/dev/null) | sort || true; }
index_files() {
  python3 -I -c 'import json, os, sys
path = os.path.join(sys.argv[1], "index.json")
if os.path.exists(path):
    for entry in json.load(open(path, encoding="utf-8"))["prompts"]:
        print(entry["file"])' "$SIRIO_PROMPTS_DIR" | sort
}
# The index title of the prompt at PATH, decoded; `<missing>` for no entry.
index_title() { # path
  python3 -I -c 'import json, os, sys
path = os.path.join(sys.argv[1], "index.json")
name = os.path.basename(sys.argv[2])
entries = json.load(open(path, encoding="utf-8"))["prompts"] if os.path.exists(path) else []
for entry in entries:
    if entry["file"] == name:
        sys.stdout.write(entry["title"])
        raise SystemExit
sys.stdout.write("<missing>")' "$SIRIO_PROMPTS_DIR" "$1"
}
title_is() { [ "$(index_title "$1")" = "$2" ]; }
files_are() { [ "$(md_list)" = "$1" ]; }
file_is() { [ -f "$1" ] && cmp -s "$1" "$2"; }
gone() { [ ! -e "$1" ] && [ "$(index_title "$1")" = "<missing>" ]; }
ends_with() { # file text
  python3 -I -c 'import sys
sys.exit(0 if open(sys.argv[1], "rb").read().endswith(sys.argv[2].encode()) else 1)' "$1" "$2" 2>/dev/null
}

# The card of the prompt at PATH in the last prompts read: its index, or fail.
card_of() { # path
  local i=0 count
  count="$(value count)"
  while [ "$i" -lt "${count:-0}" ]; do
    [ "$(value "card${i}Path")" = "$1" ] && { echo "$i"; return 0; }
    i=$((i + 1))
  done
  return 1
}
card_value() { # path suffix: from a fresh read
  local i
  read_state prompts
  i="$(card_of "$1")" || fail "$SCENARIO: no card for $1 -- read: $(last_read)"
  value "card${i}$2"
}
card_shows() { # path suffix want
  local i
  STATE="$(read_verb prompts 2>&1)" || return 1
  i="$(card_of "$1")" || return 1
  [ "$(value "card${i}$2")" = "$3" ]
}
no_card() { # path
  STATE="$(read_verb prompts 2>&1)" || return 1
  ! card_of "$1" >/dev/null
}

# Tab titles, one per line, from `surface tabs read`.
tab_titles() {
  ctlq surface tabs read --json | python3 -I -c 'import json, sys
row = json.load(sys.stdin)[0]
for index in range(1, int(row["count"]) + 1):
    print(row["tab." + str(index)].split("|", 2)[2])'
}
tabs_titled() { # title: how many
  local titles
  titles="$(tab_titles)" || { echo "-1"; return 0; }
  printf '%s\n' "$titles" | grep -Fxc -- "$1" || true
}
tab_count_is() { # title n
  [ "$(tabs_titled "$1")" = "$2" ]
}

# ---------------------------------------------------------------- the display

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
# captures/<theme>-<name>.png of this app's window, once it has drawn (after
# SETTLE seconds, 2 by default).
capture() { # name [settle]
  sleep "${2:-2}"
  local colours size file="$OUT_DIR/captures/$THEME-$1.png"
  with_timeout 20 import -display "$DISPLAY_TARGET" -window "$WIN" "$file" || fail "$SCENARIO: capture of $1 failed"
  [ -s "$file" ] || fail "$SCENARIO: $1.png is empty"
  colours="$(identify -format '%k' "$file")"
  [ "$colours" -ge 200 ] || fail "$SCENARIO: $1 is blank ($colours colours)"
  size="$(xwininfo -display "$DISPLAY_TARGET" -id "$WIN" | awk '/Width:/ {w = $2} /Height:/ {h = $2} END {print w "x" h}')"
  [ "$(identify -format '%wx%h' "$file")" = "$size" ] \
    || fail "$SCENARIO: $1.png is $(identify -format '%wx%h' "$file"), the window is $size"
  note "CAPTURE: captures/$THEME-$1.png ($colours colours, $size; pid $APP_PID)"
}
# Xvfb has no window manager, so the window is focused before every key.
focus_app() { DISPLAY="$DISPLAY_TARGET" with_timeout 10 xdotool windowfocus --sync "$WIN"; }
xkey() { # key names...
  focus_app
  DISPLAY="$DISPLAY_TARGET" xdotool key "$@"
}
xtype() { # text
  focus_app
  python3 -I "$WORK/xtext.py" "$DISPLAY_TARGET" "$1"
}
# Lines typed one at a time with a real Return between them.
xtype_lines() { # line...
  xtype "$1"
  shift
  local line
  for line in "$@"; do
    xkey Return
    xtype "$line"
  done
}
# The centre of an `x,y,w,h` bounds value, in window pixels
# (GPUI_X11_SCALE_FACTOR=1 makes a window pixel an X pixel).
center_of() { # bounds
  local x y w h
  IFS=, read -r x y w h <<<"$1"
  echo "$((x + w / 2)) $((y + h / 2))"
}
point_at() { # bounds
  local point
  [ -n "$1" ] || fail "$SCENARIO: no bounds to point at -- read: $(last_read)"
  point="$(center_of "$1")"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$WIN" ${point% *} ${point#* }
}
click_at_bounds() { # bounds [button]
  focus_app
  point_at "$1"
  DISPLAY="$DISPLAY_TARGET" xdotool click "${2:-1}"
}
click_key() { # key of the prompts read [button]
  read_state prompts
  [ -n "$(value "$1")" ] || fail "$SCENARIO: the prompts read has no $1 -- read: $(last_read)"
  click_at_bounds "$(value "$1")" "${2:-1}"
}
click_card() { # path suffix [button]
  local bounds
  bounds="$(card_value "$1" "$2")"
  [ -n "$bounds" ] || fail "$SCENARIO: card $2 of $1 has no bounds -- read: $(last_read)"
  click_at_bounds "$bounds" "${3:-1}"
}
# The CLIPBOARD of this display, as bytes, into FILE.
clipboard_to() { # file
  with_timeout 10 env -i PATH=/usr/bin:/bin HOME="$SD/home" GDK_BACKEND=x11 DISPLAY="$DISPLAY_TARGET" \
    python3 -I - "$1" <<'PY'
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, Gtk
text = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()
open(sys.argv[1], "wb").write(b"" if text is None else text.encode("utf-8"))
PY
}
clipboard_matches() { # file
  clipboard_to "$SD/clipboard.bin" && cmp -s "$SD/clipboard.bin" "$1"
}

# ---------------------------------------------------------------- an app

app_start() { # theme
  local n=0
  THEME="$1"
  SD="$WORK/$THEME"
  mkdir -p "$SD/home" "$SD/tmp" "$SD/cwd" "$SD/host" "$SD/xdg" "$SD/xdg-config" "$SD/xdg-state" \
    "$SD/xdg-cache" "$SD/xdg-runtime" "$SD/projects"
  chmod 700 "$SD/xdg-runtime"
  SIRIO_SOCKET="$SD/c.sock"
  SIRIO_DB="$SD/session.sqlite"
  [ "${#SIRIO_SOCKET}" -lt 100 ] || fail "the socket path is too long for a unix socket: $SIRIO_SOCKET (set SIRIO_E2E_TMP to a shorter directory)"
  "$SEED" --database "$SIRIO_DB" --appearance "$THEME"
  WENV=(
    "HOME=$SD/home" "PATH=$WORK/stubs:/usr/bin:/bin" "SHELL=$WORK/shell/shell" "TMPDIR=$SD/tmp" "LANG=C.UTF-8"
    "XDG_CONFIG_HOME=$SD/xdg-config" "XDG_DATA_HOME=$SD/xdg" "XDG_STATE_HOME=$SD/xdg-state"
    "XDG_CACHE_HOME=$SD/xdg-cache" "XDG_RUNTIME_DIR=$SD/xdg-runtime"
    "SIRIO_DB=$SIRIO_DB" "SIRIO_SOCKET=$SIRIO_SOCKET" "SIRIO_CREDENTIALS=$SD/credentials.json"
    "SIRIO_HOST_HOME=$SD/host" "SIRIO_PROJECTS_DIR=$SD/projects" "SIRIO_PROMPTS_DIR=$SIRIO_PROMPTS_DIR"
    "DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1" "VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"
  )
  # `exec` keeps the subshell's PID: $! is the app, which the captures match.
  (cd "$SD/cwd" && exec env -i "${WENV[@]}" "$BIN" >>"$SD/app.log" 2>&1) &
  APP_PID=$!
  while [ ! -S "$SIRIO_SOCKET" ] && [ "$n" -lt 300 ]; do n=$((n + 1)); sleep 0.1; done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket (see $SD/app.log)"
  n=0
  while ! ctlq surface tabs read >/dev/null 2>&1 && [ "$n" -lt 600 ]; do n=$((n + 1)); sleep 0.1; done
  ctlq surface tabs read >/dev/null 2>&1 || fail "sirio's control actions never became ready"
  WIN=""
  for _ in $(seq 1 50); do
    WIN="$(find_window)"
    [ -n "$WIN" ] && break
    sleep 0.2
  done
  [ -n "$WIN" ] || fail "no window with _NET_WM_PID=$APP_PID"
  note "app up (pid $APP_PID, window $WIN, $THEME)"
}
app_quit() {
  [ -n "$APP_PID" ] || return 0
  ctlq quit >/dev/null 2>&1 || true
  local n=0
  while alive "$APP_PID" && [ "$n" -lt 200 ]; do n=$((n + 1)); sleep 0.1; done
  alive "$APP_PID" && fail "$SCENARIO: sirio $APP_PID did not quit after sirioctl quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
  SIRIO_HOST_HOME="$SD/host" "$PROBE" shutdown --force >/dev/null 2>&1 || true
}
# Adds the repository and selects its worktree: New prompt needs one.
select_repo() {
  local id
  ctl project add "$REPO" >/dev/null
  wait_until "the repository is listed" repo_listed
  id="$(worktree_id)"
  ctl select-workspace --workspace "$id" >/dev/null
  wait_on prompts canEdit=1
}
repo_listed() { [ -n "$(worktree_id)" ]; }
worktree_id() {
  ctlq list-workspaces 2>/dev/null | awk -F '\t' -v repo="$REPO" '$4 == repo {print $1; exit}'
}
# Clicks New prompt and answers the new prompt's path.
new_prompt() {
  local before path
  before="$(md_list)"
  click_key newBounds
  wait_until "New prompt made a file" new_file_since "$before"
  path="$SIRIO_PROMPTS_DIR/$(comm -13 <(printf '%s\n' "$before") <(md_list) | head -n 1)"
  wait_on editor state=loaded "path=$path"
  settle_focus
  echo "$path"
}
# An open puts the keyboard in place at the frame after the one that draws
# the tab, and nothing reports it: a short pause before the first key.
settle_focus() { sleep 0.5; }
new_file_since() { [ -n "$(comm -13 <(printf '%s\n' "$1") <(md_list))" ]; }
# Opens the Delete dialog for PATH through the card's context menu.
open_delete_dialog() { # path
  click_card "$1" Bounds 3
  wait_on prompts 'menuDeleteBounds~=,'
  click_key menuDeleteBounds
  wait_on prompts 'confirmDeleteBounds~=,'
}

# ---------------------------------------------------------------- scenarios

THEME="light"
SCENARIO="launch"
app_start light
select_repo

SCENARIO="empty_state"
wait_on prompts count=0 shown=0 rightPanelView=files
[ ! -e "$SIRIO_PROMPTS_DIR" ] || fail "$SCENARIO: reading the view created $SIRIO_PROMPTS_DIR"
click_key railBounds
wait_on prompts shown=1 rightPanelView=prompts count=0 canEdit=1 'newBounds~=,'
[ ! -e "$SIRIO_PROMPTS_DIR" ] || fail "$SCENARIO: showing the view created $SIRIO_PROMPTS_DIR"
note "OK: no prompts folder after the view was shown"
capture 1-empty
ok 1 empty_state

SCENARIO="new_and_type"
TITLE_A='Revisione “Codex” 1/10'
A="$(new_prompt)"
note "new prompt: $A"
wait_until "the tab reads Untitled prompt" tab_count_is "Untitled prompt" 1
# New puts the keyboard in the empty title; Tab moves it to the text.
xtype "$TITLE_A"
xkey Tab
xtype_lines "Rivedi il diff di questa PR" "Segnala solo i bug veri" "Rispondi in italiano"
printf 'Rivedi il diff di questa PR\nSegnala solo i bug veri\nRispondi in italiano' >"$WORK/a-2.txt"
# No Ctrl+S anywhere: the pause after the last key saves.
wait_until "the file holds the three typed lines" file_is "$A" "$WORK/a-2.txt"
wait_until "index.json holds the title" title_is "$A" "$TITLE_A"
wait_until "the card shows the title" card_shows "$A" Title "$TITLE_A"
wait_until "the card's preview is the first two lines" card_shows "$A" Preview 'Rivedi il diff di questa PR\nSegnala solo i bug veri'
wait_until "the tab is labelled with the title" tab_count_is "$TITLE_A" 1
wait_on editor dirty=false message=
ok 2 new_and_type

SCENARIO="copy"
click_card "$A" CopyBounds
wait_until "Copy shows its check" card_shows "$A" Copied 1
# The check lasts 2 s, so the capture comes first and the slower CLIPBOARD
# read after it; the transcript says whether the check outlived the capture.
capture 3-copy 0.3
if card_shows "$A" Copied 1; then
  note "the check still showed after the capture: the frame shows it"
else
  note "the check ended during the capture: the frame may not show it"
fi
wait_until "the CLIPBOARD holds the file's bytes" clipboard_matches "$A"
ok 3 copy

SCENARIO="copy_right_after_typing"
click_card "$A" EditBounds
wait_on editor state=loaded "path=$A"
# Edit gives the text the keyboard (the title is not empty).
settle_focus
xkey ctrl+End
point_at "$(card_value "$A" CopyBounds)"
focus_app
# One xdotool call sends the key and the click back to back (no delay, no
# process start between them). The whole call, xdotool's own start included,
# must stay under the 400 ms autosave delay, so Copy is what writes the key.
started="$(date +%s%N)"
DISPLAY="$DISPLAY_TARGET" xdotool key --delay 0 X click 1
elapsed=$(( ($(date +%s%N) - started) / 1000000 ))
[ "$elapsed" -lt 400 ] || fail "$SCENARIO: the xdotool call took $elapsed ms, not under the 400 ms autosave delay"
note "the key and the click were one xdotool call of $elapsed ms"
printf 'Rivedi il diff di questa PR\nSegnala solo i bug veri\nRispondi in italianoX' >"$WORK/a-4.txt"
wait_until "the CLIPBOARD ends with the key typed just before Copy" clipboard_matches "$WORK/a-4.txt"
wait_until "the file holds the key too" file_is "$A" "$WORK/a-4.txt"
title_is "$A" "$TITLE_A" || fail "$SCENARIO: the key went to the title: $(index_title "$A")"
note "OK: the title is unchanged"
ok 4 copy_right_after_typing

SCENARIO="edit_dedupes"
click_card "$A" EditBounds
click_card "$A" EditBounds
hold_until 2 "one tab for the prompt" tab_count_is "$TITLE_A" 1
ok 5 edit_dedupes

SCENARIO="new_then_close_untouched"
files_before="$(md_list)"
entries_before="$(index_files)"
U="$(new_prompt)"
note "new prompt: $U"
wait_until "the untitled tab is open" tab_count_is "Untitled prompt" 1
# The keyboard is in the empty title field: the realistic place to close from.
xkey ctrl+w
wait_until "the untitled tab closed" tab_count_is "Untitled prompt" 0
wait_until "no new file remains" [ ! -e "$U" ]
[ "$(md_list)" = "$files_before" ] || fail "$SCENARIO: the files changed: $(md_list | tr '\n' ' ')"
[ "$(index_files)" = "$entries_before" ] || fail "$SCENARIO: the index changed: $(index_files | tr '\n' ' ')"
wait_until "the card is gone" no_card "$U"
note "OK: the folder and the index are as before"
ok 6 new_then_close_untouched

SCENARIO="delete_with_tab_open"
TITLE_B="Da eliminare"
B="$(new_prompt)"
note "new prompt: $B"
xtype "$TITLE_B"
xkey Tab
xtype "Testo da eliminare"
printf 'Testo da eliminare' >"$WORK/b.txt"
wait_until "B is saved" file_is "$B" "$WORK/b.txt"
wait_until "B's title is saved" title_is "$B" "$TITLE_B"
# The card opens the tab that is already there, with the text focused.
click_card "$B" Bounds
wait_on editor state=loaded "path=$B"
settle_focus
xkey ctrl+End
# A key, then at once the context menu, Delete… and the confirmation.
point_at "$(card_value "$B" Bounds)"
focus_app
DISPLAY="$DISPLAY_TARGET" xdotool key Y click 3
wait_on prompts 'menuDeleteBounds~=,'
click_key menuDeleteBounds
wait_on prompts 'confirmDeleteBounds~=,'
click_key confirmDeleteBounds
wait_until "B's file and entry are gone" gone "$B"
wait_until "B's card is gone" no_card "$B"
wait_until "B's tab closed" tab_count_is "$TITLE_B" 0
hold_until 1 "nothing brings B back" gone "$B"
[ -f "$A" ] || fail "$SCENARIO: deleting B removed A"
ok 7 delete_with_tab_open

SCENARIO="plain_md_unchanged"
cp "$REPO/notes.md" "$WORK/notes-before.md"
ctl surface editor open --path "$REPO/notes.md" >/dev/null
wait_on editor state=loaded "path=$REPO/notes.md" mode=preview breadcrumb=notes.md
ctl surface editor mode --mode=code >/dev/null
wait_on editor mode=code
ctl surface editor type --text=Z >/dev/null
wait_on editor dirty=true
hold_on 2 editor dirty=true
cmp -s "$REPO/notes.md" "$WORK/notes-before.md" || fail "$SCENARIO: the plain file was saved without Ctrl+S"
note "OK: no autosave for a plain file"
xkey ctrl+s
wait_on editor dirty=false
grep -q Z "$REPO/notes.md" || fail "$SCENARIO: Ctrl+S did not write the plain file"
note "OK: Ctrl+S saved it"
xkey ctrl+w
wait_until "the plain tab closed" tab_count_is notes.md 0
ok 8 plain_md_unchanged

SCENARIO="no_worktree"
ctl close-workspace --workspace "$(worktree_id)" >/dev/null
wait_on prompts canEdit=0 shown=1
files_before="$(md_list)"
read_state prompts
if [ -n "$(value newBounds)" ]; then
  click_key newBounds
fi
hold_until 1 "New prompt made nothing" files_are "$files_before"
click_card "$A" CopyBounds
wait_until "Copy still copies the file" clipboard_matches "$A"
wait_until "Copy shows its check" card_shows "$A" Copied 1
ctl select-workspace --workspace "$(worktree_id)" >/dev/null
wait_on prompts canEdit=1
ok 9 no_worktree

SCENARIO="unwritable"
click_card "$A" EditBounds
wait_on editor state=loaded "path=$A"
settle_focus
xkey ctrl+End
cp "$A" "$WORK/a-10.txt"
cp "$SIRIO_PROMPTS_DIR/index.json" "$WORK/index-10.json"
files_before="$(md_list)"
chmod a-w "$SIRIO_PROMPTS_DIR"
note "the prompts folder is now read-only"
xtype W
wait_on editor 'message~=Save failed' 'text~=italianoXW' dirty=true
hold_until 1 "the file is as before" cmp -s "$A" "$WORK/a-10.txt"
click_key newBounds
wait_on identity 'toast~=Could not create a prompt: '
note "toast: $(value toast)"
capture 10-toast
[ "$(md_list)" = "$files_before" ] || fail "$SCENARIO: New prompt wrote a file: $(md_list | tr '\n' ' ')"
cmp -s "$SIRIO_PROMPTS_DIR/index.json" "$WORK/index-10.json" || fail "$SCENARIO: New prompt changed index.json"
note "OK: no file and no index entry"
chmod u+w "$SIRIO_PROMPTS_DIR"
note "the prompts folder is writable again"
click_card "$A" EditBounds
wait_on editor state=loaded "path=$A"
settle_focus
xkey ctrl+End
xtype V
wait_until "the next key saved the text" ends_with "$A" "italianoXWV"
wait_on editor dirty=false
ok 10 unwritable

# ---------------------------------------------------------------- three prompts, both themes

SCENARIO="captures"
TITLE_C="Analisi approfondita della migrazione del database con le note sui rischi e sul ritorno indietro"
C="$(new_prompt)"
xtype "$TITLE_C"
xkey Tab
xtype_lines "Considera ogni tabella, ogni indice e ogni vincolo prima di proporre una sequenza di passi annullabili" "Poi elenca i rischi uno per uno" "Infine stima i tempi"
wait_until "C's title is saved" title_is "$C" "$TITLE_C"
wait_until "C's text is saved" ends_with "$C" "Infine stima i tempi"
TITLE_D="Commit"
D="$(new_prompt)"
xtype "$TITLE_D"
xkey Tab
xtype "Scrivi il messaggio di commit"
wait_until "D's title is saved" title_is "$D" "$TITLE_D"
wait_until "D's text is saved" ends_with "$D" "messaggio di commit"
wait_on prompts count=3
capture prompts
open_delete_dialog "$D"
capture delete-dialog
xkey Escape
wait_on prompts confirmDeleteBounds=
[ -f "$D" ] && title_is "$D" "$TITLE_D" || fail "$SCENARIO: Escape did not keep D"
note "OK: Escape cancelled the delete"
app_quit

SCENARIO="dark"
app_start dark
select_repo
click_key railBounds
wait_on prompts shown=1 count=3
for pair in "$A|$TITLE_A" "$C|$TITLE_C" "$D|$TITLE_D"; do
  card_shows "${pair%%|*}" Title "${pair#*|}" || fail "$SCENARIO: after the restart ${pair%%|*} does not read ${pair#*|}"
done
note "OK: the same three prompts after the restart"
capture prompts
open_delete_dialog "$C"
capture delete-dialog
xkey Escape
wait_on prompts confirmDeleteBounds=
app_quit

# ---------------------------------------------------------------- teardown

SCENARIO="teardown"
collect_logs
stop_hosts
terminate "$XVFB_PID"
XVFB_PID=""
leftover=""
for _ in $(seq 1 25); do
  leftover="$(pgrep -f "$WORK" 2>/dev/null || true)"
  [ -z "$leftover" ] && break
  sleep 0.2
done
[ -z "$leftover" ] || fail "processes of this run survived teardown: $(echo "$leftover" | tr '\n' ' ')"
echo "OK: no process of this run is left"
echo "artifact: $OUT_DIR"
echo "PROMPTS E2E OK"
