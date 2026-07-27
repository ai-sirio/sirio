import Foundation

enum PiWire {
    struct Response: Sendable, Equatable {
        var id: String?
        var command: String
        var success: Bool
        var data: JSONValue?
        var error: String?
    }

    enum Message: Sendable, Equatable {
        case response(Response)
        case event(type: String, fields: [String: JSONValue])
    }

    struct DecodeFailure: Error, Sendable, Equatable {
        var reason: String
    }

    static func decode(_ rawLine: Data) throws -> Message {
        var line = rawLine
        if line.last == UInt8(ascii: "\r") { line.removeLast() }
        let value = try JSONDecoder().decode(JSONValue.self, from: line)
        guard case .object(let fields) = value,
              case .string(let type)? = fields["type"] else {
            throw DecodeFailure(reason: "top-level object with string type required")
        }
        if type == "response" {
            guard case .string(let command)? = fields["command"],
                  case .bool(let success)? = fields["success"] else {
                throw DecodeFailure(reason: "response requires command and success")
            }
            return .response(Response(
                id: fields["id"]?.stringValue,
                command: command,
                success: success,
                data: fields["data"],
                error: fields["error"]?.stringValue))
        }
        return .event(type: type, fields: fields)
    }

    static func command(id: String, type: String,
                        fields: [String: JSONValue] = [:]) throws -> Data {
        var object = fields
        object["type"] = .string(type)
        object["id"] = .string(id)
        return try encodedLine(object)
    }

    static func extensionUIValue(id: String, value: String) throws -> Data {
        try encodedLine(["type": .string("extension_ui_response"),
                         "id": .string(id), "value": .string(value)])
    }

    static func extensionUIConfirmation(id: String, confirmed: Bool) throws -> Data {
        try encodedLine(["type": .string("extension_ui_response"),
                         "id": .string(id), "confirmed": .bool(confirmed)])
    }

    static func extensionUICancel(id: String) throws -> Data {
        try encodedLine(["type": .string("extension_ui_response"),
                         "id": .string(id), "cancelled": .bool(true)])
    }

    private static func encodedLine(_ fields: [String: JSONValue]) throws -> Data {
        var data = try JSONEncoder().encode(JSONValue.object(fields))
        data.append(UInt8(ascii: "\n"))
        return data
    }
}
