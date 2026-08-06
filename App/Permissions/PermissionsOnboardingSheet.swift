import SwiftUI
import Inject

/// Sheet mostrato una sola volta al primo avvio: stessa lista permessi della
/// pagina Settings più un bottone Continue. Il chiamante persiste il flag
/// `hasSeenPermissionsOnboarding`; qui nessuno stato.
struct PermissionsOnboardingSheet: View {
    @ObserveInjection private var inject

    var onContinue: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            PermissionsSettingsView()
                .frame(width: 560, height: 460)
            Divider()
            HStack {
                Text("You can change these anytime in Settings → Permissions.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Continue", action: onContinue)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(12)
        }
    .enableInjection()
    }
}
