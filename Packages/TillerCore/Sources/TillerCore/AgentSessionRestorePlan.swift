import Foundation

/// Decides which stored agent session refs a worktree restore should keep and
/// which it should delete.
///
/// This is the step that made the feature dangerous to get wrong: the refs are
/// keyed by `TerminalContentID`, and anything not matched is deleted. Run
/// against an empty or not-yet-loaded layout it would wipe every ref the user
/// has, so the decision lives here, pure and covered, instead of inline in the
/// restore sequence.
public enum AgentSessionRestorePlan {
    public struct Plan: Equatable, Sendable {
        /// Refs whose terminal is still present, eligible to resume.
        public let resumable: [AgentSessionRef]
        /// Refs pointing at terminals that no longer exist.
        public let prunable: [AgentSessionRef]

        public init(resumable: [AgentSessionRef], prunable: [AgentSessionRef]) {
            self.resumable = resumable
            self.prunable = prunable
        }
    }

    /// `liveContentIDs` must be the restored layout's terminals. Passing an
    /// empty set marks everything prunable, which is correct only for a
    /// worktree that genuinely has no terminals left.
    public static func plan(
        refs: [AgentSessionRef], liveContentIDs: Set<TerminalContentID>
    ) -> Plan {
        var resumable: [AgentSessionRef] = []
        var prunable: [AgentSessionRef] = []
        for ref in refs {
            if liveContentIDs.contains(ref.contentID) {
                resumable.append(ref)
            } else {
                prunable.append(ref)
            }
        }
        return Plan(resumable: resumable, prunable: prunable)
    }
}
