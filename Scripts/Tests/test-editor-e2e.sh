#!/bin/bash
set -eEuo pipefail

# End-to-end test of the file editor on Ely's CodeEditor (plan Task 6d): a real,
# isolated debug Sirio on a private Xvfb, driven through the debug-only
# `sirioctl surface editor` verbs, the window's own keystrokes, the Settings
# page, and the files on disk. Every check compares a whole value against the
# value it expects and says what it expected when it differs. Linux and X11
# only: its keys are ctrl- and alt-based, and its captures are X windows.
#
# Scenarios, in the order they run (the Settings page hides the editor, so it
# runs last):
#   load_states       each fixture opens to the state, error and path the app gives it
#   edit_save         typing, undo, redo, a two-range edit and save, byte for byte
#   crlf              a CRLF file keeps its line endings through an edit and a save
#   conflict          a clean disk change is adopted; a dirty one asks, then Keep
#                     and Reload answer it
#   markdown          a Markdown file's Preview and its code mode
#   colour            a multi-line raw string, captured (a capture cannot be judged
#                     for colour here: it must exist, be non-blank and be named)
#   copy_propagates   an empty-selection copy copies nothing; a selection's copy is
#                     pasted back, which reads the clipboard through the editor
#   pane_keys         shift-alt-down adds a cursor; ctrl-alt-down moves focus to the
#                     pane below, shown by a pane close that spares the file
#   diagnostics       the fake language server's two findings (an Error and a
#                     Warning, Scripts/Tests/fake_lsp.py) are counted and captured;
#                     a character typed before them moves them with the text, and
#                     the hover card follows them; undo clears them until the
#                     server publishes again, which the sequence shows
#   hover             a rested pointer asks the server, and its answer is the card
#   definition        a platform click on a call opens the definition's file at its
#                     line (the reveal)
#   link              a platform click on a Markdown link's label opens its target
#   context           the menu's Copy, run by a right press, reaches the clipboard
#   prefs             the Editor switches on nested.rs. Each switch with a visible
#                     effect (minimap, whitespace, rainbowBrackets, indentGuides,
#                     stickyScroll, lineNumbers, cursorShape, and the ruler at
#                     column 20) is captured in each of its states, and the two
#                     captures must differ byte for byte: a switch that draws
#                     nothing the capture can see fails here. codeLens and
#                     inlayHints have no Ely view option and no language server
#                     runs in this test, so their state is checked and no capture
#                     is claimed for them. Then Settings → Editor, and its ruler
#                     column field's resync across a Blur
#
# The artifact: --out-dir DIR (default artifacts/editor-e2e-<stamp>-<pid>) keeps
# transcript.log, app.log, the probe answers and the PID-matched window captures
# in frames/. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-editor-e2e.sh [--xvfb | --display :N] [--state-only]
#          [--out-dir DIR] [--appearance light|dark]
#
#   --xvfb        start a private Xvfb on a free display between :90 and :99
#   --display :N  use an Xvfb this user started (never your own $DISPLAY)
#   --state-only  take no window captures and no capture comparisons; the window
#                 still draws, so the editor's state is read from a drawn app
#   --appearance  seed light or dark (default dark)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
SEED="$TARGET_DIR/debug/examples/appearance_seed"

STATE_ONLY=0
START_XVFB=0
DISPLAY_TARGET=""
OUT_DIR=""
APPEARANCE="dark"
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --appearance) APPEARANCE="${2:?--appearance needs light or dark}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$APPEARANCE" in light|dark) ;; *) echo "--appearance is light or dark" >&2; exit 2 ;; esac
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/editor-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

# Tools the checks use: xdotool focuses the window and clicks Settings fields,
# xwininfo and xprop find the app's window, import and identify take the
# captures. Every X tool is pointed at this run's display, never at $DISPLAY.
for tool in python3 git xdotool xwininfo xprop xdpyinfo; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
if [ "$STATE_ONLY" -eq 0 ]; then
  for tool in import identify; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi
[ "$(id -u)" -ne 0 ] || fail "run as a user, not root: root reads a chmod 000 file, so 'unreadable' would not be unreadable"
[ -n "$DISPLAY_TARGET" ] || [ "$START_XVFB" -eq 1 ] || fail "no display; pass --xvfb or --display :N (the editor draws, even with --state-only)"
[ -z "$DISPLAY_TARGET" ] || [ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY; pass --xvfb, or an Xvfb you started on another number"

# ---------------------------------------------------------------- build

echo "building sirio, sirioctl, sirio-host and the appearance seed"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host -p sirio_persistence --example appearance_seed)
for binary in "$BIN" "$CTL" "$SEED"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run

BASE="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/editor-XXXXXX")"
WT="$RUN_DIR/repo"
APP_PID=""
XVFB_PID=""
SCENARIO="setup"
SCENARIOS_PASSED=0
STATE=""

is_a_host() {
  local comm
  comm="$(cat "/proc/$1/comm" 2>/dev/null || true)"
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
# Ends a child of this script: TERM, then KILL if it has not exited within 5 s.
# The watchdog's sleep writes nowhere, so it does not hold the transcript's pipe.
terminate() { # pid
  kill -TERM "$1" 2>/dev/null || return 0
  (sleep 5; kill -KILL "$1" 2>/dev/null) >/dev/null 2>&1 &
  local watchdog=$!
  wait "$1" 2>/dev/null || true
  kill "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
}
stop_hosts() {
  # Only the hosts whose state files are in this run's own host root. A host is
  # detached, so it is not this shell's child: its exit is polled, for 5 s, then
  # it is killed.
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
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  cp "$RUN_DIR"/app.log "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN_DIR/fake-lsp/requests.log" "$OUT_DIR/lsp-requests.log" 2>/dev/null || true
  [ -d "$RUN_DIR/host/log" ] && cp -R "$RUN_DIR/host/log" "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
  exit "$rc"
}
trap cleanup EXIT

# A display: a private Xvfb on a free number, or the one given.
VULKAN_ENV=()
if [ "$START_XVFB" -eq 1 ]; then
  command -v Xvfb >/dev/null || fail "Xvfb is required for --xvfb"
  for number in $(seq 90 99); do
    [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
  done
  [ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
  # A 1920x1080 screen: the window (1470x833 with Settings open) is then whole in
  # every capture, where a larger window would be clipped at the screen edge.
  Xvfb "$DISPLAY_TARGET" -screen 0 1920x1080x24 >"$OUT_DIR/xvfb.log" 2>&1 &
  XVFB_PID=$!
  for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
  DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
fi
echo "display: $DISPLAY_TARGET (appearance $APPEARANCE, state-only $STATE_ONLY)"
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi

# The checks' helpers, written beside the run: escaping as the app escapes a
# file's text, byte offsets into it, and the byte comparisons.
cat >"$RUN_DIR/e2e_tools.py" <<'PY'
import sys

def escape(text):
    return text.replace("\\", "\\\\").replace("\n", "\\n")

def unescape(text):
    out, i = [], 0
    while i < len(text):
        if text[i] == "\\" and i + 1 < len(text):
            out.append("\n" if text[i + 1] == "n" else text[i + 1])
            i += 2
        else:
            out.append(text[i])
            i += 1
    return "".join(out)

def read(path):
    with open(path, "rb") as handle:
        return handle.read().decode("utf-8")

command = sys.argv[1]
if command == "escape":
    sys.stdout.write(escape(read(sys.argv[2])))
elif command == "offset":
    # offset ESCAPED_TEXT NEEDLE: the byte offset of NEEDLE in the unescaped text.
    text = unescape(sys.argv[2])
    index = text.find(sys.argv[3])
    if index < 0:
        sys.exit(f"'{sys.argv[3]}' is not in the text")
    sys.stdout.write(str(len(text[:index].encode("utf-8"))))
elif command == "length":
    sys.stdout.write(str(len(unescape(sys.argv[2]).encode("utf-8"))))
elif command == "line-after":
    # line-after ESCAPED_TEXT NEEDLE LINES: the byte offset of the start of the line
    # LINES lines below the line NEEDLE is on.
    text = unescape(sys.argv[2])
    index = text.find(sys.argv[3])
    if index < 0:
        sys.exit(f"'{sys.argv[3]}' is not in the text")
    lines = text.split("\n")
    line = text[:index].count("\n") + int(sys.argv[4])
    if line >= len(lines):
        sys.exit(f"the text has no line {line}")
    sys.stdout.write(str(len("".join(piece + "\n" for piece in lines[:line]).encode("utf-8"))))
elif command == "with-suffix":
    # with-suffix ESCAPED SUFFIX: the escaped text of the unescaped text plus SUFFIX.
    sys.stdout.write(escape(unescape(sys.argv[2]) + sys.argv[3]))
elif command == "same-bytes":
    # same-bytes ACTUAL EXPECTED: exit 0 only when the two files are byte-identical;
    # otherwise both byte strings are printed, so the failure says what differs.
    with open(sys.argv[2], "rb") as a, open(sys.argv[3], "rb") as b:
        actual, expected = a.read(), b.read()
    if actual != expected:
        sys.stderr.write(f"expected {expected!r}\n     got {actual!r}\n")
        sys.exit(1)
else:
    sys.exit(f"unknown tool command {command}")
PY
TOOLS="$RUN_DIR/e2e_tools.py"
escaped() { python3 "$TOOLS" escape "$1"; }

# A private world: HOME and every XDG directory under the run dir, stub agent
# CLIs first on PATH (so no mise wrapper runs), and a host root of its own.
export XDG_STATE_HOME="$RUN_DIR/xdg/state" XDG_DATA_HOME="$RUN_DIR/xdg/data"
export XDG_CONFIG_HOME="$RUN_DIR/xdg/config" XDG_CACHE_HOME="$RUN_DIR/xdg/cache"
export XDG_RUNTIME_DIR="$RUN_DIR/xdg/runtime"
mkdir -p "$RUN_DIR/home" "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR" "$RUN_DIR/host"
chmod 700 "$XDG_RUNTIME_DIR"
mkdir -p "$RUN_DIR/stubs"
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

# The database the app opens, seeded with the appearance and the one Editor
# setting the minimap scenario starts from. appearance_seed has no flag for
# a setting, so the setting row is written here, under the key the app loads.
"$SEED" --database "$SIRIO_DB" --appearance "$APPEARANCE"
python3 - "$SIRIO_DB" <<'PY'
import sqlite3, sys
connection = sqlite3.connect(sys.argv[1])
connection.execute(
    "INSERT INTO setting (key, value) VALUES ('editor.minimap', 'false') "
    "ON CONFLICT(key) DO UPDATE SET value = excluded.value")
connection.commit()
connection.close()
PY

# The fixtures, in a fresh git repository inside the run dir.
mkdir -p "$WT"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
python3 - "$WT" <<'PY'
import os, sys
root = sys.argv[1]

def write(name, data):
    with open(os.path.join(root, name), "wb") as handle:
        handle.write(data)

# main.rs: 60 lines, with a three-line raw string at 41 to 43.
main = [f"fn line_{n}() -> u32 {{ {n} }}\n" for n in range(1, 41)]
main += [
    'const BANNER: &str = r#"first raw line\n',
    "second raw line\n",
    'third raw line"#;\n',
]
main += [f"fn line_{n}() -> u32 {{ {n} }}\n" for n in range(44, 61)]
write("main.rs", "".join(main).encode("utf-8"))
write("crlf.txt", b"a\r\nb\r\n")
write("mixed.txt", b"one\r\ntwo\nthree\r\n")
write("empty.txt", b"")
write("nonl.txt", b"alpha\nbeta")
write("wide.txt", b"w" * 20000 + b"\n")
write("notes.md", "# Notes\n\n## Section one\n\nSome [link](main.rs) text.\n\n- item\n".encode("utf-8"))
write("binary.bin", b"\x00\x01\x02binary\x00data\xff\xfe")
big = b"".join(f"line {n} of the big file\n".encode("utf-8") for n in range(1, 200000))
write("big.txt", big[:1153434])
write("unreadable.txt", b"secret\n")
# calls.rs: one call, to outer() in nested.rs, which a definition click lands on.
write("calls.rs", b"fn caller() -> u32 {\n    outer(1)\n}\n")
# nested.rs: bodies indented by 4, 8 and 12 spaces, and one impl block of about
# 80 lines, longer than the pane; one of its lines runs past 60 columns.
nested = [
    "// nested.rs: bodies indented by 4, 8 and 12 spaces, and one impl block\n",
    "// longer than the pane.\n",
    "struct Thing {\n",
    "    id: u32,\n",
    "}\n",
    "\n",
    "fn outer(seed: u32) -> u32 {\n",
    "    let value = {\n",
    "        let inner = {\n",
    "            seed + 7\n",
    "        };\n",
    "        inner * 2\n",
    "    };\n",
    "    value\n",
    "}\n",
    "\n",
    "impl Thing {\n",
]
for n in range(1, 9):
    body = [
        f"    fn method_{n}(&self) -> u32 {{\n",
        "        let base = {\n",
        "            let scaled = {\n",
        f"                self.id + {n}\n",
        "            };\n",
        "            scaled\n",
        "        };\n",
        "        base\n",
        "    }\n",
        "\n",
    ]
    if n == 1:
        body.insert(1, '        let label = "one line here runs well past the sixty columns the pane shows";\n')
    nested += body
nested += ["}\n"]
write("nested.rs", "".join(nested).encode("utf-8"))
PY
git -C "$WT" add -A
git -C "$WT" commit -q -m first
chmod 000 "$WT/unreadable.txt"

# The language server for .rs: the fake one (Scripts/Tests/fake_lsp.py), never a
# real rust-analyzer. Its answers name the text they point at, so the findings
# follow the file through every edit. The hover and the messages are the words
# the diagnostics and hover scenarios look for.
FAKE_HOVER="fake hover: a function that returns a number"
FAKE_ERROR="fake error: line_7 is wrong"
FAKE_WARNING="fake warning: line_12 is unused"
mkdir -p "$XDG_CONFIG_HOME/sirio" "$RUN_DIR/fake-lsp"
cat >"$RUN_DIR/fake-lsp/answers.json" <<JSON
{
  "hover": "$FAKE_HOVER",
  "diagnostics_file": "main.rs",
  "diagnostics": [
    {"needle": "line_7", "severity": 1, "message": "$FAKE_ERROR"},
    {"needle": "line_12", "severity": 2, "message": "$FAKE_WARNING"}
  ],
  "definition": {"path": "$WT/nested.rs", "needle": "fn outer"}
}
JSON
cat >"$XDG_CONFIG_HOME/sirio/languages.toml" <<TOML
[[language]]
name = "rust"
extensions = ["rs"]
command = "python3"
args = ["-I", "$ROOT/Scripts/Tests/fake_lsp.py", "$RUN_DIR/fake-lsp/answers.json", "$RUN_DIR/fake-lsp/requests.log"]
TOML

# ---------------------------------------------------------------- the app

app_start() {
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 \
    ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 300); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.1
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
  for _ in $(seq 1 600); do
    "$CTL" surface tabs read >/dev/null 2>&1 && return 0
    sleep 0.1
  done
  fail "sirio's control actions never became ready"
}
app_start

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
say() { echo "--- $*"; }

# The workspace the editor verbs act in: the repository, selected.
ctl project add "$WT" >/dev/null 2>&1 || true
for _ in $(seq 1 40); do
  "$CTL" select-workspace --workspace "$WT" >/dev/null 2>&1 && break
  sleep 0.5
done
CURRENT="$("$CTL" current-workspace --json 2>&1 || true)"
case "$CURRENT" in *"$WT"*) ;; *) fail "the repository is not the selected workspace: $CURRENT" ;; esac
echo "OK: the repository is the selected workspace"

# ---------------------------------------------------------------- the editor's state

read_state() {
  STATE="$("$CTL" surface editor state 2>&1)" || fail "$SCENARIO: surface editor state did not answer: $STATE"
}
sfield() { printf '%s\n' "$STATE" | sed -n "s/^$1=//p" | head -n 1; }

# Polls the editor until KEY reads VALUE; the first mismatch after the poll's
# time is the failure, and it names the scenario, the key and both values.
expect_state() { # key value
  local got="" tries=0
  while [ "$tries" -lt 150 ]; do
    read_state
    got="$(sfield "$1")"
    if [ "$got" = "$2" ]; then
      echo "OK: $1=$(printf '%s' "$2" | cut -c1-160)"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: $1 expected $(printf '%s' "$2" | cut -c1-400), got $(printf '%s' "$got" | cut -c1-400)"
}
# Waits until the editor is past loading (loaded or error), then answers it.
settle() {
  local tries=0
  while [ "$tries" -lt 300 ]; do
    read_state
    case "$(sfield state)" in loading|"") ;; *) return 0 ;; esac
    tries=$((tries + 1))
    sleep 0.1
  done
  fail "$SCENARIO: the file never left loading"
}
open_file() { # path
  ctl surface editor open --path="$1" >/dev/null
  settle
}

# The window of this app, for captures and clicks.
find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
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
capture_window() { # name window: frames/<name>.png from that window, taken now
  local colours
  [ -n "$2" ] || fail "$SCENARIO: no window with _NET_WM_PID=$APP_PID for $1"
  import -display "$DISPLAY_TARGET" -window "$2" "$OUT_DIR/frames/$1.png"
  [ -s "$OUT_DIR/frames/$1.png" ] || fail "$SCENARIO: $1.png is empty"
  colours="$(identify -format '%k' "$OUT_DIR/frames/$1.png")"
  [ "$colours" -ge 200 ] || fail "$SCENARIO: $1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}
capture() { # name: frames/<name>.png, once the window has drawn the state just read
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  capture_window "$1" "$(find_window)"
}
# Two captures of one switch, in its two states, must differ: byte-identical
# frames mean the switch drew nothing the capture can see. Skipped under
# --state-only, which takes no captures.
must_differ() { # label capture capture
  [ "$STATE_ONLY" -eq 0 ] || return 0
  if cmp -s "$OUT_DIR/frames/$2.png" "$OUT_DIR/frames/$3.png"; then
    fail "$SCENARIO: $1 captures are identical ($2.png and $3.png): the switch drew nothing the capture can see"
  fi
  echo "DIFFERS: $1 $2.png and $3.png"
}
# A click in the app's window at (X, Y), counted from the window's corner. The
# coordinates below are the Settings → Editor page's own fields in a window of
# CLICK_WINDOW; a window of any other size is refused rather than clicked at the
# wrong place.
click_at() { # x y
  local window size
  window="$(find_window)"
  [ -n "$window" ] || fail "$SCENARIO: no window to click in"
  size="$(xwininfo -display "$DISPLAY_TARGET" -id "$window" | awk '/Width:/ {w = $2} /Height:/ {h = $2} END {print w "x" h}')"
  [ "$size" = "$CLICK_WINDOW" ] || fail "$SCENARIO: the window is $size, not the $CLICK_WINDOW these clicks are placed for"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$1" "$2" click 1
}
# The app's window takes X input focus. Without a window manager a click-away
# does not blur a Settings field (the Blur never commits, measured on this
# Xvfb); with the window focused it does.
focus_window() {
  local window
  window="$(find_window)"
  [ -n "$window" ] || fail "$SCENARIO: no window to focus"
  DISPLAY="$DISPLAY_TARGET" xdotool windowfocus --sync "$window" \
    || fail "$SCENARIO: the window could not take X input focus"
}
# The Ruler column field and the Tab size field of Settings → Editor, in the
# window's own coordinates (the Ruler column field is enabled with Ruler on).
CLICK_WINDOW=1470x833
RULER_FIELD=(1139 660)
TABSIZE_FIELD=(1139 705)

# Writes a file the app has open, as another program would.
write_disk() { # path source-file
  python3 - "$1" "$2" <<'PY'
import sys
with open(sys.argv[2], "rb") as source, open(sys.argv[1], "wb") as target:
    target.write(source.read())
PY
}

# ---------------------------------------------------------------- scenarios

scenario() { # name function
  SCENARIO="$1"
  say "scenario $1"
  "$2"
  SCENARIOS_PASSED=$((SCENARIOS_PASSED + 1))
  echo "PASS $1"
}

scenario_load_states() {
  # The state and error each fixture opens to, as the app answers today. The
  # messages are the app's own words, recorded from a run; @WT@ is the run's
  # repository, which the two path-naming messages carry.
  local rows=(
    "main.rs|loaded|"
    "crlf.txt|loaded|"
    "mixed.txt|loaded|"
    "empty.txt|loaded|"
    "nonl.txt|loaded|"
    "wide.txt|loaded|"
    "notes.md|loaded|"
    "nested.rs|loaded|"
    "binary.bin|error|This file appears to be binary and cannot be shown as text."
    "big.txt|error|This file is too large to open (1153434 bytes; limit is 1048576 bytes)."
    "unreadable.txt|error|This file cannot be read (Permission denied (os error 13)): @WT@/unreadable.txt"
    "missing.rs|error|This file does not exist: @WT@/missing.rs"
  )
  local row name want_state want_error
  for row in "${rows[@]}"; do
    IFS='|' read -r name want_state want_error <<<"$row"
    want_error="${want_error//@WT@/$WT}"
    ctl surface editor open --path="$WT/$name" >/dev/null
    settle
    local got_state got_error
    got_state="$(sfield state)"
    got_error="$(sfield error)"
    echo "load_state $name state=$got_state error=$got_error"
    [ "$got_state" = "$want_state" ] || fail "$SCENARIO: $name state expected $want_state, got $got_state"
    [ "$got_error" = "$want_error" ] || fail "$SCENARIO: $name error expected '$want_error', got '$got_error'"
    expect_state path "$WT/$name"
  done
  echo "OK: every fixture opens to the state the app gives it"
}

scenario_edit_save() {
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state language Rust
  cp "$WT/main.rs" "$RUN_DIR/edit-0.txt"
  python3 - "$RUN_DIR/edit-0.txt" "$RUN_DIR/edit-1.txt" <<'PY'
import sys
with open(sys.argv[1], "rb") as source, open(sys.argv[2], "wb") as target:
    target.write("è😀漢\n".encode("utf-8") + source.read())
PY
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor type --text=$'è😀漢\n' >/dev/null
  expect_state dirty true
  expect_state text "$(escaped "$RUN_DIR/edit-1.txt")"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/edit-0.txt")"
  expect_state dirty false
  ctl surface editor keys --keys=ctrl-shift-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/edit-1.txt")"

  # Two ranges, each the nine bytes of a function name, typed over at once.
  local first second
  first="$(python3 "$TOOLS" offset "$(escaped "$RUN_DIR/edit-1.txt")" "fn line_2")"
  second="$(python3 "$TOOLS" offset "$(escaped "$RUN_DIR/edit-1.txt")" "fn line_5")"
  ctl surface editor select --ranges="$first..$((first + 9));$second..$((second + 9))" >/dev/null
  ctl surface editor type --text=X >/dev/null
  python3 - "$RUN_DIR/edit-1.txt" "$RUN_DIR/edit-2.txt" "$first" "$second" <<'PY'
import sys
with open(sys.argv[1], "rb") as source:
    data = source.read()
first, second = int(sys.argv[3]), int(sys.argv[4])
edited = data[:first] + b"X" + data[first + 9:second] + b"X" + data[second + 9:]
with open(sys.argv[2], "wb") as target:
    target.write(edited)
PY
  expect_state text "$(escaped "$RUN_DIR/edit-2.txt")"

  ctl surface editor save >/dev/null
  expect_state dirty false
  python3 "$TOOLS" same-bytes "$WT/main.rs" "$RUN_DIR/edit-2.txt" \
    || fail "$SCENARIO: main.rs on disk is not the expected bytes after save"
  echo "OK: main.rs on disk is byte-for-byte the edited text"
}

scenario_crlf() {
  open_file "$WT/crlf.txt"
  expect_state state loaded
  expect_state dirty false
  expect_state line_ending crlf
  printf 'za\r\nb\r\n' >"$RUN_DIR/crlf-expected.txt"
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor type --text=z >/dev/null
  expect_state dirty true
  ctl surface editor save >/dev/null
  expect_state dirty false
  expect_state line_ending crlf
  python3 "$TOOLS" same-bytes "$WT/crlf.txt" "$RUN_DIR/crlf-expected.txt" \
    || fail "$SCENARIO: crlf.txt on disk is not the expected bytes (expected and got are printed above)"
  echo "OK: crlf.txt kept its CRLF endings through the edit and the save"
}

scenario_conflict() {
  open_file "$WT/main.rs"
  expect_state state loaded
  # A clean buffer adopts the disk change without asking.
  cp "$WT/main.rs" "$RUN_DIR/conflict-0.txt"
  python3 - "$RUN_DIR/conflict-0.txt" "$RUN_DIR/conflict-1.txt" <<'PY'
import sys
with open(sys.argv[1], "rb") as source, open(sys.argv[2], "wb") as target:
    target.write(b"// external one\n" + source.read())
PY
  write_disk "$WT/main.rs" "$RUN_DIR/conflict-1.txt"
  expect_state text "$(escaped "$RUN_DIR/conflict-1.txt")"
  expect_state dirty false
  expect_state conflict none

  # A dirty buffer keeps its text and asks: Keep answers it, Reload takes the disk.
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor type --text=y >/dev/null
  expect_state dirty true
  python3 - "$RUN_DIR/conflict-1.txt" "$RUN_DIR/conflict-buffer.txt" <<'PY'
import sys
with open(sys.argv[1], "rb") as source, open(sys.argv[2], "wb") as target:
    target.write(b"y" + source.read())
PY
  python3 - "$RUN_DIR/conflict-1.txt" "$RUN_DIR/conflict-2.txt" <<'PY'
import sys
with open(sys.argv[1], "rb") as source, open(sys.argv[2], "wb") as target:
    target.write(b"// external two\n" + source.read())
PY
  write_disk "$WT/main.rs" "$RUN_DIR/conflict-2.txt"
  expect_state conflict changed
  expect_state dirty true
  expect_state text "$(escaped "$RUN_DIR/conflict-buffer.txt")"
  ctl surface editor conflict --action=keep >/dev/null
  expect_state conflict none
  expect_state text "$(escaped "$RUN_DIR/conflict-buffer.txt")"
  ctl surface editor conflict --action=reload >/dev/null
  expect_state text "$(escaped "$RUN_DIR/conflict-2.txt")"
  expect_state dirty false
  expect_state conflict none
  echo "OK: a clean change was adopted; Keep kept the buffer; Reload took the disk"
}

scenario_markdown() {
  open_file "$WT/notes.md"
  expect_state state loaded
  expect_state language Markdown
  ctl surface editor mode --mode=preview >/dev/null
  expect_state mode preview
  capture markdown-preview
  ctl surface editor mode --mode=code >/dev/null
  expect_state mode code
  expect_state text "$(escaped "$WT/notes.md")"
  capture markdown-code
}

scenario_colour() {
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state mode code
  expect_state language Rust
  # A selection in the raw string's middle line scrolls it into view.
  local at
  at="$(python3 "$TOOLS" offset "$(escaped "$WT/main.rs")" "second raw line")"
  ctl surface editor select --ranges="$at..$at" >/dev/null
  expect_state selections "$at..$at"
  capture colour-raw-string
}

scenario_copy_propagates() {
  open_file "$WT/main.rs"
  expect_state state loaded
  local before first second end
  read_state
  before="$(sfield text)"
  end="$(python3 "$TOOLS" length "$before")"
  # Copy the first function's name, then copy from an empty selection.
  first="$(python3 "$TOOLS" offset "$before" "fn line_1")"
  ctl surface editor select --ranges="$first..$((first + 9))" >/dev/null
  ctl surface editor keys --keys=ctrl-c >/dev/null
  expect_state state loaded
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor keys --keys=ctrl-c >/dev/null
  expect_state state loaded
  expect_state text "$before"
  # Paste at the end: the clipboard's text is what the buffer gains.
  ctl surface editor select --ranges="$end..$end" >/dev/null
  ctl surface editor keys --keys=ctrl-v >/dev/null
  expect_state text "$(python3 "$TOOLS" with-suffix "$before" "fn line_1")"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$before"
  # A selection's copy reaches the clipboard too.
  second="$(python3 "$TOOLS" offset "$before" "fn line_2")"
  ctl surface editor select --ranges="$second..$((second + 9))" >/dev/null
  ctl surface editor keys --keys=ctrl-c >/dev/null
  ctl surface editor select --ranges="$end..$end" >/dev/null
  ctl surface editor keys --keys=ctrl-v >/dev/null
  expect_state text "$(python3 "$TOOLS" with-suffix "$before" "fn line_2")"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$before"
  expect_state dirty false
  echo "OK: the empty copy left the clipboard as it was; the selection's copy is what pastes"
}

scenario_pane_keys() {
  open_file "$WT/main.rs"
  expect_state state loaded
  # The byte offset where the buffer's second line starts: shift-alt-down puts a
  # cursor there from the first line's start.
  local buffer newline first_line_bytes line_feed=$'\n'
  buffer="$(sfield text)"
  newline="$(python3 "$TOOLS" offset "$buffer" "$line_feed")"
  first_line_bytes=$((newline + 1))
  # A terminal below the file: the file's pane is then the one above.
  ctl pane split down >/dev/null
  sleep 1
  ctl pane focus up >/dev/null
  sleep 0.5
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor keys --keys=shift-alt-down >/dev/null
  expect_state selections "0..0;$first_line_bytes..$first_line_bytes"
  capture pane-keys-split
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor keys --keys=ctrl-alt-down >/dev/null
  sleep 1
  # The file is the tab's only file pane, so state names it whichever pane has
  # focus. Where focus is shows only in the close below.
  expect_state path "$WT/main.rs"
  # Focus has moved to the pane below only if a close now takes that pane and
  # leaves the file: otherwise the close takes the file's pane and the file goes.
  ctl pane close >/dev/null
  sleep 1
  "$CTL" surface editor state >/dev/null 2>&1 \
    || fail "$SCENARIO: ctrl-alt-down did not move focus to the pane below: the pane close took the file's pane"
  expect_state path "$WT/main.rs"
  expect_state state loaded
  echo "OK: shift-alt-down added a cursor; ctrl-alt-down moved focus to the pane below"
}

# The title of the active tab, from `surface tabs read --json`: tab.N is
# "kind|snapshot|title", and active is N.
active_tab_title() {
  "$CTL" surface tabs read --json | python3 -c '
import json, sys
report = json.load(sys.stdin)[0]
print(report["tab." + report["active"]].split("|", 2)[2])'
}
expect_active_tab() { # title
  local tries=0 got=""
  while [ "$tries" -lt 150 ]; do
    got="$(active_tab_title)"
    [ "$got" = "$1" ] && { echo "OK: the active tab is $1"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: the active tab is '$got', expected '$1'"
}
# Sets one key of the fake language server's answers. It reads them again for
# each message, so the next message sees the new value; the file is replaced
# whole, so the server never reads half of it.
fake_lsp_answers() { # key json-value
  python3 - "$RUN_DIR/fake-lsp/answers.json" "$1" "$2" <<'PY'
import json, os, sys
path, key, value = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    answers = json.load(handle)
answers[key] = json.loads(value)
with open(path + ".tmp", "w", encoding="utf-8") as handle:
    json.dump(answers, handle)
os.replace(path + ".tmp", path)
PY
}
# The fake language server logs each message it gets, as `request M` or
# `notification M`: a line ending in M is the server being sent that method.
lsp_asked() { # method
  grep -q " $1\$" "$RUN_DIR/fake-lsp/requests.log" 2>/dev/null \
    || fail "$SCENARIO: the language server was never asked $1"
  echo "OK: the language server was asked $1"
}

scenario_diagnostics() {
  open_file "$WT/main.rs"
  expect_state state loaded
  # The server publishes its two findings for the text it holds: an Error on the
  # name line_7 and a Warning on line_12.
  expect_state diagnostics 2
  capture diagnostics
  local before old_at error_at
  before="$(sfield text)"
  old_at="$(python3 "$TOOLS" offset "$before" "line_7")"
  # The server holds its answers while the text moves, so the findings the
  # editor and the card read are the ones Ely moved, not a fresh publish.
  fake_lsp_answers publish_on_change false
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor type --text=x >/dev/null
  expect_state dirty true
  # The findings move with the text, the Error one byte later, and both stay.
  expect_state diagnostics 2
  error_at="$(python3 "$TOOLS" offset "$(sfield text)" "line_7")"
  [ "$error_at" -eq $((old_at + 1)) ] \
    || fail "$SCENARIO: the Error is at $error_at, not one byte past $old_at, after the typed character"
  # The card reads the editor's findings, so the Error answers at its new place.
  ctl surface editor hover --offset="$error_at" >/dev/null
  expect_state hover "$FAKE_ERROR\\n$FAKE_HOVER"
  capture diagnostics-hover
  # At the Error's old place there is a space now: no finding, though Sirio's own
  # copy of the findings still names that place. The card shows the server's
  # answer alone.
  ctl surface editor hover --offset="$old_at" >/dev/null
  expect_state hover "$FAKE_HOVER"
  # Undo restores the text. Ely clears its findings with the restored text, and
  # the server publishes them again for it. The sequence shows the clear, if the
  # read catches it, and the count then comes back to 2.
  fake_lsp_answers publish_on_change true
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state dirty false
  expect_state text "$before"
  local seen="" count tries=0
  while [ "$tries" -lt 20 ]; do
    read_state
    count="$(sfield diagnostics)"
    case " $seen " in *" $count "*) ;; *) seen="$seen $count" ;; esac
    tries=$((tries + 1))
    sleep 0.1
  done
  echo "undo: diagnostics seen over 2 s:$seen"
  expect_state diagnostics 2
  lsp_asked textDocument/didChange
  echo "OK: the findings moved with the typed character, the card followed them, and undo's restore gave them back"
}

scenario_hover() {
  open_file "$WT/main.rs"
  expect_state state loaded
  local at
  at="$(python3 "$TOOLS" offset "$(sfield text)" "line_1")"
  ctl surface editor hover --offset="$at" >/dev/null
  # Nothing is under line_1 but the server's answer, which the card then shows.
  expect_state hover "$FAKE_HOVER"
  lsp_asked textDocument/hover
  echo "OK: the hover at line_1 is the server's answer"
}

scenario_definition() {
  open_file "$WT/calls.rs"
  expect_state state loaded
  local at line_six
  at="$(python3 "$TOOLS" offset "$(sfield text)" "outer(1)")"
  ctl surface editor click --offset="$at" --platform=true >/dev/null
  expect_active_tab nested.rs
  expect_state path "$WT/nested.rs"
  # The server's definition is the line "fn outer" begins; the reveal puts the
  # caret at the start of that line, which is the selection the file reads.
  line_six="$(python3 - "$WT/nested.rs" <<'PY'
import sys
lines = open(sys.argv[1], encoding="utf-8").read().split("\n")
print(len("".join(line + "\n" for line in lines[:6]).encode("utf-8")))
PY
)"
  expect_state selections "$line_six..$line_six"
  lsp_asked textDocument/definition
  echo "OK: the definition opened nested.rs with the caret on its line 7"
}

scenario_link() {
  open_file "$WT/notes.md"
  expect_state state loaded
  ctl surface editor mode --mode=code >/dev/null
  expect_state mode code
  local at
  at="$(python3 "$TOOLS" offset "$(sfield text)" "link")"
  # A click inside the label, not on the (target) beside it.
  ctl surface editor click --offset=$((at + 1)) --platform=true >/dev/null
  expect_active_tab main.rs
  expect_state path "$WT/main.rs"
  echo "OK: the link's label opened main.rs"
}

scenario_context() {
  open_file "$WT/main.rs"
  expect_state state loaded
  local before first
  before="$(sfield text)"
  first="$(python3 "$TOOLS" offset "$before" "line_2")"
  ctl surface editor select --ranges="$first..$((first + 6))" >/dev/null
  # A right press inside the selection keeps it; the menu's Copy copies it.
  ctl surface editor context --offset=$((first + 2)) --action=copy >/dev/null
  expect_state selections "$first..$((first + 6))"
  # The clipboard, read as copy_propagates reads it: a paste into the empty
  # scratch file, then undone, so the fixture is left as it was.
  open_file "$WT/empty.txt"
  expect_state state loaded
  ctl surface editor keys --keys=ctrl-v >/dev/null
  expect_state text "line_2"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text ""
  expect_state dirty false
  echo "OK: the menu's Copy reached the clipboard: the paste read line_2"
  # Send to agent is not run here: no agent chat can be opened in this world (see
  # the report), and a chat is what the selection would go to.
}

prefs_line() {
  printf 'minimap=%s,stickyScroll=%s,indentGuides=%s,rainbowBrackets=%s,codeLens=%s,inlayHints=%s,whitespace=%s,ruler=%s,rulerColumn=%s,lineNumbers=%s,cursorShape=%s,tabSize=%s' \
    "${PREF[minimap]}" "${PREF[stickyScroll]}" "${PREF[indentGuides]}" "${PREF[rainbowBrackets]}" \
    "${PREF[codeLens]}" "${PREF[inlayHints]}" "${PREF[whitespace]}" "${PREF[ruler]}" \
    "${PREF[rulerColumn]}" "${PREF[lineNumbers]}" "${PREF[cursorShape]}" "${PREF[tabSize]}"
}
set_pref() { # key value: the verb, then the state's whole prefs line
  PREF[$1]="$2"
  ctl surface editor pref --key="$1" --value="$2" >/dev/null
  expect_state prefs "$(prefs_line)"
}
on_off() { if [ "$1" = true ]; then echo on; else echo off; fi; }

# A switch with a visible effect: captured in its on and off states, which must differ.
switch_pref() { # key
  local key="$1" value
  if [ "${PREF[$key]}" = true ]; then value=false; else value=true; fi
  set_pref "$key" "$value"
  capture "prefs-$key-$(on_off "$value")"
  if [ "$value" = true ]; then value=false; else value=true; fi
  set_pref "$key" "$value"
  capture "prefs-$key-$(on_off "$value")"
  must_differ "$key" "prefs-$key-on" "prefs-$key-off"
}

scenario_prefs() {
  declare -gA PREF=(
    [minimap]=false [stickyScroll]=true [indentGuides]=true [rainbowBrackets]=true
    [codeLens]=true [inlayHints]=true [whitespace]=false [ruler]=false
    [rulerColumn]=100 [lineNumbers]=absolute [cursorShape]=line [tabSize]=4
  )
  open_file "$WT/nested.rs"
  expect_state state loaded
  expect_state prefs "$(prefs_line)"
  local key value window at buffer

  # The ruler first, at column 20, well inside the pane.
  set_pref rulerColumn 20
  set_pref ruler true
  capture prefs-ruler-on
  set_pref ruler false
  capture prefs-ruler-off
  must_differ ruler prefs-ruler-on prefs-ruler-off

  # The switches read at the top of nested.rs, where indented bodies start.
  for key in minimap whitespace rainbowBrackets indentGuides; do
    switch_pref "$key"
  done
  # codeLens and inlayHints: no view option and no language server here, so only
  # their state is checked. Both start on.
  for key in codeLens inlayHints; do
    set_pref "$key" false
    set_pref "$key" true
  done
  capture prefs-lineNumbers-absolute
  set_pref lineNumbers relative
  capture prefs-lineNumbers-relative
  set_pref lineNumbers hidden
  capture prefs-lineNumbers-hidden
  set_pref lineNumbers absolute
  must_differ lineNumbers prefs-lineNumbers-absolute prefs-lineNumbers-relative
  must_differ lineNumbers prefs-lineNumbers-absolute prefs-lineNumbers-hidden
  must_differ lineNumbers prefs-lineNumbers-relative prefs-lineNumbers-hidden

  # The caret 50 lines into impl Thing. select centres it, so the impl line leaves
  # the top of the pane and sticky scroll has a header to pin.
  read_state
  buffer="$(sfield text)"
  at="$(python3 "$TOOLS" line-after "$buffer" "impl Thing {" 50)"
  ctl surface editor select --ranges="$at..$at" >/dev/null
  expect_state selections "$at..$at"
  switch_pref stickyScroll

  # Each cursor shape, with the caret in its blink-on phase: two harmless keys
  # focus the editor, the select restarts the blink, and the capture follows
  # within a quarter second.
  for value in block underline line; do
    set_pref cursorShape "$value"
    window=""
    [ "$STATE_ONLY" -eq 1 ] || window="$(find_window)"
    ctl surface editor keys --keys=right >/dev/null
    ctl surface editor keys --keys=left >/dev/null
    ctl surface editor select --ranges="$at..$at" >/dev/null
    if [ "$STATE_ONLY" -eq 0 ]; then
      sleep 0.25
      capture_window "prefs-cursorShape-$value" "$window"
    fi
  done
  must_differ cursorShape prefs-cursorShape-block prefs-cursorShape-underline
  must_differ cursorShape prefs-cursorShape-block prefs-cursorShape-line
  must_differ cursorShape prefs-cursorShape-underline prefs-cursorShape-line

  echo "OK: every Editor switch reached the state, and each visible one drew captures that differ"
  scenario_settings_resync
}

scenario_settings_resync() {
  # Settings → Editor: the page's own fields, the ruler column's field among
  # them, and a Blur that must not put the field's old text back.
  ctl surface settings open --section=editor >/dev/null
  sleep 1
  capture settings-editor
  set_pref ruler true
  capture settings-editor-ruler-on
  # The ruler column field takes focus, the verb sets its value (the page
  # redraws the field with it), and a click in the tab size field blurs the
  # ruler column field: its Blur must commit the value the page now shows.
  focus_window
  click_at "${RULER_FIELD[0]}" "${RULER_FIELD[1]}"
  sleep 0.5
  ctl surface editor pref --key=rulerColumn --value=40 >/dev/null
  PREF[rulerColumn]=40
  expect_state prefs "$(prefs_line)"
  capture settings-editor-ruler-40-focused
  click_at "${TABSIZE_FIELD[0]}" "${TABSIZE_FIELD[1]}"
  # The commit, if the stale text commits, lands within a moment of the Blur:
  # the state is read after it has had time to.
  sleep 1.5
  expect_state prefs "$(prefs_line)"
  capture settings-editor-after-blur
  echo "OK: rulerColumn stayed 40 after the field lost focus"
}

# ---------------------------------------------------------------- run

scenario load_states scenario_load_states
scenario edit_save scenario_edit_save
scenario crlf scenario_crlf
scenario conflict scenario_conflict
scenario markdown scenario_markdown
scenario colour scenario_colour
scenario copy_propagates scenario_copy_propagates
scenario pane_keys scenario_pane_keys
scenario diagnostics scenario_diagnostics
scenario hover scenario_hover
scenario definition scenario_definition
scenario link scenario_link
scenario context scenario_context
scenario prefs scenario_prefs

stop_hosts
echo "EDITOR E2E OK ($SCENARIOS_PASSED scenarios, appearance $APPEARANCE, state-only $STATE_ONLY)"
