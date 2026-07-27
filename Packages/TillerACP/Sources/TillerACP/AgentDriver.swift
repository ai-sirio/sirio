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
    /// Sets reasoning effort on drivers that support it natively (Codex) or
    /// via a prompt prefix (Claude), bypassing `setConfigOption`'s
    /// server-echo round trip since these agents never report "effort" in
    /// `configOptions`. No-op for drivers without a native effort concept.
    func setEffort(_ effort: String?) async
    /// The fixed effort level list for drivers whose "effort" isn't a
    /// server-reported `configOptions` entry (OpenCode is; Claude and Codex
    /// aren't). Nil when the driver has no effort concept at all.
    func staticEffortOptions() async -> SessionConfigOption?
    func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async
    /// Whether the driver can carry a chosen option back to the agent
    /// (`updatedInput`). Drivers that cannot make question cards fall back to
    /// plain allow/reject.
    nonisolated var supportsStructuredAnswers: Bool { get }
}

public extension AgentDriver {
    nonisolated var supportsStructuredAnswers: Bool { false }
    func setEffort(_ effort: String?) async {}
    func staticEffortOptions() async -> SessionConfigOption? { nil }
}

extension ACPSession: AgentDriver {}
