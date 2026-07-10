import SwiftUI
import TillerCore

/// Toolbar essenziale della modalità codice: ogni bottone applica una
/// trasformazione pura (MarkdownSyntax) sulla selezione corrente.
struct MarkdownToolbar: View {
    @Bindable var document: MarkdownDocument
    @Binding var selection: TextSelection?

    var body: some View {
        HStack(spacing: 2) {
            button("bold", help: "Grassetto") { wrap("**", "**") }
            button("italic", help: "Corsivo") { wrap("*", "*") }
            Divider().frame(height: 14)
            button("1.square", help: "Titolo 1") { prefixLines("# ") }
            button("2.square", help: "Titolo 2") { prefixLines("## ") }
            button("3.square", help: "Titolo 3") { prefixLines("### ") }
            Divider().frame(height: 14)
            button("list.bullet", help: "Lista") { prefixLines("- ") }
            button("link", help: "Link") { wrap("[", "](url)") }
            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
    }

    private func button(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11))
                .frame(width: 22, height: 20)
        }
        .buttonStyle(.plain)
        .help(help)
    }

    /// Range della selezione corrente; cursore (o niente selezione) →
    /// range vuoto a fine testo, così i marker vengono comunque inseriti.
    private func currentRange() -> Range<String.Index> {
        if case .selection(let range) = selection?.indices {
            return range
        }
        return document.text.endIndex..<document.text.endIndex
    }

    private func wrap(_ prefix: String, _ suffix: String) {
        let (newText, newSelection) = MarkdownSyntax.wrap(
            document.text, selection: currentRange(), prefix: prefix, suffix: suffix
        )
        document.text = newText
        selection = TextSelection(range: newSelection)
    }

    private func prefixLines(_ linePrefix: String) {
        let (newText, newSelection) = MarkdownSyntax.prefixLines(
            document.text, selection: currentRange(), linePrefix: linePrefix
        )
        document.text = newText
        selection = TextSelection(range: newSelection)
    }
}
