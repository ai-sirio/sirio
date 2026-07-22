import Foundation

/// What ChatController needs from a chat backend, regardless of wire protocol.
/// ACPSession satisfies it as-is; native drivers (Claude stream-json, Codex
/// app-server, OpenCode HTTP) implement the same surface and emit the same
/// canonical ACPSessionEvent/SessionUpdate stream.
public protocol AgentDriver: Actor {
    nonisolated var events: AsyncStream<ACPSessionEvent> { get }
    func start() async throws
    func stop() async
    func connect(cwd: String, resumeSessionId: String?,
                 mcpServers: [McpServerSpec]) async throws -> SessionHandle
    func prompt(_ blocks: [ContentBlock]) async throws -> StopReason
    func cancel() async
    func setMode(_ modeId: String) async throws
    func setModel(_ modelId: String) async throws
    func setConfigOption(id: String, value: String) async throws -> [SessionConfigOption]?
    func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async
}

extension ACPSession: AgentDriver {}
