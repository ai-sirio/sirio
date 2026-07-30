import AppKit

@MainActor
public final class PaneTabStripView: NSView {
    public let dragSession: DragSession

    public override init(frame frameRect: NSRect) {
        dragSession = DragSession()
        super.init(frame: frameRect)
    }

    public convenience init() {
        self.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
