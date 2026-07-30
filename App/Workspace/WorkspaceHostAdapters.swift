import AppKit
import Foundation
import TillerCore
import TillerWorkspace

@MainActor
protocol WorkspaceRuntimeReleaser: AnyObject {
    func releaseRuntime() async
}

@MainActor
final class WorkspaceContentHostAdapter: WorkspaceContentHost, WorkspaceRuntimeReleaser {
    let tabID: WorkspaceTabID
    let viewController: NSViewController
    private let visibility: (Bool) -> Void
    private let focus: (FocusIntent) -> Bool
    private let releaseAction: () async -> Void
    private var didRelease = false

    init(tabID: WorkspaceTabID, viewController: NSViewController,
         visibility: @escaping (Bool) -> Void = { _ in },
         focus: @escaping (FocusIntent) -> Bool = { _ in true },
         release: @escaping () async -> Void = {}) {
        self.tabID = tabID
        self.viewController = viewController
        self.visibility = visibility
        self.focus = focus
        self.releaseAction = release
    }

    func setVisible(_ isVisible: Bool) { visibility(isVisible) }
    func fulfill(_ intent: FocusIntent) -> Bool { focus(intent) }

    func releaseRuntime() async {
        guard !didRelease else { return }
        didRelease = true
        await releaseAction()
    }
}
