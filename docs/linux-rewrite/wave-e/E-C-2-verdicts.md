# Wave E slice E-C-2 — critic verdicts

## `F-CTRL-WORK-01` — PASSED

Real-restart instrument (not the builder's unit test): launched a dedicated `tiller` process
under a headless Wayland compositor with `TILLER_DB`/`TILLER_SOCKET` pointed at a scratch
DB/socket, `ctl project.add` + `ctl worktree.set worktree=<path> comment=restart-proof-comment`,
read the `worktree` table directly with Python's `sqlite3` (comment present), `kill`ed the process
outright, re-read the DB with the process dead (comment still present), then launched a **second**,
independent `tiller` process against the same DB file — a genuine cold restart that runs `main()`'s
startup `schedule_catalog` from scratch. Read the DB again immediately after the second process's
socket came up: comment still `restart-proof-comment`, not NULL. Then, with process 2 still live,
triggered a second, live `schedule_catalog` call (`ctl project.add` for a second project) and read
the DB a fourth time: comment still intact. This exercises exactly the defect described (comment
wiped on every `schedule_catalog`, including startup) through a real process boundary, and it
survived both a real restart and a live second trigger.

## `F-CTRL-BROWSER-04` — PASSED

Live drive, Wayland lane (`TILLER_WL_LABEL=ec2browser`): `ctl project.add`, `ctl browser.open
url=https://example.com`, `ctl browser.snapshot` → `{"ok":false,"error":"browser.snapshot failed:
Browser child is unavailable"}` — the real, state-dependent handler error, discriminated from the
old blanket `"browser.snapshot is unsupported on Linux: browser automation is not implemented"`
pre-rejection it used to return. `ctl browser.screenshot` in the same run still correctly returned
the old blanket unsupported message. Matches the row's stated route exactly on both counts.

## `F-CTRL-BROWSER-05` — PASSED

Live drive, X11 lane (`DISPLAY=:1`, took the shared drive lock for the run, released on exit):
launched a dedicated `tiller` instance, `ctl project.add`, `ctl browser.open
url=https://example.com`, then `ctl browser.wait timeoutMs=8000` twice. Both calls returned
`{"loading":"false","timedOut":"false","title":"Example Domain","url":"https://example.com/"}` —
`loading` genuinely flipped to `false` with a real page title, not a stuck/default value. This is
the exact half prior passes (wave-D, and this slice's own builder) could not close because the X11
lane was held by another agent the whole time; it was free this pass and the result is a clean
content-level success, discriminating this from the broken-build state (which returns "Browser
child is unavailable" and never gets a title).

## `F-CTRL-BROWSER-06` — PASSED

Same X11-lane instance and run as BROWSER-05, sequentially: `ctl browser.eval
script="document.title"` → `{"result":"\"Example Domain\""}`; `ctl browser.eval
script="document.body.innerText.slice(0,80)"` → `{"result":"\"Example Domain\\n\\nThis domain is
for use in documentation examples without needing\""}`. Real page content, not the
"Browser child is unavailable" state-dependent error this same handler returns when the WebView
child never came up (as it does under the Wayland lane) — closes the half the wave-D critic and
this slice's builder both left open for lack of a free X11 slot.

## `F-CORE-WSP-04` — FAILED — absent

Re-grepped fresh at HEAD (independent of the builder's and wave-D's identical claim):
`grep -rn "LayoutCommand\|classify_layout_command" crates/` finds callers only inside
`tiller_project/src/layout.rs` itself plus a bare re-export in `tiller_project/src/lib.rs` — zero
references anywhere in `tiller`/`tiller_ui`. `docs/linux-rewrite/tasks/P78-...md` independently
confirms this as "`LayoutCommand` has zero app callers (dead enum)". This is not a case of a real
behaviour merely hard to reach (which would be UNREACHABLE) — the enum and classifier exist as
code but are never invoked by the running app under any input, so the ledger's described behaviour
does not exist in the product. No production code changed for this row (confirmed via `git status`
and re-grep); builder's "blocked" claim holds.

## `F-CORE-WSP-08` — FAILED — absent

Same instrument as WSP-04: `grep -rn "WorkspaceTabViewState\|WorkspaceTab\b" crates/` finds
references only in `tiller_project/src/layout.rs` and the `lib.rs` re-export — zero callers in the
app. `docs/linux-rewrite/tasks/P78-...md` confirms independently: "session store never persists
`view_state`", and defines the pass bar for this row explicitly as "set state, quit, relaunch,
state is there" — a bar that cannot be met when nothing in the app ever writes or reads
`view_state` through a persistence path. Genuinely absent, not merely unreached; no production code
changed for this row.
