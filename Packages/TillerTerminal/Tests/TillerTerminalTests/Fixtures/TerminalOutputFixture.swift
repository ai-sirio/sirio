import Foundation

enum TerminalOutputFixture {
    static let startSentinel = Data("TILLER_TERMINAL_FIXTURE_START\n".utf8)
    static let endSentinel = Data("\nTILLER_TERMINAL_FIXTURE_END".utf8)

    static func make() -> Data {
        var output = startSentinel
        var generator = TerminalFixtureRNG(seed: 0x7E_1A_1A1)
        var lineNumber = 1
        while output.count < 256 * 1024 * 4 {
            let colour = 1 + Int(generator.next() % 7)
            let token = String(generator.next(), radix: 16)
            let line = "\u{1B}[3\(colour)mLINE \(lineNumber): token=\(token) — café 🚀\u{1B}[0m\n"
            output.append(contentsOf: line.utf8)
            lineNumber += 1
        }
        output.append(endSentinel)
        return output
    }
}

private struct TerminalFixtureRNG: RandomNumberGenerator {
    private var state: UInt64

    init(seed: UInt64) { state = seed }

    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var value = state
        value = (value ^ (value >> 30)) &* 0xBF58_476D_1CE4_E5B9
        value = (value ^ (value >> 27)) &* 0x94D0_49BB_1331_11EB
        return value ^ (value >> 31)
    }
}
