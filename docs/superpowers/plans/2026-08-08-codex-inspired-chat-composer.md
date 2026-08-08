# Codex-Inspired Chat Composer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Tiller's split, nested chat input with a centered, airy, single-surface Codex-inspired composer while preserving the existing blue ring and all chat behavior.

**Architecture:** Add one deterministic layout unit that owns width calculation and centering, then compose it from `ChatComposerView`. Keep transcript/composer relationship in `ChatPaneView`, control-state presentation in `ComposerControlBar`, and all draft/session state in the existing `ComposerDocument` and `ChatController` owners.

**Tech Stack:** Swift 6, SwiftUI/AppKit on macOS 15+, swift-testing, XcodeGen, xcodebuild.

---

## Scope and working-tree guardrails

The approved design is
`docs/superpowers/specs/2026-08-08-codex-inspired-chat-composer-design.md`.
This is one cohesive UI workstream; it does not need to be split into separate
plans.

The checkout already contains unrelated local transcript, palette, project-file,
test, and visual-review changes. Do not revert, rewrite, stage, or commit those
files. In particular, this plan does not own `App/AppTheme.swift`,
`App/Chat/TranscriptView.swift`, `AppTests/ComposerFieldTests.swift`,
`Tiller.xcodeproj/project.pbxproj`, the Xcode-parity transcript files, or the
existing `artifacts/` and `docs/visual-reviews/` directories. `xcodegen generate`
may refresh the generated Xcode project during verification; never hand-edit or
stage that generated diff as part of this feature.

Before every commit, run `git diff --cached --name-only` and confirm that it lists
only the files named in that task's commit step.

## File responsibility map

- **Create `App/Chat/ComposerLayout.swift`:** pure sizing rules plus the SwiftUI
  `Layout` that centers exactly one composer subtree.
- **Create `AppTests/ComposerLayoutMetricsTests.swift`:** deterministic coverage
  of the 84%, 1,440 pt cap, minimum-inset, and symmetry rules.
- **Modify `App/Chat/ChatPaneView.swift`:** remove only the local transcript/input
  `Divider()` and expose the existing frame-capture hook as a reusable view
  modifier.
- **Modify `App/Chat/ChatComposerView.swift`:** apply the centered layout, unified
  card surface, 104–180 pt editor height, 22 pt corners, and existing blue ring.
- **Modify `App/Chat/ComposerControlBar.swift`:** reorder existing controls and
  render stable 30 x 30 pt Send/Loading/Stop circles.
- **Modify `AppTests/ChatControllerTests.swift`:** extend the existing live
  chat-pane layout probe at narrow and wide widths.
- **Modify `AppTests/ComposerControlBarTests.swift`:** cover the action-state,
  size, symbol, opacity, and accessibility presentation constants used by the
  control bar.

No persistence, controller, document, package, `project.yml`, or theme source
changes are required.

---

### Task 1: Add deterministic composer sizing and centering

**Files:**
- Create: `App/Chat/ComposerLayout.swift`
- Create: `AppTests/ComposerLayoutMetricsTests.swift`

- [ ] **Step 1: Write the failing sizing tests**

Create `AppTests/ComposerLayoutMetricsTests.swift` with the complete suite:

```swift
import CoreGraphics
import Testing

@testable import Tiller

@Suite("ComposerLayoutMetrics")
struct ComposerLayoutMetricsTests {
    private let accuracy: CGFloat = 0.001

    @Test func preferredWidthUsesEightyFourPercentBelowTheCap() {
        let width = ComposerLayoutMetrics.width(availableWidth: 1_000)
        #expect(abs(width - 840) < accuracy)
    }

    @Test func widthStopsAtTheMaximum() {
        let width = ComposerLayoutMetrics.width(availableWidth: 2_000)
        #expect(abs(width - 1_440) < accuracy)
    }

    @Test func narrowWidthPreservesSixteenPointInsets() {
        let width = ComposerLayoutMetrics.width(availableWidth: 100)
        #expect(abs(width - 68) < accuracy)
        #expect(abs(ComposerLayoutMetrics.horizontalInset(
            availableWidth: 100, contentWidth: width) - 16) < accuracy)
    }

    @Test func derivedInsetsAreSymmetric() {
        let availableWidth: CGFloat = 640
        let contentWidth = ComposerLayoutMetrics.width(availableWidth: availableWidth)
        let leading = ComposerLayoutMetrics.horizontalInset(
            availableWidth: availableWidth, contentWidth: contentWidth)
        let trailing = availableWidth - contentWidth - leading

        #expect(abs(leading - trailing) < accuracy)
    }

    @Test func invalidNegativeWidthCollapsesSafely() {
        #expect(ComposerLayoutMetrics.width(availableWidth: -50) == 0)
    }
}
```

- [ ] **Step 2: Run the focused suite and verify the red state**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/ComposerLayoutMetricsTests
```

Expected: the test target fails to compile because `ComposerLayoutMetrics` does
not exist. If test discovery reports zero tests, treat that as a failure rather
than a red-state success.

- [ ] **Step 3: Implement the minimal deterministic layout unit**

Create `App/Chat/ComposerLayout.swift`:

```swift
import SwiftUI

enum ComposerLayoutMetrics {
    static let preferredWidthFraction: CGFloat = 0.84
    static let maximumWidth: CGFloat = 1_440
    static let minimumHorizontalInset: CGFloat = 16

    static func width(availableWidth: CGFloat) -> CGFloat {
        let availableWidth = max(0, availableWidth)
        let preferredWidth = availableWidth * preferredWidthFraction
        let insetConstrainedWidth = max(
            0, availableWidth - (minimumHorizontalInset * 2))
        return min(preferredWidth, min(maximumWidth, insetConstrainedWidth))
    }

    static func horizontalInset(availableWidth: CGFloat,
                                contentWidth: CGFloat) -> CGFloat {
        max(0, (max(0, availableWidth) - max(0, contentWidth)) / 2)
    }
}

/// Gives one composer subtree its approved responsive width while reporting the
/// full pane width to its parent, so the child remains centered as the pane resizes.
struct CenteredComposerLayout: Layout {
    func sizeThatFits(proposal: ProposedViewSize,
                     subviews: Subviews,
                     cache: inout ()) -> CGSize {
        let availableWidth = max(0, proposal.width ?? 0)
        guard let subview = subviews.first else {
            return CGSize(width: availableWidth, height: 0)
        }
        let contentWidth = ComposerLayoutMetrics.width(
            availableWidth: availableWidth)
        let contentSize = subview.sizeThatFits(
            ProposedViewSize(width: contentWidth, height: proposal.height))
        return CGSize(width: availableWidth, height: contentSize.height)
    }

    func placeSubviews(in bounds: CGRect,
                       proposal: ProposedViewSize,
                       subviews: Subviews,
                       cache: inout ()) {
        guard let subview = subviews.first else { return }
        let contentWidth = ComposerLayoutMetrics.width(
            availableWidth: bounds.width)
        let contentProposal = ProposedViewSize(
            width: contentWidth, height: proposal.height)
        let contentSize = subview.sizeThatFits(contentProposal)
        subview.place(
            at: CGPoint(x: bounds.midX, y: bounds.minY),
            anchor: .top,
            proposal: ProposedViewSize(
                width: contentWidth, height: contentSize.height))
    }
}
```

- [ ] **Step 4: Run the sizing tests and verify the green state**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/ComposerLayoutMetricsTests
```

Expected: `ComposerLayoutMetricsTests` runs five tests and all five pass.

- [ ] **Step 5: Commit only the layout unit and its tests**

```bash
git add App/Chat/ComposerLayout.swift AppTests/ComposerLayoutMetricsTests.swift
git diff --cached --check
git diff --cached --name-only
git commit -m "feat: add responsive composer layout"
```

Expected staged paths: exactly the two paths above.

---

### Task 2: Center and unify the composer, then remove the local divider

**Files:**
- Modify: `App/Chat/ChatPaneView.swift:9-32,93-104,163-176`
- Modify: `App/Chat/ChatComposerView.swift:12-120,272-310`
- Modify: `AppTests/ChatControllerTests.swift:778-824`

- [ ] **Step 1: Extend the existing layout test to express the approved geometry**

In
`pendingPermissionRendersAboveComposerWhilePromptRemainsOpen()`, replace the
single 640 pt host-layout/assertion block from `host.setFrameSize` through the
approval/composer assertion with this loop:

```swift
        for paneWidth in [CGFloat(640), CGFloat(2_000)] {
            capture.frames = [:]
            host.setFrameSize(NSSize(width: paneWidth, height: 480))
            host.layoutSubtreeIfNeeded()
            for _ in 0..<20 {
                if capture.frames[.approvalPanel] != nil,
                   capture.frames[.composer] != nil { break }
                await Task.yield()
            }

            let approvalFrame = try #require(capture.frames[.approvalPanel])
            let composerFrame = try #require(capture.frames[.composer])
            let expectedWidth = ComposerLayoutMetrics.width(
                availableWidth: paneWidth)
            let leadingInset = composerFrame.minX
            let trailingInset = paneWidth - composerFrame.maxX

            #expect(approvalFrame.maxY <= composerFrame.minY)
            #expect(abs(composerFrame.width - expectedWidth) < 1)
            #expect(abs(leadingInset - trailingInset) < 1)
        }
```

Keep the existing setup, pending-permission assertions, and
`await driver.releasePrompt()` unchanged.

- [ ] **Step 2: Run the layout test and verify the red state**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/ChatControllerTests/pendingPermissionRendersAboveComposerWhilePromptRemainsOpen
```

Expected: the assertions fail because the current capture wraps the full-width
`ChatComposerView`, not an 84%-width centered composer. The existing assertion
that the pending panel remains above the composer must still pass.

- [ ] **Step 3: Make chat-pane frame capture reusable and remove only the local divider**

In `App/Chat/ChatPaneView.swift`, add this extension after the existing
`EnvironmentValues` extension:

```swift
extension View {
    @ViewBuilder
    func captureChatPaneLayout(_ role: ChatPaneLayoutRole,
                               enabled: Bool) -> some View {
        if enabled {
            background(GeometryReader { proxy in
                Color.clear.preference(
                    key: ChatPaneLayoutPreferenceKey.self,
                    value: [role: proxy.frame(in: .named("chat-pane"))])
            })
        } else {
            self
        }
    }
}
```

Replace the approval/composer section of `ChatPaneView.body` with:

```swift
            PendingQuestionBar(permissions: snapshot.composerPermissions,
                               controller: controller)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .layoutPriority(1)
                .captureChatPaneLayout(
                    .approvalPanel, enabled: layoutCaptureEnabled)
            ChatComposerView(controller: controller, worktreePath: worktree.path,
                             document: document)
```

Delete the old `Divider()` and the private `captureLayout` view-builder method.
Do not change the workspace split divider, drop overlay, coordinate space, or
controller activation code.

- [ ] **Step 4: Apply the centered single-surface composition**

In `App/Chat/ChatComposerView.swift`:

1. Add the layout-capture environment value with the other properties:

```swift
    @Environment(\.chatPaneLayoutCaptureEnabled) private var layoutCaptureEnabled
```

2. Remove `colorScheme`, `cardBackgroundStyle`, `loadingBackgroundStyle`,
   `isConnecting`, and the unused local `loadingButton`. They are no longer
   composer responsibilities; connection chrome already lives in
   `ComposerControlBar`.

3. Replace `body` with:

```swift
    var body: some View {
        CenteredComposerLayout {
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
            .frame(maxWidth: .infinity, alignment: .leading)
            .captureChatPaneLayout(.composer, enabled: layoutCaptureEnabled)
        }
        .padding(.vertical, 16)
        .enableInjection()
    }
```

4. Replace `card` and the `ChatTextEditor` construction with:

```swift
    private var card: some View {
        VStack(alignment: .leading, spacing: 12) {
            editor
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
            ComposerControlBar(
                controller: controller, document: document, onAttach: attachImage,
                onSend: sendCurrent, canSend: canSend, canInteract: canInteract)
        }
        .padding(12)
        .background(AppTheme.cardFill,
                    in: RoundedRectangle(cornerRadius: 22))
        .overlay {
            ComposerBorderView(
                isAnimating: isPrompting,
                isFocused: document.isFocused,
                reduceMotion: reduceMotion)
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.15),
                   value: document.isFocused)
    }

    private var editor: some View {
        ZStack(alignment: .topLeading) {
            if document.isEmpty {
                Text(editorPlaceholder)
                    .foregroundStyle(.secondary)
                    .allowsHitTesting(false)
            }
            ChatTextEditor(document: document, isEditable: canInteract,
                           minHeight: 104, maxHeight: 180,
                           onSubmit: sendCurrent, onSlashKey: handleSlashKey)
        }
        .disabled(!canInteract)
        .onChange(of: document.mentionQuery) { _, query in
            refreshMentionCandidates(query: query)
        }
        .onChange(of: document.slashQuery) {
            slashPopupDismissed = false
            slashSelectionIndex = 0
        }
    }
```

This removes only the editor's black `composerFieldFill` background. Keep
`AppTheme.composerFieldFill` itself untouched because it is not owned by this
feature.

5. Change the private `ComposerBorderView.cornerRadius` constant from `14` to
   `22`. Do not change its colors, line widths, phase, animation timing, focus
   behavior, or Reduce Motion branch.

- [ ] **Step 5: Run layout and composer behavior tests**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/ComposerLayoutMetricsTests \
  -only-testing:TillerTests/ChatControllerTests/pendingPermissionRendersAboveComposerWhilePromptRemainsOpen \
  -only-testing:TillerTests/ComposerSendTests \
  -only-testing:TillerTests/ComposerDocumentTests \
  -only-testing:TillerTests/ComposerChipTests
```

Expected: all selected tests run and pass. The layout test passes at 640 pt and
2,000 pt; composer draft, send, mention, slash-command, chip, and attachment
behavior remains green.

- [ ] **Step 6: Commit only the centered composer changes**

```bash
git add App/Chat/ComposerLayout.swift App/Chat/ChatPaneView.swift \
        App/Chat/ChatComposerView.swift AppTests/ChatControllerTests.swift \
        AppTests/ComposerLayoutMetricsTests.swift
git diff --cached --check
git diff --cached --name-only
git commit -m "feat: center and simplify chat composer"
```

`ComposerLayout.swift` and its test were committed in Task 1 and normally have
no new diff here; naming them in `git add` is harmless and keeps any Task 2
layout adjustment coupled to its tests. Never add the generated Xcode project.

---

### Task 3: Reorder the control bar and add stable circular actions

**Files:**
- Modify: `App/Chat/ComposerControlBar.swift:20-64,239-287`
- Modify: `AppTests/ComposerControlBarTests.swift:6-37`

- [ ] **Step 1: Replace stale border-helper coverage with action-chrome tests**

Replace `AppTests/ComposerControlBarTests.swift` with:

```swift
import Testing

@testable import Tiller

@Suite("ComposerControlBar")
@MainActor
struct ComposerControlBarTests {
    @Test func connectingShowsTheLoadingControl() {
        #expect(ComposerControlBar.trailingControl(for: .connecting) == .loading)
    }

    @Test func promptingShowsTheStopControl() {
        #expect(ComposerControlBar.trailingControl(for: .prompting) == .stop)
    }

    @Test func everyOtherStateShowsTheSendControl() {
        for state in [ChatController.ChatState.idle, .ready, .needsAuth,
                      .disconnected(message: nil)] {
            #expect(ComposerControlBar.trailingControl(for: state) == .send)
        }
    }

    @Test func primaryActionsUseStableCodexInspiredChrome() {
        #expect(ComposerControlBar.primaryActionSize == 30)
        #expect(ComposerControlBar.inactiveActionFillOpacity == 0.18)
        #expect(ComposerControlBar.sendSystemImage == "arrow.up")
    }

    @Test func primaryActionsExposeStateSpecificLabels() {
        #expect(ComposerControlBar.TrailingControl.send.accessibilityLabel == "Send")
        #expect(ComposerControlBar.TrailingControl.loading.accessibilityLabel
                == "Starting the agent")
        #expect(ComposerControlBar.TrailingControl.stop.accessibilityLabel
                == "Stop the turn")
    }
}
```

The old `borderStyle` tests are removed because that helper is unused by the
real blue `ComposerBorderView`; keeping them would test dead code instead of the
approved action presentation.

- [ ] **Step 2: Run the focused suite and verify the red state**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/ComposerControlBarTests
```

Expected: compilation fails because the new action size, opacity, symbol, and
accessibility presentation members do not exist yet.

- [ ] **Step 3: Add the tested action presentation and approved control order**

In `App/Chat/ComposerControlBar.swift`, remove the type-level `colorScheme`
environment property and the unused `borderStyle(isFocused:)` method. Keep
`PillBackground`'s own appearance environment unchanged.

Add these members immediately before `TrailingControl`:

```swift
    static let primaryActionSize: CGFloat = 30
    static let inactiveActionFillOpacity = 0.18
    static let sendSystemImage = "arrow.up"

    enum TrailingControl {
        case loading, stop, send

        var accessibilityLabel: String {
            switch self {
            case .loading: "Starting the agent"
            case .stop: "Stop the turn"
            case .send: "Send"
            }
        }
    }
```

Replace `body` with the approved leading/trailing grouping:

```swift
    var body: some View {
        HStack(spacing: 8) {
            Button(action: onAttach) {
                Image(systemName: "plus")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Attach image")
            .disabled(!canInteract)

            modePill
            Spacer()
            overflowMenu
            contextUsageIndicator
            agentPill
            switch Self.trailingControl(for: controller.state) {
            case .loading: loadingButton
            case .stop: stopButton
            case .send: sendButton
            }
        }
        .enableInjection()
    }
```

Replace the three primary-action views with:

```swift
    private var sendButton: some View {
        Button(action: onSend) {
            Image(systemName: Self.sendSystemImage)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(canSend ? Color.white : Color.secondary)
                .frame(width: Self.primaryActionSize,
                       height: Self.primaryActionSize)
                .background(
                    canSend
                        ? AnyShapeStyle(Color.accentColor)
                        : AnyShapeStyle(Color.secondary.opacity(
                            Self.inactiveActionFillOpacity)),
                    in: Circle())
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.return, modifiers: [])
        .disabled(!canSend)
        .accessibilityLabel(TrailingControl.send.accessibilityLabel)
        .help(TrailingControl.send.accessibilityLabel)
    }

    private var loadingButton: some View {
        ProgressView()
            .controlSize(.small)
            .frame(width: Self.primaryActionSize,
                   height: Self.primaryActionSize)
            .background(
                Color.secondary.opacity(Self.inactiveActionFillOpacity),
                in: Circle())
            .accessibilityLabel(TrailingControl.loading.accessibilityLabel)
            .help("Starting the agent…")
    }

    private var stopButton: some View {
        Button {
            Task { await controller.cancelTurn() }
        } label: {
            Image(systemName: "stop.fill")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(.white)
                .frame(width: Self.primaryActionSize,
                       height: Self.primaryActionSize)
                .background(Color.red.opacity(0.8), in: Circle())
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.escape, modifiers: [])
        .accessibilityLabel(TrailingControl.stop.accessibilityLabel)
        .help(TrailingControl.stop.accessibilityLabel)
    }
```

Do not add a microphone, change menu actions, alter context-meter logic, or
change model/effort and permission-mode state flow.

- [ ] **Step 4: Run control and behavior regressions**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/ComposerControlBarTests \
  -only-testing:TillerTests/ComposerSendTests \
  -only-testing:TillerTests/ComposerDocumentTests \
  -only-testing:TillerTests/ComposerChipTests
```

Expected: all selected suites run and pass. `ComposerControlBarTests` reports
five passing tests, and the existing send/draft/chip suites remain green.

- [ ] **Step 5: Commit only the control-bar change and its tests**

```bash
git add App/Chat/ComposerControlBar.swift AppTests/ComposerControlBarTests.swift
git diff --cached --check
git diff --cached --name-only
git commit -m "feat: restyle composer controls"
```

Expected staged paths: exactly the two paths above.

---

### Task 4: Run the repository gate and perform live visual verification

**Files:**
- No source files should change in this task.
- Inspect only; do not stage generated or unrelated working-tree changes.

- [ ] **Step 1: Run all App-target tests using the repository's current test path**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages
```

Expected: the App test run succeeds and reports a non-zero test count. If it
fails, inspect the complete log rather than relying on the final xcodebuild
epilogue, fix only regressions caused by the composer commits, and rerun.

- [ ] **Step 2: Run the single authoritative repository gate**

Run:

```bash
Scripts/ci.sh
```

Expected final output: `CI OK`. This is required before implementation can be
called complete. A focused suite or successful build is not a substitute.

- [ ] **Step 3: Launch the built app and verify dark appearance**

Run:

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Open a chat in a wide center pane, then a narrow center pane, and verify all of
the following against the supplied Codex reference and the approved spec:

- no horizontal divider above the composer;
- one unified elevated card with no black inset field;
- equal left/right margins at both widths and a 1,440 pt cap when space permits;
- 22 pt corners and an editor that starts at 104 pt and grows before scrolling;
- blue idle/focus ring and animated processing ring remain intact;
- leading group is attachment plus mode; trailing group is overflow, context,
  model/effort, and the primary action;
- Send, Loading, and Stop stay in one 30 x 30 pt circular footprint without
  shifting adjacent controls;
- Return, Shift-Return, Escape, attachment, drop, slash command, mention,
  permission, and queued-draft interactions still behave as before.

- [ ] **Step 4: Verify light appearance, Reduce Motion, and accessibility labels**

Switch macOS/Tiller to light appearance and repeat the wide/narrow checks. Enable
Reduce Motion and verify that the existing non-animated border treatment remains
legible. With VoiceOver or Accessibility Inspector, verify the icon-only actions
announce `Send`, `Starting the agent`, and `Stop the turn` in their respective
states.

If the environment cannot launch or inspect the desktop app, preserve the build
and test evidence but explicitly report visual verification as blocked. Do not
claim pixel-level or interaction approval from static checks alone.

- [ ] **Step 5: Confirm commit and worktree scope**

Run:

```bash
git log -3 --oneline
git diff --check
git status --short
```

Expected composer commits, in order:

```text
feat: add responsive composer layout
feat: center and simplify chat composer
feat: restyle composer controls
```

The status may still list the pre-existing unrelated transcript, palette,
generated-project, configuration, artifact, and visual-review work. Confirm that
none of it was included in the three composer commits. If verification itself
made an unintended generated-file change, leave the user's existing file state
untouched and report it rather than resetting or staging it.
