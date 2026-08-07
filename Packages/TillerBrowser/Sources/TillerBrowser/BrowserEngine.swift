public enum BrowserEngine {}
import Foundation

public struct BrowserPage: Equatable, Sendable {
    public let url: URL
    public let title: String
    public let faviconURL: URL?

    public init(url: URL, title: String, faviconURL: URL? = nil) {
        self.url = url
        self.title = title
        self.faviconURL = faviconURL
    }
}

public enum BrowserResult: Equatable, Sendable {
    case page(BrowserPage)
    case value(String)
    case snapshot(BrowserSnapshot)
}
