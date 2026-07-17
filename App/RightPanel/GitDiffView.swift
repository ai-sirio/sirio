import AppKit
import SwiftUI
import TillerGit

struct GitDiffView: View {
    @Bindable var panelModel: RightPanelModel
    let requestDiscard: (PendingGitDiscard) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Picker("Changed file", selection: Binding(
                    get: { panelModel.selectedDiffPath },
                    set: { path in
                        guard let path,
                              let entry = panelModel.status.entries.first(where: {
                                  $0.path == path
                              }) else { return }
                        Task { await panelModel.selectDiff(entry) }
                    })) {
                    ForEach(panelModel.status.entries, id: \.path) { entry in
                        Text(entry.path.value).tag(Optional(entry.path))
                    }
                }
                .labelsHidden()
                Button { selectAdjacent(-1) } label: {
                    Image(systemName: "chevron.up")
                }
                .buttonStyle(.plain)
                .help("Previous changed file")
                Button { selectAdjacent(1) } label: {
                    Image(systemName: "chevron.down")
                }
                .buttonStyle(.plain)
                .help("Next changed file")
                Button { Task { await panelModel.refresh() } } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .help("Refresh Diff")
            }
            .padding(8)
            Divider()

            if panelModel.status.isClean {
                ContentUnavailableView("Working tree clean", systemImage: "checkmark.circle")
            } else if panelModel.diffLoading && panelModel.diff == nil {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = panelModel.diffError {
                ContentUnavailableView {
                    Label("Diff unavailable", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                } actions: {
                    Button("Riprova") { Task { await panelModel.ensureDiffLoaded() } }
                    if let url = selectedFileURL {
                        Button("Apri file") { NSWorkspace.shared.open(url) }
                    }
                }
            } else if let diff = panelModel.diff {
                if diff.isBinary {
                    ContentUnavailableView(
                        "Binary diff unavailable", systemImage: "doc.richtext")
                } else {
                    VStack(spacing: 0) {
                        DiffPane(rows: panelModel.diffRows, side: .old)
                        Divider()
                        DiffPane(rows: panelModel.diffRows, side: .new)
                    }
                }
                actionBar
            }
        }
    }

    private var selectedFileURL: URL? {
        guard let root = panelModel.rootURL,
              let path = panelModel.selectedDiffPath else { return nil }
        return root.appendingPathComponent(path.value)
    }

    private var actionBar: some View {
        HStack {
            Spacer()
            if let entry = panelModel.selectedEntry {
                if entry.isConflicted {
                    Text("Resolve in terminal")
                        .font(.caption)
                        .foregroundStyle(AppTheme.gitConflict)
                } else {
                    if entry.isStaged {
                        Button("Unstage") {
                            Task { await panelModel.unstage([entry]) }
                        }
                    }
                    if entry.hasWorktreeChanges {
                        Button("Discard", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: .changes, entries: [entry]))
                        }
                        Button("Stage") {
                            Task { await panelModel.stage([entry]) }
                        }
                        .buttonStyle(.borderedProminent)
                    } else if entry.isUntracked {
                        Button("Discard", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: .untracked, entries: [entry]))
                        }
                        Button("Stage") {
                            Task { await panelModel.stage([entry]) }
                        }
                        .buttonStyle(.borderedProminent)
                    }
                }
            }
        }
        .disabled(panelModel.mutationInProgress)
        .padding(8)
        .overlay(alignment: .top) { Divider() }
    }

    private func selectAdjacent(_ delta: Int) {
        let entries = panelModel.status.entries
        guard !entries.isEmpty else { return }
        let current = entries.firstIndex {
            $0.path == panelModel.selectedDiffPath
        } ?? 0
        let next = min(max(current + delta, 0), entries.count - 1)
        Task { await panelModel.selectDiff(entries[next]) }
    }
}

private enum DiffSide {
    case old, new
}

private struct DiffPane: View {
    let rows: [GitDiffSideBySideRow]
    let side: DiffSide

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(rows) { row in
                    if row.isHunk {
                        Text(row.left?.text ?? "")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(AppTheme.subtitle)
                            .padding(.vertical, 1)
                            .padding(.leading, 8)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(AppTheme.diffHunkBackground)
                    } else {
                        lineRow(side == .old ? row.left : row.right)
                    }
                }
            }
            .padding(.vertical, 4)
        }
        .frame(maxHeight: .infinity)
    }

    private func lineRow(_ line: GitDiffLine?) -> some View {
        let number = side == .old ? line?.oldLineNumber : line?.newLineNumber
        return HStack(alignment: .top, spacing: 0) {
            Text(number.map(String.init) ?? "")
                .frame(width: 38, alignment: .trailing)
                .foregroundStyle(AppTheme.meta)
            Text(line?.text ?? " ")
                .textSelection(.enabled)
                .foregroundStyle(foreground(for: line))
                .padding(.leading, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(.system(size: 11, design: .monospaced))
        .padding(.vertical, 1)
        .background(background(for: line))
    }

    private func foreground(for line: GitDiffLine?) -> Color {
        switch line?.kind {
        case .addition: AppTheme.diffAddition
        case .deletion: AppTheme.diffDeletion
        default: AppTheme.subtitle
        }
    }

    private func background(for line: GitDiffLine?) -> Color {
        switch line?.kind {
        case .addition: AppTheme.diffAdditionBackground
        case .deletion: AppTheme.diffDeletionBackground
        default: .clear
        }
    }
}
