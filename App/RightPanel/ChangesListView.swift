import SwiftUI
import TillerCore
import TillerGit
import Inject

struct ChangesListView: View {
    @ObserveInjection private var inject

    private enum SectionKind: Equatable { case staged, changes, untracked }

    @Bindable var panelModel: RightPanelModel
    let worktree: Worktree
    let onOpenFile: (URL) -> Void
    let requestDiscard: (PendingGitDiscard) -> Void

    @AppStorage(AppSettings.fileIconThemeKey) private var fileIconThemeRaw = FileIconTheme.sfSymbols.rawValue

    private var iconTheme: FileIconTheme {
        FileIconTheme(rawValue: fileIconThemeRaw) ?? .sfSymbols
    }

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
                .help("Refresh Changes")
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
                    Button("Retry") { Task { await panelModel.refresh() } }
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
    .enableInjection()
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
                    if kind == .changes || kind == .untracked {
                        Button("Discard all", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: kind == .untracked ? .untracked : .changes,
                                entries: actionable))
                        }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    }
                }

                ForEach(entries, id: \.path) { entry in
                    changedFile(entry, section: kind)
                }
            }
        }
    }

    private func changedFile(_ entry: GitStatusEntry, section: SectionKind) -> some View {
        VStack(spacing: 0) {
            ChangedFileRow(
                entry: entry,
                stat: panelModel.diffStats[entry.path],
                isExpanded: panelModel.diffStore.isExpanded(entry.path),
                iconTheme: iconTheme,
                isStagedSection: section == .staged,
                onToggle: {
                    Task { await panelModel.diffStore.toggle(entry, repoPath: worktree.path) }
                },
                onStage: { Task { await panelModel.stage([entry]) } },
                onUnstage: { Task { await panelModel.unstage([entry]) } },
                onDiscard: {
                    requestDiscard(PendingGitDiscard(
                        kind: entry.isUntracked ? .untracked : .changes,
                        entries: [entry]))
                },
                onOpenFile: { onOpenFile(fileURL(for: entry)) })

            if panelModel.diffStore.isExpanded(entry.path) {
                diffBody(for: entry)
            }
        }
    }

    @ViewBuilder
    private func diffBody(for entry: GitStatusEntry) -> some View {
        switch panelModel.diffStore.state(for: entry.path) {
        case .idle, .loading:
            HStack(spacing: 8) {
                ProgressView()
                    .controlSize(.small)
                Text("Loading diff…")
                    .font(.caption)
                    .foregroundStyle(AppTheme.meta)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 20)
            .padding(.vertical, 8)
        case .loaded(let diff):
            if diff.isBinary {
                ContentUnavailableView(
                    "Binary diff unavailable", systemImage: "doc.richtext")
                    .padding(.vertical, 8)
            } else {
                FileDiffBody(diff: diff, fileURL: fileURL(for: entry))
            }
        case .failed(let error):
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle")
                    .foregroundStyle(AppTheme.gitModified)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Diff unavailable")
                        .font(.caption.weight(.semibold))
                    Text(error)
                        .font(.caption2)
                        .foregroundStyle(AppTheme.meta)
                        .lineLimit(2)
                }
                Spacer()
                Button("Retry") {
                    Task { await panelModel.diffStore.retry(entry, repoPath: worktree.path) }
                }
                .buttonStyle(.plain)
                .font(.caption)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 8)
        }
    }

    private func fileURL(for entry: GitStatusEntry) -> URL {
        URL(fileURLWithPath: worktree.path, isDirectory: true)
            .appendingPathComponent(entry.path.value)
    }
}
