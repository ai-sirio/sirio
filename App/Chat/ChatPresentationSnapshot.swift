import Foundation
import TillerACP

/// Immutable values derived from a chat transcript for one SwiftUI redraw.
struct ChatPresentationSnapshot: Equatable, Sendable {
    struct Cache: Equatable, Sendable {
        fileprivate var agentMessageSegments: [String: CachedAgentMessageSegments] = [:]

        init() {}

        fileprivate init(agentMessageSegments: [String: CachedAgentMessageSegments]) {
            self.agentMessageSegments = agentMessageSegments
        }
    }

    struct BuildResult: Equatable, Sendable {
        let snapshot: ChatPresentationSnapshot
        let cache: Cache
    }

    typealias Segmenter = (String) -> [AgentMessageSegment]

    let items: [TranscriptItem]
    let grouped: ToolCallTree.Grouped
    let composerPermissions: [ComposerPermission]
    let hasPendingPermission: Bool
    let activeSubagentTasks: [SubagentTaskInfo]
    let currentActivity: String?
    let agentMessageSegments: [String: [AgentMessageSegment]]

    var hasPlanAwaitingApproval: Bool {
        grouped.pendingPlanApproval?.isPending == true
    }

    static let empty = ChatPresentationSnapshot(
        items: [], grouped: ToolCallTree.group(items: []),
        composerPermissions: [], hasPendingPermission: false,
        activeSubagentTasks: [], currentActivity: nil,
        agentMessageSegments: [:])

    /// Builds all transcript-derived values in one rebuild.
    /// Agent-message segmentation is reused when both the item ID and value
    /// are unchanged from the previous build.
    static func build(
        items: [TranscriptItem],
        previousCache: Cache = Cache(),
        segmenter: Segmenter = AgentMessageSegmenter.segments
    ) -> BuildResult {
        var segmentsByID: [String: [AgentMessageSegment]] = [:]
        var nextCache: [String: CachedAgentMessageSegments] = [:]
        segmentsByID.reserveCapacity(items.count)
        nextCache.reserveCapacity(previousCache.agentMessageSegments.count)

        for item in items {
            guard case .agentMessage(let id, let text, _) = item else { continue }
            let segments: [AgentMessageSegment]
            if let cached = previousCache.agentMessageSegments[id], cached.item == item {
                segments = cached.segments
            } else {
                segments = segmenter(text)
            }
            segmentsByID[id] = segments
            nextCache[id] = CachedAgentMessageSegments(item: item, segments: segments)
        }

        let grouped = ToolCallTree.group(items: items)
        let hasPendingPermission = items.contains { item in
            guard case .toolCall(let call) = item else { return false }
            return call.permission?.isPending == true
        }

        return BuildResult(
            snapshot: ChatPresentationSnapshot(
                items: items,
                grouped: grouped,
                composerPermissions: ComposerPermissions.extract(from: items),
                hasPendingPermission: hasPendingPermission,
                activeSubagentTasks: SubagentTasks.extract(from: items),
                currentActivity: currentActivity(in: items),
                agentMessageSegments: segmentsByID),
            cache: Cache(agentMessageSegments: nextCache))
    }

    /// Assigns only a changed snapshot, preserving Observation's redraw
    /// behavior when an ACP flush produced no observable presentation change.
    static func assignIfChanged(
        _ current: inout ChatPresentationSnapshot,
        _ candidate: ChatPresentationSnapshot,
        onPublish: () -> Void = {}
    ) {
        guard current != candidate else { return }
        current = candidate
        onPublish()
    }

    func segments(for itemID: String) -> [AgentMessageSegment] {
        agentMessageSegments[itemID] ?? []
    }

    private static func currentActivity(in items: [TranscriptItem]) -> String? {
        for item in items.reversed() {
            guard case .toolCall(let call) = item else { continue }
            if call.status == .pending || call.status == .inProgress { return call.title }
        }
        return nil
    }
}

private struct CachedAgentMessageSegments: Equatable, Sendable {
    let item: TranscriptItem
    let segments: [AgentMessageSegment]
}
