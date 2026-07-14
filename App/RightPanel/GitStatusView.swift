import SwiftUI
import TillerGit

struct PendingGitDiscard: Identifiable {
    enum Kind: Equatable { case changes, untracked }

    let id = UUID()
    let kind: Kind
    let entries: [GitStatusEntry]

    var title: String {
        entries.count == 1
            ? "Discard \(entries[0].path.value)?"
            : "Discard \(entries.count) files?"
    }

    var message: String {
        kind == .untracked
            ? "Untracked files will be permanently deleted."
            : "Unstaged changes will be restored from the Git index."
    }
}

struct GitStatusView: View {
    private enum SectionKind: Equatable { case staged, changes, untracked }

    @Bindable var panelModel: RightPanelModel
    let onOpenDiff: (GitStatusEntry) -> Void
    let requestDiscard: (PendingGitDiscard) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(panelModel.status.isClean ? "Working tree clean" : "Local changes")
                    .font(.system(size: 12, weight: .semibold))
                Spacer()
                Button { Task { await panelModel.refresh() } } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .help("Refresh Status")
            }
            .padding(8)
            Divider()

            if panelModel.gitLoading && panelModel.status.isClean {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = panelModel.gitError {
                ContentUnavailableView {
                    Label("Git status unavailable", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                } actions: {
                    Button("Riprova") { Task { await panelModel.refresh() } }
                }
            } else if panelModel.status.isClean {
                ContentUnavailableView("Working tree clean", systemImage: "checkmark.circle")
            } else {
                ScrollView {
                    LazyVStack(spacing: 10) {
                        statusSection(
                            "Staged", kind: .staged, entries: panelModel.status.staged,
                            actionTitle: "Unstage all"
                        ) { entries in
                            Task { await panelModel.unstage(entries) }
                        }
                        statusSection(
                            "Changes", kind: .changes, entries: panelModel.status.changes,
                            actionTitle: "Stage all"
                        ) { entries in
                            Task { await panelModel.stage(entries) }
                        }
                        statusSection(
                            "Untracked", kind: .untracked, entries: panelModel.status.untracked,
                            actionTitle: "Stage all"
                        ) { entries in
                            Task { await panelModel.stage(entries) }
                        }
                    }
                    .padding(8)
                }
            }
        }
        .disabled(panelModel.mutationInProgress)
    }

    @ViewBuilder
    private func statusSection(
        _ title: String,
        kind: SectionKind,
        entries: [GitStatusEntry],
        actionTitle: String,
        action: @escaping ([GitStatusEntry]) -> Void
    ) -> some View {
        if !entries.isEmpty {
            let actionable = entries.filter { !$0.isConflicted }
            VStack(spacing: 2) {
                HStack {
                    Text("\(title) (\(entries.count))")
                        .font(.system(size: 11, weight: .semibold))
                    Spacer()
                    Button(actionTitle) { action(actionable) }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    if kind == .changes {
                        Button("Discard all", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: .changes, entries: actionable))
                        }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    } else if kind == .untracked {
                        Button("Discard all", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: .untracked, entries: actionable))
                        }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    }
                }
                ForEach(entries, id: \.path) { entry in
                    statusRow(entry, section: kind)
                }
            }
        }
    }

    private func statusRow(_ entry: GitStatusEntry, section: SectionKind) -> some View {
        HStack(spacing: 7) {
            Text(symbol(entry))
                .font(.system(size: 10, weight: .bold, design: .monospaced))
                .foregroundStyle(color(entry))
                .frame(width: 14)
            Text(entry.path.value)
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            if entry.isConflicted {
                Text("Resolve in terminal")
                    .font(.caption2)
                    .foregroundStyle(AppTheme.gitConflict)
            } else if section == .staged {
                Button("Unstage") {
                    Task { await panelModel.unstage([entry]) }
                }
                .buttonStyle(.plain)
            } else {
                Button("Discard", role: .destructive) {
                    requestDiscard(PendingGitDiscard(
                        kind: entry.isUntracked ? .untracked : .changes,
                        entries: [entry]))
                }
                .buttonStyle(.plain)
                Button("Stage") {
                    Task { await panelModel.stage([entry]) }
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 5)
        .contentShape(Rectangle())
        .background(AppTheme.rowHover.opacity(0.001),
                    in: RoundedRectangle(cornerRadius: 6))
        .onTapGesture { onOpenDiff(entry) }
        .help(entry.isConflicted ? "Conflicted" : entry.path.value)
    }

    private func symbol(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "U" }
        if entry.isUntracked { return "?" }
        if entry.indexState == .added { return "A" }
        if entry.indexState == .deleted || entry.worktreeState == .deleted { return "D" }
        if entry.indexState == .renamed || entry.worktreeState == .renamed { return "R" }
        return "M"
    }

    private func color(_ entry: GitStatusEntry) -> Color {
        if entry.isConflicted { return AppTheme.gitConflict }
        if entry.isUntracked { return AppTheme.gitUntracked }
        if entry.isStaged { return AppTheme.gitStaged }
        return AppTheme.gitModified
    }
}
