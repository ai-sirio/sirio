import Foundation

/// Result of a successful connect: what the app layer needs to render and
/// persist (Plan 2 stores `sessionId`; Plan 3 renders modes).
public struct SessionHandle: Sendable, Equatable {
    public var sessionId: String
    public var agentCapabilities: AgentCapabilities
    public var modes: SessionModeState?
    public var didResume: Bool
}

/// Session-level happenings the view model consumes.
public enum ACPSessionEvent: Sendable {
    case update(SessionUpdate)
    case permissionRequested(requestId: JSONRPCID, toolCall: ToolCallUpdate,
                             options: [PermissionOption])
    case disconnected
}

public enum ACPSessionError: Error, Sendable {
    case notConnected
    case unsupportedProtocolVersion(Int)
}

/// Drives one agent conversation over an ACPClient: handshake, session
/// creation/resume, prompting, cancellation, permission answers, and serving
/// the agent's fs requests through an ACPFileSystem.
public actor ACPSession {
    public static let protocolVersion = 1

    private let client: ACPClient
    private let fileSystem: any ACPFileSystem
    private var sessionId: String?
    private var pumpTask: Task<Void, Never>?
    private let eventContinuation: AsyncStream<ACPSessionEvent>.Continuation
    public nonisolated let events: AsyncStream<ACPSessionEvent>

    public init(client: ACPClient, fileSystem: any ACPFileSystem) {
        self.client = client
        self.fileSystem = fileSystem
        (events, eventContinuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
    }

    /// Starts the underlying client and the incoming-message pump.
    public func start() async throws {
        try await client.start()
        pumpTask = Task { [weak self] in
            guard let self else { return }
            for await incoming in self.client.incoming {
                await self.dispatch(incoming)
            }
            await self.finish()
        }
    }

    public func stop() async {
        pumpTask?.cancel()
        await client.stop()
        finish()
    }

    /// Handshakes and opens (or resumes) a session. Resume happens only when
    /// the agent advertises `loadSession` AND a resumeSessionId is given;
    /// otherwise falls back to a fresh session.
    public func connect(cwd: String, resumeSessionId: String?) async throws -> SessionHandle {
        let initialize = try await client.request(
            "initialize",
            params: InitializeParams(
                protocolVersion: Self.protocolVersion,
                clientCapabilities: ClientCapabilities(
                    fs: FileSystemCapability(readTextFile: true, writeTextFile: true),
                    terminal: false)),
            as: InitializeResult.self)
        guard initialize.protocolVersion == Self.protocolVersion else {
            throw ACPSessionError.unsupportedProtocolVersion(initialize.protocolVersion)
        }

        if let resumeSessionId, initialize.agentCapabilities.loadSession {
            let loaded = try await client.request(
                "session/load",
                params: LoadSessionParams(sessionId: resumeSessionId, cwd: cwd),
                as: LoadSessionResult.self)
            sessionId = resumeSessionId
            return SessionHandle(sessionId: resumeSessionId,
                                 agentCapabilities: initialize.agentCapabilities,
                                 modes: loaded.modes, didResume: true)
        }

        let created = try await client.request(
            "session/new", params: NewSessionParams(cwd: cwd), as: NewSessionResult.self)
        sessionId = created.sessionId
        return SessionHandle(sessionId: created.sessionId,
                             agentCapabilities: initialize.agentCapabilities,
                             modes: created.modes, didResume: false)
    }

    /// Sends one user turn; suspends until the agent finishes it.
    public func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
        guard let sessionId else { throw ACPSessionError.notConnected }
        let result = try await client.request(
            "session/prompt", params: PromptParams(sessionId: sessionId, prompt: blocks),
            as: PromptResult.self)
        return result.stopReason
    }

    public func cancel() async {
        guard let sessionId else { return }
        try? await client.notify("session/cancel", params: CancelParams(sessionId: sessionId))
    }

    public func setMode(_ modeId: String) async throws {
        guard let sessionId else { throw ACPSessionError.notConnected }
        _ = try await client.request(
            "session/set_mode", params: SetModeParams(sessionId: sessionId, modeId: modeId),
            as: JSONValue.self)
    }

    public func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async {
        try? await client.respond(
            to: requestId, result: RequestPermissionResult(outcome: outcome))
    }

    private func dispatch(_ incoming: ACPIncoming) async {
        switch incoming {
        case .notification(let method, let params):
            guard method == "session/update", let params,
                  let note = try? params.decoded(SessionNotification.self) else { return }
            eventContinuation.yield(.update(note.update))

        case .request(let id, let method, let params):
            switch method {
            case "session/request_permission":
                guard let params,
                      let request = try? params.decoded(RequestPermissionParams.self) else {
                    try? await client.respondError(to: id, code: -32602,
                                                   message: "invalid permission params")
                    return
                }
                eventContinuation.yield(.permissionRequested(
                    requestId: id, toolCall: request.toolCall, options: request.options))

            case "fs/read_text_file":
                await serveRead(id: id, params: params)

            case "fs/write_text_file":
                await serveWrite(id: id, params: params)

            default:
                try? await client.respondError(to: id, code: -32601,
                                               message: "method not supported: \(method)")
            }
        }
    }

    private func serveRead(id: JSONRPCID, params: JSONValue?) async {
        do {
            guard let params else { throw ACPSessionError.notConnected }
            let read = try params.decoded(ReadTextFileParams.self)
            let content = try fileSystem.readTextFile(
                path: read.path, line: read.line, limit: read.limit)
            try await client.respond(to: id, result: ReadTextFileResult(content: content))
        } catch {
            try? await client.respondError(to: id, code: -32000,
                                           message: "read failed: \(error)")
        }
    }

    private func serveWrite(id: JSONRPCID, params: JSONValue?) async {
        do {
            guard let params else { throw ACPSessionError.notConnected }
            let write = try params.decoded(WriteTextFileParams.self)
            try fileSystem.writeTextFile(path: write.path, content: write.content)
            try await client.respond(to: id, result: JSONValue.null)
        } catch {
            try? await client.respondError(to: id, code: -32000,
                                           message: "write failed: \(error)")
        }
    }

    private func finish() {
        eventContinuation.yield(.disconnected)
        eventContinuation.finish()
    }
}
