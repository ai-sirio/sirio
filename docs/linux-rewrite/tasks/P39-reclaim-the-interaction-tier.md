# P39 — Reclaim the interaction tier you just made reachable

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Your own notes are in `docs/linux-rewrite/PI-HANDOFF.md`.

## P34 built the editor, closed the routed defects, and found the thing that unblocked everyone

The editor exists: a headless model with `FileView` delegating to it, 7 new tests, 91 `tiller_ui`
and 24 `tiller_git` tests, `CI OK`. You also took the two routed `changes.rs` defects and closed
them properly — the unborn-HEAD staged file now reports `+4 −0` instead of zero, and a missing `git`
reports a spawn failure instead of silence.

**And you found the capability nobody knew was there.** `gpui::TestAppContext` with
`VisualTestContext` runs the real element tree with no display, produces a drawn frame with a
debug-bounds map, and dispatches real mouse and keyboard events through the real dispatch path. You
used it to prove F-CHG-09 the way the entry actually asks: a real `.git` removal, the error panel
and Retry asserted **in a drawn frame**, a **real click** on Retry, and recovery after re-init. The
`.debug_selector(id)` idiom you added is what makes elements findable.

That has been broadcast to every agent, because it changes what "blocked on display" means for a
large part of the inventory. It is the second unblock of the day and the more valuable one.

Two other things worth keeping:

- **You flagged the one deliberately ambiguous state** rather than hiding it: the control-socket
  `ChangesReport` keeps `usize` counts because its consumers want numbers, and defaults an uncounted
  file to zero — documented in the module header, with the human surface being the honest one.
  Naming a compromise is what makes it a compromise instead of a bug.
- **The machine's git speaks Italian.** Tests now assert on the locale-independent prefix while the
  user-facing detail shows whatever language git uses. A test asserting on English git output would
  have passed on someone else's machine and failed here, which is the worst kind of flake.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh
```

Still no display: pixels remain unavailable. codex11 is in `tiller_persistence/**` and
`tiller_terminal/**`; codex12 is in `tiller_control/**`, `tiller/src/main.rs` and `tiller_git/**`.
**`tiller_ui/**` and `tiller_theme/**` are yours.**

## The piece

Go back through the entries this project wrote off as `NOT EXERCISED — blocked on display` and
**reclaim the ones that were always about behaviour rather than appearance.**

The line to hold, and it is the same one that was got wrong once already: *the program knowing a
fact* is not *the user seeing it* — **but the user clicking something is behaviour, and behaviour is
now testable.**

Reachable now, in your crates:

- **`F-EDIT-01`** — Code and Preview modes: switching between them and each showing the right
  content is behaviour. Their typography is appearance.
- **`F-EDIT-09`** — double-click a file in the explorer and it opens in the right editor.
- **`F-EDIT-12`** — dragging a file row into a pane, if GPUI's test harness can express the drag;
  say so plainly if it cannot.
- **`F-TAB` interaction entries** — clicking a tab activates it, the close control removes it,
  the overflow menu lists tabs with the active one marked, double-click renames, Escape cancels a
  drag. The **chords** (`Ctrl-Tab`, `Ctrl-1…9`, `Ctrl-W`) dispatch through the same path — if
  `VisualTestContext` can send a key event, those stop being unproven too, which would close a block
  that has been stuck since P20.
- **`F-CHG` click entries** — expanding a file, expanding a collapsed-context band, the
  stage/unstage/discard buttons, `Collapse All`/`Expand All`.

Where a tab or pane action lives in the shell rather than in your crates, **say which and it will be
routed** — codex12 owns `main.rs`.

**Report each reclaimed entry with the verdict it now deserves**, and be strict: an entry that says
*see the dirty indicator* is still appearance; an entry that says *invoke close on a dirty tab and
confirm the prompt appears* is behaviour. Where an entry mixes both, prove the behaviour and record
the appearance half as still owed.

## Evidence

For each reclaimed entry, a test that finds the element in the drawn frame and drives it — not a
test of the handler function in isolation. The whole value of what you found is that the element is
laid out, hit-tested and dispatched for real; a unit test on the callback proves less than the old
socket transcripts did.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Zed's editor is right there and remains the strongest transplant temptation in this project.
- **A capability nobody exercised does not exist.**
- Appearance stays `NOT EXERCISED — blocked on display`. Do not let the new capability quietly
  widen into "we can check the UI now" — it cannot check what it looks like.

## Reporting

Reply in **12 lines or fewer**: how many entries you reclaimed and their new verdicts, whether
`VisualTestContext` can send key chords and drags, what needs routing to codex12, what remains
appearance-only, and the honest remainder.
