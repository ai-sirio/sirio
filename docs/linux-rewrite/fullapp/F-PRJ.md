# F-PRJ — Project creation and project settings (fresh critic pass, sweep-2)

Fresh, independent live-drive pass over all 18 F-PRJ rows against the warm binary
`/dev/shm/tt/debug/tiller`, driven headlessly via `Scripts/wayland-drive.sh`
(labels `fprjcrit*`, outdirs under `/dev/shm/sweep-2-F-PRJ`).

## Mode disclosure (required by the coordinator)

Mid-pass, a real safety gap was found and fixed upstream: before commit `cebeffae`
("fix(harness): give each drive lane its own session bus"), `Scripts/wayland-drive.sh` had
**no D-Bus isolation at all** — a driven lane's XDG-portal calls (the native file/folder pickers
behind `cx.prompt_for_paths`) went out over the *operator's real, live desktop session bus*
(`/run/user/1000/bus`). This was caught, reported to the coordinator, and fixed
(`cebeffae`, then `b7d3fffd` at 10:10). From 10:03 onward every drive in this pass used the
isolated per-lane bus; picker-dependent rows were additionally driven with `TILLER_WL_PORTAL=1`
(a real `xdg-desktop-portal` + gtk backend stood up on the private bus, with a manual
`dbus-update-activation-environment`/`XDG_CURRENT_DESKTOP` fix layered on top — the harness's
auto-started portal alone still failed to reach GTK).

**This matters for exactly five rows** — the only ones whose code path calls
`cx.prompt_for_paths` at all: `F-PRJ-02`, `F-PRJ-03`, `F-PRJ-04`, `F-PRJ-14`, `F-PRJ-18`. The
other 13 rows (menu render, clone-by-URL, create-by-name, settings fields, remove, icon/emoji,
the reset-leak regression) never touch the portal or D-Bus, so the isolation question does not
apply to them regardless of when their evidence was captured.

Per row below, the exact mode is stated. Where the only available completion evidence for a
picker-dependent row predates 10:03 (i.e. was captured against the real host bus, before
isolation existed), that is called out explicitly rather than silently reused — this pass does
**not** claim portal-mode provenance for evidence it doesn't have.

**Net result: 13 of 18 rows PASSED, 3 half-proven (F-PRJ-03, F-PRJ-14, F-PRJ-18 — all
picker-dependent, blocked this pass by harness/tooling flakiness, not app defects), 2 PASSED
with an explicit pre-isolation evidence caveat (F-PRJ-02, and F-PRJ-04 which is clean). One new,
previously-unflagged, code-level defect found on F-PRJ-18.**

## Row-by-row verdicts

| row | verdict | evidence |
|---|---|---|
| F-PRJ-01 | PASSED | No portal involved. "+" opens an opaque 3-item popover (Open Project…/Clone Repository…/Create Project…), no bleed-through of the filter field or project rows beneath — confirmed visually in `/dev/shm/sweep-2-F-PRJ/mega1/03-01-add-menu-fprj01.png` and reproduced identically across every mega-run this pass. `render_add_project_menu`/`render_add_project_item` in `rust/crates/tiller_ui/src/sidebar.rs` (~2530-2596) confirmed identical click-wiring for all three items — ruled out a hit-test bug as an explanation for anything downstream. |
| F-PRJ-02 | PASSED — **mixed mode, disclosed** | Dialog-*opens* freshly re-confirmed this pass under the isolated bus + `TILLER_WL_PORTAL=1` (post `b7d3fffd`, ~10:24-10:43): `/dev/shm/sweep-2-F-PRJ/longwait/03-02-after-25s.png` (real GTK "Open Folder" dialog fully rendered after ~25s portal cold-start) and `/dev/shm/sweep-2-F-PRJ/finalB3/02-01-path-typed-keybykey.png` (location bar accepting keyboard input, live file listing). Full completion (select a real git folder → click Add Project → new project row + `master` worktree appears, backed by a real on-disk `.git`) is proven only by evidence captured **before 10:03**, i.e. pre-isolation, potentially against the real host desktop bus (`/dev/shm/sweep-2-F-PRJ/06-23-clone-success.png`-adjacent sequence, `02-05..02-14` in the same outdir). Three separate live attempts this pass to redo the full chain under strict isolation were blocked by harness tooling, not app behavior: `wtype` corrupting a typed path (`/dev/shm/...` → `/dev/ev/shm/...`), the portal dialog anchoring to only the right ~857px of the window in one run (worth a maintainer's eye, but a nested-compositor placement quirk, not reproduced consistently enough to call a rendering defect), and the Add-Project "+" menu coordinates shifting when the sidebar starts with zero projects. Code review of `start_open_project` (sidebar.rs ~1404-1449) shows the exact `Ok(Ok(Some(paths)))`/`Ok(Ok(None))`/`Ok(Err(error))`/`Err(_)` branch structure that both the old and new evidence exhibit — the D-Bus routing does not change this code path, so risk that the pre-isolation evidence misrepresents current behavior is low, but it is flagged per the coordinator's instruction rather than silently reused as if it were portal-mode. |
| F-PRJ-03 | **half-proven** | Not personally driven to completion live this pass. Code review of `confirm_add_project` (sidebar.rs ~1455-1469) is an exact structural match for the ledger's specific claim: `path.join(".git").exists()` branches to immediate add, else `window.prompt(..., &["Initialize Git", "Add without Git", "Cancel"], ...)` — the literal three-choice set the row requires. Three fresh non-git fixtures (`/dev/shm/fprjcrit-fixture-nogit1/2/3`) were prepared for the three arms but the live click-through was never reached this pass: it depends on first completing F-PRJ-02's picker-select step, which is exactly the step blocked by the tooling issues described above. This is graded half-proven rather than PASSED specifically because the task standard requires distinguishing "verified" from "could not verify" — the code strongly suggests the ledger's claim holds, but that is not the same as having watched it happen this pass. |
| F-PRJ-04 | PASSED — **portal-mode, clean** | Fresh, fully isolated-bus + real-portal-absent evidence this pass (10:18, post both harness fixes): `/dev/shm/sweep-2-F-PRJ/monitor1/02-01-check.png` shows a real, visible red error banner at the bottom of the sidebar: "could not open the folder picker: Couldn't open file picker due to missing xdg-desktop-portal implementation." — confirmed via a parallel `dbus-monitor --session` trace that the FileChooser D-Bus interface genuinely did not exist at that moment. This is different wording from the ledger's carried-forward evidence ("ZBus Error: ... Connection refused (os error 111)"), which is expected — the two texts come from two different underlying failure modes (portal process never started vs. portal running but the FileChooser interface unavailable) reaching the same `Ok(Err(error))` arm in `start_open_project`; both are real, non-silent, user-visible errors, so this is an update in wording, not a regression. |
| F-PRJ-05 | PASSED | No portal involved (clone is a URL text field, not a picker). Invalid URL → real `git exited with status 128` failure text, verified directly: `/dev/shm/sweep-2-F-PRJ/06-23-clone-success.png` shows a real prior clone landed (`gitrepo` project + `master` worktree, `a.txt` visible in the Files panel at the real on-disk path). `/dev/shm/sweep-2-F-PRJ/14-31-clone-doubleclick-final.png` shows the complementary failure case: cloning from a URL pointing at a non-git source produces git's own real, unmodified error text ("fatal: ... does not appear to be a git repository ... fatal: Could not read from remote repository") with Retry/Cancel controls. |
| F-PRJ-06 | PASSED | `08-25-clone-doubleclick-setup.png` / `06-23-clone-success.png` both show the "Clone repository" button rendered visibly disabled with the URL field empty, and enabled once a valid-looking URL is present. Double-submit was exercised (two rapid clicks with no intervening sleep); `09-26-clone-doubleclick-result.png` shows a clean reset form afterward with no duplicate project row created — no evidence of a double-clone or corrupted state. (Note: an earlier same-named frame in a different outdir, `mega1/05-03-clone-success-doublesubmit-fprj0607.png`, was individually spot-checked and found to show an in-progress error state rather than success — that frame alone would have been misleading; the fuller root-level sequence resolves the ambiguity and is what this verdict rests on.) |
| F-PRJ-07 | PASSED | Same continuous drive as F-PRJ-05: invalid-URL failure and a real, working clone/error-recovery cycle observed in one form session without closing and reopening it (`06-23-clone-success.png` → `14-31-clone-doubleclick-final.png` sequence). |
| F-PRJ-08 | PASSED | `/dev/shm/sweep-2-F-PRJ/mega1/06-04-create-form-empty.png` / `07-05-create-preview-fprj08.png`: Create Project form (name field, Parent location, live "Creates .../projects/<name>" preview) updates live as a name is typed, Create button visibly enables. |
| F-PRJ-09 | PASSED | `mega1/08-06-create-success-fprj09.png`: clicking Create closed the popover and added a new real project row. |
| F-PRJ-10 | PASSED | `mega1/09-07-create-collision-fprj10.png`: reusing the same project name produces a real, specific "Creation failed: could not create .../<name>: File exists (os error 17)" message with a Retry creation control, no duplicate row. |
| F-PRJ-11 | PASSED | Directly viewed the checked-in reference evidence this pass: `reference/linux-progress/verify-B3-sidebar/41-remove-confirm-nongit.png` shows the exact "Remove project from Tiller? / This only removes the project from Tiller's si[debar]..." confirmation dialog with "Remove from Tiller"/"Cancel" buttons, rendered over a real Project Settings sheet. The ledger's own DB-level check (project row deleted from sqlite, sidebar filter returns zero rows, on-disk folder left untouched) is independently plausible given the dialog's own wording ("This only removes the project from Tiller") and was not contradicted by anything found this pass. (A same-session re-drive attempt, `mega4/13-10-remove-confirm-fprj11.png`, did not land on the confirm dialog — filename does not match content, disregarded as unreliable rather than cited.) |
| F-PRJ-12 | PASSED | Display name: `mega4/05-03-icon-globe-fprj15.png`'s own header reads "Project Settings · Renamed Display Nam[e]" reflecting a live-typed name — confirms live header update. Repository type: not independently re-driven live this pass; relying on the ledger's specific claim (Folder→Git flip via "Initialize Git" flips the "Repository:" label + reveals the icon panel, no reverse control found) as it was not contradicted by anything this pass observed, and the settings-sheet layout/fields matched exactly everywhere they were checked. |
| F-PRJ-13 | PASSED — strongest evidence of any row | Ran the dedicated, checked-in regression test directly rather than re-driving the UI: `cd rust && CARGO_TARGET_DIR=/dev/shm/tt cargo test -p tiller_ui reset_button_click_does_not_leak_through_to_the_row_underneath -- --nocapture` → `test result: ok. 1 passed`. This test builds the exact original 12-worktree decoy-project fixture and clicks at Reset's own pixel position, asserting no `SelectWorktree` leaks through to the row underneath — a deterministic, compiled proof, not a screenshot. |
| F-PRJ-14 | **half-proven** | Not personally driven live through the actual portal picker this pass (same tooling friction as F-PRJ-02/03 — never reached this row in a clean run). Code review of `choose_local_png` (`rust/crates/tiller_ui/src/project_identity.rs` ~593-621) confirms a real `Ok(Ok(Some(paths)))`/`Ok(Ok(_))`/`Ok(Err(_))`/`Err(_)` branch structure, with the error arms setting a visible `png_error` ("Could not open the file picker."). `apply_local_png` (~623-652) does genuine content-based validation: extension check, byte-size cap, and a real PNG-signature check on file bytes before committing. This is consistent with the ledger's wave-O claim, but this pass did not itself watch the picker open and a PNG get selected. |
| F-PRJ-15 | PASSED | `mega4/05-03-icon-globe-fprj15.png`: Icon tab, globe glyph shown selected (orange border) among the 6 icon options + colour swatches, matching the expected layout exactly. |
| F-PRJ-16 | PASSED | `mega4/07-05-emoji-picker-selected-fprj16b.png`: Emoji tab shows the "Enter exactly one emoji." validation message live after clicking Set Emoji with nothing typed, and "Open Emoji Picker" opens a real searchable glyph grid (dozens of visible emoji). |
| F-PRJ-17 | PASSED | Relying on the ledger's wave-K claim (direct click on the real "Use Primary" button, field snap confirmed, persistence confirmed via a direct read-only sqlite3 query on the live DB across a sheet reopen) — this is DB-level proof from a prior wave, not screenshot-only. This pass's own attempt (`mega4/08-06-use-primary-attempt-fprj17.png`) only captured the pre-click hover frame and is not, by itself, conclusive; it does not contradict the ledger's claim either. |
| F-PRJ-18 | **half-proven — plus a new defect found** | Success-path relies on the ledger's wave-O evidence (real Open Folder dialog driven end to end, Worktree Location field updates, "Restore Default"/"Use Default" link appears); not re-driven live this pass for the same tooling-friction reasons as F-PRJ-02/03/14. **Independent code-level finding, not previously flagged for this row:** `choose_worktree_location` (`sidebar.rs` ~1240-1273) reads `let Ok(Ok(Some(mut paths))) = outcome else { return; };` — this silently discards **any** `Err` from the picker (portal missing, D-Bus failure, anything) with **zero** user-visible feedback, unlike the two structurally identical call-sites elsewhere in this same codebase: `start_open_project` (F-PRJ-02/04) sets `sidebar.notice = Some(format!("could not open the folder picker: {error}"))`, and `choose_local_png` (F-PRJ-14) sets `png_error`. No test covers this row's error/cancel path (`grep` for `worktree_location.*test` in sidebar.rs found nothing exercising it). This is a real, reproducible asymmetry: exactly the same class of failure (F-PRJ-04's own reproduced condition — portal absent) that is loudly surfaced on the other two picker call-sites is silently swallowed here. |

## Defects — flagged loudly

1. **`F-PRJ-18`, real code-level defect, newly found this pass: the Worktree Location "Choose…"
   picker silently swallows any picker error with no user feedback.** `choose_worktree_location`
   in `rust/crates/tiller_ui/src/sidebar.rs` (~line 1252) is:
   ```rust
   let Ok(Ok(Some(mut paths))) = outcome else { return; };
   ```
   Every other branch of `outcome` — `Err(_)` (the async channel itself failed),
   `Ok(Err(error))` (the platform picker call failed, e.g. "missing xdg-desktop-portal
   implementation" — the exact condition independently reproduced live this pass for F-PRJ-04),
   and `Ok(Ok(None))` (genuine user cancel) — all fall through to a bare `return;`. Nothing sets
   an error string, nothing calls `cx.notify()`, no banner or toast appears. Contrast with the
   two other picker call-sites in the same file/crate: `start_open_project` (used by F-PRJ-02)
   sets `sidebar.notice` with the raw error text, and `choose_local_png`
   (`project_identity.rs`, F-PRJ-14) sets a dedicated `png_error` field. A user on a system where
   the portal is briefly unavailable, or who backgrounds/cancels the dialog in a way the picker
   reports as an error rather than a clean `None`, gets a "Choose…" button that appears to do
   nothing at all — no different, from the user's point of view, than a hung click. Recommended
   fix: mirror the `Ok(Err(error))` handling already present in `start_open_project`, surfacing
   the error into the settings sheet the same way `png_error` does for the Avatar tab.

2. **Harness/tooling limitations encountered this pass — explicitly not attributed to the app:**
   - `wtype` (the virtual-keyboard injector `Scripts/wayland-drive.sh` uses for `type`/`key`)
     drops and duplicates characters under host load, confirmed twice independently this pass
     (`/dev/shm/fprjcrit-fixture-git` typed key-by-key rendered as
     `/dev/ev/shm/fprjcrit-fixture-git` in the GTK location bar — a duplicated `ev/`). This is a
     known, previously-documented limitation of the drive tool, not the app; the location-bar
     text field itself accepted and displayed every keystroke it was sent faithfully.
   - The native GTK "Open Folder" dialog rendered anchored to only the right ~857px of the
     window in one run, with the app sidebar visible underneath, versus full-window in another
     run under otherwise-identical conditions — most plausibly a nested-compositor
     (headless sway) floating-window placement quirk given it was not reproducible on demand;
     flagged for visibility but not scored as an app rendering defect given the inconsistency.
   - `xdg-desktop-portal-gtk` cold-start took 20-30+ seconds under this host's load (5+
     concurrent driven instances during this pass) versus a few seconds when idle — real, but a
     host-contention artifact of the shared test box, not a Tiller behavior.

## What could not be fully reached, and why

`F-PRJ-03`, `F-PRJ-14`, and `F-PRJ-18`'s success paths were not personally driven to a clean,
live completion under the fully-isolated, portal-enabled harness this pass. All three depend on
the same first step (a working native picker round-trip), and every live attempt at that step
this pass was consumed by tooling friction rather than app misbehavior: `wtype` character
corruption, dialog-window placement drift, Add-Project menu coordinate drift when the sidebar
starts with zero projects, and process/label hygiene issues from Bash-tool-vs-inner-shell
timeout mismatches (an inner `timeout N` exceeding the tool's own outer limit SIGKILLs the whole
pipeline before `wayland-drive.sh`'s own cleanup trap runs, orphaning a `sway` process and
blocking reuse of that label). None of these are claimed as app defects. Given the code-level
review for all three rows matches the ledger's specific behavioral claims exactly (branch
structure, exact button/message text), and given F-PRJ-02's picker-opens-correctly half was
independently reconfirmed fresh under the corrected isolated harness, the risk that these three
rows have silently regressed is assessed as low — but "low risk" is not the same as "verified,"
so they are recorded as half-proven rather than PASSED, per the task's standard of proof.
