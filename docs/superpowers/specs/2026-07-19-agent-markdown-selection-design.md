---
title: Continuous text selection inside agent markdown replies
date: 2026-07-19
status: approved
---

# Continuous text selection inside agent markdown replies

## Background

In `TranscriptView.swift:44-46`, each `.agentMessage` renders via
`Markdown(text).markdownTheme(.tiller).textSelection(.enabled)`
(`MarkdownUI`). Reported bug: dragging to select text stops at a block
boundary inside a single agent reply (e.g. between a paragraph and a code
block, or across a heading) — the user can select part of the reply, then
hits what looks like "a line" where selection can't continue into the rest.

Root cause: MarkdownUI renders a parsed markdown document as a tree of
separate SwiftUI views, one per block (heading, paragraph, code block, list,
blockquote, thematic break, table), stacked in a block sequence.
`.textSelection(.enabled)` applied once at the top does not merge sibling
`Text` views into one selectable range — this is a general SwiftUI
limitation, not specific to MarkdownUI: native continuous drag-selection
across separate `Text` views requires one shared text storage (`NSTextView`),
which the block-sequence rendering doesn't provide.

Confirmed in-scope: only `.agentMessage` markdown rendering. `ThoughtRow`
(`TranscriptView.swift:154-182`) renders plain `Text`, not `Markdown`, so it
doesn't hit this fragmentation and is untouched. Cross-message selection
(e.g. spanning a user bubble and the agent reply below it, or jumping over a
tool-call card) is explicitly out of scope — different visual containers
(alignment, background, padding) that shouldn't share one text flow; addressed,
if ever, as a separate future request.

## Decisions

- **Scope: per-message, not per-transcript.** One `NSTextView` per
  `.agentMessage` item, not one `NSTextView` spanning the whole transcript.
  Keeps `TranscriptView`'s existing `LazyVStack`/streaming/scroll-to-bottom
  structure untouched; tool-call cards, plan cards, dividers, thought rows
  all stay exactly as they are today.
- **Markdown → `NSAttributedString` via Foundation's `AttributedString(markdown:)`**
  (`.full` interpretedSyntax), not a hand-rolled `cmark-gfm` (the parser already vendored transitively by MarkdownUI) AST walker.
  Zero new dependencies. Accepted risk: GFM fidelity (tables, task lists)
  may not fully match MarkdownUI's `.gitHub` theme — verified acceptable by
  spike (see Testing) before full build-out; `cmark-gfm` AST walk stays
  as documented fallback if the spike fails.
- **No internal scroller.** Unlike `ChatTextEditor`, the new view has no
  `maxHeight` clamp — it reports full content height and lives inside
  `TranscriptView`'s existing outer `ScrollView`, matching how `Markdown(text)`
  behaves today.
- **Malformed markdown mid-stream degrades to plain text, not a crash or blank
  view.** Streaming writes partial markdown (e.g. an unclosed code fence)
  that can fail to parse; falling back to unstyled-but-selectable text keeps
  the reply visible and copyable while it's incomplete.

## Architecture / data flow

1. `TranscriptView.itemView(_:)`'s `.agentMessage` case
   (`TranscriptView.swift:43-46`) replaces
   `Markdown(text).markdownTheme(.tiller).textSelection(.enabled)` with
   `AgentMarkdownTextView(markdown: text)`.
2. `AgentMarkdownTextView.updateNSView` runs on every SwiftUI re-render
   (including each streamed chunk while `isComplete == false`); it compares
   the incoming markdown string against the last-rendered source string
   (same guard shape as `ChatTextEditor.swift:50`) and only re-parses /
   re-renders when it actually changed, to avoid re-parsing markdown on
   every unrelated view update.
3. On change, calls `MarkdownAttributedStringRenderer.render(_:)` to get an
   `NSAttributedString`, assigns it to `textView.textStorage`, then
   recalculates height the same way `ChatTextEditor.Coordinator.recalculateHeight`
   does (`layoutManager.usedRect(for:)`).

## Components

**Create in `App/Chat/`:**

- `MarkdownAttributedStringRenderer.swift` — pure function
  `render(_ markdown: String) -> NSAttributedString`. Parses via
  `AttributedString(markdown:options:)` with `.full` interpretedSyntax; walks
  the result's runs and their `PresentationIntent` (heading level, code
  block, list, blockquote) to apply `TillerMarkdownTheme`-equivalent
  styling: 13pt body, 12pt monospaced for code, semibold 15/14/13 for
  h1/h2/h3, matching margins. On parse failure, returns the raw markdown
  string as a plain unstyled `NSAttributedString` (still selectable).

- `AgentMarkdownTextView.swift` — `NSViewRepresentable` following
  `ChatTextEditor`'s shape: `makeNSView` builds an `NSTextView`
  (`isEditable = false`, `isSelectable = true`, `drawsBackground = false`,
  `textContainer.widthTracksTextView = true`) with no enclosing
  `AutoSizingScrollView` clamp — reports full `usedRect` height via
  `sizeThatFits`, same technique as `ChatTextEditor.sizeThatFits`. No
  `Coordinator` delegate methods needed beyond height recalculation (no
  editing, no `doCommandBy` handling). Links: `AttributedString(markdown:)`
  preserves `.link` attributes; a non-editable, selectable `NSTextView`
  opens `.link` runs via its default click handling (`NSWorkspace`) with no
  extra code — matches MarkdownUI's clickable-link behavior today.

**Change:**

- `TranscriptView.swift:44-46` — swap the `Markdown` call for
  `AgentMarkdownTextView(markdown: text)`.

**Untouched:** `ThoughtRow`, `ToolCallCardView`, `EditSummaryCardView`,
`planCard`, `turnDivider`, `userBlockView`, `TillerMarkdownTheme.swift` (its
values become the source of truth the new renderer's mapping copies from,
but the file itself isn't touched unless the mapping needs a value it
doesn't currently expose cleanly).

## Error handling

| Situation | Behavior |
|---|---|
| `AttributedString(markdown:)` throws (malformed/partial markdown mid-stream) | Fall back to plain unstyled text, still selectable. Not silent — this is a deliberate degrade path, not a swallowed error. |
| Empty string | Empty `NSAttributedString`, view collapses to zero height (matches current empty-message behavior). |

## Testing

- `MarkdownAttributedStringRendererTests.swift` (swift-testing): fixtures for
  bold/italic/inline-code, headings h1-h3, fenced code block, ordered/unordered
  list, blockquote, link — assert extracted plain text and key attributes
  (font size, weight) at expected ranges. Includes a fixture with a
  deliberately unclosed code fence to exercise the fallback path.
- **Spike gate before full build-out:** run the renderer against a handful of
  real captured agent replies (should include at least one GFM table and one
  task list, since those are the identified fidelity risk) and manually
  compare against current MarkdownUI rendering. If tables/task-lists render
  unacceptably, fall back to the `cmark-gfm` AST-walk approach documented
  in Decisions instead of shipping degraded output.
- Continuous drag-selection itself is a manual QA check (not unit-testable):
  select across a paragraph → code block → list boundary in a real agent
  reply and confirm one continuous, copyable selection.
