# Critic pass 13 — the display is back; spend it

**You are pireview, pane `w1:p6`.** Your context was just reset, so this brief is everything you
need. You judge; you do not build, and you never judge a piece you built.

## The machine rebooted at ~19:01 and the display works

This is not a hopeful guess. I ran the instrument and it answered:

```
$ bash Scripts/linux-shot.sh /tmp/postreboot-shot.png
OK   /tmp/postreboot-shot.png  (1470x833 · 8401 colours · window 0x600001)
```

8401 colours means it cleared the flatness guard — real pixels, not the black rectangle that fooled
us before. The shot shows a complete three-column app: `Projects` header with `+` and a `Filter`
field, the `tiller` project with `rust/gpui-rewrite` (badged **Primary**) and `linux/gpui-waku`
worktrees, `Chat` / `Terminal` rows, `New Worktree…`; a centre tab bar with `Chat` and `Terminal`
tabs and a composer reading `Message…` with `+`, an `idle` dot, `Opus Plan Mode`, a `0%` ring and an
avatar; a `Files` panel with a real tree; and a status bar with `Claude` / `Codex` / `OpenCode Go`
chips.

**The display outage was never just seven blocked rows.** It silently forced code-reading verdicts
across the entire UI half of the ledger, which is where every one of the ten false PASSEDs came
from — 21% false among rows judged by reading, ~0% among rows judged by executing. That excuse is
now gone. Spend this pass on the things only a working display can settle.

## Two costs of the reboot, so nothing surprises you

- `/tmp/critic-target` and `/tmp/critic-pass12` were both wiped. Your next build is cold. Disk is
  fine: 13% used, 375G free. Re-create the shared cache and keep using it —
  `export CARGO_TARGET_DIR=/tmp/critic-target`.
- **Three of your pass-12 tests no longer exist.** `projects_header_add_rows_render`,
  `filter_narrows…` and `project_chevron…` are absent from the live tree; I checked with a positive
  control (the same grep finds `right_click_context_menu_dispatches_a_typed_worktree_action` in
  `sidebar.rs`, so the zero is real). They lived only in `/tmp/critic-pass12`, which is gone.

## 1. The structural rule this pass establishes

`F-SID-01/02/04` were upgraded in pass 12 from "unreplayable PASSED" to "PASSED on three new green
drawn critic tests". Those tests are now deleted, so those three verdicts are exactly as
unreplayable as they were before — but they *look* proven, which is worse.

**A proof that lives in `/tmp` is not a proof.** It is the same defect as prose in a report having
no owner, and as a quarantine that reads like a pass: the artefact looks like evidence and cannot be
re-run by anyone.

From this pass on, **critic tests land in the live repository.** Write them into the real tree
(`tiller_ui/src/sidebar.rs` tests, or a `critic_*` test module) as part of your pass. Your
independence comes from your own snapshot, your own build and your own execution — it has never come
from your tests being invisible to everyone else. Then either re-establish `F-SID-01/02/04` with
tests that survive, or downgrade them; your call, stated.

## 2. `F-PRJ` — eighteen rows nobody has ever touched

Every one of the 18 `F-PRJ` rows is `NOT EXERCISED` **and** never claimed by anyone. It is the
largest untouched surface in the ledger and the only whole family in that state.

They are not absent — the screenshot shows the projects surface exists and looks finished. They have
simply never been tried. That combination is exactly where false confidence accumulates, and you now
have a display to try them with: add a project, filter it, collapse and restore its chevron, mark a
primary worktree, create a worktree, remove a project and confirm the prompt.

Judge them by exercising. A row you cannot reach, say so and why.

## 3. Then the rows the outage was blocking

The 8 `NOT EXERCISED — blocked on display` rows have lost their excuse. Take them, and take the
appearance tier with them — `SHOT-LIST.md` was written for exactly this moment. Compare against
`00-ui-observed-from-screenshots.md` and the waku reference bar in
`03-visual-bar-and-gpui-patterns.md`; appearance debt (side-by-side, colours, typography) has been
deferred all day for want of a screen.

## 4. Stop maintaining the Totals block by hand

I wrote `Scripts/ledger-totals.py`. Run `python3 Scripts/ledger-totals.py --write`.

Your pass-12 recompute was correct when you made it and wrong within minutes, because builders
append and claim rows continuously while only you recompute. The block read 388/130/12/18 against a
body of 389/123/7/31. It also hid something: `F-SET-05` contained a raw `|` in its evidence (the
no-op handler `|_,_,_| {}`), which shifts every column after it and defeated the `judged` cross-cut —
"never independently judged" read **54** against a true **68**. I escaped that pipe and rewrote the
block; the script now reports both and is idempotent. Use it instead of counting.

## Do not judge work that is in flight

Three builders are mid-piece right now. Snapshot as usual, but do not spend verdicts on:
`tiller_ui/src/chat.rs` (`pi`, D1 stop-and-queue), `tiller_persistence` (`codex11`, P56),
`tiller/src/main.rs` (`codex12`, P57 chat wiring). The `tiller_ui` lib tests currently do not compile
— two lifetime errors in `pi`'s in-progress D1 tests. That is expected mid-TDD and is not a finding.

## Reporting

Update `INVENTORY-LEDGER.md`, run the totals script, and append to `CRITIC-findings-log.md`. Then
**15 lines or fewer**: what the display let you settle that you could not before, the `F-PRJ` verdicts,
what you did about `F-SID-01/02/04`, where your new tests now live, the appearance-tier findings with
shot filenames, and the honest remainder.
