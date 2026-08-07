import Foundation
@preconcurrency import WebKit

@MainActor
public final class BrowserSurface: NSObject, WKNavigationDelegate {
    public let webView: WKWebView
    public let userAgentPolicy: UserAgentPolicy

    private var navigationContinuation: CheckedContinuation<BrowserPage, Error>?

    public init(
        dataStore: WKWebsiteDataStore = .default(),
        userAgentPolicy: UserAgentPolicy = UserAgentPolicy()
    ) {
        self.userAgentPolicy = userAgentPolicy
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = dataStore
        let webView = WKWebView(frame: .zero, configuration: configuration)
        self.webView = webView
        super.init()
        webView.navigationDelegate = self
        webView.customUserAgent = userAgentPolicy.userAgent
    }

    public func execute(_ command: BrowserCommand) async throws -> BrowserResult {
        switch command {
        case .open(let url):
            return .page(try await open(url))
        case .navigate(let navigation):
            return .page(try await navigate(navigation))
        case .get(let value):
            return .value(try await get(value))
        }
    }

    public func open(_ urlString: String) async throws -> BrowserPage {
        guard let url = Self.validURL(urlString) else {
            throw BrowserError.invalidURL
        }
        return try await open(url)
    }

    public func open(_ url: URL) async throws -> BrowserPage {
        guard Self.validURL(url) != nil else { throw BrowserError.invalidURL }
        return try await load { [webView] in
            webView.load(URLRequest(url: url))
        }
    }

    public func navigate(_ navigation: BrowserCommand.Navigation) async throws -> BrowserPage {
        switch navigation {
        case .back:
            guard webView.canGoBack else { throw BrowserError.navigationUnavailable }
            return try await load { [webView] in webView.goBack() }
        case .forward:
            guard webView.canGoForward else { throw BrowserError.navigationUnavailable }
            return try await load { [webView] in webView.goForward() }
        case .reload:
            guard webView.url != nil else { throw BrowserError.navigationUnavailable }
            return try await load { [webView] in webView.reload() }
        }
    }

    public func get(_ value: BrowserCommand.Get) async throws -> String {
        switch value {
        case .url:
            guard let url = webView.url else { throw BrowserError.navigationUnavailable }
            return url.absoluteString
        case .text:
            return try await evaluateString(
                "document.body ? document.body.innerText : ''")
        case .html:
            return try await evaluateString(
                "document.body ? document.body.outerHTML : ''")
        }
    }

    private func load(_ start: () -> WKNavigation?) async throws -> BrowserPage {
        try await withCheckedThrowingContinuation { continuation in
            navigationContinuation = continuation
            guard start() != nil else {
                navigationContinuation = nil
                continuation.resume(throwing: BrowserError.navigationFailed(
                    hint: "WebKit did not start the navigation"))
                return
            }
        }
    }

    private func evaluateString(_ script: String) async throws -> String {
        do {
            let result = try await webView.evaluateJavaScript(script)
            guard let string = result as? String else {
                throw BrowserError.jsError(hint: "JavaScript did not return a string")
            }
            return string
        } catch let error as BrowserError {
            throw error
        } catch {
            throw BrowserError.jsError(hint: error.localizedDescription)
        }
    }

    private func page() async throws -> BrowserPage {
        guard let url = webView.url else { throw BrowserError.navigationUnavailable }
        let title = (try? await evaluateString("document.title")) ?? webView.title ?? ""
        return BrowserPage(url: url, title: title)
    }

    private static func validURL(_ string: String) -> URL? {
        guard let url = URL(string: string) else { return nil }
        return validURL(url)
    }

    private static func validURL(_ url: URL) -> URL? {
        guard let scheme = url.scheme, !scheme.isEmpty else { return nil }
        guard url.isFileURL || url.host != nil else { return nil }
        return url
    }

    public func webView(_ webView: WKWebView, didFinish _: WKNavigation!) {
        guard let continuation = navigationContinuation else { return }
        navigationContinuation = nil
        Task { @MainActor in
            do {
                continuation.resume(returning: try await page())
            } catch {
                continuation.resume(throwing: error)
            }
        }
    }

    public func webView(
        _: WKWebView,
        didFail _: WKNavigation!,
        withError error: Error
    ) {
        navigationContinuation?.resume(throwing: BrowserError.navigationFailed(
            hint: error.localizedDescription))
        navigationContinuation = nil
    }

    public func webView(
        _: WKWebView,
        didFailProvisionalNavigation _: WKNavigation!,
        withError error: Error
    ) {
        navigationContinuation?.resume(throwing: BrowserError.navigationFailed(
            hint: error.localizedDescription))
        navigationContinuation = nil
    }
}
