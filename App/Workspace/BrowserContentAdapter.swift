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
    private var contentWorktreeIDs: [BrowserContentID: UUID] = [:]
    private var drivingStates: [BrowserContentID: BrowserDrivingState] = [:]

    /// The coordinator owns persistence and tab titles. The adapter reports
    /// every committed page change through this callback, including changes
    /// initiated by the chrome rather than the control socket.
    var onPageChange: ((WorkspaceTabID, BrowserPage) -> Void)?
    var onExternalURL: ((WorkspaceTabID, UUID, URL, BrowserNavigationOrigin) -> Void)?
    var originAuthorization: BrowserSurface.OriginAuthorization = { _, _ in false }

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
        contentWorktreeIDs[contentID] = worktree.id
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
        if case .browser(let contentID) = tab.content { contentWorktreeIDs[contentID] = worktree.id }
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
                drivingState: drivingState(for: contentID),
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
        contentWorktreeIDs.removeValue(forKey: contentID)
        liveRecords.removeValue(forKey: contentID)
        restoredRecords.removeValue(forKey: contentID)
        faviconURLs.removeValue(forKey: contentID)
        initialURLs.removeValue(forKey: contentID)
        drivingStates.removeValue(forKey: contentID)
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
        contentWorktreeIDs[contentID] = record.worktreeID
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
        let worktreeID = contentWorktreeIDs[contentID]
        if worktreeID == nil {
            NSLog("Tiller opened a browser surface with no worktree: not persisting its session")
        }
        // Falling back to `.default()` would put an unattributed surface in the
        // store every other worktree shares, so an unknown worktree loses
        // persistence rather than isolation.
        let surface = BrowserSurface(
            dataStore: worktreeID.map(BrowserWebsiteDataStore.persistent) ?? .nonPersistent(),
            worktreeID: worktreeID,
            originAuthorization: originAuthorization)
        if let tab = tabs.first(where: { $0.value.content == .browser(contentID) }) {
            surface.onExternalURL = { [weak self] url, origin in
                guard let worktreeID = self?.worktreeIDs[tab.key] else { return }
                self?.onExternalURL?(tab.key, worktreeID, url, origin)
            }
        }
        surfaces[contentID] = surface
        return surface
    }

    func open(contentID: BrowserContentID, url: String) async throws -> BrowserPage {
        try await surface(for: contentID).open(url)
    }

    func snapshot(contentID: BrowserContentID) async throws -> BrowserSnapshot {
        try await surface(for: contentID).snapshot()
    }

    func act(contentID: BrowserContentID, action: BrowserAct, generation: Int?)
        async throws -> BrowserActResult {
        try await surface(for: contentID).act(action, generation: generation)
    }

    func wait(contentID: BrowserContentID, condition: BrowserWaitCondition, timeoutMs: Int)
        async throws -> Int {
        try await surface(for: contentID).wait(condition, timeoutMs: timeoutMs)
    }

    func eval(contentID: BrowserContentID, script: String) async throws -> String {
        try await surface(for: contentID).eval(script)
    }

    func console(contentID: BrowserContentID, since: Double?)
        async throws -> [BrowserConsoleEntry] {
        try await surface(for: contentID).console(since: since)
    }

    func withAgentCommand<T>(contentID: BrowserContentID, operation: () async throws -> T)
        async throws -> T {
        let state = drivingState(for: contentID)
        state.isAgentDriving = true
        defer { state.isAgentDriving = false }
        return try await operation()
    }

    func navigate(contentID: BrowserContentID,
                  action: BrowserCommand.Navigation) async throws -> BrowserPage {
        try await surface(for: contentID).navigate(action)
    }

    func get(contentID: BrowserContentID, value: BrowserCommand.Get, selector: String?) async throws -> String {
        try await surface(for: contentID).get(value, selector: selector)
    }

    func screenshot(contentID: BrowserContentID, path: String?) async throws -> String {
        let image = try await surface(for: contentID).screenshot()
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

    private func drivingState(for contentID: BrowserContentID) -> BrowserDrivingState {
        if let state = drivingStates[contentID] { return state }
        let state = BrowserDrivingState()
        drivingStates[contentID] = state
        return state
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

}

extension Notification.Name {
    static let tillerBrowserFocusAddressBar = Notification.Name(
        "dev.tiller.browser.focusAddressBar")
}
