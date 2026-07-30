import SwiftUI
import TillerCore

@MainActor
public struct WorkspaceView: NSViewControllerRepresentable {
    private let layout: WorkspaceLayout
    private let delta: WorkspaceLayoutDelta?
    private let hostProvider: WorkspaceHostProvider
    private let intentSink: WorkspaceIntentSink

    public init(
        layout: WorkspaceLayout,
        delta: WorkspaceLayoutDelta?,
        hostProvider: WorkspaceHostProvider,
        intentSink: WorkspaceIntentSink
    ) {
        self.layout = layout
        self.delta = delta
        self.hostProvider = hostProvider
        self.intentSink = intentSink
    }

    public func makeNSViewController(context: Context) -> WorkspaceViewController {
        let controller = WorkspaceViewController(
            hostProvider: hostProvider, intentSink: intentSink
        )
        controller.update(layout: layout, delta: delta)
        return controller
    }

    public func updateNSViewController(_ controller: WorkspaceViewController, context: Context) {
        controller.update(layout: layout, delta: delta)
    }
}
