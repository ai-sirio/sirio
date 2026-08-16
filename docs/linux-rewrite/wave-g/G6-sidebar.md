# Wave G slice G6-sidebar — 3 rows

**Scheduling round 6 of 6.** You run alone this round.

Earlier rounds have landed; later rounds have not started. Every file below is yours exclusively right now — but **re-read each from disk before editing**; an earlier round may have changed it since this brief was written, so do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller/src/session.rs`
- `rust/crates/tiller_agents/src/claude.rs`
- `rust/crates/tiller_agents/src/codex.rs`
- `rust/crates/tiller_ui/src/sidebar.rs`
- `rust/crates/tiller_ui/src/status_bar.rs`
- `rust/crates/tiller_usage/src/claude.rs`
- `rust/crates/tiller_usage/src/model.rs`

## Rows

### `F-SID-11` — ledger line 80, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Latest critic evidence (2026-08-16, current tree):** Live restart test (two independent runs, same TILLER_DB, fresh tiller process each time): ctl worktree.set comment=MARK returns ok server-side, but the sidebar row shows no comment segment at all after a forced repaint. The builder's RefreshSidebar wiring does fix same-process rendering (confirmed: comment appears immediately after worktree.set within one live process) but not the restart case, which is what the row's own recorded defect was about.

### `F-PRJ-05` — ledger line 98, currently **FAILED — defective**

- **Files:** — none mapped; find them
- **Latest critic evidence (2026-08-16, current tree):** Live re-drive: Clone form opens and Cancel is present, but typing the full URL drops all but 1 of 44 chars (only h landed), so Destination derived live to /home/enzopalmisano/h, not the claimed Hello-World destination. Frame 05-43-typed.png. Independently reproduces the F-PRJ-06/P123 character-drop defect on this same form.

### `F-USE-03` — ledger line 266, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_agents/src/claude.rs, rust/crates/tiller_agents/src/codex.rs, rust/crates/tiller_ui/src/status_bar.rs, rust/crates/tiller_usage/src/claude.rs, rust/crates/tiller_usage/src/model.rs
- **Latest critic evidence (2026-08-16, current tree):** cargo test -p tiller_usage --lib: 40/40 pass, including model::tests::success_replaces_the_previous_state (TimedOut->Stale reducer) and status_bar.rs's unavailable_reasons_render_distinct_text, proving Stale renders identical text to Loaded and is distinguished only by segment_dimmed. A real claude CLI is present in this environment, but Claude::TIMEOUT=25s has no control-socket shortcut, and forcing a first real fetch plus a second real 25s hang risked this dispatch's 180s-silence kill for marginal gain over the existing unit-level proof, so the live UI dimming transition was not attempted — same deferral as wave D and the builder, not a regression or new suspicion.

