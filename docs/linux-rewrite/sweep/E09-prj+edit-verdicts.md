# Verdicts — E09-prj+edit (F-PRJ, F-EDIT)

Adjudicated against `docs/linux-rewrite/sweep/E09-prj+edit-evidence.md` and its captures under
`reference/linux-progress/drive-E09-prj+edit/`. I did not drive this slice and did not build any
of the surfaces it touches — no source edit anywhere, no compile, no edit under `rust/`. Every
capture cited below (and several the driver did not cite) was opened and inspected directly, not
taken on the driver's prose.

Four of six rows check out close to as described; two do not. `F-PRJ-16` is marked
`exercised-working` by the driver but the row's own documented VERIFY clause requires exercising
`Open Emoji Picker`, which was clicked and produced no observable effect — the driver's own prose
says so, but the returned verdict doesn't reflect it. `F-PRJ-06`'s cited "settled" captures do not
show what the surrounding prose claims they show.

---

## F-PRJ-06 — Prevent cloning with an empty URL or while a clone is running

**Verdict: half-proven (unchanged).**

Empty-URL disablement (proven by prior evidence) is not re-claimed here and stands. The
double-submission-guard half remains structurally unreachable on this lane: no network, so any
clone attempt resolves within a single frame, and back-to-back synthetic clicks are unreliable
tooling (first click of a pair dropped in 4/4 attempts per the driver's own log) — genuinely no
observable in-flight window to race.

**But the specific supporting claim overclaims its own captures.** The JSON return and evidence.md
both assert a completed cycle — "clicked Clone repository … button relabels 'Retry clone' with
real red git-failure text," citing `06-fprj06c-doubleclick.png` (captioned as showing "the settled
single-click outcome for URL `https://x.io/a/b.git`") and `07-fprj06-settle2.png`. Neither image
shows that. Both show the pre-submission state: the button still reads **Clone repository**, the
status line still reads **Ready to clone**, and the URL field holds only a partially-typed string
(`ht` in the first, `h` in the second) — not `https://x.io/a/b.git`, not a submitted URL, and no
red error text anywhere in frame. I checked every `fprj06*`-labelled capture in the directory
(`b` and `c` sequences plus the untagged `02`–`05` set); none show a `Retry clone` state or any
failure text. The lane's typing delivery looks at least as unreliable here as its click delivery —
a real, useful observation — but it is not what the driver's prose describes.

Net effect on the row: none. The guard half was already not reachable before this drive and stays
not reachable now: the new attempt's *conclusion* holds, its *narrated supporting evidence* does
not and should not be read as a live-verified single-click failure cycle.

Captures checked: `02-f-prj-06-url-typed.png`, `03-f-prj-06-double-click.png`, `04-url-typed.png`,
`05-double-click.png`, `02-fprj06b-menu.png`…`06-fprj06-doubleclick.png`,
`02-fprj06c-menu.png`…`07-fprj06-settle2.png`. None discriminate toward the claimed outcome.

---

## F-PRJ-09 — Prevent creating a project with an empty name or while creation is running

**Verdict: half-proven (unchanged).**

Empty-name disablement (proven by prior evidence) is not re-claimed here and stands. This drive's
new evidence for the duplicate-submission-guard half is, unlike F-PRJ-06's, exactly what it's
described as: `06-fprj09b-click1.png` shows a real new sidebar entry, `e09testproj` at
`/home/enzopalmisano/e09testproj`, appearing after the first click, and the dialog gone.
`07-fprj09b-click2.png` shows the same sidebar state with the pointer landed on now-empty space —
the second click had nothing left to hit. Same structural conclusion as F-PRJ-06 (instant
local-filesystem completion leaves no in-flight window for a genuine race), but here the capture
set actually backs the narrative.

Captures: `06-fprj09b-click1.png`, `07-fprj09b-click2.png` (both discriminating).

---

## F-PRJ-14 — Set a project avatar from a GitHub remote, uploaded PNG, or favicon domain

**Verdict: half-proven (upgraded from NOT EXERCISED).**

The row's VERIFY clause names three arms: GitHub avatar, PNG upload, favicon domain. Only the
first was driven. `03-fprj14b-avatar.png` shows all three controls present (`Choose PNG…`, a
GitHub user/repo field + `Use GitHub Avatar`, a domain field + `Use Favicon` clipped by sheet
width). `06-fprj14b-ghclick1.png` shows a real accepted submission: typed `octocat`, clicked
`Use GitHub Avatar`, got the confirmation line `Current: GitHub avatar for octocat`. That's live,
verified, and matches the frame.

Missing half, named explicitly: `Choose PNG…` and the favicon-domain arm were not tried, and
whether the accepted GitHub avatar actually reaches the sidebar/project row (durability) was not
checked — a real open question given the already-logged unwired-`on_change` seam that defeats
`F-PRJ-13`/`F-PRJ-15` for the sibling Icon tab (`SEAMS.md:64`). Not claimed as a defect here since
it wasn't tested either way for this row.

Captures: `03-fprj14b-avatar.png`, `06-fprj14b-ghclick1.png` (both discriminating for the
GitHub-avatar arm only).

---

## F-PRJ-16 — Choose one emoji as the project icon or open the emoji picker

**Verdict: half-proven (upgraded from NOT EXERCISED; downgraded from the driver's claimed
exercised-working).**

The row's own VERIFY text (`01-inventory-app.md:55`) is explicit: "Choose Emoji, enter one emoji,
try invalid multi-character input, **and click Open Emoji Picker**; confirm the accepted icon and
validation behavior." Two of three required actions are genuinely, live-proven:
`06-fprj16b-setemoji-ab.png` shows invalid `ab` rejected with the exact text `Enter exactly one
emoji.`; `06-fprj16c-setemoji-result.png` shows a real single emoji (🎉, typed via `wtype` — the
lane's first proven non-ASCII input) accepted, swatch updated, no error. Both frames match the
claim precisely.

The third required action does not close. `04-fprj16d-openpicker.png` shows `Open Emoji Picker`
clicked with the field empty and nothing observably happens — no overlay, no visible state change.
The driver's own prose says as much ("that specific door in the clause is unexercised") but then
returns `exercised-working` for the row anyway, which overclaims against the clause's own explicit
third requirement. Marking `half-proven`: entry/validation half closed live, `Open Emoji Picker`
half still owed (and it is not yet known whether the no-op is a disabled/gated state or an absent
feature — that distinction wasn't tested).

Captures: `06-fprj16b-setemoji-ab.png`, `06-fprj16c-setemoji-result.png` (discriminating, entry
half), `04-fprj16d-openpicker.png` (discriminating for showing the picker half unreached, not for
supporting the row as a whole).

---

## F-EDIT-05 — Handle a file changed on disk with Reload or Keep

**Verdict: PASSED (upgraded from NOT EXERCISED).**

Full clause closed live, verified by actual buffer content rather than click landing alone.
`04-fedit05l-dirty.png` shows a genuinely dirty tab (`edited` badge, local text
`DIRTY-LOCAL-EDITline1`). After an external append while the tab stayed open,
`99-fedit05p-banner2.png` shows the exact banner text `This file changed on disk.` with `Reload`
and `Keep` controls, gated correctly: a separate clean-tab control run
(`99-fedit05k-conflict.png`) reloaded silently with no banner at all, proving the gate is on dirty
state and not a blanket external-change watcher. `99-fedit05n-keep.png` (Keep) shows the buffer
still holding the local dirty text, disk change not pulled in. `99-fedit05q-reload.png` (Reload)
shows the buffer replaced with disk content including the external line
(`external-change-line-2`), local edit discarded. Both outcomes are real content, not just a
dismissed banner.

Captures: `04-fedit05l-dirty.png`, `99-fedit05k-conflict.png`, `99-fedit05p-banner2.png`,
`99-fedit05n-keep.png`, `99-fedit05q-reload.png` (all discriminating).

---

## F-EDIT-12 — Drag a file/changed-file row into a pane

**Verdict: NOT EXERCISED (unchanged).**

`Scripts/wayland-virtual-pointer.c` was read directly: it parses exactly two operations, `move`
and `click`; `click` is hard-coded to move, sleep 25ms, press, sleep 25ms, release. No
button-down-only, button-up-only, or motion-while-held primitive exists anywhere in the binary or
in `wayland-drive.sh`'s wrappers — a drag cannot be composed from what this lane's tooling exposes.
`WAYLAND-LANE.md:24` and `:29-30` independently document the same limitation ("Pointer drags …
are not yet exercised," "still require `DISPLAY=:1` until separately proven"), so this is not a
platform-level impossibility (`UNREACHABLE`) — the X11 lane is expected to be able to prove it,
just not this Wayland-only assignment. Stays `NOT EXERCISED`, now backed by a direct source read
instead of a lock-timeout stand-in.

No captures (none needed — this is a static-code check of the drive tooling itself, correctly not
dressed up as a live attempt).

---

## Not exercised further

None — all six rows in the slice were reached and judged above.
