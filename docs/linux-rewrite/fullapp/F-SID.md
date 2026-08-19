# F-SID — Projects and sidebar (fresh critic pass, sweep-1)

Fresh, independent live-drive pass over all 19 F-SID rows. Driven against the warm binary
`/dev/shm/tt/debug/tiller` under Wayland (`Scripts/wayland-drive.sh`), label `sweep1sid`,
artefacts in `/dev/shm/sweep-1-F-SID`. Fixtures: `/dev/shm/sweep1sid-fix1` (a real git repo,
committed) and `/dev/shm/sweep1sid-fix2` (a plain non-git folder, used later to exercise
`Initialize Git repository`).

The previous ledger verdict for every one of these 19 rows was PASSED. This pass re-drove
every row from a live app rather than trusting that column. Net result: the PASSED verdicts
hold for the great majority of rows, with two genuine nuances worth flagging loudly (see
Defects/observations below) and one row (`F-SID-05`) downgraded to half-proven because the
literal clause does not fully hold for project rows.

**Environment note**: this host was under severe, worsening contention for most of the pass
(other critics' full app instances plus a live `rustc`/build-critic compile; load average
climbed from ~14 to 90+ over the session). Most UI actions still landed correctly with
generous sleeps; a handful of `wayland-drive.sh` invocations failed at the compositor-connect
step (`Unable to connect to .../sway.sock`, `wl_display_connect: No such file or directory`)
and were simply retried after a short wait — those are recorded here as harness/host noise,
not app defects, per the standard-of-proof rule. One single app crash (`panicked ... NoCompositor`)
happened once under peak load (~load avg 90) and did not reproduce on retry; it is noted but
not scored against any row since it is not clearly attributable and is outside F-SID's scope.

## Row-by-row verdicts

| row | verdict | evidence |
|---|---|---|
| F-SID-01 | PASSED | Every screenshot this pass (e.g. `03-02-two-projects.png`) shows the sidebar header "Projects" with a '+' Add-Project control at top-right, both present before and after adding projects. |
| F-SID-02 | PASSED | Clicked the Filter field, typed "fix1": `03-11-filter-fix1.png` shows only `sweep1sid-fix1` (fix2 disappears). Cleared the field fully (`End` + repeated `Backspace`): `02-15-filter-fully-cleared.png` shows both projects again. Side-finding reconfirmed live: filtering to a project's own name hides that project's OWN worktree rows too (only the header row matches the filter text; branch names don't literally match, so worktree children get filtered out along with genuinely non-matching siblings). Matches the ledger's prior side-finding exactly. |
| F-SID-03 | NOT EXERCISED | Opened the Add-Project '+' menu and confirmed it offers "Open Project...", "Clone Repository...", "Create Project..." (`02-08-add-project-menu.png`) — all three modes visible and clickable — but ran out of host-contention-free time this pass to drive one to completion. The ledger's existing PASSED evidence (wave I) already drove all three modes live in an earlier pass; this pass did not re-drive completion, so it neither confirms nor contradicts that PASSED — treat the menu as re-confirmed present, completion as unexercised by me. |
| F-SID-04 | PASSED | Clean test (filter empty, no contamination): `03-16-collapsed-clean.png` — one click on the fix1 chevron hides `master` and `New Worktree...` (fix2 untouched); `04-17-expanded-clean.png` — a second click restores both exactly. (An earlier attempt in this pass was run while a filter was still active and produced a misleading partial-hide result — re-tested cleanly per this note before drawing a conclusion.) |
| F-SID-05 | half-proven | Worktree-row click: fully proven. Clicking a worktree row highlights it and changes the central pane (Files panel path, status bar breadcrumb, terminal/chat content) to that worktree, confirmed repeatedly (e.g. `03-19-worktree-row-clicked.png`, and every subsequent action that switched worktrees). Project-row click: only toggles expand/collapse and shows a highlight while the row is under the cursor; it does **not** change the central surface at all. Isolated test: with `master` selected/shown, clicking the `sweep1sid-fix2` project header (`02-18-project-row-clicked.png`) toggled its disclosure triangle but the Files panel and status bar stayed on `master · /dev/shm/sweep1sid-fix1` — unchanged. The row's contract bundles "click a project row and then a worktree row" with one shared "and confirm...the central surface changes" clause; the worktree half holds, the project half does not, so this is graded half-proven rather than a clean PASSED. |
| F-SID-06 | PASSED | `tillerctl`-equivalent `ctl notify session=pane-0 status=running` flipped a solid orange dot onto the `Terminal` tab (`02-24-notify-running.png` tab strip) and the Files-panel footer read "Activity 1 running" — but did **not** put any badge on the still-visible `master` worktree row's branch icon, and collapsing the project with `status=running` active produced no badge on the collapsed project header either (`zoom-25-fullrow-v2.png`, zoomed pixel-check). Re-ran with `status=error` instead: the worktree row got a clear red dot next to the branch icon (`zoom-29-fullrow.png`... wait see below) and — decisively — collapsing the project **with `status=error` already active** showed a red dot badge on the collapsed `sweep1sid-fix1` project row (`zoom-29-fullrow.png`, a tight pixel-crop confirms a solid red circle left of the folder icon). So the literal clause ("collapse the project, confirm the project row still shows a status badge") is satisfied for error/needs-attention status. See side-finding below: "running" alone does not escalate to a project-level badge, only urgent statuses do. |
| F-SID-07 | PASSED | Clicked the gear icon on the `sweep1sid-fix1` project row: `04-20-gear-clicked.png` shows a full "Project Settings · sweep1sid-fix1" sheet — display name, project-icon picker (Icon/Emoji/Avatar tabs), colour swatches, Default Worktree Base, Worktree Location with a Choose... picker, Remove Project, and Close. |
| F-SID-08 | PASSED | Right-clicked the still-plain-folder `sweep1sid-fix2` project row; its context menu offered an enabled "Initialize Git repository" (fix1's own menu shows this item greyed "is already initialized", confirming the menu correctly distinguishes state — `02-06-rightclick-project-row.png`). Clicked it: the sidebar's fix2 worktree row flipped from a plain folder icon to the git-branch icon labelled "master" with a "Primary" badge, and a "New Worktree..." control appeared under it (git-only feature) — `02-67-git-initialized.png`. Independently verified on disk: `ls /dev/shm/sweep1sid-fix2/.git` shows a real git dir, and `git status` inside it reports "Sul branch master" (branch master, no commits yet). |
| F-SID-09 | PASSED | Right-clicked the `sweep1sid-fix2` project row, clicked "Show in File Manager": a real Cosmic Files GUI window opened tiled alongside Tiller inside the nested compositor, breadcrumb "Filesystem > dev > shm > sweep1sid-fix2", listing the real `b.txt` — `02-68-show-in-file-manager.png`. This is a second, independent GUI process actually launched by the app, not a synthetic path check. |
| F-SID-10 | NOT EXERCISED | Not reached this pass — ran out of contention-free window before driving Remove Project to completion. (Not driven live in this pass; see "what I could not reach".) |
| F-SID-11 | PASSED | Worktree rows show branch name + path + Primary badge for a git worktree (`master`, `04-17-expanded-clean.png`), a blank-branch folder-style row for the not-yet-git fix2 worktree (same shot), and a live agent-status dot that flips colour with real activity (orange/running and red/error, see F-SID-06 evidence) plus a genuine agent-panel identity ("Claude Code" tab, F-SID-14 evidence). The "comment" field specifically was not separately exercised this pass. |
| F-SID-12 | PASSED | Right-clicked non-primary `sid-wt-a`, its menu's first item read "Set Primary" (master's own menu read "Unset Primary", confirming the flip is state-aware — `02-37-rightclick-sidwta.png`). Clicked it: `master` lost its "Primary" badge and `sid-wt-a` gained it — `03-40-primary-set-to-a.png`. Confirmed the flag persisted across an app relaunch (`02-38-primary-set-to-a.png` still shows it after a kill+relaunch cycle). |
| F-SID-13 | PASSED | Used the "New Worktree..." control under `sweep1sid-fix1` three times (branch names typed, Enter pressed): `sid-wt-a` (`03-32-worktree-a-created.png`), `sid-wt-b` (`02-33-worktree-b-created.png`), `sid-wt-c` all appeared under the project at their expected paths (`/dev/shm/sweep1sid-fix1-sid-wt-a` etc.), each a real new git worktree. |
| F-SID-14 | PASSED | Drove all three named trials from a worktree's context menu, each producing a real new tab: "New Terminal" on `sid-wt-b` opened a live shell tab and switched selection to it (`03-50-new-terminal-b-result.png`); "Claude Code" on `sid-wt-a` opened a genuine Claude Code CLI session showing its real workspace-trust prompt (`02-59-claudecode-on-sidwta.png`); "New Chat" on the fix2 worktree opened a real Chat tab with a connecting Claude Code composer (`02-60-newchat-on-fix2wt.png`). |
| F-SID-15 | NOT EXERCISED | Not reached this pass — see "what I could not reach". |
| F-SID-16 | PASSED | Dragged the `sweep1sid-fix2` project row from 2nd to 1st position (`drag 150 357 150 110`): order changed from (fix1, fix2) to (fix2, fix1), landing exactly at the top — `03-65-after-drag-project.png` vs the preceding state. |
| F-SID-17 | PASSED | Dragged worktree `sid-wt-b` (3rd) up to just under `master` inside `sweep1sid-fix1`: order changed from (master, sid-wt-a, sid-wt-b) to (master, sid-wt-b, sid-wt-a) — `03-63-after-drag-worktree.png` vs `02-62-before-drag-worktree.png`. |
| F-SID-18 | half-proven | Confirmed repeatedly throughout the pass as the default state of every freshly created worktree before its first tab: "No Terminals" / "Open a new terminal to get started." / an orange "New Terminal" button (e.g. `03-19-worktree-row-clicked.png`, `02-38-primary-set-to-a.png`). Did not get a final, isolated screenshot of a *freshly selected, still-empty* worktree in this exact pass's own numbered sequence for `sid-wt-c` before running out of contention-free time, but the empty state itself is unambiguously demonstrated multiple times above under the same app build. |
| F-SID-19 | NOT EXERCISED | Not reached this pass — see "what I could not reach". |

## Defects / observations worth flagging loudly

1. **F-SID-06 nuance, not a defect**: a plain `running` status does not escalate to a
   project-level badge on a collapsed project (verified with a tight pixel crop showing only
   the plain "..." kebab, no dot, at `zoom-25-fullrow-v2.png`), while `error` status does
   (red dot, `zoom-29-fullrow.png`). The row's contract just says "activity"; I read this as a
   reasonable, deliberate design choice (only attention-worthy states bubble up through a
   collapsed group) rather than a defect, but it is worth a human's eyes since the ledger's
   prior evidence for this row only exercised the `error` path and did not call out that
   `running` behaves differently.

2. **F-SID-05, a real (if minor) contract gap**: clicking a *project* row never changes the
   central surface, only a *worktree* row does. If the row's intent is "either kind of
   sidebar row is selectable and drives the main content," project rows do not fulfil that;
   they only expand/collapse. Downgraded from the ledger's PASSED to half-proven for this
   reason — worth a maintainer decision on whether project rows are supposed to be
   independently selectable at all.

## What I could not reach, and why

- **F-SID-03** (Add Project completion), **F-SID-10** (Remove Project), **F-SID-15** (Remove
  Worktree), **F-SID-19** (`Ctrl+T` from the empty state) were not driven to a recorded verdict
  in this pass. Reason: pure host contention, not an app or harness defect in the row itself.
  From roughly the point `F-SID-09` was confirmed onward, this shared host's load average rose
  from ~14 to 90+ (other critics' full app instances plus a live compile competing for the same
  CPUs), and `wayland-drive.sh` invocations began intermittently failing at the compositor-connect
  step before any of my actions could run. I recovered from several such failures with longer
  waits between invocations, but ran out of a productive window before finishing the remaining four
  rows. All four have every prerequisite already live and ready to go (a disposable second project
  `sweep1sid-fix2`, three disposable worktrees `sid-wt-a/b/c` under `sweep1sid-fix1`) — none is
  blocked on anything structural, only on host CPU availability at the time this pass ended.
- One single `panicked ... NoCompositor` app crash happened once, at the single highest-load
  moment observed (~load avg 90-92), and did not reproduce on the very next retry a few minutes
  later at lower load. Recorded here for visibility but not scored against any specific F-SID row
  since it could not be tied to a specific UI action (it happened while the app was merely idling
  between my invocations) and did not reproduce.

## Artefacts

All screenshots referenced above are under `/dev/shm/sweep-1-F-SID/` on this host (tmpfs,
not committed — per instructions, screenshots stay off `/`, only this report is committed).
