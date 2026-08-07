import AppKit
import Foundation
@preconcurrency import WebKit

@MainActor
public final class BrowserSurface: NSObject, WKNavigationDelegate, WKUIDelegate {
    public let webView: WKWebView
    public let userAgentPolicy: UserAgentPolicy
    public var onPageChange: ((BrowserPage) -> Void)?
    public var onLoadingChange: ((Bool) -> Void)?
    public var onExternalURL: ((URL, BrowserNavigationOrigin) -> Void)?
    public var onNavigationType: ((WKNavigationType) -> Void)?

    private var navigationContinuation: CheckedContinuation<BrowserPage, Error>?
    private let worktreeID: UUID?
    private let originAuthorization: OriginAuthorization?
    private var generation = 0
    private var latestSnapshot: BrowserSnapshot?
    private var agentNavigationPending = false

    public typealias OriginAuthorization = @MainActor (UUID, URL) async -> Bool

    public init(
        dataStore: WKWebsiteDataStore = .default(),
        userAgentPolicy: UserAgentPolicy = UserAgentPolicy(),
        worktreeID: UUID? = nil,
        originAuthorization: OriginAuthorization? = nil
    ) {
        self.userAgentPolicy = userAgentPolicy
        self.worktreeID = worktreeID
        self.originAuthorization = originAuthorization
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = dataStore
        configuration.userContentController.addUserScript(SnapshotBuilder.userScript)
        configuration.userContentController.addUserScript(BrowserUserScripts.console)
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
        case .snapshot:
            return .snapshot(try await snapshot())
        case .eval(let script):
            return .value(try await eval(script))
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
            try await authorizePageAccess()
            return try await evaluateString(try documentValueScript(
                selector: selector, property: "innerText"))
        case .html:
            try await authorizePageAccess()
            return try await evaluateString(try documentValueScript(
                selector: selector, property: "outerHTML"))
        }
    }

    public func snapshot() async throws -> BrowserSnapshot {
        try await authorizePageAccess()
        generation += 1
        let script = "window.__tillerSnapshot(\(generation))"
        let result = try await webView.evaluateJavaScript(script)
        let snapshot = try SnapshotBuilder.decode(result, generation: generation)
        latestSnapshot = snapshot
        return snapshot
    }

    public func act(_ action: BrowserAct, generation: Int? = nil) async throws -> BrowserActResult {
        try await authorizePageAccess()
        guard actionReferenceIsCurrent(action, generation: generation) else {
            throw BrowserError.staleRef
        }
        agentNavigationPending = true
        let script = try SnapshotBuilder.actionScript(action)
        do {
            _ = try await webView.evaluateJavaScript(script)
            return BrowserActResult()
        } catch let error as BrowserError {
            throw error
        } catch {
            throw BrowserError.jsError(hint: error.localizedDescription)
        }
    }

    public func wait(_ condition: BrowserWaitCondition, timeoutMs: Int) async throws -> Int {
        guard timeoutMs > 0 else {
            throw BrowserError.invalidArgument(hint: "timeoutMs must be a positive integer")
        }
        let started = Date()
        while true {
            if try await waitConditionMatches(condition) {
                return Int(Date().timeIntervalSince(started) * 1_000)
            }
            if Date().timeIntervalSince(started) * 1_000 >= Double(timeoutMs) {
                throw BrowserError.timeout
            }
            try await Task.sleep(for: .milliseconds(50))
        }
    }

    public func eval(_ script: String) async throws -> String {
        try await authorizePageAccess()
        agentNavigationPending = true
        do {
            let result = try await webView.evaluateJavaScript(script)
            return try SnapshotBuilder.jsonString(from: result)
        } catch let error as BrowserError {
            throw error
        } catch {
            throw BrowserError.jsError(hint: error.localizedDescription)
        }
    }

    public func console(since: Double? = nil) async throws -> [BrowserConsoleEntry] {
        try await authorizePageAccess()
        let result = try await webView.evaluateJavaScript(
            "window.__tillerConsoleBuffer || []")
        let entries = try SnapshotBuilder.decodeConsole(result)
        guard let since else { return entries }
        return entries.filter { $0.at >= since }
    }

    /// Pixels are page content. An authenticated page leaks at least as much
    /// through an image as through its text, so capturing lives here behind the
    /// same gate rather than in a caller reaching into `webView` directly.
    public func screenshot() async throws -> NSImage {
        try await authorizePageAccess()
        return try await withCheckedThrowingContinuation { continuation in
            webView.takeSnapshot(with: nil) { image, error in
                if let image {
                    continuation.resume(returning: image)
                } else {
                    continuation.resume(throwing: error ?? BrowserError.navigationFailed(
                        hint: "WebKit snapshot failed"))
                }
            }
        }
    }

    private func load(_ start: () -> WKNavigation?) async throws -> BrowserPage {
        // A navigation started while another is pending used to overwrite the
        // stored continuation, so the first caller was never resumed and waited
        // forever. For an agent driving the surface over the socket that is the
        // worst failure available: the request never answers at all.
        if let superseded = navigationContinuation {
            navigationContinuation = nil
            superseded.resume(throwing: BrowserError.navigationFailed(
                hint: "Superseded by a later navigation on the same surface"))
        }
        return try await withCheckedThrowingContinuation { continuation in
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
        // WebKit does not reliably populate `title` for file and freshly
        // committed documents, so the chrome reads it with JavaScript. This runs
        // after every navigation, including a human typing in the address bar.
        try await authorizePageAccess(for: .chromeMetadata)
        let title = try await evaluateString("document.title")
        let faviconString = try await evaluateString(
            "document.querySelector(\"link[rel~=icon]\")?.href || \"\"")
        return BrowserPage(
            url: url, title: title,
            faviconURL: faviconString.isEmpty ? nil : URL(string: faviconString))
    }

    private func documentValueScript(selector: String?, property: String) throws -> String {
        let encodedSelector = try selector.map {
            String(decoding: try JSONEncoder().encode($0), as: UTF8.self)
        }
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
        agentNavigationPending = false
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

    public func webView(_: WKWebView, didCommit _: WKNavigation!) {
        generation += 1
        latestSnapshot = nil
    }

    public func webView(
        _: WKWebView,
        didFail _: WKNavigation!,
        withError error: Error
    ) {
        agentNavigationPending = false
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
        agentNavigationPending = false
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
        onNavigationType?(navigationAction.navigationType)
        if ["http", "https", "file", "about"].contains(scheme) {
            decisionHandler(.allow)
        } else {
            let origin: BrowserNavigationOrigin
            if agentNavigationPending {
                origin = .agentAction
                agentNavigationPending = false
            } else if navigationAction.navigationType == .linkActivated {
                origin = .humanGesture
            } else {
                NSLog("Tiller ignored non-human browser navigation URL: %@", url.absoluteString)
                decisionHandler(.cancel)
                return
            }
            onExternalURL?(url, origin)
            decisionHandler(.cancel)
        }
    }

    /// Why a script is about to run. The origin gate exists to make agent access
    /// to a page deliberate; it is not a gate on Tiller drawing its own window.
    private enum PageAccessPurpose {
        /// A fixed script Tiller runs to fill in its own chrome (title, favicon).
        /// Never carries agent input and never reaches the page's data.
        case chromeMetadata
        /// Anything an agent asked for.
        case agentRequest
    }

    private func authorizePageAccess(
        for purpose: PageAccessPurpose = .agentRequest
    ) async throws {
        guard let url = webView.url else { throw BrowserError.navigationUnavailable }
        guard purpose == .agentRequest else { return }
        guard let worktreeID else {
            if OriginPolicy.isLocal(url) { return }
            throw BrowserError.originDenied
        }
        // The grant set lives in the App-side store, which `originAuthorization`
        // consults before it prompts — so the local check is the only decision
        // this layer can honestly make on its own.
        if OriginPolicy.isLocal(url) { return }
        guard let originAuthorization,
              await originAuthorization(worktreeID, url) else {
            throw BrowserError.originDenied
        }
    }

    private func actionReferenceIsCurrent(_ action: BrowserAct, generation: Int?) -> Bool {
        let ref: String?
        switch action {
        case .click(let value, _), .fill(let value, _, _), .type(let value, _, _),
             .press(let value, _, _), .scroll(let value, _, _, _):
            ref = value
        }
        guard let ref else { return true }
        guard let latestSnapshot,
              generation == latestSnapshot.generation else { return false }
        return latestSnapshot.nodes.contains { $0.ref == ref }
    }

    private func waitConditionMatches(_ condition: BrowserWaitCondition) async throws -> Bool {
        switch condition {
        case .urlContains(let value):
            return webView.url?.absoluteString.contains(value) == true
        case .loadState(let state):
            guard state == "complete" || state == "loaded" else {
                throw BrowserError.invalidArgument(
                    hint: "loadState must be complete or loaded")
            }
            return !webView.isLoading
        case .selector, .text, .function:
            try await authorizePageAccess()
            let script: String
            switch condition {
            case .selector(let value):
                script = "Boolean(document.querySelector(\(try jsonLiteral(value))))"
            case .text(let value):
                script = "(document.body?.innerText || '').includes(\(try jsonLiteral(value)))"
            case .function(let value):
                script = "Boolean((\(value))())"
            case .urlContains, .loadState:
                throw BrowserError.invalidArgument(hint: "Unsupported wait condition")
            }
            do { return (try await webView.evaluateJavaScript(script) as? Bool) == true }
            catch { throw BrowserError.jsError(hint: error.localizedDescription) }
        }
    }

    private func jsonLiteral(_ value: String) throws -> String {
        String(decoding: try JSONEncoder().encode(value), as: UTF8.self)
    }
}
