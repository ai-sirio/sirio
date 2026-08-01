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

    var body: some View {
        HStack(spacing: 4) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 2) {
                    ForEach(model.entries, id: \.tabID) { entry in
                        PaneTabStripItem(
                            entry: entry,
                            onActivate: { model.onActivate(entry.tabID) },
                            onClose: { model.onClose(entry.tabID) })
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
        .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
    }
}

/// Mirrors TabBarItem's shape so a pane's tabs read as the same control the
/// legacy bar used, minus the pieces the universal engine does not carry yet
/// (rename, dirty dot, agent status).
private struct PaneTabStripItem: View {
    let entry: TabMenuEntry
    let onActivate: () -> Void
    let onClose: () -> Void

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
        }
        .onHover { hovering = $0 }
        .onTapGesture(perform: onActivate)
        .help(entry.title)
    }
}

@MainActor
func makePaneTabStrip(_ model: PaneTabStripModel) -> NSView {
    NSHostingView(rootView: PaneTabStripBar(model: model))
}
