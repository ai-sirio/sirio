# Text you can select and copy (`sirio_ui::text_selection`, `sirio_ui::markdown`)

gpui paints a `&str`/`String`/`SharedString` child and does nothing else with it —
no hitbox, no mouse handlers — and bezel has no selectable text, so **text drawn as a
plain child cannot be selected**. Only the chat transcript, the file editor and the
terminal had their own selection; everything else (Settings, change requests, banners,
dialogs, the diff) was inert. Use `selectable_text(..)` where a string is shown to be
read: it lays out exactly like the string it replaces, takes a drag, a double-click (one
segment of a path or branch: `/` and `\` separate, `-` `.` `_` do not) and a triple-click
(the whole line), and Ctrl/Cmd+C copies it. One
selection exists at a time, in a `Global`; selecting moves focus into a sink (the
workspace root's `root_focus`) so a terminal or composer is never handed a copy.

- **Never inside something clickable** — a row, a button, a tab, a palette entry: a
  press there fires the click on release even after a drag, and the I-beam would replace
  the pointer. The titlebar's drag region is out too.
- **Selection is per run of text.** A drag does not continue into the next label; the
  chat transcript alone has a document-wide offset space. In a loop, give each item an
  identity with `.id(..)` (the default is the call site), or two identical strings
  select together.
- **Ctrl+C must never be swallowed.** gpui dispatches a bound action *before* any raw
  `on_key_down` and stops there unless the handler propagates, so `on_copy` declines
  (`cx.propagate()`) whenever it has nothing to copy or the sink is not focused, and the
  binding is scoped `!Terminal`. A terminal's Ctrl+C stays SIGINT.
- **Focus taken at the press does not hold inside a pane.** A pane's own press handler
  (`select_pane`) runs after its content's, ignores `prevent_default`, and hands focus to
  the pane's surface, which for a chat is the composer. The chat transcript
  (`TranscriptInteraction::extend_selection`) and `text_selection::extend` therefore take
  focus on the drag, once something is selected: a plain click leaves the composer
  focused, and a drag leaves Ctrl/Cmd+C to the selection.
- Rendered Markdown is drawn by `ely-documents` and selected by Sirio, never by Ely's own
  selection (it takes focus on a press and keeps Ctrl+C in its key handler). Every body goes
  through `sirio_ui::markdown::markdown_view`, which installs a host `TextGather` (vendored
  change L1): each drawn text reaches the gather in push order with its layout and link
  ranges, and the gather returns the selection's highlight. `SelectionHost` picks the owner:
  `Run` is the app-wide selection of `text_selection` (the Markdown Preview and a change
  request's bodies, through `change_request_tab::forge_body`), `Transcript` is the chat's one
  offset space (a reply starts at its `source_start` in the transcript's plain text), `None`
  draws without selection (the subagent card). Selection, Ctrl+C and the right-click menu
  (Copy, Select all, Open link, Copy link address) stay Sirio's. A link opens only when
  nothing was selected or dragged (`ClickGate`). Text Ely draws without pushing to the gather
  (math, an alert's title, a footnote's label) cannot be hit or highlighted, but Select All
  and a selection spanning it copy it (`markdown/text.rs`, the plain text of a tree in push
  order). The chat's own cards use the transcript selection (`Chat::render_plain_text`) so
  Select All highlights what it copies.
- The host wires three things: `text_selection::init`, `set_sink(root_focus)`, and
  `.on_action(text_selection::on_copy)` on the root of **each** render branch (Settings
  and the main shell). `text_selected_in_settings_is_copied_by_the_real_shell` and
  `text_selected_in_the_main_shell_…` in `sirio` fail if any of them is missing.
- Tests: host the view under `text_selection::testing::host`, then `copy_line` /
  `copy_span` — a real drag and a real Ctrl+C against the real clipboard.
