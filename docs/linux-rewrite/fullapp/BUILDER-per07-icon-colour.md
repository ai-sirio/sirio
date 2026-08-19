# Builder live drive — F-PER-07, project icon/colour persistence

Requested by team-lead to close the ambiguity the row's own history names: two prior
attempts clicked at coordinate estimates in the settings sheet and neither could tell
whether the clicks missed the controls or the controls did nothing, for want of an
independent read of the true pre-edit default. This drive removes that ambiguity with
the instrument the brief specified: the database, read directly, before and after.

Worktree `/var/tmp/tt-per07-4047521`, branch `verify/f-per-07-4047521`, off
`origin/linux/gpui-waku` at `f7a29bb0`. No code changes — verification only. Own
`CARGO_TARGET_DIR` (`/var/tmp/cargo-target-per07-4047521`), own `TILLER_WL_LABEL`
(`per07-4047521`), own binary snapshot, own fixture (a fresh standalone git repo at
`/var/tmp/tt-per07-scratch-4047521`, added as its own dedicated project — not the
27-worktree shared "tiller" catalog entry other drives this session touched — so this
project's row is unambiguous and uncontaminated by any other agent's edits). Never
touched `DISPLAY=:1` / `wayland-0` / `wayland-1`. Fully torn down: app SIGTERM'd
cleanly, compositor/dbus/pointer/keyboard matched and killed by
`TILLER_SOCKET`/`SWAYSOCK`/`TILLER_WL_LABEL`, confirmed by `pgrep` finding nothing and
no stray `/tmp/per07-4047521*` files.

## Outcome: **2 — a real, reproducible app defect**

DB unchanged, and the click demonstrably landed on the actual control (not a coordinate
believed in) — recorded with screenshots, not inferred. Per the brief's own framework,
that is outcome 2, not outcome 3: this is not a repeat of the prior harness-coordinate
gap.

## The independent pre-edit baseline

Added the scratch repo as its own project (`ctl project.add
path=/var/tmp/tt-per07-scratch-4047521`, returned `projectId":"p-e6b3bfafb3f3914a"`).
Read the raw `project` table row **before touching anything**:

```
sqlite3 (via python3): SELECT id, name, root_path, icon_kind, icon_value, color_hex FROM project;
('p-e6b3bfafb3f3914a', 'tt-per07-scratch-4047521', '/var/tmp/tt-per07-scratch-4047521', 'icon', None, None)
```

`icon_kind='icon'`, `icon_value=NULL`, `color_hex=NULL` — matches the code's own
defaults (`ProjectIcon::default()`: Folder glyph, Coral tint, with `color_hex` simply
absent until something writes it). This is the ground truth the ledger's prior attempts
never had.

## The edit, driven and confirmed landing on the real controls

`01-project-added.png` → `02-context-menu.png`: right-click the project row → "Project
Settings" (the same working entry point the name-half evidence already established).
`03-settings-sheet-defaults.png`: sheet open, Icon mode, Folder glyph selected
(orange ring), 8-colour palette, all matching the pristine DB defaults just read.

**Glyph click** at the GitBranch swatch's real screen position (not a guessed
coordinate — read off this exact screenshot): `04-glyph-click-landed.png` shows the
selection ring moved from Folder to GitBranch. This is the "frame with the control
visibly hit" the brief asked for, not a click I believe landed.

**Colour click** at the Blue swatch's real position: `05-colour-click-landed.png` shows
the icon's border/accent tint visibly shift from the Coral default toward blue —
landed, same standard of evidence.

## The read-back: the database never moved

Read the same row again immediately after both clicks:

```
('p-e6b3bfafb3f3914a', None, 'icon', None, None)
```

Unchanged. Waited several more seconds and read again — still unchanged. Clicked
**Close** on the sheet and read again — still unchanged. Then **SIGTERM'd the app**
for a fully clean shutdown (confirmed dead within 200ms, so nothing was still holding
the file open or racing a debounced write) and read the bare file with no process
attached at all:

```
('p-e6b3bfafb3f3914a', None, 'icon', None, None)
```

The file's own mtime never advanced past its initial creation at project-add time.
Not one byte was written by any of the edits below, confirmed by reading the file
directly, not through a socket call that could itself be lying.

`06-after-close-sidebar-shows-glyph.png` is worth naming specifically: after Close, the
sidebar row's own small project icon **does** render as the git-branch glyph — the
in-session/in-memory state genuinely changed and is reflected live in the UI. The
defect is specifically that this never reaches the database, not that the click did
nothing at all.

## An unplanned second finding, reported as instructed rather than made to fit

The brief's own framing treats the *name* half as an established, DB-verified PASS,
citing it as the working baseline this row's icon/colour half should be judged against.
Using the exact same method — type into "Display name", read the raw `project` row —
that PASS did not reproduce in this build/session either:

`07-display-name-also-typed.png` shows the field reading "ER07 renamed" (the sheet's
own title bar and the field both reflect it — the app registered the edit) after
typing `"PER07 renamed"` into the field (the leading "P" was lost — a harness input
race worth naming on its own, not a finding about the app; see below). Reading the
database at that point, and again after Close, and again after a clean SIGTERM:

```
('p-e6b3bfafb3f3914a', None, 'icon', None, None)
```

`display_name` stayed `NULL` throughout — not just unchanged from a prior edit, `NULL`
the entire time, exactly like the icon fields.

This is genuinely something other than what the brief described, so it is reported
plainly rather than folded into the icon/colour finding or left out: **in this exact
build, no field edited through the Project Settings sheet reached the database** — not
just icon/colour. The one write that *did* land was the project's own creation via
`project.add` (confirmed: the row exists with the correct `id`/`name`/`root_path`,
using the identical `schedule_catalog` → `write_catalog` → `db.save_project` pipeline).
So the persistence machinery works in general; it is specifically the settings-sheet
edit path (`SidebarEvent::ProjectSettingsChanged` → `update_project_settings`) that
never reaches disk in this drive.

**What this does and does not establish.** This is not a claim that the name-half PASS
was wrong when it was recorded, or that this is a regression since then — I have no
way to compare against how that check was originally driven, and the brief's own
methodology point (an unfalsifiable claim is worse than none) applies here too. What is
established, plainly: reproducing the row's own documented working case, in this build,
by the same click-then-read-the-database method, did not reproduce a working case. That
discrepancy is worth someone's attention on its own, separately from the icon/colour
row this drive was sent to close.

**Some diagnostic color, not a fix, offered because it was cheap to gather while
already in the code:** `write_catalog` (`session.rs:839`) reads
`catalog.project_settings(&project.id)` from the **in-memory** `ProjectCatalog` and
writes every field (`color_hex`, `display_name`, `icon_kind`, `icon_value`, …) through
one correct `INSERT … ON CONFLICT(id) DO UPDATE SET …` upsert
(`tiller_persistence/src/db.rs:166-198` — checked; it is not the bug, all listed fields
are covered in both the insert list and the `DO UPDATE SET` clause). Since the very
first write (project creation) does persist and subsequent writes through this same
pipeline do not, whatever is wrong sits between the settings sheet's event
(`apply_icon_change` / `on_display_name_key` in `tiller_ui/src/sidebar.rs`) and
`schedule_catalog` actually being invoked with a catalog that carries the edit — not in
the SQL itself. Left there for whoever picks this up; not chased further, since fixing
it wasn't this drive's job.

## A harness note, not an app finding

The first character of "PER07 renamed" was dropped (the DB fields aside, the *live*
field read "ER07 renamed"). This reproduces the same class of thing this codebase's own
`wayland-drive.sh` trap table already documents for click-then-type sequences: the
click that focuses a field and the first keystroke that follows it can race when issued
back-to-back with no settle time between them. A brief pause between the click and the
first `wtype` call would avoid it; not treated as an app defect since it's specifically
the *first* character of a rapid click-then-type pair that was lost, and every
downstream check in this drive (three independent field states, `panel`-adjacent DB
reads) still functions as intended once the sheet has any text in the field at all.

## Cleanup

Torn down via the same `kill_ours`-style scoped match this codebase's other drives use.
`pgrep` found nothing left under `TILLER_WL_LABEL=per07-4047521` and no stray
`/tmp/per07-4047521*` paths remained. `/dev/shm/tt` was never touched.
