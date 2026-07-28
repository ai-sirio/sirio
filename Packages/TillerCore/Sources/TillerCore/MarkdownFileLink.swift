import Foundation

/// Traduce la stringa grezza di un link attivato nel terminale (o un path
/// da drag & drop) in un file URL markdown. Unica fonte di verità per le
/// estensioni riconosciute. Pura: non tocca il filesystem — l'esistenza
/// del file la verifica chi apre il documento.
public enum MarkdownFileLink {
    public static let extensions: Set<String> = ["md", "markdown"]

    public static func isMarkdown(_ url: URL) -> Bool {
        extensions.contains(url.pathExtension.lowercased())
    }

    public static func resolve(_ raw: String, worktreePath: String) -> URL? {
        guard let fileURL = FileLink.resolve(raw, worktreePath: worktreePath),
              isMarkdown(fileURL) else { return nil }
        return fileURL
    }
}
