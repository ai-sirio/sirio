import SwiftUI
import TillerCore

@MainActor
public struct WorkspaceView: NSViewControllerRepresentable {
    private let layout: WorkspaceLayout
    private let delta: WorkspaceLayoutDelta?
    private let hostProvider: WorkspaceHostProvider
    private let intentSink: WorkspaceIntentSink
    private let stripFactory: PaneTabStripFactory?
    private let emptyStateFactory: PaneEmptyStateFactory?

    public init(
        layout: WorkspaceLayout,
        delta: WorkspaceLayoutDelta?,
        hostProvider: WorkspaceHostProvider,
        intentSink: WorkspaceIntentSink,
        stripFactory: PaneTabStripFactory? = nil,
        emptyStateFactory: PaneEmptyStateFactory? = nil
    ) {
        self.layout = layout
        self.delta = delta
        self.hostProvider = hostProvider
        self.intentSink = intentSink
        self.stripFactory = stripFactory
        self.emptyStateFactory = emptyStateFactory
    }

    public func makeNSViewController(context: Context) -> WorkspaceViewController {
        let controller = WorkspaceViewController(
            hostProvider: hostProvider,
            intentSink: intentSink,
            stripFactory: stripFactory,
            emptyStateFactory: emptyStateFactory
        )
        controller.update(layout: layout, delta: delta)
        return controller
    }

    public func updateNSViewController(_ controller: WorkspaceViewController, context: Context) {
        controller.update(layout: layout, delta: delta)
    }
}
