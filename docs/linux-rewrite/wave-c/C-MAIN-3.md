# Wave C slice C-MAIN-3 — 15 rows

**Serialised slice — you are link 3 of 4 in the `C-MAIN` chain.** The links before you have already landed and the links after you have not started, so **no sibling holds your files while you work.** Commit only the files listed below.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-EDIT-08` — ledger line 226, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** NOT EXERCISED 2026-08-14: dbus-monitor on FileChooser genuinely empty across two attempts (confirmed against driver's committed capture, git show 4fb4642). Cannot separate 'picker absent' from 'this lane's modifier-chord key delivery unproven' (WAYLAND-LANE.md). Prior visual-absence basis for FAILED-defective was already doubted by triage. NOTE: working-tree copy of one capture file is now contaminated by an apparent concurrent sibling's overwrite (unrelated Add Project FileChooser call) -- judge from git history, 

### `F-GIT-BRANCH-01` — ledger line 486, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_git/src/branches.rs, rust/crates/tiller_ui/src/sidebar.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** P116 confirmed a worktree/branch via raw git branch --all and socket list-workspaces, but never calls GitBranches::list/list_branches (branches.rs:14,36) - zero callers outside tests/p99_git_rows.rs (grep-confirmed). No branch name containing a space was created or listed through the Git layer as VERIFY requires.

### `F-PER-01` — ledger line 237, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_acp/src/chat.rs
- **Evidence on record:** pass 17 overturns the pass-14 PASSED for the clause's **chats** object: after TWO completed real ACP exchanges in the UI chat tab (>20s settle before the kill), `chat_turn` held **0 rows** and `session_ref` **0 rows** (WAL-aware read-only read), and the relaunched app showed an EMPTY transcript (p17-ai0-restored). Debounce loss is ruled out: settings and tab_state written by the same app life survived the same SIGTERM. The pass-14 tests are real but prove the STORE and the SOCKET door — the UI chat path never write

### `F-PER-08` — ledger line 244, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** half-proven (unchanged). Critic independently confirmed via source read (main.rs newly_allowed / browser.* dispatch) that no socket path can synthesize a permission grant, and cross-checked against F-WIN-06's fresh capture showing the lane's browser still renders chrome only. General-settings half stays proven from prior evidence; browser-origin half remains could-not-reach on this Wayland lane.

### `F-PERSIST-DB-05` — ledger line 508, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_acp/src/chat.rs
- **Evidence on record:** **the persistence was built on the path the user does not use.** Per conjunct: (a) legacy Swift tab-table — absent by design on this lineage, unchanged; (b) *"chat sessions/items persist transcript metadata and content"* — the **store** does this and the replayed test proves the store, but the row's own VERIFY is app-level: *"Create … chat tabs, **restart**, and inspect … transcript restoration."* That cannot happen. `load_chat_transcript` has exactly **2 production callers, both in `tiller_acp/src/chat.rs`** (the 

### `F-PRJ-03` — ledger line 96, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Evidence on record:** Live-driven: sidebar + menu fully enumerated — only Clone Repository… and Create Project…, no Open Project/browse entry exists. ctrl+o probe inconclusive (portal invisible to X11), not counted as a second negative. shots/22.

### `F-SET-04` — ledger line 292, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** DB half closed live (own click + 2x kill/relaunch, screenshots match); functional half ('restored agent-session behavior changes accordingly') unproven and independently confirmed absent by F-CORE-ACT-24 (real restarts never use --resume/--continue) and by resume_command() having zero production call sites

### `F-SET-09` — ledger line 297, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/settings.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** Click now confirmed to reach a real, wired consumer (source + 2 green drawn tests) — closes the 'is there even a route' question. Feedback/control-state-change half unproven and structurally absent: no install-status field anywhere in source; screenshot shows button unchanged post-click, matching an independently-viewed earlier frame (d1h)

### `F-SET-10` — ledger line 298, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/status_bar.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** Live-drove myself (wayland lane, adjset10): real click toggled Claude Show in usage bar OFF (Codex stayed ON, ruling out a stuck default); DB wrote usage.claudeVisible=false; fresh relaunch against the same DB read it back OFF from disk alone. DB half absent is stale - main.rs:8032-8066 + session.rs:1663-1664 wire it end to end. Owed half: refresh-interval-change and Refresh-now-confirmed-update, still test-only.

### `F-SET-18` — ledger line 306, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/settings.rs, rust/crates/tiller_agents/src/lib.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** Live 03-02-settings-agents.png reconfirms all 5 agent rows show only binary path + pill (ACP chat available/No ACP server); no Install/progress/Update/Retry control drawn. Reconfirms existing verdict; nothing new to close the missing half with.

### `F-SET-22` — ledger line 310, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/settings.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** pass 13's "display-only pills, no on_click" is **stale** — the swatch is clickable and the choice reaches `SettingsSnapshot.agent_colors` (green drawn test `agent_color_click_selects_a_new_accent_and_persists`, settings.rs:3547, real `simulate_click`, and only the clicked row moves). **The defect is downstream**: `agent_colors` has zero references anywhere outside settings.rs — no consumer reads it — and `app_settings_from_snapshot` (main.rs:7582) drops it, `AppSettings` having no such field. So the clause's second

### `F-SID-06` — ledger line 75, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/sidebar.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** UNCHANGED — P109's ### F-SID-06 section actually exercises the real F-SID-15 clause (worktree removal via hover ×), not this row's collapsed-project descendant-badge clause. No P109 evidence exists for this row. Existing P106 half-proven verdict confirmed as-is (dots on worktree rows shown live; collapsed-project badge not exercised, code gates dots to worktree rows).

### `F-SID-11` — ledger line 80, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/sidebar.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** UNCHANGED — P109's ### F-SID-11 section actually drives a context-menu census (Set Primary + New-Tab items + no Remove Worktree item), not this row's branch/folder/primary/comment/status display clause. No P109 evidence exists for this row. Existing P106 half-proven verdict confirmed as-is (branch/path/Primary/status shown live; no folder-worktree row exists, comment not rendered).

### `F-SID-12` — ledger line 81, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/sidebar.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** half-proven (unchanged). Critic re-confirmed BTN_LEFT hardcoding (wayland-virtual-pointer.c) and checked the command-palette 'Set Primary Worktree' entry as a possible alternate route — it also requires the same out-of-scope ctrl-shift-p chord, so no chord-free path exists. Matches existing record.

### `F-SID-15` — ledger line 84, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/sidebar.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** Promoted from NOT EXERCISED using the evidence filed under P109's ### F-SID-06 header (mislabeled — see notes). Named entry point (context-menu Remove Worktree item) proven absent by a full menu census. Only door is hover × — clicking it removed the worktree AND its on-disk directory with zero confirmation of any kind. Disappears-from-sidebar half proven true; confirm-prompt half proven false. Destructive action, no safety confirmation, not reachable via the clause's named route. shots/110,121,123.

