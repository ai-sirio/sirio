import SwiftUI
import AppKit
import TillerCore
import TillerControl

/// General settings: app info plus the tillerctl wiring an agent needs to talk
/// back to Tiller from a pane's shell. Moved from the former SettingsView.
struct GeneralSettingsView: View {
    var updater: UpdaterModel
    var model: AppModel
    @State private var copied = false
    @AppStorage(AppSettings.resumeAgentSessionsKey) private var resumeAgentSessions = true
    @AppStorage(AppSettings.controlSocketEnabledKey) private var controlSocketEnabled = true
    @AppStorage(AppSettings.maxMountedWorktreesKey) private var maxMountedWorktrees = 0

    var body: some View {
        Form {
            Section("About") {
                LabeledContent("Version", value: AppVersion.current)
                LabeledContent {
                    Button("Check for Updates") { updater.checkForUpdates() }
                } label: {
                    Text("Updates")
                    if case .checking = updater.state {
                        Text("Controllo in corso…")
                    } else if case .upToDate = updater.state {
                        Text("Sei aggiornato.")
                    }
                }
            }

            Section("Agents") {
                Toggle(isOn: $resumeAgentSessions) {
                    Text("Resume agent sessions on launch")
                    Text("Relaunch supported agents with their previous conversation after Tiller restarts.")
                }
            }
            Section("Performance") {
                Toggle(isOn: Binding(
                    get: { maxMountedWorktrees > 0 },
                    set: { maxMountedWorktrees = $0 ? 6 : 0 }
                )) {
                    Text("Limit mounted worktrees")
                    Text("Frees terminal RAM by unmounting idle worktrees beyond this count. Never touches the selected worktree, one with a running or waiting agent, or one with unsaved tabs.")
                }
                if maxMountedWorktrees > 0 {
                    Stepper("Keep \(maxMountedWorktrees) mounted", value: $maxMountedWorktrees, in: 2...50)
                }
            }
            Section("tillerctl") {
                Toggle(isOn: $controlSocketEnabled) {
                    Text("Enable control socket")
                    Text("Required by tillerctl and by agent lifecycle hooks — disabling it degrades agent status badges to title/process detection only.")
                }
                .onChange(of: controlSocketEnabled) { _, enabled in
                    model.setControlSocketEnabled(enabled)
                }
                LabeledContent("Bundled binary", value: tillerctlBundledPath)
                LabeledContent("Control socket", value: ControlSocket.defaultPath())
                LabeledContent {
                    Button(copied ? "Copied" : "Copy install command") {
                        let command = "sudo ln -sf '\(tillerctlBundledPath)' /usr/local/bin/tillerctl"
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(command, forType: .string)
                        copied = true
                    }
                } label: {
                    Text("Install on PATH")
                    Text("Symlinks tillerctl into /usr/local/bin so agents in any shell can reach it.")
                }
            }
            Section("Agent Skill") {
                LabeledContent {
                    Button("Install Skill") {
                        AgentSkillInstaller.openTerminalAndInstall()
                    }
                } label: {
                    Text("tillerctl skill")
                    Text("Installa una skill che insegna a Claude Code, Codex e OpenCode come usare tillerctl per orchestrare pane e worktree.")
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    }

    private var tillerctlBundledPath: String {
        Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/tillerctl").path
    }
}
