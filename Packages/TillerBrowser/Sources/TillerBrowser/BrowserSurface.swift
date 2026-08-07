import AppKit
import Foundation
@preconcurrency import WebKit

@MainActor
public final class BrowserSurface: NSObject, WKNavigationDelegate, WKUIDelegate {
    public let webView: WKWebView
    public let userAgentPolicy: UserAgentPolicy
    public var onPageChange: ((BrowserPage) -> Void)?
    public var onLoadingChange: ((Bool) -> Void)?
    public var onExternalURL: ((URL) -> Void)?

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
        webView.uiDelegate = self
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
        guard Self.isInAppScheme(url) else {
            NSLog("Tiller ignored agent-originated non-HTTP browser URL: %@", url.absoluteString)
            throw BrowserError.navigationFailed(hint: "Only http, https, file, and about URLs are supported")
        }
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

    public func stop() {
        webView.stopLoading()
        onLoadingChange?(false)
    }

    public func get(_ value: BrowserCommand.Get) async throws -> String {
        try await get(value, selector: nil)
    }

    public func get(_ value: BrowserCommand.Get, selector: String?) async throws -> String {
        switch value {
        case .url:
            guard let url = webView.url else { throw BrowserError.navigationUnavailable }
            return url.absoluteString
        case .text:
            return try await evaluateString(documentValueScript(
                selector: selector, property: "innerText"))
        case .html:
            return try await evaluateString(documentValueScript(
                selector: selector, property: "outerHTML"))
        }
    }

    private func load(_ start: () -> WKNavigation?) async throws -> BrowserPage {
        try await withCheckedThrowingContinuation { continuation in
            navigationContinuation = continuation
            onLoadingChange?(true)
            guard start() != nil else {
                navigationContinuation = nil
                onLoadingChange?(false)
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
        let faviconString = (try? await evaluateString(
            "document.querySelector(\"link[rel~=icon]\")?.href || \"\"")) ?? ""
        return BrowserPage(
            url: url, title: title,
            faviconURL: faviconString.isEmpty ? nil : URL(string: faviconString))
    }

    private func documentValueScript(selector: String?, property: String) -> String {
        let encodedSelector = selector.flatMap { try? JSONEncoder().encode($0) }
            .map { String(decoding: $0, as: UTF8.self) }
        let target = if let encodedSelector {
            "document.querySelector(\(encodedSelector))"
        } else {
            "document.body"
        }
        return "\(target) ? \(target).\(property) : ''"
    }

    private static func validURL(_ string: String) -> URL? {
        guard let url = URL(string: string) else { return nil }
        return validURL(url)
    }

    private static func validURL(_ url: URL) -> URL? {
        guard let scheme = url.scheme, !scheme.isEmpty else { return nil }
        guard url.isFileURL || url.host != nil || url.absoluteString == "about:blank" else {
            return nil
        }
        return url
    }

    private static func isInAppScheme(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased() else { return false }
        return ["http", "https", "file", "about"].contains(scheme)
    }

    public func webView(_ webView: WKWebView, didFinish _: WKNavigation!) {
        let continuation = navigationContinuation
        navigationContinuation = nil
        Task { @MainActor in
            do {
                let page = try await page()
                onLoadingChange?(false)
                onPageChange?(page)
                continuation?.resume(returning: page)
            } catch {
                onLoadingChange?(false)
                continuation?.resume(throwing: error)
            }
        }
    }

    public func webView(
        _: WKWebView,
        didFail _: WKNavigation!,
        withError error: Error
    ) {
        onLoadingChange?(false)
        navigationContinuation?.resume(throwing: BrowserError.navigationFailed(
            hint: error.localizedDescription))
        navigationContinuation = nil
    }

    public func webView(
        _: WKWebView,
        didFailProvisionalNavigation _: WKNavigation!,
        withError error: Error
    ) {
        onLoadingChange?(false)
        navigationContinuation?.resume(throwing: BrowserError.navigationFailed(
            hint: error.localizedDescription))
        navigationContinuation = nil
    }

    public func webView(
        _ webView: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        guard navigationAction.targetFrame == nil else { return nil }
        webView.load(navigationAction.request)
        return nil
    }

    public func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void
    ) {
        guard let url = navigationAction.request.url,
              let scheme = url.scheme?.lowercased() else {
            decisionHandler(.cancel)
            return
        }
        if ["http", "https", "file", "about"].contains(scheme) {
            decisionHandler(.allow)
        } else {
            if navigationAction.navigationType == .linkActivated {
                if let onExternalURL {
                    onExternalURL(url)
                } else {
                    NSWorkspace.shared.open(url)
                }
            } else {
                NSLog("Tiller ignored non-human browser navigation URL: %@", url.absoluteString)
            }
            decisionHandler(.cancel)
        }
    }
}
