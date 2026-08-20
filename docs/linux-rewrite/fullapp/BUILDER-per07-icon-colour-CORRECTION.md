# Correction to `debf6e80` — F-PER-07 icon/colour DOES persist

`debf6e80` concluded outcome 2 (a real defect: click lands, DB never moves). That
conclusion is wrong. This redo, done against team-lead's explicit falsification
test, shows the values reaching the database every time, with the sidecars
present and the app never stopped. The row should return to PASSED, matching
the name half.

## What was wrong with the original evidence

`debf6e80`'s strongest evidence was: the main `.sqlite` file's mtime never
advanced past project-creation time, and a post-SIGTERM read of the bare file
(no process attached) still showed the pristine defaults. Team-lead identified
the flaw directly: the database opens `PRAGMA journal_mode = WAL`
(`tiller_persistence/src/db.rs:142`). Under WAL, a committed write lands in the
`-wal` sidecar; the main file's mtime and content do not change until a
checkpoint runs. SIGTERM triggers no checkpoint (no signal handler, the
process just dies), so reading "the bare file after SIGTERM" specifically
throws away the one place a real write would be sitting. Mtime-never-advanced
is the *expected* signature of a working WAL write, not evidence of a missing
one.

## The corrected test, run twice

Team-lead's ask was specific: re-read the database with the `-wal`/`-shm`
sidecars present and the app still running, before any teardown, and also
probe the "pre-seeded row vs. in-session `project.add` row" hypothesis as the
likeliest explanation for why team-lead's own reproduction succeeded where
mine failed.

Both conditions were run, back to back, in the same live app process
(`per07pre-4047521`, DB at `/tmp/per07pre-4047521.sqlite`):

**Condition 1 — a row that went through a full boot/normalization cycle.**
A project row was pre-seeded directly via SQL with an arbitrary id (app not
running), then the app was launched against that DB. Boot-time catalog
normalization rewrote the seeded row to the same canonical derived id
`project.add` always produces for that path (`p-e6b3bfafb3f3914a`) — confirmed
directly by reading the DB after boot. Its Settings sheet was opened
(`01-preseed-settings-defaults.png`, defaults confirmed: Folder glyph, no
display name, matching the pristine row). Display name, glyph (GitBranch),
and colour (Blue) were each clicked individually with a confirming screenshot
after every click (`02-preseed-edits-landed.png` shows all three landed:
selection ring on GitBranch, blue ring on the Blue swatch, name field reading
`PRESEED_MARKER_4047521`).

Sidecars checked **before** touching the file with any reader:

```
-rw-r--r-- ... 172032 ago 20 02:00 /tmp/per07pre-4047521.sqlite
-rw-r--r-- ...  32768 ago 20 02:00 /tmp/per07pre-4047521.sqlite-shm
-rw-r--r-- ... 362592 ago 20 02:01 /tmp/per07pre-4047521.sqlite-wal
```

App still running (`pgrep` confirmed the binary alive). Read through a normal
(non-immutable) `sqlite3.connect`, which is WAL-aware:

```
('p-e6b3bfafb3f3914a', 'PRESEED_MARKER_4047521', 'icon', 'git-branch', 'blue')
```

All three values present. The read connection itself triggered a passive
checkpoint (sidecars were gone immediately after, main file mtime advanced) —
directly demonstrating, in this exact session, the checkpoint mechanics
team-lead described.

**Condition 2 — a row created in-session via `project.add`, same process.**
To isolate team-lead's specific hypothesis, a second fixture
(`/var/tmp/tt-per07-insession-4047521`) was added via the control socket's
`project.add` in the *same still-running* app process used above — same
build, same OS session, same everything except how the row came to exist.
(A stale orphaned process from an earlier relaunch was found sharing the same
DB file at this point and was killed before this step — see the harness note
below — to keep this condition clean.)

Settings sheet opened (`03-insession-settings-defaults.png`, defaults
confirmed). Display name, glyph (GitBranch), and colour (Blue) clicked
individually; `04-insession-glyph-landed-zoom.png` and
`05-insession-colour-landed-zoom.png` are tight crops confirming each landed
on the real control (selection ring + tinted icon border, colour ring on
Blue).

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

(The doubled marker text is a harness click-then-type race — the same trap
`wayland-drive.sh`'s own table documents — not an app bug; a `wtype` call
that appeared to drop was actually delayed-rendering and landed on a later
screenshot, and a retry then appended on top. It doesn't affect what this
condition tests.)

Both conditions land the write. Team-lead's specific alternative hypothesis
— pre-seeded row vs. in-session `project.add` row — is **not** the
explanation, since both now persist identically. The full explanation for the
original null result is the WAL-blind reading method itself: reading the bare
file after SIGTERM cannot see a committed write that is sitting in `-wal`,
full stop.

## Conclusion

Outcome 1 per the original brief's own framework: the DB changed, the feature
works. `debf6e80`'s outcome-2 conclusion (real defect) is withdrawn. The
icon/colour half of F-PER-07 should be promoted to PASSED alongside the name
half, using this redo (sidecars checked before any read, app never stopped,
two independent rows) as the verifying evidence.

## A harness bug found along the way, worth fixing separately

While re-launching `per07pre-4047521` repeatedly with `TILLER_WL_KEEP=1` under
one label, an old app process (from an earlier relaunch) was found still
alive alongside a newer one, both holding the same DB file. Cause:
`Scripts/wayland-drive.sh:204` —

```
kill_ours TILLER_SOCKET "$SOCK" tiller
```

— matches the literal comm `tiller`, but a renamed binary via `TILLER_WL_BIN`
(as used throughout this drive: `tt-per07-bin-4047521`) has a different comm
name (`tt-per07-bin-40`, kernel-truncated to 15 chars), so this call never
matches it and the previous instance is never reaped by name. In practice the
old process usually dies anyway as a side effect of its compositor being
killed by the neighbouring `kill_ours SWAYSOCK "$SWAYSOCK" sway` call
(losing the Wayland connection is fatal to a GPUI app) — which is almost
certainly why this has gone unnoticed — but that's incidental, not a fix, and
the one time it didn't cascade cleanly (two live copies of the app open on
one DB) is exactly the kind of state that would produce nondeterministic
persistence results if hit at the wrong moment. The fix is the same one
already applied a few lines away in `cleanup()` (line 190, with the comment
explaining why): use `"$(basename "$BIN")"` instead of the literal `tiller`
at line 204 too. Not fixed here — flagging it since it surfaced by accident
during this redo, not something this task was scoped to touch.
