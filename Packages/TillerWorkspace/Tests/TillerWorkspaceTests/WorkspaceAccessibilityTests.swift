import AppKit
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct WorkspaceAccessibilityTests {
    @Test
    func paneLabelsUseVisualPositionAndActiveTabTitle() {
        let controller = PaneGroupController(id: PaneGroupID())
        controller.setAccessibilityPosition(position: 2, total: 4, activeTabTitle: "Codex")

        #expect(controller.accessibilityLabel() == "Pane 2 of 4 - Codex")
    }

    @Test
    func eachTabExposesExactlyTheFiveApprovedActions() {
        #expect(WorkspaceAccessibility.tabActions.map(\.title) == [
            "Activate", "Move Earlier", "Move Later", "Move Tab To...", "Close"
        ])
    }

    @Test
    func theRotorListsPaneGroupsInLayoutReadingOrder() {
        let first = WorkspacePaneAccessibility(
            id: PaneGroupID(), position: 1, total: 3, activeTabTitle: "Terminal"
        )
        let second = WorkspacePaneAccessibility(
            id: PaneGroupID(), position: 2, total: 3, activeTabTitle: "Codex"
        )
        let third = WorkspacePaneAccessibility(
            id: PaneGroupID(), position: 3, total: 3, activeTabTitle: "Notes"
        )

        let rotor = WorkspaceAccessibility.paneGroupsRotor(panes: [first, second, third])

        #expect(rotor.name == "Pane Groups")
        #expect(rotor.entries.map(\.id) == [first.id, second.id, third.id])
        #expect(rotor.entries.map(\.label) == [first.label, second.label, third.label])
    }

    @Test
    func dividerExposesOrientationAdjacentPanesAndPercentage() {
        let divider = WorkspaceAccessibility.divider(
            axis: .horizontal,
            firstPaneLabel: "Pane 1",
            secondPaneLabel: "Pane 2",
            fraction: 0.5
        )

        #expect(divider.orientation == .vertical)
        #expect(divider.firstPaneLabel == "Pane 1")
        #expect(divider.secondPaneLabel == "Pane 2")
        #expect(divider.percentage == 50)
        #expect(divider.label == "Vertical divider between Pane 1 and Pane 2, 50 percent")
        #expect(divider.actions.map(\.title) == ["Increment", "Decrement"])
    }

    @Test
    func aSuccessfulMoveAnnouncesDestinationAndSourceCollapse() {
        let announcer = WorkspaceAnnouncer()
        announcer.announce(.moved(
            tabTitle: "Codex", destinationPane: 3, position: 2,
            sourcePane: 1, sourceCollapsed: true
        ))

        #expect(announcer.last == "Moved Codex to Pane 3, position 2. Pane 1 closed.")
        #expect(announcer.announcementCount == 1)
    }

    @Test
    func anInvalidActionAnnouncesItsReasonOnce() {
        let announcer = WorkspaceAnnouncer()
        announcer.announce(.invalid(.destinationUnavailable))
        announcer.announce(.invalid(.destinationUnavailable))

        #expect(announcer.last == "Action unavailable: destination pane is unavailable.")
        #expect(announcer.announcementCount == 1)
    }

    @Test
    func reducedMotionCommitsInstantlyWithAStaticHighlight() {
        let presentation = WorkspaceAccessibilityPresentation(reduceMotion: true)

        #expect(presentation.topologyTransition == .instant)
        #expect(presentation.highlight == .static)
        #expect(presentation.announcement == "Layout updated.")
    }

    @Test
    func increasedContrastAndReducedTransparencyChangeBoundariesNotSemantics() {
        let normal = WorkspaceAccessibilityPresentation()
        let accessible = WorkspaceAccessibilityPresentation(
            increaseContrast: true, reduceTransparency: true
        )

        #expect(accessible.boundaryStyle == .strong)
        #expect(accessible.boundaryStyle != normal.boundaryStyle)
        #expect(accessible.colorOnlyMeaning == false)
        #expect(accessible.tabActions == normal.tabActions)
        #expect(accessible.paneActions == normal.paneActions)
    }
}
