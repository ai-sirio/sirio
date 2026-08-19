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
