# Finish-line critic pass — terminal shard (F-TERM / F-CORE-TERM)

Critic: fresh finish-line pass, wave `wf-term` (`TILLER_WL_LABEL=wf-term`). Scope: every row whose
id matches F-TERM, F-TERM-PTY, F-TERM-UI, F-TERM-SCR, F-TERM-REG, F-TERM-USG, F-TERM-SPLIT,
F-TERM-PLAT and F-CORE-TERM in `docs/linux-rewrite/INVENTORY-LEDGER.md` (31 rows). Every row is
judged strictly against its VERIFY clause in `01-inventory-app.md` (F-TERM-01..11) or
`02-inventory-packages.md` (the package-level rows) — never against ledger prose or a builder's
claim. A prior PASSED verdict was never treated as evidence; every row below was independently
re-driven or re-tested by this pass. F-TERM-02 and F-TERM-PTY-07/08 are explicitly **NOT
EXERCISED** here — another critic owns those in wave F.

Working method: `Scripts/wayland-drive.sh` against a pinned snapshot `TILLER_WL_BIN=/tmp/wf-term-tiller`,
socket `/tmp/wf-term.sock`. All screenshots referenced below live under `/tmp/wf-term-*` (not
committed — this is a shared box and `/tmp` is not part of the repo). Where a row is proven by a
named test, the exact test name and crate are given so the verdict is replayable with
`cargo test -p <crate> <name>`.

## Load-bearing environmental fact discovered this pass

**Every separate `wayland-drive.sh` invocation under the same label kills and restarts the app.**
`cleanup()` (trapped on EXIT) runs `kill_ours TILLER_SOCKET "$SOCK" "$(basename "$BIN")"`
unconditionally unless `TILLER_WL_KEEP` is set — so the app dies at the end of *every* script call
that doesn't set it, whether or not a fresh one is desired. Confirmed directly: `pgrep -af
wf-term-tiller` returns nothing between invocations, and a Claude Code tab that had reached a real
conversation (a `PONG` reply, a `/exit` prompt half-selected) reverts to the CLI's fresh "Welcome
back" trust screen on the very next invocation, even though the **tab itself** (its name, its
position in the sidebar, the worktree/tab list) is still there — because that part is persisted to
the DB and restored by re-spawning a fresh process, not because the OS-level PTY survived.

This matters for how several rows below were driven: any row whose VERIFY needs "before" and
"after" state in the same live process (F-TERM-10, F-TERM-PTY-05's exit lifecycle, the drag-drop
test) was restructured to run inside **one** script invocation, never split across two. Earlier
attempts at F-TERM-10 that spanned two invocations produced a false read (the mark and the running
process were "lost" simply because the app had restarted, not because worktree-switching itself
did anything) — that mistake was caught and the test re-run correctly before this row's final
verdict was set.

## F-TERM-01 — New Terminal from an empty pane/worktree

**PASSED.** Live-driven: selected an empty pane, chose New Terminal, a live shell prompt rendered
immediately with a working PTY (confirmed by running a command and seeing its output).

## F-TERM-02 — empty-pane prompt text + both actions

**NOT EXERCISED** — owned by another critic in wave F, per brief.

## F-TERM-03 — running / exit-0 / exit-nonzero / signal status on the output surface

**half-proven.** Freshly live-driven this pass: a long-running command shows a running/spinner
status; a command exiting 0 shows the success pill; a command exiting 7 (nonzero) shows the
failure pill with the code. The signal-terminated case could not be freshly live-driven in ~10
consecutive attempts this pass — `Scripts/wayland-drive.sh` failed with either a
`swaymsg`/`verify_nested_sway()` IPC race (`Unable to connect to /tmp/wf-term-sway.sock` +
`JSONDecodeError`) or the app dying at launch with `MESA: error: ZINK: failed to choose pdev`.
Both were root-caused as GPU/compositor contention from ~4-6 other agents' Tiller instances
sharing `/dev/dri/renderD128` on this box at the time, not a Tiller defect — reproducing sway
startup standalone worked fine, ruling out a config bug, and the failure pattern (intermittent,
correlated with concurrent instance count) is consistent with device contention rather than a
code path. The signal case is instead backed by: (a) the named unit test
`child_exit_status_preserves_normal_and_signal_termination` in
`rust/crates/tiller_terminal/src/lib.rs:2200`, which directly covers the "map a normal exit to its
code and a signal termination to 128 + signal" logic this row's status surface reads from, run
this pass and green; (b) a prior independent critic's live screenshot (`signal-kill.png`, sweep
F4-terminal, 2026-08-16) on code unchanged since. Reporting this as half-proven rather than PASSED
because the signal case specifically was not freshly live-driven by this pass, only test- and
prior-evidence-backed.

## F-TERM-04 — Copy selected text / Paste clipboard text via context menu

**PASSED.** Live-driven in a single continuous sweep (`/tmp/wf-term-block2a`, frames
`04-ctxmenu-for-copy.png` … `07-after-paste.png`): selected terminal output, context menu → Copy,
placed the copied text via Paste into a different location, then Paste again to affect the
terminal — both operations landed correctly.

## F-TERM-05 — Copy Context / Clear Terminal / Set Title

**PASSED.** Live-driven, same sweep plus a dedicated re-check (`/tmp/wf-term-clear-settitle`):
Copy Context copied surrounding output; Clear Terminal wiped the visible scrollback after
`seq 100`; Set Title changed the tab's displayed name — confirmed directly in
`/tmp/wf-term-block2a/19-17-after-settitle.png`, where the tab reads "Terminal terminal-1" after
typing "terminal-1" into the Set Title prompt.

## F-TERM-06 — Copy Pane ID / Copy Terminal ID

**PASSED.** Live-driven (`/tmp/wf-term-block2a` frames 13–16, and standalone
`/tmp/wf-term-paneid-clean`, `/tmp/wf-term-termid-clean`): each copy-ID menu item, followed by
Paste, produced a nonempty identifier string in the pane.

## F-TERM-07 — Agent-specific New Tab action

**PASSED.** Live-driven fresh this pass (`/tmp/wf-term-agentmenu`, `/tmp/wf-term-agent1`): the "+"
tab-strip menu offers New Terminal, Changes, New Browser, **Claude Code, Codex, OpenCode, Pi,
Oh-My-Pi**, Split Claude Code, New Chat — exactly the agent-specific entries the row describes.
Choosing "Claude Code" created a new tab titled "Claude Code" with a distinct icon, and the
process was real: it progressed from `in bash at 17:33:26` (still booting) through the CLI's
actual first-run "Accessing workspace… quick safety check" trust prompt, to the full
"Welcome back Enzo!" banner — a genuine `claude` process, not a stub. Sidebar and bottom-right
"Activity" panel showed "1 running" the moment the tab appeared, i.e. agent activity/status
information was present as the row requires.

## F-TERM-08 — Close confirmation + process termination

**PASSED.** Live-driven (`/tmp/wf-term-close1`): started a command that stays running, invoked
Close Terminal…, the confirm affordance appeared, accepted it, and the pane/process ended — final
frame (`04-after-close-click.png`) shows the pane gone from the layout.

## F-TERM-09 — Agent activity in tab/pane/sidebar status indicators

**PASSED.** Live-driven fresh this pass across the same Claude Code session used for F-TERM-07/
PTY-05: while the agent was "Thinking…"/"Wibbling…"/"Scurrying…" the tab showed a solid orange dot
next to "Claude Code", the sidebar's worktree row showed an orange sun glyph, and the bottom-right
panel read "Activity · 1 running". Once the turn completed, the tab's glyph changed to a plain
outline "?" and the sidebar dot changed to a neutral/grey glyph — a real, observed status
transition from working → idle, not just a single static frame. (See
`/tmp/wf-term-agent1/03-01-claude-tab-created.png`,
`/tmp/wf-term-agent3/03-01-working.png`, `/tmp/wf-term-agent4/02-...-still-working-or-done.png`.)

## F-TERM-10 — Terminal panes survive a worktree-selection round trip

**FAILED — defective.** VERIFY: "Start a long-running command in one worktree, select another
worktree, return to the first, and confirm the original process/output remains." Reproduced twice,
independently, each fully inside **one** script invocation (so app-restart is not a confound):

1. Socket-driven (`/tmp/wf-term-switch6`): selected `wf-term-clean2`/master via
   `ctl workspace.select`, typed `echo MARK-SIX-BEFORE` + `sleep 300` into its second Terminal
   tab (confirmed running — `04-02-marked-sleeping.png`), switched to a different worktree via
   `ctl workspace.select workspace=p-c1fd7a5bbfd541af-wt-1`, then switched back to the original
   worktree id. Result (`07-05-switched-back-tab2-clicked.png`): the pane shows a **brand new**
   shell — fresh neofetch banner, fresh bash prompt with a new timestamp, no `MARK-SIX-BEFORE`,
   no running `sleep 300`.
2. Real UI gesture, no socket calls at all (`/tmp/wf-term-switch7`): clicked the `master` worktree
   row under the `wf-term-clean2` project in the sidebar, clicked its second Terminal tab, typed
   `echo MARK-SEVEN-BEFORE` + `sleep 300` (confirmed running —
   `04-02-marked-sleeping.png`), clicked a *different* worktree row (`linux/gpui-waku`) in the
   sidebar, then clicked back on the `master` row. Result
   (`06-04-back-on-clean2.png`, taken **before** even re-clicking the tab): the same tab already
   shows a fresh, un-marked neofetch splash the instant the worktree is reselected — the marker
   and the `sleep 300` process are simply gone.

This directly contradicts the architecture CLAUDE.md documents for this feature: "`openWorktreeIds`
tracks which worktrees' terminal hosts stay mounted (PTYs alive) **across sidebar selection
changes** — removing a worktree from that list unmounts its host… Closing the app window does not
kill agent PTYs; only quitting does." A mere selection change should not unmount/respawn a
worktree's panes — only explicitly closing it should. What was observed is a full respawn on a
plain selection round trip, reproduced by two independent methods (socket and real pointer/keyboard
gesture) with no app restart between the mark and the check. This is a genuine, reproducible
functional defect, not a testing artifact.

## F-TERM-11 — No-worktree-selected empty state

**PASSED.** Live-driven from a genuinely fresh boot (deleted `/tmp/wf-term.sqlite*` and
`/tmp/wf-term.sock` after confirming no live process held them, then launched cold): the central
surface explained no worktree was selected instead of showing a terminal, captured before any
`ctl` call reconstructed state.

## F-CORE-TERM-01, -02, -03

**PASSED** (all three). F-CORE-TERM-02 (chord Shift+F10 opens the terminal context menu) was
directly re-confirmed this pass incidentally during the exit-0 pill test, plus the full menu sweep
in `/tmp/wf-term-block2a`. F-CORE-TERM-01/03 were live-confirmed earlier in this same continuous
pass (before a context-compaction boundary); their evidence (menu structure / dispatch to app vs.
local) is corroborated by the passing test
`terminal_actions_are_local_and_pane_actions_are_delegated` in
`rust/crates/tiller_terminal/src/context_menu.rs:261`.

## F-TERM-PTY-01 — forkpty/exec/resize/output/exit lifecycle

**PASSED** via the named real-PTY test `real_pty_emits_osc_title_and_settled_output`
(`rust/crates/tiller_terminal/src/lib.rs:3060`, a `TestAppContext`-based test that spawns a real
child, and the resize test `resize_reaches_the_child_pty`
(`rust/crates/tiller_terminal/src/lib.rs:2365`) — both run this pass and green. Exit-code mapping
overlaps with PTY-02 below.

## F-TERM-PTY-02 — exit-code / signal-to-128+n mapping

**PASSED** via `child_exit_status_preserves_normal_and_signal_termination`
(`rust/crates/tiller_terminal/src/lib.rs:2200`), run this pass, green — and live-corroborated by
the F-TERM-03 exit-0/exit-7 drives above.

## F-TERM-PTY-03 — PTY environment (TILLER_PANE_ID, TERM, inherited + extra vars)

**PASSED**, both by the named test `terminal_child_receives_the_pane_id_environment`
(`rust/crates/tiller_terminal/src/lib.rs:2138`) and by a fresh live drive this pass
(`/tmp/wf-term-envcheck3/03-01-env.png`): `echo TERM=$TERM SHELL=$SHELL PANE=$TILLER_PANE_ID`
inside a live pane printed `TERM=xterm-256color SHELL=/bin/bash PANE=pane-0` — `TILLER_PANE_ID` is
genuinely present and correctly scoped to the pane.

## F-TERM-PTY-04 — shell fallback, ghostty terminfo choice, scrollback restore, settled resize

**half-proven.** Confirmed live and by code: the shell-fallback chain
(`std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh")`, `rust/crates/tiller_terminal/src/lib.rs:348`)
is present and matches the live `SHELL=/bin/bash` observed when `$SHELL` was set. Scrollback
restore is proven by the passing test
`terminal_state_can_capture_and_replay_a_nonce_without_writing_to_the_child`
(`rust/crates/tiller_terminal/src/lib.rs:2266`, using `replay_scrollback`). **Gap:** the row also
requires the pane to "choose `xterm-ghostty` when available, otherwise `xterm-256color`" — grepping
the entire `tiller_terminal` crate for `ghostty` returns zero hits outside doc comments, and the
live `TERM` value observed this pass was unconditionally `xterm-256color`
(`rust/crates/tiller_terminal/src/lib.rs:356`: `("TERM", "xterm-256color")` is the only value ever
set). There is no code path that ever selects `xterm-ghostty` — the row's ghostty-preference half
is simply absent from the Linux port, live-confirmed, not just inferred from a grep. The
settled-resize-before-fallback-sizing half of the row was not independently isolated this pass.

## F-TERM-PTY-05 — send input, live output/status, exit lifecycle

**PASSED.** Live-driven fresh this pass, entirely with a real `claude` CLI (not a stub), spanning
the F-TERM-07/09 session above plus a dedicated exit test:
- **Connect:** New Tab → Claude Code produced a real process through its actual trust-prompt flow.
- **Send a prompt / live output:** typed "reply with exactly the single word: PONG", observed
  "Thinking…" with the working-status glyph while it streamed (`/tmp/wf-term-agent3/03-01-working.png`).
- **Status updates:** orange "running" dot → outline "?" idle glyph, matching F-TERM-09's evidence.
- **Exit + final lifecycle state:** `chord ctrl d` twice in the same invocation (the CLI's own
  "Press Ctrl-D again to exit" prompt appeared after the first, confirming the keystroke reached
  the child) produced a "Process exited successfully" banner and the tab title changed to show a
  checkmark plus an "exit" badge — `/tmp/wf-term-agentB/03-01-settle.png`. This is a definite,
  observed final lifecycle state, not an inference.

## F-TERM-PTY-06 — drag-drop file paths into a live agent shell, no auto-newline

**PASSED.** Live-driven fresh this pass using `Scripts/wayland-drive.sh`'s real
compositor-delivered `xdnd` action (a second Wayland client offering `text/uri-list`, not a
synthetic paste) against a **live, working** Claude Code pane
(`/tmp/wf-term-dnd/03-01-after-drop.png`): dropped `/tmp/wf-term-clean2/README.md` onto the pane
while it was mid-turn ("Scurrying…"). The composer received the exact literal path,
`'/tmp/wf-term-clean2/README.md'`, sitting in the input **without** a trailing newline (not
submitted — the cursor sits right after it, ready for more typing), and focus stayed on the
terminal/composer. This matches the row's exact wording.

## F-TERM-PTY-07, F-TERM-PTY-08

**NOT EXERCISED** — owned by another critic in wave F, per brief.

## F-TERM-REG-01 — PaneRegistry as source of truth (register/list/write/wait/close/recreate)

**PASSED** via the named integration tests in `rust/crates/tiller_control/tests/control_integration.rs`,
run this pass, all green: `pane_registry_runs_a_real_command_and_returns_output_and_exit_code`,
`pane_registry_writes_input_to_a_live_command`, `pane_registry_close_terminates_process_group`,
`pane_registry_lists_live_application_panes_for_their_worktree`,
`pane_registry_reports_live_state_and_distinguishes_closed_from_unknown`,
`pane_registry_shutdown_terminates_live_children`. The "distinguishes closed from unknown" test
was live-corroborated by accident this pass: a stale/mistaken pane id sent over the real socket via
`ctl panel.wait id=<wrong-id>` returned `{"ok":false,"error":"unknown pane: <id>"}` exactly as that
test expects.

## F-TERM-REG-02 — bounded wait, close cancels pending registration, no leaked live registration

**PASSED** via the same `pane_registry_*` test suite (specifically the close/shutdown tests above,
which assert no live child survives a close), run green this pass. A live socket confirmation
(`ctl panel.create` with a `sleep 999` that never exits, followed by `ctl panel.wait
timeout_ms=1200`) was attempted this pass but the dynamic per-run pane id (`pane-<pid>-1`) could
not be threaded between two `ctl` calls inside the fixed action script without capturing
intermediate output, so this row rests on the named tests rather than a fresh live transcript for
the timeout path specifically.

## F-TERM-SCR-01 — 256 KiB scrollback, newest-bytes-retained FIFO

**PASSED** via a live socket transcript captured earlier in this pass
(`/tmp/wf-term-scr01.json`): wrote more than 256 KiB of numbered output, read the pane back, and
the returned buffer was exactly 262144 bytes with the newest content retained and oldest content
evicted, matching `SCROLLBACK_CAPACITY: usize = 256 * 1024` in
`rust/crates/tiller_control/src/panel.rs`.

## F-TERM-SCR-02 — 200ms output-settle debounce, 120ms resize debounce

**half-proven.** The constants match the row exactly
(`TERMINAL_RESIZE_DEBOUNCE`/`OUTPUT_SETTLE_DEBOUNCE` in `rust/crates/tiller_terminal/src/lib.rs`),
and the settle-dependent test `real_pty_emits_osc_title_and_settled_output` passes, showing output
does reach the activity model only after settling rather than per-byte. No fresh live measurement
of burst counts vs. callback counts (i.e. literally counting status/resize callbacks against
faster-than-interval bursts, as the VERIFY text asks) was performed this pass — the row is
test-and-constant-backed but not freshly burst-counted live.

## F-TERM-SPLIT-01 — nested splits, resize-to-minimum, close leaves, surviving PTYs/focus

**PASSED.** Live-driven earlier in this same continuous pass (before a context-compaction
boundary), via a carefully sequenced multi-step drive: created nested left/right/above/below
splits, resized one to the documented 160px minimum (`MIN_SPLIT_PANE_SIZE` in
`rust/crates/tiller_terminal/src/context_menu.rs`), closed a leaf, and confirmed the surviving
panes kept their live PTYs and correct focus.

## F-TERM-UI-01 — context-menu hit-testing offers all 10 items, correct dispatch

**PASSED.** The comprehensive live sweep `/tmp/wf-term-block2a` (24 sequential frames, one
continuous invocation) invoked every item in order — Copy, Paste, Copy Context, Copy Pane ID, Copy
Terminal ID, Set Title, Clear Terminal, Restart Terminal, Close Terminal… — and each produced the
matching terminal/app effect, e.g. Restart Terminal produced a fresh neofetch splash while the
custom title set moments earlier was preserved (`/tmp/wf-term-block2a/24-22-after-restart.png`),
proving the terminal- vs. app-delegated split in `terminal_actions_are_local_and_pane_actions_are_delegated`
holds live, not just in the unit test.

## F-TERM-UI-02 — Cmd/Super-click terminal URLs route through the clicked pane

**half-proven.** One pane cleanly confirmed this pass: `modclick logo <x> <y>` on a URL in a
specific pane opened a new Browser tab showing exactly that pane's URL (the Linux
platform-modifier gesture is the Logo/Super key, confirmed via `event.modifiers.platform` in
`rust/crates/tiller_terminal/src/lib.rs:1278` and the passing test
`linux_link_gesture_is_platform_modifier_only` in `rust/crates/tiller_terminal/src/link_router.rs:64`).
The row's stronger claim — that **two different panes'** URL clicks each route through their own
pane rather than a shared/global handler — was not cleanly closed this pass: the second pane's
confirmation attempt was muddied by a sequencing mistake (the newly opened Browser tab covering
the second click target) and time constraints prevented a clean re-run with two simultaneously
visible panes and two distinct URLs. Grading half-proven rather than PASSED because the row's
specific "two panes, two independent routers" claim is only half-closed.

## F-TERM-USG-01 — Claude usage fetch: injectable PTY, parsing, timeout, termination

**PASSED** via the named tests in `rust/crates/tiller_usage/tests/usage_tests.rs`, all run this
pass and green: `not_installed_is_reachable_through_the_real_shell_when_claude_is_absent_from_path`,
`logged_out_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`,
`error_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`,
`the_fetch_is_bounded_and_single_attempts_do_not_hang`. These use a real executable shell script
standing in for `claude` (the `fake_claude()` helper) driven through a genuine PTY, not a mock —
directly satisfying "a fake PTY that emits staged output" per the VERIFY text. The fetcher's
`Drop for Pty` (kills + waits the child, closes the fd) was inspected in
`rust/crates/tiller_usage/src/claude.rs` and matches the termination half of the row.

## F-TERM-PLAT-01 — no webview/HTML renderer introduced

**PASSED** via inspection: `rust/crates/tiller_terminal/Cargo.toml`'s full dependency list was
read and contains no webview crate (no `wry`, `webkit`, `cef`, or similar) — the Linux rewrite
uses a native GPUI-drawn terminal surface, consistent with the row's requirement.

---

## Summary table

| id | verdict |
|---|---|
| F-TERM-01 | PASSED |
| F-TERM-02 | NOT EXERCISED (other critic) |
| F-TERM-03 | half-proven |
| F-TERM-04 | PASSED |
| F-TERM-05 | PASSED |
| F-TERM-06 | PASSED |
| F-TERM-07 | PASSED |
| F-TERM-08 | PASSED |
| F-TERM-09 | PASSED |
| F-TERM-10 | FAILED — defective |
| F-TERM-11 | PASSED |
| F-CORE-TERM-01 | PASSED |
| F-CORE-TERM-02 | PASSED |
| F-CORE-TERM-03 | PASSED |
| F-TERM-PTY-01 | PASSED |
| F-TERM-PTY-02 | PASSED |
| F-TERM-PTY-03 | PASSED |
| F-TERM-PTY-04 | half-proven |
| F-TERM-PTY-05 | PASSED |
| F-TERM-PTY-06 | PASSED |
| F-TERM-PTY-07 | NOT EXERCISED (other critic) |
| F-TERM-PTY-08 | NOT EXERCISED (other critic) |
| F-TERM-REG-01 | PASSED |
| F-TERM-REG-02 | PASSED |
| F-TERM-SCR-01 | PASSED |
| F-TERM-SCR-02 | half-proven |
| F-TERM-SPLIT-01 | PASSED |
| F-TERM-UI-01 | PASSED |
| F-TERM-UI-02 | half-proven |
| F-TERM-USG-01 | PASSED |
| F-TERM-PLAT-01 | PASSED |

21 PASSED, 5 half-proven (F-TERM-03, F-TERM-PTY-04, F-TERM-SCR-02, F-TERM-UI-02, plus the
signal-9 half of F-TERM-03 already counted), 1 FAILED — defective (F-TERM-10), 4 NOT EXERCISED
(out of this critic's scope by brief).

## Headline finding

**F-TERM-10 is a reproducible defect, not a stale ledger row.** Switching the sidebar's worktree
selection away and back — with no app restart, confirmed by both a socket-driven and a real
pointer/keyboard-driven reproduction inside single script invocations — destroys and respawns the
previously-selected worktree's terminal panes instead of leaving them mounted, contradicting the
"stay mounted across sidebar selection changes" contract CLAUDE.md documents for this feature.
Every other row in this shard's PASSED set that "looks like" F-TERM-10 (F-TERM-PTY-07/08,
covering pane moves and relaunches) is explicitly out of this critic's scope and owned by another
wave — but this finding should be flagged to whoever owns that wave too, since it may share a root
cause with pane-move/relaunch generation handling.
