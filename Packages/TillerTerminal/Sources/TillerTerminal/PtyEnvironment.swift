import Foundation

/// Composes the environment for a spawned pane process.
///
/// Precedence (last wins): base process env < TERM override < caller-supplied
/// extras (TILLER_ENV, TILLER_SOCKET, TILLER_WORKTREE_ID from the app layer) <
/// the pane's own TILLER_PANE_ID. The pane id is appended here — not by the
/// caller — because only PtyRuntime knows which leaf UUID it spawns for.
enum PtyEnvironment {
    static func compose(
        base: [String: String],
        term: String,
        extra: [String: String],
        paneId: UUID
    ) -> [String] {
        var env = base
        env["TERM"] = term
        env.merge(extra) { _, new in new }
        env["TILLER_PANE_ID"] = paneId.uuidString
        return env.map { "\($0.key)=\($0.value)" }
    }
}
