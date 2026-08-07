import Foundation

public struct UserAgentPolicy: Equatable, Sendable {
    public static let safariDesktop =
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        + "AppleWebKit/605.1.15 (KHTML, like Gecko) "
        + "Version/18.0 Safari/605.1.15"

    private let overrideUserAgent: String?

    public init(overrideUserAgent: String? = nil) {
        self.overrideUserAgent = overrideUserAgent
    }

    public var userAgent: String {
        overrideUserAgent ?? Self.safariDesktop
    }

    /// The single override point is intentionally independent of the URL.
    public func userAgent(for _: URL) -> String { userAgent }
}
