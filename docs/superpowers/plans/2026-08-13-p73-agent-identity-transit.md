# P73 agent identity transit Implementation Plan

> For agentic workers: use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Preserve a selected non-default ACP agent identity from SQLite through session restoration and chat resume, while surfacing the three P71 Half B failures in the existing sidebar/file notices.

**Architecture:** Extend the existing SessionTab value passed between tiller_persistence and main.rs with the already-persisted optional agent id. Centralize restoration metadata resolution so known ACP adapters use Chat::launch_with_command, while absent/unknown/non-ACP identities fall back to the normal default without claiming a wrong identity; retain the same metadata in RetainedChat. Route only the three specified stderr-only user actions to the already-existing notice setters.

**Tech Stack:** Rust, GPUI TestAppContext/VisualTestContext, tiller_persistence TabRecord v10, tiller_agents ACP catalog, tiller_ui Chat/FileView/Sidebar.

## Global Constraints

- Work only in /home/enzopalmisano/Scrivania/Progetti/tiller-linux, branch linux/gpui-waku.
- Do not edit tiller_ui/src/chat.rs, settings.rs, sidebar.rs, status_bar.rs, file_view.rs, tiller_agents/**, tiller_persistence/**, or the other owners' files named by P73.
- Keep the existing add_chat_tab None branch unchanged.
- Preserve unknown/absent identities as a safe default; never panic on legacy v9 rows.
- Use Codex, not Claude, as the persisted non-default identity in the round-trip regression.
- Run the gate's exact clippy command with -D warnings; report unrelated failures separately.
- Do not copy code from reference checkouts or rewrite unrelated shared-worktree changes.

### Task 1: Carry agent identity through SessionTab

Files:
- Modify rust/crates/tiller/src/session.rs:286-305, 585-830
- Test rust/crates/tiller/src/session.rs unit tests near the existing session round-trip tests

Interfaces:
- SessionTab gains pub agent_id: Option<String>.
- write_layout writes SessionTab.agent_id into TabRecord.agent_id.
- restore copies TabRecord.agent_id into SessionTab.agent_id.
- Fresh/default and legacy construction sites use None.

- [ ] Step 1: Write the failing persistence test

Add a unit test named a_codex_agent_identity_round_trips_through_session_store that creates a real temporary SQLite database, schedules a SessionLayout with one active chat SessionTab whose agent_id is Some("codex".into()), flushes the store, restores it, and asserts the restored tab's agent_id is exactly Some("codex".into()). The expected string is a literal and the test exercises SQLite rather than mirroring a builder.

- [ ] Step 2: Run the test and verify the expected compile failure

Run cargo test -p tiller --bin tiller session::tests::a_codex_agent_identity_round_trips_through_session_store.
Expected: compile failure because SessionTab has no agent_id field and/or the persistence mapping does not carry it.

- [ ] Step 3: Implement the minimal transit field

Add agent_id: Option<String> to SessionTab, set it to None in every default/test literal, write it to TabRecord.agent_id in write_layout, and read record.agent_id when rebuilding restored tabs.

- [ ] Step 4: Run the focused test and existing session tests

Run cargo test -p tiller --bin tiller session::tests::a_codex_agent_identity_round_trips_through_session_store session::tests::a_layout_saves_and_restores_identically.
Expected: both pass and the existing layout equality test remains unchanged for tabs with None.

### Task 2: Restore and resume the correct ACP agent

Files:
- Modify rust/crates/tiller/src/main.rs:293, 2643-2690, 3724, 3952-3980, 6640-6800
- Test rust/crates/tiller/src/main.rs existing resume drawn test plus a new launch/restore drawn test

Interfaces:
- RetainedChat gains agent_id: Option<String>.
- The current layout copies OpenTab.agent_id into SessionTab.agent_id.
- A restoration resolver maps a recognized adapter with an ACP program to its AgentCommand, icon, and canonical id; absent, unknown, or non-ACP ids use default_chat_command() and return no claimed agent identity.
- Both restore_tabs and restore_tabs_in_workspace launch with the resolver command and restore the resolved icon/id.
- resume_chat uses the same resolver and restores the retained icon/id.
- close_tab retains the closed chat's OpenTab.agent_id.

- [ ] Step 1: Write the failing resume assertion

Extend the existing drawn test drawn_tab_context_resume_chat_reopens_the_retained_session so its retained fixture is Codex and its assertion requires the reopened OpenTab.agent_id to be Some("codex"), not merely that a chat tab exists. Add the new agent_id field to the fixture only after observing the production compile failure.

- [ ] Step 2: Run the test and verify it fails for the old behavior

Run cargo test -p tiller --bin tiller drawn_tab_context_resume_chat_reopens_the_retained_session -- --nocapture.
Expected: first compile failure for the widened fixture, then after adding only the test field it fails the identity assertion because old resume creates Chat::launch with agent_id: None.

- [ ] Step 3: Implement identity retention and resolver

Add agent_id to RetainedChat, copy it from the closed OpenTab, copy every live tab's id into SessionTab in layout, and implement a small resolver using AGENT_CATALOG, adapter.acp_program(), acp_agent_command, and default_chat_command(). Use Chat::launch_with_command(command, working_directory, cx) in both restore functions and resume; no change to add_chat_tab's unreachable None guard.

- [ ] Step 4: Add and run the quit/relaunch drawn round-trip

Add drawn_restore_round_trips_codex_identity_through_quit_and_relaunch using a real temporary SessionStore/SQLite layout containing a Codex chat, restore, and the drawn shell pump. Assert the restored OpenTab.agent_id is Some("codex") and its icon is Some(Icon::Codex); do not use Claude as the expected identity.

Run cargo test -p tiller --bin tiller drawn_tab_context_resume_chat_reopens_the_retained_session drawn_restore_round_trips_codex_identity_through_quit_and_relaunch -- --nocapture.
Expected: both pass after the resolver and transit field are wired.

- [ ] Step 5: Add the safe fallback check

Add a focused unit test for the resolver with None and an unknown id, asserting no panic and no claimed adapter id while the returned command is the default command. Run it with the two drawn tests.

### Task 3: Wire P71 Half B notices

Files:
- Modify rust/crates/tiller/src/main.rs:2932-2940, 5622-5675
- Test rust/crates/tiller/src/main.rs drawn tests beside existing shell tests

Interfaces:
- Duplicate/nested add-project, project-add errors, save failures, and file-picker-open failures call Sidebar::set_notice through the existing Entity<Sidebar>.
- No change to Sidebar, FileView, or any other stderr diagnostic.

- [ ] Step 1: Write the failing duplicate-project drawn test

Add drawn_add_project_duplicate_shows_sidebar_notice that creates a real tracked project in the fixture catalog, invokes the real workspace add-project path with the same directory, pumps the drawn workspace, and asserts the sidebar-notice selector is present. The expected visible message must be the existing literal already tracked or nested: <path>, and the assertion must also verify the workspace catalog did not gain a second project.

- [ ] Step 2: Run the test and verify it fails

Run cargo test -p tiller --bin tiller drawn_add_project_duplicate_shows_sidebar_notice -- --nocapture.
Expected: the catalog remains unchanged but sidebar-notice is absent because add_project currently prints only to stderr.

- [ ] Step 3: Route the three sites

Replace the two add_project eprintln arms with self.sidebar.update(cx, |sidebar, cx| sidebar.set_notice(...)). In handle_open_file, route the real picker error to Sidebar::set_notice; in handle_save_file, route the real FileView::save error to FileView::set_notice. Leave the ACP None branch and all diagnostic-only eprintlns unchanged.

- [ ] Step 4: Run the focused UI and existing file tests

Run cargo test -p tiller --bin tiller drawn_add_project_duplicate_shows_sidebar_notice drawn_tab_context_open_file_uses_the_picker_and_adds_an_editor_tab -- --nocapture.
Expected: the duplicate notice is drawn from the real failed insertion; the existing picker success path still passes. Confirm the save-failure path uses a real unwritable/read-only fixture if a matching test is added.

### Task 4: Verification and handoff

Files:
- No new production files.

- [ ] Step 1: Format and inspect the owned diff

Run cargo fmt --all -- --check and git diff --check -- rust/crates/tiller/src/main.rs rust/crates/tiller/src/session.rs. Do not format or revert unrelated concurrent edits without authorization.

- [ ] Step 2: Run the exact gate stages

Run the exact commands from Scripts/ci-linux.sh, beginning with grep -n clippy Scripts/ci-linux.sh and the owned-crate clippy command cargo clippy --workspace --all-targets --exclude tiller --exclude tiller_ui -- -D warnings, followed by the gate build/test stages. Record failures from files owned by other agents separately.

- [ ] Step 3: Run all focused P73/P71 tests

Run the SessionTab persistence test, both named Codex round-trip/resume tests, the resolver fallback test, the duplicate-project drawn notice test together with the existing ACP mapping test.

- [ ] Step 4: Report honestly

Report the three widened links, the two named non-default Codex drawn round-trip tests, RetainedChat.agent_id, the P71 sites and disconnected-call cause, exact gate invocation and not-yours failures, missing tiller_theme tokens, and any remaining unverified behavior in no more than 12 lines.
