# Wave D slice D-MAIN-5 — 7 rows

**Serialised slice — link 5 of 8 in the `D-MAIN` chain.** Earlier links have landed; later links have not started. Nobody else holds `main.rs`. **Re-read the file from disk before editing** — an earlier link changed it since this brief was written.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-PRJ-03` — ledger line 96, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** FAILED -- absent, evidence corrected. Live: ctl project.add path=/tmp/cmain3-nongit (real non-.git folder) -> ok:true, added SILENTLY, zero prompt (frame 09-nongit-add.png). grep confirms no browse-time Initialize-Git/Add-without-Git/Cancel prompt exists anywhere (only a post-add sidebar context action). Separately: the ledger's own evidence sentence (no Open Project entry exists) is FALSE and stale -- live frame 07-menuopen.png shows Open Project.../Clone Repository.../Create Project... all three, Open Project wired to a real cx.prompt_for_paths folder picker (commit 5c1248f, predates wave-C 

### `F-SET-04` — ledger line 292, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** UPGRADED half-proven -> FAILED -- defective. C-MAIN-1's commit 32fffaf added a real prod caller of resume_command (main.rs:7807, restored_agent_shell <- resumable_session_refs <- restore_tabs/restore_tabs_in_workspace). Traced the full chain: saved_session_refs = session_store.load_session_refs() (main.rs:8450) passed into restore_tabs UNCONDITIONALLY; grepped every .resume_agent_sessions site in main.rs + tiller_persistence/db.rs -- used only for settings snapshot/DB round-trip and system.capabilities reporting, never checked in the restore/resume path. Toggling the setting off has zero effec

### `F-SET-09` — ledger line 297, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/settings.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged). grep re-confirms zero InstallStatus/install-progress field in settings.rs (only hit is a test name). Click-reaches-consumer half stays proven; feedback/control-state-change half stays structurally absent. sweep C-MAIN-3, 2026-08-15

### `F-SET-10` — ledger line 298, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/status_bar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged). git log f3b6169..HEAD -- tiller_ui/src/status_bar.rs shows only 8b7c265 (Codex refresh-classification copy), unrelated to this row's toggle/interval/Refresh-now clause. DB half stays proven from prior live kill/relaunch evidence; refresh-interval-change and Refresh-now-confirmed-update stay test-only. sweep C-MAIN-3, 2026-08-15

### `F-SET-18` — ledger line 306, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_agents/src/lib.rs, rust/crates/tiller_ui/src/settings.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged). grep re-confirms zero Install/Update/Retry control site in settings.rs's agent rows. No wave-C commit added one. Record stands. sweep C-MAIN-3, 2026-08-15

### `F-SET-22` — ledger line 310, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/settings.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** FAILED -- defective (confirmed, unchanged). settings_snapshot_from_app_settings (main.rs:8284) still carries its own comment verbatim (F-SET-22 has no AppSettings field yet...) and hard-codes agent_colors: SettingsSnapshot::default().agent_colors. Grepped the whole AppSettings struct: no agent_colors field exists. Click-to-snapshot route is real (prior evidence); value dead-ends exactly where the row's own defect names. sweep C-MAIN-3, 2026-08-15

### `F-SID-06` — ledger line 75, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged). git log f3b6169..HEAD -- tiller_ui/src/sidebar.rs: 3 commits, all New-Worktree-prompt base/location fields + GitHub avatar prefill -- none touch collapsed-project badges. Standing P106 half-proven (dots shown live on worktree rows; collapsed-project badge not exercised, code gates dots to worktree rows) stands. sweep C-MAIN-3, 2026-08-15

