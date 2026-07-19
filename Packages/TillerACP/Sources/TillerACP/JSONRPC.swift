import Foundation

/// JSON-RPC 2.0 request/response identifier (number or string).
public enum JSONRPCID: Sendable, Hashable {
    case number(Int)
    case string(String)
}

extension JSONRPCID: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let number = try? container.decode(Int.self) {
            self = .number(number)
        } else {
            self = .string(try container.decode(String.self))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .number(let value): try container.encode(value)
        case .string(let value): try container.encode(value)
        }
    }
}

/// JSON-RPC 2.0 error object.
public struct JSONRPCError: Error, Sendable, Equatable, Codable {
    public var code: Int
    public var message: String
    public var data: JSONValue?

    public init(code: Int, message: String, data: JSONValue? = nil) {
        self.code = code
        self.message = message
        self.data = data
    }
}

/// One decoded JSON-RPC 2.0 wire message (a single newline-delimited line).
public enum JSONRPCMessage: Sendable, Equatable {
    case request(id: JSONRPCID, method: String, params: JSONValue?)
    case notification(method: String, params: JSONValue?)
    case response(id: JSONRPCID, result: JSONValue?, error: JSONRPCError?)
}

public extension JSONRPCMessage {
    struct DecodeFailure: Error { public let reason: String }

    static func decode(_ line: Data) throws -> JSONRPCMessage {
        let value = try JSONDecoder().decode(JSONValue.self, from: line)
        guard case .object(let fields) = value else {
            throw DecodeFailure(reason: "top-level JSON is not an object")
        }
        let id = try fields["id"].map { try $0.decoded(JSONRPCID.self) }
        if case .string(let method)? = fields["method"] {
            if let id {
                return .request(id: id, method: method, params: fields["params"])
            }
            return .notification(method: method, params: fields["params"])
        }
        guard let id else {
            throw DecodeFailure(reason: "message has neither method nor id")
        }
        let error = try fields["error"].map { try $0.decoded(JSONRPCError.self) }
        return .response(id: id, result: fields["result"], error: error)
    }

    /// Encodes the message as one JSON line terminated by `\n`.
    func encodedLine() throws -> Data {
        var fields: [String: JSONValue] = ["jsonrpc": .string("2.0")]
        switch self {
        case .request(let id, let method, let params):
            fields["id"] = try JSONValue.encoding(id)
            fields["method"] = .string(method)
            fields["params"] = params
        case .notification(let method, let params):
            fields["method"] = .string(method)
            fields["params"] = params
        case .response(let id, let result, let error):
            fields["id"] = try JSONValue.encoding(id)
            fields["result"] = result
            fields["error"] = try error.map { try JSONValue.encoding($0) }
        }
        var data = try JSONEncoder().encode(JSONValue.object(fields))
        data.append(UInt8(ascii: "\n"))
        return data
    }
}
