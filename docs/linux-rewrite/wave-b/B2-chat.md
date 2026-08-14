# Wave B slice B2-chat — 6 rows to build

**chat surface**

## Files you own this wave

- `rust/crates/tiller_ui/src/chat.rs`

No other agent will touch these. **You must not edit any file outside this list** —
every other source file belongs to a sibling and an edit there is silently lost or
silently overwrites theirs. If a fix genuinely needs a file you do not own, stop and
report it rather than making the change.

## Rows

### `F-CHAT-05` — ledger line 154, currently **half-proven**

- **Triage:** both, size M
- **Files triage expects:** rust/crates/tiller_ui/src/chat.rs
- **Approach:** can_send() ignores pending_question() entirely (relies incidentally on `streaming`); composer placeholder never distinguishes permission-wait from ordinary mid-turn queueing. Decide whether queueable-not-disabled satisfies the clause, then live-drive a genuine unresolved permission_request (this CLI auto-applies edits instead; try a non-allow-listed Bash call or a different agent).
- **Evidence on record:** Live escalation to a file-write prompt (02-f05-write-mid.png) produced an auto-applied Edit card with Pending/Revert, not a true ACP permission_request -- no Allow/Deny option buttons, confirmed by direct frame inspection. Composer stayed queueable ('Type to queue for the next turn...') during that state, matching F-CHAT-06, not newly proving F-CHAT-05's permission-wait clause. This build's instal

### `F-CHAT-12` — ledger line 161, currently **FAILED — defective**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/composer.rs
- **Approach:** Chip removal (remove_composer_chip) never explicitly re-requests composer_focus; the ancestor container's on_mouse_down that normally grabs it isn't recovering focus post-removal live. Add an explicit window.focus(&self.composer_focus, cx) to the chip-remove click handler or to remove_composer_chip itself.
- **Evidence on record:** conjunction split (P92 live): the removal itself works at BOTH levels — chip disappeared from the composer (p92-o4b→o5b) and the next sent message's payload carried no file part (JSONL: plain-text "Say only: yes") — **but the × leaves the composer keyboard-dead**: after clicking ×, five recovery attempts a real user would make (field click, typing, Return, Escape, field click again) all produced n

### `F-CHAT-13` — ledger line 162, currently **FAILED — absent**

- **Triage:** reclassify, size M
- **Files triage expects:** rust/crates/tiller_ui/src/chat.rs
- **Approach:** grep -rn ExternalPaths crates/ is empty repo-wide; chat.rs has zero .on_drop of any kind. This is code-absent, confirmable without any driver tooling, not NOT EXERCISED as currently recorded — should read FAILED — absent like F-CHAT-35. Build: an ExternalPaths drop target, the attach overlay, chip/message wiring, and a rejection state for unsupported types.
- **Evidence on record:** Confirmed on_drop/ExternalPaths are real GPUI primitives used elsewhere in this app (terminal, sidebar, right_panel, pane divider drag types) but zero uses for chat's file-drop case and zero ExternalPaths anywhere in rust/crates — structural absence, not an unreached instrument wall. Reclassified from NOT EXERCISED.

### `F-CHAT-14` — ledger line 163, currently **FAILED — defective**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_markdown/src/file_events.rs, rust/crates/tiller_markdown/src/lib.rs
- **Approach:** following_edited_files toggles a bool nothing reads; FileSystemEventMonitor (the watcher that would implement it) has zero consumers in tiller/src or tiller_ui/src. Wire the watcher's events into chat.rs when the toggle is on and a tool call reports an edited-file location.
- **Evidence on record:** conjunctive clause, one half live and one half dead; the dead half is settled without a drive. **Follow Edited Files does nothing.** `following_edited_files` (chat.rs:532) has exactly six references in the workspace: declaration, init `false` (:624), its own menu label (:3363), its own toggle (:3401), and two test assertions (:6064/:6073) that the bool flipped. Nothing reads it to act. The watcher

### `F-CHAT-16` — ledger line 165, currently **FAILED — absent**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/chat.rs
- **Approach:** The existing model_picker popover (shared with the already-PASSED F-CHAT-17 effort selector) genuinely lacks a search field and any 'Recommended' label/no-match state — confirmed absent by reading the whole render arm. Model selection itself already works when available_models is non-empty. Add search filtering and a Recommended designation (check tiller_acp::ModelCatalog for a recommended flag first).
- **Evidence on record:** Live-driven: pill click opens an Effort selector, not a model picker — no search, no list, no Recommended label; clicking High does update the badge, proving clicks land. Popover also undismissable. shots/62.

### `F-CHAT-18` — ledger line 167, currently **FAILED — absent**

- **Triage:** reclassify, size S
- **Files triage expects:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_acp/src/lib.rs
- **Approach:** A fully click-wired (toggle_context_popover on context-ring's on_click), Escape-dismissable, drawn-tested (context_ring_shows_reported_usage_and_escape_dismisses_popover) context popover already exists at HEAD, directly contradicting the 'opens nothing' verdict — captured in the same pre-h_full-fix P109 batch as F-CHAT-02. Re-drive first. Real residual gap once re-verified: ContextUsage carries only used/size/cost, no separate input/output/cache breakdown the clause names.
- **Shared cause:** Same pre-h_full-fix P109 drive batch as F-CHAT-02 and F-CHAT-16 (commit 2f3a1b4, 18 min before fix 23af26c).
- **Evidence on record:** Kept FAILED — absent but on new grounds: ContextUsage (tiller_acp/src/lib.rs:164) has no input/output/cache fields anywhere in the ACP layer, so the clause's named breakdown is structurally impossible regardless of click success — corroborated independently by 3 other repo audits of the same struct. Old 'click opens nothing' evidence confirmed stale (pre-.h_full()-fix, composer pinned to window to

