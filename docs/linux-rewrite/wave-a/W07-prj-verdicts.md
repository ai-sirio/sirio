# Wave A slice W07-prj — critic verdicts

Adjudicated independently (did not drive or build this slice). HEAD `4073297`,
worktree `tiller-linux`, branch `linux/gpui-waku`.

## `F-PRJ-15` — ledger line 108, was **FAILED — defective** → **PASSED**

Verified independently by opening every cited capture with `Read`.

- `02-hover-row.png` / `02-settings-open.png`: baseline — orange folder icon selected, the
  six-glyph Icon grid (folder / git-branch / chat / terminal / document / globe) renders
  correctly, matching the ledger's own prior description.
- `03-immediately-after.png`: after clicking the git-branch glyph, the selection ring moved onto
  it (folder de-selected) while the settings panel stayed open — the successful click, distinct
  from the two flaked attempts below.
- `02-after-close.png`: the sidebar row for `w07prj-testrepo` now shows the **green git-branch
  glyph** in place of the orange folder — exactly the clause the ledger said was discarded.
- `02-relaunch-check.png`: pixel-identical to the post-close frame after a full kill+relaunch of
  the app against the same `TILLER_DB`, with no action beyond a forced repaint — confirms the
  write survived process death, i.e. it round-tripped through SQLite, not an in-memory
  `Rc<RefCell<>>`.
- `git log` confirms commit `28a41fa` ("feat(P97): the project settings card writes through and
  reads back") is on this branch, predates HEAD `4073297`, and its message independently
  describes exactly the bug this row's stale verdict was based on: the icon used to land in an
  `Rc<RefCell<ProjectIcon>>` and "stop there, read back out into an underscore-prefixed binding
  that discarded it."
- The flake investigation is honestly reported and holds up under inspection:
  `02-icon-picked.png` / `02-after-click-branch.png` show the settings sheet closing outright
  with the folder still selected on `02-reopen.png` (an implicit-dismiss flake at the same
  coordinate); `02-after-colour-click.png` shows a colour-swatch click on the same panel does
  *not* close it — consistent with the driver's "input-timing flake, not a property of the icon
  grid" read, and not something the row's own pass/fail clause depends on.

**Why PASSED and not half-proven:** the ledger's sole objection was "the chosen glyph never
reaches the project... the grid is real, its output is discarded." This drive shows the glyph
reaching the sidebar row and surviving a cold process restart against the same DB — the default
project icon is the orange folder, so a green git-branch glyph could not have arrived by any
route other than the one exercised. That is exercised-live-and-observed-to-work, not
source-plus-a-green-test.

## `F-PRJ-14` — ledger line 107, was **half-proven** → **half-proven** (same label, narrower half owed)

Verified independently.

- `02-avatar-open.png`: Avatar tab shows Choose PNG…, GitHub-avatar and favicon-domain controls,
  matching the row's affordance claim.
- `02-favicon-typed.png`: real text `example.com` lands in the domain field (dark text, not grey
  placeholder) — this Settings-sheet field accepts synthetic keystrokes fine, unlike the popovers
  in F-PRJ-06/07/09 below.
- `03-favicon-committed.png` / `02-favicon-closed2.png`: "Current: favicon for example.com"
  appears live after clicking Use Favicon, and after Close the sidebar row switches to the
  **green Globe glyph** (was the default orange folder) — the same P97 propagation path
  F-PRJ-15 just confirmed, now shown working for a second, previously-untried source
  (favicon-domain, not just GitHub-avatar).
- `02-precheck.png`: reopening Settings later in the session (after an unrelated restart) still
  shows "Current: favicon for example.com" — persistence confirmed for this arm too.
- `03-png-dialog2.png`: cursor lands squarely on Choose PNG…, no dialog appears, no state change,
  no error text anywhere in the frame — consistent with the claim that this lane (headless
  Wayland, no `xdg-desktop-portal`) has no way to drive a native file-open dialog. The affordance
  is present and clickable; nothing in the capture suggests it misbehaves under a lane that can
  actually open a dialog, so this reads as an environmental gap, not a demonstrated defect.

**Why half-proven, not PASSED, and not `NOT EXERCISED` for the whole row:** the favicon-domain
arm is now fully closed with live, discriminating, persisted proof — that half moved from
untried to proven. The PNG-upload arm is still genuinely untried, blocked by the lane rather than
by the app; that half alone would be `NOT EXERCISED`. A row that is one-half-proven,
one-half-blocked is exactly what `half-proven` describes. The half still owed is now narrower and
named precisely: PNG-upload propagation, drivable only from a lane with portal/file-dialog
support (X11 or a portal-enabled compositor), not from this Wayland lane.

## `F-PRJ-06` / `F-PRJ-09` / `F-PRJ-07` — ledger lines 99 / 102 / 100, all **half-proven** → **half-proven** (unchanged)

Verified independently.

- `02-slowgit-baseline.png` / `02-typed-check2.png`: "Loading Files…" hangs well past normal,
  confirming the sleep-wrapped git PATH shim was genuinely in the loop for this session — the
  harness itself works and is worth keeping for a future pass.
- `02-clone-form.png` / `02-url-typed.png` / `02-url-typed2.png` / `crop-url2.png` (F-PRJ-06,
  F-PRJ-07): after click+type, the Repository URL field still shows only the grey placeholder
  text with a bare text-cursor caret visible mid-string — no committed text ever lands.
- `02-create-form.png` / `02-typed-check.png` / `02-typed-check2.png` / `crop-field*.png`
  (F-PRJ-09): identical result for the Create-project "Project name" field — placeholder
  untouched across repeated attempts, including with inserted sleeps.
- `02-cancel-check.png` / `02-cancel-retry.png`: three plain clicks directly on the same card's
  Cancel button, with the pressed/hover highlight visibly reaching the button in every frame, and
  the card is still open in both after-frames. This is a well-designed control — it isolates the
  defect to click/keystroke delivery into this class of popover generally, not to text-entry
  specifically and not to a focus-timing race.
- `02-sanity2.png`: the sidebar Filter field — a plain, non-popover input — accepts an identical
  click+type (`sanitytest` lands as real text) in the same session, and `02-favicon-typed.png`
  above shows the Project-Settings sheet (also non-popover) accepting typed text too. This
  positive control is what makes the isolation credible: it is specifically the `+`-menu's
  anchored/floating popover surface that drops input on this lane, not synthetic input in
  general.
- `02-f07-invalid-submit.png`: same result on F-PRJ-07's fresh attempt — field still shows
  placeholder, status still "Ready to clone."

**Why half-proven, unchanged, for all three, and not `NOT EXERCISED` / `UNREACHABLE`:** each row
already had one half proven from prior evidence on record — the empty-URL/empty-name
disablement guards (F-PRJ-06/09) and the invalid-URL failure state with Retry-clone relabeling
(F-PRJ-07) — none of which this drive contradicts or retests; it only reconfirms the other half
(the double-submit guard, and the correct-URL retry path) is still unreached, now for a
better-isolated reason (an anchored-popover input-delivery gap on this Wayland lane, not the
sub-frame-race timing previously assumed). `UNREACHABLE` would require no route existing on Linux
at all, which is false — the guard logic is unit-tested and the affordance renders; this is a
lane limitation. This matches the driver's own honest self-report of `discriminating: false` for
all three rows — no new pass/fail information was produced, only a sharper diagnosis of why the
owed half stays owed. I did not attempt to re-drive these on X11 myself — that lane is a shared
mutex this slice's brief places out of scope.

## Notes

No disagreement with the driver's claims or discriminating self-assessments — all five rows'
prose, capture citations, and (where given) `discriminating` flags check out against the actual
pixels; nothing here is an overclaim. Two things worth carrying forward:

1. The `F-PRJ-15` reclassification is a real ledger-wide signal, not just a single-row fix: it
   confirms commit `28a41fa` (P97) actually closed the icon-propagation gap that `F-PRJ-13`
   found too (same shared-cause group per the triage brief) — any other row still citing "grid
   is real, output discarded" as a live defect should be re-checked against this HEAD rather than
   assumed still broken.
2. The newly-isolated anchored-popover input gap (F-PRJ-06/07/09) is a distinct, more specific
   finding than the "sub-frame completion" theory the ledger carried before. A future pass
   chasing these three rows should attack input delivery into this popover class first (or move
   to X11), not chase git-clone/git-init timing further — the timing harness is already validated
   and waiting.
