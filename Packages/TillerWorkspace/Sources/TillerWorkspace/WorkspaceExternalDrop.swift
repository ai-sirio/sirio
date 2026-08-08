import AppKit

/// The only pasteboard type accepted by the workspace root for a diff drop.
/// Keeping this separate from `public.file-url` prevents terminal panes from
/// interpreting a Changes-row drag as a request to paste a path.
public enum WorkspaceExternalDrop {
    public static let diffPasteboardType = NSPasteboard.PasteboardType(
        "it.tiller.diff-drag")
}
