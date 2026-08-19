# wf-judge2 — fresh critic pass over wave N's six fixes

Lane `wf-judge2`. Fixed binary pinned at `/tmp/wf-judge2-tiller` (md5 `c12e759d`, built from
current HEAD `4fa499cc`). Pre-fix binary pinned at `/tmp/wf-judge2-tiller-PREFIX` (md5 `d370e815`),
built from `19c6a9e9` — the commit immediately before the first of the six fix commits
(`1bd009ef`..`c1f27dc5`), reconstructed with `git archive 19c6a9e9 | tar -x` into
`/tmp/wf-judge2/prefix-tree` (never `git checkout`/`stash`/`reset` on the working tree) and built
there with its own `cargo build --manifest-path rust/Cargo.toml -p tiller`. All six fix commits
are strictly independent (each touches only its own row's files plus the builder's own report),
confirmed with `git diff --stat <commit>^ <commit>` on all six before relying on one shared
pre-fix binary for every row's reproduction.

I am the critic, not the builder — none of `FIX-waveN-failures.md`'s six rows are self-promoted
here; every verdict below is this pass's own live drive, both sides (reproduce broken, confirm
fixed), per `EVIDENCE-STANDARD.md`.

---

## F-TAB-26 — Close Anyway on a tab's sole pane

**Verdict: PASSED**

### Code check against the report's claim

`git show 1bd009ef -- rust/crates/tiller/src/main.rs` matches the report's root-cause and fix
description exactly: `request_close_terminal_at` now computes
`let whole_tab = tab.panes.leaf_ids().len() <= 1;` and routes both the confirmed
(`PendingPaneClose { whole_tab, .. }`) and immediate no-confirmation paths through
`close_tab_by_id` when true, where before it always hardcoded `whole_tab: false`.

### Reproduced original defect (pre-fix binary, lane `wfj2pre26`)

Fresh boot, `project.add` on this repo, `panel.list` → `pane-0`/Chat, `pane-1`/Terminal (2 panes).
`ctl notify session=pane-1 status=running`, right-click the terminal, click "Close Terminal…" →
banner reads **"This pane has running work. Close anyway?"** (screenshot
`/tmp/wf-judge2/shots/pre26/03-04-confirm-banner.png`, terminal identifies itself as
`wf-judge2-tiller-PREFIX` confirming the binary under test). Click "Close Anyway" →
**hard discriminator**: the next `panel.list` is byte-identical to before —
`[{"id":"pane-0","tab":"Chat",...},{"id":"pane-1","tab":"Terminal","active":"true",...}]`, still 2
panes. The confirm banner dismisses; the tab and its sole pane are still fully present. Matches the
ledger's original finding and the report's reproduction exactly.

### Confirmed fix (current-HEAD binary, lane `wfj2tab26`)

**Sole-pane case (re-confirmed):** identical sequence on `/tmp/wf-judge2-tiller` — banner now reads
**"This tab has running work. Close anyway?"** (screenshot `02-...menu-open` /
`03-...confirm-banner.png` under `/tmp/wf-judge2/shots/tab26/`). After "Close Anyway", `panel.list`
drops from 2 panes to exactly **1** (`pane-0`/Chat only) — hard discriminator, plus a screenshot
showing only the Chat tab remains in the strip.

**Multi-pane case (the builder's own named gap, closed here):** in the same lane, `chord ctrl t`
opened a fresh Terminal tab (`pane-1`), right-click → "Split Right" made it two panes in one tab
(`pane-1` left, `pane-2` right — `panel.list` shows 3 panes total: Chat, Terminal/pane-1,
Terminal/pane-2). Marked `pane-2` `Running`, right-clicked it, "Close Terminal…", "Close Anyway" —
banner correctly reads **"This pane has running work"** (not "tab", since this pane is not its
tab's only one). `panel.list` before: 3 panes (`pane-0`, `pane-1`, `pane-2`). After: exactly **2**
(`pane-0`/Chat, `pane-1`/Terminal) — `pane-2` is gone, `pane-1` and the Terminal **tab** both
survive. Screenshot `/tmp/wf-judge2/shots/tab26/04-52-after-close-anyway.png` shows a single,
undivided Terminal pane — the divider is gone, confirming the tab did not close, only the one pane
did. This is exactly the multi-pane behaviour the fix's `whole_tab = leaf_ids().len() <= 1` guard
predicts and the builder's own regression-test sibling assertions already covered in-process; this
pass reproduces it live end-to-end for the first time.

**Trap avoided, recorded for whoever reads this next:** the confirm banner's "Close Anyway" button
sits at a **fixed screen position** (`(816, 504)` in this 1715x972 frame), not anchored to the
right-click coordinate that opened the context menu. A first attempt at the multi-pane drive
right-clicked at `(1000, 300)` and, wrongly assuming the banner position would shift with it,
clicked `(1116, 504)` — landing on empty background, leaving the banner open and unresolved.
Re-driving with the correct fixed `(816, 504)` in the same invocation worked; the earlier mistake
is left out of the evidence above and only noted here as a process trap, not a defect.

### Gap disposition

The builder's own named gap ("only the sole-pane path was re-driven live … the multi-pane case
rests on the regression test's sibling assertions") is **closed**: both paths are now independently
live-driven with a `panel.list` hard discriminator on each side of the fix.

---

## F-TAB-14 — double-click-to-rename + context-menu "silent fail"

**Verdict: PASSED** (double-click-to-rename built and live-confirmed; the context-menu claim is
independently re-confirmed as a harness timing artifact, not an app defect — matching, not
overturning, the builder's own conclusion)

### Code check

`git show f8a60bf3 -- rust/crates/tiller/src/main.rs` matches the report: the tab row's `on_click`
now matches on `ClickEvent::Mouse(mouse) => mouse.up.click_count`, calling `begin_tab_rename` when
`click_count >= 2` and `select_tab` otherwise, kept as a single listener (not a sibling
`on_mouse_down`) for the stated hitbox-nesting reason.

### Reproduced original defect (pre-fix binary, lane `wfj2pre14`)

Fresh boot, `project.add`. Double-clicked the Terminal tab (two real, separate `click 505 51`
calls). **Hard discriminator**: `panel.list`'s `title` field for `pane-1` stays `"Terminal"`
before and after; screenshot `/tmp/wf-judge2/shots/pre14/02-10-after-doubleclick.png` shows **no**
rename box of any kind appears — clean absence, not a race (contrast with the fix side below,
where the box always appears immediately). The double click only reselects, exactly as the ledger
and the report describe.

### Confirmed fix (current-HEAD binary)

First two attempts (lane `wfj2tab14`, `wfj2tab14b`) reproduced a *different*, harness-side failure
worth recording: the rename field visually opened on double-click every time, but typed text
(`type RENAMEDBYJUDGE` / `type keytest123`) landed nowhere — not in the field, not leaked to the
shell — even with a positive keyboard-alive control (`key z`, `type abctypecheck` both landed fine
against a focused terminal in the same lane). `uptime` read **load average 25–40 on 12 cores**
during these attempts (several other agents' builds/drives running concurrently, per
`ENVIRONMENT.md`'s own warning about this box). Lane `wfj2tab14d` repeated the identical gesture
with a **generous 2 s settle** before typing and 1 s before `Return` (plus a `PRECHECK` positive
control typed into the terminal first, confirmed via `panel.scrollback` before the rename attempt):
`panel.list` afterward reads `{"tab":"TerminalRENAMETEST","title":"TerminalRENAMETEST", ...}` for
`pane-1` — hard discriminator, corroborated by a screenshot showing the tab strip and sidebar both
reading `TerminalRENAMETEST`. **This pass's own false negative is the same class of harness trap
`WAYLAND-LANE.md` documents for deferred menus** (input arriving before a freshly-mounted focus
target finishes linking into the dispatch tree), now shown to bite a freshly-opened rename field
too under load — recorded here so the next critic does not mistake a short sleep for a defect.

### Context-menu "silent fail" — re-driven, conclusion holds

Per the report's own named gap ("if a critic can still reproduce the silent-fail with a real,
generous sleep … that would overturn this pass's harness-artifact conclusion"): lane `wfj2tab14f`,
right-clicked the Terminal tab, **2 s sleep**, clicked "Rename" (`483, 133` in the drawn menu),
**2 s sleep**, typed `CTXMENURENAME`, `Return`. `panel.list` → `title:"TerminalCTXMENURENAME"` —
the context-menu path committed cleanly. This does **not** overturn the builder's conclusion; it
reinforces it; the failure only ever reproduces at synthetic-input speeds no human gesture reaches.

### Gap disposition

Both named gaps are closed: (1) the double-click fix is independently re-confirmed (and its own
harness trap identified and documented); (2) the context-menu race was re-tried with a generous
sleep and did not overturn the harness-artifact conclusion — it reproduces only without one.

---

## F-TAB-24 — Escape mid-tab-drag restores the pre-drag order

**Verdict: PASSED** (core claim, 3+-tab case, and drop-clears-snapshot case all independently
live-confirmed; a fourth, broader claim about sidebar drags — not part of this ledger row — was
tested anyway and the builder's own characterization of it did **not** hold up live, recorded below
as an additional finding, not a change to this row's verdict)

### Code check

`git show 128a2ecf -- rust/crates/tiller/src/main.rs` matches the report: `TabDragSnapshot` (id order
+ active tab, taken when a tab drag starts) and `cancel_tab_drag` (an `on_action` listener for the
same `CloseSettingsSurface`/Escape action F-SET-02 uses, checked first) which restores `self.tabs`
from the snapshot and calls `cx.stop_active_drag`, returning `false` (falling through to F-SET-02's
own settings-close/propagate logic) when `tab_drag_snapshot` is `None`. The listener has to be an
`on_action`, not a raw key listener, because `Window::dispatch_key_down_up_event` (raw key dispatch)
is not reached while `cx.active_drag` is `Some`, while keybinding-matched action dispatch still is —
the report's stated reason, confirmed by reading `cancel_tab_drag`'s call site directly.

### Reproduced original defect (pre-fix binary, lane `wfj2pre24c`, preceded by `wfj2pre24`/`wfj2pre24b`)

First attempt (`wfj2pre24`) used only 2 intermediate `move` waypoints with no sleep between them and
showed **no** reorder even *without* Escape — a false-negative risk (gesture not registering at all,
which would make the defect look fixed on an unfixed binary purely from a weak drag recipe). Fixed by
adding more waypoints with `sleep 0.1–0.2` between each; `wfj2pre24b` (5 waypoints, no Escape,
positive control) then genuinely reordered the tabs, validating the recipe. `wfj2pre24c` repeated the
identical recipe *with* `key Escape` pressed mid-drag (button still held, before `up`): **hard
discriminator** — terminal self-identifies as `wf-judge2-tiller-PREFIX` (screenshot
`/tmp/wf-judge2/shots/pre24c/02-01-baseline-order.png`, baseline tab order **Chat, Terminal**); after
the Escape-mid-drag gesture, `/tmp/wf-judge2/shots/pre24c/03-02-after-escape-mid-drag.png` shows the
order **flipped to Terminal, Chat** despite the Escape press — the pre-fix binary drops the reorder
into place regardless of Escape, exactly as the ledger and report describe.

### Confirmed fix (current-HEAD binary, lane `wfj2fix24`)

Identical recipe on `/tmp/wf-judge2-tiller`: baseline **Chat, Terminal**
(`02-01-baseline-order.png`); after Escape-mid-drag, `03-02-after-escape-mid-drag.png` shows order
**unchanged, still Chat, Terminal** — hard discriminator, tab strip screenshot. A same-lane positive
control (`04-03-positive-control-no-escape.png`, identical waypoints, no Escape) shows the order
**does** flip to Terminal, Chat without the keypress — ruling out "gesture too weak to register" as
an alternative explanation for the fix-side result.

**3+-tab case (the builder's own named gap, closed here):** opened a third tab (`chord ctrl t`) in
the same worktree, giving **Terminal, Chat, Terminal** (`02-10-three-tabs-baseline.png`). Escape
mid-drag on the third tab: `02-20-3tab-escape-mid-drag.png` shows order **unchanged** (T, C, T). A
matched positive control without Escape (`02-21-3tab-positive-control-no-escape.png`, screenshot
caught mid-relayout per the `shot`-forces-a-real-resize trap but the tab strip itself is legible)
shows the order **does** change to **Terminal, Terminal, Chat** — confirming the drag registers and
that Escape, specifically, is what holds the 3-tab order in place.

**Drop-clears-snapshot case (the builder's own named gap, closed here):** in the same lane, after
letting the positive-control drag actually **drop** (committing Terminal↔Chat, order now T, T, C), a
*subsequent, unrelated* Escape press was sent. `03-22-escape-after-drop-noop.png` shows the order
**stays T, T, C** — the already-committed reorder is not spuriously reverted by a stray Escape after
the drag is over, confirming `tab_drag_snapshot` is correctly cleared on a successful drop and doesn't
linger to corrupt a later, unrelated Escape.

### Additional finding: sidebar drag (`ReorderScope::Projects`/`Worktrees`) — not part of this row

The report's own words on this: *"sidebar drags (`ReorderScope::Projects` / `Worktrees`) use a
structurally different preview/commit split (`Sidebar::preview_reorder`/`confirm_reorder`, which
*also* mutates its `rows` vector live on hover with no Escape handling of its own) — untouched by
this pass and not covered by the ledger row, but a critic auditing 'drag cancel' more broadly should
know it is not fixed there."* Code check confirms the claim's premise: `grep -rn stop_active_drag`
across every crate finds exactly one call site, inside `cancel_tab_drag`, gated on
`self.tab_drag_snapshot.is_some()` — `sidebar.rs` has no Escape or `active_drag` handling of its own
at all.

Live-driving it anyway (lane `wfj2sidebar`, fresh `project.add` on this repo — `worktreeCount:"6"` —
giving 6 draggable worktree rows under one project header): a robust drag of the `9a42249` row
through 7 waypoints down to below `984defa`, **no** Escape, moved it unambiguously to the bottom of
the list (`/tmp/wf-judge2/shots/sidebar/03-51-robust-after-no-escape.png`) — positive control,
gesture registers. The identical recipe with `key Escape` inserted before the final `up`
(`03-61-robust-escape-after.png`) left the row order **completely unchanged**, matching the pristine
baseline. Repeated once more with a shorter recipe (down/up at the `9a42249`/`wl-proof-branch`
boundary): same pattern — a one-position swap without Escape
(`03-41-after-matched-posctrl-no-escape.png`), no change at all with Escape
(`03-31-after-escape-mid-drag.png`). Two independent gesture recipes, each run with and without
Escape, all four runs consistent: **the sidebar drag did not land when Escape was pressed mid-drag,
contradicting the report's characterization that this path has "no Escape handling of its own."**

I can't fully explain this from the code — no call site stops or reverts a sidebar `RowDrag` on
Escape — so the most likely explanation is an accidental side effect of F-SET-02's own "Escape
returns focus to the sidebar, which is rendered again on the next frame" behavior (documented in the
`handle_close_settings_surface` doc comment) landing on a component rebuild that re-derives `rows`
from the canonical project list, discarding the live-only, never-`confirm_reorder`ed preview mutation
— but that is inference, not something I directly observed, and I'm not treating it as evidence
either way. What I *am* reporting as evidence is the observed, reproduced-4/4-times behavior: on this
binary, right now, Escape mid-sidebar-drag does not leave a stray reorder on screen. This does not
change F-TAB-24's own verdict (its clause is specifically about tab drags, and that clause is fully
closed above) — it's recorded so the next critic auditing "drag cancel" more broadly starts from a
live result instead of re-trusting a claim this pass's own drive did not reproduce.

### Gap disposition

All three of the report's own named gaps for this row (3+-tab case, drop-clears-snapshot case, and
the caveat about sidebar drags being unaudited) are closed: the first two with a live PASS, the third
by actually driving it — where the live result contradicts the report's characterization rather than
confirming it.

---

## F-CHG-02 — Files panel shows an unrelated real file tree on a genuinely empty catalog

**Verdict: PASSED**

### Code check

`git show 25900f64` matches the report: `RightPanel::bind_worktree`
(`rust/crates/tiller_ui/src/right_panel.rs`) mirrors the existing `clear_worktree`, and
`TillerWorkspace::sync_activity` (`rust/crates/tiller/src/main.rs`) now calls `bind_worktree`/
`clear_worktree` based on `has_current_worktree()` on every render, not just at the two call sites
(`select_worktree`, `close_workspace`) that existed before.

### Reproduced original defect (pre-fix binary, lane `wfj2prechg02`)

Fresh empty DB, no `project.add`. **Hard discriminator**: `ctl project.list` → `projects:[]`,
`ctl workspace.list` → `workspaces:[]` — genuinely empty catalog, confirmed at the control-socket
level, not just visually. Screenshot `/tmp/wf-judge2/shots/prechg02/03-02-settled.png` shows the
center pane correctly reading "No worktree selected. Add a project, then select a worktree." while
the Files panel on the right **simultaneously renders this repo's own real tree** (`.agents`,
`.claude`, `App`, `rust`, `Scripts`, `reference`, …, rooted at
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`) — the exact divergence the ledger row and
report describe, reproduced independently on the pre-fix binary.

### Confirmed fix (current-HEAD binary)

**Matching-fallback case (lane `wfj2chg02`):** fresh empty DB, `ctl project.add` on this same repo's
own path. `ctl workspace.current` afterward returns a real workspace
(`branch:"linux/gpui-waku"`, `path:.../tiller-linux`) — hard discriminator at the control-socket
level, not a screenshot guess — and the Files panel and center pane both track it consistently across
frames.

**Non-matching case (the builder's own named gap, closed here, lane `wfj2chg02b`):** fresh empty DB,
`ctl project.add` on a throwaway scratch git repo
(`$SCRATCH/wfjudge/unrelated-proj`, `git init`+one commit, nothing to do with this repo or its
fallback directory) — `added:"true"`, `worktreeCount:"1"`. **Hard discriminator**:
`ctl workspace.current` immediately afterward returns `{"ok":false,"error":"no current workspace"}`
— `has_current_worktree()` is genuinely false, no worktree was auto-selected by adding an unrelated
project — and `ctl workspace.list` confirms the new project's sole worktree has `"selected":"false"`.
Screenshot `/tmp/wf-judge2/shots/chg02b/04-03-after-settle.png` shows the sidebar now lists
`unrelated-proj` alongside its `master` worktree, **and both the center pane and the Files panel still
correctly read their own "No worktree selected" placeholders** — no divergence, no stray tree from
either panel. This is exactly the "more common real-world shape" the report flagged as undriven: a
project that does *not* match the fallback directory, confirmed live not to spuriously bind the Files
panel.

### Gap disposition

Gap (1) (the non-matching-project case) is closed above with a control-socket hard discriminator
(`workspace.current` erroring) plus a screenshot showing both panels agree. Gap (2) (not individually
re-driving all ~44 `sync_activity` call sites) is accepted as the report frames it — a full audit of
44 unrelated call sites is out of proportion to this row, and the two live cases actually exercised
(matching-fallback boot-then-add, and a genuinely unrelated add) are the two shapes that matter for
this specific bug class (worktree becomes current / worktree does not become current). Gap (3) (the
pre-existing `titlebar.rs` double-click test failure) is not this row's concern and is not re-verified
here.

---

## F-CHG-18 — a diff dropped onto a terminal reached a real handler and then went nowhere a user could see

**Verdict: PASSED**

### Code check

`git show 544a1f54 -- rust/crates/tiller/src/main.rs` matches the report: `subscribe_terminal_drop`
is added alongside the three pre-existing per-terminal subscriptions in
`mount_terminal_subscriptions`, and on `TerminalDropEvent::Diff { path, .. }` calls
`workspace.add_changes_tab(Some(path.clone()), cx)` — the same call
`ChangesTabActionEvent::OpenDiff` already used. `grep -rn "TerminalDropEvent"` across
`rust/crates/tiller/src` and `rust/crates/tiller_ui/src` now finds this subscription as a real
consumer, where before the fix it found none — independently re-confirmed, not just re-quoted from
the report.

### Reproduced original defect (pre-fix binary, lane `wfj2prechg18`)

Fresh `project.add`, opened a Changes tab (`+` menu → "Changes"), showing this repo's own real
uncommitted local diff (`Local changes (4)`: `main.rs`, `chat.rs` changed, 2 untracked reference
PNGs) — a genuine non-empty Changes panel, not a fixture. `ctl panel.list` before the drop: exactly
**3** panes (Chat, Terminal, Changes). Drove the exact gesture the report worked out (drag-start on
the `main.rs` row, `chord ctrl+shift Tab` mid-drag to switch to the Terminal tab without releasing
the mouse button, move onto the terminal content, release): screenshot
`/tmp/wf-judge2/shots/prechg18/02-20-after-drop-unfixed.png` shows a **"Dropped diff:
rust/crates/tiller/src/main.rs" toast** (top right, terminal self-identifies as
`wf-judge2-tiller-PREFIX`) — the drop genuinely reached `receive_diff_drop` — and **hard
discriminator**: `ctl panel.list` afterward is still exactly **3** panes, Terminal merely became the
active tab, no Changes tab appeared. Exactly the ledger's "accepted and then went nowhere a user could
see," reproduced independently on the pre-fix binary, not read from the report.

### Confirmed fix (current-HEAD binary, lanes `wfj2chg18b` and `wfj2chg18c`)

Identical gesture, same file row, same coordinates, only the binary differs. `wfj2chg18b`: `panel.list`
before **3** panes; after, **4** — a new `pane-3`, `tab:"Changes"`, `active:"true"`. Screenshot
`/tmp/wf-judge2/shots/chg18b/02-20-after-drop.png` shows a brand-new, focused Changes tab (second tab
labelled "Changes", with its own close button) rendering the diff of exactly `main.rs` — the file
whose row was dragged, visible line-for-line. Re-driven a second time (`wfj2chg18c`, gap 1 below,
dragging the same row at slightly different pixel coordinates to rule out a coordinate-specific
fluke): same result, `panel.list` 3→4, new `pane-3` Changes tab active. Two independent runs, same
outcome, plus the one negative-control run above on the unfixed binary showing the drop is received
but produces no tab — isolating the fix's effect from the gesture/coordinates themselves, the same
logic the report used, now independently re-run rather than trusted from its text.

### Gap disposition

Gap (1) (re-drive independently, more than once per binary) is closed: this pass drove the gesture
itself from scratch — reconstructing the `+`-menu-to-open-Changes step, which the report's own text
didn't need to spell out since it already had a Changes tab open — once on the pre-fix binary and
twice on the fixed binary, all three consistent. Gap (2) (no production layout puts Changes and
Terminal in the same split, so this gesture is the only route to the fix today) is accepted as a real,
separate UX-discoverability finding the report itself scoped out of this row's own clause ("wire the
consumer," which is proven wired) — not re-litigated here. Gap (3) (`TerminalDropEvent::Files`, the
sibling variant, untouched) is accepted as out of scope: the ledger row and this pass both concern the
`Diff` variant only.
