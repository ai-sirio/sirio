import Testing
@testable import TillerACP

@Suite("ModelPickerFilter")
struct ModelPickerFilterTests {
    let models = [
        ModelInfo(modelId: "claude-opus-4-8", name: "Opus 4.8",
                  description: "Deepest reasoning"),
        ModelInfo(modelId: "claude-sonnet-5", name: "Sonnet 5",
                  description: "Best coding model"),
        ModelInfo(modelId: "claude-haiku-4-5", name: "Haiku 4.5", description: nil),
    ]

    @Test func emptyQueryReturnsAllInOrder() {
        #expect(ModelPickerFilter.filter(models, query: "").map(\.modelId)
                == models.map(\.modelId))
        #expect(ModelPickerFilter.filter(models, query: "   ").map(\.modelId)
                == models.map(\.modelId))
    }

    @Test func matchesNameCaseInsensitively() {
        #expect(ModelPickerFilter.filter(models, query: "opus").map(\.modelId)
                == ["claude-opus-4-8"])
    }

    @Test func matchesModelIdAndDescription() {
        #expect(ModelPickerFilter.filter(models, query: "haiku-4").map(\.modelId)
                == ["claude-haiku-4-5"])
        #expect(ModelPickerFilter.filter(models, query: "coding").map(\.modelId)
                == ["claude-sonnet-5"])
    }

    @Test func noMatchReturnsEmpty() {
        #expect(ModelPickerFilter.filter(models, query: "gemini").isEmpty)
    }
}
