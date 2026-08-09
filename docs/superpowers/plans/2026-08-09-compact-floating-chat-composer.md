# Compact Floating Chat Composer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the approved composer compact and `#20232D`, align transcript content to the same responsive column, and float the bottom UI over messages with a measured Codex-like fade.

**Architecture:** Keep width and bottom-overlay geometry as pure metrics in `ComposerLayout.swift`, then have `TranscriptView` consume the shared width and a caller-provided bottom inset. `ChatPaneView` owns the full-height transcript/overlay composition, measures the interactive bottom stack through a preference, draws a non-interactive gradient behind it, and feeds the measured inset back into the transcript without observing live scroll geometry.

**Tech Stack:** Swift 6, SwiftUI `Layout`, `PreferenceKey`, Swift Testing, AppKit `NSHostingView`, XcodeGen, macOS 15+

---

## File Structure

- `App/AppTheme.swift` — add the fixed composer-only `#20232D` surface token and remove the obsolete inset-field token.
- `App/Chat/ComposerLayout.swift` — retain shared responsive width logic and add compact/overlay geometry constants plus pure calculations.
- `App/Chat/ChatComposerView.swift` — consume the compact metrics and composer surface; remove outer horizontal padding so the visible card uses the shared column width.
- `App/Chat/TranscriptView.swift` — center the lazy transcript content with the shared layout and append the caller-provided bottom scroll clearance.
- `App/Chat/ChatPaneView.swift` — compose transcript, fade, banners, permission UI, and composer as a measured bottom overlay.
- `AppTests/ComposerLayoutMetricsTests.swift` — cover compact heights, fade extension, clearance, and clamping.
- `AppTests/ComposerFieldTests.swift` — replace stale inset-field assertions with exact composer-surface color assertions.
- `AppTests/ChatControllerTests.swift` — verify the real chat-pane geometry at 640 and 2000 points.
- `AppTests/TranscriptViewLayoutRegressionTests.swift` — retain the guard against live scroll-geometry feedback.

`Tiller.xcodeproj` is generated. Run `xcodegen generate` before focused Xcode tests, never hand-edit or commit the generated project, and stage only the exact source/test files named by each task.

The user has explicitly prohibited `Scripts/ci.sh`. Do not run it and do not replace it with an equivalent all-repository gate.

---

### Task 1: Compact composer metrics and fixed surface

**Files:**
- Modify: `App/Chat/ComposerLayout.swift`
- Modify: `App/AppTheme.swift`
- Modify: `App/Chat/ChatComposerView.swift`
- Modify: `AppTests/ComposerLayoutMetricsTests.swift`
- Modify: `AppTests/ComposerFieldTests.swift`

- [ ] **Step 1: Add failing compact-metric tests**

Append these assertions to `ComposerLayoutMetricsTests`:

```swift
@Test func compactComposerHeightsMatchTheApprovedDesign() {
    #expect(ComposerLayoutMetrics.editorMinimumHeight == 56)
    #expect(ComposerLayoutMetrics.editorMaximumHeight == 128)
}
```

- [ ] **Step 2: Replace obsolete field-color tests with the failing composer-surface contract**

Keep the existing deterministic `resolved(_:_: )` helper in `ComposerFieldTests.swift`, remove the two `composerFieldFill` tests, and replace them with:

```swift
@Suite("ComposerStyle", .serialized)
@MainActor
struct ComposerStyleTests {
    @Test func composerSurfaceMatchesTheApprovedHexInEveryAppearance() {
        for appearance in [NSAppearance.Name.aqua, .darkAqua] {
            let surface = resolved(AppTheme.composerFill, appearance)
            #expect(abs(surface.redComponent - (32.0 / 255.0)) < 0.0001)
            #expect(abs(surface.greenComponent - (35.0 / 255.0)) < 0.0001)
            #expect(abs(surface.blueComponent - (45.0 / 255.0)) < 0.0001)
            #expect(abs(surface.alphaComponent - 1.0) < 0.0001)
        }
    }
}
```

- [ ] **Step 3: Regenerate the project and verify both RED failures**

Run the two selectors in separate commands to avoid the app-hosted multi-test runner issue:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerLayoutMetricsTests

xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerStyleTests
```

Expected: the first command fails because `editorMinimumHeight` / `editorMaximumHeight` do not exist; the second fails because `AppTheme.composerFill` does not exist. An exit caused only by a selector typo is not a valid RED.

- [ ] **Step 4: Add the compact metrics**

Add these constants beside the width constants in `ComposerLayoutMetrics`:

```swift
static let editorMinimumHeight: CGFloat = 56
static let editorMaximumHeight: CGFloat = 128
```

- [ ] **Step 5: Add the composer-only color token and remove the obsolete token**

In `AppTheme`, add a fixed, non-dynamic token near `cardFill`:

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

Remove `composerFieldFill`; after the unified-card change it has no production call site and its old tests are being replaced by the fixed-surface contract.

- [ ] **Step 6: Apply the compact geometry and exact color to the real composer**

In `ChatComposerView`:

```swift
// body: preserve vertical clearance but let the visible card fill the shared column.
.padding(.vertical, 10)
```

Replace the card fill and editor heights:

```swift
.background(AppTheme.composerFill, in: RoundedRectangle(cornerRadius: 22))
```

```swift
ChatTextEditor(
    document: document,
    isEditable: canInteract,
    minHeight: ComposerLayoutMetrics.editorMinimumHeight,
    maxHeight: ComposerLayoutMetrics.editorMaximumHeight,
    onSubmit: sendCurrent,
    onSlashKey: handleSlashKey
)
```

Do not modify the blue ring, processing animation, editor padding, control order, 30-point action footprint, chips, slash commands, mentions, or drag/drop behavior.

- [ ] **Step 7: Verify GREEN**

Re-run the two focused commands from Step 3 separately.

Expected: `ComposerLayoutMetricsTests` passes all existing width tests plus the compact-height test; `ComposerStyleTests` passes with one test and exact sRGB components in aqua and darkAqua. Treat existing dependency SwiftLint messages separately from the command exit and `TEST SUCCEEDED` result.

- [ ] **Step 8: Commit only Task 1 files**

```bash
git add App/AppTheme.swift \
  App/Chat/ComposerLayout.swift \
  App/Chat/ChatComposerView.swift \
  AppTests/ComposerLayoutMetricsTests.swift \
  AppTests/ComposerFieldTests.swift
git diff --cached --check
git commit -m "feat: compact chat composer surface"
```

---

### Task 2: Shared transcript column and overlay metrics

**Files:**
- Modify: `App/Chat/ComposerLayout.swift`
- Modify: `App/Chat/TranscriptView.swift`
- Modify: `AppTests/ComposerLayoutMetricsTests.swift`
- Modify: `AppTests/TranscriptViewLayoutRegressionTests.swift`

- [ ] **Step 1: Add failing bottom-overlay metric tests**

Add to `ComposerLayoutMetricsTests`:

```swift
@Test func bottomOverlayMetricsMatchTheApprovedFadeAndClearance() {
    #expect(ChatBottomOverlayMetrics.fadeExtension == 36)
    #expect(ChatBottomOverlayMetrics.bottomClearance == 16)
    #expect(ChatBottomOverlayMetrics.fadeHeight(for: 100) == 136)
    #expect(ChatBottomOverlayMetrics.contentInset(for: 100) == 116)
}

@Test func bottomOverlayMetricsClampNegativeMeasurements() {
    #expect(ChatBottomOverlayMetrics.fadeHeight(for: -20) == 36)
    #expect(ChatBottomOverlayMetrics.contentInset(for: -20) == 16)
}
```

- [ ] **Step 2: Run the metric test to verify RED**

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerLayoutMetricsTests
```

Expected: compile failure because `ChatBottomOverlayMetrics` does not exist.

- [ ] **Step 3: Add pure overlay geometry and the height preference**

Append to `ComposerLayout.swift`:

```swift
struct ChatBottomOverlayMetrics {
    static let fadeExtension: CGFloat = 36
    static let bottomClearance: CGFloat = 16

    static func fadeHeight(for overlayHeight: CGFloat) -> CGFloat {
        max(0, overlayHeight) + fadeExtension
    }

    static func contentInset(for overlayHeight: CGFloat) -> CGFloat {
        max(0, overlayHeight) + bottomClearance
    }
}

struct ChatBottomOverlayHeightPreferenceKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

extension View {
    func captureChatBottomOverlayHeight() -> some View {
        background {
            GeometryReader { proxy in
                Color.clear.preference(
                    key: ChatBottomOverlayHeightPreferenceKey.self,
                    value: proxy.size.height
                )
            }
        }
    }
}
```

- [ ] **Step 4: Add a caller-provided bottom inset and shared centering to TranscriptView**

Add the defaulted view input so current call sites keep compiling until Task 3. Keep it as a `var`: Swift's synthesized memberwise initializer includes a defaulted `var`, while a defaulted `let` would not expose the argument that Task 3 passes explicitly.

```swift
var bottomContentInset: CGFloat = 0
```

Replace the direct lazy stack inside the `ScrollView` with the shared layout:

```swift
ScrollView {
    CenteredComposerLayout {
        LazyVStack(alignment: .leading, spacing: 0) {
            ForEach(grouped.roots) { item in
                itemView(item, meta: nil, grouped: grouped, snapshot: snapshot)
                    .padding(.top, Self.topSpacing(for: item))
                    .id(item.id)
            }
            if controller.state == .prompting {
                thinkingRow.padding(.top, 14)
            }
            Color.clear
                .frame(height: max(1, bottomContentInset))
                .id("bottom")
        }
        .padding(.vertical, 14)
    }
}
```

Remove the old `.padding(.horizontal, 16)`. The full-width scroll view remains unchanged; only its lazy content receives the 84% / 1440 / 16 shared column.

- [ ] **Step 5: Preserve the no-live-scroll-geometry regression guard**

Extend `TranscriptViewLayoutRegressionTests` with a second source-level contract:

```swift
@Test("transcript bottom clearance does not observe live scroll geometry")
func bottomClearanceAvoidsScrollGeometryFeedback() throws {
    let repositoryRoot = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
    let source = try String(
        contentsOf: repositoryRoot.appendingPathComponent("App/Chat/TranscriptView.swift"),
        encoding: .utf8
    )

    #expect(source.contains("bottomContentInset"))
    #expect(!source.contains(".onScrollGeometryChange("))
}
```

- [ ] **Step 6: Verify GREEN**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerLayoutMetricsTests

xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/TranscriptViewLayoutRegressionTests
```

Expected: both focused suites pass; the transcript guard still proves that no live scroll-geometry callback was introduced.

- [ ] **Step 7: Commit only Task 2 files**

```bash
git add App/Chat/ComposerLayout.swift \
  App/Chat/TranscriptView.swift \
  AppTests/ComposerLayoutMetricsTests.swift \
  AppTests/TranscriptViewLayoutRegressionTests.swift
git diff --cached --check
git commit -m "feat: align transcript with composer column"
```

---

### Task 3: Floating bottom overlay and soft fade

**Files:**
- Modify: `App/Chat/ChatPaneView.swift`
- Modify: `App/Chat/TranscriptView.swift`
- Modify: `AppTests/ChatControllerTests.swift`

- [ ] **Step 1: Extend the layout probe with failing roles and assertions**

Add these cases to `ChatPaneLayoutRole` in the test compilation path:

```swift
case transcriptViewport
case transcriptContent
case transcriptBottomSpacer
case bottomFade
case bottomOverlay
```

First update `pendingPermissionRendersAboveComposerWhilePromptRemainsOpen()` so its wait loop requires every frame:

```swift
if capture.frames[.approvalPanel] != nil,
   capture.frames[.composer] != nil,
   capture.frames[.transcriptViewport] != nil,
   capture.frames[.transcriptContent] != nil,
   capture.frames[.transcriptBottomSpacer] != nil,
   capture.frames[.bottomFade] != nil,
   capture.frames[.bottomOverlay] != nil {
    break
}
```

Then add these assertions after requiring the frames:

```swift
let transcriptViewport = try #require(capture.frames[.transcriptViewport])
let transcriptContent = try #require(capture.frames[.transcriptContent])
let bottomSpacer = try #require(capture.frames[.transcriptBottomSpacer])
let fadeFrame = try #require(capture.frames[.bottomFade])
let overlayFrame = try #require(capture.frames[.bottomOverlay])

#expect(abs(transcriptContent.width - composerFrame.width) < 0.5)
#expect(abs(transcriptContent.minX - composerFrame.minX) < 0.5)
#expect(transcriptViewport.maxY >= composerFrame.maxY)
#expect(overlayFrame.minY < transcriptViewport.maxY)
#expect(abs(
    bottomSpacer.height
        - ChatBottomOverlayMetrics.contentInset(for: overlayFrame.height)
) < 0.5)
#expect(abs(
    fadeFrame.height
        - ChatBottomOverlayMetrics.fadeHeight(for: overlayFrame.height)
) < 0.5)
#expect(approvalFrame.maxY <= composerFrame.minY)
```

Do not remove the existing 640/2000 expected-width or symmetric-inset assertions.

- [ ] **Step 2: Run the real chat-pane test to verify RED**

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  '-only-testing:TillerTests/ChatControllerTests/pendingPermissionRendersAboveComposerWhilePromptRemainsOpen()'
```

Expected: compile failure for missing layout roles or missing captured frames. Do not accept a failure caused by a malformed selector.

- [ ] **Step 3: Expose the new capture roles in production**

Update `ChatPaneLayoutRole` in `ChatPaneView.swift`:

```swift
enum ChatPaneLayoutRole: Hashable {
    case approvalPanel
    case composer
    case transcriptViewport
    case transcriptContent
    case transcriptBottomSpacer
    case bottomFade
    case bottomOverlay
}
```

In `TranscriptView`, read the existing capture environment:

```swift
@Environment(\.chatPaneLayoutCaptureEnabled) private var layoutCaptureEnabled
```

Apply the roles to the relevant views:

```swift
Color.clear
    .frame(height: max(1, bottomContentInset))
    .captureLayout(.transcriptBottomSpacer, enabled: layoutCaptureEnabled)
    .id("bottom")
```

```swift
LazyVStack(alignment: .leading, spacing: 0) {
    // existing rows, thinking row, and bottom spacer
}
.padding(.vertical, 14)
.captureLayout(.transcriptContent, enabled: layoutCaptureEnabled)
```

Capture the scroll viewport after its scrolling modifiers:

```swift
.captureLayout(.transcriptViewport, enabled: layoutCaptureEnabled)
```

- [ ] **Step 4: Convert the lower pane to a measured overlay**

Add state to `ChatPaneView`:

```swift
@State private var bottomOverlayHeight: CGFloat = 0
```

Keep the auth/disconnect switch at the top of the outer `VStack`. Replace the current transcript-plus-bottom-rows sequence with:

```swift
ZStack(alignment: .bottom) {
    TranscriptView(
        controller: controller,
        worktree: worktree,
        appModel: appModel,
        bottomContentInset: ChatBottomOverlayMetrics.contentInset(
            for: bottomOverlayHeight
        )
    )

    VStack(spacing: 0) {
        Spacer(minLength: 0)
        LinearGradient(
            colors: [AppTheme.chatSurface.opacity(0), AppTheme.chatSurface],
            startPoint: .top,
            endPoint: .bottom
        )
        .frame(height: ChatBottomOverlayMetrics.fadeHeight(for: bottomOverlayHeight))
        .captureLayout(.bottomFade, enabled: layoutCaptureEnabled)
    }
    .allowsHitTesting(false)

    bottomOverlay(snapshot: snapshot)
        .captureChatBottomOverlayHeight()
        .captureLayout(.bottomOverlay, enabled: layoutCaptureEnabled)
}
.onPreferenceChange(ChatBottomOverlayHeightPreferenceKey.self) { height in
    bottomOverlayHeight = max(0, height)
}
```

Extract the current bottom rows into a view builder so their semantic order is unchanged:

```swift
@ViewBuilder
private func bottomOverlay(snapshot: ChatPresentationSnapshot) -> some View {
    VStack(spacing: 0) {
        if let promptError = controller.promptError {
            banner("Turn error", detail: promptError, actionTitle: "OK") {
                controller.promptError = nil
            }
        }
        if let mcpWarning = controller.mcpWarning {
            banner("MCP configuration", detail: mcpWarning, actionTitle: "OK") {
                controller.mcpWarning = nil
            }
        }
        if snapshot.hasPlanAwaitingApproval {
            banner(
                "Plan awaiting approval",
                detail: "Review the proposed plan in the transcript, then approve or reject it.",
                actionTitle: "OK"
            ) {}
        }
        PendingQuestionBar(
            permissions: snapshot.composerPermissions,
            controller: controller
        )
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .layoutPriority(1)
        .captureLayout(.approvalPanel, enabled: layoutCaptureEnabled)

        CenteredComposerLayout {
            ChatComposerView(
                controller: controller,
                worktreePath: worktree.path,
                document: document
            )
            .captureLayout(.composer, enabled: layoutCaptureEnabled)
        }
    }
}
```

The gradient must remain below the interactive overlay in Z-stack order, must blend into `AppTheme.chatSurface`, and must not use `AppTheme.composerFill`.

- [ ] **Step 5: Run the layout integration test to verify GREEN**

Run the exact command from Step 2.

Expected: one test passes at both 640 and 2000 points; approval stays above composer, transcript/composer widths and origins match, the transcript viewport intersects the bottom overlay, and fade/spacer heights track the measured overlay.

- [ ] **Step 6: Run focused interaction regressions**

Run each selector in a separate app-hosted process:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerSendTests

xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerDocumentObservationTests

xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/TranscriptViewLayoutRegressionTests
```

Expected: send/document behavior remains green and the transcript still contains no live scroll-geometry observer.

- [ ] **Step 7: Commit only Task 3 files**

```bash
git add App/Chat/ChatPaneView.swift \
  App/Chat/TranscriptView.swift \
  AppTests/ChatControllerTests.swift
git diff --cached --check
git commit -m "feat: float composer over transcript"
```

---

### Task 4: Focused final verification and visual acceptance

**Files:**
- Verify only; no source changes expected

- [ ] **Step 1: Regenerate the project and run the approved focused suite set**

Run each selector separately to avoid the previously diagnosed Swift Testing app-host sequence hang:

```bash
xcodegen generate
```

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerLayoutMetricsTests
```

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerStyleTests
```

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  '-only-testing:TillerTests/ChatControllerTests/pendingPermissionRendersAboveComposerWhilePromptRemainsOpen()'
```

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/TranscriptViewLayoutRegressionTests
```

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerControlBarTests
```

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -parallel-testing-enabled NO -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ComposerChipTests
```

Expected: every command exits 0 with `TEST SUCCEEDED`. Report dependency SwiftLint and session-restore diagnostics separately; do not reinterpret them as product assertions.

- [ ] **Step 2: Build the Debug app normally**

```bash
xcodebuild build -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates
```

Expected: exit 0 and `BUILD SUCCEEDED`. Do not archive, install, or run `Scripts/ci.sh`.

- [ ] **Step 3: Perform live visual checks with the built artifact**

Open the exact Debug `Tiller.app` produced by Step 2 and verify:

- resting editor is visibly compact and grows only to the 128-point cap;
- composer surface is `#20232D` and the blue ring still renders;
- visible card edges and transcript content edges align at compact and wide pane widths;
- messages travel behind the composer and fade smoothly into the chat surface;
- the last message can scroll completely above the composer;
- pending permission UI stays above the composer and remains clickable;
- scrollbar, wheel/trackpad scroll, autoscroll, drag/drop, slash popup, mention popup, send, stop, and loading controls remain usable;
- light appearance retains the fixed `#20232D` composer while the fade still resolves into the light chat background.

Capture a screenshot if Computer Use permissions are available. Static inspection and layout tests do not count as live visual evidence.

- [ ] **Step 4: Leave the worktree scoped and report validation honestly**

```bash
git status --short
git diff --check
```

Expected: no source/test changes remain unstaged. `Tiller.xcodeproj/project.pbxproj` may show only XcodeGen registration changes; never include it in a source commit. The parent agent must remove only its generated diff with `apply_patch` before branch delivery. Report focused test counts, build result, visual evidence, and any unavailable checks. Never claim the prohibited full gate passed.
