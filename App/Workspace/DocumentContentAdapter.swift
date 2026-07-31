import AppKit
import SwiftUI
import Foundation
import TillerCode
import TillerCore
import TillerWorkspace

@MainActor
final class DocumentContentAdapter: WorkspaceContentAdapter {
    let kind: WorkspaceContentKind = .document
    private let boundary: AdapterBoundary
    private var tabs: [WorkspaceTabID: WorkspaceTab] = [:]
    private var runtimes: [WorkspaceTabID: AdapterRuntimeToken] = [:]
    private var details: [WorkspaceTabID: DocumentDetail] = [:]
    private var buffers: [WorkspaceTabID: String] = [:]
    private var hydrated: Set<WorkspaceTabID> = []
    private var markdownDocuments: [WorkspaceTabID: MarkdownDocument] = [:]
    private var codeDocuments: [WorkspaceTabID: CodeDocument] = [:]

    init(boundary: AdapterBoundary = AdapterBoundary()) { self.boundary = boundary }

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        guard let tab = prepareTab(request: request, worktreeID: worktree.id) else {
            throw ContentAdapterError.unsupportedRequest
        }
        let token = runtimes[tab.id]!
        return PreparedContent(tab: tab, generationID: token.generationID, opaqueToken: token)
    }

    func prepareTab(request: ContentRequest) -> WorkspaceTab? {
        prepareTab(request: request, worktreeID: UUID())
    }

    private func prepareTab(request: ContentRequest, worktreeID: UUID) -> WorkspaceTab? {
        guard case .openFile(let url, let editor) = request else { return nil }
        let documentID = DocumentID.makeCanonical(worktreeID: worktreeID, path: url.path)
        let tab = WorkspaceTab(id: WorkspaceTabID(), title: url.lastPathComponent,
                              titleIsAutoNamed: true, content: .document(documentID, editor: editor))
        let token = AdapterRuntimeToken(
            tabID: tab.id, contentID: documentID.canonicalPath,
            generationID: ResourceGenerationID())
        token.phase = .preparing
        tabs[tab.id] = tab
        runtimes[tab.id] = token
        details[tab.id] = .available
        return tab
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {
        guard let runtime = runtimes[tab.id], !runtime.released, !hydrated.contains(tab.id) else { return }
        hydrated.insert(tab.id)
        await boundary.hydrate(tab, worktree)
        runtime.phase = .active
    }

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        guard case .document(let documentID, let editor) = tab.content else {
            return WorkspaceContentHostAdapter(tabID: tab.id, viewController: NSViewController())
        }
        let url = URL(fileURLWithPath: documentID.canonicalPath)
        let rootView: AnyView
        do {
            switch editor {
            case .markdown:
                let document = try markdownDocuments[tab.id] ?? MarkdownDocument(fileURL: url)
                markdownDocuments[tab.id] = document
                rootView = AnyView(MarkdownEditorTabView(document: document))
            case .code:
                let document = try codeDocuments[tab.id] ?? CodeDocument(fileURL: url)
                codeDocuments[tab.id] = document
                rootView = AnyView(CodeEditorTabView(document: document))
            }
        } catch {
            details[tab.id] = .missing
            return WorkspaceContentHostAdapter(tabID: tab.id, viewController: NSViewController())
        }
        return WorkspaceContentHostAdapter(
            tabID: tab.id, viewController: NSHostingController(rootView: rootView))
    }

    /// Saves the tab's document, if it has unsaved changes. No-op for tabs
    /// that never hydrated (e.g. closed before their host was ever built).
    func save(tabID: WorkspaceTabID) throws {
        if let document = markdownDocuments[tabID] {
            try document.save()
        } else if let document = codeDocuments[tabID] {
            try document.save()
        }
    }

    func isDirty(tabID: WorkspaceTabID) -> Bool {
        markdownDocuments[tabID]?.isDirty ?? codeDocuments[tabID]?.isDirty ?? false
    }

    func markdownDocument(for tabID: WorkspaceTabID) -> MarkdownDocument? { markdownDocuments[tabID] }
    func codeDocument(for tabID: WorkspaceTabID) -> CodeDocument? { codeDocuments[tabID] }

    func checkpoint(tab: WorkspaceTab) async { await boundary.checkpoint(tab) }

    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID {
        let token = runtimes[tab.id] ?? AdapterRuntimeToken(
            tabID: tab.id, contentID: tab.content.contentIdentifierString,
            generationID: ResourceGenerationID())
        token.generationID = ResourceGenerationID()
        token.phase = .preparing
        token.released = false
        hydrated.remove(tab.id)
        runtimes[tab.id] = token
        tabs[tab.id] = tab
        return token.generationID
    }

    func close(tab: WorkspaceTab) async {
        guard let runtime = runtimes[tab.id], !runtime.released else { return }
        runtime.phase = .closing
        runtime.released = true
        await boundary.close(tab)
        runtimes.removeValue(forKey: tab.id)
        tabs.removeValue(forKey: tab.id)
        details.removeValue(forKey: tab.id)
        buffers.removeValue(forKey: tab.id)
        markdownDocuments.removeValue(forKey: tab.id)
        codeDocuments.removeValue(forKey: tab.id)
        hydrated.remove(tab.id)
    }

    func dispose(prepared: PreparedContent) async {
        guard let token = prepared.opaqueToken as? AdapterRuntimeToken, !token.disposed else { return }
        token.disposed = true
        token.released = true
        await boundary.dispose(prepared.opaqueToken)
        runtimes.removeValue(forKey: token.tabID)
        tabs.removeValue(forKey: token.tabID)
        details.removeValue(forKey: token.tabID)
        markdownDocuments.removeValue(forKey: token.tabID)
        codeDocuments.removeValue(forKey: token.tabID)
    }

    func setBuffer(_ buffer: String, for tabID: WorkspaceTabID) { buffers[tabID] = buffer }
    func buffer(for tabID: WorkspaceTabID) -> String? { buffers[tabID] }
    func markMissing(tabID: WorkspaceTabID) { details[tabID] = .missing }
    func markDirty(tabID: WorkspaceTabID) { details[tabID] = .dirty }
    func markConflicted(tabID: WorkspaceTabID) { details[tabID] = .conflicted }
    func documentDetail(for tabID: WorkspaceTabID) -> DocumentDetail? { details[tabID] }
    func requiresExplicitChoice(tabID: WorkspaceTabID) -> Bool {
        switch details[tabID] {
        case .dirty, .conflicted, .missing: true
        default: false
        }
    }
    func phase(for tabID: WorkspaceTabID) -> ContentPhase? { runtimes[tabID]?.phase }
    func generation(for tabID: WorkspaceTabID) -> ResourceGenerationID? { runtimes[tabID]?.generationID }
}

enum ContentAdapterError: Error, Equatable, Sendable {
    case unsupportedRequest
}
