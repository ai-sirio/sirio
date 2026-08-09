import AppKit
import SwiftUI
import TillerCore
import TillerGit
import Inject

private struct FileExplorerRowSelectionModifier: ViewModifier {
    let action: () -> Void

    func body(content: Content) -> some View {
        content.simultaneousGesture(
            TapGesture(count: 1).onEnded { action() }
        )
    }
}

extension View {
    func fileExplorerRowSelection(action: @escaping () -> Void) -> some View {
        modifier(FileExplorerRowSelectionModifier(action: action))
    }
}

struct FileExplorerView: View {
    @ObserveInjection private var inject

    @Bindable var appModel: AppModel
    @Bindable var panelModel: RightPanelModel
    let worktree: Worktree
    @State private var selectedPath: String?
    @FocusState private var treeFocused: Bool
    @AppStorage(AppSettings.fileIconThemeKey) private var fileIconThemeRaw = FileIconTheme.sfSymbols.rawValue

    private var iconTheme: FileIconTheme {
        FileIconTheme(rawValue: fileIconThemeRaw) ?? .sfSymbols
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(worktree.path)
                    .font(AppFont.system(size: 11))
                    .foregroundStyle(AppTheme.meta)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                Button {
                    Task { await panelModel.refresh() }
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .help("Refresh Files")
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)

            if panelModel.filesLoading && panelModel.childrenByDirectory.isEmpty {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = panelModel.filesError {
                ContentUnavailableView {
                    Label("Files unavailable", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                } actions: {
                    Button("Retry") { Task { await panelModel.refresh() } }
                }
            } else {
                ScrollView {
                    LazyVStack(spacing: 1) {
                        ForEach(panelModel.visibleRows) { row in
                            fileRow(row)
                        }
                    }
                    .padding(.vertical, 4)
                }
                .focusable()
                .focusEffectDisabled()
                .focused($treeFocused)
                .onKeyPress(.downArrow) { moveSelection(1); return .handled }
                .onKeyPress(.upArrow) { moveSelection(-1); return .handled }
                .onKeyPress(.space) { toggleSelectedDirectory(); return .handled }
                .onKeyPress(.return) { openSelected(); return .handled }
            }
        }
    .enableInjection()
    }

    private func fileRow(_ row: FileExplorerRow) -> some View {
        let node = row.node
        let selected = selectedPath == node.relativePath
        return HStack(spacing: 6) {
            if node.kind.isDirectory {
                Image(systemName: panelModel.expandedDirectories.contains(node.relativePath)
                      ? "chevron.down" : "chevron.right")
                    .font(AppFont.system(size: 9, weight: .semibold))
                    .frame(width: 10)
            } else {
                Color.clear.frame(width: 10, height: 1)
            }
            if node.kind.isDirectory {
                Color.clear.frame(width: 14, height: 1)
                Text(node.name)
                    .font(AppFont.system(size: 12))
                    .foregroundStyle(nameColor(for: node))
                    .lineLimit(1)
                    .truncationMode(.middle)
            } else {
                iconView(for: node)
                    .frame(width: 14)
                Text(node.name)
                    .font(AppFont.system(size: 12))
                    .foregroundStyle(nameColor(for: node))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    // Scope double-click to the file name so row taps stay immediate.
                    .onTapGesture(count: 2) {
                        selectedPath = node.relativePath
                        open(node)
                    }
            }
            Spacer(minLength: 4)
            if let entry = panelModel.statusByPath[node.relativePath] {
                Text(GitStatusStyle.symbol(entry))
                    .font(AppFont.mono(size: 10, weight: .semibold))
                    .foregroundStyle(GitStatusStyle.color(entry))
                    .help(statusLabel(entry))
            } else if node.kind.isDirectory,
                      let dirStatus = panelModel.directoryStatusByPath[node.relativePath] {
                Circle()
                    .fill(directoryColor(dirStatus))
                    .frame(width: 6, height: 6)
                    .help(directoryLabel(dirStatus))
            }
        }
        .padding(.leading, CGFloat(row.depth) * 14 + 8)
        .padding(.trailing, 8)
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        // `URL` is already Transferable and vends `.fileURL`, so a row drag
        // is indistinguishable from a Finder drag at every drop target.
        .draggable(dragURL(for: node) ?? URL(fileURLWithPath: "/"))
        .background(selected ? AppTheme.selectionFill : Color.clear,
                    in: RoundedRectangle(cornerRadius: 6))
        .fileExplorerRowSelection {
            selectedPath = node.relativePath
            treeFocused = true
            if node.kind.isDirectory {
                Task { await panelModel.toggleDirectory(node.relativePath) }
            }
        }
        .contextMenu {
            Button("Open") { open(node) }
                .disabled(node.kind.isDirectory)
            Button("Show in Finder") { reveal(node) }
            Button("Copy path") { copyPath(node) }
        }
        .overlay(alignment: .bottomLeading) {
            if let error = panelModel.directoryErrors[node.relativePath] {
                Text(error)
                    .font(AppFont.caption2)
                    .foregroundStyle(AppTheme.gitConflict)
                    .padding(.leading, CGFloat(row.depth) * 14 + 38)
            }
        }
    }

    private func dragURL(for node: FileTreeNode) -> URL? {
        panelModel.rootURL.map { node.url(relativeTo: $0) }
    }

    private func moveSelection(_ delta: Int) {
        let rows = panelModel.visibleRows
        guard !rows.isEmpty else { return }
        let current = rows.firstIndex {
            $0.node.relativePath == selectedPath
        } ?? (delta > 0 ? -1 : 0)
        selectedPath = rows[min(max(current + delta, 0), rows.count - 1)].node.relativePath
    }

    private func toggleSelectedDirectory() {
        guard let row = panelModel.visibleRows.first(where: {
            $0.node.relativePath == selectedPath
        }), row.node.kind.isDirectory else { return }
        Task { await panelModel.toggleDirectory(row.node.relativePath) }
    }

    private func openSelected() {
        guard let row = panelModel.visibleRows.first(where: {
            $0.node.relativePath == selectedPath
        }) else { return }
        if row.node.kind.isDirectory { toggleSelectedDirectory() } else { open(row.node) }
    }

    private func open(_ node: FileTreeNode) {
        guard let root = panelModel.rootURL else { return }
        let url = node.url(relativeTo: root)
        appModel.openDocument(fileURL: url, in: worktree)
    }

    private func reveal(_ node: FileTreeNode) {
        guard let root = panelModel.rootURL else { return }
        NSWorkspace.shared.activateFileViewerSelecting([node.url(relativeTo: root)])
    }

    private func copyPath(_ node: FileTreeNode) {
        guard let root = panelModel.rootURL else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(node.url(relativeTo: root).path, forType: .string)
    }

    @ViewBuilder
    private func iconView(for node: FileTreeNode) -> some View {
        FileTypeIcon(
            key: iconKey(for: node),
            theme: iconTheme,
            tint: node.kind.isDirectory ? AppTheme.meta : AppTheme.subtitle)
    }

    private func iconKey(for node: FileTreeNode) -> FileIconKey {
        switch node.kind {
        case .directory: FileIconKey.key(forDirectoryName: node.name)
        case .symbolicLink: .symlink
        case .file: FileIconKey.key(forFileName: node.name)
        }
    }

    private func statusLabel(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "Conflicted" }
        if entry.isUntracked { return "Untracked" }
        if entry.isStaged && entry.hasWorktreeChanges { return "Staged and modified" }
        if entry.isStaged { return "Staged" }
        return "Modified"
    }

    private func nameColor(for node: FileTreeNode) -> Color {
        if let entry = panelModel.statusByPath[node.relativePath] {
            return GitStatusStyle.color(entry)
        }
        if node.kind.isDirectory,
           let dirStatus = panelModel.directoryStatusByPath[node.relativePath] {
            return directoryColor(dirStatus)
        }
        return .primary
    }

    private func directoryColor(_ status: DirectoryGitStatus) -> Color {
        switch status {
        case .conflicted: AppTheme.gitConflict
        case .changed: AppTheme.gitModified
        case .untracked: AppTheme.gitUntracked
        }
    }

    private func directoryLabel(_ status: DirectoryGitStatus) -> String {
        switch status {
        case .conflicted: "Contains conflicts"
        case .changed: "Contains changes"
        case .untracked: "Contains untracked files"
        }
    }
}
