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

## `F-SID-07` — ledger line 76

**Verdict: PASSED** (`heldUp: true`, now on independent live provenance instead of a critic reading
`P104-report`)

Live re-drive, one continuous drive (right-click "tiller" -> Project Settings -> edit -> Close), never
reading the prior report: right-click on the project row opened the 4-item context menu (Project
Settings / Initialize Git repository / Show in File Manager / Remove Project); Project Settings opened
a full settings view (Display name field, Project icon picker, Colour swatches, Reset, Remove Project,
Close). Typed `F2CRIT-RENAMED` into Display name -- landed in full, and the view's own header
live-updated to `Project Settings · F2CRIT-RENAMED`
(`reference/linux-progress/wave-f2-critic/05-83-name-typed.png`). Clicked a different icon (terminal
glyph) -- the header's icon swapped live too
(`reference/linux-progress/wave-f2-critic/06-84-icon-clicked.png`). Discriminator: after navigating
back to the sidebar, the project row itself now reads `F2CRIT-RENAMED` with the new terminal icon,
not `tiller` with the default folder icon -- proving the edit persisted into the sidebar, not just the
settings header (`reference/linux-progress/wave-f2-critic/02-90-check-sidebar-after-reset.png`).

## `F-SID-08` — ledger line 77

**Verdict: PASSED** (`heldUp: true`, now on independent live provenance)

Live re-drive against a genuinely non-git folder created for this pass (`/tmp/f2crit-nongit-proj`,
confirmed no `.git` before the test): added it as a project, right-clicked its row -- "Initialize Git
repository" was enabled (not greyed, unlike the already-git `tiller` project's menu) -- and clicked it.
Instrument: real filesystem read-back and `git status`, not just the UI. `ls /tmp/f2crit-nongit-proj/.git`
now shows a real git admin directory (`branches`, `config`, ...) and
`git -C /tmp/f2crit-nongit-proj status` reports `On branch master / No commits yet` -- a genuine `git
init` ran. The sidebar's own worktree row also updated live from a bare path to a `master` branch row
(`reference/linux-progress/wave-f2-critic/03-103-after-init-git.png`). A second right-click afterward
confirmed the menu item now reads "Git is already initialized" and is disabled again, matching the new
state.

## `F-SID-09` — ledger line 78

**Verdict: PASSED** (`heldUp: true`, now on independent live provenance)

Live re-drive: right-clicked the non-git-turned-git test project's row and clicked "Show in File
Manager". Instrument: the host process tree, not a screenshot -- `ps aux` immediately showed a new
process `/usr/bin/cosmic-files /tmp/f2crit-nongit-proj`, i.e. COSMIC Files launched with exactly the
selected project's directory as its argument. Killed the spawned process afterward as cleanup. This is
stronger than the original evidence (a critic reading `P104-report`'s description of the same
behaviour) since it captures the real subprocess argv, not a rendered window.

