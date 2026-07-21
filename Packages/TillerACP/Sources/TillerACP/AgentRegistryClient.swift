import Foundation

/// Fetches the official ACP registry with a disk cache: fresh cache (mtime
/// within maxAge) is served without touching the network; a failed fetch
/// falls back to any cached copy; decoding is validated before the cache is
/// overwritten (a bad payload never clobbers a good cache).
public actor AgentRegistryClient {
    public static let defaultURL = URL(
        string: "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json")!

    private let cacheURL: URL
    private let fetch: @Sendable () async throws -> Data
    private let now: @Sendable () -> Date

    public init(cacheURL: URL,
                fetch: @escaping @Sendable () async throws -> Data,
                now: @escaping @Sendable () -> Date = { Date() }) {
        self.cacheURL = cacheURL
        self.fetch = fetch
        self.now = now
    }

    /// Convenience production initializer.
    public init(cacheURL: URL, session: URLSession = .shared) {
        self.init(cacheURL: cacheURL, fetch: {
            let (data, _) = try await session.data(from: Self.defaultURL)
            return data
        })
    }

    public func registry(maxAge: TimeInterval = 86_400,
                         forceRefresh: Bool = false) async throws -> ACPRegistry {
        if !forceRefresh, let cached = cachedRegistry(),
           let fetchedAt = lastFetchedAt(),
           now().timeIntervalSince(fetchedAt) < maxAge {
            return cached
        }
        do {
            let data = try await fetch()
            let registry = try JSONDecoder().decode(ACPRegistry.self, from: data)
            try? data.write(to: cacheURL, options: .atomic)
            return registry
        } catch {
            if let cached = cachedRegistry() { return cached }
            throw error
        }
    }

    public func lastFetchedAt() -> Date? {
        (try? FileManager.default.attributesOfItem(
            atPath: cacheURL.path))?[.modificationDate] as? Date
    }

    private func cachedRegistry() -> ACPRegistry? {
        guard let data = try? Data(contentsOf: cacheURL) else { return nil }
        return try? JSONDecoder().decode(ACPRegistry.self, from: data)
    }
}
