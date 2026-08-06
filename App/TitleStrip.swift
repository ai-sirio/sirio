import SwiftUI

struct TitleStripGroup<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: TitlebarGeometry.controlSpacing) {
            content
        }
        .frame(height: TitlebarGeometry.accessoryHeight, alignment: .center)
    }
}

struct TitlebarControlSlot<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        content
            .frame(width: TitlebarGeometry.controlFrame.width,
                   height: TitlebarGeometry.controlFrame.height)
    }
}
