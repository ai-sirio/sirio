# Right Worktree Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a persistent, hideable right-side worktree panel with a lazy file explorer, unified local Git diff, and interactive Git status.

**Architecture:** Extend the existing `HSplitView` to a third trailing column. Keep filesystem primitives in `TillerCore`, every `/usr/bin/git` call and parser in `TillerGit`, and compose them through one `@MainActor @Observable` `RightPanelModel` in `App`. The panel exists only while visible, follows `AppModel.selectedWorktree`, and rejects stale async results with a generation token.

**Tech Stack:** Swift 6, SwiftUI/Observation, macOS 15, CoreServices FSEvents, Foundation `Process`, Swift Testing (`@Test` / `#expect`), XcodeGen.

**Spec:** `docs/superpowers/specs/2026-07-14-right-worktree-panel-design.md`

## Global Constraints

- Deployment target remains macOS **15.0** and Swift language mode remains **6.0**.
- No new third-party dependency and no persistence schema migration.
- `TillerCore` must not import `TillerGit`, SwiftUI, or AppKit; `TillerGit` remains a standalone leaf package.
- Only `App/` may compose `TillerCore`, `TillerGit`, SwiftUI, and AppKit.
- Right-panel defaults: hidden, width **360 pt**, mode **Files**; allowed width **280...600 pt**.
- Persist only visibility, width, and selected mode. Expanded directories and selected files reset on worktree change and app relaunch.
- Files shows dotfiles but excludes `.git`; symlinked directories are never traversed.
- Diff is unified, per file, relative to `HEAD`; untracked and unborn-`HEAD` files compare against `/dev/null`.
- Git mutations are status-snapshot-driven, serialized, non-optimistic, and use `[String]` process arguments only.
- Staged entries offer Unstage, Changes offer Stage/Discard, Untracked offer Stage/Discard. Staged never offers Discard.
- Discard always requires a native confirmation with Cancel as the default action.
- Diff capture stops at **5 MiB or 20,000 lines** and never returns a partial patch as complete.
- Tests use Swift Testing, not XCTest. Run the narrow package test first, then `Scripts/ci.sh` at every App integration gate.
- New `App/RightPanel/*.swift` files are picked up by `project.yml`'s `sources: [App]`; regenerate `Tiller.xcodeproj`, never edit it.
- Commit subjects are Conventional Commits, lower-case imperative.

## File Structure

### Create

- `Packages/TillerCore/Sources/TillerCore/FileTree.swift` — safe lazy directory listing and file-tree value types.
- `Packages/TillerCore/Sources/TillerCore/FileSystemEventMonitor.swift` — recursive multi-root FSEvents async stream.
- `Packages/TillerCore/Tests/TillerCoreTests/FileTreeTests.swift` — loader, ordering, traversal, dotfile, and symlink contracts.
- `Packages/TillerCore/Tests/TillerCoreTests/FileSystemEventMonitorTests.swift` — nested and multi-root event delivery plus stop behavior.
- `Packages/TillerGit/Sources/TillerGit/GitRepository.swift` — git-dir and unborn-`HEAD` detection.
- `Packages/TillerGit/Sources/TillerGit/GitStatus.swift` — validated Git paths, porcelain parser, status snapshot.
- `Packages/TillerGit/Sources/TillerGit/GitDiff.swift` — bounded per-file diff load and unified patch parser.
- `Packages/TillerGit/Sources/TillerGit/GitActions.swift` — stage, unstage, tracked discard, untracked discard.
- `Packages/TillerGit/Tests/TillerGitTests/GitTestSupport.swift` — shared temporary-repository helpers for new tests.
- `Packages/TillerGit/Tests/TillerGitTests/GitStatusTests.swift` — parser and live status behavior.
- `Packages/TillerGit/Tests/TillerGitTests/GitDiffTests.swift` — parser, combined diff, untracked, unborn, binary, submodule-summary behavior.
- `Packages/TillerGit/Tests/TillerGitTests/GitActionsTests.swift` — mutation semantics on real temporary repositories.
- `App/RightPanel/RightPanelMode.swift` — UI-only route and effective-mode fallback.
- `App/RightPanel/RightPanelModel.swift` — worktree lifecycle, lazy file cache, monitor debounce, Git loads and mutations.
- `App/RightPanel/RightPanelView.swift` — segmented shell and mode routing.
- `App/RightPanel/FileExplorerView.swift` — flattened lazy tree, selection, keyboard handling, open/reveal/copy.
- `App/RightPanel/GitDiffView.swift` — file picker, unified diff rows, per-file actions and large/binary states.
- `App/RightPanel/GitStatusView.swift` — staged/changes/untracked sections, bulk actions and discard confirmation.

### Modify

- `Packages/TillerCore/Sources/TillerCore/AppSettings.swift:4-63` — right-panel keys, defaults, range and clamp.
- `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift:4-55` — preference defaults and width clamping.
- `Packages/TillerGit/Sources/TillerGit/GitRunner.swift:2-114` — raw captured result, accepted exit codes, bounded stdout, localized errors; preserve existing APIs.
- `Packages/TillerGit/Tests/TillerGitTests/GitRunnerTests.swift:34-62` — accepted exit code and output-limit coverage.
- `App/ContentView.swift:6-155` — panel state/model, toolbar toggle, third split column, activation lifecycle and divider cover.
- `App/TillerApp.swift:5-67` — View-menu toggle and shared persisted visibility.
- `App/AppTheme.swift:12-63` — semantic Git/diff colors.
- `README.md:30-36,88-96` — feature bullet and right-panel shortcut, only after the app smoke test passes.

---

### Task 1: Persisted panel preferences and route

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift:4-63`
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift:4-55`
- Create: `App/RightPanel/RightPanelMode.swift`

**Interfaces:**
- Consumes: existing `AppSettings` static-key pattern and `@AppStorage` raw values.
- Produces: `AppSettings.rightPanelVisibleKey`, `rightPanelWidthKey`, `rightPanelModeKey`, `defaultRightPanelVisible`, `defaultRightPanelWidth`, `rightPanelWidthRange`, `clampRightPanelWidth(_:)`; `RightPanelMode.effective(rawValue:isGitRepository:)`.

- [ ] **Step 1: Write the failing settings tests**

Append to `AppSettingsTests.swift`:

```swift
@Test func rightPanelDefaultsAreStable() {
    #expect(AppSettings.defaultRightPanelVisible == false)
    #expect(AppSettings.defaultRightPanelWidth == 360)
    #expect(AppSettings.rightPanelWidthRange == 280...600)
}

@Test func rightPanelWidthClampsToSupportedRange() {
    #expect(AppSettings.clampRightPanelWidth(120) == 280)
    #expect(AppSettings.clampRightPanelWidth(420) == 420)
    #expect(AppSettings.clampRightPanelWidth(900) == 600)
}
```

- [ ] **Step 2: Run the focused tests and confirm failure**

Run:

```bash
swift test --package-path Packages/TillerCore --filter rightPanel
```

Expected: compilation fails because the right-panel settings do not exist.

- [ ] **Step 3: Add the settings constants and clamp**

Insert before the closing brace of `AppSettings`:

```swift
    public static let rightPanelVisibleKey = "rightPanel.visible"
    public static let rightPanelWidthKey = "rightPanel.width"
    public static let rightPanelModeKey = "rightPanel.mode"

    public static let defaultRightPanelVisible = false
    public static let defaultRightPanelWidth = 360.0
    public static let rightPanelWidthRange: ClosedRange<Double> = 280...600

    public static func clampRightPanelWidth(_ width: Double) -> Double {
        min(max(width, rightPanelWidthRange.lowerBound), rightPanelWidthRange.upperBound)
    }
```

Create `App/RightPanel/RightPanelMode.swift`:

```swift
import Foundation

/// UI route for the worktree tools panel. Persistence stores rawValue only.
enum RightPanelMode: String, CaseIterable, Identifiable {
    case files
    case diff
    case status

    var id: String { rawValue }

    var title: String {
        switch self {
        case .files: "Files"
        case .diff: "Diff"
        case .status: "Status"
        }
    }

    var systemImage: String {
        switch self {
        case .files: "folder"
        case .diff: "plus.forwardslash.minus"
        case .status: "arrow.triangle.branch"
        }
    }

    var requiresGit: Bool { self != .files }

    static func effective(rawValue: String, isGitRepository: Bool) -> RightPanelMode {
        let saved = RightPanelMode(rawValue: rawValue) ?? .files
        return saved.requiresGit && !isGitRepository ? .files : saved
    }
}
```

- [ ] **Step 4: Run package tests and compile the App route**

Run:

```bash
swift test --package-path Packages/TillerCore --filter rightPanel
xcodegen generate
Scripts/ci.sh
```

Expected: focused tests pass and the final command prints `CI OK`.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSettings.swift \
  Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift \
  App/RightPanel/RightPanelMode.swift
git commit -m "feat: define right panel preferences"
```

---

### Task 2: Safe lazy file-tree loader

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/FileTree.swift`
- Create: `Packages/TillerCore/Tests/TillerCoreTests/FileTreeTests.swift`

**Interfaces:**
- Consumes: Foundation `FileManager` and file-resource values.
- Produces: `FileTreeNodeKind`, `FileTreeNode`, `FileTreeError`, `FileTreeLoader.children(at:rootURL:)` and `FileTreeNode.url(relativeTo:)`.

- [ ] **Step 1: Write loader contract tests**

Create `FileTreeTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerCore

private func makeTreeRoot() throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-file-tree-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return root
}

@Test func fileTreeListsDirectoriesBeforeFilesAndExcludesDotGit() throws {
    let root = try makeTreeRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(
        at: root.appendingPathComponent("Sources", isDirectory: true),
        withIntermediateDirectories: true)
    try FileManager.default.createDirectory(
        at: root.appendingPathComponent(".git", isDirectory: true),
        withIntermediateDirectories: true)
    try "z".write(to: root.appendingPathComponent("z.txt"), atomically: true, encoding: .utf8)
    try "a".write(to: root.appendingPathComponent("a.txt"), atomically: true, encoding: .utf8)
    try "env".write(to: root.appendingPathComponent(".env"), atomically: true, encoding: .utf8)

    let nodes = try FileTreeLoader.children(at: "", rootURL: root)

    #expect(nodes.first?.name == "Sources")
    #expect(nodes.filter { $0.kind == .file }.map(\.name) == [".env", "a.txt", "z.txt"])
    #expect(!nodes.contains { $0.name == ".git" })
}

@Test func fileTreeDoesNotTreatDirectorySymlinkAsDirectory() throws {
    let root = try makeTreeRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let target = root.appendingPathComponent("Target", isDirectory: true)
    let link = root.appendingPathComponent("Target Link")
    try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
    try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)

    let node = try #require(FileTreeLoader.children(at: "", rootURL: root)
        .first { $0.name == "Target Link" })

    #expect(node.kind == .symbolicLink)
    #expect(!node.kind.isDirectory)
}

@Test func fileTreeRejectsTraversalOutsideRoot() throws {
    let root = try makeTreeRoot()
    defer { try? FileManager.default.removeItem(at: root) }

    #expect(throws: FileTreeError.pathOutsideRoot("../outside")) {
        _ = try FileTreeLoader.children(at: "../outside", rootURL: root)
    }
}
```

- [ ] **Step 2: Run the focused tests and confirm failure**

Run:

```bash
swift test --package-path Packages/TillerCore --filter fileTree
```

Expected: compilation fails because `FileTreeLoader` and related types do not exist.

- [ ] **Step 3: Implement value types and one-directory loading**

Create `FileTree.swift`:

```swift
import Foundation

public enum FileTreeNodeKind: Equatable, Sendable {
    case directory
    case file
    case symbolicLink

    public var isDirectory: Bool { self == .directory }
}

public struct FileTreeNode: Identifiable, Equatable, Sendable {
    public let relativePath: String
    public let name: String
    public let kind: FileTreeNodeKind

    public var id: String { relativePath }

    public init(relativePath: String, name: String, kind: FileTreeNodeKind) {
        self.relativePath = relativePath
        self.name = name
        self.kind = kind
    }

    public func url(relativeTo rootURL: URL) -> URL {
        rootURL.appendingPathComponent(relativePath)
    }
}

public enum FileTreeError: Error, Equatable {
    case pathOutsideRoot(String)
    case notDirectory(String)
}

extension FileTreeError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .pathOutsideRoot(let path): "Path escapes the worktree root: \(path)"
        case .notDirectory(let path): "Path is not a directory: \(path)"
        }
    }
}

public enum FileTreeLoader {
    public static func children(at relativePath: String, rootURL: URL) throws -> [FileTreeNode] {
        let root = rootURL.standardizedFileURL
        let directory = try validatedURL(relativePath: relativePath, root: root)
        let values = try directory.resourceValues(forKeys: [.isDirectoryKey])
        guard values.isDirectory == true else { throw FileTreeError.notDirectory(relativePath) }

        return try FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
            options: []
        )
        .filter { $0.lastPathComponent != ".git" }
        .map { child in
            let resource = try child.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            let kind: FileTreeNodeKind
            if resource.isSymbolicLink == true {
                kind = .symbolicLink
            } else if resource.isDirectory == true {
                kind = .directory
            } else {
                kind = .file
            }
            let childRelative = relativePath.isEmpty
                ? child.lastPathComponent
                : relativePath + "/" + child.lastPathComponent
            return FileTreeNode(relativePath: childRelative, name: child.lastPathComponent, kind: kind)
        }
        .sorted { lhs, rhs in
            if lhs.kind.isDirectory != rhs.kind.isDirectory { return lhs.kind.isDirectory }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }
    }

    private static func validatedURL(relativePath: String, root: URL) throws -> URL {
        guard !relativePath.hasPrefix("/") else {
            throw FileTreeError.pathOutsideRoot(relativePath)
        }
        let components = relativePath.split(separator: "/", omittingEmptySubsequences: false)
        guard !components.contains("..") else {
            throw FileTreeError.pathOutsideRoot(relativePath)
        }
        let candidate = relativePath.isEmpty
            ? root
            : root.appendingPathComponent(relativePath, isDirectory: true).standardizedFileURL
        let rootComponents = root.pathComponents
        let candidateComponents = candidate.pathComponents
        guard candidateComponents.starts(with: rootComponents) else {
            throw FileTreeError.pathOutsideRoot(relativePath)
        }
        return candidate
    }
}
```

- [ ] **Step 4: Run the package tests**

Run:

```bash
swift test --package-path Packages/TillerCore --filter fileTree
```

Expected: all `fileTree` tests pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/FileTree.swift \
  Packages/TillerCore/Tests/TillerCoreTests/FileTreeTests.swift
git commit -m "feat: add lazy file tree loader"
```

---

### Task 3: Recursive multi-root filesystem monitor

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/FileSystemEventMonitor.swift`
- Create: `Packages/TillerCore/Tests/TillerCoreTests/FileSystemEventMonitorTests.swift`

**Interfaces:**
- Consumes: macOS CoreServices FSEvents.
- Produces: `FileSystemEventMonitor.init?(roots:latency:)`, `events: AsyncStream<[URL]>`, and idempotent `stop()`.

- [ ] **Step 1: Write the monitor integration tests**

Create `FileSystemEventMonitorTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerCore

private actor EventPaths {
    private var values: [String] = []
    func append(_ urls: [URL]) { values.append(contentsOf: urls.map(\.path)) }
    func contains(_ suffix: String) -> Bool { values.contains { $0.hasSuffix(suffix) } }
    func count() -> Int { values.count }
}

private func makeMonitorRoot(_ label: String) throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-fsevents-\(label)-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return root
}

@Test func monitorDeliversNestedAndSecondRootEvents() async throws {
    let worktree = try makeMonitorRoot("worktree")
    let gitDir = try makeMonitorRoot("gitdir")
    defer {
        try? FileManager.default.removeItem(at: worktree)
        try? FileManager.default.removeItem(at: gitDir)
    }
    let monitor = try #require(FileSystemEventMonitor(roots: [worktree, gitDir], latency: 0.05))
    defer { monitor.stop() }
    let paths = EventPaths()
    let reader = Task {
        for await batch in monitor.events { await paths.append(batch) }
    }
    defer { reader.cancel() }

    let nested = worktree.appendingPathComponent("Sources", isDirectory: true)
    try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
    try "x".write(to: nested.appendingPathComponent("A.swift"), atomically: true, encoding: .utf8)
    try "index".write(to: gitDir.appendingPathComponent("index"), atomically: true, encoding: .utf8)

    for _ in 0..<60 {
        if await paths.contains("Sources/A.swift") && await paths.contains("index") { break }
        try await Task.sleep(for: .milliseconds(50))
    }
    let sawNestedFile = await paths.contains("Sources/A.swift")
    let sawGitIndex = await paths.contains("index")
    #expect(sawNestedFile)
    #expect(sawGitIndex)
}

@Test func stoppedMonitorFinishesStreamAndDeliversNoNewEvents() async throws {
    let root = try makeMonitorRoot("stop")
    defer { try? FileManager.default.removeItem(at: root) }
    let monitor = try #require(FileSystemEventMonitor(roots: [root], latency: 0.05))
    let paths = EventPaths()
    let reader = Task {
        for await batch in monitor.events { await paths.append(batch) }
    }

    monitor.stop()
    _ = await reader.result
    let countAfterStop = await paths.count()
    try "late".write(to: root.appendingPathComponent("late.txt"), atomically: true, encoding: .utf8)
    try await Task.sleep(for: .milliseconds(200))
    let finalCount = await paths.count()
    #expect(finalCount == countAfterStop)
}
```

- [ ] **Step 2: Run the focused tests and confirm failure**

Run:

```bash
swift test --package-path Packages/TillerCore --filter monitor
```

Expected: compilation fails because `FileSystemEventMonitor` does not exist.

- [ ] **Step 3: Implement the FSEvents wrapper**

Create `FileSystemEventMonitor.swift`:

```swift
import CoreServices
import Foundation

public final class FileSystemEventMonitor: @unchecked Sendable {
    private final class CallbackBox: @unchecked Sendable {
        let continuation: AsyncStream<[URL]>.Continuation
        init(continuation: AsyncStream<[URL]>.Continuation) {
            self.continuation = continuation
        }
    }

    public let events: AsyncStream<[URL]>
    private let lock = NSLock()
    private var stream: FSEventStreamRef?
    private var continuation: AsyncStream<[URL]>.Continuation?

    public init?(roots: [URL], latency: TimeInterval = 0.1) {
        guard !roots.isEmpty else { return nil }
        let pair = AsyncStream<[URL]>.makeStream()
        self.events = pair.stream
        self.continuation = pair.continuation

        let box = CallbackBox(continuation: pair.continuation)
        let boxInfo = Unmanaged.passRetained(box).toOpaque()
        var context = FSEventStreamContext(
            version: 0,
            info: boxInfo,
            retain: nil,
            release: { info in
                guard let info else { return }
                Unmanaged<CallbackBox>.fromOpaque(info).release()
            },
            copyDescription: nil
        )
        let callback: FSEventStreamCallback = { _, info, count, eventPaths, _, _ in
            guard let info else { return }
            let box = Unmanaged<CallbackBox>.fromOpaque(info).takeUnretainedValue()
            let strings = unsafeBitCast(eventPaths, to: NSArray.self) as? [String] ?? []
            box.continuation.yield(strings.prefix(count).map { URL(fileURLWithPath: $0) })
        }
        let paths = roots.map { $0.standardizedFileURL.path } as CFArray
        let flags = FSEventStreamCreateFlags(
            kFSEventStreamCreateFlagFileEvents
                | kFSEventStreamCreateFlagUseCFTypes
                | kFSEventStreamCreateFlagWatchRoot)
        guard let stream = FSEventStreamCreate(
            kCFAllocatorDefault,
            callback,
            &context,
            paths,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            latency,
            flags
        ) else {
            Unmanaged<CallbackBox>.fromOpaque(boxInfo).release()
            pair.continuation.finish()
            return nil
        }
        FSEventStreamSetDispatchQueue(
            stream,
            DispatchQueue(label: "dev.tiller.filesystem-events", qos: .utility))
        guard FSEventStreamStart(stream) else {
            FSEventStreamInvalidate(stream)
            FSEventStreamRelease(stream)
            pair.continuation.finish()
            return nil
        }
        self.stream = stream
    }

    public func stop() {
        lock.lock()
        let current = stream
        stream = nil
        let currentContinuation = continuation
        continuation = nil
        lock.unlock()
        guard let current else { return }
        FSEventStreamStop(current)
        FSEventStreamInvalidate(current)
        FSEventStreamRelease(current)
        currentContinuation?.finish()
    }

    deinit { stop() }
}
```

- [ ] **Step 4: Run the monitor tests and the complete Core suite**

Run:

```bash
swift test --package-path Packages/TillerCore --filter monitor
swift test --package-path Packages/TillerCore
```

Expected: both commands pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/FileSystemEventMonitor.swift \
  Packages/TillerCore/Tests/TillerCoreTests/FileSystemEventMonitorTests.swift
git commit -m "feat: monitor worktree filesystem changes"
```

---

### Task 4: Bounded raw Git command capture

**Files:**
- Modify: `Packages/TillerGit/Sources/TillerGit/GitRunner.swift:2-114`
- Modify: `Packages/TillerGit/Tests/TillerGitTests/GitRunnerTests.swift:34-62`
- Create: `Packages/TillerGit/Tests/TillerGitTests/GitTestSupport.swift`

**Interfaces:**
- Consumes: existing `GitRunner.run` and `runStreaming` callers; both signatures remain source-compatible.
- Produces: `GitCommandResult`, `GitOutputLimits`, `GitRunner.runCaptured(_:in:acceptedExitCodes:limits:)`, `GitError.outputTooLarge`.

- [ ] **Step 1: Add shared test repository support**

Create `GitTestSupport.swift`:

```swift
import Foundation

@discardableResult
func runGitForTest(_ arguments: [String], in directory: URL) throws -> String {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    process.arguments = arguments
    process.currentDirectoryURL = directory
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr
    try process.run()
    process.waitUntilExit()
    let output = String(decoding: stdout.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
    if process.terminationStatus != 0 {
        let error = String(decoding: stderr.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        throw NSError(domain: "GitTest", code: Int(process.terminationStatus),
                      userInfo: [NSLocalizedDescriptionKey: error])
    }
    return output
}

func makeGitTestRepository(withCommit: Bool = true) throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-git-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    try runGitForTest(["init", "-b", "main"], in: root)
    try runGitForTest(["config", "user.email", "test@tiller.dev"], in: root)
    try runGitForTest(["config", "user.name", "Tiller Test"], in: root)
    if withCommit {
        try "one\n".write(to: root.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
        try runGitForTest(["add", "--", "file.txt"], in: root)
        try runGitForTest(["commit", "-m", "root"], in: root)
    }
    return root
}
```

- [ ] **Step 2: Write accepted-exit and limit tests**

Append to `GitRunnerTests.swift`:

```swift
@Test func runCapturedAcceptsNoIndexDifferenceExitCode() async throws {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-no-index-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    try "new\n".write(to: directory.appendingPathComponent("new.txt"), atomically: true, encoding: .utf8)

    let result = try await GitRunner.runCaptured(
        ["diff", "--no-index", "--", "/dev/null", directory.appendingPathComponent("new.txt").path],
        in: directory.path,
        acceptedExitCodes: [0, 1]
    )

    #expect(result.exitCode == 1)
    #expect(result.stdoutString.contains("+new"))
}

@Test func runCapturedRejectsOutputBeyondLimit() async throws {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-output-limit-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    try String(repeating: "line\n", count: 1000)
        .write(to: directory.appendingPathComponent("large.txt"), atomically: true, encoding: .utf8)

    await #expect(throws: GitError.outputTooLarge(maxBytes: 128, maxLines: 20)) {
        _ = try await GitRunner.runCaptured(
            ["diff", "--no-index", "--", "/dev/null", directory.appendingPathComponent("large.txt").path],
            in: directory.path,
            acceptedExitCodes: [0, 1],
            limits: GitOutputLimits(maxBytes: 128, maxLines: 20)
        )
    }
}
```

- [ ] **Step 3: Run tests and confirm API failures**

Run:

```bash
swift test --package-path Packages/TillerGit --filter runCaptured
```

Expected: compilation fails because the captured-result API and limit error do not exist.

- [ ] **Step 4: Add the captured-result types and error case**

Replace `GitError` and add the result types at the top of `GitRunner.swift`:

```swift
public enum GitError: Error, Equatable {
    case commandFailed(code: Int32, stderr: String)
    case outputTooLarge(maxBytes: Int, maxLines: Int)
}

extension GitError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .commandFailed(let code, let stderr):
            let message = stderr.trimmingCharacters(in: .whitespacesAndNewlines)
            return message.isEmpty ? "Git exited with status \(code)." : message
        case .outputTooLarge(let maxBytes, let maxLines):
            return "Diff exceeds \(maxBytes) bytes or \(maxLines) lines."
        }
    }
}

public struct GitOutputLimits: Equatable, Sendable {
    public let maxBytes: Int
    public let maxLines: Int

    public init(maxBytes: Int, maxLines: Int) {
        self.maxBytes = maxBytes
        self.maxLines = maxLines
    }
}

public struct GitCommandResult: Equatable, Sendable {
    public let stdout: Data
    public let stderr: String
    public let exitCode: Int32

    public var stdoutString: String { String(decoding: stdout, as: UTF8.self) }
}
```

Add these private helpers below `StreamingState`:

```swift
private final class CapturedOutputState: @unchecked Sendable {
    private let lock = NSLock()
    private var output = Data()
    private var error = Data()
    private var lineCount = 0
    private(set) var exceeded = false

    func appendStdout(_ data: Data, limits: GitOutputLimits?) -> Bool {
        lock.lock(); defer { lock.unlock() }
        guard !exceeded else { return false }
        let newLines = data.reduce(into: 0) { count, byte in
            if byte == 0x0A { count += 1 }
        }
        if let limits,
           output.count + data.count > limits.maxBytes || lineCount + newLines > limits.maxLines {
            exceeded = true
            return true
        }
        output.append(data)
        lineCount += newLines
        return false
    }

    func appendStderr(_ data: Data) {
        lock.lock(); defer { lock.unlock() }
        error.append(data)
    }

    func snapshot() -> (stdout: Data, stderr: String, exceeded: Bool) {
        lock.lock(); defer { lock.unlock() }
        return (output, String(decoding: error, as: UTF8.self), exceeded)
    }
}

private final class SendableProcessBox: @unchecked Sendable {
    let process: Process
    init(_ process: Process) { self.process = process }
}
```

- [ ] **Step 5: Implement `runCaptured` and delegate `run` to it**

Replace the body of `GitRunner.run` and insert `runCaptured` before `runStreaming`:

```swift
    public static func run(_ arguments: [String], in directory: String) async throws -> String {
        try await runCaptured(arguments, in: directory).stdoutString
    }

    public static func runCaptured(
        _ arguments: [String],
        in directory: String,
        acceptedExitCodes: Set<Int32> = [0],
        limits: GitOutputLimits? = nil
    ) async throws -> GitCommandResult {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                let processBox = SendableProcessBox(process)
                process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
                process.arguments = arguments
                process.currentDirectoryURL = URL(fileURLWithPath: directory)
                let stdout = Pipe()
                let stderr = Pipe()
                process.standardOutput = stdout
                process.standardError = stderr
                let state = CapturedOutputState()

                do {
                    try process.run()
                } catch {
                    continuation.resume(throwing: error)
                    return
                }

                let readers = DispatchGroup()
                readers.enter()
                DispatchQueue.global(qos: .userInitiated).async {
                    while true {
                        let data = stdout.fileHandleForReading.availableData
                        if data.isEmpty { break }
                        if state.appendStdout(data, limits: limits) {
                            processBox.process.terminate()
                        }
                    }
                    readers.leave()
                }
                readers.enter()
                DispatchQueue.global(qos: .userInitiated).async {
                    while true {
                        let data = stderr.fileHandleForReading.availableData
                        if data.isEmpty { break }
                        state.appendStderr(data)
                    }
                    readers.leave()
                }

                process.waitUntilExit()
                readers.wait()
                let snapshot = state.snapshot()
                if snapshot.exceeded, let limits {
                    continuation.resume(throwing: GitError.outputTooLarge(
                        maxBytes: limits.maxBytes, maxLines: limits.maxLines))
                } else if acceptedExitCodes.contains(process.terminationStatus) {
                    continuation.resume(returning: GitCommandResult(
                        stdout: snapshot.stdout,
                        stderr: snapshot.stderr,
                        exitCode: process.terminationStatus))
                } else {
                    continuation.resume(throwing: GitError.commandFailed(
                        code: process.terminationStatus, stderr: snapshot.stderr))
                }
            }
        }
    }
```

Keep `runStreaming` unchanged.

- [ ] **Step 6: Run the full Git package suite**

Run:

```bash
swift test --package-path Packages/TillerGit
```

Expected: all existing runner/worktree/clone tests plus the new captured-result tests pass.

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerGit/Sources/TillerGit/GitRunner.swift \
  Packages/TillerGit/Tests/TillerGitTests/GitRunnerTests.swift \
  Packages/TillerGit/Tests/TillerGitTests/GitTestSupport.swift
git commit -m "feat: add bounded git output capture"
```

---

### Task 5: Typed repository metadata and status

**Files:**
- Create: `Packages/TillerGit/Sources/TillerGit/GitRepository.swift`
- Create: `Packages/TillerGit/Sources/TillerGit/GitStatus.swift`
- Create: `Packages/TillerGit/Tests/TillerGitTests/GitStatusTests.swift`

**Interfaces:**
- Consumes: `GitRunner.run` and `runCaptured` from Task 4.
- Produces: `GitPath`, `GitFileState`, `GitStatusEntry`, `GitStatusSnapshot`, `GitStatus.load(in:)`, `GitStatus.parse(_:)`, `GitRepository.gitDirectory(in:)`, `GitRepository.hasHead(in:)`.

- [ ] **Step 1: Write parser and live-status tests**

Create `GitStatusTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerGit

@Test func porcelainParserPreservesRenameAndSpecialPaths() throws {
    let raw = Data(" M changed file.txt\0R  new name.txt\0old name.txt\0?? line\nbreak.txt\0".utf8)
    let snapshot = try GitStatus.parse(raw)

    #expect(snapshot.entries.count == 3)
    #expect(snapshot.changes.map(\.path.value).contains("changed file.txt"))
    let renamed = try #require(snapshot.staged.first { $0.path.value == "new name.txt" })
    #expect(renamed.originalPath?.value == "old name.txt")
    #expect(snapshot.untracked.map(\.path.value).contains("line\nbreak.txt"))
}

@Test func porcelainParserClassifiesMixedStateInBothSections() throws {
    let snapshot = try GitStatus.parse(Data("MM file.txt\0UU conflict.txt\0".utf8))
    let mixed = try #require(snapshot.entries.first { $0.path.value == "file.txt" })
    #expect(snapshot.staged.contains(mixed))
    #expect(snapshot.changes.contains(mixed))
    #expect(snapshot.entries.first { $0.path.value == "conflict.txt" }?.isConflicted == true)
}

@Test func gitPathRejectsUnsafeValues() {
    #expect(throws: GitPathError.absolutePath("/tmp/x")) { _ = try GitPath("/tmp/x") }
    #expect(throws: GitPathError.parentTraversal("../x")) { _ = try GitPath("../x") }
    #expect(throws: GitPathError.nulByte) { _ = try GitPath("bad\0path") }
}

@Test func liveStatusIncludesStagedUnstagedAndUntracked() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "two\n".write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "file.txt"], in: repo)
    try "three\n".write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    try "new\n".write(to: repo.appendingPathComponent("new.txt"), atomically: true, encoding: .utf8)

    let snapshot = try await GitStatus.load(in: repo.path)

    #expect(snapshot.staged.map(\.path.value) == ["file.txt"])
    #expect(snapshot.changes.map(\.path.value) == ["file.txt"])
    #expect(snapshot.untracked.map(\.path.value) == ["new.txt"])
    let gitDirectory = try await GitRepository.gitDirectory(in: repo.path)
    let hasHead = try await GitRepository.hasHead(in: repo.path)
    #expect(FileManager.default.fileExists(atPath: gitDirectory.path))
    #expect(hasHead)
}
```

- [ ] **Step 2: Run focused tests and confirm failure**

Run:

```bash
swift test --package-path Packages/TillerGit --filter porcelain
swift test --package-path Packages/TillerGit --filter liveStatus
```

Expected: compilation fails because the metadata and status types do not exist.

- [ ] **Step 3: Implement repository metadata**

Create `GitRepository.swift`:

```swift
import Foundation

public enum GitRepository {
    public static func gitDirectory(in repoPath: String) async throws -> URL {
        let raw = try await GitRunner.run(
            ["rev-parse", "--path-format=absolute", "--git-dir"], in: repoPath)
        return URL(fileURLWithPath: raw.trimmingCharacters(in: .whitespacesAndNewlines),
                   isDirectory: true).standardizedFileURL
    }

    public static func hasHead(in repoPath: String) async throws -> Bool {
        let result = try await GitRunner.runCaptured(
            ["rev-parse", "--verify", "--quiet", "HEAD"],
            in: repoPath,
            acceptedExitCodes: [0, 1])
        return result.exitCode == 0
    }
}
```

- [ ] **Step 4: Implement validated paths and status parsing**

Create `GitStatus.swift`:

```swift
import Foundation

public enum GitPathError: Error, Equatable {
    case empty
    case nulByte
    case absolutePath(String)
    case parentTraversal(String)
}

extension GitPathError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .empty: "Git path is empty."
        case .nulByte: "Git path contains a NUL byte."
        case .absolutePath(let path): "Git path must be relative: \(path)"
        case .parentTraversal(let path): "Git path escapes the repository: \(path)"
        }
    }
}

public struct GitPath: Hashable, Comparable, Sendable {
    public let value: String

    public init(_ value: String) throws {
        guard !value.isEmpty else { throw GitPathError.empty }
        guard !value.utf8.contains(0) else { throw GitPathError.nulByte }
        guard !value.hasPrefix("/") else { throw GitPathError.absolutePath(value) }
        guard !value.split(separator: "/", omittingEmptySubsequences: false).contains("..") else {
            throw GitPathError.parentTraversal(value)
        }
        self.value = value
    }

    public static func < (lhs: GitPath, rhs: GitPath) -> Bool {
        lhs.value.localizedStandardCompare(rhs.value) == .orderedAscending
    }
}

public enum GitFileState: String, Equatable, Sendable {
    case modified
    case added
    case deleted
    case renamed
    case copied
    case typeChanged
    case unmerged
    case untracked
}

public struct GitStatusEntry: Hashable, Sendable {
    public let path: GitPath
    public let originalPath: GitPath?
    public let indexState: GitFileState?
    public let worktreeState: GitFileState?

    public var isStaged: Bool { indexState != nil && indexState != .untracked }
    public var hasWorktreeChanges: Bool { worktreeState != nil && worktreeState != .untracked }
    public var isUntracked: Bool { indexState == .untracked || worktreeState == .untracked }
    public var isConflicted: Bool { indexState == .unmerged || worktreeState == .unmerged }

    public var mutationPaths: [GitPath] {
        originalPath.map { [path, $0] } ?? [path]
    }
}

public struct GitStatusSnapshot: Equatable, Sendable {
    public let entries: [GitStatusEntry]
    public static let empty = GitStatusSnapshot(entries: [])

    public var staged: [GitStatusEntry] { entries.filter(\.isStaged) }
    public var changes: [GitStatusEntry] {
        entries.filter { $0.hasWorktreeChanges && !$0.isUntracked }
    }
    public var untracked: [GitStatusEntry] { entries.filter(\.isUntracked) }
    public var isClean: Bool { entries.isEmpty }
}

public enum GitStatusParseError: Error, Equatable {
    case malformedRecord
    case missingRenameSource(String)
    case unsupportedStatus(UInt8)
}

extension GitStatusParseError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .malformedRecord: "Git returned a malformed porcelain status record."
        case .missingRenameSource(let path): "Git omitted the rename source for \(path)."
        case .unsupportedStatus(let byte): "Git returned unsupported status byte \(byte)."
        }
    }
}

public enum GitStatus {
    public static func load(in repoPath: String) async throws -> GitStatusSnapshot {
        let result = try await GitRunner.runCaptured(
            ["status", "--porcelain=v1", "-z", "--untracked-files=all"], in: repoPath)
        return try parse(result.stdout)
    }

    public static func parse(_ data: Data) throws -> GitStatusSnapshot {
        let records = data.split(separator: 0, omittingEmptySubsequences: true)
        var index = 0
        var entries: [GitStatusEntry] = []
        while index < records.count {
            let record = records[index]
            guard record.count >= 4, record[record.startIndex + 2] == 0x20 else {
                throw GitStatusParseError.malformedRecord
            }
            let x = record[record.startIndex]
            let y = record[record.startIndex + 1]
            let conflicted = x == 0x55 || y == 0x55
                || [(0x44, 0x44), (0x41, 0x55), (0x55, 0x44),
                    (0x55, 0x41), (0x44, 0x55), (0x41, 0x41)].contains { $0 == (x, y) }
            let current = try GitPath(String(decoding: record.dropFirst(3), as: UTF8.self))
            let renameOrCopy = x == 0x52 || x == 0x43 || y == 0x52 || y == 0x43
            var original: GitPath?
            if renameOrCopy {
                index += 1
                guard index < records.count else {
                    throw GitStatusParseError.missingRenameSource(current.value)
                }
                original = try GitPath(String(decoding: records[index], as: UTF8.self))
            }
            entries.append(GitStatusEntry(
                path: current,
                originalPath: original,
                indexState: try state(for: x, conflicted: conflicted),
                worktreeState: try state(for: y, conflicted: conflicted)))
            index += 1
        }
        return GitStatusSnapshot(entries: entries.sorted { $0.path < $1.path })
    }

    private static func state(for byte: UInt8, conflicted: Bool) throws -> GitFileState? {
        if byte == 0x20 { return conflicted ? .unmerged : nil }
        if conflicted { return .unmerged }
        switch byte {
        case 0x4D: return .modified
        case 0x41: return .added
        case 0x44: return .deleted
        case 0x52: return .renamed
        case 0x43: return .copied
        case 0x54: return .typeChanged
        case 0x3F: return .untracked
        default: throw GitStatusParseError.unsupportedStatus(byte)
        }
    }
}
```

- [ ] **Step 5: Run the full status tests**

Run:

```bash
swift test --package-path Packages/TillerGit --filter GitStatus
swift test --package-path Packages/TillerGit
```

Expected: status-focused and complete package suites pass.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerGit/Sources/TillerGit/GitRepository.swift \
  Packages/TillerGit/Sources/TillerGit/GitStatus.swift \
  Packages/TillerGit/Tests/TillerGitTests/GitStatusTests.swift
git commit -m "feat: add typed git status snapshots"
```

---

### Task 6: Unified per-file Git diff

**Files:**
- Create: `Packages/TillerGit/Sources/TillerGit/GitDiff.swift`
- Create: `Packages/TillerGit/Tests/TillerGitTests/GitDiffTests.swift`

**Interfaces:**
- Consumes: `GitRepository.hasHead`, `GitStatusEntry`, `GitRunner.runCaptured`.
- Produces: `GitDiffLineKind`, `GitDiffLine`, `GitFileDiff`, `GitDiff.load(entry:in:)`, `GitDiff.parse(_:path:)`, `GitDiff.outputLimits`.

- [ ] **Step 1: Write parser and live-diff tests**

Create `GitDiffTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerGit

@Test func unifiedParserTracksLineNumbersAndCounts() throws {
    let patch = """
    diff --git a/file.txt b/file.txt
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,3 @@
     one
    -two
    +second
    +three
    """
    let diff = try GitDiff.parse(patch, path: GitPath("file.txt"))
    let changes = diff.lines.filter { $0.kind == .addition || $0.kind == .deletion }

    #expect(diff.additions == 2)
    #expect(diff.deletions == 1)
    #expect(changes[0].oldLineNumber == 2)
    #expect(changes[0].newLineNumber == nil)
    #expect(changes[1].oldLineNumber == nil)
    #expect(changes[1].newLineNumber == 2)
}

@Test func parserRecognizesBinaryAndSubmoduleSummary() throws {
    let binary = try GitDiff.parse(
        "Binary files a/image.png and b/image.png differ\n", path: GitPath("image.png"))
    let submodule = try GitDiff.parse(
        "@@ -1 +1 @@\n-Subproject commit aaa\n+Subproject commit bbb\n", path: GitPath("Vendor"))
    #expect(binary.isBinary)
    #expect(submodule.isSubmodule)
}

@Test func liveDiffCombinesStagedAndUnstagedChanges() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "two\n".write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "file.txt"], in: repo)
    try "three\n".write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.entries.first)

    let diff = try await GitDiff.load(entry: entry, in: repo.path)

    #expect(diff.lines.contains { $0.kind == .deletion && $0.text == "one" })
    #expect(diff.lines.contains { $0.kind == .addition && $0.text == "three" })
}

@Test func untrackedAndUnbornFilesDiffFromDevNull() async throws {
    let repo = try makeGitTestRepository(withCommit: false)
    defer { try? FileManager.default.removeItem(at: repo) }
    try "new\n".write(to: repo.appendingPathComponent("new.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.untracked.first)

    let diff = try await GitDiff.load(entry: entry, in: repo.path)

    #expect(diff.deletions == 0)
    #expect(diff.lines.contains { $0.kind == .addition && $0.text == "new" })
}
```

- [ ] **Step 2: Run focused tests and confirm failure**

Run:

```bash
swift test --package-path Packages/TillerGit --filter Diff
```

Expected: compilation fails because the Git diff types and API do not exist.

- [ ] **Step 3: Implement bounded diff loading and parsing**

Create `GitDiff.swift`:

```swift
import Foundation

public enum GitDiffLineKind: Equatable, Sendable {
    case metadata
    case hunk
    case context
    case addition
    case deletion
}

public struct GitDiffLine: Identifiable, Equatable, Sendable {
    public let id: Int
    public let kind: GitDiffLineKind
    public let oldLineNumber: Int?
    public let newLineNumber: Int?
    public let text: String
}

public struct GitFileDiff: Equatable, Sendable {
    public let path: GitPath
    public let lines: [GitDiffLine]
    public let additions: Int
    public let deletions: Int
    public let isBinary: Bool
    public let isSubmodule: Bool
}

public enum GitDiff {
    public static let outputLimits = GitOutputLimits(
        maxBytes: 5 * 1024 * 1024,
        maxLines: 20_000)

    public static func load(entry: GitStatusEntry, in repoPath: String) async throws -> GitFileDiff {
        let hasHead = try await GitRepository.hasHead(in: repoPath)
        let result: GitCommandResult
        if entry.isUntracked || !hasHead {
            let absolute = URL(fileURLWithPath: repoPath, isDirectory: true)
                .appendingPathComponent(entry.path.value).path
            result = try await GitRunner.runCaptured(
                ["diff", "--no-color", "--no-ext-diff", "--no-index", "--unified=3",
                 "--", "/dev/null", absolute],
                in: repoPath,
                acceptedExitCodes: [0, 1],
                limits: outputLimits)
        } else {
            result = try await GitRunner.runCaptured(
                ["diff", "--no-color", "--no-ext-diff", "--unified=3", "HEAD", "--"]
                    + entry.mutationPaths.map(\.value),
                in: repoPath,
                limits: outputLimits)
        }
        return try parse(result.stdoutString, path: entry.path)
    }

    public static func parse(_ patch: String, path: GitPath) throws -> GitFileDiff {
        let rawLines = patch.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        var lines: [GitDiffLine] = []
        var oldLine: Int?
        var newLine: Int?
        var additions = 0
        var deletions = 0
        var isBinary = false
        var isSubmodule = false

        for raw in rawLines where !raw.isEmpty {
            let id = lines.count
            if raw.hasPrefix("Binary files ") || raw == "GIT binary patch" {
                isBinary = true
                lines.append(GitDiffLine(id: id, kind: .metadata,
                                         oldLineNumber: nil, newLineNumber: nil, text: raw))
            } else if raw.hasPrefix("@@ "), let starts = hunkStarts(raw) {
                oldLine = starts.old
                newLine = starts.new
                lines.append(GitDiffLine(id: id, kind: .hunk,
                                         oldLineNumber: nil, newLineNumber: nil, text: raw))
            } else if oldLine != nil, raw.hasPrefix("+") && !raw.hasPrefix("+++") {
                let text = String(raw.dropFirst())
                if text.hasPrefix("Subproject commit ") { isSubmodule = true }
                lines.append(GitDiffLine(id: id, kind: .addition,
                                         oldLineNumber: nil, newLineNumber: newLine, text: text))
                additions += 1
                newLine = newLine.map { $0 + 1 }
            } else if oldLine != nil, raw.hasPrefix("-") && !raw.hasPrefix("---") {
                let text = String(raw.dropFirst())
                if text.hasPrefix("Subproject commit ") { isSubmodule = true }
                lines.append(GitDiffLine(id: id, kind: .deletion,
                                         oldLineNumber: oldLine, newLineNumber: nil, text: text))
                deletions += 1
                oldLine = oldLine.map { $0 + 1 }
            } else if oldLine != nil, raw.hasPrefix(" ") {
                lines.append(GitDiffLine(id: id, kind: .context,
                                         oldLineNumber: oldLine, newLineNumber: newLine,
                                         text: String(raw.dropFirst())))
                oldLine = oldLine.map { $0 + 1 }
                newLine = newLine.map { $0 + 1 }
            } else {
                lines.append(GitDiffLine(id: id, kind: .metadata,
                                         oldLineNumber: nil, newLineNumber: nil, text: raw))
            }
        }
        return GitFileDiff(path: path, lines: lines, additions: additions,
                           deletions: deletions, isBinary: isBinary, isSubmodule: isSubmodule)
    }

    private static func hunkStarts(_ line: String) -> (old: Int, new: Int)? {
        let fields = line.split(separator: " ")
        guard fields.count >= 3,
              fields[1].first == "-", fields[2].first == "+" else { return nil }
        func start(_ field: Substring) -> Int? {
            let digits = field.dropFirst().split(separator: ",", maxSplits: 1).first
            return digits.flatMap { Int($0) }
        }
        guard let old = start(fields[1]), let new = start(fields[2]) else { return nil }
        return (old, new)
    }
}
```

- [ ] **Step 4: Run diff tests and the Git package suite**

Run:

```bash
swift test --package-path Packages/TillerGit --filter Diff
swift test --package-path Packages/TillerGit
```

Expected: all commands pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerGit/Sources/TillerGit/GitDiff.swift \
  Packages/TillerGit/Tests/TillerGitTests/GitDiffTests.swift
git commit -m "feat: add unified worktree diffs"
```

---

### Task 7: Safe Git status mutations

**Files:**
- Create: `Packages/TillerGit/Sources/TillerGit/GitActions.swift`
- Create: `Packages/TillerGit/Tests/TillerGitTests/GitActionsTests.swift`

**Interfaces:**
- Consumes: status-derived `GitStatusEntry`, `GitRepository.hasHead`, `GitRunner.run`.
- Produces: `GitActions.stage(_:in:)`, `unstage(_:in:)`, `discardChanges(_:in:)`, `discardUntracked(_:in:)` and typed `GitActionError`.

- [ ] **Step 1: Write real-repository mutation tests**

Create `GitActionsTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerGit

@Test func discardChangesPreservesStagedContent() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("file.txt")
    try "two\n".write(to: file, atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "file.txt"], in: repo)
    try "three\n".write(to: file, atomically: true, encoding: .utf8)
    let beforeDiscard = try await GitStatus.load(in: repo.path)
    let entry = try #require(beforeDiscard.changes.first)

    try await GitActions.discardChanges([entry], in: repo.path)

    #expect(try String(contentsOf: file, encoding: .utf8) == "two\n")
    let afterDiscard = try await GitStatus.load(in: repo.path)
    #expect(afterDiscard.staged.map(\.path.value) == ["file.txt"])
}

@Test func stageAndUnstageLeaveWorkingFileIntact() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("file.txt")
    try "two\n".write(to: file, atomically: true, encoding: .utf8)
    let beforeStage = try await GitStatus.load(in: repo.path)
    var entry = try #require(beforeStage.changes.first)
    try await GitActions.stage([entry], in: repo.path)
    let afterStage = try await GitStatus.load(in: repo.path)
    entry = try #require(afterStage.staged.first)

    try await GitActions.unstage([entry], in: repo.path)

    #expect(try String(contentsOf: file, encoding: .utf8) == "two\n")
    let afterUnstage = try await GitStatus.load(in: repo.path)
    #expect(afterUnstage.changes.map(\.path.value) == ["file.txt"])
}

@Test func unbornUnstageKeepsFileAndReturnsItToUntracked() async throws {
    let repo = try makeGitTestRepository(withCommit: false)
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("new.txt")
    try "new\n".write(to: file, atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "new.txt"], in: repo)
    let beforeUnstage = try await GitStatus.load(in: repo.path)
    let entry = try #require(beforeUnstage.staged.first)

    try await GitActions.unstage([entry], in: repo.path)

    #expect(FileManager.default.fileExists(atPath: file.path))
    let afterUnstage = try await GitStatus.load(in: repo.path)
    #expect(afterUnstage.untracked.map(\.path.value) == ["new.txt"])
}

@Test func discardUntrackedRemovesOnlySnapshotPaths() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "remove\n".write(to: repo.appendingPathComponent("remove.txt"), atomically: true, encoding: .utf8)
    try "keep\n".write(to: repo.appendingPathComponent("keep.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let remove = try #require(snapshot.untracked.first { $0.path.value == "remove.txt" })

    try await GitActions.discardUntracked([remove], in: repo.path)

    #expect(!FileManager.default.fileExists(atPath: repo.appendingPathComponent("remove.txt").path))
    #expect(FileManager.default.fileExists(atPath: repo.appendingPathComponent("keep.txt").path))
}

@Test func unstageRenameUsesCurrentAndOriginalPaths() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try runGitForTest(["mv", "file.txt", "renamed.txt"], in: repo)
    try runGitForTest(["add", "-A", "--", "file.txt", "renamed.txt"], in: repo)
    let beforeUnstage = try await GitStatus.load(in: repo.path)
    let rename = try #require(beforeUnstage.staged.first {
        $0.path.value == "renamed.txt"
    })
    #expect(rename.originalPath?.value == "file.txt")

    try await GitActions.unstage([rename], in: repo.path)

    let afterUnstage = try await GitStatus.load(in: repo.path)
    #expect(afterUnstage.staged.isEmpty)
    #expect(afterUnstage.changes.map(\.path.value).contains("file.txt"))
    #expect(afterUnstage.untracked.map(\.path.value).contains("renamed.txt"))
}
```

- [ ] **Step 2: Run focused tests and confirm failure**

Run:

```bash
swift test --package-path Packages/TillerGit --filter GitActions
```

Expected: compilation fails because `GitActions` does not exist.

- [ ] **Step 3: Implement the four mutation operations**

Create `GitActions.swift`:

```swift
import Foundation

public enum GitActionError: Error, Equatable {
    case emptySelection
    case invalidEntry(String)
}

extension GitActionError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .emptySelection: "No Git paths were selected."
        case .invalidEntry(let message): message
        }
    }
}

public enum GitActions {
    public static func stage(_ entries: [GitStatusEntry], in repoPath: String) async throws {
        let paths = try pathArguments(entries)
        _ = try await GitRunner.run(["add", "-A", "--"] + paths, in: repoPath)
    }

    public static func unstage(_ entries: [GitStatusEntry], in repoPath: String) async throws {
        guard entries.allSatisfy(\.isStaged) else {
            throw GitActionError.invalidEntry("Unstage requires staged entries.")
        }
        let paths = try pathArguments(entries)
        if try await GitRepository.hasHead(in: repoPath) {
            _ = try await GitRunner.run(["restore", "--staged", "--"] + paths, in: repoPath)
        } else {
            _ = try await GitRunner.run(
                ["rm", "--cached", "--force", "--"] + paths, in: repoPath)
        }
    }

    public static func discardChanges(
        _ entries: [GitStatusEntry], in repoPath: String
    ) async throws {
        guard entries.allSatisfy({ $0.hasWorktreeChanges && !$0.isUntracked }) else {
            throw GitActionError.invalidEntry("Discard Changes requires tracked worktree changes.")
        }
        _ = try await GitRunner.run(
            ["restore", "--worktree", "--"] + pathArguments(entries), in: repoPath)
    }

    public static func discardUntracked(
        _ entries: [GitStatusEntry], in repoPath: String
    ) async throws {
        guard entries.allSatisfy(\.isUntracked) else {
            throw GitActionError.invalidEntry("Discard Untracked requires untracked entries.")
        }
        _ = try await GitRunner.run(
            ["clean", "-f", "-d", "--"] + pathArguments(entries), in: repoPath)
    }

    private static func pathArguments(_ entries: [GitStatusEntry]) throws -> [String] {
        guard !entries.isEmpty else { throw GitActionError.emptySelection }
        var seen: Set<GitPath> = []
        var output: [String] = []
        for path in entries.flatMap(\.mutationPaths) where seen.insert(path).inserted {
            output.append(path.value)
        }
        return output
    }
}
```

- [ ] **Step 4: Run mutation tests and the Git suite**

Run:

```bash
swift test --package-path Packages/TillerGit --filter GitActions
swift test --package-path Packages/TillerGit
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerGit/Sources/TillerGit/GitActions.swift \
  Packages/TillerGit/Tests/TillerGitTests/GitActionsTests.swift
git commit -m "feat: add safe git status actions"
```

---

### Task 8: Worktree-scoped right-panel coordinator

**Files:**
- Create: `App/RightPanel/RightPanelModel.swift`

**Interfaces:**
- Consumes: `Worktree`, `FileTreeLoader`, `FileSystemEventMonitor`, `GitRepository`, `GitStatus`, `GitDiff`, `GitActions`.
- Produces: `RightPanelModel.activate(worktree:isGitRepository:)`, `deactivate()`, `visibleRows`, directory expansion, diff selection, refresh, and serialized mutation methods consumed by Tasks 9-10.

- [ ] **Step 1: Create the observable state and lifecycle**

Create `RightPanelModel.swift` with these complete state types and lifecycle methods:

```swift
import Foundation
import Observation
import TillerCore
import TillerGit

struct FileExplorerRow: Identifiable, Equatable {
    let node: FileTreeNode
    let depth: Int
    var id: String { node.id }
}

@MainActor @Observable
final class RightPanelModel {
    private(set) var worktree: Worktree?
    private(set) var isGitRepository = false
    private(set) var childrenByDirectory: [String: [FileTreeNode]] = [:]
    private(set) var expandedDirectories: Set<String> = []
    private(set) var directoryErrors: [String: String] = [:]
    private(set) var status = GitStatusSnapshot.empty
    private(set) var selectedDiffPath: GitPath?
    private(set) var diff: GitFileDiff?
    private(set) var filesLoading = false
    private(set) var gitLoading = false
    private(set) var diffLoading = false
    private(set) var mutationInProgress = false
    var filesError: String?
    var gitError: String?
    var diffError: String?
    var monitorError: String?

    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var monitor: FileSystemEventMonitor?
    @ObservationIgnored private var monitorTask: Task<Void, Never>?
    @ObservationIgnored private var debounceTask: Task<Void, Never>?
    @ObservationIgnored private var pendingPaths: Set<String> = []

    var rootURL: URL? {
        worktree.map { URL(fileURLWithPath: $0.path, isDirectory: true).standardizedFileURL }
    }

    var visibleRows: [FileExplorerRow] {
        var rows: [FileExplorerRow] = []
        func append(_ key: String, depth: Int) {
            for node in childrenByDirectory[key] ?? [] {
                rows.append(FileExplorerRow(node: node, depth: depth))
                if node.kind.isDirectory, expandedDirectories.contains(node.relativePath) {
                    append(node.relativePath, depth: depth + 1)
                }
            }
        }
        append("", depth: 0)
        return rows
    }

    func activate(worktree: Worktree?, isGitRepository: Bool) async {
        guard self.worktree?.id != worktree?.id || self.isGitRepository != isGitRepository else {
            return
        }
        deactivate()
        guard let worktree else { return }
        self.worktree = worktree
        self.isGitRepository = isGitRepository
        let token = generation
        await loadInitial(token: token)
        guard token == generation else { return }
        await startMonitor(token: token)
    }

    func deactivate() {
        generation += 1
        monitorTask?.cancel()
        monitorTask = nil
        debounceTask?.cancel()
        debounceTask = nil
        monitor?.stop()
        monitor = nil
        pendingPaths.removeAll()
        worktree = nil
        isGitRepository = false
        childrenByDirectory = [:]
        expandedDirectories = []
        directoryErrors = [:]
        status = .empty
        selectedDiffPath = nil
        diff = nil
        filesLoading = false
        gitLoading = false
        diffLoading = false
        mutationInProgress = false
        filesError = nil
        gitError = nil
        diffError = nil
        monitorError = nil
    }
}
```

- [ ] **Step 2: Add lazy loading, targeted invalidation, and debounce**

Add this extension in the same file:

```swift
extension RightPanelModel {
    func toggleDirectory(_ path: String) async {
        if expandedDirectories.remove(path) != nil { return }
        expandedDirectories.insert(path)
        if childrenByDirectory[path] == nil { await loadDirectory(path, token: generation) }
    }

    func refresh() async {
        await refresh(changedPaths: [], token: generation, forceAllLoadedDirectories: true)
    }

    private func loadInitial(token: Int) async {
        guard let rootURL, let worktree else { return }
        filesLoading = true
        gitLoading = isGitRepository
        let fileTask = Task.detached(priority: .userInitiated) {
            try FileTreeLoader.children(at: "", rootURL: rootURL)
        }
        let statusTask = Task {
            isGitRepository ? try await GitStatus.load(in: worktree.path) : .empty
        }

        do {
            let nodes = try await fileTask.value
            guard token == generation else { return }
            childrenByDirectory[""] = nodes
            filesError = nil
        } catch {
            guard token == generation else { return }
            filesError = error.localizedDescription
        }
        filesLoading = false

        do {
            let snapshot = try await statusTask.value
            guard token == generation else { return }
            apply(snapshot)
            gitError = nil
        } catch {
            guard token == generation else { return }
            gitError = error.localizedDescription
        }
        gitLoading = false
    }

    private func loadDirectory(_ key: String, token: Int) async {
        guard let rootURL else { return }
        do {
            let nodes = try await Task.detached(priority: .userInitiated) {
                try FileTreeLoader.children(at: key, rootURL: rootURL)
            }.value
            guard token == generation else { return }
            childrenByDirectory[key] = nodes
            directoryErrors[key] = nil
        } catch {
            guard token == generation else { return }
            directoryErrors[key] = error.localizedDescription
        }
    }

    private func startMonitor(token: Int) async {
        guard let rootURL, let worktree else { return }
        monitorError = nil
        var roots = [rootURL]
        if isGitRepository {
            do {
                roots.append(try await GitRepository.gitDirectory(in: worktree.path))
            } catch {
                monitorError = "Git metadata monitoring unavailable: \(error.localizedDescription)"
            }
        }
        guard token == generation else { return }
        guard let monitor = FileSystemEventMonitor(roots: Array(Set(roots))) else {
            monitorError = "Automatic filesystem refresh is unavailable."
            return
        }
        self.monitor = monitor
        monitorTask = Task { [weak self] in
            for await urls in monitor.events {
                guard !Task.isCancelled else { return }
                await self?.scheduleRefresh(paths: urls.map(\.path), token: token)
            }
        }
    }

    private func scheduleRefresh(paths: [String], token: Int) {
        guard token == generation else { return }
        pendingPaths.formUnion(paths)
        debounceTask?.cancel()
        debounceTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(250))
            guard !Task.isCancelled, let self, token == self.generation else { return }
            let paths = Array(self.pendingPaths)
            self.pendingPaths.removeAll()
            await self.refresh(changedPaths: paths, token: token, forceAllLoadedDirectories: false)
        }
    }

    private func refresh(
        changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool
    ) async {
        guard token == generation, let rootURL, let worktree else { return }
        let loadedKeys = childrenByDirectory.keys.filter { key in
            if forceAllLoadedDirectories || changedPaths.isEmpty { return true }
            let directory = key.isEmpty ? rootURL.path : rootURL.appendingPathComponent(key).path
            return changedPaths.contains { changed in
                changed == directory || changed.hasPrefix(directory + "/")
                    || directory.hasPrefix(changed + "/")
            }
        }
        for key in loadedKeys { await loadDirectory(key, token: token) }
        guard token == generation, isGitRepository else { return }
        do {
            let snapshot = try await GitStatus.load(in: worktree.path)
            guard token == generation else { return }
            apply(snapshot)
            gitError = nil
            if let selected = selectedEntry, diff != nil { await loadDiff(selected) }
        } catch {
            guard token == generation else { return }
            gitError = error.localizedDescription
        }
    }
}
```

- [ ] **Step 3: Add status reconciliation, diff loading, and mutations**

Add the final extension:

```swift
extension RightPanelModel {
    var allChangedEntries: [GitStatusEntry] { status.entries }

    var selectedEntry: GitStatusEntry? {
        guard let selectedDiffPath else { return nil }
        return status.entries.first { $0.path == selectedDiffPath }
    }

    func selectDiff(_ entry: GitStatusEntry) async {
        selectedDiffPath = entry.path
        await loadDiff(entry)
    }

    func ensureDiffLoaded() async {
        if let selectedEntry {
            if diff?.path != selectedEntry.path { await loadDiff(selectedEntry) }
        } else if let first = status.entries.first {
            await selectDiff(first)
        }
    }

    func stage(_ entries: [GitStatusEntry]) async {
        await mutate { try await GitActions.stage(entries, in: $0) }
    }

    func unstage(_ entries: [GitStatusEntry]) async {
        await mutate { try await GitActions.unstage(entries, in: $0) }
    }

    func discardChanges(_ entries: [GitStatusEntry]) async {
        await mutate { try await GitActions.discardChanges(entries, in: $0) }
    }

    func discardUntracked(_ entries: [GitStatusEntry]) async {
        await mutate { try await GitActions.discardUntracked(entries, in: $0) }
    }

    private func apply(_ snapshot: GitStatusSnapshot) {
        status = snapshot
        if let selectedDiffPath,
           snapshot.entries.contains(where: { $0.path == selectedDiffPath }) {
            return
        }
        selectedDiffPath = snapshot.entries.first?.path
        diff = nil
        diffError = nil
    }

    private func loadDiff(_ entry: GitStatusEntry) async {
        guard let worktree else { return }
        let token = generation
        diffLoading = true
        defer { if token == generation { diffLoading = false } }
        do {
            let loaded = try await GitDiff.load(entry: entry, in: worktree.path)
            guard token == generation, selectedDiffPath == entry.path else { return }
            diff = loaded
            diffError = nil
        } catch {
            guard token == generation, selectedDiffPath == entry.path else { return }
            diff = nil
            diffError = error.localizedDescription
        }
    }

    private func mutate(
        _ operation: @escaping @Sendable (String) async throws -> Void
    ) async {
        guard !mutationInProgress, let worktree else { return }
        mutationInProgress = true
        defer { mutationInProgress = false }
        do {
            try await operation(worktree.path)
            gitError = nil
            await refresh(changedPaths: [], token: generation,
                          forceAllLoadedDirectories: true)
        } catch {
            gitError = error.localizedDescription
            await refresh(changedPaths: [], token: generation,
                          forceAllLoadedDirectories: true)
        }
    }
}
```

- [ ] **Step 4: Regenerate and compile**

Run:

```bash
xcodegen generate
Scripts/ci.sh
```

Expected: `CI OK`. No view uses the model yet, but the complete coordinator compiles against the package APIs.

- [ ] **Step 5: Commit**

```bash
git add App/RightPanel/RightPanelModel.swift
git commit -m "feat: coordinate right panel worktree data"
```

---

### Task 9: File explorer UI

**Files:**
- Create: `App/RightPanel/FileExplorerView.swift`
- Modify: `App/AppTheme.swift:12-63`

**Interfaces:**
- Consumes: `RightPanelModel`, `AppModel.openMarkdownTab`, `MarkdownFileLink.isMarkdown`, and `GitStatusEntry`.
- Produces: an independently compilable Files mode consumed by `RightPanelView` in Task 10.

- [ ] **Step 1: Add semantic Git status colors**

Insert before `AppTheme.dynamic`:

```swift
    static let gitStaged = dynamic(
        light: NSColor(srgbRed: 0.08, green: 0.42, blue: 0.20, alpha: 1),
        dark: NSColor(srgbRed: 0.55, green: 0.82, blue: 0.63, alpha: 1))
    static let gitModified = dynamic(
        light: NSColor(srgbRed: 0.58, green: 0.35, blue: 0.05, alpha: 1),
        dark: NSColor(srgbRed: 0.91, green: 0.69, blue: 0.36, alpha: 1))
    static let gitUntracked = dynamic(
        light: NSColor(srgbRed: 0.08, green: 0.36, blue: 0.60, alpha: 1),
        dark: NSColor(srgbRed: 0.43, green: 0.68, blue: 0.91, alpha: 1))
    static let gitConflict = dynamic(
        light: NSColor(srgbRed: 0.62, green: 0.12, blue: 0.16, alpha: 1),
        dark: NSColor(srgbRed: 0.90, green: 0.58, blue: 0.60, alpha: 1))
```

- [ ] **Step 2: Create the flattened lazy file explorer**

Create `FileExplorerView.swift`:

```swift
import AppKit
import SwiftUI
import TillerCore
import TillerGit

struct FileExplorerView: View {
    @Bindable var appModel: AppModel
    @Bindable var panelModel: RightPanelModel
    let worktree: Worktree
    @State private var selectedPath: String?
    @FocusState private var treeFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(worktree.path)
                    .font(.system(size: 11))
                    .foregroundStyle(AppTheme.meta)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                Button {
                    Task { await panelModel.refresh() }
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .help("Refresh Files")
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)

            if panelModel.filesLoading && panelModel.childrenByDirectory.isEmpty {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = panelModel.filesError {
                ContentUnavailableView {
                    Label("Files unavailable", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                } actions: {
                    Button("Riprova") { Task { await panelModel.refresh() } }
                }
            } else {
                ScrollView {
                    LazyVStack(spacing: 1) {
                        ForEach(panelModel.visibleRows) { row in
                            fileRow(row)
                        }
                    }
                    .padding(.vertical, 4)
                }
                .focusable()
                .focused($treeFocused)
                .onKeyPress(.downArrow) { moveSelection(1); return .handled }
                .onKeyPress(.upArrow) { moveSelection(-1); return .handled }
                .onKeyPress(.space) { toggleSelectedDirectory(); return .handled }
                .onKeyPress(.return) { openSelected(); return .handled }
            }
        }
    }

    private func fileRow(_ row: FileExplorerRow) -> some View {
        let node = row.node
        let selected = selectedPath == node.relativePath
        return HStack(spacing: 6) {
            if node.kind.isDirectory {
                Image(systemName: panelModel.expandedDirectories.contains(node.relativePath)
                      ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .frame(width: 10)
            } else {
                Color.clear.frame(width: 10, height: 1)
            }
            Image(systemName: icon(for: node))
                .foregroundStyle(node.kind.isDirectory ? AppTheme.meta : AppTheme.subtitle)
                .frame(width: 14)
            Text(node.name)
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 4)
            if let entry = panelModel.status.entries.first(where: {
                $0.path.value == node.relativePath
            }) {
                Text(statusSymbol(entry))
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundStyle(statusColor(entry))
                    .help(statusLabel(entry))
            }
        }
        .padding(.leading, CGFloat(row.depth) * 14 + 8)
        .padding(.trailing, 8)
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        .background(selected ? AppTheme.selectionFill : Color.clear,
                    in: RoundedRectangle(cornerRadius: 6))
        .onTapGesture { selectedPath = node.relativePath; treeFocused = true }
        .onTapGesture(count: 2) {
            selectedPath = node.relativePath
            if node.kind.isDirectory {
                Task { await panelModel.toggleDirectory(node.relativePath) }
            } else {
                open(node)
            }
        }
        .contextMenu {
            Button("Apri") { open(node) }
                .disabled(node.kind.isDirectory)
            Button("Mostra nel Finder") { reveal(node) }
            Button("Copia percorso") { copyPath(node) }
        }
        .overlay(alignment: .bottomLeading) {
            if let error = panelModel.directoryErrors[node.relativePath] {
                Text(error)
                    .font(.caption2)
                    .foregroundStyle(AppTheme.gitConflict)
                    .padding(.leading, CGFloat(row.depth) * 14 + 38)
            }
        }
    }

    private func moveSelection(_ delta: Int) {
        let rows = panelModel.visibleRows
        guard !rows.isEmpty else { return }
        let current = rows.firstIndex {
            $0.node.relativePath == selectedPath
        } ?? (delta > 0 ? -1 : 0)
        selectedPath = rows[min(max(current + delta, 0), rows.count - 1)].node.relativePath
    }

    private func toggleSelectedDirectory() {
        guard let row = panelModel.visibleRows.first(where: {
            $0.node.relativePath == selectedPath
        }), row.node.kind.isDirectory else { return }
        Task { await panelModel.toggleDirectory(row.node.relativePath) }
    }

    private func openSelected() {
        guard let row = panelModel.visibleRows.first(where: {
            $0.node.relativePath == selectedPath
        }) else { return }
        if row.node.kind.isDirectory { toggleSelectedDirectory() } else { open(row.node) }
    }

    private func open(_ node: FileTreeNode) {
        guard let root = panelModel.rootURL else { return }
        let url = node.url(relativeTo: root)
        if MarkdownFileLink.isMarkdown(url) {
            appModel.openMarkdownTab(fileURL: url, in: worktree)
        } else {
            NSWorkspace.shared.open(url)
        }
    }

    private func reveal(_ node: FileTreeNode) {
        guard let root = panelModel.rootURL else { return }
        NSWorkspace.shared.activateFileViewerSelecting([node.url(relativeTo: root)])
    }

    private func copyPath(_ node: FileTreeNode) {
        guard let root = panelModel.rootURL else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(node.url(relativeTo: root).path, forType: .string)
    }

    private func icon(for node: FileTreeNode) -> String {
        switch node.kind {
        case .directory: "folder"
        case .symbolicLink: "link"
        case .file: "doc"
        }
    }

    private func statusSymbol(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "U" }
        if entry.isUntracked { return "?" }
        if entry.indexState == .added { return "A" }
        if entry.indexState == .deleted || entry.worktreeState == .deleted { return "D" }
        if entry.indexState == .renamed || entry.worktreeState == .renamed { return "R" }
        return "M"
    }

    private func statusLabel(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "Conflicted" }
        if entry.isUntracked { return "Untracked" }
        if entry.isStaged && entry.hasWorktreeChanges { return "Staged and modified" }
        if entry.isStaged { return "Staged" }
        return "Modified"
    }

    private func statusColor(_ entry: GitStatusEntry) -> Color {
        if entry.isConflicted { return AppTheme.gitConflict }
        if entry.isUntracked { return AppTheme.gitUntracked }
        if entry.isStaged { return AppTheme.gitStaged }
        return AppTheme.gitModified
    }
}
```

- [ ] **Step 3: Regenerate and compile the Files mode**

Run:

```bash
xcodegen generate
Scripts/ci.sh
```

Expected: `CI OK`.

- [ ] **Step 4: Commit**

```bash
git add App/AppTheme.swift App/RightPanel/FileExplorerView.swift
git commit -m "feat: add right panel file explorer"
```

---
### Task 10: Git Diff, Status, and panel shell UI

**Files:**
- Create: `App/RightPanel/RightPanelView.swift`
- Create: `App/RightPanel/GitDiffView.swift`
- Create: `App/RightPanel/GitStatusView.swift`
- Modify: `App/AppTheme.swift:12-75`

**Interfaces:**
- Consumes: Task 8 model methods, Task 9 `FileExplorerView`, and `GitStatusSnapshot` sections.
- Produces: `RightPanelView(appModel:panelModel:modeRaw:isGitRepository:onClose:)` with Files/Diff/Status routing and one discard-confirmation path.

- [ ] **Step 1: Add unified-diff colors**

Insert with the Git colors added in Task 9, before `AppTheme.dynamic`:

```swift
    static let diffAddition = gitStaged
    static let diffAdditionBackground = dynamic(
        light: NSColor(srgbRed: 0.88, green: 0.96, blue: 0.90, alpha: 1),
        dark: NSColor(srgbRed: 0.08, green: 0.24, blue: 0.14, alpha: 1))
    static let diffDeletion = gitConflict
    static let diffDeletionBackground = dynamic(
        light: NSColor(srgbRed: 0.98, green: 0.89, blue: 0.90, alpha: 1),
        dark: NSColor(srgbRed: 0.27, green: 0.08, blue: 0.10, alpha: 1))
    static let diffHunkBackground = dynamic(
        light: NSColor(srgbRed: 0.88, green: 0.92, blue: 0.98, alpha: 1),
        dark: NSColor(srgbRed: 0.10, green: 0.17, blue: 0.28, alpha: 1))
```

- [ ] **Step 2: Create Status sections and the shared discard request**

Create `GitStatusView.swift`:

```swift
import SwiftUI
import TillerGit

struct PendingGitDiscard: Identifiable {
    enum Kind: Equatable { case changes, untracked }

    let id = UUID()
    let kind: Kind
    let entries: [GitStatusEntry]

    var title: String {
        entries.count == 1
            ? "Discard \(entries[0].path.value)?"
            : "Discard \(entries.count) files?"
    }

    var message: String {
        kind == .untracked
            ? "Untracked files will be permanently deleted."
            : "Unstaged changes will be restored from the Git index."
    }
}

struct GitStatusView: View {
    private enum SectionKind: Equatable { case staged, changes, untracked }

    @Bindable var panelModel: RightPanelModel
    let onOpenDiff: (GitStatusEntry) -> Void
    let requestDiscard: (PendingGitDiscard) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(panelModel.status.isClean ? "Working tree clean" : "Local changes")
                    .font(.system(size: 12, weight: .semibold))
                Spacer()
                Button { Task { await panelModel.refresh() } } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .help("Refresh Status")
            }
            .padding(8)
            Divider()

            if panelModel.gitLoading && panelModel.status.isClean {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = panelModel.gitError {
                ContentUnavailableView {
                    Label("Git status unavailable", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                } actions: {
                    Button("Riprova") { Task { await panelModel.refresh() } }
                }
            } else if panelModel.status.isClean {
                ContentUnavailableView("Working tree clean", systemImage: "checkmark.circle")
            } else {
                ScrollView {
                    LazyVStack(spacing: 10) {
                        statusSection(
                            "Staged", kind: .staged, entries: panelModel.status.staged,
                            actionTitle: "Unstage all"
                        ) { entries in
                            Task { await panelModel.unstage(entries) }
                        }
                        statusSection(
                            "Changes", kind: .changes, entries: panelModel.status.changes,
                            actionTitle: "Stage all"
                        ) { entries in
                            Task { await panelModel.stage(entries) }
                        }
                        statusSection(
                            "Untracked", kind: .untracked, entries: panelModel.status.untracked,
                            actionTitle: "Stage all"
                        ) { entries in
                            Task { await panelModel.stage(entries) }
                        }
                    }
                    .padding(8)
                }
            }
        }
        .disabled(panelModel.mutationInProgress)
    }

    @ViewBuilder
    private func statusSection(
        _ title: String,
        kind: SectionKind,
        entries: [GitStatusEntry],
        actionTitle: String,
        action: @escaping ([GitStatusEntry]) -> Void
    ) -> some View {
        if !entries.isEmpty {
            let actionable = entries.filter { !$0.isConflicted }
            VStack(spacing: 2) {
                HStack {
                    Text("\(title) (\(entries.count))")
                        .font(.system(size: 11, weight: .semibold))
                    Spacer()
                    Button(actionTitle) { action(actionable) }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    if kind == .changes {
                        Button("Discard all", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: .changes, entries: actionable))
                        }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    } else if kind == .untracked {
                        Button("Discard all", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: .untracked, entries: actionable))
                        }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    }
                }
                ForEach(entries, id: \.path) { entry in
                    statusRow(entry, section: kind)
                }
            }
        }
    }

    private func statusRow(_ entry: GitStatusEntry, section: SectionKind) -> some View {
        HStack(spacing: 7) {
            Text(symbol(entry))
                .font(.system(size: 10, weight: .bold, design: .monospaced))
                .foregroundStyle(color(entry))
                .frame(width: 14)
            Text(entry.path.value)
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            if entry.isConflicted {
                Text("Resolve in terminal")
                    .font(.caption2)
                    .foregroundStyle(AppTheme.gitConflict)
            } else if section == .staged {
                Button("Unstage") {
                    Task { await panelModel.unstage([entry]) }
                }
                .buttonStyle(.plain)
            } else {
                Button("Discard", role: .destructive) {
                    requestDiscard(PendingGitDiscard(
                        kind: entry.isUntracked ? .untracked : .changes,
                        entries: [entry]))
                }
                .buttonStyle(.plain)
                Button("Stage") {
                    Task { await panelModel.stage([entry]) }
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 5)
        .contentShape(Rectangle())
        .background(AppTheme.rowHover.opacity(0.001),
                    in: RoundedRectangle(cornerRadius: 6))
        .onTapGesture { onOpenDiff(entry) }
        .help(entry.isConflicted ? "Conflicted" : entry.path.value)
    }

    private func symbol(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "U" }
        if entry.isUntracked { return "?" }
        if entry.indexState == .added { return "A" }
        if entry.indexState == .deleted || entry.worktreeState == .deleted { return "D" }
        if entry.indexState == .renamed || entry.worktreeState == .renamed { return "R" }
        return "M"
    }

    private func color(_ entry: GitStatusEntry) -> Color {
        if entry.isConflicted { return AppTheme.gitConflict }
        if entry.isUntracked { return AppTheme.gitUntracked }
        if entry.isStaged { return AppTheme.gitStaged }
        return AppTheme.gitModified
    }
}
```

- [ ] **Step 3: Create the unified Diff view**

Create `GitDiffView.swift`:

```swift
import AppKit
import SwiftUI
import TillerGit

struct GitDiffView: View {
    @Bindable var panelModel: RightPanelModel
    let requestDiscard: (PendingGitDiscard) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Picker("Changed file", selection: Binding(
                    get: { panelModel.selectedDiffPath },
                    set: { path in
                        guard let path,
                              let entry = panelModel.status.entries.first(where: {
                                  $0.path == path
                              }) else { return }
                        Task { await panelModel.selectDiff(entry) }
                    })) {
                    ForEach(panelModel.status.entries, id: \.path) { entry in
                        Text(entry.path.value).tag(Optional(entry.path))
                    }
                }
                .labelsHidden()
                Button { selectAdjacent(-1) } label: {
                    Image(systemName: "chevron.up")
                }
                .buttonStyle(.plain)
                .help("Previous changed file")
                Button { selectAdjacent(1) } label: {
                    Image(systemName: "chevron.down")
                }
                .buttonStyle(.plain)
                .help("Next changed file")
                Button { Task { await panelModel.refresh() } } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .help("Refresh Diff")
            }
            .padding(8)
            Divider()

            if panelModel.status.isClean {
                ContentUnavailableView("Working tree clean", systemImage: "checkmark.circle")
            } else if panelModel.diffLoading && panelModel.diff == nil {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = panelModel.diffError {
                ContentUnavailableView {
                    Label("Diff unavailable", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                } actions: {
                    Button("Riprova") { Task { await panelModel.ensureDiffLoaded() } }
                    if let url = selectedFileURL {
                        Button("Apri file") { NSWorkspace.shared.open(url) }
                    }
                }
            } else if let diff = panelModel.diff {
                if diff.isBinary {
                    ContentUnavailableView(
                        "Binary diff unavailable", systemImage: "doc.richtext")
                } else {
                    ScrollView([.vertical, .horizontal]) {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(diff.lines) { line in
                                UnifiedDiffRow(line: line)
                            }
                        }
                    }
                }
                actionBar
            }
        }
    }

    private var selectedFileURL: URL? {
        guard let root = panelModel.rootURL,
              let path = panelModel.selectedDiffPath else { return nil }
        return root.appendingPathComponent(path.value)
    }

    private var actionBar: some View {
        HStack {
            Spacer()
            if let entry = panelModel.selectedEntry {
                if entry.isConflicted {
                    Text("Resolve in terminal")
                        .font(.caption)
                        .foregroundStyle(AppTheme.gitConflict)
                } else {
                    if entry.isStaged {
                        Button("Unstage") {
                            Task { await panelModel.unstage([entry]) }
                        }
                    }
                    if entry.hasWorktreeChanges {
                        Button("Discard", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: .changes, entries: [entry]))
                        }
                        Button("Stage") {
                            Task { await panelModel.stage([entry]) }
                        }
                        .buttonStyle(.borderedProminent)
                    } else if entry.isUntracked {
                        Button("Discard", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: .untracked, entries: [entry]))
                        }
                        Button("Stage") {
                            Task { await panelModel.stage([entry]) }
                        }
                        .buttonStyle(.borderedProminent)
                    }
                }
            }
        }
        .disabled(panelModel.mutationInProgress)
        .padding(8)
        .overlay(alignment: .top) { Divider() }
    }

    private func selectAdjacent(_ delta: Int) {
        let entries = panelModel.status.entries
        guard !entries.isEmpty else { return }
        let current = entries.firstIndex {
            $0.path == panelModel.selectedDiffPath
        } ?? 0
        let next = min(max(current + delta, 0), entries.count - 1)
        Task { await panelModel.selectDiff(entries[next]) }
    }
}

private struct UnifiedDiffRow: View {
    let line: GitDiffLine

    var body: some View {
        HStack(spacing: 0) {
            Text(line.oldLineNumber.map(String.init) ?? "")
                .frame(width: 38, alignment: .trailing)
            Text(line.newLineNumber.map(String.init) ?? "")
                .frame(width: 38, alignment: .trailing)
            Text(marker).frame(width: 18)
            Text(line.text).textSelection(.enabled)
            Spacer(minLength: 8)
        }
        .font(.system(size: 11, design: .monospaced))
        .foregroundStyle(foreground)
        .padding(.vertical, 1)
        .background(background)
    }

    private var marker: String {
        switch line.kind {
        case .addition: "+"
        case .deletion: "-"
        case .context: " "
        case .hunk: "@@"
        case .metadata: ""
        }
    }

    private var foreground: Color {
        switch line.kind {
        case .addition: AppTheme.diffAddition
        case .deletion: AppTheme.diffDeletion
        default: AppTheme.subtitle
        }
    }

    private var background: Color {
        switch line.kind {
        case .addition: AppTheme.diffAdditionBackground
        case .deletion: AppTheme.diffDeletionBackground
        case .hunk: AppTheme.diffHunkBackground
        default: .clear
        }
    }
}
```

- [ ] **Step 4: Create the segmented shell and one confirmation owner**

Create `RightPanelView.swift`:

```swift
import SwiftUI
import TillerCore

struct RightPanelView: View {
    @Bindable var appModel: AppModel
    @Bindable var panelModel: RightPanelModel
    @Binding var modeRaw: String
    let isGitRepository: Bool
    let onClose: () -> Void
    @State private var pendingDiscard: PendingGitDiscard?

    private var effectiveMode: RightPanelMode {
        .effective(rawValue: modeRaw, isGitRepository: isGitRepository)
    }

    private var selectedMode: Binding<RightPanelMode> {
        Binding(
            get: { effectiveMode },
            set: { modeRaw = $0.rawValue })
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Picker("Worktree tools", selection: selectedMode) {
                    ForEach(RightPanelMode.allCases) { mode in
                        Label(mode.title, systemImage: mode.systemImage)
                            .tag(mode)
                            .disabled(mode.requiresGit && !isGitRepository)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()

                Button(action: onClose) {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.plain)
                .help("Nascondi pannello destro (⌃⌘I)")
                .accessibilityLabel("Nascondi pannello destro")
            }
            .padding(8)
            Divider()
            if let monitorError = panelModel.monitorError {
                Label(monitorError, systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(AppTheme.gitModified)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                Divider()
            }

            Group {
                if let worktree = panelModel.worktree {
                    switch effectiveMode {
                    case .files:
                        FileExplorerView(
                            appModel: appModel,
                            panelModel: panelModel,
                            worktree: worktree)
                    case .diff:
                        GitDiffView(
                            panelModel: panelModel,
                            requestDiscard: { pendingDiscard = $0 })
                    case .status:
                        GitStatusView(
                            panelModel: panelModel,
                            onOpenDiff: { entry in
                                modeRaw = RightPanelMode.diff.rawValue
                                Task { await panelModel.selectDiff(entry) }
                            },
                            requestDiscard: { pendingDiscard = $0 })
                    }
                } else {
                    ContentUnavailableView(
                        "No worktree selected",
                        systemImage: "sidebar.right",
                        description: Text(
                            "Select a worktree to inspect its files and changes."))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .task(id: effectiveMode) {
            if effectiveMode == .diff { await panelModel.ensureDiffLoaded() }
        }
        .alert(item: $pendingDiscard) { pending in
            Alert(
                title: Text(pending.title),
                message: Text(pending.message),
                primaryButton: .destructive(Text("Discard")) {
                    Task {
                        switch pending.kind {
                        case .changes:
                            await panelModel.discardChanges(pending.entries)
                        case .untracked:
                            await panelModel.discardUntracked(pending.entries)
                        }
                    }
                },
                secondaryButton: .cancel(Text("Annulla")))
        }
    }
}
```

- [ ] **Step 5: Regenerate and compile all panel views**

Run:

```bash
xcodegen generate
Scripts/ci.sh
```

Expected: `CI OK`.

- [ ] **Step 6: Commit**

```bash
git add App/AppTheme.swift App/RightPanel/RightPanelView.swift \
  App/RightPanel/GitDiffView.swift App/RightPanel/GitStatusView.swift
git commit -m "feat: add git diff and status panel views"
```

---
### Task 11: Integrate the third split column and commands

**Files:**
- Modify: `App/ContentView.swift:6-155`
- Modify: `App/TillerApp.swift:5-67`

**Interfaces:**
- Consumes: all App panel files from Tasks 8-10 and `AppModel.isGitProject(id:)`.
- Produces: visible end-to-end feature, toolbar toggle, View-menu command, persisted resize, activation/deactivation lifecycle.

- [ ] **Step 1: Add panel state to `ContentView`**

Add these properties beside the existing sidebar state:

```swift
    @AppStorage(AppSettings.rightPanelVisibleKey)
    private var rightPanelVisible = AppSettings.defaultRightPanelVisible
    @AppStorage(AppSettings.rightPanelWidthKey)
    private var rightPanelWidth = AppSettings.defaultRightPanelWidth
    @AppStorage(AppSettings.rightPanelModeKey)
    private var rightPanelModeRaw = RightPanelMode.files.rawValue
    @State private var rightPanelModel = RightPanelModel()
```

Add this context type outside `ContentView`:

```swift
private struct RightPanelContext: Hashable {
    let worktreeId: UUID?
    let gitProject: Bool
    let visible: Bool
}
```

Add computed context inside `ContentView`:

```swift
    private var rightPanelContext: RightPanelContext {
        let worktree = model.selectedWorktree
        return RightPanelContext(
            worktreeId: worktree?.id,
            gitProject: worktree.map { model.isGitProject(id: $0.projectId) } ?? false,
            visible: rightPanelVisible)
    }
```

- [ ] **Step 2: Add the toolbar toggle**

Inside the existing `.primaryAction` group, before Split, add:

```swift
                    Button {
                        rightPanelVisible.toggle()
                    } label: {
                        Image(systemName: "sidebar.right")
                    }
                    .help(rightPanelVisible
                          ? "Nascondi pannello destro (⌃⌘I)"
                          : "Mostra pannello destro (⌃⌘I)")
                    .accessibilityLabel("Pannello destro")
```

- [ ] **Step 3: Add the third `HSplitView` child and trailing divider cover**

After the central `VStack` in `splitContent`, add:

```swift
            if rightPanelVisible {
                RightPanelView(
                    appModel: model,
                    panelModel: rightPanelModel,
                    modeRaw: $rightPanelModeRaw,
                    isGitRepository: rightPanelContext.gitProject,
                    onClose: { rightPanelVisible = false })
                .frame(
                    minWidth: CGFloat(AppSettings.rightPanelWidthRange.lowerBound),
                    idealWidth: CGFloat(AppSettings.clampRightPanelWidth(rightPanelWidth)),
                    maxWidth: CGFloat(AppSettings.rightPanelWidthRange.upperBound),
                    maxHeight: .infinity)
                .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width in
                    rightPanelWidth = AppSettings.clampRightPanelWidth(Double(width))
                }
            }
```

Add a second overlay to the `HSplitView` after the existing leading overlay:

```swift
        .overlay(alignment: .trailing) {
            if rightPanelVisible {
                SidebarMaterialContainer()
                    .frame(width: 2)
                    .offset(x: -CGFloat(rightPanelWidth))
                    .ignoresSafeArea()
                    .allowsHitTesting(false)
            }
        }
```

Extend the animation chain:

```swift
        .animation(.easeInOut(duration: 0.2), value: rightPanelVisible)
```

- [ ] **Step 4: Activate and stop panel work from workspace context**

Attach this task to the root `Group` in `ContentView.body`, after `.frame`:

```swift
        .task(id: rightPanelContext) {
            guard rightPanelVisible else {
                rightPanelModel.deactivate()
                return
            }
            await rightPanelModel.activate(
                worktree: model.selectedWorktree,
                isGitRepository: rightPanelContext.gitProject)
        }
```

Add this shutdown hook beside it:

```swift
        .onDisappear { rightPanelModel.deactivate() }
```

- [ ] **Step 5: Add the shared View-menu toggle in `TillerApp`**

Add beside `sidebarVisible`:

```swift
    @AppStorage(AppSettings.rightPanelVisibleKey)
    private var rightPanelVisible = AppSettings.defaultRightPanelVisible
```

Inside `CommandGroup(after: .sidebar)`, after the existing left-sidebar button, add:

```swift
                Button(rightPanelVisible
                       ? "Nascondi pannello destro"
                       : "Mostra pannello destro") {
                    rightPanelVisible.toggle()
                }
                .keyboardShortcut("i", modifiers: [.control, .command])
```

- [ ] **Step 6: Build and run the first end-to-end smoke path**

Run:

```bash
Scripts/ci.sh
```

Expected: `CI OK`.

Then launch Tiller from Xcode, select a Git worktree, toggle the right panel with the toolbar and `⌃⌘I`, expand a folder, edit a nested file from a terminal, and confirm Files/Status/Diff refresh within one debounce cycle. Confirm hiding the panel stops updates until reopened.

- [ ] **Step 7: Commit**

```bash
git add App/ContentView.swift App/TillerApp.swift
git commit -m "feat: integrate right worktree tools panel"
```

---

### Task 12: End-to-end verification and documentation

**Files:**
- Modify: `README.md:30-36,88-96`

**Interfaces:**
- Consumes: the complete feature from Task 11.
- Produces: verified destructive flows, persistence and edge states, plus user-facing feature and shortcut documentation.

- [ ] **Step 1: Exercise every destructive and edge-state contract before documentation**

Use temporary Git worktrees, not the Tiller repository itself:

1. mixed staged + unstaged file: Discard Changes restores the staged content only;
2. staged file: only Unstage appears;
3. untracked file: Discard confirmation names the path and deletion occurs only after confirmation;
4. bulk Changes and bulk Untracked: confirmation shows the exact count;
5. conflicted entry: status is visible and actions say “Resolve in terminal”;
6. unborn repository: Files, Status, Diff, Stage and Unstage work without `HEAD`;
7. binary file: explicit binary state with file actions;
8. generated text diff over 5 MiB: explicit too-large state, no partial rows;
9. non-Git folder: Files works, Diff/Status are disabled and effective mode falls back to Files;
10. symlinked directory: visible but not expandable;
11. rapid worktree switching during refresh: no old paths or diff appear;
12. panel hidden: no filesystem monitor or Git refresh remains active.

Expected: every item matches the spec. If one fails, stop this task, return to
the owning task above, add the missing regression test, correct the source, and
repeat that task’s focused verification before continuing.

- [ ] **Step 2: Verify persistence and UI regressions**

Set panel mode to Status, resize to a non-default width, leave it visible, quit and relaunch. Confirm mode, width and visibility restore. Then hide it, relaunch again, and confirm it stays hidden. Verify sidebar left toggle, terminal split, Markdown tab opening, drag/drop and usage bar remain unchanged.

- [ ] **Step 3: Run the full repository gate**

Run:

```bash
Scripts/ci.sh
```

Expected: final line `CI OK`.

- [ ] **Step 4: Update README only after the smoke test passes**

Add this feature bullet after Native terminal:

```markdown
- 🔎 **Worktree tools panel** — a hideable right sidebar for lazy file browsing, unified local diffs, and stage/unstage/discard status workflows
```

Add this shortcut row after “Open markdown file in selected worktree”:

```markdown
| Toggle right worktree panel | `⌃⌘I` |
```

- [ ] **Step 5: Re-run the documentation-safe gate**

Run:

```bash
Scripts/ci.sh
```

Expected: final line `CI OK`.

- [ ] **Step 6: Commit documentation**

```bash
git add README.md
git commit -m "docs: document right worktree panel"
```
