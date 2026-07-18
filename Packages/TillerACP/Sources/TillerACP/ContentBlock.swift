import Foundation

/// One prompt/output content block, discriminated by `type` on the wire.
/// Unrecognized types decode as `.unknown` so protocol growth never breaks us.
public enum ContentBlock: Sendable, Equatable {
    case text(String)
    case image(mimeType: String, data: String)
    case resourceLink(uri: String, name: String)
    case resource(uri: String, text: String)
    case unknown(type: String)
}

extension ContentBlock: Codable {
    private enum CodingKeys: String, CodingKey {
        case type, text, mimeType, data, uri, name, resource
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)
        switch type {
        case "text":
            self = .text(try container.decode(String.self, forKey: .text))
        case "image":
            self = .image(mimeType: try container.decode(String.self, forKey: .mimeType),
                          data: try container.decode(String.self, forKey: .data))
        case "resource_link":
            self = .resourceLink(uri: try container.decode(String.self, forKey: .uri),
                                 name: try container.decode(String.self, forKey: .name))
        case "resource":
            // Embedded resource: {type:"resource", resource:{uri, text}}
            let inner = try container.decode(EmbeddedResource.self, forKey: .resource)
            self = .resource(uri: inner.uri, text: inner.text)
        default:
            self = .unknown(type: type)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .text(let text):
            try container.encode("text", forKey: .type)
            try container.encode(text, forKey: .text)
        case .image(let mimeType, let data):
            try container.encode("image", forKey: .type)
            try container.encode(mimeType, forKey: .mimeType)
            try container.encode(data, forKey: .data)
        case .resourceLink(let uri, let name):
            try container.encode("resource_link", forKey: .type)
            try container.encode(uri, forKey: .uri)
            try container.encode(name, forKey: .name)
        case .resource(let uri, let text):
            try container.encode("resource", forKey: .type)
            try container.encode(EmbeddedResource(uri: uri, text: text), forKey: .resource)
        case .unknown(let type):
            try container.encode(type, forKey: .type)
        }
    }

    private struct EmbeddedResource: Codable {
        var uri: String
        var text: String
    }
}
