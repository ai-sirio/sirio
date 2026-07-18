import Foundation

/// Pure state machine that folds the `session/update` stream (plus permission
/// requests and turn boundaries) into a renderable, persistable transcript.
/// No I/O, no concurrency — fully unit-testable.
public struct TranscriptReducer: Sendable, Equatable {
    public private(set) var items: [TranscriptItem] = []
    public private(set) var currentModeId: String?
    public private(set) var availableCommands: [AvailableCommand] = []

    private var openAgentMessageIndex: Int?
    private var openThoughtIndex: Int?
    private var openUserMessageIndex: Int?
    private var nextOrdinal = 0

    public init() {}

    private mutating func makeId(_ prefix: String) -> String {
        defer { nextOrdinal += 1 }
        return "\(prefix)-\(nextOrdinal)"
    }

    private mutating func closeOpenStreams() {
        if let index = openAgentMessageIndex,
           case .agentMessage(let id, let text, _) = items[index] {
            items[index] = .agentMessage(id: id, text: text, isComplete: true)
        }
        openAgentMessageIndex = nil
        openThoughtIndex = nil
        openUserMessageIndex = nil
    }

    /// Records the user's prompt (called by the session when a turn starts).
    public mutating func userPrompted(_ blocks: [ContentBlock]) {
        closeOpenStreams()
        items.append(.userMessage(id: makeId("user"), blocks: blocks))
    }

    /// Marks the turn finished; open streams complete, pending permissions
    /// attached to tool calls resolve as cancelled.
    public mutating func turnEnded(_ reason: StopReason) {
        closeOpenStreams()
        for index in items.indices {
            guard case .toolCall(var item) = items[index],
                  item.permission?.isPending == true else { continue }
            item.permission?.resolution = .cancelled
            items[index] = .toolCall(item)
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
            openAgentMessageIndex = nil
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
            openAgentMessageIndex = nil
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

        case .unknown:
            break
        }
    }

    private mutating func upsert(_ item: ToolCallItem) {
        if let index = toolCallIndex(item.toolCallId),
           case .toolCall(let existing) = items[index] {
            var merged = item
            merged.permission = existing.permission
            items[index] = .toolCall(merged)
        } else {
            items.append(.toolCall(item))
        }
    }

    private mutating func applyToolCallUpdate(_ update: ToolCallUpdate) {
        if let index = toolCallIndex(update.toolCallId),
           case .toolCall(var item) = items[index] {
            item.merge(update)
            items[index] = .toolCall(item)
        } else {
            // Update for a call we never saw (e.g. mid-stream reconnect):
            // materialize a minimal card rather than dropping information.
            var item = ToolCallItem(toolCallId: update.toolCallId, title: update.title ?? "",
                                    kind: update.kind ?? .other,
                                    status: update.status ?? .pending)
            item.merge(update)
            closeOpenStreams()
            items.append(.toolCall(item))
        }
    }

    private func toolCallIndex(_ toolCallId: String) -> Int? {
        items.lastIndex {
            if case .toolCall(let item) = $0 { return item.toolCallId == toolCallId }
            return false
        }
    }
}
