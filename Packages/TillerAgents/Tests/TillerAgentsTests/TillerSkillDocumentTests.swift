import Testing
@testable import TillerAgents

@Test func skillDocumentInvariants() {
    let doc = TillerSkillDocument.markdown
    #expect(doc.hasPrefix("---\n"))                 // frontmatter opens the file
    #expect(doc.contains("name: tiller"))
    #expect(doc.contains("TILLER_ENV"))                // env guard documented
    #expect(doc.contains("$TILLER_PANE_ID"))           // self-identity for notify
    #expect(doc.contains("$TILLER_WORKTREE_ID"))       // identity for panel create
    #expect(doc.contains("Machine-managed by Tiller")) // overwrite warning header
    #expect(doc.contains("tillerctl panel create"))
    #expect(doc.contains("tillerctl panel wait"))
    #expect(doc.contains("tillerctl worktree set"))
}
