# Wave C slice C-CHAT-1 — 7 rows

**Serialised slice — you are link 1 of 3 in the `C-CHAT` chain.** The links before you have already landed and the links after you have not started, so **no sibling holds your files while you work.** Commit only the files listed below.

## Files you own

- `rust/crates/tiller_acp/src/lib.rs`
- `rust/crates/tiller_markdown/src/file_events.rs`
- `rust/crates/tiller_markdown/src/lib.rs`
- `rust/crates/tiller_persistence/src/migrations.rs`
- `rust/crates/tiller_ui/src/chat.rs`
- `rust/crates/tiller_ui/src/settings.rs`
- `rust/crates/tiller_usage/src/account.rs`

## Rows

### `F-CHAT-02` — ledger line 151, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/settings.rs
- **Evidence on record:** Reproduced twice independently: default Chat tab renders a red error card ('could not launch ACP agent: ACP agent requires authentication (Login)') with working Retry, replacing prior wholly-blank transcript. Retry re-runs the live code path and stacks a second identical card, proving deterministic wiring. Still the pre-existing generic ErrorKind::Connection card, no dedicated AuthRequired banner styling, no CLI login-argv guidance.

### `F-CHAT-05` — ledger line 154, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Claude's ~/.claude/settings.json sets defaultMode:auto (global, not prompt-dependent); Codex ACP bridge fails auth outright (ACP agent requires authentication), confirmed live; Pi/OpenCode/omp have no ACP program at all. New guard code (can_send, insert_text/backspace/delete/send, placeholder branch) verified structurally + its fixture-agent test passes, but no live discriminating evidence of the gated behaviour itself was obtained.

### `F-CHAT-13` — ledger line 162, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Code now genuinely exists (real on_drop/drag_over gated on the F-CHAT-05 rule at chat.rs:5863, drop_external_paths at chat.rs:2083) and its two new tests drive real gpui::FileDropEvent objects and pass - moved off FAILED-absent. The live drag gesture itself is categorically unreachable by any harness on this machine (ENVIRONMENT.md documents the XDND limitation; the Wayland virtual-pointer client implements no drag-offer protocol), so it cannot be promoted further.

### `F-CHAT-14` — ledger line 163, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_markdown/src/file_events.rs, rust/crates/tiller_markdown/src/lib.rs
- **Evidence on record:** Toggled Follow Edited Files on via real overflow menu, sent a real turn to a live claude-agent-acp session that edited README.md (confirmed on disk); claude-agent-acp tools.js reports a real non-empty location. No file tab opened - two independent forced-repaint captures after the edit show only pre-existing Chat/Terminal tabs. Root cause: TillerWorkspace::bind_chat is only wired from add_chat_tab/resume_chat; restore_tabs and restore_tabs_in_workspace (the paths a real user actually hits) build TabContent::Chat di

### `F-CHAT-15` — ledger line 164, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_acp/src/lib.rs
- **Evidence on record:** Live-clicked mode/model pill on real session (no fixture): before/after frames pixel-identical in pill region, no dropdown/popover opens. Confirmed structurally: AcpClient::mode_catalog()/set_mode() have zero call sites in chat.rs; pill click handler still only flips a hardcoded 4-way UI-state string. Verdict unchanged from ledger.

### `F-CHAT-18` — ledger line 167, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_acp/src/lib.rs
- **Evidence on record:** Re-confirmed tiller_acp::ContextUsage (rust/crates/tiller_acp/src/lib.rs:210) carries only used/size/cost, no input/output/cache-token fields; none of this wave's three code commits (82d4ec5, 7c8c787, ce3c13e) touch the context-usage render arm - only the doc commit (62904af) mentions this row. Confirmation, not a new drive.

### `F-CHAT-20` — ledger line 169, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Re-confirmed independently: neither wayland-drive.sh's DSL nor wayland-virtual-pointer.c exposes a scroll/axis primitive (fresh grep, zero hits); follow-tail mechanism re-verified in source (chat.rs:939,993-1000). Manual-scroll-ownership half remains genuinely unreachable on this lane's tooling, X11 lane out of scope for this slice.

