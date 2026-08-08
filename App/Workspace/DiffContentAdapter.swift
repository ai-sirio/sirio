import AppKit
import Foundation
import SwiftUI
import TillerCode
import TillerCore
import TillerGit
import TillerWorkspace

@MainActor
final class DiffContentAdapter: WorkspaceContentAdapter {
    let kind: WorkspaceContentKind = .diff
    private let boundary: AdapterBoundary
    private var tabs: [WorkspaceTabID: WorkspaceTab] = [:]
    private var runtimes: [WorkspaceTabID: AdapterRuntimeToken] = [:]

    init(boundary: AdapterBoundary = AdapterBoundary()) {
        self.boundary = boundary
    }

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        guard case .openDiff(let url) = request else {
            throw ContentAdapterError.unsupportedRequest
        }
        let documentID = DocumentID.make(worktreeID: worktree.id, fileURL: url)
        let tab = WorkspaceTab(
            id: WorkspaceTabID(),
            title: url.lastPathComponent,
            titleIsAutoNamed: true,
            content: .diff(documentID))
        let token = AdapterRuntimeToken(
            tabID: tab.id,
            contentID: documentID.canonicalPath,
            generationID: ResourceGenerationID())
        token.phase = .preparing
        tabs[tab.id] = tab
        runtimes[tab.id] = token
        return PreparedContent(tab: tab, generationID: token.generationID, opaqueToken: token)
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {
        guard let runtime = runtimes[tab.id], !runtime.released else { return }
        await boundary.hydrate(tab, worktree)
        guard !runtime.released else { return }
        runtime.hydrated = true
        runtime.phase = .active
    }

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        guard case .diff(let documentID) = tab.content else {
            return WorkspaceContentHostAdapter(tabID: tab.id, viewController: NSViewController())
        }
        let view = SideBySideDiffView(documentID: documentID, worktreePath: worktree.path)
        let controller = NSHostingController(rootView: view)
        return WorkspaceContentHostAdapter(tabID: tab.id, viewController: controller)
    }

    func checkpoint(tab: WorkspaceTab) async {}

    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID {
        let token = runtimes[tab.id] ?? AdapterRuntimeToken(
            tabID: tab.id,
            contentID: tab.content.contentIdentifierString,
            generationID: ResourceGenerationID())
        token.generationID = ResourceGenerationID()
        token.phase = .preparing
        token.released = false
        token.hydrated = false
        tabs[tab.id] = tab
        runtimes[tab.id] = token
        return token.generationID
    }

    func close(tab: WorkspaceTab) async {
        guard let runtime = runtimes[tab.id], !runtime.released else { return }
        runtime.phase = .closing
        runtime.released = true
        await boundary.close(tab)
        runtimes.removeValue(forKey: tab.id)
        tabs.removeValue(forKey: tab.id)
    }

    func dispose(prepared: PreparedContent) async {
        guard let token = prepared.opaqueToken as? AdapterRuntimeToken, !token.disposed else { return }
        token.disposed = true
        token.released = true
        await boundary.dispose(prepared.opaqueToken)
        runtimes.removeValue(forKey: token.tabID)
        tabs.removeValue(forKey: token.tabID)
    }
}

enum DiffTabLoadState: Equatable {
    case loading
    case loaded(GitFileDiff)
    case unavailable(DiffUnavailableReason)
}

struct SideBySideDiffView: View {
    let documentID: DocumentID
    let worktreePath: String
    let loader: @Sendable (GitStatusEntry, String) async throws -> GitFileDiff

    @State private var state: DiffTabLoadState = .loading

    init(
        documentID: DocumentID,
        worktreePath: String,
        loader: @escaping @Sendable (GitStatusEntry, String) async throws -> GitFileDiff = {
            entry, path in try await GitDiff.load(entry: entry, in: path)
        }
    ) {
        self.documentID = documentID
        self.worktreePath = worktreePath
        self.loader = loader
    }

    private var fileURL: URL { URL(fileURLWithPath: documentID.canonicalPath) }

    var body: some View {
        Group {
            switch state {
            case .loading:
                ProgressView("Loading diff…")
            case .loaded(let diff):
                SideBySideDiffBody(diff: diff, fileURL: fileURL)
            case .unavailable(let reason):
                ContentUnavailableView(
                    "Diff unavailable",
                    systemImage: "exclamationmark.triangle",
                    description: Text(reason.message))
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: documentID.canonicalPath) {
            await load()
        }
        .onReceive(NotificationCenter.default.publisher(
            for: .tillerChangesDidRefresh)) { notification in
                guard let worktreeID = notification.object as? UUID,
                      worktreeID == documentID.worktreeID else { return }
                Task { await load() }
            }
    }

    private func load() async {
        state = .loading
        let root = URL(fileURLWithPath: worktreePath, isDirectory: true)
            .standardizedFileURL.path
        let filePath = fileURL.standardizedFileURL.path
        guard filePath.hasPrefix(root + "/") else {
            state = .unavailable(.error("The file is outside this worktree."))
            return
        }
        let relativePath = String(filePath.dropFirst(root.count + 1))
        guard let gitPath = try? GitPath(relativePath) else {
            state = .unavailable(.error("The file path is invalid."))
            return
        }
        do {
            let status = try await GitStatus.load(in: worktreePath)
            guard let entry = status.entries.first(where: { $0.path == gitPath }) else {
                state = .unavailable(.clean)
                return
            }
            let diff = try await loader(entry, worktreePath)
            if let reason = DiffTabAvailability.reason(for: diff) {
                state = .unavailable(reason)
            } else {
                state = .loaded(diff)
            }
        } catch {
            state = .unavailable(DiffTabAvailability.reason(for: error))
        }
    }
}

enum SideBySideDiffLayout {
    static let minimumColumnWidth: CGFloat = 360
    private static let lineNumberWidth: CGFloat = 40
    private static let codeLeadingPadding: CGFloat = 8

    static func columnWidth(for diff: GitFileDiff) -> CGFloat {
        let font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        let longestLineWidth = diff.lines.reduce(CGFloat.zero) { width, line in
            guard line.kind == .addition || line.kind == .deletion || line.kind == .context else {
                return width
            }
            let measured = (line.text as NSString).size(withAttributes: [.font: font]).width
            return max(width, measured)
        }
        return max(
            minimumColumnWidth,
            ceil(lineNumberWidth + codeLeadingPadding + longestLineWidth))
    }
}

private struct SideBySideDiffBody: View {
    let diff: GitFileDiff
    let fileURL: URL

    @Environment(\.colorScheme) private var colorScheme
    @State private var highlights: DiffHighlights?
    @State private var columnWidth = SideBySideDiffLayout.minimumColumnWidth

    private struct HighlightRequest: Hashable {
        let path: String
        let oldHash: Int?
        let newHash: Int?
    }

    private var requestID: HighlightRequest {
        HighlightRequest(
            path: fileURL.standardizedFileURL.path,
            oldHash: diff.oldText?.hashValue,
            newHash: diff.newText?.hashValue)
    }

    var body: some View {
        let rows = GitDiffSideBySide.rows(from: diff)
        ScrollView([.vertical, .horizontal]) {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(rows) { row in
                    if row.isHunk, let hunk = row.left {
                        Text(hunk.text)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(AppTheme.subtitle)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 3)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(AppTheme.diffHunkBackground)
                    } else {
                        HStack(alignment: .top, spacing: 1) {
                            side(row.left, useOldHighlights: true, width: columnWidth)
                            Divider()
                            side(row.right, useOldHighlights: false, width: columnWidth)
                        }
                    }
                }
            }
            .padding(8)
        }
        .task(id: requestID) {
            // Measured here rather than in `body`: the width comes from sizing
            // every line's text, and `body` runs again as soon as the
            // highlights below land — measuring 20k lines twice for a width
            // that cannot have changed.
            columnWidth = SideBySideDiffLayout.columnWidth(for: diff)
            highlights = await DiffHighlightCache.shared.highlights(
                path: fileURL, oldText: diff.oldText, newText: diff.newText)
        }
    }

    @ViewBuilder
    private func side(
        _ line: GitDiffLine?, useOldHighlights: Bool, width: CGFloat
    ) -> some View {
        if let line {
            HStack(alignment: .top, spacing: 0) {
                Text(lineNumber(for: line, useOldHighlights: useOldHighlights))
                    .frame(width: 40, alignment: .trailing)
                    .foregroundStyle(AppTheme.meta)
                codeText(for: line, useOldHighlights: useOldHighlights)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                    .textSelection(.enabled)
                    .padding(.leading, 8)
            }
            .font(.system(size: 11, design: .monospaced))
            .padding(.vertical, 2)
            .frame(width: width, alignment: .leading)
            .background(background(for: line))
        } else {
            Color.clear
                .frame(width: width, height: 18)
        }
    }

    private func lineNumber(for line: GitDiffLine, useOldHighlights: Bool) -> String {
        (useOldHighlights ? line.oldLineNumber : line.newLineNumber).map(String.init) ?? ""
    }

    @ViewBuilder
    private func codeText(for line: GitDiffLine, useOldHighlights: Bool) -> some View {
        let map = useOldHighlights ? highlights?.old : highlights?.new
        if let number = useOldHighlights ? line.oldLineNumber : line.newLineNumber,
           let map,
           !map.ranges(forLine: number).isEmpty {
            Text(AttributedCodeRenderer.renderLine(
                line.text,
                ranges: map.ranges(forLine: number),
                theme: .tiller(isDark: colorScheme == .dark),
                font: .monospacedSystemFont(ofSize: 11, weight: .regular)))
        } else {
            Text(line.text)
                .foregroundStyle(foreground(for: line))
        }
    }

    private func foreground(for line: GitDiffLine) -> Color {
        switch line.kind {
        case .addition: AppTheme.diffAddition
        case .deletion: AppTheme.diffDeletion
        default: AppTheme.subtitle
        }
    }

    private func background(for line: GitDiffLine) -> Color {
        switch line.kind {
        case .addition: AppTheme.diffAdditionBackground
        case .deletion: AppTheme.diffDeletionBackground
        default: .clear
        }
    }
}
