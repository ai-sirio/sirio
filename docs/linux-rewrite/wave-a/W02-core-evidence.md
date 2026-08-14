# W02-core evidence log

Slice: `docs/linux-rewrite/wave-a/W02-core.md`. One section per row, appended as each row
finishes (never batched). Lane: Wayland only (`TILLER_WL_LABEL=wavea-W02-core`).

## `F-CORE-AUTH-01`

**Claim:** could-not-reach.

**Drove:** Reviewed `AgentAccountIdentity::parse_claude_json` (tiller_usage/src/account.rs:52)
and its caller at `tiller_ui/src/settings.rs:548`. Confirmed the prior pass's half-proof
(spawn + PKCE URL) is real per the code path — `settings.rs:548` genuinely feeds
`claude`'s real stdout into the parser, no mock in between. Did not attempt to carry the
OAuth flow to completion: doing so requires a real `claude` account, a real browser
completing the PKCE authorize redirect, and pasting back a real authorization code — none
of which a scripted Wayland-lane drive can produce without live credentials belonging to a
real account. Forcing it through with throwaway credentials risks writing bogus
`~/.claude` state that could affect the shared machine outside this worktree, which the
task charter's "record what you saw" instruction does not license.

**Observed:** No new capture. The half-proof already on record (PKCE URL confirmed live)
stands; `parse_claude_json`'s parse of genuine completed-login JSON remains undriven.

**Verdict left as:** half-proven, unchanged — this pass could not add the missing half.

## `F-CORE-DOM-03`

**Claim:** could-not-reach.

**Drove:** Clicked the sidebar's "+" Add Project affordance (`add-project` at sidebar
top-right, `sidebar.rs:2419`), then clicked "Open Project…" in the menu that renders at
`top:30 right:12` (`sidebar.rs:1746`), which calls `start_open_project` ->
`cx.prompt_for_paths` (`project_identity.rs:491`). Captured before/after
(`06-before-open.png`, `07-after-open-click.png`) and additionally queried
`swaymsg -t get_tree` on this lane's own nested compositor immediately after the click.

**Observed:** The nested compositor's window tree shows exactly one `con` (the Tiller
toplevel itself, `focused=true`) both before and after the click — no second toplevel, no
portal dialog window appeared anywhere in this instance's own tree. This reproduces
P120's finding on the current HEAD (`4073297`) rather than an old build: the button is
real (menu opens, item is clickable, `start_open_project` genuinely calls
`cx.prompt_for_paths`), but a real `xdg-desktop-portal` file-chooser call, per
`ENVIRONMENT.md:77`, can plausibly be routed to and rendered by the **host desktop's**
portal backend rather than this lane's own isolated compositor — in which case it would
never appear in this lane's tree or `grim` capture even if it fired and opened
successfully. This lane cannot distinguish "the portal never opened" from "the portal
opened invisibly on the host desktop", and the task charter restricts this slice to the
Wayland lane only (no `DISPLAY=:1`, no host-desktop interaction) — so resolving the
ambiguity is out of reach from here, not a defect finding.

**Captures:** `reference/linux-progress/wavea-W02-core/02-06-before-open.png`,
`03-07-after-open-click.png`, `02-08-tree-check.png`.

**Verdict left as:** NOT EXERCISED, unchanged — instrument ambiguity persists on current HEAD.

## `F-CORE-FILE-03`

**Claim:** could-not-reach.

**Drove:** Confirmed `terminal_file_drop`/`classify_file_drop` (`tiller_project/src/file.rs`)
is a real consumed data layer per DEAD-MODULES.md's correction. Checked
`Scripts/wayland-drive.sh`'s action vocabulary (`ctl`, `click`, `move`, `type`, `key`,
`title`, `shot`) for a drag primitive: there is none — no press-hold/motion/release
sequence, only an instantaneous `click`. XDND drag-and-drop cannot be synthesized with what
this lane exposes, matching `ENVIRONMENT.md:76-78`'s documented limitation.

**Observed:** No capture attempted; this is a tooling gap, not a code question — the drive
lock (`DISPLAY=:1`) is also off-limits to this slice, and even that lane's own docs note it
lacks a press/motion/release primitive for the virtual pointer.

**Verdict left as:** NOT EXERCISED, unchanged — needs a human hand or a different drive tool.

## `F-CORE-FILE-06`

**Claim:** partially-exercised.

**Drove:** Added a scratch git repo (`/tmp/w02file06-repo`, one file `testfile.txt`,
outside this worktree — never touched anything under `rust/`), selected it as the current
workspace over the socket (`workspace.select workspace=<scratch-wt-id>`), then double-clicked
its file row in the real Files panel (`file-row` at `right_panel.rs:495`, reached at the
computed on-screen position `TITLE_BAR_HEIGHT(32)+HEADER_HEIGHT(40)+TOOLBAR_HEIGHT(34)/2` from
`main.rs`/`right_panel.rs`'s own layout constants) to drive `open_file` -> `add_file_tab` — this
is genuinely `FileView`, not the Changes panel: opening it visibly replaced the terminal content
with a distinct render (colour count and stddev of the main content region both dropped sharply,
consistent with plain editor text replacing a terminal).

Read `Editor::check_external`/`MarkdownDocument::refresh_from_disk`
(`tiller_ui/src/editor.rs:547`, `tiller_markdown/src/document.rs:92`) first: a clean (non-dirty)
buffer **silently reloads** on an external disk change and reports `Conflict::None` — the banner
is design-reserved for the case where local edits and a disk change collide. So I drove two
distinct gestures:

1. **Clean-buffer external edit** (`echo >> testfile.txt` with no local edits made in the app):
   captured before/after at matching resolution (1715x972) — pixel-identical in the editor
   content region (`md5` equal on that crop), confirming the monitor's silent-reload path fires
   without a banner, exactly as the code predicts. This is real evidence the
   `FileSystemEventMonitor` -> `poll_file_system_events` -> `check_external` pipeline is live
   and reacts to a real out-of-band inotify event — the thing the ledger's stale "not
   exercised" verdict and the triage note's overclaim-correction both call for.
2. **Dirty-buffer external edit** (clicked into the editor, `type LOCALEDIT_MARKER2`, then the
   same external `echo >>`) to try to hit the `ChangedOnDisk` conflict-banner branch: inconclusive.
   A large visual diff appeared between the pre-type and post-type captures (consistent with
   typed text landing), but the post-external-edit capture was pixel-identical to the
   pre-external-edit one in the editor content columns — no banner rendered. I cannot rule out
   that the click into the editor missed `file-editor`'s focus target (a blind coordinate click,
   not confirmed by `debug_bounds` at runtime) and the buffer was never actually marked dirty,
   versus a genuine gap in the conflict path.

**Observed:** silent-reload-on-clean-buffer path confirmed live; the `ChangedOnDisk` banner
branch (dirty buffer + external edit) not confirmed — inconclusive, not disproven.

**Captures:** `reference/linux-progress/wavea-W02-core/02-20-editor-open-a.png`,
`04-22-after-edit-a.png` (clean-buffer pair, identical), `02-26-dirty-b.png`,
`04-28-conflict-b.png` (dirty-buffer pair, inconclusive), plus intermediates 09/10/11/13-19/23-25.

**Verdict left as:** half-proven (upgraded from NOT EXERCISED) — the silent-reload half is now
real evidence the pipeline this row cares about is live and distinct from the Changes panel; the
conflict-banner half remains owed.

## `F-CORE-TERM-02`

**Claim:** could-not-reach.

**Drove:** Confirmed in the current source that `open_context_menu` is wired only to
`MouseButton::Right` (`tiller_terminal/src/lib.rs`) with no keyboard-triggered path anywhere
in the crate. `Scripts/wayland-drive.sh`'s `pointer_command`/virtual-pointer client only
sends a left-button `click` (see `wayland-virtual-pointer.c` invocation) — there is no
right-click primitive on this lane. Per the task charter this slice is restricted to the
Wayland lane only (`DISPLAY=:1` and `linux-drive.sh` are explicitly off-limits here), and
that is the only lane `WAYLAND-LANE.md` documents as supporting a real right-click gesture.

**Observed:** No new capture. This reconfirms the row's own prior evidence
(`p17-rclick-term.png`, per-item effects already unit-tested from pass 12) rather than
attempting a gesture this lane cannot produce.

**Verdict left as:** half-proven, unchanged — the per-item effects remain reachable only on
the `DISPLAY=:1` lane, out of scope for this slice.
