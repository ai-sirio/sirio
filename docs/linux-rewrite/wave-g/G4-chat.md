# Wave G slice G4-chat — 4 rows

**Scheduling round 4 of 6.** You run alone this round.

Earlier rounds have landed; later rounds have not started. Every file below is yours exclusively right now — but **re-read each from disk before editing**; an earlier round may have changed it since this brief was written, so do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller_acp/src/lib.rs`
- `rust/crates/tiller_agents/src/error.rs`
- `rust/crates/tiller_control/src/client.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/file_view.rs`

## Rows

### `F-CHAT-05` — ledger line 154, currently **half-proven**

- **Files:** rust/crates/tiller_ui/src/chat.rs
- **Latest critic evidence (2026-08-16, current tree):** Confirmed offline-placeholder branch sits in Chat::render's live composer_parts path (not test-only), and offline_composer_shows_its_own_placeholder is a discriminating real-missing-binary test that passes; not independently live-driven against a real disconnected agent this pass, so kept below PASSED.

### `F-CHAT-20` — ledger line 169, currently **half-proven**

- **Files:** rust/crates/tiller_ui/src/chat.rs
- **Latest critic evidence (2026-08-16, current tree):** chat.rs scroll_handler (cx.defer re-arming FollowMode::Tail on is_scrolled_to_end) reads as mechanically correct on code review; not independently live-driven this pass given time lost to environment instability -- no new evidence beyond the builder's own honest non-live-driven admission.

### `F-CHAT-33` — ledger line 182, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_agents/src/error.rs, rust/crates/tiller_control/src/client.rs, rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/chat.rs
- **Latest critic evidence (2026-08-16, current tree):** looks_like_mcp_warning and its call site confirmed still present and unmodified via grep. Negative evidence rests on three independent live attempts across two prior sessions (not just this builder), so left unchanged; did not spend this pass's time on a fourth identical live attempt, agreeing with the builder's own reasoning that it would add no new information.

### `F-EDIT-07` — ledger line 225, currently **FAILED — defective**

- **Files:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/file_view.rs
- **Latest critic evidence (2026-08-16, current tree):** Opened a fresh README.md with an inline code span and a fenced `bash block (comment + command line) in Preview mode: the Markdown badge and inline-code coloring are correct, but 4x pixel zoom shows the fenced block's comment and command lines render in the exact same flat color — no per-token highlighting at all. Source confirms: CodeBlock rendering (chat.rs render_markdown_block, CodeBlock arm) goes through render_plain_text, which applies zero HighlightStyles, unlike Inline::Code which does; the editor's own code_spans highlighter never fires for the Preview/fenced-block path.`

