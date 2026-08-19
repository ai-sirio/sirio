# F-CHG-06, F-CORE-FILE-01, and the restart question — one fixture, three observations

Requested by team-lead: verify two of their own tonight's fixes (F-CHG-06,
F-CORE-FILE-01) plus the Browser/Changes restart question, off current
`linux/gpui-waku`. Worktree rebased onto `origin/linux/gpui-waku` at
`480e8cde` before starting (only my own not-yet-merged Browser/Changes
commit replayed on top, cleanly).

## Fixture

Real git repo, one commit, then:

- `file1.txt` — modified, `git add`ed (staged), then modified **again** on
  top (`git status --porcelain`: `MM`) — the staged-then-modified-again
  target for Observation 1.
- `file2.txt` — brand new file, `git add`ed, never touched again (`A `) —
  pure staged neighbor.
- `file9.txt` — existing tracked file, modified, never staged (` M`) — pure
  unstaged-modification neighbor.
- `file10.txt` — untouched, clean — sort-order subject only.
- `zdir/nested.txt` — a directory, tracked, untouched.

wayland-drive label `cdbgO757`, fixed binary rebuilt against the rebased
tree. Host load 6–20 throughout (one transient spike to 42 from other
activity on the box mid-session; waited it out before driving anything,
recorded below).

## Observation 1 — F-CHG-06: do the Files tree and Changes list agree on `file1.txt`?

Opened the Changes tab and the Files panel simultaneously
(`01-changes-and-files-side-by-side.png`). Sampled pixel RGB directly with
ImageMagick (`convert file.png -crop 1x1+X+Y txt:-`) rather than eyeballing,
at both panels, for all three files:

**Files tree** (`02-files-tree-dots-zoom.png`, 5x nearest-neighbor crop for
visual reference; exact values sampled from the unscaled screenshot):

| file | RGB | hex |
|---|---|---|
| file1.txt (staged + modified again) | (122,199,145) | `#7AC791` |
| file2.txt (pure staged, reference) | (122,199,145) | `#7AC791` |
| file9.txt (pure modified, reference) | (242,184,71) | `#F2B847` |

**Changes list** (`03-changes-list-icons-zoom.png`; icon-ring pixel, same
sampling method), checked in **both** of file1.txt's two listings — once
under "Staged", once under "Changed" (it has both a staged and an unstaged
hunk, so it legitimately appears in both sections):

| row | RGB | hex |
|---|---|---|
| file1.txt, Staged section | (103,165,122) | `#67A57A` |
| file2.txt, Staged section | (103,165,122) | `#67A57A` |
| file1.txt, Changed section | (103,165,122) | `#67A57A` |
| file9.txt, Changed section | (197,151,61) | `#C5973D` |

**Result: they agree.** file1.txt is bit-identical to file2.txt's green in
both panels, and clearly distinct from file9.txt's amber in both panels —
including its own "Changed"-section row, which reads green rather than
amber despite living under that heading. That is the precedence rule
(conflicted > untracked > staged > modified) applied per-file, not
per-section, exactly as described. Re-sampled after a full restart
(`07-restart-changes-restored.png`) — identical values, unchanged.

This is a positive result: F-CHG-06 holds under a live, pixel-sampled,
in-frame-referenced test.

## Observation 2 — F-CORE-FILE-01: does `file10.txt` sort after `file2.txt`?

Read directly off the rendered Files panel, no sampling needed since it's
just visible text order: `04-files-tree-sort-order.png` shows, top to
bottom: `zdir` (directory, leads), `file1.txt`, `file2.txt`, `file9.txt`,
`file10.txt`. `.git` is not listed.

That is natural-numeric order. Plain lexicographic order would have put
`file10.txt` second (`'1' < '2'` as characters), ahead of `file2.txt` and
`file9.txt`: `file1, file10, file2, file9`. The rendered order is not that —
it is the correct `localizedStandardCompare`-style order team-lead
described.

**Result: correct**, read from the panel's own rendered output rather than
inferred from the fix existing in source, per the instruction that a
previously-failed row here means the UI's own output is the only thing that
counts.

## Observation 3 — Browser and Changes through a restart, again, on the current tree

(Supersedes my earlier, now-rebased-in commit `06bec06c` / `a8c762a9`,
done against `linux/gpui-waku` before tonight's other fixes landed. Redone
here against the current tree with the same fixture used for the other two
observations, so all three share one session.)

Created a Terminal tab (`05-fresh-terminal-tab.png`) alongside the existing
Changes/Changes/Browser tabs — four tabs total. Full restart (same label,
same DB). All four survive:

- Terminal: fresh shell, fresh system-info banner (new timestamp),
  `06-restart-terminal-restored.png`.
- Changes: identical content and identical sampled pixel colours to
  before the restart, `07-restart-changes-restored.png`.
- Browser: toolbar/address-bar chrome correctly sized,
  `08-restart-browser-restored.png`.

No oversized-`centre-surface` symptom on any of them. Same pre-existing,
unrelated nested-Wayland limitation on the embedded Browser page content
(`Direct XCB build failed... GPUI returned unsupported handle`) as in the
earlier pass — identical banner text before and after restart, not a
regression.

## One thing worth recording: a load spike mid-session

Partway through, host load jumped to ~42 (another agent's activity, not
mine) and one `wayland-drive.sh` invocation failed with `Unable to connect
to .../sway.sock` — a reused-label race the script's own header documents
as a harness limitation, not an app defect. I did not drive anything through
that failure; I killed the half-started instance, waited (`uptime` polled
every 5s) until load was back under 20, and re-ran cleanly. No screenshot in
this doc was taken during the spike.
