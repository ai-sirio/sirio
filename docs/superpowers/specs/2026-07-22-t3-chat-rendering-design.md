# T3-Style Chat Text Rendering

**Date:** 2026-07-22
**Status:** Approved for planning

## Goal

Make the ACP chat's assistant-message rendering match the T3 Code chat look
(reference screenshot: dark, airy prose; code blocks as bordered cards with a
header bar showing the language and a copy button; inline code as rounded
chips; real list bullets) — while keeping the existing single-`NSTextView`
architecture so drag-selection stays continuous across block boundaries
(see `2026-07-19-agent-markdown-selection-design.md`).

## Decisions made during brainstorming

1. **Full T3 look** — typography, inline-code chips, code-block cards with
   header bar (language label + copy button), real bullets.
2. **Selection continuity is preserved** — one `NSTextView` per message.
   Cards are drawn by a custom `NSLayoutManager`; the header bar is a SwiftUI
   overlay (not selectable text, same as T3 web).
3. **Syntax highlighting via Highlightr** (highlight.js through
   JavaScriptCore, ~185 languages). Splash was rejected: it highlights Swift
   only. Streaming cost is handled with a cache.

## Architecture

Four components, all in `App/Chat/` (UI-layer code; nothing here belongs in a
package — it is all AppKit/SwiftUI):

| Component | Role |
|---|---|
| `MarkdownAttributedStringRenderer` (extended) | T3 typography, re-inserted list bullets, custom attributes marking inline-code ranges and code-block ranges (with language + raw code metadata) |
| `CodeBlockLayoutManager` (new, `NSLayoutManager` subclass) | Draws rounded cards behind code blocks (dark fill + 1px border, including header zone) and rounded chips behind inline code, via `fillBackgroundRectArray` / `drawBackground` overrides |
| `AgentMarkdownTextView` overlay (extended) | Language label + copy button positioned over each card's header rect; rects computed from the layout manager, exposed through the `Coordinator` |
| `CodeHighlighter` (new) | Highlightr wrapper with a `(code, language, appearance) → NSAttributedString` cache; monochrome fallback for unknown languages |

TextKit 1 is used explicitly (already forced today: `sizeThatFits` touches
`layoutManager`), which is what makes the custom-`NSLayoutManager` drawing
approach viable.

**Single source of truth:** custom `NSAttributedString` attributes travel with
the text. The layout manager re-reads them at draw time (no extra plumbing),
and the overlay derives header rects from the same ranges.

## Typography and prose

- Body 13pt; `lineSpacing` 3 → ~4.5; `paragraphSpacing` 6 → 9.
- **Bullets:** `AttributedString(markdown:)` discards list markers. The
  renderer re-inserts them (`•` for unordered, `N.` for ordered, taken from
  the `listItem` ordinal) with a hanging indent (tab stop at 16pt). Nesting
  depth from the count of `listItem` components in the presentation intent.
- **Inline code:** chip — 12pt monospace, near-label foreground color (teal
  removed), rounded background (radius 4) drawn by the layout manager with
  horizontal padding.
- Headings, blockquotes, links: unchanged (already close to T3).

## Code block card

- Block range marked with a custom attribute carrying `language` (from the
  fence info string, fallback `"text"`) and the raw code string.
- The layout manager unions the range's line-fragment rects, outsets them
  (12pt padding, +28pt above for the header zone) and draws a rounded rect
  (radius 8, subtle fill, 1px separator-color border).
- `paragraphSpacingBefore` on the block's first paragraph reserves the header
  height in text layout, so text never flows under the header.
- **Overlay per card:** language label (left) and copy button (right). Copy
  puts the raw code on the pasteboard and shows a checkmark for ~1.5s.
- **Content:** highlighted by `CodeHighlighter` using a Highlightr theme pair
  matched to Tiller's palette; follows the app's existing appearance setting
  (dark/light), cache invalidated on appearance change.

## Streaming and performance

- Messages already re-render per streamed chunk; the only new cost is
  highlighting → cached by `(code, language, appearance)` hash. During
  streaming only the trailing partial block changes; highlight.js on small
  blocks is ~ms.
- Guard: blocks over 20k characters stay monochrome until their fence closes.
- The existing height cache in `AgentMarkdownTextView.sizeThatFits` is
  unchanged.

## Error handling

- Unparsable markdown / unclosed fence: existing fallbacks unchanged (plain
  text; open fences already covered by tests).
- Language unknown to Highlightr → monochrome content, never a visible error.
- Highlightr initialization failure (JavaScriptCore) → monochrome everywhere;
  the app must not crash or log per-render.

## Testing

Tests first (`swift-testing`), extending `MarkdownAttributedStringRendererTests`
plus new suites:

- Bullets re-inserted for unordered/ordered/nested lists.
- Inline-code runs carry the chip attribute and mono font.
- Code-block runs carry the block attribute with language + raw code;
  fence info string captured; fallback `"text"`.
- `CodeHighlighter`: cache hit/miss, unknown-language fallback, >20k guard.
- Card drawing, overlay positioning, copy button: manual QA checklist
  (visual, both appearances, during streaming).

## New dependency

Highlightr via SPM, added to the App target in `project.yml`
(`xcodegen generate` required after).
