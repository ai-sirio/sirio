# Wave D slice D-P1 — 7 rows

Runs in parallel with every other slice. Every file your rows touch is listed below and no other slice owns any of them.

## Files you own

- `rust/crates/tiller_acp/src/lib.rs`
- `rust/crates/tiller_markdown/src/file_events.rs`
- `rust/crates/tiller_markdown/src/lib.rs`
- `rust/crates/tiller_ui/src/browser.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/project_forms.rs`

## Rows

### `F-CHAT-05` — ledger line 154, currently **half-proven**

- **Files:** rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Unchanged: guard code re-confirmed at chat.rs:1628; no new live drive attempted (no available agent on this box exercises the specific scenario). sweep C-CHAT-1, 2026-08-15

### `F-CHAT-14` — ledger line 163, currently **FAILED — defective**

- **Files:** rust/crates/tiller_markdown/src/file_events.rs, rust/crates/tiller_markdown/src/lib.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live: real restored chat tab, Follow ON, real Edit tool call completed and confirmed on disk, but tab strip still shows only Chat/Terminal -- no file tab opened (frame 02-after-turn2.png). Code read: restore_launch_snapshot (session.restore path) never gained the bind_chat loop the integrator added elsewhere. sweep C-CHAT-1, 2026-08-15

### `F-CHAT-18` — ledger line 167, currently **FAILED — absent**

- **Files:** rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Static: read vendored crate source directly at v1/agent.rs:3371 and v2/agent.rs -- confirms input_tokens/output_tokens/cached_read_tokens live only on a separate #[cfg(unstable_end_turn_token_usage)] Usage struct, not UsageUpdate. sweep C-CHAT-1, 2026-08-15

### `F-CHAT-33` — ledger line 182, currently **half-proven**

- **Files:** rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_ui/src/chat.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Code genuinely present & unit-tested (real fix from code-absent). Two live attempts (broken .mcp.json via fresh instance; direct SDK binary invocation) show the real claude CLI does not emit mcp-warning-shaped stderr during an ordinary turn for this failure mode -- transcript card never observed live. half-proven, not FAILED (code is real) and not PASSED (live behavior unconfirmed). sweep C-CHAT-3, 2026-08-15

### `F-PRJ-06` — ledger line 99, currently **half-proven**

- **Files:** rust/crates/tiller_ui/src/project_forms.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Critic re-drove the real popover chrome (labels cp1verify7/8/9) rather than inheriting the prior claim, and narrowed it: text entry DOES land when the URL field is clicked first (destination auto-derived), but button clicks do not -- Cancel left the form open across two drives despite visible hover highlight, and Clone repository with a valid URL left status on Ready to clone. Guard itself proven by the_drawn_clone_button_cannot_start_a_second_clone via direct mount; popover submit-click delivery is the confirmed structural gap, in sidebar.rs which this slice does not own. sweep C-P1, 2026-08-

### `F-PRJ-09` — ledger line 102, currently **half-proven**

- **Files:** rust/crates/tiller_ui/src/project_forms.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Critic re-ran the_drawn_create_button_cannot_start_a_second_creation (1 passed, direct-mounted CreateForm, two real create-submit clicks, exactly one Complete). Create popover not re-driven separately, but sidebar.rs render_project_form wraps Clone and Create in the identical absolute project-form-overlay whose submit-click delivery gap was just re-confirmed live for F-PRJ-06. sweep C-P1, 2026-08-15

### `F-BRW-01` — ledger line 250, currently **FAILED — defective**

- **Files:** rust/crates/tiller_ui/src/browser.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** X11 lane, DISPLAY=:1: pixel scan of two Browser tabs (one pre-existing, one freshly opened via ctl browser.open+tab.select this session, surface:15) both show content x=331..1059 vs pane's true chrome bounds x=386..1236 (measured via background scan) - 55px bleed into sidebar, 177px unpainted gap before Files panel. Frames 01-browser-click.png, 02-fresh-browser.png. Builder's own 'no shrink, no sidebar coverage' claim does not survive measurement. sweep C-P4, 2026-08-15

