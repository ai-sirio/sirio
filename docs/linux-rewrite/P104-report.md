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
