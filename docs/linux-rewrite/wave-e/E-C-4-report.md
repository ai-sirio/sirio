# Wave E slice E-C-4 — report

## `F-SID-15` — fixed

Root cause confirmed exactly as recorded: `render_context_menu` (`sidebar.rs`) painted the
sidebar context menu inline in tree order, so later siblings in the sidebar's own child list
(the always-present "New Worktree…" row) painted on top of it and intercepted clicks aimed at
items positioned near the bottom of the menu, such as Remove Worktree — even though the menu was
visibly drawn above them. Wrapped the menu in `deferred(...).with_priority(1)`, the identical
pattern already used for the tab context menu (`main.rs::render_tab_context_menu`) and documented
there. Added the `deferred` import.

Live-verified end to end (Wayland lane, `TILLER_WL_LABEL=ecfour3`): `project.add` this repo,
`rightclick 150 222` on the `linux/gpui-waku` worktree row, `click 82 340` on "Remove Worktree" —
the real confirm dialog ("Remove worktree? This permanently deletes the worktree's directory…",
Remove Worktree / Cancel) appeared centred over the whole window. Did not click through to actual
removal. A same-invocation attempt without a settling `shot` between the rightclick and the click
reproduced the *old* symptom (menu re-opened, New Worktree form appeared instead) — that is the
lane's own documented resize-jiggle confound (`shot` toggles output resolution to force a
repaint), not a regression; the stabilized sequence is the valid reproduction.

`howToExercise`: `rightclick` a worktree row to open its context menu, `shot` to let the frame
settle, then `click` squarely on "Remove Worktree" (the last item). A real "Remove worktree?"
confirm dialog with Remove Worktree/Cancel buttons should appear centred over the window; the
context menu itself must close.

## `F-TAB-01` — fixed

Root cause was not the click-routing confusion recorded from wave C/D — that finding is
superseded. Live redrive at HEAD showed a *new, cleaner* symptom exactly matching the wave-E
critic note: expanding `docs` in the Files panel flashed "Loading files…" and settled back
collapsed, with no click on a child ever needed to trigger it. Traced to
`RightPanel::refresh()`: the periodic 1s auto-refresh loop (`ensure_tree_refresh`) reruns
`refresh()` every tick, and `refresh()`'s success arm replaced `self.file_tree` wholesale with a
freshly-walked, single-level (non-recursive) listing where every node defaults to
`expanded: false, children: []`. Any folder the user had expanded therefore silently collapsed —
and its already-loaded children were discarded — on the very next periodic tick.

Fix: added `preserve_expansion(old, new)` in `right_panel.rs`, which carries a matching old
node's `expanded` flag and already-loaded `children` forward onto the corresponding freshly-walked
node, and used it in `refresh()`'s `Ok` arm instead of the raw assignment. Covered by a new test,
`a_periodic_refresh_does_not_collapse_an_expanded_folder`, which expands a folder, drives a second
`refresh()` (simulating the periodic tick), and asserts the folder stays expanded with its
children intact. All 18 `right_panel` tests pass.

Live-verified (Wayland lane): expanded `docs`, waited 2.5s (spanning multiple periodic-refresh
ticks — confirmed by the status-bar clock and Codex usage percentage both advancing between
frames), and `docs` was still expanded with `linux-rewrite`/`superpowers`/`visual-reviews` visible.

`howToExercise`: open a project's Files panel, click a folder row to expand it (e.g. `docs`), wait
2+ seconds without touching anything else, and confirm it is still expanded — before the fix it
silently collapsed on the next periodic refresh tick.

## `F-TAB-11` — blocked (architectural gap, not a one-line fix)

Re-confirmed via source read that the standing diagnosis still holds exactly: `TerminalContextItem`
(`tiller_terminal/src/context_menu.rs`) has only `label`/`action`/`route`; `ITEMS` is a flat
`const [TerminalContextItem; 12]`, so it cannot vary per-invocation even in principle without a
signature change; `split_disabled_reason` (`tiller/src/panes.rs`) is a fully-implemented, fully
tested pure function with zero production callers.

Did not attempt a partial wire-up, for the same reason the prior wave declined to: a half-threaded
`enabled`/`disabled_reason` that isn't backed by a real geometry check would be worse than the
honest absence — a critic driving it would find items "enabled" for a reason that was never
actually computed. Two things block a real implementation, not one:

1. **No pane pixel-bounds tracking exists anywhere in the model.** `panes.rs`'s `PaneSize` is a
   pure value type used only by `split_disabled_reason`'s own unit tests; nothing in `main.rs`'s
   `render_pane_tree` captures a leaf pane's actual laid-out size (there is no `canvas()`/bounds
   cell for pane leaves the way `tab_bar.rs`'s `anchor_bounds` captures the new-tab button). This
   would need to be added and threaded into `TerminalView` via a setter (e.g.
   `set_split_availability(horizontal: Option<String>, vertical: Option<String>, cx)`), called each
   render once bounds are known, guarded against redundant `cx.notify()` the way
   `right_panel.rs::set_activity` already does.
2. **The `tab_count` parameter's semantics need product clarification, not just wiring.**
   `split_disabled_reason(direction, size, tab_count)` returns `SoleTabInGroup` when
   `tab_count == 1`, and its own test names this "the sole tab cannot create a pane group split."
   Taken literally against `main.rs`'s model, the only `usize` in scope at a menu-open call site
   that could plausibly be called `tab_count` is `self.tabs.len()` — the number of *workspace*
   tabs, not panes within the one being split. Wiring that in as-is would disable every pane split
   whenever a fresh workspace has exactly one tab open (the common case), which reads as a product
   regression, not a completed feature. Before wiring this parameter to anything, whoever owns
   this row next needs to confirm from the Swift original or the fable spec whether "sole tab in
   its pane group" means "this tab's own pane-group has only one pane" (in which case the real
   caller-side count is the number of leaves in `PaneNode<TabContent>` for *this* tab, computed via
   a leaf-counting walk over the tree — straightforward once bounds tracking above exists) or
   something else. Wiring the wrong one produces a plausible-looking but behaviorally wrong
   feature that a live drive would immediately catch.

Recommended real shape of the work once semantics are confirmed: (1) add a leaf-count helper to
`panes.rs` alongside `split_disabled_reason`; (2) add a bounds-capture `canvas()` per pane leaf in
`main.rs::render_pane_tree`, feeding a `Cell<Option<Bounds<Pixels>>>` per pane (mirroring
`tab_bar.rs`'s `anchor_bounds` pattern); (3) add `enabled`/`disabled_reason` fields to
`TerminalContextItem` and change `context_menu::items()` to `items_for(&SplitEligibility)`, a
small struct owned by `context_menu.rs` so `tiller_terminal` doesn't need to depend on `tiller`'s
`SplitDisabledReason` type; (4) at the terminal's context-menu-open call site in `main.rs`, compute
both directions' eligibility from the captured bounds + leaf count and call
`terminal.set_split_availability(...)` before opening.

`wantedForeignFiles`: none beyond the four already owned by this row — the gap is real work inside
them, not a change that needs to land elsewhere.

## `F-TERM-08` — verified live, no code change needed

Gating code (`pane_close_needs_confirmation`, `request_close_terminal_at`,
`confirm_pending_pane_close`) reads correctly on inspection and matches
`tiller_activity::ActivityStatus::requires_close_confirmation`'s rule (duplicated intentionally
across the `tiller_ui`/`tiller_activity` type boundary — see the comment at `main.rs:2443`).

Live-verified end to end (Wayland lane): `ctl notify session=pane-1 status=running` (confirmed by
the red tab dot and "1 running" in the Activity footer), `rightclick` the pane to open its
12-item menu, `click` "Close Terminal…" — the real "This pane has running work. Close anyway?"
banner appeared with Close Anyway/Cancel controls. The standing `half-proven` was correctly
diagnosed by the wave-D critic as inconclusive under load contention (~75 load average) rather
than a real defect: at ~20 load average the same gesture worked, though the `rightclick` itself
still silently no-op'd on roughly half of individual attempts (a documented lane trap, not an app
symptom — a second `rightclick` at the same coordinates reliably opened the menu when the first
one dropped).

`howToExercise`: `ctl notify session=<pane-id> status=running` on a terminal pane, then
`rightclick` it (retry once if the menu doesn't appear — a known lane input-drop, not a defect)
and `click` "Close Terminal…". The pane must show the "This pane has running work. Close anyway?"
banner rather than closing immediately.

## `F-TERM-UI-01` — no regression found; still not exhaustively exercised

Independently reconfirmed the 12-item menu (Copy, Paste, Copy Context, Set Title, Copy Pane ID,
Copy Terminal ID, Split Left/Right/Above/Down, Clear Terminal, Close Terminal…) renders completely
and correctly across three separate live captures this session. Attempted to additionally exercise
"Clear Terminal" (typed `echo hello-world-marker`, confirmed it in scrollback, reopened the menu,
clicked the item): the click landed exactly on the item with hover styling active in the capture,
but the following frame still showed the menu open and the scrollback unchanged — indistinguishable
from the same click-vs-repaint race already documented for this lane (`F-TAB-01`'s wave-D note,
`F-SID-15` above) rather than a reproducible defect, given the item's `on_click` handler
(`terminal.handle_context_action`) is identical in shape to the Split/Copy handlers already proven
by other rows. Did not have budget to chase a stable repro of that specific race to a conclusion.

`howToExercise`: right-click a terminal pane (retry the rightclick once if the menu doesn't open —
lane trap, not a defect), then separately drive each of Copy, Copy Context, Set Title, Copy
Terminal ID, Split Above, Split Down, and Clear Terminal, inserting a `shot` between the rightclick
and the click so the frame is settled before the click coordinate is computed. Clear Terminal and
Split Above/Down have a directly visible pass/fail signature (scrollback empties; pane geometry
splits); Copy/Copy Context/Copy Terminal ID need a paste-back into the same pane to observe.
