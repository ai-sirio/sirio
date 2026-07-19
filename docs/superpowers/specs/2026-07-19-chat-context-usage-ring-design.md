---
title: Circular context-window indicator in chat composer
date: 2026-07-19
status: approved
---

# Circular context-window indicator in chat composer

## Background

The chat composer (`App/Chat/ChatComposerView.swift`) has no visibility into
how full an agent's context window is. `SessionUpdate`
(`Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift`) — the type that
decodes ACP `session/update` notifications — only knows about message
chunks, tool calls, plan, available-commands, and mode updates; anything
else decodes into `.unknown(String)` and is dropped.

Investigation of the two source ACP agent implementations Tiller talks to
confirmed both already emit a `usage_update` session/update kind carrying
exactly the numbers needed, so no client-side token estimation or per-CLI
stdout parsing is required:

- `agentclientprotocol/claude-agent-acp` (`src/acp-agent.ts`): sends
  `{ sessionUpdate: "usage_update", used: <int>, size: <int> }` on every
  assistant `message_start`/`message_delta`, where `used` sums
  `input_tokens + output_tokens + cache_read_input_tokens +
  cache_creation_input_tokens` (a proxy for post-turn context occupancy) and
  `size` is `session.contextWindowSize` (seeded from `getContextUsage()`,
  refreshed on model switches).
- `anomalyco/opencode` (`sst/opencode`'s current org after a repo rename;
  `packages/opencode/src/acp/usage.ts` + `service.ts`): sends the same
  `sessionUpdate: "usage_update"` shape, confirmed by its own test
  (`test/acp/usage.test.ts`: "sends ACP usage_update with context size and
  cumulative assistant cost").

A third-party ACP client (`textcortex/spritz`, Go) independently
pattern-matches `"usage_update"` as a known protocol kind alongside
`"session_info_update"`, suggesting this has become a de-facto convention
across ACP implementations rather than a private extension of one agent.

Codex/Pi/omp emission of `usage_update` was not verified (GitHub API rate
limit hit mid-research). This is not a blocker — the design degrades
gracefully (see below) for any agent that never sends it.

## Decisions

- **Real protocol signal, not a client-side estimate.** Decode the existing
  `usage_update` kind into `SessionUpdate`; do not add heuristic
  token-counting.
- **Hidden by default.** If a session never receives a `usage_update`
  (agent doesn't emit it), the indicator does not render at all — no
  "unavailable" placeholder state, unlike `UsageBarView`'s four-state
  unavailability handling for API-quota bars. This is a different kind of
  signal (per-session protocol data vs. per-provider API polling), and a
  missing ring is uninteresting on its own — there is nothing actionable
  the user can do about an agent not implementing the extension.
- **Placement:** `ChatComposerView`'s `controlBar`, between `Spacer` and the
  attach-image (paperclip) button — closest to the point of sending, not
  grouped with the mode/agent/effort pills on the left.
- **Fixed accent color**, not usage-based (green/yellow/red) thresholds.
- **Fills as usage grows** (0% at session start → 100% at the model's
  context-window size), not a draining/battery metaphor.
- **Hover tooltip**, two lines, native SwiftUI `.help()`:
  ```
  73% remaining
  146,000 / 200,000 tokens
  ```
- Resets automatically on new conversation / session start: `TranscriptReducer`
  is reconstructed (`reducer = TranscriptReducer()`) at those points already,
  so `contextUsage` returns to `nil` and the ring disappears until the new
  session's first `usage_update` arrives — no extra reset logic needed.

## Architecture / data flow

1. `SessionUpdate.swift` — new `ContextUsage` struct (`used: Int, size:
   Int`, `Sendable, Equatable, Codable`) mirroring the existing
   `PlanEntry`/`AvailableCommand` pattern; new `case usageUpdate(ContextUsage)`
   added to the `SessionUpdate` enum. Decoder gains discriminator case
   `"usage_update"` decoding `used`/`size` keys.
2. `TranscriptReducer.swift` — new `public private(set) var contextUsage:
   ContextUsage?`; `apply(_:)` gains `case .usageUpdate(let usage):
   contextUsage = usage`.
3. `ChatController.swift` — new computed property `var contextUsage:
   ContextUsage? { reducer.contextUsage }`, same pattern as the existing
   `currentModeId` passthrough.
4. `ChatComposerView.swift` — new `contextUsageIndicator` view inserted into
   `controlBar` (line ~76, right after `Spacer`). Renders only when
   `controller.contextUsage` is non-nil and `size > 0`.

## Components

**Edit:**
- `Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift` — `ContextUsage`
  struct, new enum case, decoder branch.
- `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift` —
  `contextUsage` field, new switch case.
- `App/Chat/ChatController.swift` — `contextUsage` passthrough property.
- `App/Chat/ChatComposerView.swift` — `contextUsageIndicator` view, wired
  into `controlBar`; add `@Environment(\.accessibilityReduceMotion) private
  var reduceMotion` (not yet present in this file; mirrors `UsageBarView`'s
  existing use of the same environment key).

No new files — this is a small addition across an existing decode/reduce/
expose/render chain, not a new subsystem.

## Rendering detail

```swift
@ViewBuilder
private var contextUsageIndicator: some View {
    if let usage = controller.contextUsage, usage.size > 0 {
        let fraction = min(1, max(0, Double(usage.used) / Double(usage.size)))
        let remaining = Int(((1 - fraction) * 100).rounded())
        ZStack {
            Circle().stroke(.quaternary, lineWidth: 2)
            Circle()
                .trim(from: 0, to: fraction)
                .stroke(Color.accentColor, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: 16, height: 16)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.3), value: fraction)
        .help("\(remaining)% remaining\n\(usage.used.formatted()) / \(usage.size.formatted()) tokens")
    }
}
```
`Int.formatted()` applies locale-aware grouping automatically (no manual
number formatting). `fraction` clamped to `[0, 1]` defensively — a `used`
that exceeds `size` (context overflow before compaction) should render as a
full ring, not crash a `trim`.

## Testing

- `Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift` — new
  test decoding a `usage_update` JSON payload into
  `.usageUpdate(ContextUsage(used:size:))`, following the existing
  `decodesAgentMessageChunk`-style tests in that file.
- `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift` (or
  wherever reducer tests currently live) — `apply(.usageUpdate(...))` sets
  `contextUsage` to the given value; a subsequent `.usageUpdate` overwrites
  it (not accumulates).

No dedicated UI test for `contextUsageIndicator` — the logic that can be
wrong (decode, reduce, fraction math) is entirely covered by the two tests
above; the view itself is a thin, stateless render of already-tested data,
consistent with how the rest of `ChatComposerView`'s pills are untested.

## Out of scope

- Codex/Pi/omp emission of `usage_update` was not verified against their
  source. If they don't send it, the ring simply never appears for those
  agents — no follow-up work required by this spec; a future spec can add
  a fallback signal for those agents specifically if it becomes a problem.
- No settings toggle to hide the ring — it's small, hover-only, and already
  conditionally hidden when no data exists.
