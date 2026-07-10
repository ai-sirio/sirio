import Testing
import Foundation
@testable import TillerCore

private func range(of substring: String, in text: String) -> Range<String.Index> {
    text.range(of: substring)!
}

@Test func wrapBoldsSelectedText() {
    let text = "ciao mondo"
    let (out, sel) = MarkdownSyntax.wrap(text, selection: range(of: "mondo", in: text),
                                         prefix: "**", suffix: "**")
    #expect(out == "ciao **mondo**")
    #expect(String(out[sel]) == "mondo")
}

@Test func wrapWithEmptySelectionInsertsMarkersAndCursorInside() {
    let text = "ciao "
    let (out, sel) = MarkdownSyntax.wrap(text, selection: text.endIndex..<text.endIndex,
                                         prefix: "**", suffix: "**")
    #expect(out == "ciao ****")
    #expect(sel.isEmpty)
    #expect(out.distance(from: out.startIndex, to: sel.lowerBound) == 7)
}

@Test func prefixLinesAddsHeadingToCurrentLine() {
    let text = "titolo\ncorpo"
    let (out, _) = MarkdownSyntax.prefixLines(text, selection: range(of: "tit", in: text),
                                              linePrefix: "# ")
    #expect(out == "# titolo\ncorpo")
}

@Test func prefixLinesCoversEveryLineTouchedBySelection() {
    let text = "uno\ndue\ntre"
    let sel = range(of: "no\ndu", in: text)
    let (out, _) = MarkdownSyntax.prefixLines(text, selection: sel, linePrefix: "- ")
    #expect(out == "- uno\n- due\ntre")
}

@Test func wrapAsLinkPlacesSelectionOnText() {
    let text = "vedi qui"
    let (out, sel) = MarkdownSyntax.wrap(text, selection: range(of: "qui", in: text),
                                         prefix: "[", suffix: "](url)")
    #expect(out == "vedi [qui](url)")
    #expect(String(out[sel]) == "qui")
}
