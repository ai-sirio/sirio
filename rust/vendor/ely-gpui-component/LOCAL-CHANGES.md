# Ely chat integration

Upstream: https://github.com/ZacharyZhang-NY/Ely-GPUI-Components
Revision: `e17e31a6890c09ebcfa8b61133d7bc7c625edf69`
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
  host's own editor, with host-supplied send eligibility (`ready`) and focus
  (`.focused`). It takes no `forms::Enter` action and offers no completion
  rows, so the host's keyboard, IME and popups stay in charge; the default
  constructor keeps its upstream behaviour. In custom mode the shell wears
  the input fill (`sunken`) so the host's field and the card read as one
  surface, its focused border steps to `fg_muted` instead of the ring colour,
  and the tool row wraps. `.trailing` puts tools at the end of the row, next
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

- `src/theme/palette.rs` and `src/theme/syntax.rs` moved, with `Mode`, into
  the sibling crate `rust/vendor/ely-palette`, which this crate re-exports at
  the original paths: `theme::{Palette, Syntax, Mode, …}` are the same types.
  Sirio's theme crate holds Ely's palette as its colour vocabulary without
  compiling Ely's components. `Palette` and `Syntax` derive `Copy` (all their
  fields are `Hsla`), so Sirio's `Theme` stays `Copy`. The files are otherwise
  upstream's.
