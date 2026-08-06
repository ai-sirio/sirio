import Testing

@testable import Tiller

@Suite("Transient messages")
@MainActor
struct TransientMessageTests {
    @Test func showingAMessagePublishesIt() {
        let model = AppModel()
        #expect(model.transientMessage == nil)
        model.showTransientMessage("Image is too large (max 10 MB)")
        #expect(model.transientMessage == "Image is too large (max 10 MB)")
    }

    @Test func aSecondMessageReplacesTheFirst() {
        let model = AppModel()
        model.showTransientMessage("first")
        model.showTransientMessage("second")
        #expect(model.transientMessage == "second")
    }
}
