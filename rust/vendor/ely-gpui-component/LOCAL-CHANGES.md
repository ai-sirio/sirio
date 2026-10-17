# Ely chat integration

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
  editor/document/terminal actions. Eager asset validation belongs to the
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
- **This crate:** `cargo test -p ely-gpui-component --features test-support`,
  79 passed (77 without the feature: two fit tests need it). Its own
  warnings — unused `pub(crate)` re-exports of the parts of Ely this chat does
  not use — are upstream's subset, not suppressed.
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
Every file below is verbatim unless its entry says otherwise.

Vendored files:

- `src/editor/{buffer,cursor,edit,keys,moves,line,row,view,layout,gutter,decor,syntax,ime,find,status,state}.rs`:
  verbatim, except the two local changes listed below.
- `src/editor/tests/mod.rs` (was `tests.rs`; moved by the `b29d66c` port
  below): verbatim, gated `cfg(all(test, feature = "test-support"))`
  as upstream. Nothing was removed: none of its 15 tests names a module this
  crate does not declare.
- `src/editor/mod.rs`: local. It declares only the files above. Upstream's
  `mod.rs` also declares `chat`, `console`, `extensions`, `filters`, `hints`,
  `hover`, `keybindings`, `preview`, `problems`, `refactor`, `search`,
  `settings`, `symbols`, `tasks` and `welcome`, all excluded, and their
  re-exports with them. `anchors` and `measure` arrive from `main` in Task 1b.
- `src/navigation/breadcrumb.rs`: verbatim. `src/navigation/mod.rs` declares
  it and re-exports `Breadcrumb` and `Crumb`.
- `src/shell/bars.rs`: only `StatusBarItem`, its `ClickHandler` alias and the
  imports it needs. `Toolbar`, `ToolbarGroup`, `ToolbarSeparator` and
  `StatusBar` are not vendored. `src/shell/mod.rs` is local: it declares `bars`
  and re-exports `StatusBarItem`.
- `src/compat.rs`: `tint`, copied verbatim from upstream `charts/paint.rs`,
  with the `Hsla` and `Palette` imports it needs. `code_colors` and its users
  are unchanged.
- `src/lib.rs`: declares `pub mod editor;` and `pub mod shell;`.

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
  this file changed by that vendoring; the `b29d66c` port below changes its
  pointer callers.
- Warnings, not suppressed, following the precedent of the unused re-exports
  above: `keys::bind_keys` has no caller until `init_editor` (Task 1c) calls it,
  and `layout::stack` and `syntax::colors` (re-exported as `code_colors`) have
  no user inside this crate yet.

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
  `src/editor/mod.rs` is local, so the port adds only `mod anchors;`; the four
  warnings recorded above move down one line (mod.rs:28, 29, 35).
  `tests/stale.rs` is verbatim except two tests, dropped because they name
  code in modules this crate does not vendor:
  `an_output_channel_gone_from_its_list_draws` (`console::OutputPanel`) and
  `a_glob_too_big_for_a_regex_matches_nothing` (`filters::passes`). The seven
  stale-state tests that remain pass.
  Skipped, not vendored: `src/editor/console.rs` (the output panel's shown
  channel), `src/editor/filters.rs` (the glob size limit) and
  `tasks/panics.md`. Their panics are not ported: the output panel and globs
  are outside the closure.
- `9fb607b` "Code editor: columns sit where gpui lays them out": `ime.rs`,
  `measure.rs` (new) and `tests/wide.rs` (new) are verbatim. `state.rs`,
  `line.rs`, `state.rs`, `view.rs`, `mod.rs` and `tests/mod.rs` did not apply:
  their context named upstream code from commits between `b29d66c` and this
  one that the vendored copy does not have (the typewriter, `Metrics::height`,
  `line.rs`'s imports, and the `preview`, `chinese` and `history` modules,
  which are not vendored). Their added lines were typed in by hand. Hand-applied
  pieces: `Metrics::size` (after `line`); `self.metrics.size = pixels` after
  `self.metrics.line = line`; in `line.rs` the `measure::Measured` import, the
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
