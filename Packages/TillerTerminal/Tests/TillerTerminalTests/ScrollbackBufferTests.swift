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

// MARK: - Reference model

/// Naive model of the ring buffer: keep everything, expose the last
/// `capacity` bytes. Slow but obviously correct — used to diff the real
/// implementation byte for byte.
private struct ReferenceScrollback {
    let capacity: Int
    private var all = Data()

    init(capacity: Int) { self.capacity = capacity }

    mutating func append(_ data: Data) {
        guard capacity > 0 else { return }
        all.append(data)
        if all.count > capacity {
            all = Data(all.suffix(capacity))
        }
    }

    func snapshot() -> Data { all }

    func tail(_ maxBytes: Int) -> Data {
        guard maxBytes > 0 else { return Data() }
        return Data(all.suffix(maxBytes))
    }
}

/// Deterministic generator so the differential test is reproducible.
private struct SeededGenerator: RandomNumberGenerator {
    private var state: UInt64

    init(seed: UInt64) { state = seed }

    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}

@Test func ringBufferMatchesReferenceModelAcrossRandomAppends() async {
    let capacity = 64
    let buffer = ScrollbackBuffer(capacity: capacity)
    var reference = ReferenceScrollback(capacity: capacity)
    var generator = SeededGenerator(seed: 0x5EED_1234)
    var nextByte: UInt8 = 0

    for _ in 0..<300 {
        let size = Int.random(in: 0...200, using: &generator)
        var chunk = Data(capacity: size)
        for _ in 0..<size {
            chunk.append(nextByte)
            nextByte &+= 1
        }
        await buffer.append(chunk)
        reference.append(chunk)

        #expect(await buffer.snapshot() == reference.snapshot())
        for window in [1, 7, 63, 64, 65, 1000] {
            #expect(await buffer.tail(window) == reference.tail(window))
        }
    }
}

@Test func ringBufferAppendSizesNotDivisibleByCapacity() async {
    let capacity = 10
    let buffer = ScrollbackBuffer(capacity: capacity)
    var reference = ReferenceScrollback(capacity: capacity)
    var nextByte: UInt8 = 0

    for _ in 0..<50 {
        var chunk = Data()
        for _ in 0..<3 {
            chunk.append(nextByte)
            nextByte &+= 1
        }
        await buffer.append(chunk)
        reference.append(chunk)
        #expect(await buffer.snapshot() == reference.snapshot())
    }
}

@Test func ringBufferPreservesSentinelOrderAfterHeavyOutput() async {
    let capacity = 1024
    let buffer = ScrollbackBuffer(capacity: capacity)
    await buffer.append(Data("<<START>>".utf8))
    for line in 1...5000 {
        await buffer.append(Data("line \(line)\n".utf8))
    }
    await buffer.append(Data("<<END>>".utf8))

    let snapshot = await buffer.snapshot()
    #expect(snapshot.count == capacity)
    let text = String(decoding: snapshot, as: UTF8.self)
    #expect(text.hasSuffix("<<END>>"))
    #expect(!text.contains("<<START>>"))

    // Line numbers still appear in ascending order, with no gaps, in the
    // window that survived.
    let numbers = text
        .split(separator: "\n")
        .compactMap { segment -> Int? in
            guard segment.hasPrefix("line ") else { return nil }
            return Int(segment.dropFirst("line ".count))
        }
    #expect(numbers.count > 1)
    #expect(numbers == Array(numbers.sorted()))
    #expect(zip(numbers, numbers.dropFirst()).allSatisfy { $1 == $0 + 1 })
}
