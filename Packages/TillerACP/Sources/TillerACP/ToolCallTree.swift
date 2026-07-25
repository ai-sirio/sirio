import Foundation

/// Splits a flat transcript into top-level items plus each task's children,
/// in one pass. Called once per items change — never per rendered card.
public enum ToolCallTree {
    public struct Grouped: Sendable, Equatable {
        public var roots: [TranscriptItem]
        public var children: [String: [ToolCallItem]]
        /// Pending plan-mode approval (kind .switchMode), collected here so
        /// views never rescan the transcript to find it.
        public var pendingPlanApproval: PermissionState?

        public func children(of toolCallId: String) -> [ToolCallItem] {
            children[toolCallId] ?? []
        }
    }

    public static func group(items: [TranscriptItem]) -> Grouped {
        var known = Set<String>()
        for item in items {
            if case .toolCall(let call) = item { known.insert(call.toolCallId) }
        }

        var roots: [TranscriptItem] = []
        var children: [String: [ToolCallItem]] = [:]
        var pendingPlan: PermissionState?

        for item in items {
            guard case .toolCall(let call) = item else {
                roots.append(item)
                continue
            }
            if call.kind == .switchMode, let permission = call.permission,
               permission.isPending {
                pendingPlan = permission
            }
            // An unknown parent means the child keeps its place in the flow:
            // information is never hidden because an item is missing.
            if let parent = call.parentToolCallId, known.contains(parent) {
                children[parent, default: []].append(call)
            } else {
                roots.append(item)
            }
        }

        return Grouped(roots: roots, children: children,
                       pendingPlanApproval: pendingPlan)
    }
}
