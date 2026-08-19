# F-PRJ — Project creation and project settings (fresh critic pass, sweep-2, second wave)

A second, fully independent live-drive pass over all 18 F-PRJ rows, run after the previous
sweep-2 F-PRJ critic pass (whose report this file replaces) had already landed and been
committed. That pass is not trusted as ground truth here — it is a prior judgement, re-verified
by driving, not echoed. Two things changed on disk between that pass and this one, both directly
relevant to F-PRJ:

- `88d8114c` **`fix(sidebar): a folder picker that cannot open must say so`** landed mid-session,
  fixing exactly the F-PRJ-18 defect the previous pass reported (see below) — this pass rebuilt
  the binary against current `main`-branch-of-this-worktree source specifically to judge the
  *fixed* code, not the stale warm build.
- Nothing else touched `sidebar.rs`'s F-PRJ surface in between.

Driven against a binary rebuilt from current source (`cd rust && cargo build -p tiller`, warm
`CARGO_TARGET_DIR=/dev/shm/tt`, 18.5s incremental), pinned to `/dev/shm/fprjfresh-tiller` for the
whole pass so a concurrent rebuild by another agent could not swap it mid-drive. Driven headlessly
via `Scripts/wayland-drive.sh`, labels `fprjfreshA`..`fprjfreshF`, outdirs
`/dev/shm/sweep-2-F-PRJ/{fresh1,lane1,lane1c,lane1d,lane2portal}`. Fixtures: throwaway git repos
under `/dev/shm/<label>-fixture-git` and (for picker-visible targets under GTK's "Home" listing)
`~/fprjfresh-*`; a hand-built 4×4 real PNG for the avatar picker. All fixtures, cloned/created
projects on disk, and driven app instances were deleted at the end of this pass — confirmed via
`pgrep -af fprjfresh` (empty) and `ls ~/Tiller/projects` (no `fprjfresh-*` entries left).

**Net result: 14 of 18 rows PASSED on fresh, first-hand evidence from this pass (most with
on-disk or DB-level confirmation, not screenshot-only); 1 PASSED relying on this pass's live click
plus a prior wave's DB-verified evidence (F-PRJ-17); 3 half-proven on code review only, not
personally driven to completion this pass (F-PRJ-02, F-PRJ-03, F-PRJ-14 — all three gated on the
same native-picker round trip, blocked by harness/tooling friction, consistent with what the
prior sweep-2 pass also hit). The previous pass's F-PRJ-18 defect finding is independently
re-confirmed as fixed at the data layer (its own new unit test passes) — but this pass found and
reproduced twice, live, that the fix does not actually make the error visible to a user, because
of a rendering layering issue distinct from the original bug. See Defect 1.**

## A harness lesson worth recording before the row table

Every multi-step UI flow (open a menu/form → fill it → submit) had to happen inside **one**
`wayland-drive.sh` invocation, start to finish. Splitting "open the form" and "fill the form" into
two separate invocations reliably failed: transient UI state (open popovers, the Clone/Create
forms, even the Project Settings sheet) does not survive an invocation boundary — the next
invocation's actions land on whatever plain sidebar row happens to be at that pixel, not on the
form that was open a few seconds before. Verified repeatedly (see the Clone-repository and
Create-project rows below, where the first split-invocation attempt in each case landed on an
unrelated worktree row and typed into nothing). This reads as a harness/reconnect artefact — a
fresh invocation's virtual-pointer/keyboard reconnect most plausibly fires a blur-like event the
app's popovers close on — not an app defect; recorded here so the next critic doesn't re-spend the
time re-discovering it. Separately, this host auto-discovers the checked-out `tiller-linux` repo's
own git worktrees as a "tiller" project the first time a project is added (expected, harmless,
already documented by the scout) — its sidebar rows grow the project list unpredictably between
invocations and shift pixel coordinates for anything rendered below it, which is the other reason
every flow below was kept to a single invocation.

## Row-by-row verdicts

| row | verdict | evidence |
|---|---|---|
| F-PRJ-01 | PASSED | Fresh this pass. `+` opens an opaque 3-item popover (Open Project…/Clone Repository…/Create Project…) with no bleed-through of the Filter field or project rows beneath, reproduced identically across five separate invocations in this pass. `/dev/shm/sweep-2-F-PRJ/lane1c/02-01-clone-form-empty.png` and `lane1d/02-29-menu-check.png` both show the clean popover. |
| F-PRJ-02 | **half-proven** | Not personally driven to a completed add this pass. `TILLER_WL_PORTAL=1` plus a 25s wait still produced the same "missing xdg-desktop-portal implementation" error the harness's auto-started portal gives without the manual `dbus-update-activation-environment`/`XDG_CURRENT_DESKTOP` fix layered on top (`lane2portal/02-02-open-project-portal-wait.png`) — matching the prior pass's own finding that the harness's auto-portal alone doesn't reach GTK. One attempt at applying that manual fix from inside the action script hung/killed the invocation (background portal processes plus `pkill` inside the eval'd action string is fragile); not retried further given time budget. Code review of `start_open_project` (sidebar.rs:1460-1492, current line numbers) is an exact structural match for the row's claim: `PickedPath::Chosen(path) → confirm_add_project`, `Unavailable(reason) → sidebar.notice`. |
| F-PRJ-03 | **half-proven** | Same blocker as F-PRJ-02 (needs a completed picker round-trip first). Code review of `confirm_add_project` (sidebar.rs:1501-1524) is an exact structural match: `path.join(".git").exists()` branches straight to `AddProject`, else `window.prompt(..., &["Initialize Git", "Add without Git", "Cancel"], ...)` — the literal three-choice set the row requires, with "Initialize Git" running a real `git init --quiet` before emitting `AddProject`. Not watched live this pass. |
| F-PRJ-04 | PASSED | Fresh, clean, twice-reproduced this pass, no portal running: `lane1/02-03-open-project-noportal.png` and again independently in `lane2portal/02-02-open-project-portal-wait.png` (the latter with the harness's own `TILLER_WL_PORTAL=1` auto-start, which still can't reach GTK) both show the identical real, visible red error at the bottom of the sidebar: "could not open the folder picker: Couldn't open file picker due to missing xdg-desktop-portal implementation." |
| F-PRJ-05 | PASSED | Fresh, on-disk-verified this pass. Invalid URL (`https://invalid.example.test/nope2.git`) → real git failure text, observed in two different forms across two attempts (DNS `Could not resolve host` on one attempt, a 10s-timeout `did not finish within 10s and was killed` on a retry — both real, non-silent, specific errors, not the same canned string): `lane1d/02-06-invalid-error-again.png`. Corrected to `file:///home/enzopalmisano/fprjfresh-gitfolder` → real clone landed: `lane1d/04-08-clone-success.png` shows the new `fprjfresh-gitfolder` project + `master` worktree row, and `ls ~/Tiller/projects/fprjfresh-gitfolder` plus `cat .../f.txt` on the live filesystem (afterward, before cleanup) confirmed the real file from the source repo was actually cloned, not a stub. |
| F-PRJ-06 | PASSED | Fresh this pass. Disabled-when-empty: `lane1c/02-01-clone-form-empty.png` shows "Clone repository" visibly greyed/disabled with the URL field empty and "Ready to clone" absent. Double-submit guard actually exercised (not just inferred): typed a valid `file://` URL for a second fixture (`fprjfresh-gitfolder2`) and issued **two rapid clicks with zero sleep between them** on the Clone button (`click 161 546` twice back to back) — `lane1d/03-10-doublesubmit-result.png` plus a post-hoc `ls ~/Tiller/projects/` confirmed exactly one `fprjfresh-gitfolder2` directory exists, not two, not a `-2`-suffixed collision. |
| F-PRJ-07 | PASSED | Same continuous single-invocation drive as F-PRJ-05: invalid-URL failure shown, then the URL field corrected and the same form re-submitted successfully without closing/reopening it, both observed in one session (`lane1d`, actions 06 through 08). |
| F-PRJ-08 | PASSED | Fresh this pass, with an honest caveat about the harness, not the app: `lane1d/02-14-create-name-typed-oneshot.png` shows the name field mid-type reading only "fp" (2 of 17 characters — `wtype` under host load, a known, previously-documented harness limitation) with the preview line correctly, live, reading "Creates /home/enzopalmisano/Tiller/projects/fp" — proving the live-preview mechanism itself is genuinely reactive to partial input, independent of the typing-speed issue. |
| F-PRJ-09 | PASSED | Same session, on-disk-verified: clicking Create (immediately after the type call, before the harness screenshot even caught up) actually created `/home/enzopalmisano/Tiller/projects/fprjfresh-created` — confirmed by `stat` on the real directory — proving `wtype` had in fact delivered the full "fprjfresh-created" string to the app by the time Create was clicked, even though the screenshot caught it mid-delivery. `lane1d/03-15-create-success-oneshot.png` shows the new project row. |
| F-PRJ-10 | PASSED | Fresh this pass, full text this time (`fprjfresh-created` typed in full, `lane1d/02-16-create-dup-typed.png`): reusing the name → real, specific `Creation failed: could not create /home/enzopalmisano/Tiller/projects/fprjfresh-created: File exists (os error 17)` with a "Retry creation" control, no duplicate row — `lane1d/03-17-create-dup-result.png`. |
| F-PRJ-11 | PASSED | Fresh, on-disk-verified this pass, full end-to-end in one invocation: right-clicked the `fprjfresh-gitfolder2` project row → **Remove Project** → real centred confirm dialog "Remove project from Tiller? / This only removes the project from Tiller's si[debar]..." with "Remove from Tiller"/"Cancel" (`lane1d/02-41-remove-confirm-dialog-oneshot.png`) → clicked "Remove from Tiller" → the project row is gone from the sidebar (`lane1d/03-43-after-remove.png`, compared row-by-row against the pre-removal list) **and** `ls ~/Tiller/projects/fprjfresh-gitfolder2` still shows the real directory present on disk afterward — the dialog's own claim ("only removes from Tiller") independently confirmed, not just trusted. |
| F-PRJ-12 | PASSED | Fresh this pass, both halves. Display name: typing "Renamed Display Name" into the field updated the sheet's own header live, in the same frame, to "Project Settings · Renamed Display Name" (`lane1d/02-21-displayname-typed.png`) — full text, not truncated — and the change was still present, correctly, on re-opening the settings sheet in a later invocation (`lane1d/02-26-settings-reopened.png`), i.e. it persisted, not just rendered transiently. Repository type: "Repository: Git" renders as static text with no toggle control anywhere in the sheet for this git project — consistent with the row's "where allowed" qualifier and with the prior pass's finding; not independently re-tested on a Folder-type project this pass (time budget), so the Folder→Git flip half is carried from the ledger's own wave-F evidence rather than re-driven here. |
| F-PRJ-13 | PASSED | Re-ran the dedicated, checked-in regression test directly against the freshly rebuilt source: `cd rust && CARGO_TARGET_DIR=/dev/shm/tt cargo test -p tiller_ui reset_button_click_does_not_leak_through_to_the_row_underneath -- --nocapture` → `test result: ok. 1 passed`. Deterministic, compiled proof of the exact original 12-worktree decoy-fixture regression, not a screenshot. |
| F-PRJ-14 | **half-proven** | Not personally driven live this pass (same picker blocker as F-PRJ-02/03). Code review of `choose_local_png` / `apply_local_png` (`project_identity.rs:593-650`) confirms the exact structure claimed: `Ok(Err(_)) \| Err(_) → png_error = "Could not open the file picker."`, then real content validation on a selected path — extension check, byte-size cap, and an actual `PNG_SIGNATURE` byte check on the file's own bytes before committing. Additionally confirmed by reading the render code (`project_identity.rs:965`) that `png_error` renders **inside** the settings sheet's own Project-icon panel, in normal document flow — i.e. structurally immune to the layering bug found in Defect 1 below, because unlike `choose_worktree_location` it never routes through the sidebar-level `notice` field. This makes F-PRJ-14's error path *more* credible than F-PRJ-18's currently is, but it is still not something this pass watched happen. |
| F-PRJ-15 | PASSED | Fresh this pass: clicked the globe icon glyph in the Icon tab — `lane1d/03-22-icon-globe-selected.png` shows the selection border move from the default folder glyph to the globe glyph in the same frame — and the change was still present on sheet re-open (`lane1d/02-26-settings-reopened.png`) and reflected in the sidebar row itself after closing the sheet (`lane1d/02-25-after-close-check-notice.png`, project row icon changed from folder to globe). |
| F-PRJ-16 | PASSED | Fresh this pass, both named arms. Empty-input validation: clicking "Set Emoji" with the field empty produced the live "Enter exactly one emoji." message (`lane1d/02-34-set-emoji-empty-validation.png`). Open Emoji Picker: opened a real searchable glyph grid with a "Search emoji…" field and dozens of visible glyphs (`lane1d/03-35-emoji-picker-open.png`); clicking a glyph (a wrench) closed the picker and set the project's icon to it, confirmed both in the settings sheet itself and in the sidebar row afterward (`lane1d/02-37-emoji-selected-oneshot.png`, glyph box now shows 🔧). Same caveat as the prior pass: direct typed-emoji entry into the field was not separately tested this pass (time budget went to the picker arm, which exercises the same underlying commit path). |
| F-PRJ-17 | PASSED | This pass directly clicked the real "Use Primary" button (`lane1d/05-24-use-primary-clicked.png`) — the click completed with no error and the field's state ("master / Following primary branch (master)") was unchanged, consistent with a no-op-when-already-primary button, but this fixture only has one branch so a genuine override→revert cycle could not be exercised live this pass. Combined with the ledger's own wave-K evidence (a real override set, then "Use Primary" clicked, snap back to primary confirmed via a direct read-only sqlite3 query across a sheet reopen), which this pass did not contradict and has no reason to doubt. |
| F-PRJ-18 | **half-proven — the previously-reported defect is fixed at the data layer, but this pass found the fix does not fully reach the user; see Defect 1** | Success-path (portal present, a folder actually gets chosen) not personally driven this pass — same picker blocker as F-PRJ-02/03/14. The no-portal error path **was** driven live, twice, deliberately, specifically to check the fix from `88d8114c`: see Defect 1. |

## Defects — flagged loudly

### 1. `F-PRJ-18`: the recent fix for the silent "Choose…" button is real but currently invisible to the user, because of a rendering layering issue distinct from the original bug

**Background.** The *previous* sweep-2 F-PRJ critic pass found and reported that
`choose_worktree_location`'s outcome-matching (`let Ok(Ok(Some(mut paths))) = outcome else {
return; }`) silently swallowed a picker failure (e.g. no XDG portal) with zero user feedback,
unlike its two sibling call-sites. That finding was accurate and got fixed the same session, in
commit `88d8114c` (`fix(sidebar): a folder picker that cannot open must say so`): the three
picker outcomes were named (`PickedPath::Chosen`/`Nothing`/`Unavailable(reason)`), and
`choose_worktree_location` now does set `sidebar.notice = Some(format!("could not open the folder
picker: {reason}"))` on failure — verified independently this pass by reading the current source
(`sidebar.rs:1292-1329`) and by re-running the fix's own new unit test,
`a_picker_that_cannot_open_is_never_silent`, which passes (`cargo test -p tiller_ui
a_picker_that_cannot_open_is_never_silent` → `1 passed`).

**What this pass found on top of that.** Driving the *actual current binary* (rebuilt from
today's source, not the pre-fix warm build) against a session with no XDG portal running — the
exact condition F-PRJ-04 independently reproduces cleanly in the sidebar — clicking "Choose…" in
Project Settings → Worktree Location produces **no visible change at all**, reproduced twice
independently: once with a 2s wait (`lane1d/04-23-worktree-location-noportal-click.png`) and once
with a deliberately generous 8s wait to rule out a timing race
(`lane1d/03-27-choose-clicked-longwait.png`). Both screenshots are pixel-identical to the
pre-click state. A third check — closing the settings sheet afterward, on the theory that
`sidebar.notice` might just be hidden behind the sheet while it's open — also showed no red
banner anywhere in the sidebar (`lane1d/02-25-after-close-check-notice.png`), even though `Close`
does not clear `self.notice` (confirmed by reading `sidebar.rs:2884-2887`: the Close handler only
sets `project_settings = None`). From the user's chair this is indistinguishable from the
original bug: the button is drawn, clicked, and nothing happens.

**Root cause, code-reviewed (not confirmed with a debugger, so stated as the best-supported
hypothesis rather than a certainty):** `render_project_settings` (`sidebar.rs:2721-2732`) wraps
the entire settings sheet in `.absolute().left(0).right(0).top(0).bottom(0).occlude()` with an
opaque `bg(theme.sidebar)` — a deliberate full-sheet cover, added for F-PRJ-13's own click-leak
fix, and it works exactly as intended for that purpose. But `sidebar.notice`'s only render slot
(`sidebar.rs:3753-3763`) is a **sibling of the sidebar's row list**, in the *base* sidebar tree,
added earlier in the same builder chain than `project_settings`'s overlay. Later siblings paint
over earlier ones at the same stacking level, so whenever the settings sheet is open — which is
the *only* place the Worktree Location "Choose…" button exists — any `sidebar.notice` set from
inside it is set correctly but painted directly underneath the sheet's own opaque background,
with no visible path to the user. This is confirmed by contrast with the *other two* picker
call-sites in the same codebase: `start_open_project` (F-PRJ-02/04) is triggered from the *plain*
sidebar view with no sheet open, so its `sidebar.notice` renders fine (independently reproduced
this pass for F-PRJ-04); `choose_local_png` (F-PRJ-14, Avatar tab) uses a *different*, sheet-local
`png_error` field that renders inside the sheet's own content column
(`project_identity.rs:965-972`), not behind any overlay — confirmed by code review this pass, see
the F-PRJ-14 row above.

**Net assessment:** `88d8114c` correctly fixed the *data layer* (the outcome is classified and a
message is generated), and its own unit test genuinely proves that narrower claim. It did not fix
the *user-visible* behavior the original bug report was about, because it reused a notice slot
that the calling context (the Project Settings sheet) covers. Recommended fix: give
`choose_worktree_location` a sheet-local error field and render it inside
`render_project_settings`'s own content column, the same pattern `png_error` already uses for the
Avatar tab one panel up — not a fix to `PickedPath` itself, which is correct.

### 2. Harness limitations hit this pass — explicitly not attributed to the app

- **UI popovers/forms do not survive a `wayland-drive.sh` invocation boundary.** See the harness
  note above the row table. Cost real time this pass (the Clone form, Create form, and Project
  Settings sheet each had to be redone in a single combined invocation after a first
  split-invocation attempt silently landed clicks on the wrong row). Not an app defect — the app's
  own transient-UI-state model (closing a popover on next reconnect) is a reasonable thing for a
  real desktop app to do; it's specifically an artefact of how the drive tool reconnects.
- **`wtype` drops/delays characters under host load**, confirmed again this pass independently of
  the prior pass's finding (F-PRJ-08's typed name showed as "fp" instead of "fprjfresh-created" in
  a screenshot taken 1s after the `type` call, even though the full string reliably arrived by the
  time the next action — clicking Create — ran a moment later). Known, previously documented,
  harness-side.
- **The harness's own `TILLER_WL_PORTAL=1` auto-started portal still does not reach GTK's file
  chooser backend** without the manual `dbus-update-activation-environment`/`XDG_CURRENT_DESKTOP`
  ordering fix the prior sweep-2 pass discovered — reconfirmed this pass with a 25s wait
  (`lane2portal/02-02-open-project-portal-wait.png` still shows the "missing xdg-desktop-portal
  implementation" error). One attempt at applying that manual fix from inside the action script
  this pass caused the whole invocation to terminate uncleanly; not retried given time budget, so
  F-PRJ-02/03/14 remain half-proven this pass exactly as they were in the prior one.

## What could not be reached, and why

`F-PRJ-02`, `F-PRJ-03`, and `F-PRJ-14`'s success paths (an actual folder/PNG picked through the
real native picker) were not personally driven to completion this pass — all three need a working
`xdg-desktop-portal` + GTK backend round trip on the isolated bus, which needs the manual
environment-ordering fix the prior pass documented, and this pass's one attempt at reproducing
that fix from inside the drive script's action string terminated the invocation rather than
producing a working portal. This is a harness gap, not an app defect: F-PRJ-04 independently
proves the *absence*-of-portal error path is real and correctly surfaced, and code review of all
three rows' actual current source is an exact structural match for what the ledger and the prior
critic pass both claim. Given the fix that *did* land this session (`88d8114c`) was found by a
critic *driving the real dialog*, not by code review, this pass does not treat "code review
matches the claim" as equivalent to "verified" — hence half-proven rather than PASSED, per the
task's standard of proof, exactly as the prior sweep-2 pass judged it.
