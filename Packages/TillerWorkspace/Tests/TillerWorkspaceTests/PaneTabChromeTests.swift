import AppKit
import Testing
import TillerCore

@testable import TillerWorkspace

/// A pane group renders its own tabs. The universal renderer mounted only the
/// active content host, so a worktree with several tabs showed no way to reach
/// any but one of them — the tab chrome these pin was never built.
@Suite("PaneTabChromeTests")
@MainActor
struct PaneTabChromeTests {
    private func tab(_ title: String) -> WorkspaceTab {
        WorkspaceTab(
            id: WorkspaceTabID(), title: title, titleIsAutoNamed: false,
            content: .terminal(TerminalContentID()))
    }

    private func group(_ tabs: [WorkspaceTab], active: WorkspaceTab?) -> PaneGroup {
        PaneGroup(id: PaneGroupID(), tabs: tabs, activeTabID: active?.id)
    }

    @Test func aGroupPublishesOneTabEntryPerTabMarkingTheActiveOne() {
        let first = tab("one")
        let second = tab("two")
        let sink = RecordingTabIntentSink()
        let controller = PaneGroupController(id: PaneGroupID(), intentSink: sink)

        controller.update(
            group: group([first, second], active: second),
            isFocused: false,
            hostProvider: EmptyHostProvider())

        #expect(controller.tabEntries.map(\.title) == ["one", "two"])
        #expect(controller.tabEntries.map(\.isActive) == [false, true])
    }

    @Test func activatingATabAsksTheWorkspaceToActivateIt() {
        let only = tab("one")
        let sink = RecordingTabIntentSink()
        let controller = PaneGroupController(id: PaneGroupID(), intentSink: sink)
        controller.update(
            group: group([only], active: only),
            isFocused: false,
            hostProvider: EmptyHostProvider())

        controller.activateTab(only.id)

        #expect(sink.intents == [.activateTab(only.id)])
    }

    @Test func closingATabAsksTheWorkspaceToCloseIt() {
        let only = tab("one")
        let sink = RecordingTabIntentSink()
        let controller = PaneGroupController(id: PaneGroupID(), intentSink: sink)
        controller.update(
            group: group([only], active: only),
            isFocused: false,
            hostProvider: EmptyHostProvider())

        controller.closeTab(only.id)

        #expect(sink.intents == [.requestClose(only.id)])
    }

    @Test func entriesCarryTheWorkspaceContentReference() throws {
        let only = tab("one")
        let controller = PaneGroupController(id: PaneGroupID())

        controller.update(
            group: group([only], active: only),
            isFocused: true,
            hostProvider: EmptyHostProvider())

        let entry = try #require(controller.tabEntries.first)
        #expect(entry.content == only.content)
        #expect(controller.strip.isFocusedGroup)
    }

    @Test func theReconcilerMarksOnlyTheActiveGroupAsFocused() throws {
        let first = tab("one")
        let second = tab("two")
        let firstGroup = group([first], active: first)
        let secondGroup = group([second], active: second)
        let split = SplitID()
        let layout = try #require(try WorkspaceLayout.make(
            root: .split(
                id: split, axis: .horizontal, fraction: 0.5,
                first: .group(firstGroup.id), second: .group(secondGroup.id)),
            groups: [firstGroup.id: firstGroup, secondGroup.id: secondGroup],
            activeGroupID: secondGroup.id
        ).get())
        let reconciler = WorkspaceReconciler(hostProvider: EmptyHostProvider())

        reconciler.reconcile(to: layout, delta: nil)

        #expect(reconciler.groupController(firstGroup.id)?.strip.isFocusedGroup == false)
        #expect(reconciler.groupController(secondGroup.id)?.strip.isFocusedGroup == true)
    }

    /// The new-tab intent must name *this* group, or a click in one pane opens
    /// a tab in another.
    @Test func theNewTabButtonNamesTheGroupThatOwnsIt() {
        let groupID = PaneGroupID()
        let sink = RecordingTabIntentSink()
        let controller = PaneGroupController(id: groupID, intentSink: sink)

        controller.requestNewTab()

        #expect(sink.intents == [.requestNewTab(into: groupID)])
    }

    /// The "+" opens a menu whose agent and chat entries go through AppModel,
    /// which opens into the *active* group. Without this the menu of a pane
    /// the user just clicked would silently open its tab in another pane.
    @Test func theStripCanActivateTheGroupItBelongsTo() {
        let groupID = PaneGroupID()
        let sink = RecordingTabIntentSink()
        let controller = PaneGroupController(id: groupID, intentSink: sink)

        controller.strip.onActivateGroup()

        #expect(sink.intents == [.activateGroup(groupID)])
    }

    /// The reconciler builds group controllers itself, so the sink has to reach
    /// them through it — otherwise the chrome renders but no click does
    /// anything, which reads as a dead UI rather than a missing wire.
    @Test func theReconcilerGivesEveryGroupItsIntentSink() {
        let sink = RecordingTabIntentSink()
        let only = tab("one")
        let paneGroup = group([only], active: only)
        guard case .success(let layout) = WorkspaceLayout.make(
            root: .group(paneGroup.id), groups: [paneGroup.id: paneGroup],
            activeGroupID: paneGroup.id
        ) else {
            Issue.record("could not build the layout")
            return
        }
        let reconciler = WorkspaceReconciler(
            hostProvider: EmptyHostProvider(), intentSink: sink)

        reconciler.reconcile(to: layout, delta: nil)
        reconciler.groupController(paneGroup.id)?.activateTab(only.id)

        #expect(sink.intents == [.activateTab(only.id)])
    }
}

@MainActor
private final class RecordingTabIntentSink: WorkspaceIntentSink {
    var intents: [WorkspaceIntent] = []
    func send(_ intent: WorkspaceIntent) { intents.append(intent) }
}

@MainActor
private final class EmptyHostProvider: WorkspaceHostProvider {
    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? { nil }
}
