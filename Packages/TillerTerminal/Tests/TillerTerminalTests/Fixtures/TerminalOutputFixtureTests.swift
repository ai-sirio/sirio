import Foundation
import Testing

@Test func terminalOutputFixtureIsDeterministicAndWrapsScrollback() {
    let first = TerminalOutputFixture.make()
    let second = TerminalOutputFixture.make()

    #expect(first == second)
    #expect(first.count > 256 * 1024 * 3)
    #expect(first.starts(with: TerminalOutputFixture.startSentinel))
    #expect(first.suffix(TerminalOutputFixture.endSentinel.count) == TerminalOutputFixture.endSentinel)
    #expect(first.range(of: Data("\u{1B}[".utf8)) != nil)
    #expect(first.range(of: Data("🚀".utf8)) != nil)

    let body = first.dropFirst(TerminalOutputFixture.startSentinel.count)
        .dropLast(TerminalOutputFixture.endSentinel.count)
    let lines = String(decoding: body, as: UTF8.self).split(separator: "\n")
    let numbers = lines.compactMap { line -> Int? in
        guard let marker = line.range(of: "LINE ") else { return nil }
        let number = line[marker.upperBound...].prefix(while: { $0 != ":" })
        return Int(number)
    }
    #expect(numbers.count == lines.count)
    #expect(numbers == Array(1...numbers.count))
}
