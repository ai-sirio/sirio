# Airy Graphite Dark Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retune Tiller's default dark neutral palette to the approved airy graphite values while preserving light mode, state colors, geometry, opacity, blur, terminal rendering, and package boundaries.

**Architecture:** `AppSurfaceColor` remains the cross-package source of truth for shared chrome, chat, and terminal RGB/hex values. `AppTheme` consumes those shared values and owns appearance-aware semantic roles for elevated surfaces and text, while targeted SwiftUI views use local `colorScheme`-aware `AnyShapeStyle` fallbacks rather than globally replacing `.quaternary`. `TillerTerminalTheme` continues consuming `AppSurfaceColor.terminalHex`, with terminal and chat remaining aliases.

**Tech Stack:** Swift 6, SwiftUI, AppKit, macOS 15+, TillerCore package tests using `swift-testing`, TillerTerminal/Ghostty theme tests, and the repository gate `Scripts/ci.sh`.

## Global Constraints

- dark chrome/sidebar `#1B1C1F` = RGB 27/28/31;
- dark chat/terminal `#28292C` = RGB 40/41/44; terminal remains aliases of chat;
- dark elevated card/control `#343539` = RGB 52/53/57;
- dark primary text `#F2F3F5` = RGB 242/243/245;
- dark secondary text `#A8ABB2` = RGB 168/171/178;
- user resolved text mapping: `title` and `titleSelected` both become primary; `subtitle` and `meta` both become secondary;
- user resolved code-block mapping: code `cardFill` dark becomes `#343539`; `chipFill` remains unchanged to preserve contrast.
- Every light branch/appearance, accents/status/Git/diff colors, terminal ANSI/afterglow, opacity `0.96`, Ghostty blur `20`, geometry/layout, state tokens (`rowHover`, selection, hairline, borders), and no palette preference/migration/error path remain unchanged.
- Hover, selection, hairline, and border tokens stay unchanged in this implementation. If manual validation finds a legibility problem in one of those tokens, completion is blocked and a separate explicit palette decision is requested; no unapproved neutral value is invented in this scope.
- Direct `.quaternary` fills are not globally replaced. They are changed only when the fill represents a targeted elevated card or control surface, so those components use the approved `#343539` semantic role.
- Do not introduce new AppTheme or App view-test infrastructure; update only the existing AppTheme mappings listed in Task 2.
- The implementation must preserve package boundaries: TillerCore owns shared values, TillerTerminal consumes them for Ghostty, and App maps them into appearance-aware SwiftUI colors. No unnamed view is included in this change.

---

## File structure and change map

### Modified by the implementation

- `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift:4-17` — update exact dark chrome/chat expectations; preserve light, equality, and opacity assertions.
- `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift:7-18` — add the explicit terminal hex assertion.
- `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift:9-12,27-30,45-51` — update dark shared constants/comments; keep terminal aliases and all light/opacity code intact.
- `Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift:5-6` — update the stale dark-color documentation only.
- `App/AppTheme.swift:18-39,76-95,125-130` — update stale shared-surface comments and only the approved dark semantic mappings.
- `App/Chat/ChatComposerView.swift:19,46-59,90-95` — use dark `AppTheme.cardFill` for the composer card and loading circle, preserving exact light fallbacks.
- `App/Chat/ComposerControlBar.swift:18,234-258,278-285` — use dark `AppTheme.cardFill` for disabled/loading controls and pills, preserving the context indicator stroke and accent/red branches.
- `App/Chat/TranscriptView.swift:11-16,207-217` — use dark `AppTheme.cardFill` for the user bubble only.
- `App/Chat/ComposerChipAttachmentView.swift:5-23` — use dark `AppTheme.cardFill` for the attachment chip only.
- `App/Chat/CodeBlockStyle.swift:18-22` — change only the dark code-card fill to exact `#343539`; preserve light `cardFill`, `chipFill`, and border.

### Inspected and intentionally unchanged

- `App/SidebarMaterialContainer.swift:7-23,43-59` — shared background/tint composition already consumes `AppTheme` and `AppSurfaceColor`.
- `App/ContentView.swift:13-100` — no palette-specific direct fill requires editing.
- `App/FloatingMainSurface.swift:3-41` — shared main surface already consumes `MainSurfaceMaterial` and unchanged border/shadow tokens.
- `App/Chat/ModelPickerPopover.swift:42-72` — `.quaternary.opacity(0.5)` is selected-row state and must remain unchanged.
- `App/AIProvidersSettingsView.swift` — passive metadata badge/status rendering is not a targeted elevated control and must remain unchanged.

The only file created by this documentation task is this plan. The implementation worker must modify only the files listed under “Modified by the implementation”; no product code, tests, the committed spec, or git state is changed while writing this plan.

## Task 1: Write failing shared palette tests

**Files:**
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift:4-17`
- Modify: `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift:7-18`
- Inspect only: `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift:9-55`

**Interfaces:**
- Consumes: existing public `AppSurfaceColor.red`, `green`, `blue`, `hex`, `chatRed`, `chatGreen`, `chatBlue`, `chatHex`, and `terminalHex` properties.
- Produces: failing exact-value coverage for the approved dark constants and the terminal hex contract that Task 2 must satisfy.

- [ ] **Step 1: Update the Core chrome and chat test expectations.** Replace only the two dark assertion groups with this compilable Swift:

```swift
@Test func chromeMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.red == 27.0 / 255.0)
    #expect(AppSurfaceColor.green == 28.0 / 255.0)
    #expect(AppSurfaceColor.blue == 31.0 / 255.0)
    #expect(AppSurfaceColor.hex == "1B1C1F")
    #expect(AppSurfaceColor.lightHex == "E9EAED")
}

@Test func chatSurfaceMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.chatRed == 40.0 / 255.0)
    #expect(AppSurfaceColor.chatGreen == 41.0 / 255.0)
    #expect(AppSurfaceColor.chatBlue == 44.0 / 255.0)
    #expect(AppSurfaceColor.chatHex == "28292C")
    #expect(AppSurfaceColor.chatLightHex == "F6F6F8")
}
```

- [ ] **Step 2: Add the explicit Terminal hex assertion without changing existing configuration assertions.** Insert this line immediately after the `let scrollbackLimit` declaration in `themeAppliesFontSizeToBothConfigurations`:

```swift
    #expect(AppSurfaceColor.terminalHex == "28292C")
```

- [ ] **Step 3: Run the focused Core tests and record the expected red result.**

Run:

```bash
swift test --package-path Packages/TillerCore --filter chromeMatchesApprovedDarkAndLightValues
swift test --package-path Packages/TillerCore --filter chatSurfaceMatchesApprovedDarkAndLightValues
```

Expected: both commands fail because the unchanged implementation still returns `08090A` and `101112` instead of `1B1C1F` and `28292C`; the unchanged light assertions remain part of each test.

- [ ] **Step 4: Run the focused Terminal test and record the expected red result.**

Run:

```bash
swift test --package-path Packages/TillerTerminal --filter themeAppliesFontSizeToBothConfigurations
```

Expected: FAIL at `#expect(AppSurfaceColor.terminalHex == "28292C")`, while the existing complete light/dark configuration assertions still execute against the old shared token.

- [ ] **Step 5: Stage no red-test commit; retain the test changes for Task 2.** Confirm with `git diff --check` that the test-only edits have no whitespace errors. Do not commit this task because the approved boundary includes these tests in Task 2's implementation commit.

## Task 2: Implement shared tokens and AppTheme dark semantic mappings

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift:9-12,27-30,45-51`
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift:5-6`
- Modify: `App/AppTheme.swift:18-39,76-95,125-130`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift:4-17`
- Test: `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift:7-18`

**Interfaces:**
- Consumes: the failing exact-value tests from Task 1.
- Produces: `AppSurfaceColor.hex == "1B1C1F"`, `AppSurfaceColor.chatHex == "28292C"`, `AppSurfaceColor.terminalHex == AppSurfaceColor.chatHex`, and AppTheme dark roles using the approved primary, secondary, and elevated values.

- [ ] **Step 1: Preflight the working tree before source edits.** Run the following commands before changing any source or test file:

```bash
git status --short
git diff -- App/Chat/TranscriptView.swift
```

Expected: inspect and preserve any pre-existing unstaged work in `.tiller/omp-hook.ts`, `App/Chat/ChatController.swift`, and `App/Chat/TranscriptView.swift`. Never stash, reset, clean, or discard these changes. Because `TranscriptView.swift` is a planned target, integrate only the one user-bubble fill edit surgically without overwriting existing modifications; if the local diff overlaps that exact `.background(.quaternary.opacity(0.7), in: RoundedRectangle(cornerRadius: 8))` expression, stop and request guidance before editing it.

- [ ] **Step 2: Change only the shared dark constants and comments.** Replace the relevant declarations in `AppSurfaceColor.swift` with this compilable Swift; do not alter lines 18-25, 36-43, or 66-78:

```swift
    /// Primary sidebar/chrome surface (#1B1C1F).
    public static let red: Double = 27.0 / 255.0
    public static let green: Double = 28.0 / 255.0
    public static let blue: Double = 31.0 / 255.0

    /// Central chat surface (#28292C).
    public static let chatRed: Double = 40.0 / 255.0
    public static let chatGreen: Double = 41.0 / 255.0
    public static let chatBlue: Double = 44.0 / 255.0

    /// Terminal pane surface (#28292C) — matches the chat surface above so
    /// terminal and chat panes are visually identical. Kept as its own named
    /// token because it's consumed by a different subsystem: Ghostty's theme
    /// config (`TillerTerminal`), not SwiftUI (`AppTheme`, App target).
    public static let terminalRed: Double = chatRed
    public static let terminalGreen: Double = chatGreen
    public static let terminalBlue: Double = chatBlue
```

- [ ] **Step 3: Update only the TillerTerminal stale documentation.** Replace the file header comment in `TillerTerminalTheme.swift` with this compilable Swift comment and leave `theme(fontSize:translucencyEnabled:)` behavior unchanged:

```swift
/// Terminal color theme matching the app's unified surface color family:
/// #28292C for dark mode and #F6F6F8 for light mode. Font size is injected
/// into both configurations.
```

- [ ] **Step 4: Replace only the approved dark AppTheme branches.** Preserve every light expression exactly. The complete affected declarations must be:

```swift
    /// Central surface for the terminal main pane (#28292C dark, #F6F6F8 light).
    /// It matches `chatSurface` so terminal and chat panes share one surface.
    static let terminalSurface = dynamic(
        light: NSColor(srgbRed: AppSurfaceColor.chatLightRed,
                       green: AppSurfaceColor.chatLightGreen,
                       blue: AppSurfaceColor.chatLightBlue,
                       alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.terminalRed,
                      green: AppSurfaceColor.terminalGreen,
                      blue: AppSurfaceColor.terminalBlue,
                      alpha: 1))
    /// Central surface for the chat main pane (#28292C dark, #F6F6F8 light).
    /// It matches `terminalSurface` so terminal and chat panes share one surface.
    static let chatSurface = dynamic(
        light: NSColor(srgbRed: AppSurfaceColor.chatLightRed,
                       green: AppSurfaceColor.chatLightGreen,
                       blue: AppSurfaceColor.chatLightBlue,
                       alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.chatRed,
                      green: AppSurfaceColor.chatGreen,
                      blue: AppSurfaceColor.chatBlue,
                      alpha: 1))

    static let title = dynamic(
        light: NSColor(srgbRed: 0.15, green: 0.16, blue: 0.20, alpha: 1),
        dark: NSColor(srgbRed: 242.0 / 255.0, green: 243.0 / 255.0, blue: 245.0 / 255.0, alpha: 1))
    static let titleSelected = dynamic(
        light: NSColor(srgbRed: 0.05, green: 0.05, blue: 0.08, alpha: 1),
        dark: NSColor(srgbRed: 242.0 / 255.0, green: 243.0 / 255.0, blue: 245.0 / 255.0, alpha: 1))
    static let subtitle = dynamic(
        light: NSColor(srgbRed: 0.35, green: 0.37, blue: 0.45, alpha: 1),
        dark: NSColor(srgbRed: 168.0 / 255.0, green: 171.0 / 255.0, blue: 178.0 / 255.0, alpha: 1))
    /// Light value is darker than a naive mirror of the dark one: `meta` is
    /// caption-sized, and the previous #6B7085 cleared WCAG AA by 0.04.
    static let meta = dynamic(
        light: NSColor(srgbRed: 0.38, green: 0.40, blue: 0.48, alpha: 1),
        dark: NSColor(srgbRed: 168.0 / 255.0, green: 171.0 / 255.0, blue: 178.0 / 255.0, alpha: 1))
    static let primaryPillBg = dynamic(
        light: NSColor(srgbRed: 0.88, green: 0.885, blue: 0.92, alpha: 1),
        dark: NSColor(srgbRed: 52.0 / 255.0, green: 53.0 / 255.0, blue: 57.0 / 255.0, alpha: 1))
    static let filterFieldBg = dynamic(
        light: .white,
        dark: NSColor(srgbRed: 52.0 / 255.0, green: 53.0 / 255.0, blue: 57.0 / 255.0, alpha: 1))
```

Also replace only the existing `AppTheme.cardFill` dark branch with `NSColor(srgbRed: 52.0 / 255.0, green: 53.0 / 255.0, blue: 57.0 / 255.0, alpha: 1)`; leave its light expression and all accent/status/diff/state declarations unchanged. Do not edit `App/Chat/CodeBlockStyle.swift` in this task; its dark `CodeBlockStyle.cardFill` edit is reserved for Task 3.

- [ ] **Step 5: Rerun the focused package tests and verify green.**

Run:

```bash
swift test --package-path Packages/TillerCore --filter chromeMatchesApprovedDarkAndLightValues
swift test --package-path Packages/TillerCore --filter chatSurfaceMatchesApprovedDarkAndLightValues
swift test --package-path Packages/TillerCore --filter terminalSurfaceMatchesChatSurfaceInBothAppearances
swift test --package-path Packages/TillerCore --filter sharedSurfaceOpacityIs096
swift test --package-path Packages/TillerTerminal --filter themeAppliesFontSizeToBothConfigurations
```

Expected: every command exits 0 with a passing test; the terminal test continues to assert the full light/dark configurations, opacity `0.96`, blur `20`, font size, and scrollback limit.

- [ ] **Step 6: Commit the shared-token implementation and Task 1 tests.**

Run:

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift App/AppTheme.swift
git commit -m "feat: retune dark surface palette"
```

Expected: one Conventional Commit is created containing only the shared palette, semantic AppTheme mappings, terminal comment, and exact-value tests.

## Task 3: Apply dark-only elevated fills to targeted chat and code views

**Files:**
- Modify: `App/Chat/ChatComposerView.swift:19,46-59,90-95`
- Modify: `App/Chat/ComposerControlBar.swift:18,234-258,278-285`
- Modify: `App/Chat/TranscriptView.swift:11-16,207-217`
- Modify: `App/Chat/ComposerChipAttachmentView.swift:5-23`
- Modify: `App/Chat/CodeBlockStyle.swift:18-22`
- Inspect only: `App/Chat/ModelPickerPopover.swift:42-72`, `App/AIProvidersSettingsView.swift`, `App/SidebarMaterialContainer.swift:7-23,43-59`, `App/ContentView.swift:13-100`, `App/FloatingMainSurface.swift:3-41`

**Interfaces:**
- Consumes: `AppTheme.cardFill` from Task 2 and SwiftUI `EnvironmentValues.colorScheme`.
- Produces: dark targeted fills using `#343539`, exact existing light `.quaternary` fills, unchanged state/selection/border/stroke behavior, and unchanged accent/red/status branches.

- [ ] **Step 1: Add appearance-aware styles to `ChatComposerView`.** Add the environment and these properties inside `ChatComposerView`, then replace only the two targeted backgrounds:

```swift
    @Environment(\.colorScheme) private var colorScheme

    private var cardBackgroundStyle: AnyShapeStyle {
        colorScheme == .dark
            ? AnyShapeStyle(AppTheme.cardFill)
            : AnyShapeStyle(Color.quaternary.opacity(0.4))
    }

    private var loadingBackgroundStyle: AnyShapeStyle {
        colorScheme == .dark
            ? AnyShapeStyle(AppTheme.cardFill)
            : AnyShapeStyle(Color.quaternary)
    }
```

Use `.background(cardBackgroundStyle, in: RoundedRectangle(cornerRadius: 14))` at the existing card background and `.background(loadingBackgroundStyle, in: Circle())` at the existing loading button. Leave regular-material popovers, selection fill, and the card border unchanged.

- [ ] **Step 2: Add appearance-aware styles to `ComposerControlBar` without changing the indicator stroke.** Add `@Environment(\.colorScheme) private var colorScheme` beside `reduceMotion`. Replace only the disabled send branch and loading button background with this compilable code:

```swift
    private var sendButton: some View {
        Button(action: onSend) {
            Text("Send")
                .font(.caption.weight(.semibold))
                .foregroundStyle(canSend ? Color.white : Color.secondary)
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(
                    canSend
                        ? AnyShapeStyle(Color.accentColor)
                        : colorScheme == .dark
                            ? AnyShapeStyle(AppTheme.cardFill)
                            : AnyShapeStyle(.quaternary),
                    in: RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.return, modifiers: [])
        .disabled(!canSend)
    }

    private var loadingButton: some View {
        ProgressView()
            .controlSize(.small)
            .padding(.horizontal, 10)
            .frame(height: 24)
            .background(
                colorScheme == .dark
                    ? AnyShapeStyle(AppTheme.cardFill)
                    : AnyShapeStyle(.quaternary),
                in: RoundedRectangle(cornerRadius: 6))
            .help("Starting the agent…")
    }
```

Do not change `Circle().stroke(.quaternary, lineWidth: 2)` at lines 205-212; it is a state/indicator stroke. Preserve the accent send branch and red stop branch exactly.

- [ ] **Step 3: Make `PillBackground` dark-only while preserving its exact light fallback.** Replace the private modifier with this compilable code:

```swift
private struct PillBackground: ViewModifier {
    @Environment(\.colorScheme) private var colorScheme

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 8).padding(.vertical, 4)
            .background(
                colorScheme == .dark
                    ? AnyShapeStyle(AppTheme.cardFill)
                    : AnyShapeStyle(Color.quaternary.opacity(0.6)),
                in: Capsule())
    }
}
```

- [ ] **Step 4: Change only the user bubble, attachment chip, and code card.** Apply these exact compilable snippets in their existing scopes:

```swift
// In TranscriptView, inside struct TranscriptView:
    @Environment(\.colorScheme) private var colorScheme

// Replace only the user bubble background:
        .background(
            colorScheme == .dark
                ? AnyShapeStyle(AppTheme.cardFill)
                : AnyShapeStyle(Color.quaternary.opacity(0.7)),
            in: RoundedRectangle(cornerRadius: 8))
```

```swift
// In ComposerChipView, inside struct ComposerChipView:
    @Environment(\.colorScheme) private var colorScheme

// Replace only the attachment chip background:
        .background(
            colorScheme == .dark
                ? AnyShapeStyle(AppTheme.cardFill)
                : AnyShapeStyle(Color.quaternary.opacity(0.8)),
            in: RoundedRectangle(cornerRadius: 5))
```

```swift
// In CodeBlockStyle.cardFill, preserve the light branch and change only dark:
    static let cardFill = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: 52.0 / 255.0,
                      green: 53.0 / 255.0,
                      blue: 57.0 / 255.0,
                      alpha: 1)
            : NSColor.black.withAlphaComponent(0.04)
    }
```

Leave the transcript divider/tertiary styles, chip separator border, code light `cardFill`, `chipFill`, and `cardBorder` exactly unchanged.

- [ ] **Step 5: Verify the excluded and shared consumer files remain untouched.** Confirm the implementation does not edit `ModelPickerPopover.swift` selected-row `.quaternary.opacity(0.5)`, `AIProvidersSettingsView.swift` passive metadata/status badge rendering, or the three shared consumer files. Confirm no unconditional `.quaternary` replacement was made.

- [ ] **Step 6: Run the repository gate and inspect the diff.**

Run:

```bash
Scripts/ci.sh
git diff --check
```

Expected: `Scripts/ci.sh` prints `CI OK`; `git diff --check` prints no errors; the diff contains only dark-only targeted fills and the exact light fallbacks shown above.

- [ ] **Step 7: Commit the targeted view changes.**

Run:

```bash
git add App/Chat/ChatComposerView.swift App/Chat/ComposerControlBar.swift App/Chat/TranscriptView.swift App/Chat/ComposerChipAttachmentView.swift App/Chat/CodeBlockStyle.swift
git commit -m "feat: apply elevated palette to chat controls"
```

Expected: one Conventional Commit is created containing only targeted elevated chat/control/code fills.

## Task 4: Final invariant, CI, and manual dark/light verification

**Files:**
- Inspect: all files listed in the change map and the committed spec `docs/superpowers/specs/2026-08-04-airy-graphite-dark-palette-design.md`
- Modify: none unless a verification correction is required; if a correction is required, do not claim completion until the focused tests, CI, and manual checklist are rerun.

**Interfaces:**
- Consumes: completed implementation and both Conventional Commits from Tasks 2 and 3.
- Produces: verified palette invariants and a clean completion decision; no new code or test infrastructure.

- [ ] **Step 1: Re-run the complete automated gate and whitespace check.**

Run:

```bash
Scripts/ci.sh
git diff --check
```

Expected: `Scripts/ci.sh` prints `CI OK`; `git diff --check` reports no whitespace errors.

- [ ] **Step 2: Confirm exact source invariants by reviewing the final diff.** Verify that the final diff shows `1B1C1F`, `28292C`, `343539`, `F2F3F5`, and `A8ABB2` only in the approved dark/shared mappings; terminal constants remain aliases of chat; light branches remain byte-for-byte unchanged where specified; opacity `0.96`, blur `20`, ANSI/afterglow, accents/status/Git/diff colors, geometry, hover, selection, hairline, and borders are unchanged.

- [ ] **Step 3: Perform the manual dark-appearance inspection in a built app.** Completion is blocked if any item is illegible or regressed; do not invent replacement values. Inspect:

  - sidebar and chrome render as `#1B1C1F`;
  - chat and terminal central surfaces render identically as `#28292C`;
  - composer card, loading circle, disabled/loading controls, mode/model pills, user bubble, attachment chip, and code-block card render as `#343539`;
  - primary title text and selected title text render as `#F2F3F5`;
  - subtitle and metadata text render as `#A8ABB2`;
  - hover, selection, hairline, and border tokens remain visible and unchanged;
  - accent, status, Git, diff, ANSI, and afterglow colors remain unchanged;
  - terminal translucency remains `0.96` and Ghostty blur remains `20`;
  - no global `.quaternary` regression appears in the model picker selected row or passive provider metadata.

- [ ] **Step 4: Perform the manual light-appearance inspection in a built app.** Confirm every preserved light fallback remains unchanged: sidebar/chrome, chat, terminal, composer card and loading circle, disabled/loading controls, pills, user bubble, attachment chip, code card, text roles, selection, hover, borders, status/accent colors, terminal ANSI/afterglow, opacity, blur, and geometry.

- [ ] **Step 5: Leave git state unchanged after verification.** Do not create another commit for a clean verification pass. If a correction was necessary, run the exact focused tests, `Scripts/ci.sh`, `git diff --check`, and the full dark/light checklist again before reporting completion.
