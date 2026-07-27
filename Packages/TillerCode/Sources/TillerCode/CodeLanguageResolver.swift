import Foundation
import CodeEditLanguages

public typealias TillerCodeLanguage = CodeLanguage

public enum CodeLanguageResolver {
    public static var plainText: TillerCodeLanguage { .default }

    public static func language(for url: URL, contents: String? = nil) -> CodeLanguage {
        CodeLanguage.detectLanguageFrom(
            url: url,
            prefixBuffer: contents.map { String($0.prefix(4_096)) },
            suffixBuffer: contents.map { String($0.suffix(4_096)) })
    }

    public static func language(forFence raw: String) -> CodeLanguage {
        let hint = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let alias = [
            "js": "js", "javascript": "js",
            "ts": "ts", "typescript": "ts",
            "py": "py", "python": "py",
            "sh": "sh", "shell": "sh", "bash": "sh",
            "yml": "yml", "yaml": "yaml",
            "md": "md", "markdown": "md"
        ][hint] ?? hint
        guard !alias.isEmpty, alias != "text", alias != "plaintext" else { return .default }
        return CodeLanguage.detectLanguageFrom(
            url: URL(fileURLWithPath: "/tmp/source.\(alias)"))
    }
}
