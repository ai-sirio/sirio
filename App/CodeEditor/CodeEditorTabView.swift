import CodeEditSourceEditor
import SwiftUI
import TillerCode

struct CodeEditorTabView: View {
    @Bindable var document: CodeDocument
    @Environment(\.colorScheme) private var colorScheme
    @State private var editorState = SourceEditorState()
    private static let highlightByteLimit = 2_000_000

    private var detectedLanguage: TillerCodeLanguage {
        guard document.text.utf8.count <= Self.highlightByteLimit else {
            return CodeLanguageResolver.plainText
        }
        return CodeLanguageResolver.language(for: document.fileURL, contents: document.text)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            if document.externalChangeConflict { conflictBanner }
            if document.fileDeleted { deletedBanner }
            SourceEditor(
                $document.text,
                language: detectedLanguage,
                configuration: SourceEditorConfiguration(
                    appearance: .init(
                        theme: .tiller(isDark: colorScheme == .dark),
                        font: .monospacedSystemFont(ofSize: 12, weight: .regular),
                        wrapLines: false),
                    behavior: .init(isEditable: true, isSelectable: true,
                                    indentOption: .spaces(count: 4)),
                    // Left nil, the scroll view auto-adjusts its top inset to the
                    // window safe area — non-zero here because the window is
                    // `.fullSizeContentView` — which offsets the gutter by that
                    // much, since it sits at `textView.origin.y - contentInsets.top`.
                    layout: .init(contentInsets: NSEdgeInsets())),
                state: $editorState)
            // The gutter is a floating subview of the scroll view, so it lives
            // outside the clip view and is as tall as the whole document. Scrolled
            // down, it reaches above the scroll view and — `NSView.clipsToBounds`
            // being false by default since macOS 14 — paints over the tab bar.
            .clipped()
        }
        .background(AppTheme.background)
    }

    private var header: some View {
        HStack(spacing: 8) {
            Text(document.fileURL.path)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Text(detectedLanguage.tsName)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }

    private var conflictBanner: some View {
        HStack {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            Text("File changed on disk.")
            Spacer()
            Button("Reload") { document.reloadFromDisk() }
            Button("Keep") { document.keepLocalBuffer() }
        }
        .font(.system(size: 12))
        .padding(8)
        .background(.orange.opacity(0.15))
    }

    private var deletedBanner: some View {
        HStack {
            Image(systemName: "trash").foregroundStyle(.red)
            Text("File deleted on disk. ⌘S recreates it.")
            Spacer()
        }
        .font(.system(size: 12))
        .padding(8)
        .background(.red.opacity(0.15))
    }
}
