import Testing
@testable import TillerACP

@Suite struct ChatPromptBuilderTests {
    @Test func buildsTextMentionsAndImagesInOrder() {
        let blocks = ChatPromptBuilder.build(
            text: "fix this",
            mentionPaths: ["App/AppModel.swift"],
            images: [ImageAttachment(mimeType: "image/png", base64Data: "aGk=")],
            worktreePath: "/w")
        #expect(blocks == [
            .text("fix this"),
            .resourceLink(uri: "file:///w/App/AppModel.swift", name: "AppModel.swift"),
            .image(mimeType: "image/png", data: "aGk="),
        ])
    }

    @Test func omitsEmptyText() {
        let blocks = ChatPromptBuilder.build(
            text: "  \n", mentionPaths: ["a.txt"], images: [], worktreePath: "/w")
        #expect(blocks == [.resourceLink(uri: "file:///w/a.txt", name: "a.txt")])
    }
}
