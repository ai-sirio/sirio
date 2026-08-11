import SwiftUI
import AppKit
import TillerACP
import TillerAgents
import Inject

/// The composer's bottom row, in the AIR arrangement: attach on the left with
/// the model picker, and the secondary controls collapsed into an overflow
/// menu on the right so the row carries 5 elements instead of 8.
struct ComposerControlBar: View {
    @ObserveInjection private var inject

    let controller: ChatController
    let document: ComposerDocument
    let onAttach: () -> Void
    let onSend: () -> Void
    let canSend: Bool
    let canInteract: Bool
    let agentAccentColor: Color

    @State private var modelPickerShown = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    enum TrailingControl: Equatable { case loading, stop, send }

    enum PrimaryActionShape: Equatable { case circle }

    struct PrimaryActionPresentation: Equatable {
        let kind: TrailingControl
        let footprintSize: CGFloat
        let shape: PrimaryActionShape
        let accessibilityLabel: String
        let accessibilityHelp: String
        let systemImage: String?
    }

    static let primaryActionSize: CGFloat = 30
    static let inactiveActionFillOpacity = 0.18
    static let inactiveActionFill = Color.secondary.opacity(inactiveActionFillOpacity)
    static let sendSystemImage = "arrow.up"
    static let sendAccessibilityLabel = "Send"
    static let loadingAccessibilityLabel = "Starting the agent"
    static let stopAccessibilityLabel = "Stop the turn"

    /// Which control closes the row. Startup is slow enough on some agents that
    /// a plain Send button reads as "nothing happened", so connecting gets a
    /// spinner of its own instead of falling through to send.
    static func trailingControl(for state: ChatController.ChatState) -> TrailingControl {
        switch state {
        case .connecting: .loading
        case .prompting: .stop
        default: .send
        }
    }

    /// Send button fill: the active agent's accent color while a send is
    /// possible, the shared neutral inactive fill otherwise.
    static func sendFill(canSend: Bool, agentAccentColor: Color) -> Color {
        canSend ? agentAccentColor : inactiveActionFill
    }

    /// Send glyph: white on the filled accent circle while a send is
    /// possible, the agent's accent color on the neutral inactive fill otherwise.
    static func sendGlyphColor(canSend: Bool, agentAccentColor: Color) -> Color {
        canSend ? .white : agentAccentColor
    }

    static func primaryActionPresentation(
        for state: ChatController.ChatState
    ) -> PrimaryActionPresentation {
        let kind = trailingControl(for: state)
        switch kind {
        case .send:
            return PrimaryActionPresentation(
                kind: kind,
                footprintSize: primaryActionSize,
                shape: .circle,
                accessibilityLabel: sendAccessibilityLabel,
                accessibilityHelp: sendAccessibilityLabel,
                systemImage: sendSystemImage)
        case .loading:
            return PrimaryActionPresentation(
                kind: kind,
                footprintSize: primaryActionSize,
                shape: .circle,
                accessibilityLabel: loadingAccessibilityLabel,
                accessibilityHelp: loadingAccessibilityLabel,
                systemImage: nil)
        case .stop:
            return PrimaryActionPresentation(
                kind: kind,
                footprintSize: primaryActionSize,
                shape: .circle,
                accessibilityLabel: stopAccessibilityLabel,
                accessibilityHelp: stopAccessibilityLabel,
                systemImage: "stop.fill")
        }
    }

    var body: some View {
        let primaryAction = Self.primaryActionPresentation(for: controller.state)
        HStack(spacing: 8) {
            HStack(spacing: 8) {
                attachmentButton
                modePill
            }

            Spacer()

            HStack(spacing: 8) {
                overflowMenu
                contextUsageIndicator
                agentPill
                switch primaryAction.kind {
                case .loading: loadingButton(presentation: primaryAction)
                case .stop: stopButton(presentation: primaryAction)
                case .send: sendButton(presentation: primaryAction)
                }
            }
        }
    .enableInjection()
    }

    private var attachmentButton: some View {
        Button(action: onAttach) {
            Image(systemName: "plus")
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.plain)
        .help("Attach image")
        .disabled(!canInteract)
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
                    Text(mode.displayName).font(AppFont.caption)
                    Image(systemName: "chevron.down").font(AppFont.system(size: 7, weight: .bold))
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
                    Text(currentModeName).font(AppFont.caption)
                    Image(systemName: "chevron.down").font(AppFont.system(size: 7, weight: .bold))
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .modifier(PillBackground())
        } else {
            HStack(spacing: 5) {
                Circle().fill(statusDotColor).frame(width: 6, height: 6)
                Text(stateLabel).font(AppFont.caption)
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
        case .detached: "idle"
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
                    Text(currentModelName).font(AppFont.caption).lineLimit(1)
                    if let effort = controller.effortOption,
                       effort.currentValue != nil {
                        Text(effortLabel(effort))
                            .font(AppFont.caption2.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }
                    Image(systemName: "chevron.down").font(AppFont.system(size: 7, weight: .bold))
                }
            }
            .buttonStyle(.plain)
            .modifier(PillBackground())
            .popover(isPresented: $modelPickerShown, arrowEdge: .top) {
                ModelPickerPopover(controller: controller,
                                   isPresented: $modelPickerShown)
            }
        } else {
            Text(agentDisplayName).font(AppFont.caption)
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
    /// stable. Empty/dimmed until the agent reports usage. Turns red past the
    /// 80% warning threshold so it stays distinct from the agent's accent.
    private var contextUsageIndicator: some View {
        let usage = controller.contextUsage
        let fraction = usage.flatMap { $0.size > 0 ? min(1, max(0, Double($0.used) / Double($0.size))) : nil } ?? 0
        let warning = fraction > 0.8
        return ZStack {
            Circle().stroke(.quaternary, lineWidth: 2)
            if usage != nil {
                Circle()
                    .trim(from: 0, to: fraction)
                    .stroke(Self.contextRingColor(
                        warning: warning, agentAccentColor: agentAccentColor),
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

    /// Context ring stroke: red past the warning threshold so a nearly-full
    /// context stays distinguishable from the agent's own accent color.
    static func contextRingColor(warning: Bool, agentAccentColor: Color) -> Color {
        warning ? .red : agentAccentColor
    }

    struct ContextUsageDetail: Equatable {
        let percentLine: String
        let tokensLine: String
        let costLine: String?
        let breakdownLine: String?
    }

    /// Turns a driver's `ContextUsage` into the popover's display strings.
    /// Cost and the token breakdown are Claude-only on the wire today, so
    /// both lines are `nil` for every other agent.
    ///
    /// Locale is pinned to `en_US`: the popover's display contract (asserted
    /// in ComposerControlBarTests) uses en_US grouping/currency, so the
    /// strings stay deterministic regardless of the machine's locale.
    static func contextUsageDetail(_ usage: ContextUsage) -> ContextUsageDetail {
        let fraction = usage.size > 0 ? min(1, max(0, Double(usage.used) / Double(usage.size))) : 0
        let percent = Int((fraction * 100).rounded())
        let displayLocale = Locale(identifier: "en_US")
        let costLine = usage.costUsd.map { "Cost: " + $0.formatted(.currency(code: "USD").locale(displayLocale)) }
        let breakdownLine: String? = {
            guard let input = usage.inputTokens, let output = usage.outputTokens else { return nil }
            var line = "Input: \(input.formatted(.number.locale(displayLocale))) · Output: \(output.formatted(.number.locale(displayLocale)))"
            if let write = usage.cacheCreationTokens, let read = usage.cacheReadTokens {
                line += " · Cache write: \(write.formatted(.number.locale(displayLocale))) · Cache read: \(read.formatted(.number.locale(displayLocale)))"
            }
            return line
        }()
        return ContextUsageDetail(
            percentLine: "\(percent)% of context used",
            tokensLine: "\(usage.used.formatted(.number.locale(displayLocale))) / \(usage.size.formatted(.number.locale(displayLocale))) tokens",
            costLine: costLine,
            breakdownLine: breakdownLine)
    }

    @ViewBuilder
    private func primaryActionChrome<Content: View>(
        presentation: PrimaryActionPresentation,
        fill: AnyShapeStyle,
        @ViewBuilder content: () -> Content
    ) -> some View {
        switch presentation.shape {
        case .circle:
            content()
                .frame(width: presentation.footprintSize,
                       height: presentation.footprintSize)
                .background(fill, in: Circle())
        }
    }

    private func sendButton(presentation: PrimaryActionPresentation) -> some View {
        return Button(action: onSend) {
            primaryActionChrome(
                presentation: presentation,
                fill: AnyShapeStyle(Self.sendFill(
                    canSend: canSend, agentAccentColor: agentAccentColor))) {
                if let systemImage = presentation.systemImage {
                    Image(systemName: systemImage)
                        .font(AppFont.system(size: 12, weight: .bold))
                        .foregroundStyle(Self.sendGlyphColor(
                            canSend: canSend, agentAccentColor: agentAccentColor))
                }
            }
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.return, modifiers: [])
        .disabled(!canSend)
        .accessibilityLabel(presentation.accessibilityLabel)
        .help(presentation.accessibilityHelp)
    }

    /// A spinner in the primary action's fixed footprint while the agent starts.
    private func loadingButton(presentation: PrimaryActionPresentation) -> some View {
        return primaryActionChrome(
            presentation: presentation,
            fill: AnyShapeStyle(Self.inactiveActionFill)) {
            ProgressView()
                .controlSize(.small)
                .tint(agentAccentColor)
        }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(presentation.accessibilityLabel)
            .help(presentation.accessibilityHelp)
    }

    private func stopButton(presentation: PrimaryActionPresentation) -> some View {
        return Button {
            Task { await controller.cancelTurn() }
        } label: {
            primaryActionChrome(
                presentation: presentation,
                fill: AnyShapeStyle(Color.red.opacity(0.8))) {
                if let systemImage = presentation.systemImage {
                    Image(systemName: systemImage)
                        .font(AppFont.system(size: 10, weight: .bold))
                        .foregroundStyle(.white)
                }
            }
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.escape, modifiers: [])
        .accessibilityLabel(presentation.accessibilityLabel)
        .help(presentation.accessibilityHelp)
    }
}

/// Capsule chrome shared by the composer's mode/agent pills.
private struct PillBackground: ViewModifier {
    @Environment(\.colorScheme) private var colorScheme

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 8).padding(.vertical, 4)
            .background(
                colorScheme == .dark
                    ? AnyShapeStyle(AppTheme.cardFill)
                    : AnyShapeStyle(.quaternary.opacity(0.6)),
                in: Capsule())
    }
}
