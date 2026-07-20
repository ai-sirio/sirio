import Foundation
import TillerCore

/// Legge il transcript JSONL nativo di Claude Code per il pane corrente.
/// Convenzione: `~/.claude/projects/<worktreePath con "/" sostituito da "-">/<sessionRef>.jsonl`.
public struct ClaudeTranscriptSource: TranscriptSource {
    private let worktreePath: String
    private let sessionRef: String
    private let homeDirectory: URL

    public init(worktreePath: String, sessionRef: String,
                homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser) {
        self.worktreePath = worktreePath
        self.sessionRef = sessionRef
        self.homeDirectory = homeDirectory
    }

    public func recentText() -> String? {
        let escaped = worktreePath.replacingOccurrences(of: "/", with: "-")
        let path = homeDirectory
            .appendingPathComponent(".claude/projects/\(escaped)/\(sessionRef).jsonl")
        guard let data = try? Data(contentsOf: path), !data.isEmpty else { return nil }
        let lines = String(decoding: data, as: UTF8.self).split(separator: "\n")
        var chunks: [String] = []
        for line in lines {
            guard let obj = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any],
                  let message = obj["message"] as? [String: Any] else { continue }
            if let content = message["content"] as? String {
                chunks.append(content)
            } else if let blocks = message["content"] as? [[String: Any]] {
                for block in blocks where block["type"] as? String == "text" {
                    if let text = block["text"] as? String { chunks.append(text) }
                }
            }
        }
        guard !chunks.isEmpty else { return nil }
        return chunks.joined(separator: "\n")
    }
}
