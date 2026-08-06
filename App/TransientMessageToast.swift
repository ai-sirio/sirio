import SwiftUI
import Inject

/// Bottom-right transient message. Mirrors `UpdateToastView`'s chrome so the
/// two never look like different systems, without inheriting its
/// updater-specific state machine.
struct TransientMessageToast: View {
    @ObserveInjection private var inject

    let message: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.triangle")
                .foregroundStyle(AppTheme.title)
            Text(message)
                .lineLimit(2)
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(AppTheme.hairline))
    .enableInjection()
    }
}
