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
project's row is unambiguous and uncontaminated by any other agent's edits).

## Outcome: **the feature works — icon, colour, and display name all persist**

**This corrects the verdict originally recorded here.** The first pass of this drive
concluded outcome 2 ("a real, reproducible app defect": click lands, DB never moves).
That conclusion was wrong, and the paragraph it rested on was invalid reasoning, not
just an unlucky read — see "What was wrong with the original verdict" below. Three
independent repros — two in this redo, one by team-lead using a from-scratch database —
all show the edit reaching disk. There is no remaining positive evidence for a defect
on this path.

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
never had. (This baseline-first discipline was right and stays right — nothing below
changes it.)

## The edit, driven and confirmed landing on the real controls

`01-project-added.png` → `02-context-menu.png`: right-click the project row → "Project
Settings" (the same working entry point the name-half evidence already established).
`03-settings-sheet-defaults.png`: sheet open, Icon mode, Folder glyph selected
(orange ring), 8-colour palette, all matching the pristine DB defaults just read.

**Glyph click** at the GitBranch swatch's real screen position (not a guessed
coordinate — read off this exact screenshot): `04-glyph-click-landed.png` shows the
selection ring moved from Folder to GitBranch.

**Colour click** at the Blue swatch's real position: `05-colour-click-landed.png` shows
the icon's border/accent tint visibly shift from the Coral default toward blue — landed,
same standard of evidence.

`06-after-close-sidebar-shows-glyph.png`: after Close, the sidebar row's own small
project icon renders as the git-branch glyph — in-memory state genuinely changed and is
reflected live in the UI.

## What was wrong with the original verdict

The original read-back section reported the database "never moved" after these clicks,
Close, and a clean SIGTERM, and treated the main `.sqlite` file's mtime never advancing
past project-creation time as confirming evidence, on the theory that a SIGTERM'd,
fully-dead process with "nothing still holding the file open" ruled out a race.

Team-lead identified the flaw directly: the database opens
`PRAGMA journal_mode = WAL` (`tiller_persistence/src/db.rs:142`). Under WAL, a committed
write lands in the `-wal` sidecar file; the main file's mtime and content do not change
until a checkpoint runs. SIGTERM triggers no checkpoint at all — there is no signal
handler, the process just dies — so "SIGTERM for a clean shutdown, then read the bare
file with no process attached" specifically discards the one place a real write would
be sitting. **Mtime-never-advanced is the expected signature of a working WAL write,
not evidence of a missing one.** That paragraph is wrong and is withdrawn, not softened.

I no longer have this exact instance to re-examine: cleanup for this drive removed
`/tmp/per07-4047521.sqlite` along with everything else under that label (see Cleanup,
below — this was confirmed clean at the time, which is exactly the problem in
hindsight), and I don't have the literal read command/connection flags from that
original session preserved anywhere outside this document, which only recorded
outcomes. So I can't forensically replay the exact read that came back empty, and I'm
not going to guess at a mechanism I can't check. What I can do, and did, is rerun the
whole test properly and see whether it reproduces — it doesn't, twice, under conditions
designed specifically to rule out WAL-blindness.

## The redo: sidecars checked first, app never stopped, run twice

Team-lead's ask was specific: re-read the database with the `-wal`/`-shm` sidecars
present and the app still running, before any teardown, and also probe "a pre-seeded
row vs. an in-session `project.add` row" as the likeliest explanation for why team-lead's
own first reproduction attempt had succeeded where mine failed. Both conditions were run
back to back in one live app process (label `per07pre-4047521`, DB at
`/tmp/per07pre-4047521.sqlite`):

**Condition 1 — a row that went through a full boot/normalization cycle.** A project
row was pre-seeded directly via SQL with an arbitrary id (app not running), then the app
was launched against that DB. Boot-time catalog normalization rewrote the seeded row to
the same canonical derived id `project.add` always produces for that path
(`p-e6b3bfafb3f3914a` — the identical id this exact drive's own baseline used above).
Settings sheet opened, defaults confirmed matching the pristine row. Display name, glyph
(GitBranch), and colour (Blue) were each clicked individually with a confirming
screenshot after every click (`builder-per07-redo-shots/02-preseed-edits-landed.png`:
selection ring on GitBranch, blue ring on the Blue swatch, name field reading
`PRESEED_MARKER_4047521`).

Sidecars checked **before** touching the file with any reader:

```
-rw-r--r-- ... 172032 ago 20 02:00 /tmp/per07pre-4047521.sqlite
-rw-r--r-- ...  32768 ago 20 02:00 /tmp/per07pre-4047521.sqlite-shm
-rw-r--r-- ... 362592 ago 20 02:01 /tmp/per07pre-4047521.sqlite-wal
```

App confirmed alive via `pgrep`. Read through a normal (non-immutable)
`sqlite3.connect`, which is WAL-aware:

```
('p-e6b3bfafb3f3914a', 'PRESEED_MARKER_4047521', 'icon', 'git-branch', 'blue')
```

All three values present. The read connection itself triggered a passive checkpoint
(sidecars gone immediately after, main file mtime advanced) — the WAL mechanics
team-lead described, watched happening directly in this exact session.

**Condition 2 — a row created in-session via `project.add`, same live process.** To
isolate the specific hypothesis, a second fixture (`/var/tmp/tt-per07-insession-4047521`)
was added via the control socket's `project.add` in the *same still-running* process
used above — same build, same OS session, same everything except how the row came to
exist. (A stale orphaned process from an earlier relaunch under the same label was found
sharing the same DB file at this point and was killed before this step, to keep the
condition clean — see the harness note at the end.)

Settings sheet opened, defaults confirmed. Display name, glyph, and colour clicked
individually; `builder-per07-redo-shots/04-insession-glyph-landed-zoom.png` and
`05-insession-colour-landed-zoom.png` are tight crops confirming each landed on the real
control.

Sidecars checked before any read:

```
-rw-r--r-- ... 208896 ago 20 02:07 /tmp/per07pre-4047521.sqlite
-rw-r--r-- ...  32768 ago 20 02:11 /tmp/per07pre-4047521.sqlite-shm
-rw-r--r-- ... 894072 ago 20 02:11 /tmp/per07pre-4047521.sqlite-wal
```

App still running. Read:

```
('p-f8eb042838e93bf1', 'INSESSION_MARKER_4047521INSESSION_MARKER_4047521', 'icon', 'git-branch', 'blue')
```

Also landed. (The doubled marker text is a harness click-then-type race, the same class
already documented below — not an app bug: one `wtype` call appeared to drop on the
screenshot taken right after it, but had actually landed with delayed rendering, and a
retry then appended on top. Doesn't affect what this condition tests.)

Both conditions land the write identically, so the pre-seeded-vs-in-session hypothesis
is **not** the explanation for the original discrepancy — team-lead independently
confirmed the same thing with a third repro, from an empty database, entirely on their
own side, landing all three fields. Three independent positive repros, zero remaining
reproducible negatives.

## The display-name finding, also withdrawn

The original drive additionally reported that `display_name` also never reached the
database, treating that as a second, unplanned finding worth flagging on its own — the
right instinct (a result that contradicted a recorded PASS was reported instead of
quietly dropped, which is what got this whole row a second look). But it rests on the
same WAL-blind reads as the icon/colour finding, and both redo conditions above show
`display_name` persisting correctly (`PRESEED_MARKER_4047521` and
`INSESSION_MARKER_4047521...` both landed). This finding is withdrawn along with the
icon/colour one. The diagnostic pointer offered alongside it — that `save_project`'s SQL
itself is a correct upsert covering every field, so the break (if any) sits between the
settings-sheet event and the write actually reaching disk — no longer has anywhere to
point, since nothing between those two places is now shown to be broken either.

## A harness note, not an app finding

A leading character was dropped typing into the display-name field in the original
drive (`"PER07 renamed"` landed as `"ER07 renamed"`), and the redo hit variants of the
same class again (a full block of filter text silently not landing at all until the
settle time before typing was lengthened; a `wtype` call whose result didn't show up
until a later screenshot). This reproduces the same class of thing this codebase's own
`wayland-drive.sh` trap table already documents for click-then-click sequences, now also
seen for click-then-type: the click that focuses a field and the keystrokes that follow
it can race, or the repaint confirming them can lag behind the actual state, when issued
back-to-back with little settle time. Not treated as an app defect; a longer pause
between the click and the `wtype`/`type` call, and not trusting a screenshot taken too
soon after typing, avoided it for the rest of the redo.

**A real, separate harness bug, found by accident and flagged rather than fixed:**
running the same label repeatedly with `TILLER_WL_KEEP=1` (to avoid a full
recompositor-restart between every probe) left an old app process alive alongside a new
one, both holding the same DB file, at one point during the redo. Cause:
`Scripts/wayland-drive.sh:204` —

```
kill_ours TILLER_SOCKET "$SOCK" tiller
```

— matches the literal comm name `tiller`, but a renamed binary via `TILLER_WL_BIN` (used
throughout this drive and the redo) has a different, kernel-truncated comm
(`tt-per07-bin-40`), so this call never matches it and the previous instance is never
reaped by name. In practice the old process usually still dies, as a side effect of its
compositor being killed by the neighbouring `kill_ours SWAYSOCK "$SWAYSOCK" sway` call
(losing the Wayland connection is fatal to a GPUI app) — almost certainly why this has
gone unnoticed — but that's incidental, not a fix, and the one time it didn't cascade
cleanly is exactly the kind of state that could produce nondeterministic persistence
results if hit at the wrong moment. The fix already exists a few lines away in
`cleanup()` (line 190, with a comment explaining why): use `"$(basename "$BIN")"`
instead of the literal `tiller` at line 204 too. Not fixed here — out of scope for this
task, flagged for whoever picks up harness maintenance.

## Cleanup

Original drive: torn down via the same `kill_ours`-style scoped match this codebase's
other drives use. `pgrep` found nothing left under `TILLER_WL_LABEL=per07-4047521` and
no stray `/tmp/per07-4047521*` paths remained — which is also, in hindsight, why the
exact original read can no longer be forensically replayed (see above). `/dev/shm/tt`
was never touched.

Redo drive (label `per07pre-4047521`): torn down the same way after the redo concluded
— `dbus-daemon`, `sway`, the virtual-pointer client, and the app binary all killed by
PID, confirmed clean via `pgrep`.
