import SwiftUI
import AppKit
import TillerACP

/// Message input: multiline text (⏎ send, ⇧⏎ newline), @-mention
/// autocomplete over worktree files, slash commands from the agent, image
/// attachment from pasteboard/file picker, mode selector, send/stop.
struct ChatComposerView: View {
    let controller: ChatController
    let worktreePath: String

    @State private var text = ""
    @State private var mentionPaths: [String] = []
    @State private var images: [ImageAttachment] = []
    @State private var mentionQuery: String?
    @State private var mentionCandidates: [String] = []

    private var isPrompting: Bool { controller.state == .prompting }
    private var canInteract: Bool {
        (controller.state == .ready || isPrompting) && !controller.hasPendingPermission
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let query = mentionQuery, !mentionCandidates.isEmpty {
                mentionPopup(query: query)
            }
            attachmentChips
            queuedList
            editor
            controlBar
        }
        .padding(10)
        .background(.bar)
    }

    // MARK: - Subviews

    private var editor: some View {
        TextField(controller.hasPendingPermission
                  ? "In attesa di risposta al permesso…"
                  : isPrompting ? "Scrivi per accodare al prossimo turno…"
                  : "Scrivi un messaggio…",
                  text: $text, axis: .vertical)
            .textFieldStyle(.plain)
            .lineLimit(1...8)
            .disabled(!canInteract)
            .onSubmit(sendCurrent)
            .onChange(of: text) { updateMentionQuery() }
    }

    private var controlBar: some View {
        HStack(spacing: 10) {
            modeSelector
            Button {
                attachImage()
            } label: {
                Image(systemName: "photo.badge.plus")
            }
            .buttonStyle(.plain)
            .help("Allega immagine (appunti o file)")
            .disabled(!canInteract)
            slashCommandsMenu
            Spacer()
            if isPrompting {
                Button {
                    Task { await controller.cancelTurn() }
                } label: {
                    Label("Stop", systemImage: "stop.fill")
                }
                .keyboardShortcut(.escape, modifiers: [])
            }
            Button("Invia", action: sendCurrent)
                .keyboardShortcut(.return, modifiers: [])
                .disabled(!canInteract ||
                          text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                          && mentionPaths.isEmpty && images.isEmpty)
        }
        .controlSize(.small)
    }

    @ViewBuilder
    private var modeSelector: some View {
        if let modes = controller.modes, !modes.availableModes.isEmpty {
            Menu {
                ForEach(modes.availableModes, id: \.id) { mode in
                    Button(mode.name) {
                        Task { await controller.setMode(mode.id) }
                    }
                }
            } label: {
                let current = modes.availableModes
                    .first { $0.id == controller.currentModeId }?.name
                Text(current ?? "Modalità")
                    .font(.caption)
            }
            .fixedSize()
        }
    }

    @ViewBuilder
    private var slashCommandsMenu: some View {
        if !controller.availableCommands.isEmpty {
            Menu {
                ForEach(controller.availableCommands, id: \.name) { command in
                    Button("/\(command.name)") {
                        text = "/\(command.name) "
                    }
                    .help(command.description)
                }
            } label: {
                Image(systemName: "slash.circle")
            }
            .fixedSize()
        }
    }

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
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 6))
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
