import AppKit
import TillerCore
import TillerWorkspace

@MainActor
final class FakeContentHost: WorkspaceContentHost {
    let tabID: WorkspaceTabID
    let viewController: NSViewController
    var isVisible: Bool
    var recordedFocusIntents: [FocusIntent] = []
    var fulfillResult: Bool

    init(
        tabID: WorkspaceTabID,
        viewController: NSViewController = NSViewController(),
        isVisible: Bool = true,
        fulfillResult: Bool = true
    ) {
        self.tabID = tabID
        self.viewController = viewController
        self.isVisible = isVisible
        self.fulfillResult = fulfillResult
    }

    func setVisible(_ isVisible: Bool) {
        self.isVisible = isVisible
    }

    @discardableResult
    func fulfill(_ intent: FocusIntent) -> Bool {
        recordedFocusIntents.append(intent)
        return fulfillResult
    }
}
