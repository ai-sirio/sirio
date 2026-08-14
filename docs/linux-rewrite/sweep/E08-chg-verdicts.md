# Critic verdicts — E08-chg (F-CHG)

Adjudicated by a critic that neither built nor drove this slice. Driver return:
`docs/linux-rewrite/sweep/E08-chg-evidence.md`, captures under
`reference/linux-progress/drive-E08-chg/`. HEAD under test: `4073297`.

## F-CHG-06 (ledger line 197) — verdict: `PASSED` (up from half-proven)

Driver drove two real things live over the socket: (1) `surface.changes.read` returning
correct per-section counts on a dirty fixture (data half, reconfirms pass 13), and (2) a
genuine state transition — `git add untracked.txt` run **outside** the app, then a re-read
with **zero UI action**, picked up by the app's own periodic timer
(`CHANGES_REFRESH_INTERVAL`, `changes.rs:63`) and correctly reclassified
Untracked(1)→Staged(2). That is real, discriminating proof of the "explicit Files refresh"
half the ledger called owed.

What the driver could not do (text-only agent) — read the glyph-per-status mapping off the
PNGs — I did, by cropping and zooming `chg06-before-1715.png` / `chg06-after-1715.png`
(4-12x, ImageMagick, capture files unmodified):

- The **Changes list** (center panel) draws a small file-icon per row whose *outline/fill
  color* differs by section: green for Staged, amber/orange for Changed, blue for Untracked.
  Same icon shape throughout (a generic file glyph, no letter) — the differentiator is color,
  not shape.
- The transition frame proves this is a live, correct mapping, not a coincidence: in the
  before/after pair, `untracked.txt`'s icon is **blue** while it sits under "Untracked", and
  **green** (matching `staged.txt`) once the external `git add` promotes it under "Staged" —
  the color follows the file's actual status through a real refresh, not a static per-file
  color.
- Distinct from this: the **right-hand "Files" tree** panel's per-file dots (next to "Diff")
  are the *same* amber color for all three files regardless of status in both frames — that
  panel does not carry a per-status signal at all. Pass 13's "three per-file status dots …
  right-panel files tree" evidence, if it meant this panel, was not actually showing
  differentiation; the real mapping lives in the center Changes list, confirmed here.

Both previously-missing halves (glyph-identity mapping, explicit refresh) are now directly
observed working from the driver's own captures. This is exercised-live-and-observed-to-work,
matching this ledger's existing bar for `PASSED` on live (non-drawn-test) evidence (cf.
F-CHG-19, F-CHG-21).

Crops used: `chg06-before/after-1715.png` region `335,95+250x220` (Changes list) and
`1305,95+380x160` (Files tree dots), 4x; single-icon crops at 12x confirm shape identity.

## F-CHG-11 (ledger line 202) — verdict: `PASSED` (up from half-proven)

Driver's socket-driven `stage_all` and `discard_all` calls are corroborated by direct sighted
read of the two new captures, matching the driver's `git status` account exactly:

- `chg11-after-stageall.png`: "Staged (3)" lists `changed.txt`, `staged.txt`, `untracked.txt`
  together — confirms `stage_all` moved the one remaining unstaged file into the index, all
  three visibly staged.
- `chg11-after-discardall.png`: only "Untracked (2)" remains, listing `untracked.txt` and a
  newly-added `untracked2.txt`; `changed.txt`/`staged.txt` are gone from the list entirely
  (clean). This is exactly `git restore --worktree` semantics — unstaged tracked-file edits
  reverted, untracked files left alone — and it is a genuine state transition (the driver
  reset the index and reintroduced dirt specifically to make `discard_all` non-trivial, not a
  no-op replay).

Combined with `unstage_all` already proven live in a prior pass, all three bulk controls on
this row are now independently confirmed working. Moving to `PASSED`.

## F-CHG-22 (ledger line 213) — verdict: `half-proven` (unchanged; evidence and owed half
refined)

The row's actual ask (`01-inventory-app.md:155`) is to "confirm each status label/glyph" for
running/needs-input/done/error/idle in the Activity section. Needs-input and idle were
already visually confirmed in a prior pass (P104 §Group 5).

This drive adds genuine new **data/process**-level proof that running/done/error are
distinct states in the app's own model: `panel.state` returned `exitStatus:"success"` for a
real completed `claude` run and `exitStatus:"code:1"` for a forced `cmd=false` failure — a
difference in kind, not just value — and `panel.scrollback` showed a live "Whisking…" spinner
line while `claude` was mid-turn. That is real progress and is worth recording.

But it does **not** close the row's visual half, and the gap is more specific than the
driver's own "I can't read glyphs" caveat suggests. I opened all three new captures directly:

- `chg22-running.png` (1400x900), `chg22-running2.png` (1715x972), `chg22-done-error.png`
  (1715x972) all show the **same "Changes" tab** from the CHG-06/11 fixture drive (`Local
  changes` list of `untracked.txt`/`untracked2.txt`) — not the terminal pane running `claude`,
  not `panel.list`, not the Activity section's contents. In `running2`/`done-error`, the
  Activity section is visible only as a **collapsed** `> Activity` row at the bottom of the
  Files panel — never expanded, never captured open.
- So these captures carry **zero** visual information about Running/Done/Error glyph
  rendering, independent of any glyph-reading limitation — the relevant UI region simply isn't
  on screen in any of the three frames. Framing them as "forced repaint captured
  chg22-running.png/running2.png **during that window**" is technically true (temporally
  simultaneous) but misleading as evidence for a row that lives or dies on what's rendered in
  the Activity section.

Verdict stays `half-proven`. The owed half is unchanged in kind (visual glyph confirmation
for running/done/error) though now narrower in a different sense: the data model is proven
correct for all three, so what's left is purely "does the Activity row actually draw a
distinct label/glyph for each" — a `tab.select` onto the Activity section (or the terminal
tab hosting the agent) plus a capture while expanded, not yet attempted on this or any prior
pass for these three states.

## F-CHG-18 (ledger line 209) — verdict: `NOT EXERCISED` (unchanged)

Independently confirmed the driver's `could-not-reach`: read `Scripts/wayland-drive.sh`
directly — `pointer_command`/`move`/`click` (lines ~253-270) each issue one atomic op over the
virtual-pointer FIFO; no button-down/move/button-up sequence exists anywhere in the file, and
no drag-shaped `ControlAction` exists as a socket bypass either. `WAYLAND-LANE.md` (line 24,
29-32) explicitly documents pointer drags as not-yet-exercised on this lane and routes them to
`DISPLAY=:1` + the drive lock, which this slice is barred from using. No route exists inside
this slice's boundary; a route does exist elsewhere (`DISPLAY=:1`), so this is an
environmental block, not a platform-impossibility — `NOT EXERCISED` is correct, matching the
row's existing verdict. `discriminating: false` as the driver stated.

## Notes for the orchestrator (not row findings)

- **Overclaim caught**: the driver's structured return marked F-CHG-22 `"claim":
  "exercised-working"`. That overstates what was actually shown — the process/exit-code half
  is real and new, but the row's visual requirement is untouched, and the captures offered in
  support don't depict the relevant UI at all (a different tab entirely), which is a stronger
  gap than the driver's own "not closed" note (which reads as only a glyph-legibility
  limitation) let on. Recommend the ledger evidence text make this literal — "no capture shows
  the Activity section" — rather than "visual rendering not confirmed", so the next driver
  knows to actually navigate there rather than re-run the same socket calls.
- F-CHG-06 and F-CHG-11 both moved to `PASSED` on the strength of this critic's own sighted
  read of the driver's captures (the driver is text-only and correctly flagged this
  limitation rather than guessing) — no disagreement with the driver's own `partially-exercised`
  / `exercised-working` self-assessment on those two rows; the sighted check simply completed
  what the driver already knew it couldn't finish itself.
