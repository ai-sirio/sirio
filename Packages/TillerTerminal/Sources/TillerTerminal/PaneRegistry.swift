import Foundation

/// Registry giving non-UI code (control socket, agents) access to live
/// panes: write to stdin, snapshot scrollback, await exit. UI code
/// (PtyRuntime) registers/unregisters; AppModel's control handler consumes.
public actor PaneRegistry {
    public static let shared = PaneRegistry()

    private struct Entry {
        let pty: PtyProcess
        let scrollback: ScrollbackBuffer
        var exitCode: Int32?
        var waiters: [(token: UInt64, cont: CheckedContinuation<Int32?, Never>)] = []
    }

    private var entries: [UUID: Entry] = [:]
    private var nextWaiterToken: UInt64 = 0

    public init() {}

    public func register(paneId: UUID, pty: PtyProcess, scrollback: ScrollbackBuffer) {
        entries[paneId] = Entry(pty: pty, scrollback: scrollback)
    }

    public func unregister(paneId: UUID) {
        if var entry = entries.removeValue(forKey: paneId) {
            // Resolve leftover waiters with whatever we know.
            for (_, waiter) in entry.waiters { waiter.resume(returning: entry.exitCode) }
            entry.waiters.removeAll()
        }
    }

    public func isRegistered(paneId: UUID) -> Bool { entries[paneId] != nil }

    /// PID of the pane's shell process, for foreground-process agent
    /// identification (Layer D). Nil when the pane is gone or never spawned.
    public func shellPid(paneId: UUID) -> pid_t? {
        guard let entry = entries[paneId], entry.exitCode == nil else { return nil }
        let pid = entry.pty.processId
        return pid > 0 ? pid : nil
    }

    public func write(paneId: UUID, data: Data) -> Bool {
        guard let entry = entries[paneId] else { return false }
        entry.pty.write(data)
        return true
    }

    public func snapshot(paneId: UUID) async -> Data? {
        guard let entry = entries[paneId] else { return nil }
        return await entry.scrollback.snapshot()
    }

    public func markExited(paneId: UUID, code: Int32) {
        guard var entry = entries[paneId] else { return }
        entry.exitCode = code
        let waiters = entry.waiters
        entry.waiters = []
        entries[paneId] = entry
        for (_, waiter) in waiters { waiter.resume(returning: code) }
    }

    /// nil timeout = wait forever. Returns nil for unknown pane or timeout.
    ///
    /// Parks a continuation in the pane's waiters list, keyed by a unique
    /// token so that a timeout Task resolves its own waiter rather than the
    /// first parked one. The continuation is resumed by markExited (with the
    /// exit code) or unregister (with whatever code is known).  When a timeout
    /// is set, a background Task fires after the delay and calls resolveWaiter
    /// with the continuation's token, which unhooks and resumes only that
    /// waiter with nil — there is no self-resume on task cancellation, so
    /// entries are never removed without draining their waiters.
    public func waitExit(paneId: UUID, timeoutMs: Int?) async -> Int32? {
        guard let entry = entries[paneId] else { return nil }
        if let code = entry.exitCode { return code }
        let token = nextWaiterToken
        nextWaiterToken += 1
        return await withCheckedContinuation { (cont: CheckedContinuation<Int32?, Never>) in
            guard var entry = entries[paneId] else {
                cont.resume(returning: nil)
                return
            }
            if let code = entry.exitCode {
                cont.resume(returning: code)
                return
            }
            entry.waiters.append((token: token, cont: cont))
            entries[paneId] = entry
            if let timeoutMs {
                Task { [paneId] in
                    try? await Task.sleep(for: .milliseconds(timeoutMs))
                    await self.resolveWaiter(paneId: paneId, token: token)
                }
            }
        }
    }

    /// If the waiter identified by `token` is still parked, resume it with nil
    /// (timeout). Actor-serialised against markExited/unregister, so a
    /// continuation is never double-resumed: whichever drain site runs first
    /// removes it from waiters; subsequent drain sites either skip the already-
    /// removed token or see an empty list and are no-ops.
    private func resolveWaiter(paneId: UUID, token: UInt64) {
        guard var entry = entries[paneId],
              let idx = entry.waiters.firstIndex(where: { $0.token == token }) else { return }
        let cont = entry.waiters.remove(at: idx).cont
        entries[paneId] = entry
        cont.resume(returning: nil)
    }
}
