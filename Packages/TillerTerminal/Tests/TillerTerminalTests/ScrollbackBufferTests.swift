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

@Test func ringBufferHandlesExactCapacityWrite() async {
    let buffer = ScrollbackBuffer(capacity: 10)
    await buffer.append(Data("0123456789".utf8))
    #expect(await buffer.snapshot() == Data("0123456789".utf8))
}

@Test func ringBufferHandlesOverCapacitySingleWrite() async {
    let buffer = ScrollbackBuffer(capacity: 10)
    await buffer.append(Data("0123456789ABCDE".utf8))
    #expect(await buffer.snapshot() == Data("56789ABCDE".utf8))
}

@Test func ringBufferHandlesWrapBoundary() async {
    let buffer = ScrollbackBuffer(capacity: 16)
    await buffer.append(Data("AAAAAAAAAAAAAAAA".utf8))
    await buffer.append(Data("BBBB".utf8))
    let snap = await buffer.snapshot()
    #expect(snap.count == 16)
    #expect(snap.prefix(12) == Data("AAAAAAAAAAAA".utf8))
    #expect(snap.suffix(4) == Data("BBBB".utf8))
}

@Test func ringBufferTailReturnsLastNBytes() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hello world foo bar baz".utf8))
    let tail = await buffer.tail(8)
    #expect(String(decoding: tail, as: UTF8.self) == " bar baz")
}

@Test func ringBufferTailReturnsAllWhenShorterThanN() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hi".utf8))
    let tail = await buffer.tail(100)
    #expect(String(decoding: tail, as: UTF8.self) == "hi")
}

@Test func ringBufferTailAcrossWrapBoundary() async {
    let buffer = ScrollbackBuffer(capacity: 16)
    await buffer.append(Data("AAAAAAAAAAAAAAAA".utf8))
    await buffer.append(Data("BBBB".utf8))
    let tail = await buffer.tail(8)
    #expect(tail.count == 8)
    #expect(String(decoding: tail, as: UTF8.self) == "AAAABBBB")
}

@Test func ringBufferEmptyTail() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    #expect(await buffer.tail(100).isEmpty)
}

@Test func ringBufferAppendEmptyData() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hello".utf8))
    await buffer.append(Data())
    #expect(await buffer.snapshot() == Data("hello".utf8))
}

@Test func ringBufferUtf8Preservation() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    let text = "héllo wörld 🎉"
    await buffer.append(Data(text.utf8))
    let decoded = String(decoding: await buffer.snapshot(), as: UTF8.self)
    #expect(decoded == text)
}

@Test func ringBufferUtf8AcrossCapacityBoundary() async {
    let buffer = ScrollbackBuffer(capacity: 10)
    await buffer.append(Data("héllo".utf8))
    #expect(await buffer.snapshot() == Data("héllo".utf8))
    await buffer.append(Data("wörld".utf8))
    let snap = await buffer.snapshot()
    // "héllo" (6 bytes) + "wörld" (6 bytes) = 12 bytes, capacity 10:
    // last 10 bytes preserved as raw bytes (decoding is caller's concern).
    #expect(snap.count == 10)
}

@Test func ringBufferAnsiSequencesPreserved() async {
    let buffer = ScrollbackBuffer(capacity: 1000)
    let ansiText = "\u{1B}[32mhello\u{1B}[0m\n\u{1B}[31mworld\u{1B}[0m\n"
    await buffer.append(Data(ansiText.utf8))
    let snap = String(decoding: await buffer.snapshot(), as: UTF8.self)
    #expect(snap == ansiText)
}

@Test func ringBufferChunkBoundaryAnsiSplit() async {
    let buffer = ScrollbackBuffer(capacity: 1000)
    await buffer.append(Data("\u{1B}[3".utf8))
    await buffer.append(Data("2mhello\u{1B}[0m".utf8))
    let snap = String(decoding: await buffer.snapshot(), as: UTF8.self)
    #expect(snap == "\u{1B}[32mhello\u{1B}[0m")
}

@Test func ringBufferRepeatedAppendAfterWrap() async {
    let buffer = ScrollbackBuffer(capacity: 16)
    for _ in 0..<10 {
        await buffer.append(Data("ABCD".utf8))
    }
    let snap = await buffer.snapshot()
    #expect(snap.count == 16)
    #expect(String(decoding: snap, as: UTF8.self) == "ABCDABCDABCDABCD")
}

@Test func ringBufferZeroCapacity() async {
    let buffer = ScrollbackBuffer(capacity: 0)
    await buffer.append(Data("hello".utf8))
    #expect(await buffer.snapshot().isEmpty)
    #expect(await buffer.tail(10).isEmpty)
}

@Test func ringBufferOversizedTail() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hello world".utf8))
    let tail = await buffer.tail(10000)
    #expect(tail == Data("hello world".utf8))
}
