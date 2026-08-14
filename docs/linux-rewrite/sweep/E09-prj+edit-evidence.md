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
