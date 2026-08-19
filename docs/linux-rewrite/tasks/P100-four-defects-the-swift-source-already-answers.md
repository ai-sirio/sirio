# P100 — Four defects the Swift source already answers

Four rows the 2026-08-19 critic passes moved to `FAILED — defective`. I read the original for each
one before briefing it, because two of the four turned out to have a *different* fix than the
critic's report implies, and one of the two is much smaller than it looked.

Every anchor below was read on 2026-08-19 at `d04d79b4`. Re-read before editing; line numbers move.

---

## 1. `F-TERM-08` — closing a terminal never asks

**Reproduced live:** a plain `sleep 300` running in a non-agent terminal, right-click → "Close
Terminal…", and the pane closes instantly with no prompt at all.

**The port's cause,** `rust/crates/tiller/src/main.rs` `request_close_terminal_at` (~4817): the
banner is gated on `pane_close_needs_confirmation(status)`, and `status` comes from
`pane_activity_status` — the *agent-recognition* status (`AgentCatalog` Layers A–D). An ordinary
shell command is not a recognised agent CLI, so it can never reach `Running`/`NeedsInput`/`Error`
and the gate is permanently shut for ordinary terminal use.

**What the original actually does — and this is the part the report did not check.** The gate does
not exist there. `ActivityStatus.requiresCloseConfirmation`
(`Packages/TillerCore/Sources/TillerCore/ActivityStatus.swift:26`) has exactly **one** non-test call
site in the whole app: `App/RightPanel/ActivitySectionView.swift:112`, the *Activity panel's* close
button. The terminal close paths do not consult it:

- `App/TerminalContextMenuProvider.swift:58` `showCloseConfirmAlert(paneId:)` — builds an `NSAlert`
  **unconditionally**, "Close Terminal" / *"Termina il processo in esecuzione in questo pane."* /
  Cancel + destructive Close, and closes only on the second button.
- `App/SidebarView.swift:678` — `.alert("Close terminal?", isPresented: $confirmingClose)` /
  *"The running process will be terminated."* / Cancel + destructive Close. The menu item at line
  720 just sets `confirmingClose = true`; there is no condition anywhere on that path.

So the contract is **closing a terminal pane always asks**, and the port turned an unconditional
confirmation into a conditional one. The status gate is right where the Activity panel uses it and
wrong here.

**The fix is therefore small:** the terminal close path holds the close unconditionally. Keep
`pane_close_needs_confirmation` for the Activity-row path (`whole_tab`), where it matches the
original. The captured `status` stays useful — the banner already words itself from it — but it must
not decide *whether* to appear.

Do not lose `F-TAB-26` in the process: `close_terminal_at` refuses to remove a tab's last leaf, so a
single-pane tab's "Close Anyway" has to close the whole tab. That is what `whole_tab` is for and it
is already correct.

---

## 2. `F-TERM-05` — "Set Title" has nowhere to type

`set_terminal_title` (`rust/crates/tiller/src/main.rs:4241`) sets `tab.title = format!("Terminal
{terminal_id}")` and says so itself: *"The Linux context event carries identity, not text. Use that
stable identity as the app-owned title until a text-entry prompt is added."* Clicking it renames the
tab to `Terminal terminal-0` and the user's subsequent typing lands in the shell.

**The original,** `App/TerminalContextMenuProvider.swift:76` `showSetTitleAlert(paneId:)`:

| element | value |
|---|---|
| title | `Set Title` |
| body | `Enter the new title for "<current tab title>":` |
| accessory | `NSTextField` **pre-filled with the current title**, 240×22 |
| buttons | `OK` (first, default) then `Cancel` |
| on OK | `renameTab(tab.id, in: worktree.id, to: input.stringValue)` |

**This is the expensive one, and not because of the rename.** There is no text-entry prompt anywhere
in the port — `grep` for `rename_tab|pending_rename|RenamePrompt` returns nothing. This row needs a
modal text-input primitive built first. Build it as one, in `tiller_ui`, with its own tests: tab
rename, worktree rename and project rename all want it, and three ad-hoc versions is the outcome to
avoid. Pre-filled value, Enter commits, Escape cancels, empty input is a no-op rather than a blank
title.

---

## 3. `F-CORE-DOM-06` — an out-of-range Ctrl-number jumps to the last tab

With 4 tabs open, an isolated `ctrl-7` moves the selection to tab 4. `ctrl-1`…`ctrl-4` and `ctrl-9`
are all correct.

**Two implementations of one feature, and the correct one is dead:**

- Live: `TabSelection::jump` (`rust/crates/tiller/src/panes.rs:271`), called from `main.rs:7754` —
  `active: position.saturating_sub(1).min(self.tab_count - 1)`. Its own doc comment explains the
  bug: *"Positions beyond the group select the last tab, matching the documented Ctrl-9 behavior."*
  Ctrl-9's "last tab" is a **special case for 9**, not a rule for every out-of-range number, and
  generalising it is what produces the clamp.
- Dead: `tiller_project::domain::numeric_tab_selection` (`domain.rs:64`) — returns the last index
  for `9`, `None` for anything out of range, and is unit-tested. `tiller_project` is already a
  dependency of `tiller` and the function is already exported from its `lib.rs:53`.

**Fix: delete `jump` and call `numeric_tab_selection` at the one live site**, leaving the selection
untouched on `None`. Deleting the duplicate is the point — a second implementation that drifts is
exactly how this row broke.

Note the existing test `jumping_to_a_tab_uses_one_based_positions_and_clamps_to_the_last_tab`
(`panes.rs:801`) asserts `jump(1)`, `jump(5)` and `jump(9)` on a 5-tab selection. None of those is
out of range, so the test never covered the defect and it is not asserting the wrong behaviour — it
is simply silent about it. Whatever replaces it must cover `jump(7)` on 4 tabs.

---

## 4. `F-CORE-FILE-01` and `-08` — the file tree sorts and draws like a placeholder

**Sort** (`rust/crates/tiller_project/src/file.rs:217`): the comparator is
`(!is_directory, name.to_lowercase(), name)`. Dirs-before-files and case-insensitivity are right;
what is missing is everything else. `file10.txt` renders before `file2.txt`, reproduced live.
`Packages/TillerCore/Sources/TillerCore/FileTree.swift:82` calls Swift's
`localizedStandardCompare`, which is natural-numeric *and* diacritic-folding.

The natural-numeric half is the reproduced defect and is a pure function — write it in
`tiller_project::domain` or beside `read_directory`, with tests, no new dependency. The
diacritic-folding half (`é-note.md` sorting after every ASCII name by raw codepoint) was read from
the comparator rather than driven, so treat it as the second half of the same row and say which half
your evidence covers. There is **no** unicode crate in the workspace today
(`unicode-normalization`, `icu`, `caseless`, `deunicode` — none present), so folding means either
adding one deliberately or writing a bounded Latin-1 fold and documenting the bound. Do not silently
half-fold.

**Icons** (`rust/crates/tiller_ui/src/right_panel.rs`, `file_glyph()`): 3 of ~40 file mappings and
0 of 15 folder mappings. Everything else falls through to one shared `Icon::File` /
`Icon::FolderFill`. The original's key set is
`Packages/TillerCore/Sources/TillerCore/FileIconKey.swift` — read it and port the table, don't
invent one.

**Source the glyphs from `rust/assets/icons/comet/`.** That directory's provenance was verified on
2026-08-19 (`8b4ea9b7`): the user asked for it by URL on 13 Aug, quoted and dated in
`comet/ATTRIBUTION.md`. The standing "inspiration only, never code" rule does not apply to it. What
does still apply is P76's rule in `icons.rs`: comet is a **replacement**, not an addition — the 22
Phosphor "thin" originals and comet's rounded set do not mix, so a new mapping must not reintroduce
Phosphor glyphs beside comet ones. Where comet has no equivalent shape, say so in the row rather
than mixing families.
