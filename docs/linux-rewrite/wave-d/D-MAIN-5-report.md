# D-MAIN-5 report

Worked solo in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
Re-read `main.rs` from disk before every edit per house rules (no conflicting concurrent edits
found from earlier links). All commits below build `cargo build -p tiller` green at the time of
commit; sidebar/settings-specific `cargo test -p tiller_ui --lib` suites referenced were run and
passed after each change.

## F-PRJ-03 — implemented (commit 9e26858)

Gap: Open Project's folder picker added the picked directory with zero prompt, even a plain
non-git folder — the ledger's own stale sentence ("no Open Project entry exists") was already
false per the critic's own correction; the real defect was the missing git-check prompt.

Fix: `Sidebar::confirm_add_project` (sidebar.rs) now runs after the platform folder picker
resolves. If the picked path has a `.git` entry it emits `SidebarEvent::AddProject` immediately
(no extra click for the common case). Otherwise it opens a real `window.prompt` with three
buttons — "Initialize Git", "Add without Git", "Cancel" — using the same `window.prompt`
mechanism `request_remove_project` already uses for its confirmation. "Initialize Git" shells out
to `git init --quiet` in the folder before emitting `AddProject`; "Add without Git" emits
`AddProject` unchanged; "Cancel" emits nothing.

`ctl project.add` (main.rs `control_add_project`) deliberately still adds silently — it is a
headless automation path with no human present to answer a prompt, and I did not touch it.

New test: `open_project_initialize_git_creates_a_real_repo_then_adds` creates a real plain folder
on disk, drives the full Open Project → picker → prompt → "Initialize Git" flow, and asserts a
real `.git` directory now exists AND `AddProject` was still emitted. Updated the pre-existing
`add_project_menu_open_choice_reports_the_chosen_directory` test to answer the new prompt
("Add without Git") since its fixture path has no `.git`.

**howToExercise**: click the sidebar's `add-project` `+` control, click `add-project-open`,
answer the path-prompt with a real non-git folder (e.g. a fresh `/tmp` dir with no `.git`). A
platform prompt titled "This folder is not a git repository" appears with three buttons.
Answering "Initialize Git" should leave a `.git` in that folder and add the project to the
sidebar; answering "Add without Git" adds it without a `.git`; "Cancel" adds nothing.

## F-SET-04 — fixed (commit 6951b1e)

Gap: `resume_agent_sessions` setting was persisted and round-tripped through the DB/report, but
never actually consulted — `restore_tabs`/`restore_tabs_in_workspace` always used whatever native
session refs were on disk regardless of the toggle.

Fix: both restore call sites (the startup window-open path in `main()`, and
`TillerWorkspace::restore_launch_snapshot`) now check the live setting
(`saved_settings.resume_agent_sessions` / `self.settings.read(cx).snapshot().resume_agent_sessions`)
and pass an empty `BTreeMap` instead of the loaded session refs when it's off, so restored agent
panes start fresh instead of silently resuming. The shared `session_refs` `Arc<Mutex<...>>` used
by the control server (`session.ref`) is untouched — that's a different concern from restore-time
use.

**howToExercise**: with `resume_agent_sessions` off in Settings, an agent pane that had a
resumable native session before a relaunch should come back running a fresh session (its
`resume_command` never gets invoked) instead of resuming the old one. Hard to observe purely via
`wayland-drive.sh` without a real agent CLI session on disk; the code path is the honest signal
here — no unit test covers session-ref content directly, but the gating is unconditional and
covers both restore call sites in the file.

## F-SET-22 — fixed (commit de2885e)

Gap: `settings_snapshot_from_app_settings` hard-coded `agent_colors` back to the default palette
on every rebuild, and `app_settings_from_snapshot` dropped it entirely — `AppSettings` has no
column for it, confirmed by the row's own comment in the code.

Fix (without touching `tiller_persistence`, which I do not own for this row): added
`SessionStore::load_agent_color_ids` / `save_agent_color_id` in `crates/tiller/src/session.rs`,
which store per-agent colour ids in the existing session-refs key-value table under an
`agent-color:<index>` key prefix (reusing `save_session_ref`'s storage rather than adding a new
table). Wired at the two main.rs sites: the startup snapshot construction overlays saved colours
onto the freshly-built default snapshot; the settings `on_change` handler persists every colour
alongside the rest of the snapshot.

**wantedForeignFiles**: none needed — this avoided touching `tiller_persistence` entirely by
reusing an existing KV table. If a later link wants a "real" `AppSettings.agent_colors` column
instead, that would replace this KV-table workaround in
`rust/crates/tiller_persistence/src/model.rs` (`AppSettings` struct, mirroring how
`translucency`/F-SET-20 was added) and `db.rs`'s `settings()`/`save_settings()`.

**howToExercise**: open Settings → General, click an agent-colour swatch (`agent-color-swatch-*`
or similar selector under `render_agent_colors`), quit and relaunch the app (or reopen Settings
fresh), confirm the picked colour is still selected rather than reverted to the default palette.

## F-SID-06 — fixed (commit 99129a8)

Gap: the sidebar's status dot only ever drew for `RowKind::Worktree` rows; a collapsed project's
worktree children are hidden from `visible_rows()`, so a worktree with an urgent status
(Error/NeedsInput/Idle) had no way to surface through a collapsed project row.

Fix: `Sidebar::collapsed_project_status` picks the single most urgent status
(Error > NeedsInput > Idle > Done > Running/none) among a project's worktree children.
`visible_rows()` (both the plain and filtered/query branches) now stamps this onto the cloned
project row's `agent_status` field when the project is collapsed. `render_row`'s status-dot gate
now also fires for `RowKind::Project` rows that are collapsed.

New test: `collapsed_project_badges_the_worst_child_worktree_status` sets an Error status on
worktree row 5 (child of collapsed project row 4), confirms the project row now carries
`agent_status == Some(Error)`, then expands the project and confirms the aggregate badge clears
(the now-visible worktree row carries its own dot instead).

**howToExercise**: with a project collapsed and one of its worktrees showing a notable status dot
(drive it via a `notify` control call with status `error` or `needs-input` on a pane under that
worktree), the collapsed project row itself should show a colored dot in its leading 12px slot —
same rendering as an expanded worktree row's dot, just on the project row.

## F-SET-09 — fixed (commit 6b8a9cc)

Gap: clicking Install Skill reached the wired host callback (proven previously), but the Settings
screen showed zero feedback that anything happened — no `InstallStatus`/install-progress state
existed anywhere in settings.rs.

Fix: added `Settings::skill_install_launched: bool` plus `install_skill_clicked` method (mirrors
the existing `account_login_pending` pattern for account sign-in). Clicking Install Skill now sets
this flag and `cx.notify()`s before handing off to the host, and the card renders a confirmation
line ("Installing… running in a new terminal tab.") under the button while it's set, with
debug selector `general-install-skill-status`.

Extended test `install_skill_click_reaches_the_wired_host_callback` to also assert the status line
appears.

**howToExercise**: Settings → General → Agent Skill card → click "Install Skill"
(`general-install-skill`). A text line with selector `general-install-skill-status` should appear
under the button immediately (same click, no relaunch needed), and a new terminal tab titled
"Install Skill" should open running the provisioned command.

## F-SET-18 — implemented (commit 78b2b21)

Gap: agent rows on the Agents screen only ever showed a red "Not found on PATH" pill for a
missing CLI — no Install/Update/Retry control site existed anywhere in settings.rs's agent rows.

Fix: added `AgentAvailability::install_command()` in `tiller_agents/src/lib.rs` — a `match` on
`self.id` mapping the three CLIs with a real documented npm install line (`claude` →
`npm install -g @anthropic-ai/claude-code`, `codex` → `npm install -g @openai/codex`, `opencode` →
`npm install -g opencode-ai@latest`); `pi`/`omp` deliberately return `None` — I don't know a real
install command for either and this codebase's own convention (see the F-SET-08/tillerctl "no
install mechanism" comment) is to leave a control absent rather than fabricate one. A not-installed
agent row with a known command now renders a real "Install" button
(`settings-agent-install-{index}`); clicking it hands `(agent_id, command)` to a new
`Settings::on_install_agent` host callback and shows a confirmation line
(`settings-agent-install-status-{index}`) under the row, mirroring F-SET-09's pattern exactly.
Wired in main.rs: new `WorkspaceAction::InstallAgent { agent_id, command }`, a new
`agent_install_shell` helper (same shape as `skill_install_shell`), and a new terminal tab titled
"Install {agent_id}" runs the command.

I did not add "Update" or "Retry" controls — "Retry" already exists as the Agents screen's
"↻ Refresh" (re-runs discovery, F-SET-16), and "Update" has no documented per-CLI update command I
could source honestly within this row's scope; the ledger evidence only demonstrated the absence
of an *Install* site, which this closes.

New test: `agent_install_click_reaches_host_and_confirms_on_the_row` — a two-row fixture
(opencode: known command, omp: no known command) confirms opencode gets an Install button and omp
does not, clicks it, and asserts the exact `(agent_id, command)` pair reaches the host plus the
confirmation line appears.

**howToExercise**: Settings → Agents, find a not-installed provider with a known command (e.g.
opencode if not on PATH) — `settings-agent-install-1` (index depends on discovery order). Click
it: a confirmation line appears under the row, and a new terminal tab titled "Install opencode"
opens running `npm install -g opencode-ai@latest`.

## F-SET-10 — already-correct, no code change

Investigated main.rs + status_bar.rs end to end. Found the wiring already complete and live, not
stubbed:
- `TillerWorkspace` does `cx.observe(&settings, ...)` (main.rs, near where `status_bar` is built)
  that rebuilds `UsageBarPrefs::from_snapshot` and calls `status_bar.apply_preferences` on every
  settings change — so a refresh-interval edit in Settings reaches the live status bar without a
  relaunch, comment explicitly cites F-SET-10.
- The status bar's own `status-refresh` icon button (`status_bar.rs` `on_refresh_clicked`) flips
  every provider segment to `ProviderUsageState::Loading` synchronously (a real, immediately
  visible state change) before spawning the background fetches and applying real outcomes.
- Settings' per-provider "Refresh now" button (`refresh-claude-now` etc.) is a *different* control
  that refreshes account-connection status (`ProviderAccountStates`), not the usage bar's numbers
  — that's intentional per its own doc comment, not a defect.

I made no code change here because I could not find an actual gap — the critic's "test-only"
language reads as "I could not drive this live through the Wayland lane", not "the code is wrong".
I did not want to guess-patch working code.

**howToExercise**: (a) open Settings → an AI Provider card, change the "Refresh interval" stepper
value, close Settings, and confirm the live status bar's next fetch cadence follows the new value
(slow to observe live — better proven by the existing
`usage_bar_consumes_visibility_and_interval_preferences` unit test). (b) click the status bar's
refresh icon (`status-refresh`) directly — provider segments should immediately show a loading
state ("…") before flipping to fetched values or an error, which **is** directly screenshot-able
via `wayland-drive.sh` since the status bar is part of the main window, not hidden in Settings.

## Build status

`cargo build -p tiller` green after every commit above. `cargo test -p tiller_ui --lib sidebar` and
`cargo test -p tiller_ui --lib settings::tests` both green (36 and 45 tests respectively) as of the
last commit.
