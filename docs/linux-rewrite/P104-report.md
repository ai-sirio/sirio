# P104 report — thirty rows, one launch

Critic: `sonnet`. Branch `linux/gpui-waku`. Driven against a single live app launch
(`rust/target/debug/tiller`), display `:1`, using a dedicated scratch git fixture repo
(never the live worktree) and a scratch `TILLER_DB`. No `rust/` source read or edited, no
builders' notes read, `INVENTORY-LEDGER.md` not touched. This file reports observations
only — no verdict column, no PASSED/FAILED. Screenshots referenced below live in
`reference/linux-progress/`.

## Setup notes (not a recipe row)

Workspace: a fresh scratch git repo (`README.md`, `src/lib.rs`, one commit) opened as the
app's cwd, one Terminal tab and one Chat tab open at launch — matching Session 1's stated
starting state ("one workspace on a git worktree, one terminal tab open"). A second,
separate project was also registered via the sidebar's "+" → "Create Project..." dialog for
Group 4's sidebar rows; its on-disk/sidebar name came out as `p104-drivenp104-driven`
(doubled) after the name field was typed into twice across two attempts (the first attempt's
text did not appear to register on screen at the time, per a focus-timing issue in my own
driving harness — this was my own setup action, not a scripted recipe gesture, so it is
recorded here as a caveat rather than as a finding against any row).

Right-click was verified to be mechanically delivered by this harness before trusting any
"no menu" observation below: right-clicking inside the terminal pane's body reliably opens a
context menu (`p104-g1-tab21-control-terminal-menu.png`) with, top to bottom: **Copy, Paste,
Copy Context, Set Title, Copy Pane ID, Copy Terminal ID, Split Left, Split Right, Split
Above, Split Down, Clear Terminal, Close Terminal…**. All "no menu appeared" observations
below were taken with ≥1.5s settle after the click, several with multiple staggered
follow-up captures (per ENVIRONMENT.md's stale-frame warning), and with the Files panel's
known z-order-over-menu defect checked for (no partial/truncated menu edge was visible in
any of them).

## Group 1 · tab strip and tab menu

**F-SID-19** — Gesture: click the terminal pane, then press `ctrl-t` three times, one
screenshot after each press. Screen: after press 1, a second tab appeared in the strip
labelled "Terminal" (dirty-dot indicator), next to the original "Terminal" tab; strip now
reads Chat, Terminal, Terminal (new tab active/focused). After press 2, the strip was
unchanged from after press 1 — no third tab appeared. After press 3, still unchanged — no
change from press 1's result. Captures: `p104-g1-sid19-ctrlt1.png`,
`p104-g1-sid19-ctrlt2.png`, `p104-g1-sid19-ctrlt3.png`.

**F-TAB-02** — Gesture: kept pressing `ctrl-t` well past three (a further 8 presses in one
batch, then 6 more in a second batch, each with a re-click on the terminal body first).
Screen: of the 14 additional presses across both batches, only 2 produced a new tab (strip
went from 2 Terminal tabs to 4 Terminal tabs total, plus Chat); the remaining 12 presses
produced no visible change. The strip never visually ran out of width — empty space
remained to the right of the last tab in every capture. A downward-chevron control
(`tab-overflow-button`, presumably) appeared at the strip's right edge once 4 Terminal tabs
were present, ahead of any actual pixel overflow. Clicking that chevron produced no visible
dropdown or list in two separate attempts (immediately, and again after the tab count had
not changed) — only a faint underline highlight near the chevron itself. Captures:
`p104-g1-tab02-8more.png` (strip state, chevron visible), `p104-g1-tab02-overflow-click.png`
(after clicking the chevron — no list appeared).

**F-TAB-21** — Gesture: right-click a tab in the strip. Tried on: the first Terminal tab (3
separate attempts with staggered 1.5–5.5s settle and multiple follow-up captures), the
active/last Terminal tab, and the Chat tab. Screen: no context menu appeared in any attempt
— confirmed not a stale-capture artifact by taking a second and third capture 2s apart each
time and finding them identical to the first (except once, where a *stale terminal-pane*
menu from an earlier gesture was still on screen and cleared on its own ~2s later, per
ENVIRONMENT.md's documented input-queue-lag behaviour — that stale menu was the terminal
body's Copy/Paste/... menu carried over from a prior action, not a tab menu). Right-click's
mechanical delivery was independently confirmed working in the same session (see Setup notes
above and control capture). Capture: `p104-g1-tab21-rclick-tab.png`.

**F-TAB-15** — Gesture: right-click the second tab (first Terminal tab) and look for a Close
entry to choose. Screen: no menu appeared (consistent with F-TAB-21), so no Close entry was
reachable and no tab was closed by this gesture. Capture: `p104-g1-tab15-rclick2nd.png`.

**F-TAB-17** — Gesture: with 4 Terminal tabs + Chat present (≥4 satisfied), right-click the
second tab looking for Close Others, then (intended) reopen tabs and right-click the second
tab again looking for Close Tabs to the Right. Screen: no menu appeared on the first
right-click, so Close Others was not reachable; the second half of the row (which depends on
having used Close Others first) was not attempted since its precondition was never reached.
Capture: `p104-g1-tab17-rclick2nd.png`.

**F-TAB-12** — Gesture: right-click the last (active) tab looking for Move Earlier, then
right-click the first tab (Chat) and compare how Move Earlier presents there. Screen: no
menu appeared on either right-click; tab strip order was unchanged after both gestures.
Captures: `p104-g1-tab12-rclick-last.png`, `p104-g1-tab12-rclick-first.png`.

**F-TAB-13** — Two conjuncts, split and recorded separately. First: right-click inside the
terminal pane → Split Right. Screen: the menu listed above opened; clicking "Split Right"
produced a visible vertical split — two terminal panes side by side, the right one shown
with an orange-highlighted, active-looking title pill and a running elapsed-time readout
(`14:14:31`), the left one with a plain "0ms" pill. This conjunct is confirmed on screen.
Captures: `p104-g1-tab13-split-menu.png`, `p104-g1-tab13-split-result.png`. Second:
right-click a tab and report every "Move to …" entry. Screen: no menu appeared (consistent
with F-TAB-21), so no "Move to …" entries were observed and nothing was moved. This conjunct
did not reach an on-screen menu. Capture: `p104-g1-tab13-moveto-attempt.png`.

**F-TAB-14** — Gesture: right-click a tab looking for a Rename entry, intending to type a
name and press Enter, then repeat and press Escape. Screen: no menu appeared, so no Rename
entry, no `tab-rename-field`, and no typing was attempted — there was nothing on screen to
type into. Capture: `p104-g1-tab14-rename-attempt.png`.

## Group 2 · file tabs and the editor

**Setup note (not a recipe row):** the shared setup gesture is "press `ctrl-o`, open
`README.md`, then a `.rs` file." `ctrl-o` produced no picker (see F-EDIT-08 below, and three
prior attempts in Group 1's session in different focus contexts — a consistent negative).
Both files were instead opened from the Files panel via a tight double-click
(`xdotool click --repeat 2 --delay 180`; a plain sequential `click()` twice does not open a
row, it only toggles selection). Doing this surfaced a load-bearing fact for the whole group:
**opening a file does not add a new tab to the top strip.** The strip stayed at Chat + 4
Terminal tabs throughout Group 2 — opening `main.rs` or `README.md` instead replaces whatever
the content pane already showed (observed replacing both the live Chat view and, later,
`main.rs`'s own view). Clicking the "Chat" tab chip itself switches back to the real Chat
composer, discarding whatever file was being shown in that slot. There is no route found
(double-click, `ctrl-o`, or the Files-row right-click menu) that opens a second file
concurrently with a first — see F-TAB-16 below. This governs how "both files open" and
"return to the tab" are read for every row in this group: there is one content pane, not a
per-file tab.

Also worth flagging once here rather than per-row: after using the Files-row right-click menu
(F-TAB-16, below) and clicking its "Open" entry, four consecutive captures across ~8s of
staggered sleeps all showed the context menu still on screen, unchanged pixel-for-pixel — a
far longer instance of ENVIRONMENT.md's stale-frame/input-queue-lag warning than seen
elsewhere this session. A later, unrelated capture revealed the click had in fact landed and
opened the file. Treated as confirmation the lag can span several capture cycles, not settled
as any new absence.

**F-EDIT-07** — Gesture: with both files open, report how each tab presents its text for
`.md` vs `.rs`. Screen: "both open" was not reachable (see setup note); reporting each file's
own presentation as driven. `main.rs`: monospace text, line-numbered gutter, Rust syntax
colouring (e.g. `pub fn` and the string literal each a distinct colour), a grey "Rust" badge
next to the path bar, no mode toggle, no formatting toolbar. `README.md`: has a "Preview |
Code" toggle the `.rs` view lacks; Preview shows a proportional-font rendered heading and
paragraph (no line numbers); Code shows monospace source with line numbers plus a `B I H List
Link` formatting toolbar (also absent from the `.rs` view) directly under the mode toggle, and
a grey "Markdown" badge next to the path bar. Captures: `p104-g2-edit07-mainrs-code.png`,
`p104-g2-edit07-readme-preview.png`.

**F-EDIT-01** — Gesture: on the `.md` tab, find the mode control, click each side, report the
content area in each mode. Screen: mode control is the "Preview | Code" pair described above.
Preview: renders "P104 fixture repo" as a large heading and the next line as a plain
paragraph — Markdown syntax characters are not shown. Code: raw Markdown source with a
leading `#` on the heading line, line numbers, and the `B I H List Link` toolbar appears above
the text. Capture: `p104-g2-edit01-md-code-toolbar.png` (Code side; Preview side captured
under F-EDIT-07 above).

**F-EDIT-03** — Gesture: switch the `.md` to Preview, report what is displayed and how it
arrives. Screen: same rendered heading + paragraph as above; it appeared already-settled in
the first post-click capture, no loading state was observed. Capture:
`p104-g2-edit07-readme-preview.png`.

**F-EDIT-02** — Gesture: in Code mode select a word, click Bold, then Italic on the toolbar,
report the buffer text after each click. Screen: double-clicked the word "fixture" on line 1
— no visible selection highlight appeared in the capture taken immediately after. Clicked
`B`: buffer text unchanged, no markup inserted, no bold styling visible. Clicked `I`
afterward: the entire first line was wrapped end to end — `*# P104 fixture repo*` — not
scoped to the double-clicked word — and the path bar showed a red-dot "edited" indicator.
`ctrl-z` (tried twice) did not revert this change; the line was restored by manually
selecting it and retyping the original text, to keep the fixture clean for the rows that
follow. Captures: `p104-g2-edit02-bold-noop.png`, `p104-g2-edit02-italic-result.png`.

**F-EDIT-04** — Gesture: type a character in a file tab, press `ctrl-s`, report the
indicator before/after and `git diff --stat` from a terminal pane. Screen: after typing `x`
at the end of the body line, the path bar showed a red-dot "edited" label (there is no
per-tab dot for this, since the file has no tab of its own — see setup note). After `ctrl-s`
the "edited" label disappeared. `git diff --stat` in a terminal pane reported:
`README.md | 2 +-` / `1 file changed, 1 insertion(+), 1 deletion(-)`. Captures:
`p104-g2-edit04-before-save.png`, `p104-g2-edit04-after-save.png`,
`p104-g2-edit04-06-terminal.png`.

**F-EDIT-06** — Gesture: covered by the same action — after `ctrl-s`, `cat` the file in a
terminal pane, report its content. Screen (same terminal pane/capture as F-EDIT-04):
```
# P104 fixture repo

Scratch git fixture for driving the tiller app during the P104 critic pass.x
```
Capture: `p104-g2-edit04-06-terminal.png`.

**F-EDIT-05** — Gesture: with the `.md` tab open, append to the same file from a terminal
pane (`echo x >> README.md`), return to the tab, report any banner verbatim and what each
control does. Screen: ran the `echo` from a terminal pane, then reopened `README.md` from its
Files-panel row (the only way found to return to it — see setup note). No banner of any kind
appeared; the reopened view simply showed the updated 4-line content (the new `x` line
present) as if freshly read from disk, with no "file changed / reload?" prompt and therefore
no controls to test. Capture: `p104-g2-edit05-reopen-no-banner.png`.

**F-EDIT-08** — Gesture: press `ctrl-o` and pick the already-open `README.md` again, report
the tab strip and which tab has focus. Screen: with `README.md` the active content and its
own row highlighted in the Files panel, `ctrl-o` was pressed — no file picker appeared. Tab
strip unchanged (Chat, Terminal ×4); the same slot kept showing `README.md`'s content, with
no visible change of any kind. This is the fourth consistent negative for `ctrl-o` across the
session, in a fourth distinct focus context. Capture: `p104-g2-edit08-ctrlo-noeffect.png`.

**F-TAB-16** — Two conjuncts, split and recorded separately. First: type in a file tab
without saving, then close that tab, report the dialog verbatim and each button's effect.
Typed `y` onto the end of `README.md`'s body line (making it dirty, "edited" label shown),
then clicked the `×` on the "Chat" tab chip — the only tab hosting the file view (see setup
note). Screen: the file view was replaced by the live Chat composer immediately, with **no
dialog of any kind** — the unsaved `y` was silently discarded (confirmed by reopening
`README.md` afterward: the saved 4-line content was there, without the `y`). Captures:
`p104-g2-tab16-dirty-before-close.png`, `p104-g2-tab16-close-no-dialog.png`. Second: repeat
closing the whole workspace with two dirty tabs, report that dialog too. Screen: this
precondition was not reachable. The Files-row right-click menu (confirmed working — see the
setup note above) offers only `Open`, `Reveal in File Manager`, `Copy Path` — no "open in new
tab" or equivalent — and no gesture tried in this group (double-click, `ctrl-o`, this menu)
produces a second concurrently-open file; each one replaces the single content pane's current
file. With no discovered way to get two files open at once, "two dirty tabs" could not be set
up, and this conjunct was not driven. Capture: `p104-g2-tab16-rowmenu-no-newtab.png`.

## Group 3 · right panel, Files tab

This is the same right-hand "Files" panel used throughout Groups 1-2 (already open at
`ctrl-shift-i` from session setup).

**F-CHG-03** — Gesture: click Refresh, report what the panel shows between the click and the
list settling. Screen: no transient state was observed at all — the capture taken
immediately after the click and a second capture ~1s later are both identical to the
pre-click state (no spinner, no skeleton row, no flash). The listing (including `README.md`'s
orange-dot "Diff" badge) was unchanged throughout. The optional Retry-control path (inducing a
failed refresh via `chmod 000 .git`) was not attempted — it risked destabilising the fixture
mid-session for a row marked optional. Capture: `p104-g3-chg03-refresh-immediate.png`.

**F-CHG-05** — Gesture: click a file row, then press down, up, space; report selection and
what each key did. Screen: clicked `main.rs`'s row — the selection highlight visibly stayed
on `README.md` (carried over from earlier), not `main.rs`; the click itself did not appear to
move it. `Down`: no visible change in the immediate capture. `Up`: the highlight moved to
`main.rs`'s row (the row above `README.md`). `Space`: no further visible change. Across all
three keys the path bar and content pane stayed on `main.rs` throughout — none of Down, Up, or
Space opened, closed, or changed the active file; they only moved a selection cursor
independent of what's open. Capture: `p104-g3-chg05-after-space.png`.

**F-EDIT-10** — Gesture: right-click a file row, report the menu's items. Screen: right-click
on `main.rs`'s row opened a menu directly at the row (not elsewhere in the panel), listing top
to bottom: **Open, Reveal in File Manager, Copy Path**. Capture:
`p104-g3-edit10-rclick-menu.png`.

**F-EDIT-11** — Gesture: choose Copy Path, paste into a terminal pane, report the pasted
text. Screen: clicked "Copy Path" from the menu above. Pasting into a terminal pane with
`ctrl-shift-v` produced nothing — the surrounding command executed as if nothing had been
inserted (`bash: errore di sintassi vicino al token non atteso ";"` from
`echo PASTE_START; ; echo PASTE_END`). Retried with plain `ctrl-v`: same empty result
(`echo PASTE2_START; ; echo PASTE2_END`, same syntax error). Checking the X11 clipboard
directly (outside the app, via `xclip -selection clipboard -o`) showed Copy Path had in fact
written the correct absolute path
(`.../scratchpad/p104-fixture-repo/main.rs`) to the clipboard — the failure is specific to
those two paste shortcuts inside the terminal pane, not to Copy Path itself. Using the
terminal pane's own right-click → **Paste** menu entry (documented in the Setup notes'
control-test menu) instead of a keyboard shortcut worked: the line read
`echo PASTE3_START; /tmp/claude-1000/-home-enzopalmisano--claude/21454512-bda7-4263-9407-7cd8167468c6/scratchpad/p104-fixture-repo/main.rs`
— the exact copied path, correctly pasted. Captures: `p104-g3-edit11-ctrlshiftv-fail.png`,
`p104-g3-edit11-ctrlv-fail.png`, `p104-g3-edit11-menu-paste-success.png`.

## Group 4 · sidebar

Setup for this group: registered a second project via the sidebar's "+" → "Create Project"
dialog, attempted first under the name `p104-sid07-git` to reuse a folder that had already
been `git init`'d directly via a terminal — the dialog refused with a verbatim red-text
error, **"Creation failed: could not create /home/enzopalmisano/p104-sid07-git: File exists
(os error 17)"** (the dialog only ever makes a brand-new folder; it is not an "add existing
folder" flow). Recovered by creating a fresh, differently-named folder through the same
dialog (`p104-sid07b`) and then running `git init` against that new folder directly from a
terminal, giving a genuine git project for the git-side of `F-SID-08`'s comparison alongside
the already-registered plain, non-git `p104-sid08-plain`.

**F-SID-07** — Gesture: right-click the project header, choose Project Settings, report the
fields shown and which respond to input. Screen: right-clicking `p104-sid07b`'s header opened
a menu, top to bottom: **Project Settings, Initialize Git repository, Show in File Manager,
Remove Project** (capture: `p104-g4-sid07-rclick-menu.png`). Choosing Project Settings opened
a card (capture: `p104-g4-sid07-settings-panel.png`) titled "Project Settings · p104-sid07b",
showing: the path `/home/enzopalmisano/p104-sid07b`; a "Repository: Folder" label; a "Display
name" text field (empty, placeholder-only); an "Initialize Git" button; a "Project icon"
control with three tabs (Icon / Emoji / Avatar) and, under the Icon tab, five icon choices
plus a globe icon and a row of colour swatches; a "Reset" button; a "Close" link; and a small
grey project-id string (`p-1be93e71585bdff2`). Fields tested for response: typing into
Display name updated the card's own title live, from "Project Settings · p104-sid07b" to
"Project Settings · SID07 Display" as each character landed (capture:
`p104-g4-sid07-displayname-live.png`), and the change persisted into the sidebar itself after
Close — the project's row now reads "SID07 Display" instead of its folder name. Clicking a
colour swatch (blue) drew a selection ring around it (capture:
`p104-g4-sid07-colour-selected.png`) — responds to input. Clicking the "Emoji" tab swapped the
panel to an emoji preview box with "Set Emoji" / "Open Emoji Picker" buttons (capture:
`p104-g4-sid07-emoji-tab.png`) — tab strip responds to input. Clicking "Initialize Git" on
this already-git project produced no visible on-screen change and, checked from a terminal,
left the repo exactly as it was (`git status` still clean, `git log` still showing only the
prior `init` commit) — the button is present and clickable but is a no-op with no toast/error
shown, on a project already under git.

**F-SID-08** — Gesture: add a plain non-git folder as a project (done in setup:
`p104-sid08-plain`, confirmed via terminal `git status` beforehand to not be a repository);
right-click it and the git project, and report how "Initialize Git repository" presents on
each; then click it on the plain folder and report `git status` from a terminal. Screen: on
the git project (`p104-sid07b`), a second right-click's menu showed **Initialize Git
repository** greyed out, with inline reason text reading verbatim **"Git is already
initialized"** immediately to its right (capture: `p104-g4-sid09-git-initgit-greyed.png` —
this same screenshot also serves `F-SID-09` below). On the plain folder (`p104-sid08-plain`),
the same menu entry showed **Initialize Git repository** as a normal, non-greyed,
no-reason-text item (capture: `p104-g4-sid08-plain-initgit-enabled.png`). Clicking it: a
terminal `git status` on `p104-sid08-plain` immediately afterward reported `Sul branch
master` / `Non ci sono ancora commit` ("On branch master" / "No commits yet") with a fresh
`.git` directory present — the click had genuinely run `git init` on the plain folder, taking
it from "not a repository" to a valid, empty, uncommitted git repository.

**F-SID-09** — Gesture: right-click the project, choose Show in File Manager, report what
opens on the desktop, or what appears at the sidebar's bottom if nothing does. Screen:
something did open — a separate desktop window, **COSMIC Files**, titled "p104-sid07b —
COSMIC Files", confirmed both by its window title and by its backing process
(`/usr/bin/cosmic-files /home/enzopalmisano/p104-sid07b`, launched via an `xdg-open` helper),
showing the project folder's single `note.md` file (capture:
`p104-g4-sid09-cosmicfiles-window.png`). Nothing appeared at the bottom of the app's own
sidebar, consistent with the recipe's stated fallback only applying when nothing opens.

## Group 5 · activity signals

Setup for this group: opened the right-hand "Activity" section (a collapsible entry at the
bottom of the same panel used for Group 3's Files tab); expanding it replaces the panel with
a flat list of rows — one per terminal pane and per opened file across the whole session, each
showing a title, its worktree path, and a small circular status glyph. An ACP-capable agent
CLI is present on PATH (`/home/enzopalmisano/.local/bin/claude`,
`/home/enzopalmisano/.local/bin/codex`), so `F-CHG-22` was reachable today.

**F-TERM-PTY-05** — Gesture: in a terminal pane run `sleep 8`; report the sidebar worktree
row's dot and each Activity-panel row's status text, while it runs and after. Screen: ran
`sleep 8` twice, captured mid-run (command echoed, no completion badge yet — capture:
`p104-g5-term05-sleep-midrun.png`) and after completion (the terminal block itself grows a
duration badge reading `8s Xms` once the command returns — capture:
`p104-g5-term05-sleep-completed.png`). Neither the sidebar's "master" worktree row nor any row
in the Activity panel showed any visible change at any point across three captures (start,
mid-run, after) — no dot appeared on the worktree row, and every Activity row kept its plain,
uniform circular glyph throughout, with no status text of any kind on any row (running or
otherwise) anywhere in the panel. The only place the command's running/finished state was
visible at all was the terminal block's own inline duration badge, not the sidebar or the
Activity panel.

**F-CHG-22** — Gesture: start an agent CLI in a terminal pane, give it a task that ends in a
question/permission prompt; while it waits, report every Activity row's status label and the
sidebar dot. Screen: launched `claude` in a terminal pane. As soon as it reached its own idle
input prompt (before any task was given), that pane's tab-strip label changed from a plain
dot to a literal **`?`** glyph, and the matching row in the Activity panel swapped its usual
plain circle for the same **`?`** glyph — every other row (12 other Terminal rows, `README.md`,
`main.rs`) kept its plain circle. First tried `Run the shell command: echo
hi-from-nested-agent` — this executed immediately with no permission prompt at all (this
nested session's Bash tool was evidently pre-approved), so it did not exercise the row.
Second try, `Edit the file main.rs and add a comment at the top saying // nested agent edit`,
did reach a genuine permission prompt: a diff preview of the proposed one-line insertion,
followed verbatim by:

```
Do you want to make this edit to main.rs?
  1. Yes
  2. Yes, allow all edits during this session (shift+tab)
  3. No

Esc to cancel · Tab to amend
```

(capture: `p104-g5-chg22-permission-prompt.png`). While this prompt was up, the same pane's
Activity-panel row still showed the **`?`** glyph in place of its status circle (unchanged
from the idle-prompt state above — the recipe's cited id, `activity-status-needs-input-…`, is
consistent with this being the same "needs input" state rather than a distinct one for
permission questions specifically); no other Activity row changed; and the sidebar's worktree
rows showed no dot of any kind, same as `F-TERM-PTY-05`. Selected "3. No" to decline the edit
— the transcript printed `User rejected update to main.rs` (capture:
`p104-g5-chg22-after-decline.png`), the Activity row's **`?`** glyph reverted to a plain
circle immediately, and a terminal `git diff -- main.rs` afterward confirmed the file was
untouched (only the pre-existing `README.md` change remained in `git status`). Exited the
nested session with `/exit` afterward.

## Group 6 · changes surface

Setup for this group: in the scratch fixture repo, staged one modified file
(`src/lib.rs`, a new `farewell()` function appended) and left a second modified file
(`README.md`) unstaged, then committed both as a base commit; a branch (`conflict-branch`)
was created from the prior commit with a different edit to the same `README.md` line, then
merged into the fixture's `master` to produce a genuine, unresolved merge conflict
(`git status`: `UU README.md`) for `F-CHG-16`. Opened the surface via the command palette
(`ctrl-k` → typed "Changes" → the single filtered result → Return did not visibly select it;
clicking the filtered row directly did).

**F-CHG-11** — Gesture: report the section headers and the control at each header's right
edge; click the staged section's control; report `git status` before and after. Screen: the
panel's own top header reads "Local changes (2)" with four controls at its right edge —
`Stage all`, `Expand All`, `Collapse All`, `Discard all`. Below it, two section headers:
`Staged (1)` with `Unstage all` at its right edge, and `Changed (1)` with `Stage all` at its
right edge (capture: `p104-g6-chg11-before.png`). `git status` before: `src/lib.rs` staged
(`M ` in the index), `README.md` unstaged (` M` in the worktree). Clicked `Unstage all` on the
`Staged` section — the on-screen change did not appear in the first capture taken immediately
after (a repeat of the session's known stale-frame lag), but a terminal `git status --short`
taken at the same moment already showed `M README.md` / ` M src/lib.rs` — both files unstaged.
A capture taken ~3s later confirmed the screen had caught up: the `Staged` section was gone
entirely and both files now sat under `Changed (2)` (capture: `p104-g6-chg11-after-unstage.png`).

**F-CHG-13** — Gesture: open a changed file's row menu and choose Open diff; report what
opens and its tab title. Screen: expanding the `src/lib.rs` row reveals inline row controls at
its right edge — `Discard`, `Stage`, `Open diff` (with a trailing `↗` glyph) — rather than a
separate right-click/kebab menu (capture: `p104-g6-chg13-row-controls.png`). Clicking
`Open diff` did not open any new tab in the top tab strip and did not change the main pane's
content, which continued to show the same collapsed `Local changes (2)` list. What it did do:
a second entry, also labelled verbatim `Changes`, was appended to the right-hand Activity
list (which already had one `Changes` entry from opening the surface) (capture:
`p104-g6-chg13-opendiff-result.png`). Clicking that new Activity row selected it (grey
highlight) but showed identical `Local changes (2)` content — not an isolated diff scoped to
`src/lib.rs`. So the observed tab title for "Open diff" is literally `Changes`, and its
content is the same multi-file list, not a per-file diff view.

**F-CHG-16** — Gesture: create a conflict first (merge a branch touching the same line); find
the conflicted row's "Resolve in terminal" control, click it, and report what the terminal
pane receives. Screen: with the fixture repo's genuine `UU README.md` conflict present, the
Changes panel showed one file under `Staged (1)` and the same file again under `Changed (1)`,
both labelled `README.md`. Expanding the `Changed` row's `README.md` revealed inline
conflict-marker diff content and four controls: `Discard`, `Stage`, `Open diff`,
`Resolve in terminal` (trailing `↗`) (capture: `p104-g6-chg16-conflict-controls.png`).
Clicking `Resolve in terminal` replaced the active pane's content with a new terminal pane
that had already run a command on entry; the pane's inline receive, verbatim:

```
diff --cc README.md
index 4bb8216,8c3f725..0000000
--- a/README.md
+++ b/README.md
@@ -1,4 -1,3 +1,8 @@@
  # P104 fixture repo

++<<<<<<< HEAD
 +Scratch git fixture for driving the tiller app during the P104 critic pass.x
 +x
++=======
+ Scratch git fixture for driving the tiller app during the P104 critic pass -- CONFLICT BRANCH.
++>>>>>>> conflict-branch

Resolve conflict at README.md
$
```

(capture: `p104-g6-chg16-terminal-receive.png`), leaving the shell sitting at a fresh prompt
for manual resolution. The right-hand Activity list gained a new row labelled verbatim
`Resolve README.md`, distinct from the generic `Terminal`/`Changes` row labels used elsewhere.

**F-CHG-18** — Gesture: press and drag a changed-file row a few centimetres without
releasing; report what appears under the cursor while dragging; release outside any target
and report the state afterward. Screen: pressed down on the collapsed `Changed` section's
`README.md` row and dragged down and sideways in two steps. While held, a small floating chip
reading verbatim `Diff` appeared over the panel, near the drag's starting area (capture:
`p104-g6-chg18-dragging.png`) — it did not visibly track the live cursor position across the
two intermediate `mousemove` steps taken during the same drag. Released the mouse button in
empty space below the file list, outside any row or drop target: the `Diff` chip remained
on screen, unchanged, for at least 3 further seconds after the release with no further input.
A subsequent click elsewhere in the panel made the chip disappear; at that point the
`Staged (1)` / `Changed (1)` sections were unchanged from before the drag — the file had not
been staged, unstaged, or reordered by the drag-and-release-outside-target (capture:
`p104-g6-chg18-after-settle.png`).
