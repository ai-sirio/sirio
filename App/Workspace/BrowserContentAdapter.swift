import AppKit
import Foundation
import SwiftUI
@preconcurrency import WebKit
import TillerBrowser
import TillerCore
import TillerWorkspace

@MainActor
final class BrowserContentAdapter: WorkspaceContentAdapter {
    let kind: WorkspaceContentKind = .browser

    private var tabs: [WorkspaceTabID: WorkspaceTab] = [:]
    private var worktreeIDs: [WorkspaceTabID: UUID] = [:]
    private var surfaces: [BrowserContentID: BrowserSurface] = [:]
    private var liveRecords: [BrowserContentID: BrowserContentRecordValue] = [:]
    private var restoredRecords: [BrowserContentID: BrowserContentRecordValue] = [:]
    private var faviconURLs: [BrowserContentID: URL] = [:]
    private var initialURLs: [BrowserContentID: String] = [:]

    /// The coordinator owns persistence and tab titles. The adapter reports
    /// every committed page change through this callback, including changes
    /// initiated by the chrome rather than the control socket.
    var onPageChange: ((WorkspaceTabID, BrowserPage) -> Void)?

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        guard case .newBrowser(let url) = request else {
            throw ContentAdapterError.unsupportedRequest
        }
        let contentID = BrowserContentID()
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "Browser", titleIsAutoNamed: false,
            content: .browser(contentID))
        let token = AdapterRuntimeToken(
            tabID: tab.id, contentID: contentID.rawValue.uuidString,
            generationID: ResourceGenerationID())
        tabs[tab.id] = tab
        worktreeIDs[tab.id] = worktree.id
        let initialURL: String
        if let url, !url.isEmpty { initialURL = url } else { initialURL = "about:blank" }
        initialURLs[contentID] = initialURL
        liveRecords[contentID] = BrowserContentRecordValue(
            id: contentID, worktreeID: worktree.id, url: initialURL, title: "Browser")
        return PreparedContent(tab: tab, generationID: token.generationID, opaqueToken: token)
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {
        tabs[tab.id] = tab
        worktreeIDs[tab.id] = worktree.id
    }

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        guard case .browser(let contentID) = tab.content else {
            return WorkspaceContentHostAdapter(
                tabID: tab.id, viewController: NSViewController())
        }
        let surface = surface(for: contentID)
        surface.onPageChange = { [weak self] page in
            self?.recordPage(page, for: tab.id, contentID: contentID)
        }
        let initialURL = initialURLs[contentID] ?? restoredRecords[contentID]?.url
        let viewController = NSHostingController(
            rootView: BrowserPaneView(
                tabID: tab.id,
                surface: surface,
                initialURL: initialURL,
                onPageChange: nil))
        return WorkspaceContentHostAdapter(
            tabID: tab.id,
            viewController: viewController,
            focus: { _ in
                surface.webView.window?.makeFirstResponder(surface.webView)
                return true
            },
            release: { [weak self] in
                self?.stop(contentID: contentID)
            })
    }

    func checkpoint(tab: WorkspaceTab) async {}

    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID {
        tabs[tab.id] = tab
        return ResourceGenerationID()
    }

    func close(tab: WorkspaceTab) async {
        guard case .browser(let contentID) = tab.content else { return }
        stop(contentID: contentID)
        tabs.removeValue(forKey: tab.id)
        worktreeIDs.removeValue(forKey: tab.id)
        surfaces.removeValue(forKey: contentID)
        liveRecords.removeValue(forKey: contentID)
        restoredRecords.removeValue(forKey: contentID)
        faviconURLs.removeValue(forKey: contentID)
        initialURLs.removeValue(forKey: contentID)
    }

    func dispose(prepared: PreparedContent) async {
        guard let token = prepared.opaqueToken as? AdapterRuntimeToken else { return }
        guard let uuid = UUID(uuidString: token.contentID) else { return }
        stop(contentID: BrowserContentID(uuid))
        tabs.removeValue(forKey: token.tabID)
        worktreeIDs.removeValue(forKey: token.tabID)
    }

    func record(for contentID: BrowserContentID, worktreeID: UUID)
        -> BrowserContentRecordValue? {
        guard let record = liveRecords[contentID] ?? restoredRecords[contentID],
              record.worktreeID == worktreeID else { return nil }
        return record
    }

    func setRestoredRecord(_ record: BrowserContentRecordValue, for contentID: BrowserContentID) {
        restoredRecords[contentID] = record
    }

    func setLiveRecord(_ record: BrowserContentRecordValue, for contentID: BrowserContentID) {
        liveRecords[contentID] = record
    }

    func setLivePage(_ page: BrowserPage, for tabID: WorkspaceTabID) {
        guard case .browser(let contentID) = tabs[tabID]?.content else { return }
        recordPage(page, for: tabID, contentID: contentID)
    }

    func surface(for contentID: BrowserContentID) -> BrowserSurface {
        if let surface = surfaces[contentID] { return surface }
        let surface = BrowserSurface()
        surfaces[contentID] = surface
        return surface
    }

    func open(contentID: BrowserContentID, url: String) async throws -> BrowserPage {
        try await surface(for: contentID).open(url)
    }

    func navigate(contentID: BrowserContentID,
                  action: BrowserCommand.Navigation) async throws -> BrowserPage {
        try await surface(for: contentID).navigate(action)
    }

    func get(contentID: BrowserContentID, value: BrowserCommand.Get, selector: String?) async throws -> String {
        try await surface(for: contentID).get(value, selector: selector)
    }

    func screenshot(contentID: BrowserContentID, path: String?) async throws -> String {
        let image = try await snapshot(of: surface(for: contentID).webView)
        let outputPath = path ?? NSTemporaryDirectory()
            + "tiller-browser-\(UUID().uuidString).png"
        let outputURL = URL(fileURLWithPath: outputPath)
        guard let tiff = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiff),
              let png = bitmap.representation(using: .png, properties: [:]) else {
            throw BrowserError.navigationFailed(hint: "WebKit did not produce a PNG snapshot")
        }
        try png.write(to: outputURL, options: .atomic)
        return outputURL.path
    }

    func stop(contentID: BrowserContentID) {
        surfaces[contentID]?.stop()
    }

    func focusAddressBar(tabID: WorkspaceTabID) {
        NotificationCenter.default.post(
            name: .tillerBrowserFocusAddressBar, object: tabID.rawValue)
    }

    func faviconURL(for contentID: BrowserContentID) -> URL? {
        faviconURLs[contentID]
    }

    private func recordPage(_ page: BrowserPage, for tabID: WorkspaceTabID,
                            contentID: BrowserContentID) {
        guard let worktreeID = worktreeIDs[tabID] else { return }
        let record = BrowserContentRecordValue(
            id: contentID, worktreeID: worktreeID,
            url: page.url.absoluteString,
            title: page.title.isEmpty ? "Browser" : page.title)
        liveRecords[contentID] = record
        if let faviconURL = page.faviconURL {
            faviconURLs[contentID] = faviconURL
        } else {
            faviconURLs.removeValue(forKey: contentID)
        }
        onPageChange?(tabID, page)
    }

    private func snapshot(of webView: WKWebView) async throws -> NSImage {
        try await withCheckedThrowingContinuation { continuation in
            webView.takeSnapshot(with: nil) { image, error in
                if let image { continuation.resume(returning: image) }
                else {
                    continuation.resume(throwing: error ?? BrowserError.navigationFailed(
                        hint: "WebKit snapshot failed"))
                }
            }
        }
    }
}

extension Notification.Name {
    static let tillerBrowserFocusAddressBar = Notification.Name(
        "dev.tiller.browser.focusAddressBar")
}
