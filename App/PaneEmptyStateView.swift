import AppKit
import SwiftUI
import TillerWorkspace
import Inject

struct PaneEmptyStateView<NewTabMenu: View>: View {
    @ObserveInjection private var inject

    @Bindable var stripModel: PaneTabStripModel
    @ViewBuilder var newTabMenu: () -> NewTabMenu

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "rectangle.on.rectangle.slash")
                .font(.system(size: 24, weight: .light))
                .foregroundStyle(AppTheme.meta)
            Text("Empty Pane")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(AppTheme.title)
            Text("Open a terminal, agent, or chat in this pane.")
                .font(.system(size: 12))
                .foregroundStyle(AppTheme.subtitle)
            HStack(spacing: 8) {
                Button("New Terminal") {
                    stripModel.onActivateGroup()
                    stripModel.onNewTab()
                }
                Menu("New…") { newTabMenu() }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(AppTheme.terminalSurface)
        .accessibilityElement(children: .contain)
    .enableInjection()
    }
}

@MainActor
func makePaneEmptyState<NewTabMenu: View>(
    _ stripModel: PaneTabStripModel,
    @ViewBuilder newTabMenu: @escaping () -> NewTabMenu
) -> NSView {
    NSHostingView(rootView: PaneEmptyStateView(
        stripModel: stripModel,
        newTabMenu: newTabMenu))
}
