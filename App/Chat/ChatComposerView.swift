import SwiftUI
import AppKit
import TillerACP
import TillerAgents

/// Message input styled as a floating rounded card: text on top, control row
/// below (mode pill with status dot, agent pill, follow, new conversation,
/// attach, circular send).
/// "/" opens a slash-command popup fed by the agent's advertised commands;
/// "@" keeps the file-mention autocomplete. ⏎ send, ⇧⏎ newline.
struct ChatComposerView: View {
    let controller: ChatController
    let worktreePath: String

    @State private var document = ComposerDocument()
    @State private var mentionCandidates: [String] = []
    @State private var slashSelectionIndex = 0
    @State private var slashPopupDismissed = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var isPrompting: Bool { controller.state == .prompting }
    private var isConnecting: Bool { controller.state == .connecting }
    private var canInteract: Bool {
        (controller.state == .ready || isPrompting || controller.state == .detached)
            && !controller.presentationSnapshot.hasPendingPermission
    }
    private var canSend: Bool { canInteract && !document.isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if slashPopupVisible {
                slashPopup
            }
            if let query = document.mentionQuery, !mentionCandidates.isEmpty {
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
        let border = ComposerControlBar.borderStyle(isFocused: document.isFocused)
        return VStack(alignment: .leading, spacing: 8) {
            editor
            ComposerControlBar(
                controller: controller, document: document, onAttach: attachImage,
                onSend: sendCurrent, canSend: canSend, canInteract: canInteract)
        }
        .padding(12)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14)
            .strokeBorder(border.color, lineWidth: border.width))
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.15), value: document.isFocused)
    }

    private var editorPlaceholder: String {
        controller.presentationSnapshot.hasPendingPermission
            ? "Waiting for permission response…"
            : isPrompting ? "Type to queue for the next turn…"
            : "Message…"
    }

    private var editor: some View {
        ZStack(alignment: .topLeading) {
            if document.isEmpty {
                Text(editorPlaceholder)
                    .foregroundStyle(.secondary)
                    .allowsHitTesting(false)
            }
            ChatTextEditor(document: document, isEditable: canInteract, minHeight: 36,
                           maxHeight: 160, onSubmit: sendCurrent,
                           onSlashKey: handleSlashKey)
        }
        .disabled(!canInteract)
        .onChange(of: document.mentionQuery) { _, query in
            refreshMentionCandidates(query: query)
        }
        .onChange(of: document.slashQuery) {
            slashPopupDismissed = false
            slashSelectionIndex = 0
        }
    }


    private var loadingButton: some View {
        ProgressView()
            .controlSize(.small)
            .frame(width: 26, height: 26)
            .background(.quaternary, in: Circle())
            .help("Starting the agent…")
    }

    // MARK: - Slash commands

    /// Active while the draft is a single "/token": the query is what follows
    /// the slash, matched as a case-insensitive prefix of the command names.
    private var slashCandidates: [AvailableCommand] {
        guard canInteract, let query = document.slashQuery?.lowercased() else { return [] }
        let all = controller.availableCommands
        guard !query.isEmpty else { return Array(all.prefix(10)) }
        return Array(all.filter { $0.name.lowercased().hasPrefix(query) }.prefix(10))
    }

    private var slashPopupVisible: Bool {
        !slashCandidates.isEmpty && !slashPopupDismissed
    }

    private func handleSlashKey(_ key: SlashKey) -> Bool {
        guard slashPopupVisible,
              let effect = SlashCommandSelection.effect(
                  for: key, index: slashSelectionIndex, count: slashCandidates.count)
        else { return false }
        switch effect {
        case .moved(let index):
            slashSelectionIndex = index
        case .accepted(let index):
            acceptSlashCommand(slashCandidates[index])
        case .dismissed:
            slashPopupDismissed = true
        }
        return true
    }

    private var slashPopup: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(slashCandidates.enumerated()), id: \.element.name) { index, command in
                Button {
                    acceptSlashCommand(command)
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
                .background(index == min(slashSelectionIndex, slashCandidates.count - 1)
                                ? AppTheme.selectionFill : Color.clear,
                            in: RoundedRectangle(cornerRadius: 4))
            }
        }
        .padding(6)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
    }

    // MARK: - Attachments / queue

    @ViewBuilder
    private var queuedList: some View {
        ForEach(Array(controller.queued.enumerated()), id: \.offset) { _, queuedText in
            Label(queuedText, systemImage: "clock")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
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
        guard canInteract else { return }
        let draft = document.takeDraft()
        mentionCandidates = []
        controller.send(text: draft.text, mentionPaths: draft.mentionPaths,
                        images: draft.images)
    }

    private func acceptSlashCommand(_ command: AvailableCommand) {
        _ = document.replaceSlashToken(with: .skill(name: command.name))
    }

    private func refreshMentionCandidates(query: String?) {
        guard let query else {
            mentionCandidates = []
            return
        }
        let path = worktreePath
        Task.detached(priority: .userInitiated) {
            let hits = FileMentionIndex.candidates(
                worktreePath: path, query: query, limit: 8)
            await MainActor.run {
                if document.mentionQuery == query { mentionCandidates = hits }
            }
        }
    }

    /// Swaps the live "@token" for a file chip at the same position, so the
    /// chip lands where the user was typing.
    private func acceptMention(_ path: String) {
        guard let query = document.mentionQuery else { return }
        let range = (document.storage.string as NSString).range(of: "@\(query)",
                                                                options: .backwards)
        guard range.location != NSNotFound else { return }
        document.insert(.file(path: path), replacing: range)
        mentionCandidates = []
    }

    /// Opens a file picker for an image and inserts it as a chip at the caret.
    /// (Direct ⌘V clipboard-paste interception is a known gap — stray PNG data
    /// lingering on the general pasteboard from an unrelated copy would
    /// silently hijack every click of this button.)
    private func attachImage() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.png, .jpeg]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url,
              let data = try? Data(contentsOf: url) else { return }
        let mime = url.pathExtension.lowercased() == "png" ? "image/png" : "image/jpeg"
        let attachment = ImageAttachment(mimeType: mime,
                                         base64Data: data.base64EncodedString())
        document.insert(.image(attachment),
                        replacing: NSRange(location: document.storage.length, length: 0))
    }
}
