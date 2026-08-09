# Xcode-parity chat rows

**Date:** 2026-08-08
**Status:** Implementing

## Goal

Make Tiller's chat transcript read like a native macOS coding assistant: one
quiet transcript surface, compact rows, explicit status, and structured code
changes. The reference is Xcode's dark Coding Assistant panel; Tiller keeps
its own iconography and preserves its existing worktree/session features.

## Component boundaries

| Component | Responsibility | Independent judgment |
| --- | --- | --- |
| `ThinkingRowView` | Live status plus collapsed/expanded reasoning text | Is the active state legible without adding a heavy card? |
| `ToolCallCardView` + `ChatRowChrome` | One compact tool row, status badge, line-count badge, hover/active state | Can a user scan the tool history in one pass? |
| `ChatDiffPreviewView` | Inline old/new rows with line numbers and +/- markers | Can additions and deletions be read without opening another surface? |
| `MessageRowView` | Assistant markdown plus timestamp, duration, and copy affordance | Does metadata stay quiet but discoverable? |
| `ChatRowSurface` | Shared hover, active rail, spacing and color tokens | Do rows feel like one system rather than unrelated cards? |

The existing `AgentMarkdownTextView`, `MarkdownAttributedStringRenderer`,
TextKit stack, Tree-sitter paths, and streaming snapshot cache remain untouched
by the visual refactor.

## Visual rules

- The transcript background is shared; cards are reserved for actions and
  proposals that need a boundary.
- Hover uses a restrained row fill. Active/running/permission-pending rows add a
  two-point accent rail and never change layout height abruptly.
- Thinking and historical thought rows collapse by default and expand in place.
- Tool rows expose status as words (`Queued`, `Running`, `Done`, `Failed`) and a
  compact line count. Diffs use a fixed line-number gutter and explicit `+` / `−`
  signs with semantic green/red backgrounds.
- Assistant timestamps come from the existing turn divider date through
  `TimelineRow.MessageMeta`; no new transcript persistence format is needed.

## Verification loop

For each component, capture the same dark-window-size Xcode reference and a
Tiller capture, give only those images and the goal to a blind critic, record
the winner and largest mismatch, fix that mismatch, and recapture. If the
desktop environment cannot produce a live capture, report the blocker and use
the closest available official Xcode dark reference plus a component-rendered
Tiller artifact; do not claim a live UI pass.

