#!/bin/bash
set -eEuo pipefail

# End-to-end test of Sirio as Claude Code's IDE (/ide): a real, isolated debug Sirio on a
# private Xvfb, a stand-in client (Scripts/Tests/ide_client.py) running in one of its terminal
# panes the way `claude` would, and the files on disk. Everything is private: HOME (so the
# lockfile goes to $HOME/.claude/ide/ inside the run), the database, the control socket, the host
# root and TMPDIR (on disk under ~/.cache/st, never the small /tmp tmpfs). Nothing of the real
# ~/.claude, the installed Sirio or a running Sirio is touched: every sirioctl and socket call
# below names this run's own socket. Linux and X11 only.
#
# Cases, in the order they run:
#   lockfile_and_env  the lockfile has exactly six keys, mode 0600 in a 0700 directory, ideName
#                     "Sirio (debug)", the app's pid and the repository as a workspace folder; a
#                     new pane's environment holds the port the state reports
#   token_refused     a client that offers a wrong token is refused (401) and is no connection
#   handshake         a client with the right token is accepted, the server echoes the mcp
#                     subprotocol, tools/list names exactly the four tools, the state lists the
#                     connection with the client's pid
#   switch_off        `surface.ide.switch on=false`: the lockfile is gone, the client's connection
#                     is closed, the state says enabled:false with no port; on=true: the same
#                     port and token are back
#   switch_flap       the switch sent off, on, off, on from one process, back to back, while a
#                     start is held in flight (the debug knob SIRIO_DEBUG_IDE_START_DELAY_MS, set
#                     for the whole run): the port is the original, the state agrees, and exactly
#                     one lockfile exists
#   port_survives_catalog_change
#                     a worktree added with workspace.create joins the lockfile's workspace
#                     folders, and a pane in it still gets the port
#   restart_keeps_port
#                     quit (the lockfile goes) and relaunch: the same port and token, the
#                     lockfile is back, and the old pane's client reconnects with that token
#   inherited_port    Sirio launched with CLAUDE_CODE_SSE_PORT=1 hands its own port to panes;
#                     with the switch off a new pane has none
#   owner_resolved    the state shows the client's connection owned by the pane it runs in
#                     (the pane id panel create returned) and by that pane's worktree
#   selection_utf16  opening a file sends its selection once (a focused file says it; once
#                    within the run's send window); in a file whose line 3 is
#                    `let s = "<emoji>x";` the `x`, selected with surface.editor.select,
#                    arrives as selection_changed with start {2, 11}, end {2, 12} (UTF-16
#                    columns), isEmpty false, text "x"
#   selection_coalesced
#                    20 surface.editor.select calls 25 ms apart: at most one notification per
#                    send window (SEND_AFTER_MS) of the burst's duration, plus one, and the last
#                    one is the final selection (the 100 ms window of a release build is proved
#                    by sirio_ide's unit tests)
#   focus_sends_selection
#                    selecting a terminal tab sends nothing; selecting the file tab again sends
#                    exactly one notification, the current selection (each count is read after
#                    the send window has passed)
#   at_mention       two Claude terminals: the first one's pane is focused before its client
#                    starts, the second is created after it, the file is opened, both clients
#                    connect and lines 5 to 7 are sent with "Add to Agent Thread"
#                    (surface.editor.context ... send-to-agent): at_mentioned lineStart 4,
#                    lineEnd 6 reaches the first (the one used last), not the second; its tab is
#                    focused and nothing is written into the pane
#   selection_while_terminal_active
#                    with the file in the Secondary half and the Claude terminal the active tab,
#                    a click and Shift+Down (xdotool) in the file reach the client as
#                    selection_changed for the new lines
#   at_mention_to_new_terminal
#                    a terminal created after the others, focused when its client connects, gets
#                    the next at-mention
#   no_eligible_agent_toast
#                    with every terminal and the chat closed, the same command leaves the toast
#                    "No Claude terminal or chat in this worktree" in the state's toasts
#   at_mention_without_focus_history
#                    in a fresh worktree, a terminal nobody focused (another tab was active when
#                    its client connected) still gets the at-mention
#   owner_after_restart
#                    the client's pane is in a worktree that is not selected; Sirio quits and
#                    relaunches and the client reconnects: the state shows it ownerless, in
#                    that worktree (its cwd); six seconds later (the reconnect's retries are
#                    over) selecting the worktree gives it an owner (no file tab is restored, so
#                    only the selection can resolve it)
#   at_mention_after_relaunch
#                    the same relaunch with a file as the active tab: one at-mention from that
#                    file reaches the reconnected client (nothing was focused since the
#                    relaunch), focuses its tab and leaves the toast count as it was
#   selection_not_lost_to_another_worktree
#                    a file of another worktree sits in this worktree's strip: select in this
#                    worktree's file, then select the other tab once the select is answered;
#                    the case fails unless the tab change was answered less than SEND_AFTER_MS
#                    after the select was sent (inside the select's send window), and the
#                    client still gets the selection of the first file
#   diagnostics_file the fake language server (Scripts/Tests/fake_lsp.py; the run maps every
#                    extension it opens, .py and .rs, to it through the first-match
#                    languages.toml, so no other server is looked for, and an end check
#                    backstops that for live processes) reports one warning on `hello` in
#                    a.py; getDiagnostics {uri} for that file answers one item with the uri
#                    echoed, severity "Warning", start {1, 4}, end {1, 9}
#   diagnostics_outside_worktree
#                    getDiagnostics {uri: file:///etc/hostname} answers [] (not an error)
#   diagnostics_workspace
#                    a.py of a second worktree carries the same finding; getDiagnostics {} from a
#                    client owned by the first worktree's pane answers only the first worktree's
#                    files, the seeded one included
#   diagnostics_ownerless_cwd
#                    after every earlier delayed resolution has fired, a client started outside
#                    every Sirio shell (run by the script itself) with its cwd in the second
#                    worktree asks getDiagnostics {} at once; the answer holds only the second
#                    worktree's files, and the state's hook_resolutions (resolutions a hook
#                    started, not a request) is the same before the client started and right
#                    after the answer, else the case fails naming the broken precondition: then
#                    the request resolved the connection itself. The state then shows that
#                    connection (by pid) ownerless there
#   diagnostics_ownerless_elsewhere
#                    the same, with the same hook_resolutions guard, with its cwd outside every
#                    worktree: the answer holds the files of every worktree of the sidebar (both
#                    seeded ones, a.py of each); the state shows it ownerless with no worktree
#   accept           (openDiff) a client sends openDiff for p.txt: a tab titled "✻ p.txt" opens in
#                    Secondary, the active tab is still the Claude terminal, the bar names the
#                    owner terminal and the counts [1, 1], the worktree is the repository's; nothing is answered
#                    until `surface.ide.act accept`, which answers FILE_SAVED and the proposal; the
#                    file on disk is untouched
#   edit_then_accept a CRLF file: `act edit text="edited\n"`, `act show_diff`, `act accept` answers
#                    exactly `edited\r\n`
#   edit_keys        `act edit`, then real xdotool keys typed into the Edit body (guarded the way
#                    selection_while_terminal_active is: the click must not change the active tab);
#                    Accept answers text holding the typed characters
#   reject           `act reject` answers DIFF_REJECTED and the tab name
#   close_confirm    `act close` asks "Closing rejects Claude's change" (Reject, Cancel); Cancel keeps
#                    the tab and answers nothing; close again + confirm_reject answers DIFF_REJECTED
#   close_others_guarded
#                    Close Others on a file tab of Secondary with the proposal among the others, and
#                    `surface.tabs.close ... confirm=yes` on the proposal itself, ask before anything
#                    closes; Cancel keeps every tab; Reject answers, then the others close
#   close_tab_withdraws
#                    the client's close_tab answers TAB_CLOSED, the pending openDiff gets -32000 (no
#                    decision), the tab is gone
#   close_all        closeAllDiffTabs answers CLOSED_2_DIFF_TABS, both openDiffs get -32000
#   replace_in_place an openDiff with the tab_name of an open one: the first gets -32000, no second
#                    tab, Accept answers the second's text
#   disconnect_closes
#                    the client closing its socket closes the tab with the toast "Claude closed its
#                    proposed change" and sends no decision
#   new_file         a path that does not exist yet opens, all added ("+2 -0"); Sirio creates nothing
#   fifo_path        a FIFO as old_file_path opens a tab saying why, with no Edit (act edit fails);
#                    Reject still answers
#   too_large        a file over 1 MiB: the refusal body, act edit fails, Accept answers FILE_SAVED
#                    with Claude's text
#   dirty_warning    the file is open in an editor tab; once its proposal is open, typing into it
#                    (`surface.editor.type`) makes the state's dirty true and the bar warn
#   captures         the tab in Unified and Split (`surface.changes.view`)
#   no_decision_without_user
#                    with an openDiff pending, switching the bridge off, the client dropping its
#                    socket, and the app quitting each send no FILE_SAVED, DIFF_REJECTED or
#                    TAB_CLOSED for that request id (read from the client's transcript)
#   worktree_close_guarded
#                    two Claudes wait on a worktree (one in its pane, one outside every Sirio
#                    shell): `workspace.close` over the socket fails; the sidebar's confirmed
#                    "Remove worktree?" (driven by `act close_worktree`) asks before git touches the
#                    checkout or any session ends; Cancel keeps the checkout on disk, listed by
#                    `git worktree list`, its branch and both connections; Reject answers
#                    DIFF_REJECTED (read by the outside client), then the checkout is gone from disk
#                    and from git, and the pane's session ends
#   reject_leaves_no_inert_tab
#                    a dirty file tab among Close Others: Reject in the confirmation answers
#                    DIFF_REJECTED and the proposal's tab is gone at once; the dirty-tab prompt is up
#                    (the state says so), a click on Cancel closes it (the state says so), the dirty tab stays
#   confirm_stable_tabs
#                    a Close Others question is open, another worktree is selected, Reject: no tab
#                    of that worktree (clean file tabs, which close without asking) closes and nothing
#                    is answered (the question held identities, not positions)
#   confirm_stable_worktree
#                    three worktrees a-gone, b-asked, c-next made with git under one directory (git
#                    lists them sorted by path): the removal question of b-asked is open while a-gone
#                    is removed behind the app's back and the sidebar rescans, so b-asked's row moves
#                    up by one (asserted from the listing); Reject removes b-asked (looked up again
#                    by path) and neither c-next nor any other worktree
#   focus_in_secondary_keeps_editor
#                    the Primary terminal is the active tab and the user clicks (xdotool) into the file
#                    shown in Secondary, then an openDiff arrives: the proposal joins the strip, the
#                    app's pane state says Secondary still shows the file, and keys typed after (xdotool)
#                    land in that file's buffer (read from the editor state)
#   focus_in_primary_shows_proposal
#                    a file shown in Secondary, a click into the Primary terminal, an openDiff: the
#                    proposal is what Secondary shows
#   symlink_title    a symlink CLAUDE.md -> AGENTS.md as the file: the tab is titled "✻ CLAUDE.md"
#   project_close_guarded
#                    the same for the sidebar's "Remove project from Sirio?" (RemoveProject, driven by
#                    `act remove_project`) on a second project: it asks first and ends no session;
#                    Cancel keeps the project listed and both Claudes connected; Reject answers
#                    DIFF_REJECTED (read by the outside client), then the project is gone
#   parked_worktree  relaunched with the mounted-worktree cap seeded at 2 in the isolated database
#                    (appearance_seed --limit-mounted 2) and a stale file left in the scratch
#                    directory (which the launch sweeps; the directory is 0700): a proposal for a
#                    worktree that is not on screen has no tab, state on_screen false, and the toast
#                    "Claude proposes a change in <name>", the bar naming its owner terminal; selecting
#                    the worktree brings the tab; it survives that worktree's eviction; Accept
#                    answers; and a Claude outside every Sirio shell (no owner) proposing a file of a
#                    worktree not on screen is held under the file's worktree too
#   reject_with_comments_clipboard
#                    one Old-side comment on a line and one New-side comment on a range, written
#                    through `surface.ide.comment` (the composer's own path): the state counts 2
#                    comments and 2 cards and no open composer; Reject answers DIFF_REJECTED,
#                    closes the tab, the state says last_delivery "clipboard", the toast says
#                    "Comments copied: paste them into Claude", and the CLIPBOARD (read from outside
#                    the app, on this run's display) is exactly the two `@<path>#L<n> ...` lines,
#                    the path relative to the worktree, `#L<a>-<b>` for the range, `(proposed) `
#                    before the New side's text
#   comment_composer_anchor
#                    a 300-line file with every tenth line changed (thirty hunks, so line 150
#                    starts well below the viewport): a comment with commit=false opens the
#                    composer at Old line 150 holding a seven-character draft (the state shows
#                    side, line, draft length, no comment, no card, and in_view true: the diff's
#                    list, whose layout gpui computed, has scrolled that line's row into its
#                    viewport); Reject answers DIFF_REJECTED and the CLIPBOARD, which the case
#                    owned before, is unchanged and `deliveries` has not moved: a draft is never
#                    delivered
#   composer_typing_keeps_line
#                    the same composer, then a real click into its text box and real keys
#                    (xdotool): the draft grows to the typed length and in_view is still true (a
#                    keystroke re-splices the diff's list; the view must not jump to the top)
#   comment_refusals the verb's errors leave the tab as it was and name their reason: an unknown
#                    tab, a line that is not in the diff, a side that is neither old nor new, an
#                    end_line that is no number, a commit that is neither true nor false, a
#                    comment while Edit shows no diff, and (Ruling 27) a New-side comment on the
#                    diff of the edited text
#   comments_on_withdraw
#                    two comments and an open draft, then Claude's close_tab: TAB_CLOSED, the
#                    openDiff gets -32000, one delivery, and the clipboard holds the two comments
#                    and not the draft
#   comments_on_close_all
#                    two proposals with a comment each, then closeAllDiffTabs: both openDiffs get
#                    -32000, ONE delivery (the copy and its toast are one step), and the clipboard holds both comments
#   comments_on_replace
#                    a comment, then an openDiff with the same tab_name: the old openDiff gets
#                    -32000, one delivery, the clipboard holds the old proposal's comment
#   comments_on_accept
#                    a comment, then Accept: FILE_SAVED, one delivery, the clipboard holds it
#   comments_on_confirmation_reject
#                    a comment, `act close`, the question, confirm_reject: DIFF_REJECTED, one
#                    delivery, then the clipboard
#   comments_on_disconnect
#                    a comment, then the client closes its socket: no decision, one delivery and
#                    the clipboard
#   (`deliveries` in the state counts the clipboard deliveries since launch, so each case tells its
#   own from an earlier one's; the newest toast is the copy's.)
#
# `surface.ide.act` (debug builds only) presses what a user presses: accept, reject, edit (with
# `text`, which errors when Edit is unavailable), show_diff, close, confirm_reject, cancel,
# close_others (`text` is the kept tab's position), close_worktree and remove_project.
# `surface.ide.comment {tab_name, side: old|new, line, end_line?, text, commit?}` (debug builds only)
# does what a click on the diff's gutter and typing do: it opens the composer at that anchor,
# sets its text and adds the comment, unless commit=false leaves the text as a draft.
#
#
# Three debug-only knobs (read under cfg(debug_assertions), so absent from release builds) are set
# for every launch: SIRIO_DEBUG_IDE_START_DELAY_MS=600 holds each listener start in flight
# (switch_flap), SIRIO_DEBUG_IDE_SEND_AFTER_MS widens the selection send window from 100 ms
# to SEND_AFTER_MS (the selection cases; the value and why are at SEND_AFTER_MS below), and
# SIRIO_DEBUG_IDE_RESOLVE_AFTER_MS=3000 delays the first resolution a connection's announcing
# itself starts (never one a request runs; when it fires it resolves every ownerless connection),
# for the whole run: the cases that wait for an owner or a worktree in the state wait up to 3 s
# longer for it, which with the ownerless cases' own waits adds 15 to 19 s to a run.
#
# The artifact: --out-dir DIR (default artifacts/ide-bridge-e2e-<stamp>-<pid>) keeps
# transcript.log, app.log, client-*.jsonl (every frame a client sent and received), the lockfile
# with its authToken redacted, the PID-matched window captures in frames/ and socket.jsonl: one
# JSON line per request this script sent to the app's control socket, with the app's complete
# answer. The state, settings, switch and selection-burst calls go straight to the socket; every sirioctl call
# (the app-ready poll's attempts included) goes through a recording proxy that forwards it
# unchanged. A request that got no answer (a connect error, an EOF, a timeout, an answer that is
# not JSON) is a line with the request and an `error` and no `response`. NOT recorded: the
# `system.ping` route probe sirioctl sends by itself, and only before `panel.*` calls (the proxy
# forwards it and leaves it out of the log). Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-ide-bridge-e2e.sh [--xvfb | --display :N] [--state-only] [--proposals-only] [--out-dir DIR]
#
#   --xvfb        start a private Xvfb on a free display between :90 and :99
#   --display :N  use an Xvfb this user started (never your own $DISPLAY)
#   --state-only  take no window captures; the window still draws
#   --proposals-only
#                 run only the openDiff cases (development; it prints a different last line and
#                 is not the evidence of a run)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
SEED="$TARGET_DIR/debug/examples/appearance_seed"
CLIENT="$ROOT/Scripts/Tests/ide_client.py"

STATE_ONLY=0
PROPOSALS_ONLY=0
START_XVFB=0
DISPLAY_TARGET=""
OUT_DIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --state-only) STATE_ONLY=1; shift ;;
    --proposals-only) PROPOSALS_ONLY=1; shift ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/ide-bridge-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: unexpected error at line $LINENO" >&2' ERR

for tool in python3 git xdotool xwininfo xprop xdpyinfo; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
if [ "$STATE_ONLY" -eq 0 ]; then
  for tool in import identify; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi
python3 -I -c 'import gi; gi.require_version("Gtk", "3.0")' 2>/dev/null || fail "python3-gi with GTK 3 is required (it reads and owns the CLIPBOARD)"
[ -n "$DISPLAY_TARGET" ] || [ "$START_XVFB" -eq 1 ] || fail "no display; pass --xvfb or --display :N"
[ -z "$DISPLAY_TARGET" ] || [ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY; pass --xvfb, or an Xvfb you started on another number"

# ---------------------------------------------------------------- build

echo "building sirio, sirioctl, sirio-host and the appearance seed"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host -p sirio_persistence --example appearance_seed)
for binary in "$BIN" "$CTL" "$SEED"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# ---------------------------------------------------------------- the run

BASE="${SIRIO_E2E_TMP:-$HOME/.cache/st/ide-e2e-tmp}"
mkdir -p "$BASE"
RUN_DIR="$(mktemp -d "$BASE/ide-XXXXXX")"
WT="$RUN_DIR/repo"
APP_PID=""
XVFB_PID=""
CASE="setup"
CASES_PASSED=0
CLIENT_N=0
CLIENT_TAB=""
CLIP_PID=""
# The selection send delay of this run, in ms: every launch sets the debug-only
# SIRIO_DEBUG_IDE_SEND_AFTER_MS to it (read under cfg(debug_assertions), absent from release
# builds), so the app waits this long, not 100 ms, before it sends a connection's coalesced
# selection. selection_not_lost_to_another_worktree needs a select and then a tab change, two
# control requests one after the other, to land inside one window: each waits for a tick of
# the app's 40 ms control poll and one UI pass. Under Xvfb, over six runs, the select was
# answered 7 to 44 ms after its send and the tab change 86 to 135 ms after its own, the pair
# 102 to 179 ms after the select's send, so 1000 ms holds the pair more than five times over on
# a loaded machine, and the case fails, rather than pass for nothing, if it ever does not. Every case that waits for a selection to arrive, or for
# none to, waits past this window: a larger value only makes the run slower.
SEND_AFTER_MS=1000
# How long the app holds back the first resolution a connection's announcing itself starts (the
# debug-only SIRIO_DEBUG_IDE_RESOLVE_AFTER_MS); when that timer fires it resolves EVERY ownerless
# connection, and nothing cancels it. The delay only makes a hook resolution unlikely between an
# ownerless client's connect and its request; the ownerless diagnostics cases assert none ran
# there from the state's hook_resolutions.
RESOLVE_AFTER_MS=3000
sleep_ms() { sleep "$(($1 / 1000)).$(printf '%03d' $(($1 % 1000)))"; }

is_a_host() {
  local comm
  comm="$(cat "/proc/$1/comm" 2>/dev/null || true)"
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
terminate() { # pid: TERM, then KILL after 5 s
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
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  [ -z "${PROXY_PID:-}" ] || kill "$PROXY_PID" 2>/dev/null || true
  [ -z "${OUTSIDE_PID:-}" ] || kill "$OUTSIDE_PID" 2>/dev/null || true
  [ -z "$CLIP_PID" ] || kill "$CLIP_PID" 2>/dev/null || true
  cp "$RUN_DIR"/app.log "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN_DIR"/socket.jsonl "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN_DIR"/client-*.jsonl "$OUT_DIR/" 2>/dev/null || true
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
echo "display: $DISPLAY_TARGET (state-only $STATE_ONLY)"
lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
if [ -n "$lvp" ]; then VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp"); fi

# A private world: HOME and every XDG directory under the run dir, stub agent CLIs first on
# PATH (so no mise wrapper runs), a host root and a TMPDIR of its own on disk.
export XDG_STATE_HOME="$RUN_DIR/xdg/state" XDG_DATA_HOME="$RUN_DIR/xdg/data"
export XDG_CONFIG_HOME="$RUN_DIR/xdg/config" XDG_CACHE_HOME="$RUN_DIR/xdg/cache"
export XDG_RUNTIME_DIR="$RUN_DIR/xdg/runtime"
mkdir -p "$RUN_DIR/home" "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" \
  "$XDG_RUNTIME_DIR" "$RUN_DIR/host" "$RUN_DIR/tmp"
chmod 700 "$XDG_RUNTIME_DIR"
mkdir -p "$RUN_DIR/stubs"
for cli in claude codex opencode pi omp; do
  printf '#!/bin/sh\necho "2.1.257 (Claude Code)"\n' >"$RUN_DIR/stubs/$cli"
  chmod +x "$RUN_DIR/stubs/$cli"
done
export PATH="$RUN_DIR/stubs:$PATH"
export HOME="$RUN_DIR/home"
export TMPDIR="$RUN_DIR/tmp"
# A pane's shell: no rc file of the user's matters, whatever their login shell is.
export SHELL=/bin/sh
export SIRIO_SOCKET="$RUN_DIR/c.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
export SIRIO_CREDENTIALS="$RUN_DIR/credentials.json"
export SIRIO_HOST_HOME="$RUN_DIR/host"
unset CLAUDE_CODE_SSE_PORT SIRIO_CONFIG_DIR
[ "${#SIRIO_SOCKET}" -lt 100 ] || fail "the socket path is too long for a unix socket: $SIRIO_SOCKET"
LOCK_DIR="$HOME/.claude/ide"

"$SEED" --database "$SIRIO_DB" --appearance dark

# The only language server the run can start: the fake one (Scripts/Tests/fake_lsp.py), named
# by a stub first on PATH and by languages.toml entries that replace the built-in ones for every
# extension the run opens (.py and .rs), so a pyright, pylsp or rust-analyzer on this machine is
# never looked for; the user's own languages.toml is never read (SIRIO_CONFIG_DIR is unset).
# The guarantee is that mapping (the first matching entry wins); `assert_only_the_fake_server`
# is a backstop at the end, a denylist over the live children of the app's last launch. It reports one
# warning on the text `hello` of a.py (line 1, columns 4 to 9, 0-based UTF-16).
mkdir -p "$XDG_CONFIG_HOME/sirio" "$RUN_DIR/fake-lsp"
cat >"$RUN_DIR/fake-lsp/answers.json" <<'JSON'
{
  "diagnostics_file": "a.py",
  "diagnostics": [{"needle": "hello", "severity": 2, "message": "fake warning: hello is unused"}]
}
JSON
cat >"$RUN_DIR/stubs/fake-lsp" <<STUB
#!/bin/sh
exec python3 -I "$ROOT/Scripts/Tests/fake_lsp.py" "\$@"
STUB
chmod +x "$RUN_DIR/stubs/fake-lsp"
cat >"$XDG_CONFIG_HOME/sirio/languages.toml" <<TOML
[[language]]
name = "python"
extensions = ["py"]
command = "fake-lsp"
args = ["$RUN_DIR/fake-lsp/answers.json", "$RUN_DIR/fake-lsp/requests.log"]

[[language]]
name = "rust"
extensions = ["rs"]
command = "fake-lsp"
args = ["$RUN_DIR/fake-lsp/answers.json", "$RUN_DIR/fake-lsp/requests.log"]
TOML

mkdir -p "$WT"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
printf 'fn main() {}\n' >"$WT/main.rs"
git -C "$WT" add main.rs
git -C "$WT" commit -q -m initial

# ---------------------------------------------------------------- helpers

# One control-socket call, logged to socket.jsonl, answering the result map as JSON (the
# state verb's own JSON is unwrapped from its `state` key).
cat >"$RUN_DIR/sockcall.py" <<'PY'
import json, os, socket, sys, time

method, params = sys.argv[1], dict(arg.split("=", 1) for arg in sys.argv[2:])
request = {"id": f"e2e-{os.getpid()}-{time.time_ns()}", "method": method, "params": params}
log_path = os.path.join(os.environ["RUN_DIR"], "socket.jsonl")


def write_line(entry):
    with open(log_path, "a", encoding="utf-8") as log:
        log.write(json.dumps(entry, sort_keys=True) + "\n")


# A connect error, an EOF before an answer, a timeout or an answer that is not JSON leaves a
# line with the request and an `error` and no `response`, then fails as it always did.
try:
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(20)
        sock.connect(os.environ["SIRIO_SOCKET"])
        sock.sendall((json.dumps(request) + "\n").encode())
        data = b""
        while not data.endswith(b"\n"):
            chunk = sock.recv(65536)
            if not chunk:
                break
            data += chunk
    if not data:
        raise EOFError("no answer")
    response = json.loads(data)
except (OSError, EOFError, ValueError) as error:
    write_line({"request": request, "error": type(error).__name__})
    raise
write_line({"request": request, "response": response})
if not response.get("ok"):
    sys.stderr.write(f"{method} failed: {response.get('error')}\n")
    sys.exit(1)
result = response.get("result") or {}
if "state" in result:
    print(result["state"])
else:
    print(json.dumps(result))
PY
export RUN_DIR
sockcall() { python3 -I "$RUN_DIR/sockcall.py" "$@"; }
# Several `surface.ide.switch` calls from one process, back to back, each on its own connection
# and logged like sockcall's: SWITCHES are on=true or on=false.
cat >"$RUN_DIR/sockburst.py" <<'BURST'
import json, os, socket, sys, time

with open(os.path.join(os.environ["RUN_DIR"], "socket.jsonl"), "a", encoding="utf-8") as log:
    for value in sys.argv[1:]:
        request = {"id": f"burst-{time.time_ns()}", "method": "surface.ide.switch",
                   "params": {"on": value}}
        try:
            with socket.socket(socket.AF_UNIX) as sock:
                sock.settimeout(20)
                sock.connect(os.environ["SIRIO_SOCKET"])
                sock.sendall((json.dumps(request) + "\n").encode())
                data = b""
                while not data.endswith(b"\n"):
                    chunk = sock.recv(65536)
                    if not chunk:
                        break
                    data += chunk
            if not data:
                raise EOFError("no answer")
            response = json.loads(data)
        except (OSError, EOFError, ValueError) as error:
            log.write(json.dumps({"via": "burst", "request": request, "error": type(error).__name__}, sort_keys=True) + "\n")
            raise
        log.write(json.dumps({"via": "burst", "request": request, "response": response}, sort_keys=True) + "\n")
        if not response.get("ok"):
            sys.exit(f"switch {value} failed: {response.get('error')}")
BURST
sockburst() { python3 -I "$RUN_DIR/sockburst.py" "$@"; }
# N `surface.editor.select` calls 25 ms apart, each on a connection of its own opened beforehand.
# Sent without a gap they would be one pass of the app's control poll and one change of the
# editor's selection (gpui merges the notifies of a pass), so they are spread over several
# passes, each a change the app notes. The run's send window (SEND_AFTER_MS) is longer than the
# burst, so the case's bound (one notification per window the burst spans, plus one) is one or
# two, which a Sirio that sent each change at once (about one notification per pass) exceeds;
# the 100 ms window of a release build is proved by the unit tests of sirio_ide::coalesce.
# Logged like sockcall's, in request order; the ranges are the arguments, the elapsed
# milliseconds from the first send to the last answer are printed.
cat >"$RUN_DIR/selburst.py" <<'SELBURST'
import json, os, socket, sys, time

log_path = os.path.join(os.environ["RUN_DIR"], "socket.jsonl")
requests = [{"id": f"selburst-{i}-{time.time_ns()}", "method": "surface.editor.select",
             "params": {"ranges": ranges}} for i, ranges in enumerate(sys.argv[1:])]


def write_line(entry):
    with open(log_path, "a", encoding="utf-8") as log:
        log.write(json.dumps(entry, sort_keys=True) + "\n")


socks = []
try:
    for _ in requests:
        sock = socket.socket(socket.AF_UNIX)
        sock.settimeout(20)
        sock.connect(os.environ["SIRIO_SOCKET"])
        socks.append(sock)
    started = time.monotonic()
    for sock, request in zip(socks, requests):
        sock.sendall((json.dumps(request) + "\n").encode())
        time.sleep(0.025)
    answers = []
    for sock in socks:
        data = b""
        while not data.endswith(b"\n"):
            chunk = sock.recv(65536)
            if not chunk:
                break
            data += chunk
        if not data:
            raise EOFError("no answer")
        answers.append(json.loads(data))
    elapsed = int((time.monotonic() - started) * 1000)
except (OSError, EOFError, ValueError) as error:
    for request in requests:
        write_line({"via": "selburst", "request": request, "error": type(error).__name__})
    raise
finally:
    for sock in socks:
        sock.close()
failed = None
for request, response in zip(requests, answers):
    write_line({"via": "selburst", "request": request, "response": response})
    if not response.get("ok") and failed is None:
        failed = f"select {request['params']['ranges']} failed: {response.get('error')}"
if failed:
    sys.exit(failed)
print(elapsed)
SELBURST
# One `surface.editor.select`, then one `surface.tabs.select` sent only once the select is
# answered, each on a connection of its own opened beforehand. The select is answered from
# inside the pass that runs it and its selection is noted at the end of that same pass, so the
# tab change, queued after the answer, runs in a later pass: after that note, never before it.
# Times are milliseconds from just before the select is sent, read when each answer has
# arrived (each request blocks on its own answer before anything else is sent). Logged like
# sockcall's, with `sent_ms` and `answered_ms`; a request left without an answer (a connect
# error, an EOF, a timeout, an answer that is not JSON) is a line with an `error`. Prints, in
# ms: select answered, tab change sent, tab change answered.
cat >"$RUN_DIR/selfocus.py" <<'SELFOCUS'
import json, os, socket, sys, time

ranges, tab = sys.argv[1], sys.argv[2]
log_path = os.path.join(os.environ["RUN_DIR"], "socket.jsonl")
requests = [
    {"id": f"selfocus-0-{time.time_ns()}", "method": "surface.editor.select", "params": {"ranges": ranges}},
    {"id": f"selfocus-1-{time.time_ns()}", "method": "surface.tabs.select", "params": {"index": tab}},
]


def write_line(entry):
    with open(log_path, "a", encoding="utf-8") as log:
        log.write(json.dumps(dict(entry, via="selfocus"), sort_keys=True) + "\n")


socks, sent, answered, answers = [], [], [], []
try:
    for _ in requests:
        sock = socket.socket(socket.AF_UNIX)
        sock.settimeout(20)
        sock.connect(os.environ["SIRIO_SOCKET"])
        socks.append(sock)
    start = time.monotonic()
    for sock, request in zip(socks, requests):
        sent.append(int((time.monotonic() - start) * 1000))
        sock.sendall((json.dumps(request) + "\n").encode())
        data = b""
        while not data.endswith(b"\n"):
            chunk = sock.recv(65536)
            if not chunk:
                break
            data += chunk
        if not data:
            raise EOFError("no answer")
        answered.append(int((time.monotonic() - start) * 1000))
        answers.append(json.loads(data))
        if not answers[-1].get("ok"):
            break
except (OSError, EOFError, ValueError) as error:
    for index, request in enumerate(requests):
        if index < len(answers):
            write_line({"request": request, "response": answers[index],
                        "sent_ms": sent[index], "answered_ms": answered[index]})
        else:
            write_line({"request": request, "error": type(error).__name__})
    raise
finally:
    for sock in socks:
        sock.close()
for request, response, at, back in zip(requests, answers, sent, answered):
    write_line({"request": request, "response": response, "sent_ms": at, "answered_ms": back})
    if not response.get("ok"):
        sys.exit(f"{request['method']} failed: {response.get('error')}")
print(answered[0], sent[1], answered[1])
SELFOCUS
selfocus() { python3 -I "$RUN_DIR/selfocus.py" "$@"; }
selburst() { python3 -I "$RUN_DIR/selburst.py" "$@"; }
# Every sirioctl call goes through a recording proxy: it listens on its own socket, forwards
# each request line to the app's socket and the answer back, and appends both, complete, to
# socket.jsonl. The `system.ping` route probe sirioctl sends before a `panel.*` call is forwarded
# and not recorded.
cat >"$RUN_DIR/proxy.py" <<'PROXY'
import json, os, socket, socketserver, sys, threading

listen, upstream, log_path = sys.argv[1:4]
log_lock = threading.Lock()


def record(entry):
    with log_lock:
        with open(log_path, "a", encoding="utf-8") as log:
            log.write(json.dumps(entry, sort_keys=True) + "\n")


class Handler(socketserver.StreamRequestHandler):
    def handle(self):
        up = None
        try:
            for line in self.rfile:
                try:
                    request = json.loads(line)
                except ValueError:
                    request = line.decode("utf-8", "replace")
                entry = {"via": "sirioctl", "request": request}
                try:
                    if up is None:
                        up = socket.socket(socket.AF_UNIX)
                        up.connect(upstream)
                        up_file = up.makefile("rwb")
                    up_file.write(line)
                    up_file.flush()
                    answer = up_file.readline()
                    if not answer:
                        raise OSError("the app closed the connection")
                    entry["response"] = json.loads(answer)
                    self.wfile.write(answer)
                    self.wfile.flush()
                except (OSError, ValueError) as error:
                    entry["error"] = f"{type(error).__name__}"
                    if not (isinstance(request, dict) and request.get("method") == "system.ping"):
                        record(entry)
                    return
                if not (isinstance(request, dict) and request.get("method") == "system.ping"):
                    record(entry)
        finally:
            if up is not None:
                up.close()


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


with Server(listen, Handler) as server:
    server.serve_forever()
PROXY
PROXY_SOCKET="$RUN_DIR/p.sock"
python3 -I "$RUN_DIR/proxy.py" "$PROXY_SOCKET" "$SIRIO_SOCKET" "$RUN_DIR/socket.jsonl" &
PROXY_PID=$!
for _ in $(seq 1 50); do [ -S "$PROXY_SOCKET" ] && break; sleep 0.1; done
[ -S "$PROXY_SOCKET" ] || fail "the recording proxy did not start"
printf '#!/bin/sh\nSIRIO_SOCKET="%s" exec "%s" "$@"\n' "$PROXY_SOCKET" "$CTL" >"$RUN_DIR/ctl-proxied"
chmod +x "$RUN_DIR/ctl-proxied"
CTL="$RUN_DIR/ctl-proxied"
ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
say() { echo "--- $*"; }

# A value out of JSON on stdin: a Python expression over `d`.
jq_py() { python3 -I -c 'import json, sys; d = json.load(sys.stdin); print(eval(sys.argv[1]))' "$1"; }
ide_state() { sockcall surface.ide.state; }
state_expr() { ide_state | jq_py "$1"; }

# Polls `surface.settings.read` until it reports claudeIdeBridge=WANT (the Settings switch).
expect_settings_switch() { # want
  local tries=0 got=""
  "$CTL" surface settings open >/dev/null 2>&1 || true
  while [ "$tries" -lt 50 ]; do
    got="$(sockcall surface.settings.read 2>/dev/null | jq_py 'd.get("claudeIdeBridge")' 2>/dev/null || true)"
    [ "$got" = "$1" ] && { echo "OK: surface.settings.read: claudeIdeBridge=$1"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: surface.settings.read never reported claudeIdeBridge=$1 (last: ${got:-none})"
}

# Polls until `state_expr EXPR` prints WANT.
expect_state() { # label expr want
  local got="" tries=0
  while [ "$tries" -lt 100 ]; do
    got="$(state_expr "$2" 2>/dev/null || true)"
    if [ "$got" = "$3" ]; then
      echo "OK: $1 ($2 = $3)"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: $1: state $2 expected $3, got $got (state: $(ide_state 2>&1 | cut -c1-600))"
}

app_start() { # [NAME=value...]: extra variables for the app
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 \
    SIRIO_DEBUG_IDE_START_DELAY_MS=600 SIRIO_DEBUG_IDE_SEND_AFTER_MS="$SEND_AFTER_MS" \
    SIRIO_DEBUG_IDE_RESOLVE_AFTER_MS="$RESOLVE_AFTER_MS" \
    ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} "$@" "$BIN" >>"$RUN_DIR/app.log" 2>&1) &
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
app_quit() {
  "$CTL" quit >/dev/null 2>&1 || true
  local tries=0
  while kill -0 "$APP_PID" 2>/dev/null && [ "$tries" -lt 200 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  kill -0 "$APP_PID" 2>/dev/null && fail "sirio did not quit after sirioctl quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}
select_repo() {
  ctl project add "$WT" >/dev/null 2>&1 || true
  local tries=0
  while [ "$tries" -lt 40 ]; do
    "$CTL" select-workspace --workspace "$WT" >/dev/null 2>&1 && break
    tries=$((tries + 1))
    sleep 0.5
  done
  case "$("$CTL" current-workspace --json 2>&1 || true)" in *"$WT"*) ;; *) fail "the repository is not the selected workspace" ;; esac
}
# The id of the worktree whose path is $1.
worktree_id() { # path
  "$CTL" list-workspaces --json | python3 -I -c '
import json, sys
for row in json.load(sys.stdin):
    if row["path"] == sys.argv[1]:
        print(row["id"])
        break' "$1"
}
project_id() { # path of one of its worktrees
  "$CTL" list-workspaces --json | python3 -I -c '
import json, sys
for row in json.load(sys.stdin):
    if row["path"] == sys.argv[1]:
        print(row["project"])
        break' "$1"
}

lock_file() { ls "$LOCK_DIR"/*.lock 2>/dev/null | head -n 1; }
lock_field() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
# Waits for a lockfile and prints its path.
wait_lockfile() {
  local tries=0 file=""
  while [ "$tries" -lt 100 ]; do
    file="$(lock_file || true)"
    [ -n "$file" ] && { printf '%s' "$file"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: no lockfile appeared in $LOCK_DIR"
}
wait_no_lockfile() {
  local tries=0
  while [ "$tries" -lt 100 ]; do
    [ -z "$(lock_file || true)" ] && return 0
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: the lockfile is still there: $(lock_file)"
}

# A pane in worktree $1 that writes the port it was given to a file, then answers it. A pane
# with none (or a blank one) answers "-".
pane_port() { # worktree-id
  local out="$RUN_DIR/port-$RANDOM.txt" id tries=0 got
  id="$("$CTL" panel create --worktree "$1" --cmd "printf '%s' \"\${CLAUDE_CODE_SSE_PORT-}\" > $out; sleep 300")"
  [ -n "$id" ] || fail "$CASE: panel create named no pane"
  while [ ! -f "$out" ] && [ "$tries" -lt 100 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ -f "$out" ] || fail "$CASE: the pane $id never wrote its environment"
  got="$(cat "$out")"
  "$CTL" panel close "$id" >/dev/null 2>&1 || true
  printf '%s' "${got:--}"
}

# The client in a pane of worktree $1, driven through a FIFO; its transcript is
# $RUN_DIR/client-N.jsonl. Sets CLIENT_FIFO, CLIENT_LOG and CLIENT_PANE.
start_client() { # worktree-id
  CLIENT_N=$((CLIENT_N + 1))
  CLIENT_FIFO="$RUN_DIR/client-$CLIENT_N.fifo"
  CLIENT_LOG="$RUN_DIR/client-$CLIENT_N.jsonl"
  rm -f "$CLIENT_FIFO" "$CLIENT_LOG"
  mkfifo "$CLIENT_FIFO"
  CLIENT_PANE="$("$CTL" panel create --worktree "$1")"
  [ -n "$CLIENT_PANE" ] || fail "$CASE: panel create named no pane"
  local tries=0
  while ! "$CTL" panel state "$CLIENT_PANE" >/dev/null 2>&1 && [ "$tries" -lt 100 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  "$CTL" panel write "$CLIENT_PANE" --enter --input "$(command -v python3) $CLIENT --fifo $CLIENT_FIFO --transcript $CLIENT_LOG" >/dev/null
  tries=0
  while ! grep -q '"event": "ready"' "$CLIENT_LOG" 2>/dev/null && [ "$tries" -lt 150 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  grep -q '"event": "ready"' "$CLIENT_LOG" 2>/dev/null || fail "$CASE: the client never started in pane $CLIENT_PANE (pane: $("$CTL" panel read "$CLIENT_PANE" 2>&1 | head -c 600))"
}
# The client in an application terminal tab of the selected worktree (`surface.terminal.open`:
# a real pane Sirio holds a view of, which `panel create`'s host-only session is not), run by
# `sh -c` as the shell's child. Sets CLIENT_FIFO, CLIENT_LOG, CLIENT_PANE and CLIENT_TAB.
start_app_client() { # [gate file: the client starts when it exists]
  CLIENT_N=$((CLIENT_N + 1))
  CLIENT_FIFO="$RUN_DIR/client-$CLIENT_N.fifo"
  CLIENT_LOG="$RUN_DIR/client-$CLIENT_N.jsonl"
  rm -f "$CLIENT_FIFO" "$CLIENT_LOG"
  mkfifo "$CLIENT_FIFO"
  local wait_for=""
  [ -z "${1:-}" ] || wait_for="while [ ! -e $1 ]; do sleep 0.1; done; "
  CLIENT_PANE="$(sockcall surface.terminal.open "cmd=${wait_for}$(command -v python3) $CLIENT --fifo $CLIENT_FIFO --transcript $CLIENT_LOG; :" | jq_py 'd["pane"]')"
  [ -n "$CLIENT_PANE" ] || fail "$CASE: surface.terminal.open named no pane"
  CLIENT_TAB="$(tabs_field count)"
  [ -z "${1:-}" ] || return 0
  wait_client_ready
}
wait_client_ready() {
  local tries=0
  while ! grep -q '"event": "ready"' "$CLIENT_LOG" 2>/dev/null && [ "$tries" -lt 150 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  grep -q '"event": "ready"' "$CLIENT_LOG" 2>/dev/null || fail "$CASE: the client never started in $CLIENT_PANE (tab $CLIENT_TAB)"
}
end_app_client() {
  client_do quit
  sockcall surface.tabs.close "index=$CLIENT_TAB" confirm=yes >/dev/null
}
# Closes every tab of a kind (file, chat, terminal): their clients end with them.
close_tabs_of_kind() { # kind
  local index
  while index="$(tab_index "$1")" && [ "$index" != 0 ]; do
    sockcall surface.tabs.close "index=$index" confirm=yes >/dev/null
  done
}
client_do() { printf '%s\n' "$*" >"$CLIENT_FIFO"; }
# How many transcript records match a Python expression over `r`.
client_count() { # expr
  python3 -I - "$CLIENT_LOG" "$1" <<'PY'
import json, sys
n = 0
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    if eval(sys.argv[2]):
        n += 1
print(n)
PY
}
wait_client() { # label expr count
  local tries=0 got=""
  while [ "$tries" -lt 100 ]; do
    got="$(client_count "$2")"
    [ "$got" = "$3" ] && { echo "OK: client $1 ($3)"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: client $1: expected $3 records of $2, got $got ($(tail -n 5 "$CLIENT_LOG" | cut -c1-300))"
}
end_client() {
  client_do quit
  "$CTL" panel close "$CLIENT_PANE" >/dev/null 2>&1 || true
}

# The params of every notification METHOD the client received, one JSON object per line.
client_notes() { # method
  python3 -I - "$CLIENT_LOG" "$1" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    frame = r.get("frame", {})
    if r.get("dir") == "in" and frame.get("method") == sys.argv[2]:
        print(json.dumps(frame["params"], sort_keys=True))
PY
}
note_count() { client_notes "$1" | wc -l | tr -d ' '; }
# Polls until the client holds COUNT notifications of METHOD.
wait_notes() { # method count
  local tries=0 got=""
  while [ "$tries" -lt 100 ]; do
    got="$(note_count "$1")"
    [ "$got" = "$2" ] && return 0
    tries=$((tries + 1))
    sleep 0.1
  done
  fail "$CASE: the client holds $got $1 notifications, expected $2 (last: $(client_notes "$1" | tail -n 2 | cut -c1-300))"
}
# The position of the last tab of a kind (file, chat, terminal).
tab_index() { # kind
  "$CTL" surface tabs read --json | python3 -I -c '
import json, sys
d = json.load(sys.stdin)[0]
print(max([int(k[4:]) for k, v in d.items() if k.startswith("tab.") and v.startswith(sys.argv[1] + "|")] or [0]))' "$1"
}
file_tab_index() { tab_index file; }
tabs_field() { "$CTL" surface tabs read --json | jq_py "d[0][\"$1\"]"; }
# The file the selection cases use: line 3 is `let s = "<emoji>x";` (the x is column 11 in UTF-16
# units, 13 bytes in). Prints the byte offsets: x, start of line 5, end of line 5, a point inside
# line 6, end of line 7.
write_sel_file() {
  python3 -I - "$1" <<'PY'
import sys
lines = ["fn main() {", "// keep", 'let s = "\U0001F600x";', "// four", "five();", "six();", "seven();", "}"]
text = "\n".join(lines) + "\n"
open(sys.argv[1], "w", encoding="utf-8").write(text)
data = text.encode("utf-8")
starts = [0]
for i, byte in enumerate(data):
    if byte == 0x0A:
        starts.append(i + 1)
x = starts[2] + len('let s = "'.encode()) + len("\U0001F600".encode())
assert data[x:x + 1] == b"x"
print(x, starts[4], starts[4] + len("five();"), starts[5] + 2, starts[6] + len("seven();"))
PY
}
# Selects RANGES on the active file tab, waiting for a file that is still loading.
select_ranges() { # ranges
  local tries=0
  while ! sockcall surface.editor.select "ranges=$1" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -lt 50 ] || fail "$CASE: surface.editor.select $1 never succeeded: $(sockcall surface.editor.select "ranges=$1" 2>&1 | tail -n 1)"
    sleep 0.2
  done
}

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
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window colours
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window with _NET_WM_PID=$APP_PID for $1"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png"
  [ -s "$OUT_DIR/frames/$1.png" ] || fail "$CASE: $1.png is empty"
  colours="$(identify -format '%k' "$OUT_DIR/frames/$1.png")"
  [ "$colours" -ge 200 ] || fail "$CASE: $1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}

# ---------------------------------------------------------------- the cases

case_run() { # name function
  CASE="$1"
  say "case $1"
  "$2"
  CASES_PASSED=$((CASES_PASSED + 1))
  echo "PASS $1"
}

WT_ID=""
PORT=""
TOKEN=""

case_lockfile_and_env() {
  local file mode dir_mode keys port folders
  file="$(wait_lockfile)"
  port="$(basename "$file" .lock)"
  case "$port" in ''|*[!0-9]*) fail "$CASE: the lockfile is not named by a port: $file" ;; esac
  keys="$(python3 -I -c 'import json, sys; print(",".join(sorted(json.load(open(sys.argv[1])))))' "$file")"
  [ "$keys" = "authToken,ideName,pid,runningInWindows,transport,workspaceFolders" ] \
    || fail "$CASE: the lockfile's keys are $keys"
  mode="$(stat -c %a "$file")"
  [ "$mode" = 600 ] || fail "$CASE: the lockfile's mode is $mode, not 600"
  dir_mode="$(stat -c %a "$LOCK_DIR")"
  [ "$dir_mode" = 700 ] || fail "$CASE: the lockfile directory's mode is $dir_mode, not 700"
  [ "$(lock_field "$file" ideName)" = "Sirio (debug)" ] || fail "$CASE: ideName is $(lock_field "$file" ideName)"
  [ "$(lock_field "$file" pid)" = "$APP_PID" ] || fail "$CASE: pid is $(lock_field "$file" pid), the app is $APP_PID"
  [ "$(lock_field "$file" transport)" = ws ] || fail "$CASE: transport is $(lock_field "$file" transport)"
  [ "$(lock_field "$file" runningInWindows)" = False ] || fail "$CASE: runningInWindows is $(lock_field "$file" runningInWindows)"
  folders="$(python3 -I -c 'import json, sys; print("\n".join(json.load(open(sys.argv[1]))["workspaceFolders"]))' "$file")"
  printf '%s\n' "$folders" | grep -Fxq "$WT" || fail "$CASE: workspaceFolders ($folders) lack $WT"
  PORT="$port"
  TOKEN="$(lock_field "$file" authToken)"
  [ "${#TOKEN}" = 32 ] || fail "$CASE: the token is ${#TOKEN} characters, not 32 hex"
  python3 -I - "$file" "$OUT_DIR/lockfile.redacted.json" <<'PY'
import json, sys
body = json.load(open(sys.argv[1]))
body["authToken"] = "<redacted>"
json.dump(body, open(sys.argv[2], "w"), indent=2, sort_keys=True)
PY
  echo "OK: lockfile $file: six keys, 0600 in a 0700 directory, Sirio (debug), pid $APP_PID, $WT"
  expect_state "the state's port is the lockfile's" 'd["port"]' "$port"
  expect_state "the state is enabled" 'd["enabled"]' True
  expect_state "the state names the lockfile" 'd["lockfile"]' "$port.lock"
  [ "$(ide_state | jq_py '"authToken" in json.dumps(d) or "'"$TOKEN"'" in json.dumps(d)')" = False ] \
    || fail "$CASE: the state holds the token"
  [ "$(pane_port "$WT_ID")" = "$port" ] || fail "$CASE: a new pane's CLAUDE_CODE_SSE_PORT is not $port"
  echo "OK: a new pane's environment holds the port $port"
}

case_token_refused() {
  start_client "$WT_ID"
  client_do "token wrong"
  wait_client "refused with 401" 'r.get("event") == "refused" and r.get("status") == 401' 1
  expect_state "no connection after a refused handshake" 'len([c for c in d["connections"]])' 0
  end_client
}

case_handshake() {
  start_client "$WT_ID"
  wait_client "connected, the server echoes mcp" 'r.get("event") == "connected" and r.get("subprotocol") == "mcp"' 1
  client_do "call tools/list {}"
  wait_client "tools/list answered" 'r.get("dir") == "in" and r.get("frame", {}).get("id") == "call-1"' 1
  local tools
  tools="$(python3 -I - "$CLIENT_LOG" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    if r.get("dir") == "in" and r.get("frame", {}).get("id") == "call-1":
        print(",".join(sorted(t["name"] for t in r["frame"]["result"]["tools"])))
PY
)"
  [ "$tools" = "close_tab,closeAllDiffTabs,getDiagnostics,openDiff" ] || [ "$tools" = "closeAllDiffTabs,close_tab,getDiagnostics,openDiff" ] \
    || fail "$CASE: tools/list is $tools"
  echo "OK: tools/list names exactly $tools"
  local pid
  pid="$(python3 -I - "$CLIENT_LOG" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    if r.get("dir") == "out" and r.get("frame", {}).get("method") == "ide_connected":
        print(r["frame"]["params"]["pid"])
PY
)"
  expect_state "one connection, the client's pid" '[c["pid"] for c in d["connections"]]' "[$pid]"
  # The held client stays for the next case.
  HELD_CLIENT_FIFO="$CLIENT_FIFO"
  HELD_CLIENT_LOG="$CLIENT_LOG"
  HELD_CLIENT_PANE="$CLIENT_PANE"
}

case_switch_off() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  sockcall surface.ide.switch on=false >/dev/null
  wait_no_lockfile
  echo "OK: the lockfile is gone"
  wait_client "the connection closed" 'r.get("event") == "closed"' 1
  expect_state "the state says off" 'd["enabled"]' False
  expect_settings_switch false
  expect_state "no port while off" 'd["port"]' None
  expect_state "no connections while off" 'len(d["connections"])' 0
  [ "$(pane_port "$WT_ID")" = "-" ] || fail "$CASE: a pane started with the switch off has a port"
  echo "OK: a pane started with the switch off has none"
  sockcall surface.ide.switch on=true >/dev/null
  local file
  file="$(wait_lockfile)"
  [ "$(basename "$file" .lock)" = "$PORT" ] || fail "$CASE: the port changed from $PORT to $(basename "$file" .lock)"
  [ "$(lock_field "$file" authToken)" = "$TOKEN" ] || fail "$CASE: the token changed with the switch"
  expect_state "back on, the same port" 'd["port"]' "$PORT"
  expect_settings_switch true
  [ "$(pane_port "$WT_ID")" = "$PORT" ] || fail "$CASE: a pane started after the switch came back lacks the port"
  echo "OK: on again: port $PORT and the token are the same, panes get the port"
  end_client
}

case_switch_flap() {
  local file count
  sockburst false true false true
  file="$(wait_lockfile)"
  expect_state "the flapped switch is on" 'd["enabled"]' True
  expect_state "the flapped bridge has a port" 'd["port"] is not None' True
  [ "$(basename "$file" .lock)" = "$PORT" ] || fail "$CASE: the port moved from $PORT to $(basename "$file" .lock)"
  expect_state "the state's port is still the original" 'd["port"]' "$PORT"
  sleep 1
  count="$(ls "$LOCK_DIR"/*.lock | wc -l)"
  [ "$count" = 1 ] || fail "$CASE: $count lockfiles exist: $(ls "$LOCK_DIR")"
  [ "$(lock_field "$(lock_file)" authToken)" = "$TOKEN" ] || fail "$CASE: the token changed"
  echo "OK: after off, on, off, on: port $PORT, one lockfile"
}

case_port_survives_catalog_change() {
  local project before after second second_id
  project="$(project_id "$WT")"
  [ -n "$project" ] || fail "$CASE: no project for $WT"
  before="$(lock_field "$(wait_lockfile)" workspaceFolders)"
  ctl new-workspace --project "$project" --branch ide-second >/dev/null
  local tries=0 file
  file="$(wait_lockfile)"
  while [ "$tries" -lt 100 ]; do
    after="$(lock_field "$file" workspaceFolders)"
    [ "$after" != "$before" ] && break
    tries=$((tries + 1))
    sleep 0.2
  done
  [ "$after" != "$before" ] || fail "$CASE: the lockfile's workspaceFolders never changed: $after"
  second="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-second"]
print(paths[0] if paths else "")')"
  [ -n "$second" ] || fail "$CASE: sirioctl does not list the new worktree"
  case "$after" in *"$second"*) ;; *) fail "$CASE: workspaceFolders ($after) lack $second" ;; esac
  echo "OK: workspaceFolders now names $second"
  [ "$(basename "$(wait_lockfile)" .lock)" = "$PORT" ] || fail "$CASE: the port changed with the catalog"
  second_id="$(worktree_id "$second")"
  # A pane can start only in a worktree that is mounted; selecting it mounts it.
  ctl select-workspace --workspace "$second_id" >/dev/null
  [ "$(pane_port "$second_id")" = "$PORT" ] || fail "$CASE: a pane in the new worktree lacks the port $PORT"
  echo "OK: a pane in the new worktree still gets $PORT"
}

case_restart_keeps_port() {
  start_client "$WT_ID"
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "one connection" 'len(d["connections"])' 1
  app_quit
  wait_no_lockfile
  echo "OK: quitting removed the lockfile"
  wait_client "the old connection closed" 'r.get("event") == "closed"' 1
  app_start
  select_repo
  local file
  file="$(wait_lockfile)"
  [ "$(basename "$file" .lock)" = "$PORT" ] || fail "$CASE: the port changed across a restart: $PORT then $(basename "$file" .lock)"
  [ "$(lock_field "$file" authToken)" = "$TOKEN" ] || fail "$CASE: the token changed across a restart"
  expect_state "the relaunched state's port" 'd["port"]' "$PORT"
  WT_ID="$(worktree_id "$WT")"
  client_do "token $TOKEN"
  wait_client "reconnected with the same token" 'r.get("event") == "connected"' 2
  expect_state "the reconnected client is a connection" 'len(d["connections"])' 1
  end_client
}

case_inherited_port() {
  app_quit
  app_start CLAUDE_CODE_SSE_PORT=1
  select_repo
  WT_ID="$(worktree_id "$WT")"
  wait_lockfile >/dev/null
  expect_state "the inherited port is not Sirio's" 'd["port"]' "$PORT"
  [ "$(pane_port "$WT_ID")" = "$PORT" ] || fail "$CASE: a pane got the launch's port, not Sirio's $PORT"
  echo "OK: a pane gets Sirio's $PORT, not the launch's 1"
  sockcall surface.ide.switch on=false >/dev/null
  wait_no_lockfile
  [ "$(pane_port "$WT_ID")" = "-" ] || fail "$CASE: with the switch off a pane kept the launch's port"
  echo "OK: with the switch off a pane has no port"
  sockcall surface.ide.switch on=true >/dev/null
  capture ide-final
}

SEL_FILE="$WT/sel.rs"
SEL_X=""; SEL_L5S=""; SEL_L5E=""; SEL_L6=""; SEL_L7E=""
FILE_TAB=""

case_owner_resolved() {
  # The switch was turned back on by the last case: a terminal made now needs the port.
  wait_lockfile >/dev/null
  expect_state "the bridge is back on its port" 'd["port"]' "$PORT"
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "one connection" 'len(d["connections"])' 1
  expect_state "the connection is owned by the pane the client runs in" 'd["connections"][0]["owner_pane"]' "$CLIENT_PANE"
  expect_state "the owner's worktree is the repository" 'd["connections"][0]["worktree"]' "$WT"
  HELD_CLIENT_FIFO="$CLIENT_FIFO"
  HELD_CLIENT_LOG="$CLIENT_LOG"
  HELD_CLIENT_PANE="$CLIENT_PANE"
}

case_selection_utf16() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  read -r SEL_X SEL_L5S SEL_L5E SEL_L6 SEL_L7E < <(write_sel_file "$SEL_FILE")
  local before
  before="$(note_count selection_changed)"
  ctl surface editor open --path "$SEL_FILE" >/dev/null
  FILE_TAB="$(tabs_field active)"
  # Opening a file focuses its tab, and focusing a file tab says its selection (spec 4.4).
  wait_notes selection_changed "$((before + 1))"
  sleep_ms "$((SEND_AFTER_MS + 500))"
  [ "$(note_count selection_changed)" = "$((before + 1))" ] || fail "$CASE: opening the file said its selection more than once"
  echo "OK: opening the file sent its selection once"
  before="$(note_count selection_changed)"
  select_ranges "$SEL_X..$((SEL_X + 1))"
  wait_notes selection_changed "$((before + 1))"
  local got
  got="$(client_notes selection_changed | tail -n 1)"
  python3 -I - "$got" "$SEL_FILE" <<'PY' || fail "$CASE: the notification is not the selected x: $got"
import json, sys
n = json.loads(sys.argv[1])
s = n["selection"]
ok = (n["text"] == "x" and n["filePath"] == sys.argv[2] and n["fileUrl"] == "file://" + sys.argv[2]
      and s["start"] == {"line": 2, "character": 11} and s["end"] == {"line": 2, "character": 12}
      and s["isEmpty"] is False)
sys.exit(0 if ok else 1)
PY
  echo "OK: selection_changed: start {2,11}, end {2,12}, isEmpty false, text x"
}

case_selection_coalesced() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  local before elapsed ranges="" i got bound
  before="$(note_count selection_changed)"
  for i in $(seq 1 19); do ranges="$ranges 0..$i"; done
  # shellcheck disable=SC2086
  elapsed="$(selburst $ranges "$SEL_L5S..$SEL_L5E")"
  # At most one notification per window of the run's send delay that the burst spans, plus one.
  # The last selection is noted at the end of the pass that answered it, which can be a moment
  # after its answer is read: 100 ms of slack covers that.
  bound=$(((elapsed + 100) / SEND_AFTER_MS + 1))
  echo "the burst of 20 selections took ${elapsed} ms: at most $bound notifications"
  local tries=0
  while [ "$tries" -lt 100 ]; do
    got="$(client_notes selection_changed | tail -n 1)"
    case "$got" in *'"text": "five();"'*|*'"text":"five();"'*) break ;; esac
    tries=$((tries + 1))
    sleep 0.1
  done
  python3 -I - "$got" <<'PY' || fail "$CASE: the last notification is not the final selection: $got"
import json, sys
n = json.loads(sys.argv[1])
s = n["selection"]
sys.exit(0 if (n["text"] == "five();" and s["start"] == {"line": 4, "character": 0}
               and s["end"] == {"line": 4, "character": 7}) else 1)
PY
  sleep_ms "$((SEND_AFTER_MS + 500))"
  got="$(($(note_count selection_changed) - before))"
  [ "$got" -ge 1 ] && [ "$got" -le "$bound" ] || fail "$CASE: $got notifications for a ${elapsed} ms burst, expected 1 to $bound"
  echo "OK: $got notifications for the burst (bound $bound), the last is the final selection"
}

case_focus_sends_selection() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  local before
  before="$(note_count selection_changed)"
  ctl surface tabs select "$CLIENT_TAB" >/dev/null
  sleep_ms "$((SEND_AFTER_MS + 600))"
  [ "$(note_count selection_changed)" = "$before" ] || fail "$CASE: focusing a terminal tab sent a selection"
  echo "OK: focusing a terminal tab sends nothing"
  ctl surface tabs select "$FILE_TAB" >/dev/null
  wait_notes selection_changed "$((before + 1))"
  sleep_ms "$((SEND_AFTER_MS + 600))"
  [ "$(note_count selection_changed)" = "$((before + 1))" ] || fail "$CASE: focusing the file tab sent more than one notification"
  case "$(client_notes selection_changed | tail -n 1)" in *five*) ;; *) fail "$CASE: the focus notification is not the current selection" ;; esac
  echo "OK: focusing the file tab sends exactly one notification, the current selection"
}

# Two Claude terminals: the first one's pane is focused while its client has not started yet (so
# the focus is recorded before the pane is eligible), the second is created after it (active,
# never focused by a click), the file is opened (so neither terminal is focused when its client
# connects) and both clients start; then lines 5 to 7 are selected and sent: the first terminal gets the at-mention (the one used
# last), the second does not, though it connected later.
case_at_mention() {
  CLIENT_FIFO="$HELD_CLIENT_FIFO" CLIENT_LOG="$HELD_CLIENT_LOG" CLIENT_PANE="$HELD_CLIENT_PANE"
  close_tabs_of_kind file
  end_app_client
  local gate_a="$RUN_DIR/gate-a" gate_b="$RUN_DIR/gate-b"
  rm -f "$gate_a" "$gate_b"
  start_app_client "$gate_a"
  A_FIFO="$CLIENT_FIFO" A_LOG="$CLIENT_LOG" A_PANE="$CLIENT_PANE" A_TAB="$CLIENT_TAB"
  ctl surface tabs select "$A_TAB" >/dev/null
  start_app_client "$gate_b"
  B_FIFO="$CLIENT_FIFO" B_LOG="$CLIENT_LOG" B_PANE="$CLIENT_PANE" B_TAB="$CLIENT_TAB"
  # The file becomes the active tab: neither terminal is focused when its client connects.
  ctl surface editor open --path "$SEL_FILE" >/dev/null
  FILE_TAB="$(tabs_field active)"
  touch "$gate_a"
  CLIENT_FIFO="$A_FIFO" CLIENT_LOG="$A_LOG" CLIENT_PANE="$A_PANE" CLIENT_TAB="$A_TAB"
  wait_client_ready
  wait_client "A connected" 'r.get("event") == "connected"' 1
  touch "$gate_b"
  CLIENT_FIFO="$B_FIFO" CLIENT_LOG="$B_LOG" CLIENT_PANE="$B_PANE" CLIENT_TAB="$B_TAB"
  wait_client_ready
  wait_client "B connected" 'r.get("event") == "connected"' 1
  expect_state "A and B are owned by their panes" 'sorted(c["owner_pane"] for c in d["connections"])' "$(python3 -I -c 'import sys; print(sorted(sys.argv[1:]))' "$A_PANE" "$B_PANE")"
  CLIENT_FIFO="$A_FIFO" CLIENT_LOG="$A_LOG" CLIENT_PANE="$A_PANE" CLIENT_TAB="$A_TAB"
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  wait_notes at_mentioned 1
  python3 -I - "$(client_notes at_mentioned | tail -n 1)" "$SEL_FILE" <<'PY' || fail "$CASE: the at_mentioned params are wrong: $(client_notes at_mentioned | tail -n 1)"
import json, sys
n = json.loads(sys.argv[1])
sys.exit(0 if (n == {"filePath": sys.argv[2], "lineStart": 4, "lineEnd": 6}) else 1)
PY
  echo "OK: the terminal used last got at_mentioned lineStart 4, lineEnd 6"
  [ "$(CLIENT_LOG="$B_LOG" note_count at_mentioned)" = 0 ] || fail "$CASE: the other terminal got the at-mention too"
  local tries=0
  while [ "$(tabs_field active)" != "$A_TAB" ] && [ "$tries" -lt 50 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ "$(tabs_field active)" = "$A_TAB" ] || fail "$CASE: the terminal's tab ($A_TAB) is not focused (active: $(tabs_field active))"
  echo "OK: the terminal's tab is focused"
  case "$("$CTL" panel read "$A_PANE" 2>&1)" in *sel.rs*) fail "$CASE: something was written into the pane" ;; esac
  echo "OK: nothing was written into the pane"
  capture ide-at-mention
}

# The file sits in the Secondary half, the Claude terminal of the Primary half is the active tab
# (as after an at-mention), and the user selects new lines in the file with the mouse and keys:
# the selection must reach Claude though the file's tab is not the active one.
case_selection_while_terminal_active() {
  CLIENT_FIFO="$A_FIFO" CLIENT_LOG="$A_LOG" CLIENT_PANE="$A_PANE" CLIENT_TAB="$A_TAB"
  [ "$(tabs_field active)" = "$A_TAB" ] || fail "$CASE: the terminal is not the active tab"
  local window geometry width height before
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window for pid $APP_PID"
  geometry="$(DISPLAY="$DISPLAY_TARGET" xdotool getwindowgeometry "$window" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')"
  read -r width height <<<"$geometry"
  before="$(note_count selection_changed)"
  # Line 3 of the file, in the middle of the Secondary half (measured on ide-at-mention.png).
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$((width * 62 / 100))" "$((height * 18 / 100))" click 1
  sleep 0.3
  DISPLAY="$DISPLAY_TARGET" xdotool key shift+Down
  [ "$(tabs_field active)" = "$A_TAB" ] || fail "$CASE: the click made the file's tab the active one; the layout under test is gone"
  local tries=0 got=""
  while [ "$tries" -lt 50 ]; do
    got="$(client_notes selection_changed | tail -n 1)"
    python3 -I - "$got" <<'PY' && break
import json, sys
try:
    s = json.loads(sys.argv[1])["selection"]
except (ValueError, KeyError, IndexError):
    sys.exit(1)
sys.exit(0 if (s["start"]["line"] == 2 and s["end"]["line"] == 3 and s["isEmpty"] is False) else 1)
PY
    tries=$((tries + 1))
    sleep 0.2
  done
  [ "$tries" -lt 50 ] || fail "$CASE: no selection_changed for the new lines reached the client (last: $got; $before before)"
  echo "OK: the selection of the visible, inactive file reached the client: $got"
}

# A terminal created after another one is the active tab and has had no click, so nothing
# recorded it: its client connecting while it is focused makes it the one used last.
case_at_mention_to_new_terminal() {
  start_app_client
  wait_client "C connected" 'r.get("event") == "connected"' 1
  expect_state "C is owned by its pane" '"'"$CLIENT_PANE"'" in [c["owner_pane"] for c in d["connections"]]' True
  ctl surface tabs select "$FILE_TAB" >/dev/null
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  wait_notes at_mentioned 1
  [ "$(CLIENT_LOG="$A_LOG" note_count at_mentioned)" = 1 ] || fail "$CASE: the earlier terminal got a second at-mention"
  echo "OK: the new terminal, focused when it connected, got the at-mention"
}

# A file of another worktree can sit in this worktree's strip (surface.editor.open, the Open File
# picker, a followed file take any path). The user selects in this worktree's file and, before
# that selection's send is due, selects the other tab: the connection owned here must still get
# the selection of the file it was waiting to send, not lose it to a file of another worktree.
# The window is this run's SEND_AFTER_MS, opened by the select's own change (nothing is pending
# when it is sent: the case waits out the window of the focus that precedes it). The tab change
# is sent once the select is answered, so the app runs it after the select's change, and
# whatever it marks it marks before it answers; the case fails unless it was answered less than
# SEND_AFTER_MS after the select was sent, which puts it inside the window.
case_selection_not_lost_to_another_worktree() {
  local second other_tab timing sel_answered tab_sent tab_answered before got tries=0
  second="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-second"]
print(paths[0] if paths else "")')"
  [ -n "$second" ] || fail "$CASE: sirioctl does not list the second worktree"
  write_sel_file "$second/other.rs" >/dev/null
  ctl surface editor open --path "$second/other.rs" >/dev/null
  other_tab="$(tabs_field active)"
  before="$(note_count selection_changed)"
  ctl surface tabs select "$FILE_TAB" >/dev/null
  # Focusing the file says its selection: wait for that send, then stay idle past the window, so
  # no send is pending for the connection when the select arrives.
  while [ "$(note_count selection_changed)" -le "$before" ] && [ "$tries" -lt 100 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ "$(note_count selection_changed)" -gt "$before" ] || fail "$CASE: focusing this worktree's file sent no selection"
  sleep_ms "$((SEND_AFTER_MS + 500))"
  before="$(note_count selection_changed)"
  # "six();", line 6 of this worktree's sel.rs.
  timing="$(selfocus "$((SEL_L5E + 1))..$((SEL_L5E + 7))" "$other_tab")"
  read -r sel_answered tab_sent tab_answered <<<"$timing"
  echo "the select was sent at 0 ms and answered at $sel_answered ms; the tab change was sent at $tab_sent ms and answered at $tab_answered ms"
  [ "$tab_answered" -lt "$SEND_AFTER_MS" ] || fail "$CASE: the tab change was answered $tab_answered ms after the select was sent, not inside the $SEND_AFTER_MS ms window: the case would prove nothing"
  echo "OK: the tab change was answered inside the window ($tab_answered < $SEND_AFTER_MS ms)"
  tries=0
  while [ "$tries" -lt "$(((SEND_AFTER_MS + 5000) / 100))" ]; do
    got="$(client_notes selection_changed | tail -n +"$((before + 1))" | grep -F "\"text\": \"six();\"" | grep -F "$SEL_FILE" || true)"
    [ -z "$got" ] || break
    tries=$((tries + 1))
    sleep 0.1
  done
  [ -n "$got" ] || fail "$CASE: the client never got the selection of $SEL_FILE (last: $(client_notes selection_changed | tail -n 1))"
  echo "OK: the selection of this worktree's file reached its client although another worktree's tab was selected inside its window"
  sockcall surface.tabs.close "index=$other_tab" confirm=yes >/dev/null
}

case_no_eligible_agent_toast() {
  close_tabs_of_kind terminal
  expect_state "no connection once the clients are gone" 'len(d["connections"])' 0
  # A new worktree opens with a chat tab, which can take an at-mention: close it.
  close_tabs_of_kind chat
  ctl surface tabs select "$(file_tab_index)" >/dev/null
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  expect_state "the toast is in the state" '"No Claude terminal or chat in this worktree" in d["toasts"]' True
}

# A terminal nobody focused (created, then another tab made active before its client connects) is
# owned but is in no focus history: the at-mention reaches it, and leaves no toast.
case_at_mention_without_focus_history() {
  local project third third_id gate="$RUN_DIR/gate-third"
  project="$(project_id "$WT")"
  ctl new-workspace --project "$project" --branch ide-third >/dev/null
  third="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-third"]
print(paths[0] if paths else "")')"
  [ -n "$third" ] || fail "$CASE: sirioctl does not list the third worktree"
  third_id="$(worktree_id "$third")"
  write_sel_file "$third/sel.rs" >/dev/null
  ctl select-workspace --workspace "$third_id" >/dev/null
  rm -f "$gate"
  start_app_client "$gate"
  ctl surface editor open --path "$third/sel.rs" >/dev/null
  touch "$gate"
  wait_client_ready
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "owned by its pane" '"'"$CLIENT_PANE"'" in [c["owner_pane"] for c in d["connections"]]' True
  read -r SEL_X SEL_L5S SEL_L5E SEL_L6 SEL_L7E < <(write_sel_file "$third/sel.rs")
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  wait_notes at_mentioned 1
  echo "OK: the owned terminal nobody focused got the at-mention"
  end_app_client
  expect_state "no connection once that client is gone" 'len(d["connections"])' 0
  ctl select-workspace --workspace "$WT_ID" >/dev/null
}

case_owner_after_restart() {
  local second second_id tries=0
  second="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-second"]
print(paths[0] if paths else "")')"
  [ -n "$second" ] || fail "$CASE: sirioctl does not list the second worktree"
  second_id="$(worktree_id "$second")"
  ctl select-workspace --workspace "$second_id" >/dev/null
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "owned while its worktree is selected" 'd["connections"][0]["owner_pane"]' "$CLIENT_PANE"
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  app_quit
  wait_client "the old connection closed" 'r.get("event") == "closed"' 1
  app_start
  select_repo
  WT_ID="$(worktree_id "$WT")"
  client_do "token $TOKEN"
  wait_client "reconnected" 'r.get("event") == "connected"' 2
  expect_state "the reconnected client is a connection" 'len(d["connections"])' 1
  expect_state "ownerless while its worktree is not mounted" 'd["connections"][0]["owner_pane"]' None
  expect_state "its worktree is found from its cwd" 'd["connections"][0]["worktree"]' "$second"
  # The state can show no owner until a pane of that worktree is live, so "ownerless" here holds
  # by construction and is not asserted: the proof of this case is the select step below. The
  # worktree is set by the first resolution (RESOLVE_AFTER_MS after the reconnect), whose retries
  # end within five seconds of it: select the worktree after that, so only the selection can
  # resolve the owner. The sleep counts from the moment the worktree was seen set above.
  sleep 6
  ctl select-workspace --workspace "$second_id" >/dev/null
  expect_state "selecting the worktree gives it an owner" 'd["connections"][0]["owner_pane"] is not None' True
  expect_state "the owner's worktree is the second one" 'd["connections"][0]["worktree"]' "$second"
  end_app_client
}

# After a relaunch nothing has been focused: one at-mention from a file of the worktree still
# reaches the owned terminal, focuses its tab and raises no toast. A file is the active tab when
# Sirio quits, so the restored terminal is shown (it gets its pid) without being focused.
case_at_mention_after_relaunch() {
  local second second_id tries=0
  second="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-second"]
print(paths[0] if paths else "")')"
  second_id="$(worktree_id "$second")"
  ctl select-workspace --workspace "$second_id" >/dev/null
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
  read -r SEL_X SEL_L5S SEL_L5E SEL_L6 SEL_L7E < <(write_sel_file "$second/sel.rs")
  ctl surface editor open --path "$second/sel.rs" >/dev/null
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  app_quit
  wait_client "the old connection closed" 'r.get("event") == "closed"' 1
  app_start
  select_repo
  WT_ID="$(worktree_id "$WT")"
  client_do "token $TOKEN"
  wait_client "reconnected" 'r.get("event") == "connected"' 2
  ctl select-workspace --workspace "$second_id" >/dev/null
  expect_state "selecting the worktree gives it an owner" 'd["connections"][0]["owner_pane"] is not None' True
  # After the relaunch nothing has been focused: one at-mention still reaches the owned terminal,
  # focuses its tab and raises no toast.
  local before toasts_before toasts_after client_tab
  before="$(note_count at_mentioned)"
  toasts_before="$(state_expr 'len(d["toasts"])')"
  client_tab="$(tab_index terminal)"
  [ "$(tabs_field active)" != "$client_tab" ] || fail "$CASE: the terminal is already the active tab"
  select_ranges "$SEL_L5S..$SEL_L7E"
  sockcall surface.editor.context "offset=$SEL_L6" action=send-to-agent >/dev/null
  wait_notes at_mentioned "$((before + 1))"
  tries=0
  while [ "$(tabs_field active)" != "$client_tab" ] && [ "$tries" -lt 50 ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  [ "$(tabs_field active)" = "$client_tab" ] || fail "$CASE: the client's tab ($client_tab) is not focused (active: $(tabs_field active))"
  sleep 0.5
  toasts_after="$(state_expr 'len(d["toasts"])')"
  [ "$toasts_after" = "$toasts_before" ] || fail "$CASE: the at-mention raised a toast ($toasts_before then $toasts_after)"
  echo "OK: after the relaunch the at-mention reached the client, focused its tab and raised no toast ($toasts_before toasts both times)"
  end_app_client
}

# The text of the client's answer to its call-N: the JSON the one text item holds. A JSON-RPC
# error is a failure.
client_answer() { # N
  python3 -I - "$CLIENT_LOG" "call-$1" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    frame = r.get("frame", {})
    if r.get("dir") == "in" and frame.get("id") == sys.argv[2]:
        if "error" in frame:
            sys.exit("call failed: " + json.dumps(frame["error"]))
        print(frame["result"]["content"][0]["text"])
PY
}
# Waits until the drawer of the selected worktree counts WANT (errors/warnings/notes) for the
# active file.
expect_problems() { # want
  local tries=0 got=""
  while [ "$tries" -lt 100 ]; do
    got="$("$CTL" surface editor state 2>/dev/null | sed -n 's/^problems=//p' | head -n 1)"
    [ "$got" = "$1" ] && { echo "OK: the drawer counts $1"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: the drawer counts ${got:-nothing}, expected $1"
}
write_diag_file() { # path
  printf 'import os\ndef hello():\n    pass\n' >"$1"
}

case_diagnostics_file() {
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  write_diag_file "$WT/a.py"
  ctl surface editor open --path "$WT/a.py" >/dev/null
  expect_problems 0/1/0
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
  client_do "call tools/call {\"name\":\"getDiagnostics\",\"arguments\":{\"uri\":\"file://$WT/a.py\"}}"
  wait_client "getDiagnostics answered" 'r.get("dir") == "in" and r.get("frame", {}).get("id") == "call-1"' 1
  client_answer 1 | python3 -I -c '
import json, sys
want_uri = sys.argv[1]
items = json.load(sys.stdin)
assert len(items) == 1, items
item = items[0]
assert item["uri"] == want_uri, item["uri"]
assert len(item["diagnostics"]) == 1, item
d = item["diagnostics"][0]
assert d["severity"] == "Warning", d
assert d["range"]["start"] == {"line": 1, "character": 4}, d
assert d["range"]["end"] == {"line": 1, "character": 9}, d
assert "hello" in d["message"], d
' "file://$WT/a.py" || fail "$CASE: the answer is not the seeded warning: $(client_answer 1 | cut -c1-400)"
  echo "OK: one Warning at 1:4-1:9 for a.py, the uri echoed"
  end_app_client
}

case_diagnostics_outside_worktree() {
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
  client_do 'call tools/call {"name":"getDiagnostics","arguments":{"uri":"file:///etc/hostname"}}'
  wait_client "getDiagnostics answered" 'r.get("dir") == "in" and r.get("frame", {}).get("id") == "call-1"' 1
  [ "$(client_answer 1)" = "[]" ] || fail "$CASE: expected [], got $(client_answer 1 | cut -c1-300)"
  echo "OK: a file outside every worktree answers []"
  end_app_client
}

case_diagnostics_workspace() {
  local second
  second="$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-second"]
print(paths[0] if paths else "")')"
  [ -n "$second" ] || fail "$CASE: sirioctl does not list the second worktree"
  ctl select-workspace --workspace "$(worktree_id "$second")" >/dev/null
  write_diag_file "$second/a.py"
  ctl surface editor open --path "$second/a.py" >/dev/null
  expect_problems 0/1/0
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
  expect_state "owned by its pane" 'd["connections"][0]["owner_pane"]' "$CLIENT_PANE"
  client_do 'call tools/call {"name":"getDiagnostics","arguments":{}}'
  wait_client "getDiagnostics answered" 'r.get("dir") == "in" and r.get("frame", {}).get("id") == "call-1"' 1
  client_answer 1 | python3 -I -c '
import json, sys
mine, other = sys.argv[1], sys.argv[2]
uris = [item["uri"] for item in json.load(sys.stdin)]
assert "file://" + mine + "/a.py" in uris, uris
assert not [u for u in uris if u.startswith("file://" + other + "/")], uris
' "$WT" "$second" || fail "$CASE: the workspace answer is not this worktree only: $(client_answer 1 | cut -c1-400)"
  echo "OK: the owner's worktree answers its file, and nothing of the second worktree"
  end_app_client
}

# A client run by this script itself, outside every Sirio shell (spec 4.2's `claude` started in an
# outside terminal): its ancestry meets no pane's shell, its cwd is $1. Sets CLIENT_FIFO,
# CLIENT_LOG and OUTSIDE_PID.
start_outside_client() { # cwd
  CLIENT_N=$((CLIENT_N + 1))
  CLIENT_FIFO="$RUN_DIR/client-$CLIENT_N.fifo"
  CLIENT_LOG="$RUN_DIR/client-$CLIENT_N.jsonl"
  rm -f "$CLIENT_FIFO" "$CLIENT_LOG"
  mkfifo "$CLIENT_FIFO"
  local port
  port="$(state_expr 'd["port"]')"
  (cd "$1" && exec env CLAUDE_CODE_SSE_PORT="$port" python3 "$CLIENT" --fifo "$CLIENT_FIFO" --transcript "$CLIENT_LOG" >/dev/null 2>&1) &
  OUTSIDE_PID=$!
  wait_client_ready
}
end_outside_client() {
  client_do quit
  wait "$OUTSIDE_PID" 2>/dev/null || true
}
second_worktree() {
  "$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == "ide-second"]
print(paths[0] if paths else "")'
}
# The state expression for a field of the outside client's own connection (by its pid: an earlier
# connection may still be listed).
outside_conn() { # field
  printf '[c for c in d["connections"] if c["pid"] == %s][0]["%s"]' "$OUTSIDE_PID" "$1"
}
# The state's count of resolutions a hook started (not a request): see IdeBridge.hook_resolutions.
hook_resolutions() { state_expr 'd["hook_resolutions"]'; }
# Waits until every delayed resolution an earlier connection started has fired, and the retry
# chain such a resolution leaves has ended (it ends at its next 500 ms tick once the connection
# is gone): RESOLVE_AFTER_MS after the earlier case's connect plus a margin, counted from now.
# The earlier case connected before this one began, so it is at least that long ago.
wait_hooks_quiet() {
  sleep_ms "$((RESOLVE_AFTER_MS + 1500))"
}
# Fails, naming the precondition, when a hook resolved a connection since the count BEFORE: the
# ownerless cases mean to prove the request's own resolution, which a hook's would stand in for.
assert_no_hook_resolution() { # before
  local after
  after="$(hook_resolutions)"
  [ "$after" = "$1" ] || fail "$CASE: precondition broken: a hook resolution ran between the client's connect and the answer (hook_resolutions $1 then $after), so the answer proves nothing about the request resolving for itself"
  echo "OK: no hook resolution ran in between (hook_resolutions $1 before, $after after)"
}
# The uris of a getDiagnostics {} answer, one per line.
workspace_uris() {
  client_do 'call tools/call {"name":"getDiagnostics","arguments":{}}'
  wait_client "getDiagnostics answered" 'r.get("dir") == "in" and r.get("frame", {}).get("id") == "call-1"' 1 >&2
  client_answer 1 | python3 -I -c '
import json, sys
for item in json.load(sys.stdin):
    print(item["uri"])'
}

case_diagnostics_ownerless_cwd() {
  local second uris hooks_before
  second="$(second_worktree)"
  [ -n "$second" ] || fail "$CASE: sirioctl does not list the second worktree"
  wait_hooks_quiet
  hooks_before="$(hook_resolutions)"
  start_outside_client "$second"
  wait_client "connected" 'r.get("event") == "connected"' 1
  # Asked at once. No hook resolution may have run between the connect and the answer (checked
  # from the state's count); then only the request's own resolution can have told the app which
  # worktree the client is in, and the content below shows it did.
  uris="$(workspace_uris)"
  assert_no_hook_resolution "$hooks_before"
  [ "$uris" = "file://$second/a.py" ] || fail "$CASE: expected only file://$second/a.py, got: $uris"
  expect_state "ownerless" "$(outside_conn owner_pane)" None
  expect_state "in the worktree its cwd is in" "$(outside_conn worktree)" "$second"
  echo "OK: the ownerless client of the second worktree got that worktree's file only"
  end_outside_client
}

case_diagnostics_ownerless_elsewhere() {
  local second uris hooks_before
  second="$(second_worktree)"
  [ -n "$second" ] || fail "$CASE: sirioctl does not list the second worktree"
  wait_hooks_quiet
  hooks_before="$(hook_resolutions)"
  start_outside_client "$RUN_DIR"
  wait_client "connected" 'r.get("event") == "connected"' 1
  uris="$(workspace_uris | sort)"
  assert_no_hook_resolution "$hooks_before"
  expect_state "ownerless" "$(outside_conn owner_pane)" None
  expect_state "in no worktree" "$(outside_conn worktree)" None
  [ "$uris" = "$(printf 'file://%s/a.py\nfile://%s/a.py\n' "$WT" "$second" | sort)" ] \
    || fail "$CASE: expected the a.py of $WT and of $second, got: $uris"
  echo "OK: the ownerless client with no worktree got the files of every worktree"
  end_outside_client
}

# The run's only language servers are the fake one: every descendant of the app that looks like a
# language server runs fake_lsp.py, and at least one such process ran.
assert_only_the_fake_server() {
  CASE="only_the_fake_server"
  local pids="$APP_PID" frontier="$APP_PID" next child cmd fake=0 other=""
  while [ -n "$frontier" ]; do
    next=""
    for pid in $frontier; do
      for child in $(pgrep -P "$pid" 2>/dev/null || true); do next="$next $child"; pids="$pids $child"; done
    done
    frontier="$next"
  done
  for pid in $pids; do
    cmd="$(tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null || true)"
    case "$cmd" in
      *fake_lsp.py*) fake=$((fake + 1)) ;;
      *rust-analyzer*|*pyright*|*pylsp*|*typescript-language-server*|*gopls*|*clangd*|*jdtls*|*zls*|*lua-language-server*) other="$other [$cmd]" ;;
    esac
  done
  [ -z "$other" ] || fail "$CASE: a real language server ran:$other"
  [ "$fake" -ge 1 ] || fail "$CASE: no fake language server was running"
  echo "OK: $fake fake language server process(es) and no other language server under the app"
}

# ---------------------------------------------------------------- proposals (openDiff)

# Cases of Claude's `openDiff` (spec 2026-11-15 4.7, 4.8, 10.4, 10.8). Each starts its own client
# in an application terminal tab, so the active tab is a Primary terminal and a proposal opens
# beside it in Secondary. The client sends an openDiff with `send` (it does not wait: the call
# stays unanswered until the user decides), and the cases read the answer, or its absence, from
# the transcript by request id.

# The path of the worktree on branch $1, empty when there is none.
branch_worktree() { # branch
  "$CTL" list-workspaces --json | python3 -I -c '
import json, sys
paths = [row["path"] for row in json.load(sys.stdin) if row["branch"] == sys.argv[1]]
print(paths[0] if paths else "")' "$1"
}
# The path of the worktree on branch $1, made when it does not exist yet. A worktree made late in
# a run can be dropped from the sidebar by a catalog refresh that began before it existed, so the
# cases that remove one remove ide-second, which every run makes at its start.
ensure_worktree() { # branch
  local path
  path="$(branch_worktree "$1")"
  if [ -z "$path" ]; then
    ctl new-workspace --project "$(project_id "$WT")" --branch "$1" >/dev/null 2>&1 || true
    path="$(branch_worktree "$1")"
    # A worktree git has but the sidebar dropped (an older refresh landed after it) comes back
    # with the rescan a select runs.
    local tries=0
    while [ -z "$path" ] && [ "$tries" -lt 20 ]; do
      tries=$((tries + 1))
      "$CTL" select-workspace --workspace "$WT_ID" >/dev/null 2>&1 || true
      sleep 0.5
      path="$(branch_worktree "$1")"
    done
  fi
  [ -n "$path" ] || fail "$CASE: no worktree on branch $1"
  printf '%s' "$path"
}

# Selects a worktree just created: its row reaches the control state a moment after the listing.
select_new_worktree() { # path
  local tries=0
  until "$CTL" select-workspace --workspace "$1" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -lt 50 ] || fail "$CASE: $1 never became selectable ($("$CTL" select-workspace --workspace "$1" 2>&1 | head -n 2); rows: $("$CTL" list-workspaces --json | python3 -I -c 'import json, sys; print([(r["branch"], r["path"][-30:]) for r in json.load(sys.stdin)])'))"
    sleep 0.2
  done
}

# A Claude in a new application terminal of the selected worktree, connected.
proposal_client() {
  start_app_client
  wait_client "connected" 'r.get("event") == "connected"' 1
}
# openDiff NAME PATH CONTENTS: CONTENTS has printf-style escapes (\n, \r). The old file is PATH
# unless a fourth argument names another.
open_diff() { # tab_name path contents [old_path]
  local line
  line="$(python3 -I -c '
import json, sys
name, path, contents = sys.argv[1:4]
old = sys.argv[4] if len(sys.argv) > 4 else path
print(json.dumps({"name": "openDiff", "arguments": {"old_file_path": old, "new_file_path": path,
    "new_file_contents": contents.encode().decode("unicode_escape"), "tab_name": name}}))' "$@")"
  client_do "send tools/call $line"
}
# The JSON-RPC id of the last openDiff the client sent for tab_name $1.
req_id() { # tab_name
  python3 -I - "$CLIENT_LOG" "$1" <<'PY'
import json, sys
found = ""
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    f = r.get("frame", {})
    if r.get("dir") == "out" and f.get("method") == "tools/call" and f["params"].get("name") == "openDiff" \
            and f["params"]["arguments"].get("tab_name") == sys.argv[2]:
        found = f["id"]
print(found)
PY
}
# The JSON-RPC id of the FIRST openDiff the client sent for tab_name $1.
req_id_first() { # tab_name
  python3 -I - "$CLIENT_LOG" "$1" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    f = r.get("frame", {})
    if r.get("dir") == "out" and f.get("method") == "tools/call" and f["params"].get("name") == "openDiff" \
            and f["params"]["arguments"].get("tab_name") == sys.argv[2]:
        print(f["id"])
        break
PY
}
# The answer to request $1 as JSON: {"texts": [...]} for a result, {"error": code} for an error,
# nothing while it is unanswered.
response_json() { # call-N
  python3 -I - "$CLIENT_LOG" "$1" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    f = r.get("frame", {})
    if r.get("dir") == "in" and f.get("id") == sys.argv[2]:
        if "error" in f:
            print(json.dumps({"error": f["error"]["code"]}))
        else:
            print(json.dumps({"texts": [c["text"] for c in f["result"]["content"]]}))
PY
}
wait_response() { # call-N: prints the answer as JSON
  local tries=0 got=""
  while [ "$tries" -lt 100 ]; do
    got="$(response_json "$1")"
    [ -z "$got" ] || { printf '%s' "$got"; return 0; }
    tries=$((tries + 1))
    sleep 0.1
  done
  fail "$CASE: no answer to $1 (last frames: $(tail -n 3 "$CLIENT_LOG" | cut -c1-300))"
}
# The answer to $1 satisfies a Python expression over `d` ({"texts": [...]} or {"error": code}).
assert_response() { # call-N expr description
  local got
  got="$(wait_response "$1")"
  printf '%s' "$got" | python3 -I -c 'import json, sys; d = json.load(sys.stdin); sys.exit(0 if eval(sys.argv[1]) else 1)' "$2" \
    || fail "$CASE: $3: the answer to $1 is $got"
  echo "OK: $3 ($got)"
}
# Request $1 has not been answered at all (no FILE_SAVED, DIFF_REJECTED, TAB_CLOSED, nor an error).
assert_unanswered() { # call-N description
  local got
  got="$(response_json "$1")"
  [ -z "$got" ] || fail "$CASE: $2: $1 was answered: $got"
  echo "OK: $2 ($1 has no answer)"
}
# No decision was ever sent for request $1, whatever else was (an error carries none).
assert_no_decision() { # call-N description
  local got
  got="$(response_json "$1")"
  case "$got" in *FILE_SAVED*|*DIFF_REJECTED*|*TAB_CLOSED*) fail "$CASE: $2: $1 got a decision: $got" ;; esac
  echo "OK: $2 ($1: ${got:-no answer})"
}
act() { # tab_name action [text]
  sockcall surface.ide.act "tab_name=$1" "action=$2" ${3+"text=$3"} >/dev/null
}
act_alone() { sockcall surface.ide.act "action=$1" ${2+"text=$2"} >/dev/null; }
pexpr() { printf '[p for p in d["proposals"] if p["tab_name"] == "%s"][0]["%s"]' "$1" "$2"; }
expect_proposal() { # tab_name field want
  expect_state "proposal $1: $2" "$(pexpr "$1" "$2")" "$3"
}
expect_no_proposals() { expect_state "no proposal is open" 'len(d["proposals"])' 0; }
# The 1-based position of the first tab titled $1, 0 when none.
tab_pos() { # title
  "$CTL" surface tabs read --json | python3 -I -c '
import json, sys
d = json.load(sys.stdin)[0]
print(next((int(k[4:]) for k, v in d.items() if k.startswith("tab.") and v.split("|", 2)[2] == sys.argv[1]), 0))' "$1"
}
expect_tab() { # title: waits for the tab, prints its position
  local tries=0 pos=0
  while [ "$tries" -lt 100 ]; do
    pos="$(tab_pos "$1")"
    [ "$pos" != 0 ] && { printf '%s' "$pos"; return 0; }
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: no tab titled $1 (tabs: $("$CTL" surface tabs read --json | cut -c1-500))"
}
expect_no_tab() { # title
  local tries=0
  while [ "$tries" -lt 100 ]; do
    [ "$(tab_pos "$1")" = 0 ] && return 0
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: the tab titled $1 is still there"
}
tab_count() { tabs_field count; }
toast_count() { # text
  state_expr 'len([t for t in d["toasts"] if t == "'"$1"'"])'
}
# The file's bytes are as they were: Sirio never writes a file Claude proposed a change to.
assert_file_is() { # path printf-text
  python3 -I -c '
import sys
want = sys.argv[2].encode().decode("unicode_escape").encode()
sys.exit(0 if open(sys.argv[1], "rb").read() == want else 1)' "$1" "$2" || fail "$CASE: $1 was changed by Sirio"
}

# ---- the CLIPBOARD, from outside the app (python3-gi with GTK 3 on this run's own display, the
# way test-prompts-e2e.sh does: forced onto X11, a private HOME, nothing of the desktop's)

# Owns this display's CLIPBOARD with a text until it is stopped (60 s at most).
cat >"$RUN_DIR/clipown.py" <<'PY'
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk
Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text(sys.argv[1], -1)
GLib.timeout_add_seconds(60, Gtk.main_quit)
Gtk.main()
PY
# The CLIPBOARD of this display, as bytes, into FILE.
clipboard_to() { # file
  timeout 10 env -i PATH=/usr/bin:/bin HOME="$RUN_DIR/home" GDK_BACKEND=x11 DISPLAY="$DISPLAY_TARGET" \
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
# Puts TEXT on this display's CLIPBOARD, held by clipown.py until clipboard_stop.
clipboard_own() { # text
  env -i PATH=/usr/bin:/bin HOME="$RUN_DIR/home" GDK_BACKEND=x11 DISPLAY="$DISPLAY_TARGET" \
    python3 -I "$RUN_DIR/clipown.py" "$1" &
  CLIP_PID=$!
  sleep 1
}
clipboard_stop() {
  [ -z "$CLIP_PID" ] || terminate "$CLIP_PID"
  CLIP_PID=""
}
# Polls until the CLIPBOARD is exactly TEXT (printf %s: no trailing newline).
expect_clipboard() { # label text
  local tries=0
  printf '%s' "$2" >"$RUN_DIR/clipboard.want"
  while [ "$tries" -lt 50 ]; do
    if clipboard_to "$RUN_DIR/clipboard.got" 2>/dev/null && cmp -s "$RUN_DIR/clipboard.got" "$RUN_DIR/clipboard.want"; then
      echo "OK: $1 (the clipboard holds $(wc -c <"$RUN_DIR/clipboard.want" | tr -d ' ') bytes, as wanted)"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$CASE: $1: the clipboard is not what was wanted. want: $(cat "$RUN_DIR/clipboard.want" | cut -c1-300) got: $(cut -c1-300 "$RUN_DIR/clipboard.got" 2>/dev/null)"
}
# `surface.ide.comment`: what a click on the gutter and typing do. Fails when the app refuses.
comment() { # tab_name side line text [end_line] [commit]
  sockcall surface.ide.comment "tab_name=$1" "side=$2" "line=$3" "text=$4" ${5:+"end_line=$5"} ${6:+"commit=$6"} >/dev/null
}
COMMENTS_TOAST='Comments copied: paste them into Claude'
# How many times comments went to the clipboard since launch (one per ending).
deliveries() { state_expr 'd["deliveries"]'; }
# One delivery since BEFORE (a count taken with `deliveries`), and the newest toast is the copy's.
expect_one_delivery() { # before
  expect_state "exactly one clipboard delivery" 'd["deliveries"]' "$(($1 + 1))"
  expect_state "the delivery is the clipboard" 'd["last_delivery"]' clipboard
  expect_state "the newest toast says so" 'd["toasts"][-1]' "$COMMENTS_TOAST"
}
# A proposal of FILE (old four lines, a proposal that changes two of them and adds one) opened
# as TAB, with an Old-side comment on line 2 and, when $3 is two, a New-side comment on the range 4-5.
open_commented() { # tab_name file count
  printf 'alpha\nbeta\ngamma\ndelta\n' >"$WT/$2"
  open_diff "$1" "$WT/$2" 'alpha\nBETA\ngamma\nDELTA\nepsilon\n'
  expect_proposal "$1" ready True
  comment "$1" old 2 "old note"
  [ "$3" != 2 ] || comment "$1" new 4 "new note" 5
  expect_proposal "$1" comments "$3"
}
# A 300-line file with every tenth line changed (thirty hunks): line 150 starts below the viewport.
open_long() { # tab_name file
  seq -f 'line %g' 1 300 >"$WT/$2"
  local changed
  changed="$(seq 1 300 | awk '{ if ($1 % 10 == 0) print "CHANGED " $1; else print "line " $1 }' | awk '{ printf "%s\\n", $0 }')"
  open_diff "$1" "$WT/$2" "$changed"
  expect_proposal "$1" ready True
}
want_old() { printf '@%s#L2 old note' "$1"; }
want_both() { printf '@%s#L2 old note\n@%s#L4-5 (proposed) new note' "$1" "$1"; }

case_reject_with_comments_clipboard() {
  proposal_client
  expect_state "nothing was delivered yet" 'd["last_delivery"]' None
  local before
  before="$(deliveries)"
  printf 'alpha\nbeta\ngamma\ndelta\n' >"$WT/cm1.txt"
  open_diff cm1 "$WT/cm1.txt" 'alpha\nBETA\ngamma\nDELTA\nepsilon\n'
  expect_proposal cm1 ready True
  comment cm1 old 2 "old note"
  expect_proposal cm1 composer None
  expect_proposal cm1 cards 1
  comment cm1 new 4 "new note" 5
  expect_proposal cm1 composer None
  expect_proposal cm1 cards 2
  expect_proposal cm1 comments 2
  expect_proposal cm1 reject_label "Reject with 2 comments"
  act cm1 reject
  assert_response "$(req_id cm1)" 'd["texts"] == ["DIFF_REJECTED", "cm1"]' "Reject answers DIFF_REJECTED"
  expect_no_proposals
  expect_no_tab '✻ cm1.txt'
  expect_one_delivery "$before"
  expect_clipboard "the clipboard holds the two lines" "$(want_both cm1.txt)"
  end_app_client
}

case_comment_composer_anchor() {
  proposal_client
  open_long cm2 cm2.txt
  local before
  before="$(deliveries)"
  clipboard_own "sentinel before the reject"
  expect_clipboard "the case owns the clipboard" "sentinel before the reject"
  comment cm2 old 150 "a draft" "" false
  # in_view is read from the diff list's own layout (gpui's ListState) for line 150's row; the
  # scroll to the composer lands on a later layout, so it is polled for.
  expect_proposal cm2 composer "{'side': 'old', 'line': 150, 'in_view': True, 'draft_len': 7}"
  expect_proposal cm2 comments 0
  expect_proposal cm2 cards 0
  act cm2 reject
  assert_response "$(req_id cm2)" 'd["texts"] == ["DIFF_REJECTED", "cm2"]' "Reject answers DIFF_REJECTED"
  expect_no_proposals
  expect_clipboard "a draft is never delivered: the clipboard is as it was" "sentinel before the reject"
  expect_state "nothing was delivered" 'd["deliveries"]' "$before"
  clipboard_stop
  end_app_client
}

# The user types into the composer with real keys after the scroll has landed: the diff keeps
# showing the line it is about.
case_composer_typing_keeps_line() {
  proposal_client
  open_long cm7 cm7.txt
  comment cm7 old 150 "" "" false
  expect_proposal cm7 composer "{'side': 'old', 'line': 150, 'in_view': True, 'draft_len': 0}"
  capture ide-composer-typing
  local window geometry width height active
  active="$(tabs_field active)"
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window for pid $APP_PID"
  geometry="$(DISPLAY="$DISPLAY_TARGET" xdotool getwindowgeometry "$window" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')"
  read -r width height <<<"$geometry"
  # A click into the composer's text box (measured on ide-composer-typing.png: 68% across, 31%
  # down), then keys, as a user writes a comment.
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$((width * 68 / 100))" "$((height * 31 / 100))" click 1
  sleep 0.3
  [ "$(tabs_field active)" = "$active" ] || fail "$CASE: the click made another tab the active one; the layout under test is gone"
  DISPLAY="$DISPLAY_TARGET" xdotool type --delay 60 "typed"
  expect_proposal cm7 composer "{'side': 'old', 'line': 150, 'in_view': True, 'draft_len': 5}"
  act cm7 reject
  expect_no_proposals
  end_app_client
}

# The verb's errors: each leaves the tab as it was, and each names its reason (a timeout or a
# stall is not a refusal).
refused() { # description expected-error-text command...
  local what="$1" want="$2" err
  shift 2
  if err="$("$@" 2>&1 >/dev/null)"; then fail "$CASE: $what was accepted"; fi
  case "$err" in
    *"$want"*) echo "OK: $what is refused ($want)" ;;
    *) fail "$CASE: $what was refused for another reason: $err" ;;
  esac
}
case_comment_refusals() {
  proposal_client
  printf 'alpha\nbeta\ngamma\ndelta\n' >"$WT/cm3.txt"
  open_diff cm3 "$WT/cm3.txt" 'alpha\nBETA\ngamma\nDELTA\nepsilon\n'
  expect_proposal cm3 ready True
  refused "an unknown tab" "no proposal has that tab_name" comment nope old 2 "x"
  refused "a line that is not in the diff" "That line is not in the diff." comment cm3 old 99 "x"
  refused "a side that is neither old nor new" "side must be old or new" comment cm3 left 2 "x"
  refused "an end_line that is not a number" "end_line is not a line number" \
    sockcall surface.ide.comment tab_name=cm3 side=old line=2 end_line=two text=x
  refused "a commit that is not true or false" "commit must be true or false" \
    sockcall surface.ide.comment tab_name=cm3 side=old line=2 commit=maybe text=x
  expect_proposal cm3 comments 0
  expect_proposal cm3 cards 0
  expect_proposal cm3 composer None
  # Edit shows no diff, so no gutter: a click cannot comment there either.
  act cm3 edit 'edited'
  expect_proposal cm3 editing True
  refused "a comment while the proposal is being edited" "no diff is shown while the proposal is being edited" comment cm3 old 2 "x"
  # Back on the diff of the edited text, the proposed side takes no new comment (Ruling 27).
  act cm3 show_diff
  expect_proposal cm3 editing False
  expect_proposal cm3 ready True
  refused "a New-side comment once Edit changed the text" "The proposed side takes no new comment" comment cm3 new 1 "x"
  expect_proposal cm3 comments 0
  expect_proposal cm3 composer None
  act cm3 reject
  expect_no_proposals
  end_app_client
}

case_comments_on_withdraw() {
  proposal_client
  local before
  before="$(deliveries)"
  open_commented cm4 cm4.txt 2
  comment cm4 old 4 "a draft" "" false
  expect_proposal cm4 comments 2
  client_do 'call tools/call {"name":"close_tab","arguments":{"tab_name":"cm4"}}'
  wait_client "close_tab answered" 'r.get("dir") == "in" and r.get("frame", {}).get("result", {}).get("content", [{}])[0].get("text") == "TAB_CLOSED"' 1
  assert_response "$(req_id cm4)" 'd == {"error": -32000}' "the withdrawn openDiff gets -32000, no decision"
  expect_no_proposals
  expect_one_delivery "$before"
  expect_clipboard "the clipboard holds the two comments and not the draft" "$(want_both cm4.txt)"
  end_app_client
}

# closeAllDiffTabs is one ending: the comments of every proposal it closes arrive together.
case_comments_on_close_all() {
  proposal_client
  local before
  before="$(deliveries)"
  open_commented ca1 ca1.txt 1
  open_commented ca2 ca2.txt 1
  client_do 'call tools/call {"name":"closeAllDiffTabs","arguments":{}}'
  wait_client "closeAllDiffTabs answered" 'r.get("dir") == "in" and r.get("frame", {}).get("result", {}).get("content", [{}])[0].get("text") == "CLOSED_2_DIFF_TABS"' 1
  assert_response "$(req_id ca1)" 'd == {"error": -32000}' "the first gets -32000"
  assert_response "$(req_id ca2)" 'd == {"error": -32000}' "the second gets -32000"
  expect_no_proposals
  expect_one_delivery "$before"
  expect_clipboard "the clipboard holds both proposals' comments" "$(want_old ca1.txt)
$(want_old ca2.txt)"
  end_app_client
}

# An openDiff with the tab_name of an open proposal ends the old one: its comments are delivered.
case_comments_on_replace() {
  proposal_client
  local before
  before="$(deliveries)"
  open_commented cm8 cm8.txt 1
  printf 'one\n' >"$WT/cm8b.txt"
  open_diff cm8 "$WT/cm8b.txt" 'two\n'
  assert_response "$(req_id_first cm8)" 'd == {"error": -32000}' "the replaced openDiff gets -32000, no decision"
  expect_one_delivery "$before"
  expect_clipboard "the clipboard holds the old proposal's comment" "$(want_old cm8.txt)"
  expect_proposal cm8 comments 0
  act cm8 reject
  expect_no_proposals
  end_app_client
}

# Accept answers Claude as always; the comments the user wrote still reach the clipboard.
case_comments_on_accept() {
  proposal_client
  local before
  before="$(deliveries)"
  open_commented cm9 cm9.txt 1
  act cm9 accept
  assert_response "$(req_id cm9)" 'd["texts"][0] == "FILE_SAVED"' "Accept answers FILE_SAVED"
  expect_no_proposals
  expect_one_delivery "$before"
  expect_clipboard "the clipboard holds the comment" "$(want_old cm9.txt)"
  end_app_client
}

case_comments_on_confirmation_reject() {
  proposal_client
  local before
  before="$(deliveries)"
  open_commented cm5 cm5.txt 1
  act cm5 close
  expect_state "closing asks first" 'd["confirmation"]["title"]' "Closing rejects Claude's change"
  act_alone confirm_reject
  assert_response "$(req_id cm5)" 'd["texts"] == ["DIFF_REJECTED", "cm5"]' "Reject in the confirmation answers DIFF_REJECTED"
  expect_no_proposals
  expect_one_delivery "$before"
  expect_clipboard "the clipboard holds the comment" "$(want_old cm5.txt)"
  end_app_client
}

case_comments_on_disconnect() {
  proposal_client
  local before
  before="$(deliveries)"
  open_commented cm6 cm6.txt 1
  client_do close
  wait_client "the connection closed" 'r.get("event") == "closed"' 1
  expect_no_proposals
  assert_no_decision "$(req_id cm6)" "a closed connection is answered nothing"
  expect_one_delivery "$before"
  expect_clipboard "the clipboard holds the comment" "$(want_old cm6.txt)"
  end_app_client
}

case_proposal_accept() {
  proposal_client
  printf 'one\ntwo\nthree\n' >"$WT/p.txt"
  local active pos kinds positions
  active="$(tabs_field active)"
  open_diff accept-1 "$WT/p.txt" 'one\nTWO\nthree\n'
  expect_proposal accept-1 on_screen True
  expect_proposal accept-1 ready True
  pos="$(expect_tab '✻ p.txt')"
  echo "OK: the tab is titled ✻ p.txt (position $pos)"
  kinds="$(sockcall surface.layout.read | jq_py 'd["secondaryKinds"]')"
  positions="$(sockcall surface.layout.read | jq_py 'd["secondaryPositions"]')"
  case ",$kinds," in *,proposal,*) ;; *) fail "$CASE: secondaryKinds is $kinds" ;; esac
  case ",$positions," in *",$pos,"*) ;; *) fail "$CASE: the tab $pos is not in Secondary ($positions)" ;; esac
  echo "OK: the proposal is a Secondary tab ($kinds at $positions)"
  [ "$(tabs_field active)" = "$active" ] || fail "$CASE: opening the proposal moved the active tab from $active to $(tabs_field active)"
  echo "OK: focus stays on the Claude terminal (active tab $active)"
  expect_proposal accept-1 worktree "$WT"
  expect_proposal accept-1 owner_named True
  expect_proposal accept-1 counts "[1, 1]"
  expect_proposal accept-1 dirty False
  expect_proposal accept-1 refusal False
  [ "$(sockcall surface.layout.read | jq_py 'd["secondaryVisible"]')" = true ] || fail "$CASE: Secondary is not revealed"
  capture ide-proposal-bar
  assert_unanswered "$(req_id accept-1)" "nothing answers before the user does"
  act accept-1 accept
  assert_response "$(req_id accept-1)" 'd["texts"] == ["FILE_SAVED", "one\nTWO\nthree\n"]' "Accept answers FILE_SAVED and the proposal"
  expect_no_proposals
  expect_no_tab '✻ p.txt'
  assert_file_is "$WT/p.txt" 'one\ntwo\nthree\n'
  echo "OK: the file on disk is untouched: Claude writes it"
  end_app_client
}

case_proposal_edit_then_accept() {
  proposal_client
  printf 'a\r\nb\r\n' >"$WT/crlf.txt"
  open_diff edit-1 "$WT/crlf.txt" 'a\r\nB\r\n'
  expect_proposal edit-1 ready True
  expect_tab '✻ crlf.txt' >/dev/null
  act edit-1 edit $'edited\n'
  expect_proposal edit-1 editing True
  capture ide-proposal-edit
  act edit-1 show_diff
  expect_proposal edit-1 editing False
  expect_proposal edit-1 ready True
  act edit-1 accept
  assert_response "$(req_id edit-1)" 'd["texts"] == ["FILE_SAVED", "edited\r\n"]' "Accept sends the edited text in the file's CRLF"
  expect_no_proposals
  end_app_client
}

# Edit, with real keys: the typed characters reach the editor the user is looking at.
case_proposal_edit_keys() {
  proposal_client
  printf 'one\ntwo\nthree\n' >"$WT/keys.txt"
  local active window geometry width height
  active="$(tabs_field active)"
  open_diff keys-1 "$WT/keys.txt" 'one\ntwo\nthree\n'
  expect_proposal keys-1 ready True
  act keys-1 edit
  expect_proposal keys-1 editing True
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window for pid $APP_PID"
  geometry="$(DISPLAY="$DISPLAY_TARGET" xdotool getwindowgeometry "$window" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')"
  read -r width height <<<"$geometry"
  capture ide-proposal-edit-keys
  # In the middle of the Secondary half, on the editor's first lines (measured on
  # ide-proposal-edit-keys.png), then the End key so the characters land after `one`.
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$((width * 70 / 100))" "$((height * 20 / 100))" click 1
  sleep 0.3
  DISPLAY="$DISPLAY_TARGET" xdotool key ctrl+Home
  DISPLAY="$DISPLAY_TARGET" xdotool type --delay 60 "KEYS"
  [ "$(tabs_field active)" = "$active" ] || fail "$CASE: the click made another tab the active one; the layout under test is gone"
  sleep 0.5
  act keys-1 accept
  assert_response "$(req_id keys-1)" '"KEYS" in d["texts"][1] and d["texts"][0] == "FILE_SAVED"' "the characters typed with real keys are in the answer"
  end_app_client
}

case_proposal_reject() {
  proposal_client
  printf 'one\n' >"$WT/r.txt"
  open_diff reject-1 "$WT/r.txt" 'two\n'
  expect_proposal reject-1 ready True
  act reject-1 reject
  assert_response "$(req_id reject-1)" 'd["texts"] == ["DIFF_REJECTED", "reject-1"]' "Reject answers DIFF_REJECTED and the tab name"
  expect_no_proposals
  expect_no_tab '✻ r.txt'
  end_app_client
}

case_proposal_close_confirm() {
  proposal_client
  printf 'one\n' >"$WT/c.txt"
  open_diff close-1 "$WT/c.txt" 'two\n'
  expect_proposal close-1 ready True
  act close-1 close
  expect_state "closing asks first" 'd["confirmation"]["title"]' "Closing rejects Claude's change"
  expect_state "with Cancel and Reject, as drawn" 'd["confirmation"]["buttons"]' "['Cancel', 'Reject']"
  capture ide-proposal-confirm
  act_alone cancel
  expect_state "Cancel closes the question" 'd["confirmation"]' None
  expect_proposal close-1 on_screen True
  [ "$(tab_pos '✻ c.txt')" != 0 ] || fail "$CASE: Cancel closed the tab"
  assert_unanswered "$(req_id close-1)" "Cancel answers nothing"
  act close-1 close
  expect_state "closing asks again" 'd["confirmation"]["title"]' "Closing rejects Claude's change"
  act_alone confirm_reject
  assert_response "$(req_id close-1)" 'd["texts"] == ["DIFF_REJECTED", "close-1"]' "Reject in the confirmation answers DIFF_REJECTED"
  expect_no_proposals
  expect_no_tab '✻ c.txt'
  end_app_client
}

# Close Others on a file tab of Secondary, with the proposal among the others (Close Others
# closes the others of the kept tab's half): the question comes before anything closes, and
# Cancel keeps every tab.
case_close_others_guarded() {
  proposal_client
  local keep before
  printf 'one\n' >"$WT/o.txt"
  printf 'f1\n' >"$WT/f1.txt"
  printf 'f2\n' >"$WT/f2.txt"
  ctl surface editor open --path "$WT/f1.txt" >/dev/null
  keep="$(expect_tab 'f1.txt')"
  ctl surface editor open --path "$WT/f2.txt" >/dev/null
  expect_tab 'f2.txt' >/dev/null
  open_diff others-1 "$WT/o.txt" 'two\n'
  expect_proposal others-1 ready True
  before="$(tab_count)"
  act_alone close_others "$keep"
  expect_state "Close Others asks first" 'd["confirmation"]["then"]' close_tabs
  [ "$(tab_count)" = "$before" ] || fail "$CASE: tabs closed before the answer: $before then $(tab_count)"
  act_alone cancel
  sleep 1
  [ "$(tab_count)" = "$before" ] || fail "$CASE: Cancel closed tabs: $before then $(tab_count)"
  expect_proposal others-1 on_screen True
  assert_unanswered "$(req_id others-1)" "Cancel answers nothing"
  echo "OK: Cancel keeps all $before tabs and the proposal"
  # Closing a single tab through the verb, confirm=yes included, asks too.
  sockcall surface.tabs.close "index=$(tab_pos '✻ o.txt')" confirm=yes >/dev/null
  expect_state "closing it through the verb asks first" 'd["confirmation"]["then"]' close_tabs
  act_alone cancel
  expect_proposal others-1 on_screen True
  act_alone close_others "$keep"
  act_alone confirm_reject
  assert_response "$(req_id others-1)" 'd["texts"] == ["DIFF_REJECTED", "others-1"]' "Reject answers, then the original close goes on"
  expect_no_proposals
  expect_no_tab 'f2.txt'
  [ "$(tab_count)" -lt "$before" ] || fail "$CASE: after Reject the other tabs did not close ($before then $(tab_count))"
  echo "OK: the other tabs closed after Reject ($before then $(tab_count))"
  # Close Others closed the Claude terminal too, so its client is gone: nothing to quit.
  close_tabs_of_kind file
  close_tabs_of_kind terminal
}

# Closing the proposal's worktree asks before any session ends; the release verb refuses. Two
# Claudes wait on the worktree: one in a pane of it (its connection ends with the sessions, so it
# proves that nothing ended while asking), and one outside every Sirio shell (it outlives the
# close, so its transcript shows the answer Reject sent).
case_worktree_close_guarded() {
  local third third_id in_log in_fifo
  third="$(ensure_worktree ide-third)"
  third_id="$(worktree_id "$third")"
  ctl select-workspace --workspace "$third_id" >/dev/null
  printf 'x\ny\n' >"$third/w.txt"
  printf 'u\nv\n' >"$third/w2.txt"
  proposal_client
  open_diff wt-1 "$third/w.txt" 'x\nY\n'
  expect_proposal wt-1 ready True
  in_log="$CLIENT_LOG"
  in_fifo="$CLIENT_FIFO"
  start_outside_client "$third"
  wait_client "the outside client connected" 'r.get("event") == "connected"' 1
  open_diff wt-2 "$third/w2.txt" 'u\nV\n'
  expect_proposal wt-2 ready True
  expect_proposal wt-2 worktree "$third"
  expect_state "two Claudes are connected" 'len(d["connections"])' 2
  if "$CTL" close-workspace --workspace "$third_id" >"$RUN_DIR/close.out" 2>&1; then
    fail "$CASE: workspace.close succeeded with a proposal open: $(cat "$RUN_DIR/close.out")"
  fi
  grep -q 'Claude has a proposed change open in this worktree' "$RUN_DIR/close.out" \
    || fail "$CASE: workspace.close failed for another reason: $(cat "$RUN_DIR/close.out")"
  echo "OK: workspace.close refuses: $(head -n 1 "$RUN_DIR/close.out")"
  expect_state "the refusal closed nothing" 'len(d["connections"])' 2
  act wt-1 close_worktree
  expect_state "closing the worktree asks first" 'd["confirmation"]["then"]' remove_worktree
  expect_state "about both proposals" 'sorted(d["confirmation"]["proposals"])' "['wt-1', 'wt-2']"
  sleep 1
  expect_state "no session ended while asking" 'len(d["connections"])' 2
  act_alone cancel
  sleep 1
  expect_state "Cancel keeps both clients connected" 'len(d["connections"])' 2
  expect_state "and both proposals" 'len(d["proposals"])' 2
  assert_unanswered "$(req_id wt-2)" "Cancel answers nothing"
  [ "$("$CTL" list-workspaces --json | python3 -I -c '
import json, sys
print([r["mounted"] for r in json.load(sys.stdin) if r["id"] == sys.argv[1]][0])' "$third_id")" = true ] \
    || fail "$CASE: Cancel unmounted the worktree"
  [ -d "$third" ] || fail "$CASE: Cancel removed the checkout directory"
  git -C "$WT" worktree list --porcelain | grep -Fxq "worktree $third" || fail "$CASE: after Cancel git no longer lists $third"
  git -C "$WT" rev-parse --verify -q refs/heads/ide-third >/dev/null || fail "$CASE: Cancel deleted the local branch"
  echo "OK: Cancel keeps the checkout on disk, listed by git, its branch, the worktree mounted and the clients connected"
  act wt-1 close_worktree
  act_alone confirm_reject
  assert_response "$(req_id wt-2)" 'd["texts"] == ["DIFF_REJECTED", "wt-2"]' "Reject answers DIFF_REJECTED before the checkout is removed"
  local tries=0
  while [ -d "$third" ] && [ "$tries" -lt 100 ]; do tries=$((tries + 1)); sleep 0.2; done
  [ ! -d "$third" ] || fail "$CASE: the checkout is still on disk after Reject"
  git -C "$WT" worktree list --porcelain | grep -Fxq "worktree $third" && fail "$CASE: git still lists $third after Reject"
  echo "OK: after Reject the checkout directory is gone and git no longer lists it"
  expect_state "the pane's Claude went with its sessions, the outside one stayed" 'len(d["connections"])' 1
  expect_no_proposals
  end_outside_client
  CLIENT_LOG="$in_log" CLIENT_FIFO="$in_fifo"
  ctl select-workspace --workspace "$WT_ID" >/dev/null
}

# The sidebar's "Remove project from Sirio?" continues into RemoveProject, which asks first when a
# worktree of the project holds a proposal. A second project of its own, so the run keeps its repo.
case_project_close_guarded() {
  local repo2="$RUN_DIR/repo2" id2 in_log in_fifo tries=0
  rm -rf "$repo2"
  mkdir -p "$repo2"
  git -C "$repo2" init -q -b main
  git -C "$repo2" config user.email tester@example.invalid
  git -C "$repo2" config user.name Tester
  printf 'seed\n' >"$repo2/seed.txt"
  git -C "$repo2" add seed.txt
  git -C "$repo2" commit -q -m initial
  ctl project add "$repo2" >/dev/null
  id2="$(worktree_id "$repo2")"
  [ -n "$id2" ] || fail "$CASE: the second project is not listed"
  ctl select-workspace --workspace "$id2" >/dev/null
  printf 'p\nq\n' >"$repo2/q.txt"
  printf 'u\nv\n' >"$repo2/q2.txt"
  proposal_client
  open_diff pj-1 "$repo2/q.txt" 'p\nQ\n'
  expect_proposal pj-1 ready True
  in_log="$CLIENT_LOG"
  in_fifo="$CLIENT_FIFO"
  start_outside_client "$repo2"
  wait_client "the outside client connected" 'r.get("event") == "connected"' 1
  open_diff pj-2 "$repo2/q2.txt" 'u\nV\n'
  expect_proposal pj-2 ready True
  expect_state "two Claudes are connected" 'len(d["connections"])' 2
  act pj-1 remove_project
  expect_state "removing the project asks first" 'd["confirmation"]["then"]' remove_project
  sleep 1
  expect_state "no session ended while asking" 'len(d["connections"])' 2
  act_alone cancel
  sleep 1
  expect_state "Cancel keeps both clients connected" 'len(d["connections"])' 2
  expect_state "and both proposals" 'len(d["proposals"])' 2
  [ -n "$(worktree_id "$repo2")" ] || fail "$CASE: Cancel removed the project from the sidebar"
  assert_unanswered "$(req_id pj-2)" "Cancel answers nothing"
  echo "OK: Cancel keeps the project listed and the sessions alive"
  act pj-1 remove_project
  act_alone confirm_reject
  assert_response "$(req_id pj-2)" 'd["texts"] == ["DIFF_REJECTED", "pj-2"]' "Reject answers DIFF_REJECTED before the project goes"
  while [ -n "$(worktree_id "$repo2")" ] && [ "$tries" -lt 100 ]; do tries=$((tries + 1)); sleep 0.2; done
  [ -z "$(worktree_id "$repo2")" ] || fail "$CASE: the project is still listed after Reject"
  echo "OK: after Reject the project is gone"
  expect_state "the pane's Claude went with its sessions, the outside one stayed" 'len(d["connections"])' 1
  expect_no_proposals
  end_outside_client
  CLIENT_LOG="$in_log" CLIENT_FIFO="$in_fifo"
  ctl select-workspace --workspace "$WT_ID" >/dev/null
}

# A pending question holds identities, not positions (a sidebar row id is the project's index
# times 1000 plus the worktree's index). Git lists linked worktrees sorted by path, so three made
# under one fresh directory are adjacent and in order: a-gone, b-asked, c-next. The removal
# question of b-asked is open while a-gone is removed behind the app's back and the sidebar
# rescans: b-asked's row moves up by one, and the old row id now names c-next. Reject removes
# b-asked (looked up again by path) and not c-next.
case_confirm_stable_worktree() {
  local dir="$RUN_DIR/rowdir" name tries=0 order
  rm -rf "$dir"
  mkdir -p "$dir"
  for name in a-gone b-asked c-next; do
    git -C "$WT" worktree add -q -b "e69-$name" "$dir/$name"
  done
  # The listing order of this directory's worktrees, as the sidebar and list-workspaces give it.
  order() {
    "$CTL" list-workspaces --json | python3 -I -c '
import json, sys
print(",".join(r["path"][len(sys.argv[1]) + 1:] for r in json.load(sys.stdin) if r["path"].startswith(sys.argv[1] + "/")))' "$dir"
  }
  # A select rescans the project; wait until the three are listed.
  while [ "$(order)" != "a-gone,b-asked,c-next" ] && [ "$tries" -lt 40 ]; do
    tries=$((tries + 1))
    ctl select-workspace --workspace "$WT_ID" >/dev/null
    sleep 0.5
  done
  [ "$(order)" = "a-gone,b-asked,c-next" ] || fail "$CASE: the sidebar lists '$(order)', not a-gone,b-asked,c-next"
  printf 'a\n' >"$dir/b-asked/r.txt"
  start_outside_client "$dir/b-asked"
  wait_client "the outside client connected" 'r.get("event") == "connected"' 1
  open_diff st-1 "$dir/b-asked/r.txt" 'b\n'
  expect_proposal st-1 ready True
  act st-1 close_worktree
  expect_state "the removal asks first" 'd["confirmation"]["then"]' remove_worktree
  # a-gone leaves behind the app's back: unmount it, remove it, rescan until it is unlisted.
  ctl close-workspace --workspace "$dir/a-gone" >/dev/null 2>&1 || true
  git -C "$WT" worktree remove --force "$dir/a-gone"
  git -C "$WT" branch -D e69-a-gone >/dev/null
  tries=0
  while [ "$(order)" != "b-asked,c-next" ] && [ "$tries" -lt 40 ]; do
    tries=$((tries + 1))
    ctl select-workspace --workspace "$WT_ID" >/dev/null
    sleep 0.5
  done
  [ "$(order)" = "b-asked,c-next" ] || fail "$CASE: the rows did not move (listing: $(order))"
  echo "OK: b-asked moved up one place under the question (listing: $(order))"
  expect_state "the question is still open" 'd["confirmation"]["then"]' remove_worktree
  act_alone confirm_reject
  assert_response "$(req_id st-1)" 'd["texts"] == ["DIFF_REJECTED", "st-1"]' "Reject answers DIFF_REJECTED"
  tries=0
  while [ -d "$dir/b-asked" ] && [ "$tries" -lt 150 ]; do tries=$((tries + 1)); sleep 0.2; done
  [ ! -d "$dir/b-asked" ] || fail "$CASE: the worktree the question was about is still on disk"
  [ -d "$dir/c-next" ] || fail "$CASE: c-next was removed in b-asked's place"
  git -C "$WT" worktree list --porcelain | grep -Fxq "worktree $dir/c-next" || fail "$CASE: git lost c-next"
  "$CTL" list-workspaces --json | python3 -I -c '
import json, os, subprocess, sys
listed = subprocess.run(["git", "-C", sys.argv[1], "worktree", "list", "--porcelain"], capture_output=True, text=True).stdout
for row in json.load(sys.stdin):
    if row["path"] == sys.argv[2]:
        continue
    assert os.path.isdir(row["path"]), "removed " + row["path"]
    assert "worktree " + row["path"] in listed, "git lost " + row["path"]' "$WT" "$dir/b-asked" \
    || fail "$CASE: a worktree other than the asked one is gone"
  echo "OK: only b-asked was removed; c-next and every other worktree are intact on disk and in git"
  end_outside_client
  ctl select-workspace --workspace "$WT_ID" >/dev/null
}

# A Close Others question, then another worktree is selected: Reject touches no tab of it.
case_confirm_stable_tabs() {
  local keep before second i
  # Clean file tabs in the other worktree first: they close without asking, so a close that
  # reached them would show. Switching away and back restores each strip with position ids, so a
  # question that held ids would find them in the other worktree.
  second="$(ensure_worktree ide-second)"
  select_new_worktree "$second"
  for i in 1 2 3 4 5 6; do
    printf 'f%s\n' "$i" >"$second/t$i.txt"
    ctl surface editor open --path "$second/t$i.txt" >/dev/null
  done
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  proposal_client
  printf 'one\n' >"$WT/s1.txt"; printf 'f1\n' >"$WT/sf1.txt"; printf 'f2\n' >"$WT/sf2.txt"
  ctl surface editor open --path "$WT/sf1.txt" >/dev/null
  keep="$(expect_tab 'sf1.txt')"
  ctl surface editor open --path "$WT/sf2.txt" >/dev/null
  expect_tab 'sf2.txt' >/dev/null
  open_diff tabs-1 "$WT/s1.txt" 'two\n'
  expect_proposal tabs-1 ready True
  act_alone close_others "$keep"
  expect_state "Close Others asks first" 'd["confirmation"]["then"]' close_tabs
  select_new_worktree "$second"
  before="$(tab_count)"
  act_alone confirm_reject
  sleep 1
  [ "$(tab_count)" = "$before" ] || fail "$CASE: Reject closed tabs of the other worktree ($before then $(tab_count))"
  echo "OK: no tab of the other worktree closed ($before tabs)"
  expect_state "the question is gone and the proposal kept" 'len(d["proposals"])' 1
  assert_unanswered "$(req_id tabs-1)" "nothing was answered for a question about another worktree"
  close_tabs_of_kind file
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  act tabs-1 reject
  assert_response "$(req_id tabs-1)" 'd["texts"][0] == "DIFF_REJECTED"' "rejected"
  close_tabs_of_kind file
  client_do quit
  close_tabs_of_kind terminal
}

# Reject in the confirmation closes the proposal's own tab at once: when the original close is
# then cancelled (a dirty tab among the others asks), no tab nobody answers for is left.
case_reject_leaves_no_inert_tab() {
  proposal_client
  printf 'one\n' >"$WT/id1.txt"; printf 'f\n' >"$WT/idf.txt"; printf 'd\n' >"$WT/idd.txt"
  local keep window
  ctl surface editor open --path "$WT/idf.txt" >/dev/null
  keep="$(expect_tab 'idf.txt')"
  ctl surface editor open --path "$WT/idd.txt" >/dev/null
  expect_tab 'idd.txt' >/dev/null
  sockcall surface.editor.type text=dirty >/dev/null
  open_diff inert-1 "$WT/id1.txt" 'two\n'
  expect_proposal inert-1 ready True
  act_alone close_others "$keep"
  expect_state "Close Others asks first" 'd["confirmation"]["then"]' close_tabs
  act_alone confirm_reject
  assert_response "$(req_id inert-1)" 'd["texts"] == ["DIFF_REJECTED", "inert-1"]' "Reject answers"
  expect_no_tab '✻ id1.txt'
  expect_state "the dirty-tab prompt is up" 'd["prompt_up"]' True
  capture ide-proposal-dirty-prompt
  echo "OK: the proposal's tab is gone while the dirty-tab prompt is still up"
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window for pid $APP_PID to cancel the prompt in"
  # Cancel is the second button of the centred dialog (measured on ide-proposal-dirty-prompt.png).
  local geometry width height
  geometry="$(DISPLAY="$DISPLAY_TARGET" xdotool getwindowgeometry "$window" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')"
  read -r width height <<<"$geometry"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$((width * 50 / 100))" "$((height * 552 / 1000))" click 1
  expect_state "Cancel closed the prompt" 'd["prompt_up"]' False
  [ "$(tab_pos 'idd.txt')" != 0 ] || fail "$CASE: the dirty tab was closed"
  expect_no_proposals
  close_tabs_of_kind file
  end_app_client
}

# Where the keyboard focus is decides what Secondary shows when a proposal arrives (spec 4.7).
# The Primary terminal is the active tab and the user clicks into the file shown in Secondary
# (the click does not change the active tab): the proposal joins the strip, Secondary still shows
# the file, and keys typed afterwards land in its buffer (read from the editor state).
case_focus_in_secondary_keeps_editor() {
  proposal_client
  printf 'one\ntwo\nthree\n' >"$WT/fs.txt"; printf 'p\n' >"$WT/fs2.txt"
  ctl surface editor open --path "$WT/fs.txt" >/dev/null
  local filepos window geometry width height
  filepos="$(expect_tab 'fs.txt')"
  ctl surface tabs select "$CLIENT_TAB" >/dev/null
  [ "$(tabs_field active)" = "$CLIENT_TAB" ] || fail "$CASE: the terminal is not the active tab"
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window for pid $APP_PID"
  geometry="$(DISPLAY="$DISPLAY_TARGET" xdotool getwindowgeometry "$window" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')"
  read -r width height <<<"$geometry"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$((width * 62 / 100))" "$((height * 18 / 100))" click 1
  sleep 0.8
  [ "$(tabs_field active)" = "$CLIENT_TAB" ] || fail "$CASE: the click made another tab the active one; the layout under test is gone"
  expect_state "the keyboard focus is in Secondary" 'd["secondary_focused"]' True
  expect_state "Secondary shows the file" 'd["secondary_shown"]' "$filepos"
  open_diff fs-1 "$WT/fs2.txt" 'x\n'
  expect_proposal fs-1 on_screen True
  expect_tab '✻ fs2.txt' >/dev/null
  expect_state "Secondary still shows the file, not the proposal" 'd["secondary_shown"]' "$filepos"
  DISPLAY="$DISPLAY_TARGET" xdotool type --delay 60 "KEYS"
  sleep 0.5
  # Only now make the file the active tab, to read its buffer through the editor state.
  ctl surface tabs select "$filepos" >/dev/null
  case "$("$CTL" surface editor state --json | jq_py 'd[0].get("text", "")')" in
    *KEYS*) ;;
    *) fail "$CASE: the typed keys did not reach the file's buffer ($("$CTL" surface editor state --json | jq_py 'd[0].get("text", "")' | head -c 120))" ;;
  esac
  echo "OK: the keys typed after the proposal opened landed in the file's buffer"
  act fs-1 reject
  assert_response "$(req_id fs-1)" 'd["texts"][0] == "DIFF_REJECTED"' "rejected"
  close_tabs_of_kind file
  end_app_client
}

# The other direction: a file is shown in Secondary and the user clicks into the Primary terminal:
# the proposal becomes what Secondary shows.
case_focus_in_primary_shows_proposal() {
  proposal_client
  printf 'one\n' >"$WT/fp.txt"; printf 'p\n' >"$WT/fp2.txt"
  ctl surface editor open --path "$WT/fp.txt" >/dev/null
  expect_tab 'fp.txt' >/dev/null
  local window geometry width height pos
  window="$(find_window)"
  [ -n "$window" ] || fail "$CASE: no window for pid $APP_PID"
  geometry="$(DISPLAY="$DISPLAY_TARGET" xdotool getwindowgeometry "$window" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')"
  read -r width height <<<"$geometry"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$((width * 35 / 100))" "$((height * 45 / 100))" click 1
  sleep 0.8
  expect_state "the keyboard focus left Secondary" 'd["secondary_focused"]' False
  open_diff fp-1 "$WT/fp2.txt" 'x\n'
  expect_proposal fp-1 on_screen True
  pos="$(expect_tab '✻ fp2.txt')"
  expect_state "Secondary shows the proposal" 'd["secondary_shown"]' "$pos"
  act fp-1 reject
  assert_response "$(req_id fp-1)" 'd["texts"][0] == "DIFF_REJECTED"' "rejected"
  close_tabs_of_kind file
  end_app_client
}

# A symlink CLAUDE.md -> AGENTS.md shows as the name Claude sent.
case_symlink_title() {
  proposal_client
  printf 'x\n' >"$WT/AGENTS.md"
  rm -f "$WT/CLAUDE.md"
  ln -s AGENTS.md "$WT/CLAUDE.md"
  open_diff link-1 "$WT/CLAUDE.md" 'y\n'
  expect_tab '✻ CLAUDE.md' >/dev/null
  echo "OK: the tab is titled ✻ CLAUDE.md"
  act link-1 reject
  assert_response "$(req_id link-1)" 'd["texts"][0] == "DIFF_REJECTED"' "rejected"
  rm -f "$WT/CLAUDE.md"
  end_app_client
}

case_close_tab_withdraws() {
  proposal_client
  printf 'one\n' >"$WT/w1.txt"
  open_diff withdraw-1 "$WT/w1.txt" 'two\n'
  expect_proposal withdraw-1 ready True
  client_do 'call tools/call {"name":"close_tab","arguments":{"tab_name":"withdraw-1"}}'
  wait_client "close_tab answered" 'r.get("dir") == "in" and r.get("frame", {}).get("result", {}).get("content", [{}])[0].get("text") == "TAB_CLOSED"' 1
  assert_response "$(req_id withdraw-1)" 'd == {"error": -32000}' "the withdrawn openDiff gets -32000, no decision"
  expect_no_proposals
  expect_no_tab '✻ w1.txt'
  end_app_client
}

case_close_all() {
  proposal_client
  printf 'one\n' >"$WT/ca1.txt"
  printf 'one\n' >"$WT/ca2.txt"
  open_diff all-1 "$WT/ca1.txt" 'two\n'
  open_diff all-2 "$WT/ca2.txt" 'two\n'
  expect_state "two proposals" 'len(d["proposals"])' 2
  client_do 'call tools/call {"name":"closeAllDiffTabs","arguments":{}}'
  wait_client "closeAllDiffTabs answered" 'r.get("dir") == "in" and r.get("frame", {}).get("result", {}).get("content", [{}])[0].get("text") == "CLOSED_2_DIFF_TABS"' 1
  assert_response "$(req_id all-1)" 'd == {"error": -32000}' "the first gets -32000"
  assert_response "$(req_id all-2)" 'd == {"error": -32000}' "the second gets -32000"
  expect_no_proposals
  expect_no_tab '✻ ca1.txt'
  expect_no_tab '✻ ca2.txt'
  end_app_client
}

case_replace_in_place() {
  proposal_client
  printf 'one\n' >"$WT/rp.txt"
  open_diff same-1 "$WT/rp.txt" 'two\n'
  expect_proposal same-1 ready True
  local tabs
  tabs="$(tab_count)"
  open_diff same-1 "$WT/rp.txt" 'three\n'
  # The first request is superseded: the server answers it with an error.
  local first
  first="$(python3 -I - "$CLIENT_LOG" <<'PY'
import json, sys
ids = []
for line in open(sys.argv[1], encoding="utf-8"):
    r = json.loads(line)
    f = r.get("frame", {})
    if r.get("dir") == "out" and f.get("method") == "tools/call" and f["params"].get("name") == "openDiff":
        ids.append(f["id"])
print(ids[-2])
PY
)"
  assert_response "$first" 'd == {"error": -32000}' "the replaced openDiff gets -32000, no decision"
  sleep 1
  [ "$(tab_count)" = "$tabs" ] || fail "$CASE: the replacement made another tab ($tabs then $(tab_count))"
  expect_state "still one proposal" 'len(d["proposals"])' 1
  expect_proposal same-1 ready True
  act same-1 accept
  assert_response "$(req_id same-1)" 'd["texts"] == ["FILE_SAVED", "three\n"]' "the replacement is what Accept answers"
  expect_no_proposals
  end_app_client
}

case_disconnect_closes() {
  proposal_client
  printf 'one\n' >"$WT/d.txt"
  local before
  before="$(toast_count 'Claude closed its proposed change')"
  open_diff gone-1 "$WT/d.txt" 'two\n'
  expect_proposal gone-1 ready True
  client_do close
  wait_client "the connection closed" 'r.get("event") == "closed"' 1
  expect_no_proposals
  expect_no_tab '✻ d.txt'
  expect_state "the toast says Claude closed it" 'len([t for t in d["toasts"] if t == "Claude closed its proposed change"])' "$((before + 1))"
  assert_no_decision "$(req_id gone-1)" "a closed connection is answered nothing"
  end_app_client
}

case_new_file() {
  proposal_client
  rm -f "$WT/brand-new.txt"
  open_diff new-1 "$WT/brand-new.txt" 'first\nsecond\n'
  expect_proposal new-1 ready True
  expect_proposal new-1 refusal False
  expect_proposal new-1 counts "[2, 0]"
  act new-1 reject
  assert_response "$(req_id new-1)" 'd["texts"][0] == "DIFF_REJECTED"' "rejected"
  [ ! -e "$WT/brand-new.txt" ] || fail "$CASE: Sirio created the file"
  end_app_client
}

case_fifo_path() {
  proposal_client
  rm -f "$WT/fifo.txt"
  mkfifo "$WT/fifo.txt"
  open_diff fifo-1 "$WT/fifo.txt" 'text\n'
  expect_proposal fifo-1 refusal True
  expect_tab '✻ fifo.txt' >/dev/null
  capture ide-proposal-fifo
  if sockcall surface.ide.act tab_name=fifo-1 action=edit "text=x" >/dev/null 2>&1; then
    fail "$CASE: act edit answered ok on a refused proposal"
  fi
  echo "OK: Edit is not available on a refused proposal (act edit fails)"
  act fifo-1 reject
  assert_response "$(req_id fifo-1)" 'd["texts"][0] == "DIFF_REJECTED"' "a refused proposal can still be rejected"
  rm -f "$WT/fifo.txt"
  end_app_client
}

case_too_large() {
  proposal_client
  python3 -I -c 'import sys; open(sys.argv[1], "w").write("x" * 1100000 + "\n")' "$WT/big.txt"
  open_diff big-1 "$WT/big.txt" 'small\n'
  expect_proposal big-1 refusal True
  expect_tab '✻ big.txt' >/dev/null
  capture ide-proposal-too-large
  if sockcall surface.ide.act tab_name=big-1 action=edit "text=x" >/dev/null 2>&1; then
    fail "$CASE: act edit answered ok on an over-cap proposal"
  fi
  act big-1 accept
  assert_response "$(req_id big-1)" 'd["texts"] == ["FILE_SAVED", "small\n"]' "Accept sends Claude's text unchanged"
  rm -f "$WT/big.txt"
  end_app_client
}

# The bar warns while the file has unsaved edits in an editor tab, and follows them live.
case_dirty_warning() {
  proposal_client
  printf 'one\ntwo\n' >"$WT/dirty.txt"
  ctl surface editor open --path "$WT/dirty.txt" >/dev/null
  local file_pos
  file_pos="$(expect_tab 'dirty.txt')"
  open_diff dirty-1 "$WT/dirty.txt" 'one\nTWO\n'
  expect_proposal dirty-1 ready True
  expect_proposal dirty-1 dirty False
  ctl surface tabs select "$file_pos" >/dev/null
  sockcall surface.editor.type text=typed >/dev/null
  expect_proposal dirty-1 dirty True
  ctl surface tabs select "$(tab_pos '✻ dirty.txt')" >/dev/null
  capture ide-proposal-dirty
  act dirty-1 reject
  assert_response "$(req_id dirty-1)" 'd["texts"][0] == "DIFF_REJECTED"' "rejected"
  expect_no_proposals
  ctl surface tabs select "$(tab_pos 'dirty.txt')" >/dev/null
  close_tabs_of_kind file
  end_app_client
}

case_proposal_captures() {
  proposal_client
  printf 'alpha\nbeta\ngamma\ndelta\n' >"$WT/cap.txt"
  open_diff cap-1 "$WT/cap.txt" 'alpha\nBETA\ngamma\nDELTA\nepsilon\n'
  expect_proposal cap-1 ready True
  ctl surface tabs select "$(expect_tab '✻ cap.txt')" >/dev/null
  sockcall surface.changes.view mode=unified >/dev/null
  capture ide-proposal-unified
  sockcall surface.changes.view mode=split >/dev/null
  capture ide-proposal-split
  sockcall surface.changes.view mode=unified >/dev/null
  act cap-1 reject
  assert_response "$(req_id cap-1)" 'd["texts"][0] == "DIFF_REJECTED"' "rejected"
  end_app_client
}

# With three openDiffs pending, each answered a different way by something other than the
# user, no decision is ever sent: switching the bridge off, quitting, and the client dropping
# its socket.
case_no_decision_without_user() {
  local req
  printf 'one\n' >"$WT/nd.txt"
  # The switch goes off.
  proposal_client
  open_diff nd-1 "$WT/nd.txt" 'two\n'
  expect_proposal nd-1 ready True
  req="$(req_id nd-1)"
  sockcall surface.ide.switch on=false >/dev/null
  wait_client "the connection closed" 'r.get("event") == "closed"' 1
  expect_no_proposals
  expect_no_tab '✻ nd.txt'
  assert_no_decision "$req" "switching off sends no decision"
  sockcall surface.ide.switch on=true >/dev/null
  wait_lockfile >/dev/null
  end_app_client
  # The client drops its socket.
  proposal_client
  open_diff nd-2 "$WT/nd.txt" 'two\n'
  expect_proposal nd-2 ready True
  req="$(req_id nd-2)"
  client_do close
  wait_client "the connection closed" 'r.get("event") == "closed"' 1
  expect_no_proposals
  assert_no_decision "$req" "a dropped socket is answered no decision"
  end_app_client
  # Sirio quits.
  proposal_client
  open_diff nd-3 "$WT/nd.txt" 'two\n'
  expect_proposal nd-3 ready True
  req="$(req_id nd-3)"
  app_quit
  wait_client "the connection closed" 'r.get("event") == "closed"' 1
  assert_no_decision "$req" "quitting sends no decision"
  app_start
  select_repo
  WT_ID="$(worktree_id "$WT")"
  wait_lockfile >/dev/null
  client_do quit
}

# A proposal for a worktree that is not on screen is held without a tab, announced by a toast,
# comes back with its worktree, and survives that worktree's eviction. Run on a relaunch with the
# mounted-worktree cap seeded at 2 in the isolated database. A = ide-second, B = the repository
# (where Claude runs), C = ide-third; the eviction takes the first idle non-selected worktree in
# catalog order, which is B once A, B and C are mounted.
case_parked_worktree() {
  mkdir -p -m 700 "$RUN_DIR/ide-proposals"
  printf 'left behind by a crash' >"$RUN_DIR/ide-proposals/stale-proposal-file"
  app_quit
  "$SEED" --database "$SIRIO_DB" --appearance dark --limit-mounted 2
  app_start
  select_repo
  WT_ID="$(worktree_id "$WT")"
  [ ! -e "$RUN_DIR/ide-proposals/stale-proposal-file" ] || fail "$CASE: the scratch directory was not swept at launch"
  [ "$(stat -c %a "$RUN_DIR/ide-proposals")" = 700 ] || fail "$CASE: the scratch directory is not private"
  echo "OK: the scratch directory was swept at launch and is private"
  wait_lockfile >/dev/null
  local a c a_id c_id
  a="$(ensure_worktree ide-second)"
  c="$(ensure_worktree ide-third)"
  a_id="$(worktree_id "$a")"
  c_id="$(worktree_id "$c")"
  mounted() { # worktree id
    "$CTL" list-workspaces --json | python3 -I -c '
import json, sys
print([r["mounted"] for r in json.load(sys.stdin) if r["id"] == sys.argv[1]][0])' "$1"
  }
  printf 'b\n' >"$WT/parked.txt"
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  proposal_client
  ctl select-workspace --workspace "$a_id" >/dev/null
  open_diff parked-1 "$WT/parked.txt" 'B\n'
  expect_proposal parked-1 on_screen False
  expect_proposal parked-1 owner_named True
  expect_state "the toast names the worktree" '"Claude proposes a change in main" in d["toasts"]' True
  [ "$(tab_pos '✻ parked.txt')" = 0 ] || fail "$CASE: a proposal for a worktree that is not on screen has a tab"
  echo "OK: held without a tab, announced by a toast"
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  expect_proposal parked-1 on_screen True
  expect_proposal parked-1 owner_named True
  expect_tab '✻ parked.txt' >/dev/null
  echo "OK: selecting the worktree brings its tab"
  ctl select-workspace --workspace "$a_id" >/dev/null
  ctl select-workspace --workspace "$c_id" >/dev/null
  expect_state "B was evicted" 'len(d["proposals"])' 1
  [ "$(mounted "$WT_ID")" = false ] || fail "$CASE: B is still mounted, so no eviction happened and the case proves nothing"
  echo "OK: B was evicted (mounted $(mounted "$WT_ID"))"
  expect_proposal parked-1 on_screen False
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  expect_proposal parked-1 on_screen True
  expect_tab '✻ parked.txt' >/dev/null
  echo "OK: the proposal outlived its worktree's eviction"
  act parked-1 accept
  assert_response "$(req_id parked-1)" 'd["texts"] == ["FILE_SAVED", "B\n"]' "Accept answers after the eviction"
  expect_no_proposals
  client_do quit
  close_tabs_of_kind terminal
  # A Claude outside every Sirio shell (no owner): its proposal for a file of a worktree that is
  # not on screen is held under the file's worktree, and shows when that worktree is selected.
  ctl select-workspace --workspace "$a_id" >/dev/null
  start_outside_client "$WT"
  wait_client "the outside client connected" 'r.get("event") == "connected"' 1
  printf 'o\n' >"$WT/own.txt"
  open_diff own-1 "$WT/own.txt" 'O\n'
  expect_proposal own-1 on_screen False
  [ "$(tab_pos '✻ own.txt')" = 0 ] || fail "$CASE: an ownerless proposal for a worktree not on screen has a tab"
  ctl select-workspace --workspace "$WT_ID" >/dev/null
  expect_proposal own-1 on_screen True
  expect_tab '✻ own.txt' >/dev/null
  echo "OK: the ownerless proposal was held under the file's worktree and came back with it"
  act own-1 accept
  assert_response "$(req_id own-1)" 'd["texts"] == ["FILE_SAVED", "O\n"]' "Accept answers the ownerless proposal"
  end_outside_client
  # The closing check wants the fake language server of this launch running: open its file.
  [ -f "$WT/a.py" ] || write_diag_file "$WT/a.py"
  ctl surface editor open --path "$WT/a.py" >/dev/null
  expect_problems 0/1/0
}

run_proposal_cases() {
  case_run accept case_proposal_accept
  case_run edit_then_accept case_proposal_edit_then_accept
  case_run edit_keys case_proposal_edit_keys
  case_run reject case_proposal_reject
  case_run reject_with_comments_clipboard case_reject_with_comments_clipboard
  case_run comment_composer_anchor case_comment_composer_anchor
  case_run comment_refusals case_comment_refusals
  case_run composer_typing_keeps_line case_composer_typing_keeps_line
  case_run comments_on_withdraw case_comments_on_withdraw
  case_run comments_on_close_all case_comments_on_close_all
  case_run comments_on_replace case_comments_on_replace
  case_run comments_on_accept case_comments_on_accept
  case_run comments_on_confirmation_reject case_comments_on_confirmation_reject
  case_run comments_on_disconnect case_comments_on_disconnect
  case_run close_confirm case_proposal_close_confirm
  case_run close_others_guarded case_close_others_guarded
  case_run reject_leaves_no_inert_tab case_reject_leaves_no_inert_tab
  case_run confirm_stable_tabs case_confirm_stable_tabs
  case_run close_tab_withdraws case_close_tab_withdraws
  case_run close_all case_close_all
  case_run replace_in_place case_replace_in_place
  case_run disconnect_closes case_disconnect_closes
  case_run new_file case_new_file
  case_run fifo_path case_fifo_path
  case_run too_large case_too_large
  case_run dirty_warning case_dirty_warning
  case_run captures case_proposal_captures
  case_run no_decision_without_user case_no_decision_without_user
  case_run worktree_close_guarded case_worktree_close_guarded
  case_run project_close_guarded case_project_close_guarded
  case_run confirm_stable_worktree case_confirm_stable_worktree
  case_run focus_in_secondary_keeps_editor case_focus_in_secondary_keeps_editor
  case_run focus_in_primary_shows_proposal case_focus_in_primary_shows_proposal
  case_run symlink_title case_symlink_title
  case_run parked_worktree case_parked_worktree
}

app_start
select_repo
WT_ID="$(worktree_id "$WT")"
[ -n "$WT_ID" ] || fail "sirioctl does not list the worktree $WT"
echo "OK: the repository is the selected workspace ($WT_ID)"

if [ "$PROPOSALS_ONLY" -eq 1 ]; then
  wait_lockfile >/dev/null
  ensure_worktree ide-second >/dev/null
  ensure_worktree ide-third >/dev/null
  run_proposal_cases
  stop_hosts
  echo "IDE BRIDGE PROPOSALS ONLY ($CASES_PASSED cases, state-only $STATE_ONLY)"
  exit 0
fi

case_run lockfile_and_env case_lockfile_and_env
case_run token_refused case_token_refused
case_run handshake case_handshake
case_run switch_off case_switch_off
case_run switch_flap case_switch_flap
case_run port_survives_catalog_change case_port_survives_catalog_change
case_run restart_keeps_port case_restart_keeps_port
case_run inherited_port case_inherited_port
case_run owner_resolved case_owner_resolved
case_run selection_utf16 case_selection_utf16
case_run selection_coalesced case_selection_coalesced
case_run focus_sends_selection case_focus_sends_selection
case_run at_mention case_at_mention
case_run selection_while_terminal_active case_selection_while_terminal_active
case_run at_mention_to_new_terminal case_at_mention_to_new_terminal
case_run selection_not_lost_to_another_worktree case_selection_not_lost_to_another_worktree
case_run no_eligible_agent_toast case_no_eligible_agent_toast
case_run at_mention_without_focus_history case_at_mention_without_focus_history
case_run owner_after_restart case_owner_after_restart
case_run at_mention_after_relaunch case_at_mention_after_relaunch
case_run diagnostics_file case_diagnostics_file
case_run diagnostics_outside_worktree case_diagnostics_outside_worktree
case_run diagnostics_workspace case_diagnostics_workspace
case_run diagnostics_ownerless_cwd case_diagnostics_ownerless_cwd
case_run diagnostics_ownerless_elsewhere case_diagnostics_ownerless_elsewhere
run_proposal_cases
assert_only_the_fake_server

stop_hosts
echo "IDE BRIDGE E2E OK ($CASES_PASSED cases, state-only $STATE_ONLY)"
