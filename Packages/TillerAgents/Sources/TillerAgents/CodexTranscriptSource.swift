import Foundation
import TillerCore

/// Legge il rollout JSONL nativo di Codex per il pane corrente. Il nome
/// file (`rollout-<data>-<uuid>.jsonl`) incorpora l'id di sessione ma non
/// la sua directory-per-data, quindi la ricerca fa una enumerazione
/// ricorsiva di `~/.codex/sessions` filtrando per suffisso `-<id>.jsonl`.
public struct CodexTranscriptSource: TranscriptSource {
    private let sessionRef: String
    private let homeDirectory: URL

    public init(sessionRef: String,
                homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser) {
        self.sessionRef = sessionRef
        self.homeDirectory = homeDirectory
    }

    public func recentText() -> String? {
        let sessionsRoot = homeDirectory.appendingPathComponent(".codex/sessions")
        guard let enumerator = FileManager.default.enumerator(
            at: sessionsRoot, includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        ) else { return nil }
        let suffix = "-\(sessionRef.lowercased()).jsonl"
        guard let match = (enumerator.compactMap { $0 as? URL })
            .first(where: { $0.lastPathComponent.lowercased().hasSuffix(suffix) })
        else { return nil }
        guard let data = try? Data(contentsOf: match), !data.isEmpty else { return nil }
        let lines = String(decoding: data, as: UTF8.self).split(separator: "\n")
        var chunks: [String] = []
        for line in lines {
            guard let obj = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any],
                  let blocks = obj["content"] as? [[String: Any]] else { continue }
            for block in blocks where block["type"] as? String == "text" {
                if let text = block["text"] as? String { chunks.append(text) }
            }
        }
        guard !chunks.isEmpty else { return nil }
        return chunks.joined(separator: "\n")
    }
}
