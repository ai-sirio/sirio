# Wave F2-project — re-verification verdicts

Instrument: `Scripts/wayland-drive.sh` (label `f2critic`), live app at HEAD, `reference/linux-progress/wave-f2-critic/`.

## `F-PRJ-05` — ledger line 98

**Verdict: FAILED — defective**

Live re-drive: `+` -> `Clone Repository...` opens the form and Cancel is present (both confirmed). But
typing the full URL `https://github.com/octocat/Hello-World.git` into the Repository URL field drops
all but 1 of 44 characters -- only `h` landed -- so "Destination derived live" resolves to
`/home/enzopalmisano/h`, not the claimed `Hello-World` destination the PASSED evidence recorded.
Frame: `reference/linux-progress/wave-f2-critic/05-43-typed.png` (field outlined active, contents `h`,
Destination line reads `/home/enzopalmisano/h`, Clone repository button enabled on that garbage
input). This independently reproduces the character-drop defect already filed against the same form
under `F-PRJ-06` (`WAYLAND-LANE.md`/P123) -- the mechanism that live-derives Destination from
keystrokes does work, but the row's actual claim (a real URL typed in, correctly derived) is not
achievable as the form now behaves.

## `F-PRJ-08` — ledger line 101

**Verdict: PASSED** (`heldUp: true`)

Live re-drive, fresh instance: `+` -> `Create Project...` opens the form with every element the PASSED
evidence names -- Project name field (placeholder `project-folder-name`), Parent location
`/home/enzopalmisano`, live `Creates /home/enzopalmisano/` preview, Create project button, Cancel.
Frame: `reference/linux-progress/wave-f2-critic/03-51-create-form.png`. Discriminator beyond the
default state: typed a distinguishing name and watched the preview live-update to
`Creates /home/enzopalmisano/f2cr` and the Create button go from disabled-grey to enabled
(`reference/linux-progress/wave-f2-critic/05-63-typed.png`), so the mechanism the row claims is real,
not just a static screenshot. Note for whoever next touches `F-PRJ-09` (not re-judged here, out of
this slice): the name field also dropped keystrokes this attempt (4 of 16 chars landed, `f2cr`),
the same defect class documented for the Clone form's URL field under `F-PRJ-06` -- `F-PRJ-09`'s
"landed in full" evidence may not be perfectly repeatable across attempts.

