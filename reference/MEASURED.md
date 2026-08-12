# Measured geometry of the reference

Every number below was read off the frozen shots with `probe.py`, not estimated.
Units are points; the window is 1470 x 833.

Capture drift is up to 2 per channel, so a colour here may sit 1-2 below its token.
Where a token exists, **use the token, not the measured value**.

## Columns

| region | x range | background | token |
|---|---|---|---|
| sidebar | 1 - 324 | `#141416` | canvas `#131417` |
| divider | **325 (1pt)** | `#000000` | pure black, not the hairline token |
| centre pane | 326 - 1063 | `#1a1a1d` | background `#1b1c1f` |
| divider | **1064 (1pt)** | `#000000` | |
| right panel | 1065 - 1468 | `#1a1a1d` | background `#1b1c1f` |

The two column dividers are **1pt of pure black**, full height. This is the seam:
getting it 2pt wide, or tinted, or offset by even a point is visible.

## Horizontal bands (centre column)

| band | y range | height | background |
|---|---|---|---|
| title bar | 1 - 31 | 32 | canvas `#141416`, spans the full window width |
| tab bar | 32 - 65 | 34 | `#1a1a1d` |
| content | 66 - 792 | | |
| status bar | 793 - 832 | 40 | canvas `#141416`, full width |

## Tab bar

- Active tab is marked by a **2pt line on its top edge at y=36-37, colour `#8595fc`**
  (periwinkle). Note the 4pt gap between the top of the tab bar (y=32) and the accent.
- Tab label cap-height sits around y=46-47.
- First tab occupies x 326-439, second x 440-571; a tab is ~114-131 wide depending on
  its title, so tabs size to content, they are not a fixed width.
- Leading glyph inside a tab starts ~16pt in from the tab's left edge and is ~12pt wide,
  tinted with the agent's accent (Claude = `#ca7250`).
- The trailing "+" button sits at x 1046-1055, vertically centred in the bar.

## Sidebar

- Filter field: y 64-91 (**28 tall**), background `#2e2e32`, inset 20 from the left.
- Project row: ~30 tall. Selected/hovered background `#1e2028`.
- Worktree row: y 133-163 (**30 tall**). Selected background `#272932`, drawn as a
  rounded rect from **x=27 to x=317** — i.e. inset 27 left, 8 right, not full bleed.
- Tab row: ~24 tall, selected background `#272932`, indented further.
- Indent guide: a 2pt vertical line at **x=20-22**, colour `#242426`, connecting a
  worktree's tab rows to their parent.
- Row glyphs are ~12 wide and start ~38 in for worktree rows.

## Status bar

- 40 tall, canvas background, content baseline around y=818.
- Left to right: gear, refresh, then usage items (agent glyph + text).
- Right-aligned: `<branch> · <abbreviated path>`.

## How to check your work

```bash
python3 reference/probe.py diff mine.png reference/shots/01-chat-empty.png
```

Report `mean_delta`, and the rows it names as worst. A mean_delta under ~2 on a
region you are claiming parity for is the target; the worst-rows list tells you
which band is wrong.
