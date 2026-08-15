# Wave C slice C-CHAT-3 — 6 rows

**Serialised slice — you are link 3 of 3 in the `C-CHAT` chain.** The links before you have already landed and the links after you have not started, so **no sibling holds your files while you work.** Commit only the files listed below.

## Files you own

- `rust/crates/tiller_acp/src/lib.rs`
- `rust/crates/tiller_markdown/src/file_events.rs`
- `rust/crates/tiller_markdown/src/lib.rs`
- `rust/crates/tiller_persistence/src/migrations.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/settings.rs`
- `rust/crates/tiller_usage/src/account.rs`

## Rows

### `F-CHAT-33` — ledger line 182, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_acp/src/lib.rs
- **Evidence on record:** Confirmed by grep: mcp_warnings() and ErrorKind::McpWarning have zero references anywhere in tiller_ui/src/chat.rs, matching prior 'code-absent, not a coverage gap' finding. Turn-error half this row's half-proven rests on is untouched by this slice. Nothing observable changed.

### `F-CORE-AUTH-01` — ledger line 408, currently **half-proven**

- **Files triage named:** rust/crates/tiller_usage/src/account.rs, rust/crates/tiller_ui/src/settings.rs
- **Evidence on record:** Code reconfirmed: parse_claude_json (account.rs:52) genuinely fed by settings.rs:548 from real `claude` stdout, no mock. No new capture; OAuth completion still not attempted. Half-proof unchanged.

### `F-PERSIST-DB-06` — ledger line 509, currently **half-proven**

- **Files triage named:** rust/crates/tiller_usage/src/account.rs, rust/crates/tiller_ui/src/settings.rs, rust/crates/tiller_persistence/src/migrations.rs
- **Evidence on record:** Migration v13 schema confirmed live (schema_version=13 via direct sqlite read). Drove Settings>AI Providers -- real discovery displayed 'Signed in e.palmisano@reply.it' on screen; account_identity table read 0 rows immediately after. Account half remains fully disconnected end to end.

### `F-SET-14` — ledger line 302, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/settings.rs
- **Evidence on record:** Render half works (Signing in.../Cancel renders stably, confirmed via rapid grim burst). Cancel half fails live and reproducibly: fast, fast+300ms, and generous 2.5s-delay tests (8 polls over 8s) all show the spawned x-terminal-emulator/codex-login PID still alive after Cancel; manual kill -TERM also failed to terminate it.

### `F-SET-15` — ledger line 303, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/settings.rs
- **Evidence on record:** Reconfirmed unchanged per instructions (stays half-proven pending build-vs-N/A ruling): live re-check across multiple real Add-Account clicks on Claude and Codex shows every provider card still has exactly one Accounts row; no second row ever appeared.

### `F-SET-20` — ledger line 308, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/settings.rs
- **Evidence on record:** Reconfirmed unchanged: toggle flips its local knob visually, but surface.settings.select (Appearance) still has no translucency key anywhere in values/top-level payload, same absence as before the click; SettingsSnapshot still has no translucency field so the value cannot leave the surface.

