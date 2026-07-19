import SwiftUI
import TillerCore

/// Bottom section of the right panel: agents of the selected worktree with
/// their subagent tree. Click focuses the owning tab.
struct AgentsSectionView: View {
    @Bindable var appModel: AppModel
    let worktree: Worktree
    @State private var collapsedIds: Set<String> = []

    private var nodes: [AgentNode] {
        AgentsPanelModel.nodes(appModel: appModel, worktree: worktree)
    }

    private var runningCount: Int {
        nodes.filter { $0.status == .running }.count
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Agents")
                    .font(.subheadline.weight(.semibold))
                Spacer()
                if runningCount > 0 {
                    Text("\(runningCount) running")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            Divider()
            if nodes.isEmpty {
                ContentUnavailableView(
                    "No agents running",
                    systemImage: "person.2.slash",
                    description: Text("Agents launched in this worktree appear here."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 2) {
                        ForEach(nodes) { node in
                            row(node, depth: 0)
                            if !collapsedIds.contains(node.id) {
                                ForEach(node.children) { child in
                                    row(child, depth: 1)
                                }
                            }
                        }
                    }
                    .padding(6)
                }
            }
        }
    }

    @ViewBuilder
    private func row(_ node: AgentNode, depth: Int) -> some View {
        HStack(spacing: 6) {
            if depth == 0, !node.children.isEmpty {
                Button {
                    if collapsedIds.contains(node.id) {
                        collapsedIds.remove(node.id)
                    } else {
                        collapsedIds.insert(node.id)
                    }
                } label: {
                    Image(systemName: collapsedIds.contains(node.id)
                          ? "chevron.right" : "chevron.down")
                        .font(.caption2)
                }
                .buttonStyle(.plain)
            }
            AgentIcon(agentId: node.agentId)
                .frame(width: 14, height: 14)
            Text(node.title)
                .font(.callout)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 4)
            statusDot(node.status)
        }
        .padding(.vertical, 3)
        .padding(.leading, CGFloat(depth) * 18 + 4)
        .padding(.trailing, 6)
        .contentShape(Rectangle())
        .onTapGesture { focus(node) }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(node.title), \(node.status.humanLabel)")
    }

    @ViewBuilder
    private func statusDot(_ status: AgentStatus) -> some View {
        switch status {
        case .running:
            RunningDots(color: .green, dotSize: 3)
        case .needsInput:
            Circle().fill(.yellow).frame(width: 7, height: 7)
        case .done:
            Image(systemName: "checkmark.circle.fill")
                .font(.caption2).foregroundStyle(.secondary)
        case .error:
            Image(systemName: "xmark.circle.fill")
                .font(.caption2).foregroundStyle(.red)
        }
    }

    private func focus(_ node: AgentNode) {
        switch node.kind {
        case .chat(let tabId):
            appModel.focusTab(tabId: tabId, in: worktree)
        case .terminal(let paneId):
            guard let tab = (appModel.tabs[worktree.id] ?? [])
                .first(where: { $0.leafIds.contains(paneId) }) else { return }
            appModel.focusTab(tabId: tab.id, in: worktree)
        case .subagent:
            // Subagents focus their parent: find the top-level node owning it.
            guard let parent = nodes.first(where: { p in
                p.children.contains(where: { $0.id == node.id })
            }) else { return }
            focus(parent)
        }
    }
}
