import CoreGraphics
import Testing
@testable import TillerWorkspace

/// The two axes do not have the same minimum, because `preferredGroupSize` is
/// wider than it is tall (240x160). A pane can therefore accept a left/right
/// split while refusing a top/bottom one, or the other way round, purely
/// because of its shape — no content type is involved.
@Test func splitMinimumsDifferPerAxis() {
    // (width - dividerThickness) / 2 >= 240  ->  width >= 486
    #expect(SplitEligibility.check(
        groupSize: CGSize(width: 486, height: 1000),
        placement: .right, isSplittingItsOwnSoleGroup: false).isSuccess)
    #expect(!SplitEligibility.check(
        groupSize: CGSize(width: 485, height: 1000),
        placement: .right, isSplittingItsOwnSoleGroup: false).isSuccess)

    // (height - dividerThickness) / 2 >= 160  ->  height >= 326
    #expect(SplitEligibility.check(
        groupSize: CGSize(width: 1000, height: 326),
        placement: .below, isSplittingItsOwnSoleGroup: false).isSuccess)
    #expect(!SplitEligibility.check(
        groupSize: CGSize(width: 1000, height: 325),
        placement: .below, isSplittingItsOwnSoleGroup: false).isSuccess)
}

/// A wide, short pane accepts left/right and refuses top/bottom. This is the
/// shape that reads as "this pane can only be moved sideways".
@Test func shortWidePaneAcceptsSidewaysSplitsOnly() {
    let short = CGSize(width: 900, height: 300)

    #expect(SplitEligibility.check(
        groupSize: short, placement: .right, isSplittingItsOwnSoleGroup: false).isSuccess)
    #expect(SplitEligibility.check(
        groupSize: short, placement: .left, isSplittingItsOwnSoleGroup: false).isSuccess)
    #expect(!SplitEligibility.check(
        groupSize: short, placement: .below, isSplittingItsOwnSoleGroup: false).isSuccess)
    #expect(!SplitEligibility.check(
        groupSize: short, placement: .above, isSplittingItsOwnSoleGroup: false).isSuccess)
}

private extension Result where Success == Void {
    var isSuccess: Bool { if case .success = self { true } else { false } }
}
