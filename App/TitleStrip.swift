import SwiftUI
import Inject

struct TitleStripGroup<Content: View>: View {
    @ObserveInjection private var inject

    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: TitlebarGeometry.controlSpacing) {
            content
        }
        .frame(height: TitlebarGeometry.accessoryHeight, alignment: .center)
        .offset(y: TitlebarGeometry.sidebarVerticalCorrection)
    .enableInjection()
    }
}

struct TitlebarControlFrame<Content: View>: View {
    @ObserveInjection private var inject

    @ViewBuilder var content: Content

    var body: some View {
        content
            .frame(width: TitlebarGeometry.controlFrame.width,
                   height: TitlebarGeometry.controlFrame.height)
    .enableInjection()
    }
}
