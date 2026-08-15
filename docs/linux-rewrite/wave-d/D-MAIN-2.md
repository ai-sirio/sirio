# Wave D slice D-MAIN-2 — 7 rows

**Serialised slice — link 2 of 8 in the `D-MAIN` chain.** Earlier links have landed; later links have not started. Nobody else holds `main.rs`. **Re-read the file from disk before editing** — an earlier link changed it since this brief was written.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-CHG-02` — ledger line 193, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/changes.rs, rust/crates/tiller_ui/src/right_panel.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live before/after: surface.changes.open on a just-closed worktree now ok:true (real fix). But no empty-state UI exists in changes.rs/right_panel.rs (grep-reconfirmed) -- row's actual clause still unmet. sweep C-MAIN-1, 2026-08-15

### `F-CHG-13` — ledger line 204, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/changes.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** main.rs:2819/4018 (this slice's own file) discard the OpenDiff path unconditionally; no single-file diff view exists anywhere. Builder's foreign-file-only scope claim is inaccurate. sweep C-MAIN-1, 2026-08-15

### `F-CORE-ACT-10` — ledger line 346, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/panes.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Not re-driven: panes.rs is outside this slice's owned files and received no commits; standing confound finding stands unchanged. sweep C-MAIN-1, 2026-08-15

### `F-CORE-ACT-25` — ledger line 361, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/bootstrap.rs, rust/crates/tiller_activity/tests/activity_domain_integration.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-grepped today, zero callers outside own module/test, unchanged from prior record; no wave-C commit touched bootstrap.rs or the restore path. sweep C-MAIN-2, 2026-08-15

### `F-CORE-ACT-26` — ledger line 362, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/mount.rs, rust/crates/tiller_project/src/settings.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-grepped today, zero callers outside own test, unchanged; no wave-C commit touched mount.rs. sweep C-MAIN-2, 2026-08-15

### `F-CORE-DOM-07` — ledger line 375, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_project/src/domain.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-grepped today: zero callers outside domain.rs/tests, unchanged. No live drive attempted this pass (no socket verb to grow a transcript past the throttle gate); gap recorded plainly rather than promoted to FAILED. sweep C-MAIN-2, 2026-08-15

### `F-CORE-FILE-04` — ledger line 389, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_project/src/file_link.rs, rust/crates/tiller_terminal/src/link_router.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_ui/src/file_view.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-grepped today: FileViewEvent emitted at file_view.rs:379, its only subscriber is its own module test (line 2398); main.rs never references it (grep, 0 hits) though it wires the sibling RightPanelEvent/ChatEvent OpenFile variants. Live click-through attempted but not completed (state lost to harness relaunch behavior); code evidence stands unchanged. sweep C-MAIN-2, 2026-08-15

