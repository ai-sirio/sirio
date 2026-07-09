import Foundation

public struct ControlRequest: Codable, Sendable, Equatable {
    public let id: String
    public let method: String            // "panel.create" | "panel.write" | "panel.read" | "panel.wait" | "notify" | "session.ref" | "worktree.set"
    public let params: [String: String]

    public init(id: String, method: String, params: [String: String]) {
        self.id = id
        self.method = method
        self.params = params
    }
}

public struct ControlResponse: Codable, Sendable, Equatable {
    public let id: String
    public let ok: Bool
    public let result: [String: String]?
    public let error: String?

    private enum CodingKeys: String, CodingKey {
        case id, ok, result, error
    }

    public init(id: String, ok: Bool, result: [String: String]?, error: String?) {
        self.id = id
        self.ok = ok
        self.result = result
        self.error = error
    }

    public static func success(id: String, result: [String: String] = [:]) -> ControlResponse {
        ControlResponse(id: id, ok: true, result: result, error: nil)
    }

    public static func failure(id: String, error: String) -> ControlResponse {
        ControlResponse(id: id, ok: false, result: nil, error: error)
    }
}

public enum ControlFraming {
    private static let encoder: JSONEncoder = {
        let enc = JSONEncoder()
        enc.outputFormatting = [.sortedKeys]
        return enc
    }()

    private static let decoder = JSONDecoder()

    public static func encodeLine<T: Encodable>(_ value: T) throws -> Data {
        var data = try encoder.encode(value)
        data.append(0x0A)
        return data
    }

    public static func decodeRequest(line: Data) throws -> ControlRequest {
        try decoder.decode(ControlRequest.self, from: line)
    }

    public static func decodeResponse(line: Data) throws -> ControlResponse {
        try decoder.decode(ControlResponse.self, from: line)
    }
}

public enum ControlSocket {
    /// $TILLER_SOCKET override, else ~/Library/Application Support/Tiller/control.sock
    public static func defaultPath(environment: [String: String] = ProcessInfo.processInfo.environment) -> String {
        if let override = environment["TILLER_SOCKET"] {
            return override
        }
        let urls = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
        let appSupport = urls.first ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Library/Application Support")
        return appSupport.appendingPathComponent("Tiller/control.sock").path
    }
}
