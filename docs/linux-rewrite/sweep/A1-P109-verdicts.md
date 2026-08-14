# A1-P109 adjudication verdicts

Adjudicator pass over the 32 rows listed in `docs/linux-rewrite/sweep/A1-P109.md`.
Evidence source: `docs/linux-rewrite/P109-report.md`, cross-checked against each
row's real VERIFY clause in `docs/linux-rewrite/01-inventory-app.md` and its
current cell in `docs/linux-rewrite/INVENTORY-LEDGER.md`. Every capture cited
below was opened and visually inspected with the Read tool (not taken on the
report's prose alone) from `reference/linux-progress/p109/shots/`.

Governed by `docs/linux-rewrite/EVIDENCE-STANDARD.md`. I did not drive any of
this evidence and did not build any of the underlying features; I judged what
other agents already produced.

## Headline finding: P106's close-out batch mislabeled three rows, and P109 inherited the mislabeling

`P106-report.md`'s numbered close-out batch (items 3, 4, 5) attached the wrong
ledger-row headers to three gestures it drove, and `P109-report.md` copied
those headers verbatim as `### F-SID-06`, `### F-SID-11`, `### F-SID-15`. None
of the three headers' content matches that row's actual VERIFY clause:

- The prose under **`### F-SID-11`** ("right-click a worktree row, confirm
  `Set/Unset Primary` + seven New-Tab items, and no `Remove Worktree` entry")
  is a context-menu census. The real `F-SID-11` clause is about **displayed
  identity/status fields on rows** (branch, folder name, primary, comment,
  agent status) — unrelated. This section supplies no evidence for the real
  `F-SID-11`.
- The prose under **`### F-SID-15`** ("click a folder-project row with no
  worktree children, observe nothing opens, chevron discloses zero children")
  is about childless-project click/disclosure behavior. The real `F-SID-15`
  clause is **"Remove a worktree from its context menu ... confirm any
  prompt ... confirm it disappears."** Unrelated. This section supplies no
  evidence for the real `F-SID-15`.
- The prose under **`### F-SID-06`** ("hover a worktree row, click its `×`,
  observe whether any confirmation prompt appears") is exactly the real
  **`F-SID-15`** clause (remove-worktree, confirm-prompt, confirm-disappears),
  driven through the only door that exists for it (the row's hover `×`,
  since the row's own `F-SID-11`-headed section already proved no
  context-menu `Remove Worktree` item exists at all). The real `F-SID-06`
  clause — a collapsed-project descendant-activity badge — was never touched
  anywhere in `P109-report.md`.

I have remapped the evidence to the rows it actually supports rather than the
headers it was filed under. This means: the real `F-SID-06` and `F-SID-11`
rows get **no new evidence from P109** (their existing P106-sourced
`half-proven` verdicts stand, confirmed unchanged), and the real `F-SID-15`
gets **new, strong, live evidence** promoting it out of `NOT EXERCISED`.

## Headline finding: the chat transcript pane does not render turn content live

`F-CHAT-02` drove a real send through the visible composer (context indicator
advanced `0%→5%`, tab picked up a checkmark — proof a turn was actually sent)
and the transcript pane stayed completely empty for 14s and after a forced
repaint. This is not scoped to the auth-required scenario the row names — it
is a general rendering failure that also blocks every row downstream of it
needing transcript content to exist: `F-CHAT-21`, `F-CHAT-22` (blocked on this
exact defect) and `F-CHAT-23`/`F-CHAT-31` (blocked on a second, distinct
defect below). Four of my 32 rows are `UNREACHABLE` because of these two
compounding defects, not because no one tried.

## Second finding: a write-tool approval gate has no answer path

A later, different send (`F-CHAT-23`) did produce visible transcript content
— a `? Question waiting · Write p109-test.txt` banner — proving the
transcript pipeline is not universally broken, only broken for the plain
turn in `F-CHAT-02`. But the approval question itself was never answerable:
`Show`, clicking the banner body, and the composer's `Stop` button all did
nothing; the filesystem confirmed the write never happened. The only control
that did anything was the Activity panel's `×`, which destroys the whole tab
rather than resolving the question. `F-CHAT-23` and `F-CHAT-31` are both
`UNREACHABLE` because of this — no completed tool row, diff, or file link was
ever produced to click.

## Third finding: destructive worktree removal has zero confirmation and isn't reachable via the named menu

Real `F-SID-15` (see remapping above): the app happily deletes a worktree
directory from disk with no prompt of any kind, and the row's own named entry
point (a context-menu `Remove Worktree` item) does not exist — the only door
is an easy-to-mis-click hover `×`. Recorded as `FAILED — defective`, not
merely absent: the feature executes, destructively, without its safety
half.

## Row verdicts

### F-CHAT-02 — FAILED — absent
Clause: authentication-required banner + CLI guidance + Retry.
A real turn was sent via the composer (context `0%→5%`, tab checkmark) and
the transcript pane rendered nothing at all — no user bubble, no assistant
response, no auth banner, no Retry — through a 14s wait and a forced
tab-switch repaint. Frames opened: `shots/57-after-send.png`,
`shots/60-transcript-wait14.png`, `shots/61-transcript-after-tabswitch.png`.
Evidence discriminates: the context/tab-checkmark change proves the send
happened; the pane's total silence is a real, validated negative.

### F-CHAT-16 — FAILED — absent
Clause: model pill → search → matching/no-match → Recommended labels → select.
The pill (post-send) opens an **Effort** selector (`Default/Low/Medium/
High/Xhigh/Max`), not a model picker — no search field, no model list, no
Recommended label. Clicking `High` did update the badge, proving clicks
land and state updates work; it's simply the wrong feature behind this
control. The popover also would not dismiss (outside-click, Escape, and
re-clicking the trigger all failed). Frame opened: `shots/62-model-chevron-click.png`.

### F-CHAT-18 — FAILED — absent
Clause: context ring → fraction/remaining + full input/output/cache/cost
breakdown.
Clicked the context indicator both before and after a send; both times
nothing opened, same "textarea gets focus, nothing else happens" outcome.
Frames opened: `shots/45-context-indicator-click.png`,
`shots/55-context-indicator-fresh.png`.

### F-CHAT-21 — UNREACHABLE
Clause: click a Thinking row, confirm expand/collapse.
Depends on a transcript rendering a Thought entry. Per F-CHAT-02, the
transcript rendered nothing for any turn attempted in this report; no Thought
entry ever appeared to click. Same frames as F-CHAT-02 (transcript empty).

### F-CHAT-22 — UNREACHABLE
Clause: click a WorkGroup / collapsed older-turn row, confirm unfold/fold.
No grouped-step or older-turn entry was ever rendered in any send attempted
(neither the plain F-CHAT-02 send nor the tool-call send in F-CHAT-23, which
stalled before producing a completed step). Same frames as F-CHAT-02/23.

### F-CHAT-23 — UNREACHABLE
Clause: expand a completed tool-call card, inspect output, Dismiss an
unrenderable permission call.
A write-tool send produced a `? Question waiting · Write p109-test.txt`
banner that never resolved — `Show`, the banner body, and `Stop` all did
nothing across three isolated attempts; the filesystem confirmed the write
never ran. No completed tool row was ever produced. The only working control
(Activity panel `×`) destroys the whole tab instead of answering the
question. Frames opened: `shots/71-working-wait10.png`,
`shots/77-activity-x-click.png`.

### F-CHAT-31 — UNREACHABLE
Clause: open a diff preview from chat, inspect numbered add/remove lines.
Same send, same stuck approval gate as F-CHAT-23 — no diff-bearing tool
result was ever produced before the flow stalled. Same frames as F-CHAT-23.

### F-CHG-01 — FAILED — absent
Clause: toggle right panel, click Files and Changes, confirm each replaces
the other.
Promoted from the prior `half-proven` (the "owed" toggle test is no longer
untested — it's tested and negative). Toggled the titlebar right-panel icon
closed then reopened: both states show the identical Files content
(`.remember`, `Activity`) — there is no separate Changes view anywhere in
the toggle. It is a pure show/hide of one panel, not a Files/Changes switch.
Frames opened: `shots/215-chg01-toggle-close-precise.png`,
`shots/216-chg01-toggle-reopen.png`.

### F-CHG-20 — FAILED — defective
Clause: "No activity" empty state + running-count state.
With Chat/Terminal tabs open, Activity showed two live, correctly-labelled
running rows (running state is present). With both closed via their `×`
controls (nothing running), the label that should read "No activity" instead
renders as a handful of isolated, sub-pixel-scale coloured specks — checked
three independent ways (crop to the true window edge ruling out clipping,
brightness/contrast pass ruling out low-contrast-but-legible text, and a
scroll/zoom repaint attempt) and confirmed real each time, not an artifact of
the check. Frames opened: `shots/224b-crop-full.png`,
`shots/224d-crop-contrast.png`.

### F-PER-07 — PASSED
Clause: change project identity/icon/settings, quit/relaunch, confirm
persisted.
Full live round trip: changed display name (typed live, sheet header
live-updated) and icon (fourth grid icon, selection border went orange) in
Project Settings; closed the sheet — sidebar row updated immediately to both
new values together. App process was found already self-exited at the moment
of the deliberate quit (a recurring, separately-flagged infra defect, not
chased here); relaunched on the same `TILLER_DB` and the sidebar read `P109
Renamed Project` with the same terminal-window icon, with no further
interaction. Also confirms the sheet exposes no field for either
worktree-location column, as the row expects. Frames opened:
`shots/231-per07-settings-sheet.png`, `shots/239-per07-relaunched.png`.

### F-PRJ-03 — FAILED — absent
Clause: browse a non-Git folder, confirm the prompt offers Initialize
Git/Add without Git/Cancel, exercise each.
The sidebar `+` menu was opened and fully enumerated: exactly two items,
`Clone Repository…` and `Create Project…`. No browse-to-a-folder / "Open
Project…" entry exists anywhere in it, so the three-way prompt this clause
depends on has no entry point to reach. A `ctrl+o` keyboard probe produced no
visible change but is explicitly noted as inconclusive (a Wayland portal
picker would be invisible to this X11 capture), not counted as a second
negative. Frame opened: `shots/22-plus-menu.png`.

### F-PRJ-04 — PASSED
Clause: make folder selection OR project insertion fail, confirm a visible
(non-silent) error.
The `+` menu's browse route doesn't exist (same finding as F-PRJ-03), so the
"folder selection" arm can't be exercised — but this is an OR clause, and the
"project insertion fails" arm is directly satisfied by this same report's own
live evidence under F-PRJ-07 (clone) and F-PRJ-10 (create): both show a real,
visible, verbatim red-text failure message and neither silently added a
project. Cross-referencing this evidence from the F-PRJ-07/10 sections is a
deliberate call, flagged here for transparency — it is the same live UI
gesture answering the same clause's other arm, not adjacency-trap reasoning.
Frames opened: `shots/05a-clone-submit.png`, `shots/09b-create-submit-r1.png`.

### F-PRJ-07 — half-proven
Clause: invalid clone URL → confirm failure message → correct the URL → retry.
The failure half is fully confirmed live: invalid URL submitted, button
relabeled `Retry clone`, verbatim red failure text shown. The
"correct-the-URL-and-retry" half was never driven — no second, valid
submission was attempted. (Side finding, not scored: the string actually
passed to `git clone` was garbled/duplicated relative to the field's
displayed value — worth a builder's attention, not adjudicated here.) Frame
opened: `shots/05a-clone-submit.png`.

### F-PRJ-10 — PASSED
Clause: force project creation to fail, confirm a visible failure is
reported. (No retry requirement in this clause, unlike F-PRJ-07.)
Created a project name colliding with an existing directory; submit produced
a verbatim, visible red failure message
("could not create /home/enzopalmisano/Scrivania: File exists (os error
17)"). Fully satisfies the clause as written. Frame opened:
`shots/09b-create-submit-r1.png`.

### F-PRJ-11 — FAILED — absent
Clause: Project Settings → trash control → confirm deletion prompt → accept
→ project disappears, directory remains.
The full Project Settings sheet was opened and every element enumerated:
path, Repository type, Display name, icon picker (Icon/Emoji/Avatar +
colour), a `Reset` button, `Close`, project id. There is no removal/trash
control anywhere on it — `Reset` is scoped to the icon/colour picker directly
above it, not to project removal. The only `Remove Project` control found in
this entire flow is one level up, on the row's right-click context menu, not
inside the sheet the clause specifically names. Frames opened:
`shots/29b-displayname-recheck.png`, `shots/33-settings-reopened.png` (both
show the full sheet with no removal control present).

### F-PRJ-12 — half-proven
Clause: switch Repository Type Git/Folder, edit Display Name, close, confirm
sidebar reflects new name.
Display-name half: field accepts live typing and the sheet's own header
live-updates to match — confirmed working. Repository-type half
(Folder→Git via `Initialize Git`): functionally confirmed via the
filesystem (a real `.git/` directory appeared) and a fresh reopen of the
sheet (correctly read `Repository: Git`, button gone) and the context menu
(`Initialize Git repository` correctly greyed with a reason) — but the
**already-open** sheet itself does not live-refresh to reflect the change,
a separate staleness bug. What was not directly captured: closing the sheet
after the display-name edit and screenshotting the sidebar row itself
showing the new name (the sheet occludes the sidebar while open, and the
report moved on to Initialize Git before closing). The renamed value is
corroborated indirectly — a later row (F-PRJ-17) derives a new worktree's
parent-folder name from this exact edited display name — but that is not the
same as seeing the sidebar row's label change. Frames opened:
`shots/29b-displayname-recheck.png`, `shots/33-settings-reopened.png`.

### F-PRJ-17 — FAILED — absent
Clause: exercise each default-worktree-base option (current/pinned/primary/
no-primary), reopen, confirm it persists.
The `New Worktree…` popover contains exactly one control: a branch-name text
field. No base-selection control of any kind is present, before or after
typing. There is nothing to exercise. Frame opened:
`shots/34-new-worktree.png`.

### F-PRJ-18 — FAILED — absent
Clause: set a custom worktree location via a chooser, then use a
default-parent control, confirm the path changes.
Same popover as F-PRJ-17 — only the branch-name field. No location chooser
or default-parent control exists anywhere in this flow. Frame opened:
`shots/34-new-worktree.png`.

### F-SET-16 — half-proven
Clause: search agents, click Refresh, confirm matching rows and an updated
timestamp/status.
Search half proven working: typing a no-match string filtered the five-row
list to zero (no "no results" text, but the filtering mechanism itself
demonstrably responds), and clearing it restored all five — a real,
discriminating 5→0→5 round trip. Refresh half proven absent: clicking
`Refresh` produced a byte-identical capture before/after — no spinner, no
reorder, and no updated-timestamp text was found anywhere on the screen,
refreshed or not. Frames opened: `shots/101-agents-fresh.png`,
`shots/102-refresh-fresh.png`, `shots/105-search-nomatch-r4.png`.

### F-SET-21 — N/A — platform
Clause: choose each Files-icon-theme option, confirm file-tree icons change.
The control shows a single button reading `Material`; clicking it produced a
pixel-identical capture (no dropdown, popover, or menu of any kind). This
isn't scored as a defect: per the census, the second-option list this
control would offer is macOS-only by `cfg` gate, so on Linux there is
currently exactly one file-icon theme shipped — there is nothing to
"choose... each option" between yet. The row's premise (multiple options)
does not hold on this platform today. Frame opened:
`shots/90-fileicons-click.png`.

### F-SID-06 (real clause) — half-proven — UNCHANGED
Clause: start activity in a child pane, collapse the project, confirm the
project row still shows a status badge.
See "Headline finding" above: `P109-report.md`'s `### F-SID-06` section
actually drives the real `F-SID-15` clause (worktree removal), not this one.
No section in `P109-report.md` exercises the real `F-SID-06` clause. The
existing `half-proven` verdict (P106: live notify dots on worktree rows
confirmed; the collapsed-project descendant badge specifically was not
exercised, and code gates dots to worktree rows) stands unchanged — this is
a confirmed, not inflated, result.

### F-SID-11 (real clause) — half-proven — UNCHANGED
Clause: select a Git worktree and a folder worktree, start an agent in one,
confirm each row displays branch/folder-name/primary/comment/agent-status.
See "Headline finding" above: `P109-report.md`'s `### F-SID-11` section
actually drives a context-menu census (Set Primary + New-Tab items + absence
of Remove Worktree), not this row's identity/status-field clause. No section
in `P109-report.md` exercises the real `F-SID-11` clause. The existing
`half-proven` verdict (P106: live Git rows showed branch/path/Primary/status,
but no folder-worktree row exists and the required comment is not rendered)
stands unchanged — confirmed, not inflated.

### F-SID-15 (real clause) — FAILED — defective — promoted from NOT EXERCISED
Clause: right-click a worktree, choose Remove Worktree, confirm any prompt,
confirm it disappears from the sidebar.
See "Headline finding" above: this is the evidence filed under
`P109-report.md`'s `### F-SID-06` header. The named entry point doesn't
exist at all — the same report's context-menu census (filed under its
`### F-SID-11` header) enumerated the full worktree right-click menu and
found no `Remove Worktree` item anywhere in it; the only removal door is the
row's hover `×`. Clicking it removed the worktree with **zero confirmation
of any kind** — no dialog, no banner — and deleted the underlying directory
from disk (the Files panel, still pointed at the deleted path, switched to
`Files unavailable: ... (os error 2)`). The "disappears from the sidebar"
half is proven true; the "confirm any prompt" half is proven false — a
destructive, disk-deleting action fires with no safety confirmation, via a
door the clause doesn't even name. Frames opened:
`shots/110-sid11-rclick-featuretest.png` (menu census, no Remove Worktree
item), `shots/121-sid06-x-click.png`, `shots/123-sid06-after-cancel.png`.

### F-SID-16 — PASSED
Clause: drag one project row above/below another, release, confirm order
changes.
Two independent real press-move-release drags of `.claude` past `Sonnet
P109 Test Display` were driven. Both immediate post-drag captures showed the
order unchanged — but a later, unrelated capture (after a hover-only
mousemove, not another drag) showed the order flipped. This is scored as a
stale-repaint defect, not a failed reorder: two drag attempts preceded the
flip and a bare hover event cannot itself cause a reorder, only force a
repaint of one already committed at release time. The clause's literal ask
("confirm the order changes") is satisfied; the repaint lag is noted as a
real secondary defect. Relaunch-persistence was not separately checked (not
required by this row's clause). Frame opened:
`shots/119-sid06-hover-featuretest.png` (order flipped).

### F-SID-17 — FAILED — defective
Clause: drag one worktree row to another worktree's position in the same
project, confirm order changes.
The identical drag technique that produced a (delayed) reorder for F-SID-16
was used here (`feature-test` dragged onto `master`'s position, 15
interpolated steps) and produced **no reorder, immediate or delayed** — a
follow-up capture taken after a subsequent hover event (ruling out the
stale-repaint explanation that applies to F-SID-16) still showed the
original order. A real, comparable gesture was driven and produced nothing.
Frame opened: `shots/120-sid06-hover-featuretest-v2.png`.

### F-TAB-01 — FAILED — defective
Clause: create terminal/chat/document/diff/browser tabs, activate each,
modify a document, confirm tab decorations.
Drove the Files-panel-click route to open a document tab. Every click on an
expanded folder's child row (a file or subfolder), tried three separate
times with fully isolated single clicks, collapsed the parent folder instead
of acting on the child — no document tab was ever opened, so neither the
document-tab-icon conjunct nor the modify-a-document dirty-route conjunct
could be reached. This is a real UI bug (child rows are not individually
clickable), not a gesture that was skipped. Frames opened:
`shots/135-tab01-remember-retry.png`, `shots/142-tab01-gitignore-isolated.png`.

### F-TAB-11 — FAILED — absent
Clause: resize until a split is ineligible or select the sole tab in a
group, open the split menu, confirm a disabled reason is shown.
With a single pane and no splitter present, sole-tab right-click was driven
at multiple locations. Away from the prompt line it opened nothing; on the
prompt line it opened a menu whose only entry, verbatim, is `Copy` — no
`Split Left/Right/Above/Down` items appear at all, disabled or otherwise, and
no reason text of any kind. The clause's premise — a split menu with a
disabled item carrying an explanatory reason — does not exist on this build;
the sole-tab state produces a plain clipboard menu instead. Frame opened:
`shots/146-tab11-rclick-retry.png`.

### F-TAB-18 — PASSED
Clause: drag a tab to a different position in the same strip, confirm order
changes.
A real press-move-release drag (the lane's `drag()` helper, 12 interpolated
steps) moved `Claude Code` from the third slot to the first. Result: strip
order changed from `Chat, Terminal, Claude Code` to `Chat, Claude Code,
Terminal`, and the sidebar's per-worktree tab list updated to match. Frame
opened: `shots/151-tab18-after-drag.png`.

### F-TAB-24 — PASSED
Clause: begin a tab drag, press Escape before dropping, confirm the tab
remains in its original position.
Used the lane's `drag_escape()` helper (mousedown, move to the drop target,
Escape, mouseup) on the same tab strip and a comparable drag distance/target
as F-TAB-18's successful reorder. Result: order held unchanged
(`Chat, Claude Code, Terminal`). This is discriminating specifically because
F-TAB-18 proves the same class of gesture at a similar boundary position
normally **does** commit a reorder when not escaped — so "nothing moved" here
is a real comparative result, not coincidence or an inert screen. (The report
itself is honest that it cannot tell from the screen alone whether Escape
actively cancelled the drag or the drop simply never committed once Escape
fired mid-drag — the row only asks to confirm the tab stayed put, which is
what was observed.) Frame opened: `shots/152-tab24-after-drag-escape.png`.

### F-TAB-28 — FAILED — defective
Clause: activate a clean tab, press ⌘W/ctrl-w, confirm close or (for a dirty
tab) the documented close confirmation.
Clean-tab half: `Chat` (no messages, no dirty indicator) made active,
`ctrl-w` pressed three times (isolated, and once after explicitly re-clicking
the tab header to rule out focus sitting in the composer) — no change each
time, byte-identical captures. Dirty-tab half: `Terminal` (a live PTY,
carries `tab_is_dirty`) made active, `ctrl-w` pressed — no confirm dialog, no
close, byte-identical capture. `ctrl-w` produces zero observable effect on
either path. Frames opened: `shots/164-tab28-sidebar-chat-click.png`,
`shots/165-tab28-ctrlw-chat.png`, `shots/168-tab28-terminal-select.png`,
`shots/169-tab28-ctrlw-terminal.png`.

### F-WIN-01 — PASSED
Clause: launch, confirm workspace appears, press ⌘, (Settings replaces the
workspace), click Back.
No gear icon exists on the titlebar (confirmed by cropping the full strip —
only icon present is the Files-panel toggle); the equivalent trigger is the
bottom-left status-bar gear at `(27,948)`. Clicking it fully replaced the
whole workspace — no sidebar, no tab strip — with a `‹ Back  Settings` header
over the `General` section (Version, toggles, etc). Clicking `‹ Back`
restored the workspace completely: sidebar, tab strip
(`Chat`/`Claude Code`/`Terminal`), and Files panel all back to their prior
state. This is a full, live, click-only exercise of the row's core
requirement (route swap both directions); the literal `⌘,` keychord itself
was not tested (no macOS-style binding to test on this platform), but the
functional behavior the clause is checking — full-workspace-replace and a
clean exit route — is directly confirmed. Frames opened:
`shots/173-win01-titlebar-crop.png`, `shots/174-win01-gear-click.png`,
`shots/175-win01-back-click.png`.

### F-WIN-10 — FAILED — absent
Clause: trigger an app error/message, confirm a bottom-right toast appears,
wait to confirm it disappears.
Two real, distinct errors were triggered through the sidebar `+` → `Create
Project…` door (a duplicate-path OS error and a client-side invalid-name
validation error) — both rendered as persistent inline red text inside the
dialog, directly confirmed by opening both full-screen captures: no toast,
bottom-right or anywhere else on screen, appears in either frame, and neither
message auto-dismissed (both were still on screen until the dialog was
explicitly cancelled). The trigger genuinely worked (ruling out "nothing
happened" as the explanation) and the specific affordance the clause names —
a bottom-right, auto-dismissing toast — was not found anywhere. Frames
opened: `shots/208-win10-dup-create-click.png`,
`shots/210-win10-invalid-path-result.png`.
