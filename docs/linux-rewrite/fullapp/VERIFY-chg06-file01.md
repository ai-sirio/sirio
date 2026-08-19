# Live verification — F-CHG-06 and F-CORE-FILE-01

Driven 2026-08-20 against `c392542c`, binary built from HEAD and frozen at
`/tmp/tiller-verify-frozen` before the drive so no rebuild could swap it mid-run. Nested
Wayland lane (`Scripts/wayland-drive.sh`, label `verify-chg06-file01`, `wayland-3`). Frames in
`verify-chg06-file01-shots/`.

**Which half this drives.** `WAYLAND-LANE.md` (lines 102-107) draws the line: socket-driving a
surface proves **the handler and the render**, never the gesture. Both rows here claim a property
of the *render* — the order names appear in, and the colour a status marker is painted — so the
render half is the whole of what they ask for. No gesture is claimed and none was driven.

## The fixture, and why it can fail

`/var/tmp/tt-verify-chg06-file01-fixture`, a fresh git repo built for this drive. Its own
`git status --porcelain`:

```
 M zz-plain-modified.txt          <- modified, never staged   -> must be AMBER
A  zz-pure-staged-a.txt           <- staged only              -> must be GREEN
A  zz-pure-staged-b.txt           <- staged only              -> must be GREEN
AM zz-staged-and-modified.txt     <- staged AND further edited -> the case under test
?? zz-untracked.txt               <- never added              -> must be BLUE
```

Two deliberate properties:

- **`zz-plain-modified.txt` is a discriminating control.** Without an amber file in frame, "the
  staged-and-modified file is green" would also pass if a bug painted *everything* green. The
  control is what makes the reading a measurement instead of an observation.
- **The natural-sort group is known to be able to fail.** `ls` on the same directory prints
  `file10.txt file1.txt file20.txt file2.txt` — the exact wrong order. If the panel rendered
  plain lexicographic order, this fixture would show it.

## F-CORE-FILE-01 — natural sort, read off the screen

`01-files-tree-natural-sort-and-status-dots.png`, Files panel, top four rows:

```
file1.txt
file2.txt
file10.txt
file20.txt
```

`file10.txt` renders **after** `file2.txt`. That is the clause the row has been half-proven on
since the fix landed: the comparator was wired to `right_panel::read_tree`, unit-proven with a
positive control, but never once seen in the rendered panel. It has now been seen.

## F-CHG-06 — the two views agree, measured rather than eyeballed

`02-files-and-changes-agree.png` is a single frame holding both views: the Changes tab in the
centre column, the Files tree in the right panel, same worktree, same files. `surface.changes.read`
on the same instance confirms the state the frame depicts — `zz-staged-and-modified.txt` appears
in **both** the `Staged (3)` and `Changed (2)` sections, which is the precise condition that used
to render it amber in one view and green in the other.

Sampled pixel values:

| file | Files tree dot | Changes list icon |
|---|---|---|
| `zz-plain-modified.txt` | `#F2B847` amber | `#856830` |
| `zz-pure-staged-a.txt` | `#7AC791` green | `#4C7458` |
| `zz-pure-staged-b.txt` | `#7AC791` green | `#4C7458` |
| **`zz-staged-and-modified.txt`** | **`#7AC791` green** | **`#4C7458` in *both* sections** |
| `zz-untracked.txt` | `#8CA3FF` blue | `#566292` |

**The two columns are not the same number, and that needs explaining rather than hand-waving.**
The Files tree paints a filled dot; the Changes list paints a glyph, which is antialiased and so
reaches the screen as a partial-coverage composite over the `#1A1A1A` background. To show the
*underlying* colour is nevertheless identical, recover the alpha separately from each of the three
channels and check whether they agree — if the two views used different hues, the three per-channel
alphas would diverge:

| Changes icon | best-matching tree token | recovered alpha | spread across R/G/B |
|---|---|---|---|
| `zz-pure-staged-a` | `staged` | 0.521 | 0.0008 |
| `zz-pure-staged-b` | `staged` | 0.521 | 0.0008 |
| **`zz-staged-and-modified` (Staged section)** | **`staged`** | **0.521** | **0.0008** |
| **`zz-staged-and-modified` (Changed section)** | **`staged`** | **0.521** | **0.0008** |
| `zz-plain-modified` | `modified` | 0.493 | 0.0065 |
| `zz-untracked` | `untracked` | 0.525 | 0.0023 |

Three independent channels agreeing on one alpha to within 0.0008 is only possible if the Changes
icon is the *same theme token* as the tree dot, drawn at partial coverage. So the claim is not
"the colours look alike" — it is that both views resolve to the identical value, which is exactly
what routing them through the single `tiller_ui::git_status_style` resolver was meant to guarantee.

Note the second-to-last row deliberately: in the **Changed** section, the staged-and-modified file
still reads green. That is correct and is the Swift behaviour the row is ported against — staged
beats modified in the colour, independently of which section lists the file.

## What is not claimed

- No gesture was driven. Neither row's clause needs one, but no keyboard or pointer interaction
  with these two panels is proven by this pass.
- The amber/green/blue tokens were checked against each other and against the theme, not against a
  fresh reading of the Swift original — that comparison was already done by the critic recorded in
  the `F-CHG-06` ledger cell and is not re-derived here.
