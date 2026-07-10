import SwiftUI
import TillerCore

/// Full-page in-window settings: back header + category sidebar + detail pane.
struct SettingsSurface: View {
    var model: AppModel
    var updater: UpdaterModel
    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(AppTheme.hairline)
            HStack(spacing: 0) {
                categorySidebar
                Divider().overlay(AppTheme.hairline)
                detailPane
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .background(AppTheme.background)
    }

    private var header: some View {
        HStack(spacing: 10) {
            Button {
                model.closeSettings()
            } label: {
                Label("Back", systemImage: "chevron.left")
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.escape, modifiers: [])
            Text("Settings")
                .font(.headline)
                .foregroundStyle(AppTheme.title)
            Spacer()
        }
        .padding(12)
        .background(AppTheme.background)
    }

    private var categorySidebar: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(SettingsCategory.allCases) { category in
                Button {
                    model.settingsCategory = category
                } label: {
                    Label(category.title, systemImage: category.symbol)
                        .foregroundStyle(model.settingsCategory == category ? AppTheme.title : AppTheme.subtitle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 6)
                        .padding(.horizontal, 10)
                        .background(model.settingsCategory == category ? AppTheme.selectionFill : Color.clear)
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                }
                .buttonStyle(.plain)
            }
            Spacer()
        }
        .padding(8)
        .frame(width: 200)
        .background(AppTheme.background)
    }

    @ViewBuilder
    private var detailPane: some View {
        switch model.settingsCategory {
        case .aiProviders: AIProvidersSettingsView(store: model.usage, accounts: model.agentAccounts)
        case .general: GeneralSettingsView(updater: updater)
        case .appearance: AppearanceSettingsView()
        case .permissions: PermissionsSettingsView()
        }
    }
}
