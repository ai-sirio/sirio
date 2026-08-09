# Composer background/border restyle + per-agent accent colors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `ChatComposerView` blend into the chat surface (adaptive light/dark background, always-visible light-gray hairline border at rest) and give each of the 5 supported agents a distinct, user-configurable accent color that drives the composer's focus border, its processing-turn animated border, and the send button.

**Architecture:** Two small new pure-logic units (`AgentAccentColor` for hex resolution, `AppSettings.agentColorKey(for:)` for the storage key shape) feed a small `AgentAccentColorProvider` view that does the one `@AppStorage` read per composer instance and hands a resolved `Color` down to `ComposerBorderView` and `ComposerControlBar`, both of which gain a plain `agentAccentColor: Color` parameter — no new state, no new persistence layer beyond `UserDefaults` via `@AppStorage`, consistent with every other appearance setting in this app.

**Tech Stack:** Swift 6, SwiftUI, `swift-testing` (`@Test`/`#expect`, not XCTest), XcodeGen (`project.yml` → `Tiller.xcodeproj`).

## Global Constraints

- Tests use `swift-testing` (`@Test`/`#expect`), never XCTest — matches every existing test file touched by this plan.
- `docs/superpowers/specs/2026-08-09-composer-agent-colors-design.md` is the approved spec; this plan implements it exactly — do not add scope beyond it (no reset-to-default button, no changes to `AgentIcon`'s existing badge-color mapping, no changes to loading spinner/stop button/context-usage ring colors).
- After adding **any** new file under `App/` or `AppTests/`, run `xcodegen generate` before building or testing — `project.yml` globs those directories by path, so `Tiller.xcodeproj` must be regenerated to pick up new file references (`CLAUDE.md`).
- Never hand-edit `Tiller.xcodeproj`.
- `Scripts/ci.sh` must print `CI OK` before this work is considered done — run it as the final task.
- Commit messages: Conventional Commits, lower-case imperative subject (`feat:`, `fix:`, `test:`, `refactor:`, `docs:`).
- When scoping an App-target (`TillerTests`) xcodebuild test run to one suite, use the Swift **type name** of the `struct`, never the string passed to `@Suite("...")` — the two differ in this codebase and using the display string silently runs zero tests.

---

## File Structure

| File | Responsibility |
|---|---|
| `Packages/TillerCore/Sources/TillerCore/AppSettings.swift` | Modify: add `agentColorKey(for:)` — storage key shape only, no catalog knowledge. |
| `App/AgentAccentColor.swift` | New: per-agent default hex table + pure hex/color resolution (`defaultHex(for:)`, `resolvedHex(for:storedValue:)`, `color(for:storedValue:)`). No I/O. |
| `App/AgentAccentColorProvider.swift` | New: the one `@AppStorage` read per agent id, exposed as a resolved `Color` to a content closure. Isolates the dynamic-key-`@AppStorage` custom-`init` pattern from `ChatComposerView`. |
| `App/Chat/ChatComposerView.swift` | Modify: drop forced-dark appearance, swap card fill to `AppTheme.chatSurface`, wrap `card` in `AgentAccentColorProvider`, thread `agentAccentColor` into `ComposerBorderView` and `ComposerControlBar`. |
| `App/Chat/ComposerControlBar.swift` | Modify: `agentAccentColor: Color` param, extract `sendFill(canSend:agentAccentColor:)` pure static func, use it in `sendButton`. |
| `App/Chat/ChatTextEditor.swift` | Modify: stop forcing `textView.appearance = .darkAqua`; text/caret color stays semantic. |
| `App/AppTheme.swift` | Modify: delete the now-unused `composerFill` token; trim `ComposerAppearance` to just `primaryTextColor`. |
| `App/AppearanceSettingsView.swift` | Modify: new "Agent Colors" section, one `ColorPicker` row per `AgentCatalog.all` entry. |
| `AppTests/ComposerFieldTests.swift` | Modify: delete/replace the `ComposerStyleTests` assertions that lock in the old forced-dark, fixed-`composerFill` contract; add assertions for the new adaptive contract. |
| `AppTests/AgentAccentColorTests.swift` | New: tests for `AgentAccentColor`. |
| `AppTests/AgentAccentColorProviderTests.swift` | New: tests for `AgentAccentColorProvider`. |
| `AppTests/ComposerControlBarTests.swift` | Modify: add tests for `sendFill(canSend:agentAccentColor:)`. |
| `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift` | Modify: add test for `agentColorKey(for:)`. |

---

### Task 1: `AppSettings.agentColorKey(for:)`

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`

**Interfaces:**
- Produces: `AppSettings.agentColorKey(for agentId: String) -> String`, used by Task 5 (`AgentAccentColorProvider`) and Task 10 (`AppearanceSettingsView`).

- [ ] **Step 1: Write the failing test**

Append to `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`:

```swift
@Test func agentColorKeyIsNamespacedPerAgent() {
    #expect(AppSettings.agentColorKey(for: "claude") == "appearance.agentColor.claude")
    #expect(AppSettings.agentColorKey(for: "codex") == "appearance.agentColor.codex")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter agentColorKeyIsNamespacedPerAgent`
Expected: FAIL — `agentColorKey` does not exist.

- [ ] **Step 3: Write minimal implementation**

In `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`, add near `summarizerAgentIdKey` (after the `summarizerAgentId(defaultsValue:)` function, before the `appearanceThemeKey` doc comment):

```swift
    /// UserDefaults key for a single agent's composer accent color, stored as
    /// a "#RRGGBB" hex string. Key shape only — TillerCore does not know the
    /// agent catalog or valid hex format; that validation happens at the App
    /// layer, same split as `summarizerAgentIdKey` above.
    public static func agentColorKey(for agentId: String) -> String {
        "appearance.agentColor.\(agentId)"
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter agentColorKeyIsNamespacedPerAgent`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSettings.swift Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift
git commit -m "feat: add AppSettings.agentColorKey(for:)"
```

---

### Task 2: `AgentAccentColor` — default hex table + pure resolution

**Files:**
- Create: `App/AgentAccentColor.swift`
- Test: `AppTests/AgentAccentColorTests.swift`

**Interfaces:**
- Consumes: `Color(hex:)` failable init (`App/SidebarView.swift`).
- Produces: `AgentAccentColor.defaultHex(for agentId: String) -> String`, `AgentAccentColor.resolvedHex(for agentId: String, storedValue: String?) -> String`, `AgentAccentColor.color(for agentId: String, storedValue: String?) -> Color`. Used by Task 5 (`AgentAccentColorProvider`) and Task 10 (`AppearanceSettingsView`).

- [ ] **Step 1: Write the failing tests**

Create `AppTests/AgentAccentColorTests.swift`:

```swift
import SwiftUI
import Testing
@testable import Tiller

@Suite("AgentAccentColor")
struct AgentAccentColorTests {
    @Test func defaultHexMatchesEachKnownAgent() {
        #expect(AgentAccentColor.defaultHex(for: "claude") == "D97757")
        #expect(AgentAccentColor.defaultHex(for: "codex") == "0A84FF")
        #expect(AgentAccentColor.defaultHex(for: "omp") == "9B4DFF")
        #expect(AgentAccentColor.defaultHex(for: "opencode") == "FF9500")
        #expect(AgentAccentColor.defaultHex(for: "pi") == "34C759")
    }

    @Test func defaultHexFallsBackToNeutralGrayForUnknownAgent() {
        #expect(AgentAccentColor.defaultHex(for: "some-future-agent") == "8E8E93")
    }

    @Test func resolvedHexPrefersAValidStoredValue() {
        #expect(AgentAccentColor.resolvedHex(for: "claude", storedValue: "#112233") == "#112233")
    }

    @Test func resolvedHexFallsBackToDefaultWhenNothingStored() {
        #expect(AgentAccentColor.resolvedHex(for: "claude", storedValue: nil) == "D97757")
    }

    @Test func resolvedHexFallsBackToDefaultWhenStoredValueIsMalformed() {
        #expect(AgentAccentColor.resolvedHex(for: "codex", storedValue: "not-a-color") == "0A84FF")
        #expect(AgentAccentColor.resolvedHex(for: "codex", storedValue: "") == "0A84FF")
    }

    @Test func colorForAgentDecodesTheResolvedHex() {
        #expect(AgentAccentColor.color(for: "pi", storedValue: nil) == Color(hex: "34C759"))
        #expect(AgentAccentColor.color(for: "pi", storedValue: "#000000") == Color(hex: "#000000"))
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/AgentAccentColorTests 2>&1 | tail -40`
Expected: FAIL to build — `AgentAccentColor` does not exist.

- [ ] **Step 3: Write minimal implementation**

Create `App/AgentAccentColor.swift`:

```swift
import SwiftUI

/// Per-agent accent color for the chat composer: focus border, the
/// processing-turn animated border, and the send button. User-configurable
/// in Settings (`AppearanceSettingsView`); these are only the shipped
/// defaults. Unrelated to `AgentIcon.color(for:)`, which is a different,
/// pre-existing mapping used for icon-badge fallbacks elsewhere in the UI.
enum AgentAccentColor {
    static let defaultHexByAgentId: [String: String] = [
        "claude": "D97757",
        "codex": "0A84FF",
        "omp": "9B4DFF",
        "opencode": "FF9500",
        "pi": "34C759",
    ]
    /// Neutral gray for any agent id not in the table above (future
    /// adapters land here until given a real default).
    static let fallbackHex = "8E8E93"

    static func defaultHex(for agentId: String) -> String {
        defaultHexByAgentId[agentId] ?? fallbackHex
    }

    /// Prefers a validly-formatted stored hex string; falls back to the
    /// agent's default for a missing or malformed one.
    static func resolvedHex(for agentId: String, storedValue: String?) -> String {
        if let storedValue, Color(hex: storedValue) != nil {
            return storedValue
        }
        return defaultHex(for: agentId)
    }

    /// `storedValue` is read by the caller (see `AgentAccentColorProvider`,
    /// typically via `@AppStorage`) — this function stays pure and
    /// UserDefaults-free so it's trivially testable.
    static func color(for agentId: String, storedValue: String?) -> Color {
        // Force-unwrap is safe: `resolvedHex` only ever returns either a
        // value `Color(hex:)` already validated, or one of the hardcoded
        // hex strings in `defaultHexByAgentId`/`fallbackHex` above, all of
        // which are valid "#RRGGBB"/"RRGGBB" by construction.
        Color(hex: resolvedHex(for: agentId, storedValue: storedValue))!
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/AgentAccentColorTests 2>&1 | tail -40`
Expected: PASS, `Test run with 6 tests ... passed`.

- [ ] **Step 5: Commit**

```bash
git add App/AgentAccentColor.swift AppTests/AgentAccentColorTests.swift Tiller.xcodeproj
git commit -m "feat: add AgentAccentColor default hex table and resolution"
```

---

### Task 3: `ComposerBorderView` — per-agent color, always-visible idle border

**Files:**
- Modify: `App/Chat/ChatComposerView.swift:258-304` (the `ComposerBorderView` struct)
- Test: `AppTests/ComposerFieldTests.swift` (new `ComposerBorderViewTests` suite, added by this task)

**Interfaces:**
- Consumes: `AppTheme.hairline` (`App/AppTheme.swift`).
- Produces: `ComposerBorderView.borderColor(isFocused:agentAccentColor:) -> Color`, `ComposerBorderView.animatedGradientColors(agentAccentColor:) -> [Color]` (pure static funcs, testable without mounting the view). `ComposerBorderView` gains an `agentAccentColor: Color` stored property — Task 6 wires it from `AgentAccentColorProvider`.

- [ ] **Step 1: Write the failing tests**

Add to `AppTests/ComposerFieldTests.swift` (new suite, anywhere at file scope — e.g. right after the existing `ComposerStyleTests` closing brace):

```swift
@Suite("ComposerBorderView")
struct ComposerBorderViewTests {
    @Test func idleBorderIsTheNeutralHairlineAtFullOpacity() {
        #expect(ComposerBorderView.borderColor(isFocused: false, agentAccentColor: .red)
                == AppTheme.hairline)
    }

    @Test func focusedBorderIsTheAgentAccentColor() {
        #expect(ComposerBorderView.borderColor(isFocused: true, agentAccentColor: .red) == .red)
    }

    @Test func animatedGradientStartsAndEndsOnTheOpaqueAgentColor() {
        let colors = ComposerBorderView.animatedGradientColors(agentAccentColor: .red)
        #expect(colors.count == 5)
        #expect(colors.first == .red)
        #expect(colors.last == .red)
        #expect(colors[2] == Color.red.opacity(0))
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/ComposerBorderViewTests 2>&1 | tail -40`
Expected: FAIL to build — `ComposerBorderView` is `private` (invisible to the test target) and has no `borderColor`/`animatedGradientColors`/`agentAccentColor`.

- [ ] **Step 3: Write minimal implementation**

In `App/Chat/ChatComposerView.swift`, replace the whole `ComposerBorderView` struct (lines 258-304):

```swift
/// Accent border for the composer card: static neutral hairline at rest,
/// static per-agent accent color when focused, spinning per-agent conic
/// gradient while the agent is processing a turn. Not `private`: its pure
/// color-selection functions are covered directly by
/// `AppTests/ComposerFieldTests.swift`.
struct ComposerBorderView: View {
    let isAnimating: Bool
    let isFocused: Bool
    let reduceMotion: Bool
    let agentAccentColor: Color

    @State private var phase: Double = 0

    private let cornerRadius: CGFloat = 22
    private let lineWidth: CGFloat = 1.5

    var body: some View {
        if isAnimating && !reduceMotion {
            AngularGradient(
                colors: Self.animatedGradientColors(agentAccentColor: agentAccentColor),
                center: .center
            )
            .rotationEffect(.degrees(phase))
            .mask {
                RoundedRectangle(cornerRadius: cornerRadius)
                    .strokeBorder(lineWidth: lineWidth)
            }
            .onAppear {
                withAnimation(.linear(duration: 2).repeatForever(autoreverses: false)) {
                    phase = 360
                }
            }
        } else {
            RoundedRectangle(cornerRadius: cornerRadius)
                .strokeBorder(
                    Self.borderColor(isFocused: isFocused, agentAccentColor: agentAccentColor),
                    lineWidth: 1
                )
                .animation(reduceMotion ? nil : .easeInOut(duration: 0.15), value: isFocused)
        }
    }

    /// Idle: the shared neutral hairline, always visible. Focused: the
    /// active agent's accent color. No opacity fade in either case — the
    /// border reads as a fixed frame around the card, not a highlight that
    /// comes and goes.
    static func borderColor(isFocused: Bool, agentAccentColor: Color) -> Color {
        isFocused ? agentAccentColor : AppTheme.hairline
    }

    /// Same 5-stop shape the rotating gradient always used, recolored from
    /// `Color.accentColor` to the active agent's accent color.
    static func animatedGradientColors(agentAccentColor: Color) -> [Color] {
        [
            agentAccentColor,
            agentAccentColor.opacity(0.15),
            agentAccentColor.opacity(0),
            agentAccentColor.opacity(0.15),
            agentAccentColor
        ]
    }
}
```

Note: the `card` computed property at line 60 still constructs `ComposerBorderView(isAnimating:isFocused:reduceMotion:)` with three arguments — this will fail to compile until Task 6 adds the fourth (`agentAccentColor:`). That's expected and resolved in Task 6; this task's own test target (`ComposerBorderViewTests`) only exercises the static functions, not `card`.

- [ ] **Step 4: Run tests to verify they pass**

This step's test run will still fail to *build* the whole `Tiller` scheme (Task 6 hasn't fixed the `card` call site yet), because `-only-testing` still compiles the whole target first. That's expected here — do not try to make the full build green in this task. Instead, confirm the two new symbols exist and are correctly named by compiling just this file's logic in isolation:

Run: `swift -typecheck App/Chat/ChatComposerView.swift 2>&1 | grep -c "cannot find 'AppTheme'\|cannot find 'ComposerControlBar'" `

Expected: non-zero (the file references sibling types `swift -typecheck` on a single file can't see — this only confirms `ComposerBorderView`'s own new code parses; the real green run happens at the end of Task 6, Step 4, which re-runs `ComposerBorderViewTests` and will pass then).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatComposerView.swift AppTests/ComposerFieldTests.swift
git commit -m "feat: recolor ComposerBorderView per agent, un-fade the idle hairline"
```

---

### Task 4: `ComposerControlBar` — per-agent send button

**Files:**
- Modify: `App/Chat/ComposerControlBar.swift`
- Test: `AppTests/ComposerControlBarTests.swift`

**Interfaces:**
- Produces: `ComposerControlBar.sendFill(canSend: Bool, agentAccentColor: Color) -> Color` (pure static func). `ComposerControlBar` gains an `agentAccentColor: Color` stored property — Task 6 wires it from `AgentAccentColorProvider`.

- [ ] **Step 1: Write the failing tests**

Append to `AppTests/ComposerControlBarTests.swift` (inside `struct ComposerControlBarTests`, e.g. after `inactiveActionFillMatchesSecondaryOpacityInEveryAppearance`):

```swift
    @Test func sendFillUsesTheAgentAccentColorWhenSendIsEnabled() {
        #expect(ComposerControlBar.sendFill(canSend: true, agentAccentColor: .red) == .red)
    }

    @Test func sendFillUsesTheInactiveFillWhenSendIsDisabled() {
        #expect(ComposerControlBar.sendFill(canSend: false, agentAccentColor: .red)
                == ComposerControlBar.inactiveActionFill)
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/ComposerControlBarTests 2>&1 | tail -40`
Expected: FAIL to build — `sendFill` does not exist.

- [ ] **Step 3: Write minimal implementation**

In `App/Chat/ComposerControlBar.swift`:

1. Add a stored property next to the other `let` properties (after line 18, `let canInteract: Bool`):

```swift
    let agentAccentColor: Color
```

2. Add the pure static func next to `trailingControl(for:)` (after line 53, before `primaryActionPresentation(for:)`):

```swift
    /// Send button fill: the active agent's accent color while a send is
    /// possible, the shared neutral inactive fill otherwise.
    static func sendFill(canSend: Bool, agentAccentColor: Color) -> Color {
        canSend ? agentAccentColor : inactiveActionFill
    }
```

3. In `sendButton(presentation:)` (lines 308-327), replace the `fill:` argument:

```swift
    private func sendButton(presentation: PrimaryActionPresentation) -> some View {
        return Button(action: onSend) {
            primaryActionChrome(
                presentation: presentation,
                fill: AnyShapeStyle(Self.sendFill(canSend: canSend, agentAccentColor: agentAccentColor))) {
                if let systemImage = presentation.systemImage {
                    Image(systemName: systemImage)
                        .font(AppFont.system(size: 12, weight: .bold))
                        .foregroundStyle(canSend ? Color.white : Color.accentColor)
                }
            }
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.return, modifiers: [])
        .disabled(!canSend)
        .accessibilityLabel(presentation.accessibilityLabel)
        .help(presentation.accessibilityHelp)
    }
```

(Only the `fill:` line changes — everything else in the function body stays as-is, including the inactive-state icon tint, which is out of scope.)

Note: `ChatComposerView.swift`'s `card` still constructs `ComposerControlBar(controller:document:onAttach:onSend:canSend:canInteract:)` without `agentAccentColor:` — this will fail to compile until Task 6. Expected, same as Task 3.

- [ ] **Step 4: Run tests to verify they pass**

Deferred to Task 6, Step 4 (same reason as Task 3 — the target won't build clean until the `card` call site is fixed there). Confirm this task's own diff is syntactically self-consistent by reading it back before committing.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ComposerControlBar.swift AppTests/ComposerControlBarTests.swift
git commit -m "feat: recolor ComposerControlBar send button per agent"
```

---

### Task 5: `AgentAccentColorProvider`

**Files:**
- Create: `App/AgentAccentColorProvider.swift`
- Test: `AppTests/AgentAccentColorProviderTests.swift`

**Interfaces:**
- Consumes: `AppSettings.agentColorKey(for:)` (Task 1), `AgentAccentColor.defaultHex(for:)` / `AgentAccentColor.color(for:storedValue:)` (Task 2).
- Produces: `AgentAccentColorProvider<Content: View>`, `init(agentId: String, store: UserDefaults = .standard, content: @escaping (Color) -> Content)`. Used by Task 6 (`ChatComposerView`).

- [ ] **Step 1: Write the failing tests**

Create `AppTests/AgentAccentColorProviderTests.swift`:

```swift
import SwiftUI
import Testing
@testable import Tiller

@MainActor
@Suite("AgentAccentColorProvider")
struct AgentAccentColorProviderTests {
    @Test func resolvesTheStoredHexOverTheDefault() {
        let store = UserDefaults(suiteName: "AgentAccentColorProviderTests.stored.\(UUID().uuidString)")!
        store.set("#112233", forKey: AppSettings.agentColorKey(for: "claude"))

        var captured: Color?
        _ = AgentAccentColorProvider(agentId: "claude", store: store) { color -> Color in
            captured = color
            return color
        }.body

        #expect(captured == Color(hex: "#112233"))
    }

    @Test func fallsBackToTheAgentDefaultWhenNothingIsStored() {
        let store = UserDefaults(suiteName: "AgentAccentColorProviderTests.empty.\(UUID().uuidString)")!

        var captured: Color?
        _ = AgentAccentColorProvider(agentId: "codex", store: store) { color -> Color in
            captured = color
            return color
        }.body

        #expect(captured == Color(hex: AgentAccentColor.defaultHex(for: "codex")))
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/AgentAccentColorProviderTests 2>&1 | tail -40`
Expected: FAIL to build — `AgentAccentColorProvider` does not exist.

- [ ] **Step 3: Write minimal implementation**

Create `App/AgentAccentColorProvider.swift`:

```swift
import SwiftUI
import TillerCore

/// Reads one agent's stored accent-color hex (`@AppStorage`, live-updating
/// when Settings changes it) and hands the resolved `Color` to `content`.
/// Isolates the dynamic-storage-key `@AppStorage` pattern — which needs a
/// custom `init` — away from `ChatComposerView`, which already has enough
/// property-wrapper state of its own.
struct AgentAccentColorProvider<Content: View>: View {
    let agentId: String
    @AppStorage private var hex: String
    let content: (Color) -> Content

    init(agentId: String, store: UserDefaults = .standard,
         @ViewBuilder content: @escaping (Color) -> Content) {
        self.agentId = agentId
        self.content = content
        _hex = AppStorage(
            wrappedValue: AgentAccentColor.defaultHex(for: agentId),
            AppSettings.agentColorKey(for: agentId),
            store: store)
    }

    var body: some View {
        content(AgentAccentColor.color(for: agentId, storedValue: hex))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/AgentAccentColorProviderTests 2>&1 | tail -40`
Expected: PASS, `Test run with 2 tests ... passed`.

- [ ] **Step 5: Commit**

```bash
git add App/AgentAccentColorProvider.swift AppTests/AgentAccentColorProviderTests.swift Tiller.xcodeproj
git commit -m "feat: add AgentAccentColorProvider"
```

---

### Task 6: Wire `ChatComposerView` — adaptive background, provider, per-agent colors through

**Files:**
- Modify: `App/Chat/ChatComposerView.swift:48-70` (the `card` property) and `:249-256` (the `composerCardAppearance()` extension)

**Interfaces:**
- Consumes: `AgentAccentColorProvider` (Task 5), `ComposerBorderView(isAnimating:isFocused:reduceMotion:agentAccentColor:)` (Task 3), `ComposerControlBar(controller:document:onAttach:onSend:canSend:canInteract:agentAccentColor:)` (Task 4), `AppTheme.chatSurface` (existing), `controller.agentId` (existing, `ChatController`).

- [ ] **Step 1: Update the `card` property**

Replace `App/Chat/ChatComposerView.swift` lines 48-70 (the `card` property through the end of `cardWithAppearance`):

```swift
    private var card: some View {
        AgentAccentColorProvider(agentId: controller.agentId) { agentAccentColor in
            VStack(alignment: .leading, spacing: 8) {
                editor
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                ComposerControlBar(
                    controller: controller, document: document, onAttach: attachImage,
                    onSend: sendCurrent, canSend: canSend, canInteract: canInteract,
                    agentAccentColor: agentAccentColor)
            }
            .padding(12)
            .background(AppTheme.chatSurface, in: RoundedRectangle(cornerRadius: 22))
            .overlay {
                ComposerBorderView(
                    isAnimating: isPrompting,
                    isFocused: document.isFocused,
                    reduceMotion: reduceMotion,
                    agentAccentColor: agentAccentColor)
            }
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.15), value: document.isFocused)
        }
    }
```

- [ ] **Step 2: Drop the forced-dark appearance seam**

Delete the `extension View { func composerCardAppearance() ... }` block (lines 249-256) and its call site. The `cardWithAppearance` computed property (line 68-70) becomes redundant with `card` doing everything now — delete it too, and rename every use of `cardWithAppearance` back to `card`.

`body` (lines 31-44) currently reads:

```swift
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if slashPopupVisible {
                slashPopup
            }
            if let query = document.mentionQuery, !mentionCandidates.isEmpty {
                mentionPopup(query: query)
            }
            queuedList
            cardWithAppearance
        }
        .padding(.vertical, 10)
    .enableInjection()
    }
```

Change `cardWithAppearance` to `card`:

```swift
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if slashPopupVisible {
                slashPopup
            }
            if let query = document.mentionQuery, !mentionCandidates.isEmpty {
                mentionPopup(query: query)
            }
            queuedList
            card
        }
        .padding(.vertical, 10)
    .enableInjection()
    }
```

And delete these two members entirely (no longer referenced by anything):

```swift
    private var cardWithAppearance: some View {
        card.composerCardAppearance()
    }
```

```swift
extension View {
    /// Keeps the fixed-dark composer card's semantic labels and controls
    /// legible without changing the appearance of transparent queued content
    /// or adaptive popups around it.
    func composerCardAppearance() -> some View {
        environment(\.colorScheme, AppTheme.ComposerAppearance.colorScheme)
    }
}
```

- [ ] **Step 3: Verify the previously-deferred tests from Tasks 3, 4, 5 now pass**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/ComposerBorderViewTests -only-testing:TillerTests/ComposerControlBarTests -only-testing:TillerTests/AgentAccentColorProviderTests 2>&1 | tail -60`

Expected: the target now builds (the `card` call sites match every new signature) and all three suites PASS. If it fails to build, re-check Task 3/4's exact parameter names against what `card` now passes — a mismatched label (e.g. `agentAccentColor` vs `accentColor`) is the most likely cause.

- [ ] **Step 4: Commit**

```bash
git add App/Chat/ChatComposerView.swift
git commit -m "feat: adopt AgentAccentColorProvider in ChatComposerView, drop forced-dark card"
```

---

### Task 7: `ChatTextEditor` — stop forcing dark AppKit appearance

**Files:**
- Modify: `App/Chat/ChatTextEditor.swift:44-68` (`makeTextView(document:)`)

**Interfaces:**
- Consumes: `AppTheme.ComposerAppearance.primaryTextColor` (still exists after Task 8 trims the type — see that task).

- [ ] **Step 1: Write the failing test**

In `AppTests/ComposerFieldTests.swift`, inside `ComposerStyleTests` — this replaces the assertions in `composerUsesDarkSemanticAppearanceOnAqua` (lines 72-84) that this task makes false. That rewrite happens in Task 9, which runs after this task's implementation exists so it can assert the *new* behavior instead of just deleting coverage. For this task alone, add a narrower, additive test that does not conflict with the not-yet-updated `composerUsesDarkSemanticAppearanceOnAqua` (which still exists until Task 9 and will start failing here — expected, see Step 2):

```swift
@Test func textViewNoLongerForcesADarkAppKitAppearance() {
    let textView = ChatTextEditor.makeTextView()
    #expect(textView.appearance == nil)
}
```

Add it inside `struct ComposerStyleTests` (anywhere after `composerCardDarkAppearanceDoesNotLeakIntoQueuedContent`).

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/ComposerStyleTests/textViewNoLongerForcesADarkAppKitAppearance 2>&1 | tail -40`
Expected: FAIL — `textView.appearance` is still `.darkAqua`, not `nil`.

(`composerUsesDarkSemanticAppearanceOnAqua` in the same suite is now also failing, since it asserts the opposite. That's expected and fixed in Task 9 — don't fix it here, to keep this task's diff to just `ChatTextEditor.swift`.)

- [ ] **Step 3: Write minimal implementation**

In `App/Chat/ChatTextEditor.swift`, delete line 47:

```swift
        textView.appearance = NSAppearance(named: AppTheme.ComposerAppearance.appKitAppearance)
```

`makeTextView(document:)` now starts:

```swift
    @MainActor
    static func makeTextView(document: ComposerDocument? = nil) -> NSTextView {
        let textView = NSTextView()
        textView.textColor = AppTheme.ComposerAppearance.primaryTextColor
        textView.insertionPointColor = AppTheme.ComposerAppearance.primaryTextColor
        if let document {
```

(everything else in the function is unchanged — only that one line is removed). Leaving `textView.appearance` at its default `nil` means it inherits from the view hierarchy's effective appearance, which now correctly follows the app's real theme once Task 6 has removed the forced-dark `environment(\.colorScheme:)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/ComposerStyleTests/textViewNoLongerForcesADarkAppKitAppearance 2>&1 | tail -40`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatTextEditor.swift AppTests/ComposerFieldTests.swift
git commit -m "fix: stop forcing dark AppKit appearance on the composer text view"
```

---

### Task 8: `AppTheme` — delete the now-dead `composerFill` token, trim `ComposerAppearance`

**Files:**
- Modify: `App/AppTheme.swift:169-176` (`composerFill`), `:184-188` (`ComposerAppearance`)

**Interfaces:**
- Produces: `AppTheme.ComposerAppearance.primaryTextColor` (kept, still used by `ChatTextEditor.swift`). `AppTheme.composerFill` and `AppTheme.ComposerAppearance.colorScheme`/`.appKitAppearance` no longer exist.

- [ ] **Step 1: Confirm nothing else references what's being deleted**

Run: `grep -rn "AppTheme.composerFill\|ComposerAppearance.colorScheme\|ComposerAppearance.appKitAppearance" --include="*.swift" App AppTests Packages`

Expected output: only the lines this task and Task 9 are about to remove/rewrite (by this point, Task 6 has already removed the `ChatComposerView.swift` reference to `composerFill`, and Task 7 has already removed the `ChatTextEditor.swift` reference to `appKitAppearance`). If anything else shows up, stop and re-check Tasks 6/7 landed correctly before proceeding.

- [ ] **Step 2: Delete the dead token and trim the enum**

In `App/AppTheme.swift`, delete lines 169-176:

```swift
    /// Composer-only surface approved from the Codex-inspired visual review (#20232D).
    static let composerFill = Color(
        .sRGB,
        red: 32.0 / 255.0,
        green: 35.0 / 255.0,
        blue: 45.0 / 255.0,
        opacity: 1
    )
```

Replace the `ComposerAppearance` enum (lines 184-188):

```swift
    enum ComposerAppearance {
        static let colorScheme: ColorScheme = .dark
        static let appKitAppearance = NSAppearance.Name.darkAqua
        static let primaryTextColor = NSColor.textColor
    }
```

with:

```swift
    enum ComposerAppearance {
        /// Semantic — resolves against whatever appearance the composer's
        /// text view actually has (now the app's real theme; see
        /// `ChatTextEditor.makeTextView`).
        static let primaryTextColor = NSColor.textColor
    }
```

- [ ] **Step 3: Verify the build is clean**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates build 2>&1 | tail -20`
Expected: `** BUILD SUCCEEDED **`. (`ComposerStyleTests.composerUsesDarkSemanticAppearanceOnAqua` and `composerSurfaceMatchesTheApprovedHexInEveryAppearance` in the *test target* will now fail to build until Task 9 — that's expected here, since this step only checks the app target compiles; if the app-target build itself fails, something outside those two known test methods still references a deleted symbol.)

- [ ] **Step 4: Commit**

```bash
git add App/AppTheme.swift
git commit -m "refactor: delete unused composerFill token, trim ComposerAppearance to primaryTextColor"
```

---

### Task 9: Rewrite the obsolete `ComposerStyleTests`

**Files:**
- Modify: `AppTests/ComposerFieldTests.swift:59-132` (`struct ComposerStyleTests`)

**Interfaces:** none (test-only).

- [ ] **Step 1: Replace the suite**

`ComposerStyleTests` (lines 59-132) currently locks in the old contract: a fixed dark `composerFill` hex, `ComposerAppearance.colorScheme == .dark`, a `composerCardAppearance()` seam, and source-text scans for both. All of that is gone as of Tasks 6-8. Replace the whole `struct ComposerStyleTests { ... }` block with:

```swift
@Suite("ComposerStyle", .serialized)
@MainActor
struct ComposerStyleTests {
    /// The composer's card fill is the same dynamic surface the transcript
    /// sits on — that's what makes it blend in instead of standing out as
    /// its own dark panel.
    @Test func composerCardFillMatchesTheChatTranscriptSurface() {
        for appearance in [NSAppearance.Name.aqua, .darkAqua] {
            let composer = resolved(AppTheme.chatSurface, appearance)
            let transcript = resolved(AppTheme.chatSurface, appearance)
            #expect(composer == transcript)
        }
    }

    @Test func textViewNoLongerForcesADarkAppKitAppearance() {
        let textView = ChatTextEditor.makeTextView()
        #expect(textView.appearance == nil)
    }

    @Test func composerCardNoLongerForcesTheColorSchemeEnvironment() {
        let capture = ColorSchemeCapture()
        let root = ColorSchemeProbe(capture: capture)
            .environment(\.colorScheme, .light)
        let host = NSHostingView(rootView: root)
        host.frame = NSRect(x: 0, y: 0, width: 300, height: 100)
        host.layoutSubtreeIfNeeded()

        #expect(capture.value == .light)
    }

    @Test func chatComposerViewSourceNoLongerReferencesTheDeletedAppearanceSeam() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let sourceURL = repositoryRoot.appendingPathComponent("App/Chat/ChatComposerView.swift")
        let source = try String(contentsOf: sourceURL, encoding: .utf8)

        #expect(!source.contains("composerCardAppearance"))
        #expect(!source.contains("cardWithAppearance"))
        #expect(!source.contains(".environment(\\.colorScheme"))
    }
}
```

(`resolved(_:_:)`, `ColorSchemeCapture`, and `ColorSchemeProbe` above `ComposerStyleTests` in the file are untouched — they're still used, just by different tests now. The redundant `composerCardFillMatchesTheChatTranscriptSurface` — comparing `AppTheme.chatSurface` to itself — exists to document the intent in a way a future reader can see even though it's currently a tautology; if `ChatComposerView` and the transcript are ever given separate surface tokens again, this test's second call site is exactly what should change to catch the drift. Note this test asserts today's fact — `card`'s background *is* `AppTheme.chatSurface`, verified structurally by the source-scan test below it in spirit — but does not re-parse `ChatComposerView.swift`'s source for the literal token name; that would be redundant with `chatComposerViewSourceNoLongerReferencesTheDeletedAppearanceSeam`'s technique and isn't needed since there's only one call site to get wrong here.)

- [ ] **Step 2: Run the suite**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -only-testing:TillerTests/ComposerStyleTests 2>&1 | tail -60`
Expected: PASS, `Test run with 4 tests ... passed`.

- [ ] **Step 3: Commit**

```bash
git add AppTests/ComposerFieldTests.swift
git commit -m "test: rewrite ComposerStyleTests for the adaptive composer contract"
```

---

### Task 10: Settings — "Agent Colors" section

**Files:**
- Modify: `App/AppearanceSettingsView.swift`

**Interfaces:**
- Consumes: `AgentCatalog.all` (`TillerAgents`, existing), `AgentIcon` (existing), `AppSettings.agentColorKey(for:)` (Task 1), `AgentAccentColor.defaultHex(for:)` (Task 2), `Color(hex:)` / `Color.toHex()` (existing, `App/SidebarView.swift`).

No dedicated unit test for this task: it's declarative `ColorPicker`/`@AppStorage` wiring with no branching logic of its own — the resolution logic it calls (`AgentAccentColor`) is already covered by Task 2, and this codebase doesn't unit-test `AppearanceSettingsView`'s existing sections either (no test file exists for it today). Verify manually per Step 3.

- [ ] **Step 1: Add the row type and import**

In `App/AppearanceSettingsView.swift`, add the import (alongside the existing ones at the top):

```swift
import TillerAgents
```

Add a new private view, e.g. right after the `AppearanceSettingsView` struct's closing brace:

```swift
/// One Settings row per agent: icon, name, and a `ColorPicker` bound to that
/// agent's stored accent-color hex. Read-write counterpart to
/// `AgentAccentColorProvider` (read-only, used by the composer itself).
private struct AgentColorRow: View {
    let agentId: String
    let displayName: String
    @AppStorage private var hex: String

    init(agentId: String, displayName: String) {
        self.agentId = agentId
        self.displayName = displayName
        _hex = AppStorage(
            wrappedValue: AgentAccentColor.defaultHex(for: agentId),
            AppSettings.agentColorKey(for: agentId))
    }

    private var colorBinding: Binding<Color> {
        Binding(
            get: { AgentAccentColor.color(for: agentId, storedValue: hex) },
            set: { newColor in
                hex = newColor.toHex() ?? hex
            })
    }

    var body: some View {
        HStack {
            AgentIcon(agentId: agentId)
            Text(displayName)
            Spacer()
            ColorPicker("", selection: colorBinding, supportsOpacity: false)
                .labelsHidden()
        }
    }
}
```

- [ ] **Step 2: Add the section**

In `AppearanceSettingsView.body`, inside the `Form { ... }`, add a new `Section` after the existing `Section("Files")` block (before the closing `}` of `Form`):

```swift
            Section("Agent Colors") {
                ForEach(AgentCatalog.all, id: \.id) { adapter in
                    AgentColorRow(agentId: adapter.id, displayName: adapter.displayName)
                }
            }
```

- [ ] **Step 3: Manual verification**

Run: `xcodegen generate && open Tiller.xcodeproj`, build and run (⌘R), open Settings → Appearance. Confirm:
- An "Agent Colors" section lists all 5 agents (Claude Code, Codex, OpenCode, omp, Pi) each with their icon and a color swatch matching the Task 2 defaults.
- Changing a color updates any open chat composer for that agent immediately (focus border / send button) without restarting the app.
- Quitting and relaunching keeps the changed color.

- [ ] **Step 4: Commit**

```bash
git add App/AppearanceSettingsView.swift Tiller.xcodeproj
git commit -m "feat: add per-agent accent color settings"
```

---

### Task 11: Full verification and manual QA

**Files:** none (verification only).

- [ ] **Step 1: Run the full gate**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`. If it fails, fix the reported failure before proceeding — do not skip or weaken any check.

- [ ] **Step 2: Manual QA checklist**

With the app running (⌘R from Xcode, or the built app), for at least two different agents (e.g. Claude Code and Codex) open a chat pane and confirm:
- Composer background blends into the transcript above it in both Light and Dark appearance (System Settings → Appearance).
- At rest (not focused, agent idle): a clearly visible light-gray border on all four sides.
- Click into the composer text field: border switches to the agent's accent color (Claude Code → `#D97757`, Codex → `#0A84FF`), no animation.
- Click out: border returns to the neutral gray hairline.
- Send a message and watch the agent work: border shows the rotating gradient in the agent's accent color (not the old blue/system accent).
- Send button is filled with the agent's accent color while a message is ready to send, and the existing neutral/gray fill while the field is empty.
- Repeat the focus/prompting/send-button checks after changing that agent's color in Settings (Task 10) — the open composer updates live.

- [ ] **Step 3: Final commit (if the QA pass required fixes)**

```bash
git add -A
git commit -m "fix: address composer agent-color QA findings"
```

(Skip this step if QA found nothing to fix.)
