import Testing
import Foundation
@testable import TillerTerminal

@Test func bufferKeepsEverythingUnderCapacity() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hello ".utf8))
    await buffer.append(Data("world".utf8))
    #expect(await buffer.snapshot() == Data("hello world".utf8))
}

@Test func bufferDropsOldestBeyondCapacity() async {
    let buffer = ScrollbackBuffer(capacity: 8)
    await buffer.append(Data("0123456789".utf8))
    #expect(await buffer.snapshot() == Data("23456789".utf8))
    await buffer.append(Data("AB".utf8))
    #expect(await buffer.snapshot() == Data("456789AB".utf8))
}
