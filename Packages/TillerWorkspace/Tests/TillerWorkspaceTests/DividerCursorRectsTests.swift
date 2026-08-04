import CoreGraphics
import Testing
@testable import TillerWorkspace

@Test func cursorRectsCoverTheGapBetweenSideBySidePanes() {
    let rects = DividerCursorRects.rects(
        subviewFrames: [
            CGRect(x: 0, y: 0, width: 197, height: 400),
            CGRect(x: 203, y: 0, width: 197, height: 400)
        ],
        bounds: CGRect(x: 0, y: 0, width: 400, height: 400),
        isVertical: true)

    #expect(rects == [CGRect(x: 197, y: 0, width: 6, height: 400)])
}

@Test func cursorRectsCoverTheGapBetweenStackedPanes() {
    let rects = DividerCursorRects.rects(
        subviewFrames: [
            CGRect(x: 0, y: 0, width: 400, height: 97),
            CGRect(x: 0, y: 103, width: 400, height: 97)
        ],
        bounds: CGRect(x: 0, y: 0, width: 400, height: 200),
        isVertical: false)

    #expect(rects == [CGRect(x: 0, y: 97, width: 400, height: 6)])
}

/// `WorkspaceNativeSplitView` exchanges the two content frames after layout, so
/// subview order and on-screen order disagree for a stacked split. Measuring by
/// array index would put the cursor rect outside the visible gap — the same
/// trap `DividerPosition.readingOrder` exists to avoid.
@Test func cursorRectsIgnoreSubviewOrderAndFollowGeometry() {
    let swapped = DividerCursorRects.rects(
        subviewFrames: [
            CGRect(x: 0, y: 103, width: 400, height: 97),
            CGRect(x: 0, y: 0, width: 400, height: 97)
        ],
        bounds: CGRect(x: 0, y: 0, width: 400, height: 200),
        isVertical: false)

    #expect(swapped == [CGRect(x: 0, y: 97, width: 400, height: 6)])
}

@Test func cursorRectsHandleNestedSplitsAndDegenerateInput() {
    let three = DividerCursorRects.rects(
        subviewFrames: [
            CGRect(x: 0, y: 0, width: 100, height: 50),
            CGRect(x: 106, y: 0, width: 100, height: 50),
            CGRect(x: 212, y: 0, width: 100, height: 50)
        ],
        bounds: CGRect(x: 0, y: 0, width: 312, height: 50),
        isVertical: true)
    #expect(three.count == 2)

    #expect(DividerCursorRects.rects(
        subviewFrames: [CGRect(x: 0, y: 0, width: 10, height: 10)],
        bounds: CGRect(x: 0, y: 0, width: 10, height: 10),
        isVertical: true).isEmpty)
    #expect(DividerCursorRects.rects(
        subviewFrames: [], bounds: .zero, isVertical: true).isEmpty)
}
