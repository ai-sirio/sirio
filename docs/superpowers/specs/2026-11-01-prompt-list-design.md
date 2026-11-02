# Saved prompts in the right panel

**Date:** 2026-11-01
**Status:** design
**Mockups:** `2026-11-01-prompt-list-mockups.html` (beside this file; list B, then
editor placement 1)

## §0 Intent

The user keeps about twenty long, reusable prompts in a note in macOS Notes (a
coordinator brief for herdr workers, a review rubric, a brainstorming opener…).
Almost all of them apply to every project. Every use is the same trip: switch to
Notes, find the prompt, copy it, come back, paste it into an agent's terminal or
chat. Sometimes the paste goes somewhere outside Sirio. This feature brings the
prompts into Sirio's right panel, one click away from the clipboard.

Decided with the user, in order:

1. **What it replaces:** the Notes note: about twenty prompts, global rather than
   per project. They are pasted into terminals, agent chats and other apps, so
   **copying to the clipboard is the core gesture.**
2. **No sync for now.** ("sincronizzazione non serve per ora")
3. **List B**, cards showing the title and the first two lines of the text. Three
   layouts were shown: A compact rows, B cards with a preview, C expandable rows.
   The user chose B, and pointed out it lacked an edit button ("B ma manca il
   pulsante di modifica"). So **a pencil sits beside the copy button**, and both
   are always visible.
4. **Editor placement 1:** the prompt opens as a centre-area tab in Sirio's own
   file editor. The rejected options were inline in the sidebar and a modal
   window.
5. The rest of the play-back was accepted as written ("va bene così, procedi"):
   - clicking a card's body opens it in the editor, like the pencil;
   - copying happens only from the copy button;
   - the title has its own field above the text;
   - New prompt opens an empty tab with the cursor in the title;
   - the prompt saves itself as you type;
   - the pencil on a prompt that is already open goes to its tab;
   - Delete is in the card's right-click menu and asks for confirmation;
   - the newest prompt is on top, with no drag-to-reorder;
   - the section works without a selected worktree;
   - the prompts are stored as one Markdown file each, in a Sirio folder.

Everything below that is not one of these five is a call made in the
brainstorm. §9 lists those calls again for the user to check.

## §1 Goals, non-goals, anti-goals

**Goals**

- One click copies a prompt's full text, exactly as written, to the system
  clipboard.
- Writing and editing a long prompt is as comfortable as editing a file: full
  width, with the editor's undo, find and selection.
- The same list in every project and worktree.

**Non-goals** (not asked for; do not build them):

- Sync between machines.
- Folders, tags, search or filtering. Twenty cards fit without them.
- Drag-to-reorder.
- Inserting a prompt straight into a terminal or chat. The clipboard is the
  route.
- Placeholders or variables inside prompts.
- Importing from Notes. The user pastes each prompt once.
- Watching the folder for edits made by other programs (§4).

**Anti-goals** (the feature fails if any of these happens, even when everything
else works):

- What lands on the clipboard differs from what the editor shows, whether by a
  missing last keystroke, an added title, front matter or changed line endings.
- Something the user typed is lost: a crash, a quit or a closed tab drops text.
- A deleted prompt comes back because a save wrote it again.
- Plain file tabs change behaviour. Autosave is for prompts only (§3).
- An E2E or a debug build touches the user's real prompts (§4, §7).
- Text typed in a prompt tab is thrown away because the file changed on disk
  (§3, "Outside edits").

## §2 The list: a Prompts view in the right panel

A seventh `PanelView`, **`Prompts`**, comes last in `PanelView::ORDER`
(`rust/crates/sirio_ui/src/right_panel/mod.rs`), after Open. Its rail icon must
differ from every other rail icon and from the chat's. Like the rest of the rail,
it comes from the vendored Zed set (`rust/assets/icons/zed/`, copied byte for
byte; see `icons.rs` and its byte-equality test). The builder picks the glyph.
The socket name is `prompts`, and the element id is `right-panel-tab-prompts`.

**Header row:** the label `Prompts`, the count in muted text, and a **New
prompt** button on the right (Ely button, plus icon).

**Cards**, newest first, in a scrolling column:

- The title on one line, semibold, with an ellipsis. An empty title shows
  `Untitled prompt` in muted text.
- The text's first two lines in muted text, clamped with an ellipsis. Leading
  blank lines are skipped.
- On the right, two Ely icon buttons that are always visible: **Edit** (Lucide
  `Pencil`) and **Copy** (`Copy`). Each has a tooltip.
- Raised background, theme border and radius, with the border brightening on
  hover. Use only `theme.ely` tokens, so light and dark both follow the theme
  (see the mockup for proportions).
- **A click on the card body** behaves like Edit.
- **Right-click** opens the card's context menu with one row, **Delete…**. It
  follows the Ely `ContextMenu` precedent in `right_panel/change_requests.rs`
  (`row_menu`).
- **Delete…** opens a confirmation dialog: "Delete *title*?", detail "This
  removes the prompt from disk.", a ghost Cancel and a Danger **Delete**. Follow
  `render_discard_dialog` in `changes.rs`. A confirmed delete removes the file
  and its index entry, and closes any open tab of that prompt *without saving*
  (see §3, "never recreate").

**Copy:**

- Copy puts the prompt's full text on the clipboard with
  `cx.write_to_clipboard(ClipboardItem::new_string(..))`. The text is exactly the
  file's contents.
- If the prompt is open in a tab with an autosave still pending, the copied text
  is the editor's current text. The pending save is flushed first, or the text is
  read from the view. The builder decides how, as long as the result holds.
- **Feedback:** the copy button turns into a check for 2 s, the way
  `copy_local_text` in `chat/mod.rs` does it. A later copy restarts the timer.
  There is no toast.

**Empty state:** a short line ("No prompts yet") and a New prompt button in the
body. The header button stays too.

**Refresh:** the list reads the folder when the view is shown or built. The panel
is rebuilt on every worktree switch. It reads again after any write Sirio makes
itself: autosave, title change, new, delete. Edits made in another program show
up the next time the view is shown. There is no watcher. `sirio_markdown`'s
monitor is inert on macOS, and the host's `notify` watcher covers worktrees only.

**Failures:** a Copy, New prompt or Delete that fails (a file that cannot be
read, a folder that cannot be written) raises the workspace's existing toast
(`show_toast` in `main.rs`, the one a failed project identity save uses) with
the reason. The card stays as it was and Copy shows no check. The editor's
per-tab bell is not reachable from the right panel, and no new kind of notice
is added.

## §3 The editor tab: prompt mode on `FileView`

A prompt opens as an ordinary **file tab**: `TabKind::Editor`,
`TabContent::File`, through `add_file_tab` (`rust/crates/sirio/src/main.rs`). It
is not a new tab kind. About 46 `TabContent::File` sites give it the dirty mark,
the close paths, dedupe and persistence. A new kind would silently skip all of
them.

**Prompt mode** is a property of `FileView`. It is on when the file's parent
directory is the prompts folder (§4). Every route then gets it: the Prompts
list, Ctrl+O on a prompt file, and a tab restored from `editor_path` after a
restart. In prompt mode:

- **The breadcrumb is replaced by the title field:** a single-line Ely
  `TextInput` (`ely_ui::new_input`) bound to the prompt's index title. Changing it
  updates the index (§4) on the same debounce as the text. Ely's
  `TextInput::set_text` emits `Changed`, so **never write the title back from the
  `Changed` handler**, or it loops.
- **Code mode is forced.** A `.md` file opens in Preview today
  (`initial_markdown_mode`), so prompt mode skips that and hides the
  Preview/Code switch.
- **Autosave:** after an edit to the text or the title, the prompt is saved once
  typing pauses, on a short debounce. The builder picks the delay. A few hundred
  milliseconds is right. Pending saves are also flushed when the tab closes and
  when the app quits. **Closing a prompt tab never asks a question.** **Plain
  file tabs stay explicit-save** (Ctrl+S) and keep their dirty-close prompt.
  Prompt mode must not change them.
- **Never recreate:** a prompt save does not write a file that is no longer on
  disk, whether Sirio's own delete removed it or another program did. The tab
  then shows the editor's existing missing-file state. This rule is what keeps a
  deleted prompt deleted, even when a tab for it is parked in another worktree.
- The tab's label is the prompt's title, or `Untitled prompt`.
- **Outside edits:** the last write wins, and typed text is never discarded.
  - A prompt tab with nothing left to save takes the disk's new text. The editor
    already does this when it checks the disk on focus, and through the file
    monitor on Linux and Windows.
  - A prompt tab still holding unsaved typing keeps it, and its next autosave
    overwrites the file.
  - Prompt mode never shows the Reload/Keep conflict banner and never refuses an
    autosave because of a conflict. Today `Editor::save` refuses while a
    conflict is open (`editor.rs`), and closing a dirty tab in conflict opens the
    discard dialog. Prompt mode bypasses both.
  - Editing the same prompt elsewhere while its tab is open is rare, so this
    keeps the simple rule without designing a merge.

**New prompt** creates the file at once, because a `FileView` builds no
`CodeEditor` for a missing file. It then creates an empty index entry at the top,
opens the tab and **focuses the title field**. Focus a freshly drawn element via
`on_next_frame`, from the render that draws it. Focusing it directly reads the
last drawn frame, the known gpui trap. **An untitled prompt with an empty text
deletes itself** (file and index entry) when its tab closes, so pressing New and
backing out leaves nothing behind.

**Edit / card click:** `add_file_tab` already dedupes against the current
worktree's tabs, so the pencil on an open prompt selects its tab. File tabs open
in the **Secondary pane** (`TabKind::Editor.default_pane()`), so the prompt sits
beside the agent's terminal rather than replacing it. That is a deliberate
difference from the mockup (§9).

`docs/agents/architecture/file-editor.md` gains one paragraph on prompt mode:
the autosave exception, and the never-recreate rule.

## §4 Storage

**The folder:**

- Linux: `$XDG_DATA_HOME/Sirio/prompts`, else `~/.local/share/Sirio/prompts`.
  This matches the `Sirio/projects` precedent in `sirio_project/src/domain.rs`.
- macOS: `~/Library/Application Support/Sirio/prompts`.
- Windows: under `%LOCALAPPDATA%\Sirio\`, matching `app_support_root`. The
  builder confirms it.

**Debug builds** default to a `prompts` folder beside their checkout-scoped
database. This follows `database_path_for` in `rust/crates/sirio/src/session.rs`,
the precedent that keeps a dev build away from the installed app's data. Only
release builds use the folders above.

**`SIRIO_PROMPTS_DIR`** overrides the folder verbatim, in every build. Every E2E
sets it anyway (§7), so the test's isolation does not rest on the build profile.
The folder is created on first write, never on read.

**One file per prompt:**

- The file's contents are the prompt's text, exactly what Copy puts on the
  clipboard. There is no front matter and no title line.
- Filenames are **generated by Sirio**: ASCII, unique, `.md`, never renamed.
  They are never derived from the title, so a title can hold any character.
- New files are written LF. The editor keeps an existing file's own line ending
  (`Document`).

**`index.json`** in the same folder holds the titles and the order:

- Shape: `{"version":1,"prompts":[{"file":"…md","title":"…"}, …]}`, in display
  order, newest first.
- It is written atomically (temp file plus rename) on every change Sirio makes.
- Each change **re-reads the index from disk and applies only itself** (insert,
  retitle or remove) before writing. It never writes back a copy held in memory.
  Nothing stops two Sirio processes from running at once, and this way they lose
  at most a change made at the very same moment, not each other's earlier edits.
- Reconciliation rules, applied on every read:
  1. a `.md` file with no index entry is shown with its file stem as title,
     after the indexed ones, sorted by name;
  2. an index entry whose file is missing is dropped;
  3. a missing index means every file is unindexed (rule 1);
  4. an index that cannot be parsed is treated as missing, and is rewritten on
     the next change.
- Files that are not `.md`, and subfolders, are ignored.

The store (folder resolution, read with reconciliation, create, retitle, delete,
read text) has no gpui types. Its home (a module in `sirio_ui`, or a small crate)
is the builder's call. Read `docs/agents/architecture/crates.md` first if it
becomes a crate.

## §5 Without a worktree

The right panel's body draws "No worktree selected" for every view today
(`mod.rs`, the `!self.worktree_selected` branch). **Prompts is the exception**:
its body renders with or without a worktree. With no worktree, the list, Copy and
Delete work. **Edit, New prompt and the card click are disabled**, with the
tooltip "Select a worktree to edit prompts", because with no worktree the centre
draws no tabs.

## §6 What stays exactly as it is

- Plain file tabs: explicit save, the dirty-close prompt, Preview for `.md`.
- The other six right-panel views, their order and their socket names.
- `PanelView` stays an app-wide, non-persisted global, so Files is still the
  default view at launch.
- `TabKind`, `persisted_kind` and the session schema. A prompt tab persists as a
  `"file"` tab with its `editor_path`.

## §7 Testing

End to end, against the real binary, following `test-project-identity-e2e.sh`.

- **A debug-only socket verb, `surface.prompts.read`:** it returns the cards
  (file, title, preview), the panel's view state, and the **bounds** of New
  prompt and of each card's Edit and Copy buttons. The test needs those bounds
  because no verb clicks by selector. Register it in `DEBUG_METHODS`
  (`sirio_control/src/served.rs`); the debug-only serving test covers it.
- **`Scripts/Tests/test-prompts-e2e.sh`:**
  - Setup: `--xvfb`, a private `HOME` and XDG dirs, `SIRIO_DB`, and a **mandatory
    `SIRIO_PROMPTS_DIR`**.
  - It drives the app with real X clicks at the reported bounds and with typed
    keys.
  - It prints `PROMPTS E2E OK`.
  - It leaves its run directory as the artifact: the socket transcript, the
    prompts folder, and PID-matched window captures in light and dark.

**Scenarios:**

1. The empty state shows.
2. New prompt, then type a title and a multi-line body. Without pressing Ctrl+S,
   the file bytes equal what was typed and the index holds the title. The card
   shows the title and its first two lines.
3. Copy: the X CLIPBOARD (read as in the project-identity test) equals the file
   bytes, and the button shows the check.
4. Type into the open tab and copy immediately. The clipboard has the last
   keystroke.
5. Edit an existing prompt. The pencil selects the already-open tab instead of
   opening a second one.
6. New prompt, then close the tab untouched. No file and no index entry remain.
7. Delete through the context menu and the confirm dialog, with the prompt's tab
   open. The file and its entry are gone, the tab is closed, and nothing
   reappears after a pause longer than the autosave delay.
8. A plain `.md` file tab in a worktree still opens in Preview and still needs
   Ctrl+S. This guards the autosave anti-goal.
9. No worktree: the list and Copy work, and New and Edit are disabled.

**Unit tests:** only for the store's reconciliation (§4 rules 1 to 4, atomic
write). Following the repo rule, the list of ways it can fail is written before
the code. Nothing tautological gets added.

`docs/agents/e2e-tests.md` gains the new script's entry.

## §8 Delivery

- One PR, `feat: save and copy prompts from the right panel`.
- The version stays at **0.39.0**: #703 was the cycle's first `feat:` and already
  moved it.
- `Scripts/ci.sh` runs only when the user asks for it.

## §9 Calls made in the brainstorm, for the user to check

1. **The title lives in `index.json`, not in the file.** That keeps the file
   exactly what gets copied. Filenames are generated and opaque, so a file opened
   in another program is identified by its contents, not its name.
2. **The editor opens in the Secondary pane**, beside the terminal, as every file
   tab does. The mockup drew it as a tab in the terminal's strip.
3. **"Works without a worktree"** means list, Copy and Delete. Edit and New need a
   worktree, because the centre draws no tabs without one.
4. **Copy feedback** is the button turning into a check for 2 s. The mockup's
   green toast is dropped.
5. **No folder watcher.** Edits made outside Sirio show up the next time the view
   is shown.
6. **The folder location** per OS (§4). A debug build uses its own folder
   beside its checkout database, so dev builds never see or touch your real
   prompts. Only release builds, like the installed app, use the real folder.
7. **UI strings are in English** like the rest of Sirio: Prompts, New prompt,
   Edit, Copy, Delete…, Untitled prompt, No prompts yet.
8. **Pressing New and closing the tab untouched leaves nothing behind.**
9. **A deleted prompt never comes back**, even if a tab for it was open in
   another worktree. That tab shows the missing-file state instead.
10. **An edit made in another program while the prompt is open in Sirio: the
    last write wins.** A tab with nothing unsaved takes the new text. A tab
    where you are typing keeps your text and overwrites the file. Sirio never
    asks and never discards what you typed.
11. **A failed Copy, New prompt or Delete is reported** in the workspace's
    existing toast (§2, "Failures"). Chosen by Enzo on 2026-11-02 over staying
    silent.
