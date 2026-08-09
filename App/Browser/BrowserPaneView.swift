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
    let drivingState: BrowserDrivingState
    let onPageChange: ((BrowserPage) -> Void)?

    @State private var address: String = ""
    @State private var isLoading = false
    @State private var canGoBack = false
    @State private var canGoForward = false
    @State private var faviconURL: URL?
    @State private var errorMessage: String?
    @FocusState private var addressBarFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            chrome
            if let errorMessage {
                Text(errorMessage)
                    .font(AppFont.caption)
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 6)
                    .background(AppTheme.chatSurface)
                    .accessibilityLabel("Browser error")
            }
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
                do {
                    _ = try await surface.open(initialURL)
                } catch {
                    errorMessage = (error as? BrowserError)?.userMessage
                        ?? error.localizedDescription
                }
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
            if drivingState.isAgentDriving {
                Label("Agent driving", systemImage: "bolt.fill")
                    .font(AppFont.caption.weight(.semibold))
                    .foregroundStyle(.orange)
                    .accessibilityLabel("Agent driving")
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(AppTheme.chatSurface)
    }

    private func openAddress() {
        let trimmed = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        // The engine stays strict so the agent-facing API can answer
        // invalid_url; a human typing "www.google.it" gets the scheme filled in.
        guard let url = UserInputURL.normalize(trimmed) else {
            errorMessage = "\(trimmed) is not a valid address."
            return
        }
        run { try await surface.open(url) }
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
        errorMessage = nil
        Task { @MainActor in
            defer {
                isLoading = false
                updateNavigationState()
            }
            do {
                // `try?` here used to discard the reason entirely, so a failed
                // navigation looked identical to nothing happening at all.
                let page = try await operation()
                address = page.url.absoluteString
                faviconURL = page.faviconURL
                onPageChange?(page)
            } catch {
                errorMessage = (error as? BrowserError)?.userMessage
                    ?? error.localizedDescription
            }
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
