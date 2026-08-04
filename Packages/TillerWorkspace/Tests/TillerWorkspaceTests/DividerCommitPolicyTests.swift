import AppKit
import Testing
import TillerWorkspace

@Suite
struct DividerCommitPolicyTests {
    @Test
    func draggingTracksWithoutCommitting() {
        #expect(DividerCommitPolicy.step(for: .leftMouseDragged) == .move)
    }

    @Test
    func releasingCommits() {
        #expect(DividerCommitPolicy.step(for: .leftMouseUp) == .commit)
    }

    /// A window resize or a reconcile also resizes subviews. Committing there
    /// would write a preferred fraction the user never chose.
    @Test
    func programmaticAndUnrelatedResizesAreIgnored() {
        #expect(DividerCommitPolicy.step(for: nil) == .ignore)
        #expect(DividerCommitPolicy.step(for: .keyDown) == .ignore)
        #expect(DividerCommitPolicy.step(for: .applicationDefined) == .ignore)
    }

    @Test
    func aSideBySideSplitMeasuresTheLeftPane() {
        let position = DividerPosition.readingOrder(
            first: CGRect(x: 0, y: 0, width: 300, height: 400),
            second: CGRect(x: 306, y: 0, width: 294, height: 400),
            bounds: CGRect(x: 0, y: 0, width: 600, height: 400),
            isVertical: true
        )

        #expect(position == 300)
    }

    /// The vertical axis exchanges the two content frames after native layout,
    /// so the subview at index 0 is not the pane Core calls first. Measuring by
    /// position rather than by index survives that.
    @Test
    func aStackedSplitMeasuresTheTopPaneWhicheverSubviewHoldsIt() {
        let top = CGRect(x: 0, y: 106, width: 600, height: 294)
        let bottom = CGRect(x: 0, y: 0, width: 600, height: 100)
        let bounds = CGRect(x: 0, y: 0, width: 600, height: 400)

        #expect(
            DividerPosition.readingOrder(
                first: top, second: bottom, bounds: bounds, isVertical: false
            ) == 294
        )
        #expect(
            DividerPosition.readingOrder(
                first: bottom, second: top, bounds: bounds, isVertical: false
            ) == 294
        )
    }
}
