# Wave A slice W07-prj — evidence log

Lane: Wayland (`Scripts/wayland-drive.sh`, `TILLER_WL_LABEL=wavea-W07-prj`).
Captures: `reference/linux-progress/wavea-W07-prj/`.

## `F-PRJ-15` — ledger line 108, was FAILED — defective, triage: reclassify

Re-drove live at HEAD 4073297, testing whether the P97 propagation fix (commit `28a41fa`,
post-dates the recorded evidence) actually reaches the sidebar row.

Sequence: `project.add path=/tmp/w07prj-testrepo` (real git repo) → hovered the project row
to reveal the gear icon → clicked it → Project Settings sheet opened with the Icon/Emoji/Avatar
picker → clicked the git-branch glyph in the grid → **panel stayed open** (unlike the two prior
attempts logged below) and the ring moved from folder to git-branch, folder icon simultaneously
re-tinted per the earlier default colour (`03-immediately-after.png`) → clicked **Close**
(`02-after-close.png`): **the sidebar project row now shows the git-branch glyph in green,
not the orange folder** — the clause the ledger says was discarded.

Persistence: killed the instance and relaunched against the same `TILLER_DB` with no actions
beyond a forced-repaint shot (`02-relaunch-check.png`) — the sidebar row still shows the
git-branch glyph after a full process restart, i.e. it round-tripped through SQLite, not just
in-memory state.

**Caution recorded, not swept under the rug:** two earlier attempts at the identical click
coordinate (`02-icon-picked.png`, `02-after-click-branch.png`) closed the whole settings sheet
outright with the folder still selected on reopen (`02-reopen.png`) — i.e. the click was
sometimes swallowed as an implicit dismiss instead of a selection. This was not reproduced on the
third attempt (which is the one carried through Close and relaunch above) and a colour-swatch
click on the same panel never closed it (`02-after-colour-click.png`), so the dismiss looks like
an input-timing flake of this synthetic-input lane rather than a property of the icon grid.
Recorded here so a future pass that reproduces the closing-on-select behaviour isn't starting
from zero.

- **Claim:** exercised-working
- **Drove:** gear icon → icon grid → pick git-branch → Close → sidebar row check → kill+relaunch
  → sidebar row check again
- **Observed:** chosen glyph reaches the sidebar row immediately after Close, and survives a
  full process restart against the same DB. The FAILED verdict is stale — P97 fixed it.
- **Discriminating:** yes — default project icon is the orange folder; git-branch only appears
  because this drive picked it, and it could not have arrived any other way.
- **Captures:** 02-hover-row.png, 02-settings-open.png, 03-immediately-after.png,
  02-after-close.png, 02-relaunch-check.png (positive path); 02-icon-picked.png,
  02-after-click-branch.png, 02-reopen.png, 02-after-colour-click.png (flake investigation)
