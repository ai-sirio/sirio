# W10-chg critic verdicts

Adjudicated against HEAD `4073297` on `linux/gpui-waku`. Captures inspected directly
(`Read`, `identify`, `compare -metric AE`) under `reference/linux-progress/wavea-W10-chg/`,
not taken from the driver's prose — including several captures the driver's own evidence
log did not cite, where the cited ones turned out to need cross-checking. Source claims
re-checked with `grep`/`sed` against the current tree; nothing edited, nothing compiled.

## `F-CHG-01` — PASSED

Live evidence and code read both hold up. `02-chg01-a-changes-tab.png` through
`05-chg01-d-tab3.png` show four visibly distinct frames as `tab.select index=0..3` walks
the tab strip: Changes (loading), Chat (composer), Terminal (bash prompt), Changes (loaded,
`Local changes (44)` with a real Untracked list) — Chat/Terminal/Changes genuinely sit as
peers in one tab strip, not modes of one right panel. Independently re-grepped
`right_panel.rs`: the Files header hardcodes the literal string `"Files"` (line 641) with
its own close button, and the file's only other "Changes" reference is an unrelated
`ActivitySurface` test fixture label (line 2041) — no toggle enum/state anywhere switches
this panel between a Files and a Changes mode. The triage's reclassification is correct:
the peer-tab model is real, live, and code-verified.

**Evidence discriminates:** yes.

## `F-CHG-03` — half-proven

The genuinely new half here — a real, non-self-healing error state — is proven. The owed
half the driver claims to have closed (a single Retry click reliably recovers the panel)
is not established by the frames actually cited, and a wider sweep of the capture
directory shows why.

- `02-chg03-h-ok-a.png` (healthy tree) vs `02-chg03-scan-err.png` (post-`chmod 000`, both
  the Changes tab and the Files panel show "Permission denied (os error 13)" with Retry):
  full-frame `compare -metric AE` = 78401-order, matches the driver's number. Real,
  distinct error UI — this directly refutes the old P104 claim that refresh produced no
  visible transition at all.
- The driver's two cited "recovered" frames, `03-chg03-scan-1.png` and
  `04-chg03-scan-2.png`, were re-measured directly (`compare -metric AE` against both the
  OK and the error frame) rather than trusted from prose. Both read numerically close to
  OK (AE ≈ 5,471 vs OK-a) — but a fresh, careful **look** at both shows why: in both
  frames the **Changes tab content** has recovered (`Local changes (3)` with all three
  files back), while the **Files right panel** — the specific surface this row is about,
  by the driver's own scoping note ("this is the Files right-panel error path... not the
  Changes tab") — is still showing `Files unavailable: Permission denied (os error 13)`
  with an unclicked-looking Retry. A full-frame AE diff cannot tell these two outcomes
  apart, because the Changes content pane is visually much larger than the narrow Files
  sidebar; a large "recovery" number can come entirely from the bigger pane while the
  actual subject of the row stays broken. This is the same pattern in five more
  uncited frames from the same drive (`04-chg03-g-after-retry-click.png`,
  `05-chg03-d-recovered.png` — note this file's own name claims "recovered" and it is
  not — `06-chg03-l-after-a.png`, `06-chg03-scan-4.png`, `07-chg03-m-after-b.png`): seven
  of eight post-click captures in this session show the Files panel still stuck.
- Only one frame in the whole directory, `07-scan-c5.png` (not among the driver's cited
  captures), shows the Files panel actually clear — `changed.txt`/`staged.txt`/
  `untracked.txt` each with a working Diff link, no error text. So the capability is real
  and does work; it just isn't reliably hit by a single click at the driver's chosen
  coordinate, and the row's own submitted evidence happens to cite two of the seven
  frames where it didn't land.
- The transient Loading-flash sub-claim remains unobserved on this lane, same as the
  driver states — code-verified only (`right_panel.rs:698-708`).

**Owed:** a capture (any exists already, uncited — `07-scan-c5.png`) that actually shows
the Files panel itself recovering should be the one cited, and the loading flash remains
unproven live.

**Evidence discriminates:** no — the driver's chosen frames do not distinguish "the Files
panel recovered" from "the Changes tab recovered while the Files panel stayed broken";
both look like a large positive diff by the full-frame metric used.

## `F-CHG-05` — PASSED

Sighted read directly confirms the claim the driver could not (text-only agent, per their
own evidence). `02-chg05-b-clicked.png` shows the Files panel focused with `staged.txt` /
`changed.txt` / `untracked.txt` listed. `02-chg05-c-after-enter.png` — after
click→Down→Down→Return — shows a **new editor tab named `untracked.txt`** in both the tab
strip and the worktree's tab list, with its real content (`unt...`) rendered as numbered
source (`plain text` badge, line `1`). This is not merely "a large repaint"; it is
literally the opened-file view `on_file_key`'s Enter-on-a-file branch
(`right_panel.rs:614-617`, `open_file` → `RightPanelEvent::OpenFile` →
`workspace.add_file_tab`, confirmed live at `main.rs:2738`) is documented to produce.
Re-measured the driver's zero-floor control directly: `02-chg05-ctrl-c.png` vs
`04-chg05-ctrl-e.png` (same-resolution, no-op pair) diff to AE = 0 exactly, and the
click→Return pair diffs to AE = 154,544 — both numbers match the driver's report.

**Evidence discriminates:** yes.

## `F-CHG-18` — NOT EXERCISED (unchanged)

Independently re-read `changes.rs:993` (`on_drag` sits in the row's production render
closure, not `#[cfg(test)]`) and `tiller_terminal/src/lib.rs:1448-1450`
(`on_drop::<(PathBuf, String)>` wired to `receive_diff_drop`) — both real, non-test
code, confirming the driver's premise. Independently re-read `Scripts/wayland-drive.sh`:
the exported action vocabulary is exactly `ctl pointer_command move click type key shot
title` — `click`/`move` are each one atomic virtual-pointer op, no press-hold/motion/
release primitive exists. `WAYLAND-LANE.md:24` states outright that "Pointer drags,
right-click, modifiers/chords and IME/non-ASCII text are not yet exercised" and routes
them to `DISPLAY=:1` (line 275), which this slice is explicitly barred from using. No
capture is possible or expected here.

**Evidence discriminates:** no (nothing to observe — the claim is that no primitive
exists, and the search for one came up empty on both the shell-script and
system.capabilities fronts).

## `F-CHG-22` — half-proven (unchanged; the claimed new half is not actually shown)

This is the same "accidentally re-shows the Changes tab / never actually expanded"
failure mode the row was already flagged for, reproduced in the driver's new attempt too.
Cropped and 3×-zoomed the exact activity-box region (`405x171+1310+801`, per the driver's
own geometry) in all three officially cited frames:

- `02-final2-collapsed.png`, `03-final2-expanded-1.png`, `04-final2-expanded-2.png` all
  show the **same collapsed Activity row** — chevron pointing right (`>`), no status rows
  underneath, in every one of the three. The only visible difference between "collapsed"
  and "expanded-2" is the mouse cursor arrow sitting over the "Activity" label in the
  latter — nothing in the panel itself changed shape, size, or content.
- The measured diffs the driver cites (full-frame AE 1,799; crop AE 219) are real numbers
  — `compare -metric AE` reproduces them exactly — but they are explained entirely by the
  cursor glyph appearing in frame, not by any expand/collapse state change. A small,
  concentrated diff in exactly the region a UI element renders is consistent with *either*
  a real content change *or* a cursor now sitting in that same corner; this pair of
  screenshots cannot tell the two apart, and a direct 3× zoom resolves it: no rows, no
  chevron rotation.
- Checked further: an earlier attempt in the same directory using a different naming
  scheme (`02-chg22-a-before-expand.png`, `04-chg22-e-expanded-a.png`,
  `04-chg22-i-expanded-1.png`) shows the identical pattern — collapsed chevron, no rows,
  in both the "before" and "after-expand" frames of that attempt too. Across two separate
  attempts in this session, no single frame anywhere in the directory shows the Activity
  section actually open with status rows visible.
- Also noted: `03-final2-expanded-1.png` is `1400x900`, not `1715x972` like its neighbors
  — the claimed click coordinate `(1512, 940)` is off the right edge of a 1400-wide
  window, which is consistent with that particular click landing nowhere useful.

The data-tier half of this row (status mapping, glyph rendering) was already established
in an earlier sweep and is unaffected by this finding. The visual half — an actually
expanded Activity section with the running/needs-input/done trio rendered — remains
unproven; this drive's new captures do not supply it despite the claim.

**Evidence discriminates:** no — the cited "expanded" frames are visually indistinguishable
from "collapsed" apart from cursor position.

## Summary of disagreements with the driver

Two of the five rows in this driver's return do not survive a direct look at the pixels:

- **F-CHG-03**: claimed `exercised-working`/full recovery via Retry; the two cited
  "recovered" frames actually show the Files panel — the row's own stated subject — still
  broken, with only the (separate) Changes tab content having recovered. Downgraded
  `FAILED — defective` → **half-proven**, not `PASSED`.
- **F-CHG-22**: claimed `exercised-working` for an expanded Activity section with three
  agent statuses visible; all three cited frames show the section still collapsed, with
  no status rows, differing only by mouse cursor position. Verdict stays
  **half-proven**, unchanged from the ledger — the owed half is still owed.

F-CHG-01 and F-CHG-05 hold up under direct inspection and move to `PASSED`; F-CHG-05 in
particular is confirmed more strongly than the driver's own evidence claims, since the
driver (correctly) flagged themselves as unable to read the pixels and this critic can.
F-CHG-18 is a clean, unchanged `NOT EXERCISED` — no primitive exists on this lane, verified
independently.
