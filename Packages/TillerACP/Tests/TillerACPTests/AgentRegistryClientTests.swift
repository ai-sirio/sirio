import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentRegistryClientTests {
    static let registryJSON = Data("""
    {"version": "1.0.0", "agents": [
      {"id": "claude-acp", "name": "Claude Agent", "version": "0.60.0",
       "distribution": {"npx": {"package": "p@0.60.0"}}}]}
    """.utf8)

    func tempCacheURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("registry-\(UUID().uuidString).json")
    }

    @Test func fetchesAndCachesOnFirstCall() async throws {
        let cache = tempCacheURL()
        let client = AgentRegistryClient(cacheURL: cache,
                                         fetch: { Self.registryJSON })
        let registry = try await client.registry()
        #expect(registry.agents.first?.id == "claude-acp")
        #expect(FileManager.default.fileExists(atPath: cache.path))
    }

    @Test func freshCacheSkipsNetwork() async throws {
        let cache = tempCacheURL()
        try Self.registryJSON.write(to: cache)
        let client = AgentRegistryClient(
            cacheURL: cache,
            fetch: { throw URLError(.notConnectedToInternet) })
        let registry = try await client.registry()  // must not hit fetch
        #expect(registry.agents.count == 1)
    }

    @Test func forceRefreshBypassesFreshCache() async throws {
        let cache = tempCacheURL()
        try Self.registryJSON.write(to: cache)
        let client = AgentRegistryClient(
            cacheURL: cache,
            fetch: { Data("""
                {"version": "1.0.1", "agents": []}
                """.utf8) })
        let registry = try await client.registry(forceRefresh: true)
        #expect(registry.version == "1.0.1")
    }

    @Test func fetchFailureFallsBackToStaleCache() async throws {
        let cache = tempCacheURL()
        try Self.registryJSON.write(to: cache)
        let past = Date(timeIntervalSinceNow: -200_000)  // cache older than TTL
        try FileManager.default.setAttributes(
            [.modificationDate: past], ofItemAtPath: cache.path)
        let client = AgentRegistryClient(
            cacheURL: cache,
            fetch: { throw URLError(.notConnectedToInternet) })
        let registry = try await client.registry()
        #expect(registry.agents.count == 1)
    }

    @Test func fetchFailureWithoutCacheThrows() async {
        let client = AgentRegistryClient(
            cacheURL: tempCacheURL(),
            fetch: { throw URLError(.notConnectedToInternet) })
        await #expect(throws: (any Error).self) {
            _ = try await client.registry()
        }
    }
}
