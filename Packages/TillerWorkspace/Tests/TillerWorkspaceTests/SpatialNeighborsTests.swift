import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite
struct SpatialNeighborsTests {
    @Test
    func neighborPrefersGreatestOrthogonalOverlapThenShortestEdgeDistance() {
        let origin = PaneGroupID()
        let farWithFullOverlap = PaneGroupID()
        let nearWithFullOverlap = PaneGroupID()
        let nearWithPartialOverlap = PaneGroupID()
        let frames = [
            origin: CGRect(x: 0, y: 0, width: 100, height: 100),
            farWithFullOverlap: CGRect(x: 140, y: 0, width: 50, height: 100),
            nearWithFullOverlap: CGRect(x: 110, y: 0, width: 20, height: 100),
            nearWithPartialOverlap: CGRect(x: 101, y: 40, width: 20, height: 60)
        ]

        let result = SpatialNeighbors.neighbor(
            of: origin,
            direction: .right,
            frames: frames,
            readingOrder: [origin, farWithFullOverlap, nearWithFullOverlap, nearWithPartialOverlap]
        )

        #expect(result == nearWithFullOverlap)
    }

    @Test
    func focusNeverWrapsAtTheOuterEdge() {
        let leftmost = PaneGroupID()
        let rightmost = PaneGroupID()
        let frames = [
            leftmost: CGRect(x: 0, y: 0, width: 100, height: 100),
            rightmost: CGRect(x: 120, y: 0, width: 100, height: 100)
        ]

        #expect(SpatialNeighbors.neighbor(
            of: rightmost,
            direction: .right,
            frames: frames,
            readingOrder: [leftmost, rightmost]
        ) == nil)
        #expect(SpatialNeighbors.neighbor(
            of: leftmost,
            direction: .left,
            frames: frames,
            readingOrder: [leftmost, rightmost]
        ) == nil)
    }

    @Test
    func tiesBreakByReadingOrderDeterministically() {
        let origin = PaneGroupID()
        let firstCandidate = PaneGroupID()
        let secondCandidate = PaneGroupID()
        let frames = [
            origin: CGRect(x: 0, y: 0, width: 100, height: 100),
            firstCandidate: CGRect(x: 120, y: 10, width: 40, height: 80),
            secondCandidate: CGRect(x: 120, y: 10, width: 40, height: 80)
        ]

        let result = SpatialNeighbors.neighbor(
            of: origin,
            direction: .right,
            frames: frames,
            readingOrder: [origin, secondCandidate, firstCandidate]
        )

        #expect(result == secondCandidate)
    }
}
