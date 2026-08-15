# D-MAIN-6 critic verdicts

Critic pass, independent of the builder. HEAD at verify time: `45600761b4996804fd43b41cc6d22f15901cbb5a`
(post wave-D integration, on top of `4560076`). Instruments: `Scripts/wayland-drive.sh` (labels
`critD6a`..`critD6g`), live `project.add`/`worktree.set` over the raw control socket, a real full
process restart against the same `TILLER_DB` for the persistence half of `F-SID-11`, precise
pixel-cropping (`convert -crop`) of captures to confirm cursor-on-target before trusting a click's
absence of effect, and direct reads of `sidebar.rs`, `tab_machinery.rs`, `main.rs`, `context_menu.rs`
and `session.rs`.

The shared machine was under severe contention throughout (`uptime` load average 60-110, heavy
swap) — many `wayland-drive.sh` invocations crashed mid-script, produced blank first frames, or
silently restarted the instance between calls (kept-alive `TILLER_WL_KEEP` sessions kept losing
their `kill_ours` race). Two testing-methodology traps cost real time and are recorded so the next
critic doesn't repeat them: (1) `pointer_command`'s `motion_absolute` fraction is computed against
whatever `OUTPUT_W/OUTPUT_H` the **most recent `shot` call** set, not the resolution of whichever
screenshot you happen to be reading coordinates off — coordinates read from an out-of-sync frame
silently click the wrong element with no error. (2) splitting a right-click and its follow-up
item-click across **two separate script invocations** unreliably drops the click (proven by a
positive control below); doing both inside one `eval` block is reliable.

## `F-TAB-13` — PASSED

Live, single-invocation drive (`critD6f`): `shot` (normalizes to 1715x972) → `rightclick 536 65` on
the `Terminal` tab strip entry (2 tabs open, 1 pane group) → `shot` (confirms the menu, calibrates
the next click at 1400x900) → `click 510 432` on the now-enabled "Move to New Pane" item, all
inside one action block. Result: the `Chat` tab strip entry vanished entirely (only `Terminal`
remained in the strip — the exact "row disappears from the original strip position/count" the
report describes) and the content area re-rendered as a real two-region split. This discriminates
cleanly from doing nothing, since the default state always shows both tabs.

A first attempt at this same click, done by splitting rightclick and click across two separate
`wayland-drive.sh` invocations, showed **zero effect** twice in a row, including on a `Close` item
used as a positive control (which also silently no-op'd across two invocations, then correctly
opened a real "Close dirty tab?" confirm dialog once redone as a single-invocation click) — this
was a testing-methodology artifact (see traps above), not a defect in the app; recorded here so it
isn't mistaken for one on re-verification.

## `F-SID-12` — PASSED

Live, Wayland lane (`critD6b`). `rightclick` a non-primary worktree row (`linux/gpui-waku`): the
context menu's **first** item is "Set Primary", no `ctrl-shift-p` chord anywhere in the path,
confirming the standing chord-requirement finding was purely the previous lane's missing
right-click primitive. Clicking it moved the `Primary` badge from `rust/gpui-rewrite` onto
`linux/gpui-waku` in the next captured frame — discriminating, since the default/idle state always
shows the original worktree as Primary.

## `F-SID-15` — FAILED — defective

Live, Wayland lane (`critD6a` and `critD6b`, two independent app instances, same result both times,
single-invocation `rightclick`+`click` each time — not the cross-invocation trap above). The
"Remove Worktree" item now exists in the menu (confirmed) and `sidebar.rs`'s confirm-gate code
(`request_remove_worktree_row` → `window.prompt`) reads correctly. But clicking directly on its
label does not reach it: pixel-cropping the result shows the cursor precisely centred on "Remove
Worktree" with hover styling active, the context menu still fully open (it never called
`dispatch_context_action`, which unconditionally clears `context_menu` as its first line), and
instead the "+ New Worktree…" row — a normal, always-present sidebar row positioned directly
beneath the menu's last item — opened its own branch-name creation form. Reproduced identically
twice, in separate processes, with the click landing squarely inside the item's label text both
times. `render_context_menu` (`sidebar.rs:1989`) is not wrapped in `deferred(...)`, unlike the tab
context menu a few hundred lines away, which is explicitly commented as needing that wrapper for
correct click routing over a later-in-tree sibling. No confirm dialog ever appeared. The destructive
action remains functionally unreachable via the one route the row is graded on, even though the
code that would make it safe is present and well-written.

## `F-SID-11` — half-proven

**Folder-worktree row: confirmed working, live.** `project.add` on a plain non-git folder
(`/tmp/critD6-plainfolder`, `worktreeCount:1` in the response) rendered a real worktree row —
path and a `Primary` badge — under the project card, where the standing evidence said no row
existed at all. Discriminating: the default/absent state (no row) is categorically different from
a rendered row.

**Comment: confirmed NOT working, live, twice.** `ctl worktree.set worktree=<path>
comment=CRITD6COMMENTMARK` returned `ok` with the comment echoed back. Two independent
forced-repaint captures of the row afterward (pixel-cropped) show no comment text anywhere near the
path/Primary badge. Killed the instance and relaunched the real `tiller` binary against the same
`TILLER_DB` (a genuine restart, not a re-render) — the comment still does not appear on reload
either. Reading `main.rs`'s `worktree.set` handler confirms why: it calls `persist_worktree_comment`
and updates `self.state` (the control-socket's own bookkeeping) but never calls `refresh_sidebar`
or otherwise pushes the new comment into the already-materialized `Sidebar` entity's row list, and
it isn't present after a fresh load either — the comment is accepted and appears to persist to the
control layer, but the sidebar never renders it under any condition tested. This contradicts the
integrator's "Verified" note for this half of the fix.

## `F-TAB-01` — FAILED — defective (unchanged, own reproduction)

No commit touched `right_panel.rs` this wave (`git log 5626555..HEAD` for that path shows only an
unrelated `F-CORE-FILE-03` drag-source commit). Live redrive (`critD6c`), careful to keep the
expand-click and the follow-up capture at matched resolutions: expanding "docs" in the Files panel
produced a transient full-panel "Loading files…" state and then **settled back collapsed** —
worse than the previously recorded manifestation (parent collapses only on a *child* click); here
it could not even be held open long enough to attempt the child-click gesture the row describes.
Consistent with the standing `FAILED — defective` verdict; root cause still unconfirmed.

## `F-TAB-08` — UNREACHABLE

`tab_bar.rs`'s `render_chat_empty` now takes `entity: Entity<Self>` and wires `.on_click` to emit
`TabBarEvent::OpenAgentSettings` (commit `7517594`), and `main.rs` subscribes to it and calls the
already-working `workspace.open_settings(Some(SettingsCategory::Agents), cx)` — this reads as a
complete, correctly-wired path. Could not drive it live: the row requires a Chat tab opened with
**no supported agent found on PATH**, and this environment has Claude Code and Codex genuinely
installed (visible in the status bar and the New-tab agent menu throughout every capture this
session), with no documented lane mechanism to override the driven instance's `PATH`. Not
exercised for lack of a reachable precondition, not for lack of time; do not read the code-level
soundness above as a substitute for a live verdict.

## `F-TAB-11` — FAILED — absent (unchanged)

No commit touched `context_menu.rs`, `tiller_terminal/src/lib.rs`, or `panes.rs`'s
`split_disabled_reason` wiring this wave (only an unrelated test-only commit touched
`tiller_terminal/src/lib.rs`). Independently confirmed via source: `TerminalContextItem` still has
exactly three fields (`label`, `action`, `route`), `ITEMS` is still a flat compile-time
`const [TerminalContextItem; 12]`, and `panes.rs::split_disabled_reason` remains an
`#[allow(dead_code)]` function with zero callers. The disabled-with-reason capability the row is
graded on does not exist in any form; matches the standing verdict.
