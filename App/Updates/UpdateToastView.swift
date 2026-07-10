import SwiftUI
import TillerCore

/// Toast in basso a destra per il ciclo di update. Visibile solo negli stati
/// che chiedono un'azione; .checking e .upToDate vivono in Settings.
struct UpdateToastView: View {
    var updater: UpdaterModel

    var body: some View {
        switch updater.state {
        case .available(let version):
            toast(icon: "arrow.down.circle") {
                Text("Tiller \(version) disponibile")
                Button("Scarica") { updater.download() }
                    .buttonStyle(.borderedProminent)
            }
        case .downloading(let version, let progress):
            toast(icon: "arrow.down.circle") {
                Text("Download di Tiller \(version)…")
                ProgressView(value: progress)
                    .frame(width: 140)
            }
        case .readyToInstall(let version):
            toast(icon: "checkmark.circle") {
                Text("Tiller \(version) pronto")
                Button("Aggiorna e riavvia") { updater.installAndRelaunch() }
                    .buttonStyle(.borderedProminent)
            }
        case .error(let message):
            toast(icon: "exclamationmark.triangle") {
                Text(message).lineLimit(2)
            }
        case .idle, .checking, .upToDate:
            EmptyView()
        }
    }

    private func toast(icon: String, @ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 10) {
            Image(systemName: icon)
                .foregroundStyle(AppTheme.title)
            content()
            Button {
                updater.dismiss()
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.subtitle)
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(AppTheme.hairline))
    }
}
