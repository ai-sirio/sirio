# P106 report — exercising the miscalled rows

Brief: `tasks/P106-exercise-the-miscalled.md`. Observations only — **no verdicts**; a critic
sets those. Captures live in `reference/linux-progress/p106-fable-*.png` (fable's) and are cited
by filename. Lane for the fable slice: Wayland (`Scripts/wayland-drive.sh`, label `fable`,
display `wayland-3`, socket `/tmp/fable.sock`) — no drive lock, **no synthetic input**, so every
click/drag/chord below is recorded as owed, with the exact gesture.

One lane fact that shaped the evidence: `TILLER_DB=/tmp/fable.sqlite` persists across
`wayland-drive.sh` runs, so this instance **booted with tabs restored from a previous
fable-labelled session** (Chat + Terminal for `linux/gpui-waku`, with an empty project catalog).
Frames 01–03 show that inherited state, not a fresh app.

## fable — window / sidebar / tabs / changes (17 rows, censused by codex12)

### F-WIN-01 — workspace ⇄ Settings routes (census: BUILT)

Drove the state half over the socket:

- `ctl surface.settings.open` → full state reply (five sections: ai-providers, agents, general,
  permissions, appearance, plus values). **Settings replaced the whole workspace** — no sidebar,
  no tab strip; header reads `‹ Back  Settings` — `p106-fable-15-win01-settings-replaces-workspace.png`.
- `ctl tab.select index=2` from inside Settings → reply `ok {}` but the **Settings route stayed
  on top** — `p106-fable-16-win01-back-via-tab-select.png` (frame is fresh: forced repaint at a
  different resolution). No socket method exits Settings; once opened, every later capture is
  occluded until relaunch.
- After `system.quit` + relaunch, the app launched into the **workspace** route (Settings not
  persisted) — `p106-fable-17-win07-relaunch-autorestored.png`.

Chord needle, because the ledger's absence claim aged: `"ctrl-,"` **is bound** at
`tiller/src/main.rs:167` (plus a test that simulates it at :9254). The ledger's pass-12 evidence
"no ctrl-, chord in any crate" no longer describes today's tree.

Owed: gesture — press `ctrl-,` in a live window and confirm Settings opens; click `‹ Back` and
confirm the workspace returns (Escape is a second exit per the F-SET-02 comments).

### F-WIN-07 — restore previous launch (census: PARTIAL — restore built, History menu absent)

Built half, driven twice:

1. Same-launch cycle: `ctl workspace.close workspace=p-c1fd7a5bbfd541af-wt-0` →
   `{"closed":"true","path":".../tiller"}`; `workspace.list` then showed wt-0 `mounted:"false"`.
   `ctl session.restore` → `{"path":".../tiller-linux","restoredCount":"0"}` — and wt-0 **stayed
   unmounted** (`workspace.list` unchanged).
2. Quit/relaunch cycle: `system.quit`, relaunch on the same DB. The launch itself auto-restored
   **everything** — both worktrees `mounted:"true"` (including wt-0, which was unmounted at
   quit), all four tabs, and the Terminal tab's 4-pane split layout with fresh live shells
   (`panel.list` returned 8 panes) — `p106-fable-17-win07-relaunch-autorestored.png`. A second
   explicit `session.restore` again returned `restoredCount:"0"` —
   `p106-fable-18-win07-explicit-restore.png`.

For the critic: `session.restore` answered `ok` with `restoredCount:"0"` in both invocations,
including immediately after an unmount in the same launch, while the *launch path* demonstrably
restores. Whether the snapshot is consumed at boot or the explicit restore is a no-op needs a
reading of `restore_launch_snapshot` (main.rs:3921) against these replies. Not judged here.

Missing half, needle re-run: `grep -rniE "previous launch|history"` over `tiller`/`tiller_ui` →
no History menu, no "Restore Previous Launch" entry, no ⇧⌘O; the only hits are unrelated
(scrollback/chat-retention/browser history vec). Confirmed absent.

### F-WIN-10 — transient toasts (census: PARTIAL — sidebar notice built, toast absent)

Built half — **not reachable over the socket**, and the attempt is itself the observation:

- `ctl project.add path=/tmp/fable-nope` → error reply `cannot add /tmp/fable-nope: No such file
  or directory (os error 2)`; **no sidebar notice rendered** —
  `p106-fable-02-win10-invalid-path.png`.
- `ctl project.add <already-tracked path>` → `ok` with `added:"false"`, silently; no notice —
  `p106-fable-03-win10-duplicate-notice.png`.
- Cause, read not driven: the socket route is `control_add_project` (dispatched at
  main.rs:2653), a separate path from the UI `add_project` whose `Ok(false)`/`Err` arms call
  `sidebar.set_notice` ("already tracked or nested: …", main.rs:3226-3235). Every `set_notice`
  caller in the tree is behind a gesture: Add-Project picker events, sidebar context-menu
  actions (init-git, reveal), agent-launch failure, file-picker/save failure.

Owed: gesture — click the sidebar `+` → Add Project and pick an already-tracked folder (routes
to the duplicate notice), then observe the notice render and whether it ever dismisses.

Missing half, needle re-run: the only "toast" in the tree is the theme radius token asserted at
`tiller_ui/src/conformance.rs:156` — no toast surface, no auto-dismiss. Live corroboration: no
notice/toast element appeared in any of the 18 frames of this pass.

### F-SID-06 — project status badge (census: PARTIAL — worktree dot built, project badge absent)

Built half (worktree dot), driven through the full status cycle on the restored terminal pane:
`ctl notify session=pane-1 status=<s>` for running / needs-input / error / done (each replied
`queued:"true"`):

- `running` → **no dot** — `p106-fable-04-status-running.png`. Deliberate, not a gap:
  `sidebar.rs:2055-2065` maps `ActivityStatus::Running => None` under the comment "A dot only
  appears for a notable status — matching the reference".
- `needs-input` → amber dot — `p106-fable-05-status-needs-input.png`; `error` → red dot —
  `p106-fable-06-status-error.png`; `done` → dot in the `theme.tab_done` colour —
  `p106-fable-07-status-done.png`. (`set_worktree_status` → `status_dot_color` live.)

Missing half: the dot is computed under a `(kind == RowKind::Worktree)` gate — project rows
never get one. Live corroboration: the `tiller` project row shows no badge in any frame while
its child worktree carried every status (04–07). The clause's collapsed-project variant needs a
disclosure click this lane cannot deliver.

Owed: gesture — start activity, click the project row's chevron to collapse it, and record
whether any badge appears on the collapsed project row (per the code gate, none should).

### F-SID-11 — worktree row identity (census: PARTIAL — identity built, comment absent)

Driven: `ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux` →
`{"added":"true","projectId":"p-c1fd7a5bbfd541af","worktreeCount":"2"}`. Rows render:
project `tiller` + path; worktree `rust/gpui-rewrite` + path + **Primary** badge; worktree
`linux/gpui-waku` + path + status dot + nested tab rows (Chat/Terminal/Changes/Browser as they
opened) — `p106-fable-03-…png`, `p106-fable-08-…png`, `p106-fable-17-…png`. Branch names are the
row titles. Agent-status display: the dot cycle under F-SID-06.

The clause's *folder worktree*: `ctl project.add path=/tmp/fable-folder` →
`{"added":"true","worktreeCount":"0"}` — the folder project renders as a **bare project row
with no worktree child at all** (`fable-folder`, `/tmp/fable-folder`,
`p106-fable-17-…png`), so there is no folder-worktree row on which to inspect identity. On this
build the clause's folder-worktree half has no surface to exercise.

Missing half, needle re-run: no comment render anywhere in `sidebar.rs` (sole grep hit is a
doc-comment). Sharpened by the socket: `workspace.list` **does** expose a `comment` field
(`""` for both worktrees) — the model carries a comment; the row render never draws one.

### F-SID-15 — remove worktree from context menu (census: PARTIAL — hover × built, menu item absent)

Built half (the hover × route): not drivable on this lane — it is a hover + click. The control
exists in code: `remove-worktree-{row_id}` at `sidebar.rs:2272-2285` → `remove_worktree_row`
:1438 → `tiller_git::remove_worktree`. Owed: gesture — hover a worktree row, click the ×,
confirm the row disappears, and record whether **any** confirmation prompt appears (the pass-12
ledger note says this route confirms nothing).

Missing half, re-read today: the worktree arm of `context_menu_items` (sidebar.rs ~700-760)
carries Set Primary / Unset Primary and seven New-Tab actions (New Terminal, Claude Code, Codex,
OpenCode, Pi, Oh-My-Pi, New Chat) and ends there — **no Remove Worktree item**. Census needle
re-run agrees (no `remove.?worktree` label among menu items).

### F-SID-16 — reorder projects by dragging (census: BUILT)

The whole clause is a drag; this lane has no synthetic input (WAYLAND-LANE trap 3), so none of
it can be closed here. Driven precondition: **two project rows** exist and render (`tiller`,
`fable-folder`) — `p106-fable-14-sid16-two-projects.png`, `p106-fable-17-…png`.

Owed: gesture — press-drag one project row above/below the other in the projects list and
release; confirm the order changes and survives (census wiring: `Sidebar::row_drag`
sidebar.rs:527, `ReorderScope::Projects`, committed through main.rs:3269).

### F-SID-17 — reorder worktrees by dragging (census: BUILT)

Same lane limit. Driven precondition: project `tiller` lists two worktree rows
(`rust/gpui-rewrite`, `linux/gpui-waku`) — every sidebar frame from
`p106-fable-03-…png` onward.

Owed: gesture — drag `linux/gpui-waku` to `rust/gpui-rewrite`'s position within the same
project; confirm the order changes (census wiring: worktree drag scopes → `reorder_sidebar`
main.rs:3281).

### F-TAB-01 — tab strip decorations (census: BUILT)

Driven four tab kinds and the full status vocabulary:

- Kinds: restored Chat + Terminal; `ctl surface.changes.open` + `tab.select index=3` (Changes);
  `ctl browser.open url=https://example.com` + `tab.select index=4` (Browser). Four distinct
  per-kind icons render (speech bubble / terminal / diff / globe) —
  `p106-fable-08-chg01-changes-tab.png`, `p106-fable-09-tab01-browser-tab.png`.
- Status indicator (same notify cycle as F-SID-06): ○ idle (baseline), **●** amber running
  (`…04…png`), **?** needs-input (`…05…png`, strip), **!** error (`…06…png`, strip), **✓** done
  (`…08/09…png`). Vocabulary matches `tab_status_glyph` (main.rs:2285).
- Close control: renders **on the active tab only** (`.when(active, …)` main.rs:5716) — the ×
  follows the active tab across frames 03/08/09.
- Dirty indicator: red-orange ● rendered beside the Terminal tab — fed not by an edited
  document but by `tab_is_dirty` (main.rs:5766): "live terminals, streaming chats, and unsaved
  editors are dirty"; the restored terminal owns a live PTY. Photographed on the **inactive**
  Terminal tab (`…09…png`). Observation: in frames where Terminal is *active* (05/06) the
  status glyph + × render and **no dirty dot is visible**; the dot appeared only on inactive
  tabs in this pass. Whether active-tab dirty is swapped for the close control by design needs
  the reference; recorded as seen.

Owed: gesture — the clause's document tab (file open is a picker/Files-panel click) and the
modify-a-document dirty route (typing); neither document-kind icon nor unsaved-editor dirty was
exercisable here.

### F-TAB-11 — split disabled reasons (census: PARTIAL — model built test-only, UI absent)

Built conjunct: `split_disabled_reason` (panes.rs:218) **cannot be driven from the running
app** — re-verified today: its only references outside the definition are the `#[cfg(test)]`
module import (panes.rs:610) and tests. No production caller exists, so no gesture or socket
call can make it fire.

Missing conjunct, re-verified at the struct level: `TerminalContextItem` is
`{label, action, route}` (context_menu.rs:29-33) — **no disabled state, no reason field** — and
the menu items are static consts. Nothing renders a disabled split entry or its reason.

Owed: nothing exercisable — both the clause's "resize until ineligible" and "sole tab" trials
dead-end at a menu that has no disabled-reason surface. (The right-click to open the terminal
context menu itself is also input-gated on this lane.)

---

*(batch 2 — F-TAB-18/23/24/28, F-CHG-01/20, F-PER-07 — follows)*
