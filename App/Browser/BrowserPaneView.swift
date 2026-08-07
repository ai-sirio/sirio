import AppKit
import SwiftUI
@preconcurrency import WebKit
import TillerBrowser
import TillerCore

@MainActor
struct BrowserPaneView: View {
    let tabID: WorkspaceTabID
    let surface: BrowserSurface
    let initialURL: String?
    let onPageChange: ((BrowserPage) -> Void)?

    @State private var address: String = ""
    @State private var isLoading = false
    @State private var canGoBack = false
    @State private var canGoForward = false
    @State private var faviconURL: URL?
    @FocusState private var addressBarFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            chrome
            BrowserWebView(webView: surface.webView)
        }
        .background(AppTheme.terminalSurface)
        .onReceive(NotificationCenter.default.publisher(
            for: .tillerBrowserFocusAddressBar)) { notification in
                guard let requestedTabID = notification.object as? UUID,
                      requestedTabID == tabID.rawValue else { return }
                addressBarFocused = true
            }
        .task(id: initialURL) {
            if let initialURL, surface.webView.url == nil {
                _ = try? await surface.open(initialURL)
            }
            updateNavigationState()
        }
    }

    private var chrome: some View {
        HStack(spacing: 6) {
            Button { navigate(.back) } label: {
                Image(systemName: "chevron.left")
            }
            .disabled(!canGoBack)
            .help("Back")

            Button { navigate(.forward) } label: {
                Image(systemName: "chevron.right")
            }
            .disabled(!canGoForward)
            .help("Forward")

            Button { reload() } label: {
                Image(systemName: isLoading ? "xmark" : "arrow.clockwise")
            }
            .help(isLoading ? "Stop" : "Reload")

            if let faviconURL {
                AsyncImage(url: faviconURL) { phase in
                    if case .success(let image) = phase { image.resizable() }
                    else { Image(systemName: "globe") }
                }
                .frame(width: 14, height: 14)
            } else {
                Image(systemName: "globe")
                    .frame(width: 14, height: 14)
                    .foregroundStyle(AppTheme.meta)
            }

            TextField("Enter URL", text: $address)
                .textFieldStyle(.roundedBorder)
                .focused($addressBarFocused)
                .onSubmit { openAddress() }
                .accessibilityLabel("Address")

            if isLoading {
                ProgressView()
                    .controlSize(.small)
                    .accessibilityLabel("Loading")
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(AppTheme.chatSurface)
    }

    private func openAddress() {
        let trimmed = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        run { try await surface.open(trimmed) }
    }

    private func navigate(_ action: BrowserCommand.Navigation) {
        run { try await surface.navigate(action) }
    }

    private func reload() {
        if isLoading {
            surface.stop()
            isLoading = false
        } else {
            navigate(.reload)
        }
    }

    private func run(_ operation: @escaping () async throws -> BrowserPage) {
        isLoading = true
        Task { @MainActor in
            defer {
                isLoading = false
                updateNavigationState()
            }
            guard let page = try? await operation() else { return }
            address = page.url.absoluteString
            faviconURL = page.faviconURL
            onPageChange?(page)
        }
    }

    private func updateNavigationState() {
        canGoBack = surface.webView.canGoBack
        canGoForward = surface.webView.canGoForward
        if let url = surface.webView.url { address = url.absoluteString }
    }
}

private struct BrowserWebView: NSViewRepresentable {
    let webView: WKWebView

    func makeNSView(context: Context) -> WKWebView { webView }
    func updateNSView(_ nsView: WKWebView, context: Context) {}
}
