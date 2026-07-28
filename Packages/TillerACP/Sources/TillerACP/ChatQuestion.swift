import Foundation

/// A question put to the user, normalised from either source: an
/// `AskUserQuestion`-shaped tool input, or a plain permission request.
/// Views read this and never touch the raw payloads.
public struct ChatQuestion: Sendable, Equatable {
    public struct TextInput: Sendable, Equatable {
        public var placeholder: String?
        public var prefill: String?

        public init(placeholder: String? = nil, prefill: String? = nil) {
            self.placeholder = placeholder
            self.prefill = prefill
        }
    }

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
    public var textInput: TextInput?
    public var chosenOptionId: String?
    /// Built from a structured `AskUserQuestion`-shaped input rather than a raw
    /// permission gate, so the chosen option can be sent back as a value the
    /// agent understands. Do not infer this from `prompt` being non-empty —
    /// Pi's `ui/select` repeats its header there and the prompt is dropped.
    public var isStructured: Bool = false
    /// The turn ended before anyone answered — the reducer cancels pending
    /// permissions on `turnEnded`. Offering the buttons again would be a lie.
    public var isExpired: Bool

    public var isAnswered: Bool { chosenOptionId != nil }
    public var isResolved: Bool { isAnswered || isExpired }
    /// Whether the card offers any way to answer. Pi's `ui/input` requests
    /// carry a text field and no options, so option count alone would hide
    /// them — and a hidden question deadlocks the composer, which stays
    /// locked while the permission is pending.
    public var hasControls: Bool { !options.isEmpty || textInput != nil }

    private struct StructuredQuestion {
        var header: String
        var prompt: String
        var options: [Option]
        var textInput: TextInput?
    }

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
                                textInput: structured.textInput,
                                chosenOptionId: chosen,
                                isStructured: true,
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
            textInput: nil,
            chosenOptionId: chosen,
            isExpired: expired)
    }

    /// `AskUserQuestion` input: `{questions: [{header, question, options: [{label, description}]}]}`.
    /// Only the first question is surfaced; multi-question payloads are rare and
    /// the extra ones would need a second card.
    private static func structuredQuestion(from rawInput: JSONValue?) -> StructuredQuestion? {
        guard case .object(let input)? = rawInput,
              case .array(let questions)? = input["questions"],
              case .object(let first)? = questions.first,
              case .string(let prompt)? = first["question"] else { return nil }
        let textInput: TextInput? = if case .object(let metadata)? = input["_tillerTextInput"] {
            TextInput(placeholder: metadata["placeholder"]?.stringValue,
                      prefill: metadata["prefill"]?.stringValue)
        } else {
            nil
        }
        let header: String = if case .string(let value)? = first["header"] {
            value
        } else {
            prompt
        }
        // Pi sends `header` and `question` set to the same string for
        // `ui/select`, and a missing header falls back to the question above —
        // either way the card would print the same sentence twice.
        let whitespace = CharacterSet.whitespacesAndNewlines
        let body = header.trimmingCharacters(in: whitespace)
            == prompt.trimmingCharacters(in: whitespace) ? "" : prompt
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
            let baseID = option["id"]?.stringValue ?? (label.isEmpty ? "option-\(index)" : label)
            let rejection = option["isRejection"]?.boolValue ?? false
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
            return Option(id: id, label: label, detail: detail, isRejection: rejection)
        }
        guard !options.isEmpty || textInput != nil else { return nil }
        return StructuredQuestion(header: header, prompt: body, options: options,
                                  textInput: textInput)
    }
}
