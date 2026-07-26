import Foundation

/// Pure state machine that folds the `session/update` stream (plus permission
/// requests and turn boundaries) into a renderable, persistable transcript.
/// No I/O, no concurrency — fully unit-testable.
public struct TranscriptReducer: Sendable, Equatable {
    public private(set) var items: [TranscriptItem] = []
    public private(set) var currentModeId: String?
    public private(set) var availableCommands: [AvailableCommand] = []
    public private(set) var contextUsage: ContextUsage?
    /// Seconds each closed turn took, keyed by its turnDivider id. Transient
    /// UI state (message meta); never persisted with the transcript.
    public private(set) var turnDurations: [String: TimeInterval] = [:]
    private var turnStartedAt: Date?

    private var openAgentMessageIndex: Int?
    private var openThoughtIndex: Int?
    private var openUserMessageIndex: Int?
    private var nextOrdinal = 0
    private var usedIDs: Set<String>
    /// Path (grezzi, come inviati dall'agente) dei tool call kind == .edit
    /// completati nel turno corrente; svuotato a inizio e fine turno.
    private var turnEditPaths: [String] = []

    public init(existingIDs: Set<String> = []) {
        usedIDs = existingIDs
    }

    /// Seeds `contextUsage` from a prior session's last known value, before
    /// any live updates apply — used on worktree remount so the ring shows
    /// immediately instead of waiting for the next `usage_update`.
    public mutating func restoreContextUsage(_ usage: ContextUsage?) {
        contextUsage = usage
    }

    private mutating func makeId(_ prefix: String) -> String {
        var id = "\(prefix)-\(nextOrdinal)"
        while usedIDs.contains(id) {
            nextOrdinal += 1
            id = "\(prefix)-\(nextOrdinal)"
        }
        nextOrdinal += 1
        usedIDs.insert(id)
        return id
    }

    public mutating func closeAgentMessage() {
        if let index = openAgentMessageIndex,
           case .agentMessage(let id, let text, _) = items[index] {
            items[index] = .agentMessage(id: id, text: text, isComplete: true)
        }
        openAgentMessageIndex = nil
    }

    private mutating func closeOpenStreams() {
        closeAgentMessage()
        openThoughtIndex = nil
        openUserMessageIndex = nil
    }

    /// Records the user's prompt (called by the session when a turn starts).
    public mutating func userPrompted(_ blocks: [ContentBlock], at date: Date = Date()) {
        turnStartedAt = date
        turnEditPaths = []
        closeOpenStreams()
        items.append(.userMessage(id: makeId("user"), blocks: blocks))
    }

    /// Marks the turn finished; open streams complete, pending permissions
    /// attached to tool calls resolve as cancelled, and a timestamped divider
    /// closes the turn (rendered as the "— 11:56 PM —" separator).
    public mutating func turnEnded(_ reason: StopReason, at date: Date = Date()) {
        closeOpenStreams()
        for index in items.indices {
            guard case .toolCall(var item) = items[index],
                  item.permission?.isPending == true else { continue }
            item.permission?.resolution = .cancelled
            items[index] = .toolCall(item)
        }
        if !turnEditPaths.isEmpty {
            items.append(.editSummary(id: makeId("edits"), paths: turnEditPaths))
            turnEditPaths = []
        }
        if !items.isEmpty {
            let dividerId = makeId("divider")
            if let start = turnStartedAt {
                turnDurations[dividerId] = date.timeIntervalSince(start)
                turnStartedAt = nil
            }
            items.append(.turnDivider(id: dividerId, at: date))
        }
    }

    public mutating func apply(_ update: SessionUpdate) {
        switch update {
        case .agentMessageChunk(let block):
            guard case .text(let chunk) = block else { return }
            if let index = openAgentMessageIndex,
               case .agentMessage(let id, let text, false) = items[index] {
                items[index] = .agentMessage(id: id, text: text + chunk, isComplete: false)
            } else {
                items.append(.agentMessage(id: makeId("agent"), text: chunk, isComplete: false))
                openAgentMessageIndex = items.count - 1
            }
            openThoughtIndex = nil
            openUserMessageIndex = nil

        case .agentThoughtChunk(let block):
            guard case .text(let chunk) = block else { return }
            if let index = openThoughtIndex, case .thought(let id, let text) = items[index] {
                items[index] = .thought(id: id, text: text + chunk)
            } else {
                items.append(.thought(id: makeId("thought"), text: chunk))
                openThoughtIndex = items.count - 1
            }
            closeAgentMessage()
            openUserMessageIndex = nil

        case .userMessageChunk(let block):
            // Live turns never emit these; they arrive when session/load
            // replays the conversation, so they must rebuild user messages.
            guard case .text(let chunk) = block else { return }
            if let index = openUserMessageIndex,
               case .userMessage(let id, var blocks) = items[index] {
                if case .text(let existing) = blocks.last {
                    blocks[blocks.count - 1] = .text(existing + chunk)
                } else {
                    blocks.append(.text(chunk))
                }
                items[index] = .userMessage(id: id, blocks: blocks)
            } else {
                items.append(.userMessage(id: makeId("user"), blocks: [.text(chunk)]))
                openUserMessageIndex = items.count - 1
            }
            closeAgentMessage()
            openThoughtIndex = nil

        case .toolCall(let call):
            closeOpenStreams()
            upsert(ToolCallItem(call))

        case .toolCallUpdate(let update):
            applyToolCallUpdate(update)

        case .plan(let entries):
            if let index = items.lastIndex(where: {
                if case .plan = $0 { return true } else { return false }
            }), case .plan(let id, _) = items[index] {
                items[index] = .plan(id: id, entries: entries)
            } else {
                items.append(.plan(id: makeId("plan"), entries: entries))
            }

        case .availableCommandsUpdate(let commands):
            availableCommands = commands

        case .currentModeUpdate(let modeId):
            currentModeId = modeId

        case .usageUpdate(let usage):
            contextUsage = usage

        case .unknown:
            break
        }
    }

    private mutating func upsert(_ item: ToolCallItem) {
        if let index = toolCallIndex(item.toolCallId),
           case .toolCall(let existing) = items[index] {
            var merged = item
            merged.permission = existing.permission
            merged.terminalOutput = existing.terminalOutput
            merged.terminalExit = existing.terminalExit
            merged.transcriptID = existing.transcriptID
            items[index] = .toolCall(merged)
            recordEditPaths(of: merged)
        } else {
            var item = item
            reservePresentationID(for: &item)
            items.append(.toolCall(item))
            recordEditPaths(of: item)
        }
    }

    private mutating func applyToolCallUpdate(_ update: ToolCallUpdate) {
        if let index = toolCallIndex(update.toolCallId),
           case .toolCall(var item) = items[index] {
            item.merge(update)
            items[index] = .toolCall(item)
            recordEditPaths(of: item)
        } else {
            // Update for a call we never saw (e.g. mid-stream reconnect):
            // materialize a minimal card rather than dropping information.
            var item = ToolCallItem(toolCallId: update.toolCallId, title: update.title ?? "",
                                    kind: update.kind ?? .other,
                                    status: update.status ?? .pending)
            item.merge(update)
            reservePresentationID(for: &item)
            closeOpenStreams()
            items.append(.toolCall(item))
            recordEditPaths(of: item)
        }
    }

    /// Attaches a pending permission to its tool call, creating the card from
    /// the request's embedded tool call payload when we never saw the call.
    public mutating func permissionRequested(requestId: JSONRPCID,
                                             toolCall: ToolCallUpdate,
                                             options: [PermissionOption]) {
        applyToolCallUpdate(toolCall)
        guard let index = toolCallIndex(toolCall.toolCallId),
              case .toolCall(var item) = items[index] else { return }
        item.permission = PermissionState(requestId: requestId, options: options)
        items[index] = .toolCall(item)
    }

    /// Records the user's (or cancellation's) answer to a permission request.
    public mutating func permissionResolved(requestId: JSONRPCID,
                                            resolution: PermissionState.Resolution) {
        for index in items.indices {
            guard case .toolCall(var item) = items[index],
                  item.permission?.requestId == requestId,
                  item.permission?.isPending == true else { continue }
            item.permission?.resolution = resolution
            items[index] = .toolCall(item)
            return
        }
    }

    private mutating func recordEditPaths(of item: ToolCallItem) {
        guard item.kind == .edit, item.status == .completed else { return }
        var paths = item.locations.map(\.path)
        for content in item.content {
            if case .diff(let path, _, _) = content { paths.append(path) }
        }
        for path in paths where !turnEditPaths.contains(path) {
            turnEditPaths.append(path)
        }
    }

    private mutating func reservePresentationID(for item: inout ToolCallItem) {
        guard !usedIDs.contains(item.id) else {
            item.transcriptID = makeId("tool")
            return
        }
        usedIDs.insert(item.id)
    }

    private func toolCallIndex(_ toolCallId: String) -> Int? {
        items.lastIndex {
            if case .toolCall(let item) = $0 { return item.toolCallId == toolCallId }
            return false
        }
    }
}
