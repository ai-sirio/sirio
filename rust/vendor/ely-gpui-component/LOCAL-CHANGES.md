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
