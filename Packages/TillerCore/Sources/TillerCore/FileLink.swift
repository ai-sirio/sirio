import Foundation

public enum FileLink {
    public static func resolve(_ raw: String, worktreePath: String) -> URL? {
        let stripped = raw.replacingOccurrences(
            of: #":\d+(?::\d+)?$"#,
            with: "",
            options: .regularExpression)
        let url: URL
        if let parsed = URL(string: stripped), parsed.scheme == "file" {
            url = URL(fileURLWithPath: parsed.path)
        } else if stripped.hasPrefix("/") {
            url = URL(fileURLWithPath: stripped)
        } else if !stripped.contains("://") {
            url = URL(fileURLWithPath: worktreePath).appendingPathComponent(stripped)
        } else {
            return nil
        }
        return url.standardizedFileURL
    }
}
