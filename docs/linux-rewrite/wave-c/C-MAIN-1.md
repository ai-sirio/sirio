# Wave C slice C-MAIN-1 — 15 rows

**Serialised slice — you are link 1 of 4 in the `C-MAIN` chain.** The links before you have already landed and the links after you have not started, so **no sibling holds your files while you work.** Commit only the files listed below.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-AGENT-OMP-03` — ledger line 471, currently **half-proven**

- **Files triage named:** rust/crates/tiller_agents/src/omp.rs, rust/crates/tiller_agents/src/lib.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** Command-string half proven: summarizer_command exists in omp.rs, matches spec, unit test reran green. Live-run half independently reconfirmed still blocked: ran the exact generated command (oh-my-pi --print --no-tools '<prompt>') myself, identical upstream SyntaxError as OMP-01. main.rs has no caller for any adapter (F-SET-05's separately-tracked gap, not this row's).

### `F-AGENT-SAFE-02` — ledger line 473, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_agents/src/hook_migrator.rs, rust/crates/tiller_agents/src/claude.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** NOT EXERCISED (unchanged): P120's evidence proves F-AGENT-SAFE-01's clause, not this row's. ClaudeHookMigrator never invoked; still 0 refs outside its module (grep re-confirmed). Report overclaim, wrong-row filing.

### `F-AGENT-SESSION-01` — ledger line 474, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_agents/src/session_validator.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** P116's session.ref/restore-session drive exercises main.rs:1417's session_refs label map, not AgentSessionValidator::is_likely_valid (session_validator.rs:11) - zero callers outside test files (grep-confirmed). No Claude/Codex session file was created or removed; clause untouched.

### `F-AGENT-SESSION-02` — ledger line 475, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_agents/src/transcript.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** P116 honestly confirms no transcript/recent-text API exists in system.capabilities. Correctly reported as non-exercise, not an overclaim. ClaudeTranscriptSource-equivalent reader remains untested.

### `F-BRW-04` — ledger line 253, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Evidence on record:** **P96 headless lane:** raw `browser.open` with `url=https://` returned `ok:true` and silently fell back to `https://example.com`; raw `browser.navigate` to both `http://127.0.0.1:9/p96-unreachable` and `http://does-not-exist.invalid/p96-unreachable` returned `ok:true` with the requested URL, blank title, and no error field. The available browser control path exposes neither required invalid-address nor navigation-failure error

### `F-BRW-05` — ledger line 254, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Evidence on record:** Live-driven, positive+negative control: browser.act driving=true/false toggles an orange Agent driving pill on/off in the Browser toolbar (frames viewed, confirmed). Render wiring to agent_driving() is proven. But browser.act is the ONLY caller of set_agent_driving in the tree and is explicitly a manual stub ('only the driving flag is implemented') -- no real ACP browser action ever sets this flag; the clause's trigger half has no live implementation.

### `F-BRW-09` — ledger line 258, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** Live-driven: user-typed chat message with markdown link syntax rendered as literal unstyled raw text, not a link (capture confirmed). Source re-read confirms why: Entry::User uses render_plain_text (chat.rs:3606, no link parsing); only Entry::Assistant uses render_markdown (:3672). The sole click handler (chat.rs:777) unconditionally calls cx.open_url with no modifier check and no internal-tab routing anywhere in chat.rs -- neither the internal-tab-default nor the modifier-bypass half of the clause exists.

### `F-CHAT-34` — ledger line 183, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** No fresh evidence in P111/P114/P116 for this row directly. P114 cites it only as F-CHAT-35's blocking dependency ('F-CHAT-34's browser/open/delete UI has no production surface'), consistent with the standing finding: chat_sessions has no production caller, UI is resume-most-recent only.

### `F-CHAT-35` — ledger line 184, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** no no-past-chats empty state

### `F-CHG-02` — ledger line 193, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/right_panel.rs, rust/crates/tiller_ui/src/changes.rs
- **Evidence on record:** pass 13, exercised: `close-workspace` → `current-workspace` = none, then `surface changes open` still serves the last worktree's data (transcript) — there is no no-worktree state and no explanation text anywhere in changes.rs/right_panel.rs. Frame CHG-02b documents it

### `F-CHG-13` — ledger line 204, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/changes.rs
- **Evidence on record:** P104 §Group 6: Open diff added a generic Changes activity entry whose content remained the multi-file list, not an isolated file/diff tab.

### `F-CORE-ACT-10` — ledger line 346, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller/src/panes.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** NOT EXERCISED (downgraded): independent crop/compare found the claimed positive u0->u1 diff is a tab-active-highlight confound (tab glyph stays hollow in all 7 captures), and the w1/w2 pair shows an accidental live Chat/Claude-Code conversation (typed input misrouted from Terminal to Chat), not Layer-D detection. No valid discriminating evidence either way this pass.

### `F-CORE-ACT-19` — ledger line 355, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_activity/src/model.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** FAILED — defective: live D-Bus Notify title="Claude Code — finished" is agent+STATUS, clause requires agent+worktree-label. model.rs:448 confirms. Body format (branch+project) is correct. P120, 2026-08-14.

### `F-CORE-ACT-20` — ledger line 356, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/notification.rs
- **Evidence on record:** half-proven: live D-Bus proves visible pane suppresses, hidden pane notifies (both directions of the app_active/pane_visible gate). No-agent-running and identical-status suppression branches untested. P120, 2026-08-14.

### `F-CORE-ACT-24` — ledger line 360, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** FAILED — absent: 2 real restarts each spawn Claude Code with a fresh random --session-id, no --resume/--continue either time — restore-plan resumable/prunable distinction never applied, live. P120, 2026-08-14.

