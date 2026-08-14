# P109 — the owed gestures, driven

**Driver: `sonnet`, as critic.** Driven on `DISPLAY=:1` (XWayland), holding
`/tmp/tiller-drive-1.lockd` for the duration of the batch. Fixture database
`TILLER_DB=/tmp/sonnet-p109.sqlite`. Gestures below are the ones named per-row
in `P106-report.md`; this report does not re-derive them, only drives them and
records what the screen showed. No verdict column — `pireview` sets verdicts
in `P108`.

**Infrastructure note.** Partway through the `F-PRJ` batch the app stopped
responding to input reliably — clicks landed but keystrokes queued for a very
long time (over a minute in one case) or were dropped outright, confirmed
non-lag by repeated captures with generous settle time and by checking the
process was not CPU-spinning (`ep_poll`, ~7-15% CPU, all threads sleeping).
The app was restarted. The first restart attempt produced no window on `:1`
at all: the launching shell had `WAYLAND_DISPLAY=wayland-1` set globally, and
the app appears to prefer a native Wayland surface over XWayland when that
variable is present, so it rendered somewhere this driving lane cannot see or
click. Relaunching with `WAYLAND_DISPLAY` explicitly unset produced a window
on `:1` immediately and fully restored normal responsiveness for the rest of
the session. Flagging this because it cost real time and because any other
lane hitting unexplained "typing does nothing" symptoms on this app should
check for a stray `WAYLAND_DISPLAY` in its launch environment before
concluding the control is broken.

## F-PRJ rows

### F-PRJ-03

Owed gesture (P106): click `+` → `Open Project…` and choose a non-Git
directory.

Clicked the sidebar `+` button. The menu showed exactly two items, top to
bottom: `Clone Repository…`, `Create Project…`. There is no `Open Project…`
entry in this menu. Capture:
`shots/22-plus-menu.png` (also confirmed earlier in the session at
`shots/01-plus-menu.png`).

Supplementary probe: pressed `ctrl+o` with the app window focused, on the
hypothesis that it might be a keyboard-only route to the same picker. No
visible change appeared in the X capture (`shots/10a-ctrlo.png`). This is
reported as inconclusive, not as a negative result: a Wayland-native portal
file picker would be invisible to this X11 capture even if it opened, per
`ENVIRONMENT.md`. The gesture named by the row — click `+` → `Open Project…`
— does not exist in the UI as driven.

### F-PRJ-04

Owed gesture (P106): same route as F-PRJ-03 (`+` → `Open Project…`),
different underlying census question. Same finding applies: the `+` menu has
no `Open Project…` entry. Capture: `shots/22-plus-menu.png`.

### F-PRJ-07

Owed gesture (P106): drive a failing clone through the visible dialog and
observe the error state.

Clicked `+` → `Create Project…`'s sibling `Clone Repository…`, which opened a
"Clone repository" dialog ("Paste a Git URL and choose where its folder
should live."). Typed `https://github.com/nonexistent-owner-zzz9/nonexistent-repo-zzz9`
into `Repository URL`. The `Destination` field showed
`/home/enzopalmisano/nonexistent-repo-zzz9`, computed live from the URL. The
submit button read `Clone repository`; after submitting it relabeled to
`Retry clone` and this text appeared below it, verbatim, in red:

```
Clone failed: git exited with status 128: Cloning into '/home/enzopalmisano/nonexistent-repo-zzz9'...
remote: Not Found
fatal: repository 'https://github.com/nonexistent-owner-zzz9/nonexistent-repo-zzz9.githttps://github.com/nonexistent-owner-zzz9/nonexistent-repo-zzz9.gitabhttps://github.com/tiller-org-nonexistent-zzz9/nonexistent-repo-zzz9.git/' not found
```

Note the `fatal: repository '...'` clause: it contains three URL fragments
run together (two copies of the typed URL plus a third, `.../tiller-org-nonexistent-zzz9/...`,
with a stray `ab` joining two of them), even though the `Repository URL`
field on screen showed only the single, current, clean value at submit time.
The field had been cleared and retyped three times before this submission.
This is reported as an observed discrepancy between the rendered field value
and the string actually passed to the underlying `git clone` invocation — not
diagnosed further, per the critic role. Capture: `shots/05a-clone-submit.png`.

### F-PRJ-10

Owed gesture (P106): drive a failing project creation through the visible
dialog and observe the error state.

Clicked `+` → `Create Project…`. Typed `Scrivania` into `Project name` (a
name colliding with an existing directory). `Parent location` showed
`/home/enzopalmisano`, and the preview line read `Creates
/home/enzopalmisano/Scrivania`. The submit button read `Create project`;
after submitting it relabeled to `Retry creation` and this text appeared
below it, verbatim, in red:

```
Creation failed: could not create /home/enzopalmisano/Scrivania: File exists (os error 17)
```

Capture: `shots/09b-create-submit-r1.png`.

### F-PRJ-11

Owed gesture (P106): right-click the project row, choose `Project Settings`,
exercise the sheet's project removal control.

First created a real project to right-click, since none existed: `+` →
`Create Project…` → typed `sonnet-p109-test` → `Create project`. This
succeeded and added a `sonnet-p109-test` row to the sidebar (capture:
`shots/26-project-created.png`).

Right-clicked the project row. The context menu showed, top to bottom:
`Project Settings`, `Initialize Git repository`, `Show in File Manager`,
`Remove Project` (capture: `shots/27-project-rclick.png`). Clicked `Project
Settings`, which opened a sheet titled `Project Settings · sonnet-p109-test`
showing the project path, a `Repository: Folder` label, a `Display name`
field, an `Initialize Git` button, a `Project icon` picker (Icon / Emoji /
Avatar tabs, an icon grid, a colour swatch row), and a `Reset` button, ending
in a `Close` link and the project id `p-2045d15e0c84e416`.

**There is no project-removal control anywhere on this sheet.** The `Reset`
button is scoped to the icon/colour picker above it (its position directly
under `Colour` makes this legible), not to project removal. The only
`Remove Project` control found anywhere in this flow is the one on the
row's right-click context menu itself, one level up from the sheet the row
names. The owed gesture's target — "the sheet's project removal control" —
does not exist; removal is not reachable from inside `Project Settings`.
Capture: `shots/28-project-settings.png`.

### F-PRJ-12

Owed gesture (P106): right-click the project row, choose `Project Settings`,
edit the display-name field, and exercise the repository-type switch.

Continuing in the same sheet as F-PRJ-11: clicked the `Display name` field
and typed `Sonnet P109 Test Display`. The field showed the typed text and,
live, the sheet's own header updated to `Project Settings · Sonnet P109 Test
Display` (capture: `shots/29b-displayname-recheck.png`). This half of the
gesture is confirmed working.

Clicked `Initialize Git` (the only repository-type control on the sheet).
The sheet itself showed no change for several seconds — `Repository: Folder`
and the `Initialize Git` button stayed exactly as before, with no busy state
shown (captures: `shots/30-initialize-git.png`,
`shots/30b-initialize-git-recheck.png`). Checking the filesystem directly
showed a real `.git/` directory had in fact been created under
`/home/enzopalmisano/sonnet-p109-test` — the action executed. Closing the
sheet and reopening `Project Settings` from a fresh right-click showed the
sheet now correctly reading `Repository: Git`, with the `Initialize Git`
button gone (capture: `shots/33-settings-reopened.png`); the row's context
menu in between also correctly showed `Initialize Git repository` greyed out
with the reason `Git is already initialized` (capture:
`shots/32-rclick-again.png`). **Finding: the repository-type switch works,
but the open `Project Settings` sheet does not live-update to reflect it —
the change is only visible on a fresh open of the sheet, even though the
context menu one level up does update promptly.**

### F-PRJ-17

Owed gesture (P106): click `New Worktree…` and exercise each
default-worktree-base option (current, pinned, primary, and no-primary),
then observe the resulting parent path.

Closed the settings sheet and clicked `New Worktree…` under the project row
(now showing a `master` / `Primary` worktree from the git-init above). This
opened a small popover titled `New worktree in Sonnet P109 Test Display`
containing exactly one control: a `branch name` text field, with the hint
text `Enter to create · Esc to cancel` beneath it (capture:
`shots/34-new-worktree.png`). No base-selection control of any kind —
current/pinned/primary/no-primary or otherwise — is present on this
popover, before or after typing into the field (capture:
`shots/35b-worktree-recheck.png`). **Finding: there is exactly one worktree
flow reachable from this control, with no default-worktree-base option to
exercise; the four-way choice named by the row does not exist in the UI.**

Typed `feature-test` and pressed `Enter`. A new `feature-test` worktree row
appeared under the project (capture: `shots/36-worktree-created.png`).
Checking the filesystem for the resulting parent path: the worktree was
created at `/home/enzopalmisano/Sonnet P109 Test Display-feature-test` — a
sibling of the project directory, named from the project's **display name**
(the one edited in F-PRJ-12, including its space and mixed case), not from
the original folder name `sonnet-p109-test`.

### F-PRJ-18

Owed gesture (P106): open `New Worktree…`, look for the location
chooser/default-parent control, and exercise it with a custom location.

Same popover as F-PRJ-17 (capture: `shots/34-new-worktree.png`): the only
control is the `branch name` field. No location chooser, default-parent
control, or any other affordance for specifying where the worktree should be
created is present. **Finding: no custom-location control exists in this
flow to exercise** — the parent path is decided entirely by the app (see
F-PRJ-17's finding: it derives from the project's display name), with no
user-facing override.
