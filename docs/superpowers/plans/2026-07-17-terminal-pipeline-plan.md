# Terminal Pipeline — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

## Goal

Replace the O(n) `Data.removeFirst` scrollback storage with a correct fixed-capacity byte ring buffer using explicit `head` + `count` semantics, add a `tail(_:)` method for bounded content-signal extraction, and replace the per-event PTY read buffer allocation with a reusable stack buffer. All changes are Swift 6 actor-isolated and maintain the existing public API.

## Architecture

### Ring Buffer Design

The ring buffer uses a fixed-capacity `Data` storage array with `head` (oldest byte index) and `count` (number of stored bytes) semantics. This avoids the `Data.removeFirst` O(n) shift entirely.

```
State: storage (fixed capacity), head (0..capacity), count (0..capacity)

Append(data):
  For each byte in data:
    storage[(head + count) % capacity] = byte
    count += 1
    if count > capacity: head = (head + 1) % capacity; count = capacity

Snapshot():
  If count == 0: return Data()
  If head + count <= capacity: return storage[head..<head+count]
  Else: return storage[head..<] + storage[0..<(head+count)%capacity]

Tail(maxBytes):
  Let n = min(maxBytes, count)
  Start at (head + count - n) % capacity
  Read n bytes in logical order (oldest-to-newest within the window)
```

### PTY Read Buffer

Replace the per-event `var buffer = [UInt8](repeating: 0, count: 64 * 1024)` with a stored property `private var readBuffer = [UInt8](repeating: 0, count: 64 * 1024)` allocated once per `PtyProcess` instance. The `DispatchSourceRead` handler already runs on a serial queue, so a single reusable buffer is safe.

## Tech Stack

- Swift 6, macOS 15+
- `swift-testing` (`@Test` / `#expect`)
- `Scripts/ci.sh` as verification gate
- No new dependencies

## Global Constraints

- Public API unchanged: `actor ScrollbackBuffer { init(capacity:), append(_:), snapshot() }` plus new `tail(_:)`.
- `tail(_:)` returns bytes in logical oldest→newest order within the returned window.
- Capacity-zero behavior: if `capacity == 0`, `append` is a no-op, `snapshot()` returns empty `Data`, `tail(_:)` returns empty `Data`.
- No bytes lost except deterministic oldest-byte eviction at capacity.
- UTF-8 sequences and ANSI escape codes spanning chunk boundaries are preserved as raw bytes (decoding is the caller's responsibility).
- `Scripts/ci.sh` must print `CI OK`.

---

## Interfaces

### Consumed

```swift
// ScrollbackBuffer (TillerTerminal)
public actor ScrollbackBuffer {
    public init(capacity: Int = 256 * 1024)
    public func append(_ data: Data)
    public func snapshot() -> Data
}

// PtyProcess (TillerTerminal)
public init(onOutput: @escaping @Sendable (Data) -> Void)
private func startReadLoop()

// PtyRuntime.emitContentSignal() (TillerTerminal)
private func emitContentSignal() async
```

### Produced

```swift
// ScrollbackBuffer additions:
public func tail(_ maxBytes: Int) -> Data

// PtyProcess additions:
private var readBuffer: [UInt8]

// PtyRuntime.emitContentSignal() change:
// Use scrollback.tail(10 * 1024) instead of scrollback.snapshot()
```

---

## Tasks

### Task 1: Write ring buffer tests (before implementation)

**File:** `Packages/TillerTerminal/Tests/TillerTerminalTests/ScrollbackBufferTests.swift` (append)

All tests use `swift-testing` (`@Test` / `#expect`). Each test is independent and verifies a specific ring buffer behavior.

**Step 1.1 — Basic append and snapshot under capacity:**

```swift
@Test func ringBufferPreservesExactContentUnderCapacity() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hello ".utf8))
    await buffer.append(Data("world".utf8))
    #expect(await buffer.snapshot() == Data("hello world".utf8))
}
```

**Step 1.2 — Drop oldest beyond capacity:**

```swift
@Test func ringBufferDropsOldestBeyondCapacity() async {
    let buffer = ScrollbackBuffer(capacity: 8)
    await buffer.append(Data("0123456789".utf8))
    #expect(await buffer.snapshot() == Data("23456789".utf8))
    await buffer.append(Data("AB".utf8))
    #expect(await buffer.snapshot() == Data("456789AB".utf8))
}
```

**Step 1.3 — Exact capacity write:**

```swift
@Test func ringBufferHandlesExactCapacityWrite() async {
    let buffer = ScrollbackBuffer(capacity: 10)
    await buffer.append(Data("0123456789".utf8))
    #expect(await buffer.snapshot() == Data("0123456789".utf8))
}
```

**Step 1.4 — Over-capacity single write:**

```swift
@Test func ringBufferHandlesOverCapacitySingleWrite() async {
    let buffer = ScrollbackBuffer(capacity: 10)
    await buffer.append(Data("0123456789ABCDE".utf8))
    #expect(await buffer.snapshot() == Data("56789ABCDE".utf8))
}
```

**Step 1.5 — Wrap-around behavior:**

```swift
@Test func ringBufferHandlesWrapBoundary() async {
    let buffer = ScrollbackBuffer(capacity: 16)
    await buffer.append(Data("AAAAAAAAAAAAAAAA".utf8))
    await buffer.append(Data("BBBB".utf8))
    let snap = await buffer.snapshot()
    #expect(snap.count == 16)
    #expect(snap.prefix(12) == Data("AAAAAAAAAAAA".utf8))
    #expect(snap.suffix(4) == Data("BBBB".utf8))
}
```

**Step 1.6 — `tail(_:)` returns last N bytes:**

```swift
@Test func ringBufferTailReturnsLastNBytes() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hello world foo bar baz".utf8))
    let tail = await buffer.tail(8)
    #expect(String(decoding: tail, as: UTF8.self) == "oo bar baz")
}
```

**Step 1.7 — `tail(_:)` returns all when shorter than N:**

```swift
@Test func ringBufferTailReturnsAllWhenShorterThanN() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hi".utf8))
    let tail = await buffer.tail(100)
    #expect(String(decoding: tail, as: UTF8.self) == "hi")
}
```

**Step 1.8 — `tail(_:)` across wrap boundary:**

```swift
@Test func ringBufferTailAcrossWrapBoundary() async {
    let buffer = ScrollbackBuffer(capacity: 16)
    await buffer.append(Data("AAAAAAAAAAAAAAAA".utf8))
    await buffer.append(Data("BBBB".utf8))
    let tail = await buffer.tail(8)
    #expect(tail.count == 8)
    #expect(String(decoding: tail, as: UTF8.self) == "AAAABBBB")
}
```

**Step 1.9 — Empty tail:**

```swift
@Test func ringBufferEmptyTail() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    #expect(await buffer.tail(100).isEmpty)
}
```

**Step 1.10 — Append empty data is no-op:**

```swift
@Test func ringBufferAppendEmptyData() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hello".utf8))
    await buffer.append(Data())
    #expect(await buffer.snapshot() == Data("hello".utf8))
}
```

**Step 1.11 — UTF-8 preservation (multi-byte characters):**

```swift
@Test func ringBufferUtf8Preservation() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    let text = "héllo wörld 🎉"
    await buffer.append(Data(text.utf8))
    let decoded = String(decoding: await buffer.snapshot(), as: UTF8.self)
    #expect(decoded == text)
}
```

**Step 1.12 — UTF-8 split across capacity boundary:**

```swift
@Test func ringBufferUtf8AcrossCapacityBoundary() async {
    let buffer = ScrollbackBuffer(capacity: 10)
    await buffer.append(Data("héllo".utf8))
    #expect(await buffer.snapshot() == Data("héllo".utf8))
    await buffer.append(Data("wörld".utf8))
    let snap = await buffer.snapshot()
    let decoded = String(decoding: snap, as: UTF8.self)
    // "héllo" (7 bytes) + "wörld" (7 bytes) = 14 bytes, capacity 10
    // Last 10 bytes should be preserved. Exact content depends on UTF-8 encoding.
    #expect(snap.count == 10)
}
```

**Step 1.13 — ANSI sequences preserved as raw bytes:**

```swift
@Test func ringBufferAnsiSequencesPreserved() async {
    let buffer = ScrollbackBuffer(capacity: 1000)
    let ansiText = "\u{1B}[32mhello\u{1B}[0m\n\u{1B}[31mworld\u{1B}[0m\n"
    await buffer.append(Data(ansiText.utf8))
    let snap = String(decoding: await buffer.snapshot(), as: UTF8.self)
    #expect(snap == ansiText)
}
```

**Step 1.14 — ANSI sequence split across two appends:**

```swift
@Test func ringBufferChunkBoundaryAnsiSplit() async {
    let buffer = ScrollbackBuffer(capacity: 1000)
    await buffer.append(Data("\u{1B}[3".utf8))
    await buffer.append(Data("2mhello\u{1B}[0m".utf8))
    let snap = String(decoding: await buffer.snapshot(), as: UTF8.self)
    #expect(snap == "\u{1B}[32mhello\u{1B}[0m")
}
```

**Step 1.15 — Repeated append after wrap:**

```swift
@Test func ringBufferRepeatedAppendAfterWrap() async {
    let buffer = ScrollbackBuffer(capacity: 16)
    for _ in 0..<10 {
        await buffer.append(Data("ABCD".utf8))
    }
    let snap = await buffer.snapshot()
    #expect(snap.count == 16)
    #expect(String(decoding: snap, as: UTF8.self) == "ABCDABCDABCDABCD")
}
```

**Step 1.16 — Zero capacity:**

```swift
@Test func ringBufferZeroCapacity() async {
    let buffer = ScrollbackBuffer(capacity: 0)
    await buffer.append(Data("hello".utf8))
    #expect(await buffer.snapshot().isEmpty)
    #expect(await buffer.tail(10).isEmpty)
}
```

**Step 1.17 — Oversized tail request:**

```swift
@Test func ringBufferOversizedTail() async {
    let buffer = ScrollbackBuffer(capacity: 100)
    await buffer.append(Data("hello world".utf8))
    let tail = await buffer.tail(10000)
    #expect(tail == Data("hello world".utf8))
}
```

**Verification:**
```bash
cd Packages/TillerTerminal && swift test --filter ScrollbackBufferTests
```
Expected: all existing tests fail (they use the old `Data`-backed implementation). The new implementation will make them pass.

---

### Task 2: Implement ring buffer

**File:** `Packages/TillerTerminal/Sources/TillerTerminal/ScrollbackBuffer.swift`

Full replacement of the current 22-line file:

```swift
import Foundation

/// Fixed-capacity byte ring buffer: appends raw PTY output and keeps only
/// the last `capacity` bytes. Uses explicit `head` + `count` semantics into
/// a fixed `storage` array, avoiding `Data.removeFirst()` O(n) shifts.
///
/// Thread safety: actor isolation (all methods are async).
public actor ScrollbackBuffer {
    private var storage: Data
    private let capacity: Int
    private var head: Int = 0
    private var count: Int = 0

    public init(capacity: Int = 256 * 1024) {
        self.capacity = capacity
        self.storage = Data(count: capacity)
    }

    public func append(_ data: Data) {
        guard capacity > 0, !data.isEmpty else { return }
        let bytes = data
        if bytes.count >= capacity {
            // Data larger than capacity: keep only the last `capacity` bytes.
            let offset = bytes.count - capacity
            // Safety: withUnsafeMutableBytes on actor-isolated `storage` is safe
            // because the actor serializes access and the closure is synchronous.
            bytes.withUnsafeBytes { src in
                storage.withUnsafeMutableBytes { dst in
                    dst.copyBytes(from: UnsafeRawBufferPointer(rebasing: src[offset...]))
                }
            }
            head = 0
            count = capacity
            return
        }
        // Normal case: copy into ring.
        for byte in bytes {
            let idx = (head + count) % capacity
            storage[idx] = byte
            if count < capacity {
                count += 1
            } else {
                head = (head + 1) % capacity
            }
        }
    }

    public func snapshot() -> Data {
        guard count > 0 else { return Data() }
        if head + count <= capacity {
            return Data(storage[head..<head + count])
        }
        let first = storage[head..<capacity]
        let second = storage[0..<(head + count) % capacity]
        return first + second
    }

    /// Returns the last `maxBytes` bytes as a single `Data` in logical
    /// oldest-to-newest order within the returned window, or fewer if the
    /// buffer contains less data.
    public func tail(_ maxBytes: Int) -> Data {
        guard count > 0, maxBytes > 0 else { return Data() }
        let n = min(maxBytes, count)
        let start = (head + count - n) % capacity
        if start + n <= capacity {
            return Data(storage[start..<start + n])
        }
        let first = storage[start..<capacity]
        let second = storage[0..<(start + n) % capacity]
        return first + second
    }
}
```

**Verification:**
```bash
cd Packages/TillerTerminal && swift test --filter ScrollbackBufferTests
```
Expected: all 17 new tests pass, plus the 2 existing tests (`bufferKeepsEverythingUnderCapacity`, `bufferDropsOldestBeyondCapacity`).

---

### Task 3: Implement reusable PTY read buffer

**File:** `Packages/TillerTerminal/Sources/TillerTerminal/PtyProcess.swift`

**Step 3.1 — Add stored property (after line 38):**

```
oldString:     private let queue = DispatchQueue(label: "tiller.pty.read")
newString:     private let queue = DispatchQueue(label: "tiller.pty.read")
    private var readBuffer = [UInt8](repeating: 0, count: 64 * 1024)
```

**Step 3.2 — Replace per-event allocation in `startReadLoop` (lines 157-166):**

```
oldString:         source.setEventHandler { [weak self] in
            guard let self, self.masterFD >= 0 else { return }
            var buffer = [UInt8](repeating: 0, count: 64 * 1024)
            let n = read(self.masterFD, &buffer, buffer.count)
            if n > 0 {
                self.onOutput(Data(buffer[0..<n]))
            } else {
                self.stopReadLoop()
                self.reapChildWithRetry()
            }
        }
newString:         source.setEventHandler { [weak self] in
            guard let self, self.masterFD >= 0 else { return }
            let n = read(self.masterFD, &self.readBuffer, self.readBuffer.count)
            if n > 0 {
                self.onOutput(Data(self.readBuffer[0..<n]))
            } else {
                self.stopReadLoop()
                self.reapChildWithRetry()
            }
        }
```

**Verification:**
```bash
cd Packages/TillerTerminal && swift test --filter PtyProcessTests
```
Expected: all 8 existing tests pass unchanged.

---

### Task 4: Use bounded tail in content signal

**File:** `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`, lines 219-226

**Edit:**
```
oldString:     private func emitContentSignal() async {
        let data = await scrollback.snapshot()
        let text = stripANSI(String(decoding: data, as: UTF8.self))
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        let tail = lines.suffix(40).joined(separator: "\n")
        guard !tail.isEmpty else { return }
        onContentSignal?(paneId, tail)
    }
newString:     private func emitContentSignal() async {
        let tailData = await scrollback.tail(10 * 1024)
        guard !tailData.isEmpty else { return }
        let text = stripANSI(String(decoding: tailData, as: UTF8.self))
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        let tail = lines.suffix(40).joined(separator: "\n")
        guard !tail.isEmpty else { return }
        onContentSignal?(paneId, tail)
    }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 5: Verify no-loss with before/after profile

**Step 5.1 — Enable signposts and capture baseline:**
```bash
defaults write dev.tiller debug.signpostMetrics -bool YES
# Launch Tiller, profile with Instruments (os_signpost template)
# Record ptyIngest, scrollbackAppend, scrollbackSnapshot, contentSignal intervals
```

**Step 5.2 — Run automated no-loss verification:**
```bash
cd Packages/TillerTerminal && swift test --filter ScrollbackBufferTests
```
Expected: all tests pass, confirming byte-level correctness.

**Step 5.3 — Compare `contentSignal` interval duration:**
- Before (old `snapshot()`): ~256 KB decoded per settle
- After (new `tail(10*1024)`): ~10 KB decoded per settle
- Expected: ~96% reduction in bytes decoded per settle

**Step 5.4 — Disable signposts:**
```bash
defaults write dev.tiller debug.signpostMetrics -bool NO
```

---

## Acceptance Criteria

| AC | Verification |
|----|-------------|
| AC2 | ≥ 20% terminal-pipeline CPU reduction under heavy output (measured via `os_signpost` `ptyIngest` + `contentSignal` intervals) |
| AC6 | No progressive RAM growth after 10× pane open/close (ring buffer uses at most `capacity + 1` bytes) |
| AC7 | No lost bytes — automated `ScrollbackBuffer` byte-level tests pass (17 tests covering UTF-8, ANSI, wrap, overflow, zero/oversized tail, repeated append) |
| AC12 | `Scripts/ci.sh` prints `CI OK` |

## Commit Messages

```bash
# After Task 1 (tests written, expected to fail):
git add Packages/TillerTerminal/Tests/TillerTerminalTests/ScrollbackBufferTests.swift
git commit -m "test: add ring buffer correctness tests for ScrollbackBuffer"

# After Task 2:
git add Packages/TillerTerminal/Sources/TillerTerminal/ScrollbackBuffer.swift
git commit -m "perf: replace Data.removeFirst with O(1) byte ring buffer in ScrollbackBuffer"

# After Task 3:
git add Packages/TillerTerminal/Sources/TillerTerminal/PtyProcess.swift
git commit -m "perf: reuse PTY read buffer instead of per-event allocation"

# After Task 4:
git add Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift
git commit -m "perf: use bounded tail extraction in content-signal path"
```
