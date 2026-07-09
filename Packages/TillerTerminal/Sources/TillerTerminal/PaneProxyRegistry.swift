import Foundation

/// Keeps the per-pane `TerminalSurfaceProxy` reachable from the AppKit split
/// host's context-menu handler. Registration/unregistration mirrors pane
/// appearance so the registry only holds live proxies.
@MainActor
public final class PaneProxyRegistry {
    public static let shared = PaneProxyRegistry()
    private var proxies: [UUID: TerminalSurfaceProxy] = [:]
    private let lock = NSLock()

    private init() {}

    public func register(paneId: UUID, proxy: TerminalSurfaceProxy) {
        lock.lock()
        proxies[paneId] = proxy
        lock.unlock()
    }

    public func unregister(paneId: UUID) {
        lock.lock()
        proxies.removeValue(forKey: paneId)
        lock.unlock()
    }

    public func proxy(for paneId: UUID) -> TerminalSurfaceProxy? {
        lock.lock()
        defer { lock.unlock() }
        return proxies[paneId]
    }
}
