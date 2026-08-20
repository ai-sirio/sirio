# F-PER-07 — the icon/colour edit does reach the database; the read protocol was wrong

**Date:** 2026-08-20 · **Verifier:** team-lead · **Base:** `90329a22`
**Binary under test:** `/var/tmp/cargo-target-per07-4047521/debug/tiller` — *the reporting
builder's own build*, deliberately, so "different code" is not available as an explanation.
**Frames:** `verify-per07-read-protocol-shots/` · **Reader:** `verify-per07-readproj.py`

## What this resolves

`debf6e80` (branch `verify/f-per-07-4047521`, doc `BUILDER-per07-icon-colour.md`) reports
F-PER-07 as **outcome 2 — a real, reproducible app defect**: the Project Settings icon and
colour edits visibly land in the UI but "never reach the database". That commit was held out
of the merge after the finding failed to reproduce twice.

It is now refuted, not on the balance of repeated attempts, but at the level of the
**instrument**. The claim rests on a read protocol that returns a false negative on a
database *demonstrably containing* the values.

Nothing here is a criticism of the gestures. The builder's clicks landed, their frames prove
it, and my frames reproduce theirs exactly. The gesture half of that work stands.

## The defect in the protocol

The reported reads were taken from the bare `.sqlite` file — copied or opened away from its
`-wal` / `-shm` neighbours, with the app SIGTERM'd first. The database runs in WAL mode
(`PRAGMA journal_mode = WAL`, `tiller_persistence/src/db.rs:142`). Under WAL:

1. A committed write lands in the `-wal` sidecar, not in the main file.
2. **The main file's mtime does not advance until a checkpoint runs.**
3. `SIGTERM` is not a clean shutdown — there is no handler, so no checkpoint runs, and the
   sidecar survives the process.

So the bare file is a *pre-checkpoint snapshot*. Reading it shows the state as of the last
checkpoint, which looks exactly like "the write never happened".

### Demonstration, with a control younger than the effect

The trap in verifying a null result is that the obvious control is too old. The builder's
read-back does contain one non-null column, `icon_kind='icon'` — but that was written at
project-add time, *before* the disputed edit. A stale read returns the whole pre-edit row:
`name`, `root_path` and `icon_kind` all populated, `icon_value` and `color_hex` NULL. That is
bit-for-bit the reported observation. A pre-edit column therefore proves the row exists; it
cannot distinguish "no write happened" from "I am reading an old snapshot".

The control has to be **younger than the effect under test**. I used a second `project.add`
— a write nobody disputes — issued *after* the baseline:

| read | projects seen | main `.sqlite` mtime |
| --- | --- | --- |
| after adding A, sidecars present | 1 | 1787184607.438 |
| after adding B, sidecars present | 2 | 1787184607.438 |
| **bare copy, app still running** | **0** | 1787184607.438 |
| **bare copy, after SIGTERM, process confirmed exited** | **0** | 1787184607.438 |
| same file, sidecars present, process exited | 2 | 1787184607.438 |

Two committed writes; the main file's mtime never advanced by one microsecond; the `-wal`
survived the SIGTERM at 160712 bytes, proving no checkpoint ran. **The bare read reports zero
projects on a database that contains two.**

This also disposes of the mtime argument in `debf6e80`. "The file's own mtime never advanced"
is offered there as the clinching evidence. Under WAL it is the *expected* observation whether
or not the write happened, so it carries no information at all.

## The substantive answer: all three fields persist

Their binary, their controls, their gestures — the only difference is the read.

| stage | `color_hex` | `display_name` | `icon_value` |
| --- | --- | --- | --- |
| A — pre-edit baseline | `None` | `None` | `None` |
| B — after glyph + colour clicks | `'blue'` | `None` | `'git-branch'` |
| C — after Close | `'blue'` | `None` | `'git-branch'` |
| after app restart, before any gesture | `'blue'` | `None` | `'git-branch'` |
| after typing into Display name, settled | `'blue'` | `'Renamed Alpha'` | `'git-branch'` |

`01-sheet-defaults-folder-and-coral.png` is the pristine sheet: Folder glyph ringed, coral
default, matching the pre-edit row. `02-glyph-click-landed.png` and
`03-colour-click-landed.png` show the selection ring moved to GitBranch and to the blue swatch
— the same standard of evidence the builder set, and the same result.

`04-after-restart-sidebar-glyph.png` and `05-after-restart-sheet-selections.png` are the part
their run never reached: after a **full process restart**, the sidebar row renders the
git-branch glyph and the reopened sheet still shows both selections. The values did not merely
get written, they round-trip.

The **bare-file read of this same instance, after SIGTERM, still reports zero rows** — while
the sidecar-present read of the identical file returns
`('p-9d1c6cc204d5707f', 'lead-per07wal-A', '/var/tmp/lead-per07wal-A', 'blue', …, 'git-branch')`.

## A second trap, independent of WAL: the persist is debounced

Reading immediately after a gesture can catch a *partial* value. Typing `Renamed Alpha` and
reading straight away returned `display_name='Re'`; a read after Close returned `'Ren'`; with
no further input at all, later reads returned the full `'Renamed Alpha'`. The write is
debounced and lags the keystrokes.

This matters because it produces the same surface symptom as the WAL trap — a field that looks
unwritten — from an entirely different cause, and it will not be fixed by getting the sidecars
right. A verifier who reads once, promptly, and sees an absent or truncated value has measured
their own timing, not the app.

## What is *not* claimed

- **The builder's exact tuple was not reproduced.** Their bare read returned the row with
  NULLs; mine returns *no rows*, because less had been checkpointed in my run. That is a
  difference in how stale the snapshot was, not a difference in kind — their tuple is exactly
  what a stale read of a pre-edit checkpoint looks like. The mechanism is reproduced; the
  precise degree of staleness is not.
- **Their original database no longer exists** (their own cleanup, `BUILDER-per07-icon-colour.md`
  line 157), so the reported reads cannot be re-run against the original artefact. This
  write-up refutes the protocol, and therefore any null result the protocol produced — it does
  not re-examine their data, which is gone.
- **The second finding in `debf6e80`** — that the already-PASSED *name* half also failed to
  persist — is produced by the same protocol and falls with it. The table above independently
  shows `display_name` reaching the database. I drove that here; I did not re-derive the
  original name-half evidence.
- **Emoji and Avatar icon modes were not driven.** Only the Icon-mode glyph grid and the
  8-colour palette were exercised.
- The empty-looking Display name field in `04-name-typed` (scratch only, not kept) alongside a
  populated `display_name` column is unexplained and was not chased; it did not affect any
  conclusion above, since the DB is the authority for this row and the field was not re-read
  after the value settled.

## For the record

Method note for whoever verifies a DB-backed row next: read the database **in place with its
sidecars present**, using a read-only URI (`file:…?mode=ro`). Opening it read-write triggers a
checkpoint, which silently repairs the very staleness you are trying to detect — that mistake
invalidated my own first attempt at testing this hypothesis, which is why the earlier message
to the builder called WAL *plausible* rather than demonstrated. It is demonstrated now.
