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

/// How much of the file the diff tab shows. The changes list in the right panel
/// is always `.hunks`; only the tab offers the choice.
enum DiffScope: String, CaseIterable {
    case wholeFile
    case hunks

    var title: String {
        switch self {
        case .wholeFile: "Whole file"
        case .hunks: "Changes only"
        }
    }

    var contextLines: Int {
        switch self {
        case .wholeFile: GitDiff.wholeFileContextLines
        case .hunks: 3
        }
    }
}

struct SideBySideDiffView: View {
    let documentID: DocumentID
    let worktreePath: String
    let loader: @Sendable (GitStatusEntry, String, Int) async throws -> GitFileDiff

    @State private var state: DiffTabLoadState = .loading
    @State private var scope: DiffScope = .wholeFile
    /// Set when the whole file blew the output limit and the tab fell back to
    /// hunks on its own, so the reason is on screen instead of a silent switch.
    @State private var didFallBackToHunks = false

    init(
        documentID: DocumentID,
        worktreePath: String,
        loader: @escaping @Sendable (GitStatusEntry, String, Int) async throws -> GitFileDiff = {
            entry, path, contextLines in
            try await GitDiff.load(entry: entry, in: path, contextLines: contextLines)
        }
    ) {
        self.documentID = documentID
        self.worktreePath = worktreePath
        self.loader = loader
    }

    private var fileURL: URL { URL(fileURLWithPath: documentID.canonicalPath) }

    var body: some View {
        VStack(spacing: 0) {
            scopeBar
            Divider()
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
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: documentID.canonicalPath) {
            await load()
        }
        .task(id: scope) {
            await load()
        }
        .onReceive(NotificationCenter.default.publisher(
            for: .tillerChangesDidRefresh)) { notification in
                guard let worktreeID = notification.object as? UUID,
                      worktreeID == documentID.worktreeID else { return }
                Task { await load() }
            }
    }

    private var scopeBar: some View {
        HStack(spacing: 8) {
            Picker("", selection: $scope) {
                ForEach(DiffScope.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 220)
            if didFallBackToHunks {
                Text("File too large to show in full")
                    .font(AppFont.system(size: 11))
                    .foregroundStyle(AppTheme.subtitle)
            }
            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
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
            didFallBackToHunks = false
            let diff: GitFileDiff
            do {
                diff = try await loader(entry, worktreePath, scope.contextLines)
            } catch where DiffTabAvailability.isOutputLimit(error) && scope == .wholeFile {
                // A whole-file diff carries the entire file, so a large file
                // trips the output limit where its hunks never would. Showing
                // the changes beats showing "too large" — the fallback is
                // labelled in the bar rather than silently swapping the mode.
                diff = try await loader(entry, worktreePath, DiffScope.hunks.contextLines)
                didFallBackToHunks = true
            }
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
    static let minimumColumnWidth: CGFloat = 120
    static let dividerWidth: CGFloat = 1
    static let contentPadding: CGFloat = 8

    /// Halves the viewport rather than measuring the longest line. Sizing to the
    /// content is what put the right-hand column past the right edge of the
    /// pane: one long line in the file was enough to leave a side-by-side view
    /// with only one visible side. Long lines are clipped instead.
    static func columnWidth(forViewport viewport: CGFloat) -> CGFloat {
        let usable = viewport - 2 * contentPadding - dividerWidth
        return max(minimumColumnWidth, (usable / 2).rounded(.down))
    }

    /// A new or untracked file is all additions, so every left cell would be
    /// filler. Half a pane of nothing compares nothing: those render full width.
    static func isSingleColumn(_ diff: GitFileDiff) -> Bool {
        !diff.lines.contains { $0.kind == .deletion }
    }
}

private struct SideBySideDiffBody: View {
    let diff: GitFileDiff
    let fileURL: URL

    @Environment(\.colorScheme) private var colorScheme
    @State private var highlights: DiffHighlights?

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
        let isSingleColumn = SideBySideDiffLayout.isSingleColumn(diff)
        // One GeometryReader for the whole body, not one per row: the columns
        // are sized from the pane, and the pane's width is the same for every
        // row in it.
        GeometryReader { proxy in
            let columnWidth = isSingleColumn
                ? proxy.size.width - 2 * SideBySideDiffLayout.contentPadding
                : SideBySideDiffLayout.columnWidth(forViewport: proxy.size.width)
            let contentWidth = isSingleColumn
                ? columnWidth
                : columnWidth * 2 + SideBySideDiffLayout.dividerWidth
            ScrollView(.vertical) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(rows) { row in
                        if row.isHunk, let hunk = row.left {
                            Text(hunk.text)
                                .font(AppFont.mono(size: 11))
                                .foregroundStyle(AppTheme.subtitle)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                // An explicit width, not `.infinity`: the header
                                // has to line up with the columns below it.
                                .frame(width: contentWidth, alignment: .leading)
                                .background(AppTheme.diffHunkBackground)
                        } else if isSingleColumn {
                            side(row.right ?? row.left, useOldHighlights: false, width: columnWidth)
                        } else {
                            HStack(alignment: .top, spacing: 0) {
                                side(row.left, useOldHighlights: true, width: columnWidth)
                                Divider().frame(width: SideBySideDiffLayout.dividerWidth)
                                side(row.right, useOldHighlights: false, width: columnWidth)
                            }
                        }
                    }
                }
                .padding(SideBySideDiffLayout.contentPadding)
            }
        }
        .task(id: requestID) {
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
                    .truncationMode(.tail)
                    .textSelection(.enabled)
                    .padding(.leading, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .font(AppFont.mono(size: 11))
            .padding(.vertical, 2)
            .frame(width: width, alignment: .leading)
            .background(background(for: line))
            // Without this a line wider than its column paints over the other
            // one; the column is a boundary, not a suggestion.
            .clipped()
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
                font: AppFont.nsMono(size: 11)))
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
