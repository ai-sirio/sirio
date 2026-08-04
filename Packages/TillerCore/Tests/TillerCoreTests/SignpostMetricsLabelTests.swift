import Foundation
import Testing
@testable import TillerCore

@Test func lengthLabelRoundsFiniteValues() {
    #expect(SignpostMetrics.lengthLabel(320) == "320")
    #expect(SignpostMetrics.lengthLabel(319.6) == "319")
    #expect(SignpostMetrics.lengthLabel(0) == "0")
    #expect(SignpostMetrics.lengthLabel(-12) == "-12")
}

/// SwiftUI proposes an unbounded width during layout. Converting that to Int
/// traps and takes the whole process down — which is exactly what happened in
/// AgentMarkdownTextView.sizeThatFits, killing the test host mid-run.
@Test func lengthLabelNamesNonFiniteValuesInsteadOfTrapping() {
    #expect(SignpostMetrics.lengthLabel(.infinity) == "unbounded")
    #expect(SignpostMetrics.lengthLabel(-.infinity) == "unbounded")
    #expect(SignpostMetrics.lengthLabel(.nan) == "unbounded")
}

/// `Int(CGFloat.greatestFiniteMagnitude)` also traps: it is finite, but far
/// outside Int's range.
@Test func lengthLabelSurvivesValuesBeyondIntRange() {
    #expect(SignpostMetrics.lengthLabel(.greatestFiniteMagnitude) == "unbounded")
}
