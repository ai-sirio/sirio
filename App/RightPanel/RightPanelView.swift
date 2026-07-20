import SwiftUI
import AppKit
import TillerCore

struct RightPanelView: View {
    @Bindable var appModel: AppModel
    @Bindable var panelModel: RightPanelModel
    @Binding var modeRaw: String
    let isGitRepository: Bool
    let onClose: () -> Void
    @State private var pendingDiscard: PendingGitDiscard?
    @AppStorage(AppSettings.rightPanelAgentsFractionKey)
    private var agentsFraction = AppSettings.defaultRightPanelAgentsFraction

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
                    .frame(height: max(120, geo.size.height * (1 - agentsFraction)))
                splitDivider(totalHeight: geo.size.height)
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
        .coordinateSpace(name: "rightPanelSplit")
        .task(id: effectiveMode) {
            if effectiveMode == .diff { await panelModel.ensureDiffLoaded() }
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
                secondaryButton: .cancel(Text("Annulla")))
        }
    }

    /// Existing panel content (header picker + Files/Diff/Status), unchanged.
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
                    case .diff:
                        GitDiffView(
                            panelModel: panelModel,
                            requestDiscard: { pendingDiscard = $0 })
                    case .status:
                        GitStatusView(
                            panelModel: panelModel,
                            onOpenDiff: { entry in
                                modeRaw = RightPanelMode.diff.rawValue
                                Task { await panelModel.selectDiff(entry) }
                            },
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

    private func splitDivider(totalHeight: CGFloat) -> some View {
        Divider()
            .frame(maxWidth: .infinity)
            .frame(height: 7)
            .contentShape(Rectangle())
            .onHover { inside in
                if inside { NSCursor.resizeUpDown.push() } else { NSCursor.pop() }
            }
            .gesture(
                DragGesture(coordinateSpace: .named("rightPanelSplit"))
                    .onChanged { value in
                        guard totalHeight > 0 else { return }
                        let newFraction = agentsFraction - value.translation.height / totalHeight
                        agentsFraction = min(0.6, max(0.15, newFraction))
                    })
    }
}
