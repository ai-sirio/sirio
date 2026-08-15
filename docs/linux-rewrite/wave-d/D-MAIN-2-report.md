# D-MAIN-2 report

Slice: link 2 of 8 in the `D-MAIN` chain. Files owned: `rust/crates/tiller/src/main.rs`.
Re-read `main.rs` from disk before every edit, as instructed — line numbers in the
brief had already drifted from the D-MAIN-1 commits.

## F-CORE-FILE-04 — fixed

`FileViewEvent::OpenFile` (emitted at `tiller_ui/src/file_view.rs:379` on a resolved
markdown/document link click) had no subscriber outside its own module test. Added
`TillerWorkspace::subscribe_file_view` in `main.rs` (mirrors `subscribe_right_panel`
and `bind_chat`) and call it from `add_file_tab`'s `FileView::new` creation site, so
the event now routes through the same de-duplicating `add_file_tab` path the Files
panel and chat transcript already use. The two other `FileView::new` call sites in
`main.rs` are test fixtures that build `OpenTab` directly, not through `add_file_tab`,
so they were left alone. `cargo build -p tiller` green.

**howToExercise:** open a markdown file containing a relative link to another file in
the worktree (e.g. `[see](other.md)`) in the file view, click the link. The clicked
path should open as a new file tab (or focus an already-open one) rather than doing
nothing.

Commit: `fix(F-CORE-FILE-04): wire FileViewEvent::OpenFile to add_file_tab`

## F-CORE-ACT-10 — already-correct, exercise-only gap

Re-checked live: `panes::refresh_process_signal` (Layer D) IS wired into `main.rs` —
`start_process_signal_refresh` (main.rs, spawns a loop on `panes::PROCESS_SIGNAL_INTERVAL`)
calls it every 500ms per pane and pushes transitions through
`workspace.post_activity_notification` / `sync_activity`. This is the same code path
F-CORE-ACT-06 already live-proved (sweep C-P2). The prior critic's NOT EXERCISED verdict
was a scope confound only ("panes.rs is outside this slice's owned files"), not a real
gap — panes.rs did not need a commit because there was nothing to fix in it; the wiring
lives in main.rs and was already correct before this slice started. No code change made.

**howToExercise:** spawn a real Claude Code pane, then in that pane's shell start a
plain child process with a name Tiller doesn't recognize as an agent (e.g. `sleep 300 &`)
— nothing should change. Then run an actual catalog agent binary as a foreground child
and watch the pane glyph flip to Running from process detection alone (same drive as
F-CORE-ACT-06's frames 12/23/25).

## F-CHG-02 — blocked (needs `tiller_ui/src/changes.rs`)

`ChangesTab::render_body` (changes.rs:1269) has three states: error (`git_error`),
loading (`git_task.is_some() && entries.is_empty()`), and the section list. When the
repo is clean (or, per the row's clause, the worktree was just closed and reopened with
nothing to show) and `sections` is empty, there is a fourth, unhandled case: it falls
through to `div().id("changes-list")...children(empty iterator)` — a blank panel with no
message. This is main.rs-adjacent but the fix is entirely inside changes.rs's own render
path; nothing in main.rs decides what ChangesTab shows internally.

**Precise foreign-file change needed** (`tiller_ui/src/changes.rs`, `render_body`):
after computing `sections`, if `sections.is_empty()` (and neither error nor loading),
return an empty-state element analogous to the existing `#changes-loading` div — same
`flex_1().items_center().justify_center()` centering, `id("changes-empty")` for a debug
selector, text such as "No changes" — instead of falling through to the empty list.

**howToExercise (once landed):** ctl `surface.changes.open` on a worktree with a clean
git tree (or a just-closed worktree per the row's original clause); the Changes tab
should show a centered "No changes" message, not a blank body.

## F-CHG-13 — blocked (needs `tiller_ui/src/changes.rs`)

Confirmed live in the current `main.rs` (re-read from disk, not the brief's stale line
numbers): both `RightPanelActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx)`
and `ChangesTabActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx)` discard the
path and open the generic multi-file Changes tab. `right_panel.rs:440` and
`changes.rs:1074` both emit `OpenDiff(path)` expecting the receiver to do something
path-specific with it, but `ChangesTab` has no API to focus, scroll to, or isolate a
single file's diff — `grep -n "focus_path\|selected_path\|single.*diff" changes.rs`
comes back empty. Building a real single-file diff view is a `changes.rs` feature, not
a main.rs wiring fix; I did not invent an API on a file I don't own.

**Precise foreign-file change needed** (`tiller_ui/src/changes.rs`): add a
`pub fn focus_path(&mut self, path: &Path, cx: &mut Context<Self>)` (or equivalent) that
expands the section containing `path` and scrolls/highlights that row — reusing the
existing `expanded_changes`/`expanded_bands` state already used for manual expand/collapse.

**Corresponding main.rs change I did not make** (blocked on the above landing first):
change `fn add_changes_tab(&mut self, cx: &mut Context<Self>)` to
`fn add_changes_tab(&mut self, focus_path: Option<PathBuf>, cx: &mut Context<Self>)`,
call `changes.update(cx, |tab, cx| tab.focus_path(&path, cx))` after construction/reuse
when `focus_path` is `Some`, and change both `OpenDiff(_path)` match arms
(main.rs ~2886, ~4094 as of this commit) to `OpenDiff(path) => workspace.add_changes_tab(Some(path.clone()), cx)`.

**howToExercise (once landed):** in the right panel's changed-files activity list,
click a specific file's diff-open affordance; the Changes tab should open with that
file's section expanded/scrolled into view, not just the panel in its default state.

## F-CORE-ACT-25 / F-CORE-ACT-26 / F-CORE-DOM-07 — blocked, architectural gap

All three wrap pure, already-tested policy modules
(`tiller_activity::bootstrap::BootstrapRestoreOrder`, `tiller_activity::mount::WorktreeMountPolicy`,
`tiller_project::domain::AutoNamingThrottle`) that have zero callers anywhere in the app.
Re-confirmed live by reading `main.rs` end to end for the concepts each policy needs:

- `BootstrapRestoreOrder`/`WorktreeMountPolicy` both require a set of **concurrently
  mounted worktrees** (`open_worktree_ids`, a mount cap, per-worktree PTY hosts kept
  alive off-screen) — the Swift original's `AppModel.openWorktreeIds`. `main.rs`'s
  `TillerWorkspace` has no such concept: `select_worktree` (main.rs ~3584) replaces
  `self.tabs` wholesale and calls `self.panes.set_external(&old_path, Vec::new())`,
  i.e. the previous worktree's panes are torn down synchronously on switch, not kept
  mounted in the background. There is exactly one active worktree's tabs/PTYs alive at
  a time. `limit_mounted_worktrees`/`mounted_worktrees` exist only as persisted Settings
  values (main.rs ~8445) with no consumer.
- `AutoNamingThrottle` requires an automatic-tab-naming feature (something that
  periodically proposes a generated name from transcript growth) to gate. No such
  feature exists anywhere in `main.rs` — there is no "generate title from transcript"
  call site to throttle.

Wiring any of these three honestly requires building the underlying feature first (a
multi-worktree background-mount subsystem for ACT-25/ACT-26; an automatic-naming
request path for DOM-07), which is materially larger than a row-sized fix and touches
files I don't own (`tiller_ui/src/sidebar.rs` for mount state, `tiller_project/src/settings.rs`,
possibly a new module). I did not fabricate a caller just to make `grep` find one.
Reporting `blocked` rather than a false `fixed`.

**howToExercise:** none — not reachable in the current build. A critic re-grepping
`bootstrap.rs`/`mount.rs`/`AutoNamingThrottle` callers will still find zero outside
their own modules/tests.
