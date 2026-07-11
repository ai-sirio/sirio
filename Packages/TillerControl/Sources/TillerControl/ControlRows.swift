import Foundation

/// The control protocol's result payload is a flat `[String: String]`.
/// List responses embed their rows as one JSON-array string value under a
/// single key — this is the shared encoder/decoder for that convention.
public enum ControlRows {
    public static func encode(_ rows: [[String: String]]) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(rows) else { return "[]" }
        return String(decoding: data, as: UTF8.self)
    }

    public static func decode(_ json: String) -> [[String: String]]? {
        try? JSONDecoder().decode([[String: String]].self, from: Data(json.utf8))
    }
}
