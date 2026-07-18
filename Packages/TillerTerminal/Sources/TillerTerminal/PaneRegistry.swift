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

    private struct RegistrationWaiter {
        let token: UInt64
        let cont: CheckedContinuation<Bool, Never>
    }

    private var entries: [UUID: Entry] = [:]
    private var registrationWaiters: [UUID: [RegistrationWaiter]] = [:]
    private static let maxCancelledRegistrations = 1_024

    private var cancelledRegistrations: Set<UUID> = []
    private var cancelledRegistrationOrder: [UUID] = []
    private var nextWaiterToken: UInt64 = 0

    public init() {}

    public func register(paneId: UUID, pty: PtyProcess, scrollback: ScrollbackBuffer) {
        guard !consumeCancelledRegistration(paneId) else {
            pty.terminate()
            return
        }
        entries[paneId] = Entry(pty: pty, scrollback: scrollback)
        resolveRegistrationWaiters(paneId: paneId, registered: true)
    }

    public func unregister(paneId: UUID) {
        if var entry = entries.removeValue(forKey: paneId) {
            for (_, waiter) in entry.waiters { waiter.resume(returning: entry.exitCode) }
            entry.waiters.removeAll()
        } else {
            recordCancelledRegistration(paneId)
        }
        resolveRegistrationWaiters(paneId: paneId, registered: false)
    }

    public func cancelRegistration(paneId: UUID) {
        recordCancelledRegistration(paneId)
        if var entry = entries.removeValue(forKey: paneId) {
            entry.pty.terminate()
            for (_, waiter) in entry.waiters { waiter.resume(returning: entry.exitCode) }
            entry.waiters.removeAll()
        }
        resolveRegistrationWaiters(paneId: paneId, registered: false)
    }

    public func waitUntilRegistered(paneId: UUID, timeoutMs: Int) async -> Bool {
        if entries[paneId] != nil { return true }
        if cancelledRegistrations.contains(paneId) { return false }

        let token = nextWaiterToken
        nextWaiterToken += 1
        return await withTaskCancellationHandler {
            await withCheckedContinuation { (cont: CheckedContinuation<Bool, Never>) in
                if entries[paneId] != nil {
                    cont.resume(returning: true)
                    return
                }
                if cancelledRegistrations.contains(paneId) || Task.isCancelled {
                    cont.resume(returning: false)
                    return
                }
                registrationWaiters[paneId, default: []].append(
                    RegistrationWaiter(token: token, cont: cont)
                )
                Task { [paneId] in
                    try? await Task.sleep(for: .milliseconds(max(0, timeoutMs)))
                    self.resolveRegistrationWaiter(
                        paneId: paneId, token: token, registered: false
                    )
                }
            }
        } onCancel: {
            Task {
                await self.resolveRegistrationWaiter(
                    paneId: paneId, token: token, registered: false
                )
            }
        }
    }

    public func isRegistered(paneId: UUID) -> Bool { entries[paneId] != nil }

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

    public func waitExit(paneId: UUID, timeoutMs: Int?) async -> Int32? {
        guard let entry = entries[paneId] else { return nil }
        if let code = entry.exitCode { return code }
        let token = nextWaiterToken
        nextWaiterToken += 1
        return await withTaskCancellationHandler {
            await withCheckedContinuation { (cont: CheckedContinuation<Int32?, Never>) in
                guard var entry = entries[paneId], !Task.isCancelled else {
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
                        self.resolveWaiter(paneId: paneId, token: token)
                    }
                }
            }
        } onCancel: {
            Task { await self.resolveWaiter(paneId: paneId, token: token) }
        }
    }

    private func recordCancelledRegistration(_ paneId: UUID) {
        guard cancelledRegistrations.insert(paneId).inserted else { return }
        cancelledRegistrationOrder.append(paneId)
        if cancelledRegistrations.count > Self.maxCancelledRegistrations {
            let evicted = cancelledRegistrationOrder.removeFirst()
            cancelledRegistrations.remove(evicted)
        }
    }

    private func consumeCancelledRegistration(_ paneId: UUID) -> Bool {
        guard cancelledRegistrations.remove(paneId) != nil else { return false }
        cancelledRegistrationOrder.removeAll { $0 == paneId }
        return true
    }

    private func resolveWaiter(paneId: UUID, token: UInt64) {
        guard var entry = entries[paneId],
              let idx = entry.waiters.firstIndex(where: { $0.token == token }) else { return }
        let cont = entry.waiters.remove(at: idx).cont
        entries[paneId] = entry
        cont.resume(returning: nil)
    }

    private func resolveRegistrationWaiter(
        paneId: UUID, token: UInt64, registered: Bool
    ) {
        guard var waiters = registrationWaiters[paneId],
              let index = waiters.firstIndex(where: { $0.token == token }) else { return }
        let waiter = waiters.remove(at: index)
        if waiters.isEmpty {
            registrationWaiters.removeValue(forKey: paneId)
        } else {
            registrationWaiters[paneId] = waiters
        }
        waiter.cont.resume(returning: registered)
    }

    private func resolveRegistrationWaiters(paneId: UUID, registered: Bool) {
        let waiters = registrationWaiters.removeValue(forKey: paneId) ?? []
        for waiter in waiters { waiter.cont.resume(returning: registered) }
    }
}
