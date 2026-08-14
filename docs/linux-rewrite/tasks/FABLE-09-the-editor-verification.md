# FABLE-09 — verify the editor input tier

**Owner: `fable`, as critic.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`.

A builder has just claimed the editor's keyboard-input tier in `tiller_ui/src/file_view.rs`, plus
the two `F-EDIT` rows that sat on it. Nobody independent has touched it. **You did not build it, so
you can judge it.**

**Start fresh.** Do not read `docs/linux-rewrite/tasks/P84-*.md`, and do not read the builder's
pane. You are judging the app against the inventory, not against anybody's account of what they did.
The rows below are the whole contract.

## Why this one matters more than its size suggests

The claim arrives with a green suite and a self-reported live check. That combination is exactly how
false PASSEDs have entered this ledger before — pass 12 confirmed ten of them, and pass 17 (yours)
overturned `F-PER-01` on the same shape of evidence: real tests that proved the store while the
user's actual path wrote nothing. **Code plus a green test is `NOT EXERCISED`, never `PASSED`.**

There is a specific trap here. Before tonight the Code view accepted clicks, `Home`/`End`,
`shift+End`, the `B` toolbar button and `ctrl-s` — five gestures that all worked — while typing a
character did nothing at all. A surface that responds to almost everything reads as working. Test
the character path itself, not the surface's general responsiveness.

## What to exercise

Take a scratch Markdown file inside the worktree so you can `cat` it from disk between steps. Disk
read-back is the proof; a screenshot of text on screen is not, because an unsaved buffer looks
identical to a saved one.

1. **Printable characters.** Click inside a line of the Code view — inside the text bounds, not
   below the last line; a missed click looks exactly like a dead surface. Type ASCII, capture, and
   confirm the characters appear where the caret was.
2. **`ctrl-s` then disk.** Save and `cat` the file. The typed characters must be on disk.
3. **Backspace and Delete**, including across a non-empty selection.
4. **Enter inserts a newline** and does not submit or close anything.
5. **Typing replaces a selection** rather than inserting beside it.
6. **`F-EDIT-05`** — *"Open a file, modify it externally, return to Tiller, and confirm the conflict
   banner offers Reload and Keep; exercise both."* Modify the file from a shell while the app holds
   it open, return focus, and drive **both** actions. The current row records the banner as absent
   while the machinery behind it exists, so the question is specifically whether a banner now
   appears and whether its two buttons do what they say.
7. **`F-EDIT-03`** — *"Open a Markdown file larger than the configured preview threshold and confirm
   its path and Large file — manual preview state are shown."* The current row records a hardcoded
   1 MiB notice instead of a manual-preview state; check whether the control now exists and whether
   using it actually renders the preview.
8. **Regression on what already passed:** `F-EDIT-01` (Code/Preview toggle) and `F-EDIT-02` (one
   toolbar control changing the source). Both are `PASSED` today and must stay true.

## Verdicts

You own the ledger for these rows. Write `F-EDIT-01`, `-02`, `-03`, `-04`, `-05` and `-06` with
evidence a stranger could replay: the gesture, the coordinates, the file, the bytes you read back.

- Anything you could not exercise is `NOT EXERCISED` — say why, and do not soften it to
  `half-proven`.
- If a row is defective rather than missing, `FAILED — defective` and name the defect.
- If the builder's claim holds, say so plainly. A critic that only ever finds fault is as useless as
  one that never does.

**Do not upgrade a row you did not personally drive.** Recompute the Totals block from the body
rather than adjusting it by hand — the recount one-liner is in the block itself.

## Driving the display

One X pointer, four agents; the lock is self-healing:

```bash
TILLER_DRIVE_LABEL=fable TILLER_DRIVE_LOCK_WAIT=900 timeout 900 \
  Scripts/linux-drive.sh reference/linux-progress/f09-<name>.png '<actions>' 8
```

Give every in-script `shot` a **distinct filename** or the driver's own final capture overwrites it
and your frames arrive out of order. Take **two** captures after an action — the first often shows
the pre-action frame. Budget the timeout generously; a 560 s drive died tonight because `cargo
build` contended with three other agents compiling.

Your own `ENVIRONMENT.md` notes still apply: ASCII only through `type`, and coordinates are tied to
the current layout.

## Done means

1. The six `F-EDIT` rows judged, each with replayable evidence.
2. At least one round-trip proved **against the disk**, not against a screenshot.
3. Totals recomputed from the body.
4. Committed — findings that live only in the working tree are verdicts nobody can replay, which is
   why 149 files of pass-17 evidence had to be rescued tonight.
