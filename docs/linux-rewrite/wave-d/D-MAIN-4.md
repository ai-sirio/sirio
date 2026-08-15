# Wave D slice D-MAIN-4 — 7 rows

**Serialised slice — link 4 of 8 in the `D-MAIN` chain.** Earlier links have landed; later links have not started. Nobody else holds `main.rs`. **Re-read the file from disk before editing** — an earlier link changed it since this brief was written.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-CTRL-BROWSER-06` — ledger line 449, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live today (cmain2a): browser.eval and browser.console both returned explicit ok:false 'unsupported on Linux: not implemented' — not the recorded queued:true no-op. Same 988d9e9 pre-wave-C fix. Reclassified defective->absent. sweep C-MAIN-2, 2026-08-15

### `F-CTRL-WORK-01` — ledger line 435, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_persistence/src/model.rs, rust/crates/tiller_project/src/worktree.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Real restart today: set comment=RESTART_PROOF_MARKER_9182 (echoed live), fully killed the process (no TILLER_WL_KEEP), relaunched a fresh process against the identical DB file (same label), read comment back via a session-only worktree.set — came back empty. Direct restart proof, not just the source comment; verdict unchanged, evidence strengthened. sweep C-MAIN-2, 2026-08-15

### `F-EDIT-08` — ledger line 226, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** NOT EXERCISED (unchanged). Builder landed nothing. Source read: DocumentRegistry (editor.rs, the component the row's comment names) has zero production callers, but add_file_tab (main.rs:4423) implements its own real inline dedup (scans open File tabs, focuses existing instead of duplicating) -- a second, wired, untried route via Files-panel double-click, not the ctrl+o portal. Attempted live double-click on f.txt; coordinates crossed a resolution toggle and landed on stray targets (opened a duplicate Changes tab, not the file) -- inconclusive, not proof either way. sweep C-MAIN-3, 2026-08-15

### `F-GIT-BRANCH-01` — ledger line 486, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_git/src/branches.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** NOT EXERCISED (unchanged). git log f3b6169..HEAD -- tiller_git/src/branches.rs is empty (no wave-C commit from any slice). grep re-confirms GitBranches::list/list_branches: zero callers outside tests/p99_git_rows.rs. Record stands. sweep C-MAIN-3, 2026-08-15

### `F-PER-01` — ledger line 237, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_acp/src/chat.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** FAILED -- defective (unchanged). git log f3b6169..HEAD -- tiller_ui/src/chat.rs shows only the 3 C-CHAT-chain commits (status pill / AuthRequired banner / MCP warnings), none touching the persistence write path; tiller_acp/src/chat.rs untouched in range (confirmed via INTEGRATION.md cross-slice sweep). Pass-17's live restart finding (chat_turn=0, session_ref=0 rows after two real completed ACP exchanges, WAL-aware read) stands as the most recent evidence. sweep C-MAIN-3, 2026-08-15

### `F-PER-08` — ledger line 244, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged). grep re-confirms newly_allowed (main.rs:4645-4668) is the sole site that can synthesize a browser-origin permission grant, reachable only via browser.* socket calls, not any settings UI control. General-settings half stays proven; browser-origin half stays unreached on this lane. sweep C-MAIN-3, 2026-08-15

### `F-PERSIST-DB-05` — ledger line 508, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_acp/src/chat.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** FAILED -- defective (unchanged). grep re-confirms load_chat_transcript's 2 production callers stay confined to tiller_acp/src/chat.rs; no wave-C commit (any slice) touched that call graph. Row's own VERIFY (create chat tabs, restart, inspect transcript restoration in-app) still unexerciseable on the path the app actually uses. sweep C-MAIN-3, 2026-08-15

