import Foundation
import TillerACP
import TillerCore

@MainActor
struct ChatTranscriptSource: @MainActor TranscriptSource {
    let controller: ChatController

    func recentText() -> String? {
        Self.extractText(from: controller.items)
    }

    static func extractText(from items: [TranscriptItem]) -> String? {
        var chunks: [String] = []
        for item in items {
            switch item {
            case .userMessage(_, let blocks):
                for block in blocks {
                    if case .text(let text) = block { chunks.append(text) }
                }
            case .agentMessage(_, let text, _):
                chunks.append(text)
            default:
                continue
            }
        }
        guard !chunks.isEmpty else { return nil }
        return chunks.joined(separator: "\n")
    }
}
