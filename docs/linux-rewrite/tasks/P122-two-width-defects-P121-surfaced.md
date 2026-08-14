# P122 — two width defects P121 surfaced while verifying P117

**Filed by the orchestrator, 2026-08-14, from `docs/linux-rewrite/P121-report.md`.**
Unowned. Neither is a P117 regression — P121 checked and said so explicitly — and neither
blocked its verdict. They are filed here so that closing the pane that found them loses
nothing.

Both are **width** problems. P117 was a height problem. That is the whole reason they are a
separate task: the fix that gave every centre surface a real height does not touch either of
these, and P121's captures show both on the *fixed* binary.

## 1. A chat bubble does not wrap to a narrowed pane

Split the Chat pane, then send a message into the now-narrower half. The user bubble is **cut
at the pane's right edge** rather than wrapping. P121 sent `P121SPLIT_MARKER_ENZO` and the
bubble drew `P121S`.

- Evidence: `reference/linux-progress/p121/05-10-split-with-marker.png`, zoom
  `10c-split-marker-zoom2x.png`. Confirmed by direct inspection, not inferred.
- The marker discriminates: that string exists nowhere but in P121's own drive, so the frame
  cannot be explained by anything the app would have drawn on its own.
- Where to look: chat bubble rendering in `rust/crates/tiller_ui/src/chat.rs`. P117's diff
  never touched it.

## 2. Nested splits clip past the viewport instead of reflowing

Four consecutive `pane.split direction=down` calls produce a lopsided stack — each split
roughly halves the **newly created** pane rather than the whole group — and the bottom of the
stack is cut off by the window edge rather than the group reflowing or becoming scrollable.

- Evidence: `reference/linux-progress/p121/02-11-quad-split-down.png`.
- P121 deliberately did **not** bisect this against the pre-fix commit, and gave the reason:
  the pre-fix binary cannot render a split at all, so there is nothing to compare against.
  Do not read that as "unverified" — it is a stated limit of the comparison, not an omission.
- Two questions are tangled here and should be separated before either is fixed: *is the
  halving rule wrong* (each split halving the new pane, not the group), and *should an
  overflowing group clip or scroll*. They may have one cause or two — establish which first.
  That question is what made P117 tractable.

## What P121 established that bears on this

Splitting is a **newly reachable** surface. On the pre-fix binary two splits rendered
completely blank (`15-prefix-quad2-split-down.png`); on the fixed binary the same drive gives
two correctly proportioned ~450 px panes (`04-09-split-down.png`). So neither defect above is
a regression — they are behaviour that only became observable once panes had real height.
Expect more of this class: **anything gated behind "a pane with content in it" has never been
exercised.**

## Rules

- Whoever takes this does **not** judge it. `INVENTORY-LEDGER.md` stays untouched by the
  builder.
- Reproduce with `Scripts/wayland-drive.sh` under your own `TILLER_WL_LABEL`; captures land in
  `reference/linux-progress/`, never `/tmp`.
- A green test does not close either item. The proof is a frame showing a wrapped bubble and a
  group that reflows.
