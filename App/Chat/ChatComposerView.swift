import SwiftUI
import AppKit
import TillerACP
import TillerAgents

/// Message input styled as a floating rounded card: text on top, control row
/// below (mode pill with status dot, agent pill, attach, circular send).
/// "/" opens a slash-command popup fed by the agent's advertised commands;
/// "@" keeps the file-mention autocomplete. ⏎ send, ⇧⏎ newline.
struct ChatComposerView: View {
    let controller: ChatController
    let worktreePath: String

    @State private var text = ""
    @State private var mentionPaths: [String] = []
    @State private var images: [ImageAttachment] = []
    @State private var mentionQuery: String?
    @State private var mentionCandidates: [String] = []
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var isPrompting: Bool { controller.state == .prompting }
    private var canInteract: Bool {
        (controller.state == .ready || isPrompting) && !controller.hasPendingPermission
    }
    private var canSend: Bool {
        canInteract && !(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                         && mentionPaths.isEmpty && images.isEmpty)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !slashCandidates.isEmpty {
                slashPopup
            }
            if let query = mentionQuery, !mentionCandidates.isEmpty {
                mentionPopup(query: query)
            }
            queuedList
            card
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }

    // MARK: - Card

    private var card: some View {
        VStack(alignment: .leading, spacing: 8) {
            attachmentChips
            editor
            controlBar
        }
        .padding(12)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14)
            .strokeBorder(.separator.opacity(0.5), lineWidth: 1))
    }

    private var editor: some View {
        TextField(controller.hasPendingPermission
                  ? "Waiting for permission response…"
                  : isPrompting ? "Type to queue for the next turn…"
                  : "Message…",
                  text: $text, axis: .vertical)
            .textFieldStyle(.plain)
            .lineLimit(1...8)
            .disabled(!canInteract)
            .onSubmit(sendCurrent)
            .onChange(of: text) { updateMentionQuery() }
    }

    private var controlBar: some View {
        HStack(spacing: 8) {
            modePill
            agentPill
            effortPill
            Spacer()
            contextUsageIndicator
            Button {
                attachImage()
            } label: {
                Image(systemName: "paperclip")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Attach image (clipboard or file)")
            .disabled(!canInteract)
            if isPrompting {
                stopButton
            } else {
                sendButton
            }
        }
    }

    // MARK: - Pills

    private var statusDotColor: Color {
        switch controller.state {
        case .ready: .green
        case .prompting: .orange
        default: .secondary.opacity(0.5)
        }
    }

    @ViewBuilder
    private var modePill: some View {
        if let modes = controller.modes, !modes.availableModes.isEmpty {
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

    /// Model selector when the agent advertises models (both v1 agents do);
    /// plain agent badge otherwise.
    @ViewBuilder
    private var agentPill: some View {
        if let models = controller.models, !models.availableModels.isEmpty {
            Menu {
                ForEach(models.availableModels, id: \.modelId) { model in
                    Button {
                        Task { await controller.setModel(model.modelId) }
                    } label: {
                        if model.modelId == models.currentModelId {
                            Label(modelMenuTitle(model), systemImage: "checkmark")
                        } else {
                            Text(modelMenuTitle(model))
                        }
                    }
                }
            } label: {
                HStack(spacing: 5) {
                    Text(currentModelName).font(.caption).lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .modifier(PillBackground())
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

    private func modelMenuTitle(_ model: ModelInfo) -> String {
        if let description = model.description, !description.isEmpty {
            return "\(model.name) — \(description)"
        }
        return model.name
    }

    /// Reasoning-effort select ("MED" pill); only agents that expose it
    /// (OpenCode) get the pill.
    @ViewBuilder
    private var effortPill: some View {
        if let effort = controller.effortOption,
           let choices = effort.options, !choices.isEmpty {
            Menu {
                ForEach(choices, id: \.value) { choice in
                    Button {
                        Task { await controller.setEffort(choice.value) }
                    } label: {
                        if choice.value == effort.currentValue {
                            Label(choice.name, systemImage: "checkmark")
                        } else {
                            Text(choice.name)
                        }
                    }
                }
            } label: {
                HStack(spacing: 4) {
                    Text(effortLabel(effort))
                        .font(.caption2.weight(.semibold))
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .modifier(PillBackground())
            .help(effort.name ?? "Effort")
        }
    }

    /// Context-window usage ring; hidden entirely when the agent never sent
    /// a `usage_update` (e.g. it doesn't implement that ACP extension) —
    /// there's nothing actionable the user can do about a missing signal,
    /// so no "unavailable" placeholder state (unlike UsageBarView).
    @ViewBuilder
    private var contextUsageIndicator: some View {
        if let usage = controller.contextUsage, usage.size > 0 {
            let fraction = min(1, max(0, Double(usage.used) / Double(usage.size)))
            let remaining = Int(((1 - fraction) * 100).rounded())
            ZStack {
                Circle().stroke(.quaternary, lineWidth: 2)
                Circle()
                    .trim(from: 0, to: fraction)
                    .stroke(Color.accentColor, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
            .frame(width: 16, height: 16)
            .contentShape(Circle())
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.3), value: fraction)
            .help("\(remaining)% remaining\n\(usage.used.formatted()) / \(usage.size.formatted()) tokens")
        }
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

    // MARK: - Send / stop

    private var sendButton: some View {
        Button(action: sendCurrent) {
            Image(systemName: "arrow.up")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(canSend ? Color.white : Color.secondary)
                .frame(width: 26, height: 26)
                .background(canSend ? AnyShapeStyle(Color.accentColor)
                                    : AnyShapeStyle(.quaternary),
                            in: Circle())
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
                .frame(width: 26, height: 26)
                .background(Color.red.opacity(0.8), in: Circle())
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.escape, modifiers: [])
        .help("Stop the turn")
    }

    // MARK: - Slash commands

    /// Active while the draft is a single "/token": query is what follows the
    /// slash, matched as a case-insensitive prefix of the command names.
    private var slashCandidates: [AvailableCommand] {
        guard text.hasPrefix("/"), !text.contains(where: \.isWhitespace),
              canInteract else { return [] }
        let query = text.dropFirst().lowercased()
        let all = controller.availableCommands
        guard !query.isEmpty else { return Array(all.prefix(10)) }
        return Array(all.filter { $0.name.lowercased().hasPrefix(query) }.prefix(10))
    }

    private var slashPopup: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(slashCandidates, id: \.name) { command in
                Button {
                    text = "/\(command.name) "
                } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text("/\(command.name)")
                            .font(.caption.weight(.semibold))
                        Text(command.description)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        Spacer(minLength: 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 6).padding(.vertical, 3)
            }
        }
        .padding(6)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
    }

    // MARK: - Attachments / queue

    @ViewBuilder
    private var attachmentChips: some View {
        if !mentionPaths.isEmpty || !images.isEmpty {
            HStack(spacing: 6) {
                ForEach(mentionPaths, id: \.self) { path in
                    chip(label: (path as NSString).lastPathComponent,
                         systemImage: "doc") {
                        mentionPaths.removeAll { $0 == path }
                    }
                }
                ForEach(Array(images.enumerated()), id: \.offset) { index, _ in
                    chip(label: "Immagine \(index + 1)", systemImage: "photo") {
                        images.remove(at: index)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var queuedList: some View {
        ForEach(Array(controller.queued.enumerated()), id: \.offset) { _, queuedText in
            Label(queuedText, systemImage: "clock")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
    }

    private func chip(label: String, systemImage: String,
                      onRemove: @escaping () -> Void) -> some View {
        HStack(spacing: 3) {
            Label(label, systemImage: systemImage).font(.caption)
            Button(action: onRemove) {
                Image(systemName: "xmark.circle.fill").font(.caption2)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 6).padding(.vertical, 2)
        .background(.quaternary, in: Capsule())
    }

    private func mentionPopup(query: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(mentionCandidates, id: \.self) { path in
                Button {
                    acceptMention(path)
                } label: {
                    HStack {
                        Image(systemName: "doc").font(.caption)
                        Text(path).font(.caption).lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(6)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
    }

    // MARK: - Actions

    private func sendCurrent() {
        let outgoing = text
        let mentions = mentionPaths
        let attachments = images
        guard canInteract else { return }
        text = ""
        mentionPaths = []
        images = []
        mentionQuery = nil
        controller.send(text: outgoing, mentionPaths: mentions, images: attachments)
    }

    /// The active @-token is the text from the last "@" to the caret's end,
    /// with no whitespace inside. Kept deliberately simple for v1.
    private func updateMentionQuery() {
        guard let atIndex = text.lastIndex(of: "@") else {
            mentionQuery = nil
            return
        }
        let token = String(text[text.index(after: atIndex)...])
        guard !token.contains(where: \.isWhitespace) else {
            mentionQuery = nil
            return
        }
        mentionQuery = token
        let path = worktreePath
        Task.detached(priority: .userInitiated) {
            let hits = FileMentionIndex.candidates(
                worktreePath: path, query: token, limit: 8)
            await MainActor.run {
                if mentionQuery == token { mentionCandidates = hits }
            }
        }
    }

    private func acceptMention(_ path: String) {
        if let atIndex = text.lastIndex(of: "@") {
            text = String(text[..<atIndex])
        }
        if !mentionPaths.contains(path) { mentionPaths.append(path) }
        mentionQuery = nil
        mentionCandidates = []
    }

    /// Pasteboard image if present, else a file picker. (Direct ⌘V paste
    /// interception inside TextField is a known v1 gap.)
    private func attachImage() {
        if let data = NSPasteboard.general.data(forType: .png) {
            images.append(ImageAttachment(mimeType: "image/png",
                                          base64Data: data.base64EncodedString()))
            return
        }
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.png, .jpeg]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url,
              let data = try? Data(contentsOf: url) else { return }
        let mime = url.pathExtension.lowercased() == "png" ? "image/png" : "image/jpeg"
        images.append(ImageAttachment(mimeType: mime,
                                      base64Data: data.base64EncodedString()))
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
