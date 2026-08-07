import Foundation
@preconcurrency import WebKit

public enum BrowserWebsiteDataStore {
    /// WebKit owns the storage location for an identifier-based store, so there
    /// is deliberately no Tiller-side directory here: creating one would look
    /// like a cleanup boundary while holding nothing. Removing a worktree's
    /// browsing data goes through `WKWebsiteDataStore.remove(forIdentifier:)`.
    @MainActor
    public static func persistent(for worktreeID: UUID) -> WKWebsiteDataStore {
        WKWebsiteDataStore(forIdentifier: worktreeID)
    }
}
