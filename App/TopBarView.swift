// Tiller/App/TopBarView.swift
import SwiftUI
import TillerCore

/// Slim bar sopra il workspace, stessa superficie trasparente di titlebar/
/// sidebar/detail (AppTheme.background, nessun bordo/divider). Solo icone
/// azione a destra — niente breadcrumb testuale, la sidebar mostra già
/// progetto/branch. Entrambe le azioni sono secondi entry point su
/// funzioni AppModel già esistenti, nessuna nuova logica di resolution.
struct TopBarView: View {
    @Bindable var model: AppModel

    var body: some View {
        HStack(spacing: 14) {
            Spacer()
            Button {
                model.splitCurrent(.horizontal)
            } label: {
                Image(systemName: "square.split.1x2")
            }
            .buttonStyle(.plain)
            .help("Split terminale")

            Button {
                model.settingsCategory = .permissions
                model.openSettings()
            } label: {
                Image(systemName: "lock.shield")
            }
            .buttonStyle(.plain)
            .help("Permessi")
        }
        .font(AppFont.system(size: 13))
        .foregroundStyle(AppTheme.meta)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(AppTheme.background)
    }
}
