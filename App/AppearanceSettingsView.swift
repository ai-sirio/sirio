import SwiftUI
import TillerCore
import TillerAgents
import Inject

/// Appearance settings: app theme (system/light/dark) and terminal font
/// size. Writes preferences only — TillerApp applies the theme, terminal
/// panes pick up the font size live.
struct AppearanceSettingsView: View {
    @ObserveInjection private var inject

    @AppStorage(AppSettings.appearanceThemeKey) private var appearanceRaw = AppAppearance.system.rawValue
    @AppStorage(AppSettings.terminalFontSizeKey) private var terminalFontSize = AppSettings.defaultTerminalFontSize
    @AppStorage(AppSettings.uiFontSizeKey) private var uiFontSize = AppSettings.defaultUIFontSize
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
            Section("Interface") {
                Stepper(value: $uiFontSize, in: AppSettings.uiFontSizeRange) {
                    Text("Font size")
                    Text("\(uiFontSize) pt")
                }
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
            Section("Agent Colors") {
                ForEach(AgentCatalog.all, id: \.id) { adapter in
                    AgentColorRow(agentId: adapter.id, displayName: adapter.displayName)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    .enableInjection()
    }
}

/// One Settings row per agent: icon, name, and a `ColorPicker` bound to that
/// agent's stored accent-color hex. Read-write counterpart to
/// `AgentAccentColorProvider` (read-only, used by the composer itself).
private struct AgentColorRow: View {
    let agentId: String
    let displayName: String
    @AppStorage private var hex: String

    init(agentId: String, displayName: String) {
        self.agentId = agentId
        self.displayName = displayName
        _hex = AppStorage(
            wrappedValue: AgentAccentColor.defaultHex(for: agentId),
            AppSettings.agentColorKey(for: agentId))
    }

    private var colorBinding: Binding<Color> {
        Binding(
            get: { AgentAccentColor.color(for: agentId, storedValue: hex) },
            set: { newColor in
                hex = newColor.toHex() ?? hex
            })
    }

    var body: some View {
        HStack {
            AgentIcon(agentId: agentId)
            Text(displayName)
            Spacer()
            ColorPicker("", selection: colorBinding, supportsOpacity: false)
                .labelsHidden()
        }
    }
}
