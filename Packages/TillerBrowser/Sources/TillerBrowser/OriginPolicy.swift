import Foundation

public struct OriginGrant: Hashable, Codable, Sendable {
    public let worktreeID: UUID
    public let origin: String

    public init(worktreeID: UUID, origin: String) {
        self.worktreeID = worktreeID
        self.origin = origin
    }
}

/// Pure origin classification. WebKit and UI prompting deliberately do not
/// belong here: `isLocal` is the decision every sensitive browser operation
/// consults before it reaches JavaScript, and `origin` is the key a grant is
/// stored under. Whether a grant exists is the App-side store's business — this
/// type deliberately holds no grant set, so there is one source of truth for it.
public enum OriginPolicy {
    public static func origin(for url: URL) -> String? {
        guard let scheme = url.scheme?.lowercased(), !scheme.isEmpty else { return nil }
        guard let host = url.host?.lowercased(), !host.isEmpty else {
            return scheme == "about" ? "about:" : "\(scheme):"
        }
        var result = "\(scheme)://\(host)"
        if let port = url.port { result += ":\(port)" }
        return result
    }

    public static func isLocal(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased() else { return false }
        if scheme == "about" { return true }
        guard scheme == "http" || scheme == "https",
              let host = url.host?.lowercased() else { return false }
        return host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host.hasSuffix(".local")
    }
}
