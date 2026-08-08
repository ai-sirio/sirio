import TillerCore

public enum WorkspaceFocusTarget: Equatable, Sendable {
    case terminalFirstResponder
    case composer
    case editor
    case webView
    case tab(WorkspaceTabID)
}

public struct WorkspaceFocusRestoration: Equatable, Sendable {
    public let activeGroupID: PaneGroupID
    public let operational: WorkspaceFocusTarget
    public let accessibility: WorkspaceFocusTarget

    public init(
        activeGroupID: PaneGroupID,
        operational: WorkspaceFocusTarget,
        accessibility: WorkspaceFocusTarget
    ) {
        self.activeGroupID = activeGroupID
        self.operational = operational
        self.accessibility = accessibility
    }
}

public struct WorkspaceVoiceOverActivation: Equatable, Sendable {
    public let activeGroupID: PaneGroupID
    public let selectedTabID: WorkspaceTabID
    public let accessibilityFocus: WorkspaceFocusTarget
}

public enum WorkspaceFocusFulfillment: Equatable, Sendable {
    case fulfilled
    case pendingRetry
    case failed
}

public enum WorkspaceKeyboardControl: Hashable, Sendable {
    case pane(PaneGroupID)
    case tab(WorkspaceTabID)
    case tabAction(WorkspaceTabID, WorkspaceTabAccessibilityAction)
    case paneAction(PaneGroupID, WorkspacePaneAccessibilityAction)
    case dividerAction(WorkspaceDividerAccessibilityAction)

    public var requiresPointer: Bool { false }
}

/// Coordinates the application focus, first-responder, accessibility-focus,
/// and VoiceOver tab-selection systems without merging their memories.
@MainActor
public final class WorkspaceFocusCoordinator {
    public private(set) var activeGroupID: PaneGroupID?
    public private(set) var operationalFocus: WorkspaceFocusTarget?
    public private(set) var accessibilityFocus: WorkspaceFocusTarget?
    public private(set) var lastFailureAnnouncement: String?

    private var operationalMemory: [PaneGroupID: WorkspaceFocusTarget] = [:]
    private var accessibilityMemory: [PaneGroupID: WorkspaceFocusTarget] = [:]
    private var pendingFocus: PendingFocus?

    public init() {}

    public func rememberOperationalFocus(
        _ target: WorkspaceFocusTarget,
        in group: PaneGroupID
    ) {
        operationalMemory[group] = target
    }

    public func rememberAccessibilityFocus(
        _ target: WorkspaceFocusTarget,
        in group: PaneGroupID
    ) {
        accessibilityMemory[group] = target
    }

    /// Commits the active group first, then restores each focus system from
    /// its own memory. A missing memory uses the content-kind default.
    @discardableResult
    public func focusPane(
        _ group: PaneGroupID,
        contentKind: WorkspaceContentKind
    ) -> WorkspaceFocusRestoration {
        activeGroupID = group
        let operational = operationalMemory[group] ?? Self.defaultTarget(for: contentKind)
        let accessibility = accessibilityMemory[group] ?? Self.defaultTarget(for: contentKind)
        operationalFocus = operational
        accessibilityFocus = accessibility
        lastFailureAnnouncement = nil
        return WorkspaceFocusRestoration(
            activeGroupID: group,
            operational: operational,
            accessibility: accessibility
        )
    }

    /// VoiceOver selection deliberately leaves accessibility focus on the tab
    /// so the user can continue traversing the local tab list.
    public func activateTabFromVoiceOver(
        _ tab: WorkspaceTabID,
        in group: PaneGroupID
    ) -> WorkspaceVoiceOverActivation {
        activeGroupID = group
        accessibilityFocus = .tab(tab)
        accessibilityMemory[group] = .tab(tab)
        return WorkspaceVoiceOverActivation(
            activeGroupID: group,
            selectedTabID: tab,
            accessibilityFocus: .tab(tab)
        )
    }

    @discardableResult
    public func requestFocus(
        _ target: WorkspaceFocusTarget,
        in group: PaneGroupID,
        fulfill: @escaping () -> Bool
    ) -> WorkspaceFocusFulfillment {
        activeGroupID = group
        lastFailureAnnouncement = nil
        guard fulfill() else {
            pendingFocus = PendingFocus(group: group, target: target, fulfill: fulfill)
            return .pendingRetry
        }
        operationalFocus = target
        operationalMemory[group] = target
        pendingFocus = nil
        return .fulfilled
    }

    /// Hosts call this after attachment. At most one retry is attempted.
    @discardableResult
    public func hostAttached() -> WorkspaceFocusFulfillment? {
        guard let pendingFocus else { return nil }
        self.pendingFocus = nil
        guard pendingFocus.fulfill() else {
            lastFailureAnnouncement = WorkspaceAnnouncements.focusFulfillmentFailure
            return .failed
        }
        operationalFocus = pendingFocus.target
        operationalMemory[pendingFocus.group] = pendingFocus.target
        return .fulfilled
    }

    public static func defaultTarget(for kind: WorkspaceContentKind) -> WorkspaceFocusTarget {
        switch kind {
        case .terminal:
            .terminalFirstResponder
        case .chat:
            .composer
        case .document:
            .editor
        case .diff:
            .editor
        case .browser:
            .webView
        }
    }

    public static func keyboardControls(
        for layout: WorkspaceLayout
    ) -> [WorkspaceKeyboardControl] {
        var controls: [WorkspaceKeyboardControl] = []
        for groupID in layout.orderedGroupIDs {
            controls.append(.pane(groupID))
            let group = layout.group(groupID)
            for tab in group?.tabs ?? [] {
                controls.append(.tab(tab.id))
                controls.append(contentsOf: WorkspaceAccessibility.tabActions.map {
                    .tabAction(tab.id, $0)
                })
            }
            controls.append(contentsOf: WorkspaceAccessibility.paneActions.map {
                .paneAction(groupID, $0)
            })
        }
        for _ in layout.splitIDs() {
            controls.append(contentsOf: WorkspaceAccessibility.dividerActions.map {
                .dividerAction($0)
            })
        }
        return controls
    }

    private struct PendingFocus {
        let group: PaneGroupID
        let target: WorkspaceFocusTarget
        let fulfill: () -> Bool
    }
}
