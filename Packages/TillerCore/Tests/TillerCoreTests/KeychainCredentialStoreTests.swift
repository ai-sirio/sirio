import Testing
@testable import TillerCore
import Foundation

@Test func setThenGetRoundTrips() {
    let key = "tiller-test-roundtrip-\(UUID().uuidString)"
    defer { KeychainCredentialStore.delete(key: key) }
    #expect(KeychainCredentialStore.set(key: key, value: "secret-value") == true)
    #expect(KeychainCredentialStore.get(key: key) == "secret-value")
}

@Test func deleteRemovesValue() {
    let key = "tiller-test-delete-\(UUID().uuidString)"
    #expect(KeychainCredentialStore.set(key: key, value: "temp") == true)
    #expect(KeychainCredentialStore.delete(key: key) == true)
    #expect(KeychainCredentialStore.get(key: key) == nil)
}

@Test func getOnMissingKeyReturnsNil() {
    let key = "tiller-test-missing-\(UUID().uuidString)"
    #expect(KeychainCredentialStore.get(key: key) == nil)
}

@Test func setOverwritesExistingValue() {
    let key = "tiller-test-overwrite-\(UUID().uuidString)"
    defer { KeychainCredentialStore.delete(key: key) }
    KeychainCredentialStore.set(key: key, value: "first")
    KeychainCredentialStore.set(key: key, value: "second")
    #expect(KeychainCredentialStore.get(key: key) == "second")
}
