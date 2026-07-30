import SwiftUI
import TillerCore
import TillerWorkspace

enum WorkspaceMenuShortcutModifier: Hashable, Sendable {
    case command
    case control
    case option
    case shift
}

struct WorkspaceMenuShortcut: Equatable, Sendable {
    let key: String
    let modifiers: Set<WorkspaceMenuShortcutModifier>

    init(key: String, modifiers: Set<WorkspaceMenuShortcutModifier>) {
        self.key = key
        self.modifiers = modifiers
    }
}

enum WorkspaceMenuDirection: Equatable, Sendable {
    case left
    case right
    case above
    case below
}

enum WorkspaceMenuDestination: Equatable, Sendable {
    case group(PaneGroupID, index: Int)
    case newPane(SplitPlacementSide)
}

enum WorkspaceMenuAction: Equatable, Sendable {
    case nextTab
    case previousTab
    case tab(Int)
    case moveEarlier
    case moveLater
    case moveTo(WorkspaceMenuDestination)
    case closeTab
    case focusPane(WorkspaceMenuDirection)
    case split(SplitPlacementSide)
    case focusNextDivider
    case focusPreviousDivider
}

struct WorkspaceMenuItem: Equatable, Identifiable, Sendable {
    let id: String
    let label: String
    let destination: WorkspaceMenuDestination?
    let isEnabled: Bool

    init(
        id: String,
        label: String,
        destination: WorkspaceMenuDestination? = nil,
        isEnabled: Bool = true
    ) {
        self.id = id
        self.label = label
        self.destination = destination
        self.isEnabled = isEnabled
    }
}

struct WorkspaceMenuModel: Equatable, Sendable {
    let activeGroupID: PaneGroupID
    let activeTabID: WorkspaceTabID?
    let localTabIDs: [WorkspaceTabID]
    let nextTabID: WorkspaceTabID?
    let previousTabID: WorkspaceTabID?
    let moveEarlier: WorkspaceMenuItem
    let moveLater: WorkspaceMenuItem
    let moveEarlierTarget: WorkspaceTabID?
    let moveLaterTarget: WorkspaceTabID?
    let moveDestinations: [WorkspaceMenuItem]

    init(layout: WorkspaceLayout) {
        activeGroupID = layout.activeGroupID
        let group = layout.group(layout.activeGroupID)
        let tabs = group?.tabs ?? []
        localTabIDs = tabs.map(\.id)
        activeTabID = group?.activeTabID

        if let activeIndex = tabs.firstIndex(where: { $0.id == group?.activeTabID }), !tabs.isEmpty {
            nextTabID = tabs[(activeIndex + 1) % tabs.count].id
            previousTabID = tabs[(activeIndex + tabs.count - 1) % tabs.count].id
            moveEarlierTarget = activeIndex > 0 ? tabs[activeIndex - 1].id : nil
            moveLaterTarget = activeIndex + 1 < tabs.count ? tabs[activeIndex + 1].id : nil
        } else {
            nextTabID = nil
            previousTabID = nil
            moveEarlierTarget = nil
            moveLaterTarget = nil
        }

        moveEarlier = WorkspaceMenuItem(
            id: "move-earlier", label: "Move Earlier", isEnabled: moveEarlierTarget != nil)
        moveLater = WorkspaceMenuItem(
            id: "move-later", label: "Move Later", isEnabled: moveLaterTarget != nil)

        var destinations: [WorkspaceMenuItem] = []
        for (index, groupID) in layout.orderedGroupIDs.enumerated()
        where groupID != layout.activeGroupID {
            let collapsesSource = tabs.count == 1
            let label = collapsesSource
                ? "Pane \(index + 1) (collapses source pane)"
                : "Pane \(index + 1)"
            let destination = WorkspaceMenuDestination.group(
                groupID, index: layout.group(groupID)?.tabs.count ?? 0)
            destinations.append(WorkspaceMenuItem(
                id: "group-\(groupID.rawValue.uuidString)",
                label: label,
                destination: destination,
                isEnabled: activeTabID != nil))
        }
        for placement in SplitPlacementSide.allCases {
            let label: String
            switch placement {
            case .left: label = "New Pane Left"
            case .right: label = "New Pane Right"
            case .above: label = "New Pane Above"
            case .below: label = "New Pane Below"
            }
            destinations.append(WorkspaceMenuItem(
                id: "new-pane-\(placement.rawValue)",
                label: label,
                destination: .newPane(placement),
                isEnabled: tabs.count > 1 && activeTabID != nil))
        }
        moveDestinations = destinations
    }
}

struct WorkspaceMenuMovePreview: Sendable {
    let originalLayout: WorkspaceLayout
    let previewedLayout: WorkspaceLayout?

    init(layout: WorkspaceLayout, tabID: WorkspaceTabID, destination: WorkspaceMenuDestination) {
        originalLayout = layout
        let coreDestination: MoveDestination
        switch destination {
        case .group(let groupID, let index):
            coreDestination = .group(groupID, index: index)
        case .newPane(let placement):
            coreDestination = .newSplit(
                anchor: layout.activeGroupID,
                placement: placement,
                newGroup: PaneGroupID(),
                newSplit: SplitID())
        }
        if case .success(let transition) = WorkspaceLayoutEngine.apply(
            .moveTab(tabID, to: coreDestination), to: layout) {
            previewedLayout = transition.layout
        } else {
            previewedLayout = nil
        }
    }
}

@MainActor
final class WorkspaceMenuMoveSession {
    private let originalLayout: WorkspaceLayout
    private(set) var layout: WorkspaceLayout
    private(set) var isPreviewing = false
    private(set) var cancelledDestination: WorkspaceMenuDestination?

    init(layout: WorkspaceLayout) {
        originalLayout = layout
        self.layout = layout
    }

    func preview(tabID: WorkspaceTabID, destination: WorkspaceMenuDestination) {
        let preview = WorkspaceMenuMovePreview(
            layout: originalLayout, tabID: tabID, destination: destination)
        if let previewedLayout = preview.previewedLayout {
            layout = previewedLayout
            isPreviewing = true
        }
        cancelledDestination = destination
    }

    func cancel() {
        layout = originalLayout
        isPreviewing = false
    }
}

@MainActor
final class WorkspaceMenuTarget {
    let worktreeID: UUID
    let menuModel: WorkspaceMenuModel?
    private let action: (WorkspaceMenuAction) -> Void

    init(
        worktreeID: UUID,
        menuModel: WorkspaceMenuModel? = nil,
        send: @escaping (WorkspaceMenuAction) -> Void
    ) {
        self.worktreeID = worktreeID
        self.menuModel = menuModel
        action = send
    }

    func send(_ action: WorkspaceMenuAction) {
        self.action(action)
    }
}

enum WorkspaceMenuCommandCatalog {
    static let nextTabShortcut = WorkspaceMenuShortcut(
        key: "tab", modifiers: [.control])
    static let previousTabShortcut = WorkspaceMenuShortcut(
        key: "tab", modifiers: [.control, .shift])

    static let allShortcuts: [WorkspaceMenuShortcut] = [
        nextTabShortcut,
        previousTabShortcut
    ] + (1...9).map { WorkspaceMenuShortcut(key: "\($0)", modifiers: [.command]) } + [
        WorkspaceMenuShortcut(key: "left", modifiers: [.command, .option]),
        WorkspaceMenuShortcut(key: "right", modifiers: [.command, .option]),
        WorkspaceMenuShortcut(key: "up", modifiers: [.command, .option]),
        WorkspaceMenuShortcut(key: "down", modifiers: [.command, .option]),
        WorkspaceMenuShortcut(key: "right", modifiers: [.command, .option, .shift]),
        WorkspaceMenuShortcut(key: "down", modifiers: [.command, .option, .shift])
    ]

    static let labels: [String] = [
        "Next Tab", "Previous Tab",
        "Tab 1", "Tab 2", "Tab 3", "Tab 4", "Tab 5", "Tab 6", "Tab 7", "Tab 8", "Last Tab",
        "Move Earlier", "Move Later", "Move Tab To…", "Close Tab",
        "Focus Pane Left", "Focus Pane Right", "Focus Pane Above", "Focus Pane Below",
        "Split Right With…", "Split Down With…", "Focus Next Divider", "Focus Previous Divider",
        "New Pane Left", "New Pane Right", "New Pane Above", "New Pane Below"
    ]
}

private struct WorkspaceMenuTargetKey: FocusedValueKey {
    typealias Value = WorkspaceMenuTarget
}

extension FocusedValues {
    var workspaceMenuTarget: WorkspaceMenuTarget? {
        get { self[WorkspaceMenuTargetKey.self] }
        set { self[WorkspaceMenuTargetKey.self] = newValue }
    }
}

struct WorkspaceMenuCommands: Commands {
    @FocusedValue(\.workspaceMenuTarget) private var target

    var body: some Commands {
        CommandMenu("Tab") {
            Button("Next Tab") { target?.send(.nextTab) }
                .keyboardShortcut(.tab, modifiers: .control)
                .disabled(target == nil)
            Button("Previous Tab") { target?.send(.previousTab) }
                .keyboardShortcut(.tab, modifiers: [.control, .shift])
                .disabled(target == nil)
            Divider()
            ForEach(1...9, id: \.self) { number in
                Button(number == 9 ? "Last Tab" : "Tab \(number)") {
                    target?.send(.tab(number))
                }
                .keyboardShortcut(
                    KeyEquivalent(Character("\(number)")), modifiers: .command)
                .disabled(target == nil)
            }
            Divider()
            Button("Move Earlier") { target?.send(.moveEarlier) }
                .disabled(target?.menuModel?.moveEarlier.isEnabled != true)
            Button("Move Later") { target?.send(.moveLater) }
                .disabled(target?.menuModel?.moveLater.isEnabled != true)
            Menu("Move Tab To…") {
                ForEach(target?.menuModel?.moveDestinations ?? []) { item in
                    Button(item.label) {
                        if let destination = item.destination { target?.send(.moveTo(destination)) }
                    }
                    .disabled(!item.isEnabled)
                }
            }
            .disabled(target == nil)
            Button("Close Tab") { target?.send(.closeTab) }
                .keyboardShortcut("w", modifiers: .command)
                .disabled(target == nil)
        }

        CommandMenu("Pane") {
            Button("Focus Pane Left") { target?.send(.focusPane(.left)) }
                .keyboardShortcut(.leftArrow, modifiers: [.command, .option])
                .disabled(target == nil)
            Button("Focus Pane Right") { target?.send(.focusPane(.right)) }
                .keyboardShortcut(.rightArrow, modifiers: [.command, .option])
                .disabled(target == nil)
            Button("Focus Pane Above") { target?.send(.focusPane(.above)) }
                .keyboardShortcut(.upArrow, modifiers: [.command, .option])
                .disabled(target == nil)
            Button("Focus Pane Below") { target?.send(.focusPane(.below)) }
                .keyboardShortcut(.downArrow, modifiers: [.command, .option])
                .disabled(target == nil)
            Divider()
            Button("Split Right With…") { target?.send(.split(.right)) }
                .keyboardShortcut(.rightArrow, modifiers: [.command, .option, .shift])
                .disabled(target == nil)
            Button("Split Down With…") { target?.send(.split(.below)) }
                .keyboardShortcut(.downArrow, modifiers: [.command, .option, .shift])
                .disabled(target == nil)
            Divider()
            Button("Focus Next Divider") { target?.send(.focusNextDivider) }
                .disabled(target == nil)
            Button("Focus Previous Divider") { target?.send(.focusPreviousDivider) }
                .disabled(target == nil)
        }
    }
}

extension WorkspaceCoordinator {
    func handle(menuAction: WorkspaceMenuAction, in worktree: Worktree) async {
        guard let layout = layouts[worktree.id] else { return }
        let group = layout.group(layout.activeGroupID)
        let activeTab = group?.activeTabID

        switch menuAction {
        case .nextTab:
            guard let id = WorkspaceMenuModel(layout: layout).nextTabID else { return }
            await handle(.activateTab(id), in: worktree)
        case .previousTab:
            guard let id = WorkspaceMenuModel(layout: layout).previousTabID else { return }
            await handle(.activateTab(id), in: worktree)
        case .tab(let number):
            let tabs = group?.tabs ?? []
            guard let index = TabOrdering.selectionIndex(number: number, count: tabs.count) else { return }
            await handle(.activateTab(tabs[index].id), in: worktree)
        case .moveEarlier:
            guard let activeTab, let index = group?.tabs.firstIndex(where: { $0.id == activeTab }), index > 0 else { return }
            await handle(.requestMove(activeTab, to: .group(layout.activeGroupID, index: index - 1)), in: worktree)
        case .moveLater:
            guard let activeTab,
                  let tabs = group?.tabs,
                  let index = tabs.firstIndex(where: { $0.id == activeTab }),
                  index + 1 < tabs.count else { return }
            await handle(.requestMove(activeTab, to: .group(layout.activeGroupID, index: index + 1)), in: worktree)
        case .moveTo(let destination):
            guard let activeTab else { return }
            switch destination {
            case .group(let groupID, let index):
                await handle(.requestMove(activeTab, to: .group(groupID, index: index)), in: worktree)
            case .newPane(let placement):
                await handle(.requestMove(
                    activeTab, to: .edgeSplit(anchor: layout.activeGroupID, placement: placement)), in: worktree)
            }
        case .closeTab:
            guard let activeTab else { return }
            await closeTab(activeTab, in: worktree)
        case .focusPane(let direction):
            let ids = layout.orderedGroupIDs
            guard let index = ids.firstIndex(of: layout.activeGroupID) else { return }
            let nextIndex: Int
            switch direction {
            case .left, .above: nextIndex = index - 1
            case .right, .below: nextIndex = index + 1
            }
            guard ids.indices.contains(nextIndex) else { return }
            await handle(.activateGroup(ids[nextIndex]), in: worktree)
        case .split(let placement):
            await requestSplit(
                anchor: layout.activeGroupID, placement: placement,
                choice: .newTerminal, in: worktree)
        case .focusNextDivider, .focusPreviousDivider:
            let splits = layout.splitIDs()
            guard !splits.isEmpty else { return }
            let current = lastFocusIntent
            let currentIndex: Int? = {
                guard case .focusDivider(let id) = current else { return nil }
                return splits.firstIndex(of: id)
            }()
            let index: Int
            if menuAction == .focusNextDivider {
                index = min((currentIndex ?? -1) + 1, splits.count - 1)
            } else {
                index = max((currentIndex ?? splits.count) - 1, 0)
            }
            guard let splitID = splits[safe: index], let fraction = layout.preferredFraction(for: splitID) else { return }
            await handle(.setPreferredFraction(splitID, fraction), in: worktree)
        }
    }
}

private extension Array {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
