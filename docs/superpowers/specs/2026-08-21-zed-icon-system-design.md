# Zed icon system for Tiller

**Date:** 2026-08-21
**Status:** Approved design, pending implementation plan

## Context

Tiller currently mixes Comet SVGs, a few older Phosphor assets, custom agent
marks, and macOS-only SF Symbols. Generic icons are commonly sized from text
metrics such as 11.5 px and 12.5 px. The result varies by platform and makes
some glyphs appear softer or less consistent than the icons in Zed.

Zed uses a single embedded SVG set, a typed icon catalog, semantic sizes, and
GPUI's stock `svg()` element for monochrome icons. Tiller will adopt the same
assets and rendering conventions without depending on Zed's full `ui` or
`icons` crates.

## Goals

- Use the official Zed SVGs for every Tiller icon that has a suitable Zed
  equivalent.
- Render the same glyph geometry on Linux and macOS.
- Adopt the Zed-style 12 px, 14 px, and 16 px semantic size ladder.
- Preserve existing click targets, hover behavior, focus behavior, tooltips,
  accessibility labels, and application state.
- Keep Pi and Oh My Pi recognizable where Zed has no equivalent marks.
- Preserve Oh My Pi's intentional full-color gradient.
- Record upstream provenance and license information in the repository.

## Non-goals

- Importing Zed's complete `ui` or `icons` crates.
- Redesigning Tiller's titlebar, sidebar, Settings, tabs, or status bar.
- Changing theme colors, spacing, radii, or typography.
- Adding a user-selectable icon theme.
- Expanding the file-type icon taxonomy beyond the icons Tiller already
  exposes.

## Authoritative upstream snapshot

The asset source is Zed commit
[`875e2a1c458ffcf2bfe90be841eb9893f399b4ff`](https://github.com/zed-industries/zed/commit/875e2a1c458ffcf2bfe90be841eb9893f399b4ff),
observed on 2026-08-21.

Relevant upstream implementation:

- [`Icon` and `IconSize`](https://github.com/zed-industries/zed/blob/875e2a1c458ffcf2bfe90be841eb9893f399b4ff/crates/ui/src/components/icon.rs)
- [`IconButton`](https://github.com/zed-industries/zed/blob/875e2a1c458ffcf2bfe90be841eb9893f399b4ff/crates/ui/src/components/button/icon_button.rs)
- [`IconName` path catalog](https://github.com/zed-industries/zed/blob/875e2a1c458ffcf2bfe90be841eb9893f399b4ff/crates/icons/src/icons.rs)
- [Icon asset license](https://github.com/zed-industries/zed/blob/875e2a1c458ffcf2bfe90be841eb9893f399b4ff/assets/icons/LICENSES)

Only the required SVG files will be copied to
`rust/assets/icons/zed/`. They must remain byte-identical to the pinned
upstream files. The local asset directory will include an attribution file
with the upstream commit, source URLs, and ISC license text.

## Icon mapping

The public Tiller `Icon` enum remains the stable semantic API. Call sites do
not refer to upstream filenames directly.

| Tiller icon | Asset after migration | Notes |
| --- | --- | --- |
| `FolderFill` | `zed/folder.svg` | Zed's folder geometry replaces the filled/Comet shape. |
| `GitBranch` | `zed/git_branch.svg` | Worktree and git contexts. |
| `MessageSquare` | `zed/chat.svg` | Chat surfaces. |
| `SquareTerminal` | `zed/terminal.svg` | Terminal surfaces and agent settings. |
| `Close` | `zed/close.svg` | Close controls. |
| `ChevronDown` | `zed/chevron_down.svg` | Disclosure controls. |
| `ChevronRight` | `zed/chevron_right.svg` | Disclosure and navigation controls. |
| `ChevronLeft` | `zed/chevron_left.svg` | Back navigation. |
| `Settings` | `zed/settings.svg` | Settings and configuration files. |
| `RefreshCw` | `zed/rotate_cw.svg` | Refresh actions. |
| `Plus` | `zed/plus.svg` | New-tab and add actions. |
| `File` | `zed/file.svg` | Generic files and Changes surfaces. |
| `Sparkles` | `zed/sparkle.svg` | AI providers. |
| `Shield` | `zed/lock.svg` | Permissions. Zed has no shield icon in this catalog. |
| `SunMoon` | `zed/screen.svg` | Appearance. Zed has no sun/moon icon in this catalog. |
| `Globe` | `zed/public.svg` | Browser/public network surfaces. |
| `ClaudeCode` | `zed/ai_claude.svg` | Official Zed monochrome mark. |
| `Codex` | `zed/ai_open_ai.svg` | Official Zed monochrome OpenAI mark. |
| `OpenCode` | `zed/ai_open_code.svg` | Official Zed monochrome mark. |
| `Pi` | `agent-pi.svg` | Existing Tiller mark; Zed has no Pi mark. |
| `OhMyPi` | `agent-omp.svg` | Existing Tiller full-color mark. |
| `SidebarLeft` | `zed/threads_sidebar_left_open.svg` | Left sidebar toggle. |
| `PanelRight` | `zed/threads_sidebar_right_open.svg` | Right panel toggle. |
| `Archive` | `zed/archive.svg` | Archive files. |
| `Lock` | `zed/lock.svg` | Lock files. |

After all references move, the obsolete Comet icon directory and its
attribution file may be removed only if a repository-wide search confirms
that no source or test still consumes them. Older unused Phosphor generic
assets are subject to the same reference check; brand assets outside this
mapping are not part of the cleanup.

## Rendering architecture

### Monochrome icons

`IconElement` remains Tiller's compatibility-facing adapter, but its generic
path delegates to GPUI's stock SVG element using the same shape as Zed:

```rust
svg()
    .size(resolved_size)
    .flex_none()
    .path(icon.path())
    .text_color(resolved_color)
```

The adapter keeps the existing caller-facing text-color behavior so feature
modules do not need to know how SVG tinting works. Every generic icon and the
three Zed agent marks use this path on every platform.

### Full-color exception

`OhMyPi` remains the only `has_own_colours()` icon. It keeps the existing
cached full-color raster path because the SVG contains its own gradient and
must not receive the theme tint. Pi becomes the existing monochrome
`agent-pi.svg` and follows the stock tinted path.

### Platform behavior

The macOS SF Symbols branch, `system_symbol`, and `sfsymbol.rs` are removed.
There is no platform-dependent glyph selection after this migration. The
same embedded SVG bytes and semantic size are used on Linux and macOS.

### Asset loading and failures

`TillerAssets` continues to serve embedded icon bytes. `include_bytes!`
makes a missing file a compile-time failure. Tests validate that every
catalog entry resolves to a non-empty SVG with a 16 by 16 canvas. Zed assets
that omit `viewBox` but declare `width="16"` and `height="16"` are valid and
must not be modified solely to satisfy the test.

Unknown runtime asset paths continue to return `None`; there is no network
fallback.

## Semantic sizes

Tiller adds an `IconSize` type with these base values:

| Size | Base value | Usage |
| --- | ---: | --- |
| `XSmall` | 12 px | Status bar, disclosure controls, and secondary actions. |
| `Small` | 14 px | Sidebar rows, file tree, tabs, and Settings navigation. |
| `Medium` | 16 px | Titlebar cluster and primary icon actions. |
| `Custom(Pixels)` | Explicit | Intentional large illustrations and 32 px agent marks only. |

The base value participates in Tiller's existing UI scale. Semantic call
sites must not derive icon size from fractional typography metrics after the
migration.

Existing hit targets remain unchanged:

- titlebar cluster: 24 px button, 16 px icon;
- status bar: 22 px button, 12 px icon;
- all other containers retain their current row/button dimensions and only
  adopt the appropriate semantic icon size.

## Accessibility and interaction

This change does not alter event handlers or state. Icon-only interactive
controls retain their existing semantic role, tooltip, accessible label,
keyboard focus handling, and hover/active feedback. Tint continues to come
from theme semantic colors, so the same asset works in light and dark themes.

## Test strategy

Implementation follows RED/GREEN:

1. Change focused `tiller_ui::icons` tests first so they require the Zed
   paths, semantic size values, agent mapping, cross-platform SVG behavior,
   and the two Tiller fallback marks.
2. Run `cd rust && cargo test -p tiller_ui icons::tests` and retain the
   expected RED evidence.
3. Add the minimum assets and implementation required for GREEN.
4. Run the focused selector again and confirm a non-zero test count.
5. Run `cd rust && cargo test -p tiller_ui`.
6. Run `Scripts/ci.sh`; completion requires exit zero and the literal
   `CI OK` marker.
7. Run `git diff --check` and inspect the final changed-file scope.

The focused tests cover:

- every `Icon` has a unique, loadable path;
- each migrated icon resolves under `icons/zed/`;
- the three available agent brands resolve to the Zed assets;
- Pi and Oh My Pi resolve to their Tiller assets;
- only Oh My Pi is treated as full-color;
- Zed icon canvases are 16 by 16, with or without an explicit `viewBox`;
- `IconSize` resolves to 12, 14, and 16 base pixels;
- no generic icon has an SF Symbol code path.

## Visual verification

After automated gates, launch the real application and inspect:

- titlebar navigation and panel toggles;
- project sidebar rows;
- terminal/chat/browser tabs;
- file tree and Changes disclosures;
- Settings navigation;
- status-bar settings and refresh actions;
- Claude, OpenAI/Codex, OpenCode, Pi, and Oh My Pi marks.

Repeat the inspection in light and dark themes at the current UI scale. If
the environment permits additional scale factors, also inspect one
non-default scale. Record what was actually observed; do not claim visual
validation for an unavailable platform or scale.

Acceptance requires no clipping, missing glyphs, unintended recoloring, or
mixed apparent stroke weight. Hit targets and interaction behavior must be
unchanged. Visual evidence is reported separately from build and test
evidence.

## Risks and mitigations

- **Upstream drift:** assets are pinned to one Zed commit and vendored; runtime
  behavior does not depend on `main`.
- **License ambiguity:** provenance and the upstream icon license are stored
  beside the copied assets before they are committed.
- **Polychrome regression:** Oh My Pi remains on its current dedicated cached
  raster path and has a focused test.
- **UI density regression:** button and row boxes do not change; only internal
  glyph sizes move to the approved ladder.
- **Hidden platform divergence:** removing SF Symbols leaves one SVG mapping
  for every target.
- **Over-broad cleanup:** old asset files are deleted only after direct
  reference searches prove they are unused.
