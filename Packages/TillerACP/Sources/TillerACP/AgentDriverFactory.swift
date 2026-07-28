import Foundation

public enum AgentTransportKind: String, Sendable, Codable {
    case native
    case acp
}

/// Creates the driver for an agent id after resolving legacy ids and checking
/// whether native CLIs are available in the user's login-shell PATH.
public enum AgentDriverFactory {
    public static func transportKind(for agentId: String) -> AgentTransportKind {
        switch AgentIdMigration.canonical(agentId) {
        case "claude-acp", "codex-acp", "opencode", "pi": .native
        default: .acp
        }
    }

    /// Returns the binary used by a native driver, or an empty string for ACP
    /// agents and unknown ids. The App layer can use this to build install
    /// hints without consulting Tiller's install manifests.
    public static func nativeBinary(for agentId: String) -> String {
        switch AgentIdMigration.canonical(agentId) {
        case "claude-acp": "claude"
        case "codex-acp": "codex"
        case "opencode": "opencode"
        case "pi": "pi"
        default: ""
        }
    }

    /// `pathProbe` is injectable so tests do not need to depend on the host's
    /// login-shell environment. A native binary is probed once per factory
    /// call, immediately before its driver is constructed.
    public static func makeDriver(
        agentId: String, worktreePath: String,
        installStore: AgentInstallStore,
        permissionMode: PermissionMode, model: String?, effort: String?,
        resumeSessionId: String?,
        pathProbe: @Sendable (String) -> Bool = defaultPathProbe
    ) -> (any AgentDriver)? {
        let rawAgentId = agentId
        let canonicalId = AgentIdMigration.canonical(rawAgentId)
        let nativeResumeSessionId = rawAgentId == "pi-acp" ? nil : resumeSessionId

        if transportKind(for: canonicalId) == .native {
            let binary = nativeBinary(for: canonicalId)
            guard pathProbe(binary) else { return nil }
            return makeNativeDriver(
                agentId: canonicalId, worktreePath: worktreePath,
                permissionMode: permissionMode, model: model, effort: effort,
                resumeSessionId: nativeResumeSessionId)
        }

        guard let spec = AgentLaunchSpec.resolved(id: canonicalId,
                                                   installStore: installStore) else {
            return nil
        }
        let transport = ProcessTransport(
            executable: spec.executable, arguments: spec.arguments,
            cwd: worktreePath,
            environment: AgentLaunchSpec.launchEnvironment(extra: spec.environment),
            onStderrLine: stderrLogger(agentId: canonicalId))
        return ACPSession(
            client: ACPClient(transport: transport),
            fileSystem: WorktreeFileSystem(root: worktreePath))
    }

    private static func makeNativeDriver(
        agentId: String, worktreePath: String,
        permissionMode: PermissionMode, model: String?, effort: String?,
        resumeSessionId: String?
    ) -> any AgentDriver {
        let onStderrLine = stderrLogger(agentId: agentId)

        switch agentId {
        case "claude-acp":
            let launch = ClaudeStreamJSONDriver.launchTransport(
                worktreePath: worktreePath, permissionMode: permissionMode,
                model: model, resumeSessionId: resumeSessionId,
                onStderrLine: onStderrLine)
            let driver = ClaudeStreamJSONDriver(
                transport: launch.transport, permissionMode: permissionMode,
                model: model, resumeSessionId: resumeSessionId, effort: effort,
                pinnedSessionId: launch.sessionId)
            return driver

        case "codex-acp":
            let transport = CodexAppServerDriver.launchTransport(
                worktreePath: worktreePath, onStderrLine: onStderrLine)
            return CodexAppServerDriver(
                client: ACPClient(transport: transport),
                permissionMode: permissionMode, model: model, effort: effort,
                resumeConversationId: resumeSessionId)

        case "opencode":
            let connection = LazyOpenCodeProcessConnection(
                cwd: worktreePath, onStderrLine: onStderrLine)
            return OpenCodeHTTPDriver(
                connection: connection, permissionMode: permissionMode,
                resumeSessionId: resumeSessionId)

        case "pi":
            let transport = PiRPCDriver.launchTransport(
                worktreePath: worktreePath, model: model,
                resumeSessionId: resumeSessionId, onStderrLine: onStderrLine)
            return PiRPCDriver(
                transport: transport, model: model, effort: effort,
                resumeSessionId: resumeSessionId)

        default:
            // This method is only called after the native-id check above.
            fatalError("Unsupported native agent id: \(agentId)")
        }
    }

    private static func stderrLogger(agentId: String) -> @Sendable (String) -> Void {
        { line in NSLog("[chat:\(agentId)] %@", line) }
    }

    @usableFromInline
    internal static func defaultPathProbe(_ binary: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", "command -v \(binary)"]
        let output = Pipe()
        process.standardOutput = output
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }
}

/// The OpenCode process is launched by an async initializer, while the public
/// factory remains synchronous. This adapter defers that initializer until the
/// driver starts consuming its event stream or sends its first request.
private final class LazyOpenCodeProcessConnection: OpenCodeConnection, @unchecked Sendable {
    private enum ConnectionError: Error {
        case closed
    }

    private let cwd: String
    private let onStderrLine: (@Sendable (String) -> Void)?
    private let lock = NSLock()
    private var connection: OpenCodeProcessConnection?
    private var connectionTask: Task<OpenCodeProcessConnection, Error>?
    private var isClosed = false

    private enum Resolution {
        case closed
        case existing(OpenCodeProcessConnection)
        case pending(Task<OpenCodeProcessConnection, Error>)
    }

    init(cwd: String, onStderrLine: (@Sendable (String) -> Void)?) {
        self.cwd = cwd
        self.onStderrLine = onStderrLine
    }

    func request(method: String, path: String, body: Data?) async throws -> Data {
        try await resolvedConnection().request(method: method, path: path, body: body)
    }

    func events() -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            Task {
                do {
                    let connection = try await self.resolvedConnection()
                    for try await event in connection.events() {
                        continuation.yield(event)
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
        }
    }

    func close() async {
        let snapshot = closeSnapshot()

        snapshot.task?.cancel()
        await snapshot.connection?.close()
    }

    private func resolvedConnection() async throws -> OpenCodeProcessConnection {
        let resolution = prepareResolution()
        let task: Task<OpenCodeProcessConnection, Error>
        switch resolution {
        case .closed:
            throw ConnectionError.closed
        case .existing(let connection):
            return connection
        case .pending(let pending):
            task = pending
        }

        do {
            let connection = try await task.value
            if finishResolution(with: connection) {
                await connection.close()
                throw ConnectionError.closed
            }
            return connection
        } catch {
            clearFailedResolution(task)
            throw error
        }
    }

    private func prepareResolution() -> Resolution {
        lock.lock()
        defer { lock.unlock() }
        if isClosed { return .closed }
        if let connection { return .existing(connection) }
        if let connectionTask { return .pending(connectionTask) }

        let task = Task { [cwd, onStderrLine] in
            try await OpenCodeProcessConnection(
                cwd: cwd, onStderrLine: onStderrLine)
        }
        connectionTask = task
        return .pending(task)
    }

    private func finishResolution(with connection: OpenCodeProcessConnection) -> Bool {
        lock.lock()
        self.connection = connection
        connectionTask = nil
        let closed = isClosed
        lock.unlock()
        return closed
    }

    private func clearFailedResolution(
        _ task: Task<OpenCodeProcessConnection, Error>
    ) {
        lock.lock()
        if connectionTask != nil {
            connectionTask = nil
        }
        lock.unlock()
        _ = task
    }

    private func closeSnapshot() -> (
        connection: OpenCodeProcessConnection?,
        task: Task<OpenCodeProcessConnection, Error>?
    ) {
        lock.lock()
        isClosed = true
        let snapshot = (connection: connection, task: connectionTask)
        lock.unlock()
        return snapshot
    }
}
