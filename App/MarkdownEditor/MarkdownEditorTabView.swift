import SwiftUI
import MarkdownUI
import TillerCore

/// Tab editor markdown: preview MarkdownUI (default) + modalità codice,
/// toggle stile Orca in alto a destra. Banner per conflitti esterni e
/// file cancellato.
struct MarkdownEditorTabView: View {
    @Bindable var document: MarkdownDocument
    @State private var mode: EditorMode
    @State private var selection: TextSelection?

    /// Oltre questa soglia la preview non si apre da sola: MarkdownUI
    /// rallenta sui documenti enormi. ponytail: soglia fissa, config se servirà.
    private static let previewByteLimit = 2_000_000
    private let isHuge: Bool

    enum EditorMode { case preview, code }

    init(document: MarkdownDocument) {
        self.document = document
        let huge = document.text.utf8.count > Self.previewByteLimit
        self.isHuge = huge
        self._mode = State(initialValue: huge ? .code : .preview)
    }

    var body: some View {
        Group {
            VStack(spacing: 0) {
                header
                if document.externalChangeConflict { conflictBanner }
                if document.fileDeleted { deletedBanner }
                if mode == .code {
                    MarkdownToolbar(document: document, selection: $selection)
                    Divider()
                    TextEditor(text: $document.text, selection: $selection)
                        .font(.system(.body, design: .monospaced))
                        .scrollContentBackground(.hidden)
                        .padding(8)
                } else {
                    ScrollView {
                        Markdown(document.text)
                            .markdownTheme(.gitHub)
                            .textSelection(.enabled)
                            .padding(16)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .background(AppTheme.background)
        }
    }

    private var header: some View {
        HStack(spacing: 8) {
            Text(document.fileURL.path)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            if isHuge {
                Text("File grande — preview manuale")
                    .font(.system(size: 10))
                    .foregroundStyle(.orange)
            }
            Spacer()
            Picker("", selection: $mode) {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .tag(EditorMode.code)
                    .help("Codice")
                Image(systemName: "doc.richtext")
                    .tag(EditorMode.preview)
                    .help("Preview")
            }
            .pickerStyle(.segmented)
            .frame(width: 76)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }

    private var conflictBanner: some View {
        HStack {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            Text("File modificato su disco.")
            Spacer()
            Button("Ricarica") { document.reloadFromDisk() }
            Button("Mantieni") { document.keepLocalBuffer() }
        }
        .font(.system(size: 12))
        .padding(8)
        .background(.orange.opacity(0.15))
    }

    private var deletedBanner: some View {
        HStack {
            Image(systemName: "trash").foregroundStyle(.red)
            Text("File eliminato su disco. ⌘S lo ricrea.")
            Spacer()
        }
        .font(.system(size: 12))
        .padding(8)
        .background(.red.opacity(0.15))
    }
}
