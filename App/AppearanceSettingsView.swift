import SwiftUI
import TillerCore
import Inject

/// Appearance settings: app theme (system/light/dark) and terminal font
/// size. Writes preferences only — TillerApp applies the theme, terminal
/// panes pick up the font size live.
struct AppearanceSettingsView: View {
    @ObserveInjection private var inject

    @AppStorage(AppSettings.appearanceThemeKey) private var appearanceRaw = AppAppearance.system.rawValue
    @AppStorage(AppSettings.terminalFontSizeKey) private var terminalFontSize = AppSettings.defaultTerminalFontSize
    @AppStorage(AppSettings.fileIconThemeKey) private var fileIconThemeRaw = FileIconTheme.sfSymbols.rawValue
    @AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false

    var body: some View {
        Form {
            Section("Theme") {
                Picker("Appearance", selection: $appearanceRaw) {
                    ForEach(AppAppearance.allCases, id: \.rawValue) { appearance in
                        Text(appearance.title).tag(appearance.rawValue)
                    }
                }
                .pickerStyle(.segmented)
                Toggle("Translucency", isOn: $translucencyEnabled)
            }
            Section("Terminal") {
                Stepper(value: $terminalFontSize, in: AppSettings.terminalFontSizeRange) {
                    Text("Font size")
                    Text("\(terminalFontSize) pt")
                }
            }
            Section("Files") {
                Picker("File icons", selection: $fileIconThemeRaw) {
                    ForEach(FileIconTheme.allCases, id: \.rawValue) { theme in
                        Text(theme.title).tag(theme.rawValue)
                    }
                }
                .pickerStyle(.segmented)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    .enableInjection()
    }
}
