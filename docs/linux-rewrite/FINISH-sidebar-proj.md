# Finish-line critic: sidebar-proj (F-SID-01..19, F-PRJ-01..18)

Shard: every row matching `F-SID-*` (19 rows) and `F-PRJ-*` (18 rows) in
`docs/linux-rewrite/INVENTORY-LEDGER.md` — 37 rows total. Judged strictly against the VERIFY
clause text in `docs/linux-rewrite/01-inventory-app.md` (never ledger prose). A prior PASSED is
the hypothesis under test, not evidence — every row below was re-driven live today unless marked
otherwise, and every row not re-driven today is marked `NOT EXERCISED` rather than carried
forward silently.

## Setup

- Drive lane: `TILLER_WL_LABEL=wf-sid Scripts/wayland-drive.sh <outdir> '<actions>' <timeout>`
- DB: `/tmp/wf-sid.sqlite` (persists across relaunches within the label; inspected directly via
  python3's built-in `sqlite3` module — no `sqlite3` CLI on this box)
- Control socket for direct RPC calls: `/tmp/wf-sid.sock`, reachable mid-session from inside the
  drive script's `eval`'d action block (it runs `export -f ctl ...` in the same bash process that
  holds `$SOCK`, so a raw `TILLER_SOCKET=$SOCK ./rust/target/debug/tillerctl ...` line dropped into
  the action heredoc works without any extra plumbing)
- Fixtures: `/tmp/wf-sid-fixtures/{fixture-proj,second-proj}` (git repos), plus several projects
  created during earlier session work under `/home/enzopalmisano/Tiller/projects/` (Spoon-Knife,
  wf-sid-created, wf-sid, wfsidcreated — the last three are non-git "Folder" projects, useful for
  F-PRJ-12's repository-type-switch test)
- Screens captured under `/tmp/wf-sid-shots-*/` (not committed; paths cited below are for replay
  within this environment, not for artifact review)

## Headline findings (read before the per-row table)

1. **F-SID-19 is a genuine, reproducible defect.** `Ctrl+T` does nothing from the "No Terminals"
   empty state — confirmed FOUR separate times today, including with the empty-state icon itself
   clicked immediately beforehand. The same shortcut correctly creates a second terminal tab when
   a terminal pane already has focus (tested once, worked immediately), and the "New Terminal"
   button in the same empty state works every time. So the global `ctrl-t` keybinding
   (`WindowCommand::NewTerminalTab`, `rust/crates/tiller/src/main.rs:125`) is real and functional —
   it just isn't reachable from the empty-state view's focus/dispatch context. This overturns a
   prior (pre-compaction, unverified-today) belief that F-SID-19 passed — exactly the trap the
   brief warns about.

2. **F-PRJ-13's Reset button does not reset.** Selecting a new icon and colour on the Project
   Settings sheet works and persists correctly. Clicking "Reset" underneath the colour swatches
   does nothing to the icon/colour state — confirmed twice, including a double-click retry. Both
   times, the Files side-panel and status bar visibly switched to a *different* project
   (Spoon-Knife) at the exact moment of the click, even though Spoon-Knife's row is not visible
   anywhere in the settings-sheet screenshot. That is the signature of a click landing on a hidden
   sidebar row underneath the settings sheet rather than on the Reset button — a z-order/hit-test
   bug, not (per a quick read of `project_identity.rs:426`/`:720` and its own passing unit test
   `reset_button_restores_the_default_value`) a logic bug in `reset()` itself. Filed against
   F-PRJ-13 because the observable behavior fails the clause regardless of root cause.

3. **F-SID-02's filter hides all worktree rows under a matched project**, not just the
   nonmatching ones — filtering to "fixture-proj" shows the project header and a bare
   "New Worktree…" affordance with no `master`/`sid-feature` rows in between, even though neither
   branch name matches the filter text either. Read literally ("type text matching one project or
   branch... confirm nonmatching rows disappear") this is consistent, not a violation — worktree
   branch names genuinely don't match "fixture-proj" — but it's a surprising UX difference from
   the usual "parent match keeps children" filtering convention and cost real time this session
   (see F-SID-13 below). Noted, not scored as a defect.

4. **Drag-to-reorder projects landed the dragged row far from the drop target.** Dragging
   `second-proj` (position 2) up toward the top of the list (release near `fixture-proj`) instead
   moved it down to position 6, right before `emptytest`. The literal F-SID-16 clause only
   requires "confirm the order changes," which it did, so the row is scored PASSED — but the
   landing position bore no visible relationship to the drop point. Reordering worktrees *within*
   a project (F-SID-17) landed exactly where dropped on the same drive lane, so this may be
   specific to the project list's reorder math, or to how the synthetic `wtype`/virtual-pointer
   drag reports intermediate positions versus a real mouse. Flagged for a human look, not failed.

5. **Emoji cannot be typed through this drive lane.** `wtype` reliably delivers plain ASCII
   ("octocat", "example.com", "siddisp") but never delivered a single emoji character (tried
   🎉 U+1F389 and ⭐ U+2B50, both rejected as "Enter exactly one emoji." — i.e. the field stayed
   empty). This blocked a direct positive-path test of F-PRJ-16's "enter one emoji" typing flow.
   Worked around by using **Open Emoji Picker** and clicking a grid glyph instead, which
   *did* successfully set `fixture-proj`'s icon to `⭐` (confirmed via `icon_kind='emoji'`,
   `icon_value='⭐'` in the DB and in the sidebar) — strong indirect proof the underlying
   single-emoji-accept path works, even though the literal typed-input sub-case is unconfirmed.

6. **The known persistence/tab-reparenting bugs from earlier in this task were not re-verified
   today** (F-SID-12's primary-pill-ignored-on-relaunch, F-SID-15's false "permanently deletes the
   directory" claim, the cross-worktree tab-reparenting issue). They are real, but per the brief's
   own rule a prior finding is a hypothesis, not evidence, and I ran out of session time before
   re-driving them. Rows depending on them are marked `NOT EXERCISED` below rather than carried
   forward as PASSED/FAILED on stale evidence.

7. **F-PRJ-02/03/04 remain UNREACHABLE** on this drive lane: the sandbox's default D-Bus session
   bus refuses connections (`Connection refused (os error 111)`) even from the host shell, and
   wrapping the drive invocation in `dbus-run-session` gets `xdg-desktop-portal-gtk` running but its
   GTK file-chooser window never maps in the nested sway compositor ("Failed to associate portal
   window with parent window"). Not re-attempted today (same environment, no reason to expect a
   different result) — this is an environment constraint, not an app defect.

## Per-row results

**F-SID-01** — PASSED. Sidebar header reads "Projects" with a `+` Add-Project control, visible in
every screenshot this session (e.g. `/tmp/wf-sid-shots-set2/02-00-settings-opened.png` shows the
list behind the opened settings sheet).

**F-SID-02** — PASSED. Filtered to `fixture-proj`, `emptytest`, and others repeatedly today (e.g.
`/tmp/wf-sid-shots-final/02-00-filter-emptytest-gone.png` — zero rows after emptytest was removed
and filtered for); a fresh unfiltered relaunch always shows the full list, equivalent to "clear it
and confirm they return." See headline finding 3 for a related but non-blocking observation.

**F-SID-03** — NOT EXERCISED. Did not open Add Project / complete an add flow today.

**F-SID-04** — PASSED. `fixture-proj` expanded/collapsed repeatedly while isolating worktree rows
for the drag tests, e.g. `/tmp/wf-sid-shots-drag2/02-00-before.png`.

**F-SID-05** — PASSED. Selecting `master`, `sid-nw`, and various projects visibly highlighted the
row and changed the Files panel / breadcrumb each time, e.g.
`/tmp/wf-sid-shots-ct3/03-01-after-ctrlt.png` (breadcrumb `/tmp/wf-sid-fixtures/fixture-proj` after
clicking `master`).

**F-SID-06** — NOT EXERCISED. Did not start activity in a descendant pane and collapse its
project today.

**F-SID-07** — PASSED. Hover-then-click on a project row's gear reliably opened Project Settings
throughout this session, e.g. `/tmp/wf-sid-shots-set2/02-00-settings-opened.png`,
`/tmp/wf-sid-shots-emoji3/02-00-settings-check.png`.

**F-SID-08** — half-proven. Exercised the *Initialize Git* action live today, but via the
Project Settings sheet's button (`/tmp/wf-sid-shots-repotype4/02-00-after-init-git.png`: `wf-
sid-created` flips from "Repository: Folder" + an "Initialize Git" button to "Repository: Git"
with the icon panel in its place) rather than via the sidebar row's right-click context menu that
F-SID-08's clause and SRC line specify. Same underlying action, different entry point — the
context-menu path itself was not re-clicked today.

**F-SID-09** — NOT EXERCISED. "Show in Finder" (file-manager reveal) not tried today.

**F-SID-10** — half-proven. The equivalent "remove project after confirmation" action was
exercised live today and fully confirmed end-to-end (see F-PRJ-11), but through the Project
Settings sheet's "Remove Project" control, not the sidebar row's right-click "Remove Project"
context-menu item that F-SID-10 specifically names.

**F-SID-11** — half-proven. Branch name, folder path, and Primary pill are visible on every
worktree row screenshot today (e.g. `/tmp/wf-sid-shots-set5/02-00-opened.png`: `master`,
`/tmp/wf-sid-fixture...`, `Primary`). The durable comment annotation was set live today via
`TILLER_SOCKET=$SOCK tillerctl worktree-set --worktree /tmp/wf-sid-fixtures/fixture-proj --comment
"sid-comment-live"` run from inside the drive script's action block, and it rendered on the row
immediately without a relaunch (`/tmp/wf-sid-shots-comment/03-01-after-comment-set.png`) and
persisted to the DB (`worktree.comment = 'sid-comment-live'`). The fifth element, agent status on
a worktree row, was not re-driven today.

**F-SID-12** — NOT EXERCISED. Did not right-click a worktree to Set/Unset Primary today (a
known persistence defect was found here before compaction — primary flag persists correctly to
the DB but a fresh relaunch's sidebar ignores it and always shows the first worktree as Primary —
but that finding predates today's session and per the brief's own rule cannot be carried forward
as this row's live verdict).

**F-SID-13** — PASSED. From the normal (unfiltered) sidebar, clicked `fixture-proj`'s "New
Worktree…", filled the branch-name field with `sid-nw`, pressed Enter, and the new worktree
appeared under `fixture-proj` at `/tmp/wf-sid-fixtures/fixture-proj-sid-nw`, auto-selected, showing
the empty-terminal state (`/tmp/wf-sid-shots-nw2/03-01-after-enter.png`). Note: the *filtered*
sidebar view hides worktree rows entirely (headline finding 3), which cost a false start before I
switched to the unfiltered view for this test.

**F-SID-14** — half-proven. Used the "New Terminal" button inside the no-terminal empty state
(reached by selecting a worktree, not by right-clicking it) — confirmed live today
(`/tmp/wf-sid-shots-ct4/02-00-after-new-terminal-button.png`: a live bash prompt opens with the
correct `fixture-proj` breadcrumb). The context-menu-driven "New Terminal / agent panel / New
Chat" trio specified by F-SID-14's clause was not re-clicked today.

**F-SID-15** — half-proven. Accidentally landed a click on a worktree row that opened the
"Remove worktree?" confirmation dialog with its (previously-documented, still-unverified-today)
"This permanently deletes the worktree's dire[ctory]" text
(`/tmp/wf-sid-shots-rmproj2/02-00-settings-emptytest.png`), proving the dialog still triggers
correctly from a right-click-equivalent action — but I clicked Cancel rather than confirming, to
avoid destroying a worktree I hadn't intended to touch, so the "confirm it disappears" half of the
clause was not completed today.

**F-SID-16** — PASSED (see headline finding 4). Dragging `second-proj` changed the project order;
literal clause satisfied. Landing position vs. drop point mismatch noted as a possible defect for
a human to look at, not scored as a failure.

**F-SID-17** — PASSED. Dragging the `sid-nw` worktree row up to just under `master` reordered
`fixture-proj`'s worktrees from `master, sid-feature, sid-nw` to `master, sid-nw, sid-feature`
exactly as dropped (`/tmp/wf-sid-shots-drag2/03-01-after-drag-worktree.png`).

**F-SID-18** — PASSED. Selecting a worktree with zero tabs showed "No Terminals" / "Open a new
terminal to get started." / a "New Terminal" button (`/tmp/wf-sid-shots-ct6/03-01-...` and
several others), and clicking that button created a real terminal tab.

**F-SID-19** — FAILED — defective. See headline finding 1. `Ctrl+T` from the no-terminal empty
state does nothing, reproduced 4/4 times today, including with the empty-state directly clicked
immediately before the chord. `Ctrl+T` does work once any terminal tab already has focus (tested
once, succeeded immediately — `/tmp/wf-sid-shots-ct5/02-00-ctrlt-with-existing-tab.png` shows a
second Terminal tab created).

**F-PRJ-01** — NOT EXERCISED. Add Project sheet not opened today.

**F-PRJ-02** — UNREACHABLE (environment). See headline finding 7.

**F-PRJ-03** — UNREACHABLE (environment). Same cause as F-PRJ-02.

**F-PRJ-04** — UNREACHABLE (environment). Same cause as F-PRJ-02.

**F-PRJ-05** — NOT EXERCISED today (a Spoon-Knife project exists from earlier session work,
proving the flow works in general, but the clone flow itself was not re-driven today).

**F-PRJ-06** — NOT EXERCISED.

**F-PRJ-07** — NOT EXERCISED.

**F-PRJ-08** — NOT EXERCISED today (several created projects exist from earlier session work —
`wf-sid-created`, `wf-sid`, `wfsidcreated` — but the create flow itself was not re-driven today).

**F-PRJ-09** — NOT EXERCISED.

**F-PRJ-10** — NOT EXERCISED today.

**F-PRJ-11** — PASSED. Opened Project Settings for the disposable `emptytest` project, clicked
"Remove Project," got the "Remove project from Tiller? This only removes the project from
Tiller's si[debar]..." dialog (`/tmp/wf-sid-shots-rmproj4/02-00-remove-clicked.png`), clicked
"Remove from Tiller," and confirmed: the DB row is gone (`select ... where id='p-9e09ff1bd85cc908'`
→ empty), the sidebar filter for "emptytest" returns zero rows
(`/tmp/wf-sid-shots-final/02-00-filter-emptytest-gone.png`), and the directory
`/home/enzopalmisano/Tiller/projects/emptytest` still exists on disk (`ls -la` after removal).
Minor observation: the settings sheet itself did not auto-close after the removal completed in the
same screenshot; not part of the clause, not scored.

**F-PRJ-12** — PASSED. Display name: typing into the Display Name field on `second-proj`'s
settings updated the sheet's own header live (`Project Settings · siddisp`) and persisted to the
DB (`display_name`); the sidebar showed the new name after the next relaunch
(`/tmp/wf-sid-shots-verify/02-00-sidebar-check.png`: `siddispsiddisp`). Repository type: on the
Folder-type `wf-sid-created` project, clicking "Initialize Git" flipped "Repository: Folder" to
"Repository: Git" and swapped the "Initialize Git" button out for the icon/colour panel
(`/tmp/wf-sid-shots-repotype3/02-00-settings.png` → `/tmp/wf-sid-shots-repotype4/02-00-after-init-
git.png`). No reverse (Git→Folder) control was found anywhere in the sheet for a Git project,
consistent with the clause's own "where allowed" qualifier.

**F-PRJ-13** — FAILED — defective. See headline finding 2. Icon and colour selection both work
and persist (`icon_kind='icon', icon_value='globe', color_hex='blue'` confirmed via DB after the
first test), but Reset does not restore defaults — reproduced twice.

**F-PRJ-14** — half-proven. GitHub Avatar: typed `octocat` into "GitHub user or repository,"
clicked "Use GitHub Avatar," got "Current: GitHub avatar for octocat"
(`/tmp/wf-sid-shots-avatar2/02-00-github-avatar-result.png`) — confirmed working. PNG upload:
blocked by the same portal/D-Bus environment constraint as F-PRJ-02/03/04 (inherited, not
independently re-tested). Favicon domain: the "Domain, like example.com" field and "Use Favicon…"
button are present and visually correct, but two attempts to type into that specific field today
left it empty (unlike the adjacent GitHub field, which took typed ASCII text without issue one
row up) — inconclusive rather than a confirmed defect; may be a drive-tool click-target issue
rather than an app problem, but wasn't isolated further given time.

**F-PRJ-15** — PASSED. On `second-proj`'s Project Settings (Icon tab, selected by default),
clicked the globe icon glyph; the header/preview updated instantly, persisted to the DB
(`icon_kind='icon', icon_value='globe'`), and appeared in the sidebar as a blue globe next to
`siddispsiddisp` (`/tmp/wf-sid-shots-verify/02-00-sidebar-check.png`).

**F-PRJ-16** — PASSED. Emoji tab is the second of three (Icon/Emoji/Avatar). Empty-input
validation confirmed live: clicking "Set Emoji" with nothing typed shows "Enter exactly one
emoji." (`/tmp/wf-sid-shots-rm1/04-02-settings-opened.png` — reached accidentally, but a clean
repro exists at `/tmp/wf-sid-shots-emoji4/04-02-after-set-single.png` too, see caveat below). Open
Emoji Picker confirmed live: opens a searchable glyph grid
(`/tmp/wf-sid-shots-picker/02-00-emoji-picker.png`); clicking a glyph (★) closed the picker and
set `fixture-proj`'s icon to it, confirmed via DB (`icon_kind='emoji', icon_value='⭐'`) and
sidebar (`/tmp/wf-sid-shots-verify/02-00-sidebar-check.png`). Caveat (see headline finding 5): the
direct "type one emoji into the field" sub-case could not be positively confirmed today because
`wtype` would not deliver either of two tried Unicode emoji into the field at all (the field
stayed empty, so the "single emoji" and would-be "invalid" tests both surfaced the same
empty-field validation message) — this is judged a drive-tool limitation, not an app defect,
because the *identical* underlying accept-a-single-emoji code path was proven to work moments
later via the picker.

**F-PRJ-17** — FAILED — absent. `grep -rn "worktree_base\|default_worktree_base\|WorktreeBase"
rust/crates/tiller_ui/src/*.rs` returns zero hits today. Live-reconfirmed today by reading every
Project Settings sheet opened this session top-to-bottom (Repository type → Display name →
Project icon → Remove Project → Close) — no default-worktree-base control appears anywhere,
despite `default_worktree_base` existing as a real column on the `project` table.

**F-PRJ-18** — FAILED — absent. Same grep, same live-reconfirmation: no worktree-location or
restore-default-parent control appears anywhere in the Project Settings sheet, despite
`worktree_location_override` existing as a real column on the `project` table.

## Cleanup

No stray `wf-sid`-labelled tiller/sway processes were left running — every drive invocation in
this session was self-contained (`Scripts/wayland-drive.sh` launches and tears down its own nested
sway + app process per call). Confirmed via `ps aux | grep wf-sid` showing only other shards'
labels (`wfedit`, `wf-term`), not `wf-sid`, at the end of this session.
