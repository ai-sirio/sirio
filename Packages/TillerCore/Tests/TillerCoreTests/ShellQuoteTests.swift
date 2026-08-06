import Testing
@testable import TillerCore

@Test func wrapsPlainStringsInSingleQuotes() {
    #expect(shellQuote("/tmp/file.txt") == "'/tmp/file.txt'")
}

@Test func escapesEmbeddedSingleQuotes() {
    #expect(shellQuote("it's here") == "'it'\\''s here'")
}

@Test func quotesTheEmptyStringIntoAnEmptyShellWord() {
    #expect(shellQuote("") == "''")
}
