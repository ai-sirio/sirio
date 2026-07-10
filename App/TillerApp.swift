import SwiftUI
import TillerCore

@main
struct TillerApp: App {
    @State private var model = AppModel()
    @State private var updater = UpdaterModel()
    @AppStorage("sidebar.visible") private var sidebarVisible = true
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
                .preferredColorScheme(.dark)
                .onAppear {
                    appDelegate.model = model
                    updater.start()
                }
        }
        .commands {
            // Nel menu File PRIMA di Close: performKeyEquivalent trova
            // "Chiudi tab" (⌘W) prima del Close di sistema, quindi ⌘W
            // chiude la tab, non la finestra.
            CommandGroup(after: .newItem) {
                Button("Nuova tab") { model.newShellTabInSelected() }
                    .keyboardShortcut("t", modifiers: .command)
                Button("Chiudi tab") { model.closeActiveTab() }
                    .keyboardShortcut("w", modifiers: .command)
                Button("Salva") { model.saveActiveMarkdownDocument() }
                    .keyboardShortcut("s", modifiers: .command)
                Button("Apri file…") { model.openMarkdownFilePanel() }
                    .keyboardShortcut("o", modifiers: .command)
            }
            CommandGroup(replacing: .appSettings) {
                Button("Settings…") { model.openSettings() }
                    .keyboardShortcut(",", modifiers: .command)
            }
            CommandGroup(after: .sidebar) {
                Button(sidebarVisible ? "Nascondi Sidebar" : "Mostra Sidebar") {
                    sidebarVisible.toggle()
                }
                .keyboardShortcut("s", modifiers: [.control, .command])
            }
        }

        MenuBarExtra {
            AgentRosterView(model: model)
        } label: {
            MenuBarStatusIcon(status: model.menuBarAggregateStatus)
        }
        .menuBarExtraStyle(.window)
    }
}
