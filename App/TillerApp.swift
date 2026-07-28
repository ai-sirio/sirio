import SwiftUI
import TillerCore

@main
struct TillerApp: App {
    @State private var model = AppModel()
    @State private var updater = UpdaterModel()
    @AppStorage("sidebar.visible") private var sidebarVisible = true
    @AppStorage(AppSettings.rightPanelVisibleKey)
    private var rightPanelVisible = AppSettings.defaultRightPanelVisible
    @AppStorage(AppSettings.appearanceThemeKey) private var appearanceRaw = AppAppearance.system.rawValue
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    init() {
        // Why: CodexUsageFetcher writes to a subprocess pipe that can close
        // early (e.g. an older `codex` CLI without `app-server` support);
        // ignoring SIGPIPE once here, rather than per-fetch, keeps that
        // write from ever being able to terminate the whole app.
        signal(SIGPIPE, SIG_IGN)
        UserDefaults.standard.register(defaults: [
            "usage.claude.showInBar": true,
            "usage.codex.showInBar": true,
            "usage.opencodeGo.showInBar": false,
            "usage.ollamaCloud.showInBar": false,
            "usage.refreshIntervalSeconds": 300,
        ])
    }

    var body: some Scene {
        WindowGroup(id: "main") {
            ContentView(model: model, updater: updater)
                .onAppear {
                    appDelegate.model = model
                    updater.start()
                    applyAppearance()
                }
                .onChange(of: appearanceRaw) { _, _ in applyAppearance() }
        }
        .commands {
            // Nel menu File PRIMA di Close: performKeyEquivalent trova
            // "Chiudi tab" (⌘W) prima del Close di sistema, quindi ⌘W
            // chiude la tab, non la finestra.
            CommandGroup(after: .newItem) {
                Button("New Tab") { model.newShellTabInSelected() }
                    .keyboardShortcut("t", modifiers: .command)
                Button("Close Tab") { model.closeActiveTab() }
                    .keyboardShortcut("w", modifiers: .command)
                Button("Save") { model.saveActiveDocument() }
                    .keyboardShortcut("s", modifiers: .command)
                Button("Open File…") { model.openMarkdownFilePanel() }
                    .keyboardShortcut("o", modifiers: .command)
            }
            CommandMenu("History") {
                Button("Restore Previous Launch") {
                    _ = model.restoreLaunchSnapshot()
                }
                .keyboardShortcut("o", modifiers: [.command, .shift])
            }
            CommandMenu("Tab") {
                    Button("Next Tab") { model.cycleTab(forward: true) }
                    .keyboardShortcut(.tab, modifiers: .control)
                    Button("Previous Tab") { model.cycleTab(forward: false) }
                    .keyboardShortcut(.tab, modifiers: [.control, .shift])
                Divider()
                ForEach(1...9, id: \.self) { number in
                    Button(number == 9 ? "Last Tab" : "Tab \(number)") {
                        model.selectTab(number: number)
                    }
                    .keyboardShortcut(
                        KeyEquivalent(Character("\(number)")), modifiers: .command
                    )
                }
            }
            CommandGroup(replacing: .appSettings) {
                Button("Settings…") { model.openSettings() }
                    .keyboardShortcut(",", modifiers: .command)
            }
            CommandGroup(after: .sidebar) {
                Button(sidebarVisible ? "Hide Sidebar" : "Show Sidebar") {
                    sidebarVisible.toggle()
                }
                .keyboardShortcut("s", modifiers: [.control, .command])
                Button(rightPanelVisible
                       ? "Hide Right Panel"
                       : "Show Right Panel") {
                    rightPanelVisible.toggle()
                }
                .keyboardShortcut("i", modifiers: [.control, .command])
            }
        }

        MenuBarExtra {
            AgentRosterView(model: model)
        } label: {
            MenuBarStatusIcon(status: model.menuBarAggregateStatus)
        }
        .menuBarExtraStyle(.window)
    }

    private func applyAppearance() {
        let appearance = AppAppearance(rawValue: appearanceRaw) ?? .system
        NSApp.appearance = appearance.nsAppearance
    }
}
