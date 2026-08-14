# P98 Persistent Chat History Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist the GPUI chat transcript across relaunches and provide a browsable, openable, deletable chat-history surface with an empty state.

**Architecture:** Keep `chat_turn` as the serialized transcript store and add its activity timestamp plus a listing API. Carry the persisted `tab.id` through session restore and live `OpenTab` instances so the UI chat uses one durable identity. `tiller_ui::Chat` owns the save/load conversion; `TillerWorkspace` owns the history overlay and routes open/delete actions through the existing tab and prompt machinery.

**Tech Stack:** Rust 2024, Cargo workspace, rusqlite bundled SQLite, serde, GPUI, `#[test]` persistence tests, `#[gpui::test]` UI tests.

## Global Constraints

- Read and follow `docs/linux-rewrite/ENVIRONMENT.md` before implementation.
- Do not modify `docs/linux-rewrite/INVENTORY-LEDGER.md`.
- Use the existing `ChatTranscript` / `ChatTurn` / `ChatEntry` persistence shape; do not duplicate ACP protocol models.
- Save only settled turns, after `TurnEnded`; never write one row per streamed token.
- Preserve `retained_chats` as the same-process reopen fallback.
- Use path-scoped `git add -- <paths>`; never use `git add -A`.
- Keep existing unrelated dirty changes in the worktree intact.
- Use Conventional Commit messages with lower-case imperative subjects.

---

### Task 1: Add durable chat listing metadata and APIs

**Files:**
- Modify: `rust/crates/tiller_persistence/src/model.rs`
- Modify: `rust/crates/tiller_persistence/src/lib.rs`
- Modify: `rust/crates/tiller_persistence/src/migrations.rs`
- Modify: `rust/crates/tiller_persistence/src/db.rs`
- Test: `rust/crates/tiller_persistence/tests/persistence_integration.rs`

**Interfaces:**
- Produces `ChatSessionSummary { tab_id: String, title: String, agent_id: Option<String>, turn_count: usize, last_activity: i64 }`.
- Produces `AppDatabase::chat_sessions(&self, worktree_id: &str) -> Result<Vec<ChatSessionSummary>, PersistenceError>` ordered newest activity first, then tab order.
- Produces `AppDatabase::delete_chat_session(&self, tab_id: &str) -> Result<bool, PersistenceError>`; it removes `chat_turn` rows and returns whether rows existed.
- Keeps `save_chat_transcript` and `load_chat_transcript` signatures unchanged for `tiller_acp`.

- [ ] **Step 1: Write the failing persistence tests**

Add one integration test that creates a project, worktree, and two chat tabs, saves transcripts, calls `chat_sessions`, and asserts title, agent ID, turn counts, and newest-first `last_activity`. Add a second test that asserts `delete_chat_session` removes the listing and makes `load_chat_transcript` return `None` while leaving the `tab` row present.

```rust
#[test]
fn chat_sessions_list_saved_tabs_by_activity_and_delete_only_transcript() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("chat-history")).expect("open");
    db.save_project(&sample_project("project", "Project")).expect("project");
    db.save_worktree(&sample_worktree("worktree", "project", "main")).expect("worktree");
    let mut first = sample_tab("chat-1", "worktree", "First chat", "chat");
    first.agent_id = Some("codex".into());
    let second = sample_tab("chat-2", "worktree", "Second chat", "chat");
    db.save_tabs("worktree", &[first, second]).expect("tabs");
    db.save_chat_transcript(&ChatTranscript { tab_id: "chat-1".into(), turns: vec![sample_turn("one")] }).expect("first");
    db.save_chat_transcript(&ChatTranscript { tab_id: "chat-2".into(), turns: vec![sample_turn("two"), sample_turn("three")] }).expect("second");

    let sessions = db.chat_sessions("worktree").expect("list");
    assert_eq!(sessions.iter().map(|session| session.tab_id.as_str()).collect::<Vec<_>>(), ["chat-2", "chat-1"]);
    assert_eq!(sessions[0].title, "Second chat");
    assert_eq!(sessions[0].turn_count, 2);
    assert!(sessions[0].last_activity >= sessions[1].last_activity);

    assert!(db.delete_chat_session("chat-2").expect("delete"));
    assert!(db.chat_sessions("worktree").expect("list after delete").iter().all(|session| session.tab_id != "chat-2"));
    assert!(db.tabs_of_worktree("worktree").expect("tabs after delete").iter().any(|tab| tab.id == "chat-2"));
}
```

- [ ] **Step 2: Run the focused test and verify the expected failure**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller_persistence --test persistence_integration chat_sessions_list_saved_tabs_by_activity_and_delete_only_transcript -- --nocapture`

Expected: FAIL to compile because `ChatSessionSummary`, `chat_sessions`, and `delete_chat_session` do not exist.

- [ ] **Step 3: Add the model, migration, and minimal database implementation**

Add `ChatSessionSummary` and export it. Append migration v12 that adds `updated_at INTEGER NOT NULL DEFAULT 0` to `chat_turn`. In `save_chat_transcript`, capture Unix milliseconds once per save and write it with every retained turn. Implement the listing query as a grouped join of `tab` and `chat_turn` filtered to the requested worktree and `kind = 'chat'`; ignore tabs with zero transcript turns. Implement deletion as `DELETE FROM chat_turn WHERE tab_id = ?1`, returning the affected-row count. Keep quarantine and transcript decode behavior unchanged.

- [ ] **Step 4: Run the focused test and the package suite**

Run the focused command again, then run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller_persistence`

Expected: the new test and all existing persistence tests pass with exit code 0.

- [ ] **Step 5: Commit the persistence slice**

```bash
git add -- rust/crates/tiller_persistence/src/model.rs rust/crates/tiller_persistence/src/lib.rs rust/crates/tiller_persistence/src/migrations.rs rust/crates/tiller_persistence/src/db.rs rust/crates/tiller_persistence/tests/persistence_integration.rs
git commit -m "feat: list and delete persisted chat sessions"
```

### Task 2: Carry stable tab IDs through session restore

**Files:**
- Modify: `rust/crates/tiller/src/session.rs`
- Modify: `rust/crates/tiller/src/main.rs`
- Test: `rust/crates/tiller/src/session.rs`
- Test: `rust/crates/tiller/src/main.rs`

**Interfaces:**
- `SessionTab` gains `pub id: String`.
- `SessionLayout` writes each `SessionTab.id` as the `TabRecord.id` and corresponding `TabStateRecord.tab_id`.
- `OpenTab` gains `persistence_id: String`; restored tabs copy it from `SessionTab.id`, and newly-created tabs receive a unique path-scoped ID before chat launch.
- `TillerWorkspace::layout` emits each open tab's `persistence_id`; tab movement does not change it.

- [ ] **Step 1: Write the failing stable-ID round-trip test**

Extend the existing session round-trip test with non-positional IDs and assert that restore returns them unchanged after the layout is saved. Add a main-level pure test that `merge_launch_snapshot_tabs` matches an existing tab by persisted ID even when its title changes.

```rust
#[test]
fn a_layout_preserves_stable_tab_ids() {
    let dir = TempDir::new();
    let checkout = dir.0.join("checkout");
    std::fs::create_dir_all(&checkout).expect("checkout");
    let tabs = vec![SessionTab { id: "chat-stable".into(), title: "Chat".into(), kind: "chat".into(), agent_id: None, active: true }];
    let store = SessionStore::open(&dir.db_path("stable-ids"));
    store.schedule(layout(&checkout, tabs.clone()));
    store.flush_now();
    assert_eq!(restore(&dir.db_path("stable-ids"), Path::new("/tmp")).tabs, tabs);
}
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller a_layout_preserves_stable_tab_ids -- --nocapture`

Expected: FAIL to compile because `SessionTab` has no `id` field.

- [ ] **Step 3: Implement ID propagation and update constructors**

Add the field and update all session fixtures and `SessionTab` literals. In `restore_from`, copy `record.id`. In `write_layout`, use the supplied ID and generate a new ID only when a test/fallback tab has an empty ID. Add a `new_tab_persistence_id` helper using the stable worktree ID plus a process-unique timestamp/counter. Update every `OpenTab` constructor, restore helper, tab-add path, and test fixture to provide `persistence_id`.

- [ ] **Step 4: Run session/main tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller a_layout_preserves_stable_tab_ids -- --nocapture` and `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller merge_launch_snapshot_tabs -- --nocapture`.

Expected: both pass, and the existing `tiller` library tests still compile.

- [ ] **Step 5: Commit the stable-ID slice**

```bash
git add -- rust/crates/tiller/src/session.rs rust/crates/tiller/src/main.rs
git commit -m "fix: preserve chat tab identities across relaunch"
```

### Task 3: Connect the GPUI Chat to `chat_turn`

**Files:**
- Modify: `rust/crates/tiller_ui/Cargo.toml`
- Modify: `rust/crates/tiller_ui/src/chat.rs`
- Modify: `rust/crates/tiller_ui/src/lib.rs`
- Modify: `rust/crates/tiller/src/main.rs`
- Test: `rust/crates/tiller_ui/src/chat.rs`

**Interfaces:**
- `Chat::launch_with_command_and_persistence(command, cwd, database_path, tab_id, cx) -> Chat` loads before starting ACP.
- `Chat::persisted_transcript(&self) -> Option<ChatTranscript>` returns only complete turns ending in `TurnFooter`.
- Existing `Chat::restore_transcript(&str, ...)` remains for `retained_chats`.
- The UI maps rendered `Entry` variants to/from existing `ChatEntry` values and writes through `AppDatabase` on settle.

- [ ] **Step 1: Write failing conversion and settle tests**

Add a pure unit test for a user/assistant/tool/footer sequence returning one `ChatTurn` and excluding a trailing in-progress assistant entry. Add a GPUI test that seeds a database transcript, launches a persistent chat with the fixture command, pumps until connected, and asserts the restored visible transcript contains the saved user text.

```rust
#[test]
fn persisted_transcript_contains_only_completed_turns() {
    let entries = vec![Entry::User("inspect".into()), Entry::Assistant { text: "done".into(), document: parse("done") }, Entry::TurnFooter("12:00".into()), Entry::User("still streaming".into())];
    let transcript = Chat::transcript_from_entries("tab-chat", &entries);
    assert_eq!(transcript.turns.len(), 1);
    assert_eq!(transcript.turns[0].entries.len(), 3);
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller_ui persisted_transcript_contains_only_completed_turns -- --nocapture`

Expected: FAIL to compile because the persistence conversion helper and constructor do not exist.

- [ ] **Step 3: Implement minimal Chat persistence**

Add the `tiller_persistence` dependency. Store an optional `{ database_path, tab_id }` seam on `Chat`. Load with `AppDatabase::open` in the persistent constructor before `start_connection`. Convert every supported `Entry` to the corresponding `ChatEntry` and reconstruct parsed assistant documents, collapsed thoughts/tools, permission outcomes, plans, footers, and errors. At `TurnEnded`, expire unanswered cards, append the footer, save the complete transcript, then drain a queued prompt. On `new_conversation`, clear the persisted transcript after resetting entries. Log persistence failures and keep the live surface functional.

- [ ] **Step 4: Wire persistent constructors in restore/new/open paths and run tests**

Pass `database_path` and `tab.persistence_id` from both restore helpers and `add_chat_tab`. Flush the newly scheduled layout after adding a chat tab so its foreign-key tab row exists before the first completed turn can save. Keep retained-chat reopen on the string-based method. Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller_ui persisted_transcript_contains_only_completed_turns -- --nocapture` and `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller_ui`.

Expected: the conversion test, relaunch/restore test, and existing UI suite pass.

- [ ] **Step 5: Commit the UI persistence slice**

```bash
git add -- rust/crates/tiller_ui/Cargo.toml rust/crates/tiller_ui/src/chat.rs rust/crates/tiller_ui/src/lib.rs rust/crates/tiller/src/main.rs
git commit -m "feat: persist the GPUI chat transcript"
```

### Task 4: Add browse/open/delete/empty-state chat history UI

**Files:**
- Modify: `rust/crates/tiller/src/command_palette.rs`
- Modify: `rust/crates/tiller/src/main.rs`
- Test: `rust/crates/tiller/src/main.rs`

**Interfaces:**
- Adds `TabCommand::OpenChatHistory` and the visible `Chat History` palette command.
- `TillerWorkspace` owns `chat_history_open: bool` and `chat_history: Vec<ChatSessionSummary>`.
- `reload_chat_history` reads summaries for the current persisted worktree.
- `open_chat_history_session(tab_id)` selects the existing tab; if absent, restores a chat tab with the same persisted ID and transcript.
- `confirm_delete_chat_history_session(tab_id, window, cx)` uses `PromptLevel::Warning` and deletes only after the prompt returns the confirm index.

- [ ] **Step 1: Write failing state/UI tests**

Add a palette catalog assertion for `Chat History`. Add a GPUI workspace test that opens history with no saved transcript and asserts `chat-history-empty`; add a prompt test that clicks a saved row's delete control, asserts a pending prompt and unchanged DB/list before confirmation, then confirms and asserts the row disappears.

```rust
#[gpui::test]
async fn chat_history_starts_with_an_explicit_empty_state(cx: &mut TestAppContext) {
    cx.set_global(Theme::light());
    let window = cx.add_window(|_, cx| palette_test_workspace(cx));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    let workspace = visual.update(|window, _| window.root::<TillerWorkspace>().flatten().expect("workspace"));
    workspace.update(&mut visual, |workspace, cx| workspace.open_chat_history(cx));
    visual.run_until_parked();
    assert!(visual.debug_bounds("chat-history-empty").is_some());
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller chat_history_starts_with_an_explicit_empty_state -- --nocapture`

Expected: FAIL to compile because the history state, palette command, and selectors do not exist.

- [ ] **Step 3: Implement the overlay and typed routes**

Add the palette command and dispatch. Render a workspace overlay with a title, close control, rows showing title/agent/last activity, Open and Delete controls, and the explicit no-past-chats copy. Use child `on_mouse_down` propagation stops for delete controls so a delete click cannot also open the session. Route deletion through the warning prompt. When opening a session already represented by a tab, select it; otherwise create the chat tab with the stored stable ID and loaded transcript. Refresh the listing after every save/delete/open.

- [ ] **Step 4: Run UI tests and package checks**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller chat_history_starts_with_an_explicit_empty_state -- --nocapture`, then `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller`, then `PATH="$HOME/.cargo/bin:$PATH" cargo test -p tiller_ui`.

Expected: all commands exit 0; the delete test demonstrates that cancellation does not mutate the DB and confirmation removes the row.

- [ ] **Step 5: Commit the history browser slice**

```bash
git add -- rust/crates/tiller/src/command_palette.rs rust/crates/tiller/src/main.rs
git commit -m "feat: browse persisted chat history"
```

### Task 5: Final verification and handoff

**Files:**
- Verify only; do not edit `docs/linux-rewrite/INVENTORY-LEDGER.md`.

- [ ] **Step 1: Inspect the path-scoped diff and status**

Run: `git diff HEAD~4..HEAD --stat` and `git status --short`.

Expected: only the P98 commits contain the design, plan, persistence, session, UI chat, command-palette, and workspace files; unrelated pre-existing dirty files remain untouched and the inventory ledger is absent.

- [ ] **Step 2: Run the whole repository gate**

Run: `PATH="$HOME/.cargo/bin:$PATH" Scripts/ci.sh`

Expected: exit code 0 and output containing `CI OK`.

- [ ] **Step 3: Report live evidence separately from code evidence**

If the display lane is available, use a fresh fixture DB: open a chat, complete one response, quit, relaunch, open Chat History, open the saved row, cancel a delete prompt, then confirm a delete. Report each clause separately. If no live lane is available, report the code/test coverage as `NOT EXERCISED`, never `PASSED`.
