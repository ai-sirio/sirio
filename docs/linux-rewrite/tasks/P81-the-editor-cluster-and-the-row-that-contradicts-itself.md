# P81 — the editor cluster, and the row that contradicts itself

**Owner: `codex11`.** Nine rows, all in your own files, **no seam anywhere.** That is rare here and
it means nothing is blocked on another pane.

Read `../ENVIRONMENT.md`, `../OWNERSHIP.md` and `../SEAMS.md` first.

## The cluster

`editor.rs` and `file_view.rs` are yours. Nine of the thirteen `F-EDIT` rows read `FAILED — absent`:

| row | the ledger's words |
|---|---|
| `F-EDIT-01` | no Code/Preview switch (pass 3/7); **P39 builder claim of drawn switch unverified** |
| `F-EDIT-02` | no formatting toolbar |
| `F-EDIT-03` | no manual-preview state; hardcoded 1 MiB notice instead; **P39 claim unverified** |
| `F-EDIT-05` | no Reload/Keep banner |
| `F-EDIT-06` | no save path |
| `F-EDIT-07` | code renders as plain numbered lines, no language detection |
| `F-EDIT-10` | no context menu on file rows |
| `F-EDIT-11` | no copy-path code |
| `F-EDIT-12` | no product drag; payload-drag fixture is harness-only |

## Do not build anything until you have done this

**Three of these rows are probably wrong, and finding that out is worth more than a night of code.**

**`F-EDIT-06` contradicts `F-EDIT-04` outright.** `F-EDIT-06` says *"no save path"* and is dated
**pass 3 — thirteen passes behind.** `F-EDIT-04` is `PASSED` and its evidence reads:

> Linux chord ctrl-s wired in `main.rs:116` → `SaveFile` action → `handle_save_file` saves every File
> view in the active tab

Both cannot be true. Establish which, and if the save path exists, **say so and build nothing** —
that is the correct outcome for that row and it is a free tick for the critic.

**`F-EDIT-01` and `F-EDIT-03` both say "P39 builder claim unverified."** Somebody reported building
a Code/Preview switch. Before you write a second one, find out whether the first exists. If it does
and nothing reaches it, that is the dead-control shape — **fourteen rows in this ledger are already
that** — and the fix is a call site, not a component.

**`F-EDIT-10` was blocked by a platform claim that turned out to be false.** This file used to state
that XTEST cannot deliver button 3 under XWayland. It can:
`reference/linux-progress/p17-rclick-term.png` shows the terminal context menu open. So "no context
menu on file rows" is now a buildable row, not a platform limit. `F-EDIT-11` (copy path) is almost
certainly an item *in* that menu, so treat 10 and 11 as one piece of work.

Report which of the nine were stale, which were dead controls, and which were genuinely absent. That
breakdown is the most valuable thing you will produce.

## The transplant rule is under more pressure here than anywhere else

**Zed is an editor.** `F-EDIT-07` asks for language detection and syntax rendering, which is exactly
the code sitting in the reference checkout, and it is the single most tempting file in this project
to borrow from.

```bash
Scripts/transplant-check.py     # 0 clean · 1 candidates · 2 references missing
```

Run it before you hand off and state the result. It reports **46 pre-existing candidates** today —
you already saw that number on P77 — so the thing to look for is whether *your* files appear, not
whether the count is zero. Two files were deleted tonight for being 117 of 119 lines verbatim from
Zed's `hello_world.rs`. Every line of this cluster is written from scratch; the references are read
for behaviour and dimensions only.

If `F-EDIT-07` needs a syntax-highlighting approach, **design it from the requirement** — the row
asks for language detection and readable code rendering, not for a particular grammar engine. Pick
something you can implement and test in a night, and say plainly what it does and does not cover. A
partial highlighter you wrote beats a complete one you copied, in this project, always.

## Done means

1. Each of the nine labelled: stale, dead control, or genuinely absent — with the evidence.
2. What you built has tests, and the ones you ruled stale say what proves them stale.
3. `F-EDIT-10` and `F-EDIT-11` treated as one menu.
4. No edits outside your files. If something needs `main.rs`, that is a seam — register it in
   `SEAMS.md` and name the change, do not reach across.
5. `Scripts/transplant-check.py` run, with your files' status stated explicitly.

**The critic will open a file, edit it, save it, and right-click it.** That is the whole test.
