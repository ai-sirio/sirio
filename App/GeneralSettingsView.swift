import SwiftUI
import AppKit
import TillerCore
import TillerAgents
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
    @AppStorage(AppSettings.autoNamingEnabledKey) private var autoNamingEnabled = false
    @AppStorage(AppSettings.summarizerAgentIdKey)
    private var summarizerAgentId = AppSettings.defaultSummarizerAgentId

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
            Section("Automation") {
                Toggle(isOn: $autoNamingEnabled) {
                    Text("Auto-rename tabs and agents")
                    Text("Summarizes each session's conversation into a short tab title using the selected summarizer agent. Manual renames always win.")
                }
                Picker(selection: $summarizerAgentId) {
                    ForEach(AgentCatalog.all, id: \.id) { adapter in
                        Text(adapter.displayName).tag(adapter.id)
                    }
                } label: {
                    Text("Summarizer agent")
                    Text("The agent CLI that generates tab titles. Falls back to the session's own agent when it fails.")
                }
                .disabled(!autoNamingEnabled)
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
                    Text("tiller skill")
                    Text("Installed automatically inside launched worktrees for all five Tiller harnesses. This button installs the public package for supported Skills CLI agents.")
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
