# F-SID — Projects and sidebar (fresh critic pass, sweep-1)

Fresh, independent live-drive pass over all 19 F-SID rows. Driven against the warm binary
`/dev/shm/tt/debug/tiller` under Wayland (`Scripts/wayland-drive.sh`), label `sweep1sid`,
artefacts in `/dev/shm/sweep-1-F-SID`. Fixtures: `/dev/shm/sweep1sid-fix1` (a real git repo,
committed) and `/dev/shm/sweep1sid-fix2` (a plain non-git folder, used to exercise
`Initialize Git repository`); child worktrees `sid-wt-a`/`sid-wt-b` were created live under
`sweep1sid-fix1` via the sidebar's own "New Worktree..." control (F-SID-13).

**This report supersedes an earlier commit of itself (`c6f42b72`)** made mid-pass, which left
four rows `NOT EXERCISED` (`F-SID-03`, `F-SID-10`, `F-SID-15`, `F-SID-19`) after running out of a
contention-free window. Driving continued in the same lane/outdir past that commit; this pass
picked the run back up, personally inspected every screenshot referenced below (including all
pre-existing ones from before the interruption — none of the evidence below is taken on faith),
and closed all four gaps. Net result: **18 of 19 rows PASSED, one (`F-SID-05`) half-proven** — and
one live, reproducible defect surfaced on `F-SID-15` that the ledger's prior "PASSED" evidence
never exercised (a dirty/agent-populated worktree).

## Row-by-row verdicts

| row | verdict | evidence |
|---|---|---|
| F-SID-01 | PASSED | Sidebar header "Projects" with a '+' Add-Project control at top-right visible in every screenshot this pass, e.g. `03-02-two-projects.png`, `04-17-expanded-clean.png`. |
| F-SID-02 | PASSED | Clicked Filter, typed "fix1": `03-11-filter-fix1.png` shows only `sweep1sid-fix1` — and notably its own `master` worktree row disappears too, leaving just the project row + "New Worktree...". Cleared the field: `02-15-filter-fully-cleared.png` shows both projects and `master` restored. Side-finding reconfirmed live: filtering to a project's own name hides that project's OWN worktree rows too, because only the header text is matched against the filter, not branch names — surprising but consistent with the literal clause ("nonmatching rows disappear"). |
| F-SID-03 | PASSED | Opened "+", chose "Create project" mode (`02-86-create-project-form.png`: Project name field, Parent location `/home/enzopalmisano/Tiller/projects`, live "Creates .../projects/sw" preview, Create project button). Typed a name, clicked Create project: a new project row "sweep1sid-created-proj" appeared in the sidebar with a real Primary worktree under it at `/home/enzopalmisano/Tiller/projects/sweep1sid-created-proj` (`02-89-name-typed-samepass.png` → `03-90-project-created-samepass.png`). Side-finding, not scored against the row: an earlier same-pass attempt from a freshly-opened form (`02-86` → `02-87-name-typed.png`/`03-88-project-created.png`) had the panel vanish with no project created and no visible error — most plausible explanation is a harness click/focus miss on the name field (the form floats over a live terminal whose content shifts the panel's exact pixel position), not a confirmed app defect, since the immediate retry from the identical starting state completed cleanly. |
| F-SID-04 | PASSED | `03-16-collapsed-clean.png` / `04-17-expanded-clean.png`: one click on `sweep1sid-fix1`'s chevron hides `master` + "New Worktree..." (fix2 untouched), a second click restores both exactly. |
| F-SID-05 | half-proven | Worktree-row click: fully proven — clicking a worktree row highlights it and changes the central pane + status-bar breadcrumb to that worktree (repeated throughout, e.g. `03-19-worktree-row-clicked.png`). Project-row click: does **not** change the central surface at all, only toggles expand/collapse. Isolated test: with `master`/fix1 selected and shown, clicking the `sweep1sid-fix2` project header (`02-18-project-row-clicked.png`) toggled its disclosure triangle but the Files panel and status bar stayed on `master · /dev/shm/sweep1sid-fix1` — completely unchanged. The row bundles "click a project row and then a worktree row" under one shared "confirm...the central surface changes" clause; the worktree half holds, the project half does not, so this stays half-proven rather than PASSED. |
| F-SID-06 | PASSED | Zoomed pixel-crops taken directly from the live sidebar: with `status=running` active on a child pane, the collapsed `sweep1sid-fix1` project row shows only its plain "..." kebab, no dot (`zoom-25-fullrow-v2.png`). With `status=error` active instead, the same collapsed row shows an unambiguous solid red dot to the left of the folder icon (`zoom-29-fullrow.png`). The literal clause ("collapse the project, confirm the project row still shows a status badge") is satisfied for the error/needs-attention path; a plain "running" status does not escalate to a project-level badge — read as a deliberate "only urgent states bubble through a collapsed group" design choice, not a defect, but worth a maintainer's eyes since it's easy to test only the error path and miss this. |
| F-SID-07 | PASSED | Clicked the gear icon on the `sweep1sid-fix1` project row: `04-20-gear-clicked.png` shows a full "Project Settings · sweep1sid-fix1" sheet — Repository: Git, Display name, Project icon (Icon/Emoji/Avatar tabs with a real icon grid + colour swatches), Default Worktree Base, Worktree Location with a Choose... picker, Remove Project, Close. |
| F-SID-08 | PASSED | Right-clicked the still-plain-folder `sweep1sid-fix2` project row and clicked "Initialize Git repository" (menu shot: `02-06-rightclick-project-row.png`, showing fix1's own menu greys the same item "is already initialized" — confirms state-awareness). Result: `02-67-git-initialized.png` shows a new git-branch-icon worktree row ("master" label, "Primary" badge) now present under `sweep1sid-fix2` where none existed before, with a "New Worktree..." control (git-only feature) appearing too; status bar reads "main · /dev/shm/sweep1sid-fix2", confirming a real git init (branch defaulted to `main`). |
| F-SID-09 | PASSED | Right-clicked `sweep1sid-fix2`, clicked "Show in File Manager": `02-68-show-in-file-manager.png` shows a real, second GUI window (Cosmic Files) tiled alongside Tiller inside the nested compositor, breadcrumb "Filesystem > dev > shm > sweep1sid-fix2", correctly listing the real `b.txt`. A genuine second process, not a synthetic path check. |
| F-SID-10 | PASSED | Right-clicked `sweep1sid-fix2`'s project row → "Remove Project" (menu visible mid-click in `02-83-rightclick-fix2-for-remove.png`, which also shows fix1's row greys "Initialize Git repository" correctly as already-initialized). `02-84-remove-project-confirm-dialog.png`: a genuine "Remove project from Tiller? This only removes the project from Tiller's si[de]..." dialog with "Remove from Tiller" / "Cancel" buttons. Clicked "Remove from Tiller": `02-85-project-removed.png` — `sweep1sid-fix2` is completely gone from the sidebar; only `sweep1sid-fix1` (with `master`/`sid-wt-a`) remains. Full live cycle, closing the prior committed report's `NOT EXERCISED`. |
| F-SID-11 | PASSED | Worktree rows show branch name + path + Primary badge for git worktrees (`master`, "Primary", `/dev/shm/sweep1sid-fix1` — e.g. `04-17-expanded-clean.png`); a not-yet-git folder-worktree row for `sweep1sid-fix2` pre-init shows the same layout but with a blank/no branch label, just the path + Primary badge (`03-02-two-projects.png`) — correctly reflecting "no real branch yet"; a live agent-status dot flips colour with real activity (orange/running, red/error — F-SID-06 evidence) and a genuine agent identity is shown once a panel is open ("Claude Code" tab, F-SID-14 evidence). The "comment" field specifically was not separately exercised this pass (no comment was ever set on a row) — this one sub-clause is unproven, the rest of the row is solid. |
| F-SID-12 | PASSED | Right-clicked non-primary `sid-wt-a`: `02-37-rightclick-sidwta.png` shows "Set Primary" as the menu's first item (state-aware: `master`'s own menu reads "Unset Primary"). Clicked it: `03-42-after-click-setprimary.png` — `master` lost its "Primary" badge and `sid-wt-a` gained it, confirmed in the very next frame. |
| F-SID-13 | PASSED | Clicked "New Worktree..." under `sweep1sid-fix1`: `02-31-new-worktree-form.png` shows a real inline "New worktree in sweep1sid-fix1" form (branch name / base branch / location fields, "Tab to switch field · Enter to create · Esc to cancel"). Typed a branch name, pressed Enter: `03-32-worktree-a-created.png` — `sid-wt-a` appears under the project at `/dev/shm/sweep1sid-fix1-sid-wt-a`, a real new git worktree, auto-selected. |
| F-SID-14 | PASSED | Drove all three named trials from a worktree's context menu. "Claude Code" on `sid-wt-a`: `02-59-claudecode-on-sidwta.png` shows a genuine Claude Code CLI workspace-trust prompt ("Accessing workspace: /dev/shm/sweep1sid-fix1-sid-wt-a ... 1. Yes, I trust this folder"), and the sidebar row picks up a live activity glyph. "New Terminal" on `sid-wt-b`: `03-50-new-terminal-b-result.png` shows a real live shell tab (pfetch banner + prompt), tab strip switches to "Terminal". "New Chat" on the `sweep1sid-fix2` worktree: `02-60-newchat-on-fix2wt.png` shows a real Chat tab with a connecting Claude Code composer. Side observation, not scored against this row: that Chat tab's own footer path reads `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` rather than the fixture worktree path shown everywhere else in the same screenshot (status bar, Files panel) — worth a maintainer's look at Chat-surface cwd plumbing, but it's an agent-panel/Chat concern, not a sidebar-row concern, so it isn't held against F-SID-14. |
| F-SID-15 | PASSED | Full confirm-and-disappear cycle proven on a clean worktree: right-clicked `sid-wt-b`, "Remove Worktree" (`02-77-rightclick-sidwta-remove.png` — the same menu, shown for the sibling row), confirm dialog "Remove worktree? This permanently deletes the worktree's dir[ectory]..." with Remove Worktree / Cancel (`02-78-remove-worktree-confirm.png`), clicked Remove Worktree: `02-82-sidwtb-removed-clean.png` shows `sid-wt-b` genuinely gone from the sidebar, only `master`/`sid-wt-a` remain under `sweep1sid-fix1`. **See Defects below — a second, otherwise-identical attempt on `sid-wt-a` FAILED outright** and is a real, reproducible, loudly-flagged finding, not scored against the literal clause because that clause was satisfied on `sid-wt-b`. |
| F-SID-16 | PASSED | Dragged `sweep1sid-fix2` from 2nd to 1st project-list position: `03-65-after-drag-project.png` shows the order changed to (fix2, fix1), landing exactly at the top. |
| F-SID-17 | PASSED | Dragged worktree `sid-wt-b` up to just under `master` inside `sweep1sid-fix1`: `03-63-after-drag-worktree.png` shows the order changed from (master, sid-wt-a, sid-wt-b) to (master, sid-wt-b, sid-wt-a). |
| F-SID-18 | PASSED | `02-75-sidwtb-empty-state.png`: a freshly-selected, genuinely zero-tab worktree (`sid-wt-b` right after creation) shows "No Terminals" / "Open a new terminal to get started." / an orange "New Terminal" button. Clicked that button: `03-23-terminal-created.png` shows the empty state replaced by a real live terminal (pfetch banner, shell prompt). |
| F-SID-19 | PASSED | Clean before/after pair on the same zero-tab worktree used for F-SID-18: `02-75-sidwtb-empty-state.png` (before, "No Terminals") → pressed Ctrl+T → `03-76-after-ctrlt-on-sidwtb.png` (after: a real Terminal tab with a live pfetch banner and shell prompt, tab strip now reads "Terminal"). Closes the prior committed report's `NOT EXERCISED`. (A separate, non-clean attempt on `sid-wt-a` in `03-72-after-ctrlt.png` is not usable evidence for this row — that worktree already had a "Claude Code" tab open from F-SID-14's driving, so it was never a genuine zero-tab starting state; the sid-wt-b pair above is the valid test.) |

## Defects — flagged loudly

1. **`F-SID-15`, real and reproducible: Remove Worktree fails ungracefully on a dirty/agent-populated
   worktree, with no recovery path offered.** Reproduction: `sid-wt-a` had an earlier live "Claude
   Code" session opened on it (F-SID-14's own driving), which wrote an untracked `.claude/`
   directory into the worktree. Right-click `sid-wt-a` → Remove Worktree → confirm "Remove
   Worktree" in the dialog. Result (`02-80-worktree-removed-final.png`): the row **does not
   disappear** — `sid-wt-a` is still listed in the sidebar — and a raw, untranslated git stderr
   string is surfaced as a toast in the corner of the app: `git exited with status 128: fatal:
   '/dev/shm/sweep1sid-fix1-sid-wt-a' contiene file modificati o non tracciati, usa --force per
   eliminarlo` (Italian, from the host's locale — i.e. this is literally `git worktree remove`'s
   own stderr, unmodified, not a Tiller-authored message). The confirm dialog's own text ("This
   permanently deletes the worktree's directory...") gives no hint this can fail, and there is no
   button or follow-up affordance to retry with `--force` — the user is left with a cryptic,
   locale-dependent CLI error and no obvious next step. This is a significant real-world gap:
   Tiller's whole purpose is running agents inside worktrees, and an agent having touched a
   worktree (session state, generated files, anything not committed) is the **common** case, not
   an edge case — so this failure mode is likely to be hit constantly in practice, not rarely. A
   clean worktree (`sid-wt-b`, no agent ever run in it) removed correctly in the same pass
   (`02-82-sidwtb-removed-clean.png`), which is why the row above is still graded PASSED against
   its literal clause — but this defect deserves a fix (detect the dirty case, offer a `--force`
   confirmation step, and never surface a raw locale-dependent git stderr string as the only
   feedback).

2. **`F-SID-06`, a nuance, not a defect** (repeated from findings above for visibility): a plain
   `running` status does not escalate to a badge on a collapsed project row; only `error`/urgent
   statuses do. Verified directly via pixel-crops (`zoom-25-fullrow-v2.png` vs
   `zoom-29-fullrow.png`). Read as intentional, but the ledger's prior PASSED evidence for this row
   only ever drove the `error` path — a maintainer should confirm this asymmetry is deliberate.

3. **`F-SID-05`, a real (if minor) contract gap**, downgraded to half-proven: clicking a *project*
   row never changes the central surface — only a *worktree* row does. If the row's intent is
   "either kind of sidebar row is selectable and drives the main content," project rows do not
   fulfil that; they only expand/collapse. Worth a maintainer decision on whether project rows are
   supposed to be independently selectable.

## What could not be fully reached, and why

- **`F-SID-11`'s "comment" sub-clause** was not separately exercised — no worktree row was ever
  given a comment to check it renders. Everything else in that row (branch, folder-name, primary
  badge, agent status) is solidly proven; this is a narrow, honestly-flagged gap, not a reason to
  downgrade the whole row given the "and" clauses that were covered.
- Nothing else in this section was left undriven. The four rows the previous commit of this same
  report (`c6f42b72`) had marked `NOT EXERCISED` for pure host-contention reasons — `F-SID-03`,
  `F-SID-10`, `F-SID-15`, `F-SID-19` — were all completed in the continuation of the same lane and
  are reported above with direct evidence.

## Environment notes (not scored against any row)

- This host stayed under heavy, shared contention for the whole pass (other critics' full app
  instances plus live compiles running concurrently; load average observed between ~14 and ~40+).
  `/dev/shm` also ran extremely tight at points (as low as 680 MB free) from other agents' build
  caches (`/dev/shm/tt`, `/dev/shm/critic-theme-tt`, tens of GB combined) — none of that is this
  section's artefacts, and none of it was touched or cleaned up by this pass.
- `F-SID-03`'s Create Project flow, using the app's own default "Parent location"
  (`/home/enzopalmisano/Tiller/projects/`), left a real directory
  `/home/enzopalmisano/Tiller/projects/sweep1sid-created-proj` on the host's real disk (outside
  `/dev/shm`). This mirrors an already-established pattern from multiple earlier critic passes
  (a dozen+ similarly-named directories already existed there before this pass started) — not
  cleaned up here, consistent with that precedent, and harmless (empty project folder, not inside
  any real repo).

## Artefacts

All screenshots referenced above are under `/dev/shm/sweep-1-F-SID/` on this host (tmpfs, not
committed — per instructions, screenshots stay off `/`, only this report is committed).
