import AppKit
import TillerCore

@MainActor
public protocol WorkspaceContentHost: AnyObject {
    var tabID: WorkspaceTabID { get }
    var viewController: NSViewController { get }
    func setVisible(_ isVisible: Bool)

    /// Returns false when focus could not be taken; the renderer retries once
    /// after attachment and reports the failure accessibly.
    @discardableResult
    func fulfill(_ intent: FocusIntent) -> Bool
}

@MainActor
public protocol WorkspaceHostProvider: AnyObject {
    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost?
}
