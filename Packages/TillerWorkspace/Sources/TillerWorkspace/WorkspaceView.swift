import SwiftUI
import AppKit
import TillerCore

@MainActor
public struct WorkspaceView: NSViewControllerRepresentable {
    private let layout: WorkspaceLayout
    private let delta: WorkspaceLayoutDelta?
    private let isVisible: Bool
    private let hostProvider: WorkspaceHostProvider
    private let intentSink: WorkspaceIntentSink
    private let stripFactory: PaneTabStripFactory?
    private let emptyStateFactory: PaneEmptyStateFactory?
    private let diffPathFromPasteboard: @MainActor (NSPasteboard) -> String?

    public init(
        layout: WorkspaceLayout,
        delta: WorkspaceLayoutDelta?,
        isVisible: Bool = true,
        hostProvider: WorkspaceHostProvider,
        intentSink: WorkspaceIntentSink,
        stripFactory: PaneTabStripFactory? = nil,
        emptyStateFactory: PaneEmptyStateFactory? = nil,
        diffPathFromPasteboard: @escaping @MainActor (NSPasteboard) -> String? = { _ in nil }
    ) {
        self.layout = layout
        self.delta = delta
        self.isVisible = isVisible
        self.hostProvider = hostProvider
        self.intentSink = intentSink
        self.stripFactory = stripFactory
        self.emptyStateFactory = emptyStateFactory
        self.diffPathFromPasteboard = diffPathFromPasteboard
    }

    public func makeNSViewController(context: Context) -> WorkspaceViewController {
        let controller = WorkspaceViewController(
            hostProvider: hostProvider,
            intentSink: intentSink,
            stripFactory: stripFactory,
            emptyStateFactory: emptyStateFactory,
            diffPathFromPasteboard: diffPathFromPasteboard
        )
        controller.update(layout: layout, delta: delta, isVisible: isVisible)
        return controller
    }

    public func updateNSViewController(_ controller: WorkspaceViewController, context: Context) {
        controller.update(layout: layout, delta: delta, isVisible: isVisible)
    }
}
