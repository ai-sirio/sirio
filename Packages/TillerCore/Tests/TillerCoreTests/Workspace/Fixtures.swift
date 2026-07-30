import Foundation
@testable import TillerCore

enum Fixtures {
    static func singleTerminalTab() -> (layout: WorkspaceLayout, contentID: TerminalContentID) {
        let groupID = PaneGroupID()
        let contentID = TerminalContentID()
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "Terminal", titleIsAutoNamed: true,
            content: .terminal(contentID))
        let group = PaneGroup(id: groupID, tabs: [tab], activeTabID: tab.id)
        return (makeLayout(root: .group(groupID), groups: [groupID: group], activeGroupID: groupID),
                contentID)
    }

    static func twoTabsInOneGroup() -> (layout: WorkspaceLayout, tabB: WorkspaceTabID) {
        let groupID = PaneGroupID()
        let tabA = WorkspaceTab(
            id: WorkspaceTabID(), title: "A", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        let tabB = WorkspaceTab(
            id: WorkspaceTabID(), title: "B", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        let group = PaneGroup(id: groupID, tabs: [tabA, tabB], activeTabID: tabA.id)
        return (makeLayout(root: .group(groupID), groups: [groupID: group], activeGroupID: groupID),
                tabB.id)
    }

    static func freshTerminalTab() -> WorkspaceTab {
        WorkspaceTab(
            id: WorkspaceTabID(), title: "Fresh terminal", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
    }

    static func deepMixedOrientationLayout(groups groupCount: Int,
                                           tabs tabsPerGroup: Int) -> WorkspaceLayout {
        let groupCount = max(1, groupCount)
        let tabsPerGroup = max(1, tabsPerGroup)
        let groupIDs = (0..<groupCount).map { _ in PaneGroupID() }
        var groupRegistry: [PaneGroupID: PaneGroup] = [:]

        for (groupIndex, groupID) in groupIDs.enumerated() {
            let tabs = (0..<tabsPerGroup).map { tabIndex in
                WorkspaceTab(
                    id: WorkspaceTabID(),
                    title: "Group " + String(groupIndex) + " Tab " + String(tabIndex),
                    titleIsAutoNamed: true,
                    content: .terminal(TerminalContentID()))
            }
            groupRegistry[groupID] = PaneGroup(
                id: groupID, tabs: tabs, activeTabID: tabs[0].id)
        }

        func makeNode(_ range: Range<Int>, depth: Int) -> LayoutNode {
            if range.count == 1 {
                return .group(groupIDs[range.lowerBound])
            }
            let midpoint = range.lowerBound + range.count / 2
            let axis: WorkspaceSplitAxis = depth.isMultiple(of: 2) ? .horizontal : .vertical
            return .split(
                id: SplitID(), axis: axis, fraction: 0.5,
                first: makeNode(range.lowerBound..<midpoint, depth: depth + 1),
                second: makeNode(midpoint..<range.upperBound, depth: depth + 1))
        }

        return makeLayout(
            root: makeNode(0..<groupIDs.count, depth: 0), groups: groupRegistry,
            activeGroupID: groupIDs[0])
    }

    static func acceptanceEnvelope() -> WorkspaceLayout {
        let groupCount = 16
        let tabsPerGroup = 4
        let groupIDs = (0..<groupCount).map { _ in PaneGroupID() }
        var groupRegistry: [PaneGroupID: PaneGroup] = [:]

        for (groupIndex, groupID) in groupIDs.enumerated() {
            let tabs = (0..<tabsPerGroup).map { tabIndex in
                let content: WorkspaceContentRef
                switch tabIndex {
                case 0:
                    content = .terminal(TerminalContentID())
                case 1:
                    content = .chat(ChatContentID("fixture-chat-" + String(groupIndex)))
                case 2:
                    let documentID = DocumentID.make(
                        worktreeID: UUID(),
                        fileURL: URL(fileURLWithPath:
                            "/fixture/" + String(groupIndex) + "-markdown.md"))
                    content = .document(documentID, editor: .markdown)
                default:
                    let documentID = DocumentID.make(
                        worktreeID: UUID(),
                        fileURL: URL(fileURLWithPath:
                            "/fixture/" + String(groupIndex) + "-code.swift"))
                    content = .document(documentID, editor: .code)
                }
                return WorkspaceTab(
                    id: WorkspaceTabID(),
                    title: "Group " + String(groupIndex) + " Tab " + String(tabIndex),
                    titleIsAutoNamed: true, content: content)
            }
            groupRegistry[groupID] = PaneGroup(
                id: groupID, tabs: tabs, activeTabID: tabs[0].id)
        }

        func makeNode(_ range: Range<Int>, depth: Int) -> LayoutNode {
            if range.count == 1 {
                return .group(groupIDs[range.lowerBound])
            }
            let midpoint = range.lowerBound + range.count / 2
            let axis: WorkspaceSplitAxis = depth.isMultiple(of: 2) ? .horizontal : .vertical
            return .split(
                id: SplitID(), axis: axis, fraction: 0.5,
                first: makeNode(range.lowerBound..<midpoint, depth: depth + 1),
                second: makeNode(midpoint..<range.upperBound, depth: depth + 1))
        }

        return makeLayout(
            root: makeNode(0..<groupIDs.count, depth: 0), groups: groupRegistry,
            activeGroupID: groupIDs[0])
    }

    private static func makeLayout(root: LayoutNode,
                                   groups: [PaneGroupID: PaneGroup],
                                   activeGroupID: PaneGroupID) -> WorkspaceLayout {
        guard case .success(let layout) = WorkspaceLayout.make(
            root: root, groups: groups, activeGroupID: activeGroupID) else {
            preconditionFailure("fixture must construct a valid workspace layout")
        }
        return layout
    }
}
