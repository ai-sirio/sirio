import AppKit
import SwiftUI
import TillerAgents
import TillerCore
import TillerWorkspace

struct SplitMenuAgent: Equatable, Identifiable {
    let id: String
    let name: String
}

struct SplitMenuChat: Equatable, Identifiable {
    let id: String
    let title: String
}

enum SplitContentMenuGroup: Equatable {
    case thisPane
    case otherPanes
}

enum SplitContentMenuAction: Equatable {
    case newTerminal
    case agentTerminal(String)
    case newChat(String)
    case resumeChat(String)
    case openFile
    case moveExistingTab(WorkspaceTabID)
    case configureAgents
    case closeTab(WorkspaceTabID)
}

struct SplitContentMenuItem: Equatable, Identifiable {
    let id: String
    let label: String
    let action: SplitContentMenuAction?
    let isEnabled: Bool
    let disabledReason: String?
}

struct SplitContentMenuModel: Equatable {
    let items: [SplitContentMenuItem]
    let moveGroups: [SplitContentMenuGroup: [WorkspaceTabID]]
    let submenuItems: [SplitContentMenuSubmenu: [SplitContentMenuItem]]
    let tabLabels: [WorkspaceTabID: String]
    let splitItem: SplitContentMenuItem
    let defaultItem: SplitContentMenuAction

    var topLevelLabels: [String] { items.map(\.label) }
    /// Appended last, so the indices the menu view reads stay put.
    var closeItem: SplitContentMenuItem? { items.first { $0.id == "close-tab" } }
    var allMoveTabIDs: [WorkspaceTabID] {
        moveTabIDs(in: .thisPane) + moveTabIDs(in: .otherPanes)
    }

    init(
        worktreeID: UUID,
        sourceTabID: WorkspaceTabID,
        layout: WorkspaceLayout,
        layoutsByWorktree: [UUID: WorkspaceLayout],
        groupSize: CGSize,
        placement: SplitPlacementSide,
        installedAgents: [SplitMenuAgent],
        resumedChats: [SplitMenuChat]
    ) {
        let activeGroup = layout.group(layout.activeGroupID)
        let eligibility = SplitEligibility.check(
            groupSize: groupSize,
            placement: placement,
            isSoleTabOfSourceGroup: activeGroup?.tabs.count == 1)
        let splitEnabled: Bool
        let splitReason: String?
        switch eligibility {
        case .success:
            splitEnabled = true
            splitReason = nil
        case .failure(let reason):
            splitEnabled = false
            splitReason = reason.message
        }
        splitItem = SplitContentMenuItem(
            id: "split",
            label: placement == .right ? "Split Right" : "Split Down", // menu is only ever built for .right/.below
            action: .newTerminal,
            isEnabled: splitEnabled,
            disabledReason: splitReason)

        let thisPane = activeGroup?.tabs.map(\.id).filter { $0 != sourceTabID } ?? []
        let otherPanes = layout.orderedGroupIDs
            .filter { $0 != layout.activeGroupID }
            .flatMap { layout.group($0)?.tabs.map(\.id) ?? [] }
            .filter { $0 != sourceTabID }
        // The source is intentionally derived from the selected worktree only.
        // Other worktrees are not traversed into the move inventory.
        _ = layoutsByWorktree[worktreeID]
        moveGroups = [.thisPane: thisPane, .otherPanes: otherPanes]
        tabLabels = Dictionary(uniqueKeysWithValues: layout.allTabs.map { ($0.id, $0.title) })

        let chatItems: [SplitContentMenuItem] = installedAgents.isEmpty
            ? [SplitContentMenuItem(
                id: "no-agents", label: "No agents are installed.", action: nil,
                isEnabled: false, disabledReason: nil),
               SplitContentMenuItem(
                id: "configure-agents", label: "Configure Agents…",
                action: .configureAgents, isEnabled: true, disabledReason: nil)]
            : installedAgents.map {
                SplitContentMenuItem(
                    id: "chat-\($0.id)", label: $0.name,
                    action: .newChat($0.id), isEnabled: splitEnabled, disabledReason: splitReason)
            }
        let resumeItems: [SplitContentMenuItem] = resumedChats.isEmpty
            ? [SplitContentMenuItem(
                id: "no-chats", label: "No past chats.", action: nil,
                isEnabled: false, disabledReason: nil)]
            : resumedChats.map {
                SplitContentMenuItem(
                    id: "resume-\($0.id)", label: $0.title,
                    action: .resumeChat($0.id), isEnabled: splitEnabled, disabledReason: splitReason)
            }
        let moveItems = thisPane + otherPanes
        let moveEnabled = splitEnabled
        let moveItem = SplitContentMenuItem(
            id: "move", label: "Move Existing Tab", action: nil,
            isEnabled: moveEnabled,
            disabledReason: moveItems.isEmpty ? "No other tabs are available." : splitReason)

        items = [
            SplitContentMenuItem(
                id: "new-terminal", label: "New Terminal", action: .newTerminal,
                isEnabled: splitEnabled, disabledReason: splitReason),
            SplitContentMenuItem(
                id: "agent-terminal", label: "Agent Terminal", action: nil,
                isEnabled: splitEnabled, disabledReason: splitReason),
            SplitContentMenuItem(
                id: "separator-1", label: "", action: nil, isEnabled: true, disabledReason: nil),
            SplitContentMenuItem(
                id: "new-chat", label: "New Chat", action: nil,
                isEnabled: installedAgents.isEmpty || splitEnabled, disabledReason: splitReason),
            SplitContentMenuItem(
                id: "resume-chat", label: "Resume Chat", action: nil,
                isEnabled: resumedChats.isEmpty || splitEnabled, disabledReason: splitReason),
            SplitContentMenuItem(
                id: "open-file", label: "Open File…", action: .openFile,
                isEnabled: splitEnabled, disabledReason: splitReason),
            SplitContentMenuItem(
                id: "separator-2", label: "", action: nil, isEnabled: true, disabledReason: nil),
            moveItem,
            SplitContentMenuItem(
                id: "separator-3", label: "", action: nil, isEnabled: true, disabledReason: nil),
            SplitContentMenuItem(
                id: "close-tab", label: "Close Tab", action: .closeTab(sourceTabID),
                isEnabled: true, disabledReason: nil)
        ]
        submenuItems = [.newChat: chatItems, .resumeChat: resumeItems]
        defaultItem = .newTerminal
    }

    func moveTabIDs(in group: SplitContentMenuGroup) -> [WorkspaceTabID] {
        moveGroups[group] ?? []
    }

    func hasDisabledExplanation(in submenu: SplitContentMenuSubmenu) -> Bool {
        submenuItems[submenu]?.contains {
            !$0.isEnabled && $0.action == nil
        } == true
    }

    var hasConfigureAgentsAction: Bool {
        submenuItems[.newChat]?.contains(.init(
            id: "configure-agents", label: "Configure Agents…",
            action: .configureAgents, isEnabled: true, disabledReason: nil)) == true
    }

    func label(for tabID: WorkspaceTabID) -> String {
        tabLabels[tabID] ?? tabID.rawValue.uuidString
    }
}

enum SplitContentMenuSubmenu: Hashable {
    case newChat
    case resumeChat
}

@MainActor
final class SplitContentMenuSession {
    private let mutate: (ContentChoice) -> Void
    private(set) var isPresented = false

    init(mutate: @escaping (ContentChoice) -> Void) { self.mutate = mutate }

    func open() { isPresented = true }

    func choose(_ choice: ContentChoice) {
        guard isPresented else { return }
        isPresented = false
        mutate(choice)
    }
}

struct SplitContentMenu: View {
    let model: SplitContentMenuModel
    let onAction: (SplitContentMenuAction) -> Void

    var body: some View {
        Menu {
            SplitContentMenuItems(model: model, onAction: onAction)
        } label: {
            Label("Split", systemImage: "rectangle.split.2x1")
        }
    }
}

/// The menu body on its own, so the toolbar's Split button and the pane's
/// right-click menu stay one definition rather than two that drift apart.
struct SplitContentMenuItems: View {
    let model: SplitContentMenuModel
    let onAction: (SplitContentMenuAction) -> Void

    var body: some View {
        Group {
            Button("New Terminal") { onAction(.newTerminal) }
                .disabled(!model.items[0].isEnabled)
            Menu("Agent Terminal") {
                ForEach(AgentCatalog.all, id: \.id) { agent in
                    Button(agent.displayName) { onAction(.agentTerminal(agent.id)) }
                }
            }
            .disabled(!model.items[1].isEnabled)
            Divider()
            Menu("New Chat") {
                ForEach(model.submenuItems[.newChat] ?? []) { item in
                    if let action = item.action {
                        Button(item.label) { onAction(action) }
                            .disabled(!item.isEnabled)
                    } else {
                        Text(item.label).disabled(true)
                    }
                }
            }
            .disabled(!model.items[3].isEnabled)
            Menu("Resume Chat") {
                ForEach(model.submenuItems[.resumeChat] ?? []) { item in
                    if let action = item.action {
                        Button(item.label) { onAction(action) }
                            .disabled(!item.isEnabled)
                    } else {
                        Text(item.label).disabled(true)
                    }
                }
            }
            .disabled(!model.items[4].isEnabled)
            Button("Open File…") { onAction(.openFile) }
                .disabled(!model.items[5].isEnabled)
            Divider()
            Menu("Move Existing Tab") {
                let thisPane = model.moveTabIDs(in: .thisPane)
                if thisPane.isEmpty {
                    Text("No other tabs in this pane.")
                } else {
                    Text("This Pane")
                    ForEach(thisPane, id: \.self) { tabID in
                        Button(model.label(for: tabID)) {
                            onAction(.moveExistingTab(tabID))
                        }
                    }
                }
                let otherPanes = model.moveTabIDs(in: .otherPanes)
                if otherPanes.isEmpty {
                    Text("No other panes in this layout.")
                } else {
                    Text("Other Panes")
                    ForEach(otherPanes, id: \.self) { tabID in
                        Button(model.label(for: tabID)) {
                            onAction(.moveExistingTab(tabID))
                        }
                    }
                }
            }
            .disabled(!model.items[7].isEnabled)
            if let close = model.closeItem, let action = close.action {
                Divider()
                Button(close.label) { onAction(action) }
                    .disabled(!close.isEnabled)
            }
        }
    }
}

private extension SplitEligibility.Reason {
    var message: String {
        switch self {
        case .insufficientWidth(let available, let required):
            "Not enough width to split right (\(Int(available)) available; \(Int(required)) required)."
        case .insufficientHeight(let available, let required):
            "Not enough height to split down (\(Int(available)) available; \(Int(required)) required)."
        case .soleTabOfItsOwnGroup:
            "This pane has only one tab and cannot be split."
        }
    }
}
