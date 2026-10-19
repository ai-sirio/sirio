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
#   markdown          a Markdown file's Preview and its code mode; in Code its formatting
#                     buttons are drawn (format_buttons=shown)
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
#   breadcrumb        the row over src/shapes.rs names the file's path and the symbols the
#                     fake server's outline puts around the caret (impl Foo > bar): none
#                     before the first symbol, a shallower symbol closes the deeper ones,
#                     a sibling replaces its sibling; a press on a symbol part reveals its
#                     line and one on the path is refused; five typed characters ask for
#                     the symbols once, after the pause (a slow burst is undone and run
#                     again, and says so), and a save asks at once; a deep path in the
#                     pane keeps its symbols in the capture; captured
#   snapshot          src/shapes.rs and notes.md at HEAD, opened as a change request's file
#                     is (the debug verb open-snapshot): read-only, one banner (the view's,
#                     with its local-file action) and no banner of Ely's (ely_banner=false),
#                     typing, a paste, the menu's Cut and Paste and replace-all each leave
#                     the text byte-identical with a clipboard and a selection to prove it,
#                     Copy and find still work; captured with find open (the banner above
#                     the find bar) and as Markdown in Code, whose formatting buttons are
#                     not drawn (format_buttons=hidden); a file the revision has and the
#                     worktree does not opens with no local file to offer (banner_action
#                     empty, ely_banner=false); the tabs the scenario opened are closed at
#                     its end, the Secondary pane with them (secondaryVisible as it was)
#   git_marks         a file's changes against HEAD mark its gutter: the disk's when the
#                     file opens, an adopted change, a save; a mark opens its hunk in
#                     place, Escape closes it; a file restored to HEAD, a commit made
#                     elsewhere (cleared when the editor takes focus), a file outside
#                     the repository and a clean file have no marks
#   status            the row under the file (its `status` state): the branch, and ahead and
#                     behind against an upstream set up here with one commit on each side
#                     (counted with git itself); the cursor in characters (an emoji and an
#                     accent) with a two-range selection's total; the fake server named and
#                     ready for a .rs file, failed for one that launches and ends at once,
#                     and no server at all for a .txt and for a .zig with none installed
#                     (the app's PATH has no zls); two files opened while a slow server
#                     starts share its one launch and each get a didOpen; LF and CRLF;
#                     Markdown in Preview has no cursor; a snapshot has neither branch nor
#                     server; the full set in a pane with room (sidebar and right panel
#                     hidden) and the compact set in narrower ones, with a long branch
#                     name shortened and the server whole; captured
#   status_actions   the row's four interactions, through the verb the popovers share their
#                     functions with (`surface editor status`) and, for the popovers
#                     themselves, a real click and real keys: the line ending (CRLF is an
#                     unsaved change, and a save writes it on every line; back to LF); the
#                     indent (spaces or tabs, and a Tab, a Shift-Tab and a settings change
#                     as that tab sees them); the language, colour only (the server and its
#                     requests are untouched, two captures differ); go to line (a line, past
#                     the end, a word and zero); a snapshot's refusals and a Markdown
#                     Preview's; one popover at a time, Escape and Enter, focus back in the
#                     code (also from a list, and after the keyboard was elsewhere); Down and
#                     Return choose in an open list; a panel is dropped with its item (the
#                     compact row, Markdown Preview) and stays dropped; the verb's switch
#                     from one open panel to another, either way; each choice stays after a
#                     pause (`expect_state_stays`); Tabs mode's block indent, Enter after an
#                     opener and Shift-Tab on spaces; the colour follows the language
#                     (`highlight=`, what the highlighter gave line 1); each popover captured
#                     open
#   notifications     the bell behind the status row: a language server that cannot start
#                     (a failing entry added to languages.toml, drawn "did not start") and
#                     a save refused over a disk change (a real Ctrl+S, "Save failed: ...",
#                     one message) each raise a message, and each lands in the tab's list;
#                     the bell counts what arrived since the list was opened, a dismissed
#                     corner card leaves list and count alone and does not let its press
#                     reach the code, opening the list clears the count and keeps the list,
#                     the same message again is one row marked "(x3)", a message that
#                     arrives while the list is open is seen already, `value=clear` empties
#                     both; a real click opens the panel, an Enter in it wipes nothing, a
#                     row's Copy and `copy:<n>` reach the clipboard (read by a paste), and
#                     Clear closes it with the keyboard back in the code; a snapshot's and a
#                     Preview's bell take a press, a snapshot keeps its own list and a real
#                     Ctrl+S on it raises one message; the list keeps the newest 50 rows
#                     (the oldest five of 55 go); the compact row's bell takes real clicks;
#                     captured with a count, the list, a repeat, the Copy buttons, the
#                     compact row's panel and the empty list
#   find              Find over findme.txt: the match count against grep (case-insensitive,
#                     then case-sensitive), two steps forward and one back, a pattern that
#                     does not compile, close; captured with the replace row open and the
#                     minimap on
#   replace           one occurrence replaced, then every one; a single undo returns the
#                     text from before the replace-all, a second the original
#   multibyte         an emoji found twice, replaced one at a time and all at once,
#                     byte for byte
#   escape            Escape closes find from the code, and from the query field with a
#                     real X key, after which a typed key lands in the code
#   send_to_agent     the context menu's Send to agent puts the file, the line range and
#                     the selected lines in the chat's composer; a second send appends
#   focus_refresh     one real press into the code asks for the file's diff once
#                     (`git_diff_requests`), not once per handler that sees the focus
#   hover_leave       with the card of an Error drawn for a real resting pointer, a pointer
#                     that jumps onto the find bar, and onto the conflict banner, leaves no
#                     card (`hover=` empty) -- Ely says when the pointer leaves the code
#   problems_unopened  the fake server (fake_lsp.py `publish`) reports an error for a file nobody
#                     opened: the drawer opens itself on Problems, lists it, and the caret does not move
#   problems_alarm    the seen set (spec 4.4) end to end: the same error rewritten, or cleared and
#                     republished, leaves a closed drawer closed; an error with a new message opens it;
#                     a seen error fixed and returning identically does not (the accepted cost);
#                     warnings never; on Search it stays on Search
#   problems_relaunch two errors seen and closed, a quit and relaunch, the server reports them one
#                     file at a time: the drawer stays closed; a new message opens it
#   problems_open     a problem's row opens its file with the caret at the needle's byte offset (the
#                     server's column is UTF-16, past an accent and an emoji)
#   problems_server_exit  a server that ends takes its problems with it and is no longer named; the
#                     next .rs file starts it again; after a relaunch a running server with nothing to
#                     say is named
#   problems_park     a second worktree has a drawer of its own; the first, switched away and back,
#                     keeps its drawer, height and problems, one that arrived while parked included
#   search_keys       Ctrl+Shift+F (a real X key) opens the drawer on Search with the selection as the query;
#                     the keyboard is in the query field (a typed key lands there), and Escape in it
#                     hands the keyboard to the code of the tab
#   search_scope      the worktree is searched as git lists it: an untracked file is found, an ignored
#                     one is not; the summary names what was left unread
#   search_options    case, whole word and a regular expression count what the in-file find counts on the
#                     same file; an invalid pattern is the summary
#   search_dirty      an unsaved edit is found in its buffer (never on disk), listed as unsaved, and its
#                     result opens at the buffer position
#   search_open       a result opens its file with the match selected; that tab's drawer shows the same
#                     search with the result chosen, and the next result keeps the thread; captured
#   search_supersede  a second query typed while a slow first run is under way: only the second's results stand
#   search_cap        more than 20 000 matches stop at 20 000 and the summary says so
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
#          [--out-dir DIR] [--appearance light|dark] [--only NAME[,NAME...]]
#
#   --xvfb        start a private Xvfb on a free display between :90 and :99
#   --display :N  use an Xvfb this user started (never your own $DISPLAY)
#   --state-only  take no window captures and no capture comparisons; the window
#                 still draws, so the editor's state is read from a drawn app
#   --appearance  seed light or dark (default dark)
#   --only        run the named scenarios alone and print "EDITOR E2E PARTIAL"
#                 instead of "EDITOR E2E OK"; only the whole run is the artifact

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
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --appearance) APPEARANCE="${2:?--appearance needs light or dark}"; shift 2 ;;
    --only) ONLY=",${2:?--only needs scenario names},"; shift 2 ;;
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
elif command == "offset-nth":
    # offset-nth ESCAPED_TEXT NEEDLE N: the byte offset of the Nth (0-based) NEEDLE.
    text = unescape(sys.argv[2])
    index = -1
    for _ in range(int(sys.argv[4]) + 1):
        index = text.find(sys.argv[3], index + 1)
        if index < 0:
            sys.exit(f"'{sys.argv[3]}' has no occurrence {sys.argv[4]}")
    sys.stdout.write(str(len(text[:index].encode("utf-8"))))
elif command == "line-of":
    # line-of ESCAPED_TEXT NEEDLE: the 0-based line NEEDLE is on.
    text = unescape(sys.argv[2])
    index = text.find(sys.argv[3])
    if index < 0:
        sys.exit(f"'{sys.argv[3]}' is not in the text")
    sys.stdout.write(str(text[:index].count("\n")))
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
elif command == "sub":
    # sub SRC DST NEEDLE WITH COUNT NOCASE: SRC with NEEDLE replaced by WITH, the
    # first COUNT occurrences (0 for all), ignoring case when NOCASE is 1.
    import re
    text = read(sys.argv[2])
    flags = re.IGNORECASE if sys.argv[7] == "1" else 0
    text = re.sub(re.escape(sys.argv[4]), lambda _: sys.argv[5], text, count=int(sys.argv[6]), flags=flags)
    with open(sys.argv[3], "wb") as handle:
        handle.write(text.encode("utf-8"))
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
# findme.txt: what the find scenarios search. "fn" four times in three cases
# ("fnord" counts), an emoji twice, and filler to make the minimap draw.
findme = [
    "fn first() {}\n",
    'let smile = "\U0001F600";\n',
    "Fn third\n",
    "FN fourth\n",
    "// \U0001F600 once more and fnord\n",
    "let x = 1;\n",
]
findme += [f"plain filler line {n}\n" for n in range(1, 71)]
write("findme.txt", "".join(findme).encode("utf-8"))
# src/shapes.rs: what the breadcrumb scenario walks: a function, a struct, an impl
# with two methods, and a function after it. The fake server's outline (below)
# names each by the text that opens it. Line 0 mentions no symbol's name.
os.makedirs(os.path.join(root, "src"))
shapes = [
    "// shapes.rs: a free function, a struct, two methods and a last function.\n",
    "fn free() -> u32 {\n",
    "    1\n",
    "}\n",
    "\n",
    "struct Foo;\n",
    "\n",
    "impl Foo {\n",
    "    fn first(&self) -> u32 {\n",
    "        2\n",
    "    }\n",
    "\n",
    "    fn bar(&self) -> u32 {\n",
    "        3\n",
    "    }\n",
    "}\n",
    "\n",
    "fn tail() -> u32 {\n",
    "    4\n",
    "}\n",
]
write("src/shapes.rs", "".join(shapes).encode("utf-8"))
# The same text under a deep path: a breadcrumb that cannot fit a narrow pane.
os.makedirs(os.path.join(root, "crates/sirio_ui/src/file_view/nested"))
write("crates/sirio_ui/src/file_view/nested/deep_shapes.rs", "".join(shapes).encode("utf-8"))
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
  "definition": {"path": "$WT/nested.rs", "needle": "fn outer"},
  "symbols_file": ["shapes.rs", "deep_shapes.rs"],
  "symbols": [
    {"name": "free", "kind": 12, "needle": "fn free"},
    {"name": "Foo", "kind": 23, "needle": "struct Foo"},
    {"name": "impl Foo", "kind": 5, "needle": "impl Foo", "children": [
      {"name": "first", "kind": 6, "needle": "fn first"},
      {"name": "bar", "kind": 6, "needle": "fn bar"}
    ]},
    {"name": "tail", "kind": 12, "needle": "fn tail"}
  ]
}
JSON
# The drawer opens itself on an error the user has not seen, and the fake server
# reports FAKE_ERROR for main.rs in every scenario: recorded as seen, as it is for
# a user who closed the drawer on it, so only the problems scenarios (which bring
# errors of their own) ever open it.
python3 - "$SIRIO_DB" "$WT" "$FAKE_ERROR" <<'PY'
import json, sqlite3, sys
database, worktree, message = sys.argv[1:4]
connection = sqlite3.connect(database)
connection.execute(
    "INSERT INTO editor_drawer (worktree_path, open, height, tab, seen_errors) "
    "VALUES (?, 0, 240.0, 'problems', ?) "
    "ON CONFLICT(worktree_path) DO UPDATE SET seen_errors = excluded.seen_errors",
    (worktree, json.dumps([[worktree, "rust", "main.rs", message, 1]])))
connection.commit()
connection.close()
PY
# The server's program is a stub on the app's PATH, so the status row names it
# the way it names a real one (`fake-lsp`, as it names `rust-analyzer`).
# `slow-lsp` is the same server that takes two seconds to start: the status
# scenario opens two files while its launch is still running.
cat >"$RUN_DIR/stubs/fake-lsp" <<STUB
#!/bin/sh
exec python3 -I "$ROOT/Scripts/Tests/fake_lsp.py" "\$@"
STUB
cat >"$RUN_DIR/stubs/slow-lsp" <<STUB
#!/bin/sh
sleep 2
exec python3 -I "$ROOT/Scripts/Tests/fake_lsp.py" "\$@"
STUB
chmod +x "$RUN_DIR/stubs/fake-lsp" "$RUN_DIR/stubs/slow-lsp"
cat >"$XDG_CONFIG_HOME/sirio/languages.toml" <<TOML
[[language]]
name = "rust"
extensions = ["rs"]
command = "fake-lsp"
args = ["$RUN_DIR/fake-lsp/answers.json", "$RUN_DIR/fake-lsp/requests.log"]
TOML

# ---------------------------------------------------------------- the app

# The app's PATH: this script's, without a `zls`. The status scenario needs a
# .zig file whose server is not installed, and a machine with zls beside its
# Zig would answer it. A directory holding a zls (a system bin directory, say)
# cannot simply be dropped, so a directory of links to everything else in it
# stands in its place.
app_path_without_zls() {
  local dir entry copy kept="" n=0 old_ifs="$IFS"
  IFS=:
  for dir in $PATH; do
    if [ -n "$dir" ] && [ -f "$dir/zls" ] && [ -x "$dir/zls" ]; then
      n=$((n + 1))
      copy="$RUN_DIR/path-without-zls/$n"
      mkdir -p "$copy"
      for entry in "$dir"/*; do
        [ "${entry##*/}" = zls ] || ln -s "$entry" "$copy/"
      done
      echo "NOTE: $dir/zls is on this machine's PATH; the app's PATH leaves it out ($copy stands in for $dir)" >&2
      dir="$copy"
    fi
    kept="${kept:+$kept:}$dir"
  done
  IFS="$old_ifs"
  printf '%s' "$kept"
}
APP_PATH="$(app_path_without_zls)"

app_start() {
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 PATH="$APP_PATH" \
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
# expect_state, then the same value again after a pause: a value that is true
# for one frame (a panel closing and opening itself) does not pass.
expect_state_stays() { # key value
  local got
  expect_state "$1" "$2"
  sleep 0.6
  read_state
  got="$(sfield "$1")"
  [ "$got" = "$2" ] \
    || fail "$SCENARIO: $1 was $2 and a moment later $got"
}
# What the code's highlighter last gave the first line (`highlight`:
# `<language id>;spans=<n>`), polled: LANGUAGE is the one it was built with and
# COLOURED is 1 for at least one coloured span, 0 for none. A highlighter
# that serves another language's cached colours reads here as spans on a
# language that has none.
expect_highlight() { # language coloured
  local got="" spans tries=0
  while [ "$tries" -lt 150 ]; do
    read_state
    got="$(sfield highlight)"
    case "$got" in
      "$1;spans="*)
        spans="${got#*;spans=}"
        if { [ "$2" = 1 ] && [ "$spans" -gt 0 ]; } || { [ "$2" = 0 ] && [ "$spans" = 0 ]; }; then
          echo "OK: highlight=$got"
          return 0
        fi
        ;;
    esac
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: highlight expected $1 with coloured=$2, got '$got'"
}
# One field of the status row (`status`: `branch=main;ahead=1;...`), polled the
# way expect_state polls a key. The row's values hold no `;`.
status_field() { # key
  sfield status | tr ';' '\n' | sed -n "s/^$1=//p" | head -n 1
}
expect_status() { # key value
  local got="" tries=0
  while [ "$tries" -lt 150 ]; do
    read_state
    got="$(status_field "$1")"
    if [ "$got" = "$2" ]; then
      echo "OK: status $1=$2"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: status $1 expected $2, got $got (status: $(sfield status))"
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
# The pointer warped to (X, Y), counted from the window's corner, with no press: one
# motion event at the destination, none on the way. The same window-size guard.
move_to() { # x y
  local window size
  window="$(find_window)"
  [ -n "$window" ] || fail "$SCENARIO: no window to move the pointer in"
  size="$(xwininfo -display "$DISPLAY_TARGET" -id "$window" | awk '/Width:/ {w = $2} /Height:/ {h = $2} END {print w "x" h}')"
  [ "$size" = "$CLICK_WINDOW" ] || fail "$SCENARIO: the window is $size, not the $CLICK_WINDOW these moves are placed for"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$1" "$2"
}
# The app's window takes X input focus. Without a window manager a click-away
# does not blur a Settings field (the Blur never commits, measured on this
# Xvfb); with the window focused it does.
focus_window() {
  local window
  window="$(find_window)"
  [ -n "$window" ] || fail "$SCENARIO: no window to focus"
  # --sync waits for the focus to land and has no bound of its own.
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool windowfocus --sync "$window" \
    || fail "$SCENARIO: the window could not take X input focus within 10 s"
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
  if [ -n "$ONLY" ]; then
    case "$ONLY" in *",$1,"*) ;; *) return 0 ;; esac
  fi
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
  # The seed's precondition (see the setup's `editor_drawer` row): the fake server
  # reports FAKE_ERROR for main.rs, the drawer holds it as seen and stays closed.
  # When the app's seen-set row shape or its worktree key changes, the seed stops
  # matching and the drawer opens here, over every scenario that follows.
  ctl surface editor open --path="$WT/main.rs" >/dev/null
  settle
  expect_state problems 1/1/0
  sleep 0.8
  read_state
  [ "$(sfield drawer_open)" = 0 ] \
    || fail "$SCENARIO: the seeded editor_drawer row (worktree path, seen errors as [root, language, path, message, count]) no longer matches what the app persists: the drawer opened itself on main.rs's seen error. Update the seed in this script's setup."
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
  # The banner is drawn on a danger-subtle surface under the normal text colour:
  # the capture is for the eye (readable text, in both appearances).
  capture conflict-banner
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
  expect_state format_buttons shown
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
# How many tabs the window holds. The list `surface tabs read` gives is the one
# `surface tabs close` takes positions from, and a new tab is appended to it.
tab_count() {
  "$CTL" surface tabs read --json | python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["count"])'
}
# The positions of the window's snapshot tabs: a tab is "kind|snapshot|title".
snapshot_tab_positions() {
  "$CTL" surface tabs read --json | python3 -c '
import json, sys
report = json.load(sys.stdin)[0]
for key, value in report.items():
    if key.startswith("tab.") and value.split("|")[1] == "yes":
        print(key[len("tab."):])'
}
# One key of `surface layout read`, such as secondaryVisible.
layout_field() { # key
  "$CTL" surface layout read --json | python3 -c 'import json, sys; print(json.load(sys.stdin)[0][sys.argv[1]])' "$1"
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
# A check reads only the lines after the mark its caller took before its verb
# (lsp_mark), so a request an earlier scenario made cannot pass it.
lsp_mark() { # the log's line count now
  local log="$RUN_DIR/fake-lsp/requests.log"
  if [ -f "$log" ]; then wc -l <"$log"; else echo 0; fi
}
# How many requests of a method the log holds after the mark.
lsp_count() { # method mark
  tail -n +"$(($2 + 1))" "$RUN_DIR/fake-lsp/requests.log" 2>/dev/null | grep -c " $1\$" || true
}
lsp_asked() { # method mark
  tail -n +"$(($2 + 1))" "$RUN_DIR/fake-lsp/requests.log" 2>/dev/null | grep -q " $1\$" \
    || fail "$SCENARIO: the language server was never asked $1 after the verb"
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
  local mark
  mark="$(lsp_mark)"
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor type --text=x >/dev/null
  expect_state dirty true
  # The findings move with the text: the Error one byte later, and both stay.
  expect_state diagnostics 2
  # error_at is where line_7 is in the text now, read from the text and not from
  # the findings: the check below proves the text moved one byte. That the Error
  # moved with it is what the hover pair proves, since the card asks the editor's
  # findings at the new place and at the old one.
  error_at="$(python3 "$TOOLS" offset "$(sfield text)" "line_7")"
  [ "$error_at" -eq $((old_at + 1)) ] \
    || fail "$SCENARIO: the Error is at $error_at, not one byte past $old_at, after the typed character"
  # The card reads the editor's findings, so the Error answers at its new place.
  ctl surface editor hover --offset="$error_at" >/dev/null
  expect_state hover "$FAKE_ERROR\\n$FAKE_HOVER"
  # The verb rests the pointer at the window centre, so the card is drawn there,
  # not beside the Error: this capture shows the squiggle and the card's text.
  capture diagnostics-hover
  # At the Error's old place there is a space now. Ely moved the finding, so
  # nothing covers that place; Sirio's own copy of the findings still names it,
  # but the card does not read that copy. The card shows the server's answer alone.
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
  lsp_asked textDocument/didChange "$mark"
  echo "OK: the findings moved with the typed character, the card followed them, and undo's restore gave them back"
}

scenario_hover() {
  open_file "$WT/main.rs"
  expect_state state loaded
  local at mark
  at="$(python3 "$TOOLS" offset "$(sfield text)" "line_1")"
  mark="$(lsp_mark)"
  ctl surface editor hover --offset="$at" >/dev/null
  # Nothing is under line_1 but the server's answer, which the card then shows.
  expect_state hover "$FAKE_HOVER"
  lsp_asked textDocument/hover "$mark"
  echo "OK: the hover at line_1 is the server's answer"
}

scenario_definition() {
  open_file "$WT/calls.rs"
  expect_state state loaded
  local at line_six mark
  at="$(python3 "$TOOLS" offset "$(sfield text)" "outer(1)")"
  mark="$(lsp_mark)"
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
  lsp_asked textDocument/definition "$mark"
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
  # Refused: with no selection the press leaves Copy unavailable, so the row's
  # click refuses with its reason, and the verb answers that reason as its error.
  # The press itself still moves the caret to where it landed.
  local refused=""
  if refused="$(ctl surface editor context --offset="$first" --action=copy 2>&1)"; then
    fail "$SCENARIO: copy with no selection was accepted; the verb answered: $refused"
  fi
  case "$refused" in
    *"sirioctl: Select some text first"*) ;;
    *) fail "$SCENARIO: copy with no selection answered '$refused', expected 'Select some text first'" ;;
  esac
  expect_state selections "$first..$first"
  echo "OK: copy with no selection is refused: Select some text first"
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
  # Send to agent is `scenario_send_to_agent`.
}

# The composer of the workspace's chat, asked over the control socket as the chat
# E2E (test-ely-chat-ui-e2e.py) asks it: `surface.chat.open` answers the first
# chat tab's state, and `composerText` is its unsent draft. No agent runs for it.
chat_composer() {
  python3 - "$SIRIO_SOCKET" <<'PY'
import json, socket, sys
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
    connection.settimeout(20)
    connection.connect(sys.argv[1])
    connection.sendall(json.dumps({"id": "e2e-chat", "method": "surface.chat.open", "params": {}}).encode() + b"\n")
    buffer = b""
    while not buffer.endswith(b"\n"):
        chunk = connection.recv(65536)
        if not chunk:
            break
        buffer += chunk
reply = json.loads(buffer.decode())
if reply.get("error") or not reply.get("ok", True):
    sys.exit(f"surface.chat.open failed: {reply}")
result = reply.get("result", {})
if isinstance(result, dict) and "composerText" in result:
    sys.stdout.write(result["composerText"])
else:
    sys.exit(f"surface.chat.open answered no composerText: {reply}")
PY
}

scenario_send_to_agent() {
  # calls.rs: no other scenario edits it, so its text is the fixture's whatever ran before.
  open_file "$WT/calls.rs"
  expect_state state loaded
  local text start end composer expected
  text="$(sfield text)"
  start="$(python3 "$TOOLS" offset "$text" "fn caller")"
  end=$(($(python3 "$TOOLS" offset "$text" "outer(1)") + 8))
  [ -z "$(chat_composer)" ] || fail "$SCENARIO: the chat's composer was not empty before the action"
  # Lines 1 and 2 selected; the menu's Send to agent, run on the selection, appends
  # the file, the line range and the lines in a fenced block to the chat's composer.
  ctl surface editor select --ranges="$start..$end" >/dev/null
  ctl surface editor context --offset=$((start + 3)) --action=send-to-agent >/dev/null
  expect_state selections "$start..$end"
  composer="$(chat_composer)"
  expected="$(printf 'calls.rs:1-2\n\n```rust\nfn caller() -> u32 {\n    outer(1)\n```\n')"
  [ "$composer" = "$expected" ] \
    || fail "$SCENARIO: the chat's composer reads $(printf '%s' "$composer" | cut -c1-300 | sed 's/$/<eol>/'), expected $(printf '%s' "$expected" | cut -c1-300 | sed 's/$/<eol>/')"
  echo "OK: the chat's composer holds calls.rs:1-2 and the two selected lines"
  # A second send appends after a blank line instead of replacing the first.
  ctl surface editor select --ranges="$start..$((start + 9))" >/dev/null
  ctl surface editor context --offset=$((start + 3)) --action=send-to-agent >/dev/null
  composer="$(chat_composer)"
  case "$composer" in
    "$expected"*"calls.rs:1"*) ;;
    *) fail "$SCENARIO: a second send did not append to the draft: $(printf '%s' "$composer" | cut -c1-400)" ;;
  esac
  echo "OK: a second send is appended to the draft, the first kept"
}

scenario_breadcrumb() {
  # The row over the code reads the path from the worktree's root, then the
  # symbols the fake server's outline (answers.json, `symbols`) puts around the
  # caret. A symbol is placed by its start line: it holds the caret from there
  # until a symbol at its depth or a shallower one starts.
  local file="$WT/src/shapes.rs" base="src › shapes.rs" text mark
  local at_first at_bar at_tail impl_at refused pressed
  mark="$(lsp_mark)"
  open_file "$file"
  expect_state state loaded
  text="$(sfield text)"
  at_first="$(python3 "$TOOLS" offset "$text" "        2")"
  at_bar="$(python3 "$TOOLS" offset "$text" "        3")"
  at_tail="$(python3 "$TOOLS" offset "$text" "    4")"
  impl_at="$(python3 "$TOOLS" offset "$text" "impl Foo {")"

  # In bar's body: its impl, then bar. Reading this waits for the server's answer,
  # which the load asks for as soon as the server has the text.
  ctl surface editor select --ranges="$at_bar..$at_bar" >/dev/null
  expect_state breadcrumb "$base › impl Foo › bar"
  lsp_asked textDocument/documentSymbol "$mark"
  capture breadcrumb

  # A sibling replaces its sibling: first's body is in impl Foo, and not in bar.
  ctl surface editor select --ranges="$at_first..$at_first" >/dev/null
  expect_state breadcrumb "$base › impl Foo › first"
  ctl surface editor select --ranges="$at_bar..$at_bar" >/dev/null
  expect_state breadcrumb "$base › impl Foo › bar"
  # A shallower symbol closes the deeper ones: tail begins after bar's method.
  ctl surface editor select --ranges="$at_tail..$at_tail" >/dev/null
  expect_state breadcrumb "$base › tail"
  # Above every symbol there is only the path.
  ctl surface editor select --ranges=0..0 >/dev/null
  expect_state breadcrumb "$base"
  echo "OK: the chain follows the caret through a method, a sibling, a shallower symbol and the top"

  # A press on a symbol reveals its line: the caret goes to the start of the impl.
  ctl surface editor select --ranges="$at_bar..$at_bar" >/dev/null
  expect_state breadcrumb "$base › impl Foo › bar"
  pressed="$(ctl surface editor crumb --index=2)"
  # The reply is what the press left, read before any later state is: the caret
  # line is current in it, not the one the press started from.
  case "$pressed" in
    *"breadcrumb=$base › impl Foo"$'\n'* | *"breadcrumb=$base › impl Foo") ;;
    *) fail "$SCENARIO: the press' own reply did not name impl Foo as the last part: $pressed" ;;
  esac
  expect_state selections "$impl_at..$impl_at"
  expect_state breadcrumb "$base › impl Foo"
  # The path's parts are text, and a part that is not there is refused.
  refused=""
  if refused="$(ctl surface editor crumb --index=0 2>&1)"; then
    fail "$SCENARIO: a press on the path was accepted; the verb answered: $refused"
  fi
  case "$refused" in
    *'"src" is part of the path, not a symbol'*) ;;
    *) fail "$SCENARIO: a press on the path answered '$refused'" ;;
  esac
  if refused="$(ctl surface editor crumb --index=9 2>&1)"; then
    fail "$SCENARIO: a press past the last part was accepted; the verb answered: $refused"
  fi
  expect_state selections "$impl_at..$impl_at"
  echo "OK: a press on impl Foo put the caret on its line; the path and a missing part are refused"

  # Five characters typed one after another are one ask for the symbols, made
  # after the pause (1 s) and none during the typing. The fake's outline gains a
  # symbol named by the typed text, so the answer that lands is the new one.
  python3 - "$RUN_DIR/fake-lsp/answers.json" <<'PY'
import json, os, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    answers = json.load(handle)
answers["symbols"].append({"name": "typed", "kind": 12, "needle": "zzzzz"})
with open(path + ".tmp", "w", encoding="utf-8") as handle:
    json.dump(answers, handle)
os.replace(path + ".tmp", path)
PY
  ctl surface editor select --ranges=0..0 >/dev/null
  expect_state breadcrumb "$base"
  # The burst is timed: a window that draws under load can stretch one gap past
  # the pause, and then the pause rightly asked in the middle of the typing. A
  # slow burst is undone and run once more; a second slow one keeps the ask
  # counts that depend on a clean burst as "at least" and says so.
  local slow=1 attempt gap_before gap_after gaps
  for attempt in 1 2; do
    mark="$(lsp_mark)"
    slow=0
    gaps=""
    gap_before="$(date +%s.%N)"
    for _ in 1 2 3 4 5; do
      ctl surface editor type --text=z >/dev/null
      gap_after="$(date +%s.%N)"
      gaps="$gaps $(awk -v a="$gap_before" -v b="$gap_after" 'BEGIN { printf "%.2f", b - a }')"
      if awk -v a="$gap_before" -v b="$gap_after" 'BEGIN { exit !(b - a >= 0.9) }'; then
        slow=1
      fi
      gap_before="$gap_after"
    done
    echo "burst $attempt: gaps in seconds:$gaps"
    [ "$slow" -eq 0 ] && break
    if [ "$attempt" -eq 1 ]; then
      echo "NOTE: $SCENARIO: a gap in the typing burst reached 0.9 s; undoing it and typing it again"
      sleep 1.5
      for _ in 1 2 3 4 5; do ctl surface editor keys --keys=ctrl-z >/dev/null; done
      expect_state dirty false
      sleep 1.5
    fi
  done
  if [ "$slow" -eq 0 ]; then
    [ "$(lsp_count textDocument/documentSymbol "$mark")" -eq 0 ] \
      || fail "$SCENARIO: the symbols were asked for during the typing, not after the pause"
  else
    echo "NOTE: $SCENARIO: the typing burst was slow twice; not asserting that no ask came during it, nor an exact ask count after it"
  fi
  expect_state breadcrumb "$base › typed"
  sleep 1.5
  if [ "$slow" -eq 0 ]; then
    [ "$(lsp_count textDocument/documentSymbol "$mark")" -eq 1 ] \
      || fail "$SCENARIO: five typed characters asked for the symbols $(lsp_count textDocument/documentSymbol "$mark") times, expected once"
    echo "OK: five characters, one ask for the symbols, after the pause"
  else
    [ "$(lsp_count textDocument/documentSymbol "$mark")" -ge 1 ] \
      || fail "$SCENARIO: the typing was never followed by an ask for the symbols"
  fi

  # Undo returns the text; the symbol named by the typed text goes with it, once
  # the pause has passed and the symbols are asked for again. The outline the
  # fake answers loses its added symbol, as it was at the start.
  for _ in 1 2 3 4 5; do ctl surface editor keys --keys=ctrl-z >/dev/null; done
  expect_state dirty false
  python3 - "$RUN_DIR/fake-lsp/answers.json" <<'PY'
import json, os, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    answers = json.load(handle)
answers["symbols"] = [symbol for symbol in answers["symbols"] if symbol["name"] != "typed"]
with open(path + ".tmp", "w", encoding="utf-8") as handle:
    json.dump(answers, handle)
os.replace(path + ".tmp", path)
PY
  expect_state breadcrumb "$base"
  sleep 1.5

  # A save asks for the symbols with no edit behind it, so the pause after an edit
  # cannot be what asks: the text is as it was when the pauses above ended. The ask
  # is polled for (a busy window delays it), and it must come once.
  mark="$(lsp_mark)"
  ctl surface editor save >/dev/null
  local tries=0
  while [ "$tries" -lt 50 ] && [ "$(lsp_count textDocument/documentSymbol "$mark")" -eq 0 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ "$(lsp_count textDocument/documentSymbol "$mark")" -ge 1 ] \
    || fail "$SCENARIO: a save never asked for the symbols"
  sleep 2
  [ "$(lsp_count textDocument/documentSymbol "$mark")" -eq 1 ] \
    || fail "$SCENARIO: after a save the symbols were asked for $(lsp_count textDocument/documentSymbol "$mark") times, expected once"
  echo "OK: a save asked for the symbols, once, with no edit to wait for"

  # A deep path in a narrow pane: the state still reports the whole logical
  # breadcrumb, and the capture shows the symbols in view, the directories cut
  # with an ellipsis and not the symbols.
  local deep="$WT/crates/sirio_ui/src/file_view/nested/deep_shapes.rs"
  local deep_base="crates › sirio_ui › src › file_view › nested › deep_shapes.rs"
  open_file "$deep"
  expect_state state loaded
  text="$(sfield text)"
  at_bar="$(python3 "$TOOLS" offset "$text" "        3")"
  ctl surface editor select --ranges="$at_bar..$at_bar" >/dev/null
  expect_state breadcrumb "$deep_base › impl Foo › bar"
  capture breadcrumb-deep
  echo "OK: a deep path keeps the whole breadcrumb in the state; the capture shows it in the pane"
}

scenario_snapshot() {
  # A file at a commit opens as a change request's file does (the debug verb
  # open-snapshot: git's blob, the read-only tab). It carries one banner, the
  # view's own, and refuses every edit: typing, a paste, the menu's Cut and
  # Paste and replace-all each leave the text byte-identical, with something on
  # the clipboard to paste and a selection to cut, so a refusal is the cause.
  # A revision's file the worktree no longer has offers no local file, and the
  # tabs the scenario opened are closed at its end.
  local sha label snap_text refused out tabs_before secondary_before gone_sha
  sha="$(git -C "$WT" rev-parse HEAD)"
  label="Read-only · #7 at ${sha:0:7}"
  tabs_before="$(tab_count)"
  secondary_before="$(layout_field secondaryVisible)"

  # Something on the clipboard: copied out of findme.txt, shown by a paste into
  # the empty scratch file, then undone.
  open_file "$WT/findme.txt"
  expect_state state loaded
  ctl surface editor select --ranges=0..5 >/dev/null
  ctl surface editor keys --keys=ctrl-c >/dev/null
  open_file "$WT/empty.txt"
  expect_state state loaded
  ctl surface editor keys --keys=ctrl-v >/dev/null
  expect_state text "fn fi"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text ""
  expect_state dirty false
  echo "OK: the clipboard holds 'fn fi'"

  git -C "$WT" show "$sha:src/shapes.rs" >"$RUN_DIR/snap-0.txt"
  snap_text="$(escaped "$RUN_DIR/snap-0.txt")"
  ctl surface editor open-snapshot --sha="$sha" --path=src/shapes.rs >/dev/null
  settle
  expect_state state loaded
  expect_state snapshot true
  expect_state read_only true
  expect_state banner "$label"
  expect_state banner_action "Open local file"
  expect_state ely_banner false
  expect_state text "$snap_text"
  expect_state dirty false
  expect_state language Rust
  echo "OK: src/shapes.rs at HEAD opens read-only, with the one banner and its local-file action"

  # Typing and the keyboard's paste.
  ctl surface editor select --ranges=0..6 >/dev/null
  ctl surface editor type --text=Q >/dev/null
  ctl surface editor keys --keys=ctrl-v >/dev/null
  sleep 1
  expect_state text "$snap_text"
  expect_state dirty false
  echo "OK: typing and a paste leave the snapshot as it was"

  # The menu: Cut and Paste refuse with the snapshot's reason (the selection is
  # there, so it is not the missing selection that refuses them), Copy is
  # accepted.
  ctl surface editor select --ranges=0..6 >/dev/null
  for action in cut paste; do
    if out="$(ctl surface editor context --offset=2 --action=$action 2>&1)"; then
      fail "$SCENARIO: $action on a snapshot was accepted; the verb answered: $out"
    fi
    case "$out" in
      *"sirioctl: A revision cannot be changed"*) ;;
      *) fail "$SCENARIO: $action on a snapshot answered '$out', expected 'A revision cannot be changed'" ;;
    esac
  done
  ctl surface editor context --offset=2 --action=copy >/dev/null
  sleep 1
  expect_state text "$snap_text"
  expect_state dirty false
  echo "OK: the menu refuses Cut and Paste on a snapshot with its reason, and accepts Copy"

  # Find works on a snapshot; replace is refused.
  local found
  found="$(grep -io fn "$RUN_DIR/snap-0.txt" | wc -l | tr -d ' ')"
  [ "$found" -gt 0 ] || fail "$SCENARIO: the fixture holds no fn"
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor find --query=fn --replace=Y >/dev/null
  expect_state find_matches "$found"
  if out="$(ctl surface editor replace --all=true 2>&1)"; then
    fail "$SCENARIO: replace-all on a snapshot was accepted; the verb answered: $out"
  fi
  case "$out" in
    *"read-only"*) ;;
    *) fail "$SCENARIO: replace-all on a snapshot answered '$out', expected a read-only refusal" ;;
  esac
  sleep 1
  expect_state text "$snap_text"
  expect_state dirty false
  expect_state find_matches "$found"
  echo "OK: find counts $found matches in the snapshot; replace-all is refused and the text is unchanged"
  capture snapshot
  ctl surface editor find-close >/dev/null

  # A Markdown snapshot, in Code: the formatting buttons are not drawn. The
  # same state reads `shown` for an ordinary Markdown file in Code (scenario
  # markdown), so the value here is the snapshot's own, not a default.
  git -C "$WT" show "$sha:notes.md" >"$RUN_DIR/snap-md.txt"
  ctl surface editor open-snapshot --sha="$sha" --path=notes.md >/dev/null
  settle
  expect_state snapshot true
  expect_state banner "$label"
  expect_state banner_action "Open local file"
  expect_state text "$(escaped "$RUN_DIR/snap-md.txt")"
  ctl surface editor mode --mode=code >/dev/null
  expect_state mode code
  expect_state format_buttons hidden
  ctl surface editor type --text=Q >/dev/null
  sleep 1
  expect_state text "$(escaped "$RUN_DIR/snap-md.txt")"
  expect_state dirty false
  capture snapshot-md
  echo "OK: a Markdown snapshot in Code refuses typing and is captured with its row"

  # A path outside the repository tree and a short sha are refused before git.
  if out="$(ctl surface editor open-snapshot --sha="${sha:0:7}" --path=notes.md 2>&1)"; then
    fail "$SCENARIO: a short sha was accepted: $out"
  fi
  if out="$(ctl surface editor open-snapshot --sha="$sha" --path=../escape.txt 2>&1)"; then
    fail "$SCENARIO: a path that climbs was accepted: $out"
  fi
  echo "OK: a short sha and a climbing path are refused"

  # A file the revision has and the worktree does not: committed, then removed
  # from the worktree and the index by a second commit, so the worktree is clean
  # again. The bar has no local file to offer, so banner_action is empty, and
  # Ely's banner is still not drawn. sfield reads a missing key as empty too, so
  # the banner, reported with banner_action, is checked first.
  printf 'the revision keeps this file\n' >"$WT/gone.txt"
  git -C "$WT" add gone.txt
  git -C "$WT" commit -q -m "throwaway file for the snapshot scenario"
  gone_sha="$(git -C "$WT" rev-parse HEAD)"
  git -C "$WT" rm -q gone.txt
  git -C "$WT" commit -q -m "remove the throwaway file again"
  git -C "$WT" show "$gone_sha:gone.txt" >"$RUN_DIR/snap-gone.txt"
  ctl surface editor open-snapshot --sha="$gone_sha" --path=gone.txt >/dev/null
  settle
  expect_state state loaded
  expect_state snapshot true
  expect_state banner "Read-only · #7 at ${gone_sha:0:7}"
  expect_state banner_action ""
  expect_state ely_banner false
  expect_state text "$(escaped "$RUN_DIR/snap-gone.txt")"
  expect_state dirty false
  echo "OK: a revision's file the worktree lacks opens with no local file to offer"

  # Closes what the scenario opened, newest first: the snapshots, and the two
  # files it copied the clipboard through. The Secondary pane is drawn only while
  # it holds a tab, so it goes with them when this scenario opened it.
  local count
  count="$(tab_count)"
  while [ "$count" -gt "$tabs_before" ]; do
    ctl surface tabs close "$count" >/dev/null
    count=$((count - 1))
  done
  [ -z "$(snapshot_tab_positions)" ] \
    || fail "$SCENARIO: a snapshot tab is still open at $(snapshot_tab_positions | tr '\n' ' ')"
  [ "$(tab_count)" = "$tabs_before" ] \
    || fail "$SCENARIO: the window has $(tab_count) tabs, expected $tabs_before"
  [ "$(layout_field secondaryVisible)" = "$secondary_before" ] \
    || fail "$SCENARIO: secondaryVisible is $(layout_field secondaryVisible), expected $secondary_before"
  echo "OK: the scenario's tabs are closed: $tabs_before tabs, secondaryVisible=$secondary_before"
}

scenario_git_marks() {
  # The marks read each file against HEAD with no context lines. This repository
  # asks git to merge hunks that lie within three unchanged lines of each other
  # (the user's diff.interHunkContext), and the edits below depend on it: line 20
  # is deleted and line 22 rewritten, one unchanged line apart. Without the
  # zero-context flag git prints the line between them as context, and the marks
  # would be wrong or the debug build would stop.
  git -C "$WT" config diff.interHunkContext 3
  python3 - "$RUN_DIR" "$WT/marks.txt" <<'GIT_MARKS_PY'
import os, sys
run, marks = sys.argv[1], sys.argv[2]
head = [f"line {n}\n" for n in range(1, 41)]
edit = list(head)
edit[9] = "line ten\n"             # line 10 rewritten
del edit[19]                       # line 20 deleted
edit[20] = "line twenty-two\n"     # line 22, now index 20, rewritten
at = edit.index("line 30\n") + 1
edit[at:at] = ["added x\n", "added y\n"]   # two lines after line 30
adopt = list(edit)
adopt[4] = "line five\n"           # line 5 rewritten, adopted by the clean buffer

def write(path, lines):
    with open(path, "wb") as handle:
        handle.write("".join(lines).encode("utf-8"))

write(marks, head)
write(os.path.join(run, "marks-head.txt"), head)
write(os.path.join(run, "marks-edit.txt"), edit)
write(os.path.join(run, "marks-adopt.txt"), adopt)
write(os.path.join(run, "outside.txt"), ["outside the repository\n"])
GIT_MARKS_PY
  git -C "$WT" add marks.txt
  git -C "$WT" commit -q -m marks

  # Changed on disk before the file opens: the open reads the disk against HEAD.
  write_disk "$WT/marks.txt" "$RUN_DIR/marks-edit.txt"
  open_file "$WT/marks.txt"
  expect_state state loaded
  expect_state dirty false
  expect_state git_marks "9:modified,18:deleted,20:modified,29:added,30:added"
  expect_state diff_open none
  echo "OK: the open file's marks are the disk's changes against HEAD"

  # A modified line's mark opens its hunk in place, and the same press closes it.
  ctl surface editor git-mark --line=9 >/dev/null
  expect_state diff_open 9
  capture git-marks-open
  ctl surface editor git-mark --line=9 >/dev/null
  expect_state diff_open none

  # A deletion's mark is the line above the gap, and Escape closes the hunk it opens.
  ctl surface editor git-mark --line=18 >/dev/null
  expect_state diff_open 18
  capture git-marks-deleted-open
  ctl surface editor keys --keys=escape >/dev/null
  expect_state diff_open none
  echo "OK: a modified and a deleted hunk open in place; Escape closes the deleted one"

  # Another program's change is adopted by the clean buffer, and the marks follow it.
  write_disk "$WT/marks.txt" "$RUN_DIR/marks-adopt.txt"
  expect_state dirty false
  expect_state git_marks "4:modified,9:modified,18:deleted,20:modified,29:added,30:added"
  echo "OK: an adopted change refreshes the marks"

  # A save changes the diff: a line typed at the end adds its mark once saved.
  local end
  end="$(python3 "$TOOLS" length "$(sfield text)")"
  ctl surface editor select --ranges="$end..$end" >/dev/null
  ctl surface editor type --text=$'appended\n' >/dev/null
  expect_state dirty true
  ctl surface editor save >/dev/null
  expect_state dirty false
  expect_state git_marks "4:modified,9:modified,18:deleted,20:modified,29:added,30:added,41:added"
  echo "OK: a save refreshes the marks"

  # Restored to HEAD on disk, the clean buffer adopts it and the marks clear.
  write_disk "$WT/marks.txt" "$RUN_DIR/marks-head.txt"
  expect_state git_marks ""
  echo "OK: a file restored to HEAD has no marks"

  # HEAD moves while the file does not: another program's commit leaves every
  # committed line marked, and no disk event announces it. The file is changed on
  # disk again (adopted, marked as at the start), the window takes X focus (gpui
  # sends no focus event without it, the Xvfb having no window manager), a terminal
  # pane opens below and takes focus, and the commit is made in git, not through
  # Sirio. The editor then takes focus the way the verbs that act as the user's keys
  # give it, through the code editor's own focus handle, the call a click makes.
  # That move from the terminal to the editor is what the marks must follow.
  write_disk "$WT/marks.txt" "$RUN_DIR/marks-edit.txt"
  expect_state git_marks "9:modified,18:deleted,20:modified,29:added,30:added"
  focus_window
  ctl pane split down >/dev/null
  sleep 1
  git -C "$WT" commit -q -m "marks committed elsewhere" -- marks.txt
  ctl surface editor keys --keys=escape >/dev/null
  expect_state git_marks ""
  # The terminal pane goes again, so the later scenarios see one pane. The verbs
  # leave the terminal as the focused pane, so the close takes it.
  ctl pane close >/dev/null
  sleep 1
  "$CTL" surface editor state >/dev/null 2>&1 \
    || fail "$SCENARIO: the pane close took the file's pane"
  expect_state path "$WT/marks.txt"
  echo "OK: a commit made elsewhere clears the marks when the file takes focus"

  # A file outside the repository, and a committed clean one, have none. The
  # empty answer is the default, so the wait lets a late answer arrive first.
  open_file "$RUN_DIR/outside.txt"
  expect_state path "$RUN_DIR/outside.txt"
  sleep 1
  expect_state git_marks ""
  open_file "$WT/nonl.txt"
  expect_state path "$WT/nonl.txt"
  sleep 1
  expect_state git_marks ""
  echo "OK: a file outside the repository and a clean file have no marks"

  git -C "$WT" config --unset diff.interHunkContext
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

# The occurrences of NEEDLE in findme.txt as grep counts them (-i: ignoring case).
grep_count() { # flags needle
  grep -o"$1" -- "$2" "$WT/findme.txt" | wc -l | tr -d ' '
}
# Reads the state and fails unless KEY is not empty.
expect_state_set() { # key
  read_state
  [ -n "$(sfield "$1")" ] || fail "$SCENARIO: $1 expected a value, got none"
  echo "OK: $1 is set: $(sfield "$1" | cut -c1-120)"
}

scenario_status() {
  # The row under the file draws what the file and the app know. The app's part
  # is read from git, so the repository gets a real upstream with one commit on
  # each side, and the expected counts are git's own.
  local upstream="$RUN_DIR/status-upstream.git" other="$RUN_DIR/status-other"
  local counts text first second tabs_before secondary_before count sha
  git init -q --bare "$upstream"
  git -C "$WT" remote add origin "$upstream"
  git -C "$WT" push -q -u origin main
  git clone -q -b main "$upstream" "$other"
  git -C "$other" config user.email tester@example.invalid
  git -C "$other" config user.name Tester
  printf 'only upstream\n' >"$other/upstream-only.txt"
  git -C "$other" add upstream-only.txt
  git -C "$other" commit -q -m "a commit only upstream"
  git -C "$other" push -q origin main
  printf 'only here\n' >"$WT/local-only.txt"
  git -C "$WT" add local-only.txt
  git -C "$WT" commit -q -m "a commit only here" -- local-only.txt
  git -C "$WT" fetch -q origin
  counts="$(git -C "$WT" rev-list --left-right --count '@{upstream}...HEAD' | tr '\t' ' ')"
  [ "$counts" = "1 1" ] || fail "$SCENARIO: the fixture's upstream is not one behind and one ahead: behind/ahead $counts"
  tabs_before="$(tab_count)"
  secondary_before="$(layout_field secondaryVisible)"

  # A plain text file with an accent and an emoji: columns count characters.
  printf 'héllo wörld\n😀 emoji\nthird line\n' >"$WT/status-a.txt"
  open_file "$WT/status-a.txt"
  expect_state state loaded
  expect_status branch main
  expect_status ahead 1
  expect_status behind 1
  expect_status lsp none
  expect_status indent "Spaces: 4"
  expect_status encoding UTF-8
  expect_status eol LF
  expect_status language "Plain text"
  expect_status bell 0
  expect_state worktree_dirty true
  ctl surface editor select --ranges=0..0 >/dev/null
  expect_status cursor "Ln 1, Col 1"
  # Two ranges: "héllo" (5 characters, 6 bytes) and "😀 e" (3 characters, 6
  # bytes). The primary caret is the last range's end, the fourth character of
  # line 2 (byte columns would say 7).
  text="$(sfield text)"
  first="$(python3 "$TOOLS" offset "$text" "héllo")"
  second="$(python3 "$TOOLS" offset "$text" "😀")"
  ctl surface editor select --ranges="$first..$((first + 6));$second..$((second + 6))" >/dev/null
  expect_status cursor "Ln 2, Col 4 (8 selected)"
  echo "OK: the cursor counts characters and the selections' total"

  # The fake server, named by the program the entry runs (the stub on PATH,
  # `fake-lsp`), ready for a .rs file.
  printf 'fn a() {}\n// héllo\n' >"$WT/status-c.rs"
  open_file "$WT/status-c.rs"
  expect_state state loaded
  expect_status lsp fake-lsp:ready
  expect_status language Rust
  expect_status branch main
  expect_status ahead 1
  # Captured with a bare caret: the pane is narrow (the row draws its compact set,
  # without indent, encoding and line ending), and a selection's count would take
  # the room the language server's name has.
  ctl surface editor select --ranges=3..3 >/dev/null
  expect_status cursor "Ln 1, Col 4"
  capture status-row
  ctl surface editor select --ranges=3..12 >/dev/null
  expect_status cursor "Ln 2, Col 3 (9 selected)"

  # One launch serves a second file of its language opened while it runs: the
  # entry's program waits two seconds before it starts, two files are opened
  # back to back, and the server sees one handshake and a didOpen for each.
  local slow_log="$RUN_DIR/fake-lsp/slow-requests.log" opens="" tries
  rm -f "$slow_log"
  cat >>"$XDG_CONFIG_HOME/sirio/languages.toml" <<TOML

[[language]]
name = "slow"
extensions = ["slw"]
command = "slow-lsp"
args = ["$RUN_DIR/fake-lsp/answers.json", "$slow_log"]
TOML
  printf 'one\n' >"$WT/status-s1.slw"
  printf 'two\n' >"$WT/status-s2.slw"
  ctl surface editor open --path="$WT/status-s1.slw" >/dev/null
  ctl surface editor open --path="$WT/status-s2.slw" >/dev/null
  expect_status lsp slow-lsp:starting
  expect_status lsp slow-lsp:ready
  for tries in $(seq 1 50); do
    opens="$(grep -c ' textDocument/didOpen$' "$slow_log" 2>/dev/null || true)"
    [ "$opens" = 2 ] && break
    sleep 0.2
  done
  [ "$opens" = 2 ] || fail "$SCENARIO: the slow server was sent $opens didOpen, expected 2 (one per file)"
  [ "$(grep -c ' initialize$' "$slow_log")" = 1 ] \
    || fail "$SCENARIO: the slow server was started more than once: $(grep -c ' initialize$' "$slow_log") initialize"
  echo "OK: two files opened during one launch: one initialize, two didOpen"

  # A language with a server entry that is not installed says nothing.
  printf 'const a = 1;\n' >"$WT/status-b.zig"
  open_file "$WT/status-b.zig"
  expect_state state loaded
  expect_status language Zig
  expect_status lsp none
  expect_status branch main

  # A server that launches and fails: the table is read again at each open, so
  # an entry added now is found. `sh -c "exit 1"` is there and ends at once.
  cat >>"$XDG_CONFIG_HOME/sirio/languages.toml" <<'TOML'

[[language]]
name = "failing"
extensions = ["xyz"]
command = "sh"
args = ["-c", "exit 1"]
TOML
  printf 'nothing to serve\n' >"$WT/status-f.xyz"
  open_file "$WT/status-f.xyz"
  expect_state state loaded
  expect_status lsp sh:failed

  # CRLF, read from the file.
  open_file "$WT/crlf.txt"
  expect_state state loaded
  expect_status eol CRLF

  # Markdown: the Preview hides the code, so it has no cursor to show; Code does.
  open_file "$WT/notes.md"
  expect_state state loaded
  ctl surface editor mode --mode=preview >/dev/null
  expect_state mode preview
  expect_status cursor none
  expect_status language Markdown
  ctl surface editor mode --mode=code >/dev/null
  expect_state mode code
  ctl surface editor select --ranges=0..0 >/dev/null
  expect_status cursor "Ln 1, Col 1"

  # The row's two sets. The Secondary pane is about 430 px with the sidebar, the
  # terminal and the right panel all showing, and the row draws its compact set
  # there. A long branch name (checked out for these captures, with the same
  # upstream) is drawn shortened in the middle, so the server stays whole.
  local long="feature/a-very-long-branch-name-for-the-status-row"
  git -C "$WT" checkout -q -b "$long"
  git -C "$WT" branch -q --set-upstream-to=origin/main
  printf 'fn d() {}\n// héllo wörld\n' >"$WT/status-d.rs"
  open_file "$WT/status-d.rs"
  expect_state state loaded
  expect_status branch "$long"
  expect_status ahead 1
  expect_status lsp fake-lsp:ready
  expect_state status_set compact
  # With the sidebar and the right panel hidden, the pane has room for all of it.
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  expect_state status_set full
  capture status-row-full
  # The sidebar back: the pane is narrower, and a selection takes room too.
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor select --ranges=3..12 >/dev/null
  expect_status cursor "Ln 2, Col 3 (9 selected)"
  capture status-row-narrow
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  expect_state status_set compact
  git -C "$WT" checkout -q main
  git -C "$WT" branch -q -D "$long"

  # A revision has no branch of its own and no server.
  sha="$(git -C "$WT" rev-parse HEAD)"
  ctl surface editor open-snapshot --sha="$sha" --path=src/shapes.rs >/dev/null
  settle
  expect_state snapshot true
  expect_status branch none
  expect_status ahead none
  expect_status lsp none
  expect_status language Rust
  expect_status eol LF
  echo "OK: a snapshot's row names neither a branch nor a server"

  count="$(tab_count)"
  while [ "$count" -gt "$tabs_before" ]; do
    ctl surface tabs close "$count" >/dev/null
    count=$((count - 1))
  done
  [ "$(layout_field secondaryVisible)" = "$secondary_before" ] \
    || fail "$SCENARIO: secondaryVisible is $(layout_field secondaryVisible), expected $secondary_before"
}

# `status` is refused with a message naming WHY (or its first words): the verb
# answers an error and the editor is left as it was.
expect_status_refused() { # needle verb-args...
  local needle="$1" out
  shift
  if out="$("$CTL" surface editor status "$@" 2>&1)"; then
    fail "$SCENARIO: status $* was accepted, expected an error naming '$needle': $out"
  fi
  case "$out" in
    *"$needle"*) echo "OK: status $* refused: $(printf '%s' "$out" | head -n 1 | cut -c1-200)" ;;
    *) fail "$SCENARIO: status $* answered '$out', expected an error naming '$needle'" ;;
  esac
}

# Where the status row's items sit in the window of CLICK_WINDOW when the
# sidebar and the right panel are hidden and main.rs is open in Rust with LF
# and Spaces: 4 (measured on the captures this scenario takes; a window of any
# other size is refused by click_at).
STATUS_CURSOR_ITEM=(1212 775)
STATUS_INDENT_ITEM=(1274 775)
STATUS_LINE_ENDING_ITEM=(1376 775)
STATUS_LANGUAGE_ITEM=(1412 775)
# A point in the terminal pane to the left of the file's.
TERMINAL_AREA=(400 400)
# The two rows of the line ending's panel.
STATUS_LF_ROW=(1310 702)
STATUS_CRLF_ROW=(1318 733)

scenario_status_actions() {
  local eol="$WT/status-eol.txt" ind="$WT/status-ind.txt" main="$WT/main.rs"
  local tabs_before secondary_before count lines crlf offset mark last sha
  tabs_before="$(tab_count)"
  secondary_before="$(layout_field secondaryVisible)"

  # ---- the line ending: an unsaved change, written on every line by a save
  printf 'one\ntwo\nthree\n' >"$eol"
  open_file "$eol"
  expect_state state loaded
  expect_status eol LF
  expect_state dirty false
  ctl surface editor status --item=line_ending --value=crlf >/dev/null
  expect_status eol CRLF
  expect_state line_ending crlf
  expect_state dirty true
  ctl surface editor save >/dev/null
  expect_state dirty false
  lines="$(wc -l <"$eol" | tr -d ' ')"
  crlf="$(grep -c $'\r$' "$eol" || true)"
  [ "$lines" = 3 ] && [ "$crlf" = 3 ] \
    || fail "$SCENARIO: after CRLF and a save the file has $lines lines and $crlf end in CR, expected 3 and 3"
  echo "OK: CRLF is an unsaved change, and the save wrote it on all $lines lines"
  ctl surface editor status --item=line_ending --value=LF >/dev/null
  expect_status eol LF
  expect_state dirty true
  ctl surface editor save >/dev/null
  expect_state dirty false
  crlf="$(grep -c $'\r$' "$eol" || true)"
  [ "$crlf" = 0 ] || fail "$SCENARIO: back to LF and saved, $crlf lines still end in CR"
  echo "OK: back to LF, saved: no line ends in CR"
  expect_status_refused "not a line ending" --item=line_ending --value=cr
  expect_status eol LF

  # ---- the indent: what a Tab inserts, and what follows the settings
  printf 'abc\n' >"$ind"
  open_file "$ind"
  expect_state state loaded
  ctl surface editor select --ranges=0..0 >/dev/null
  expect_status indent "Spaces: 4"
  ctl surface editor pref --key=tabSize --value=8 >/dev/null
  expect_status indent "Spaces: 8"
  echo "OK: a tab with no indent of its own follows the Editor setting"
  ctl surface editor status --item=indent --value=spaces:2 >/dev/null
  expect_status indent "Spaces: 2"
  ctl surface editor pref --key=tabSize --value=4 >/dev/null
  sleep 1
  expect_status indent "Spaces: 2"
  echo "OK: a tab with an indent of its own keeps it when the setting changes"
  ctl surface editor status --item=indent --value=tabs:4 >/dev/null
  expect_status indent "Tab Size: 4"
  ctl surface editor keys --keys=tab >/dev/null
  expect_state text "$(printf '\tabc\\n')"
  capture status-indent-tab
  ctl surface editor keys --keys=shift-tab >/dev/null
  expect_state text 'abc\n'
  echo "OK: with tabs, Tab inserts a tab character and Shift-Tab takes it off"
  ctl surface editor status --item=indent --value=spaces:2 >/dev/null
  expect_status indent "Spaces: 2"
  ctl surface editor keys --keys=tab >/dev/null
  expect_state text '  abc\n'
  echo "OK: with two spaces, Tab inserts two spaces"
  ctl surface editor save >/dev/null
  expect_state dirty false
  expect_status_refused "not an indent" --item=indent --value=tabs:0
  expect_status_refused "not an indent" --item=indent --value=spaces:99
  expect_status_refused "not an indent" --item=indent --value=eight
  expect_status indent "Spaces: 2"

  # ---- the indent in Tabs mode: a block, a new line after an opener, Shift-Tab
  # on spaces
  printf 'a\nb\n' >"$WT/status-tabs-block.txt"
  open_file "$WT/status-tabs-block.txt"
  expect_state state loaded
  ctl surface editor status --item=indent --value=tabs:4 >/dev/null
  expect_status indent "Tab Size: 4"
  ctl surface editor select --ranges=0..3 >/dev/null
  ctl surface editor keys --keys=tab >/dev/null
  expect_state text "$(printf '\ta\\n\tb\\n')"
  ctl surface editor save >/dev/null
  expect_state dirty false
  [ "$(grep -c "^$(printf '\t')[ab]$" "$WT/status-tabs-block.txt")" = 2 ] \
    || fail "$SCENARIO: the saved file does not hold a tab character before each of its two lines"
  echo "OK: with tabs, Tab over two lines puts one tab character before each, on disk too"
  printf '{' >"$WT/status-tabs-brace.txt"
  open_file "$WT/status-tabs-brace.txt"
  expect_state state loaded
  ctl surface editor status --item=indent --value=tabs:4 >/dev/null
  ctl surface editor select --ranges=1..1 >/dev/null
  ctl surface editor keys --keys=enter >/dev/null
  expect_state text "$(printf '{\\n\t')"
  ctl surface editor save >/dev/null
  expect_state dirty false
  printf '{}' >"$WT/status-tabs-pair.txt"
  open_file "$WT/status-tabs-pair.txt"
  expect_state state loaded
  ctl surface editor status --item=indent --value=tabs:4 >/dev/null
  ctl surface editor select --ranges=1..1 >/dev/null
  ctl surface editor keys --keys=enter >/dev/null
  expect_state text "$(printf '{\\n\t\\n}')"
  ctl surface editor save >/dev/null
  expect_state dirty false
  echo "OK: with tabs, Enter after an opener indents the new line by one tab character, and a closer after the caret drops to its own line"
  printf '  x\n' >"$WT/status-tabs-outdent.txt"
  open_file "$WT/status-tabs-outdent.txt"
  expect_state state loaded
  ctl surface editor status --item=indent --value=tabs:4 >/dev/null
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor keys --keys=shift-tab >/dev/null
  expect_state text 'x\n'
  ctl surface editor save >/dev/null
  expect_state dirty false
  echo "OK: with tabs, Shift-Tab on a line indented with spaces takes the spaces off"
  # What Tab Size does to a tab character: the same file at 2 and at 8. The two
  # captures are for the eye, not compared: in Tabs mode the text system draws a
  # tab at its own advance, so N moves the guides and the row's label only
  # (rust/vendor/ely-gpui-component/LOCAL-CHANGES.md).
  printf '\tabc\n' >"$WT/status-tabsize.txt"
  open_file "$WT/status-tabsize.txt"
  expect_state state loaded
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor status --item=indent --value=tabs:2 >/dev/null
  expect_status indent "Tab Size: 2"
  capture status-tabsize-2
  ctl surface editor status --item=indent --value=tabs:8 >/dev/null
  expect_status indent "Tab Size: 8"
  capture status-tabsize-8

  # ---- the language: colour only
  open_file "$main"
  expect_state state loaded
  ctl surface editor select --ranges=0..0 >/dev/null
  expect_status language Rust
  expect_status lsp fake-lsp:ready
  expect_highlight rust 1
  capture status-language-rust
  mark="$(lsp_mark)"
  ctl surface editor status --item=language --value=plain >/dev/null
  expect_status language "Plain text"
  expect_status lsp fake-lsp:ready
  expect_state language Rust
  # The colour itself: the highlighter was rebuilt for plain text and gave the
  # first line no span (a stale cache would still give it Rust's).
  expect_highlight plain 0
  capture status-language-plain
  must_differ "the language's colour" status-language-rust status-language-plain
  ctl surface editor status --item=language --value=C++ >/dev/null
  expect_status language "C++"
  ctl surface editor status --item=language --value=shell >/dev/null
  expect_status language Shell
  ctl surface editor status --item=language --value=rust >/dev/null
  expect_status language Rust
  expect_highlight rust 1
  expect_status lsp fake-lsp:ready
  [ "$(lsp_count textDocument/didOpen "$mark")" = 0 ] && [ "$(lsp_count textDocument/didClose "$mark")" = 0 ] \
    || fail "$SCENARIO: the language server was told something when only the colour changed"
  echo "OK: five language choices, and the server was sent no didOpen or didClose"
  expect_status_refused "not a language" --item=language --value=klingon

  # ---- go to line
  lines="$(wc -l <"$main" | tr -d ' ')"
  [ "$lines" -ge 60 ] || fail "$SCENARIO: main.rs has $lines lines, expected at least 60"
  ctl surface editor status --item=goto --value=40 >/dev/null
  offset="$(head -n 39 "$main" | wc -c | tr -d ' ')"
  expect_status cursor "Ln 40, Col 1"
  expect_state selections "$offset..$offset"
  echo "OK: line 40: the selection is at its start, byte $offset"
  ctl surface editor status --item=goto --value=999 >/dev/null
  last=$((lines + 1))
  expect_status cursor "Ln $last, Col 1"
  echo "OK: line 999 is the last line ($last, after the file's final newline)"
  expect_status_refused "not a line number" --item=goto --value=abc
  expect_status cursor "Ln $last, Col 1"
  expect_status_refused "count from 1" --item=goto --value=0
  expect_status cursor "Ln $last, Col 1"
  echo "OK: a word and zero are refused and the caret stays"

  # ---- what a refusal looks like
  expect_status_refused "not a status item" --item=bogus --value=1
  expect_status_refused "needs a value" --item=goto
  open_file "$WT/notes.md"
  expect_state state loaded
  ctl surface editor mode --mode=preview >/dev/null
  expect_state mode preview
  expect_status_refused "Markdown Preview" --item=language --value=rust
  expect_status_refused "Markdown Preview" --item=goto --value=2
  expect_status_refused "Markdown Preview" --item=goto --open=true
  ctl surface editor status --item=indent --value=spaces:4 >/dev/null
  echo "OK: Preview refuses the language and go to line and still takes an indent"
  ctl surface editor mode --mode=code >/dev/null
  expect_state mode code
  open_file "$WT/binary.bin"
  expect_state state error
  expect_status_refused "not loaded" --item=goto --value=1
  sha="$(git -C "$WT" rev-parse HEAD)"
  ctl surface editor open-snapshot --sha="$sha" --path=src/shapes.rs >/dev/null
  settle
  expect_state snapshot true
  expect_status_refused "snapshot" --item=indent --value=tabs:4
  expect_status_refused "snapshot" --item=line_ending --value=crlf
  expect_status_refused "snapshot" --item=indent --open=true
  expect_status indent "Spaces: 4"
  expect_status eol LF
  ctl surface editor status --item=language --value=plain >/dev/null
  expect_status language "Plain text"
  expect_state dirty false
  echo "OK: a snapshot refuses an indent and a line ending, and takes a language"

  # ---- the popovers: a real click opens one, one at a time, Escape and Enter
  open_file "$main"
  expect_state state loaded
  ctl surface editor select --ranges=0..0 >/dev/null
  # Room for the whole row: the sidebar and the right panel hidden.
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  expect_state status_set full
  focus_window
  expect_state_stays status_popover none
  capture status-row-items
  click_at "${STATUS_LANGUAGE_ITEM[@]}"
  expect_state_stays status_popover language
  capture status-popover-language
  click_at "${STATUS_LINE_ENDING_ITEM[@]}"
  expect_state_stays status_popover line_ending
  capture status-popover-line-ending
  click_at "${STATUS_INDENT_ITEM[@]}"
  expect_state_stays status_popover indent
  capture status-popover-indent
  click_at "${STATUS_CURSOR_ITEM[@]}"
  expect_state_stays status_popover goto
  capture status-popover-goto
  echo "OK: a click on each item opened its popover, the one before closed"
  # A real key goes where the keyboard is: the field, which took it as the
  # panel opened. Enter with a line number goes there and closes the panel.
  x_key 4
  x_key 0
  x_key Return
  expect_status cursor "Ln 40, Col 1"
  expect_state_stays status_popover none
  x_key Down
  expect_status cursor "Ln 41, Col 1"
  echo "OK: Enter in the field went to line 40 and closed it; the next key moved the code's caret"
  # Not a number: nothing happens, the field and the panel stay.
  click_at "${STATUS_CURSOR_ITEM[@]}"
  expect_state_stays status_popover goto
  x_key a
  x_key Return
  expect_state_stays status_popover goto
  expect_status cursor "Ln 41, Col 1"
  x_key Escape
  expect_state_stays status_popover none
  x_key Down
  expect_status cursor "Ln 42, Col 1"
  echo "OK: a word does nothing; Escape closed the panel and the keyboard is in the code"
  # A choice in a list applies, closes the panel and gives the keyboard back:
  # the CRLF row, then the LF row, in the panel the item opens above itself.
  click_at "${STATUS_LINE_ENDING_ITEM[@]}"
  expect_state_stays status_popover line_ending
  expect_state line_ending lf
  click_at "${STATUS_CRLF_ROW[@]}"
  expect_state line_ending crlf
  expect_state dirty true
  expect_state_stays status_popover none
  x_key Down
  expect_status cursor "Ln 43, Col 1"
  click_at "${STATUS_LINE_ENDING_ITEM[@]}"
  expect_state_stays status_popover line_ending
  click_at "${STATUS_LF_ROW[@]}"
  expect_state line_ending lf
  expect_state dirty false
  expect_state_stays status_popover none
  echo "OK: a click on CRLF and then on LF applied each, closed the panel and left the keyboard in the code"
  # The item pressed a second time closes its panel, and the keyboard is in the
  # code.
  click_at "${STATUS_LANGUAGE_ITEM[@]}"
  expect_state_stays status_popover language
  click_at "${STATUS_LANGUAGE_ITEM[@]}"
  expect_state_stays status_popover none
  x_key Down
  expect_status cursor "Ln 44, Col 1"
  echo "OK: the item pressed again closed its panel and the keyboard is in the code"
  # The verb closes a panel as well.
  ctl surface editor status --item=line_ending --open=true >/dev/null
  expect_state_stays status_popover line_ending
  ctl surface editor status --item=line_ending --open=false >/dev/null
  expect_state_stays status_popover none
  x_key Down
  expect_status cursor "Ln 45, Col 1"
  echo "OK: the verb's open=false hands the keyboard to the code too"
  # Escape in a list reaches the panel: it closes, and the keyboard is in the
  # code, not on whatever held it before the panel opened: here the terminal,
  # which a click on it gives the keyboard.
  click_at "${TERMINAL_AREA[@]}"
  click_at "${STATUS_LANGUAGE_ITEM[@]}"
  expect_state_stays status_popover language
  x_key Escape
  expect_state_stays status_popover none
  x_key Down
  expect_status cursor "Ln 46, Col 1"
  echo "OK: Escape in the language list closed it, and the next key moved the code's caret"
  # Up and Down act in an open list without a Tab: Down and Return take the
  # row after the first.
  click_at "${STATUS_LINE_ENDING_ITEM[@]}"
  expect_state_stays status_popover line_ending
  x_key Down
  x_key Return
  expect_state_stays line_ending crlf
  expect_state_stays status_popover none
  x_key Down
  expect_status cursor "Ln 47, Col 1"
  ctl surface editor status --item=line_ending --value=lf >/dev/null
  expect_state dirty false
  click_at "${STATUS_LANGUAGE_ITEM[@]}"
  expect_state_stays status_popover language
  x_key Down
  x_key Return
  expect_status language Markdown
  expect_state_stays status_popover none
  ctl surface editor status --item=language --value=rust >/dev/null
  expect_status language Rust
  echo "OK: Down and Return in a list chose the row after the first (CRLF; Markdown), and closed it"
  ctl surface editor status --item=indent --open=true >/dev/null
  expect_state_stays status_popover indent
  ctl surface editor status --item=language --open=true >/dev/null
  expect_state_stays status_popover language
  # A panel to the left of the open one: the one open closes first, and the new
  # one stays.
  ctl surface editor status --item=indent --open=true >/dev/null
  expect_state_stays status_popover indent
  ctl surface editor status --item=line_ending --open=true >/dev/null
  expect_state_stays status_popover line_ending
  ctl surface editor status --item=language --open=true >/dev/null
  expect_state_stays status_popover language
  ctl surface editor status --item=indent --open=true >/dev/null
  expect_state_stays status_popover indent
  echo "OK: the verb switched the open panel right to left and left to right, and each stayed open"
  ctl surface editor status --item=indent --open=false >/dev/null
  expect_state_stays status_popover none
  # An item that stops being drawn takes its panel with it, for good: the row
  # goes compact (the panes' toggles, as real keys: the keyboard stays in the
  # panel) and comes back full.
  ctl surface editor status --item=indent --open=true >/dev/null
  expect_state_stays status_popover indent
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  expect_state status_set compact
  expect_status_refused "compact" --item=indent --open=true
  expect_status_refused "compact" --item=line_ending --open=true
  expect_state_stays status_popover none
  expect_state_stays status_popover none
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  expect_state status_set full
  expect_state_stays status_popover none
  echo "OK: an open indent panel went when the compact row stopped drawing its item, and did not come back with it"
  # Likewise for a language panel when the file switches to Markdown Preview.
  open_file "$WT/notes.md"
  expect_state state loaded
  ctl surface editor status --item=language --open=true >/dev/null
  expect_state_stays status_popover language
  ctl surface editor mode --mode=preview >/dev/null
  expect_state mode preview
  expect_state_stays status_popover none
  ctl surface editor mode --mode=code >/dev/null
  expect_state mode code
  expect_state_stays status_popover none
  echo "OK: an open language panel went when Preview took the code away, and did not come back"
  ctl surface editor status --item=language --open=true >/dev/null
  expect_state_stays status_popover language
  ctl surface editor status --item=language --open=false >/dev/null
  expect_state_stays status_popover none
  # Back to main.rs, whose server item makes the row wide enough to go compact
  # (notes.md has none), and to the compact set.
  open_file "$main"
  expect_state state loaded
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  expect_state status_set compact

  count="$(tab_count)"
  while [ "$count" -gt "$tabs_before" ]; do
    ctl surface tabs close "$count" >/dev/null
    count=$((count - 1))
  done
  [ "$(layout_field secondaryVisible)" = "$secondary_before" ] \
    || fail "$SCENARIO: secondaryVisible is $(layout_field secondaryVisible), expected $secondary_before"
}

# The bell's place in the window of CLICK_WINDOW with the sidebar and the right
# panel hidden (measured on the captures this scenario takes), and Clear in its
# panel when two messages are listed.
STATUS_BELL_ITEM=(1454 775)
STATUS_BELL_CLEAR=(1431 744)
# The bell as the compact row draws it, with the sidebar and the right panel
# shown, and the Copy of the older of two rows in the full row's panel.
STATUS_BELL_COMPACT=(1128 775)
STATUS_BELL_COPY_OLDER=(1445 690)
# The corner card, over the file's bottom right corner: a point of a card of
# three lines, and one inside the card whatever its size. The card sits above the
# status row and the drawer's header row (30 px, the drawer closed).
NOTIFY_CARD=(1300 690)
NOTIFY_CARD_CORNER=(1438 705)

# The bell's count and the length of the list, both polled.
expect_bell() { # unseen listed
  expect_status bell "$1"
  expect_state notifications "$2"
}

# What the clipboard holds, read the way copy_propagates reads it: a paste into
# the empty scratch file, then undone. Leaves the tab on the scratch file.
expect_clipboard() { # text
  open_file "$WT/empty.txt"
  expect_state state loaded
  ctl surface editor keys --keys=ctrl-v >/dev/null
  expect_state text "$1"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text ""
  expect_state dirty false
}

scenario_notifications() {
  local notify="$WT/notify.nfy" tabs_before secondary_before sha count i before after note
  tabs_before="$(tab_count)"
  secondary_before="$(layout_field secondaryVisible)"
  mkdir -p "$XDG_CONFIG_HOME/sirio"
  cat >>"$XDG_CONFIG_HOME/sirio/languages.toml" <<'TOML'

[[language]]
name = "notify-failing"
extensions = ["nfy"]
command = "sh"
args = ["-c", "exit 1"]
TOML
  # Forty lines, so the corner card sits over text and a press that fell
  # through it would land on a line and move the caret.
  { printf 'one\ntwo\nthree\n'; seq -f 'line %g' 4 40; } >"$notify"

  # ---- the first message: a language server that cannot start, on opening
  open_file "$notify"
  expect_state state loaded
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  # With the sidebar and the right panel hidden the row is the full set; the
  # failed server is drawn as "did not start", whole (the reason is in the card
  # and the bell), and the state still says it failed.
  expect_state status_set full
  expect_status lsp sh:failed
  expect_bell 1 1
  expect_state notification_repeats 1
  [ -n "$(sfield message)" ] || fail "$SCENARIO: the corner card names nothing after the server failed"
  expect_state_stays notifications 1
  echo "OK: a server that failed to start raised a message: bell 1, one notification"
  expect_state status_popover none
  focus_window
  capture bell-count-one

  # ---- the second: a save refused over a disk change, by a real Ctrl+S: one
  # message, worded "Save failed: ..." with no log tag
  ctl surface editor status --item=line_ending --value=crlf >/dev/null
  expect_state dirty true
  printf 'changed\non disk\n' >"$notify"
  expect_state conflict changed
  ctl surface editor keys --keys=right >/dev/null
  x_key ctrl+s
  expect_bell 2 2
  expect_state dirty true
  case "$(sfield message)" in
    "Save failed: "*) ;;
    *) fail "$SCENARIO: the corner card reads '$(sfield message)', expected 'Save failed: ...'" ;;
  esac
  echo "OK: a refused save raised the second: bell 2, two notifications"
  capture bell-count

  # The corner card goes with a click on it; the list and the count do not, and
  # the press does not reach the code under the card (the caret stays).
  before="$(status_field cursor)"
  click_at "${NOTIFY_CARD[@]}"
  expect_state message ""
  expect_state_stays notifications 2
  expect_status bell 2
  after="$(status_field cursor)"
  [ "$before" = "$after" ] \
    || fail "$SCENARIO: a click on the corner card moved the caret from '$before' to '$after'"
  echo "OK: dismissing the corner card left the list and the count alone, and the caret at $after"

  # ---- opening the list: it clears the count and keeps the list
  ctl surface editor status --item=bell --open=true >/dev/null
  expect_state_stays status_popover bell
  expect_bell 0 2
  capture bell-list
  ctl surface editor status --item=bell --open=false >/dev/null
  expect_state_stays status_popover none
  expect_bell 0 2

  # ---- the same message again is the same row: counted into it ("(x3)"), and
  # still one on the bell each time it arrives while the list is closed
  x_key ctrl+s
  expect_bell 1 2
  expect_state notification_repeats 2
  x_key ctrl+s
  expect_bell 2 2
  expect_state_stays notification_repeats 3
  echo "OK: the same message twice more is one row marked x3, and 2 on the bell"
  ctl surface editor status --item=bell --open=true >/dev/null
  expect_state_stays status_popover bell
  expect_bell 0 2
  capture bell-repeat
  # A message that arrives while the list is open is seen already: no count on
  # the bell, and its own row.
  ctl surface editor status --item=bell --value="raise:arrived while the list was open" >/dev/null
  expect_state notifications 3
  expect_state_stays notification_newest "arrived while the list was open"
  expect_status bell 0
  ctl surface editor status --item=bell --open=false >/dev/null
  expect_state_stays status_popover none
  expect_bell 0 3
  echo "OK: a message that arrived while the list was open was seen: bell 0, a third row"
  # A different message is a new row again.
  x_key ctrl+s
  expect_bell 1 4
  expect_state_stays notifications 4
  echo "OK: opening the list took the count to 0 and kept both; a message while closed: bell 1, four rows"
  ctl surface editor status --item=bell --value=clear >/dev/null
  expect_bell 0 0
  expect_state_stays notifications 0
  expect_status_refused "not a bell value" --item=bell --value=empty
  expect_status_refused "needs a value" --item=bell
  echo "OK: value=clear emptied the list and the count"
  ctl surface editor status --item=bell --open=true >/dev/null
  expect_state_stays status_popover bell
  capture bell-empty
  ctl surface editor status --item=bell --open=false >/dev/null
  expect_state_stays status_popover none
  expect_bell 0 0

  # ---- a real click opens the panel, a second one closes it, Clear empties
  # it and closes it with the keyboard in the code
  x_key ctrl+s
  x_key ctrl+s
  expect_bell 2 1
  click_at "${STATUS_BELL_ITEM[@]}"
  expect_state_stays status_popover bell
  expect_bell 0 1
  click_at "${STATUS_BELL_ITEM[@]}"
  expect_state_stays status_popover none
  ctl surface editor select --ranges=0..0 >/dev/null
  x_key Down
  expect_status cursor "Ln 2, Col 1"
  echo "OK: a click on the bell opened its panel and a second one closed it, the keyboard in the code"
  click_at "${STATUS_BELL_ITEM[@]}"
  expect_state_stays status_popover bell
  click_at "${STATUS_BELL_CLEAR[@]}"
  expect_state_stays status_popover none
  expect_bell 0 0
  x_key Down
  expect_status cursor "Ln 3, Col 1"
  echo "OK: Clear in the panel emptied the list, closed it, and the keyboard is in the code"
  # Escape closes the panel, from where the panel left the keyboard.
  click_at "${STATUS_BELL_ITEM[@]}"
  expect_state_stays status_popover bell
  x_key Escape
  expect_state_stays status_popover none
  x_key Down
  expect_status cursor "Ln 4, Col 1"
  echo "OK: Escape closed the bell's panel and the keyboard is in the code"

  # ---- a stray Enter in a freshly opened panel wipes nothing (the panel's first
  # tab stop is the newest row's Copy, Clear is the second), and a row's Copy
  # puts that row on the clipboard
  x_key ctrl+s
  ctl surface editor status --item=bell --value="raise:a later note" >/dev/null
  expect_bell 2 2
  click_at "${NOTIFY_CARD_CORNER[@]}"
  expect_state message ""
  click_at "${STATUS_BELL_ITEM[@]}"
  expect_state_stays status_popover bell
  expect_bell 0 2
  x_key Return
  expect_state_stays notifications 2
  expect_bell 0 2
  echo "OK: an Enter in the freshly opened panel left both rows"
  ctl surface editor status --item=bell --open=false >/dev/null
  expect_state_stays status_popover none
  click_at "${STATUS_BELL_ITEM[@]}"
  expect_state_stays status_popover bell
  capture bell-list-copy
  click_at "${STATUS_BELL_COPY_OLDER[@]}"
  expect_state_stays notifications 2
  ctl surface editor status --item=bell --open=false >/dev/null
  expect_state_stays status_popover none
  expect_clipboard "Save failed: the file changed on disk; choose Reload or Keep before saving"
  echo "OK: the older row's Copy put its text on the clipboard"
  open_file "$notify"
  expect_state state loaded
  ctl surface editor status --item=bell --value=copy:0 >/dev/null
  expect_clipboard "a later note"
  open_file "$notify"
  expect_state state loaded
  expect_status_refused "no notification 9" --item=bell --value=copy:9
  expect_status_refused "not a notification number" --item=bell --value=copy:x
  expect_bell 0 2
  echo "OK: copy:0 put the newest row on the clipboard; a row that is not there is refused"
  ctl surface editor status --item=bell --value=clear >/dev/null
  expect_bell 0 0

  # ---- the compact row draws the bell too: the real clicks under it prove it
  # (the sidebar and the right panel shown narrow the file's pane)
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  expect_state status_set compact
  x_key ctrl+s
  expect_bell 1 1
  focus_window
  capture bell-compact
  click_at "${STATUS_BELL_COMPACT[@]}"
  expect_state_stays status_popover bell
  expect_bell 0 1
  capture bell-list-compact
  click_at "${STATUS_BELL_COMPACT[@]}"
  expect_state_stays status_popover none
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  expect_state status_set full
  echo "OK: the compact row drew the bell, and a real click opened and closed its panel"

  # ---- a snapshot has its own list, and the bell takes a press there
  sha="$(git -C "$WT" rev-parse HEAD)"
  ctl surface editor open-snapshot --sha="$sha" --path=src/shapes.rs >/dev/null
  settle
  expect_state snapshot true
  expect_state status_set full
  expect_bell 0 0
  "$CTL" surface editor save >/dev/null 2>&1 && fail "$SCENARIO: a snapshot's save was accepted"
  expect_bell 1 1
  ctl surface editor status --item=bell --open=true >/dev/null
  expect_state_stays status_popover bell
  expect_bell 0 1
  ctl surface editor status --item=bell --open=false >/dev/null
  expect_state_stays status_popover none
  "$CTL" surface editor save >/dev/null 2>&1 || true
  expect_bell 1 1
  expect_state notification_repeats 2
  click_at "${STATUS_BELL_ITEM[@]}"
  expect_state_stays status_popover bell
  expect_bell 0 1
  click_at "${STATUS_BELL_ITEM[@]}"
  expect_state_stays status_popover none
  # A real Ctrl+S on the snapshot is one message, not two: the view raises its
  # own refusal and the handler only logs it.
  x_key ctrl+s
  expect_bell 1 1
  expect_state_stays notification_repeats 3
  echo "OK: a snapshot's Save raised its message into its own list once, by the verb and by a real Ctrl+S, and its bell opened by the verb and by a click"
  # The newest 50 rows are kept: 54 distinct messages after the one row of
  # refusals make 55 rows, and the oldest five go.
  i=1
  while [ "$i" -le 54 ]; do
    note="$(printf 'note %02d' "$i")"
    ctl surface editor status --item=bell --value="raise:$note" >/dev/null
    i=$((i + 1))
  done
  expect_bell 50 50
  expect_state_stays notifications 50
  expect_state notification_newest "note 54"
  expect_state notification_oldest "note 05"
  ctl surface editor status --item=bell --value=clear >/dev/null
  expect_bell 0 0
  echo "OK: 55 rows left the newest 50 (note 05 to note 54), and 50 on the bell"

  # ---- Markdown Preview: the bell is about the tab, not the code
  open_file "$WT/notes.md"
  expect_state state loaded
  ctl surface editor mode --mode=preview >/dev/null
  expect_state mode preview
  ctl surface editor status --item=bell --open=true >/dev/null
  expect_state_stays status_popover bell
  ctl surface editor status --item=bell --open=false >/dev/null
  expect_state_stays status_popover none
  ctl surface editor mode --mode=code >/dev/null
  expect_state mode code
  echo "OK: Preview's bell opened its panel"
  # The first file's tab kept its own list through all of it: the one message
  # the compact row's Ctrl+S raised, seen.
  open_file "$notify"
  expect_state state loaded
  expect_bell 0 1
  ctl surface editor keys --keys=ctrl-shift-s >/dev/null
  ctl surface editor keys --keys=ctrl-shift-i >/dev/null
  # Reload takes the disk's text, so the tab closes without asking.
  ctl surface editor conflict --action=reload >/dev/null
  expect_state dirty false
  expect_state conflict none

  count="$(tab_count)"
  while [ "$count" -gt "$tabs_before" ]; do
    ctl surface tabs close "$count" >/dev/null
    count=$((count - 1))
  done
  [ "$(layout_field secondaryVisible)" = "$secondary_before" ] \
    || fail "$SCENARIO: secondaryVisible is $(layout_field secondaryVisible), expected $secondary_before"
}

scenario_find() {
  local fixture="$WT/findme.txt" all cased
  open_file "$fixture"
  expect_state state loaded
  expect_state find_matches none
  expect_state find_current none
  all="$(grep_count i fn)"
  cased="$(grep_count "" fn)"
  [ "$all" != "$cased" ] || fail "$SCENARIO: the fixture has no Fn or FN ($all matches either way)"

  # The caret at 0, before the first match, so the current match is the first.
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor find --query=fn >/dev/null
  expect_state find_matches "$all"
  expect_state find_current 0
  expect_state find_error ""
  ctl surface editor find-step >/dev/null
  ctl surface editor find-step >/dev/null
  expect_state find_current 2
  ctl surface editor find-step --forward=false >/dev/null
  expect_state find_current 1
  echo "OK: $all matches ignoring case; two steps forward and one back stand on the second"

  ctl surface editor find --query=fn --case=true >/dev/null
  expect_state find_matches "$cased"
  echo "OK: $cased matches with the case: it differs from the $all ignoring it"

  # A pattern that does not compile finds nothing and says why.
  ctl surface editor find --query='(' --regex=true >/dev/null
  expect_state find_matches 0
  expect_state_set find_error
  ctl surface editor find --query=fn --case=false >/dev/null
  expect_state find_matches "$all"
  expect_state find_error ""

  # The capture: the replace row open, the minimap on, the third match ("FN" on
  # line 4) current, the find bar docked above the code.
  ctl surface editor pref --key=minimap --value=true >/dev/null
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor find --query=fn --replace=Y >/dev/null
  ctl surface editor find-step >/dev/null
  ctl surface editor find-step >/dev/null
  expect_state find_current 2
  capture find-open
  ctl surface editor pref --key=minimap --value=false >/dev/null

  ctl surface editor find-close >/dev/null
  expect_state find_matches none
  expect_state find_current none
  echo "OK: closing find leaves no count"
}

scenario_replace() {
  local fixture="$WT/findme.txt" all
  open_file "$fixture"
  expect_state state loaded
  all="$(grep_count i fn)"
  cp "$fixture" "$RUN_DIR/replace-0.txt"
  python3 "$TOOLS" sub "$fixture" "$RUN_DIR/replace-1.txt" fn Y 1 1
  python3 "$TOOLS" sub "$RUN_DIR/replace-1.txt" "$RUN_DIR/replace-2.txt" fn Y 0 1

  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor find --query=fn --replace=Y >/dev/null
  expect_state find_matches "$all"
  ctl surface editor replace --all=false >/dev/null
  expect_state text "$(escaped "$RUN_DIR/replace-1.txt")"
  expect_state find_matches "$((all - 1))"
  expect_state dirty true
  echo "OK: replacing one changed exactly one occurrence"

  ctl surface editor replace --all=true >/dev/null
  expect_state text "$(escaped "$RUN_DIR/replace-2.txt")"
  expect_state find_matches 0
  echo "OK: replacing all left none"

  # One undo, one step: the text from before the replace-all comes back whole.
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/replace-1.txt")"
  echo "OK: one undo restored every replaced occurrence"

  # Back to the file on disk, so later scenarios see it clean.
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/replace-0.txt")"
  expect_state dirty false
  ctl surface editor find-close >/dev/null
  expect_state find_matches none
}

scenario_multibyte() {
  local fixture="$WT/findme.txt" smiles
  open_file "$fixture"
  expect_state state loaded
  smiles="$(grep_count "" "😀")"
  [ "$smiles" = 2 ] || fail "$SCENARIO: the fixture has $smiles emoji, not 2"
  cp "$fixture" "$RUN_DIR/multi-0.txt"
  python3 "$TOOLS" sub "$fixture" "$RUN_DIR/multi-1.txt" "😀" x 1 0
  python3 "$TOOLS" sub "$fixture" "$RUN_DIR/multi-2.txt" "😀" x 0 0

  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor find --query="😀" --replace=x >/dev/null
  expect_state find_matches 2
  ctl surface editor replace --all=false >/dev/null
  expect_state text "$(escaped "$RUN_DIR/multi-1.txt")"
  expect_state find_matches 1
  ctl surface editor replace --all=false >/dev/null
  expect_state text "$(escaped "$RUN_DIR/multi-2.txt")"
  expect_state find_matches 0
  echo "OK: both emoji were replaced one at a time, byte for byte"

  # Undo both, then once more from the start: all at once.
  ctl surface editor keys --keys=ctrl-z >/dev/null
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/multi-0.txt")"
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor find --query="😀" --replace=x >/dev/null
  expect_state find_matches 2
  ctl surface editor replace --all=true >/dev/null
  expect_state text "$(escaped "$RUN_DIR/multi-2.txt")"
  expect_state find_matches 0
  echo "OK: both emoji were replaced at once, byte for byte"

  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/multi-0.txt")"
  expect_state dirty false
  ctl surface editor find-close >/dev/null
}

# A real key at the app's window: the key the X server gives the window that
# has X input focus, never a call into the app.
x_key() { # key
  DISPLAY="$DISPLAY_TARGET" xdotool key "$1" \
    || fail "$SCENARIO: xdotool could not send $1"
}

scenario_escape() {
  local fixture="$WT/findme.txt" xs
  open_file "$fixture"
  expect_state state loaded
  cp "$fixture" "$RUN_DIR/escape-0.txt"
  python3 "$TOOLS" sub "$fixture" "$RUN_DIR/escape-q.txt" "fn first" "qfn first" 1 0
  python3 "$TOOLS" sub "$fixture" "$RUN_DIR/escape-x.txt" "let x" "let q" 1 0

  # (i) Escape with the keyboard in the code closes find. The keys verb gives
  # the code the keyboard first, as a click on it would.
  ctl surface editor find --query=fn >/dev/null
  expect_state find_matches "$(grep_count i fn)"
  ctl surface editor keys --keys=escape >/dev/null
  expect_state find_matches none
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor type --text=q >/dev/null
  expect_state text "$(escaped "$RUN_DIR/escape-q.txt")"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/escape-0.txt")"
  echo "OK: Escape in the code closes find, and a typed q lands at the caret"

  # (ii) Escape with the keyboard in the query field, by real X keys. Opening
  # find gives the query the keyboard; a typed x replaces its selected text,
  # which shows where the key went: the count becomes the file's x's, and the
  # code selects the first of them (`let x`), which the q typed after Escape
  # then replaces.
  focus_window
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor find --query=fn >/dev/null
  xs="$(grep_count i x)"
  [ "$xs" != "$(grep_count i fn)" ] || fail "$SCENARIO: the fixture has as many x as fn"
  x_key x
  expect_state find_matches "$xs"
  expect_state text "$(escaped "$RUN_DIR/escape-0.txt")"
  x_key Escape
  expect_state find_matches none
  x_key q
  expect_state text "$(escaped "$RUN_DIR/escape-x.txt")"
  echo "OK: Escape in the query closes find, and the next key lands in the code"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/escape-0.txt")"
  expect_state dirty false
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

# ---- the carries of Task 12a that need a real pointer

# The terminal pane's body and a point inside the code of a file open beside it,
# in the window's own coordinates (a window of CLICK_WINDOW, nothing else open).
TERMINAL_PANE=(500 300)
CODE_POINT=(900 400)

# How many times the app asked for this file's diff, once it has stopped moving:
# a load, a focus or an outside change each ask, and a fetch that lands may ask
# once more, so two equal reads half a second apart.
settled_git_diff_requests() {
  local last="" now tries=0
  while [ "$tries" -lt 60 ]; do
    read_state
    now="$(sfield git_diff_requests)"
    [ "$now" != "$last" ] || { printf '%s' "$now"; return 0; }
    last="$now"
    tries=$((tries + 1))
    sleep 0.5
  done
  fail "$SCENARIO: git_diff_requests never settled (last $last)"
}

scenario_focus_refresh() {
  open_file "$WT/main.rs"
  expect_state state loaded
  focus_window
  # The caret placed by the verb, then the keyboard taken away by a press in the
  # terminal pane: the next press in the code is a gain of focus, and the code's.
  ctl surface editor select --ranges=0..0 >/dev/null
  click_at "${TERMINAL_PANE[0]}" "${TERMINAL_PANE[1]}"
  local before
  before="$(settled_git_diff_requests)"
  echo "OK: git_diff_requests=$before before the press in the code"
  click_at "${CODE_POINT[0]}" "${CODE_POINT[1]}"
  # One real press into the code is one ask for the diff, the whole-worktree
  # `git status` behind it, and not one per handler that sees the focus.
  expect_state_stays git_diff_requests $((before + 1))
  [ "$(sfield selections)" != "0..0" ] \
    || fail "$SCENARIO: the press did not reach the code (the caret is still at 0..0)"
  echo "OK: one press into the code asked for the diff once"
}

# Moves the pointer down the code at column X, from Y_FIRST in steps of 7 px, until
# the card of the Error on line_7 is drawn; answers the y it was first drawn at.
# A step is one real motion event, so the card is the one the pointer made.
sweep_for_the_error_card() { # x y_first
  local y="$2"
  while [ "$y" -lt 720 ]; do
    move_to "$1" "$y"
    sleep 0.25
    read_state
    case "$(sfield hover)" in *"$FAKE_ERROR"*) printf '%s' "$y"; return 0 ;; esac
    y=$((y + 7))
  done
  fail "$SCENARIO: the pointer never found the Error's card in column $1"
}
# The Error's name on line_7 sits at about this column (after `fn `, in a window of
# CLICK_WINDOW at the default size and font).
ERROR_COLUMN=830

scenario_hover_leave() {
  local fixture="$WT/main.rs" error_row bar_row rows_above
  open_file "$fixture"
  expect_state state loaded
  expect_state diagnostics 2
  focus_window
  # Earlier scenarios leave their edits in main.rs, so the Error's line is read from
  # the text: the code's first row is that many rows (21 px each) above it.
  rows_above="$(python3 "$TOOLS" line-of "$(sfield text)" "fn line_7")"
  cp "$fixture" "$RUN_DIR/hover-leave-0.txt"

  # (i) The find bar. Open above the code, it pushes the code down; the sweep
  # finds where the Error is now, and the pointer then jumps straight onto the bar:
  # the rows above the Error, and 17 px more, which is inside the bar whatever its height.
  ctl surface editor find --query=fn >/dev/null
  expect_state find_matches "$(grep -o -i fn "$fixture" | wc -l | tr -d ' ')"
  error_row="$(sweep_for_the_error_card $ERROR_COLUMN 120)"
  echo "OK: the card shows the Error with the pointer resting at y=$error_row"
  bar_row=$((error_row - rows_above * 21 - 17))
  move_to $ERROR_COLUMN "$bar_row"
  expect_state hover ""
  expect_state_stays hover ""
  echo "OK: a pointer that left the code for the find bar left no card behind"
  ctl surface editor find-close >/dev/null
  expect_state find_matches none

  # (ii) The conflict banner: a dirty buffer, and the file changed on disk.
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor type --text=y >/dev/null
  expect_state dirty true
  python3 - "$RUN_DIR/hover-leave-0.txt" "$RUN_DIR/hover-leave-1.txt" <<'PY'
import sys
with open(sys.argv[1], "rb") as source, open(sys.argv[2], "wb") as target:
    target.write(b"// external\n" + source.read())
PY
  write_disk "$fixture" "$RUN_DIR/hover-leave-1.txt"
  expect_state conflict changed
  error_row="$(sweep_for_the_error_card $ERROR_COLUMN 120)"
  echo "OK: the card shows the Error again, under the conflict banner, at y=$error_row"
  move_to $ERROR_COLUMN $((error_row - rows_above * 21 - 17))
  expect_state hover ""
  expect_state_stays hover ""
  echo "OK: a pointer that left the code for the conflict banner left no card behind"

  # The file as it was, for the scenarios after this one.
  ctl surface editor conflict --action=reload >/dev/null
  expect_state conflict none
  write_disk "$fixture" "$RUN_DIR/hover-leave-0.txt"
  expect_state text "$(escaped "$RUN_DIR/hover-leave-0.txt")"
  expect_state dirty false
}

# Quits the app the way a user does and starts it again on the same database,
# host root and socket path, then selects the repository as the run began.
app_restart() {
  local tries=0
  ctl quit >/dev/null 2>&1 || true
  while kill -0 "$APP_PID" 2>/dev/null && [ "$tries" -lt 150 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  if kill -0 "$APP_PID" 2>/dev/null; then
    kill -KILL "$APP_PID" 2>/dev/null || true
    fail "$SCENARIO: sirio did not quit within 15 s"
  fi
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
  app_start
  local selected=0
  for _ in $(seq 1 40); do
    "$CTL" select-workspace --workspace "$WT" >/dev/null 2>&1 && { selected=1; break; }
    sleep 0.5
  done
  [ "$selected" -eq 1 ] || fail "$SCENARIO: the repository could not be selected after the relaunch"
}

# A press at (X, Y), counted from the window's corner, dragged UP by DY pixels
# in two steps, and released. The same window-size guard as the clicks.
drag_up() { # x y dy
  local window size
  window="$(find_window)"
  [ -n "$window" ] || fail "$SCENARIO: no window to drag in"
  size="$(xwininfo -display "$DISPLAY_TARGET" -id "$window" | awk '/Width:/ {w = $2} /Height:/ {h = $2} END {print w "x" h}')"
  [ "$size" = "$CLICK_WINDOW" ] || fail "$SCENARIO: the window is $size, not the $CLICK_WINDOW this drag is placed for"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$1" "$2" mousedown 1 \
    mousemove_relative --sync -- 0 -$(($3 / 2)) mousemove_relative --sync -- 0 -$(($3 - $3 / 2)) mouseup 1 \
    || fail "$SCENARIO: xdotool could not drag"
}

# A point of the corner card over the file pane (the right panel shown), with the
# drawer closed and the card one line.
DRAWER_CARD=(1100 700)

# The drawer under the code: opened by Ctrl+J, a tab and a height chosen
# through the debug verb, kept by the worktree (a second file draws the same
# drawer) and across a quit and relaunch, then closed by the key again.
scenario_drawer() {
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state drawer_open 0
  expect_state drawer_tab problems

  # A real key: the code has the keyboard (the keys verb gives it, and a
  # caret that is already at the start moves nowhere), the window has X focus.
  focus_window
  ctl surface editor keys --keys=left >/dev/null
  x_key ctrl+j
  expect_state drawer_open 1
  expect_state drawer_tab problems
  echo "OK: Ctrl+J opens the drawer on Problems"

  ctl surface editor drawer --show=search >/dev/null
  expect_state drawer_tab search
  ctl surface editor drawer --height=300 >/dev/null
  expect_state drawer_height 300
  ctl surface editor drawer --height=10 >/dev/null
  expect_state drawer_height 96
  ctl surface editor drawer --height=300 >/dev/null
  expect_state drawer_height 300
  echo "OK: Search is shown, 300 px is kept and 10 px is raised to the 96 px floor"

  open_file "$WT/crlf.txt"
  expect_state state loaded
  expect_state drawer_open 1
  expect_state drawer_tab search
  expect_state drawer_height 300
  echo "OK: a second file of the worktree draws the same drawer"
  ctl surface editor drawer --show=problems >/dev/null
  expect_state drawer_tab problems
  capture drawer-open

  app_restart
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state drawer_open 1
  expect_state drawer_tab problems
  expect_state drawer_height 300
  echo "OK: the drawer is as it was after a quit and relaunch"

  # The top edge dragged up 50 px by a real pointer: the height follows it and
  # the drop saves it (the relaunch reads it back). At 300 px in this window the
  # drawer's edge is at y=462, and a press 3 px below it is on its 6 px strip.
  drag_up 900 465 50
  expect_state drawer_height 350
  app_restart
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state drawer_height 350
  echo "OK: a drag of the top edge sets the height, and the drop keeps it"

  focus_window
  ctl surface editor keys --keys=left >/dev/null
  x_key ctrl+j
  expect_state drawer_open 0
  expect_state drawer_height 350
  x_key ctrl+j
  expect_state drawer_open 1
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  echo "OK: Ctrl+J and the close verb shut the drawer, the height stays"

  # The corner card sits above the drawer and follows it. Raised with the drawer
  # closed, it is over DRAWER_CARD (a point of the file pane beside the right panel,
  # in a window of CLICK_WINDOW); with the drawer open that point is inside the
  # drawer, so a press there reaches the drawer (a stale card would take it and
  # dismiss itself); closed again, the card is back at that point.
  ctl surface editor status --item=bell --value="raise:a note over the drawer" >/dev/null
  expect_state message "a note over the drawer"
  capture drawer-card-closed
  ctl surface editor drawer --show=problems >/dev/null
  expect_state drawer_open 1
  sleep 1
  capture drawer-card-open
  click_at "${DRAWER_CARD[@]}"
  expect_state_stays message "a note over the drawer"
  # A right press in the drawer is not the code's: no editor menu opens. The
  # state does not name the menu, so this is a capture to look at.
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$(find_window)" 1000 600 click 3
  sleep 1
  capture drawer-right-click
  x_key Escape
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  sleep 1
  click_at "${DRAWER_CARD[@]}"
  expect_state message ""
  echo "OK: the corner card rose with the drawer and came back down with it"
}

# ---------------------------------------------------------------- problems

# Problems over the worktree (editor panels, Task 5). The fake language server
# reports files nobody opened (its `publish` answers, Scripts/Tests/fake_lsp.py)
# and the drawer lists them, opens itself on an error the user has not seen, and
# notices a server that ends. Each scenario's "new" errors carry messages no other
# scenario uses, so a seen set left behind by an earlier one cannot mask a step.
#
# The rows the earlier scenarios rely on are not disturbed: the setup records the
# one error their fake server always reports (main.rs, FAKE_ERROR) as already
# seen, the way a user who closed the drawer on it would have.

# The fixtures the scenarios report on: a file with an accent and an emoji before
# its needle (the column is UTF-16, the selection bytes), and a second file.
problems_fixtures() {
  mkdir -p "$WT/src"
  python3 - "$WT" <<'PY'
import os, sys
root = sys.argv[1]
with open(os.path.join(root, "src/lib.rs"), "wb") as handle:
    handle.write("fn alpha() -> u32 { 1 }\nfn beta() -> u32 { 2 }\n// é\U0001F600 target_marker\n".encode("utf-8"))
with open(os.path.join(root, "src/util.rs"), "wb") as handle:
    handle.write(b"fn util() {}\nfn helper() {}\n")
PY
  [ -f "$RUN_DIR/fake-lsp/answers.base.json" ] || cp "$RUN_DIR/fake-lsp/answers.json" "$RUN_DIR/fake-lsp/answers.base.json"
}
# One finding of the fake server's `publish` map: the text it names, an LSP
# severity (1 error, 2 warning), and its message.
pf() { # needle severity message
  printf '{"needle":"%s","severity":%s,"message":"%s","source":"fake-lsp"}' "$1" "$2" "$3"
}
# Rewrites the fake server's answers so it reports exactly PUBLISH, a JSON object
# of path (relative to the worktree) to findings, and nothing in main.rs; whatever
# it reported before and no longer lists is published empty. Every call changes
# the file (a nonce), so the server's poll sees a change even for the same map.
problems_publish() { # json
  python3 - "$RUN_DIR/fake-lsp/answers.json" "$RUN_DIR/fake-lsp/answers.base.json" "$1" <<'PY'
import json, os, sys, time
path, base, publish = sys.argv[1:4]
with open(base, encoding="utf-8") as handle:
    answers = json.load(handle)
answers["diagnostics_file"] = ""
answers["publish"] = json.loads(publish)
answers["nonce"] = time.time_ns()
with open(path + ".tmp", "w", encoding="utf-8") as handle:
    json.dump(answers, handle)
os.replace(path + ".tmp", path)
PY
}
# The answers as the earlier scenarios know them (no `publish`): the server puts
# main.rs's own findings back and rests.
problems_restore() {
  cp "$RUN_DIR/fake-lsp/answers.base.json" "$RUN_DIR/fake-lsp/answers.json.tmp"
  mv "$RUN_DIR/fake-lsp/answers.json.tmp" "$RUN_DIR/fake-lsp/answers.json"
}
# A drawer closed over a store with nothing in it, main.rs open: the state every
# problems scenario starts from. The seen set is whatever the empty store leaves.
problems_reset() {
  problems_fixtures
  open_file "$WT/main.rs"
  expect_state state loaded
  ctl surface editor drawer --show=problems >/dev/null
  problems_publish '{}'
  expect_state problems 0/0/0
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  # A server's silence is not a look: nothing below may be affected by a poll
  # that is still on its way.
  sleep 0.6
  expect_state drawer_open 0
}
problems_done() {
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  # The answers back as the earlier scenarios know them: main.rs's Error and
  # Warning return, which is an error the drawer has not seen (a reset's look
  # saw none). A look at Problems, then a close, is the user having seen them.
  problems_restore
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state diagnostics 2
  expect_state problems 1/1/0
  ctl surface editor drawer --show=problems >/dev/null
  ctl surface editor drawer --close >/dev/null
  expect_state_stays drawer_open 0
}

scenario_problems_unopened() {
  problems_reset
  # The caret at a known place: the drawer opens over it and must not move it.
  ctl surface editor select --ranges=5..5 >/dev/null
  expect_state selections "5..5"
  problems_publish "{\"src/lib.rs\":[$(pf alpha 1 'unopened: alpha cannot be found')]}"
  expect_state drawer_open 1
  expect_state drawer_tab problems
  expect_state problems 1/0/0
  expect_state problem_files src/lib.rs
  expect_state selections "5..5"
  echo "OK: an error in a file nobody opened opened the drawer on Problems, listed it, and left the caret"
  capture problems-open
  problems_done
}

scenario_problems_alarm() {
  problems_reset
  local one two three four
  one="$(pf alpha 1 'alarm one: alpha is wrong')"
  two="$(pf beta 1 'alarm two: beta is wrong')"
  three="$(pf alpha 1 'alarm three: alpha again')"
  four="$(pf beta 1 'alarm four: beta again')"
  # The first error is new: it opens the drawer. Closing is the user seeing it.
  problems_publish "{\"src/lib.rs\":[$one]}"
  expect_state drawer_open 1
  expect_state problems 1/0/0
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  # The same error written again, and cleared then republished with no look
  # between: neither is new.
  problems_publish "{\"src/lib.rs\":[$one]}"
  sleep 1
  expect_state_stays drawer_open 0
  problems_publish '{}'
  expect_state problems 0/0/0
  problems_publish "{\"src/lib.rs\":[$one]}"
  expect_state problems 1/0/0
  expect_state_stays drawer_open 0
  echo "OK: the same error, rewritten and then cleared and republished, left the drawer closed"
  # A message the user has not seen opens it.
  problems_publish "{\"src/lib.rs\":[$one,$two]}"
  expect_state drawer_open 1
  expect_state problems 2/0/0
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  echo "OK: an error with a new message opened the drawer"
  # Fixed while nobody looked, then back identically: the accepted cost, it stays
  # closed. A new message opens it again.
  problems_publish '{}'
  expect_state problems 0/0/0
  problems_publish "{\"src/lib.rs\":[$one]}"
  expect_state problems 1/0/0
  expect_state_stays drawer_open 0
  echo "OK: a seen error fixed and returning identically left the drawer closed (the accepted cost)"
  problems_publish "{\"src/lib.rs\":[$one,$three]}"
  expect_state drawer_open 1
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  # Warnings never open it.
  problems_publish "{\"src/lib.rs\":[$(pf alpha 2 'alarm warning: alpha is odd')]}"
  expect_state problems 0/1/0
  expect_state_stays drawer_open 0
  echo "OK: a warning left the drawer closed"
  # On Search the drawer stays on Search however new the error is.
  ctl surface editor drawer --show=search >/dev/null
  expect_state drawer_tab search
  problems_publish "{\"src/lib.rs\":[$one,$four]}"
  expect_state problems 2/0/0
  expect_state_stays drawer_open 1
  expect_state drawer_tab search
  echo "OK: with the drawer on Search a new error left it on Search"
  problems_done
}

scenario_problems_relaunch() {
  problems_reset
  local lib util
  lib="$(pf alpha 1 'relaunch lib: alpha is wrong')"
  util="$(pf helper 1 'relaunch util: helper is wrong')"
  problems_publish "{\"src/lib.rs\":[$lib],\"src/util.rs\":[$util]}"
  expect_state drawer_open 1
  expect_state problems 2/0/0
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  app_restart
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state drawer_open 0
  # The server starts with the first .rs file and reports the two files one at a
  # time; whatever prefix of them is listed, the drawer stays closed.
  expect_state problems 2/0/0
  expect_state_stays drawer_open 0
  echo "OK: after a quit and relaunch the same two errors came back one file at a time and the drawer stayed closed"
  problems_publish "{\"src/lib.rs\":[$lib,$(pf beta 1 'relaunch new: beta is wrong')],\"src/util.rs\":[$util]}"
  expect_state drawer_open 1
  echo "OK: an error with a new message opened it"
  problems_done
}

# The message of Problems' first row, in a window of CLICK_WINDOW with the drawer
# open at its default height (240 px) over main.rs: problems-open.png shows it
# when the drawer is at that height.
PROBLEM_ROW=(900 626)

scenario_problems_open() {
  problems_reset
  local at
  # The default height, whatever an earlier scenario left (the drawer scenario drags it).
  ctl surface editor drawer --height=240 >/dev/null
  expect_state drawer_height 240
  problems_publish "{\"src/lib.rs\":[$(pf target_marker 1 'open: target_marker is wrong')]}"
  expect_state drawer_open 1
  expect_state problems 1/0/0
  expect_state problem_files src/lib.rs
  # A real press on the row.
  focus_window
  click_at "${PROBLEM_ROW[@]}"
  expect_active_tab lib.rs
  expect_state path "$WT/src/lib.rs"
  expect_state state loaded
  at="$(python3 "$TOOLS" offset "$(sfield text)" "target_marker")"
  # The column the server gave is in UTF-16 units (the accent is one, the emoji
  # two); the caret is at the needle's byte offset.
  expect_state selections "$at..$at"
  echo "OK: a press on the problem's row opened src/lib.rs with the caret on the needle, past an accent and an emoji"
  # The verb chooses the same row without a pointer, from another tab, with the
  # caret of lib.rs put somewhere else first.
  ctl surface editor select --ranges=0..0 >/dev/null
  expect_state selections "0..0"
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state drawer_open 1
  ctl surface editor problem --open=0 >/dev/null
  expect_active_tab lib.rs
  expect_state selections "$at..$at"
  echo "OK: the verb opened the same row the same way"
  problems_done
}

scenario_problems_server_exit() {
  problems_reset
  local mark
  # A running server with nothing to say is named (the store is empty, so the line is drawn).
  expect_state problems_note "fake-lsp: no problems"
  problems_publish "{\"src/lib.rs\":[$(pf alpha 1 'exit: alpha is wrong')]}"
  expect_state drawer_open 1
  expect_state problems 1/0/0
  # The server ends the way a crashed one does: its entries go, and it is no longer named.
  python3 - "$RUN_DIR/fake-lsp/answers.json" <<'PY'
import json, os, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    answers = json.load(handle)
answers["exit"] = True
with open(path + ".tmp", "w", encoding="utf-8") as handle:
    json.dump(answers, handle)
os.replace(path + ".tmp", path)
PY
  expect_state problems 0/0/0
  expect_state problems_note "No language server is running in this worktree."
  echo "OK: the server ended: its problem is gone and Problems says no server is running"
  # The next .rs file opened starts it again.
  problems_restore
  mark="$(lsp_mark)"
  printf 'fn fresh() {}\n' >"$WT/src/fresh_exit.rs"
  open_file "$WT/src/fresh_exit.rs"
  expect_state state loaded
  local tries=0
  while [ "$tries" -lt 100 ] && [ "$(lsp_count initialize "$mark")" -eq 0 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ "$(lsp_count initialize "$mark")" -eq 1 ] \
    || fail "$SCENARIO: after the server ended, a new .rs file started it $(lsp_count initialize "$mark") times, expected once"
  expect_state problems_note "fake-lsp: no problems"
  echo "OK: opening another .rs file started the server again"
  # And after a relaunch with a running server and no findings (main.rs reports
  # none either: the answers list nothing).
  problems_publish '{}'
  expect_state problems 0/0/0
  app_restart
  open_file "$WT/main.rs"
  expect_state state loaded
  expect_state problems 0/0/0
  expect_state problems_note "fake-lsp: no problems"
  echo "OK: after a relaunch a running server with no findings is named"
  problems_done
}

scenario_problems_park() {
  problems_reset
  local park listed park_error
  park_error="$(pf alpha 1 'park: alpha is wrong')"
  # A second worktree of the repository, made the way the app makes one.
  ctl new-workspace --project "$WT" --branch park-problems >/dev/null
  listed="$("$CTL" list-workspaces 2>&1 || true)"
  park="$(printf '%s\n' "$listed" | awk -F'\t' '$3 == "park-problems" {print $4}' | head -n 1)"
  [ -n "$park" ] && [ -d "$park" ] || fail "$SCENARIO: the second worktree is not listed: $listed"
  ctl select-workspace --workspace "$WT" >/dev/null
  # The file whose gutter must come back right is open before the worktree is parked.
  open_file "$WT/src/lib.rs"
  expect_state state loaded
  problems_publish "{\"src/lib.rs\":[$park_error]}"
  expect_state drawer_open 1
  expect_state problems 1/0/0
  # The gutter is drawn from the same store.
  expect_state diagnostics 1
  ctl surface editor drawer --height=280 >/dev/null
  expect_state drawer_height 280
  ctl select-workspace --workspace "$park" >/dev/null
  open_file "$park/main.rs"
  expect_state state loaded
  # The other worktree has a drawer of its own, closed over nothing.
  expect_state path "$park/main.rs"
  expect_state drawer_open 0
  expect_state problems 0/0/0
  # What its sibling's server says meanwhile lands in the sibling's drawer.
  problems_publish "{\"src/lib.rs\":[$park_error,$(pf beta 2 'park: beta is odd')]}"
  sleep 1
  ctl select-workspace --workspace "$WT" >/dev/null
  open_file "$WT/src/lib.rs"
  expect_state state loaded
  expect_state path "$WT/src/lib.rs"
  # The tab was made again on the way back, and the server holds its text as it
  # was (nothing is sent again): its gutter is drawn from the store, with the
  # warning that came while the worktree was parked.
  expect_state diagnostics 2
  expect_state drawer_open 1
  expect_state drawer_tab problems
  expect_state drawer_height 280
  expect_state problems 1/1/0
  expect_state problem_files src/lib.rs
  echo "OK: a worktree switched away and back keeps its drawer, its height and every problem, the one that came while it was parked included, in Problems and in the file's gutter"
  problems_done
  ctl close-workspace --workspace "$park" >/dev/null 2>&1 || true
  git -C "$WT" worktree remove --force "$park" >/dev/null 2>&1 || true
  ctl select-workspace --workspace "$WT" >/dev/null
}

# ---------------------------------------------------------------- search

# Search in the drawer (editor panels, Task 7): the whole worktree, from inside the
# file tab. The fixtures are made here, in the run's own repository, by the
# scenario that needs them, and every scenario's needles are its own, so a result
# left by one cannot satisfy another. Nothing is committed: the files are
# untracked (git lists them) or ignored (`target/`), which is what `search_scope`
# is about.

# The files the search scenarios read: two small .rs files that both hold
# `host_root` (the first with the needle on lines 0 and 1, the second on lines
# 1, 2 and 4), written once.
search_fixtures() {
  mkdir -p "$WT/search"
  python3 - "$WT" <<'PY'
import os, sys
root = sys.argv[1]
with open(os.path.join(root, "search/other.rs"), "wb") as handle:
    handle.write(b"fn a() { host_root(); }\nfn b() { let x = host_root(); }\nfn c() {}\n")
with open(os.path.join(root, "search/session.rs"), "wb") as handle:
    handle.write(
        b"// session.rs: what the search scenarios read.\n"
        b"pub fn host_root(db: &str) -> String {\n"
        b"    let root = host_root_of(db);\n"
        b"    let data_root = root.clone();\n"
        b"    host_root_of(&data_root)\n"
        b"}\n"
    )
PY
}
# The drawer on Search, over search/session.rs, with an empty query, no filters
# and every option off: where each search scenario starts. The verb is how the
# state is set, never what a scenario is about.
search_reset() {
  search_fixtures
  open_file "$WT/search/session.rs"
  expect_state state loaded
  ctl surface editor drawer --show=search >/dev/null
  expect_state drawer_tab search
  ctl surface editor search --query= --include= --exclude= --case=false --word=false \
    --regex=false >/dev/null
  ctl surface editor search --wait >/dev/null
  expect_state search_query ""
  expect_state search_results 0
}
search_wait() {
  ctl surface editor search --wait >/dev/null
}
search_done() {
  ctl surface editor search --query= --include= --exclude= --case=false --word=false \
    --regex=false >/dev/null
  ctl surface editor search --wait >/dev/null
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
}
# The state's KEY contains NEEDLE (polled).
expect_state_has() { # key needle
  local got="" tries=0
  while [ "$tries" -lt 150 ]; do
    read_state
    got="$(sfield "$1")"
    case "$got" in *"$2"*) echo "OK: $1 holds '$2': $(printf '%s' "$got" | cut -c1-160)"; return 0 ;; esac
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: $1 expected to hold '$2', got '$(printf '%s' "$got" | cut -c1-400)'"
}

scenario_search_keys() {
  local at end
  search_reset
  at="$(python3 "$TOOLS" offset "$(sfield text)" host_root)"
  end=$((at + 9))
  cp "$WT/search/session.rs" "$RUN_DIR/search-keys-0.rs"
  python3 "$TOOLS" sub "$WT/search/session.rs" "$RUN_DIR/search-keys-q.rs" host_root q 1 0
  # The code has the keyboard and the host_root of line 1 is selected; the key is
  # a real X key at the window.
  ctl surface editor drawer --close >/dev/null
  expect_state drawer_open 0
  # The keys verb gives the code the keyboard (a select does not), and the
  # selection is made after it.
  ctl surface editor keys --keys=left >/dev/null
  ctl surface editor select --ranges="$at..$end" >/dev/null
  expect_state selections "$at..$end"
  focus_window
  x_key ctrl+shift+f
  expect_state drawer_open 1
  expect_state drawer_tab search
  expect_state search_query host_root
  search_wait
  expect_state search_results 5
  echo "OK: Ctrl+Shift+F opened Search with the selection as the query, and searched it"
  # The keyboard is in the query field: its text is selected, so a typed z
  # replaces it. The code is untouched.
  x_key z
  expect_state search_query z
  expect_state text "$(escaped "$RUN_DIR/search-keys-0.rs")"
  echo "OK: a real key typed after it landed in the query field"
  # Escape in the field hands the keyboard to the code of this tab: the next key
  # replaces the code's selection, and the query keeps its z.
  x_key Escape
  x_key q
  expect_state text "$(escaped "$RUN_DIR/search-keys-q.rs")"
  expect_state search_query z
  echo "OK: Escape in the query field gave the keyboard back to the code"
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state text "$(escaped "$RUN_DIR/search-keys-0.rs")"
  expect_state dirty false
  search_done
}

scenario_search_scope() {
  search_reset
  mkdir -p "$WT/notes" "$WT/target"
  printf 'fn made() { scope_needle_41(); }\n' >"$WT/notes/new.rs"
  printf 'fn built() { scope_needle_41(); }\n' >"$WT/target/gen.rs"
  printf 'target/\n' >"$WT/.gitignore"
  git -C "$WT" status --short --ignored | grep -q 'target/' \
    || fail "$SCENARIO: git does not ignore target/ (status: $(git -C "$WT" status --short --ignored))"
  ctl surface editor search --query=scope_needle_41 >/dev/null
  search_wait
  expect_state search_paths notes/new.rs
  expect_state search_results 1
  expect_state search_files 1
  # The summary: the count, then what the run left unread. big.txt is above the
  # size bound, binary.bin is not UTF-8 and unreadable.txt is mode 000.
  expect_state search_summary "1 results in 1 files · 3 skipped (too large/not UTF-8/unreadable)"
  expect_state search_capped 0
  echo "OK: an untracked file is searched, an ignored one is not, and the skips are named"
  ctl surface editor search --query=scope_needle_41 --include='target/**' >/dev/null
  search_wait
  expect_state search_results 0
  echo "OK: an include glob cannot bring an ignored file in"
  rm -rf "$WT/notes" "$WT/target" "$WT/.gitignore"
  search_done
}

scenario_search_options() {
  local fixture="$WT/findme.txt" all cased word regex
  search_reset
  open_file "$fixture"
  expect_state state loaded
  # What the in-file find counts with each option, asked of the open file; the
  # drawer's Search, limited to that file, must count the same.
  ctl surface editor find --query=fn >/dev/null
  expect_state_set find_matches
  all="$(sfield find_matches)"
  ctl surface editor find --query=fn --case=true >/dev/null
  expect_state_set find_matches
  cased="$(sfield find_matches)"
  ctl surface editor find --query=fn --word=true >/dev/null
  expect_state_set find_matches
  word="$(sfield find_matches)"
  ctl surface editor find --query='f\w+' --regex=true >/dev/null
  expect_state_set find_matches
  regex="$(sfield find_matches)"
  ctl surface editor find-close >/dev/null
  [ "$all" != "$cased" ] && [ "$all" != "$word" ] && [ "$all" != "$regex" ] \
    || fail "$SCENARIO: the options do not change the counts ($all $cased $word $regex)"
  ctl surface editor drawer --show=search >/dev/null
  ctl surface editor search --query=fn --include=findme.txt >/dev/null
  search_wait
  expect_state search_results "$all"
  ctl surface editor search --case=true >/dev/null
  search_wait
  expect_state search_results "$cased"
  ctl surface editor search --case=false --word=true >/dev/null
  search_wait
  expect_state search_results "$word"
  ctl surface editor search --word=false --regex=true --query='f\w+' >/dev/null
  search_wait
  expect_state search_results "$regex"
  echo "OK: case ($cased of $all), whole word ($word) and a regular expression ($regex) count as the in-file find does"
  # A pattern that does not compile is said in the summary, with no results and no panic.
  ctl surface editor search --query='(' >/dev/null
  search_wait
  expect_state search_results 0
  expect_state_has search_summary "unclosed group"
  echo "OK: an invalid regular expression is the summary, not a crash or an empty list"
  search_done
}

scenario_search_dirty() {
  search_reset
  open_file "$WT/search/session.rs"
  expect_state state loaded
  ctl surface editor select --ranges=0..0 >/dev/null
  ctl surface editor type --text=dirty_needle_77 >/dev/null
  expect_state dirty true
  ctl surface editor drawer --show=search >/dev/null
  ctl surface editor search --query=dirty_needle_77 >/dev/null
  search_wait
  expect_state search_results 1
  expect_state search_paths search/session.rs
  expect_state search_unsaved search/session.rs
  ! grep -q dirty_needle_77 "$WT/search/session.rs" || fail "$SCENARIO: the needle reached the disk"
  ctl surface editor search --open=0 >/dev/null
  expect_state selections "0..15"
  echo "OK: an unsaved edit is found in its buffer, listed as unsaved, and opened at its place"
  # Undone, and searched again: nothing is left, and nothing is marked.
  ctl surface editor keys --keys=ctrl-z >/dev/null
  expect_state dirty false
  ctl surface editor search --refresh >/dev/null
  search_wait
  expect_state search_results 0
  expect_state search_unsaved ""
  search_done
}

scenario_search_open() {
  local at
  search_reset
  open_file "$WT/main.rs"
  expect_state state loaded
  ctl surface editor drawer --show=search >/dev/null
  ctl surface editor drawer --height=300 >/dev/null
  ctl surface editor search --query=host_root --include='search/**' >/dev/null
  search_wait
  expect_state search_results 5
  expect_state search_files 2
  expect_state search_paths "search/other.rs;search/session.rs"
  ctl surface editor search --open=1 >/dev/null
  expect_active_tab other.rs
  expect_state path "$WT/search/other.rs"
  expect_state state loaded
  at="$(python3 "$TOOLS" offset "$(sfield text)" "let x = host_root")"
  at=$((at + 8))
  expect_state selections "$at..$((at + 9))"
  expect_state search_query host_root
  expect_state search_selected search/other.rs:2
  expect_state drawer_tab search
  echo "OK: a result opened its file with the match selected, and that tab's drawer shows the same search with it chosen"
  capture search-results
  ctl surface editor search --open=2 >/dev/null
  expect_active_tab session.rs
  expect_state search_selected search/session.rs:2
  echo "OK: the next result, in another file, kept the thread"
  search_done
}

# A tree big enough that a broad regular expression takes seconds: 150 files of
# about 900 KB each, every line a few words long.
search_big_tree() {
  rm -rf "$WT/search-big"
  mkdir -p "$WT/search-big"
  python3 - "$WT/search-big" <<'PY'
import os, sys
root = sys.argv[1]
line = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do\n"
body = line * (900000 // len(line))
for n in range(150):
    with open(os.path.join(root, f"f{n:03}.txt"), "w") as handle:
        handle.write(body)
        if n == 0:
            handle.write("supersede_b_needle\n")
PY
}

scenario_search_supersede() {
  search_reset
  search_big_tree
  # A: a regular expression no literal can pre-filter, over the whole big tree. It
  # is under way (the debounce has passed) when B arrives, and B reads one file.
  ctl surface editor search --query='(\w+\s+){4}supersede_a_needle' --regex=true \
    --include='search-big/**' >/dev/null
  sleep 0.6
  ctl surface editor search --query=supersede_b_needle --regex=false \
    --include='search-big/f000.txt' >/dev/null
  search_wait
  expect_state search_query supersede_b_needle
  expect_state search_results 1
  expect_state search_paths search-big/f000.txt
  # Longer than A could have run on its own: its answer, if it arrived late,
  # would have overwritten B's by now.
  sleep 4
  read_state
  [ "$(sfield search_results)" = 1 ] && [ "$(sfield search_paths)" = search-big/f000.txt ] \
    || fail "$SCENARIO: B's results were replaced: $(sfield search_results) in $(sfield search_paths)"
  echo "OK: the second query's results stand; the first run was superseded"
  rm -rf "$WT/search-big"
  search_done
}

scenario_search_cap() {
  search_reset
  mkdir -p "$WT/search-cap"
  python3 - "$WT/search-cap/many.txt" <<'PY'
import sys
with open(sys.argv[1], "w") as handle:
    for n in range(25000):
        handle.write(f"cap_needle_88 {n}\n")
PY
  ctl surface editor search --query=cap_needle_88 --include='search-cap/**' >/dev/null
  search_wait
  expect_state search_capped 1
  expect_state search_results 20000
  expect_state_has search_summary "stopped at 20 000 matches"
  # The window still answers after drawing it.
  read_state
  [ "$(sfield state)" = loaded ] || fail "$SCENARIO: the editor stopped answering after the capped search"
  echo "OK: 25 000 matches stop at 20 000 and the summary says so"
  rm -rf "$WT/search-cap"
  search_done
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
scenario breadcrumb scenario_breadcrumb
scenario snapshot scenario_snapshot
scenario git_marks scenario_git_marks
scenario status scenario_status
scenario status_actions scenario_status_actions
scenario notifications scenario_notifications
scenario find scenario_find
scenario replace scenario_replace
scenario multibyte scenario_multibyte
scenario escape scenario_escape
scenario send_to_agent scenario_send_to_agent
scenario focus_refresh scenario_focus_refresh
scenario hover_leave scenario_hover_leave
# Before prefs: that scenario leaves Settings drawn, where no file tab takes a key.
scenario drawer scenario_drawer
scenario problems_unopened scenario_problems_unopened
scenario problems_alarm scenario_problems_alarm
scenario problems_relaunch scenario_problems_relaunch
scenario problems_open scenario_problems_open
scenario problems_server_exit scenario_problems_server_exit
scenario problems_park scenario_problems_park
scenario search_keys scenario_search_keys
scenario search_scope scenario_search_scope
scenario search_options scenario_search_options
scenario search_dirty scenario_search_dirty
scenario search_open scenario_search_open
scenario search_supersede scenario_search_supersede
scenario search_cap scenario_search_cap
scenario prefs scenario_prefs

stop_hosts
if [ -n "$ONLY" ]; then
  [ "$SCENARIOS_PASSED" -gt 0 ] || fail "--only named no scenario of this script: ${ONLY//,/ }"
  echo "EDITOR E2E PARTIAL ($SCENARIOS_PASSED scenarios of --only, appearance $APPEARANCE, state-only $STATE_ONLY)"
  exit 0
fi
echo "EDITOR E2E OK ($SCENARIOS_PASSED scenarios, appearance $APPEARANCE, state-only $STATE_ONLY)"
