# Chat Cards — Task, Question, and Transcript Pass — Design

**Date:** 2026-07-25
**Status:** Approved (pending implementation)

## Context

The chat transcript renders five card-like surfaces (`ToolCallCardView`, `EditSummaryCardView`, the inline plan card in `TranscriptView`, `ComposerApprovalPanel`, `InsightCardView`). None of them share a component: each repeats `.padding(8)` + `.background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))`, with the approval panel already drifted to radius 10. `.quaternary` bypasses `AppTheme`, whose light/dark pairs are the tuned, WCAG-checked ones.

Two agent behaviours have no representation at all:

- **Tasks (subagents).** `SubagentTasks.extract` already detects `subagent_type` in `rawInput`, but only feeds the Agents panel. In chat a Task renders as a generic tool call with a wrench icon. Worse, `ClaudeWire` decodes `parent_tool_use_id` (`ClaudeWire.swift:70`) and nothing consumes it: subagent tool calls land flat in the transcript, and subagent prose is emitted as `agentMessageChunk` (`ClaudeStreamJSONDriver.swift:303`), so it reads as if the main agent said it.
- **Questions.** Permission requests render only in `ComposerApprovalPanel`, above the composer — invisible in history, so reopening a chat loses both the question and the answer. `AskUserQuestion` (multiple choice) is not handled anywhere.

## Goals

- A shared card container; every chat card drawn from `AppTheme` tokens.
- A dedicated Task card that contains its subagent's work instead of leaking it into the main flow.
- A dedicated Question card that lives in the transcript (and therefore in history), with a one-line pending reminder above the composer.
- A transcript pass: spacing hierarchy, live working state, and the fixes listed under "In scope" below.

## Non-goals

- **Multi-select questions.** `AskUserQuestion` supports it; single-select only until something needs more.
- **Any elapsed-time display.** Nothing in the transcript carries timestamps today — only `turnDivider` has a `Date` (`TranscriptItem.swift:94`) — so a task's "34s" would mean persisting `startedAt`/`endedAt` on tool calls. Out of scope; the Task card shows a tool-call count instead. Adding timestamps later is additive and does not change the shapes below.
- **Exit-code chips**, thought-row previews, user-bubble width rework. Deferred until the rest is on screen.
- **Re-enabling timeline rows.** `WorkGroupView` / `TurnFoldRow` stay disabled; this work must not depend on them.
- **Syntax highlighting inside diffs.** The bug is silent truncation, not the absence of colour.

## Decisions

| Decision | Choice | Why |
|---|---|---|
| Card visual language | Fill + left accent rail, colour-coded by kind | The kind is readable while scrolling, without parsing an icon; rail colours come from existing token math |
| Question placement | Inline card (source of truth) + compact pending bar above composer | History keeps the question and the answer; the bar needs no scroll geometry — it exists iff a request is pending |
| Task card depth | Container with nested children, collapsed by default, plus a live "current action" line while running | The nesting data already exists on the wire; collapsed children render *fewer* views than today's flat list |
| Nesting model | Persisted `parentToolCallId` + derived index | See "Architecture" |

## In scope

Numbered for traceability with the implementation plan.

1. Subagent text no longer emitted as main-agent message (`ClaudeStreamJSONDriver.swift:303`).
2. Autoscroll follows streaming text (today it is bound to `items.count`, which does not change while a message grows — `TranscriptView.swift:37`).
3. Diff truncation made visible, with `+N −M` counts (`ToolCallCardView.swift:118-128`).
4. `"Immagine"` → `"Image"` (`TranscriptView.swift:163`) — UI strings are English.
5. `pendingPlanApproval`'s linear scan of all items per redraw removed (`TranscriptView.swift:47`).
6. Shared `ChatCard` container.
7. Cards drawn from `AppTheme` tokens instead of `.quaternary`.
8. Plan card extracted to its own file, with a `3/7` progress count and collapse when complete.
9. Task card (new).
10. Question card + pending bar (new).
11. Transcript spacing hierarchy and a working-state row that names the current action instead of a mute "Thinking".

## Architecture

Parent/child nesting is stored as a field and consumed through a derived index:

```
ToolCall.parentToolCallId: String?          persisted, decodeIfPresent
ToolCallTree.group(items:) -> (roots: [TranscriptItem],
                               children: [String: [ToolCallItem]])
```

`group` is a pure function in `TillerACP`, called once per items change — not once per card. It also supplies the pending plan approval, which removes the per-redraw scan (#5).

Rejected alternatives: a field with view-side grouping (every card rescans all items — #5 multiplied); a real tree in `TranscriptReducer` (updates arrive keyed by `toolCallId` and would need tree search, and the persisted transcript changes shape, forcing a migration).

### Package split

Following the repo rule that anything not needing SwiftUI/AppKit lives in a package:

**`TillerACP`**
- `ToolCall.parentToolCallId` + propagation from `ClaudeWire`.
- `ToolCallTree` — grouping.
- `ChatQuestion` — normalises question + options from either source, so the view reads one shape.
- `PermissionOutcome` extended with a structured answer; drivers send `updatedInput`.

**`App/Chat`**
- `ChatCard.swift` (~60) — fill, accent rail, radius 10, padding 10/12. The only place a card's appearance exists.
- `TaskCardView.swift` (~120) — header (description, `subagent_type` pill, status), meta row (`N tool calls · expand`), live current-action row while running, nested children collapsed by default.
- `QuestionCardView.swift` (~110) — question text, options as buttons; once answered, options are replaced by the chosen value.
- `PendingQuestionBar.swift` (~45) — one line above the composer while a request is pending; tapping it scrolls to the card. It reuses the existing `ComposerPermission` list (`Timeline/ComposerPermissions.swift`) that feeds today's panel — no parallel pending-state type.
- `PlanCardView.swift` (~80) — extracted from `TranscriptView`.

**Modified:** `ToolCallCardView` (adopts `ChatCard`, diff fixes), `TranscriptView` (spacing, working row, #4, #5, #8), `ChatPaneView` (`ComposerApprovalPanel` → `PendingQuestionBar`).

**New `AppTheme` tokens:** `cardFill`, `railTask`, `railQuestion`, `railEdit`, `railTool` — each a light/dark pair consistent with the existing palette.

No new dependencies, no new modules.

## Data flow

```
wire (parent_tool_use_id)
  → driver     sets parentToolCallId on emitted ToolCall/ToolCallUpdate;
               subagent text attaches to the parent task instead of becoming
               an agentMessageChunk
  → reducer    flat items, field populated (no tree, no migration)
  → controller ToolCallTree.group(items:) once per items change
  → views      roots in a column; TaskCardView reads its children from the map
```

Questions: `permissionRequested` (or a tool call — see Open question) → `ChatQuestion` → inline card + pending bar. Answering sends `behavior: allow` with `updatedInput` carrying the selection. Persistence stores both the nesting and the chosen option, so a reopened chat shows what was asked and what was answered.

Autoscroll (#2) uses macOS 15's `ScrollPosition` with `isPositionedByUser`: it follows streaming only when already pinned to the bottom, and never yanks the view when the user has scrolled up to read. No `GeometryReader` inside the `ScrollView` — that is what triggered the layout storms behind the disabled timeline rows.

## Error handling

| Case | Behaviour |
|---|---|
| Legacy transcript without the field | `decodeIfPresent` → `nil` → renders as a root. No migration, no file rewrite |
| Child arrives before its parent | Rendered as a root. **No item is ever dropped for a missing parent** |
| Driver without `parent_tool_use_id` (Codex, OpenCode) | Everything is a root — today's behaviour, unchanged |
| Driver that cannot answer with `updatedInput` | Card degrades to allow/reject rather than showing buttons that do nothing |
| Session dies with a question pending | Card marked expired, pending bar clears |

## Question source

The native stream-json probe carried the question through neither `can_use_tool` nor a plain `tool_use`: both classifiers were zero because this non-interactive Claude environment reported `AskUserQuestion` unavailable after emitting `ToolSearch`. Therefore `ChatQuestion.from(toolCall:)` does not read `rawInput["questions"]` in this native probe and only sees permission options in native flow; keep both code paths because the ACP transport may differ from the native one.

## Testing

`Scripts/ci.sh` does not run the `App/` target, so `CI OK` says nothing about views. Everything with verification value therefore lives in `TillerACP`.

Covered by ci.sh (swift-testing, tests first):

- `ToolCallTree.group` — nesting, orphans, arrival order, and the "no item lost" invariant.
- `parentToolCallId` — Codable round-trip and decoding of a legacy transcript.
- `ChatQuestion` — extraction from both sources.
- `PermissionOutcome` → `control_response` JSON including `updatedInput`.
- Driver — a wire fixture carrying `parent_tool_use_id`: children marked, subagent text **not** emitted as `agentMessageChunk`.

Manual smoke checklist (views):

- [ ] Card appearance in light and dark; rails distinguishable, text contrast holds.
- [ ] Task card expands during execution; the live line tracks the current child.
- [ ] Task card collapsed with ~40 children stays compact and does not stutter while streaming.
- [ ] Question answered from the card; answered from the pending bar's target; both clear the bar.
- [ ] Reopening a saved chat shows the question and the chosen answer.
- [ ] Autoscroll follows streaming at the bottom; scrolling up suspends it.
- [ ] A Codex or OpenCode session renders exactly as before.

`Scripts/ci.sh` must print `CI OK`. Note the known flaky PTY test in `TillerTerminal` — rerun until green rather than treating the first red as a regression.
