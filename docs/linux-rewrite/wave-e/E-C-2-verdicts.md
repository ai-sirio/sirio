# Wave E slice E-C-2 — critic verdicts

**Note on provenance:** this file already contained a full set of verdicts (commit `c699ac3`,
2026-08-15 21:43, predating this dispatch) claiming all six rows PASSED including a clean
content-level success on `F-CTRL-BROWSER-05`/`06`. That commit never reached the orchestrator's
ledger (E-C-2.md still showed the rows in their pre-critique state at dispatch time), so this is
an independent re-run, not a continuation. Where my own live re-check reproduces its result I say
so; where it does not (BROWSER-05/06) I report my own evidence and flag the conflict rather than
inheriting the earlier file's word.

## `F-CTRL-WORK-01` — PASSED

Real-restart instrument, own scratch DB (`/tmp/ec2work-shared.sqlite`, separate from the prior
file's paths): via `wayland-drive.sh` with a fixed `TILLER_WL_LABEL` (same label => same
`/tmp/<label>.sqlite`), boot 1: `project.add` + `worktree.set worktree=/tmp/ec2work01/repo2
comment=survives-restart`, confirmed via `workspace.list` (comment present), process killed by the
script's own teardown. Boot 2: a genuinely separate `tiller` process launched against the same DB
file with no `project.add` this time — the project/worktree and its comment came back purely from
`main()`'s own startup restore (the exact `schedule_catalog`-then-restore-comments sequence the
defect was in). `workspace.list` on boot 2 showed `"comment":"survives-restart"`, `"selected":"true"`
(auto-restored). Source-read confirms the fix (`write_catalog` in `session.rs`) sits correctly
before the restore call at `main.rs:9466`/`9488-9497`.

## `F-CTRL-BROWSER-04` — PASSED

Live drive, Wayland lane (`TILLER_WL_LABEL=ec2crit02`): `project.add`, `browser.open
url=https://example.com`, then `browser.snapshot` → `{"ok":false,"error":"browser.snapshot failed:
Browser child is unavailable"}` — the real, state-dependent handler error, discriminated from the
old blanket `"browser.snapshot is unsupported on Linux: browser automation is not implemented"`
pre-rejection (`browser.screenshot` in the same run still returns exactly that blanket string,
confirming the discrimination is real and not just absence of any error). Matches the row's own
stated acceptance criterion exactly (JSON payload or the real unavailable error, never the blanket
rejection).

## `F-CTRL-BROWSER-05` — half-proven (contradicts a prior file's PASSED claim — see above)

Live drive, X11 lane (`DISPLAY=:1`), took the shared `linux-drive.sh` drive lock myself for two
separate runs (fresh `tiller` process each time, own scratch DB/socket, released after). Both runs:
`project.add` + `workspace.select` + `browser.open url=https://example.com`, then repeated
`browser.wait timeoutMs=8000`/`10000`. Neither run ever observed `loading` flip to `false`: a
non-blocking `browser.get` (no GTK pump, no timeout confound) read `loading:"true"`, `title:""`,
`error:""` immediately after open and again 15s later — genuinely stuck, not a slow fetch (`curl
https://example.com` from the same host returns `200` instantly, so outbound network is not the
blocker). `browser.eval`/`browser.snapshot` on the same live surface returned `"Browser child is
unavailable"`. Separately, found and confirmed a real but distinct defect while investigating:
`browser.wait`'s handler blocks the GPUI main thread for its full requested `timeoutMs`, but the
control-dispatch wrapper (`CONTROL_ACTION_TIMEOUT`, `main.rs:196`) hard-times-out replies at a fixed
5s — any `timeoutMs` above ~5000 (both my run and the prior file's `timeoutMs=8000` request) races
its own transport and can surface as `"control action timed out"` even on requests that would
otherwise have finished; this masks the real result but is not itself proof of the underlying
WebView failure — the fast, non-blocking `browser.get` stuck-`loading` reading is the clean evidence.
I could not reproduce the prior file's clean `loading:false`/`title:"Example Domain"` result despite
two independent fresh-process attempts on the same lane; recorded as `half-proven`, matching the
builder's and wave-D's own conclusion, not the intervening file's PASSED.

## `F-CTRL-BROWSER-06` — half-proven (same conflict as BROWSER-05)

Same two X11-lane runs as BROWSER-05: `browser.eval script="document.title"` consistently returned
`"Browser child is unavailable"` (first, fast attempt, no timeout confound) or `"control action
timed out"` (later attempts, same masking issue described above) — never real page content. No
reproduction of the prior file's claimed `"Example Domain"` result. Structural Wayland/X11
WebView-construction limitation stands unresolved in this environment as tested; `half-proven`.

## `F-CORE-WSP-04` — FAILED — absent

Re-grepped fresh at HEAD: `grep -rn "LayoutCommand|classify_layout_command" crates/` (from
`rust/`) finds callers only inside `tiller_project/src/layout.rs` itself plus a bare re-export in
`tiller_project/src/lib.rs` — zero references in `tiller`/`tiller_ui`. Independently confirmed by
`docs/linux-rewrite/tasks/P78-the-seven-functions-the-app-never-calls.md`: "`LayoutCommand` has
zero app callers (dead enum)". Not merely hard to reach — no path in the running app produces or
consumes it under any input, so the ledger's described behaviour does not exist in the product. No
production code changed for this row (`git status --porcelain` clean on the owned files); builder's
"blocked" claim holds.

## `F-CORE-WSP-08` — FAILED — absent

Same instrument as WSP-04: `grep -rn "WorkspaceTabViewState|WorkspaceTab\b" crates/` finds
references only in `layout.rs` and the `lib.rs` re-export. `P78-...md` confirms independently:
"session store never persists `view_state`", with the row's own pass bar defined there as "set
state, quit, relaunch, state is there" — unreachable while nothing in the app ever writes or reads
`view_state` through a persistence path. Genuinely absent, not merely unreached; no production code
changed for this row.
