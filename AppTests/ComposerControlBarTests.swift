import SwiftUI
import Testing

@testable import Tiller

@Suite("ComposerControlBar")
@MainActor
struct ComposerControlBarTests {
    @Test func focusedBorderUsesTheAccentColor() {
        let style = ComposerControlBar.borderStyle(isFocused: true)
        #expect(style.color == Color.accentColor)
        #expect(style.width == 1.5)
    }

    @Test func unfocusedBorderUsesTheSeparatorColor() {
        let style = ComposerControlBar.borderStyle(isFocused: false)
        #expect(style.color != Color.accentColor)
        #expect(style.width == 1)
    }
}
