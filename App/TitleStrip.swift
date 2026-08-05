import SwiftUI

/// The canvas band at the top of the window. AppKit still draws the traffic
/// lights over its leading edge under `.hiddenTitleBar`, so the strip reserves
/// room for them and puts the chrome buttons at the trailing end.
struct TitleStrip<Trailing: View>: View {
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 2) {
            Color.clear.frame(width: AppTheme.trafficLightInset, height: 1)
            Spacer(minLength: 0)
            trailing
        }
        .padding(.trailing, AppTheme.cardGap)
        .frame(height: AppTheme.titleStripHeight)
    }
}
