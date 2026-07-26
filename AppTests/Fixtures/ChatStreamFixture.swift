import Foundation
import TillerACP

struct ChatStreamFixture: Equatable, Sendable {
    let finalMessage: String
    let fragments: [String]
    let transcript: [TranscriptItem]

    static func make() -> Self {
        let finalMessage = makeMessage()
        return Self(
            finalMessage: finalMessage,
            fragments: split(finalMessage, into: 1_024),
            transcript: makeTranscript(finalMessage: finalMessage)
        )
    }

    private static func makeMessage() -> String {
        let words = [
            "actor", "buffer", "cache", "commit", "deterministic", "frame", "layout",
            "latency", "message", "panel", "render", "scrollback", "stream", "task",
            "terminal", "throughput", "transcript", "worktree"
        ]
        var generator = ChatFixtureRNG(seed: 0xC0DE_2026)
        var markdown = "# Streaming performance fixture\n\n"

        for section in 1...56 {
            markdown += "## Section \(section): stable workload\n\n"
            for paragraph in 0..<2 {
                var sentence: [String] = []
                for _ in 0..<30 {
                    let index = Int(generator.next() % UInt64(words.count))
                    sentence.append(words[index])
                }
                markdown += "Paragraph \(paragraph + 1) records a \(sentence.joined(separator: " ")). "
                markdown += "The same seeded sequence keeps every render comparable across runs.\n\n"
            }
            markdown += "- preserve the visible transcript order\n"
            markdown += "- measure only numeric workload counts\n"
            markdown += "- keep hidden worktrees alive\n"
            markdown += "1. append the fragment\n"
            markdown += "2. commit the settled text\n"
            markdown += "3. compare the resulting layout\n"
            markdown += "See [the performance roadmap](https://example.invalid/tiller/performance) for the baseline contract.\n\n"

            if section.isMultiple(of: 8) {
                markdown += "```swift\n"
                markdown += "struct StreamSample {\n"
                markdown += "    let ordinal: Int\n"
                markdown += "    let bytes: Int\n"
                markdown += "}\n"
                markdown += "\n"
                markdown += "let sample = StreamSample(ordinal: \(section), bytes: 52_428)\n"
                markdown += "```\n\n"
            }

            if section.isMultiple(of: 16) {
                markdown += "★ Insight ───\n"
                markdown += "Stable fixtures make regressions visible without exposing user content.\n"
                markdown += "───\n\n"
            }
        }
        return markdown
    }

    private static func split(_ message: String, into fragmentCount: Int) -> [String] {
        let characters = Array(message)
        let baseLength = characters.count / fragmentCount
        let remainder = characters.count % fragmentCount
        var fragments: [String] = []
        fragments.reserveCapacity(fragmentCount)
        var offset = 0
        for index in 0..<fragmentCount {
            let length = baseLength + (index < remainder ? 1 : 0)
            fragments.append(String(characters[offset..<(offset + length)]))
            offset += length
        }
        return fragments
    }

    private static func makeTranscript(finalMessage: String) -> [TranscriptItem] {
        var items: [TranscriptItem] = []
        items.reserveCapacity(204)
        for index in 0..<204 {
            if index == 203 {
                items.append(.agentMessage(id: "agent-final", text: finalMessage, isComplete: true))
                continue
            }
            switch index % 4 {
            case 0:
                items.append(.userMessage(
                    id: "user-\(index)", blocks: [.text("Request \(index): compare the stable workload.")]))
            case 1:
                items.append(.agentMessage(
                    id: "agent-\(index)", text: "Recorded deterministic result \(index).", isComplete: true))
            case 2:
                items.append(.toolCall(ToolCallItem(
                    toolCallId: "tool-\(index)", title: "Read fixture record \(index)",
                    kind: .read, status: .completed)))
            default:
                items.append(.turnDivider(
                    id: "divider-\(index)", at: Date(timeIntervalSince1970: 1_700_000_000 + Double(index))))
            }
        }
        return items
    }
}

private struct ChatFixtureRNG: RandomNumberGenerator {
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
