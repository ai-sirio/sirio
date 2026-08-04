import SwiftUI
import TillerCore

struct RightPanelView: View {
    @Bindable var appModel: AppModel
    @Bindable var panelModel: RightPanelModel
    @Binding var modeRaw: String
    let isGitRepository: Bool
    let onClose: () -> Void
    @State private var pendingDiscard: PendingGitDiscard?

    private var effectiveMode: RightPanelMode {
        .effective(rawValue: modeRaw, isGitRepository: isGitRepository)
    }

    private var selectedMode: Binding<RightPanelMode> {
        Binding(
            get: { effectiveMode },
            set: { modeRaw = $0.rawValue })
    }

    var body: some View {
        GeometryReader { geo in
            VStack(spacing: 0) {
                toolsRegion
                    .frame(height: geo.size.height * 0.75)
                Divider()
                Group {
                    if let worktree = panelModel.worktree {
                        AgentsSectionView(appModel: appModel, worktree: worktree)
                    } else {
                        ContentUnavailableView(
                            "No agents running",
                            systemImage: "person.2.slash")
                    }
                }
                .frame(maxHeight: .infinity)
            }
        }
        .alert(item: $pendingDiscard) { pending in
            Alert(
                title: Text(pending.title),
                message: Text(pending.message),
                primaryButton: .destructive(Text("Discard")) {
                    Task {
                        switch pending.kind {
                        case .changes:
                            await panelModel.discardChanges(pending.entries)
                        case .untracked:
                            await panelModel.discardUntracked(pending.entries)
                        }
                    }
                },
                secondaryButton: .cancel(Text("Cancel")))
        }
    }

    /// Existing panel content (header picker + Files/Changes), unchanged.
    private var toolsRegion: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Picker("Worktree tools", selection: selectedMode) {
                    ForEach(RightPanelMode.allCases) { mode in
                        Label(mode.title, systemImage: mode.systemImage)
                            .tag(mode)
                            .disabled(mode.requiresGit && !isGitRepository)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()

                Button(action: onClose) {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.plain)
                .help("Nascondi pannello destro (⌃⌘I)")
                .accessibilityLabel("Nascondi pannello destro")
            }
            .padding(8)
            Divider()
            if let monitorError = panelModel.monitorError {
                Label(monitorError, systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(AppTheme.gitModified)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                Divider()
            }

            Group {
                if let worktree = panelModel.worktree {
                    switch effectiveMode {
                    case .files:
                        FileExplorerView(
                            appModel: appModel,
                            panelModel: panelModel,
                            worktree: worktree)
                    case .status:
                        ChangesListView(
                            panelModel: panelModel,
                            worktree: worktree,
                            onOpenFile: { url in appModel.openDocument(fileURL: url, in: worktree) },
                            requestDiscard: { pendingDiscard = $0 })
                    }
                } else {
                    ContentUnavailableView(
                        "No worktree selected",
                        systemImage: "sidebar.right",
                        description: Text(
                            "Select a worktree to inspect its files and changes."))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}
