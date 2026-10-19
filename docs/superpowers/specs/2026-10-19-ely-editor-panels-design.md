# The file editor on Ely, project 2: the drawer — design

**Date:** 2026-10-19
**Status:** proposed
**Programme:** project 2 of `2026-10-17-ely-code-editor-design.md` (the
"parent spec"). Project 1 (foundations) merged as #639 and shipped in
v0.33.0.
**Mockups:** `2026-10-19-ely-editor-panels-search.html` and
`2026-10-19-ely-editor-panels-problems.html` beside this document — the file
tab with the drawer open on each tab. They are wireframes in Ely-like dark
colours, not pixel targets.

## §0 Intent

Most of the code in a Sirio worktree is written by agents. The user opens a
file to **check what an agent did**. Two questions come up there, and today
the answer to both means leaving Sirio for another editor or a terminal
`rg`:

- *Did the agent leave anything broken?* — **Problems**: every problem the
  running language servers know about in the worktree, not only the open
  file's.
- *Where else is this name?* — **Search**: find a text across the worktree,
  open each hit, and replace it everywhere when needed.

The user considers worktree search **part of the editor**: it lives in the
file tab, next to Problems, not in the right panel.

Success: with a file tab open, the user sees the worktree's errors without
asking, follows a name across the worktree and replaces it, all without
leaving the tab — and a replace never overwrites work the user has not
saved.

## §1 Changes to the parent spec

Decided by the user in this brainstorm; the parent spec's §2–§3 are
superseded on these points:

| Parent spec | Now |
|---|---|
| Worktree search is a right-panel view, `PanelView::Search`, available with no file open | Search is a **tab of the drawer**, beside Problems. The right panel does not change. |
| The drawer has Problems and Output; Output is the language server's log | **No Output tab.** Not in this project, and the programme no longer plans it. Ely's `console.rs` stays unvendored. |
| Problems shows the open file's problems | Problems shows **every problem the running servers report in the worktree**. |
| Project 1's PR named a scrolling minimap as a project-2 follow-up | **Dropped.** The user prefers a minimap that always shows the whole file, even approximately; the sampled minimap stays as it is. |

## §2 Decisions

Theirs = the user's, in this brainstorm. Mine = my call, open to review.

| Question | Decision | Whose |
|---|---|---|
| Where Search lives | Drawer tab beside Problems | Theirs |
| Search replaces too | Yes: **Replace all** across the worktree | Theirs |
| A replace meets a file open with unsaved changes | That file is **skipped** and named; every other file is written | Theirs (skip-and-name is my reading of "Sirio must refuse") |
| A replace meets a file an agent is writing | Not detected; written anyway (Sirio cannot know which file an agent touches) | Theirs |
| Problems scope | Every problem the running servers know in the worktree | Theirs |
| Output panel | Out | Theirs |
| Drawer opening | Opens **by itself on Problems** when errors appear | Theirs |
| Minimap | Unchanged | Theirs |
| One search per worktree, shared by every file tab's drawer | Yes | Mine |
| `Ctrl/Cmd+Shift+F` with no file tab open | Does nothing | Mine |
| Undo of a replace | None from Sirio; `git diff` / the Diff view remain | Mine |
| What auto-opens the drawer | An error the user has not seen, by identity (server, file, message), against a persisted seen set (§4.4) | Mine |
| Replace all on a capped result set | Disabled until the search is narrowed | Mine |
| A dirty tab's file in Search | Searched in its buffer, marked unsaved, skipped by Replace all | Mine |
| Drawer state | Open/closed, height and tab per worktree, persisted | Mine |
| Search engine | Files listed by git, matched in process with the in-file find's engine (§5.2) | Mine |
| Problems order | The current tab's file first, then files by worst severity | Mine |
| Languages with no running server | Problems says which servers are running and silent; it never looks complete when it is not (§4.4) | Mine |

## §3 Goals, non-goals, anti-goals

**Goals.** The drawer, Problems over the worktree, Search with Replace all,
the two key chords, the auto-open, each proven end to end with captures.

**Non-goals.** Output / server log. Search in the right panel. A scrolling
minimap. Replacing one hit or one file at a time. Undo of a replace. Starting
language servers to fill Problems (a server still starts only when a file of
its language is opened, as today). Anything of project 3 (completion, code
actions, rename…).

**Anti-goals** — failures even if everything else works:

- A Replace all that changes a file whose tab holds unsaved edits.
- A replace written from stale results: a file whose content changed since
  the search gets the old match positions applied.
- A drawer that reopens every time a server republishes while an agent
  works, after the user closed it.
- A Search that misses a file the agent just created because it is not yet
  added to git.
- A Problems tab that reads "no problems" when the truth is "no server is
  looking".
- A cap or a skip that is silent: every limit that drops results says so.

## §4 The drawer

### §4.1 Shape

The drawer sits inside the file tab between the code and the status row,
where the parent spec's §4 reserved its place:

```
breadcrumb
code (gutter, code, minimap)
──── drawer: [Problems ⊘3 ⚠5] [Search]              ⌃J ✕
     (the open tab's panel)
status row
```

- The tab strip is Ely's `Tabs` (already vendored). Problems' title carries
  the counts per severity, visible with the drawer closed (the closed drawer
  is its header row only, as in the parent prototype).
- The height is resizable by dragging the drawer's top edge, between a
  minimum that shows the header plus three rows and a maximum that leaves the
  code at least a few lines (builder's call on the numbers).
- `✕` and `Ctrl+J` close it to its header row.

### §4.2 State

One drawer state **per worktree**: open or closed, height, selected tab,
and the set of errors the user has seen (§4.4).
Every file tab of that worktree draws the same state, so switching tabs does
not change the drawer. The state is persisted (`sirio_persistence`, a new
migration) and restored on relaunch. A change-request snapshot tab (read
only) draws the drawer too: its file is not in the worktree's files, but the
worktree's problems and search are still the worktree's.

### §4.3 Keys

- `Ctrl+J` on Linux and Windows, `Cmd+J` on macOS (as in VS Code; gpui's
  `secondary-j`), toggles the drawer. Neither is bound today.
- `Ctrl+Shift+F` / `Cmd+Shift+F` opens the drawer on Search and focuses the
  query field, with the editor's selection (one line or less) as the query
  when there is one.
- With no file tab focused, both do nothing.
- Both are free today (no binding in Sirio, Ely, bezel or gpui). They are
  bound in the file tab's context, and the parent spec's §5.5 rule holds:
  shell bindings win collisions. `Escape` in the drawer's fields returns
  focus to the code.
- Windows swallows some `ctrl-shift` chords as system hotkeys (#374, the
  note above the window chord table in `sirio/src/main.rs`). Whether
  `ctrl-shift-f` reaches Sirio on Windows is checked by hand; if it does not,
  a Windows variant is chosen the way #374 chose its three, and recorded.

### §4.4 Opening by itself

One rule, per worktree, on the diagnostics store (§5): the drawer opens on
Problems when the worktree holds an **error the user has not seen**.

- An error is known by its **identity**: the server that reported it, its
  file (relative to the worktree) and its message — not its line, so an edit
  that shifts lines never makes an error new. Two copies of the same message
  in one file count twice.
- The **seen set** holds those identities with their counts. It changes at
  two moments only: while the drawer shows Problems, and when the user
  closes the drawer. Then it becomes the current errors, plus the seen
  entries of every (server, file) that no running server has reported since
  it started — so a look in the middle of a refill never forgets the files
  not reported yet. Nothing else moves it: not a fix, not a server exit or
  restart, not Search.
- The drawer opens when some identity's current count is above its seen
  count. So: close with three errors, the server republishes them, clears
  and republishes them, restarts, or Sirio relaunches and they come back in
  any order — it stays closed; the agent breaks something with a message
  the user has not seen, or a second copy of a seen one — it opens.
- **Accepted cost** (my call): an error the user saw, fixed while they were
  not looking at Problems, that then comes back identically (an agent
  reverting its own fix) does not reopen the drawer. It is counted in
  Problems' title all the same. And the converse, rarer one: a close that
  lands between a server clearing a file and republishing it records the
  file as fixed, so the identical republish reopens the drawer once.
- The seen set is persisted with the drawer state (§4.2), so a relaunch
  does not reopen a drawer the user closed.
- Warnings and notes never open it, and are not in the seen set.
- If the drawer is already open on Search, it stays on Search: the user is
  working there. Problems' title counts update; the seen set does not move.
- It never steals focus from the code or from a terminal: it opens, the
  caret stays where it was.

## §5 Problems

### §5.1 The store

Today `spawn_router` (`sirio/src/main.rs`) parses each
`textDocument/publishDiagnostics` and `apply_diagnostics` hands it only to
the open `FileView`s of that path; a publish for a file with no tab is
discarded. Project 2 adds a **diagnostics store per worktree**:

- keyed by (server, path): each publish replaces that server's entries for
  that path; an empty publish clears them;
- a server that exits or restarts drops all its entries;
- paths outside the worktree are ignored;
- open `FileView`s keep receiving their file's diagnostics, now from the
  store, so the gutter, the code and the minimap and Problems never disagree;
- the store is in memory: it is rebuilt from the servers after a relaunch,
  not persisted.

Problems redraws from the store, debounced (a server can publish a burst of
hundreds of files after a check).

### §5.2 Drawing

Ely's `ProblemsPanel` (`src/editor/problems.rs` at `e17e31a6`, vendored by
this project, §7): grouped by file, each row the message, the source
(`rustc`, `clippy`, …) and `line:column`; severity toggles for errors,
warnings and notes (LSP Information and Hint both count as notes). Order:
the current tab's file first, then Ely's own order (files with worse
problems first). A click opens the file at that line and column in the
same pane, as the References view does today
(`RightPanelActionEvent::OpenAtLine`), or focuses its tab if open.

### §5.3 When Problems is empty

Problems never shows a bare "no problems". It says, in one line, which
servers are running in the worktree and that they report nothing (for
example `rust-analyzer: no problems`), or that no language server is
running in this worktree yet. How complete the list is depends on the
server: rust-analyzer's check covers the whole crate, others report only
open files. That is stated in the design, not in the UI.

## §6 Search

### §6.1 Query

Ely's `SearchFilters` and the query field: the query, the three toggles of
the in-file find (match case, whole word, regex), an include and an exclude
glob field (Ely's `passes(path, include, exclude)`), and the replace field.
Ely's `ignore` toggle is not shown: Search always respects `.gitignore`.

Typing searches as you type, debounced; a new query cancels the running
search.

### §6.2 Engine

My call, refining "`git grep --untracked`" from the brainstorm (the
behaviour the user saw is the same: `.gitignore` respected, untracked files
included):

- The file list is git's: `git ls-files --cached --others
  --exclude-standard` in the worktree, through `sirio_git` (no new
  dependency). Submodules are not entered.
- Matching runs in process with the **same engine and options as the
  in-file find** (Ely's `find_all`, the `regex` crate), off the GPUI thread,
  streaming results per file. A query therefore means exactly the same thing
  in the find widget and in Search, and Replace all applies exactly the
  matches shown — `git grep`'s own regex dialect differs from Rust's.
- The matcher runs on each **whole file**, as the in-file find runs on the
  whole buffer, so `^`, `$` and `\n` mean the same in both. A match that
  spans lines is **one** result, shown on its first line and previewed to
  that line's end; Replace all replaces its whole span.
- A file whose open tab has unsaved changes is searched **in its buffer**,
  not on disk: the user sees what is in the tab, and a click lands on the
  text it shows. It is still marked *unsaved* and skipped by Replace all.
- Binary files, non-UTF-8 files and files above a size bound are skipped;
  the result summary names how many and why.
- Results are capped (builder's numbers, in the tens of thousands of
  matches); the summary says when the cap was hit.

### §6.3 Results

Ely's `SearchPanel` with `FileHits` / `Hit`: grouped by file with a count,
the line with each match highlighted, files foldable. The summary line:
`N results in M files` plus any skips and caps. Files whose open tab has
unsaved changes are marked **unsaved**.

**One search per worktree.** The query, toggles, filters, results, folds and
the selected result belong to the worktree, and every file tab's drawer in
it shows them. Clicking a result opens its file at that line (or focuses its
tab) with the match selected; the drawer there shows the same search with
that result selected, so stepping through results across files keeps the
thread. Results are not persisted across a relaunch.

Results reflect the files when the search ran. They are re-run after a
Replace all and by a refresh control; they are not live.

### §6.4 Replace all

While the replace field is not empty each hit previews its replacement
(Ely's `replacement`). With regex on, the replacement's group references
follow the in-file Replace's semantics. While the results are **capped**,
Replace all is disabled and says why (*too many results — narrow the
search*): it never changes only the shown part of a set it claims to cover.
Files Search skipped (binary, non-UTF-8, too large) are not searched and
therefore not replaced; the report repeats the summary's skip line.
**Replace all**:

1. Takes the files of the current results.
2. Skips each file whose open tab is dirty (`FileView::is_dirty`).
3. For every other file: reads it from disk **now**, re-runs the match on
   that content (never applies the stored results' positions), replaces,
   and writes it back keeping its line ending, atomically (write a sibling,
   rename). A file that vanished or became unreadable is skipped.
4. Reports in the drawer: `Replaced X in Y files`, then each skipped file by
   name and reason (unsaved changes, missing, unreadable).
5. Re-runs the search.

Open clean tabs adopt the new text through their existing external-change
path (`FileView::check_external`, driven by the file watcher); their caret
and scroll stay (`reset_keeping_view`). There is no undo from Sirio and no
confirmation dialog: the button's label carries the counts (`Replace 9 in 3
files`) so what it will do is visible before pressing.

## §7 Vendoring

From the pinned `e17e31a6` (the parent spec's §5.1 rule), as recorded
local changes in `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`:
`src/editor/problems.rs`, `src/editor/search.rs` and
`src/editor/filters.rs`, plus whatever of their imports the crate lacks.
`search.rs` also holds `ReferencesPanel`; it comes with the file, unused
until project 3. `console.rs` is not vendored. The parent spec's proof that
nothing else moved applies again (the chat and change-request E2Es pass
unchanged; a diff of the vendored tree outside the new files is empty).

## §8 Proof

End to end, the parent spec's §6 shape: new scenarios in
`Scripts/Tests/test-editor-e2e.sh` (or a sibling script, builder's call),
against a real isolated debug Sirio on a private Xvfb, with the fake
language server (`Scripts/Tests/fake_lsp.py`) taught to publish diagnostics
for files that are not open. Debug-only `surface.editor.*` verbs read the
drawer's and the store's state. Scenarios:

- the fake server publishes an error for an unopened file: the drawer opens
  by itself on Problems, the file is listed, the caret did not move;
- the user closes the drawer; the server republishes the same error, or
  clears and republishes it: it stays closed; an error with a new message: it
  opens; the errors are fixed and an error with a new message comes: it
  opens; a seen error fixed and returning identically: it stays closed (the
  accepted cost); the server restarts with the same errors: it stays closed;
  a quit and relaunch with the same errors: it stays closed;
- new errors while the drawer is on Search: it stays on Search;
- warnings only: it does not open;
- a click on a problem opens that file at its line;
- the server exits: its problems go; no server running: the empty line says
  so;
- `Ctrl+Shift+F` opens Search with the selection as query; results include
  an untracked file and exclude a `.gitignore`d one; regex and whole word
  behave as in the in-file find;
- clicking a result in another file opens it at the line, and that tab's
  drawer shows the same search;
- Replace all with one target open and dirty, one open and clean, one
  closed: the dirty one is unchanged on disk and named, the other two are
  replaced on disk, the clean tab shows the new text with its caret kept;
- a file changed on disk between the search and Replace all is replaced by
  its current content, not the stale positions;
- a regex match spanning two lines is one result and is replaced whole;
- a query that hits the cap disables Replace all with its reason;
- drawer state survives a quit and relaunch.

PID-matched captures in light and dark of: Problems open, Search with a
replacement preview, the post-replace report. The run's transcript,
captures and the files read back are the artifact.

## §9 Out of scope

Output and the server log; the right-panel search; the scrolling minimap;
per-hit or per-file replace; undo; persistence of search results; starting
servers for Problems. A separate issue, not this project: `sirio_lsp` pipes
a server's stderr and never reads it (`sirio_lsp/src/server.rs`), so a
server that writes a lot there may block; to be checked and fixed on its
own.

## §10 Risks

- **Ely's panels need code outside the vendored closure.** Same rule as
  the parent spec: rewrite against `e17e31a6` or leave out, recorded.
- **Diagnostics volume.** rust-analyzer can publish thousands of entries
  for a large workspace; the store and the panel must stay responsive
  (debounce, and Ely's list drawing only what is visible — if it does not,
  that is a recorded local change).
- **Search on a large worktree.** Streaming, cancellation and caps keep it
  bounded; the E2E includes a worktree large enough to see a cap.
- **Key handling on macOS and Windows** is unverified by the E2E, as in
  project 1; the PR says which rows were seen by hand.

## §11 Calls made by me, not by the user

Every row marked "Mine" in §2, plus: the drawer's resize limits (§4.1);
the snapshot tab drawing the drawer (§4.2); errors known by identity
against a persisted seen set, and its accepted cost (§4.4); `Cmd+J` on macOS and the Windows `ctrl-shift-f` check (§4.3);
the selection-as-query rule and `Escape` returning to the code (§4.3); whole-file
matching with a multi-line match as one result (§6.2); the store keyed by (server, path),
in memory, paths outside the worktree ignored (§5.1); LSP Information and
Hint grouped as notes (§5.2); the matching engine shared with the in-file
find instead of `git grep`'s own (§6.2); no `ignore` toggle (§6.1); results
not live and not persisted (§6.3); atomic writes, no confirmation dialog,
counts on the button (§6.4); the stderr issue kept out (§9).
