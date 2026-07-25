import AppKit
import Observation
import Testing

@testable import Tiller

/// Guards that SwiftUI actually sees the draft change.
///
/// The bug this suite exists for: `isEmpty` was computed from
/// `storage.length`, and `@Observable` only tracks stored properties. The
/// storage is a `let` mutated inside AppKit, so typing produced no
/// observable change at all — the placeholder stayed on screen underneath
/// the text the user was typing. Accepting a slash command happened to hide
/// it only because that also writes `slashQuery`, which is stored.
@Suite("ComposerDocumentObservation")
@MainActor
struct ComposerDocumentObservationTests {
    /// `onChange` fires outside the caller's isolation, so the flag it sets
    /// lives in a reference box rather than a captured local.
    private final class Flag: @unchecked Sendable {
        var isSet = false
    }

    /// Runs `mutate`, and reports whether reading `isEmpty` was invalidated.
    private func observesChange(
        to document: ComposerDocument, when mutate: () -> Void
    ) -> Bool {
        let notified = Flag()
        withObservationTracking {
            _ = document.isEmpty
        } onChange: {
            notified.isSet = true
        }
        mutate()
        return notified.isSet
    }

    @Test func typingTextNotifiesObservers() {
        let document = ComposerDocument()
        let changed = observesChange(to: document) {
            // What the text view's delegate does on every keystroke.
            document.storage.append(NSAttributedString(string: "Ciao"))
            document.refreshQueries()
        }
        #expect(changed)
    }

    @Test func insertingAChipNotifiesObservers() {
        let document = ComposerDocument()
        let changed = observesChange(to: document) {
            document.insert(.file(path: "README.md"),
                            replacing: NSRange(location: 0, length: 0))
        }
        #expect(changed)
    }

    @Test func sendingNotifiesObservers() {
        let document = ComposerDocument()
        document.storage.append(NSAttributedString(string: "Ciao"))
        document.refreshQueries()

        let changed = observesChange(to: document) {
            _ = document.takeDraft()
        }
        #expect(changed)
    }

    @Test func isEmptyStaysTrueForAnEmptyDraft() {
        let document = ComposerDocument()
        #expect(document.isEmpty)
        document.refreshQueries()
        #expect(document.isEmpty)
    }

    @Test func isEmptyTracksTheStorageContents() {
        let document = ComposerDocument()
        document.storage.append(NSAttributedString(string: "Ciao"))
        document.refreshQueries()
        #expect(!document.isEmpty)

        _ = document.takeDraft()
        #expect(document.isEmpty)
    }
}
