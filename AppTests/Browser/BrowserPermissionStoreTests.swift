import Foundation
import Testing
import TillerBrowser
@testable import Tiller

/// The store is what makes the origin prompt appear *once*. Its granted path
/// returns before any `NSAlert`, so it is the only part testable without a
/// human at the keyboard — and it is the part carrying the security promise.
@MainActor
@Suite struct BrowserPermissionStoreTests {
    private let worktreeID = UUID(uuidString: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE")!

    @Test func aGrantedOriginIsAllowedWithoutPrompting() async throws {
        let defaults = try makeDefaults(granting: "https://example.test")
        let store = BrowserPermissionStore(defaults: defaults)
        let url = try #require(URL(string: "https://example.test/a"))

        // Would hang on a modal if the grant were not honoured.
        #expect(await store.requestAccess(worktreeID: worktreeID, url: url))
    }

    @Test func grantsSurviveARestartAndAreScopedToOriginAndWorktree() throws {
        let defaults = try makeDefaults(granting: "https://example.test")

        let reloaded = BrowserPermissionStore(defaults: defaults)

        #expect(reloaded.grants.contains(
            OriginGrant(worktreeID: worktreeID, origin: "https://example.test")))
        #expect(!reloaded.grants.contains(
            OriginGrant(worktreeID: UUID(), origin: "https://example.test")))
        #expect(!reloaded.grants.contains(
            OriginGrant(worktreeID: worktreeID, origin: "https://other.test")))
    }

    /// `NSAlert.runModal()` used to answer itself with the default button when
    /// the app was not active, granting access with nobody at the keyboard.
    /// Nothing may grant a permission except an answer to the question.
    @Test func anUngrantedOriginIsDeniedWhenThereIsNobodyToAsk() async throws {
        let defaults = try makeDefaults()
        let store = BrowserPermissionStore(defaults: defaults, hostWindow: { nil })
        let url = try #require(URL(string: "https://never-granted.test/a"))

        #expect(await store.requestAccess(worktreeID: worktreeID, url: url) == false)
        #expect(store.grants.isEmpty)
    }

    @Test func revokingRemovesTheGrantFromDisk() throws {
        let defaults = try makeDefaults(granting: "https://example.test")
        let store = BrowserPermissionStore(defaults: defaults)
        let grant = OriginGrant(worktreeID: worktreeID, origin: "https://example.test")

        store.revoke(grant)

        #expect(BrowserPermissionStore(defaults: defaults).grants.isEmpty)
    }

    /// Seeds the grant the way a previous run would have left it on disk, so no
    /// test ever reaches the alert inside `requestAccess`.
    private func makeDefaults(granting origin: String? = nil) throws -> UserDefaults {
        let suite = "dev.tiller.tests.browser-grants.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defaults.removePersistentDomain(forName: suite)
        if let origin {
            let grants = [OriginGrant(worktreeID: worktreeID, origin: origin)]
            defaults.set(try JSONEncoder().encode(grants),
                         forKey: BrowserPermissionStore.defaultsKey)
        }
        return defaults
    }
}
