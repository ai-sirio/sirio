# T7-chg-brw build plan — F-CHG, F-BRW, 16 rows

Read-only triage output. No verdicts changed, no code touched. Each section names what a row
actually needs and which files a fix would touch, per `docs/linux-rewrite/triage/T7-chg-brw.md`.
Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

**Architecture note read before the rows below**: in this rewrite "Files" (the tree browser) and
"Activity" live in `rust/crates/tiller_ui/src/right_panel.rs`, a persistent right-hand panel. Git
"Changes" is a **separate, first-class tab** (`ChangesTab`, `rust/crates/tiller_ui/src/changes.rs`,
mounted by `Workspace::add_changes_tab`, `rust/crates/tiller/src/main.rs:4394`) — a deliberate
pivot away from the Swift original's single right-inspector toggle, recorded in
`docs/linux-rewrite/tasks/P21-mount-the-changes-surface.md` and
`docs/linux-rewrite/04-ux-patterns-waku-does-not-cover.md:35` (orca precedent: "a diff is a peer
of a chat, not an inspector"). Several `F-CHG` rows were written against the old single-panel
model and now straddle two different files; that split is called out per-row below.

---

## `F-CHG-01` — FAILED — absent

**Needs: reclassify.** The row's VERIFY clause (`01-inventory-app.md:134`) is "toggle the right
panel, click Files and Changes, and confirm each view replaces the other" — a single-panel
toggle. That toggle genuinely does not exist: `right_panel.rs`'s own toggle only ever shows/hides
Files (confirmed live, shots/215-216 per the manifest evidence). But this is not an oversight —
it is a **documented, still-current architecture decision**: `docs/linux-rewrite/tasks/
P21-mount-the-changes-surface.md` ("What to build" section) states plainly *"Changes is a
first-class tab, not a right-panel section"* and cites `04-ux-patterns-waku-does-not-cover.md:35`
(orca's diff-as-tab precedent) as the reason. `docs/linux-rewrite/tasks/P62-the-changes-panel.md`
independently lists `F-CHG-01` under "Leave these" with the note "(Changes moved to a Diff tab —
architectural, its own piece)". Both documents agree this is intentional, not a gap.

The user-visible capability the row is actually checking for — see the file tree and see git
changes for the same worktree without losing context — is present via two peer surfaces (the
right-panel Files tree, and a `ChangesTab` bound to the same `working_directory` and reachable
from the same tab strip), just not as one panel with an internal toggle. Building a redundant
toggle that duplicates `ChangesTab` inside `right_panel.rs` would be new, maintainable-forever
surface area for a clause the architecture already satisfies a different way.

**Recommend**: the ledger owner re-word or retire this row against the tab-based model rather
than dispatch it as a build. If it is dispatched anyway, treat it as a build (add a Changes mode
to `right_panel.rs`'s toggle, sharing `ChangesTab`'s git-status data rather than re-querying) —
but flag that this contradicts P21/P62's explicit ruling, and confirm the ruling wasn't itself
overturned before doing it.

- **files**: none (reclassify only); if dispatched as build: `rust/crates/tiller_ui/src/right_panel.rs`, `rust/crates/tiller_ui/src/changes.rs`
- **size**: n/a (reclassify) / L if built anyway

---

## `F-CHG-02` — FAILED — absent

**Needs: build.** The row's evidence ("close-workspace → current-workspace = none, then `surface
changes open` still serves the last worktree's data") is accurate, and the mechanism is a real,
already-half-built seam. `RightPanel` (right_panel.rs) already models a no-worktree empty state
correctly: a `worktree_selected: bool` field, a `clear_worktree(cx)` method (`right_panel.rs:193`)
that clears the panel's bound state and flips the flag, and a render branch
(`right_panel.rs:979-1004`, `debug_selector("right-panel-no-worktree")`) that shows "No worktree
selected" / "Select a worktree to inspect its files and changes." when the flag is false. **But
`clear_worktree` has zero callers anywhere in `rust/` outside its own definition** — `main.rs`
never calls it, so the state can never actually go false in production; every `RightPanel` is
constructed via `RightPanel::new`/`with_activity` (always `worktree_selected: true`,
`main.rs:3475`, `:8467`, `:8641`, `:8736`), and switching worktrees just replaces the panel with a
fresh always-selected instance. `Workspace.working_directory: PathBuf` (not `Option`) has no
model-level representation of "no worktree" at all, which is why it silently keeps serving the
last value.

`ChangesTab` (changes.rs) has no equivalent state whatsoever: `ChangesTab::new(repo_root, cx)`
takes a concrete path unconditionally and never re-checks whether that worktree is still current.

**Approach**: (1) find or add the point in `main.rs` where the last worktree closes / none stays
selected (near `close_workspace`, `main.rs:3608`, and `current_workspace()`'s `Option`-returning
logic, `main.rs:507`) and call `self.right_panel.update(cx, |panel, cx| panel.clear_worktree(cx))`
there — this closes the Files/Activity half essentially for free, since the render branch already
exists and is presumably tested. (2) Give `ChangesTab` an equivalent bound-worktree flag and empty
render branch (mirroring `right_panel.rs`'s pattern), and have `main.rs` clear it the same way, or
gate `surface changes open`/`add_changes_tab` on there being a current worktree at all.

- **files**: `rust/crates/tiller/src/main.rs` (`close_workspace`/`current_workspace`, wire the missing call), `rust/crates/tiller_ui/src/right_panel.rs` (verify `clear_worktree` re-entry: nothing currently re-sets `worktree_selected: true` on the *same* instance, so confirm the fix doesn't strand a panel permanently cleared), `rust/crates/tiller_ui/src/changes.rs` (new no-worktree state)
- **size**: M

---

## `F-CHG-03` — FAILED — defective

**Needs: reclassify** (exercise as the fallback). This is about the **Files** tab's Refresh
(`01-inventory-app.md:136`, `FileExplorerView.swift:23`), not `ChangesTab`. `right_panel.rs`
already has a real, drawn loading state and Retry path, not a computed-but-undrawn flag: a
`refresh_started: bool` field gates the render (`right_panel.rs:698-736`) between a
`"files-loading"` div ("Loading files…"), a `"files-error"` div with a `"files-retry"` button on
failure, and the file list otherwise — and there are two **named tests for this exact row**:
`right_panel.rs:1425` ("F-CHG-03: starting must be visible as a drawn loading state, with a
Refresh action visible") and `right_panel.rs:1451` ("F-CHG-03: a failed root refresh renders Retry
and a subsequent click..."). This matches `docs/linux-rewrite/tasks/P62-the-changes-panel.md`'s
brief, which asked exactly for this and describes it as done.

The live-drive evidence only exercised the happy path (click Refresh on a local, already-cached
file listing) and explicitly says the Retry/failure trial "was not attempted." A local directory
walk is fast enough that `refresh_started` can flip back to `false` well inside one paint frame —
the same "stale-frame lag" this very manifest documents elsewhere in this group (F-CHG-11's
evidence: "the on-screen change did not appear in the first capture... a repeat of the session's
known stale-frame lag"). A live drive that never induced the slow/error path and only checked an
inherently-instant success path is not strong evidence the loading state is absent.

**Approach**: re-drive specifically the Retry path (`chmod 000` the repo root, or point Refresh at
an inaccessible path) and capture the `"files-error"`/`"files-retry"` state; for the loading flash,
either capture at a much shorter interval after the click, or synthetically slow the walk (a huge
directory, or a `strace`/ptrace-style artificial delay) to give the "Loading files…" state time to
paint before it resolves.

- **files**: none if the reclassify holds; `rust/crates/tiller_ui/src/right_panel.rs` only if a genuine defect turns up under the Retry re-drive
- **size**: S (exercise)

---

## `F-CHG-05` — FAILED — defective

**Needs: reclassify** (exercise as the fallback). Keyboard handling in `right_panel.rs` is real
and matches the spec, not absent — `docs/linux-rewrite/STALE-FAILED-CENSUS.md:179` independently
flags this exact row "already built" citing `on_file_key` (`right_panel.rs:585`), wired via
`.on_key_down(cx.listener(Self::on_file_key))` at `right_panel.rs:758`. Reading it: `Down`/`Up`
move the selection cursor by one row (correctly clamped at the ends); `Space` toggles
expand/collapse **only on a directory row** (a no-op on a file, matching "expansion... changes
accordingly" for directories); `Enter`/`Return` opens a file or toggles a directory. This is
exactly the row's own spec (`01-inventory-app.md:138`): arrows select, Space/Return act depending
on row type.

The P104 live-drive evidence used `main.rs` — a **file**, not a directory — and only tried `Space`
on it (correctly a no-op per the code) plus `Down`/`Up`; it never tried `Enter`, which is the
gesture the code actually uses to open a file. The anomalies reported (click on `main.rs` not
visibly moving the highlight off `README.md`; `Down` showing "no visible change") are also fully
consistent with the same capture-timing lag flagged in `F-CHG-03`/`F-CHG-11` above — and the one
anomaly that *did* register (`Up` moving the highlight to `main.rs`, the row above `README.md`) is
exactly what `on_file_key`'s `"up"` arm produces from `current = README.md`'s index, which is
positive evidence the handler is live and correct, not defective.

**Approach**: re-drive with the full gesture set the spec calls for — `Down`/`Up` on both a file
and a directory row, `Space` on a directory (confirm expand/collapse), and specifically `Enter` on
a **file** row (not yet tried) to confirm it opens. Capture with a short delay after each key to
rule out paint lag.

- **files**: none if the reclassify holds; `rust/crates/tiller_ui/src/right_panel.rs` (`on_file_key`, `:585-621`) only if Enter-on-a-file turns out not to open it live
- **size**: S (exercise)

---

## `F-CHG-13` — FAILED — defective

**Needs: build.** Confirmed exactly as the manifest evidence states, and this is a known,
previously-misdiagnosed bug (`docs/linux-rewrite/QUEUE.md`'s "self-inflicted" note: an earlier
`head -5` grep on `OpenDiff` wrongly concluded no handler existed; the real defect is narrower and
worse-shaped than "unwired"). Both call sites that handle "Open diff" discard the path they were
given:

```
main.rs:2745   RightPanelActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx),
main.rs:3926   ChangesTabActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx),
```

`add_changes_tab` (`main.rs:4394`) always constructs a fresh, unfiltered `ChangesTab::new(self.
working_directory.clone(), cx)` titled literally `"Changes"` — the full multi-file Staged/
Changed/Untracked list, not a diff scoped to the one file the user clicked. `ChangesTab` has no
single-file constructor or filter mode to call instead (`changes.rs:275`, `ChangesTab::new` takes
only a `repo_root`). This matches the live evidence precisely: clicking "Open diff" adds a second,
identically-labelled `Changes` Activity entry whose content is the same collapsed multi-file list.

**Approach**: give `ChangesTab` (or a new sibling type) a path-scoped mode — either a
`with_focus(repo_root, path, cx)` constructor that opens with only that file's section
pre-expanded and the tab titled with the file's name, or a genuinely separate single-file diff
view. Wire both discarding call sites to pass the real path through instead of dropping it, and
title the resulting tab/activity entry with the filename (not the literal string `"Changes"`) so
it reads as an isolated diff, not a duplicate of the list surface.

- **files**: `rust/crates/tiller/src/main.rs` (`:2745`, `:3926`, `add_changes_tab`/new `add_diff_tab`, `:4394`), `rust/crates/tiller_ui/src/changes.rs` (`ChangesTab::new`, `:275`, needs a scoped-construction path)
- **size**: M

---

## `F-CHG-18` — NOT EXERCISED

**Needs: exercise.** The drag-and-drop machinery is real, wired production code on both ends, not
test fixtures — `docs/linux-rewrite/QUEUE.md`'s cross-check already settled a direct contradiction
between two frozen references here (`changes.rs:993` is a production `on_drag`, not test-only) and
independently confirmed the row holds. Reading it now: `changes.rs`'s changed-file row is a real
drag source carrying a `(PathBuf, String)` payload (repo-relative path + unified diff text,
`changes.rs:94`, wired at `:992-994`), and `tiller_terminal/src/lib.rs`'s `TerminalView::render`
(the production `Render` impl, not `#[cfg(test)]`) is a real drop target for that exact payload
type: `.on_drop::<(PathBuf, String)>(...)` at `lib.rs:1448`, calling `receive_diff_drop` (`:756`),
which emits `TerminalDropEvent::Diff` and draws a `"terminal-diff-drop"` chip (`:1465` region).
`right_panel.rs`'s own `#[cfg(test)]` drag fixture (`:1877-1995`) is a separate, explicitly-labeled
*harness capability proof*, not the production path — its comment says so directly ("what this
crate owns is the proof that the harness can express a payload drag at all").

The manifest's own evidence is consistent with this: a real drag on the live Wayland lane produced
a floating `"Diff"` chip (the source's drag preview) that tracked the gesture, and the block is
explicitly environmental — no synthetic-drag primitive on the Wayland compositor lane, and the one
route that has one (`DISPLAY=:1`, real X11 mouse events) is barred to this slice. That is a driving
capability gap, not a code gap.

**Approach**: prove it the same way the in-process test harness already does (`right_panel.rs
:1940`'s recipe — real `MouseDownEvent`/`MouseMoveEvent`/`MouseUpEvent` past the drag threshold,
dispatched through `simulate_event`) but end-to-end across the two real production types: drag a
`changes.rs` row's payload onto a live `TerminalView` and assert `last_dropped_diff()`
(`lib.rs:739`) is `Some` and the `"terminal-diff-drop"` chip renders — OR, if only a live desktop
drive counts as proof for this row, get access to a lane with real synthetic mouse-drag support
(the `DISPLAY=:1` X11 route this manifest already names, currently out of reach for this slice).

- **files**: none (exercise only)
- **size**: S–M depending on which proof route is available

---

## `F-CHG-20` — FAILED — defective

**Needs: build** (with genuine uncertainty flagged). The empty-state element is real and present —
`right_panel.rs:850-862` draws an `"activity-empty"` div with `"No activity"` at the same
`ACTIVITY_ROW_HEIGHT` (48px) and the same `theme.typography.footnote`/`theme.meta` tokens used
elsewhere in this file (including the no-worktree text discussed in `F-CHG-02`, which the live
evidence never flagged as illegible) — so this is not a missing-element bug, and there is a
passing test (`right_panel.rs:2002`, `activity_section_states_no_activity_when_empty`) asserting
the div's bounds exist. But that test only constructs a panel that starts empty
(`RightPanel::with_activity(dir, Vec::new())`) — **it never exercises a transition from a
populated Activity section down to empty**, which is what the live evidence actually drove
("Running state confirmed working (2 live labelled rows). Nothing-running state: ... illegible
sub-pixel specks"). A container-bounds assertion cannot catch a glyph-level repaint artifact, and
the live evidence explicitly ruled out the check itself being the artifact (edge-crop/contrast/
repaint checks).

The most likely mechanism, unverifiable from static reading alone, is a stale partial-repaint
region left over from the transition: GPUI re-renders `render_activity` each frame from
`self.activity`, but if the row count shrinks (2 real rows → 0) while `activity_expanded` stays
true, whatever invalidation region GPUI computes for the now-shorter `section` div may not fully
clear the pixels the old rows occupied, corrupting the antialiasing under the new "No activity"
text — a class of bug a bounds-only test is structurally blind to.

**Approach**: run two real activity items to completion so the section shrinks from 2 rows to 0
while expanded (not a fresh empty construction), capture immediately and again after a forced full
repaint (resize the window, or collapse/re-expand the section) to see whether the specks are a
one-time stale-paint artifact that a subsequent full invalidation clears — that would confirm the
repaint-region theory and point the fix at forcing a full redraw of the activity `section` div
when `self.activity` transitions to/from empty (e.g. an explicit key change on the container, or
`cx.notify()` plus checking whether the element structure diffing needs a fresh `ElementId`).

- **files**: `rust/crates/tiller_ui/src/right_panel.rs` (`render_activity`, `:780-862`)
- **size**: M (rendering/repaint-class bug; genuinely uncertain without a live repro)

---

## `F-CHG-22` — half-proven

**Needs: exercise.** The data-tier half the manifest cites is real and correctly wired: `main.rs
:1991` maps `AgentStatus::NeedsInput => ActivityStatus::NeedsInput` (the mapping `docs/
linux-rewrite/tasks/P62-the-changes-panel.md` flagged as broken — "NeedsInput absent from the
activity panel" — is fixed; `ActivityStatus` now has the variant and `right_panel.rs` renders a
distinct glyph/color for all five statuses: `activity_status`/`activity_status_glyph`,
`right_panel.rs:1142-1156`, covering `Idle`/`Running`/`NeedsInput`/`Done`/`Error` each with their
own `theme.tab_*` color and a distinct glyph character). Nothing here needs building.

What's owed is purely visual corroboration of the **already-expanded** Activity section, per the
manifest evidence: the three most recent captures all show the unrelated Changes tab instead. The
fix is a drive, not code.

**Approach**: open a worktree, get one agent to Running, one to Done (or Error), and one to
NeedsInput; open the right panel and click the `"activity-header"` row to expand the Activity
section (not the Changes tab — a different surface per the architecture note at the top of this
document); capture the expanded section showing all three status glyphs distinctly colored/shaped
in the same frame.

- **files**: none (exercise only)
- **size**: S

---
