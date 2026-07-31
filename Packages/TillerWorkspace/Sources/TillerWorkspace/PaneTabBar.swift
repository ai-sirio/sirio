import SwiftUI
import TillerCore

/// The tab chrome for one pane group. Every group draws its own, because a
/// single window-wide bar cannot represent a layout where each pane holds a
/// different set of tabs.
public struct PaneTabBar: View {
    let entries: [TabMenuEntry]
    let onActivate: (WorkspaceTabID) -> Void
    let onClose: (WorkspaceTabID) -> Void
    let onNewTab: () -> Void

    public var body: some View {
        HStack(spacing: 0) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 2) {
                    ForEach(entries, id: \.tabID) { entry in
                        PaneTabItem(
                            entry: entry,
                            onActivate: { onActivate(entry.tabID) },
                            onClose: { onClose(entry.tabID) })
                    }
                }
                .padding(.horizontal, 6)
            }
            Button(action: onNewTab) {
                Image(systemName: "plus")
                    .font(.system(size: 11, weight: .medium))
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .padding(.trailing, 6)
            .help("New Tab")
            .accessibilityLabel("New Tab")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(alignment: .bottom) {
            Rectangle()
                .fill(Color(nsColor: .separatorColor))
                .frame(height: WorkspaceMetrics.dividerHairlineThickness)
        }
    }
}

private struct PaneTabItem: View {
    let entry: TabMenuEntry
    let onActivate: () -> Void
    let onClose: () -> Void

    @State private var isHovering = false

    var body: some View {
        HStack(spacing: 4) {
            Text(entry.title)
                .font(.system(size: 11))
                .lineLimit(1)
                .truncationMode(.tail)
            // The close button only appears under the pointer or on the active
            // tab: showing one per tab turns a busy strip into a minefield.
            if isHovering || entry.isActive {
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 8, weight: .bold))
                        .frame(width: 12, height: 12)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Close \(entry.title)")
            } else {
                Color.clear.frame(width: 12, height: 12)
            }
        }
        .padding(.horizontal, 8)
        .frame(height: WorkspaceMetrics.tabStripHeight - 6)
        .frame(minWidth: 80, maxWidth: 200)
        .background(
            RoundedRectangle(cornerRadius: 5)
                .fill(entry.isActive
                    ? Color(nsColor: .controlAccentColor).opacity(0.18)
                    : isHovering ? Color(nsColor: .separatorColor).opacity(0.35) : .clear))
        .contentShape(RoundedRectangle(cornerRadius: 5))
        .onTapGesture(perform: onActivate)
        .onHover { isHovering = $0 }
        .help(entry.title)
    }
}
