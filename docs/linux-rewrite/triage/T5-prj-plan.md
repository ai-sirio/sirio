# T5-prj build plan — F-PRJ, 13 rows

Read-only triage output. No verdicts changed, no code touched. Each section names what a row
actually needs and which files a fix would touch, per `docs/linux-rewrite/triage/T5-prj.md`.
Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

All 18 `F-PRJ` VERIFY clauses (with their original `App/AddProjectSheet.swift` /
`App/ProjectSettingsSheet.swift` line references) are in `docs/linux-rewrite/01-inventory-app.md`
lines 40-57; that is where the exact clause wording below comes from.

---

## `F-PRJ-01` — FAILED — defective

**Needs: build.** Confirmed live: `render_add_project_menu` (`sidebar.rs:1741`) does draw an
opaque panel — `.bg(theme.card_fill)` at `sidebar.rs:1756`, and `card_fill` resolves to
`rgb_hex(0x232323)`/`0xECECEC` (`tiller_theme/src/lib.rs:269`) whose `a` is hard-coded `1.0`
(`rgb_hex`, `tiller_theme/src/lib.rs:974-981`). The background is opaque; the defect is paint
**order**, not alpha. In `Sidebar::render` (`sidebar.rs:2372`), the menu is mounted as a child
*before* the filter field and the row list:

```
.when(add_project_menu, |this| { this.child(Self::render_add_project_menu(...)) })  // :2436
.child(div().id("filter-field") ...)                                                // :2440
... rows further below ...
```

Every other absolutely-positioned overlay in this same file — `context_menu`,
`project_settings`, `project_form` — is deliberately appended as the **last** children of the
root div (`sidebar.rs:2631-2638`), specifically so later paint order puts them on top. The
add-project menu breaks that established pattern by mounting early, so the filter field and the
row list (painted after it) composite over it — exactly matching the evidence ("the sidebar
Filter field and the `tiller` project path composite straight through it").

**Approach**: move the `.when(add_project_menu, ...)` block down to sit alongside (just before
or after) the other three `.when_some(...)` overlay blocks at the end of the child chain, matching
the pattern the other three already use. This is a same-file, few-line reorder — no new state,
no new component.

- **files**: `rust/crates/tiller_ui/src/sidebar.rs` (`Sidebar::render`, ~`sidebar.rs:2372-2640`)
- **size**: S

---

## `F-PRJ-03` — FAILED — absent

**Needs: build.** The row's VERIFY clause (`01-inventory-app.md:42`) is *not* about the `+`
menu having an "Open Project" entry — it is: "Handle a browsed non-Git folder with initialize,
add without Git, and cancel choices." The ledger's evidence text ("no Open Project/browse entry
exists") is now stale on its own terms — `sidebar.rs:1760`'s `"Open Project…"` menu item, wired
to `start_open_project` (`sidebar.rs:974`, using `cx.prompt_for_paths`), is real and is the same
control F-PRJ-01 independently confirms exists (superseding whatever build F-PRJ-03's sweep ran
against). But the verdict itself still holds for the *actual* clause: whatever path is picked —
via the folder picker or via Clone/Create — always lands in `Workspace::add_project`
(`tiller/src/main.rs:2956`), which calls `ProjectCatalog::add` (`session.rs:487`). `add()` calls
`discover_project` and silently accepts the result whether or not `.git` was found — there is no
branch anywhere that offers Initialize Git / Add without Git / Cancel for a picked non-Git
folder; it is simply added as a non-git project every time (the same "Repository: Folder" state
F-PRJ-11's evidence separately enumerates).

**Approach**: this needs a new three-way prompt UI (mirroring `request_remove_project`'s use of
`window.prompt`, `sidebar.rs:1061`, or a small custom sheet if git-init-vs-not needs its own
copy) inserted between "a path was picked" and "the project is added," plus a session-layer
branch: on `Initialize Git`, call the same `init_repository` used by `SidebarContextAction::
InitializeGit` (`main.rs:3177`) before adding; on `Add without Git`, add as-is (today's only
path); on `Cancel`, discard the pick. The branch point is `add_project` in `main.rs`, so this
fix touches the file 29 other rows already touch — flag deliberately, not a surprise.

- **files**: `rust/crates/tiller/src/main.rs` (`add_project`, `:2956`), `rust/crates/tiller_ui/src/sidebar.rs` (new prompt state/render, alongside `WorktreePrompt`/`request_remove_project` for the pattern)
- **size**: M

---

## `F-PRJ-06` / `F-PRJ-09` — half-proven (both)

**Needs: exercise, both — shared cause.** The in-flight submission guard both rows are missing
proof for is a correct, already unit-tested, pure state machine in `project_forms.rs`:
`CloneFormState::begin()`/`can_submit()` (`:70-85`) and `CreateFormState::begin()`/`can_submit()`
(`:444-458`) both flip status to `Running` synchronously on the first accepted call and refuse a
second one while `Running`; `clone_state_disables_empty_url_and_double_submission` (`:762`) and
`create_state_disables_empty_name_and_double_submission` (`:801`) already assert this at the
state level. The button `on_click` handlers (`:373`, and the mirrored Create button) call
`form.submit(cx)` directly with no debounce needed — GPUI dispatches one click at a time, so the
guard is inherently race-free *if* `Running` persists long enough for a second click to land
inside it.

That's exactly what the live environment can't currently produce for either form:
`CloneForm::submit` (`:164-234`) shells out to a real `git clone` with **no network** in this
environment, so it fails near-instantly; `CreateForm::submit` (`:521-545`) does a local
`mkdir`+`git init` on `cx.background_spawn`, which also resolves in well under a frame on local
disk. In both cases `Running` never survives long enough for a live second click to land inside
it — F-PRJ-09's own evidence shows this precisely: the second click "landed on empty sidebar
space after the dialog had already closed," because `CreateFormEvent::Created` had already fired
and unmounted the form. This is a proof-environment limitation, not a code defect.

**Approach (both rows)**: prove the guard live by making the in-flight window survive at least
one paint frame — e.g. shim `git` on `$PATH` with a wrapper that `sleep`s before delegating (for
Clone), or point Create's `parent` at a path where `mkdir`/`git init` is artificially slowed
(a FUSE/throttled mount, or a wrapper git shim again, since `create_project` also shells to
`git init`). With `Running` held open, fire two real, separately-dispatched clicks on
`clone-submit`/the Create equivalent and confirm only one worker starts (one project row, not
two) and the second click is a no-op against the still-`Running` guard.

- **files**: none (exercise only; `rust/crates/tiller_ui/src/project_forms.rs` is where the guard lives, already correct)
- **size**: S each

---

## `F-PRJ-07` — half-proven

**Needs: exercise.** The failure half is already proven live. The retry half is implemented and
unit-tested, not missing: `CloneFormState::set_url` (`project_forms.rs:39-44`) resets `status`
back to `Ready` on any edit while not `Running`, and `can_submit()`/`begin()` deliberately allow
resubmission from `Failed` ("A failed attempt is intentionally submit-able so the same form is
the retry surface," `:68-69`) — covered by `clone_failure_keeps_url_and_allows_retry`
(`:775`). `clone_repository` (`tiller_git/src/clone.rs:57`) is a plain `git clone <url> <dest>`
shell-out, which accepts a local filesystem path as `url` just as readily as a remote one, so the
retry gesture does not need outbound network to be genuine.

**Approach**: after the already-proven invalid-URL failure (red text, "Retry clone" label),
edit the URL field to a real local git repository path (`file:///…` or a bare path) and click
the relabeled button; confirm the status transitions `Failed → Running → Complete`,
`CloneFormEvent::Cloned` fires, and the project appears in the sidebar.

- **files**: none (exercise only)
- **size**: S

---

## `F-PRJ-16` — half-proven

**Needs: build.** The entry/validation half is proven live. The "Open Emoji Picker" half is
genuinely unbuilt, not just unwired — traced end to end: `render_emoji_mode`
(`project_identity.rs:612-685`) draws `controls::button_maybe("project-icon-emoji-open-picker",
"Open Emoji Picker", theme, open_picker_handler)` where `open_picker_handler` comes from
`self.on_open_emoji_picker.clone().map(...)` (`:641-646`) — a host callback hook, exactly like
`on_change`/`on_change_with_context`, with its own builder method `.on_open_emoji_picker(...)`
(`:310-312`). `button_maybe` renders the control inert when the handler is `None`, which is
exactly "clicked with field empty, no observable effect." The one call site that ever supplies
this callback is a unit test (`:1306`, inside `#[cfg(test)]`) — `sidebar.rs`'s real mount site,
`open_project_settings` (`:912-920`), builds `ProjectIconPicker` with `.on_change_with_context(
...)` only and never chains `.on_open_emoji_picker(...)`. And there is nothing to wire it *to*:
a repo-wide search for an emoji-picker overlay/grid component (`EmojiPicker`, `emoji_overlay`,
etc.) turns up nothing outside that one test — no such surface exists anywhere in `rust/`.

**Approach**: this is two pieces of real work, not one wiring gap — (1) build an actual emoji
picker surface (a searchable grid overlay is the bounded-scope option; there is no Linux
equivalent of macOS's system character palette to shell out to) that on selection calls back into
`commit_emoji`-equivalent logic; (2) wire `.on_open_emoji_picker(...)` at the `sidebar.rs` mount
site to open it. Piece (2) alone is trivial; piece (1) is the actual size of this row.

- **files**: `rust/crates/tiller_ui/src/project_identity.rs` (new overlay component + `render_emoji_mode`, `:612-685`), `rust/crates/tiller_ui/src/sidebar.rs` (`open_project_settings`, `:912-920`, to chain `.on_open_emoji_picker(...)`)
- **size**: M

---

## `F-PRJ-17` / `F-PRJ-18` — FAILED — absent (both) — shared cause, traced through three crates

**Needs: build, both — one shared cause.** Confirmed live and by reading: the New Worktree
popover (`WorktreePrompt`, `sidebar.rs:282-295`, rendered `:2534-2630`) has exactly one field,
`draft` (branch name) — no base-branch or location control, matching both rows' evidence. But
this is not a from-scratch feature: it is a fully-built, three-layer seam whose *consuming* half
was simply never wired, and none of it is registered in `SEAMS.md` today.

**What already exists, bottom-up:**
- `tiller_git::create_worktree(repo, branch, path, base: Option<&str>)`
  (`tiller_git/src/worktree.rs:63-67`) and `resolve_parent_directory(root, override_dir:
  Option<&Path>)` (`:152-158`) already accept a base branch and a location override — both real
  parameters, not stubs.
- `tiller_persistence` already has durable columns for both: `default_worktree_base` and
  `worktree_location_override` (`migrations.rs:41-42`), read and written in `db.rs` (`:131,143-
  156,167-168,1194-1195,1313-1314,1326-1327`) as part of `ProjectRecord`.
- `tiller_project::Project` already carries both fields (`tiller_project/src/project.rs:34-37`).

**Where it breaks:** `rust/crates/tiller/src/session.rs`'s `CatalogProjectSettings` — the
*runtime* session model `sidebar.rs` actually consumes, distinct from `tiller_project::Project`
— has only `color_hex`/`display_name`/`icon_kind`/`icon_value` (`:375-380`); no
`default_worktree_base`, no `worktree_location_override`. Consequently:
- `write_catalog` (`session.rs:787-839`) builds a `ProjectRecord` and sets `color_hex`/
  `display_name`/`icon_kind`/`icon_value` from settings (`:807-810`) but never sets
  `record.default_worktree_base`/`record.worktree_location_override` — so even if the DB row
  already had a value, saving the catalog again would silently null it out.
- `restore_catalog` (`session.rs:841-898`) reads `record.color_hex`/`display_name`/`icon_kind`/
  `icon_value` back into `CatalogProjectSettings` (`:870-878`) but never reads
  `record.default_worktree_base`/`worktree_location_override` — so the columns are write-only
  dead weight from the app's point of view today.
- `confirm_worktree_prompt` (`sidebar.rs:1333-1373`) always calls `resolve_parent_directory(
  &repo_root, None)` (`:1354`) and `create_worktree(..., None)` (`:1366`) — both `None`s are
  exactly the two arguments this feature would supply, and there is no data available at that
  call site to supply them with even if it wanted to.
- No UI anywhere — neither the popover nor `render_project_settings` — offers a control for
  either setting, matching the Swift source's placement in `ProjectSettingsSheet.swift` rather
  than the New Worktree popover the evidence happened to check.

**Approach**: (1) add the two fields to `CatalogProjectSettings` and thread them through
`write_catalog`/`restore_catalog` so they round-trip through the existing DB columns; (2) add two
controls to `render_project_settings` (default-base: current/pinned/primary/no-primary per the
clause; location: a folder chooser plus a "restore default parent" action) that emit
`ProjectSettingsChanged` the same way the icon picker does; (3) at `confirm_worktree_prompt`,
read the owning project's settings and pass the real base/override instead of the two hard-coded
`None`s. All three steps are needed together — a fix that stops at (1)+(2) without (3) would
persist a setting nothing ever reads at worktree-creation time.

- **files**: `rust/crates/tiller/src/session.rs` (`CatalogProjectSettings` `:375`, `write_catalog` `:787`, `restore_catalog` `:841`), `rust/crates/tiller_ui/src/sidebar.rs` (`render_project_settings` `:1865`, `WorktreePrompt`/`confirm_worktree_prompt` `:282`/`:1333`)
- **size**: M

---

## Cross-row notes

**Shared causes found, in order of leverage:**
1. **F-PRJ-17 + F-PRJ-18** — one seam, three already-built layers (`tiller_git`,
   `tiller_persistence`, `tiller_project`) with the session-model + UI consuming half missing.
   The highest-leverage fix in this group: closing it clears two rows and needs no new
   persistence-layer or git-layer work at all, only plumbing and two controls.
2. **F-PRJ-06 + F-PRJ-09** — not a code defect at all; the in-flight guard (`project_forms.rs`)
   is correct and unit-tested, but both live operations resolve in sub-frame time in this
   environment, so the guard's "second click during Running" branch can never be observed live
   without artificially slowing the operation first.
3. **F-PRJ-13 + F-PRJ-15 (+ half of F-PRJ-14)** — the "defective"/"discarded" reasoning in all
   three rows describes a state that commit `28a41fa` (same day, same repo, after the evidence
   was collected) appears to have fixed. Flagged `reclassify` rather than silently assumed fixed,
   per the brief.

**Note for whoever schedules the build fleet**: F-PRJ-03 and F-PRJ-17/18 both touch
`rust/crates/tiller/src/main.rs` / `session.rs` — F-PRJ-03's non-git-folder prompt branches
inside `add_project` (`main.rs:2956`), and F-PRJ-17/18's session-model fields live in
`session.rs`, which `main.rs`'s `write_catalog`/`restore_catalog` call directly. Neither touches
the same lines, but both are `codex12`/session-owner territory per `SEAMS.md`'s naming — worth
sequencing rather than parallelizing if the same owner holds both.

**Nothing in this group needs reclassify away from a currently-`build`-shaped verdict** — the
only reclassify candidates found (F-PRJ-13, F-PRJ-15) already carried `FAILED — defective`
verdicts whose stated reasoning looks stale, not verdicts that look wrong in the other direction.


**Needs: build, but small.** Confirmed live and by reading: `render_project_settings`
(`sidebar.rs:1865-2022`) draws heading, path, repo-type text, display-name field, a conditional
`Initialize Git` button, `card.icon_picker`, `Close`, and the project id — no trash/removal
control anywhere in the sheet, matching the evidence exactly. The removal machinery itself is
not missing, only its door from this surface: `request_remove_project` (`sidebar.rs:1055-1075`)
already does everything the VERIFY clause asks — a native `window.prompt` with "Remove project
from Tiller?" / "This only removes the project from Tiller's sidebar. Files on disk will not be
deleted." and `["Remove from Tiller", "Cancel"]` — and on accept emits `SidebarEvent::
RemoveProject`, which `main.rs`'s `remove_project` (`:2999`) turns into `project_catalog.remove`
(files untouched, matching the clause). Today it is only reachable from the context menu one
level up (`dispatch_context_action`, `:959-964`).

**Approach**: add a trash/remove control to `render_project_settings` (near `Close`) whose
`on_click` calls `sidebar.request_remove_project(card.id.clone(), window, cx)` — the exact same
method the context-menu path already calls. No new confirmation logic needed; it already exists
and is already proven.

- **files**: `rust/crates/tiller_ui/src/sidebar.rs` (`render_project_settings`, `:1865-2022`)
- **size**: S

---

## `F-PRJ-12` — half-proven

**Needs: both.** Two distinct things are bundled in one row:

**(a) The staleness bug is real and traced.** `open_project_settings` (`sidebar.rs:889-940`)
snapshots `is_git: row.is_git` once into `ProjectSettingsCard` when the sheet opens. When
`Initialize Git` runs while a *different* code path drives it (`main.rs:3162-3198`), it calls
`project_catalog.refresh_project` then `workspace.refresh_sidebar(cx)` (`main.rs:2696-2705`),
which calls `sidebar.set_projects(...)` (`sidebar.rs:499-509`) — that rebuilds `self.rows` from
scratch (fresh `is_git` per row) but never looks at `self.project_settings` at all. An
already-open sheet's `card.is_git` — and therefore its "Repository: Git/Folder" text and whether
the `Initialize Git` button still renders — is frozen at open-time and never refreshed, exactly
matching "the OPEN sheet itself doesn't live-refresh."

**Approach for (a)**: in `set_projects` (or a small helper called from it), after rebuilding
`self.rows`, if `self.project_settings` is `Some` and its `id` matches a row in the new set,
patch `card.is_git` (and `card.path`, for parity) from that row before returning.

**(b) Display-name propagation is already exercise-only.** `on_display_name_key`
(`sidebar.rs:852-886`) calls `self.set_project_identity(...)` on every keystroke, which directly
rewrites the matching row's `row.title` in `self.rows` (`:800-816`) — so the sidebar row already
updates in-session before `Close` is even clicked, not just on persistence. The evidence's gap
("not directly screenshotted post-close") is a missing photo of already-correct behavior, not a
missing feature.

**Approach for (b)**: edit the display name, click `Close`, screenshot the sidebar row showing
the new name — no code change needed for this half.

- **files**: `rust/crates/tiller_ui/src/sidebar.rs` (`set_projects` `:499`, `open_project_settings` `:889`)
- **size**: S

---

## `F-PRJ-13` / `F-PRJ-15` — FAILED — defective (both) — likely stale verdict, see reclassify note

**Needs: reclassify** (with a residual **build** item for F-PRJ-13's second, unrelated defect —
see below). Both rows' "defective" reasoning is the same claim: the picker updates its own
in-panel selection but the choice "never leaves the panel" / "is discarded" after `Close`. That
claim was true when the evidence's screenshots (`orch18-*.png`) were taken, but it was fixed by
commit `28a41fa` ("feat(P97): the project settings card writes through and reads back," **Aug 14
12:59:05**, same day, landing after `SEAMS.md`'s "Last verified: 2026-08-14, 02:00" note that
still describes the seam as open). Read against the current tree:

- `ProjectIconPicker` is constructed with `.on_change_with_context(...)` at `sidebar.rs:913-919`,
  whose callback is `sidebar.apply_icon_change(project_id, value, cx)`.
- `apply_icon_change` (`sidebar.rs:838-850`) does **both** halves in one call: it updates
  `self.project_identities.insert(project_id, icon)` — the exact map `render_row`'s `glyph`/
  `glyph_color` computation reads from (`sidebar.rs:2066-2081`) — immediately, in-session, on
  every pick (not gated on `Close`); and it emits `SidebarEvent::ProjectSettingsChanged`, which
  `main.rs`'s `update_project_settings` (`:2977-2997`) turns into a `CatalogProjectSettings`
  write persisted via `schedule_catalog`.
- The commit's own message is explicit that this is unverified, not that it's broken: "Code plus
  a green test is NOT EXERCISED, not PASSED — the judgeable proof is still a restart, and a
  critic sets the verdicts." The green test is
  `sidebar::tests::project_settings_changes_update_the_row_and_emit_a_durable_edit`.

So as read, both rows' propagation half now looks correctly wired; what's actually missing is a
fresh live re-drive (pick → `Close` → sidebar row shows the new icon, immediately) plus the
restart check the commit message itself calls out (durable across relaunch). That is the
`needs: exercise` shape, but because the *verdict text* ("never leaves the panel," "is
discarded") is what's stale, this is filed as `reclassify` per the brief's own instruction — flag,
don't silently downgrade to a different bucket.

**F-PRJ-13 has a second, separate, still-live defect** the reclassify does not touch: "the
Colour row is clipped mid-swatch at the panel edge." `controls::color_picker` (`controls.rs:
495-529`) lays out `AgentAccentColor::ALL` (8 fixed 20px swatches, `settings.rs:267-276`) in a
plain `.flex().items_center().gap(...)` row with **no `.flex_wrap()` and no horizontal scroll**,
inside `controls::row_view` (`controls.rs:100-119`) which gives the label `flex_1` but leaves the
control at its natural (unshrinkable) width — all hosted inside the 325px-wide sidebar-docked
settings sheet (`SIDEBAR_WIDTH`, `sidebar.rs:66`) with 16px+8px of padding stacked on each side
before the swatch row even starts. This is a genuine, independent layout bug — not fixed by P97 —
and on its own is enough to keep F-PRJ-13 legitimately `defective` even once the propagation half
re-verifies clean.

**Approach**: (reclassify) re-drive both rows live post-`28a41fa` — pick, `Close`, confirm the
sidebar row's icon updates immediately; relaunch, confirm it persisted. (build, F-PRJ-13 only)
give the color-swatch row in `controls::color_picker` either `.flex_wrap()` or a horizontal
scroll container so all 8 swatches are reachable in the sheet's actual width.

- **files**: `rust/crates/tiller_ui/src/controls.rs` (`color_picker`, `:495-529`; `row_view`, `:100-119`) for the residual clipping defect only — the propagation wiring itself needs no file change
- **size**: S (clipping fix); reclassify/exercise portion is S

---

## `F-PRJ-14` — half-proven

**Needs: exercise.** The GitHub-avatar arm is proven live. The other two arms and the
sidebar-propagation question are all reachable through the same machinery just confirmed for
F-PRJ-13/15, not a separate mechanism: `project_identity.rs`'s `commit_favicon` (`:460-469`) and
the local-PNG commit path both end in the same shared `commit(...)` used by the GitHub-avatar
arm, which calls both `on_change`/`on_change_with_context` (`:330-365`) — i.e. the exact
`apply_icon_change` → `project_identities` → `SidebarEvent::ProjectSettingsChanged` chain P97
wired. `render_row`'s glyph match renders any `ProjectIconValue::Avatar(_)` (GitHub, favicon, or
local PNG alike) as `Icon::Globe` (`sidebar.rs:2066-2072`) — not the actual image — so "reaches
the sidebar row" should already mean a Globe glyph appears, once P97's propagation is live-
reconfirmed (see F-PRJ-13/15 above).

**Approach**: drive the two untried arms — upload a local PNG under `MAX_AVATAR_PNG_BYTES`
(`project_identity.rs:230`) via the real file picker, and enter a favicon domain — then `Close`
and confirm the sidebar row shows the Globe glyph for each, and that a bad PNG / malformed domain
still shows `png_error`/`favicon_error` correctly. This is exercise-only, riding on the same
propagation path already re-checked for F-PRJ-13/15 — no new production code expected.

- **files**: none (exercise only)
- **size**: S

---
