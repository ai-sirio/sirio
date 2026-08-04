import AppKit
import SwiftUI
import TillerCore
import TillerWorkspace

/// The tab chrome for one pane group, drawn in the app's own theme. It lives
/// here rather than in TillerWorkspace because AppTheme and
/// MainSurfaceMaterial belong to the app target; the package supplies only the
/// model and calls back for the view.
struct PaneTabStripBar: View {
    @Bindable var model: PaneTabStripModel

    /// The space tab frames are measured in. The drag coordinator offsets them
    /// into workspace-root space, so they must all share one origin.
    static let stripSpace = "paneTabStrip"

    @State private var escapeMonitor: Any?

    var body: some View {
        HStack(spacing: 4) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 2) {
                    ForEach(model.entries, id: \.tabID) { entry in
                        PaneTabStripItem(
                            entry: entry,
                            onClose: { model.onClose(entry.tabID) },
                            onFrameChange: { model.setTabFrame($0, for: entry.tabID) },
                            onDragChanged: { model.onDragChanged(entry.tabID, $0) },
                            onDragEnded: { model.onDragEnded(entry.tabID) })
                    }
                }
                .padding(.leading, 6)
            }

            Button(action: model.onNewTab) {
                Image(systemName: "plus")
                    .font(.system(size: 11))
                    .foregroundStyle(AppTheme.meta)
            }
            .buttonStyle(.plain)
            .help("New tab (⌘T)")
            .accessibilityLabel("New tab")
            .padding(.trailing, 8)
        }
        .frame(height: 32)
        .coordinateSpace(name: Self.stripSpace)
        .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
        .onChange(of: model.entries.map(\.tabID)) { _, ids in
            model.removeTabFrames(notIn: Set(ids))
        }
        .onAppear {
            escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                // 53 is Escape. The event is passed through either way: with no
                // drag in flight the cancel is a no-op, and swallowing the key
                // would break every other Escape handler in the window.
                if event.keyCode == 53 { model.onDragCancelled() }
                return event
            }
        }
        .onDisappear {
            if let escapeMonitor { NSEvent.removeMonitor(escapeMonitor) }
            escapeMonitor = nil
        }
    }
}

/// Mirrors TabBarItem's shape so a pane's tabs read as the same control the
/// legacy bar used, minus the pieces the universal engine does not carry yet
/// (rename, dirty dot, agent status).
private struct PaneTabStripItem: View {
    let entry: TabMenuEntry
    let onClose: () -> Void
    let onFrameChange: (CGRect) -> Void
    let onDragChanged: (CGPoint) -> Void
    let onDragEnded: () -> Void

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 6) {
            Text(entry.title)
                .font(.system(size: 12))
                .foregroundStyle(entry.isActive ? AppTheme.titleSelected : AppTheme.subtitle)
                .lineLimit(1)
                .truncationMode(.tail)

            if hovering {
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(HoverIconButtonStyle())
                .help("Close tab (⌘W)")
            } else {
                // Placeholder for the ×, so the tab does not resize on hover.
                Color.clear.frame(width: 16, height: 16)
            }
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
        .contentShape(Rectangle())
        .background {
            RoundedRectangle(cornerRadius: 7)
                .fill(AppTheme.background)
                .overlay {
                    if entry.isActive {
                        RoundedRectangle(cornerRadius: 7)
                            .fill(AppTheme.selectionFill)
                            .overlay(
                                RoundedRectangle(cornerRadius: 7)
                                    .stroke(AppTheme.selectionRing, lineWidth: 1)
                            )
                    } else if hovering {
                        RoundedRectangle(cornerRadius: 7).fill(AppTheme.rowHover)
                    }
                }
                // Clipped after the overlay so the straight line is trimmed to
                // the chip's rounded corners instead of poking out of them.
                .overlay(alignment: .bottom) {
                    Rectangle()
                        .fill(AppTheme.tabChipUnderline)
                        .frame(height: 1.5)
                }
                .clipShape(RoundedRectangle(cornerRadius: 7))
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .named(PaneTabStripBar.stripSpace))
        } action: { frame in
            onFrameChange(frame)
        }
        .onHover { hovering = $0 }
        // One gesture, not a tap plus a drag: a simultaneous DragGesture eats
        // the taps, and DragSession already reports a press that never crossed
        // the threshold as a plain activation.
        //
        // NSEvent.mouseLocation, not the gesture's own location: a SwiftUI drag
        // reports points in this tab's local space, which cannot address the
        // pane next door.
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { _ in onDragChanged(NSEvent.mouseLocation) }
                .onEnded { _ in onDragEnded() }
        )
        .help(entry.title)
    }
}

@MainActor
func makePaneTabStrip(_ model: PaneTabStripModel) -> NSView {
    NSHostingView(rootView: PaneTabStripBar(model: model))
}
