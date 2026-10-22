# Ely integration (chat and editor)

Upstream: https://git.zacharyzhang.com/ZacharyZhang-NY/Ely-GPUI-Components
(formerly https://github.com/ZacharyZhang-NY/Ely-GPUI-Components; the
repository moved and its history was rewritten.)
Revision: `e17e31a6890c09ebcfa8b61133d7bc7c625edf69`
Upstream `main` no longer contains this revision: `e17e31a6` is not an
ancestor of `origin/main` in the upstream clone's existing refs (checked
2026-10-17 without fetching). Vendored sources therefore come from this
revision, not from upstream `main`.
Package: `ely-gpui-component` 0.1.0, Rust 1.95, MIT OR Apache-2.0.
The upstream MIT and Apache license texts accompany these sources.

## Supported build

The `sirio-chat` build includes chat/agent surfaces and the buttons, forms,
layout, menus, overlays, primitives, motion, theme, typography, avatars,
usage/progress and feedback components they need. Module export files select
this closure; other product stacks and Gallery examples are excluded.
The default feature selects this supported build for workspace commands.
GPUI/platform resolve through Sirio's workspace aliases and Linux-only
Wayland/X11 features. The upstream GPUI git revision, default-font loading,
terminal/webview/editor dependencies and Gallery runtime are excluded.

## Local source changes

- `src/lib.rs`: idempotent `init_chat` initializes Ely's distinct theme and
  scoped `ElyInput` text actions. It registers no global Tab binding and no
  document/terminal actions. `init_editor` (below) binds the editor's keys in
  the `ElyEditor` context only. Eager asset validation belongs to the
  native probe, allowing Sirio's existing asset-free GPUI test contexts.
- `src/assets.rs` and `src/primitives/icon.rs`: offline icons use the `ely/`
  namespace in both lookup/list and references. Icon geometry is unchanged;
  upstream Inter/JetBrains Mono font files are excluded.
- `src/compat.rs`: retains the image-source helper from
  `documents/blocks/media.rs` and code-color helper from `editor/syntax.rs`.
  Message/avatar and code components import these helpers without pulling in
  either application stack.
  This file also retains `Fit/fuzzy/marked` from
  `navigation/palette/mod.rs` for Ely's searchable menus; the independent
  command palette/launcher is excluded.
- `overlays/dialog.rs`: the card (both the centred and the fullscreen one) sets
  `text_color(colors.fg)`, because a dialog is raised above its owner and
  otherwise inherited the owner's text colour (in Sirio gpui's default black,
  unreadable on a dark card).
- `layout/scroll.rs`, `overlays/hover.rs`, `primitives/tooltip.rs`: the pinned
  Bezel GPUI has `on_hover` but no newer `HoverListenerMode` policy setter.
  These components use the fork's existing hover dispatch, like Bezel's
  surfaces, rather than changing GPUI. Pointer behavior is checked natively;
  no independent-input-modality hover policy is claimed.
- `src/chat/stream.rs`: retains upstream text/cursor behavior and its pure
  regression tests. `StreamingMarkdown` is excluded because Sirio retains its
  parsed selectable Markdown renderer.
- Module-wide upstream visual suites that import excluded components are
  excluded at their module boundary. Inline upstream tests in the retained
  sources remain part of this crate's tests.

Compatibility adjustments and controlled component slots are recorded here
as they are implemented. Native rendering/input evidence is collected by
`sirio_ui/examples/ely_chat_probe.rs`; compile checks alone do not prove it.

- `chat/message.rs`: `MessageAvatar::content` accepts the host's white agent
  mark and dark plate, before Ely's default role avatar.
- `theme/{mod,tokens}.rs`: optional `ThemeMetrics` supplies Sirio's eight text
  sizes and four radii in pixels; a single px-to-rem conversion preserves the
  existing API and avoids applying font scale twice.

Linux compatibility evidence: the native probe rendered under an isolated
Xvfb X11 display (Mesa software rendering), edited/copied/pasted multiline
text, submitted via Enter, and expanded the tool card via pointer. The
physical Wayland session and native macOS/Windows input are unverified.

- `feedback/messages.rs`: `Alert::content` accepts Sirio's selectable error
  guidance and host actions; the original text-body API remains supported.
- Sirio uses Ely's `ChatContainer` with its existing `ListState`, not Ely's
  private `MessageList` feed. Row IDs are scoped to Chat entity, transcript
  generation and row index; streamed chunks retain identity.

- `src/expansion.rs`, `agent/tools.rs`, `chat/status.rs`: optional host-owned
  expansion bypasses keyed widget state; custom rich bodies and actual header
  selectors preserve virtualized interaction. Tool groups accept reported
  status, and thoughts accept an honest label when no duration was measured.
  The default Gallery constructors retain their original uncontrolled behavior.
  `ThinkingIndicator::glyph` and `ThinkingBlock::thinking_glyph` take a
  host-owned glyph in place of the sparkles, so Sirio keeps its Cluster orb;
  without one the indicator draws its original icon.
- `chat/composer.rs`: the stop control takes the same variant as send instead
  of `Secondary`; on the custom shell's fill a Secondary button's fill and
  border all but match the card, leaving only a thin outlined square.
  `SendButton::variant` picks that variant (`Primary` by default); the custom
  shell draws both as `Subtle`, the theme's hover fill, because a solid accent
  square was the one off-palette mark on the card.

- `agent/permission.rs`, `agent/control.rs`: custom request constructors plus
  rich body/action slots render only protocol-supplied controls. Human custom
  mode has no Ely editor or default Reply handler; default Gallery
  constructors preserve their built-in choices and editor behavior.
  `header_selector` puts a debug selector on the real header text, and the
  built-in "Always allow" button sits in a wrapper carrying the
  `ely-request-always` selector, so a test can prove a protocol request never
  grows Ely's fixed Once/Always/Deny set. GPUI's `debug_selector` is a no-op in
  release builds, so both are test hooks only there.

- `chat/composer.rs`: `PromptInput::custom` draws Ely's shell around the
  host's own editor, with host-supplied send eligibility (`ready`). It takes no `forms::Enter` action and offers no completion
  rows, so the host's keyboard, IME and popups stay in charge; the default
  constructor keeps its upstream behaviour. In custom mode the shell wears
  the theme's `surface`, with the host's field painting no fill of its own
  (Sirio's vendored bezel-ui `TextField::with_transparent_fill`), so the two
  read as one card in the theme's card colour; its border is `border` in both
  states, never the ring colour or a brighter step, and the tool row wraps. `.trailing` puts tools at the end of the row, next
  to the send control. `SendButton` carries `send`, `send-ready` and
  `stop-glyph` debug selectors, and `PromptInput`'s card `ely-prompt-input`
  (no-ops in release builds); the card sets a text colour so content without
  one of its own is not drawn in gpui's default black. `InputHint::new().enter(..)`
  names what Enter does now, such as `to queue ·` while an answer streams;
  `InputHint` is no longer a unit struct.
- `chat/attach.rs`: `AttachmentChip::remove_selector` puts a debug selector on
  the remove button.

- `menus/{model,draw,panel}.rs`: `MenuPanel` draws a menu's rows and surface
  exactly as the stateful hosts do, for a host that owns whether the menu is
  open, where it hangs, which row its keys mark and what closes it; a press runs
  the row's `on_click` and leaves the menu open for the host to close.
  `layer` + `Hang` float such a panel over its parent's top edge (left or right
  aligned) or at a window point, kept inside the window, occluding what is under
  it, fading in. `panel_surface` is the card itself, for content that is not
  rows. `MenuItem` gains `note` (a quiet tag at the row's end), `tooltip` (the
  full label of a cut one) and `selectors` (debug selectors on the row and its
  note, no-ops in release builds); `Menu::step` and `Menu::run` are public so a
  host can walk and press rows itself. The stateful hosts (`DropdownMenu`,
  `ContextMenu`, ...) are untouched.

- `src/theme/palette.rs` and `src/theme/syntax.rs` moved, with `Mode`, into
  the sibling crate `rust/vendor/ely-palette`, which this crate re-exports at
  the original paths: `theme::{Palette, Syntax, Mode, …}` are the same types.
  Sirio's theme crate holds Ely's palette as its colour vocabulary without
  compiling Ely's components. `Palette` and `Syntax` derive `Copy` (all their
  fields are `Hsla`), so Sirio's `Theme` stays `Copy`. The split also widens
  `color_blind_chart` from `pub(crate)` to `pub` in
  `rust/vendor/ely-palette/src/palette.rs`.
- The upstream `.clone()` calls were replaced with copies in
  `rust/vendor/ely-palette/src/syntax.rs` and these files under
  `rust/vendor/ely-gpui-component/src/`: `agent/control.rs`,
  `agent/permission.rs`, `agent/progress.rs`, `agent/tools.rs`, `chat/attach.rs`,
  `chat/cite.rs`, `chat/code.rs`, `chat/composer.rs`, `chat/conversations.rs`,
  `chat/list.rs`, `chat/message.rs`, `chat/search.rs`, `chat/status.rs`,
  `menus/panel.rs`, `theme/mod.rs` and `theme/tests.rs`.

## Final compatibility evidence

Seen on Linux x86_64 (Arch, kernel 7.2.3) with rustc 1.98.1, cargo 1.98.1 and
Zig 0.15.2; `docs/testing/ely-agent-chat.md` has the commands, the runner and
what each check does and does not prove.

- **One GPUI.** `cargo metadata --locked` resolves 30 `gpui`/`zed` packages, none
  with more than one version. This crate depends on `bezel-gpui =0.3.8` and
  `bezel-gpui-platform =0.3.8`, the packages Sirio pins.
- **This crate:** `cargo nextest run -p ely-gpui-component --features test-support`,
  137 passed with the editor (78 before it; the editor sections below quote the
  count of their own task). The test target compiles only with
  `--features test-support`; see the Editor section. Its warnings, in the parts
  of Ely this chat does not use, are upstream's subset, not suppressed.
- **The chat on top of it:** 205 chat tests, the real-app runner (11 scenarios,
  each on an isolated instance), the five agent marks in dark and light.
- **Platforms:** Linux X11 only (isolated Xvfb, software Mesa). macOS, Windows
  and a physical Wayland session were not available and are **unverified**; a
  cross-target source check was not obtained (no macOS SDK for a dependency's C
  build). Ely's own macOS Gallery says nothing about Sirio's other platforms.
- **Not established:** a real IME, and rewind natively.

Every deviation from upstream is listed above, with the file it changes.

## Change-request surfaces (2026-10-03)

Vendored from the same revision for Sirio's change-request surfaces and its
Changes view (`docs/superpowers/specs/2026-10-03-change-requests-on-ely-design.md` §4):

- `src/navigation/tabs.rs` — verbatim. `navigation/mod.rs` is local and
  declares only this module; the rest of upstream's `navigation/` is excluded.
- `src/git/badges.rs` — verbatim except one import: `GitStatus` comes from
  `super::status` instead of `crate::lists`.
- `src/git/status.rs` — upstream's `GitStatus` and its `impl`, verbatim from
  `src/lists/files.rs`, plus `use crate::data_display::Tone;`. Upstream's
  `lists/` (file tree, virtual lists, sortable lists) is excluded; only this
  type is needed. `git/mod.rs` is local and declares only `badges` and `status`.

Caveats a host must respect:

- `Tabs` panics when `selected` names no tab's `value`. Build `selected`
  from the same list passed as tabs.
- `Tabs`' arrow keys act only while its strip holds focus, and a click on a
  tab does not give it focus (`prevent_default` on mouse-down). This build
  registers no Tab traversal (see `init_chat`), so in Sirio the tabs are used
  with the pointer; keyboard use is not established.

Native rendering evidence: `sirio_ui/examples/ely_forge_probe.rs`, driven by
`Scripts/Tests/test-ely-forge-probe.sh` (`docs/testing/ely-forge-probe.md`).

### `Popover::on_close` (delivery 3, B2b)

`src/overlays/popover.rs` gains `Popover::on_close(handler)`, which runs each
time the panel closes, however it closed: its trigger, Escape, a press outside,
or focus leaving it. All four already went through the private `close`; the
hook is stored in the popover's keyed state on every render and called there
after focus is handed back, only when the panel was open. Upstream has no way
for an owner to learn the panel closed, and the change-request pickers send
their one write when it does (spec §6). `src/overlays/tests.rs` drives each
path in a real window
(`every_way_a_popover_closes_tells_its_owner_once`).

`Popover::open(bool)` lets the owner hold the panel open or closed; it
follows on the next draw, the usual ways out still close it, and every close
(the owner's included) runs `on_close`. The change-request pickers are opened
and closed over the control socket too (debug builds), and the panel must
follow the picker's own state rather than drift from it.
`a_popover_follows_its_owner_open_and_closed` proves both directions.

## Editor (project 1)

Sirio's file editor on Ely's `CodeEditor`, vendored from the same revision,
`e17e31a6`. Design: `docs/superpowers/specs/2026-10-17-ely-code-editor-design.md`.
Every file below is verbatim from `e17e31a6` unless its entry says otherwise;
the ports below change the files they name.

Vendored files:

- `src/editor/{problems,search,filters}.rs` (editor panels, project 2):
  verbatim from `e17e31a6` (`filters.rs` then takes the `b29d66c` hunk, see
  that port below), with their inline tests (`files_with_worse_problems_lead_and_filters_hide`,
  `globs_take_includes_then_drop_excludes` and the three of `search.rs`). They
  need nothing the crate lacked, so no closure file was added. `src/editor/
  console.rs` is deliberately not vendored: the programme dropped the Output
  panel. `mod.rs` re-exports `Problem`, `ProblemsPanel`, `FileHits`, `Hit`,
  `ReferencesPanel`, `SearchPanel`, `SearchResultItem`, `SearchFilters` and
  `passes`.
  Local change (Sirio's worktree search): `find.rs` gains `Finder`
  (`Finder::new(query, options)` compiles the pattern once, `find_all(text)`
  runs it) and `filters.rs` gains `GlobFilter` (`GlobFilter::new(include,
  exclude)`, `passes(path)`), each the old body of `find_all` and `passes`
  split in two; both functions stay and delegate, so the in-file find and
  `SearchFilters` behave as before. A search over many files compiles the
  query and the globs once per run instead of once per file, and a glob too big
  for a regex logs once. `mod.rs` also re-exports `Finder` and `GlobFilter`.
  Local change (the drawer's Search tab): `SearchPanel` gains `unsaved(paths)`
  (a file whose open tab holds unsaved text shows `unsaved` beside its count,
  and its hits draw no replacement preview, since Replace all skips it),
  `selected(path, line)` (that result's row is drawn with the `active` wash, the
  one a chosen conversation row takes, via a `selected` builder on
  `SearchResultItem`), `full_counts(..)` (a file given fewer lines than it matched on
  shows `3 of 412`, a folded one its whole count) and `without_summary()` (the owner
  draws a totals line of its own, with the skips and the cap, so the built-in
  one is left out). `results()` takes its per-list inputs in a `Drawing` struct
  instead of seven arguments; `ReferencesPanel` passes the empty values, so it
  draws as before. `SearchFilters` is not used by Sirio: its stacked labels and
  always-drawn ignore toggle cost more drawer height than the three-row mockup
  allows, so the drawer places two `Input`s itself.
  Local change (the drawer's Problems tab): `ProblemsPanel` gains `counts([errors,
  warnings, notes])`, the numbers its three toggles show. The panel builds a row
  for every problem it is given (no `uniform_list`), so the drawer hands it at
  most 1 500 and the toggles would otherwise count only those; without the call
  each toggle counts the problems given, as before.
- `src/editor/{buffer,cursor,edit,keys,moves,line,row,view,layout,gutter,decor,syntax,ime,find,status,state}.rs`:
  verbatim, except the local changes listed below, the ports in "Ports from
  upstream `main`" (which change `buffer`, `edit`, `ime`, `line`, `moves`,
  `state` and `view`) and the key bindings of `keys.rs` in "Keys and hosting
  hooks" below.
- `src/editor/tests/mod.rs` (was `tests.rs`; moved by the `b29d66c` port
  below): verbatim, gated `cfg(all(test, feature = "test-support"))`
  as upstream. Nothing was removed: none of its 15 tests names a module this
  crate does not declare.
- `src/editor/mod.rs`: local. It declares only the files above. Upstream's
  `mod.rs` also declares `chat`, `console`, `extensions`, `hints`,
  `hover`, `keybindings`, `preview`, `refactor`,
  `settings`, `symbols`, `tasks` and `welcome`, all excluded (`problems`,
  `search` and `filters` now come in, see the first entry above), and their
  re-exports with them. `anchors` and `measure` arrive from `main` in Task 1b.
  Task 1c adds `pub use keys::SingleCursor;` and deletes two dead re-exports;
  see "Keys and hosting hooks".
- `src/navigation/breadcrumb.rs`: verbatim. `src/navigation/mod.rs` declares
  it and re-exports `Breadcrumb` and `Crumb`.
- `src/shell/bars.rs`: only `StatusBarItem`, its `ClickHandler` alias and the
  imports it needs. `Toolbar`, `ToolbarGroup`, `ToolbarSeparator` and
  `StatusBar` are not vendored. `src/shell/mod.rs` is local: it declares `bars`
  and re-exports `StatusBarItem`.
- `src/compat.rs`: `tint`, copied verbatim from upstream `charts/paint.rs`,
  with the `Hsla` and `Palette` imports it needs. `code_colors` and its users
  are unchanged.
- `src/lib.rs`: declares `pub mod editor;` and `pub mod shell;`, and defines
  `init_editor` (Task 1c).

Local changes:

- `src/editor/row.rs`: the inline chat widget (`chat::InlineChat`) is not
  vendored, so `chat_row` draws only the gutter for `Row::Chat`. Its
  `InlineChat` and `CHAT_ROWS` imports are gone. Kept: `Row::Chat` and its
  layout rows, `marks.chat`, `chat_field`, `set_chat` and `chat_line`, the
  `Asked` and `ChatClosed` events, and Escape closing the chat in `keys.rs`.
  Consequences: `set_chat(Some(line))` still reserves three blank rows below
  the line and moves focus into `chat_field`, which nothing draws.
  `EditorEvent::Asked` is still emitted from `state.rs` when that field
  submits, but no visible control reaches it. A host must not call
  `set_chat(Some(..))` until a chat widget is vendored.
- `src/editor/line.rs`: `charts::tint` becomes `compat::tint`. Nothing else in
  this file changed by that vendoring; the ports in "Ports from upstream
  `main`" below change it further.
- No warnings from the editor: `init_editor` calls `keys::bind_keys`, and the
  two unused re-exports recorded in Task 1a are deleted in Task 1c. None come
  from `src/editor/`, `src/shell/` or `navigation/breadcrumb.rs`; the rest are
  upstream's chat-era items in other modules, not suppressed.

gpui adaptations: none. The closure compiled against the pinned Bezel GPUI
unchanged, so no hover, focus or scroll shim was needed.

Evidence: `cargo nextest run -p ely-gpui-component --features test-support`
ran 108 tests, all passed. Before this change the same command ran 78, all
passed. The 30 new are the editor's: 15 window tests in `editor::tests` and 15
inline tests in `buffer`, `cursor`, `layout`, `syntax` and `find`.

Without `--features test-support` the test target does not compile, at HEAD
as well: `overlays/mod.rs` gates its tests on `cfg(test)` alone, and they
import `gpui::TestAppContext`. That gate is outside this change and was left
as it is.

### Ports from upstream `main` (Task 1b)

Fixes the editor gained on upstream `main` after `e17e31a6`, one commit each,
in this order. `e17e31a6` is not on `main`, so each port records whether it
applied as upstream wrote it or was rewritten against the vendored code.

- `617a8d6` "Editor · FindWidget: Escape from the query closes the box":
  applied verbatim to `src/editor/find.rs` and `src/editor/tests.rs`; both
  files were identical to upstream's parent. Adds the window test
  `escape_in_the_find_query_closes_the_box`, which fails without the
  `find.rs` hunk.
- `b29d66c` "Panics · P17 Editor: owner offsets ride the text's edits, stale
  ranges, rows and globs log instead of panicking": applied verbatim to
  `src/editor/{anchors (new), edit, ime, line, moves, state, view}.rs`, whose
  upstream parents were identical to the vendored files (`line.rs` differs
  only by `compat::tint`, which the hunks do not touch). `anchors.rs` comes
  with its inline test `offsets_ride_their_edits`. `select` and `edit` moved
  from `state.rs` to `edit.rs` and log and ignore a bad range; `offset_in`
  returns `None` for a line the text has lost. `tests.rs` moved to
  `tests/mod.rs` (`mod stale;`, and `editor` made `pub(super)`).
  `src/editor/mod.rs` is local, so the port adds only `mod anchors;`.
  `tests/stale.rs` is verbatim except one test, dropped because it names
  code in a module this crate does not vendor:
  `an_output_channel_gone_from_its_list_draws` (`console::OutputPanel`).
  Skipped, not vendored: `src/editor/console.rs` (the output panel's shown
  channel) and `tasks/panics.md`; the output panel's panic is not ported.
  `src/editor/filters.rs` came in with the editor panels (project 2) and takes
  this commit's one hunk, applied with `patch -p1` from
  `git diff e17e31a6 b29d66c -- src/editor/filters.rs`: a glob whose generated
  regex fails to compile (a 500 000-byte one) logs through `inspect_err` and
  matches nothing, instead of `expect` panicking. Its test
  `a_glob_too_big_for_a_regex_matches_nothing` is restored in `tests/stale.rs`.
  Nothing else of the commit is new here. The eight stale-state tests pass.
- `9fb607b` "Code editor: columns sit where gpui lays them out": `ime.rs`,
  `measure.rs` (new) and `tests/wide.rs` (new) are verbatim. `state.rs`,
  `line.rs`, `view.rs`, `mod.rs` and `tests/mod.rs` did not apply:
  their context named upstream code from commits between `b29d66c` and this
  one that the vendored copy does not have (the typewriter, `Metrics::height`,
  `line.rs`'s imports, and the `preview`, `chinese` and `history` modules,
  which are not vendored). Their added lines were typed in by hand. Upstream's
  `secondary_press` hunk in `line.rs` had nothing to apply to: that function is
  absent on `e17e31a6`. Hand-applied pieces: `Metrics::size` (after `line`);
  `self.metrics.size = pixels` after `self.metrics.line = line`; in `line.rs`
  the `measure::Measured` import, the
  `styles` visibility becoming `pub(super)` (`ime.rs` calls it), the new
  `whitespace` (hidden marks, arrows and dots drawn as a `StyledText`), the
  `line_row` closures `x` (laid out) and `grid` (rulers and guides), selection
  width, caret width `wide`, the drag/press calls with `window`, and the
  `drag` signature. `mod.rs` gains `mod measure;`; `tests/mod.rs` gains
  `mod wide;`.
  Skipped, not vendored: `src/documents/document/tests/point.rs` and
  `src/documents/tree/tests.rs`.
  `tests/wide.rs` passes unchanged. `offset_in` keeps its `None` for a lost
  line and now takes `window` and `cx`.
- `cd61653` "Code editor: measure with the current text; the line end is a
  stop": `buffer.rs` (the text stamp, and equality that ignores it), `view.rs`
  (`Frame::stamp`), `measure.rs` (`column` picks the nearer of a glyph start
  and the line end) and `tests/wide.rs` (two new tests) are verbatim, applied
  from the upstream commit. `line.rs` is hand-applied: the `fresh` gate on the
  brackets, occurrences and matched pair, and the two comments, go onto
  `styles`, which here calls `syntax::colors` where upstream's `main` calls
  `line_colors`. Nothing else in `line.rs` changed.
  Skipped: none; the commit touches no unvendored file.

### Keys and hosting hooks (Task 1c)

- `src/lib.rs`: `init_editor(cx)` runs `init_chat`, then `editor::bind_keys`,
  behind its own guard: a private `EditorKeysBound` global, checked first, so a
  second call binds nothing. A host that ran `init_chat` first still gets the
  editor's keys, because `init_chat`'s guard (the theme) is separate and
  unchanged. `sirio_ui::ely::init` calls `init_editor` where it called
  `init_chat`.
- `src/editor/keys.rs`, non-macOS: add-cursor is `shift-alt-up` and
  `shift-alt-down`. `ctrl-alt-up` and `ctrl-alt-down` are Sirio's
  `FocusPaneAbove` and `FocusPaneBelow` (`rust/crates/sirio/src/panes.rs:108-109`).
  macOS keeps `cmd-alt-up` and `cmd-alt-down`.
- `src/editor/keys.rs`, `Copy` (`secondary-c`): when every selection is empty it
  calls `cx.propagate()` and does nothing else, so Sirio's `CopySelectedText`
  (`!Terminal`) gets the chord. With a selection the editor copies exactly as
  upstream does. Upstream's copy of the current line with no selection is
  dropped on purpose (spec §5.4; the plan's `copy_propagates`). `Cut`
  (`secondary-x`) is unchanged: with no selection upstream still cuts the
  current line, which is an edit, so the chord is not handed on.
- `src/editor/keys.rs`, Escape: `SingleCursor` stays bound to `escape` in
  `ElyEditor`, and it does not propagate. A host that needs Escape for its own
  surface while the editor has focus can `capture_action` `editor::SingleCursor`.
  Sirio does not capture it yet.
- `src/editor/mod.rs`: `pub use keys::SingleCursor;` (new). The dead re-exports
  `pub(crate) use layout::stack;` and `pub(crate) use syntax::colors as
  code_colors;` are deleted: nothing in the vendored closure used them. `line.rs`
  imports `stack` from `layout` directly, and the chat keeps its own
  `compat::code_colors`, which is unchanged.

Evidence: `cargo nextest run -p ely-gpui-component --features test-support`
runs 120 tests after Task 1c, all passed.

### Hosting hooks (Task 2)

- `src/editor/state.rs`, highlighter: `LineHighlighter(Rc<Paint>)` with a
  manual `Debug`, held in `Options::highlighter`. `CodeEditor::highlighter`
  (builder) and `set_highlighter` take `Fn(usize, &str, &App) ->
  Vec<(Range<usize>, HighlightStyle)>`. Modelled on upstream `main`'s `2b6f551`
  (`prose.rs` `line_colors`), but the signature differs: the line index comes
  first and the spans are `HighlightStyle`, not `forms::Highlight`. `prose.rs`
  and `forms::Highlight` are not vendored.
- The contract the host sees: the highlighter is called with a line's index and
  the line's whole text, whatever part of the line is shown, and its spans are
  byte offsets into that text. A span past the text is clipped; a span that cuts
  a character is dropped; either is logged.
- `state.rs`, `piece_colors(line, from, piece, cx)`: the one private helper every
  colour of the code goes through. `piece` is the part of `line` that starts
  `from` bytes in. Without a highlighter it is `syntax::colors(piece)`, which is
  upstream's call on the same text, so upstream's behaviour is unchanged. With
  one, the highlighter gets the whole line and the spans are cut to the piece.
- The helper replaces `syntax::colors` at `line.rs` (`styles`, on the row's
  text), `view.rs` (sticky headers and the minimap, on whole lines) and, a
  fourth site, `row.rs` (the ghost row's tail, `from` the ghost's offset).
  The ruling named three; upstream `main` routes the ghost tail through its
  line colours as well (`row.rs:135`).
- `line.rs`, bracket loop (deviation from upstream): with a highlighter set, the
  pass no longer recolours a bracket with `syntax.punctuation`, so the host's
  colour stands. Rainbow tints still apply when `rainbow_brackets` is on, and
  bracket-match styling (background and bold) is unchanged. Upstream `main` has
  the unconditional loop. Known limitation: bracket positions come from Ely's
  bracket finder (`syntax::brackets`, over `forms::code::lex`), so a bracket
  inside a block comment, or after a Rust lifetime, may still be tinted when
  rainbow is on.
- `state.rs`, `ViewOptions` (`Clone, Debug, PartialEq`), `set_view_options` and
  `view_options`: they map onto the existing `Options` fields. A tab size of
  zero is raised to one and logged (the `tab_size` builder still asserts).
  `editor/mod.rs` exports `ViewOptions`.
- `state.rs`, `without_breakpoints` clears `Options::breakpoints`. In
  `gutter.rs` the breakpoint slot keeps its width and still draws the severity
  icon, but draws no breakpoint dot, takes no press and has no pointer cursor.
- `state.rs`, `reset(text, cx)`: ported from upstream `main`'s `prose.rs`
  (~line 38), placed in `state.rs` as the ruling says. New buffer, caret at 0,
  history cleared, folds cleared, `marked` and `composing` cleared, marks
  cleared by `anchors::clear`, `EditorEvent::Changed`, blink restarted.
- `EditorEvent` gains `Pointer { offset, position }` and `SecondaryClick
  { offset, position }`, both emitted from one handler on the list's `body`
  (`view.rs`, render), so a move or a press emits once. The offset comes from
  `offset_over_code` (`view.rs`), which is `None` left of the code column (the
  gutter, the breakpoint slot and the git bar), and otherwise `offset_at`, which
  is `None` past the last row and on a row that is not text (a lens, a removed
  line, ghost text or the chat). `offset_at` itself is unchanged; IME uses it.
  `Pointer` fires only without a button down.
  The `body` also takes an id (`code-editor-body`) and an `on_hover`: when the
  pointer leaves it, `Pointer { offset: None, position }` is emitted once, so an
  owner that drew something for the place the pointer last rested (Sirio's hover
  card) drops it when the pointer slides onto a neighbouring surface instead of
  patching each surface.
- `EditorEvent::PlatformClick { offset }`: `press()` records
  `pressed: Option<Press>` (`Press` in `state.rs`: the offset, whether the
  platform modifier was held, and `single`, which is `click_count == 1` with
  neither Shift nor Alt). `release()` (`line.rs`, from the root's
  `on_mouse_up(Left)`) emits only for a `single` press with neither Shift nor
  Alt on the release, released at the same offset (through `offset_over_code`),
  with the platform modifier held on press or release. It then collapses the
  selection to that caret. A press under the modifier still selects by drag, as
  before; only the click collapses. Double and triple clicks, and shift- or
  alt-extended presses, keep Ely's selection. The gesture is the one
  `file_view.rs` uses for go-to-definition (~2969, ~3034); it has no click-count
  check there, and it gets one here. `on_mouse_up_out` clears `pressed`.
- `EditorEvent::GitMarkPressed { line }`: in `gutter.rs` the git bar's hit area
  is 4px wide (the 2px bar sits inside it, at the same x) and takes a press only
  on a line where a mark is drawn. That press calls `prevent_default`.
- `EditorEvent` drops `Eq`: `Point<Pixels>` is not `Eq`. Nothing outside the
  editor matched on it; `sirio_ui` does not use the editor yet.
- `keys.rs`, `Copy`: the comment says the chord is handed on, never swallowed,
  and that `text_selection::on_copy` declines while the editor has focus.
  Behaviour is unchanged.
- `sirio_ui/examples/ely_editor_probe.rs` and
  `Scripts/Tests/test-ely-editor-probe.sh` prove the highlighter's colour on
  line 3, that the lexer's keyword colour is kept out and the bracket run on
  line 4 is drawn differently from the lexer's own run (with a `lexer-on`
  control run), the gutter's geometry, the pointer's offset over the code and
  over the git bar, `GitMarkPressed`, the breakpoint slot (with a `breakpoints-on`
  control run), `SecondaryClick`, that a plain click emits no `PlatformClick`,
  and that a Super-held click does. Not proven by the probe: `reset`, the effect
  of the view options on the frame, a `None` pointer offset past the last row,
  the `Pointer` offset on a lens or ghost row, and the ghost tail and the
  clipping of out-of-range spans.

Evidence: `cargo nextest run -p ely-gpui-component --features test-support`
runs 120 tests, all passed (no unit test added; the probe is the proof).
`Scripts/Tests/test-ely-editor-probe.sh --xvfb` prints `ELY EDITOR PROBE OK`.

### File tab hosting (Task 6b1)

- `src/editor/row.rs`: a `Row::Line(line)` row carries the debug selector
  `code-editor-line-{line}` (a no-op in release builds), so a test can find the
  row of a buffer line. Nothing else in the row changes.
- `src/editor/moves.rs`: `CodeEditor::reveal_primary(cx)` scrolls the primary
  cursor's row to the centre and notifies, beside the crate-private `reveal`.
  `select` scrolls only when it changes the selection, so a host that places
  the caret where it already is (the start of a line it was asked to show) calls
  this after `select`. Sirio's `file_view` does exactly that.

### Carriage returns and silent resets (Task 6 fix round 1)

- `src/editor/edit.rs`: `lf_text` reads each `\r\n` and each lone `\r` as `\n`,
  the buffer's only line break; it borrows when there is nothing to change.
  `CodeEditor::paste` applies it before it splits the text into lines, so a
  pasted CRLF is one ordinary undo step with the caret after the pasted text.
- `src/editor/ime.rs`: `replace_text_in_range` and `replace_and_mark_text_in_range`
  apply `lf_text` to the committed text before it becomes an edit (the
  composition path), so text entered through an input method carries no `\r`
  either.
- `src/editor/state.rs`: `CodeEditor::reset_keeping_view(text, cx)`. It is
  `reset` for a change the owner makes on its own (a reload, a disk adoption):
  the new buffer, history and folds cleared, the marks and blink as `reset`
  leaves them, `Changed` emitted. The cursors keep their places, each end
  clamped to the new text and floored to a character boundary, and cursors
  that now share an offset are joined. The scroll handle is not touched, so
  the view does not move. `reset` stays for a host that wants a caret at 0.
- `src/editor/tests/mod.rs`: `a_crlf_paste_lands_as_lf_with_the_caret_after_it`,
  `a_composed_text_with_a_crlf_lands_as_lf`,
  `a_silent_reset_keeps_its_cursors_clamped_and_its_history_empty`,
  `a_silent_reset_moves_a_cursor_inside_a_character_back_onto_it`.
  `cargo nextest run -p ely-gpui-component --features test-support` runs 124 tests, all passed.

### Rulers and indent guides drawn (Task 6d fix 1)

- `src/editor/line.rs`: a ruler and an indent guide are a one-pixel fill
  (`w(px(1.))` with `bg`), not a zero-width box with a left border. Sirio's
  renderer (bezel-gpui-wgpu) paints nothing for a box of no width, so the
  upstream form never showed on screen.
  The colours are unchanged: the ruler and the plain guide use `border` at 60%
  opacity, the guide of the active block `border_strong`.

### Selecting without a scroll (Task 7a)

- `src/editor/edit.rs`: `CodeEditor::select` keeps its behaviour, but its filter
  and its "changes nothing" check moved into a private `chosen`, which returns
  `None` when the ranges name no whole character or leave the selection as it
  is. `select` sets what `chosen` returns and centres the primary row, as before.
  This is a change of shape in a file upstream also edits: a sync must keep
  `chosen` and the scroll together.
- `src/editor/edit.rs`: `CodeEditor::select_in_place(ranges, cx)` sets the same
  selections through the same `chosen` and notifies, but does not scroll. Sirio's
  file tab uses it when a right press places the caret under the pointer, so the
  view stays where it was.

### Reading the diagnostics back (Task 7b)

- `src/editor/state.rs`: `CodeEditor::diagnostics(&self) -> &[Diagnostic]`, the
  findings the owner set, as Ely holds them now. An edit moves their ranges
  (`anchors::shift`), and an undo, `reset`, `reset_keeping_view` and `set_text`
  clear them. Sirio's hover card reads them from here, so the card and the
  squiggles agree after an edit, an undo or a reload. Until the next publish
  after a clear, the card shows no diagnostic either.

### A read-only editor without its banner (Task 10b)

- `src/editor/state.rs`: `CodeEditor::without_read_only_banner()` sets the new
  `Options::read_only_banner` (default `true`) to false. A read-only editor
  built with it draws no banner and so offers no "Make editable"; its owner says
  why the text is read-only, in a banner of its own (`ReadOnlyBanner` is public).
  Sirio's file tab uses it for a snapshot: a revision has nothing to unlock.
- `src/editor/state.rs`: `CodeEditor::draws_read_only_banner()` is the one
  answer to "does this editor draw its own banner": `read_only` and
  `read_only_banner` both set. The render and `surface.editor.state`
  (`ely_banner=`) both ask it.
- `src/editor/view.rs`: the banner is drawn when `draws_read_only_banner()` is
  true. The text, the button and the `Unlock` event are unchanged. A sync must
  keep that call in the render.

### Indenting with tabs (Task 11b)

- `src/editor/state.rs`: `Options::hard_tabs` (default `false`), with the builder
  `CodeEditor::hard_tabs()`, the setter `set_hard_tabs(bool, cx)` and the reader
  `uses_hard_tabs()`. It is a separate setter, not a `ViewOptions` field, so no
  literal of `ViewOptions` changes. The text already in the buffer is left as it is.
- `src/editor/edit.rs`: with `hard_tabs` set, Tab inserts one `\t` at each caret
  (without it, spaces up to the next tab stop, as before), a Tab over a selection
  that spans lines adds one `\t` to each touched line, Shift-Tab takes one leading
  `\t` off a line that starts with one (else up to a tab size of leading
  whitespace, as before), and Enter after an opener indents the new line one `\t`
  deeper. `indent_step()` is the one place that says what a level is made of.
  The tab size still drives the indent guides and the non-tab path.
  Sirio's status row switches it per file (Indent: Spaces or Tabs).
  In Tabs mode N drives only the indent guides and the status label: the text
  system draws a tab character at its own advance, so a file indented with tabs
  looks the same at Tab Size 2 and 8 (measured in a capture of both).

### Performance (Task 12b)

The editor's frame cost grew with the file: on a 1 MiB, 39,247-line file a typed
character drew in 29 ms and a scrolled frame in 37 ms (main: 4 ms), because
`CodeEditor::render` read the whole text on every frame. Each change below makes
a frame's work follow what is drawn, or what an edit touched, and none changes
what is drawn except the minimap's sampling of a file taller than it (the
last bullet says how). Measured, against main, in
`docs/perf/2026-10-ely-code-editor.md` (`CodeEditor.render` median 25.5 ms
before, 0.04 ms after, on a 1 MiB file of 39,247 lines).

- `Cargo.toml`: depends on `sirio_perf` (path `../../crates/sirio_perf`, which
  has no dependencies of its own, so no cycle). `CodeEditor::render` and its
  steps (`CodeEditor.active_block`, `.brackets`, `.rows`, `.sticky`,
  `.minimap`), the list's row builder (`CodeEditor.visible_rows`) and the
  minimap's colour read (`CodeEditor.minimap_rows`) carry content-free
  `sirio_perf` spans, the way every Sirio `Render::render` does; every one puts
  the editor's entity id in the entity column.
- `src/editor/buffer.rs`: `Buffer` keeps a log of its last 32 edits and
  `Buffer::kept_since(stamp) -> Option<Kept>` says how many bytes at the start
  and at the end no edit since that stamp touched (`None` for another text's
  stamp or an edit that has left the log). It is the one reader of the log.
- `src/editor/syntax.rs`: `brackets(&Buffer)` is now a test-only reference;
  `BracketIndex` keeps every bracket between frames and, after an edit, lexes
  again only the lines between the first and last ones the edits touched
  (`kept_since`), shifting the brackets after them; pairing and depth are a pass
  over the brackets alone, and the list is spliced and re-paired in time
  proportional to all the brackets, not to the edit. A text it has not seen is
  read whole. `folds` is one
  pass over the lines (a stack of open headers) instead of a scan to the end of
  each block, giving the same list. `opens(buffer, line)` and
  `Structure::enclosing(buffer, line)` answer for one line what `folds` answers
  for all: whether a block opens there, and the headers of the blocks that hold
  it. A `Structure` on the editor keeps the fold list and the bracket index per
  text revision (`CodeEditor::folds`, `CodeEditor::brackets`), and the nesting
  index (see "Fix round 2" below).
- `src/editor/view.rs`: the frame no longer holds the folds or the places the
  selected word appears. The sticky header and the strong guide ask `enclosing`
  about one line; the gutter's chevron asks `opens`; `Frame` holds the needle
  (`Frame.needle`) and each drawn line looks for it on itself
  (`src/editor/line.rs`, `syntax::occurrences` over that line). A selection over
  several lines is no longer copied to make a needle that could never match. A
  line's brackets are found by search in the offset-ordered list, not by a pass
  over all of them (`line.rs`).
- `src/editor/layout.rs`: `Rows` holds the rows as a count when row `n` is line
  `n` (no fold, lens, removed line, ghost line or chat: `rows_are_lines`), so a
  file of any length costs nothing to hold, and as a list otherwise; `row_of`,
  `line_from`, `get` and `len` are what the list, the gutter, the sticky header
  and the input method ask. `CodeEditor::primary_row` answers the caret's line
  directly in the first case, and the caret motions skip the fold scan while
  nothing is folded (a held Down key had read every fold of the file per press).
  `around` is gone with its only caller.
- The minimap (`view.rs`, `layout::minimap_plan`) reads its colours in the
  canvas's prepaint, where its height is known, and for the rows it will draw:
  every row while a row is a pixel tall or more (all that a file of up to its
  height in rows has, drawn exactly as before), else one row per pixel, drawn
  as most of that pixel. A taller file used to paint every line's colours as
  sub-pixel quads on top of one another each frame; it now shows one line's
  colours per pixel of its height. `tall` (the row height used to jump from a
  click) is unchanged. **This is the one change to what is drawn:** a file
  taller than the minimap's height in rows is shown by sampled lines, not by
  all of them (see fix round 2 for which lines).

#### Fix round 2

- `src/editor/syntax.rs`: `Structure::enclosing(buffer, line)` replaces the
  climb over every line above the line (`syntax::enclosing`, now a test-only
  reference, `enclosing_by_climbing`), which cost the line's number on a file
  indented under one header (a class, a JSON document, `impl` / `mod tests`). An
  index of each line's innermost header, built in one pass over the lines
  (`nesting`), is read by following a header's own header outward, in as many
  steps as the nesting is deep. It is kept per text revision with each line's
  `shape` (blank, or its indent): an edit that leaves every shape between the
  lines `kept_since` names as it was (typing into a line) keeps the index; any
  other edit builds it again in one pass, which costs the number of lines.
  `CodeEditor::enclosing` is what the sticky header and the strong guide ask;
  the strong guide's block is asked for only while guides are drawn.
- `src/editor/layout.rs`, `view.rs`: the minimap samples a taller file by
  pixel row: pixel `y` shows the row at `floor(y / tall)`, at `y`. It used to
  step through the rows `stride` apart (`ceil(1 / tall)`), which re-picked every
  sampled row each time the stride changed and picked different lines after any
  edit; the new sampling moves no pixel's row by more than one when a line is
  added. A line shifted down by an Enter is still a different sample for most
  pixels (sampling cannot avoid that), so the picture changes at the sampled
  rows below the edit, by neighbouring lines. Files up to the minimap's height
  in rows are drawn as before.
- `src/editor/view.rs`: the minimap's tokens are painted inside one
  `Window::paint_layer`. Each `paint_quad` otherwise looks up everything
  already painted for its draw order, about a microsecond a quad, and a
  screenful of Rust is some 3,000 quads a frame (a `CodeEditor.minimap_paint`
  span times it): 3.4 ms a frame on a 24k-line Rust file, 0.3 ms in a layer. The
  quads are the same, so the picture is the same. (The first version of the
  pixel-row sampling had drawn a row at every pixel and cost 1.1 ms more than
  the stride sampling it replaced, which had been leaving about a quarter of its
  rows unpainted by accident: GPUI snaps a quad's corners to device pixels, so a
  0.7 px quad whose top falls between .5 and .8 of a pixel becomes a box of no
  height; a dark capture of a 24k-line file has 10,673 painted pixels with the
  stride sampling and 14,916 with the pixel rows. Measured in
  `docs/perf/2026-10-ely-code-editor.md`.)
- `src/editor/anchors.rs`, `state.rs`, `line.rs`, `gutter.rs`: a row's washes
  (`Marks.backgrounds`) and diagnostics are found by range, not by testing every
  one for every drawn row (a find on `e` washes ~117k places). `Reach` keeps,
  for a list of ranges, their places sorted by start with the furthest end up to
  each, so `Marks::diagnostics_touching` and `backgrounds_touching` give a line's
  candidates in the owner's order (the order they draw in: a later wash lies over
  an earlier one) for the caller's own test to narrow; they are looked up again
  by `Marks::reindex` after either list is set, carried through an edit
  (`anchors::shift`) or cleared. The lists themselves are not reordered.

### Final pass (Task 12c)

Every change the editor plan made to this crate has an entry above: the editor
closure and its ports from upstream `main` (Task 1), the highlighter, view
options, hit events and `reset` (Task 2), `row.rs`'s line selector and
`reveal_primary` (6b1), the carriage-return and silent-reset changes (Task 6),
the drawn rulers and guides (6d), `select_in_place` (7a), the `diagnostics`
getter (7b), the read-only banner hook (10b), hard tabs (11b), the pointer
leaving the code (12a, under "Hosting hooks") and the performance work (12b).
Tasks 8, 9 and 11a to 11c added no other vendored change: git marks, find, the
status row and the bell are Sirio's own (`sirio_ui::file_view` and its
modules) over Ely's marks, `FindWidget` and `StatusBarItem`.

Evidence: `cargo nextest run -p ely-gpui-component --features test-support`
runs 137 tests, all passed. The native proofs are
`Scripts/Tests/test-ely-editor-probe.sh --xvfb` (the hosting hooks, on a frame)
and `Scripts/Tests/test-editor-e2e.sh --xvfb` (the whole file tab, 27
scenarios, with the fake language server `Scripts/Tests/fake_lsp.py`); the
performance record is `docs/perf/2026-10-ely-code-editor.md`. Linux X11 only:
macOS and Windows keys and a real input method are not covered.
