public enum BrowserEngine {}
import Foundation

public struct BrowserPage: Equatable, Sendable {
    public let url: URL
    public let title: String

    public init(url: URL, title: String) {
        self.url = url
        self.title = title
    }
}

public enum BrowserResult: Equatable, Sendable {
    case page(BrowserPage)
    case value(String)
}
