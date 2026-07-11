import SwiftUI
import TillerCore

/// Appearance settings: app theme (system/light/dark) and terminal font
/// size. Writes preferences only — TillerApp applies the theme, terminal
/// panes pick up the font size live.
struct AppearanceSettingsView: View {
    @AppStorage(AppSettings.appearanceThemeKey) private var appearanceRaw = AppAppearance.system.rawValue
    @AppStorage(AppSettings.terminalFontSizeKey) private var terminalFontSize = AppSettings.defaultTerminalFontSize

    var body: some View {
        Form {
            Section("Theme") {
                Picker("Appearance", selection: $appearanceRaw) {
                    ForEach(AppAppearance.allCases, id: \.rawValue) { appearance in
                        Text(appearance.title).tag(appearance.rawValue)
                    }
                }
                .pickerStyle(.segmented)
            }
            Section("Terminal") {
                Stepper(value: $terminalFontSize, in: AppSettings.terminalFontSizeRange) {
                    Text("Font size")
                    Text("\(terminalFontSize) pt")
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    }
}
