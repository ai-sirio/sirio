import SwiftUI
import AppKit
import TillerCore
import TillerControl

/// General settings: app info plus the tillerctl wiring an agent needs to talk
/// back to Tiller from a pane's shell. Moved from the former SettingsView.
struct GeneralSettingsView: View {
    @State private var copied = false
    @AppStorage(AppSettings.resumeAgentSessionsKey) private var resumeAgentSessions = true

    var body: some View {
        Form {
            Section("About") {
                LabeledContent("Version", value: AppVersion.current)
            }

            Section("Agents") {
                Toggle(isOn: $resumeAgentSessions) {
                    Text("Resume agent sessions on launch")
                    Text("Relaunch supported agents with their previous conversation after Tiller restarts.")
                }
            }
            Section("tillerctl") {
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
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    }

    private var tillerctlBundledPath: String {
        Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/tillerctl").path
    }
}
