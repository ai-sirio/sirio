# B3-sidebar — verdicts

Critic pass, independent of the builder (no builder reasoning was read). Driven live: F-PRJ-01,
F-PRJ-11, F-PRJ-12 and F-PRJ-16 on the Wayland lane (`Scripts/wayland-drive.sh`, real synthetic
click/move/type through `wlr-virtual-pointer`/`wtype`, screenshots forced-repainted via
`grim`); F-SID-17 on `DISPLAY=:1` (`Scripts/linux-drive.sh`) because it needs a genuine
mouse-down/move/up drag, which the Wayland lane's virtual pointer does not yet expose (only
`move`/`click`). All screenshots are under
`reference/linux-progress/verify-B3-sidebar/`.

Two harness traps cost real time and are recorded for whoever drives this lane next:

- **wlr-virtual-pointer's `x`/`y` are normalized against the `x_extent`/`y_extent` you pass, not
  raw pixels.** A helper that hardcodes the extent instead of querying the compositor's live
  `swaymsg -t get_outputs` output produces a silent, proportional coordinate drift the moment the
  actual output resolution differs from the hardcoded one (as it does every other `shot`, by
  design — repaint is forced by alternating sizes). Cost: three misfired clicks before the
  extent was made live-queried per command.
- **The first `wtype` call after boot is slow** (keymap negotiation): a 0.3 s settle after the
  *first* typed string showed nothing landed, reading as "the field doesn't accept input," while
  a second key sent 0.3 s later landed concatenated onto the first, proving both had actually
  worked — just late. Fixed by giving the first type of a session ≥1 s before capturing.

## F-SID-17 — worktree/tab reorder must not drop past the trailing row — **PASSED**

Real drag, not a socket call or a test harness: on the `tiller` project (2 worktrees —
`rust/gpui-rewrite` Primary, `linux/gpui-waku`, exactly the row's own precondition) `xdotool
mousedown` on `rust/gpui-rewrite`, 30 incremental 2px `mousemove --sync` steps (80ms apart) down
onto the bottom half of `linux/gpui-waku`, then `mouseup`.

A first attempt with 3 coarse waypoints (15–20px each) produced **no** reorder — worth recording
as a negative control, since it proves the harness can fail silently on too-coarse motion rather
than the feature being broken; a plain single-click positive control on the same row at the same
coordinates (`x11-click-waku.png`) confirmed the click itself landed and highlighted the row, so
the coarse-motion drag's failure was a harness gesture problem, not a feature problem. The
finer-grained retry is a **materially different, stronger** gesture, not a repeat of the same
action:

- Mid-drag (button still held, before mouseup): `x11-mid-drag3.png` already shows the live
  reorder preview — `linux/gpui-waku` first (with its own tab children), `rust/gpui-rewrite`
  pushed to second, its own tab children now indented under it.
- After `mouseup`: `x11-after-drag3.png` — final order is `linux/gpui-waku`, then
  `rust/gpui-rewrite` (still badged `Primary`), then `New Worktree…` immediately below it. The
  `New Worktree…` affordance stayed last — the dragged row landed **above** it, not past it —
  which is exactly the defect `reorder_rows()`'s old `unwrap_or(self.rows.len())` fallback used
  to produce (confirmed by reading the fix at `sidebar.rs:622-635`, commit `2df5dca`: the fallback
  now inserts after the group's last remaining sibling instead of at the absolute end of
  `self.rows`).

This directly overturns the ledger's prior `FAILED — defective` for this row (`sweep A1-P109`,
"produced no reorder, immediate or delayed"): that finding used a technique this pass also
reproduced (coarse waypoints) and also saw fail, then went further with finer motion and got a
clean, repeatable pass with visible before/mid/after evidence.

## F-PRJ-01 — add-project menu must paint opaque — **PASSED**

Clicked the `+` next to `Projects` (293,49). `03-add-project-menu-open.png` shows a fully opaque
card — `Open Project…` / `Clone Repository…` / `Create Project…` — with **no** bleed-through of
the `Filter` field or the `tiller` project row underneath, both of which occupy exactly the
screen region the menu now covers. This is the row's own acceptance bar and it is met directly,
not inferred: the prior ledger evidence (`orch14-plus-a/b.png`) showed the same region with the
filter field and project path text visibly composited through the menu; this capture shows a
blank card in their place.

## F-PRJ-11 — Remove Project control inside Project Settings — **PASSED**

Hovered the `tiller` project row to reveal the hover-only gear (`project-settings-<id>`,
confirmed present only on hover — `08-hover-project-row.png` vs the unhovered baseline), clicked
it, and confirmed a `Remove Project` row (red, `×` glyph) now sits in the sheet itself
(`09-project-settings-opened.png`) — the row the ledger previously recorded as **absent**
(`FAILED — absent`, "no removal/trash control anywhere").

Exercised **both** outcomes of the confirmation gate, on two different projects so the working
tree stayed intact:

- **Cancel path** (`tiller`): clicked `Remove Project` → native `Remove project from Tiller?`
  prompt appeared with `Remove from Tiller`/`Cancel` (`10-remove-clicked.png`) → clicked `Cancel`
  → sheet returned to normal, project still present (`12-after-cancel-click.png`).
- **Accept path** (throwaway `verify-sid-nongit`, `/tmp/verify-sid-nongit`): same control →
  same prompt (`41-remove-confirm-nongit.png`) → clicked `Remove from Tiller` → the project is
  gone from the sidebar entirely (`43-after-close-check.png`, only `TESTNAMEa` remains) — and
  `ls /tmp/verify-sid-nongit` afterward still shows the directory and its `.git`, confirming the
  prompt's own claim ("This only removes the project from Tiller's si[debar]") is true, not just
  displayed.

## F-PRJ-12 — open Project Settings card must refresh on `set_projects` — **PASSED**

Created a real non-git folder (`/tmp/verify-sid-nongit`, confirmed with `git status` failing
before the test), added it, opened Project Settings: `Repository: Folder` + an `Initialize Git`
button, both real, both present (`15-nongit-settings.png`). Clicked `Initialize Git` — **without
closing the sheet** — and captured immediately after: `16-after-init-git.png` shows `Repository:
Git` and the `Initialize Git` button gone, live, sheet never closed. `git status` on
`/tmp/verify-sid-nongit` afterward confirms a real repo was created on disk (`Sul branch master`),
so this is a genuine host round-trip, not a UI-only flip. The sidebar row itself picked up the
change too (`17-closed-after-init.png` shows `verify-sid-nongit` now rendering as a worktree
project with a `master`/`Primary` badge).

This overturns the ledger's `half-proven` ("the OPEN sheet itself doesn't live-refresh (staleness
bug)") — the specific gap it named (the open sheet not refreshing without close/reopen) is the
exact thing this capture shows working.

## F-PRJ-16 — Open Emoji Picker must do something — **PASSED**

Opened Project Settings → Emoji tab → clicked `Open Emoji Picker` with the emoji field empty
(the ledger's own named precondition: "clicked with field empty, no observable effect, picker
overlay never reached"). This time a bordered card with a search field and a ~40-swatch grid
appeared immediately below the button (`32-picker-open2.png`) — the picker the ledger recorded as
never reached.

The grid auto-focuses its search field on open (confirmed live, not just read from source): typing
`rocket` right after opening — no extra click needed — narrowed the full grid down to a single
swatch, 🚀 (`36-emoji-search-autofocus.png`). (A first attempt that *also* clicked the search box
after opening produced no typed text landing at all — recorded as a negative result, not silently
discarded — and was root-caused to the harness, not the feature: a plain-click-then-type control
on the unrelated `Display name` field showed the same "first keystrokes after boot land late"
behavior described in the traps section above, and once the picker was reopened and typed into
*without* the redundant click, it worked cleanly and repeatably.) Clicking the filtered 🚀 swatch
committed it immediately and closed the grid in the same frame (`37-emoji-committed.png`: icon
swatch now shows 🚀, grid gone) — and the sidebar project row itself updated to the 🚀 icon
(`38-sidebar-after-emoji.png`), confirming the commit reached the host, not just the picker's own
local state.

## Summary

| Row | Verdict | Prior ledger verdict |
|---|---|---|
| F-SID-17 | PASSED | FAILED — defective |
| F-PRJ-01 | PASSED | FAILED — defective |
| F-PRJ-11 | PASSED | FAILED — absent |
| F-PRJ-12 | PASSED | half-proven |
| F-PRJ-16 | PASSED | half-proven |
