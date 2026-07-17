import AppKit
import SwiftUI
import TillerCore
import TillerGit

struct FileExplorerView: View {
    @Bindable var appModel: AppModel
    @Bindable var panelModel: RightPanelModel
    let worktree: Worktree
    @State private var selectedPath: String?
    @FocusState private var treeFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(worktree.path)
                    .font(.system(size: 11))
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
                    Button("Riprova") { Task { await panelModel.refresh() } }
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
    }

    private func fileRow(_ row: FileExplorerRow) -> some View {
        let node = row.node
        let selected = selectedPath == node.relativePath
        return HStack(spacing: 6) {
            if node.kind.isDirectory {
                Image(systemName: panelModel.expandedDirectories.contains(node.relativePath)
                      ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .frame(width: 10)
            } else {
                Color.clear.frame(width: 10, height: 1)
            }
            Image(systemName: icon(for: node))
                .foregroundStyle(node.kind.isDirectory ? AppTheme.meta : AppTheme.subtitle)
                .frame(width: 14)
            Text(node.name)
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 4)
            if let entry = panelModel.statusByPath[node.relativePath] {
                Text(statusSymbol(entry))
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundStyle(statusColor(entry))
                    .help(statusLabel(entry))
            }
        }
        .padding(.leading, CGFloat(row.depth) * 14 + 8)
        .padding(.trailing, 8)
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        .background(selected ? AppTheme.selectionFill : Color.clear,
                    in: RoundedRectangle(cornerRadius: 6))
        // Double-tap must be attached before single-tap or it never fires.
        .onTapGesture(count: 2) {
            selectedPath = node.relativePath
            if !node.kind.isDirectory { open(node) }
        }
        .onTapGesture {
            selectedPath = node.relativePath
            treeFocused = true
            if node.kind.isDirectory {
                Task { await panelModel.toggleDirectory(node.relativePath) }
            }
        }
        .contextMenu {
            Button("Apri") { open(node) }
                .disabled(node.kind.isDirectory)
            Button("Mostra nel Finder") { reveal(node) }
            Button("Copia percorso") { copyPath(node) }
        }
        .overlay(alignment: .bottomLeading) {
            if let error = panelModel.directoryErrors[node.relativePath] {
                Text(error)
                    .font(.caption2)
                    .foregroundStyle(AppTheme.gitConflict)
                    .padding(.leading, CGFloat(row.depth) * 14 + 38)
            }
        }
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
        if MarkdownFileLink.isMarkdown(url) {
            appModel.openMarkdownTab(fileURL: url, in: worktree)
        } else {
            NSWorkspace.shared.open(url)
        }
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

    private func icon(for node: FileTreeNode) -> String {
        switch node.kind {
        case .directory: "folder"
        case .symbolicLink: "link"
        case .file: "doc"
        }
    }

    private func statusSymbol(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "U" }
        if entry.isUntracked { return "?" }
        if entry.indexState == .added { return "A" }
        if entry.indexState == .deleted || entry.worktreeState == .deleted { return "D" }
        if entry.indexState == .renamed || entry.worktreeState == .renamed { return "R" }
        return "M"
    }

    private func statusLabel(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "Conflicted" }
        if entry.isUntracked { return "Untracked" }
        if entry.isStaged && entry.hasWorktreeChanges { return "Staged and modified" }
        if entry.isStaged { return "Staged" }
        return "Modified"
    }

    private func statusColor(_ entry: GitStatusEntry) -> Color {
        if entry.isConflicted { return AppTheme.gitConflict }
        if entry.isUntracked { return AppTheme.gitUntracked }
        if entry.isStaged { return AppTheme.gitStaged }
        return AppTheme.gitModified
    }
}
