# File & Image Drag and Drop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drag files and images from Finder or the Files sidebar into the chat composer (as chips) and into terminal panes (as shell-quoted paths).

**Architecture:** One pure classifier in `TillerCore` (`FileDrop`) decides what a dropped file is; every destination calls it, so no entry point can bypass the rules. The chat side turns the result into `ComposerChip`s the existing `ComposerDraft.parse` already understands; the terminal side turns it into a string written to the PTY through `PaneRegistry.write`. Views only do I/O and presentation.

**Tech Stack:** Swift 6, SwiftUI + AppKit, swift-testing (`@Test` / `#expect`), SPM packages (`TillerCore`, `TillerACP`, `TillerAgents`, `TillerTerminal`), libghostty via `GhosttyTerminal`.

**Spec:** `docs/superpowers/specs/2026-08-06-file-drag-drop-design.md`

## Global Constraints

- Tests first, using swift-testing (`@Test` / `#expect`), never XCTest.
- All user-facing UI strings in English, even though this plan is written in Italian.
- Commit messages follow Conventional Commits, lower-case imperative subject.
- Domain types are structs/enums; classes only for real identity.
- `Scripts/ci.sh` must print `CI OK` before the work is considered done. Run it once at the end (Task 11), not per task.
- Image size cap: `10 * 1_024 * 1_024` bytes, exposed as `FileDrop.maxImageBytes`.
- Image extensions, matched case-insensitively: `png`, `jpg`, `jpeg`, `gif`, `webp`.
- Never copy dropped files anywhere; never execute anything in a terminal on drop.
- Package dependencies flow one way: `TillerCore` must not import `TillerACP`, `TillerAgents`, `TillerTerminal`, or anything in `App/`.
- Files are regenerated into the Xcode project by `xcodegen generate`; new files under `App/` need it before building in Xcode.

---

### Task 1: Canonical `shellQuote` in TillerCore

`TillerCore.FileDrop` needs POSIX shell quoting, and `TillerCore` cannot import `TillerAgents`, where the only implementation lives today. Move the implementation down, leave a forwarding shim behind so the six adapters that call it stay untouched.

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/ShellQuote.swift`
- Create: `Packages/TillerCore/Tests/TillerCoreTests/ShellQuoteTests.swift`
- Modify: `Packages/TillerAgents/Sources/TillerAgents/ShellQuote.swift:1-7`

**Interfaces:**
- Consumes: nothing.
- Produces: `public func shellQuote(_ s: String) -> String` in `TillerCore`.

- [ ] **Step 1: Write the failing test**

Create `Packages/TillerCore/Tests/TillerCoreTests/ShellQuoteTests.swift`:

```swift
import Testing
@testable import TillerCore

@Test func wrapsPlainStringsInSingleQuotes() {
    #expect(shellQuote("/tmp/file.txt") == "'/tmp/file.txt'")
}

@Test func escapesEmbeddedSingleQuotes() {
    #expect(shellQuote("it's here") == "'it'\\''s here'")
}

@Test func quotesTheEmptyStringIntoAnEmptyShellWord() {
    #expect(shellQuote("") == "''")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter ShellQuote`
Expected: FAIL — `cannot find 'shellQuote' in scope`.

- [ ] **Step 3: Write minimal implementation**

Create `Packages/TillerCore/Sources/TillerCore/ShellQuote.swift`:

```swift
import Foundation

/// POSIX single-quote escaping: wraps in ' and replaces every embedded
/// ' with '\'' so the result is one shell word under sh/zsh/bash.
///
/// Lives here rather than in TillerAgents because `FileDrop` needs it too
/// and TillerCore sits below TillerAgents in the dependency graph.
public func shellQuote(_ s: String) -> String {
    "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter ShellQuote`
Expected: PASS, 3 tests.

- [ ] **Step 5: Replace the TillerAgents implementation with a shim**

Replace the first seven lines of `Packages/TillerAgents/Sources/TillerAgents/ShellQuote.swift` (everything above `/// JSON string literal`) with:

```swift
import Foundation
import TillerCore

/// Forwards to the canonical implementation in TillerCore. Kept as an
/// internal function with the same name so the six adapters that call
/// `shellQuote` need no import and no edit.
func shellQuote(_ s: String) -> String { TillerCore.shellQuote(s) }
```

- [ ] **Step 6: Run the TillerAgents suite to verify nothing moved under the adapters**

Run: `cd Packages/TillerAgents && swift test`
Expected: PASS, no compile errors in `ClaudeCodeAdapter`, `ClaudeHookMigrator`, `CodexAdapter`, `OhMyPiAdapter`, `OpenCodeAdapter`, `PiAdapter`.

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/ShellQuote.swift \
        Packages/TillerCore/Tests/TillerCoreTests/ShellQuoteTests.swift \
        Packages/TillerAgents/Sources/TillerAgents/ShellQuote.swift
git commit -m "refactor: move shellQuote to TillerCore"
```

---

### Task 2: `FileDrop` classifier

The single rule engine: what a dropped URL becomes, and what a terminal insertion string looks like.

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/FileDrop.swift`
- Create: `Packages/TillerCore/Tests/TillerCoreTests/FileDropTests.swift`

**Interfaces:**
- Consumes: `TillerCore.shellQuote(_:)` (Task 1).
- Produces:
  - `public enum DroppedItem: Equatable, Sendable` with cases `.image(url: URL, mimeType: String)`, `.file(path: String)`, `.rejected(url: URL, reason: DropRejection)`
  - `public enum DropRejection: Equatable, Sendable` with case `.imageTooLarge(byteCount: Int)`
  - `public enum FileDrop` with `public static let maxImageBytes: Int`, `public static func classify(_ inputs: [(url: URL, byteCount: Int)], worktreePath: String) -> [DroppedItem]`, `public static func terminalInsertion(_ urls: [URL]) -> String`

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerCore/Tests/TillerCoreTests/FileDropTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerCore

private func fileURL(_ path: String) -> URL { URL(fileURLWithPath: path) }

@Test func recognisesTheFiveImageTypesUnderTheSizeCap() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo/a.png"), byteCount: 1_024),
        (url: fileURL("/repo/b.jpg"), byteCount: 1_024),
        (url: fileURL("/repo/c.jpeg"), byteCount: 1_024),
        (url: fileURL("/repo/d.gif"), byteCount: 1_024),
        (url: fileURL("/repo/e.webp"), byteCount: 1_024),
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo") == [
        .image(url: fileURL("/repo/a.png"), mimeType: "image/png"),
        .image(url: fileURL("/repo/b.jpg"), mimeType: "image/jpeg"),
        .image(url: fileURL("/repo/c.jpeg"), mimeType: "image/jpeg"),
        .image(url: fileURL("/repo/d.gif"), mimeType: "image/gif"),
        .image(url: fileURL("/repo/e.webp"), mimeType: "image/webp"),
    ])
}

@Test func matchesImageExtensionsCaseInsensitively() {
    let inputs: [(url: URL, byteCount: Int)] = [(url: fileURL("/repo/A.PNG"), byteCount: 10)]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.image(url: fileURL("/repo/A.PNG"), mimeType: "image/png")])
}

@Test func rejectsImagesOverTheSizeCap() {
    let tooBig = FileDrop.maxImageBytes + 1
    let inputs: [(url: URL, byteCount: Int)] = [(url: fileURL("/repo/big.png"), byteCount: tooBig)]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.rejected(url: fileURL("/repo/big.png"),
                          reason: .imageTooLarge(byteCount: tooBig))])
}

@Test func acceptsAnImageExactlyAtTheCap() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo/edge.png"), byteCount: FileDrop.maxImageBytes)
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.image(url: fileURL("/repo/edge.png"), mimeType: "image/png")])
}

@Test func treatsNonImagesAndFoldersAsFilesWhateverTheirSize() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo/notes.txt"), byteCount: FileDrop.maxImageBytes * 3),
        (url: fileURL("/repo/Sources"), byteCount: 0),
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.file(path: "notes.txt"), .file(path: "Sources")])
}

@Test func relativisesFilesInsideTheWorktree() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo/Sources/App.swift"), byteCount: 10)
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.file(path: "Sources/App.swift")])
}

@Test func keepsAbsolutePathsForFilesOutsideTheWorktree() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/Users/me/Downloads/notes.txt"), byteCount: 10)
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.file(path: "/Users/me/Downloads/notes.txt")])
}

@Test func doesNotTreatASiblingSharingAPrefixAsInsideTheWorktree() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo-backup/notes.txt"), byteCount: 10)
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.file(path: "/repo-backup/notes.txt")])
}

@Test func quotesEveryTerminalPathAndJoinsThemWithSpaces() {
    #expect(FileDrop.terminalInsertion([fileURL("/tmp/a b.txt"), fileURL("/tmp/it's.txt")])
            == "'/tmp/a b.txt' '/tmp/it'\\''s.txt'")
}

@Test func neverEndsATerminalInsertionWithANewline() {
    #expect(FileDrop.terminalInsertion([fileURL("/tmp/a.txt")]).contains("\n") == false)
}

@Test func producesAnEmptyTerminalInsertionForNoURLs() {
    #expect(FileDrop.terminalInsertion([]) == "")
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter FileDrop`
Expected: FAIL — `cannot find 'FileDrop' in scope`.

- [ ] **Step 3: Write minimal implementation**

Create `Packages/TillerCore/Sources/TillerCore/FileDrop.swift`:

```swift
import Foundation

/// What a file dropped on Tiller turned out to be. The caller does the I/O
/// (reading the bytes of an `.image`) and the presentation (a chip, a shell
/// word, a message); this type carries the decision only.
public enum DroppedItem: Equatable, Sendable {
    case image(url: URL, mimeType: String)
    /// Relative to the worktree when the file lives inside it, absolute
    /// otherwise. `ChatPromptBuilder` accepts both.
    case file(path: String)
    case rejected(url: URL, reason: DropRejection)
}

public enum DropRejection: Equatable, Sendable {
    case imageTooLarge(byteCount: Int)
}

/// The single rule engine for dropped files. Chat, terminal, and the Files
/// sidebar all route through `classify`, so no destination can drift away
/// from the rules.
public enum FileDrop {
    /// An image is inlined into the prompt as base64, which costs about a
    /// third more than the file itself. Ten megabytes is the point past
    /// which a stray drop would stall a turn instead of enriching it.
    public static let maxImageBytes = 10 * 1_024 * 1_024

    /// Deliberately a table and not `UTType`: the design names exactly these
    /// five extensions, and a table is verifiable without asking the system
    /// anything.
    private static let imageMimeTypes: [String: String] = [
        "png": "image/png",
        "jpg": "image/jpeg",
        "jpeg": "image/jpeg",
        "gif": "image/gif",
        "webp": "image/webp",
    ]

    /// `byteCount` is a parameter rather than a `resourceValues` read so this
    /// stays pure: the size-cap tests never have to create a 10 MB file.
    public static func classify(_ inputs: [(url: URL, byteCount: Int)],
                                worktreePath: String) -> [DroppedItem] {
        inputs.map { input in
            // A folder has no image extension, so it falls through to `.file`
            // with no branch of its own — one branch fewer to diverge.
            guard let mimeType = imageMimeTypes[input.url.pathExtension.lowercased()] else {
                return .file(path: chipPath(for: input.url, worktreePath: worktreePath))
            }
            guard input.byteCount <= maxImageBytes else {
                return .rejected(url: input.url,
                                 reason: .imageTooLarge(byteCount: input.byteCount))
            }
            return .image(url: input.url, mimeType: mimeType)
        }
    }

    /// Absolute, shell-quoted, space-separated, no trailing newline: the user
    /// decides when to press return.
    public static func terminalInsertion(_ urls: [URL]) -> String {
        urls.map { shellQuote($0.path) }.joined(separator: " ")
    }

    /// The trailing "/" is what stops "/repo-backup/x" from counting as
    /// inside "/repo".
    static func chipPath(for url: URL, worktreePath: String) -> String {
        let root = worktreePath.hasSuffix("/") ? worktreePath : worktreePath + "/"
        let path = url.standardizedFileURL.path
        guard path.hasPrefix(root) else { return path }
        return String(path.dropFirst(root.count))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter FileDrop`
Expected: PASS, 11 tests.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/FileDrop.swift \
        Packages/TillerCore/Tests/TillerCoreTests/FileDropTests.swift
git commit -m "feat: add FileDrop classifier for dropped files"
```

---

### Task 3: `DroppedFileLoader`

`.onDrop` hands over `NSItemProvider`s, not URLs, and the resolution is asynchronous. Both the chat (in `App/`) and the terminal (in `TillerTerminal`) need this, so it lives in `TillerCore` where both can reach it. `NSItemProvider` is Foundation, so no UI framework leaks into the package.

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/DroppedFileLoader.swift`
- Create: `Packages/TillerCore/Tests/TillerCoreTests/DroppedFileLoaderTests.swift`

**Interfaces:**
- Consumes: nothing.
- Produces: `public enum DroppedFileLoader` with `public static func urls(from providers: [NSItemProvider]) async -> [URL]` and `public static func byteCount(of url: URL) -> Int`.

- [ ] **Step 1: Write the failing test**

Create `Packages/TillerCore/Tests/TillerCoreTests/DroppedFileLoaderTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerCore

@Test func resolvesProvidersIntoURLsInDragOrder() async throws {
    let directory = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let first = directory.appendingPathComponent("first.txt")
    let second = directory.appendingPathComponent("second.txt")
    try Data("one".utf8).write(to: first)
    try Data("two".utf8).write(to: second)

    let providers = [NSItemProvider(contentsOf: first), NSItemProvider(contentsOf: second)]
        .compactMap { $0 }
    let urls = await DroppedFileLoader.urls(from: providers)

    #expect(urls.map(\.lastPathComponent) == ["first.txt", "second.txt"])
}

@Test func readsTheByteCountOfARealFile() throws {
    let url = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent("\(UUID().uuidString).bin")
    try Data(count: 128).write(to: url)
    defer { try? FileManager.default.removeItem(at: url) }

    #expect(DroppedFileLoader.byteCount(of: url) == 128)
}

@Test func reportsZeroBytesForSomethingThatIsNotAReadableFile() {
    let missing = URL(fileURLWithPath: "/nonexistent/\(UUID().uuidString)")
    #expect(DroppedFileLoader.byteCount(of: missing) == 0)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter DroppedFileLoader`
Expected: FAIL — `cannot find 'DroppedFileLoader' in scope`.

- [ ] **Step 3: Write minimal implementation**

Create `Packages/TillerCore/Sources/TillerCore/DroppedFileLoader.swift`:

```swift
import Foundation

/// Turns the `NSItemProvider`s a drop delivers into file URLs.
///
/// Foundation only, no UI framework: the chat lives in `App/` and the
/// terminal in `TillerTerminal`, and both need exactly this.
public enum DroppedFileLoader {
    /// Resolved one provider at a time on purpose. Resolution is concurrent
    /// per provider, and gathering the results in parallel would make the
    /// chips land in whatever order the callbacks happened to fire — the
    /// user's drag order is the order they expect to see.
    public static func urls(from providers: [NSItemProvider]) async -> [URL] {
        var result: [URL] = []
        for provider in providers {
            let url: URL? = await withCheckedContinuation { continuation in
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    continuation.resume(returning: url)
                }
            }
            if let url { result.append(url) }
        }
        return result
    }

    /// Size for the image cap check. Anything unreadable reports 0, which
    /// classifies as a small file rather than silently dropping the item.
    public static func byteCount(of url: URL) -> Int {
        (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter DroppedFileLoader`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/DroppedFileLoader.swift \
        Packages/TillerCore/Tests/TillerCoreTests/DroppedFileLoaderTests.swift
git commit -m "feat: add DroppedFileLoader for NSItemProvider resolution"
```

---

### Task 4: Absolute mention paths in `ChatPromptBuilder`

Without this, a file dragged from `~/Downloads` produces `file:///worktree/Users/me/Downloads/notes.txt` — a resource link to a path that does not exist, and the agent fails silently.

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ChatPromptBuilder.swift:21-25`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ChatPromptBuilderTests.swift`

**Interfaces:**
- Consumes: nothing.
- Produces: no signature change — `ChatPromptBuilder.build(text:mentionPaths:images:worktreePath:)` now accepts absolute paths in `mentionPaths`.

- [ ] **Step 1: Write the failing test**

Append inside the existing `ChatPromptBuilderTests` struct in `Packages/TillerACP/Tests/TillerACPTests/ChatPromptBuilderTests.swift`:

```swift
    @Test func usesAbsoluteMentionPathsAsTheyAre() {
        let blocks = ChatPromptBuilder.build(
            text: "", mentionPaths: ["/Users/me/Downloads/notes.txt"],
            images: [], worktreePath: "/w")
        #expect(blocks == [.resourceLink(uri: "file:///Users/me/Downloads/notes.txt",
                                         name: "notes.txt")])
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ChatPromptBuilderTests`
Expected: FAIL — the built URI is `file:///w/Users/me/Downloads/notes.txt`.

- [ ] **Step 3: Write minimal implementation**

In `Packages/TillerACP/Sources/TillerACP/ChatPromptBuilder.swift`, replace the mention loop:

```swift
        for path in mentionPaths {
            // A dropped file can live anywhere on disk, so an absolute path is
            // used as-is; appending it to the worktree would build a URI that
            // resolves to nothing and fail silently at the agent.
            let url = path.hasPrefix("/")
                ? URL(fileURLWithPath: path)
                : URL(fileURLWithPath: worktreePath).appendingPathComponent(path)
            blocks.append(.resourceLink(uri: url.absoluteString,
                                        name: url.lastPathComponent))
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerACP && swift test --filter ChatPromptBuilderTests`
Expected: PASS, 3 tests — the two pre-existing ones still green (relative paths unchanged).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ChatPromptBuilder.swift \
        Packages/TillerACP/Tests/TillerACPTests/ChatPromptBuilderTests.swift
git commit -m "fix: keep absolute mention paths intact in prompt blocks"
```

---

### Task 5: Stop the composer text view from swallowing drops

`ChatTextEditor` builds an editable `NSTextView`, which registers for file drags by default and inserts the file itself. It sits below the SwiftUI drop area and answers first, so without this the composer — the spot users aim at most — bypasses `FileDrop` entirely.

**Files:**
- Modify: `App/Chat/ChatTextEditor.swift:42-57` (inside `makeTextView`)
- Create: `AppTests/ChatTextEditorDropTests.swift`

**Interfaces:**
- Consumes: `ChatTextEditor.makeTextView(document:)` — the existing test seam that returns a real `NSTextView` without a SwiftUI context.
- Produces: no new API.

- [ ] **Step 1: Write the failing test**

Create `AppTests/ChatTextEditorDropTests.swift`:

```swift
import AppKit
import Testing

@testable import Tiller

@Suite("ChatTextEditor drops")
@MainActor
struct ChatTextEditorDropTests {
    /// An editable NSTextView registers for file drags by default and would
    /// answer before the chat pane's own drop handler, inserting the file
    /// itself and bypassing the chip pipeline.
    @Test func textViewClaimsNoDraggedTypes() {
        let textView = ChatTextEditor.makeTextView()
        #expect(textView.registeredDraggedTypes.isEmpty)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -only-testing:TillerTests/ChatTextEditorDropTests 2>&1 | tail -20`
Expected: FAIL — `registeredDraggedTypes` is non-empty (it contains the string, URL, and file promise types AppKit installs).

- [ ] **Step 3: Write minimal implementation**

In `App/Chat/ChatTextEditor.swift`, inside `makeTextView`, right after `textView.isRichText = false`:

```swift
        // Drops are handled by ChatPaneView, which routes them through
        // FileDrop. An editable NSTextView otherwise claims the file types
        // first — it is lower in the responder chain than the SwiftUI drop
        // area — and inserts the file on its own terms.
        textView.unregisterDraggedTypes()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -only-testing:TillerTests/ChatTextEditorDropTests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatTextEditor.swift AppTests/ChatTextEditorDropTests.swift
git commit -m "fix: stop the composer text view from claiming file drops"
```

---

### Task 6: Transient message toast

Rejections need somewhere to surface. There is no generic toast today: `UpdateToastView` is a `switch` over `UpdaterModel.state`. This adds the smallest generic equivalent, in the same corner and the same visual style.

**Files:**
- Modify: `App/AppModel.swift:342` (add the two properties and the method just below the existing `var lastError: String?`)
- Create: `App/TransientMessageToast.swift`
- Modify: `App/ContentView.swift:128-131`
- Create: `AppTests/TransientMessageTests.swift`

**Interfaces:**
- Consumes: `AppTheme.title`, `AppTheme.hairline`.
- Produces:
  - `AppModel.transientMessage: String?`
  - `AppModel.showTransientMessage(_ text: String)`
  - `struct TransientMessageToast: View` with `let message: String`

- [ ] **Step 1: Write the failing test**

Create `AppTests/TransientMessageTests.swift`:

```swift
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
```

`AppModel()` is constructible with no arguments: every parameter of its initializer (`App/AppModel.swift:225-239`) has a default. No fixture needed.

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -only-testing:TillerTests/TransientMessageTests 2>&1 | tail -20`
Expected: FAIL — `value of type 'AppModel' has no member 'transientMessage'`.

- [ ] **Step 3: Add the model state**

In `App/AppModel.swift`, next to the existing `lastError` property:

```swift
    /// Short-lived bottom-right message (drop rejections today). A newer
    /// message replaces the older one and restarts the timer; the token is
    /// what stops a stale timer from clearing a fresh message.
    var transientMessage: String?
    private var transientMessageToken = 0
    static let transientMessageSeconds = 4

    func showTransientMessage(_ text: String) {
        transientMessage = text
        transientMessageToken += 1
        let token = transientMessageToken
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(AppModel.transientMessageSeconds))
            guard let self, self.transientMessageToken == token else { return }
            self.transientMessage = nil
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -only-testing:TillerTests/TransientMessageTests 2>&1 | tail -20`
Expected: PASS, 2 tests.

- [ ] **Step 5: Add the toast view**

Create `App/TransientMessageToast.swift`:

```swift
import SwiftUI
import Inject

/// Bottom-right transient message. Mirrors `UpdateToastView`'s chrome so the
/// two never look like different systems, without inheriting its
/// updater-specific state machine.
struct TransientMessageToast: View {
    @ObserveInjection private var inject

    let message: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.triangle")
                .foregroundStyle(AppTheme.title)
            Text(message)
                .lineLimit(2)
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(AppTheme.hairline))
    .enableInjection()
    }
}
```

- [ ] **Step 6: Mount it in ContentView**

In `App/ContentView.swift`, replace the existing bottom-trailing overlay:

```swift
        .overlay(alignment: .bottomTrailing) {
            VStack(alignment: .trailing, spacing: 8) {
                if let message = model.transientMessage {
                    TransientMessageToast(message: message)
                }
                UpdateToastView(updater: updater)
            }
            .padding(16)
        }
```

- [ ] **Step 7: Regenerate the project and build**

Run: `xcodegen generate && xcodebuild build -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 8: Commit**

```bash
git add App/AppModel.swift App/TransientMessageToast.swift App/ContentView.swift \
        AppTests/TransientMessageTests.swift project.yml
git commit -m "feat: add transient message toast"
```

---

### Task 7: `ComposerDropApplier` and hoisting the composer document

The chat drop area is the whole pane, but the chips belong to the composer's document, which today is `@State` private to `ChatComposerView`. Hoist it one level, and put the "items → chips" step in its own function so it is reachable from a test instead of living inside an `.onDrop` closure.

**Files:**
- Create: `App/Chat/ComposerDropApplier.swift`
- Modify: `App/Chat/ChatComposerView.swift:18` (`@State private var document` becomes a `let`)
- Modify: `App/Chat/ChatPaneView.swift:42-98` (own the document, pass it down)
- Create: `AppTests/ComposerDropApplierTests.swift`

**Interfaces:**
- Consumes: `DroppedItem` (Task 2), `ComposerDocument.insert(_:replacing:)`, `ComposerChip`, `ImageAttachment`.
- Produces: `enum ComposerDropApplier` with `@MainActor static func apply(_ items: [DroppedItem], to document: ComposerDocument) -> [String]` returning user-facing messages for the items that could not be attached.

- [ ] **Step 1: Write the failing test**

Create `AppTests/ComposerDropApplierTests.swift`:

```swift
import AppKit
import Foundation
import Testing
import TillerACP
import TillerCore

@testable import Tiller

@Suite("ComposerDropApplier")
@MainActor
struct ComposerDropApplierTests {
    private func temporaryPNG() throws -> URL {
        let url = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("\(UUID().uuidString).png")
        try Data([0x89, 0x50, 0x4E, 0x47]).write(to: url)
        return url
    }

    @Test func imagesBecomeImageChipsCarryingBase64Bytes() throws {
        let url = try temporaryPNG()
        defer { try? FileManager.default.removeItem(at: url) }
        let document = ComposerDocument()

        let messages = ComposerDropApplier.apply(
            [.image(url: url, mimeType: "image/png")], to: document)

        #expect(messages.isEmpty)
        let draft = ComposerDraft.parse(document.storage)
        #expect(draft.images == [ImageAttachment(mimeType: "image/png",
                                                 base64Data: "iVBORw==")])
    }

    @Test func filesBecomeMentionPathsInDropOrder() {
        let document = ComposerDocument()

        let messages = ComposerDropApplier.apply(
            [.file(path: "Sources/App.swift"), .file(path: "/tmp/notes.txt")],
            to: document)

        #expect(messages.isEmpty)
        #expect(ComposerDraft.parse(document.storage).mentionPaths
                == ["Sources/App.swift", "/tmp/notes.txt"])
    }

    @Test func oversizedImagesProduceAMessageAndNoChip() {
        let document = ComposerDocument()

        let messages = ComposerDropApplier.apply(
            [.rejected(url: URL(fileURLWithPath: "/tmp/huge.png"),
                       reason: .imageTooLarge(byteCount: FileDrop.maxImageBytes + 1))],
            to: document)

        #expect(messages == ["huge.png is too large (max 10 MB)"])
        #expect(document.isEmpty)
    }

    @Test func aMixedDropAttachesWhatItCanAndReportsTheRest() throws {
        let url = try temporaryPNG()
        defer { try? FileManager.default.removeItem(at: url) }
        let document = ComposerDocument()

        let messages = ComposerDropApplier.apply([
            .image(url: url, mimeType: "image/png"),
            .file(path: "notes.txt"),
            .rejected(url: URL(fileURLWithPath: "/tmp/huge.png"),
                      reason: .imageTooLarge(byteCount: FileDrop.maxImageBytes + 1)),
        ], to: document)

        let draft = ComposerDraft.parse(document.storage)
        #expect(draft.images.count == 1)
        #expect(draft.mentionPaths == ["notes.txt"])
        #expect(messages.count == 1)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -only-testing:TillerTests/ComposerDropApplierTests 2>&1 | tail -20`
Expected: FAIL — `cannot find 'ComposerDropApplier' in scope`.

- [ ] **Step 3: Write minimal implementation**

Create `App/Chat/ComposerDropApplier.swift`:

```swift
import AppKit
import Foundation
import TillerACP
import TillerCore

/// Turns classified drop items into composer chips.
///
/// Split out of the `.onDrop` closure so it can be tested without SwiftUI:
/// the closure keeps only the parts that must touch the view.
enum ComposerDropApplier {
    /// Appends one chip per attachable item at the end of the draft and
    /// returns the user-facing messages for the items that were not
    /// attached. Nothing is inserted for a rejected item.
    @MainActor
    @discardableResult
    static func apply(_ items: [DroppedItem], to document: ComposerDocument) -> [String] {
        var messages: [String] = []
        for item in items {
            switch item {
            case .image(let url, let mimeType):
                guard let data = try? Data(contentsOf: url) else {
                    messages.append("Couldn't read \(url.lastPathComponent)")
                    continue
                }
                let attachment = ImageAttachment(mimeType: mimeType,
                                                 base64Data: data.base64EncodedString())
                insert(.image(attachment), into: document)
            case .file(let path):
                insert(.file(path: path), into: document)
            case .rejected(let url, .imageTooLarge):
                messages.append("\(url.lastPathComponent) is too large (max 10 MB)")
            }
        }
        return messages
    }

    /// Chips always land at the end of the draft: a drop has no caret of its
    /// own, and the end is where `attachImage` already puts them.
    @MainActor
    private static func insert(_ chip: ComposerChip, into document: ComposerDocument) {
        document.insert(chip,
                        replacing: NSRange(location: document.storage.length, length: 0))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -only-testing:TillerTests/ComposerDropApplierTests 2>&1 | tail -20`
Expected: PASS, 4 tests.

- [ ] **Step 5: Hoist the document out of the composer**

In `App/Chat/ChatComposerView.swift`, replace line 18:

```swift
    let document: ComposerDocument
```

In `App/Chat/ChatPaneView.swift`, add the state next to the existing properties:

```swift
    /// Owned here rather than inside the composer because the whole pane is
    /// the drop target, and a drop has to reach the draft.
    @State private var document = ComposerDocument()
```

and pass it at line 96:

```swift
                ChatComposerView(controller: controller, worktreePath: worktree.path,
                                 document: document)
```

- [ ] **Step 6: Build and run the existing composer tests to catch the hoist breaking anything**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -only-testing:TillerTests/ComposerChipTests -only-testing:TillerTests/ComposerDocumentTests -only-testing:TillerTests/ComposerSendTests 2>&1 | tail -20`
Expected: PASS, all pre-existing composer tests green.

- [ ] **Step 7: Commit**

```bash
git add App/Chat/ComposerDropApplier.swift App/Chat/ChatComposerView.swift \
        App/Chat/ChatPaneView.swift AppTests/ComposerDropApplierTests.swift project.yml
git commit -m "feat: apply dropped files to the composer draft"
```

---

### Task 8: Chat pane drop area

**Files:**
- Modify: `App/Chat/ChatPaneView.swift` (drop modifier, targeting state, overlay)

**Interfaces:**
- Consumes: `DroppedFileLoader.urls(from:)`, `DroppedFileLoader.byteCount(of:)`, `FileDrop.classify(_:worktreePath:)`, `ComposerDropApplier.apply(_:to:)`, `AppModel.showTransientMessage(_:)`.
- Produces: no new API.

- [ ] **Step 1: Add the targeting state and the drop handler**

In `App/Chat/ChatPaneView.swift`, add to the properties:

```swift
    @State private var isDropTargeted = false
```

and add these methods to the struct:

```swift
    /// Drops follow the same rule as typing: a composer that cannot accept
    /// input must not quietly accumulate chips while a permission prompt is
    /// waiting.
    private var canAcceptDrop: Bool {
        (controller.state == .ready || controller.state == .prompting
            || controller.state == .detached)
            && !controller.presentationSnapshot.hasPendingPermission
    }

    private func handleDrop(_ providers: [NSItemProvider]) -> Bool {
        guard canAcceptDrop else { return false }
        let worktreePath = worktree.path
        Task { @MainActor in
            let urls = await DroppedFileLoader.urls(from: providers)
            guard !urls.isEmpty else { return }
            let inputs = urls.map { (url: $0, byteCount: DroppedFileLoader.byteCount(of: $0)) }
            let items = FileDrop.classify(inputs, worktreePath: worktreePath)
            for message in ComposerDropApplier.apply(items, to: document) {
                appModel.showTransientMessage(message)
            }
        }
        return true
    }
```

Add the imports `AppKit` and `UniformTypeIdentifiers` at the top of the file if not already present (`TillerCore` is already imported).

- [ ] **Step 2: Attach the drop modifier and the overlay**

On the outer `VStack` in `body`, after `.background { MainSurfaceMaterial(...) }`:

```swift
        .onDrop(of: [.fileURL], isTargeted: $isDropTargeted) { providers in
            handleDrop(providers)
        }
        .overlay {
            if isDropTargeted && canAcceptDrop {
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(Color.accentColor,
                                  style: StrokeStyle(lineWidth: 2, dash: [6, 4]))
                    .background(Color.accentColor.opacity(0.06),
                                in: RoundedRectangle(cornerRadius: 12))
                    .overlay {
                        Text("Drop files to attach")
                            .font(.callout.weight(.medium))
                            .foregroundStyle(AppTheme.title)
                    }
                    .padding(8)
                    .allowsHitTesting(false)
            }
        }
```

- [ ] **Step 3: Build**

Run: `xcodebuild build -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 4: Verify by hand, with screenshots**

Launch the app from Xcode (⌘R — `open` alone does not populate DerivedData for hot reload).

1. Drag a small PNG from Finder onto the middle of a chat transcript → dashed overlay appears → release → an image chip lands at the end of the draft. Screenshot.
2. Send it and confirm the agent describes the image. Screenshot.
3. Drag a `.txt` from Finder → a file chip appears with the file name. Screenshot.
4. Drag a PNG larger than 10 MB → no chip, and the toast reads `<name> is too large (max 10 MB)`. Screenshot.
5. Drag a folder → a file chip with the folder name. Screenshot.
6. Click into the composer text and drag a file directly onto the text area → still becomes a chip, is *not* inserted as text or as an AppKit attachment. This is the Task 5 regression check. Screenshot.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatPaneView.swift
git commit -m "feat: accept file drops across the chat pane"
```

---

### Task 9: Terminal drop

Implemented inside `PtyTerminalPane` because both of its call sites (`TerminalSurfaceHost.swift:116` and `SplitViewRenderer.swift:178`) route through it: single panes and splits are covered by one change.

**Files:**
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift:25-56` (add `focus()` to `TerminalSurfaceProxy`) and `:103-140` (drop modifier and overlay on `body`)
- Create: `Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalDropTests.swift`

**Interfaces:**
- Consumes: `FileDrop.terminalInsertion(_:)`, `DroppedFileLoader.urls(from:)`, `PaneRegistry.write(paneId:data:)`.
- Produces: `TerminalSurfaceProxy.focus() -> Bool`.

- [ ] **Step 1: Write the contract test**

Not a red-green cycle: `FileDrop.terminalInsertion` already exists and is green from Task 2. This test locks what the terminal path is allowed to write — quoted, space-joined, no newline — so a later edit to the insertion format cannot silently start executing commands on drop. Everything genuinely new in this task (`focus()`, the modifier wiring) is verified by the build and by Step 7.

Create `Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalDropTests.swift`:

```swift
import Foundation
import Testing
import TillerCore
@testable import TillerTerminal

/// The bytes a terminal drop writes to the PTY: the insertion string with no
/// trailing newline, because the user decides when to run the command.
@Test func terminalDropWritesQuotedPathsWithoutRunningThem() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    let payload = FileDrop.terminalInsertion([
        URL(fileURLWithPath: "/tmp/a b.txt"),
        URL(fileURLWithPath: "/tmp/c.txt"),
    ])

    #expect(payload == "'/tmp/a b.txt' '/tmp/c.txt'")
    #expect(payload.hasSuffix("\n") == false)
    // No pane is registered, so the write is refused rather than silently
    // going nowhere — the same signal the drop handler relies on.
    #expect(await registry.write(paneId: paneId, data: Data(payload.utf8)) == false)
}
```

- [ ] **Step 2: Run the contract test**

Run: `cd Packages/TillerTerminal && swift test --filter terminalDropWritesQuotedPathsWithoutRunningThem`
Expected: PASS. A failure here means Task 2 has not landed or its insertion format drifted — fix that before wiring anything.

- [ ] **Step 3: Add `focus()` to the proxy**

In `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`, inside `TerminalSurfaceProxy`, after `clearScreen()`:

```swift
    /// Makes the terminal view first responder. Used after a drop, so the
    /// path lands in a pane the user can immediately type into.
    @discardableResult
    public func focus() -> Bool {
        guard let view, let window = view.window else { return false }
        return window.makeFirstResponder(view)
    }
```

- [ ] **Step 4: Add the drop handler to the pane**

In the same file, add to `PtyTerminalPane`:

```swift
    @State private var isDropTargeted = false
```

and this method:

```swift
    /// Inserts the dropped paths at the shell's cursor. Never appends a
    /// newline: Terminal.app, iTerm2, and Ghostty all leave the return key
    /// to the user, and a drop that ran a command would be unrecoverable.
    private func handleDrop(_ providers: [NSItemProvider]) -> Bool {
        Task { @MainActor in
            let urls = await DroppedFileLoader.urls(from: providers)
            guard !urls.isEmpty else { return }
            let payload = FileDrop.terminalInsertion(urls)
            _ = await PaneRegistry.shared.write(paneId: paneId, data: Data(payload.utf8))
            runtime?.proxy.focus()
        }
        return true
    }
```

Add `import TillerCore` if the file does not already have it (it does, line 5) and `import UniformTypeIdentifiers` at the top.

- [ ] **Step 5: Attach the modifier and overlay**

On the `TerminalSurfaceView(context: state)` chain in `body`, after the last `.onChange`:

```swift
            .onDrop(of: [.fileURL], isTargeted: $isDropTargeted) { providers in
                handleDrop(providers)
            }
            .overlay {
                if isDropTargeted {
                    RoundedRectangle(cornerRadius: 6)
                        .strokeBorder(Color.accentColor,
                                      style: StrokeStyle(lineWidth: 2, dash: [6, 4]))
                        .allowsHitTesting(false)
                }
            }
```

- [ ] **Step 6: Run the package tests**

Run: `cd Packages/TillerTerminal && swift test`
Expected: PASS. Note: `PtyProcessTests` is a known flaky suite in this package — if it fails, re-run twice on the same tree before blaming this change.

- [ ] **Step 7: Verify by hand, with screenshots**

1. Drag a file whose name contains a space from Finder onto a terminal pane → the quoted path appears at the prompt, nothing runs, and the pane has keyboard focus (type a character to confirm). Screenshot.
2. Split a terminal pane and drop on the *second* pane → the path lands in that pane, not the first. Screenshot.
3. Drag a folder → its path appears; prefix with `cd ` and press return to confirm it is usable. Screenshot.

- [ ] **Step 8: Commit**

```bash
git add Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift \
        Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalDropTests.swift
git commit -m "feat: insert dropped file paths into terminal panes"
```

---

### Task 10: Files sidebar as a drag source

**Files:**
- Modify: `App/RightPanel/FileExplorerView.swift:72-146` (`fileRow`)

**Interfaces:**
- Consumes: `panelModel.rootURL`, `FileTreeNode.url(relativeTo:)`.
- Produces: rows that vend a `URL`, which SwiftUI exposes as `.fileURL` — the same payload Finder sends, so chat and terminal need no second code path.

- [ ] **Step 1: Make the row draggable**

In `App/RightPanel/FileExplorerView.swift`, in `fileRow`, after the `.contentShape(Rectangle())` modifier and before `.onTapGesture`:

```swift
        // `URL` is already Transferable and vends `.fileURL`, so a row drag
        // is indistinguishable from a Finder drag at every drop target.
        .draggable(dragURL(for: node) ?? URL(fileURLWithPath: "/"))
```

and add the helper method:

```swift
    private func dragURL(for node: FileTreeNode) -> URL? {
        panelModel.rootURL.map { node.url(relativeTo: $0) }
    }
```

- [ ] **Step 2: Build**

Run: `xcodebuild build -project Tiller.xcodeproj -scheme Tiller -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 3: Verify by hand, with screenshots — including the gesture regression**

This is the step this task exists for. A drag recogniser added to a row that already has two tap gestures is a known failure mode in this codebase, and its symptom is "hover highlights, click does nothing".

1. Single-click a file row → it still selects (highlight moves). Screenshot.
2. Double-click a file name → it still opens in a tab. Screenshot.
3. Click a directory row → it still expands/collapses. Screenshot.
4. Drag a file row into a chat pane → chip appears. Screenshot.
5. Drag a file row into a terminal pane → quoted path appears. Screenshot.
6. Arrow keys and space still move/toggle the selection.

If 1-3 regress, the fix is a movement threshold on the drag, not the removal of the tap handlers.

- [ ] **Step 4: Commit**

```bash
git add App/RightPanel/FileExplorerView.swift
git commit -m "feat: drag files out of the Files sidebar"
```

---

### Task 11: Full gate

**Files:** none — verification only.

- [ ] **Step 1: Regenerate the project**

Run: `xcodegen generate`
Expected: no error. Required because Tasks 6 and 7 added files under `App/`.

- [ ] **Step 2: Run the repo gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

If it fails, first establish whether the failure predates this work: run `Scripts/ci.sh` twice on the same tree and check whether the failing suite is `PtyProcessTests`, `DividerCursorStrip`, or `AppSettings` — all three have a documented history of failing independently of the change under test. Attribute a failure to this work only after the same tree fails twice in the same place.

- [ ] **Step 3: Confirm the manual checklist is complete**

Every screenshot from Tasks 8, 9, and 10 taken and reviewed. The one that matters most is Task 10 Step 3 items 1-3: the sidebar gesture regression is the only defect in this plan that automated tests cannot see.

- [ ] **Step 4: Commit any gate fixes**

```bash
git add -A
git commit -m "test: fix gate failures from file drag and drop"
```

---

## Verification Matrix

| Spec requirement | Task |
|---|---|
| Path assoluto fuori dal worktree | 2 (classify), 4 (prompt builder) |
| Cinque tipi immagine, case-insensitive | 2 |
| Tetto 10 MB con rifiuto esplicito | 2 (classify), 7 (message), 6 (toast) |
| Cartella → chip file | 2 |
| Nessun tetto sul numero di file | 2 (map over all inputs), 7 |
| Drop area = tutto il pane chat | 8 |
| Overlay `Drop files to attach` | 8 |
| Drop consentito solo se `canInteract` | 8 (`canAcceptDrop`) |
| Terminale: path quotati, nessuna newline, nessuna esecuzione | 2 (`terminalInsertion`), 9 |
| Terminale: focus al drop | 9 (`TerminalSurfaceProxy.focus`) |
| Terminale: funziona anche sugli split | 9 (implementato in `PtyTerminalPane`) |
| Sidebar come sorgente di drag | 10 |
| `shellQuote` non duplicato | 1 |
| Rischio `NSTextView` che mangia i drop | 5 |
| `CI OK` | 11 |
