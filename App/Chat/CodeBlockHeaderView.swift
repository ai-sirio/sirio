import AppKit
import SwiftUI
import Inject

/// The non-selectable header bar hosted above each code-block card:
/// language label on the left, copy button on the right. Overlaid as an
/// `NSHostingView` subview of `MarkdownTextView` — it is not part of the
/// text, so drag-selection flows straight through the code block (same as
/// T3 web).
struct CodeBlockHeaderView: View {
    @ObserveInjection private var inject

    let language: String
    let code: String
    @State private var copied = false

    var body: some View {
        HStack {
            Text(language)
                .font(AppFont.caption.monospaced())
                .foregroundStyle(.secondary)
            Spacer()
            Button {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(code, forType: .string)
                copied = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
            } label: {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                    .font(AppFont.system(size: 11))
                    .foregroundStyle(copied ? .green : .secondary)
            }
            .buttonStyle(.plain)
            .help("Copy code")
        }
        .padding(.horizontal, 10)
        .frame(height: CodeBlockStyle.headerHeight)
    .enableInjection()
    }
}
