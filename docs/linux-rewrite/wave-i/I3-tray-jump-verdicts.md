# I3-tray-jump critic verdicts

Slice: I3-tray-jump. Row under review: `F-USE-05` (worst-status-tab jump half; the
select-worktree half was already independently PASSED in sweep H3-tray).

## F-USE-05

**Verdict: PASSED**

### What I checked myself, not transcribed

- Read `crates/tiller/src/main.rs` directly: `worst_status_tab_id` (line ~4360),
  `select_worktree_and_jump` (~4390), `control_select_worktree_and_jump` (~4647,
  the `tray.jump` control method), and the real D-Bus tray arm
  `tray::TrayRequest::SelectWorktree(path)` (~3295-3301). Confirmed the real
  D-Bus click handler calls `workspace.select_worktree_and_jump(path, cx)` --
  the exact same method `tray.jump` drives. Not a token call site or a fix on
  the wrong path: it is one function, two callers.
- Ran `cargo test -p tiller tray_jump_lands_on_the_target_worktrees_worst_status_tab`
  fresh, independent of the builder's run: `ok`, 1 passed.

### Live drive (independent, single wayland-drive.sh invocation, fresh scratch repos)

`TILLER_WL_LABEL=critic-i3jump-b`, own `/tmp/critic-i3jump-repo1` and `-repo2`
(never used by the builder). Sequence: add both repos, select repo1, `tab.select
index=1` (forces Chat frontmost, `panel.list` confirms pane-0 active:true,
pane-1/Terminal active:false -- the opposite of the builder's starting state),
`notify session=pane-1 status=needs-input`, screenshot (Terminal tab shows a red
needs-input dot), `workspace.select workspace=repo2`, screenshot (sidebar now
highlights repo2, but the center pane still shows the *same* Chat/Terminal tabs
with the red dot still on Terminal -- confirms the single-flat-tab-list
constraint live), then `ctl tray.jump workspace=repo1` -- reply
`jumped:true tabId:1 tabTitle:"Terminal"` -- and a final screenshot shows the
Terminal tab now frontmost (Chat no longer shown), sidebar back on repo1.
Follow-up `panel.list` confirms the flip discriminatively: pane-1 (Terminal)
`active:true`, pane-0 (Chat) `active:false` -- the reverse of the pre-jump
state I captured a step earlier, so the flip can only have come from the jump
call, not from a default.

Also independently reproduced the model constraint the builder's report
documents: `panel.list worktree=/tmp/critic-i3jump-repo1` returned `[]` right
after switching to repo2 (the old path's entries are cleared and never
replaced), and `panel.list worktree=/tmp/critic-i3jump-repo2` returned the
*same* Chat/Terminal pane ids that were repo1's -- `self.tabs` is a single
window-wide list, not hydrated per worktree. This is the same "single-open-
worktree model" caveat under which `F-CHG-19` already sits at PASSED in the
ledger (line 210), so it does not sink this row either -- it is a pre-existing,
already-accepted architecture limit, not a defect this row introduces or hides.
`tray.jump` is honest about it in its own doc comment.

### Verdict rationale

Both halves of F-USE-05 are now independently proven live: select-worktree
(sweep H3-tray, unchanged) and worst-status-tab jump (this pass, fresh repos,
fresh test run, fresh screenshots, a marker-flip an idle default could not have
produced). The control-socket door (`tray.jump`) is not a parallel test-only
stand-in -- reading the source confirms the real tray click arm calls the exact
same function -- so this drive proves the production path, not a substitute
for it.
