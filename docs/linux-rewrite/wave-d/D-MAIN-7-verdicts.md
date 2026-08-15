# D-MAIN-7 critic verdicts

Critic pass, independent of the builder. HEAD at verify time: `4560076` (post wave-D
integration), binary at `rust/target/debug/tiller` rebuilt and matching. Instruments used:
`cargo test -p tiller_terminal` / `cargo test -p tiller` (per-crate, targeted tests only),
`Scripts/wayland-drive.sh` (labels `dmain7crit1`..`dmain7c15`, `dmain7cal`, `dmain7ext`), and
source inspection of `rust/crates/tiller/src/main.rs`, `rust/crates/tiller/src/panes.rs`,
`rust/crates/tiller_activity/src/model.rs`, `rust/crates/tiller_terminal/src/lib.rs`.

The machine was under heavy contention throughout this pass (`uptime` load average ~75 on a
12-core box, from many parallel critic/builder instances) — this measurably affected the Wayland
lane's right-click reliability (see `F-TERM-08` below) and is noted there specifically.

## `F-TAB-23` — PASSED

Live (Wayland lane): `project.add`, clicked the sole terminal, typed `CRIT_MARKER_LEFT`, sent
`ctl pane.split direction=left` in the same action block. Captured frame shows the new empty pane
on the **left** and the marker-bearing original on the **right** — the discriminating result (the
pre-fix defect would have put the new pane on the right regardless of direction). Source confirms
`"pane.split"` now computes `SplitPlacement` from the direction string and
`split_focused_terminal_with_placement` threads it to the same `split_terminal_at_with_placement`
call the context menu uses (`main.rs:1597-1618`, `2907-2916`). Same fix, same commit (`cd6eec7`) as
`F-TERM-SPLIT-01` below; graded separately per the ledger's two row IDs but backed by one piece of
evidence.

## `F-TERM-SPLIT-01` — PASSED

Same live drive and frame as `F-TAB-23` (`ctl pane.split direction=left` against a
`CRIT_MARKER_LEFT`-marked pane placed the new empty pane left, original right) — this is exactly
the row's own `howToExercise`. Discriminating: the pre-fix code hardcoded
`SplitPlacement::After` regardless of requested direction, so a broken build would show the new
pane on the right for `direction=left` too.

## `F-TERM-08` — half-proven

Code inspection is clean and specific: `pane_close_needs_confirmation` gates on
`Running`/`NeedsInput`/`Error` (`main.rs:2396`), `request_close_terminal_at` sets
`pending_pane_close` and returns *before* ever calling `close_terminal_at` when confirmation is
needed (`main.rs:3873-3883`), and both the context menu's Close Terminal item
(`main.rs:3313`) and Ctrl-Alt-W (`handle_close_pane` → `request_close_focused_pane`,
`main.rs:7632-7633`) route through it. The control socket's `ClosePane` intentionally bypasses it,
matching the documented `F-PRJ-03` precedent.

Live, I independently confirmed two of the three pieces this row needs: `ctl notify
session=pane-1 status=running` reliably produces a red dot on the Terminal tab and an "Activity: 1
running" line (positive control, reproduced 3 times), and right-clicking a terminal reliably draws
the full 12-item menu including "Close Terminal…" (reproduced twice, including at 1715×972 with a
running pane). I could not, however, get a clean live capture of the actual confirm banner: four
attempts at `rightclick` → `click` on the "Close Terminal…" row, single action block, produced
either the menu still open (missed click) or a plain reset-looking terminal with no banner and no
running indicator — and a control run at the *same* pixel target with **no** prior `notify` (so no
confirmation should even be needed) produced a visually identical "plain terminal" frame, meaning
this single-pane setup can't discriminate "correctly gated, no-op close" from "closed without
asking" (`close_terminal_at` itself no-ops for a tab's sole pane, `main.rs:6076`, but that guard is
never reached in the gated branch, so it doesn't explain the missing banner either way). A repeat
of the exact same `rightclick`-then-`shot` sequence (no click at all) once produced *no menu and no
running badge*, i.e. the right-click itself intermittently failed to register under this pass's
~75 load average — the same class of contention-driven flakiness the prior `D-MAIN-4` critic pass
documented for its own gestures. I cannot tell a genuine missing-banner defect from dropped
synthetic input under this load, so this stays `half-proven`: the gating code is real and correctly
wired (a clear improvement over the prior `FAILED — absent`), but the live gesture proof the row
needs was not cleanly reproducible this pass.

## `F-TERM-06` — PASSED

`cargo test -p tiller_terminal copy_pane_id_then_paste_round_trips_through_the_context_menu` —
ran it myself, passes (`1 passed; 0 failed`, 3.37s). Read the test source: it is a real GPUI
`simulate_mouse_down`/`simulate_click` against `debug_bounds` (the same mechanism the pre-existing,
long-standing Split-Left test uses), against a live `cat` PTY — clicks "Copy Pane ID", reads
`cx.read_from_clipboard()` back and asserts it holds a real `pane-N` id, then clicks "Paste" and
asserts the shell echoed that exact id into scrollback. This is a genuine simulated-click-and-
observe proof, not a mocked assertion, and it is committed to the repo (`99e5980`), so it survives
past this pass.

## `F-USE-06` — PASSED

`cargo test -p tiller restore_tabs_registers_restored_agent_identity` — ran it myself, passes.
Source confirms `register_restored_agent` is called from both `restore_tabs` and
`restore_tabs_in_workspace` (`main.rs:8812`, `8970`), and `git show --stat a5d09d7` confirms this
wiring landed 2026-08-14, before Wave D started — the builder's "already-correct, not my fix"
framing checks out against the repo, not just the builder's prose.

## `F-TERM-UI-01` — half-proven (unchanged)

Independently reconfirmed the menu still renders all 12 items including "Close Terminal…" (see
`F-TERM-08` above). Split Left/Right and Copy Pane ID/Paste are proven by the rows above; the
builder's claimed still-unverified six (Copy, Copy Context, Set Title, Copy Terminal ID, Split
Above, Split Down, Clear Terminal) were not individually exercised by me either — same honest gap,
carried forward rather than guessed at. No regression found.

## `F-TERM-09` — UNREACHABLE

Confirmed `title.rs` is unchanged since the prior critic's evidence. This row needs a real
interactive `claude`/`codex` TUI's genuine OSC title sequence, captured outside a sandbox that
forces nested agent invocations into non-interactive mode — the same constraint applies to me as a
Claude Code session in this environment, not just to the builder. I did not attempt to fake around
this; it is a real instrument gap, not a graded absence of effort.
