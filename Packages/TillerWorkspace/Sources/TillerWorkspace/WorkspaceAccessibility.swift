import AppKit
import TillerCore

public enum WorkspaceTabAccessibilityAction: CaseIterable, Equatable, Sendable {
    case activate
    case moveEarlier
    case moveLater
    case moveTabTo
    case close

    public var title: String { WorkspaceAnnouncements.actionTitle(self) }
}

public enum WorkspacePaneAccessibilityAction: CaseIterable, Equatable, Sendable {
    case focusPane
    case splitRightWith
    case splitDownWith

    public var title: String { WorkspaceAnnouncements.actionTitle(self) }
}

public enum WorkspaceDividerAccessibilityAction: CaseIterable, Equatable, Sendable {
    case increment
    case decrement

    public var title: String { WorkspaceAnnouncements.actionTitle(self) }
}

public struct WorkspacePaneAccessibility: Equatable, Sendable {
    public let id: PaneGroupID
    public let position: Int
    public let total: Int
    public let activeTabTitle: String

    public init(id: PaneGroupID, position: Int, total: Int, activeTabTitle: String) {
        self.id = id
        self.position = position
        self.total = total
        self.activeTabTitle = activeTabTitle
    }

    public var label: String {
        WorkspaceAnnouncements.paneLabel(
            position: position, total: total, activeTabTitle: activeTabTitle
        )
    }

    public var actions: [WorkspacePaneAccessibilityAction] {
        WorkspaceAccessibility.paneActions
    }
}

public struct WorkspacePaneGroupsRotor: Equatable, Sendable {
    public let name: String
    public let entries: [WorkspacePaneAccessibility]

    public init(entries: [WorkspacePaneAccessibility]) {
        name = WorkspaceAnnouncements.paneGroupsRotorTitle
        self.entries = entries
    }
}

public enum WorkspaceDividerOrientation: String, Equatable, Sendable {
    case vertical
    case horizontal

    var title: String { rawValue.capitalized }
}

public struct WorkspaceDividerAccessibility: Equatable, Sendable {
    public let orientation: WorkspaceDividerOrientation
    public let firstPaneLabel: String
    public let secondPaneLabel: String
    public let percentage: Int

    public init(
        orientation: WorkspaceDividerOrientation,
        firstPaneLabel: String,
        secondPaneLabel: String,
        percentage: Int
    ) {
        self.orientation = orientation
        self.firstPaneLabel = firstPaneLabel
        self.secondPaneLabel = secondPaneLabel
        self.percentage = percentage
    }

    public var label: String {
        WorkspaceAnnouncements.dividerLabel(
            orientation: orientation,
            firstPaneLabel: firstPaneLabel,
            secondPaneLabel: secondPaneLabel,
            percentage: percentage
        )
    }

    public var actions: [WorkspaceDividerAccessibilityAction] {
        WorkspaceAccessibility.dividerActions
    }
}

public enum WorkspaceTopologyTransition: Equatable, Sendable {
    case animated
    case instant
}

public enum WorkspaceHighlight: Equatable, Sendable {
    case animated
    case `static`
}

public enum WorkspaceBoundaryStyle: Equatable, Sendable {
    case normal
    case strong
}

public struct WorkspaceAccessibilityPresentation: Equatable, Sendable {
    public let topologyTransition: WorkspaceTopologyTransition
    public let highlight: WorkspaceHighlight
    public let boundaryStyle: WorkspaceBoundaryStyle
    public let colorOnlyMeaning: Bool
    public let announcement: String
    public let tabActions: [WorkspaceTabAccessibilityAction]
    public let paneActions: [WorkspacePaneAccessibilityAction]

    public init(
        reduceMotion: Bool = false,
        increaseContrast: Bool = false,
        reduceTransparency: Bool = false
    ) {
        topologyTransition = reduceMotion ? .instant : .animated
        highlight = reduceMotion ? .static : .animated
        boundaryStyle = increaseContrast || reduceTransparency ? .strong : .normal
        colorOnlyMeaning = false
        announcement = WorkspaceAnnouncements.layoutUpdated
        tabActions = WorkspaceAccessibility.tabActions
        paneActions = WorkspaceAccessibility.paneActions
    }
}

public enum WorkspaceAccessibility {
    public static let tabActions = WorkspaceTabAccessibilityAction.allCases
    public static let paneActions = WorkspacePaneAccessibilityAction.allCases
    public static let dividerActions = WorkspaceDividerAccessibilityAction.allCases

    public static func paneGroupsRotor(
        panes: [WorkspacePaneAccessibility]
    ) -> WorkspacePaneGroupsRotor {
        WorkspacePaneGroupsRotor(entries: panes.sorted { lhs, rhs in
            if lhs.position != rhs.position { return lhs.position < rhs.position }
            return String(describing: lhs.id) < String(describing: rhs.id)
        })
    }

    public static func divider(
        axis: WorkspaceSplitAxis,
        firstPaneLabel: String,
        secondPaneLabel: String,
        fraction: Double
    ) -> WorkspaceDividerAccessibility {
        let orientation: WorkspaceDividerOrientation = axis == .horizontal ? .vertical : .horizontal
        let percentage = Int((min(max(fraction, 0), 1) * 100).rounded())
        return WorkspaceDividerAccessibility(
            orientation: orientation,
            firstPaneLabel: firstPaneLabel,
            secondPaneLabel: secondPaneLabel,
            percentage: percentage
        )
    }
}
