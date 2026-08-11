# Context ring click popover — design

## Problem

The context-window ring in the chat composer (`ComposerControlBar.contextUsageIndicator`,
`App/Chat/ComposerControlBar.swift:272`) only exposes usage via a hover `.help()` tooltip
(percent + used/size tokens). The user wants a click-to-open popover surfacing the full
picture: context usage, session cost, and a token breakdown, when the active driver reports
them.

## Investigation findings

Driver capability differs by agent, discovered via `tokensave_search`/`tokensave_callers_for`
over `Packages/TillerACP/Sources/TillerACP/Drivers/`:

| Driver | Context usage (used/size) | Cost ($) | Token breakdown |
|---|---|---|---|
| Claude (`ClaudeStreamJSONDriver`) | yes | yes — `ClaudeResult.totalCostUsd`, parsed but **dead** (`tokensave_callers_for` returned zero callers) | yes — `ClaudeResult.usage`: `inputTokens`, `outputTokens`, `cacheCreationInputTokens`, `cacheReadInputTokens` |
| Codex (`CodexAppServerDriver`) | yes | no | no |
| OpenCode (`OpenCodeHTTPDriver`) | yes | no | no |
| Pi/omp (`PiRPCDriver`) | no (ring stays empty, existing behavior) | no | no |

Cost and breakdown are Claude-only. The popover must render them conditionally and degrade to
the current used/size-only view for every other agent.

`ChatSessionStore.setContextUsage` (`Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift:134`)
persists only `.used`/`.size` as explicit columns — it does not serialize the whole
`ContextUsage` struct. New fields added to `ContextUsage` are therefore never persisted,
which matches the requirement that cost/breakdown reset every session (no DB migration
needed).

## Design

### 1. Data model

Extend `ContextUsage` (`Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift:23`) with
optional, defaulted fields:

```swift
public struct ContextUsage: Sendable, Equatable, Codable {
    public var used: Int
    public var size: Int
    public var costUsd: Double?
    public var inputTokens: Int?
    public var outputTokens: Int?
    public var cacheCreationTokens: Int?
    public var cacheReadTokens: Int?
    public init(used: Int, size: Int, costUsd: Double? = nil,
                inputTokens: Int? = nil, outputTokens: Int? = nil,
                cacheCreationTokens: Int? = nil, cacheReadTokens: Int? = nil) {
        self.used = used
        self.size = size
        self.costUsd = costUsd
        self.inputTokens = inputTokens
        self.outputTokens = outputTokens
        self.cacheCreationTokens = cacheCreationTokens
        self.cacheReadTokens = cacheReadTokens
    }
}
```

The generic ACP wire decoder (`SessionUpdate.init(from:)`, same file, `usage_update` case)
keeps decoding only `used`/`size` — it's the path Codex/OpenCode/Pi go through, and those
drivers never report the extra fields anyway.

### 2. Source: Claude driver only

`ClaudeStreamJSONDriver.contextUsage(from:)` (`Drivers/ClaudeStreamJSONDriver.swift:518`)
builds the `ContextUsage` it yields as `.usageUpdate`. Populate the new fields from the same
`ClaudeResult` it already receives:

- `costUsd` ← `result.totalCostUsd`
- `inputTokens` ← `result.usage?.inputTokens`
- `outputTokens` ← `result.usage?.outputTokens`
- `cacheCreationTokens` ← `result.usage?.cacheCreationInputTokens`
- `cacheReadTokens` ← `result.usage?.cacheReadInputTokens`

No other driver changes.

### 3. UI: popover on click

In `ComposerControlBar` (`App/Chat/ComposerControlBar.swift`), around the existing
`contextUsageIndicator` (line 272):

- Add `@State private var showContextPopover = false`.
- When `usage != nil`, wrap the ring in a tappable control (`Button` with `.plain` style, or
  `.onTapGesture` on the existing `ZStack`) that toggles `showContextPopover`. When `usage ==
  nil`, the ring stays inert — same as today (only the `.help()` "Context usage unavailable"
  tooltip applies).
- Attach `.popover(isPresented: $showContextPopover)` presenting a small vertical stack:
  - "`{percent}% of context used`" + "`{used} / {size} tokens`" (same numbers as today's
    tooltip).
  - "`Cost: ${costUsd, formatted to 2-4 decimals}`" — only if `usage.costUsd != nil`.
  - "`Input: {n} · Output: {n}`" and, if either is non-zero, "`Cache write: {n} · Cache
    read: {n}`" — only if `usage.inputTokens != nil` (breakdown is all-or-nothing since it
    always comes from the same Claude `ClaudeUsage` payload).
- Keep the existing `.help()` tooltip as-is for the hover case; it stays useful for agents
  where clicking does nothing (Codex/OpenCode/Pi show the same used/size-only content in the
  popover as they do in the tooltip today, so a click there is a strict upgrade over a no-op).

### 4. Tests

- `Packages/TillerACP/Tests/TillerACPTests` (driver-level): extend the existing
  `ClaudeStreamJSONDriver` result-handling tests to assert `contextUsage(from:)` maps
  `totalCostUsd` and the four `ClaudeUsage` token fields onto the new `ContextUsage` fields,
  and that they're `nil` when the source result omits them.
- `AppTests/ComposerControlBarTests.swift` (view-level, following the existing
  `contextRingUsesAgentAccentNormallyAndRedForWarning` pattern at line 103): assert the
  popover content includes the cost/breakdown rows when `ContextUsage` carries them, and omits
  those rows when it doesn't (Codex/OpenCode/Pi case).

## Out of scope

- Persisting cost/breakdown across restarts (explicitly rejected by the user — resets every
  session).
- Adding cost estimation for Codex/OpenCode/Pi (none of their wire protocols report it; no
  reliable way to compute it from tokens alone without hardcoding per-model pricing).
- Any change to the ACP generic `usage_update` decoder — it's unused by Claude (which builds
  `ContextUsage` directly in Swift) and correct as-is for the other three drivers.
