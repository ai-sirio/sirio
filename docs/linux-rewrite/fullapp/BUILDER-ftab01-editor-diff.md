# Builder live drive — F-TAB-01, TabKind::Editor and TabKind::Diff opened live

Closes the row's remaining gap: `TabKind::Editor`/`TabKind::Diff`
(`rust/crates/tiller_project/src/tab.rs:10-21`) had never been opened live —
only Terminal/AgentChat/Browser tab decorations had direct screenshots. The
gap was blamed on the missing xdg-desktop-portal (see F-TAB-09), but that
portal is not actually a precondition for this row: most routes to both
`TabKind`s carry an already-known path and never touch a file dialog. This
drive opens both kinds through those no-portal routes and, separately, as a
second deliverable, attempts the Ctrl-O/native-picker path against F-TAB-09.

Worktree `/var/tmp/tt-ftab01-610923`, branch `verify/f-tab-01-610923`, off
`origin/linux/gpui-waku` at `f45fa9b0`. No code changes — verification only.
Own `CARGO_TARGET_DIR` (`/var/tmp/cargo-target-ftab01-610923`), own binary
snapshot (`/var/tmp/tt-ftab01-bin-610923`), own `TILLER_WL_LABEL`
(`ftab01-610923`), own fixture: a fresh standalone git repo at
`/var/tmp/tt-ftab01-fixture-610923` with one committed file (`notes.md`,
`README.md`, `src/main.rs`) and one uncommitted edit to `notes.md`, so the
Changes view has real content without touching any other agent's project.

## Outcome: both TabKinds opened live, through two routes each

## TabKind::Editor — file-tree double-click, no portal

`add_file_tab` (`main.rs:6964`) is reached from four routes; none of them
need the native picker (`RightPanelEvent::OpenFile`, `FileViewEvent::OpenFile`,
`ChatEvent::OpenFile`, `ChangesTabEvent::OpenFile` — only `Ctrl-O`/the
tab-context "Open File" menu item goes through `cx.prompt_for_paths`
instead). Used the simplest: the Files panel's file-tree row
(`tiller_ui/src/right_panel.rs:630-641`).

One thing worth recording precisely, since it cost a wasted round trip: file
rows gate on `event.click_count >= 2` — **a single click only selects/focuses
the row** (`right_panel.rs:636-639`, "Files follow the inventory's
double-click contract"). A first attempt with one `click` action produced
exactly the half-state that looks like nothing happened (row highlighted,
no tab, `panel.list` empty) until the source made the double-click
requirement explicit. Two `click` DSL calls at the same coordinate, back to
back, registered as one double-click.

`01-editor-tab-live.png`: `notes.md` open as a real Editor tab — tab bar
shows `notes.md` with a close control, the file path
(`/var/tmp/tt-ftab01-fixture-610923/notes.md`), a `Markdown` language badge,
and a `Preview`/`Code` mode toggle (currently in Preview, rendering the
heading and both paragraphs — including the uncommitted second paragraph,
confirming this is the live working-tree content, not a stale/committed
copy). `panel.list` corroborates a real registered pane:
`{"active":"true","id":"pane-0","tab":"notes.md","title":"notes.md"}`.

## TabKind::Diff — two independent routes, neither needs the portal

**Generic route** — `+` (new-tab button) → `Changes` menu item
(`NewTabAction::NewChanges` → `add_changes_tab(None, cx)`, `main.rs:7531`).
`02-diff-tab-generic-live.png`: a `Changes` tab with `Local changes (1)`,
a `Changed (1)` section listing `notes.md` with its `-0 +2` stat, `Unified`/
`Split` toggle, `Expand All`/`Collapse All`/`Stage all`/`Discard all`
controls — unambiguously the Diff surface, not a generic file list.

**Path-specific route** — inside a Changes tab, clicking a changed-file row
expands it (`toggle_change`) to reveal `Discard`/`Stage`/`Open diff`; clicking
**Open diff** emits `ChangesTabActionEvent::OpenDiff(path)`
(`changes.rs:1273-1284`), handled at `main.rs:6453-6454` as
`workspace.add_changes_tab(Some(path), cx)` — a **second**, distinct
`TabKind::Diff` tab, pre-focused on that file
(`ChangesTab::focus_path`, `changes.rs:1036`). `03-diff-row-expanded.png`
shows the row expanded with the real unified diff hunk (`@@ -1,3 +1,5 @@`,
the two added lines highlighted) and the action row.
`04-diff-tab-pathspecific-live.png` shows the result: a new, active `Changes`
tab (fourth in the sidebar's tab list, since each `wayland-drive.sh`
invocation in this drive re-opens the app against the same session and
prior tabs restore) already expanded to the same hunk, with no extra click
needed — confirming `focus_path` actually ran, not just that a generic tab
opened again.

`panel.list` after this step: four `Changes` panes registered, the newest
(`pane-3`) active — corroborating the tab count and focus state independent
of the screenshots.

## Cleanup

Torn down: `dbus-daemon`, `sway`, virtual-pointer client, and app binary all
killed by PID under label `ftab01-610923`, confirmed via `pgrep`.

## Second deliverable: the Ctrl-O/native-picker leg (F-TAB-09)

See the section below (added after this one, once attempted) for whether the
two-layer portal recipe reproduces wave P's PASSED result on this host. This
is deliberately kept as a separate verdict from the row above: if it fails,
that reads as "could not reproduce the recipe", not as a regression of
F-TAB-09's own PASSED status, and it does not reopen F-TAB-01, which is
closable on the no-portal routes alone.
