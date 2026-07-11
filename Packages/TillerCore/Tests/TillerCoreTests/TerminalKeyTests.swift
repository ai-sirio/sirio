import Testing
import Foundation
@testable import TillerCore

@Suite struct TerminalKeyTests {
    @Test func allNamesParse() {
        let names = ["enter", "tab", "escape", "backspace", "delete",
                     "up", "down", "left", "right"]
        for name in names {
            #expect(TerminalKey(rawValue: name) != nil, "\(name) should parse")
        }
        #expect(TerminalKey(rawValue: "banana") == nil)
    }

    @Test func encodesControlBytes() {
        #expect(TerminalKey.enter.bytes == Data([0x0D]))
        #expect(TerminalKey.tab.bytes == Data([0x09]))
        #expect(TerminalKey.escape.bytes == Data([0x1B]))
        #expect(TerminalKey.backspace.bytes == Data([0x7F]))
    }

    @Test func encodesEscapeSequences() {
        #expect(TerminalKey.up.bytes == Data("\u{1B}[A".utf8))
        #expect(TerminalKey.down.bytes == Data("\u{1B}[B".utf8))
        #expect(TerminalKey.right.bytes == Data("\u{1B}[C".utf8))
        #expect(TerminalKey.left.bytes == Data("\u{1B}[D".utf8))
        #expect(TerminalKey.delete.bytes == Data("\u{1B}[3~".utf8))
    }
}
