#!/bin/bash
set -eEuo pipefail

# End-to-end test of the richer chat messages (spec 2026-11-21-ely-chat-richer-messages-design.md §7.2): the
# real, isolated debug Sirio on a private Xvfb, its native Claude chat talking to a stub `claude`
# (Scripts/Tests/stubs/richer/claude, the stream-json fixture in its `richer` mode) and its ACP chat to a stub
# `opencode` (Scripts/Tests/stubs/richer/opencode, rust/crates/sirio_ui/tests/fixtures/chat_fixture.py in its
# `richer` mode), driven over the debug control socket and read through the debug readouts of
# `surface.chat.read` (`live`, `shownBytes`, `transcriptText`, `transcriptTextRows`, `held`, `heldLine`,
# `daySeparators`, an error row's `errorKind` and `retry`, a `limitCard` row's `phase`, a user row's `at` and a
# thought's `durationMs`), the debug `surface.chat.transcript` action `retry`, and real X input for the two
# things the socket has no verb for: the transcript menu's Select All and Copy (read back through the display's
# real clipboard) and the queue's remove button. Each Sirio gets its own HOME, XDG tree, database, socket and
# host root, under `env -i`; the user's ~/.config/mise/config.toml must not change.
#
# Scenarios (each starts a fresh Sirio; the names are the ones --only takes):
#   stream             a 40 KiB reply (the SHOWCASE document, then parts that freeze as they arrive) in 200 chunks
#                      20 ms apart: while it is live the reveal is behind (`0 < shownBytes < len(text)`, it only
#                      grows, and it is still behind past 24 KiB, which only a reply that froze its blocks as it
#                      arrived can be: a reply that freezes little hands over to plain drawing soon after its tail
#                      passes 7 KiB), and `transcriptText`
#                      holds the newest marker even where the reveal has not reached it; at the turn's end `live` is
#                      false at once with `shownBytes == len(text)`; the same text sent in one chunk gives the same
#                      plain text. Select All and Copy through the transcript menu while another reply streams
#                      (`showcase slow`, 8 s), with the transcript scrolled to its top: the live reply is then not
#                      drawn, so the reveal stands still (`shownBytes` is read again after the Copy and has not
#                      moved) while the text keeps arriving, and the clipboard holds a marker of the live reply past
#                      `shownBytes`. A reply with no blank line (`compact`, 24 KiB) never freezes a block, so once its
#                      tail passes the 7 KiB budget the reveal hands over to drawing as it arrives for the rest of the turn
#                      (spec 5.2): from 14 KiB of text on every live read has `shownBytes == len(text)` with `live` still
#                      true; it ends with `live` false and its text equals the same bytes in one chunk. SHOWCASE eight
#                      times over (`showcase x8`, every copy cites [^1] again; it freezes by a varying amount, so whether
#                      its reveal hands over depends on when the frames fall) is checked only for its end and its equivalence.
#   stop               Stop in the middle of the reveal: `shownBytes` is behind the text at the read before the Stop,
#                      and at the first read after it `live` is false and `shownBytes` the whole text; no error row,
#                      nothing held, the text equals the same bytes in one chunk; the agent killed right after has
#                      nothing to resend (`retry: relaunch`)
#   claude_turn_failed an is_error result is an error row `turnFailed` whose Retry resends (`retry: resend`) the
#                      same text on the same process, and the second answer arrives; `error_max_turns` stays a
#                      reply with no error row. Queue variant: two messages queued behind the failing turn are
#                      held (`held`, the "Waiting: press Retry or send a message." line), one is removed with its
#                      real X click, Retry lifts the hold at once, and the other is sent only at the retried turn's
#                      end. The partial reply of the failed turn equals the same bytes in one chunk.
#   acp_prompt_failed  the same over ACP: the failed `session/prompt` leaves the process up (its partial reply ended),
#                      Retry resends on the same pid, which was asked for one session; a failure that follows Stop is the
#                      late end of the stopped turn (the fixture logs that it got `session/cancel` and answered with an
#                      error), with no error row
#   claude_lost_resume the CLI exits mid-turn: the row says `retry: resume`, Retry relaunches with
#                      `--resume <id>` (the stub records it) and the prompt is resent; killed while idle, after a finished
#                      turn, the row says `retry: relaunch` and Retry starts a new process and sends nothing
#   acp_lost_composer  the ACP agent exits mid-turn (`retry: composer`): Retry relaunches, the message is back in the
#                      composer with the notice and nothing is sent; with no turn in flight (a launch that fails, then
#                      works: `retry: relaunch`) nothing is put back; with a draft in the composer the draft stays. An ACP
#                      agent killed while idle is not noticed until the next send (as built, spec 7.2): that send lands
#                      on `retry: composer`, the dead agent never hears it and Retry puts it back in the composer
#   limit              a rejected rate limit a few seconds ahead is a `limitCard` row counting (`resetsAt`), its failed
#                      turn's row says `coveredBy: limitCard` and `retry: none`, the socket's Retry is refused, a held
#                      queue says "Waiting: send a message."; then, once the reset has passed, the card reads `retry`,
#                      Retry resends, and the card ends `lifted` (and refuses Retry); a rejection in a turn that did not
#                      fail ends `lifted` once the reset has passed, and so does a card whose Retry a new prompt retired
#   day_separators     a restored transcript whose user entries are on three days has separators "Today",
#                      "Yesterday" and a full date (worked out from the clock after the relaunch, midnight-proof),
#                      an undated turn has none and no `at`, a new turn has `at` now
#   thought_end        the CLI closes its thinking block and then waits 2 s before the text: `durationMs` is the
#                      block's, under 1 s, once the thought has settled and the text has not arrived
#
# Every check prints `OK: <scenario>: <what>`; a failed one prints `FAIL: <scenario>: <why>`, its scenario ends
# there and the run goes on with the next one. The run prints CHAT RICHER MESSAGES E2E OK (<n> checks) and exits 0
# only if every check of every scenario passed.
#
# The artifact: --out-dir DIR (default artifacts/chat-richer/e2e) keeps transcript.log, checks.jsonl (every check,
# in order, each with the newest readout of the chat when it was judged, `read`, and what it measured, `detail`),
# scenarios/<name>.json (the same, per scenario, and the series each read), the stubs' call and prompt log of each Sirio
# (scenarios/<name>.<world>.stub.log, `kind<TAB>pid<TAB>text`), logs/ (the app logs) and frames/*.png (PID-matched
# window captures). Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-chat-richer-messages-e2e.sh [--out-dir DIR] [--display :N] [--only NAME[,NAME...]]
#   --display :N   the Xvfb this script starts and stops, on :140 to :159 (default :140), never $DISPLAY
#   --only         runs some scenarios and prints CHAT RICHER MESSAGES E2E PARTIAL instead
#
# Needs: python3 with gi (GTK 3) for the clipboard, xdotool, xwininfo, xprop, xdpyinfo, import, identify, Xvfb,
# Mesa's lavapipe, and the checkout's own build (this script runs `cargo build -p sirio --bin sirio` and
# `cargo build -p sirio_control --bin sirioctl`). Linux and X11 only: its input and its captures are X's.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
OUT_DIR="$ROOT/artifacts/chat-richer/e2e"
DISPLAY_TARGET=":140"
ONLY=""
WINDOW_SIZE="2200x1000"
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --only) ONLY="${2:?--only needs scenario names}"; shift 2 ;;
    -h|--help) sed -n '3,83p' "${BASH_SOURCE[0]}"; exit 0 ;;
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
RUN_DIR="$(mktemp -d "$BASE/richer-XXXXXX")"
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
  python3 -I "$ROOT/Scripts/Tests/chat_richer_messages_e2e.py" || rc=$?
[ "$(mise_hash)" = "$MISE_BEFORE" ] || fail "$MISE_CONFIG changed (was ${MISE_BEFORE:-absent}, is $(mise_hash))"
echo "mise config unchanged: ${MISE_BEFORE:-absent}"
echo "artifact: $OUT_DIR"
exit "$rc"
