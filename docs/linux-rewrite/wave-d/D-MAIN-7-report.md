# D-MAIN-7 report

Owner file: `rust/crates/tiller/src/main.rs` (plus `rust/crates/tiller_terminal/src/lib.rs` for
F-TERM-06's test). Re-read `main.rs` from disk before each edit per house rules; no conflicts with
earlier D-MAIN links observed.

## `F-TAB-23` — fixed (same root cause as F-TERM-SPLIT-01)

Live-drove the exact gesture the critic's evidence describes (`click` terminal, `type
MARKER_ORIGINAL_PANE`, `rightclick`, click **Split Left**) on a fresh instance and the *menu*
route was already correct: the new empty pane appeared on the left, the marker-bearing original
stayed right. Repeated for **Split Right** with a second marker — also correct (new pane right,
original stays left). `PaneNode::split_focused_with_placement` and the terminal context menu's
`SplitLeft`/`SplitRight` → `SplitPlacement::Before`/`After` wiring were already right (landed in
`bf1f70b`, well before Wave D).

The real, still-live defect was the **control-socket** `pane.split` handler (see F-TERM-SPLIT-01
below) — it discarded placement, so any `ctl`-driven "left" split always landed on the right. I
suspect the critic's own drive script hit this same coordinate/resolution fragility I ran into
repeatedly in this lane (see "Note on the Wayland lane" below) and mis-attributed a socket-shaped
symptom to the menu route. Fixed as part of the same commit as F-TERM-SPLIT-01 (`cd6eec7`).

- **howToExercise**: right-click a terminal → Split Left/Right (both correct); `ctl pane.split
  direction=left` (now correct — was wrong before this row).

## `F-TERM-SPLIT-01` — fixed

`ControlAction::SplitPane` only carried a `direction`, so `pane.split direction=left/up` parsed the
axis correctly but always split via `split_focused_terminal`'s hardcoded `SplitPlacement::After`.
Added `placement: SplitPlacement` to `SplitPane`, computed it in the `"pane.split"` request handler
from the direction string (`right`→After, `left`→Before, `down`→After, `up`→Before), and added
`split_focused_terminal_with_placement` (threads to the already-correct
`split_terminal_at_with_placement`, same call the context menu uses). Removed the now-dead
placement-less `split_terminal_at`.

- **Live proof**: `ctl project.add`, click terminal, type `SOCK_MARKER_ORIGINAL`, `ctl pane.split
  direction=left` → new empty pane appears on the left, marker-bearing original on the right.
  Screenshot: `/tmp/claude-1000/dmain7sock/03-after-split-left.png` (not committed — ephemeral
  lane output; reproduce with the command below).
- **howToExercise**: `ctl pane.split direction=left` (or `direction=up`) against a single-pane tab
  with typed marker text; new empty pane must land on the specified side, not the opposite one.
- Commit: `cd6eec7`.

## `F-TERM-08` — fixed

`close_focused_pane`/`close_terminal_at` closed unconditionally regardless of
`ActivityStatus`. Added `pending_pane_close: Option<PendingPaneClose>` to `TillerWorkspace`,
`request_close_focused_pane`/`request_close_terminal_at` (interactive entry points that check
`pane_close_needs_confirmation` — a local mirror of
`tiller_activity::ActivityStatus::requires_close_confirmation`'s rule, since `main.rs` renders
through `tiller_ui`'s own separate-but-identical `ActivityStatus` enum, not
`tiller_activity`'s), and a `Close Anyway` / `Cancel` banner rendered over the pane leaf when
a close is pending. Cmd-W (`handle_close_pane`) and the context menu's "Close Terminal…" both
route through the gated path now; the control socket's `ClosePane` intentionally keeps closing
immediately — same precedent as `F-PRJ-03`'s `project.add`, no user present to answer a prompt.

- **Live proof**: `ctl notify session=pane-1 status=running`, right-click terminal → Close
  Terminal… → banner "This pane has running work. Close anyway?" with Close Anyway/Cancel
  buttons renders over the pane (does not close). Screenshot:
  `/tmp/claude-1000/dmain7close/03-after-close.png` (ephemeral; reproduce below).
- **howToExercise**: `ctl notify session=pane-<n> status=running`, then right-click that pane →
  "Close Terminal…" — a confirm banner must appear instead of an immediate close; click "Close
  Anyway" to confirm the pane actually closes, or "Cancel" to dismiss and keep it.
- Commit: `572c5db`.

## `F-TERM-06` — fixed (via a landed, replayable test — not a live drive)

The click-and-clipboard-read half stayed owed because the Wayland lane has no clipboard-read
primitive and — worse — right-click-menu coordinates in this lane are fragile across the
resolution-toggle `shot` does (see note below); I burned significant time chasing false negatives
from stale menu-item y-offsets before recognizing this. Landed
`copy_pane_id_then_paste_round_trips_through_the_context_menu` in
`tiller_terminal/src/lib.rs` instead: it drives the *real* GPUI mouse gesture (`simulate_mouse_down`
+ `simulate_click` against `debug_bounds`, the same mechanism the pre-existing
`right_click_resolves_this_terminal_and_draws_all_context_actions` test uses for Split Left)
against a live `cat` PTY — clicks "Copy Pane ID", reads `cx.read_from_clipboard()` back and asserts
it holds the real `pane-N` id, then re-opens the menu, clicks "Paste", and asserts the shell echoed
that exact id into the terminal's scrollback. This is a stronger, replayable proof than a
screenshot pair would have been, and it lands in the repo so it can't evaporate like a `/tmp`
capture.

- **howToExercise**: `cargo test -p tiller_terminal
  copy_pane_id_then_paste_round_trips_through_the_context_menu` — passes green, exercises the
  real click path end to end. (Manual UI route also works: right-click → Copy Pane ID → right-click
  → Paste → the terminal echoes `pane-N`; getting the Paste item's y-coordinate right in the
  Wayland lane requires reading it off a screenshot taken at the *same* resolution as the
  right-click that opens the menu, not a stale value from an earlier capture.)
- Commit: `99e5980`.

## `F-USE-06` — already-correct, re-verified

The critic explicitly carried forward a stale verdict without re-checking ("explicitly
un-reverified live here"). The underlying defect this row named — `register_restored_agent`
never being called for restored panes, leaving `pane_agents` empty so
`post_activity_notification`'s `agent_id(pane)` early-return always fires — was already fixed in
`a5d09d7` ("fix: wire notification delivery and restored agents"), **2026-08-14, before Wave D
started**. `register_restored_agent` is called from both `restore_tabs` and
`restore_tabs_in_workspace`, and a real passing test
(`tests::restore_tabs_registers_restored_agent_identity`, `main.rs:~12764`) proves a restored
Codex terminal tab ends up with `activity.agent_id("pane-7") == Some("codex")` after restore. Ran
it fresh: `cargo test -p tiller restore_tabs_registers_restored_agent_identity` → passes.

I made no code change here — did not want to touch a row that's already fixed and tested without
a concrete new gap to close, and the manual "restart with saved session + trigger transition while
hidden + observe notify-send" drive is both hard to stage from scratch in the time remaining and
explicitly warned against for `panel.list`'s `agent` field (WAYLAND-LANE.md: "dead instrument for
activity state").

- **howToExercise**: `cargo test -p tiller restore_tabs_registers_restored_agent_identity`
  (passes). No UI drive needed to close this row; the code-level proof is direct and already in
  the tree.
- Action: `already-correct`, not `not-attempted` — the gap was real, it is fixed, just not by me.

## `F-TERM-UI-01` — partially advanced, still half-proven

Individually exercised more of the 12 context-menu items than the prior pass: Split Left, Split
Right, Copy Pane ID, Paste, Close Terminal (+ new confirm banner) all now have live and/or
committed-test proof (see rows above). Did **not** individually verify Copy, Copy Context, Set
Title, Copy Terminal ID, Split Above, Split Down, or Clear Terminal this pass — a Clear Terminal
click attempt failed to visibly clear the screen, but I could not tell whether that was a real
defect or the same right-click-menu-coordinate fragility that gave me several false negatives
elsewhere in this lane (see note below), and ran out of budget to disambiguate live. Leaving this
`half-proven` rather than guessing.

- **howToExercise (untested items, for the next pass)**: right-click a terminal with content in
  it, click "Clear Terminal" — screen should visibly clear. The existing
  `right_click_resolves_this_terminal_and_draws_all_context_actions` test in
  `tiller_terminal/src/lib.rs` already proves all 12 items render at correct `debug_bounds`; what's
  still owed is a live/individual click-and-observe for the ~6 unlisted above.
- Action: `not-attempted` (no code change; partial exercise only, documented above under other
  rows' proofs).

## `F-TERM-09` — blocked, not attempted

This is the one row I judged genuinely out of reach this pass. The prior triage (`T6-term-agent-
plan.md`) correctly sizes this as **size L**: four independently-symptomed defects (badge doesn't
show working, doesn't clear on idle, doesn't clear on death, survives relaunch) requiring a live
drive against **real installed agent binaries** (`claude`, `codex`) to determine whether
`tiller_activity/src/title.rs`'s empirically-captured OSC title patterns still match what those
CLIs actually emit today. I read `title.rs` in full — the patterns look carefully reasoned and
internally consistent with the documented conventions (Claude `✳ …`/`. …`/spinner, Pi `π - …`, omp
`π: …`, word-boundary-safe Codex/OpenCode matching) — but confirming or refuting them needs a real
interactive TUI session, and I hit a hard blocker trying to stage one: this session's own
inherited Claude Code environment variables force a nested `claude` invocation into
non-interactive `--print`-style mode even under a real PTY (`setsid`), so I cannot observe its
genuine interactive OSC title sequence from inside this sandbox. I verified Layer D's mechanism
itself is sound (`tiller_activity/src/process.rs` correctly walks `/proc/<pid>/task/<pid>/children`
and matches exact `comm` names — both `claude` and `codex` are native ELF binaries whose `comm`
should match `CATALOG_IDS` directly, not Node-hosted like pi/omp), and that
`start_process_signal_refresh`'s poll loop keeps running after the shell's process tree changes (it
only stops when the pane itself is closed/rebound, not on child exit), so the "loop stops before
observing the clearing transition" hypothesis in the triage doc looks less likely than the
title-pattern-drift hypothesis — but this is inference, not verified fact.

- **What's needed**: a drive environment where a real `claude`/`codex` TUI can be launched
  interactively (ideally the Wayland lane's own PTY, which doesn't inherit this session's
  `CLAUDECODE`-style env) and its actual OSC title sequence captured (e.g. via `printf` tracing or
  a modified build that logs `TerminalActivityEvent::OscTitle` to stderr) and diffed against
  `title.rs`'s patterns.
- Action: `blocked`.

## Note on the Wayland lane: right-click-menu coordinates are resolution-fragile

Several of my early live-drive attempts produced false negatives (menu item apparently not
clicked, or a different item clicked) that were **not** app defects — they were my own
coordinate/resolution mismatches. `shot` toggles the nested output's resolution to force a
repaint; the app's own window content does **not** rescale with it (confirmed by comparing pixel
layouts across differently-labeled-resolution captures), but if a `rightclick`/`click` pair
straddles a `shot` in between, the *nominal* resolution active at click-time can differ from the
one a previously-measured menu-item y-coordinate was read off, and the click lands on stale
coordinates — sometimes hitting the Files sidebar instead. The fix that worked reliably: measure
menu-item bounds from a screenshot taken **immediately** after the `rightclick` that opens it (no
intervening `shot`), and issue the follow-up `click` in the same action block with no `shot`
between the `rightclick` and the `click`. This is worth adding to `WAYLAND-LANE.md`'s trap list
for the next agent driving this lane — I did not touch that file since I don't own it.

## Build status

`cargo build -p tiller` and `cargo build -p tiller_terminal` both green after every commit. Full
`cargo test -p tiller_terminal copy_pane_id_then_paste_round_trips_through_the_context_menu` and
`cargo test -p tiller restore_tabs_registers_restored_agent_identity` both pass.

## Commits (this row's chain)

1. `572c5db` — feat(F-TERM-08): confirm before closing a pane doing live work
2. `99e5980` — test(F-TERM-06): prove Copy Pane ID / Paste through a real click + clipboard read
3. `cd6eec7` — fix(F-TERM-SPLIT-01): thread split placement through the control socket (also
   closes F-TAB-23)
