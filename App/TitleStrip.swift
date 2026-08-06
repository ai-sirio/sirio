import SwiftUI

struct TitleStripGroup<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: TitlebarGeometry.controlSpacing) {
            content
                .frame(width: TitlebarGeometry.controlFrame.width,
                       height: TitlebarGeometry.controlFrame.height)
        }
        .frame(height: TitlebarGeometry.accessoryHeight, alignment: .center)
    }
}
