import Foundation

/// `claude auth status`'s JSON shape. Only the fields this app reads are
/// modeled; unknown keys are ignored by `Decodable` automatically.
struct ClaudeAuthStatus: Decodable {
    let loggedIn: Bool
    let email: String?
    let orgName: String?
}

/// Parses the non-interactive identity-capture commands run after a
/// successful `claude auth login` / `codex login`, so `AgentAccountStore`
/// can label a newly-added account without scraping a TUI.
public enum AgentAccountIdentity {
    /// `claude auth status` prints clean JSON. Returns `nil` when logged
    /// out or when the output can't be decoded (CLI version drift, etc.).
    public static func parseClaudeAuthStatus(_ output: String) -> (label: String, orgName: String?)? {
        guard let data = output.data(using: .utf8),
              let status = try? JSONDecoder().decode(ClaudeAuthStatus.self, from: data),
              status.loggedIn, let email = status.email
        else { return nil }
        return (label: email, orgName: status.orgName)
    }

    /// `codex login status` has no fixed schema (varies by auth method —
    /// API key vs. ChatGPT OAuth). Best-effort: the first non-empty line,
    /// trimmed, stands in as the display label. Returns `nil` for
    /// empty/whitespace-only output.
    public static func parseCodexLoginStatus(_ output: String) -> String? {
        for line in output.split(separator: "\n", omittingEmptySubsequences: false) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if !trimmed.isEmpty { return trimmed }
        }
        return nil
    }
}
