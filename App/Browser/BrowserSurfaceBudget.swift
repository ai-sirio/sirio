import Foundation
import TillerCore

@MainActor
final class BrowserSurfaceBudget {
    static let maxHiddenLiveSurfaces = 3

    enum Activation: Equatable {
        case existing
        case reload(url: String)
    }

    private struct Entry {
        var persistedURL: String
        var isVisible = false
        var isLive = true
        /// An agent has driven this surface and may still hold refs into it.
        /// Sticky on purpose: there is no signal for "the agent is finished", and
        /// reclaiming a surface mid-loop destroys work the agent cannot recover.
        var isAgentAttached = false
        var lastVisibleOrder: UInt64?
    }

    private var entries: [BrowserContentID: Entry] = [:]
    private var nextVisibleOrder: UInt64 = 0

    func register(_ contentID: BrowserContentID, persistedURL: String) {
        if var entry = entries[contentID] {
            entry.persistedURL = persistedURL
            entry.isLive = true
            entries[contentID] = entry
        } else {
            entries[contentID] = Entry(persistedURL: persistedURL)
        }
    }

    func remove(_ contentID: BrowserContentID) {
        entries.removeValue(forKey: contentID)
    }

    func updatePersistedURL(_ url: String, for contentID: BrowserContentID) {
        guard var entry = entries[contentID] else { return }
        entry.persistedURL = url
        entries[contentID] = entry
    }

    func activate(_ contentID: BrowserContentID) -> Activation? {
        guard var entry = entries[contentID] else { return nil }
        nextVisibleOrder += 1
        let activation: Activation = entry.isLive
            ? .existing
            : .reload(url: entry.persistedURL)
        entry.isLive = true
        entry.isVisible = true
        entry.lastVisibleOrder = nextVisibleOrder
        entries[contentID] = entry
        return activation
    }

    /// Records that an agent is driving this surface and reports whether it must
    /// be reloaded first. A hidden worktree is the normal place for an agent to
    /// work while the human looks elsewhere, so an agent request counts as use
    /// just like a visible pane does.
    func attachAgent(_ contentID: BrowserContentID, persistedURL: String) -> Activation {
        var entry = entries[contentID] ?? Entry(persistedURL: persistedURL)
        let activation: Activation = entry.isLive ? .existing : .reload(url: entry.persistedURL)
        entry.isAgentAttached = true
        entry.isLive = true
        entries[contentID] = entry
        return activation
    }

    @discardableResult
    func deactivate(_ contentID: BrowserContentID) -> [BrowserContentID] {
        guard var entry = entries[contentID] else { return [] }
        entry.isVisible = false
        entries[contentID] = entry
        return evictOverBudgetHiddenSurfaces()
    }

    var liveCount: Int { entries.values.filter(\.isLive).count }

    var hiddenLiveIDs: [BrowserContentID] {
        entries.compactMap { id, entry in
            entry.isLive && !entry.isVisible ? id : nil
        }
    }

    func isLive(_ contentID: BrowserContentID) -> Bool {
        entries[contentID]?.isLive == true
    }

    private func evictOverBudgetHiddenSurfaces() -> [BrowserContentID] {
        let hidden = entries.filter { _, entry in
            entry.isLive && !entry.isVisible && !entry.isAgentAttached
        }
        let evictionCount = hidden.count - Self.maxHiddenLiveSurfaces
        guard evictionCount > 0 else { return [] }

        // Eviction is least-recently-visible first. The UUID tie-breaker keeps
        // the result deterministic when two surfaces have never been visible.
        let victims = hidden.sorted { lhs, rhs in
            let leftOrder = lhs.value.lastVisibleOrder ?? 0
            let rightOrder = rhs.value.lastVisibleOrder ?? 0
            if leftOrder != rightOrder { return leftOrder < rightOrder }
            return lhs.key.rawValue.uuidString < rhs.key.rawValue.uuidString
        }.prefix(evictionCount).map(\.key)

        for contentID in victims {
            guard var entry = entries[contentID] else { continue }
            entry.isLive = false
            entries[contentID] = entry
        }
        return victims
    }
}
