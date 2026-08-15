# Wave E slice E-P2 — 2 rows

**Scheduling round 4 of 5.** Running at the same time as: E-C-4 — none of them owns any file you own, verified before launch.

Slices in **earlier** rounds have already landed their commits; slices in **later** rounds have not started. So every file below is yours exclusively right now — but **re-read each one from disk before editing**, because an earlier round has changed some of them since this brief was written. Do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller_activity/src/model.rs`
- `rust/crates/tiller_activity/tests/activity_domain_integration.rs`
- `rust/crates/tiller_agents/src/claude.rs`
- `rust/crates/tiller_ui/src/status_bar.rs`
- `rust/crates/tiller_ui/src/tab_bar.rs`
- `rust/crates/tiller_usage/src/claude.rs`
- `rust/crates/tiller_usage/src/model.rs`

## Rows

### `F-CORE-ACT-11` — ledger line 347, currently **half-proven**

- **Files:** rust/crates/tiller_activity/src/model.rs, rust/crates/tiller_activity/tests/activity_domain_integration.rs, rust/crates/tiller_ui/src/tab_bar.rs, rust/crates/tiller_usage/src/model.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Could not reproduce live spawn-owned reachability: pixel-precise clicks on tab_bar.rs's + new-tab button (glyph center measured via 5x crop) never opened the dropdown across many attempts (verified via same-resolution before/after pixel diff, 3374px changed vs ~40000+ expected for a rendered menu); the right-click terminal context menu, a documented working positive control, also failed to render for me under identical technique, suggesting session-load flakiness (30 concurrent sway/tiller instances from other agents) rather than a new defect. model.rs itself re-read and still correct (three disjoint ownership sets). Simultaneous 3-pane cross-check remains unproven. sweep D-P3, 2026-08-15

### `F-USE-03` — ledger line 266, currently **half-proven**

- **Files:** rust/crates/tiller_agents/src/claude.rs, rust/crates/tiller_ui/src/status_bar.rs, rust/crates/tiller_usage/src/claude.rs, rust/crates/tiller_usage/src/model.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Reran cargo test -p tiller_usage --lib: 40/40 pass including success_replaces_the_previous_state (TimedOut -> Stale reducer) and the bounded-timeout PTY test. Both owned files unchanged and correct. Live stale-after-timeout transition still needs foreign status_bar.rs; not attempted live this pass (25s timeout, out of budget). No change from prior sweep. sweep D-P2, 2026-08-15

