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

## `F-PRJ-14` — ledger line 107, currently half-proven

- **Triage says:** exercise
- **Approach:** drive the untried PNG-upload and favicon-domain arms live, Close, confirm the
  sidebar row shows the Globe glyph for each.

Reused the same project row from F-PRJ-15 (already had a GitHub avatar and, further back, a
git-branch icon selected — both persisted). Opened Project Settings → Avatar tab
(`02-avatar-open.png`). **Favicon-domain arm**, previously untried: clicked the domain field,
typed `example.com`, clicked "Use Favicon" — `Current: favicon for example.com` appeared live
(`03-favicon-committed.png`). Clicked Close: **the sidebar row switched to the green Globe
glyph** (`02-favicon-closed2.png`), same propagation path F-PRJ-15 confirmed fixed. Reopening
Settings afterward showed the favicon selection had round-tripped through the DB
(`03-png-dialog2.png`, `Current: favicon for example.com` still shown after the app had been
restarted once in between by an unrelated drive in this same session).

**PNG-upload arm, still untried:** clicked "Choose PNG…" (`03-png-dialog2.png` shows the click
landing squarely on the button) and captured before/after — no dialog, no visible state change,
no error text appeared anywhere in the captured frame. `choose_local_png` hands off to the
platform's native file-open dialog, which this synthetic headless-Wayland lane has no way to
drive (no portal service, and even if one answered, its window would not necessarily land on
the `HEADLESS-1` output `grim` captures). This is the same class of gap as `F-BRW`'s embedded
browser content — a property of the lane, not evidence about the feature. Route this arm to
`DISPLAY=:1` or leave it as a known instrumentation gap.

- **Claim:** partially-exercised
- **Drove:** Avatar tab → favicon field → type domain → Use Favicon → Close → sidebar check;
  separately, Choose PNG… click with before/after capture
- **Observed:** favicon-domain arm now fully proven end to end, including propagation to the
  sidebar Globe glyph and persistence across a process restart. PNG-upload arm remains
  unexercised — not because it failed, but because its control surface (native file dialog) has
  no synthetic-input path on this lane.
- **Discriminating:** yes for the favicon half — default project icon is not Globe, and the
  glyph only appears here because this drive picked a favicon/GitHub avatar source.
- **Captures:** 02-avatar-open.png, 02-favicon-typed.png, 03-favicon-committed.png,
  02-favicon-closed.png, 02-favicon-closed2.png, 02-precheck.png, 03-png-dialog2.png

## `F-PRJ-06` and `F-PRJ-09` — ledger lines 99/102, both half-proven

- **Triage says:** exercise (shim git on PATH with a sleep wrapper so Running persists across
  a frame, then fire two real clicks and confirm only one worker starts)

Built a `git` PATH shim (`/tmp/w07prj-slowgit/git`, `sleep 2; exec /usr/bin/git "$@"`) and
launched the app with it prepended to `PATH` (the launcher's `env` call inherits the invoking
shell's `PATH`, confirmed by `Loading Files…` staying visible far longer than normal — the
wrapper is genuinely in the loop for every git shell-out, including the ones behind
create-project's implicit git init and clone's git binary). This part of the approach worked
and is worth keeping for a future pass.

**What blocked both rows is earlier than the guard being tested.** Neither the Create-project
"Project name" field nor the Clone-repository "Repository URL" field — both live inside a
floating card opened from the sidebar `+` menu — accept synthetic input on this lane. `type`
after `click`ing the field leaves the placeholder text untouched (`crop-field.png`,
`crop-field2.png`, `crop-field3.png`, `crop-url2.png`); a bare `key a`/`key b`/`key c` sequence
lands nothing either. This is not a focus-timing race — inserted `sleep 1`/`sleep 2` between
click and type made no difference. It is also not specific to text entry: three repeated clicks
directly on the same card's **Cancel** button (`02-cancel-check.png`, `02-cancel-retry.png`)
left the card open every time, though the cursor visibly shows the button's hover/pressed
highlight in every capture — hover delivery reaches the card, click delivery does not
consistently take effect inside it. The identical click-then-type sequence worked moments
earlier in this same session against the sidebar Filter field and the Project-Settings-sheet
Avatar favicon field (see F-PRJ-14 above) — so this is specific to the `+`-menu's
anchored/floating popover class of surface, not a general lane failure.

This matches the caution already on record for `F-PRJ-06` ("unreliable rapid-click delivery —
first of a pair dropped 4/4 tries") almost exactly, and extends it: it is not only the *second*
click of a rapid pair that drops here, entry into the form is not reliably drivable at all on
this lane. Given the budget for one row, I did not chase further (no `DISPLAY=:1` — that lane
is reserved and out of scope per the brief).

- **F-PRJ-06 claim:** could-not-reach
- **F-PRJ-09 claim:** could-not-reach
- **Drove:** slow-git PATH shim + app relaunch (confirmed active via `Loading Files…` staying
  up); repeated attempts to type into the Create-project name field and the Clone-repository
  URL field; repeated plain clicks on the same card's Cancel button as a control
- **Observed:** the anchored popover forms opened from the sidebar `+` menu do not reliably
  accept synthetic clicks or keystrokes on this lane — confirmed with a Cancel-button click
  control, not just the fields under test — so the in-flight double-submit guard cannot be
  exercised here. The slow-git harness itself is validated and reusable once this input gap is
  fixed or a different lane is used.
- **Captures:** 02-slowgit-baseline.png, 02-plus-menu.png, 02-create-form.png,
  02-doubleclick-fire.png, 03-after-type.png, 02-form-now.png, 02-typed-check.png,
  02-typed-check2.png, crop-field.png, crop-field2.png, crop-field3.png, 02-sanity2.png
  (positive control — Filter field accepts the same click+type), 02-clone-form.png,
  02-url-typed.png, 02-url-typed2.png, crop-url2.png, 02-cancel-check.png, 02-cancel-retry.png
