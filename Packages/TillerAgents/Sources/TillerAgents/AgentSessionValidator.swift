import Foundation

/// Cheap read-only pre-flight checks before resuming an agent session.
/// `false` means "definitely stale — restore a fresh shell instead";
/// `true` means "worth trying". Agents without a checkable on-disk store
/// are trusted (a failed resume is visible in the pane, never destructive).
public enum AgentSessionValidator {
    public static func isLikelyValid(
        agentId: String, sessionRef: String, worktreePath: String,
        claudeConfigDir: String, codexHome: String,
        fileManager: FileManager = .default
    ) -> Bool {
        switch agentId {
        case "claude":
            let slug = claudeProjectSlug(worktreePath)
            let path = "\(claudeConfigDir)/projects/\(slug)/\(sessionRef).jsonl"
            return fileManager.fileExists(atPath: path)
        case "codex":
            return codexSessionExists(sessionRef: sessionRef, codexHome: codexHome,
                                      fileManager: fileManager)
        default:
            return true
        }
    }

    /// Claude Code stores per-project sessions under
    /// `<config>/projects/<slug>/` where slug replaces every character
    /// outside [A-Za-z0-9] with "-".
    static func claudeProjectSlug(_ worktreePath: String) -> String {
        String(worktreePath.map { $0.isASCII && ($0.isLetter || $0.isNumber) ? $0 : "-" })
    }

    /// Codex rollout files embed the session UUID in the filename under
    /// `<codexHome>/sessions/`. Missing directory → not valid.
    static func codexSessionExists(sessionRef: String, codexHome: String,
                                   fileManager: FileManager) -> Bool {
        let root = "\(codexHome)/sessions"
        guard let enumerator = fileManager.enumerator(atPath: root) else { return false }
        for case let entry as String in enumerator where entry.contains(sessionRef) {
            return true
        }
        return false
    }
}
