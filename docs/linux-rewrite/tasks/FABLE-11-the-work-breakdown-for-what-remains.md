# FABLE-11 — The 92 rows that really are absent, cut into dispatchable pieces

**This brief is everything you need; your context just compacted.**

## Why this and not another recipe doc

You have written 72 recipes — 39 in `STALE-FAILED-RECIPES.md` (FABLE-09) and 33 in
`UNPROVEN-ROWS-RECIPES.md` (FABLE-10). That is several passes of work queued for `pireview`, and the
critic is no longer the tightest constraint.

**Construction is.** The ledger, measured by parsing column 2 (389 rows total):

| verdict | rows |
|---|---|
| PASSED | 188 |
| FAILED — absent | 131 |
| N/A — platform | 22 |
| half-proven | 13 |
| NOT EXERCISED | 12 |
| FAILED — defective | 8 |
| builder-claimed, unverified | 8 |
| UNREACHABLE | 7 |

Your own FABLE-08 census found **39 of the 131 already built**. So the real construction backlog is
**92 rows**, plus the 8 defective ones that are built-but-broken.

The standing goal says to *break the work into individually judgeable pieces*. Right now that cutting
happens in the orchestrator's head, one piece at a time, at dispatch. **Do it properly, once, for all
92.** That artefact is what decides who gets what for the rest of the night.

## The ownership map has just changed — use the current one, not the one in old briefs

`tiller_ui/src/settings.rs` moved from `pi` to `sonnet` (P75, user's decision). Current map:

| owner | files |
|---|---|
| `pi` | `chat.rs`, `sidebar.rs`, `status_bar.rs` |
| `codex11` | `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `browser.rs`, `tiller_git/**`, `tiller_terminal/**`, `tiller_acp/**`, `tiller_agents/**` |
| `codex12` | `main.rs`, `session.rs`, `tiller_control/**`, `tab_bar.rs` |
| `sonnet` | `titlebar.rs`, `controls.rs`, `composer.rs`, `settings.rs`, `icons.rs`, `tiller_theme/**` |
| unowned | `tiller_project/**`, `tiller_usage/**`, `tiller_markdown/**`, `tiller_persistence/**` |

Old briefs say `settings.rs` is `pi`'s. They are stale on that one line. If you find other drift
between the briefs and reality, **say so** — a stale ownership line is how two agents end up in one
file.

## What to produce

`docs/linux-rewrite/WORK-BREAKDOWN.md`. For each piece, one table row:

- **piece name** — a short handle, the way `P73` was *the identity thrown away in transit*
- **rows it closes** — the `F-XXX-NN` list
- **owner** — derived from the files it must touch, under the map above
- **size** — small / medium / large, and say what drives it
- **depends on** — another piece, a seam, or nothing. Most should be nothing; that is the point.

Group the 92 into pieces that are **individually judgeable**: a piece is right-sized when the critic
can exercise it and reach a verdict without needing a second piece to land first. Prefer more, smaller
pieces over fewer big ones — every piece that can be dispatched independently is a piece that can run
in parallel.

Then, at the end, **the dispatch order**: which piece each of the four builders should take next, and
why that one. That is the section that gets used first.

## Three things that will otherwise be got wrong

1. **A piece is defined by the files it touches, not by the `F-` prefix.** `F-SET` rows are `sonnet`'s
   now because `settings.rs` is; `F-CHAT` rows are `pi`'s because `chat.rs` is. Where a row needs two
   owners' files, that is a **seam** — name it explicitly as a two-half split, the way P71 was cut into
   Half A (`codex11`, add the setter) and Half B (`codex12`, call it). That pattern is the reason five
   builders have run all day without a collision.
2. **The 8 `FAILED — defective` rows are not in the 92 and are not the same job.** Something exists and
   is wrong. List them separately — diagnosing a defect is cheaper than building a feature, and they
   are probably the best value per hour on the whole board.
3. **Some rows will not be worth a piece at all.** If a row is one line inside a piece that already
   exists, fold it in and say so. Do not manufacture a piece per row to make the table look complete.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your pane
  may start in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Yours: one new file**, `docs/linux-rewrite/WORK-BREAKDOWN.md`. Touch no source. **Do not edit
  `INVENTORY-LEDGER.md`** — only `pireview` changes a verdict, and this changes none.
- Derive the 92 by subtracting FABLE-08's already-built 39 from the 131. **Verify that subtraction
  against the census's own stated count** rather than trusting it — a naive grep of that file returns
  143 IDs because it triages rows it did not build, and that number has already misled once.
- Commit one artefact, conventional-commit subject. **Proceed without asking for approval.**

## Reporting

**10 lines or fewer**: how many pieces the 92 cut into and the largest one; the dispatch order for the
four builders; any seam you had to name; any ownership drift you found between the briefs and reality;
which rows you folded in rather than making a piece of; and the honest remainder.
