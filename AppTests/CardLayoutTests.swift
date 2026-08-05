import Testing
@testable import Tiller

/// Half the gap sits inside each column, so a column's measured width already
/// contains its own half. The divider's centre is therefore the outer margin
/// plus that width — not the width alone, and not the width plus a whole gap.
@Test func dividerCentreLandsInTheMiddleOfTheVisibleGap() {
    #expect(CardLayout.dividerCenterX(columnWidth: 250, gap: 10) == 255)
}

/// With no gap at all the divider sits exactly on the column edge: the formula
/// degrades to today's flush layout instead of drifting.
@Test func dividerCentreIsTheColumnEdgeWhenThereIsNoGap() {
    #expect(CardLayout.dividerCenterX(columnWidth: 250, gap: 0) == 250)
}
