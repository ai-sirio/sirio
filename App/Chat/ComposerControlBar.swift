import SwiftUI
import AppKit
import TillerACP
import TillerAgents

/// The composer's bottom row, in the AIR arrangement: attach on the left with
/// the model picker, and the secondary controls collapsed into an overflow
/// menu on the right so the row carries 5 elements instead of 8.
struct ComposerControlBar: View {
    let controller: ChatController
    let document: ComposerDocument
    let onAttach: () -> Void
    let onSend: () -> Void
    let canSend: Bool
    let canInteract: Bool

    @State private var modelPickerShown = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// The composer card's border: accent while the text view holds focus,
    /// separator otherwise.
    static func borderStyle(isFocused: Bool) -> (color: Color, width: CGFloat) {
        isFocused ? (.accentColor, 1.5) : (Color(nsColor: .separatorColor), 1)
    }

    var body: some View {
        HStack(spacing: 8) {
            Button(action: onAttach) {
                Image(systemName: "plus")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Attach image")
            .disabled(!canInteract)

            agentPill
            Spacer()
            overflowMenu
            contextUsageIndicator
            modePill
            if controller.state == .prompting {
                stopButton
            } else {
                sendButton
            }
        }
    }

    private var overflowMenu: some View {
        Menu {
            Button(controller.isFollowing ? "Stop Following" : "Follow Edited Files") {
                controller.isFollowing.toggle()
            }
            Button("New Conversation") {
                Task { await controller.newConversation() }
            }
        } label: {
            Image(systemName: "ellipsis")
                .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    // MARK: - Twelve members moved verbatim

    private var statusDotColor: Color {
        switch controller.state {
        case .ready: .green
        case .prompting: .orange
        default: .secondary.opacity(0.5)
        }
    }

    @ViewBuilder
    private var modePill: some View {
        if let mode = controller.permissionMode {
            Menu {
                ForEach(PermissionMode.supported(byDriverFor: controller.agentId),
                        id: \.self) { candidate in
                    Button {
                        Task { await controller.setPermissionMode(candidate) }
                    } label: {
                        if candidate == mode {
                            Label(candidate.displayName, systemImage: "checkmark")
                        } else {
                            Text(candidate.displayName)
                        }
                    }
                }
            } label: {
                HStack(spacing: 5) {
                    Circle().fill(statusDotColor).frame(width: 6, height: 6)
                    Text(mode.displayName).font(.caption)
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .modifier(PillBackground())
            .help("Permission mode")
        } else if let modes = controller.modes, !modes.availableModes.isEmpty {
            Menu {
                ForEach(modes.availableModes, id: \.id) { mode in
                    Button(mode.name) {
                        Task { await controller.setMode(mode.id) }
                    }
                }
            } label: {
                HStack(spacing: 5) {
                    Circle().fill(statusDotColor).frame(width: 6, height: 6)
                    Text(currentModeName).font(.caption)
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .modifier(PillBackground())
        } else {
            HStack(spacing: 5) {
                Circle().fill(statusDotColor).frame(width: 6, height: 6)
                Text(stateLabel).font(.caption)
            }
            .modifier(PillBackground())
        }
    }

    private var currentModeName: String {
        controller.modes?.availableModes
            .first { $0.id == controller.currentModeId }?.name ?? "Mode"
    }

    private var stateLabel: String {
        switch controller.state {
        case .ready: "ready"
        case .prompting: "working"
        case .connecting: "connecting…"
        default: "offline"
        }
    }

    /// Compact model button opening the picker popover; plain agent badge
    /// when the agent advertises no models.
    @ViewBuilder
    private var agentPill: some View {
        if let models = controller.models, !models.availableModels.isEmpty {
            Button {
                modelPickerShown.toggle()
            } label: {
                HStack(spacing: 5) {
                    Text(currentModelName).font(.caption).lineLimit(1)
                    if let effort = controller.effortOption,
                       effort.currentValue != nil {
                        Text(effortLabel(effort))
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
            }
            .buttonStyle(.plain)
            .modifier(PillBackground())
            .popover(isPresented: $modelPickerShown, arrowEdge: .top) {
                ModelPickerPopover(controller: controller,
                                   isPresented: $modelPickerShown)
            }
        } else {
            Text(agentDisplayName).font(.caption)
                .modifier(PillBackground())
        }
    }

    private var currentModelName: String {
        guard let models = controller.models else { return agentDisplayName }
        return models.availableModels
            .first { $0.modelId == models.currentModelId }?.name
            ?? models.currentModelId
    }

    /// Context-window meter; always shown so its control-bar position stays
    /// stable. Empty/dimmed until the agent reports usage. Turns orange past
    /// the 80% warning threshold.
    private var contextUsageIndicator: some View {
        let usage = controller.contextUsage
        let fraction = usage.flatMap { $0.size > 0 ? min(1, max(0, Double($0.used) / Double($0.size))) : nil } ?? 0
        let warning = fraction > 0.8
        return ZStack {
            Circle().stroke(.quaternary, lineWidth: 2)
            if usage != nil {
                Circle()
                    .trim(from: 0, to: fraction)
                    .stroke(warning ? Color.orange : Color.accentColor,
                            style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
        }
        .frame(width: 16, height: 16)
        .contentShape(Circle())
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.3), value: fraction)
        .help(usage.map { usage in
            let percent = Int((fraction * 100).rounded())
            return "\(percent)% of context used\n\(usage.used.formatted()) / \(usage.size.formatted()) tokens"
        } ?? "Context usage unavailable")
    }

    private func effortLabel(_ effort: SessionConfigOption) -> String {
        guard let current = effort.currentValue else { return "—" }
        let name = effort.options?.first { $0.value == current }?.name ?? current
        return name.uppercased()
    }

    private var agentDisplayName: String {
        AgentCatalog.all.first { $0.id == controller.agentId }?.displayName
            ?? controller.agentId
    }

    private var sendButton: some View {
        Button(action: onSend) {
            Text("Send")
                .font(.caption.weight(.semibold))
                .foregroundStyle(canSend ? Color.white : Color.secondary)
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(canSend ? AnyShapeStyle(Color.accentColor)
                                    : AnyShapeStyle(.quaternary),
                            in: RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.return, modifiers: [])
        .disabled(!canSend)
    }

    private var stopButton: some View {
        Button {
            Task { await controller.cancelTurn() }
        } label: {
            Image(systemName: "stop.fill")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(.white)
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(Color.red.opacity(0.8), in: RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.escape, modifiers: [])
        .help("Stop the turn")
    }
}

/// Capsule chrome shared by the composer's mode/agent pills.
private struct PillBackground: ViewModifier {
    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 8).padding(.vertical, 4)
            .background(.quaternary.opacity(0.6), in: Capsule())
    }
}
