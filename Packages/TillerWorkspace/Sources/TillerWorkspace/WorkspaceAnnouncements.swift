import Foundation

public enum WorkspaceInvalidAction: Equatable, Sendable {
    case destinationUnavailable
}

public enum WorkspaceAnnouncement: Equatable, Sendable {
    case moved(
        tabTitle: String,
        destinationPane: Int,
        position: Int,
        sourcePane: Int,
        sourceCollapsed: Bool
    )
    case invalid(WorkspaceInvalidAction)
    case layoutUpdated
    case minimumPaneSize
}

/// The single source for user-facing workspace accessibility copy.
public enum WorkspaceAnnouncements {
    public static func paneLabel(position: Int, total: Int, activeTabTitle: String) -> String {
        "Pane \(position) of \(total) - \(activeTabTitle)"
    }

    public static func actionTitle(_ action: WorkspaceTabAccessibilityAction) -> String {
        switch action {
        case .activate:
            "Activate"
        case .moveEarlier:
            "Move Earlier"
        case .moveLater:
            "Move Later"
        case .moveTabTo:
            "Move Tab To..."
        case .close:
            "Close"
        }
    }

    public static func actionTitle(_ action: WorkspacePaneAccessibilityAction) -> String {
        switch action {
        case .focusPane:
            "Focus Pane"
        case .splitRightWith:
            "Split Right With..."
        case .splitDownWith:
            "Split Down With..."
        }
    }

    public static func actionTitle(_ action: WorkspaceDividerAccessibilityAction) -> String {
        switch action {
        case .increment:
            "Increment"
        case .decrement:
            "Decrement"
        }
    }

    public static func dividerLabel(
        orientation: WorkspaceDividerOrientation,
        firstPaneLabel: String,
        secondPaneLabel: String,
        percentage: Int
    ) -> String {
        "\(orientation.title) divider between \(firstPaneLabel) and \(secondPaneLabel), \(percentage) percent"
    }

    public static var paneGroupsRotorTitle: String { "Pane Groups" }
    public static var emptyPaneTitle: String { "Empty Pane" }
    public static var tabStripLabel: String { "Tabs" }
    public static var layoutUpdated: String { "Layout updated." }
    public static var minimumPaneSize: String { "Minimum pane size" }
    public static var focusFulfillmentFailure: String { "Focus could not be fulfilled." }

    public static func text(for announcement: WorkspaceAnnouncement) -> String {
        switch announcement {
        case let .moved(tabTitle, destinationPane, position, sourcePane, sourceCollapsed):
            let collapse = sourceCollapsed ? " Pane \(sourcePane) closed." : ""
            return "Moved \(tabTitle) to Pane \(destinationPane), position \(position).\(collapse)"
        case .invalid(.destinationUnavailable):
            return "Action unavailable: destination pane is unavailable."
        case .layoutUpdated:
            return layoutUpdated
        case .minimumPaneSize:
            return minimumPaneSize
        }
    }
}

@MainActor
public final class WorkspaceAnnouncer {
    public private(set) var last: String?
    public private(set) var announcementCount = 0

    private let deliver: (String) -> Void

    public init(deliver: @escaping (String) -> Void = { _ in }) {
        self.deliver = deliver
    }

    public func announce(_ announcement: WorkspaceAnnouncement) {
        let message = WorkspaceAnnouncements.text(for: announcement)
        guard message != last else { return }
        last = message
        announcementCount += 1
        deliver(message)
    }
}
