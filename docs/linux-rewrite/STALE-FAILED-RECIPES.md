# Exercise recipes for the 39 stale-FAILED rows (FABLE-09)

FABLE-08 found 39 of the 141 `FAILED — absent` rows already built (`STALE-FAILED-CENSUS.md`).
Only the critic converts a verdict, and `pireview` is one person. This doc exists to make those
39 conversions cheap: for each row, where the app starts, the click path or command that reaches
the function, and the on-screen element to look at.

**The rule that matters: a recipe says what to DO, never what to CONCLUDE.** "Open the + menu,
choose Changes, report what the panel shows" is a recipe; "verify the diff renders" is not,
because it suggests the answer — a critic who confirms instead of probing is no longer an
independent control. Nothing here marks anything PASSED; every verdict remains `pireview`'s.
Report what the screen showed, verbatim where wording matters.

**Ordering is by cost.** Groups 1–6 run in ONE app launch (30 rows); group 7 needs an agent on
PATH (+1 relaunch for its last item); group 8 is the settings surface (same launch as 7 works);
group 9 is not exercisable from the UI alone and says why. Doors were re-verified against the
FABLE-08 snapshot: command palette = `ctrl-k` (also `ctrl-shift-p`), Settings = the status-bar
gear, Changes surface = the palette's "Changes" entry. `debug_selector` ids in parentheses are
for the drawn-test harness, when preferred over a display.

## Session 1 — one workspace on a git worktree, one terminal tab open

### Group 1 · tab strip and tab menu (8 rows)

- `F-SID-19` — Press `ctrl-t` three times. Report what appears in the tab strip after each press.
- `F-TAB-02` — Keep pressing `ctrl-t` until the strip runs out of width. Report what the right
  edge of the strip shows and what clicking it lists (`tab-overflow-button`).
- `F-TAB-21` — Right-click a tab. Report the menu's items top to bottom, including greyed
  entries and any reason text on them.
- `F-TAB-15` — From that menu choose Close. Report what happens to the tab and which tab is
  selected afterwards.
- `F-TAB-17` — With ≥4 tabs, right-click the second → Close Others. Reopen tabs, right-click
  the second → Close Tabs to the Right. Report the strip after each.
- `F-TAB-12` — Right-click the last tab → Move Earlier, then right-click the first tab and
  report how Move Earlier presents there. Report the strip order after each action.
- `F-TAB-13` — Split the terminal first (right-click inside the pane → Split Right). Right-click
  a tab and report every "Move to …" entry with its enabled/disabled state and reason text;
  choose one and report where the tab went.
- `F-TAB-14` — Right-click a tab → Rename: type a name, press Enter; rename again, press
  Escape. Report the tab title after each (`tab-rename-field`).

### Group 2 · file tabs and the editor (9 rows) — press `ctrl-o`, open `README.md`, then a `.rs` file

- `F-EDIT-07` — With both files open, report how each tab presents its text (colouring, layout)
  for `.md` versus `.rs`.
- `F-EDIT-01` — On the `.md` tab find the mode control. Click each side; report what the content
  area shows in each mode.
- `F-EDIT-03` — Switch the `.md` to Preview. Report what is displayed and how it arrives.
- `F-EDIT-02` — In Code mode select a word, click Bold, then Italic on the toolbar
  (`file-format-toolbar`). Report the buffer text after each click.
- `F-EDIT-04` — Type a character in a file tab, press `ctrl-s`. Report the tab's indicator
  before and after, and `git diff --stat` for that file from a terminal pane.
- `F-EDIT-06` — Covered by the same action: after `ctrl-s`, `cat` the file in a terminal pane
  and report its content.
- `F-EDIT-05` — With the `.md` tab open, append to the same file from a terminal pane
  (`echo x >> README.md`), return to the tab. Report any banner verbatim and what each of its
  controls does when clicked.
- `F-EDIT-08` — Press `ctrl-o` and pick the already-open `README.md` again. Report the tab
  strip and which tab has focus.
- `F-TAB-16` — Type in a file tab without saving, then close that tab. Report the dialog
  verbatim and what each button does. Repeat closing the whole workspace with two dirty tabs
  and report that dialog too.

### Group 3 · right panel, Files tab (4 rows) — open it with `ctrl-shift-i`

- `F-CHG-03` — Click Refresh. Report what the panel shows between the click and the list
  settling. (A Retry control is reachable only after a failed refresh; inducing one needs an
  external failure — optional, e.g. temporarily `chmod 000 .git`.)
- `F-CHG-05` — Click a file row, then press down, up, space. Report selection and what each key
  did.
- `F-EDIT-10` — Right-click a file row. Report the menu's items.
- `F-EDIT-11` — Choose Copy Path, paste into a terminal pane. Report the pasted text
  (`file-context-copy-path`).

### Group 4 · sidebar (3 rows)

- `F-SID-07` — Right-click the project header → Project Settings. Report the fields the card
  shows and which respond to input.
- `F-SID-08` — Add a plain non-git folder as a project. Right-click it and report how
  "Initialize Git repository" presents there versus on a git project (state, reason text);
  click it on the plain folder and report `git status` of that folder from a terminal.
- `F-SID-09` — Right-click the project → Show in File Manager. Report what opens on the
  desktop, and anything that appears at the bottom of the sidebar if nothing does.

### Group 5 · activity signals (2 rows) — same session, right panel → Activity

- `F-TERM-PTY-05` — In a terminal pane run `sleep 8`. While it runs and after it ends, report
  the worktree row's dot in the sidebar and each row's status text in the Activity panel.
- `F-CHG-22` — Start an agent CLI (e.g. `claude`) in a terminal pane and give it a task that
  ends in a question/permission prompt. While it waits, report every Activity row's status
  label and the sidebar dot (`activity-status-needs-input-…` is the drawn id for that state).

### Group 6 · changes surface (4 rows) — palette (`ctrl-k`) → "Changes"; fixtures: modify two files, `git add` one

- `F-CHG-11` — Report the section headers and the control at each header's right edge; click
  the staged section's control and report `git status` before and after.
- `F-CHG-13` — Open a changed file's row menu and choose Open diff. Report what opens and its
  tab title.
- `F-CHG-16` — Create a conflict first (merge a branch touching the same line). Find the
  conflicted row's "Resolve in terminal" control, click it, and report what the terminal pane
  receives.
- `F-CHG-18` — Press and drag a changed-file row a few centimetres without releasing. Report
  what appears under the cursor while dragging; release outside any target and report the
  state afterwards.

## Session 2 — chat (needs an ACP-capable agent on PATH; last item costs a relaunch)

- `F-TAB-07` — Click the strip's New Chat control. Report the list it offers (names, order,
  disabled entries) and what choosing one does.
- `F-CHAT-08` — Send a prompt that yields a long reply. While text streams, report the round
  control at the composer's right (glyph), click it, and report the control afterwards and the
  line the transcript footer shows (`stop-glyph` / `send`).
- `F-TAB-27` — Exchange one turn, close the chat tab, right-click any tab. Report whether a
  Resume Chat entry is present, its state, and what choosing it restores.
- `F-TAB-08` — Relaunch with agents absent from PATH (e.g. `PATH=/usr/bin:/bin` minus agent
  shims). Open New Chat and report the panel's wording verbatim. Do this last — it costs the
  relaunch.

## Session 2 (same launch) — settings, via the status-bar gear (3 rows)

- `F-SET-02` — With Settings open press Escape; report which surface is visible after and where
  focus went. Reopen, open a picker menu inside Settings, press Escape once, then again;
  report each step.
- `F-SET-10` — On the providers page click the Refresh action row; report any change in the
  account-status rows (`settings-provider-account-status-…`). Toggle a provider's visibility
  and report the status bar's segments before and after.
- `F-SET-04` — Flip "resume agent sessions" on General, close and reopen Settings in the same
  run; report the toggle state. The across-relaunch half depends on the P58 persistence
  handoff (codex11) — a relaunch check is not yet meaningful, so do not spend one on it.

## Not exercisable from the UI alone (2 rows)

- `F-SET-09` + `F-AGENT-SAFE-01` — one function, two rows: `agent_skill_install_command`
  (`tiller_project/src/skill.rs:9`) has **no UI consumer** — the Copy-install-command control
  is itself an absent row (F-SET-08, removed by design per its evidence). Today the function is
  exercisable only as `cargo test -p tiller_project`; a UI recipe depends on F-SET-08 (or any
  consumer) landing first. Writing a click path here would be inventing one.

## Accounting

39 rows: 30 in session 1 (one launch, git fixtures only), 7 in session 2 (one launch with an
agent on PATH, +1 relaunch for F-TAB-08), 2 not UI-exercisable with the dependency stated.
