# Flush Column Seams Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the two vertical seams between the sidebar, the central pane and the right panel invisible at rest, and show a thin grey line there only while the pointer is on the resize band.

**Architecture:** The seams are collapsed to zero width by moving each column's gap padding to its outward side only. Two new pure geometry helpers on `CardLayout` tell `FloatingCard` which corners to square off and how far its shadow may bleed, which is what actually removes the black band. The existing 2pt `dividerCover` strip is repurposed: it keeps hiding `NSSplitView`'s hairline and additionally becomes the hover highlight, driven by an `NSTrackingArea` added to the existing `DividerCursorStripView`.

**Tech Stack:** Swift 6, SwiftUI + AppKit (`NSViewRepresentable`), swift-testing (`@Test` / `#expect`), xcodegen, `Scripts/ci.sh`.

**Spec:** `docs/superpowers/specs/2026-08-11-flush-column-seams-design.md`

## Global Constraints

- macOS 15+, Swift 6. `UnevenRoundedRectangle` and `RectangleCornerRadii` require macOS 13+ and are therefore available.
- Tests use **swift-testing** (`@Test` / `#expect`), never XCTest.
- Never hand-edit `Tiller.xcodeproj`. This plan creates **no new source files**, so no `xcodegen generate` step is needed beyond the one `Scripts/ci.sh` runs itself.
- Commit messages follow Conventional Commits with a lower-case imperative subject (`feat:`, `fix:`, `refactor:`, `test:`).
- All user-facing strings stay English. This plan adds none.
- `AppTheme` token values are fixed and must not be changed: `cardCornerRadius == 6`, `cardGap == 10`, `cardShadowRadius == 18`, `cardShadowYOffset == 6`. `AppTests/CardGeometryTests.swift:7` asserts them.
- **The CI gate has a known red baseline** as of 2026-08-06. A failing `Scripts/ci.sh` is only attributable to this work if the same failure does not reproduce on a clean tree. Never conclude from `HEAD` comparison alone.
- **Never judge a test run from `tail`.** `xcodebuild` prints an epilogue after the failures. Always redirect to a log and `grep` for the specific test name.
- Visual verification is the user's, not the implementer's. Do not take screenshots or launch the app to judge appearance.

### Running app-target tests

`AppTests/` free `@Test` functions cannot be addressed by `-only-testing:` (there is no enclosing struct). Run the whole target and grep for the test by name:

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
mkdir -p DerivedData
set +e
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  > DerivedData/apptests.log 2>&1
echo "exit=$?"
set -e
grep -E "cardCorners|shadowBleed|seamCentre|error:" DerivedData/apptests.log | head -30
```

Do **not** add `CODE_SIGNING_ALLOWED=NO` or `-derivedDataPath` to that command: with either one the test host hangs in dyld before test discovery (`Scripts/ci.sh:23-27`).

---

### Task 1: Pure corner and shadow geometry on `CardLayout`

Two pure functions that later tasks consume. Keeping them out of the view means they are the only part of this change that can be tested at all.

**Files:**
- Modify: `App/CardLayout.swift` (whole file, currently 13 lines)
- Test: `AppTests/CardGeometryTests.swift` (append)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `CardLayout.cardCorners(flushEdges: Edge.Set, radius: CGFloat) -> RectangleCornerRadii`
  - `CardLayout.shadowBleed(flushEdges: Edge.Set, radius: CGFloat) -> EdgeInsets`
  - `CardLayout.dividerCenterX(columnWidth:gap:)` is unchanged and stays as-is.

- [ ] **Step 1: Write the failing tests**

Append to `AppTests/CardGeometryTests.swift`. The file already has `import SwiftUI` and `@testable import Tiller` at the top, so no import changes are needed.

```swift
/// A flush edge squares both of its corners. Two rounded corners meeting at a
/// zero-width seam leave a notch of bare canvas above and below it.
@Test func cardCornersSquareTheFlushEdge() {
    let corners = CardLayout.cardCorners(flushEdges: .leading, radius: 6)

    #expect(corners.topLeading == 0)
    #expect(corners.bottomLeading == 0)
    #expect(corners.topTrailing == 6)
    #expect(corners.bottomTrailing == 6)
}

@Test func cardCornersStayRoundWithNoFlushEdge() {
    let corners = CardLayout.cardCorners(flushEdges: [], radius: 6)

    #expect(corners.topLeading == 6)
    #expect(corners.bottomLeading == 6)
    #expect(corners.topTrailing == 6)
    #expect(corners.bottomTrailing == 6)
}

/// The central card is flush on both sides whenever both side panels are open.
@Test func cardCornersSquareBothVerticalSeams() {
    let corners = CardLayout.cardCorners(flushEdges: [.leading, .trailing], radius: 6)

    #expect(corners.topLeading == 0)
    #expect(corners.bottomLeading == 0)
    #expect(corners.topTrailing == 0)
    #expect(corners.bottomTrailing == 0)
}

/// The sideways shadow spill is what makes a zero-width seam read as a black
/// band, so a flush edge gets no bleed at all.
@Test func shadowBleedStopsAtAFlushEdge() {
    let bleed = CardLayout.shadowBleed(flushEdges: .leading, radius: 18)

    #expect(bleed.leading == 0)
    #expect(bleed.trailing == 36)
    #expect(bleed.top == 36)
    #expect(bleed.bottom == 36)
}

@Test func shadowBleedIsUnboundedWithNoFlushEdge() {
    let bleed = CardLayout.shadowBleed(flushEdges: [], radius: 18)

    #expect(bleed.leading == 36)
    #expect(bleed.trailing == 36)
    #expect(bleed.top == 36)
    #expect(bleed.bottom == 36)
}

/// `dividerCenterX` needs no edit when a column's gap padding moves from both
/// sides to the outward side alone: the column measures half a gap narrower
/// and the seam moves by exactly that much, so the same expression keeps
/// landing on the seam. Both offsets in `ContentView` depend on this.
@Test func seamCentreFollowsThePaddingMovingOutward() {
    let cardWidth: CGFloat = 240
    let gap: CGFloat = 10

    let symmetric = CardLayout.dividerCenterX(columnWidth: cardWidth + gap, gap: gap)
    let outwardOnly = CardLayout.dividerCenterX(columnWidth: cardWidth + gap / 2, gap: gap)

    #expect(symmetric == cardWidth + gap * 1.5)
    #expect(outwardOnly == cardWidth + gap)
    #expect(symmetric - outwardOnly == gap / 2)
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Use the app-target command from Global Constraints. Expected: compile errors naming `cardCorners` and `shadowBleed` as unknown members of `CardLayout`.

```bash
grep -E "cardCorners|shadowBleed|error:" DerivedData/apptests.log | head -30
```

Expected to see `error: type 'CardLayout' has no member 'cardCorners'` (and the same for `shadowBleed`). `seamCentreFollowsThePaddingMovingOutward` will not run yet because the target does not compile — that is expected.

- [ ] **Step 3: Write the implementation**

Replace the whole of `App/CardLayout.swift` with:

```swift
import CoreGraphics
import SwiftUI

/// Where the split's divider sits, and how a card is shaped where it meets one.
///
/// The seam is built from paddings: half a gap on the outside of the split
/// container, plus whatever a column pads on its own outward side. A column's
/// measured width includes its own padding, so from the container's leading
/// edge the seam is the outer half-gap plus that width.
enum CardLayout {
    static func dividerCenterX(columnWidth: CGFloat, gap: CGFloat) -> CGFloat {
        gap / 2 + columnWidth
    }

    /// Corner radii for a card whose `flushEdges` sit against a neighbour with
    /// no gap between them.
    ///
    /// Two rounded corners meeting at a zero-width seam leave a notch of bare
    /// canvas above and below it, so a flush edge squares off both of its own
    /// corners.
    static func cardCorners(
        flushEdges: Edge.Set, radius: CGFloat
    ) -> RectangleCornerRadii {
        func radiusAt(_ first: Edge.Set, _ second: Edge.Set) -> CGFloat {
            flushEdges.contains(first) || flushEdges.contains(second) ? 0 : radius
        }

        return RectangleCornerRadii(
            topLeading: radiusAt(.leading, .top),
            bottomLeading: radiusAt(.leading, .bottom),
            bottomTrailing: radiusAt(.trailing, .bottom),
            topTrailing: radiusAt(.trailing, .top))
    }

    /// How far a card's shadow may reach past its own bounds on each side.
    ///
    /// A flush edge gets none. A shadow spilling sideways onto a neighbour is
    /// what makes a zero-width seam read as a black band — it, not the seam's
    /// width, is the dominant source of the dark stripe.
    static func shadowBleed(flushEdges: Edge.Set, radius: CGFloat) -> EdgeInsets {
        func bleedAt(_ edge: Edge.Set) -> CGFloat {
            flushEdges.contains(edge) ? 0 : radius * 2
        }

        return EdgeInsets(
            top: bleedAt(.top),
            leading: bleedAt(.leading),
            bottom: bleedAt(.bottom),
            trailing: bleedAt(.trailing))
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Use the app-target command from Global Constraints, then:

```bash
grep -E "cardCorners|shadowBleed|seamCentre" DerivedData/apptests.log | head -30
```

Expected: six `✔ Test ... passed` lines, no `✘`. Other suites in the target may fail — that is the known red baseline and is not this task's concern.

- [ ] **Step 5: Commit**

```bash
git add App/CardLayout.swift AppTests/CardGeometryTests.swift
git commit -m "feat: add flush-edge corner and shadow-bleed geometry to CardLayout"
```

---

### Task 2: `FloatingCard` honours `flushEdges`

Wires Task 1's geometry into the card. After this task the parameter exists and defaults to `[]`, so both existing call sites keep rendering exactly as before — nothing changes visually yet.

**Files:**
- Modify: `App/FloatingCard.swift` (whole file, currently 31 lines)

**Interfaces:**
- Consumes: `CardLayout.cardCorners(flushEdges:radius:)`, `CardLayout.shadowBleed(flushEdges:radius:)` from Task 1.
- Produces: `FloatingCard(tint:flushEdges:content:)` with `tint: Color = AppTheme.background` and `flushEdges: Edge.Set = []`. Task 3 passes `flushEdges`.

- [ ] **Step 1: Write the implementation**

There is no unit test for this task: `FloatingCard` is a view whose whole output is a rendered appearance, and the only assertable part of it is Task 1's geometry, already covered. Replace the whole of `App/FloatingCard.swift`:

```swift
import SwiftUI

/// A panel resting on the window canvas: tinted, rounded, and shadowed.
///
/// `flushEdges` names the sides that sit against a neighbouring card with no
/// gap. Those sides lose both their corner rounding and their shadow bleed:
/// at a zero-width seam a rounded corner leaves a notch of bare canvas, and a
/// shadow spilling sideways reads as a black band down the seam.
struct FloatingCard<Content: View>: View {
    @ObserveInjection private var inject

    @Environment(\.colorScheme) private var colorScheme
    private let tint: Color
    private let flushEdges: Edge.Set
    private let content: () -> Content

    init(tint: Color = AppTheme.background,
         flushEdges: Edge.Set = [],
         @ViewBuilder content: @escaping () -> Content) {
        self.tint = tint
        self.flushEdges = flushEdges
        self.content = content
    }

    var body: some View {
        content()
            .background { MainSurfaceMaterial(tint: tint) }
            .clipShape(UnevenRoundedRectangle(
                cornerRadii: CardLayout.cardCorners(
                    flushEdges: flushEdges, radius: AppTheme.cardCornerRadius),
                style: .continuous))
            .compositingGroup()
            .shadow(
                color: .black.opacity(colorScheme == .dark ? 0.36 : 0.14),
                radius: AppTheme.cardShadowRadius,
                y: AppTheme.cardShadowYOffset)
            .clipShape(ShadowBleedRect(insets: CardLayout.shadowBleed(
                flushEdges: flushEdges, radius: AppTheme.cardShadowRadius)))
    .enableInjection()
    }
}

/// The card's own bounds grown by however far its shadow is allowed to reach.
///
/// `.compositingGroup()` flattens the card before the shadow is applied, so
/// clipping the shadowed result to this shape is what confines the shadow to
/// the sides that are not flush against a neighbour.
///
/// The insets are read straight off the rect's edges, which assumes a
/// left-to-right layout — the only direction this window is laid out in.
private struct ShadowBleedRect: Shape {
    let insets: EdgeInsets

    func path(in rect: CGRect) -> Path {
        Path(CGRect(
            x: rect.minX - insets.leading,
            y: rect.minY - insets.top,
            width: rect.width + insets.leading + insets.trailing,
            height: rect.height + insets.top + insets.bottom))
    }
}
```

- [ ] **Step 2: Verify it builds and the existing tests still pass**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
mkdir -p DerivedData
set +e
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  > DerivedData/apptests.log 2>&1
echo "exit=$?"
set -e
grep -E "error:|cardCorners|shadowBleed|cardsUseApprovedFloatingGeometry" DerivedData/apptests.log | head -30
```

Expected: no `error:` lines, and `cardsUseApprovedFloatingGeometry` still passing.

- [ ] **Step 3: Commit**

```bash
git add App/FloatingCard.swift
git commit -m "feat: let FloatingCard square corners and clip shadow on flush edges"
```

---

### Task 3: Collapse the seams in `ContentView`

The visible change. After this task the dark bands are gone and the columns sit flush; the hover highlight arrives in Task 4.

**Files:**
- Modify: `App/ContentView.swift` — `splitContent` (`:241-271`), `dividerCover` (`:273-281`), `splitColumns()` (`:283-343`)

**Interfaces:**
- Consumes: `FloatingCard(tint:flushEdges:content:)` from Task 2.
- Produces: a `private var centralFlushEdges: Edge.Set` and a `private var dividerSeam: some View` that Task 4 turns into a function taking a `hovered` flag.

- [ ] **Step 1: Add the `centralFlushEdges` property**

Insert immediately above `private var splitContent: some View` (currently `App/ContentView.swift:241`):

```swift
    /// The sides of the central card that sit against a neighbouring column.
    ///
    /// It drives two things that must agree: which corners the card squares
    /// off, and which side keeps its half-gap of window margin. Letting them
    /// drift apart is how the central card ends up with a 5pt margin against
    /// the window edge when a side panel is hidden.
    private var centralFlushEdges: Edge.Set {
        var edges: Edge.Set = []
        if sidebarVisible { edges.insert(.leading) }
        if rightPanelVisible { edges.insert(.trailing) }
        return edges
    }
```

- [ ] **Step 2: Repaint `dividerCover` and rename it**

`NSSplitView`'s hairline is currently hidden because the gap around it is already canvas. With the seam at zero width, painting canvas there would produce the single most visible black line in the window — the exact thing this work removes. Replace the whole of `dividerCover` (`App/ContentView.swift:273-281`) with:

```swift
    /// `NSSplitView` draws its own hairline between columns. With the columns
    /// flush that line lands right on the seam, so it gets painted over with
    /// the central pane's own surface and disappears into the card.
    private var dividerSeam: some View {
        Rectangle()
            .fill(AppTheme.chatSurface)
            .frame(width: 2)
            .ignoresSafeArea()
            .allowsHitTesting(false)
    }
```

Then update both references in `splitContent` (`App/ContentView.swift:247` and `:259`), changing `dividerCover` to `dividerSeam`. Leave both `.offset(...)` expressions exactly as they are — Task 1's `seamCentreFollowsThePaddingMovingOutward` test is what pins that they stay correct.

- [ ] **Step 3: Move each column's padding to its outward side**

Three edits inside `splitColumns()`. Keep the existing modifier order in each case: padding first, then `.frame`, then `.onGeometryChange`. That order is what makes the measured width include the padding, which the seam offsets depend on.

Sidebar (`App/ContentView.swift:287`) — replace:

```swift
                    .padding(.horizontal, AppTheme.cardGap / 2)
```

with:

```swift
                    .padding(.leading, AppTheme.cardGap / 2)
```

Central card (`App/ContentView.swift:314`) — replace:

```swift
            .padding(.horizontal, AppTheme.cardGap / 2)
```

with:

```swift
            .padding(.leading, centralFlushEdges.contains(.leading) ? 0 : AppTheme.cardGap / 2)
            .padding(.trailing, centralFlushEdges.contains(.trailing) ? 0 : AppTheme.cardGap / 2)
```

Right panel (`App/ContentView.swift:325`) — replace:

```swift
                .padding(.horizontal, AppTheme.cardGap / 2)
```

with:

```swift
                .padding(.trailing, AppTheme.cardGap / 2)
```

- [ ] **Step 4: Pass `flushEdges` to both cards**

Central card (`App/ContentView.swift:297`) — replace `FloatingCard {` with:

```swift
            FloatingCard(flushEdges: centralFlushEdges) {
```

Right panel card (`App/ContentView.swift:317`) — replace `FloatingCard {` with:

```swift
                FloatingCard(flushEdges: .leading) {
```

The right panel is always immediately right of the central pane when it is visible, so its leading edge is unconditionally flush.

- [ ] **Step 5: Verify it builds and the tests still pass**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
mkdir -p DerivedData
set +e
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  > DerivedData/apptests.log 2>&1
echo "exit=$?"
set -e
grep -E "error:|cardCorners|shadowBleed|seamCentre" DerivedData/apptests.log | head -30
```

Expected: no `error:` lines; Task 1's six tests still passing. Do not launch the app to check appearance — that verification belongs to the user.

- [ ] **Step 6: Commit**

```bash
git add App/ContentView.swift
git commit -m "feat: collapse the column seams so the sidebars sit flush with the pane"
```

---

### Task 4: Hover highlight on the seam

**Files:**
- Modify: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DividerCursorRectsTests.swift` (append to `DividerCursorHitPolicyTests`)
- Modify: `App/DividerCursorStrip.swift` (whole file)
- Modify: `App/ContentView.swift` — `dividerSeam`, `splitContent` (`:245-268`), plus two new `@State` properties

**Interfaces:**
- Consumes: `dividerSeam` and the two `DividerCursorStrip()` call sites from Task 3.
- Produces: `DividerCursorStrip(cursor:onHoverChange:)` with `onHoverChange: (Bool) -> Void = { _ in }`, and `DividerCursorStripView.onHoverChange`.

- [ ] **Step 1: Pin the hit policy the highlight depends on**

Append inside the existing `struct DividerCursorHitPolicyTests` in `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DividerCursorRectsTests.swift`, after `onlyHoverEventsAreAnswered()`:

```swift
    /// The hover highlight rides on the same target as the resize cursor.
    /// If enter/exit ever stopped being answered, the strip would keep its
    /// cursor but silently stop reporting hover.
    @Test func hoverEnterAndExitAreAnswered() {
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: .mouseEntered))
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: .mouseExited))
    }
```

This is a regression guard, not a red-green cycle: `DividerCursorHitPolicy` already accepts both events, so it passes on the first run. It exists so a future edit to the policy cannot silently break the highlight.

- [ ] **Step 2: Run it — expected PASS**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller/Packages/TillerWorkspace
swift test --filter DividerCursorHitPolicyTests
```

Expected: 2 tests, both passing. If `hoverEnterAndExitAreAnswered` fails, stop — the whole Task 4 approach is invalid and the strip would need to answer hover some other way.

- [ ] **Step 3: Add the tracking area to the strip**

Replace the whole of `App/DividerCursorStrip.swift`:

```swift
import AppKit
import SwiftUI
import TillerWorkspace

/// A hover-only target sitting over an `HSplitView` divider.
///
/// The divider itself is a hairline: `NSSplitView` accepts a drag from a band
/// several points wide, but the cursor rect it installs is as thin as the line,
/// so the resize cursor only ever appeared once a drag was already under way.
/// This widens the band the cursor responds to, and reports hover for the seam
/// highlight, while letting clicks fall through to the divider — see
/// `DividerCursorHitPolicy`.
struct DividerCursorStrip: NSViewRepresentable {
    /// Wide enough to land on without aiming, narrow enough not to claim the
    /// cursor while the pointer is working inside either pane.
    static let width: CGFloat = 10

    var cursor: NSCursor = .resizeLeftRight
    var onHoverChange: (Bool) -> Void = { _ in }

    func makeNSView(context: Context) -> DividerCursorStripView {
        DividerCursorStripView(cursor: cursor)
    }

    func updateNSView(_ nsView: DividerCursorStripView, context: Context) {
        nsView.cursor = cursor
        nsView.onHoverChange = onHoverChange
    }
}

final class DividerCursorStripView: NSView {
    var cursor: NSCursor {
        didSet { window?.invalidateCursorRects(for: self) }
    }

    /// Reported from the tracking area rather than from `hitTest`: enter and
    /// exit are delivered straight to the owning view, so the strip can report
    /// hover while still refusing every click.
    var onHoverChange: (Bool) -> Void = { _ in }

    private var hoverTracking: NSTrackingArea?

    init(cursor: NSCursor) {
        self.cursor = cursor
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        DividerCursorHitPolicy.acceptsHit(eventType: NSApp.currentEvent?.type)
            ? super.hitTest(point)
            : nil
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: cursor)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let hoverTracking { removeTrackingArea(hoverTracking) }
        // .inVisibleRect keeps the area sized to the strip as the divider
        // moves, which makes the passed rect irrelevant.
        let area = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
            owner: self,
            userInfo: nil)
        addTrackingArea(area)
        hoverTracking = area
    }

    override func mouseEntered(with event: NSEvent) {
        onHoverChange(true)
    }

    override func mouseExited(with event: NSEvent) {
        onHoverChange(false)
    }

    // Hiding a side panel removes the strip without ever delivering an exit,
    // which would leave the seam lit the next time that panel is shown.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil { onHoverChange(false) }
    }

    // The strip follows a divider that moves during a drag, and cursor rects
    // are cached against the old geometry until invalidated.
    override func layout() {
        super.layout()
        window?.invalidateCursorRects(for: self)
    }
}
```

- [ ] **Step 4: Give `dividerSeam` its hovered state**

In `App/ContentView.swift`, add two `@State` properties immediately after `liveRightPanelWidth` (`App/ContentView.swift:30`):

```swift
    @State private var leftSeamHovered = false
    @State private var rightSeamHovered = false
```

Then replace the `dividerSeam` property added in Task 3 with a function:

```swift
    /// `NSSplitView` draws its own hairline between columns. With the columns
    /// flush that line lands right on the seam, so it gets painted over with
    /// the central pane's own surface and disappears into the card — and the
    /// same 2pt strip doubles as the resize affordance, lighting up in the
    /// shared `hairline` grey while the pointer is on the band.
    private func dividerSeam(hovered: Bool) -> some View {
        Rectangle()
            .fill(hovered ? AppTheme.hairline : AppTheme.chatSurface)
            .frame(width: 2)
            .ignoresSafeArea()
            .allowsHitTesting(false)
            .animation(.easeInOut(duration: 0.12), value: hovered)
    }
```

- [ ] **Step 5: Wire both seams**

In `splitContent`, replace the leading overlay body (`App/ContentView.swift:246-255` after Task 3) with:

```swift
                if sidebarVisible {
                    dividerSeam(hovered: leftSeamHovered)
                        .offset(x: CardLayout.dividerCenterX(
                            columnWidth: sidebarWidth, gap: AppTheme.cardGap) - 1)
                    DividerCursorStrip(onHoverChange: { leftSeamHovered = $0 })
                        .frame(width: DividerCursorStrip.width)
                        .offset(x: CardLayout.dividerCenterX(
                            columnWidth: sidebarWidth, gap: AppTheme.cardGap)
                            - DividerCursorStrip.width / 2)
                }
```

and the trailing overlay body with:

```swift
                if rightPanelVisible {
                    dividerSeam(hovered: rightSeamHovered)
                        .offset(x: -CardLayout.dividerCenterX(
                            columnWidth: liveRightPanelWidth, gap: AppTheme.cardGap) + 1)
                    DividerCursorStrip(onHoverChange: { rightSeamHovered = $0 })
                        .frame(width: DividerCursorStrip.width)
                        .offset(x: -CardLayout.dividerCenterX(
                            columnWidth: liveRightPanelWidth, gap: AppTheme.cardGap)
                            + DividerCursorStrip.width / 2)
                }
```

- [ ] **Step 6: Verify it builds and the tests still pass**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
mkdir -p DerivedData
set +e
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  > DerivedData/apptests.log 2>&1
echo "exit=$?"
set -e
grep -E "error:|cardCorners|shadowBleed|seamCentre|DividerCursorStrip" DerivedData/apptests.log | head -30
```

Expected: no `error:` lines. `DividerCursorStripTests` is a known-flaky suite; a failure there must be reproduced on a clean tree before being attributed to this change.

- [ ] **Step 7: Commit**

```bash
git add App/DividerCursorStrip.swift App/ContentView.swift \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DividerCursorRectsTests.swift
git commit -m "feat: light the column seam in hairline grey while the resize band is hovered"
```

---

### Task 5: Full gate

**Files:** none modified.

- [ ] **Step 1: Run the gate**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
Scripts/ci.sh
```

Expected: `CI OK`.

- [ ] **Step 2: If the gate is red, exonerate the diff before touching anything**

The gate has a known red baseline from 2026-08-06. Do not compare against `HEAD` — that comparison is what made this mistake before. Instead run the gate twice on the *same* tree state, and check whether the failing suite is one of the known-flaky ones (`TillerTerminal`'s `PtyProcessTests`, `DividerCursorStripTests`). Only a failure that reproduces deterministically and names a file this plan touched belongs to this work.

- [ ] **Step 3: Hand the manual checklist to the user**

Report the gate result and paste this list. Do not perform these checks — visual verification is the user's.

- [ ] At rest, neither seam shows a dark band.
- [ ] Hovering either seam shows the grey line and the `resizeLeftRight` cursor.
- [ ] Dragging either seam resizes as before, and the line stays lit for the whole drag.
- [ ] Hiding then re-showing the sidebar restores the central card's rounded leading corners, and leaves a full 10pt margin against the window's left edge while hidden.
- [ ] Hiding the right panel does the same on the trailing side.
- [ ] Light mode: the seam is invisible at rest and the hover line is visible.
- [ ] The seam is not left lit after hiding a side panel while hovering its divider.

---

## Known cosmetic side effect

Both `sidebarWidth` and `liveRightPanelWidth` are measured from the padded column frame, which after Task 3 is 5pt narrower than before for the same visible card. `rightPanelWidth` is persisted, so a restored layout renders its card 5pt wider than it did previously — a one-time shift, within `AppSettings.rightPanelWidthRange`, not worth a migration.
