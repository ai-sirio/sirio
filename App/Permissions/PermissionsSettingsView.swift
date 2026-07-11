import SwiftUI
import AppKit
import TillerCore

/// Pagina permessi macOS in stile Orca: banner informativo + una riga per
/// permesso con badge di stato e azione singola. Riusata sia come sezione
/// Settings sia dentro lo sheet di onboarding al primo avvio.
struct PermissionsSettingsView: View {
    @State private var model = PermissionsModel(probe: SystemPermissionProbe())

    var body: some View {
        Form {
            Section {
                LabeledContent {
                    Button("Refresh") {
                        Task { await model.refresh() }
                    }
                } label: {
                    Text("Terminal tools inherit Tiller's macOS privacy envelope.")
                    Text("Use these controls when a CLI or agent in a pane needs macOS privacy access. Tiller does not ask at startup.")
                }
            }

            Section("macOS Permissions") {
                ForEach(PermissionKind.allCases) { kind in
                    row(for: kind)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .task { await model.refresh() }
        // L'utente torna da System Settings → rileggi gli stati.
        .onReceive(NotificationCenter.default.publisher(
            for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await model.refresh() }
        }
    }

    private func row(for kind: PermissionKind) -> some View {
        let status = model.status(for: kind)
        return LabeledContent {
            Button(model.action(for: kind).label) {
                Task { await model.performAction(for: kind) }
            }
        } label: {
            HStack(spacing: 8) {
                Image(systemName: kind.symbol)
                    .frame(width: 20)
                Text(kind.title)
                statusBadge(status)
            }
            Text(kind.detail)
        }
    }

    private func statusBadge(_ status: PermissionStatus) -> some View {
        Text(status.badgeLabel)
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .foregroundStyle(badgeColor(status))
            .background(badgeColor(status).opacity(0.15))
            .clipShape(Capsule())
    }

    private func badgeColor(_ status: PermissionStatus) -> Color {
        switch status {
        case .granted: .green
        case .denied: .red
        case .notRequested, .checkManually: .secondary
        }
    }
}
