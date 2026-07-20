# Slash-Command Highlight + Git File Colors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Accent-color the `/command` token in the chat composer with full keyboard navigation of the slash popup (↑/↓/Tab/Enter/Escape), and tint file names + parent folders in the Files sidebar by git status (VS Code style).

**Architecture:** Feature 1 extends the existing `ChatTextEditor` NSTextView wrapper (key interception via `doCommandBy`, coloring via `textStorage`) and the existing `slashPopup` in `ChatComposerView`; selection logic lives in a pure helper. Feature 2 adds a pure aggregation function in `TillerGit`, a cached property on `RightPanelModel`, and coloring in `FileExplorerView`.

**Tech Stack:** Swift 6, SwiftUI + AppKit (NSTextView), swift-testing (`@Test`/`#expect`), GRDB untouched.

**Spec:** `docs/superpowers/specs/2026-07-20-slash-highlight-git-file-colors-design.md`

## Global Constraints

- Tests first (swift-testing, not XCTest).
- Conventional Commits, lower-case imperative subject, **no Co-Authored-By / attribution trailers**.
- Never touch `Tiller.xcodeproj` by hand; new files under `App/` or `AppTests/` require `xcodegen generate` (Task 7).
- Do not commit unrelated pre-existing uncommitted hunks in shared files. Before every commit run `git status -sb` and `git diff --cached` and stage only this plan's files. `App/AppModel.swift` has a known unrelated uncommitted hunk — never stage that file.
- `Scripts/ci.sh` must print `CI OK` before the work is considered done (Task 7). PTY tests are known-flaky: retry ci.sh up to 6 times before concluding failure.
- Package sources (`Packages/…`) do NOT need xcodegen; only `App/`/`AppTests/` files do.

---

### Task 1: `DirectoryStatusAggregator` in TillerGit

**Files:**
- Create: `Packages/TillerGit/Sources/TillerGit/DirectoryGitStatus.swift`
- Test: `Packages/TillerGit/Tests/TillerGitTests/DirectoryStatusAggregatorTests.swift`

**Interfaces:**
- Consumes: `GitStatusEntry`, `GitPath`, `GitFileState` (existing, `GitStatus.swift`). `GitStatusEntry.mutationPaths` returns `[path]` or `[path, originalPath]`.
- Produces: `public enum DirectoryGitStatus { case conflicted, changed, untracked }` and `public enum DirectoryStatusAggregator { static func directoryStatuses(from: [String: GitStatusEntry]) -> [String: DirectoryGitStatus] }` — used by Tasks 2 and 3.

- [ ] **Step 1: Write the failing tests**

`Packages/TillerGit/Tests/TillerGitTests/DirectoryStatusAggregatorTests.swift`:

```swift
import Testing

@testable import TillerGit

@Suite("DirectoryStatusAggregator")
struct DirectoryStatusAggregatorTests {
    private func entry(
        _ path: String, index: GitFileState? = nil,
        worktree: GitFileState? = nil, original: String? = nil
    ) throws -> GitStatusEntry {
        GitStatusEntry(
            path: try GitPath(path),
            originalPath: try original.map { try GitPath($0) },
            indexState: index, worktreeState: worktree)
    }

    private func statuses(_ entries: [GitStatusEntry]) -> [String: DirectoryGitStatus] {
        DirectoryStatusAggregator.directoryStatuses(
            from: Dictionary(entries.map { ($0.path.value, $0) },
                             uniquingKeysWith: { first, _ in first }))
    }

    @Test func nestedModifiedFileMarksAllAncestorsChanged() throws {
        let result = statuses([try entry("a/b/c.swift", worktree: .modified)])
        #expect(result == ["a": .changed, "a/b": .changed])
    }

    @Test func rootLevelFileContributesNoDirectories() throws {
        let result = statuses([try entry("README.md", worktree: .modified)])
        #expect(result.isEmpty)
    }

    @Test func untrackedDoesNotDowngradeChanged() throws {
        let result = statuses([
            try entry("a/x.swift", worktree: .modified),
            try entry("a/y.swift", worktree: .untracked),
        ])
        #expect(result == ["a": .changed])
    }

    @Test func conflictedWinsOverChangedAndUntracked() throws {
        let result = statuses([
            try entry("a/x.swift", worktree: .modified),
            try entry("a/y.swift", index: .unmerged, worktree: .unmerged),
        ])
        #expect(result == ["a": .conflicted])
    }

    @Test func untrackedOnlyDirectoryIsUntracked() throws {
        let result = statuses([try entry("new/dir/file.txt", worktree: .untracked)])
        #expect(result == ["new": .untracked, "new/dir": .untracked])
    }

    @Test func renameContributesBothPathsAncestors() throws {
        let result = statuses([
            try entry("dst/moved.swift", index: .renamed, original: "src/moved.swift")
        ])
        #expect(result == ["dst": .changed, "src": .changed])
    }

    @Test func stagedAddedFileIsChanged() throws {
        let result = statuses([try entry("a/new.swift", index: .added)])
        #expect(result == ["a": .changed])
    }

    @Test func emptyInputYieldsEmptyOutput() {
        #expect(DirectoryStatusAggregator.directoryStatuses(from: [:]).isEmpty)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerGit && swift test --filter DirectoryStatusAggregator`
Expected: FAIL with "cannot find 'DirectoryStatusAggregator' in scope"

- [ ] **Step 3: Write the implementation**

`Packages/TillerGit/Sources/TillerGit/DirectoryGitStatus.swift`:

```swift
import Foundation

/// Aggregated git state for a directory, derived from the changed files
/// beneath it. Collapsed like VS Code's explorer: staged, modified, deleted
/// and renamed descendants all mark the directory `changed`.
public enum DirectoryGitStatus: Sendable, Equatable {
    case conflicted
    case changed
    case untracked

    /// conflicted > changed > untracked
    var precedence: Int {
        switch self {
        case .conflicted: 2
        case .changed: 1
        case .untracked: 0
        }
    }
}

public enum DirectoryStatusAggregator {
    /// Maps every ancestor directory (relative path, no trailing slash) of a
    /// changed file to its aggregated status. Rename entries contribute the
    /// ancestors of both current and original paths. Root-level files have
    /// no ancestors and contribute nothing.
    public static func directoryStatuses(
        from statusByPath: [String: GitStatusEntry]
    ) -> [String: DirectoryGitStatus] {
        var result: [String: DirectoryGitStatus] = [:]
        for entry in statusByPath.values {
            let status: DirectoryGitStatus =
                entry.isConflicted ? .conflicted
                : entry.isUntracked ? .untracked
                : .changed
            for path in entry.mutationPaths {
                var components = path.value.split(separator: "/").dropLast()
                while !components.isEmpty {
                    let directory = components.joined(separator: "/")
                    if result[directory].map({ $0.precedence < status.precedence }) ?? true {
                        result[directory] = status
                    }
                    components = components.dropLast()
                }
            }
        }
        return result
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerGit && swift test --filter DirectoryStatusAggregator`
Expected: PASS (8 tests)

- [ ] **Step 5: Run the whole TillerGit suite**

Run: `cd Packages/TillerGit && swift test`
Expected: PASS, no regressions.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerGit/Sources/TillerGit/DirectoryGitStatus.swift Packages/TillerGit/Tests/TillerGitTests/DirectoryStatusAggregatorTests.swift
git commit -m "feat: add DirectoryStatusAggregator for folder git status"
```

---

### Task 2: `RightPanelModel.directoryStatusByPath`

**Files:**
- Modify: `App/RightPanel/RightPanelModel.swift`
- Test: `AppTests/RightPanelDirectoryStatusTests.swift` (new — xcodegen regen happens in Task 7; run this test only from Task 7 onward, see Step 3)

**Interfaces:**
- Consumes: `DirectoryStatusAggregator.directoryStatuses(from:)` (Task 1).
- Produces: `private(set) var directoryStatusByPath: [String: DirectoryGitStatus]` on `RightPanelModel` — used by Task 3. Also changes `private func apply(_ snapshot:)` to internal `func apply(_ snapshot:)` so the test can drive it.

- [ ] **Step 1: Write the test file** (it cannot run until Task 7 regenerates the Xcode project; write it now, execute it in Task 7)

`AppTests/RightPanelDirectoryStatusTests.swift`:

```swift
import Testing
import TillerGit

@testable import Tiller

@Suite("RightPanelDirectoryStatus")
@MainActor
struct RightPanelDirectoryStatusTests {
    @Test func applyPopulatesDirectoryStatuses() throws {
        let model = RightPanelModel()
        let entry = GitStatusEntry(
            path: try GitPath("a/b/c.swift"), originalPath: nil,
            indexState: nil, worktreeState: .modified)
        model.apply(GitStatusSnapshot(entries: [entry]))
        #expect(model.directoryStatusByPath == ["a": .changed, "a/b": .changed])
    }

    @Test func applyEmptySnapshotClearsDirectoryStatuses() throws {
        let model = RightPanelModel()
        let entry = GitStatusEntry(
            path: try GitPath("a/b.swift"), originalPath: nil,
            indexState: nil, worktreeState: .modified)
        model.apply(GitStatusSnapshot(entries: [entry]))
        model.apply(.empty)
        #expect(model.directoryStatusByPath.isEmpty)
    }
}
```

Note: `GitStatusEntry`'s memberwise init is internal to TillerGit. If the test fails to compile with "initializer is inaccessible", add `@testable import TillerGit` instead of `import TillerGit`.
Note: `GitStatusSnapshot(entries:)` — if its memberwise init is also internal, the `@testable import TillerGit` covers it too.

- [ ] **Step 2: Implement the wiring**

In `App/RightPanel/RightPanelModel.swift`:

1. Add the property right after `private(set) var statusByPath: [String: GitStatusEntry] = [:]` (line ~21):

```swift
    private(set) var directoryStatusByPath: [String: DirectoryGitStatus] = [:]
```

2. In the reset block (the method around line 75–95 that sets `statusByPath = [:]`), add immediately after `statusByPath = [:]`:

```swift
        directoryStatusByPath = [:]
```

3. Change the access level of `apply` (line ~267) from `private func apply(_ snapshot: GitStatusSnapshot)` to `func apply(_ snapshot: GitStatusSnapshot)` (internal, reachable via `@testable`), and add as the new first lines of its body — right after `statusByPath = Dictionary(...)` is assigned:

```swift
        directoryStatusByPath = DirectoryStatusAggregator.directoryStatuses(from: statusByPath)
```

- [ ] **Step 3: Verify it builds**

Run: `xcodebuild -scheme Tiller -configuration Debug -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO build -quiet`
Expected: `** BUILD SUCCEEDED **`. (The new test file is not in the project yet — that is expected; it runs in Task 7.)

- [ ] **Step 4: Commit**

```bash
git status -sb   # confirm only the two files below are about to be staged
git add App/RightPanel/RightPanelModel.swift AppTests/RightPanelDirectoryStatusTests.swift
git commit -m "feat: cache directory git statuses in RightPanelModel"
```

---

### Task 3: File and folder colors in `FileExplorerView`

**Files:**
- Modify: `App/RightPanel/FileExplorerView.swift`

**Interfaces:**
- Consumes: `panelModel.statusByPath`, `panelModel.directoryStatusByPath` (Task 2), existing `statusColor(_:)`, `AppTheme.gitConflict/gitModified/gitUntracked`.
- Produces: UI only.

- [ ] **Step 1: Tint the file/folder name**

In `fileRow(_:)`, change (current line ~82):

```swift
            Text(node.name)
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
```

to:

```swift
            Text(node.name)
                .font(.system(size: 12))
                .foregroundStyle(nameColor(for: node))
                .lineLimit(1)
                .truncationMode(.middle)
```

- [ ] **Step 2: Add the folder dot badge**

In `fileRow(_:)`, change the existing badge block (current line ~87):

```swift
            if let entry = panelModel.statusByPath[node.relativePath] {
                Text(statusSymbol(entry))
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundStyle(statusColor(entry))
                    .help(statusLabel(entry))
            }
```

to:

```swift
            if let entry = panelModel.statusByPath[node.relativePath] {
                Text(statusSymbol(entry))
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundStyle(statusColor(entry))
                    .help(statusLabel(entry))
            } else if node.kind.isDirectory,
                      let dirStatus = panelModel.directoryStatusByPath[node.relativePath] {
                Circle()
                    .fill(directoryColor(dirStatus))
                    .frame(width: 6, height: 6)
                    .help(directoryLabel(dirStatus))
            }
```

- [ ] **Step 3: Add the color/label helpers**

Add next to the existing `statusColor(_:)` helper at the bottom of the struct:

```swift
    private func nameColor(for node: FileTreeNode) -> Color {
        if let entry = panelModel.statusByPath[node.relativePath] {
            return statusColor(entry)
        }
        if node.kind.isDirectory,
           let dirStatus = panelModel.directoryStatusByPath[node.relativePath] {
            return directoryColor(dirStatus)
        }
        return .primary
    }

    private func directoryColor(_ status: DirectoryGitStatus) -> Color {
        switch status {
        case .conflicted: AppTheme.gitConflict
        case .changed: AppTheme.gitModified
        case .untracked: AppTheme.gitUntracked
        }
    }

    private func directoryLabel(_ status: DirectoryGitStatus) -> String {
        switch status {
        case .conflicted: "Contains conflicts"
        case .changed: "Contains changes"
        case .untracked: "Contains untracked files"
        }
    }
```

(UI strings in English, per project convention.)

- [ ] **Step 4: Build**

Run: `xcodebuild -scheme Tiller -configuration Debug -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO build -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 5: Commit**

```bash
git add App/RightPanel/FileExplorerView.swift
git commit -m "feat: tint sidebar files and folders by git status"
```

---

### Task 4: `SlashKey` interception in `ChatTextEditor`

**Files:**
- Modify: `App/Chat/ChatTextEditor.swift`
- Test: `AppTests/ChatTextEditorSlashTests.swift` (new — runs from Task 7 onward)

**Interfaces:**
- Consumes: existing `Coordinator.textView(_:doCommandBy:)`.
- Produces: `enum SlashKey: Equatable { case up, down, tab, enter, escape }`, `var onSlashKey: ((SlashKey) -> Bool)?` on `ChatTextEditor`, and `static func slashKey(for: Selector) -> SlashKey?` on `Coordinator` — used by Task 6.

- [ ] **Step 1: Write the test file** (runs in Task 7)

`AppTests/ChatTextEditorSlashTests.swift`:

```swift
import AppKit
import SwiftUI
import Testing

@testable import Tiller

@Suite("ChatTextEditorSlash")
@MainActor
struct ChatTextEditorSlashTests {
    private func makeEditor(
        onSlashKey: ((SlashKey) -> Bool)? = nil,
        onSubmit: @escaping () -> Void = {}
    ) -> ChatTextEditor {
        ChatTextEditor(
            text: .constant(""), isEditable: true, minHeight: 36, maxHeight: 160,
            onSubmit: onSubmit, onSlashKey: onSlashKey)
    }

    @Test func mapsSelectorsToSlashKeys() {
        typealias C = ChatTextEditor.Coordinator
        #expect(C.slashKey(for: #selector(NSResponder.moveUp(_:))) == .up)
        #expect(C.slashKey(for: #selector(NSResponder.moveDown(_:))) == .down)
        #expect(C.slashKey(for: #selector(NSResponder.insertTab(_:))) == .tab)
        #expect(C.slashKey(for: #selector(NSResponder.insertNewline(_:))) == .enter)
        #expect(C.slashKey(for: #selector(NSResponder.cancelOperation(_:))) == .escape)
        #expect(C.slashKey(for: #selector(NSResponder.moveLeft(_:))) == nil)
    }

    @Test func consumedKeyIsSwallowed() {
        var received: [SlashKey] = []
        let editor = makeEditor(onSlashKey: { key in
            received.append(key)
            return true
        })
        let coordinator = ChatTextEditor.Coordinator(editor)
        let textView = NSTextView()
        let handled = coordinator.textView(
            textView, doCommandBy: #selector(NSResponder.moveDown(_:)))
        #expect(handled)
        #expect(received == [.down])
    }

    @Test func unconsumedEnterStillSubmits() {
        var submitted = false
        let editor = makeEditor(
            onSlashKey: { _ in false }, onSubmit: { submitted = true })
        let coordinator = ChatTextEditor.Coordinator(editor)
        let handled = coordinator.textView(
            NSTextView(), doCommandBy: #selector(NSResponder.insertNewline(_:)))
        #expect(handled)
        #expect(submitted)
    }

    @Test func unconsumedArrowFallsThroughToDefault() {
        let editor = makeEditor(onSlashKey: { _ in false })
        let coordinator = ChatTextEditor.Coordinator(editor)
        let handled = coordinator.textView(
            NSTextView(), doCommandBy: #selector(NSResponder.moveUp(_:)))
        #expect(!handled)
    }

    @Test func nilHandlerKeepsCurrentBehavior() {
        var submitted = false
        let editor = makeEditor(onSlashKey: nil, onSubmit: { submitted = true })
        let coordinator = ChatTextEditor.Coordinator(editor)
        #expect(!coordinator.textView(
            NSTextView(), doCommandBy: #selector(NSResponder.moveUp(_:))))
        #expect(coordinator.textView(
            NSTextView(), doCommandBy: #selector(NSResponder.insertNewline(_:))))
        #expect(submitted)
    }
}
```

- [ ] **Step 2: Implement**

In `App/Chat/ChatTextEditor.swift`:

1. Add above the `ChatTextEditor` struct:

```swift
/// Keys the composer may steal from the text view while the slash-command
/// popup is open.
enum SlashKey: Equatable {
    case up, down, tab, enter, escape
}
```

2. Add the property after `var onSubmit: () -> Void`:

```swift
    /// Returns true to consume the key (popup navigation); false restores
    /// the default behavior. nil behaves like always-false.
    var onSlashKey: ((SlashKey) -> Bool)? = nil
```

3. Replace `Coordinator.textView(_:doCommandBy:)` (keep the doc comment above it, extend it):

```swift
        /// Slash-popup keys get first refusal via `onSlashKey`. Then plain
        /// Return sends (swallowed here); Shift+Return inserts a real
        /// newline via the default AppKit handling (`return false`).
        func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
            if let key = Self.slashKey(for: commandSelector),
               parent.onSlashKey?(key) == true {
                return true
            }
            guard commandSelector == #selector(NSResponder.insertNewline(_:)) else { return false }
            let shiftHeld = NSApp.currentEvent?.modifierFlags.contains(.shift) ?? false
            if shiftHeld { return false }
            parent.onSubmit()
            return true
        }

        static func slashKey(for selector: Selector) -> SlashKey? {
            switch selector {
            case #selector(NSResponder.moveUp(_:)): .up
            case #selector(NSResponder.moveDown(_:)): .down
            case #selector(NSResponder.insertTab(_:)): .tab
            case #selector(NSResponder.insertNewline(_:)): .enter
            case #selector(NSResponder.cancelOperation(_:)): .escape
            default: nil
            }
        }
```

- [ ] **Step 3: Build**

Run: `xcodebuild -scheme Tiller -configuration Debug -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO build -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add App/Chat/ChatTextEditor.swift AppTests/ChatTextEditorSlashTests.swift
git commit -m "feat: add slash-key interception to ChatTextEditor"
```

---

### Task 5: Accent coloring of the `/command` token

**Files:**
- Modify: `App/Chat/ChatTextEditor.swift`
- Test: extend `AppTests/ChatTextEditorSlashTests.swift`

**Interfaces:**
- Consumes: `Coordinator.textDidChange`, `updateNSView`.
- Produces: `Coordinator.applySlashHighlight(to: NSTextView)` (internal, testable).

- [ ] **Step 1: Add the failing tests**

Append to the `ChatTextEditorSlashTests` suite:

```swift
    @Test func slashTokenGetsAccentColor() {
        let editor = makeEditor()
        let coordinator = ChatTextEditor.Coordinator(editor)
        let textView = NSTextView()
        textView.string = "/he"
        coordinator.applySlashHighlight(to: textView)
        let color = textView.textStorage?.attribute(
            .foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color == .controlAccentColor)
    }

    @Test func textWithWhitespaceResetsToDefaultColor() {
        let editor = makeEditor()
        let coordinator = ChatTextEditor.Coordinator(editor)
        let textView = NSTextView()
        textView.string = "/help me"
        coordinator.applySlashHighlight(to: textView)
        let color = textView.textStorage?.attribute(
            .foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color == .textColor)
    }

    @Test func nonSlashTextKeepsDefaultColor() {
        let editor = makeEditor()
        let coordinator = ChatTextEditor.Coordinator(editor)
        let textView = NSTextView()
        textView.string = "hello"
        coordinator.applySlashHighlight(to: textView)
        let color = textView.textStorage?.attribute(
            .foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color == .textColor)
    }

    @Test func emptyTextDoesNotCrash() {
        let editor = makeEditor()
        let coordinator = ChatTextEditor.Coordinator(editor)
        let textView = NSTextView()
        coordinator.applySlashHighlight(to: textView)
        #expect(textView.string.isEmpty)
    }
```

- [ ] **Step 2: Implement**

In `App/Chat/ChatTextEditor.swift`:

1. Add to `Coordinator`:

```swift
        /// A lone "/token" draft (same condition the composer uses for the
        /// popup) is tinted with the accent color; anything else resets to
        /// the default text color. `typingAttributes` are pinned to the
        /// default so text typed after a space never inherits the accent.
        func applySlashHighlight(to textView: NSTextView) {
            guard let storage = textView.textStorage, storage.length > 0 else { return }
            let isSlashToken = textView.string.hasPrefix("/")
                && !textView.string.contains(where: \.isWhitespace)
            let color: NSColor = isSlashToken ? .controlAccentColor : .textColor
            storage.addAttribute(
                .foregroundColor, value: color,
                range: NSRange(location: 0, length: storage.length))
            textView.typingAttributes[.foregroundColor] = NSColor.textColor
        }
```

2. In `textDidChange`, after `parent.text = textView.string`, add:

```swift
            applySlashHighlight(to: textView)
```

3. In `updateNSView`, inside the `if textView.string != text {` block, after `textView.string = text`, add (programmatic text replacement — e.g. accepting a command — resets attributes):

```swift
            context.coordinator.applySlashHighlight(to: textView)
```

- [ ] **Step 3: Build**

Run: `xcodebuild -scheme Tiller -configuration Debug -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO build -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add App/Chat/ChatTextEditor.swift AppTests/ChatTextEditorSlashTests.swift
git commit -m "feat: accent-color slash-command token in chat composer"
```

---

### Task 6: Keyboard navigation + selection highlight in `ChatComposerView`

**Files:**
- Create: `App/Chat/SlashCommandSelection.swift`
- Modify: `App/Chat/ChatComposerView.swift`
- Test: `AppTests/SlashCommandSelectionTests.swift` (new — runs from Task 7 onward)

**Interfaces:**
- Consumes: `SlashKey` (Task 4), `ChatTextEditor.onSlashKey` (Task 4), existing `slashCandidates`, `AppTheme.selectionFill`.
- Produces: `SlashCommandSelection.effect(for:index:count:) -> Effect?` with `enum Effect: Equatable { case moved(Int), accepted(Int), dismissed }`.

- [ ] **Step 1: Write the failing tests**

`AppTests/SlashCommandSelectionTests.swift`:

```swift
import Testing

@testable import Tiller

@Suite("SlashCommandSelection")
struct SlashCommandSelectionTests {
    @Test func downMovesForwardAndClampsAtEnd() {
        #expect(SlashCommandSelection.effect(for: .down, index: 0, count: 3) == .moved(1))
        #expect(SlashCommandSelection.effect(for: .down, index: 2, count: 3) == .moved(2))
    }

    @Test func upMovesBackwardAndClampsAtZero() {
        #expect(SlashCommandSelection.effect(for: .up, index: 2, count: 3) == .moved(1))
        #expect(SlashCommandSelection.effect(for: .up, index: 0, count: 3) == .moved(0))
    }

    @Test func staleIndexIsClampedBeforeUse() {
        // List shrank from 5 to 2 while index was 4.
        #expect(SlashCommandSelection.effect(for: .tab, index: 4, count: 2) == .accepted(1))
        #expect(SlashCommandSelection.effect(for: .down, index: 4, count: 2) == .moved(1))
    }

    @Test func tabAndEnterAccept() {
        #expect(SlashCommandSelection.effect(for: .tab, index: 1, count: 3) == .accepted(1))
        #expect(SlashCommandSelection.effect(for: .enter, index: 1, count: 3) == .accepted(1))
    }

    @Test func escapeDismisses() {
        #expect(SlashCommandSelection.effect(for: .escape, index: 0, count: 3) == .dismissed)
    }

    @Test func emptyListYieldsNoEffect() {
        #expect(SlashCommandSelection.effect(for: .down, index: 0, count: 0) == nil)
    }
}
```

- [ ] **Step 2: Implement the pure helper**

`App/Chat/SlashCommandSelection.swift`:

```swift
/// Pure keyboard-selection logic for the slash-command popup. The view owns
/// the state; this maps (key, current index, candidate count) to an effect.
enum SlashCommandSelection {
    enum Effect: Equatable {
        case moved(Int)
        case accepted(Int)
        case dismissed
    }

    static func effect(for key: SlashKey, index: Int, count: Int) -> Effect? {
        guard count > 0 else { return nil }
        let clamped = min(max(index, 0), count - 1)
        switch key {
        case .up: return .moved(max(clamped - 1, 0))
        case .down: return .moved(min(clamped + 1, count - 1))
        case .tab, .enter: return .accepted(clamped)
        case .escape: return .dismissed
        }
    }
}
```

- [ ] **Step 3: Wire the composer**

In `App/Chat/ChatComposerView.swift`:

1. Add state after `@State private var mentionCandidates: [String] = []`:

```swift
    @State private var slashSelectionIndex = 0
    @State private var slashPopupDismissed = false
```

2. In `body`, change `if !slashCandidates.isEmpty {` to:

```swift
            if slashPopupVisible {
```

and add next to `slashCandidates` (MARK: - Slash commands section):

```swift
    private var slashPopupVisible: Bool {
        !slashCandidates.isEmpty && !slashPopupDismissed
    }
```

3. In `editor`, pass the handler and reset the state on text change — replace:

```swift
            ChatTextEditor(text: $text, isEditable: canInteract, minHeight: 36, maxHeight: 160, onSubmit: sendCurrent)
        }
        .disabled(!canInteract)
        .onChange(of: text) { updateMentionQuery() }
```

with:

```swift
            ChatTextEditor(text: $text, isEditable: canInteract, minHeight: 36,
                           maxHeight: 160, onSubmit: sendCurrent,
                           onSlashKey: handleSlashKey)
        }
        .disabled(!canInteract)
        .onChange(of: text) {
            updateMentionQuery()
            slashPopupDismissed = false
            slashSelectionIndex = 0
        }
```

4. Add the handler in the slash-commands section:

```swift
    private func handleSlashKey(_ key: SlashKey) -> Bool {
        guard slashPopupVisible,
              let effect = SlashCommandSelection.effect(
                  for: key, index: slashSelectionIndex, count: slashCandidates.count)
        else { return false }
        switch effect {
        case .moved(let index):
            slashSelectionIndex = index
        case .accepted(let index):
            text = "/\(slashCandidates[index].name) "
        case .dismissed:
            slashPopupDismissed = true
        }
        return true
    }
```

5. Highlight the selected row — in `slashPopup`, replace the `ForEach` with:

```swift
            ForEach(Array(slashCandidates.enumerated()), id: \.element.name) { index, command in
                Button {
                    text = "/\(command.name) "
                } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text("/\(command.name)")
                            .font(.caption.weight(.semibold))
                        Text(command.description)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        Spacer(minLength: 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 6).padding(.vertical, 3)
                .background(index == min(slashSelectionIndex, slashCandidates.count - 1)
                                ? AppTheme.selectionFill : Color.clear,
                            in: RoundedRectangle(cornerRadius: 4))
            }
```

- [ ] **Step 4: Build**

Run: `xcodebuild -scheme Tiller -configuration Debug -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO build -quiet`
Expected: `** BUILD SUCCEEDED **` (SlashCommandSelection.swift is not in the project yet — if the build fails with "cannot find 'SlashCommandSelection'", run `xcodegen generate` first and retry; then still re-run it in Task 7.)

- [ ] **Step 5: Commit**

```bash
git status -sb   # stage only the three files below
git add App/Chat/SlashCommandSelection.swift App/Chat/ChatComposerView.swift AppTests/SlashCommandSelectionTests.swift
git commit -m "feat: keyboard navigation for slash-command popup"
```

---

### Task 7: Project regeneration + full verification

**Files:**
- Regenerate: `Tiller.xcodeproj` (via `xcodegen generate` — never by hand)

- [ ] **Step 1: Regenerate the Xcode project**

Run: `cd /Users/enzopiopalmisano/Desktop/Progetti/tiller && xcodegen generate`
Expected: exit 0. New files picked up: `App/Chat/SlashCommandSelection.swift`, `AppTests/RightPanelDirectoryStatusTests.swift`, `AppTests/ChatTextEditorSlashTests.swift`, `AppTests/SlashCommandSelectionTests.swift`.

- [ ] **Step 2: Run the new App test suites**

Run:

```bash
xcodebuild test -quiet -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO \
  -only-testing:TillerTests/RightPanelDirectoryStatusTests \
  -only-testing:TillerTests/ChatTextEditorSlashTests \
  -only-testing:TillerTests/SlashCommandSelectionTests
```

Expected: all PASS. (A transient "test runner timed out while preparing" failure is a known hiccup — retry once.)

- [ ] **Step 3: Run the full CI gate**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`. PTY tests are known-flaky — retry up to 6 times before concluding a real failure.

- [ ] **Step 4: Commit the regenerated project**

```bash
git add Tiller.xcodeproj/project.pbxproj
git commit -m "chore: regenerate Xcode project for slash/git-colors files"
```

(If `git add` prints a gitignore warning, ignore it — the file is already tracked and the commit will succeed.)

- [ ] **Step 5: Final status check**

Run: `git status -sb`
Expected: the only remaining modifications are pre-existing unrelated hunks (notably `App/AppModel.swift`), untouched by this plan.

---

## Manual smoke checklist (post-implementation, user-run)

1. Chat tab (Claude/OpenCode): type `/` → popup appears, token turns accent-colored; ↑/↓ move the highlight; Tab and Enter insert `/command `; Escape closes the popup; Enter with popup closed still sends; Shift+Enter still inserts a newline.
2. Files sidebar: modify a file → name turns modified color with M badge; create a file → untracked color with ?; collapsed ancestor folders show tinted name + dot; resolve/commit → colors clear.

## Self-Review

- **Spec coverage:** token coloring (Task 5), keyboard nav ↑/↓/Tab/Enter/Escape (Tasks 4+6), popup highlight (Task 6), file name tint (Task 3), folder aggregation + dot (Tasks 1–3). Out-of-scope items untouched. ✓
- **Placeholder scan:** no TBDs; every code step carries full code. ✓
- **Type consistency:** `SlashKey` (Task 4) consumed by Tasks 5–6 test/impl; `DirectoryGitStatus`/`DirectoryStatusAggregator` (Task 1) consumed by Tasks 2–3; `GitStatusEntry` memberwise init accessibility flagged with the `@testable import TillerGit` fallback in Task 2. ✓
