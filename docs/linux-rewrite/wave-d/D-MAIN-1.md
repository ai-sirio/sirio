# Wave D slice D-MAIN-1 — 7 rows

**Serialised slice — link 1 of 8 in the `D-MAIN` chain.** Earlier links have landed; later links have not started. Nobody else holds `main.rs`. **Re-read the file from disk before editing** — an earlier link changed it since this brief was written.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-AGENT-OMP-03` — ledger line 471, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_agents/src/lib.rs, rust/crates/tiller_agents/src/omp.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Reran upstream oh-my-pi myself (not trusting report prose): same SyntaxError, node module-load crash, unrelated to Tiller's generated command. sweep C-MAIN-1, 2026-08-15

### `F-BRW-04` — ledger line 253, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Static re-read this pass (not re-driven live, file outside slice, no code change): same no-validation code paths P96 drove live are still present verbatim in main.rs. sweep C-MAIN-1, 2026-08-15

### `F-BRW-05` — ledger line 254, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** grep-reconfirmed: browser.act still the only call site of set_agent_driving in the whole tree. sweep C-MAIN-1, 2026-08-15

### `F-BRW-07` — ledger line 256, currently **half-proven**

- **Files:** rust/crates/tiller_ui/src/browser.rs, rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** half-proven (unchanged, independently re-verified via grep): request_permission has exactly 3 call sites in the tree -- browser.rs:887 (its own wrapper body) and 2 #[test] sites (1711,1722) -- zero production callers. Persisted-reload half (main.rs) stands proven. sweep C-U, 2026-08-15

### `F-BRW-09` — ledger line 258, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Direct re-read of chat.rs this pass: same render_plain_text/open_url code paths confirmed still present, unchanged from ledger's live evidence. sweep C-MAIN-1, 2026-08-15

### `F-CHAT-34` — ledger line 183, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Whole-workspace grep: chat_sessions() has zero non-test callers anywhere outside tiller_persistence's own tests -- structurally absent, upgraded from NOT EXERCISED. sweep C-MAIN-1, 2026-08-15

### `F-CHAT-35` — ledger line 184, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** grep-reconfirmed: no empty-state string for past chats anywhere in chat.rs. sweep C-MAIN-1, 2026-08-15

