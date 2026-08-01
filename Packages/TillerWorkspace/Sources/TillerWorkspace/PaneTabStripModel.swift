import AppKit
import TillerCore

/// Supplied by the app target, which owns the theme the strip is drawn in.
public typealias PaneTabStripFactory = @MainActor (PaneTabStripModel) -> NSView

/// What a pane group's tab strip shows and what it can ask for. The strip's
/// appearance lives in the app target, which owns the theme; the package holds
/// only this model and hands it to whatever view the app supplies.
@MainActor
@Observable
public final class PaneTabStripModel {
    public internal(set) var entries: [TabMenuEntry] = []

    public var onActivate: (WorkspaceTabID) -> Void = { _ in }
    public var onClose: (WorkspaceTabID) -> Void = { _ in }
    public var onNewTab: () -> Void = {}

    public init() {}
}
