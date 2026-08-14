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

## F-CHAT rows

**Infrastructure note.** The app stalled a second time mid-batch: two widgets
in different parts of the UI (the composer textarea and the sidebar `Filter`
field) both accepted a mouse click — focus moved, the border went orange —
but no typed character ever appeared in either, confirmed non-lag by a 15s
passive wait with zero further change. `app.log` showed three repeats of
`[session] failed to persist the layout: sqlite error: UNIQUE constraint
failed: tab.id` right before the stall; this is reported as an observed
correlation, not a diagnosed cause — a bug found is a finding, not a task to
fix. The app was restarted the same way as the first stall
(`env -u WAYLAND_DISPLAY DISPLAY=:1 TILLER_DB=/tmp/sonnet-p109.sqlite`), which
restored full responsiveness; all F-CHAT rows below were driven (or
re-confirmed) on the restarted instance. Separately: the composer's centred
placeholder path (`/home/enzopalmisano/.claude`) turned out to be the app's
default-selected project on a fresh launch, unrelated to the bound project
`sonnet-p109-test` shown in the Files panel/status bar — a false lead noted
in the prior session, resolved here, not a finding.

### F-CHAT-02

Owed gesture (P106): drive a request through the visible composer and
observe whichever state — healthy, failed, or auth-required — naturally
results.

Selected the `master` worktree's `Chat` tab for `sonnet-p109-test`, clicked
the `Message…` textarea, and typed `Run the shell command pwd and tell me
the exact output.` (capture: `shots/56-composer-typed2.png`). Clicking the
send button (the circular up-arrow at the right of the control row) had no
visible effect after a 2s wait; pressing `Enter` with the textarea focused
did send it: the textarea cleared, the `Chat` tab picked up a green
checkmark, the context indicator advanced from `0%` to `5%`, and the control
row itself changed shape — from a single unlabeled `Opus Plan Mode` pill to
`Ask ▾` next to `Model  Opus Plan Mode  XHIGH  ▾` (captures:
`shots/57-after-send.png`, `shots/59-transcript-wait4.png`).

**Finding: the transcript pane between the composer and the panel bottom
stayed completely empty** — no user-message bubble, no assistant response,
no auth banner, no Retry affordance — through a 14s wait and after forcing a
repaint by switching to the `Terminal` tab and back (captures:
`shots/60-transcript-wait14.png`, `shots/61-transcript-after-tabswitch.png`).
The context indicator's advance to `5%` and the tab's checkmark are the only
evidence the turn was actually sent; nothing about its outcome (healthy,
failed, or auth-required) is observable in the rendered UI. This confirms,
via the visible composer this row required, the same empty-transcript
condition `P106` observed over the control socket.

### F-CHAT-16

Owed gesture (P106): click the model pill, type a search, observe a
no-match result, inspect any Recommended labels, and select a model.

Before any turn had been sent, the control row showed one plain-looking pill
reading `Opus Plan Mode`, with no visible chevron. Clicked it at four
distinct coordinates spanning its text (`555,191`, `520,191`, `508,191`,
`560,189`, all window-relative) across two app instances (one pre-restart,
one fresh post-restart) — every click just focused the composer textarea
(orange border) and opened nothing; a `+`-button click alongside it, by
contrast, visibly registered (a hover/pressed background box appeared),
confirming clicks were reaching the row and this was not a lag artifact
(captures: `shots/41-model-pill-click2.png` through
`shots/46b-attach-plus-recheck.png`, `shots/54-pill-click-fresh.png`).

After sending the F-CHAT-02 message, the same control re-rendered as
`Model  Opus Plan Mode  XHIGH  ▾` — now with a visible chevron. Clicking the
chevron (`728,191`) did open a popover this time (capture:
`shots/62-model-chevron-click.png`), headed `Opus Plan Mode` over an
`Effort` row of six buttons: `Default`, `Low`, `Medium`, `High`, `Xhigh`
(selected, highlighted blue), `Max`. **This is an effort-level selector, not
a model picker**: no search field, no model list, and no `Recommended`
label anywhere in it. Clicking the `Opus Plan Mode` header text inside the
popover did nothing further (capture: `shots/63-popover-header-click.png`).
Clicking `High` did work functionally — the badge live-updated from `XHIGH`
to `HIGH` (capture: `shots/67-select-high.png`) — confirming the control
itself is wired, just not to the search-and-select flow the row names.

**Second finding: this popover would not dismiss.** Clicking outside it
(`700,400` and `700,500`, both well clear of its bounds), pressing `Escape`,
and re-clicking its own trigger chevron each left it open and unchanged
(captures: `shots/65-dismiss-check.png`, `shots/66-chevron-reclick.png`,
`shots/68-dismiss-attempt2.png`). It was still open, showing `HIGH`
selected, when the app was restarted for the stall described above. No
route to a searchable, Recommended-labelled model list was found anywhere
in this flow.

### F-CHAT-18

Owed gesture (P106): click the context indicator and inspect its percent,
token, and cost lines; observe the input/output/cache breakdown.

Clicked the `0%`/`5%` context indicator (`1161,191`) both before sending a
turn and again after (fresh app instance, capture:
`shots/55-context-indicator-fresh.png`). Neither click opened anything —
same "textarea gets focus, nothing else happens" outcome as the pre-turn
model-pill clicks in F-CHAT-16. **No percent/token/cost popover, and no
input/output/cache breakdown, was reached anywhere in this flow.**

### F-CHAT-21

Owed gesture (P106): click the Thought disclosure when a Thought entry is
present.

Depends on a rendered transcript to produce a Thought entry to click. Per
F-CHAT-02 above, the transcript pane stayed empty through the entire
send-and-wait cycle on the visible composer, so no Thought entry ever
appeared and no disclosure control was reached. This confirms `P106`'s
same reading through the route this row specifically required.

### F-CHAT-22

Owed gesture (P106): exercise the group disclosure against a transcript
containing grouped steps or an older turn.

Same dependency and same result as F-CHAT-21: the transcript never rendered
any content, so no grouped-step or older-turn entry, and no group-disclosure
control, was ever reached.

### F-CHAT-23

Owed gesture (P106): expand/inspect a completed tool row and exercise
Dismiss.

Restarted the app for a clean instance, then sent a prompt designed to
produce a tool call: `Please run pwd and then create a new file called
p109-test.txt with the text hello in it.` (capture:
`shots/70-second-send.png`). The composer went to a `working` state (orange
dot, `Type to queue for the next turn…` placeholder, send button replaced by
a stop/square icon) and a new banner appeared above it reading `? Question
waiting · Write p109-test.txt` with a `Show` link and a context percentage
that climbed to `25%` with an active spinner (capture:
`shots/71-working-wait10.png`) — evidently `Opus Plan Mode` gates a write
tool call behind an approval question, consistent with a "plan" mode.

**Finding: the approval question itself was never reachable.** Clicking
`Show` (`1192,124`), clicking the banner's body elsewhere (`700,124`), and
clicking the composer's `Stop` button (`1201,241`) each did nothing —
no confirmatory change even after waits, and the banner, `working` state,
and `25%` figure stayed frozen and identical across all three attempts
(captures: `shots/72-question-show.png`, `shots/73-question-show-recheck.png`,
`shots/75-banner-body-click.png`, `shots/76-after-stop.png`). Checked the
filesystem directly: `p109-test.txt` was never created under
`/home/enzopalmisano/sonnet-p109-test`, confirming the turn was genuinely
stuck waiting on a question with no visible way to answer it, not merely
slow.

The only control that did anything was the `×` on the `Chat` row inside the
`Activity` panel at the bottom of the Files sidebar (`1695,842`) — but this
closed the entire Chat tab outright (it disappeared from the tab strip and
from Activity, leaving only `Terminal`) rather than cancelling just the
pending question or turn (capture: `shots/77-activity-x-click.png`). **No
completed tool row was ever produced to expand or Dismiss** — the flow got
stuck at the pending-question stage every time a tool call requiring
approval was attempted, and the only recovery route destroys the surface
rather than resolving the question.

### F-CHAT-31

Owed gesture (P106): produce a safe tool diff, open the preview, and
activate a file location.

The same send described in F-CHAT-23 was the attempt to produce this: a
file-write request that would generate a diff-bearing tool call. It hit the
same stuck `Question waiting` state before any diff, tool row, or file
content was ever rendered (same captures as F-CHAT-23). **No diff preview or
file-location control was reached** — the approval gate blocks the flow
before a tool result exists to have a diff or location in.

### F-CHAT-34

Owed gesture (P106): open the chat history menu, browse/open a retained
session, and exercise delete with confirmation.

Checked every plausible entry point for a chat-history menu before landing
on one:

- Composer's `...` overflow menu (`1096,191`): only two items, `Follow
  Edited Files` and `New Conversation` — no history option.
- Tab-strip `+` button's `New Chat` submenu: only an agent choice, `Claude
  Code` / `Codex` — not a history browser.
- Right-clicking the sidebar's `Claude Code` row under the project tree:
  produced no context menu at all, only the standard hover `×` close
  affordance already documented elsewhere.
- Right-clicking the `Claude Code` tab in the top tab strip (`620,60`,
  `rclick`): **this did open a menu** (capture: `shots/84-tabstrip-rclick.png`),
  reading top to bottom: `Close Tabs to the Right` (greyed, reason `already
  the last tab`), `Move Earlier`, `Move Later` (greyed, `already the last
  tab`), `Attach to Current Terminal` (greyed, `select another terminal
  tab`), `Move to This Pane` (greyed, `no other tab is available`), `Move to
  Other Pane` (greyed, `no other pane is available`), then a divider, then
  `Resume Chat` — the only item with wording adjacent to "history."

Clicked `Resume Chat` (`600,552`). It did not open a list, picker, or any
browsing UI. It opened a brand-new third tab titled `Chat` (distinct from
the existing `Claude Code` tab — both now visible side by side in the tab
strip and in the sidebar under the project), with a completely empty
transcript pane, an `Ask ▾` control, `Model  Opus Plan Mode  XHIGH  ▾`, and
`0%` context (captures: `shots/85-resume-chat-click.png`,
`shots/86-resume-chat-click2.png`) — unchanged after a further 3s settle
wait (`shots/87-resume-chat-settled.png`). This is despite the project
having prior chat activity in this same app instance: the `Claude Code` tab
still open alongside it carries the F-CHAT-02/23 turns driven earlier in
this session, so retained history did exist to resume into, and `Resume
Chat` did not surface it.

**No chat-history menu was found anywhere in the flows checked, and the one
control whose label suggests history (`Resume Chat`) opens a new empty chat
rather than browsing or resuming any retained session.** Browsing a session
list, opening a retained session, and exercising delete-with-confirmation
were none of them reached — there is no list to browse or delete from.

## F-SET / F-SID rows

**Infrastructure note.** Two more incidents this batch, both recovered with
the same `env -u WAYLAND_DISPLAY DISPLAY=:1 TILLER_DB=/tmp/sonnet-p109.sqlite`
relaunch used earlier: (1) the same severe stall pattern recurred a third
time — clicking sidebar items in Settings (`Agents`/`Appearance`/`General`)
stopped switching sections at all, confirmed by two separate no-op clicks —
and was fixed by a restart. (2) Immediately after that restart, before any
gesture could be driven, the app process **exited on its own with no error,
no panic, and no signal trace in `app.log`, `dmesg`, or `coredumpctl`** —
`xwininfo` showed no app window and `kill -0` on the tracked PID found no
such process. A second relaunch on the same `TILLER_DB` came up clean and
stayed responsive for the rest of this batch. Reporting both as findings,
not debugging further per the "do not edit `rust/`" rule. Separately: a tight
shell loop of `xdotool key BackSpace` calls (0.2–0.4s apart) intermittently
registered only the first keypress and dropped the rest — the same field
reliably accepted a Backspace when it was sent as an isolated call with a
full tool round-trip between presses. Recorded as an observation about the
drive lane's event delivery under rapid repetition, not a claim about the
app.

### F-SET-16

Owed gesture (P106): click the `Search agents` field and type into it,
click `Refresh`, and observe whether an updated timestamp appears — the
search/refresh half was state-observed but gesture-owed; no timestamp was
observed under either PATH condition.

Opened `Settings → Agents` (gear icon at `27,948` in the bottom status bar,
then the `Agents` sidebar item) on a freshly restarted instance. Clicked
`Refresh` (`1345,119`) first with an empty search field: **zero visible
change** — no spinner, no reordering, no timestamp text anywhere on the
five-row list, before or after, byte-identical captures
(`shots/101-agents-fresh.png`, `shots/102-refresh-fresh.png`).

Then clicked the `Search agents` field (`705,119`) and typed `zzz-nomatch`:
the field accepted all eleven characters and the entire five-row list (
`Claude Code`, `Codex`, `OpenCode`, `Pi`, `Oh-My-Pi`) disappeared — filtered
to nothing, with **no "no results" message or empty-state text of any kind**,
just blank space under the search bar (`shots/105-search-nomatch-r4.png`).
Cleared the field back to empty (isolated `BackSpace` presses, one per
capture, per the infra note above) and the full five-row list reappeared
unchanged (`shots/108-search-fully-cleared.png`). **No updated-timestamp
control or text was ever found on this screen, refreshed or not.**

### F-SET-21

Owed gesture (P106): click the File icons chooser control, select the
second icon set, and observe the change — the chooser surface was visible
in P106 but no click could be delivered.

Opened `Settings → Appearance`. The `Files` group shows a `File icons` row
with a single button reading `Material` (`1335,578`). Clicked it: **no
dropdown, popover, or menu opened** — the capture before and after the
click is pixel-identical (`shots/89-gear-click.png`,
`shots/90-fileicons-click.png`). No second icon set was ever presented to
select, so no change was observed. `Material` behaves as a static label,
not an interactive chooser, on this build.

All five rows below were driven in one continuous sidebar session against
two projects: `.claude` (a plain, worktree-less folder project) and `Sonnet
P109 Test Display` (containing worktrees `master` [Primary] and
`feature-test`).

### F-SID-11

Owed gesture, per `P106-report.md`'s numbered close-out batch item 4 (used
as the authoritative gesture text for this row, per the task instructions):
right-click a worktree row and confirm the menu shows `Set/Unset Primary`
plus the seven New-Tab items, and **no** `Remove Worktree` entry.

Right-clicked `feature-test` (`150,328`, capture:
`shots/110-sid11-rclick-featuretest.png`). Menu, top to bottom, verbatim:
`Set Primary`, `New Terminal`, `Claude Code`, `Codex`, `OpenCode`, `Pi`,
`Oh-My-Pi`, `New Chat`. That is exactly `Set Primary` + the seven New-Tab
items named in the close-out batch, and **no `Remove Worktree` item is
present anywhere in the menu**, confirming the census's missing-half claim.

**Side finding:** this menu could not be dismissed by `Escape` or by
clicking outside its bounds in the main content pane — the same
undismissable-popover pattern already reported for the model-pill Effort
selector in F-CHAT-16. While attempting to dismiss it, a stray click landed
on the menu's own `Set Primary` item and actually changed the primary
worktree from `master` to `feature-test` as an unintended side effect
(visible in every capture from `shots/113-reclick-row-dismiss-test.png`
onward) — itself further evidence that this app's context menus have no
safe outside-click dismissal.

### F-SID-15

Owed gesture, per the same numbered close-out batch, item 5: click a
folder-project row (a project with no worktree children) and observe what a
worktree-less project opens to.

Clicked the `.claude` project row (`114,140`,
`shots/114-sid15-claude-folder-click.png`): **nothing opened in the main
content pane** — the previously-active worktree's tab content (`feature-test`'s
`Terminal`) stayed exactly as it was, unchanged pixel-for-pixel except for a
hover affordance (a disclosure chevron and a settings-gear icon) appearing
on the row itself. Clicked the chevron (`46,140`,
`shots/115-sid15-chevron-click.png`): it flipped from `>` to `v` (expanded)
but **disclosed zero child rows** — there is nothing under a worktree-less
project to show. A folder-project row is inert as a click target beyond
this local disclosure toggle; it does not select, open, or navigate
anywhere.

### F-SID-16

Owed gesture (P106, per-row): drag one project row above/below the other in
the Projects list, release, and confirm the order changes and survives a
relaunch.

Drove two independent real press-move-release drags (`drag()`, 12–15
interpolated steps) of the `.claude` project row down past `Sonnet P109
Test Display`: first to `(150,140)→(150,230)`
(`shots/116-sid16-drag-projects.png`), then to `(150,140)→(150,199)`
(`shots/117-sid16-drag-projects-v2.png`) — both captures showed the
original order (`.claude` above `Sonnet P109 Test Display`) completely
unchanged immediately after the drag. **The reorder did eventually take
effect**, but only became visible two unrelated actions later: a capture
taken after a subsequent mousemove-only hover event
(`shots/119-sid06-hover-featuretest.png`) showed the order flipped to
`Sonnet P109 Test Display` above `.claude`. This is a stale-repaint
finding, not a "drag doesn't work" finding — the underlying reorder
happened at drag-release time, but the sidebar did not visually repaint
until a later, unrelated input event forced it to. Relaunch-persistence of
the new order was not separately checked (the app was not restarted again
before the lock was released for this batch).

### F-SID-17

Owed gesture (P106, per-row): drag one worktree row to the other worktree's
position within the same project and confirm the order changes.

Dragged `feature-test` up onto `master`'s position within `Sonnet P109 Test
Display` (`drag 150 317 150 258 15`,
`shots/118-sid17-drag-worktrees.png`): order stayed `master` then
`feature-test`, and — unlike the F-SID-16 project drag above — it was
**still unchanged** in the next capture taken after a subsequent hover
event (`shots/120-sid06-hover-featuretest-v2.png`), ruling out the same
stale-repaint explanation. **Worktree-row drag reorder was driven with a
genuine press-move-release gesture and produced no observed effect,
immediate or delayed.**

### F-SID-06

Owed gesture, per the same numbered close-out batch, item 3: hover a
worktree row, click the `×` remove control, and observe whether any
confirmation prompt appears.

Hovered `feature-test` to reveal its row-level `×` (distinct from the `×`
on its child `Chat`/`Terminal` tabs — confirmed by first mis-targeting the
child-tab close control at the same screen position before the sidebar's
delayed repaint settled, `shots/119-sid06-hover-featuretest.png`, then
re-hovering the worktree row itself at its corrected position,
`shots/120-sid06-hover-featuretest-v2.png`). Clicked the worktree row's `×`
at `349,258` (`shots/121-sid06-x-click.png`): the capture immediately after
still showed `feature-test` present — consistent with the same
stale-repaint pattern seen in F-SID-16. A second click at the same
coordinate (`shots/122-sid06-x-click-retry.png`) landed on `New
Worktree…` (which had shifted up to that position once the first click's
removal actually repainted) and opened its inline branch-name creation
prompt as an accidental side effect; that prompt was cancelled with
`Escape` (`shots/123-sid06-after-cancel.png`).

**`feature-test` was removed with no confirmation prompt of any kind** —
no dialog, no banner, nothing to answer — matching the P106 census's ledger
note that this route "confirms nothing." The removal also deleted the
underlying worktree directory on disk: the Files panel, still pointed at
the now-gone `/home/enzopalmisano/Sonnet P109 Test Display-feature-test`,
switched to `Files unavailable: No such file or directory (os error 2)`
with a `Retry` button (visible in `shots/122` and `shots/123`).

## F-TAB rows

### F-TAB-01

Owed gesture per row prose: "the clause's document tab (file open is a
picker/Files-panel click) and the modify-a-document dirty route (typing)."

Drove the Files-panel click route. After re-selecting `master` (the prior
worktree removal had left the Files panel pointed at the deleted
`feature-test` path, showing the same `Files unavailable` error noted
above; clicking `master` at `150,210` restored it to
`/home/enzopalmisano/sonnet-p109-test`), expanded `.remember` in the Files
panel (click on the row at `1330,141`, `shots/135-tab01-remember-retry.png`)
to reveal children `logs`, `tmp`, `.gitignore`.

Clicking a child row inside the expanded folder — tried three times with
full isolated single clicks (`click()`, one per tool call, no batching):
`.gitignore` at `1345,246` (`shots/136`, then again isolated at
`shots/142-tab01-gitignore-isolated.png`) and the `logs` subfolder at
`1330,176` (`shots/138-tab01-logs-clicked.png`) — every time collapsed
`.remember` back to closed instead of acting on the child: no document tab
opened in the tab strip (still only `Chat`/`Terminal`/`Claude Code`), and
`logs` did not expand into its own children. Re-expanding `.remember` and
retrying reproduced the same collapse each time, with isolated clicks (full
round-trip settle between each) ruling out the rapid-input-coalescing
explanation documented earlier in this report — this reproduced cleanly on
single, separately-dispatched clicks.

**Finding: the Files panel's expanded-folder child rows are not
individually clickable on this lane — any click on a child (file or
subfolder) collapses the parent folder instead of opening/expanding the
child.** This blocks the owed gesture's document-tab route entirely: no
document tab could be opened via Files-panel click, so neither the
document-kind tab icon nor the modify-a-document dirty route (typing) was
reachable — both conjuncts are unexercised as a direct consequence of this
bug, not skipped.

### F-TAB-11

Owed gesture (close-out batch item + row prose, consistent): "nothing
exercisable — both the clause's 'resize until ineligible' and 'sole tab'
trials dead-end at a menu that has no disabled-reason surface." The row
prose adds that on the original census lane the right-click itself was
input-gated; on this `DISPLAY=:1` lane it is not, so the right-click was
driven directly rather than assumed unreachable.

Drove the sole-tab trial: switched to the `Terminal` tab (only pane, no
splits present) and right-clicked the terminal body.

- Right-click on empty terminal area away from the prompt (`700,400` and
  `700,500`, `shots/145-tab11-rclick-soletab.png`,
  `shots/147-tab11-rclick-empty-area.png`) opened no menu at all.
- Right-click near the shell-prompt line (`600,150`) opened a small context
  menu whose only visible entry, verbatim, is **`Copy`** —
  `shots/146-tab11-rclick-retry.png`, reproduced again identically at
  `shots/148-tab11-rclick-confirm.png`. No `Split Left`/`Split Right`/
  `Split Above`/`Split Down` items, no disabled/greyed entries, no reason
  text of any kind.

This confirms the row's conclusion, and adds a detail beyond it: in the
sole-tab state the terminal's right-click menu doesn't merely omit a
disabled-reason label on a split item — it omits the split items
themselves, showing only a plain-text `Copy` entry. "Resize until
ineligible" was not separately driven — with a single pane and no splitter
present, there is nothing on screen to resize.

Side note: the F-TAB-11 `Copy` menu from `600,150` remained open and did
not dismiss on `Escape` or on an outside click at `700,700` (matching the
undismissable-popover pattern already on record for the F-SID-11 worktree
menu and the F-CHAT-16 Effort selector). It was not chased further; it
closed on its own as a side effect of the F-TAB-18 drag below.

### F-TAB-18

Owed gesture: "drag a tab to a new strip position and drop" (close-out
batch, matching row prose verbatim).

Starting order was `Chat`, `Terminal`, `Claude Code` (the `Claude Code`
tab present in this worktree from earlier in the session — see prior
sections). Dragged the `Claude Code` tab from `745,60` to `405,60` (before
`Chat`) using the lane's real press-move-release `drag()` helper, 12
intermediate move steps.

Result — `shots/151-tab18-after-drag.png`: the strip re-ordered to `Chat`,
**`Claude Code`**, `Terminal`. The sidebar's per-worktree tab list under
`master` updated to the same new order. `Terminal` (the tab active before
the drag) remained the active tab, now in the third slot. The drag also
had the side effect of dismissing the stray `Copy` menu noted under
F-TAB-11.

### F-TAB-24

Owed gesture: "begin a tab drag, press Escape before dropping" (close-out
batch, matching row prose verbatim: "observe the strip return to its
original order (expected to fail per the re-verified absence [of a cancel
handler] — but the trial is what converts it)").

Starting order (post F-TAB-18): `Chat`, `Claude Code`, `Terminal`,
`Terminal` active. Used the lane's `drag_escape()` helper — mousedown on
`Terminal` at `710,60`, move to `420,60` (over the `Chat`/`Claude Code`
boundary), press `Escape`, then mouseup.

Result — `shots/152-tab24-after-drag-escape.png`: the strip order is
unchanged, still `Chat`, `Claude Code`, `Terminal`, `Terminal` still
active. No reorder occurred. This is consistent with the row's own
prediction (no drag-cancel handler exists in the source per the
re-verified `grep`), though from the screen alone it cannot be
distinguished whether `Escape` actively cancelled the drag or whether the
drop simply never committed once `Escape` fired mid-drag — the observation
is that the order held, not which code path produced that.

### F-TAB-23

Owed gesture: "right-click a terminal pane, click each of the four Split
items" (row prose).

Right-clicked the (sole) `Terminal` pane at several distinct locations,
each isolated (single `rclick()`, full round-trip settle before the next
action):

- On the shell-prompt text (`600,150`): opened a small menu whose only
  entry, verbatim, is **`Copy`** — `shots/145-tab11-rclick-soletab.png`,
  reproduced at `shots/146`/`shots/148`.
- On the terminal's header pill (`448,88`): opened a larger menu, verbatim
  top to bottom **`Copy`**, **`Paste`**, **`Copy Context`** —
  `shots/154-tab23-rclick-pill.png`. This menu also proved undismissable
  by outside-click (`700,700`) and by clicking its own `Copy Context` item
  (`shots/155`, `shots/156` — no visible change either time); it only
  closed as a side effect of switching tabs away and back.
- On empty terminal body away from any text (`700,400`/`700,500`, twice,
  isolated): opened **no menu at all** —
  `shots/145-tab11-rclick-soletab.png` (upper region), `shots/147`,
  `shots/161-tab23-rclick-empty-final.png`.

**No right-click at any tried location produced `Split Left`, `Split
Right`, `Split Above`, or `Split Down`.** Every menu that did open was a
plain clipboard menu (`Copy` alone, or `Copy`/`Paste`/`Copy Context`)
scoped to text/terminal content, not a pane-level menu. None of the four
Split items was reachable this way, so none could be clicked — the owed
gesture's four-item trial did not run because the menu it depends on did
not appear anywhere it was tried.

### F-TAB-28

Owed gesture: "press ctrl-w on a clean active tab (expect close), then on
a dirty one (expect the confirm; Cancel keeps the tab)" (close-out batch,
matching row prose verbatim).

Drove the clean-tab half first: made `Chat` (no messages sent, no dirty
indicator) the active tab via the sidebar row (the tab-strip click on
`Chat` at `440,60` intermittently failed to switch — reproduced twice,
`shots/162`/`shots/163` — the sidebar route at `150,258` did switch,
`shots/164-tab28-sidebar-chat-click.png`), then pressed `ctrl-w`.

- First press: no change — `Chat` remained open and active
  (`shots/165-tab28-ctrlw-chat.png`).
- Repeated, isolated: no change again (`shots/166-tab28-ctrlw-retry.png`).
- Repeated a third time after explicitly clicking the tab header first (to
  rule out focus sitting in the empty `Message…` composer swallowing the
  chord): still no change (`shots/167-tab28-ctrlw-tabfocus.png`).

Drove the dirty-tab half: switched to `Terminal` (live PTY, carries
`tab_is_dirty` per the F-TAB-01 census) and pressed `ctrl-w`.

- No confirm dialog appeared, and the tab did not close
  (`shots/169-tab28-ctrlw-terminal.png`) — byte-for-byte the same screen
  as before the keypress.

**Finding: `ctrl-w` produced no observable effect on either tab, clean or
dirty, on this lane** — no close on the clean tab, no confirm dialog on
the dirty one. The owed gesture's two-part trial (close on clean, confirm
on dirty) could not be completed because the first step never fired.
