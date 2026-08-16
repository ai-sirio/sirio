# Wave G slice G5-terminal — 5 rows

**Scheduling round 5 of 6.** You run alone this round.

Earlier rounds have landed; later rounds have not started. Every file below is yours exclusively right now — but **re-read each from disk before editing**; an earlier round may have changed it since this brief was written, so do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller_terminal/src/context_menu.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_terminal/src/link_router.rs`
- `rust/crates/tiller_ui/src/changes.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/right_panel.rs`

## Rows

### `F-TERM-SCR-02` — ledger line 528, currently **half-proven**

- **Files:** rust/crates/tiller_terminal/src/lib.rs
- **Latest critic evidence (2026-08-16, current tree):** OUTPUT_SETTLE_DEBOUNCE=200ms (lib.rs:635) and TERMINAL_RESIZE_DEBOUNCE=120ms (lib.rs:522) are real, and the resize_generation compare-and-swap (lib.rs:411-424) plus pump_terminal_events' drain-until-quiet loop (lib.rs:964-1010) are structurally correct trailing-debounce code, confirmed by reading the current source myself. No committed test exercises this behavior (the prior critic2 test was written, run, and deliberately deleted, and this task forbids writing new Rust), and I designed but did not have time to execute a live WINCH-trap-plus-divider-drag instrument this pass, so I am downgrading from PASSED (code-reading is not a live re-verification) to half-proven rather than re-affirming on the prior evidence.

### `F-TERM-PTY-05` — ledger line 530, currently **half-proven**

- **Files:** — none mapped; find them
- **Latest critic evidence (2026-08-16, current tree):** Live-reached a real Codex CLI child process through the actual command-palette UI path (the tab-strip + button never opened via mouse click across many coordinates; Ctrl+Shift+P then typing codex worked): pstree showed a genuine codex -c notify=[...tillerctl notify --session pane-2...] child with real worker threads, its login TUI rendered live in the pane, and Activity read 1 running. Confirmed real cleanup: after a same-label app teardown (a real SIGTERM quit, not SIGKILL), the specific codex PID was verified fully gone (ps -p and kill -0 both fail), not merely reparented. Could not send a real prompt and see a streamed reply because this host's codex/opencode have no stored credentials (codex login status says Not logged in), an environment gap not a refutation; the launch path itself w

### `F-TERM-UI-01` — ledger line 535, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_terminal/src/context_menu.rs, rust/crates/tiller_terminal/src/lib.rs
- **Latest critic evidence (2026-08-16, current tree):** Live, went beyond builder/prior-critic coverage: Clear Terminal (scrollback emptied), Split Above (new pane above marked original), and Split Down (new pane below, opposite of Above) all directly confirmed working with marker-based before/after captures. But Set Title reproducibly (2 runs) does nothing: click lands on the item (hover-confirmed), menu closes, but the tab label stays plain 'Terminal' with no text-entry field ever appearing and no crash logged -- source shows set_terminal_title is a stub that should at least relabel to 'Terminal {terminal_id}' and even that never renders. A concrete defect inside this row's own drive list, not just missing evidence, so downgraded from half-proven to FAILED — defective.

### `F-TERM-UI-02` — ledger line 536, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_terminal/src/link_router.rs, rust/crates/tiller_ui/src/changes.rs
- **Latest critic evidence (2026-08-16, current tree):** Code fix (9a13174) reads correctly, but reverting only the origin-subtraction and rerunning platform_modifier_click_opens_a_terminal_link showed the test still passes -- it does not discriminate (test window sits at origin 0,0). Live modclick attempts used the wrong modifier at first (ctrl, not platform/Super per link_router.rs and gpui's own docs) and were then cut short by the worktree repeatedly vanishing/reappearing; no live proof either direction exists.

### `F-CHG-18` — ledger line 209, currently **half-proven**

- **Files:** rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/changes.rs, rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/right_panel.rs
- **Latest critic evidence (2026-08-16, current tree):** Independently reached further than the builder: rightclick-Terminal-tab then click Move to New Pane (one invocation, no intervening shot) reliably produced a genuine two-pane layout with a live PTY Terminal pane, reproduced twice. Final drag-to-pill step stayed unconfirmed: one attempt used a stale cross-invocation coordinate and missed the row, a second attempt's own screenshot showed Local changes (0) -- the shared tree's changed-file list was genuinely empty from concurrent sibling commits, leaving nothing to drag.

