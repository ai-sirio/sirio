#!/bin/bash
set -eEuo pipefail

# End-to-end test of the chat's message actions (spec 2026-11-23-ely-chat-message-actions-design.md §7): Copy, Continue and
# Quote on chat messages, and Claude's Stop labelled `cancelled`, in the real, isolated debug Sirio on a private Xvfb, its
# native Claude chat talking to the stub `claude` (Scripts/Tests/stubs/richer/claude, the stream-json fixture in its
# `richer` mode) and its ACP chat to the stub `opencode` (Scripts/Tests/stubs/richer/opencode, the ACP fixture in its
# `richer` mode), the stubs of the richer chat messages E2E extended with the turns this one needs (`max tokens`, `slow`,
# `slow late`, a held `Continue`; ACP `max tokens`, `max turns`, `max tokens drop`, `slow`). It is driven over the debug
# control socket and read through `surface.chat.read` (per row `actions`, `actionsShown`, `actionBounds.<copy|quote|continue|
# row>` and a user row's `quote`; for the chat `continue`, `continueRow`, `composerQuote`, `composerQuoteBounds`, `queued`,
# `transcriptText`), and the debug verbs of `surface.chat.transcript` (`continue`, `toggleTurn`, `copy`, `quote`,
# `quoteSelection`). Every press a user makes
# with a pointer is a real X click (xdotool) at the pixels the readout reports: Copy, Quote and Continue on a row, the
# quote card's cross, the transcript menu's Select All, Copy and Quote selection. The system clipboard is read back through
# GTK on the E2E's own display (`env -i`, no WAYLAND_DISPLAY), with another application owning it before every press. Each
# Sirio gets its own HOME, XDG tree, database, socket and host root, under `env -i`; the user's ~/.config/mise/config.toml
# must not change.
#
# Scenarios (each starts a fresh Sirio; the names are the ones --only takes):
#   copy             real clicks on Copy: on a user row (hovered first: with the pointer away it has no button painted), on an
#                    older reply (hovered; Markdown: the clipboard holds the source with its asterisks) and on the last reply
#                    (always shown); each time the clipboard is compared with the row's text; then the `copy` verb on a reply
#                    (the same clipboard) and on a turn footer (refused, clipboard untouched)
#   rows             the last reply shows `always`, every other row `hover`, a footer none; a streaming reply has none, and no
#                    row is shown always while it streams; at its end the row appears; Select All and Copy through the menu put
#                    `transcriptText` on the clipboard, with no label of the action row in it
#   idle_cost        redraws (the performance trace's `Chat.render` and `SirioWorkspace.render` spans) over 5 s of an idle chat whose
#                    last reply shows its row, before any press and after a Copy press has settled: after <= before x 1.5 + 2, and
#                    neither above 3 a second; a window with a press in it must count redraws (the control), and the trace must
#                    have dropped nothing
#   continue_claude  `max tokens`, `fail max turns` and a Stop of `slow`, each in turn: the footer says why, Continue is on the
#                    last reply's row only (`continueRow`, `actions`), a folded turn draws none and the verb is refused, a real
#                    press sends a visible user row `Continue` that the stub hears on the same process; nothing after an ordinary end
#   continue_footer  a Stop before the first text (`think slow`): the turn has a thought and no reply, so Continue sits alone on its
#                    footer, which is itself the click that folds the turn; a real press on it sends the visible user row `Continue`
#                    (the stub hears it) and does not fold the turn (the control: the `toggleTurn` verb does, and the readout shows it)
#   continue_acp     the same with the opencode stub, with a typed draft and an accepted mention in the composer, both unchanged by
#                    a Continue (the draft, sent after it, carries the file block); an agent's error answer to the cancel is the
#                    late end of a stopped turn: the footer says cancelled, not ended, and Continue is offered
#   continue_hidden  Continue is `none` (and the verb says why) for an ACP chat whose agent was lost after a turn that stopped short and
#                    for one after its agent restarted, after an ordinary end, while a turn runs, and on a restored chat after an app
#                    relaunch
#   continue_resume  a Claude chat stopped by `max tokens`, then its process killed: Continue is `resume`, and a press relaunches
#                    with `--resume <session>` and then sends the prompt
#   continue_once    two `continue` verbs back to back: the first is accepted, the second is refused (a turn is running), and the stub
#                    heard one `Continue`
#   claude_stop      a Stop of a Claude reply: the footer says cancelled, nothing is added to the reply after it, Continue is offered;
#                    a Stop with an item queued sends the item and Continue is not offered over it; Stop, a second prompt before the
#                    stopped turn's late end, then the late end: no Continue while the second prompt runs
#   quote            Quote on a reply (a real press) fills the composer's card (`composerQuote`), its cross removes it; a send reaches
#                    the stub as the `> ` block and the user row reads its quote; Quote selection through the transcript menu; a second
#                    quote replaces the first; a quote sent while a turn streams is queued with its block (the `queued` readout); a
#                    command (`/compact`) goes without the quote and keeps it, `/etc/hosts is wrong` goes with it; an `@path` inside the
#                    quote reaches the agent unchanged; after an app relaunch the restored row still shows its quote; an ACP agent lost
#                    with the message hands the quote and the field back; the `quote` verb (a reply; a footer is refused) and
#                    `quoteSelection` with nothing selected (refused); Copy on a quoted user row (the block and the typed text); a
#                    quote sent with no typed text (the stub hears the block alone, the row draws the card)
#
# Every check prints `OK: <scenario>: <what>`; a failed one prints `FAIL: <scenario>: <why>`, its scenario ends there and the
# run goes on with the next one. The run prints MESSAGE ACTIONS E2E OK (<n> checks) and exits 0 only if every check of every
# scenario passed.
#
# The artifact: --out-dir DIR (default artifacts/message-actions-e2e) keeps transcript.log, checks.jsonl (every check, in order, each
# with the newest readout of the chat when it was judged, `read`, and what it measured, `detail`), scenarios/<name>.json (the same,
# per scenario, and the series each read), the stubs' call and prompt log of each Sirio (scenarios/<name>.<world>.stub.log,
# `kind<TAB>pid<TAB>text`), logs/ (the app logs) and frames/*.png (PID-matched window captures). Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-chat-message-actions-e2e.sh [--out-dir DIR] [--display :N] [--only NAME[,NAME...]]
#   --display :N   the Xvfb this script starts and stops, on :140 to :159 (default :141), never $DISPLAY
#   --only         runs some scenarios and prints MESSAGE ACTIONS E2E PARTIAL instead
#
# Needs: python3 with gi (GTK 3) for the clipboard, xdotool, xwininfo, xprop, xdpyinfo, import, identify, Xvfb,
# Mesa's lavapipe, and the checkout's own build (this script runs `cargo build -p sirio --bin sirio` and
# `cargo build -p sirio_control --bin sirioctl`). Linux and X11 only: its input and its captures are X's. Run it from a
# session with no WAYLAND_DISPLAY (`env -u WAYLAND_DISPLAY`), as the identity E2E does, and never on your own $DISPLAY.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
OUT_DIR="$ROOT/artifacts/message-actions-e2e"
DISPLAY_TARGET=":141"
ONLY=""
WINDOW_SIZE="2200x1000"
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --only) ONLY="${2:?--only needs scenario names}"; shift 2 ;;
    -h|--help) sed -n '3,76 p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$OUT_DIR" in /*) ;; *) OUT_DIR="$PWD/$OUT_DIR" ;; esac
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
# What an earlier run of this script left; nothing else in the directory is touched.
rm -rf "$OUT_DIR/frames" "$OUT_DIR/scenarios" "$OUT_DIR/logs"
rm -f "$OUT_DIR/transcript.log" "$OUT_DIR/checks.jsonl"
mkdir -p "$OUT_DIR/frames" "$OUT_DIR/scenarios" "$OUT_DIR/logs"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux"
for tool in python3 git xdotool xwininfo xprop xdpyinfo import identify Xvfb; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
[ "$(id -u)" -ne 0 ] || fail "run as a user, not root"
case "$DISPLAY_TARGET" in
  :14[0-9] | :15[0-9]) ;;
  *) fail "--display must be between :140 and :159 (got $DISPLAY_TARGET)" ;;
esac
[ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY"
python3 -I -c 'import gi; gi.require_version("Gtk", "3.0"); gi.require_version("Gdk", "3.0"); from gi.repository import Gtk' 2>/dev/null \
  || fail "python3 with gi (GTK 3) is required to read the clipboard"
LVP=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
[ -n "$LVP" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required for a framed run on Xvfb"

# The user's mise config must not move: an unisolated `claude --version` goes through the user's mise wrapper.
MISE_CONFIG="$HOME/.config/mise/config.toml"
mise_hash() { sha256sum "$MISE_CONFIG" 2>/dev/null | cut -d' ' -f1 || true; }
MISE_BEFORE=$(mise_hash)
echo "mise config: ${MISE_BEFORE:-absent}"

# The run's scratch space is on disk: /tmp is a tmpfs.
BASE="${TMPDIR:-$HOME/.cache/st/rail-e2e-tmp}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/acts-XXXXXX")"
XVFB_PID=""
terminate() { # pid: TERM, then KILL if it has not exited within 5 s
  kill -TERM "$1" 2>/dev/null || return 0
  (sleep 5; kill -KILL "$1" 2>/dev/null) >/dev/null 2>&1 &
  local watchdog=$!
  wait "$1" 2>/dev/null || true
  kill "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
}
cleanup() {
  local rc=$?
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  rm -rf --one-file-system "$RUN_DIR"
  exit "$rc"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl) 2>"$RUN_DIR/build.log" \
  || { tail -n 40 "$RUN_DIR/build.log" >&2; fail "the build failed"; }
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
[ -x "$BIN" ] || fail "no $BIN"
[ -x "$CTL" ] || fail "no $CTL"

number="${DISPLAY_TARGET#:}"
{ [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ]; } && fail "display $DISPLAY_TARGET is taken"
Xvfb "$DISPLAY_TARGET" -screen 0 2400x1200x24 >"$OUT_DIR/logs/xvfb.log" 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
echo "display: $DISPLAY_TARGET (Xvfb pid $XVFB_PID); vulkan: lavapipe"

rc=0
env ROOT="$ROOT" RUN_DIR="$RUN_DIR" OUT="$OUT_DIR" DISP="$DISPLAY_TARGET" BIN="$BIN" CTL="$CTL" \
  WINDOW_SIZE="$WINDOW_SIZE" ONLY="$ONLY" VK_DRIVER_FILES="$LVP" VK_ICD_FILENAMES="$LVP" \
  python3 -I "$ROOT/Scripts/Tests/chat_message_actions_e2e.py" || rc=$?
[ "$(mise_hash)" = "$MISE_BEFORE" ] || fail "$MISE_CONFIG changed (was ${MISE_BEFORE:-absent}, is $(mise_hash))"
echo "mise config unchanged: ${MISE_BEFORE:-absent}"
echo "artifact: $OUT_DIR"
exit "$rc"
