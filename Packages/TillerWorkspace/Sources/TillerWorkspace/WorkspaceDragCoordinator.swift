import AppKit
import TillerCore

/// Drives one tab drag: it owns the session, keeps the overlay in step with the
/// resolved target, and forwards the single resulting intent. Every outside
/// dependency arrives as a closure so the whole gesture is testable without a
/// window or a live view tree.
@MainActor
public final class WorkspaceDragCoordinator {
    private let session = DragSession()
    private weak var sink: (any WorkspaceIntentSink)?
    private let frames: @MainActor () -> [PaneGroupHitFrame]
    private let overlay: @MainActor () -> WorkspaceDragOverlay?
    private let convertToRoot: @MainActor (CGPoint) -> CGPoint

    private var tab: WorkspaceTabID?
    private var sourceGroup: PaneGroupID?
    private var tabSize = CGSize.zero
    private var title = ""

    /// `convertToRoot` maps a screen point into workspace-root flipped
    /// coordinates. It defaults to the identity so tests can speak root
    /// coordinates directly.
    public init(
        sink: (any WorkspaceIntentSink)?,
        frames: @escaping @MainActor () -> [PaneGroupHitFrame],
        overlay: @escaping @MainActor () -> WorkspaceDragOverlay?,
        convertToRoot: @escaping @MainActor (CGPoint) -> CGPoint = { $0 }
    ) {
        self.sink = sink
        self.frames = frames
        self.overlay = overlay
        self.convertToRoot = convertToRoot
    }

    public var isDragging: Bool { session.isActive }

    /// True from the press until the release, whether or not the pointer has
    /// crossed the drag threshold yet. Callers use this — not `isDragging` — to
    /// decide whether a press has already begun: re-pressing on every pointer
    /// event would move the origin the threshold is measured from, and the drag
    /// would never start.
    public var isPressing: Bool { tab != nil }

    public func pressBegan(
        tab: WorkspaceTabID,
        in group: PaneGroupID,
        atScreenPoint screenPoint: CGPoint,
        tabFrame: CGRect,
        title: String
    ) {
        self.tab = tab
        sourceGroup = group
        tabSize = tabFrame.size
        self.title = title
        session.pressBegan(tab: tab, at: convertToRoot(screenPoint), inTabFrame: tabFrame)
    }

    public func pointerMoved(toScreenPoint screenPoint: CGPoint) {
        guard let tab, let sourceGroup else { return }
        let point = convertToRoot(screenPoint)
        let snapshot = frames()

        session.pointerMoved(to: point) { candidate in
            WorkspaceHitTester.target(
                at: candidate, in: snapshot, draggedTab: tab, sourceGroup: sourceGroup
            )
        }

        guard session.isActive, let overlay = overlay() else { return }
        overlay.previewRect = DragPreviewGeometry.previewRect(
            for: session.currentTarget, in: snapshot
        )
        overlay.ghostRect = CGRect(
            origin: CGPoint(
                x: point.x - session.grabOffset.x,
                y: point.y - session.grabOffset.y
            ),
            size: tabSize
        )
        overlay.ghostTitle = title
    }

    public func released() {
        defer { finish() }
        guard let intent = session.release() else { return }
        sink?.send(intent)
    }

    public func cancel(reason: DragSession.CancelReason) {
        session.cancel(reason: reason)
        overlay()?.clear()
    }

    private func finish() {
        overlay()?.clear()
        tab = nil
        sourceGroup = nil
        tabSize = .zero
        title = ""
    }
}
