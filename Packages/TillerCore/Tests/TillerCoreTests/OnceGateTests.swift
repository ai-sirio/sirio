import Testing
@testable import TillerCore

@Test @MainActor func fireOnceRunsBodyOnlyFirstTime() {
    let gate = OnceGate()
    var count = 0
    gate.fireOnce { count += 1 }
    gate.fireOnce { count += 1 }
    gate.fireOnce { count += 1 }
    #expect(count == 1)
}

@Test @MainActor func fireOnceRunsFirstBodyNotLater() {
    let gate = OnceGate()
    var winner = ""
    gate.fireOnce { winner = "flush" }
    gate.fireOnce { winner = "deadline" }
    #expect(winner == "flush")
}
