# Composer background/border restyle + per-agent accent colors

**Date:** 2026-08-09
**Status:** Approved

## Goal

Restyle `ChatComposerView` so the card blends into the surrounding chat
surface (matching the VS Code / Claude Code reference screenshot) instead of
using its own fixed dark fill, and give each supported agent (Claude Code,
Codex, OpenCode, omp, Pi) a distinct accent color that drives the composer's
focus border, the processing-turn animated border, and the send button — all
user-configurable in Settings.

Out of scope: transcript styling, card geometry/corner radius/width (already
set by the 2026-08-08 composer spec), the loading-spinner tint, the stop
button, the context-usage ring, `AgentIcon`'s existing (different) per-agent
color mapping used for icon badges elsewhere in the UI.

## Current behavior (baseline)

- `AppTheme.composerFill`: fixed `#20232D`, always dark regardless of app
  theme. Applied via `composerCardAppearance()`, which forces
  `environment(\.colorScheme, .dark)` on the whole card.
- `ComposerBorderView`: idle/focused border is `AppTheme.hairline` at 0.24
  (idle) / 0.45 (focused) opacity, 1pt, static. While `isPrompting`, it
  switches to a rotating conic gradient in `Color.accentColor`, 1.5pt.
- `ComposerControlBar.sendButton`: fill is `Color.accentColor` when
  `canSend`, else a neutral inactive fill.
- No per-agent color concept exists for the composer. `AgentIcon.color(for:)`
  exists but is a *different* mapping (used for icon-badge fallbacks
  elsewhere: claude=.orange, codex=.green, opencode=.blue, pi=.purple,
  omp=.teal) — this work does not touch or reuse that mapping.

## Visual contract

### Background

- Composer no longer forces a fixed dark appearance. Remove
  `composerCardAppearance()` / `AppTheme.ComposerAppearance` forcing from
  `ChatComposerView`'s `card`. The card follows the app's actual light/dark
  theme, same as the rest of the chat pane.
- Card fill becomes `AppTheme.chatSurface` (the same dynamic surface the
  transcript sits on), replacing `AppTheme.composerFill`. This is what makes
  the composer blend into the pane above it, per the reference screenshot.
- `ChatTextEditor` already sets `textView.textColor` /
  `insertionPointColor` from `NSColor.textColor` (semantic — resolves against
  the view's effective appearance), so no change is needed there: once the
  forced-dark environment is gone, it correctly follows the real theme.

### Border — idle

- `AppTheme.hairline`, full opacity (no 0.24 multiplier — today's value reads
  too faint against the new blended background; the reference screenshot
  shows a clearly visible hairline on all sides at rest).
- 1pt, `cornerRadius: 22` (unchanged).

### Border — focused, not prompting

- Color becomes the active agent's accent color (see below), at full
  opacity, static (no animation). Replaces today's opacity bump on the same
  neutral hairline.
- Still 1pt. Still animates the *transition* (color/opacity change) with the
  existing `.easeInOut(duration: 0.15)` tied to `document.isFocused`.

### Border — prompting (agent working a turn)

- Unchanged mechanism: the existing rotating conic gradient, 1.5pt, driven by
  `isPrompting`, gated on Reduce Motion exactly as today.
- Only the color changes: `Color.accentColor` → the active agent's accent
  color, at the same opacity stops in the gradient definition.
- This is independent of the text field's focus state, exactly as today
  (`isAnimating` is driven by `isPrompting` alone).

### Send button

- `ComposerControlBar.sendButton`: fill becomes the active agent's accent
  color when `canSend` is true (replacing `Color.accentColor`). Icon color
  stays white. Inactive-state fill (`Self.inactiveActionFill`) is unchanged.
- Loading spinner tint, stop button (red), context-usage ring: unchanged —
  not mentioned in the request, out of scope.

## Per-agent accent colors

Agent ids come from `AgentAdapter.id` (`AgentCatalog.all`): `claude`,
`codex`, `omp`, `opencode`, `pi`.

| id | agent | default hex |
|---|---|---|
| `claude` | Claude Code | `#D97757` |
| `codex` | Codex | `#0A84FF` |
| `omp` | Oh My Pi | `#9B4DFF` |
| `opencode` | OpenCode | `#FF9500` |
| `pi` | Pi | `#34C759` |

`claude` reuses the existing `AgentIcon.claudeOrange` value for consistency.
`omp` reuses the mid-stop purple already used in `AgentIcon`'s omp gradient.
The other three are macOS system blue/orange/green — sensible, unbranded
defaults, user-overridable.

All five are user-configurable in Settings (see below); these are only the
shipped defaults.

## Storage

- `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`: add
  ```swift
  public static func agentColorKey(for agentId: String) -> String {
      "appearance.agentColor.\(agentId)"
  }
  ```
  Mirrors the existing `summarizerAgentIdKey` pattern — TillerCore owns the
  key shape, stays catalog-agnostic (no `AgentCatalog` import), same as the
  summarizer-agent key today.
- New file `App/AgentAccentColor.swift`:
  - Default-hex table (above), keyed by agent id.
  - `static func color(for agentId: String) -> Color` — reads
    `UserDefaults.standard.string(forKey: AppSettings.agentColorKey(for:))`,
    falls back to the default hex for unknown/missing values, decodes via the
    existing `Color(hex:)` failable initializer (`App/SidebarView.swift`).
    Malformed stored hex (shouldn't happen via the picker, but defensively)
    falls back to the default color, not `.gray`.

## Settings UI

- `AppearanceSettingsView.swift`: new `Section("Agent Colors")`, one row per
  `AgentCatalog.all` entry: `AgentIcon` + `displayName` + `ColorPicker`
  bound to that agent's stored hex via a dynamic-key `@AppStorage` (the
  `AppStorage(wrappedValue:_:)` init form, so each row's binding targets its
  own `agentColorKey`). Live-updates open composers immediately, same as
  every other `@AppStorage`-backed appearance setting.
- No dedicated "reset to default" control — macOS's native `ColorPicker`
  already exposes its own swatches/eyedropper; a bespoke reset button isn't
  something the request asked for.

## Files touched

- `App/Chat/ChatComposerView.swift` — drop forced-dark appearance, swap card
  fill to `AppTheme.chatSurface`, `ComposerBorderView` gains
  `agentAccentColor: Color` param, idle border loses the opacity multiplier.
- `App/Chat/ComposerControlBar.swift` — `sendButton` gains
  `agentAccentColor: Color` param, used as its `canSend` fill.
- `App/AgentAccentColor.swift` — new.
- `App/AppearanceSettingsView.swift` — new "Agent Colors" section.
- `Packages/TillerCore/Sources/TillerCore/AppSettings.swift` — new
  `agentColorKey(for:)`.
- Tests: `AppTests/ComposerControlBarTests.swift` (send button fill color
  reflects agent), new coverage for `AgentAccentColor` (hex parsing,
  fallback-to-default on missing/malformed stored value), new coverage in
  `TillerCoreTests` for `agentColorKey(for:)`.

## Explicitly not changed

- `AgentIcon.swift` and its `color(for:)` mapping — different, pre-existing
  system used for icon badges; not unified with this feature.
- Composer geometry (corner radius, width, padding) — set by the prior
  2026-08-08 spec, untouched here.
- Loading spinner, stop button, context-usage ring — no accent-color change.
