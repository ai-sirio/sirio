# Wave C slice C-CHAT-2 — 7 rows

**Serialised slice — you are link 2 of 3 in the `C-CHAT` chain.** The links before you have already landed and the links after you have not started, so **no sibling holds your files while you work.** Commit only the files listed below.

## Files you own

- `rust/crates/tiller_acp/src/lib.rs`
- `rust/crates/tiller_markdown/src/file_events.rs`
- `rust/crates/tiller_markdown/src/lib.rs`
- `rust/crates/tiller_persistence/src/migrations.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/settings.rs`
- `rust/crates/tiller_usage/src/account.rs`

## Rows

### `F-CHAT-25` — ledger line 174, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Triage's 'try Pi's ui/select' instruction was not carried out this pass — driver did a source-only re-check (Entry::Permission.text_input chat.rs:208, render_question_answer_row chat.rs:2748, both confirmed present) and confirmed pi/codex are installed, but ran no live ACP turn. Core blocker (installed Claude Code CLI never raises real AskUserQuestion) unretested. Stays NOT EXERCISED.

### `F-CHAT-26` — ledger line 175, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Captures confirm Chat tab genuinely foregrounded before send, retiring the wrong-tab theory as sole blocker — but no Entry::Permission was raised this pass (CLI auto-approved via the 'run' skill under a plain shell prompt), so the pending-bar visual state was still not reached. Owed: a plan-mode turn (the route that worked previously) with the tab now correctly foregrounded.

### `F-CHAT-27` — ledger line 176, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** No permission entry appeared this pass (same drive as F-CHAT-26), so surface.chat.stop was never exercised against a pending permission and the 'No answer — the turn ended' visual was not recaptured. Prior data-model proof (chat.rs:1479-1512, 3839-3846, 5448-5449) unchanged, unretested this pass.

### `F-CHAT-28` — ledger line 177, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Render half PROVEN LIVE: real ACP Task-tool round trip produced a visually-distinct Subagent card (purple label, rail border, collapsed chevron), confirmed by opening f28/02-02-after-send.png against render_subagent_task_card (chat.rs:4072). Not absent -- ledger's prior evidence (pass 8) predates the build commit. Click-to-expand-card + nested-tool-call gesture (VERIFY's other half) not attempted, though the header is a large always-visible full-width click target whose position was computable from the driver's own

### `F-CHAT-29` — ledger line 178, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Not absent: CopyTarget::Assistant hover-Copy control confirmed live in source (chat.rs:3628-3670, built 398c0aea, after ledger's stale pass-17 evidence). Not proven live: attempted click at (1350,186) landed in the Files panel, not the Chat column (right edge ~x=1176) -- a miscalibrated guess, not a proof of impossibility, though the control's hover-only visibility (no baseline frame shows it) is a real blocker independent of that miss. Paste-confirmation half also has no keyboard path on this lane (only right-clic

### `F-CHAT-30` — ledger line 179, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Not absent: code-block-copy-{entry}-{id} control confirmed live in source (chat.rs:2968-2974, built 0ecbd525, after ledger's stale pass-17 evidence). No live drive this pass -- driver correctly reused F-CHAT-29's structural finding (per-block position is strictly less locatable than the already-hard per-message control) rather than repeat a failed-geometry attempt. NOT EXERCISED, not FAILED-absent.

### `F-CHAT-32` — ledger line 181, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Trigger half PROVEN LIVE: real ACP Write tool call produced a render_edit_summary card (chat.rs:3442) confirmed by opening f32/02-02-after-send.png, cross-checked against the actual file on disk (cat + git status, not just chat text). Not absent. Open (file-path text, chat.rs:3479) and Revert->Confirm Revert (chat.rs:3517-3568) gesture not attempted, though both controls are always-visible and statically positioned (single diff, single row) in the driver's own capture -- 'could-not-reach' overstates the blocker; th

