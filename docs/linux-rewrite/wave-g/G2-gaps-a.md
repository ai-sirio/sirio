# Wave G slice G2-gaps-a — 3 rows

**Scheduling round 2 of 6.** You run alone this round.

Earlier rounds have landed; later rounds have not started. Every file below is yours exclusively right now — but **re-read each from disk before editing**; an earlier round may have changed it since this brief was written, so do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller_activity/src/bootstrap.rs`
- `rust/crates/tiller_activity/src/mount.rs`
- `rust/crates/tiller_activity/tests/activity_domain_integration.rs`
- `rust/crates/tiller_project/src/domain.rs`
- `rust/crates/tiller_project/src/settings.rs`
- `rust/crates/tiller_terminal/src/domain.rs`
- `rust/crates/tiller_terminal/src/lib.rs`

## Rows

### `F-CORE-ACT-25` — ledger line 361, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/bootstrap.rs, rust/crates/tiller_activity/tests/activity_domain_integration.rs, rust/crates/tiller_terminal/src/lib.rs
- **Latest critic evidence (2026-08-16, current tree):** Independently re-confirmed select_worktree still tears down the previous worktree synchronously (single working_directory field, no mounted set) and BootstrapRestoreOrder has zero production callers (grep, outside its own module/tests). Builder made no code change; matches wave-D diagnosis exactly.

### `F-CORE-ACT-26` — ledger line 362, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/mount.rs, rust/crates/tiller_activity/tests/activity_domain_integration.rs, rust/crates/tiller_project/src/settings.rs
- **Latest critic evidence (2026-08-16, current tree):** Same confirmed architectural gap as ACT-25; WorktreeMountPolicy has zero production callers (independent grep) and mounted_worktrees/limit_mounted_worktrees remain unconsumed settings. Builder made no code change.

### `F-CORE-DOM-07` — ledger line 375, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_project/src/domain.rs, rust/crates/tiller_terminal/src/domain.rs, rust/crates/tiller_terminal/src/lib.rs
- **Latest critic evidence (2026-08-16, current tree):** Independently re-confirmed AutoNamingThrottle has zero production callers (grep) and main.rs has no transcript-driven auto-naming call site to gate — only the settings toggle exists. Builder made no code change.

