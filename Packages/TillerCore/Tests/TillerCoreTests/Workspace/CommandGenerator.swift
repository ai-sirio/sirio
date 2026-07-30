import Foundation
@testable import TillerCore

struct SeededRandom {
    private var state: UInt64

    init(seed: UInt64) {
        state = seed
    }

    mutating func nextUInt64() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var value = state
        value = (value ^ (value >> 30)) &* 0xBF58_476D_1CE4_E5B9
        value = (value ^ (value >> 27)) &* 0x94D0_49BB_1331_11EB
        return value ^ (value >> 31)
    }

    mutating func nextInt(_ upperBound: Int) -> Int {
        precondition(upperBound > 0)
        return Int(nextUInt64() % UInt64(upperBound))
    }
}

enum CommandGenerator {
    private static let maximumGeneratedGroups = 8
    private static let maximumGeneratedTabs = 32

    static func next(
        for layout: WorkspaceLayout,
        rng: inout SeededRandom
    ) -> WorkspaceLayoutCommand {
        switch rng.nextInt(9) {
        case 0:
            guard layout.allTabs.count < maximumGeneratedTabs else {
                return fallback(in: layout, rng: &rng)
            }
            return insertTab(in: layout, rng: &rng)
        case 1:
            guard layout.groups.count < maximumGeneratedGroups else {
                return fallback(in: layout, rng: &rng)
            }
            return splitGroup(in: layout, rng: &rng)
        case 2:
            return moveTab(toGroupIn: layout, rng: &rng)
        case 3:
            guard layout.groups.count < maximumGeneratedGroups else {
                return moveTab(toGroupIn: layout, rng: &rng)
            }
            return moveTab(toNewSplitIn: layout, rng: &rng)
        case 4:
            guard let tab = layout.allTabs.randomElement(using: &rng) else {
                return insertTab(in: layout, rng: &rng)
            }
            return .closeTab(tab.id)
        case 5:
            guard let tab = layout.allTabs.randomElement(using: &rng) else {
                return insertTab(in: layout, rng: &rng)
            }
            return .activateTab(tab.id)
        case 6:
            guard let groupID = layout.orderedGroupIDs.randomElement(using: &rng) else {
                return insertTab(in: layout, rng: &rng)
            }
            return .activateGroup(groupID)
        case 7:
            guard let splitID = layout.splitIDs().randomElement(using: &rng) else {
                return insertTab(in: layout, rng: &rng)
            }
            let fraction = Double(rng.nextInt(80) + 10) / 100.0
            return .setPreferredFraction(splitID, fraction)
        default:
            guard let tab = layout.allTabs.randomElement(using: &rng) else {
                return insertTab(in: layout, rng: &rng)
            }
            if rng.nextInt(2) == 0 {
                return .updateViewState(tab.id, .empty)
            }
            return .renameTab(
                tab.id,
                title: "Generated \(rng.nextInt(1_000_000))",
                isAutoNamed: false)
        }
    }

    private static func insertTab(
        in layout: WorkspaceLayout,
        rng: inout SeededRandom
    ) -> WorkspaceLayoutCommand {
        let groupID = layout.orderedGroupIDs[rng.nextInt(layout.orderedGroupIDs.count)]
        let group = layout.group(groupID)!
        let tab = WorkspaceTab(
            id: WorkspaceTabID(),
            title: "Generated \(rng.nextInt(1_000_000))",
            titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        let index = rng.nextInt(group.tabs.count + 1)
        let activate = group.tabs.isEmpty || rng.nextInt(2) == 0
        return .insertTab(tab, into: groupID, index: index, activate: activate)
    }

    private static func fallback(
        in layout: WorkspaceLayout,
        rng: inout SeededRandom
    ) -> WorkspaceLayoutCommand {
        guard let tab = layout.allTabs.randomElement(using: &rng) else {
            return insertTab(in: layout, rng: &rng)
        }
        if rng.nextInt(2) == 0 {
            return .updateViewState(tab.id, .empty)
        }
        return .renameTab(
            tab.id,
            title: "Generated fallback \(rng.nextInt(1_000_000))",
            isAutoNamed: false)
    }

    private static func splitGroup(
        in layout: WorkspaceLayout,
        rng: inout SeededRandom
    ) -> WorkspaceLayoutCommand {
        let candidates = layout.orderedGroupIDs.filter {
            layout.group($0)?.tabs.isEmpty == false
        }
        guard !candidates.isEmpty else {
            return fallback(in: layout, rng: &rng)
        }
        let anchor = candidates[rng.nextInt(candidates.count)]
        let tab = WorkspaceTab(
            id: WorkspaceTabID(),
            title: "Generated split tab \(rng.nextInt(1_000_000))",
            titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        return .splitGroup(
            anchor: anchor,
            placement: SplitPlacementSide.allCases[rng.nextInt(SplitPlacementSide.allCases.count)],
            newGroup: PaneGroupID(),
            newSplit: SplitID(),
            content: .newTab(tab))
    }

    private static func moveTab(
        toGroupIn layout: WorkspaceLayout,
        rng: inout SeededRandom
    ) -> WorkspaceLayoutCommand {
        guard let tab = layout.allTabs.randomElement(using: &rng),
              !layout.orderedGroupIDs.isEmpty else {
            return fallback(in: layout, rng: &rng)
        }
        let destination = layout.orderedGroupIDs[rng.nextInt(layout.orderedGroupIDs.count)]
        let destinationCount = layout.group(destination)!.tabs.count
        let source = layout.groupContaining(tab: tab.id)!
        let index: Int
        if source == destination {
            index = rng.nextInt(destinationCount)
        } else {
            index = rng.nextInt(destinationCount + 1)
        }
        return .moveTab(tab.id, to: .group(destination, index: index))
    }

    private static func moveTab(
        toNewSplitIn layout: WorkspaceLayout,
        rng: inout SeededRandom
    ) -> WorkspaceLayoutCommand {
        let candidates = layout.orderedGroupIDs.filter {
            (layout.group($0)?.tabs.count ?? 0) > 1
        }
        guard !candidates.isEmpty else {
            return fallback(in: layout, rng: &rng)
        }
        let anchor = candidates[rng.nextInt(candidates.count)]
        let group = layout.group(anchor)!
        let tab = group.tabs[rng.nextInt(group.tabs.count)]
        return .moveTab(
            tab.id,
            to: .newSplit(
                anchor: anchor,
                placement: SplitPlacementSide.allCases[rng.nextInt(SplitPlacementSide.allCases.count)],
                newGroup: PaneGroupID(),
                newSplit: SplitID()))
    }
}

private extension Array {
    func randomElement(using rng: inout SeededRandom) -> Element? {
        guard !isEmpty else { return nil }
        return self[rng.nextInt(count)]
    }
}
