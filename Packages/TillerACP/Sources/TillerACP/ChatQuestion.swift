import Foundation

/// A question put to the user, normalised from either source: an
/// `AskUserQuestion`-shaped tool input, or a plain permission request.
/// Views read this and never touch the raw payloads.
public struct ChatQuestion: Sendable, Equatable {
    public struct Option: Sendable, Equatable, Identifiable {
        public var id: String
        public var label: String
        public var detail: String?
        public var isRejection: Bool

        public init(id: String, label: String, detail: String? = nil,
                    isRejection: Bool = false) {
            self.id = id
            self.label = label
            self.detail = detail
            self.isRejection = isRejection
        }
    }

    public var requestId: JSONRPCID
    public var toolCallId: String
    public var header: String
    public var prompt: String
    public var options: [Option]
    public var chosenOptionId: String?
    /// The turn ended before anyone answered — the reducer cancels pending
    /// permissions on `turnEnded`. Offering the buttons again would be a lie.
    public var isExpired: Bool

    public var isAnswered: Bool { chosenOptionId != nil }
    public var isResolved: Bool { isAnswered || isExpired }

    public static func from(_ call: ToolCallItem) -> ChatQuestion? {
        guard let permission = call.permission else { return nil }
        let chosen: String? = if case .selected(let optionId)? = permission.resolution {
            optionId
        } else {
            nil
        }
        let expired = permission.resolution == .cancelled
        if let structured = structuredQuestion(from: call.rawInput) {
            return ChatQuestion(requestId: permission.requestId,
                                toolCallId: call.toolCallId,
                                header: structured.header,
                                prompt: structured.prompt,
                                options: structured.options,
                                chosenOptionId: chosen,
                                isExpired: expired)
        }
        return ChatQuestion(
            requestId: permission.requestId,
            toolCallId: call.toolCallId,
            header: call.title,
            prompt: "",
            options: permission.options.map {
                Option(id: $0.optionId, label: $0.name,
                       isRejection: $0.kind == .rejectOnce || $0.kind == .rejectAlways)
            },
            chosenOptionId: chosen,
            isExpired: expired)
    }

    /// `AskUserQuestion` input: `{questions: [{header, question, options: [{label, description}]}]}`.
    /// Only the first question is surfaced; multi-question payloads are rare and
    /// the extra ones would need a second card.
    private static func structuredQuestion(from rawInput: JSONValue?)
        -> (header: String, prompt: String, options: [Option])? {
        guard case .object(let input)? = rawInput,
              case .array(let questions)? = input["questions"],
              case .object(let first)? = questions.first,
              case .string(let prompt)? = first["question"] else { return nil }
        let header: String = if case .string(let value)? = first["header"] {
            value
        } else {
            prompt
        }
        guard case .array(let rawOptions)? = first["options"] else { return nil }
        var usedIDs = Set<String>()
        let options: [Option] = rawOptions.enumerated().compactMap { index, value in
            guard case .object(let option) = value,
                  case .string(let label)? = option["label"] else { return nil }
            let detail: String? = if case .string(let text)? = option["description"] {
                text
            } else {
                nil
            }
            let baseID = label.isEmpty ? "option-\(index)" : label
            var id = baseID
            if usedIDs.contains(id) {
                id = "\(baseID)-\(index)"
                var suffix = 2
                while usedIDs.contains(id) {
                    id = "\(baseID)-\(index)-\(suffix)"
                    suffix += 1
                }
            }
            usedIDs.insert(id)
            return Option(id: id, label: label, detail: detail)
        }
        guard !options.isEmpty else { return nil }
        return (header, prompt, options)
    }
}
