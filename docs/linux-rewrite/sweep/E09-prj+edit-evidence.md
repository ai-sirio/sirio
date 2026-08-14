# E09-prj+edit — drive evidence

Slice: F-PRJ-06, F-PRJ-09, F-PRJ-14, F-PRJ-16, F-EDIT-05, F-EDIT-12. Wayland lane only
(`TILLER_WL_LABEL=drive-E09-prj+edit`). Captures under
`reference/linux-progress/drive-E09-prj+edit/`.

## F-PRJ-06 — half-proven, missing half attempted, still not closeable

Prior evidence (ledger line 99) already proved the empty-URL disablement half live. This
drive targeted the remaining half: the double-submission guard on `Clone repository`.

Sequence driven: `+` → `Clone Repository…` → click the URL field → type a URL → click
`Clone repository`. First click reliably submits (confirmed repeatedly: field non-empty →
button enabled → click → button relabels `Retry clone` and shows the real red error text
`Clone failed: git exited with status 128: fatal: repository '…' does not exist`,
e.g. `06-fprj06c-doubleclick.png` shows the settled single-click outcome for URL `https://x.io/a/b.git`
typed as `http`).

**Discovered lane behaviour, load-bearing for this row**: a synthetic `click` is only reliably
delivered to the app if a `shot` (which sleeps ~1s and forces a repaint) follows it — clicks fired
back-to-back with no intervening `shot` were repeatedly dropped entirely (confirmed the *first*
click of a bundled pair failed to register in four separate attempts: `04-fprj06b-result.png` (no
state change after two immediate clicks), and the `typed4`/`05-typed4.png`,
`06-dc.png` exploratory captures in this same session). Once a `shot` was inserted after each
click, single clicks landed 100% of the time (`05-c1.png`).

**Conclusion: the double-submission guard's missing half is not reachable by this lane's tooling.**
Two independent facts compound: (1) this lane cannot fire two clicks close enough together to
race the app — every reliably-delivered click needs the ~1s settle a `shot` provides, so
"rapid" back-to-back clicks are a synthetic-input limitation, not a probe of the guard; (2) this
sandbox has no network, so any clone attempt (valid-looking URL or not) fails inside a single
frame — `git exited with status 128` arrives before the next capture, leaving no observable
in-flight window even if a fast second click did land. Marking the guard half `could-not-reach`;
the empty-URL-disablement half remains proven from prior evidence and is not re-claimed here.

Captures: `02-fprj06-menu.png` … `07-fprj06b-settle.png`, `02-fprj06c-menu.png` …
`06-fprj06c-doubleclick.png` (all under `reference/linux-progress/drive-E09-prj+edit/`).

## F-PRJ-09 — half-proven, missing half attempted, same lane limit applies

Prior evidence (ledger line 102) proved the empty-name disablement half live. This drive
targeted the duplicate-submission guard on `Create project`.

Sequence driven: `+` → `Create Project` → click the name field → type `e09testproj` → click
`Create project` (with a `shot` settle after) → click `Create project` again → `shot`.

Result: the first click created a real project — `06-fprj09b-click1.png` shows the sidebar
populated with a new `e09testproj` row at `/home/enzopalmisano/e09testproj`, and the dialog
closed. Filesystem check (read-only `ls`, no edit) confirmed exactly one folder was created,
no duplicate. The second click (`07-fprj09b-click2.png`) landed on now-empty sidebar space
below the closed dialog and visibly did nothing — the dialog was already gone.

**Same conclusion as F-PRJ-06**: creation is local-filesystem-only and completes within a
single frame, so the dialog closes before any second click could reach the (already-gone)
`Create project` button — there is no in-flight window this lane's tooling can hit. The
duplicate-submission guard's missing half is `could-not-reach` for the same structural reason
(instant completion + no sub-second click delivery); the empty-name-disablement half remains
proven from prior evidence and is not re-claimed here.

Captures: `02-fprj09b-menu.png` … `07-fprj09b-click2.png`.

## F-PRJ-14 — Avatar tab exercised live: PNG/GitHub/favicon fields all present, GitHub arm works

Prior evidence (ledger line 107) only proved the `Avatar` tab exists, unopened. This drive opened
it and exercised the GitHub-avatar arm.

Route: hover the `tiller` project sidebar row → click the gear icon that appears → Project
Settings sheet opens directly (no separate menu) → `Avatar` tab. `03-fprj14b-avatar.png` /
`04-fprj14b-ghfieldclick.png` show all three documented controls: `Choose PNG…` button, a
`GitHub user or repository` field with `Use GitHub Avatar` button, and a `Domain, like
example.com` field with `Use Favicon` button (the last is clipped by the sheet's fixed width,
same defect family as the Colour-row clipping noted for `F-PRJ-13`).

**GitHub arm driven live**: typed `octocat` into the GitHub field, clicked `Use GitHub Avatar`
— `06-fprj14b-ghclick1.png` shows the confirmation line `Current: GitHub avatar for octocat`
appear beneath the controls. This is a real accepted submission, not just a rendered field.

Not driven: `Choose PNG…` (needs a file-picker portal, likely invisible to this lane the same
way other file dialogs are) and the favicon-domain arm (same button pattern as the GitHub arm,
not separately exercised — no reason to expect it behaves differently, but not claimed).
Whether the accepted GitHub avatar actually reaches the sidebar/project row was not checked here
— cross-reference the `on_change` propagation defect already logged against `F-PRJ-13`/`SEAMS.md`,
which plausibly affects this control too since it lives in the same picker.

Captures: `02-fprj14b-settings.png` … `07-fprj14b-ghclick2.png`.

## F-PRJ-16 — Emoji tab exercised live: single-emoji entry, invalid-input validation, both driven

Prior evidence (ledger line 109) only proved the `Emoji` tab exists, unopened. This drive opened
it and exercised both halves of the row's clause.

**Invalid multi-character input**: typed `ab` into the emoji field, clicked `Set Emoji` →
`06-fprj16b-setemoji-ab.png` shows the field outlined and the exact validation text `Enter
exactly one emoji.` appear beneath the controls — a real, visible, specific error.

**Valid single-emoji entry**: cleared the field (`BackSpace`), typed the literal emoji `🎉`
via `wtype` (this lane's docs list non-ASCII typed input as previously unexercised — it worked
here), clicked `Set Emoji` → `06-fprj16c-setemoji-result.png` shows the swatch now rendering
🎉 with no validation error. Both the accept and reject paths are real.

**`Open Emoji Picker`**: on a fresh app instance (emoji field empty — confirms the picker's
choice does not persist across restarts, consistent with the unwired-`on_change`/no-durable-store
defect already logged for `F-PRJ-13` in `SEAMS.md`), clicking `Open Emoji Picker` while the field
was empty did nothing observable — no picker overlay appeared, matching a disabled/no-op state
gated on non-empty input. The picker overlay itself was not reached (button never fired), so
that specific door in the clause is unexercised.

Captures: `02-fprj16b-settings.png` … `06-fprj16c-setemoji-result.png`, `02-fprj16d-settings.png`
… `04-fprj16d-openpicker.png`.

## F-EDIT-05 — full conflict banner exercised live, both Reload and Keep

Prior evidence (P101 critic) reached only a Files context-menu frame; the conflict gesture
itself was never driven (blocked by the `:1` drive lock for 900s). This drive reached it
cleanly on the Wayland lane by adding a throwaway git-inited fixture project
(`ctl project.add path=/tmp/e09fixture`, a single `edit-target.md`) so the real repo tree
under test is never touched.

Route: `workspace.select` the fixture worktree → double-click `edit-target.md` in Files →
switch to `Code` view → click into the buffer and type, producing a genuine dirty/`edited`
tab (confirmed: `edited` badge, dot on the tab, typed text visible in the buffer,
`04-fedit05l-dirty.png`). With the tab dirty, the file was appended to **from outside the
app** (`echo … >> edit-target.md`, a real external writer — the same class of event the row
describes), then focus returned to the tab by clicking away to `Terminal` and back.

**Result: the exact banner the clause names.** `99-fedit05p-banner2.png` shows `This file
changed on disk.` with `Reload` and `Keep` controls, the dirty local text (`DIRTY-RELOAD-TESTline1`)
still shown underneath (proving this is a real conflict prompt gated on dirty state, not a
silent reload — a first pass with a *clean* tab reloaded silently with no banner at all,
`99-fedit05k-conflict.png`, which is consistent with there being nothing to protect when there
is no local edit to lose).

**Both buttons driven and verified by outcome, not just click:**
- **Keep** (`99-fedit05n-keep.png`): banner dismissed, buffer still shows the local dirty text
  `DIRTY-LOCAL-EDITline1` — the external disk change was *not* pulled in, exactly what "Keep"
  should do.
- **Reload** (`99-fedit05q-reload.png`, separate cycle, fresh dirty edit `DIRTY-RELOAD-TEST…`):
  banner dismissed, buffer now shows `line1 / line2 / external-change-line-2` — the disk version,
  local edit discarded, `edited` badge cleared. Exactly what "Reload" should do.

Mechanical note for reproducing this: `wayland-drive.sh` restarts the app on every invocation
(`kill_ours` runs unconditionally), so the "modify externally while the tab stays open" half of
this row cannot be driven inside one `wayland-drive.sh` call. It was driven by launching with
`TILLER_WL_KEEP=1`, then — between the open-with-dirty-edit step and the recheck step — sending
the external file write via plain `Bash`, and the follow-up click/resolution-flip/`grim` capture
manually against the same already-running instance's `$VP_FIFO`/`$SWAYSOCK`/`$WAYLAND_DISPLAY`
(all still valid since the instance was never killed).

Captures: `02-fedit05e-fileopen.png`, `02-fedit05j-open.png`, `99-fedit05k-conflict.png`
(clean-tab silent reload, negative control), `04-fedit05l-dirty.png`, `99-fedit05m-conflict-dirty.png`,
`99-fedit05n-keep.png`, `99-fedit05p-banner2.png`, `99-fedit05q-reload.png`.
