# Code Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add CodeEditSourceEditor-backed code-file tabs, syntax-highlight both diff surfaces with tree-sitter, and replace Highlightr in chat without changing existing primary diff-click behavior.

**Architecture:** Introduce a leaf `TillerCode` package that owns file-backed code documents, language detection, pure tree-sitter highlight ranges, headless `NSAttributedString` construction, and pre-sliced diff line maps. The App target owns the Tiller palette bridge, SwiftUI rendering, CodeEditSourceEditor configuration, tab lifecycle, and entry-point wiring. `TillerGit` supplies bounded old/new text snapshots so diff highlighting has full-document syntax context.

**Tech Stack:** Swift 6, SwiftUI/AppKit, swift-testing, CodeEditSourceEditor 0.15.2, CodeEditLanguages 0.1.20, SwiftTreeSitter 0.9+, GRDB persistence, XcodeGen.

## Global Constraints

- Minimum deployment target stays macOS 15.0; Swift language mode stays Swift 6.0.
- Pin CodeEditSourceEditor from 0.15.2 and CodeEditLanguages exactly to 0.1.20 (matching SourceEditor 0.15.2).
- Never edit `Tiller.xcodeproj`; update `project.yml` and run `xcodegen generate`.
- `.md` and `.markdown` files keep the existing Markdown editor, toolbar, and preview.
- All other UTF-8 text files open in a `.code` tab; binary/non-UTF-8 files fall back to the system opener.
- Git-status rows and edit-summary rows retain their current primary click action (open diff); editor opening is a separate icon/context-menu action.
- Editor syntax highlighting turns off above 2,000,000 UTF-8 bytes while editing remains enabled.
- Diff highlighting turns off above 500,000 UTF-8 bytes per old/new snapshot and falls back to monochrome text.
- Highlight failures and unsupported languages are silent fallbacks, never user-facing errors.
- Tests use swift-testing (`@Test`, `#expect`), never XCTest.
- `Scripts/ci.sh` must print `CI OK` before the implementation is complete.

---

## File Map

### New package

- `Packages/TillerCode/Package.swift` — leaf package manifest and CodeEdit language/parser dependencies.
- `Packages/TillerCode/Sources/TillerCode/CodeLanguageResolver.swift` — URL and Markdown-fence language detection.
- `Packages/TillerCode/Sources/TillerCode/SyntaxHighlight.swift` — sendable semantic highlight roles/ranges.
- `Packages/TillerCode/Sources/TillerCode/CodeHighlightTheme.swift` — sendable palette/styles plus headless AppKit color conversion.
- `Packages/TillerCode/Sources/TillerCode/TreeSitterHighlighter.swift` — headless tree-sitter parser/query and attributed-output adapter.
- `Packages/TillerCode/Sources/TillerCode/CodeDocument.swift` — file-backed editable buffer and external-change handling.
- `Packages/TillerCode/Sources/TillerCode/DiffHighlighter.swift` — full-document highlights sliced into 1-based line maps plus bounded actor cache.
- `Packages/TillerCode/Tests/TillerCodeTests/*Tests.swift` — package-level TDD coverage.

### New App files

- `App/CodeEditor/EditorTheme+Tiller.swift` — Tiller light/dark SourceEditor theme and semantic-role mapping.
- `App/CodeEditor/AttributedCodeRenderer.swift` — convert pure highlight ranges into AppKit/SwiftUI attributed text.
- `App/CodeEditor/CodeEditorTabView.swift` — SourceEditor tab chrome, conflict banners, and large-file fallback.
- `App/Chat/ChatDiffPreviewView.swift` — full, syntax-highlighted tool-call diff content.
- `AppTests/CodeFileTabTests.swift` — AppModel code-tab lifecycle tests.
- `AppTests/AttributedCodeRendererTests.swift` — range-to-attributed-text tests.
- `AppTests/ChatDiffPreviewTests.swift` — no-truncation diff-row tests.

### Existing files changed

- `project.yml` — add TillerCode and CodeEditSourceEditor; eventually remove Highlightr.
- `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift` — add `.code`, file accessors, and no-pane behavior.
- `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift` — persist/restore `kind = "code"` with `filePath`.
- `Packages/TillerCore/Sources/TillerCore/AgentTree.swift` — ignore code tabs like Markdown tabs.
- `Packages/TillerCore/Sources/TillerCore/MarkdownFileLink.swift` — retain Markdown classification while delegating path parsing to `FileLink`.
- `Packages/TillerCore/Sources/TillerCore/FileLink.swift` — new pure parser for file URLs, relative paths, and `:line[:column]` suffixes.
- `Packages/TillerCore/Tests/TillerCoreTests/WorkspaceTabTests.swift` — code-tab behavior.
- `Packages/TillerCore/Tests/TillerCoreTests/MarkdownTabPersistenceTests.swift` — code-tab persistence and corrupt-row handling.
- `Packages/TillerCore/Tests/TillerCoreTests/FileLinkTests.swift` — file-reference parsing.
- `Packages/TillerGit/Sources/TillerGit/GitDiff.swift` — bounded `oldText`/`newText` snapshots.
- `Packages/TillerGit/Tests/TillerGitTests/GitDiffTests.swift` — snapshot behavior.
- `App/AppModel.swift` — code-document ownership, open/save/close lifecycle, file-opening funnel.
- `App/ContentView.swift` — `.code` dispatch, file drop, terminal URL callback.
- `App/TillerApp.swift` — generalized Save/Open commands.
- `App/WorkspaceTabIcon.swift`, `App/TabBarView.swift`, `App/SidebarView.swift` — code icon and dirty indicator.
- `App/RightPanel/FileExplorerView.swift` — route text files through `openFileTab`.
- `App/RightPanel/RightPanelView.swift` — inject open-editor callbacks.
- `App/RightPanel/GitStatusView.swift` — dedicated editor action without changing row click.
- `App/RightPanel/GitDiffView.swift` — toolbar editor action and syntax-highlighted diff lines.
- `App/Chat/EditSummaryCardView.swift` — dedicated editor action without changing row click.
- `App/Chat/ToolCallCardView.swift` — delegate diff rendering to `ChatDiffPreviewView`.
- `App/Chat/CodeHighlighter.swift` — tree-sitter facade with the existing call signature/cache policy.
- `App/Chat/MarkdownAttributedStringRenderer.swift` — remains coupled only to the facade.
- `AppTests/MarkdownAttributedStringRendererTests.swift` — retain/strengthen highlighting regression tests.

---

### Task 1: Add TillerCode and prove headless tree-sitter highlighting

**Files:**

- Create: `Packages/TillerCode/Package.swift`
- Create: `Packages/TillerCode/Sources/TillerCode/CodeLanguageResolver.swift`
- Create: `Packages/TillerCode/Sources/TillerCode/SyntaxHighlight.swift`
- Create: `Packages/TillerCode/Sources/TillerCode/CodeHighlightTheme.swift`
- Create: `Packages/TillerCode/Sources/TillerCode/TreeSitterHighlighter.swift`
- Create: `Packages/TillerCode/Tests/TillerCodeTests/CodeLanguageResolverTests.swift`
- Create: `Packages/TillerCode/Tests/TillerCodeTests/TreeSitterHighlighterTests.swift`
- Modify: `project.yml:14-64`

**Interfaces:**

- Consumes: `CodeLanguage.detectLanguageFrom(url:prefixBuffer:suffixBuffer:)`, `TreeSitterModel.shared.query(for:)`, `SwiftTreeSitter.Parser`.
- Produces:
  - `CodeLanguageResolver.language(for:contents:) -> TillerCodeLanguage`
  - `CodeLanguageResolver.language(forFence:) -> TillerCodeLanguage`
  - `CodeLanguageResolver.plainText -> TillerCodeLanguage`
  - `SyntaxHighlightRole`
  - `SyntaxHighlightRange(range: NSRange, role: SyntaxHighlightRole)`
  - `CodeHighlightColor`, `CodeHighlightStyle`, `CodeHighlightTheme`
  - `TreeSitterHighlighter.ranges(in:language:) -> [SyntaxHighlightRange]?`
  - `TreeSitterHighlighter.highlight(code:language:theme:font:) -> NSAttributedString?`

- [ ] **Step 1: Add failing language-resolution tests**

```swift
import Foundation
import Testing
import CodeEditLanguages
@testable import TillerCode

@Test func detectsLanguagesFromFileNamesAndShebangs() {
    #expect(CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/App.swift")).id == .swift)
    #expect(CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/Dockerfile")).id == .dockerfile)
    #expect(CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/script"),
        contents: "#!/usr/bin/env python3\nprint('ok')").id == .python)
    #expect(CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/notes.unknown")).id == .plainText)
}

@Test func resolvesMarkdownFenceAliases() {
    #expect(CodeLanguageResolver.language(forFence: "js").id == .javascript)
    #expect(CodeLanguageResolver.language(forFence: "typescript").id == .typescript)
    #expect(CodeLanguageResolver.language(forFence: "py").id == .python)
    #expect(CodeLanguageResolver.language(forFence: "sh").id == .bash)
    #expect(CodeLanguageResolver.language(forFence: "text").id == .plainText)
}
```

- [ ] **Step 2: Add failing headless-highlighter tests**

```swift
import AppKit
import Foundation
import Testing
@testable import TillerCode

private let fixtureTheme = CodeHighlightTheme(
    text: .init(color: .init(red: 0.8, green: 0.8, blue: 0.8)),
    roles: [
        .keyword: .init(color: .init(red: 0.8, green: 0.3, blue: 0.9), bold: true),
        .string: .init(color: .init(red: 0.4, green: 0.8, blue: 0.4)),
        .number: .init(color: .init(red: 0.9, green: 0.6, blue: 0.2)),
        .function: .init(color: .init(red: 0.3, green: 0.7, blue: 0.9)),
        .property: .init(color: .init(red: 0.3, green: 0.7, blue: 0.9))
    ])
@MainActor
private var fixtureFont: NSFont {
    NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
}

@MainActor
@Test func headlessHighlightProducesColorRunsForFourLanguages() throws {
    let fixtures = [
        ("let answer = \"forty-two\"", "/tmp/Test.swift"),
        ("def greet(name):\n    return name", "/tmp/test.py"),
        ("const answer = 42;", "/tmp/test.js"),
        ("{\"enabled\": true}", "/tmp/test.json")
    ]

    for (code, path) in fixtures {
        let output = try #require(TreeSitterHighlighter.highlight(
            code: code,
            language: CodeLanguageResolver.language(
                for: URL(fileURLWithPath: path), contents: code),
            theme: fixtureTheme,
            font: fixtureFont))
        #expect(output.length == (code as NSString).length)
        var hasSyntaxColor = false
        output.enumerateAttribute(
            .foregroundColor,
            in: NSRange(location: 0, length: output.length)) { value, _, _ in
                guard let color = value as? NSColor else { return }
                if !color.isEqual(fixtureTheme.text.color.nsColor) {
                    hasSyntaxColor = true
                }
            }
        #expect(hasSyntaxColor)
    }
}

@Test func swiftHighlightingAlsoExposesValidSemanticRanges() throws {
    let code = "let answer = \"forty-two\""
    let ranges = try #require(TreeSitterHighlighter.ranges(
        in: code,
        language: CodeLanguageResolver.language(
            for: URL(fileURLWithPath: "/tmp/Test.swift"))))
    #expect(ranges.contains { $0.role == .keyword })
    #expect(ranges.contains { $0.role == .string })
    #expect(ranges.allSatisfy { NSMaxRange($0.range) <= (code as NSString).length })
}

@MainActor
@Test func plainTextHasNoHeadlessHighlightResult() {
    #expect(TreeSitterHighlighter.highlight(
        code: "plain",
        language: CodeLanguageResolver.plainText,
        theme: fixtureTheme,
        font: fixtureFont) == nil)
}
```

- [ ] **Step 3: Add the package manifest and project dependencies**

```swift
// Packages/TillerCode/Package.swift
// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TillerCode",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerCode", targets: ["TillerCode"])],
    dependencies: [
        .package(path: "../TillerCore"),
        .package(
            url: "https://github.com/CodeEditApp/CodeEditLanguages.git",
            exact: "0.1.20"),
        .package(
            url: "https://github.com/ChimeHQ/SwiftTreeSitter.git",
            from: "0.9.0")
    ],
    targets: [
        .target(name: "TillerCode", dependencies: [
            "TillerCore", "CodeEditLanguages", "SwiftTreeSitter"
        ]),
        .testTarget(name: "TillerCodeTests", dependencies: [
            "TillerCode", "CodeEditLanguages"
        ])
    ]
)
```

Add to `project.yml`:

```yaml
packages:
  TillerCode:
    path: Packages/TillerCode
  CodeEditSourceEditor:
    url: https://github.com/CodeEditApp/CodeEditSourceEditor.git
    from: 0.15.2

# Tiller target dependencies
      - package: TillerCode
      - package: CodeEditSourceEditor

# TillerTests target dependencies (after `- target: Tiller`)
      - package: TillerCode
      - package: CodeEditSourceEditor
```

Keep Highlightr for now; Task 8 removes it only after the replacement passes.

- [ ] **Step 4: Run tests to verify the new APIs are missing**

Run:

```bash
swift test --package-path Packages/TillerCode
```

Expected: FAIL because `CodeLanguageResolver`, `SyntaxHighlightRange`, and `TreeSitterHighlighter` do not exist.

- [ ] **Step 5: Implement language resolution and semantic roles**

```swift
// CodeLanguageResolver.swift
import Foundation
import CodeEditLanguages

public typealias TillerCodeLanguage = CodeLanguage

public enum CodeLanguageResolver {
    public static var plainText: TillerCodeLanguage { .default }

    public static func language(for url: URL, contents: String? = nil) -> CodeLanguage {
        CodeLanguage.detectLanguageFrom(
            url: url,
            prefixBuffer: contents.map { String($0.prefix(4_096)) },
            suffixBuffer: contents.map { String($0.suffix(4_096)) })
    }

    public static func language(forFence raw: String) -> CodeLanguage {
        let hint = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let alias = [
            "js": "js", "javascript": "js",
            "ts": "ts", "typescript": "ts",
            "py": "py", "python": "py",
            "sh": "sh", "shell": "sh", "bash": "sh",
            "yml": "yml", "yaml": "yaml",
            "md": "md", "markdown": "md"
        ][hint] ?? hint
        guard !alias.isEmpty, alias != "text", alias != "plaintext" else { return .default }
        return CodeLanguage.detectLanguageFrom(
            url: URL(fileURLWithPath: "/tmp/source.\(alias)"))
    }
}
```

```swift
// SyntaxHighlight.swift
import Foundation

public enum SyntaxHighlightRole: String, Sendable, Hashable {
    case keyword, comment, variable, property, function, method
    case number, string, type, parameter, attribute, value, tag
}

public struct SyntaxHighlightRange: Sendable, Hashable {
    public let range: NSRange
    public let role: SyntaxHighlightRole

    public init(range: NSRange, role: SyntaxHighlightRole) {
        self.range = range
        self.role = role
    }
}
```

```swift
// CodeHighlightTheme.swift
import AppKit

public struct CodeHighlightColor: Hashable, Sendable {
    public let red: Double
    public let green: Double
    public let blue: Double
    public let alpha: Double

    public init(red: Double, green: Double, blue: Double, alpha: Double = 1) {
        self.red = red
        self.green = green
        self.blue = blue
        self.alpha = alpha
    }

    @MainActor public var nsColor: NSColor {
        NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
    }
}

public struct CodeHighlightStyle: Hashable, Sendable {
    public let color: CodeHighlightColor
    public let bold: Bool
    public let italic: Bool

    public init(color: CodeHighlightColor, bold: Bool = false, italic: Bool = false) {
        self.color = color
        self.bold = bold
        self.italic = italic
    }
}

public struct CodeHighlightTheme: Sendable {
    public let text: CodeHighlightStyle
    public let roles: [SyntaxHighlightRole: CodeHighlightStyle]

    public init(
        text: CodeHighlightStyle,
        roles: [SyntaxHighlightRole: CodeHighlightStyle]
    ) {
        self.text = text
        self.roles = roles
    }

    public func style(for role: SyntaxHighlightRole) -> CodeHighlightStyle {
        roles[role] ?? text
    }
}
```

- [ ] **Step 6: Implement the verified public tree-sitter path**

Use `TreeSitterModel.shared.query(for:)`; do not copy or call SourceEditor's internal `TreeSitterClient.quickHighlight`.

```swift
import AppKit
import Foundation
import CodeEditLanguages
import SwiftTreeSitter

public enum TreeSitterHighlighter {
    @MainActor
    public static func highlight(
        code: String,
        language: TillerCodeLanguage,
        theme: CodeHighlightTheme,
        font: NSFont
    ) -> NSAttributedString? {
        guard let ranges = ranges(in: code, language: language) else { return nil }
        let output = NSMutableAttributedString(
            string: code,
            attributes: [.font: font, .foregroundColor: theme.text.color.nsColor])
        for range in ranges {
            guard NSMaxRange(range.range) <= output.length else { continue }
            let style = theme.style(for: range.role)
            var traits: NSFontTraitMask = []
            if style.bold { traits.insert(.boldFontMask) }
            if style.italic { traits.insert(.italicFontMask) }
            output.addAttributes(
                [
                    .foregroundColor: style.color.nsColor,
                    .font: traits.isEmpty
                        ? font
                        : NSFontManager.shared.convert(font, toHaveTrait: traits)
                ],
                range: range.range)
        }
        return output
    }

    public static func ranges(
        in code: String,
        language: TillerCodeLanguage
    ) -> [SyntaxHighlightRange]? {
        guard let parserLanguage = language.language,
              let query = TreeSitterModel.shared.query(for: language.id) else { return nil }
        do {
            let parser = Parser()
            try parser.setLanguage(parserLanguage)
            guard let tree = parser.parse(code) else { return nil }
            var accepted: [NSRange: Int] = [:]
            return query.execute(in: tree)
                .resolve(with: .init(string: code))
                .flatMap(\.captures)
                .reversed()
                .compactMap { capture in
                    let range = capture.range
                    if let existing = accepted[range], existing <= capture.index { return nil }
                    guard let role = role(for: capture.name) else { return nil }
                    accepted[range] = capture.index
                    return SyntaxHighlightRange(range: range, role: role)
                }
                .sorted { $0.range.location < $1.range.location }
        } catch {
            return nil
        }
    }

    private static func role(for capture: String) -> SyntaxHighlightRole? {
        let name = capture.lowercased()
        if name.hasPrefix("comment") { return .comment }
        if name.hasPrefix("string") || name.hasPrefix("character") { return .string }
        if name.hasPrefix("number") || name.hasPrefix("float") { return .number }
        if name.hasPrefix("type") { return .type }
        if name.hasPrefix("function") || name.hasPrefix("constructor") { return .function }
        if name.hasPrefix("method") { return .method }
        if name.hasPrefix("property") { return .property }
        if name.hasPrefix("parameter") { return .parameter }
        if name.hasPrefix("attribute") { return .attribute }
        if name.hasPrefix("tag") { return .tag }
        if name.hasPrefix("constant") || name.hasPrefix("boolean") { return .value }
        if name.hasPrefix("variable") { return .variable }
        if ["keyword", "conditional", "repeat", "include", "exception"].contains(
            where: { name.hasPrefix($0) }) { return .keyword }
        return nil
    }
}
```

- [ ] **Step 7: Run the package tests and regenerate the project**

Run:

```bash
swift test --package-path Packages/TillerCode
xcodegen generate
```

Expected: all TillerCode tests PASS; each Swift/Python/JavaScript/JSON fixture contains a non-base foreground-color run; no test constructs `SourceEditor`, `NSTextView`, or any on-screen view. XcodeGen resolves CodeEditSourceEditor 0.15.2 and CodeEditLanguages 0.1.20.

This is the M0 decision gate:

- If semantic `ranges` and headless attributed output both pass, record M0 as successful and continue every task.
- If semantic `ranges` pass but attributed output fails, continue Tasks 2–7 (the editor and both diff surfaces consume ranges), skip Task 8, keep Highlightr for fenced chat code, and record the failed chat-spike criterion.
- If semantic `ranges` also fail through the public parser/query APIs, continue only Tasks 2–4 (M1 editor tabs), skip Tasks 5–8, and report the parser/query blocker.

- [ ] **Step 8: Commit the headless-highlighter spike**

```bash
git add project.yml Packages/TillerCode
git commit -m "feat: add headless tree-sitter highlighting"
```

---

### Task 2: Add the file-backed CodeDocument

**Files:**

- Create: `Packages/TillerCode/Sources/TillerCode/CodeDocument.swift`
- Create: `Packages/TillerCode/Tests/TillerCodeTests/CodeDocumentTests.swift`

**Interfaces:**

- Consumes: `TillerCore.FileSystemEventMonitor`.
- Produces: `@MainActor CodeDocument` with the same public editing contract as `MarkdownDocument`.

- [ ] **Step 1: Write failing load/save/dirty tests**

```swift
import Foundation
import Testing
@testable import TillerCode

@MainActor
@Test func codeDocumentLoadsTracksDirtyAndSaves() throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString).appendingPathExtension("swift")
    defer { try? FileManager.default.removeItem(at: url) }
    try "let old = 1\n".write(to: url, atomically: true, encoding: .utf8)
    let document = try CodeDocument(fileURL: url)
    #expect(document.text == "let old = 1\n")
    #expect(document.isDirty == false)
    document.text = "let new = 2\n"
    #expect(document.isDirty)
    try document.save()
    #expect(document.isDirty == false)
    #expect(try String(contentsOf: url, encoding: .utf8) == "let new = 2\n")
    document.stopWatching()
}

@MainActor
@Test func missingCodeDocumentCanBeRecreated() throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString).appendingPathExtension("swift")
    let document = try CodeDocument(fileURL: url)
    #expect(document.fileDeleted)
    document.text = "let recreated = true\n"
    try document.save()
    defer { try? FileManager.default.removeItem(at: url) }
    #expect(document.fileDeleted == false)
    #expect(FileManager.default.fileExists(atPath: url.path))
    document.stopWatching()
}

@MainActor
@Test func nonUTF8CodeDocumentFailsToLoad() throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString).appendingPathExtension("bin")
    defer { try? FileManager.default.removeItem(at: url) }
    try Data([0xFF, 0xFE, 0x00]).write(to: url)
    #expect(throws: (any Error).self) {
        try CodeDocument(fileURL: url)
    }
}
```

- [ ] **Step 2: Write failing external-change tests**

```swift
@MainActor
@Test func externalChangeReloadsCleanBufferButConflictsWithDirtyBuffer() throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString).appendingPathExtension("py")
    defer { try? FileManager.default.removeItem(at: url) }
    try "one\n".write(to: url, atomically: true, encoding: .utf8)
    let document = try CodeDocument(fileURL: url)

    try "two\n".write(to: url, atomically: true, encoding: .utf8)
    document.handleExternalChange()
    #expect(document.text == "two\n")
    #expect(document.externalChangeConflict == false)

    document.text = "local\n"
    try "remote\n".write(to: url, atomically: true, encoding: .utf8)
    document.handleExternalChange()
    #expect(document.text == "local\n")
    #expect(document.externalChangeConflict)

    document.keepLocalBuffer()
    #expect(document.externalChangeConflict == false)
    document.stopWatching()
}
```

- [ ] **Step 3: Run tests to verify failure**

Run:

```bash
swift test --package-path Packages/TillerCode --filter CodeDocument
```

Expected: FAIL because `CodeDocument` does not exist.

- [ ] **Step 4: Implement CodeDocument with bounded, cancellable monitoring**

```swift
import Foundation
import Observation
import TillerCore

@Observable @MainActor
public final class CodeDocument {
    public let fileURL: URL
    public var text: String
    public private(set) var savedText: String
    public private(set) var externalChangeConflict = false
    public private(set) var fileDeleted: Bool
    public var isDirty: Bool { text != savedText }

    @ObservationIgnored private var monitor: FileSystemEventMonitor?
    @ObservationIgnored private var monitorTask: Task<Void, Never>?

    public init(fileURL: URL) throws {
        self.fileURL = fileURL.standardizedFileURL
        if FileManager.default.fileExists(atPath: fileURL.path) {
            let content = try String(contentsOf: fileURL, encoding: .utf8)
            text = content
            savedText = content
            fileDeleted = false
            startWatching()
        } else {
            text = ""
            savedText = ""
            fileDeleted = true
        }
    }

    public func save() throws {
        try text.write(to: fileURL, atomically: true, encoding: .utf8)
        savedText = text
        fileDeleted = false
        externalChangeConflict = false
        startWatching()
    }

    public func reloadFromDisk() {
        guard let disk = try? String(contentsOf: fileURL, encoding: .utf8) else { return }
        text = disk
        savedText = disk
        fileDeleted = false
        externalChangeConflict = false
    }

    public func keepLocalBuffer() {
        if let disk = try? String(contentsOf: fileURL, encoding: .utf8) { savedText = disk }
        externalChangeConflict = false
    }

    public func stopWatching() {
        monitorTask?.cancel()
        monitorTask = nil
        monitor?.stop()
        monitor = nil
    }

    func handleExternalChange() {
        guard let disk = try? String(contentsOf: fileURL, encoding: .utf8),
              disk != savedText else { return }
        if isDirty { externalChangeConflict = true }
        else { text = disk; savedText = disk }
    }

    func handleFileGone() {
        stopWatching()
        fileDeleted = true
    }

    private func startWatching() {
        stopWatching()
        guard let monitor = FileSystemEventMonitor(roots: [fileURL]) else { return }
        self.monitor = monitor
        monitorTask = Task { [weak self, monitor] in
            for await urls in monitor.events {
                guard !Task.isCancelled else { return }
                guard urls.contains(where: { $0.standardizedFileURL == self?.fileURL }) else { continue }
                guard let self else { return }
                if FileManager.default.fileExists(atPath: self.fileURL.path) {
                    self.handleExternalChange()
                } else {
                    self.handleFileGone()
                    return
                }
            }
        }
    }
}
```

If strict concurrency rejects the weak optional comparison in the loop, capture `let watchedURL = fileURL` before creating the task and compare against that immutable URL.

- [ ] **Step 5: Run tests**

Run:

```bash
swift test --package-path Packages/TillerCode --filter CodeDocument
```

Expected: all CodeDocument tests PASS.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerCode
git commit -m "feat: add file-backed code documents"
```

---

### Task 3: Deliver persistent, editable code tabs end to end

**Files:**

- Modify: `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift`
- Modify: `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift:196-284`
- Modify: `Packages/TillerCore/Sources/TillerCore/AgentTree.swift:60-118`
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/WorkspaceTabTests.swift`
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/MarkdownTabPersistenceTests.swift`
- Create: `App/CodeEditor/EditorTheme+Tiller.swift`
- Create: `App/CodeEditor/AttributedCodeRenderer.swift`
- Create: `App/CodeEditor/CodeEditorTabView.swift`
- Create: `AppTests/AttributedCodeRendererTests.swift`
- Create: `AppTests/CodeFileTabTests.swift`
- Modify: `App/AppModel.swift:47-62,397-434,820-947,1003-1025,1425-1607,1838-1899`
- Modify: `App/ContentView.swift:256-343`
- Modify: `App/TillerApp.swift:39-54`
- Modify: `App/WorkspaceTabIcon.swift`
- Modify: `App/TabBarView.swift:115-164`
- Modify: `App/SidebarView.swift:505-554`

**Interfaces:**

- Consumes: `CodeDocument`, `CodeLanguageResolver`, `SourceEditor`, existing `MarkdownDocument` behavior.
- Produces:
  - `TabContent.code(fileURL:)`
  - `WorkspaceTab.fileURL`, `.codeFileURL`
  - `AppModel.codeDocuments`
  - `AppModel.openFileTab(fileURL:in:)`
  - `AppModel.codeDocument(for:)`
  - `AppModel.isDocumentDirty(tabId:)`
  - `AppModel.saveActiveDocument()`
  - `CodeEditorTabView(document:)`
  - `EditorTheme.tiller(isDark:)`
  - `AttributedCodeRenderer.renderLine(_:ranges:theme:font:)`

- [ ] **Step 1: Extend WorkspaceTab tests first**

```swift
@Test func codeTabHasNoLeavesAndExposesSharedFileURL() {
    let url = URL(fileURLWithPath: "/tmp/App.swift")
    let tab = WorkspaceTab(id: UUID(), title: "App.swift", content: .code(fileURL: url))
    #expect(tab.leafIds.isEmpty)
    #expect(tab.activityPaneIds.isEmpty)
    #expect(tab.markdownFileURL == nil)
    #expect(tab.codeFileURL == url)
    #expect(tab.fileURL == url)
}
```

Also add `#expect(markdown.fileURL == url)` to the existing Markdown test and ensure `nextShellTitle` ignores both Markdown and code tabs.

- [ ] **Step 2: Extend persistence tests first**

Change `saveAndLoadRoundTripsTerminalAndMarkdownTabs` to include:

```swift
let code = WorkspaceTab(
    id: UUID(), title: "App.swift",
    content: .code(fileURL: URL(fileURLWithPath: "/tmp/p/App.swift")))
try await store.saveTabs(
    worktreeId: worktree.id,
    tabs: [terminal, markdown, code],
    activeTabId: code.id)
let loaded = try await store.loadTabs(of: worktree.id)
#expect(loaded.tabs == [terminal, markdown, code])
#expect(loaded.activeTabId == code.id)
```

Add a corrupt-row test identical to `markdownRecordWithoutFilePathIsSkipped`, but with `kind = 'code'` and `filePath = NULL`.

- [ ] **Step 3: Run core tests to verify the new case is missing**

Run:

```bash
swift test --package-path Packages/TillerCore --filter WorkspaceTab
swift test --package-path Packages/TillerCore --filter MarkdownTabPersistence
```

Expected: compile/test FAIL because `.code`, `fileURL`, and `codeFileURL` do not exist.

- [ ] **Step 4: Add the TabContent case and persistence mapping**

```swift
public enum TabContent: Equatable, Sendable {
    case terminal(SplitTree)
    case markdown(fileURL: URL)
    case code(fileURL: URL)
    case chat(agentId: String)
}

public var fileURL: URL? {
    switch content {
    case .markdown(let url), .code(let url): url
    default: nil
    }
}

public var codeFileURL: URL? {
    if case .code(let url) = content { return url }
    return nil
}
```

Treat `.code` exactly like `.markdown` in `activityPaneIds` and `AgentTreeBuilder.build`. In `ProjectStore.saveTabs`, write `kind: "code"`, empty `treeJSON`, and `filePath`. In `loadTabs`, restore `.code(fileURL:)`; skip and log code records with missing `filePath`. Add `.code: source = nil` to `AppModel.requestAutoRename`.

- [ ] **Step 5: Run core tests**

Run:

```bash
swift test --package-path Packages/TillerCore --filter WorkspaceTab
swift test --package-path Packages/TillerCore --filter MarkdownTabPersistence
```

Expected: PASS.

- [ ] **Step 6: Write AppModel code-tab tests**

```swift
import Foundation
import Testing
@testable import Tiller
import TillerCore
import TillerTerminal

@MainActor
struct CodeFileTabTests {
    private struct Fixture {
        let model: AppModel
        let defaults: UserDefaults
        let suiteName: String

        func cleanUp() { defaults.removePersistentDomain(forName: suiteName) }
    }

    private func makeModel() -> Fixture {
        let name = "CodeFileTabTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return Fixture(
            model: AppModel(paneRegistry: PaneRegistry(), registrationTimeoutMs: 100,
                            defaults: defaults),
            defaults: defaults,
            suiteName: name)
    }

    @Test func opensCodeAndMarkdownInTheirOwnTabKindsAndDeduplicates() throws {
        let fixture = makeModel()
        defer { fixture.cleanUp() }
        let model = fixture.model
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let swiftURL = dir.appendingPathComponent("App.swift")
        let markdownURL = dir.appendingPathComponent("README.md")
        try "let value = 1\n".write(to: swiftURL, atomically: true, encoding: .utf8)
        try "# Readme\n".write(to: markdownURL, atomically: true, encoding: .utf8)
        let worktree = Worktree(id: UUID(), projectId: UUID(), branch: "main", path: dir.path)
        model.worktrees = [worktree.projectId: [worktree]]

        let code = try #require(model.openFileTab(fileURL: swiftURL, in: worktree))
        let duplicate = model.openFileTab(fileURL: swiftURL, in: worktree)
        let markdown = try #require(model.openFileTab(fileURL: markdownURL, in: worktree))

        #expect(code.codeFileURL == swiftURL.standardizedFileURL)
        #expect(duplicate?.id == code.id)
        #expect(markdown.markdownFileURL == markdownURL.standardizedFileURL)
        #expect(model.tabs[worktree.id]?.count == 2)
    }

    @Test func saveActiveDocumentWritesCodeBuffer() throws {
        let fixture = makeModel()
        defer { fixture.cleanUp() }
        let model = fixture.model
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString).appendingPathExtension("swift")
        defer { try? FileManager.default.removeItem(at: url) }
        try "old\n".write(to: url, atomically: true, encoding: .utf8)
        let worktree = Worktree(id: UUID(), projectId: UUID(), branch: "main", path: url.deletingLastPathComponent().path)
        model.worktrees = [worktree.projectId: [worktree]]
        let tab = try #require(model.openFileTab(fileURL: url, in: worktree))
        model.codeDocuments[tab.id]?.text = "new\n"
        #expect(model.isDocumentDirty(tabId: tab.id))
        model.saveActiveDocument()
        #expect(try String(contentsOf: url, encoding: .utf8) == "new\n")
        #expect(model.isDocumentDirty(tabId: tab.id) == false)
    }
}
```

- [ ] **Step 7: Run App tests to verify failure**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/CodeFileTabTests
```

Expected: FAIL because AppModel does not own code documents or expose the new lifecycle API.

- [ ] **Step 8: Implement AppModel ownership and lifecycle**

Add:

```swift
var codeDocuments: [UUID: CodeDocument] = [:]

func isDocumentDirty(tabId: UUID) -> Bool {
    markdownDocuments[tabId]?.isDirty == true || codeDocuments[tabId]?.isDirty == true
}

@discardableResult
func openFileTab(fileURL: URL, in worktree: Worktree) -> WorkspaceTab? {
    let url = fileURL.standardizedFileURL
    if let existing = tabs[worktree.id]?.first(where: {
        $0.fileURL?.standardizedFileURL == url
    }) {
        selectedWorktree = worktree
        activeTabId[worktree.id] = existing.id
        persistTabs(for: worktree.id)
        return existing
    }
    do {
        let tab: WorkspaceTab
        if MarkdownFileLink.isMarkdown(url) {
            let document = try MarkdownDocument(fileURL: url)
            tab = WorkspaceTab(id: UUID(), title: url.lastPathComponent,
                               content: .markdown(fileURL: url))
            markdownDocuments[tab.id] = document
        } else {
            let document = try CodeDocument(fileURL: url)
            tab = WorkspaceTab(id: UUID(), title: url.lastPathComponent,
                               content: .code(fileURL: url))
            codeDocuments[tab.id] = document
        }
        selectedWorktree = worktree
        tabs[worktree.id, default: []].append(tab)
        activeTabId[worktree.id] = tab.id
        persistTabs(for: worktree.id)
        return tab
    } catch {
        lastError = "Apertura \(url.lastPathComponent) fallita: \(error.localizedDescription)"
        NSWorkspace.shared.open(url)
        return nil
    }
}

func codeDocument(for tab: WorkspaceTab) -> CodeDocument? {
    guard let url = tab.codeFileURL else { return nil }
    if let document = codeDocuments[tab.id] { return document }
    guard let document = try? CodeDocument(fileURL: url) else { return nil }
    codeDocuments[tab.id] = document
    return document
}
```

Replace the save command with a document-agnostic implementation:

```swift
func saveActiveDocument() {
    guard let worktree = selectedWorktree,
          let tab = activeTab(for: worktree.id) else { return }
    do {
        if let document = markdownDocuments[tab.id] {
            try document.save()
        } else if let document = codeDocuments[tab.id] {
            try document.save()
        }
    } catch {
        lastError = "Salvataggio \(tab.fileURL?.lastPathComponent ?? tab.title) fallito: "
            + error.localizedDescription
    }
}

private func resolveDirtyClose(
    fileURL: URL,
    save: () throws -> Void
) -> Bool {
    let alert = NSAlert()
    alert.messageText = "Salvare le modifiche a \(fileURL.lastPathComponent)?"
    alert.informativeText = "Le modifiche andranno perse se non le salvi."
    alert.addButton(withTitle: "Salva")
    alert.addButton(withTitle: "Non salvare")
    alert.addButton(withTitle: "Annulla")
    switch alert.runModal() {
    case .alertFirstButtonReturn:
        do { try save(); return true }
        catch {
            lastError = "Salvataggio fallito: \(error.localizedDescription)"
            return false
        }
    case .alertSecondButtonReturn:
        return true
    default:
        return false
    }
}
```

At the top of `closeTab`, use the same alert for both document types:

```swift
if let document = markdownDocuments[tabId], document.isDirty {
    guard resolveDirtyClose(fileURL: document.fileURL, save: document.save) else { return }
}
if let document = codeDocuments[tabId], document.isDirty {
    guard resolveDirtyClose(fileURL: document.fileURL, save: document.save) else { return }
}
```

Replace `teardownMarkdownDocument` with one funnel and call it from `closeTab`, `removeProject`, and `removeWorktree`:

```swift
private func teardownDocument(tabId: UUID) {
    markdownDocuments[tabId]?.stopWatching()
    markdownDocuments[tabId] = nil
    codeDocuments[tabId]?.stopWatching()
    codeDocuments[tabId] = nil
}
```

Update idle-worktree eviction to evaluate:

```swift
hasUnsavedWork: { [weak self] id in
    (self?.tabs[id] ?? []).contains {
        self?.isDocumentDirty(tabId: $0.id) == true
    }
}
```

- [ ] **Step 9: Add a shared Tiller theme and attributed renderer**

TDD order for this step: first create `AppTests/AttributedCodeRendererTests.swift` from the test block at the end of this step, then run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/AttributedCodeRendererTests
```

Expected: compile FAIL because `EditorTheme.tiller` and `AttributedCodeRenderer` do not exist. Only after observing that failure, add the two production files below.

Create `EditorTheme.tiller(isDark:)` with the complete initializer and semantic-role mapping:

```swift
import AppKit
import CodeEditSourceEditor
import TillerCode

extension EditorTheme {
    static func tiller(isDark: Bool) -> EditorTheme {
        let text = NSColor(srgbRed: isDark ? 0.85 : 0.15,
                           green: isDark ? 0.86 : 0.16,
                           blue: isDark ? 0.89 : 0.20, alpha: 1)
        let keyword = NSColor(srgbRed: isDark ? 0.78 : 0.43,
                              green: isDark ? 0.58 : 0.24,
                              blue: isDark ? 0.96 : 0.66, alpha: 1)
        let type = NSColor(srgbRed: isDark ? 0.39 : 0.05,
                           green: isDark ? 0.78 : 0.48,
                           blue: isDark ? 0.84 : 0.53, alpha: 1)
        let string = NSColor(srgbRed: isDark ? 0.63 : 0.12,
                             green: isDark ? 0.82 : 0.50,
                             blue: isDark ? 0.58 : 0.20, alpha: 1)
        let number = NSColor(srgbRed: isDark ? 0.91 : 0.68,
                             green: isDark ? 0.69 : 0.35,
                             blue: isDark ? 0.36 : 0.04, alpha: 1)
        let comment = NSColor(srgbRed: isDark ? 0.52 : 0.38,
                              green: isDark ? 0.56 : 0.42,
                              blue: isDark ? 0.62 : 0.45, alpha: 1)
        return EditorTheme(
            text: .init(color: text),
            insertionPoint: isDark ? .white : .black,
            invisibles: .init(color: comment.withAlphaComponent(0.65)),
            background: NSColor(
                srgbRed: isDark ? 0.071 : 0.965,
                green: isDark ? 0.071 : 0.965,
                blue: isDark ? 0.086 : 0.975,
                alpha: 1),
            lineHighlight: NSColor(
                srgbRed: isDark ? 0.11 : 0.91,
                green: isDark ? 0.11 : 0.92,
                blue: isDark ? 0.14 : 0.95,
                alpha: 1),
            selection: NSColor.selectedTextBackgroundColor,
            keywords: .init(color: keyword, bold: true),
            commands: .init(color: keyword),
            types: .init(color: type),
            attributes: .init(color: keyword),
            variables: .init(color: text),
            values: .init(color: text),
            numbers: .init(color: number),
            strings: .init(color: string),
            characters: .init(color: string),
            comments: .init(color: comment, italic: true))
    }

    func attribute(for role: SyntaxHighlightRole) -> Attribute {
        switch role {
        case .keyword, .tag: keywords
        case .comment: comments
        case .type: types
        case .attribute: attributes
        case .number: numbers
        case .string: strings
        case .variable, .property, .function, .method, .parameter: variables
        case .value: values
        }
    }
}
```

Build the package-level headless theme from the same palette in the App layer:

```swift
extension CodeHighlightTheme {
    static func tiller(isDark: Bool) -> CodeHighlightTheme {
        let text = CodeHighlightColor(
            red: isDark ? 0.85 : 0.15,
            green: isDark ? 0.86 : 0.16,
            blue: isDark ? 0.89 : 0.20)
        let keyword = CodeHighlightColor(
            red: isDark ? 0.78 : 0.43,
            green: isDark ? 0.58 : 0.24,
            blue: isDark ? 0.96 : 0.66)
        let type = CodeHighlightColor(
            red: isDark ? 0.39 : 0.05,
            green: isDark ? 0.78 : 0.48,
            blue: isDark ? 0.84 : 0.53)
        let string = CodeHighlightColor(
            red: isDark ? 0.63 : 0.12,
            green: isDark ? 0.82 : 0.50,
            blue: isDark ? 0.58 : 0.20)
        let number = CodeHighlightColor(
            red: isDark ? 0.91 : 0.68,
            green: isDark ? 0.69 : 0.35,
            blue: isDark ? 0.36 : 0.04)
        let comment = CodeHighlightColor(
            red: isDark ? 0.52 : 0.38,
            green: isDark ? 0.56 : 0.42,
            blue: isDark ? 0.62 : 0.45)
        return CodeHighlightTheme(
            text: .init(color: text),
            roles: [
                .keyword: .init(color: keyword, bold: true),
                .tag: .init(color: keyword),
                .comment: .init(color: comment, italic: true),
                .type: .init(color: type),
                .attribute: .init(color: keyword),
                .number: .init(color: number),
                .string: .init(color: string),
                .variable: .init(color: text),
                .property: .init(color: text),
                .function: .init(color: text),
                .method: .init(color: text),
                .parameter: .init(color: text),
                .value: .init(color: text)
            ])
    }
}
```

Implement the renderer rather than duplicating attributed-string logic in each diff surface:

```swift
import AppKit
import CodeEditSourceEditor
import TillerCode

@MainActor
enum AttributedCodeRenderer {
    static func renderLine(
        _ text: String,
        ranges: [SyntaxHighlightRange],
        theme: EditorTheme,
        font: NSFont
    ) -> AttributedString {
        let output = NSMutableAttributedString(
            string: text,
            attributes: [.font: font, .foregroundColor: theme.text.color])
        let bounds = NSRange(location: 0, length: output.length)
        for run in ranges {
            let clipped = NSIntersectionRange(bounds, run.range)
            guard clipped.length > 0 else { continue }
            let style = theme.attribute(for: run.role)
            var traits: NSFontTraitMask = []
            if style.bold { traits.insert(.boldFontMask) }
            if style.italic { traits.insert(.italicFontMask) }
            let styledFont = traits.isEmpty
                ? font
                : NSFontManager.shared.convert(font, toHaveTrait: traits)
            output.addAttributes(
                [.foregroundColor: style.color, .font: styledFont],
                range: clipped)
        }
        return AttributedString(output)
    }
}
```

Create the test with concrete color assertions:

```swift
import AppKit
import CodeEditSourceEditor
import Testing
import TillerCode
@testable import Tiller

@MainActor
struct AttributedCodeRendererTests {
    @Test func appliesRoleColorAndPreservesPlainTextColor() throws {
        let theme = EditorTheme.tiller(isDark: true)
        let font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        let rendered = NSAttributedString(AttributedCodeRenderer.renderLine(
            "let value",
            ranges: [SyntaxHighlightRange(
                range: NSRange(location: 0, length: 3), role: .keyword)],
            theme: theme,
            font: font))
        let keyword = try #require(rendered.attribute(
            .foregroundColor, at: 0, effectiveRange: nil) as? NSColor)
        let plain = try #require(rendered.attribute(
            .foregroundColor, at: 4, effectiveRange: nil) as? NSColor)
        #expect(keyword == theme.keywords.color)
        #expect(plain == theme.text.color)
    }

    @Test func ignoresOutOfBoundsRangesAndKeepsTextLength() {
        let rendered = AttributedCodeRenderer.renderLine(
            "let value = 1",
            ranges: [
                SyntaxHighlightRange(
                    range: NSRange(location: 0, length: 3), role: .keyword),
                SyntaxHighlightRange(
                    range: NSRange(location: 99, length: 2), role: .string)
            ],
            theme: .tiller(isDark: true),
            font: .monospacedSystemFont(ofSize: 11, weight: .regular))
        #expect(NSAttributedString(rendered).length == 13)
    }
}
```

- [ ] **Step 10: Implement CodeEditorTabView and content dispatch**

```swift
import CodeEditSourceEditor
import SwiftUI
import TillerCode

struct CodeEditorTabView: View {
    @Bindable var document: CodeDocument
    @Environment(\.colorScheme) private var colorScheme
    @State private var editorState = SourceEditorState()
    private static let highlightByteLimit = 2_000_000

    private var detectedLanguage: TillerCodeLanguage {
        guard document.text.utf8.count <= Self.highlightByteLimit else {
            return CodeLanguageResolver.plainText
        }
        return CodeLanguageResolver.language(for: document.fileURL, contents: document.text)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            if document.externalChangeConflict { conflictBanner }
            if document.fileDeleted { deletedBanner }
            SourceEditor(
                $document.text,
                language: detectedLanguage,
                configuration: SourceEditorConfiguration(
                    appearance: .init(
                        theme: .tiller(isDark: colorScheme == .dark),
                        font: .monospacedSystemFont(ofSize: 12, weight: .regular),
                        wrapLines: false),
                    behavior: .init(isEditable: true, isSelectable: true,
                                    indentOption: .spaces(count: 4))),
                state: $editorState)
        }
        .background(AppTheme.background)
    }
}
```

Add the complete chrome to `CodeEditorTabView`:

```swift
private var header: some View {
    HStack(spacing: 8) {
        Text(document.fileURL.path)
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .truncationMode(.middle)
        Text(detectedLanguage.tsName)
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.secondary)
        Spacer()
    }
    .padding(.horizontal, 10)
    .padding(.vertical, 6)
}

private var conflictBanner: some View {
    HStack {
        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
        Text("File modificato su disco.")
        Spacer()
        Button("Ricarica") { document.reloadFromDisk() }
        Button("Mantieni") { document.keepLocalBuffer() }
    }
    .font(.system(size: 12))
    .padding(8)
    .background(.orange.opacity(0.15))
}

private var deletedBanner: some View {
    HStack {
        Image(systemName: "trash").foregroundStyle(.red)
        Text("File eliminato su disco. ⌘S lo ricrea.")
        Spacer()
    }
    .font(.system(size: 12))
    .padding(8)
    .background(.red.opacity(0.15))
}
```

Add deliberate content dispatch:

```swift
case .code:
    if let document = model.codeDocument(for: tab) {
        CodeEditorTabView(document: document)
    } else {
        ContentUnavailableView(
            "File non leggibile",
            systemImage: "doc.questionmark",
            description: Text(tab.codeFileURL?.path ?? ""))
    }
```

A retained missing code tab successfully creates a `CodeDocument` with `fileDeleted == true`, so it takes the editor/banner path rather than the unavailable path.

- [ ] **Step 11: Update commands, icon, dirty indicators, and exhaustive switches**

- `TillerApp`: call `saveActiveDocument()`.
- `WorkspaceTabIcon`: `.code` uses `chevron.left.forwardslash.chevron.right`.
- `TabBarView` and `SidebarView`: call `model.isDocumentDirty(tabId:)`.
- `AgentTreeBuilder` and `requestAutoRename`: `.code` is non-agent content.
- Every switch over `TabContent` must have a deliberate `.code` case; do not use `default` to hide future cases.

- [ ] **Step 12: Run focused tests and build**

Run:

```bash
swift test --package-path Packages/TillerCore
swift test --package-path Packages/TillerCode
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/CodeFileTabTests \
  -only-testing:TillerTests/AttributedCodeRendererTests
```

Expected: all tests PASS and all `TabContent` switches compile.

- [ ] **Step 13: Commit**

```bash
git add Packages/TillerCore Packages/TillerCode App AppTests project.yml
git commit -m "feat: add persistent code editor tabs"
```

---

### Task 4: Route every requested file-opening entry point through the editor

**Files:**

- Create: `Packages/TillerCore/Sources/TillerCore/FileLink.swift`
- Create: `Packages/TillerCore/Tests/TillerCoreTests/FileLinkTests.swift`
- Modify: `Packages/TillerCore/Sources/TillerCore/MarkdownFileLink.swift`
- Modify: `App/AppModel.swift:1511-1579`
- Modify: `App/ContentView.swift:175-229,256-343`
- Modify: `App/TillerApp.swift:39-54`
- Modify: `App/RightPanel/FileExplorerView.swift:158-166`
- Modify: `App/RightPanel/RightPanelView.swift:77-117`
- Modify: `App/RightPanel/GitStatusView.swift:24-188`
- Modify: `App/RightPanel/GitDiffView.swift:5-128`
- Modify: `App/Chat/EditSummaryCardView.swift:56-78`
- Modify: `App/Chat/ToolCallCardView.swift:86-105,111-147`

**Interfaces:**

- Consumes: `AppModel.openFileTab(fileURL:in:)`.
- Produces:
  - `FileLink.resolve(_:worktreePath:) -> URL?`
  - `AppModel.openFileReference(_:in:)`
  - `AppModel.openFilePanel()`
  - `GitStatusView.onOpenFile: (GitStatusEntry) -> Void`
  - `GitDiffView.onOpenFile: (URL) -> Void`

- [ ] **Step 1: Write FileLink tests**

```swift
import Foundation
import Testing
@testable import TillerCore

@Test func resolvesRelativeAbsoluteAndFileURLReferences() {
    #expect(FileLink.resolve("Sources/App.swift", worktreePath: "/repo") ==
            URL(fileURLWithPath: "/repo/Sources/App.swift").standardizedFileURL)
    #expect(FileLink.resolve("/tmp/test.py", worktreePath: "/repo") ==
            URL(fileURLWithPath: "/tmp/test.py").standardizedFileURL)
    #expect(FileLink.resolve("file:///tmp/data.json", worktreePath: "/repo") ==
            URL(fileURLWithPath: "/tmp/data.json").standardizedFileURL)
}

@Test func stripsTerminalLineAndColumnSuffixes() {
    #expect(FileLink.resolve("Sources/App.swift:42", worktreePath: "/repo")?.path ==
            "/repo/Sources/App.swift")
    #expect(FileLink.resolve("Sources/App.swift:42:7", worktreePath: "/repo")?.path ==
            "/repo/Sources/App.swift")
}

@Test func rejectsNonFileSchemes() {
    #expect(FileLink.resolve("https://example.com/file.swift", worktreePath: "/repo") == nil)
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
swift test --package-path Packages/TillerCore --filter FileLink
```

Expected: FAIL because `FileLink` does not exist.

- [ ] **Step 3: Implement FileLink and retain Markdown classification**

```swift
public enum FileLink {
    public static func resolve(_ raw: String, worktreePath: String) -> URL? {
        let stripped = raw.replacingOccurrences(
            of: #":\d+(?::\d+)?$"#,
            with: "",
            options: .regularExpression)
        let url: URL
        if let parsed = URL(string: stripped), parsed.scheme == "file" {
            url = URL(fileURLWithPath: parsed.path)
        } else if stripped.hasPrefix("/") {
            url = URL(fileURLWithPath: stripped)
        } else if !stripped.contains("://") {
            url = URL(fileURLWithPath: worktreePath).appendingPathComponent(stripped)
        } else {
            return nil
        }
        return url.standardizedFileURL
    }
}
```

Make `MarkdownFileLink.resolve` call `FileLink.resolve` and then guard `isMarkdown`, preserving existing API behavior.

- [ ] **Step 4: Generalize AppModel's reference and open-panel funnels**

```swift
func openFileReference(_ raw: String, in worktree: Worktree) {
    if let fileURL = FileLink.resolve(raw, worktreePath: worktree.path) {
        openFileTab(fileURL: fileURL, in: worktree)
    } else if let url = URL(string: raw) {
        NSWorkspace.shared.open(url)
    }
}

func openFilePanel() {
    guard let worktree = selectedWorktree else { return }
    let panel = NSOpenPanel()
    panel.directoryURL = URL(fileURLWithPath: worktree.path)
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    guard panel.runModal() == .OK, let url = panel.url else { return }
    openFileTab(fileURL: url, in: worktree)
}
```

Update `TillerApp`, terminal `onOpenURL`, drag/drop, `FileExplorerView`, and tool-call location buttons. Drop accepts the first regular file URL; `openFileTab` performs UTF-8 validation/fallback.

- [ ] **Step 5: Preserve Git-status click and add a separate editor action**

Add `let onOpenFile: (GitStatusEntry) -> Void` to `GitStatusView`. In `statusRow`, keep:

```swift
.onTapGesture { onOpenDiff(entry) }
```

Add an icon button inside the row:

```swift
Button { onOpenFile(entry) } label: {
    Image(systemName: "chevron.left.forwardslash.chevron.right")
}
.buttonStyle(.plain)
.help("Apri in editor")
```

Apply the context menu to the whole row, after the existing primary tap gesture:

```swift
.onTapGesture { onOpenDiff(entry) }
.contextMenu {
    Button("Apri in editor") { onOpenFile(entry) }
}
```

Wire it in `RightPanelView` using `panelModel.rootURL.appendingPathComponent(entry.path.value)` and `appModel.openFileTab`.

- [ ] **Step 6: Add the GitDiff toolbar editor action**

Add `let onOpenFile: (URL) -> Void` to `GitDiffView`. Next to Refresh:

```swift
if let url = selectedFileURL {
    Button { onOpenFile(url) } label: {
        Image(systemName: "chevron.left.forwardslash.chevron.right")
    }
    .buttonStyle(.plain)
    .help("Apri in editor")
}
```

Replace the error-state `NSWorkspace.shared.open(url)` call with the same closure.

- [ ] **Step 7: Preserve edit-summary click and add its editor action**

Keep the existing filename button calling `requestChatFollow`. Add a sibling icon button:

```swift
Button {
    appModel.openFileReference(path, in: worktree)
} label: {
    Image(systemName: "chevron.left.forwardslash.chevron.right")
}
.buttonStyle(.plain)
.help("Apri in editor")
```

Tool-call diff headers and location rows use the same `openFileReference` funnel.

- [ ] **Step 8: Run tests and an App build**

Run:

```bash
swift test --package-path Packages/TillerCore --filter FileLink
xcodegen generate
xcodebuild build -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS'
```

Expected: FileLink tests PASS; App builds; primary diff clicks are still wired to their original closures.

- [ ] **Step 9: Commit**

```bash
git add Packages/TillerCore App
git commit -m "feat: open project files in editor tabs"
```

---

### Task 5: Supply full-document diff context and line-sliced highlights

**Files:**

- Modify: `Packages/TillerGit/Sources/TillerGit/GitDiff.swift`
- Modify: `Packages/TillerGit/Tests/TillerGitTests/GitDiffTests.swift`
- Create: `Packages/TillerCode/Sources/TillerCode/DiffHighlighter.swift`
- Create: `Packages/TillerCode/Tests/TillerCodeTests/DiffHighlighterTests.swift`

**Interfaces:**

- Consumes: `TreeSitterHighlighter.ranges(in:language:)`, `CodeLanguageResolver`.
- Produces:
  - `GitFileDiff.oldText: String?`
  - `GitFileDiff.newText: String?`
  - `LineHighlightMap.init?(text:language:)`
  - `LineHighlightMap.ranges(forLine:)`
  - `DiffHighlights.old`, `.new`
  - `DiffHighlightCache.shared.highlights(path:oldText:newText:)`

- [ ] **Step 1: Extend live GitDiff tests with snapshots**

In `liveDiffCombinesStagedAndUnstagedChanges`, add:

```swift
#expect(diff.oldText == "one\n")
#expect(diff.newText == "three\n")
```

In `untrackedAndUnbornFilesDiffFromDevNull`, add:

```swift
#expect(diff.oldText == nil)
#expect(diff.newText == fileContents)
```

Add a deleted-file test:

```swift
@Test func deletedFileCarriesHeadSnapshotOnly() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try FileManager.default.removeItem(at: repo.appendingPathComponent("file.txt"))
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.entries.first { $0.path.value == "file.txt" })

    let diff = try await GitDiff.load(entry: entry, in: repo.path)

    #expect(diff.oldText == "one\n")
    #expect(diff.newText == nil)
}
```

- [ ] **Step 2: Run Git tests to verify failure**

Run:

```bash
swift test --package-path Packages/TillerGit --filter GitDiff
```

Expected: compile FAIL because snapshot properties do not exist.

- [ ] **Step 3: Add bounded old/new snapshots**

Add optional snapshot fields to `GitFileDiff`; preserve parser tests with defaults:

```swift
public let oldText: String?
public let newText: String?
```

`GitDiff.load` resolves:

- old path: `entry.originalPath ?? entry.path`, via `git show HEAD:<path>`;
- new path: current worktree file at `entry.path`;
- untracked/unborn old: `nil`;
- deleted new: `nil`;
- invalid UTF-8 or over 500,000 bytes: `nil` for that snapshot.

Use concrete bounded helpers and pass snapshots into the parser:

```swift
private static let snapshotLimits = GitOutputLimits(
    maxBytes: 500_000,
    maxLines: 20_000)

private static func readUTF8(at url: URL) -> String? {
    guard let data = try? Data(contentsOf: url, options: .mappedIfSafe),
          data.count <= snapshotLimits.maxBytes else { return nil }
    return String(data: data, encoding: .utf8)
}

private static func headText(
    path: GitPath,
    repoPath: String
) async -> String? {
    guard let result = try? await GitRunner.runCaptured(
        ["show", "HEAD:\(path.value)"],
        in: repoPath,
        limits: snapshotLimits) else { return nil }
    return String(data: result.stdout, encoding: .utf8)
}
```

In `load`, after determining `hasHead`:

```swift
let oldPath = entry.originalPath ?? entry.path
let oldText = hasHead && !entry.isUntracked
    ? await headText(path: oldPath, repoPath: repoPath)
    : nil
let newURL = URL(fileURLWithPath: repoPath, isDirectory: true)
    .appendingPathComponent(entry.path.value)
let newText = readUTF8(at: newURL)
return try parse(
    result.stdoutString,
    path: entry.path,
    oldText: oldText,
    newText: newText)
```

Change `parse` to accept `oldText: String? = nil` and `newText: String? = nil`, and copy both into `GitFileDiff`, preserving every existing parser call site.

- [ ] **Step 4: Run Git tests**

Run:

```bash
swift test --package-path Packages/TillerGit --filter GitDiff
```

Expected: PASS.

- [ ] **Step 5: Write DiffHighlighter tests**

```swift
import Foundation
import Testing
@testable import TillerCode

@Test func slicesGlobalRangesIntoOneBasedUTF16Lines() throws {
    let text = "let first = 1\nlet second = \"two\"\n"
    let map = try #require(LineHighlightMap(
        text: text,
        language: CodeLanguageResolver.language(
            for: URL(fileURLWithPath: "/tmp/Test.swift"))))
    #expect(map.ranges(forLine: 1).contains { $0.role == .keyword })
    #expect(map.ranges(forLine: 2).contains { $0.role == .string })
    #expect(map.ranges(forLine: 3).isEmpty)
    #expect(map.ranges(forLine: 0).isEmpty)
}

@Test func diffHighlightsUseIndependentOldAndNewDocuments() {
    let language = CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/Test.swift"))
    let highlights = DiffHighlights(
        oldText: "let old = \"before\"\n",
        newText: "let new = 42\n",
        language: language)
    #expect(highlights.old?.ranges(forLine: 1).contains { $0.role == .string } == true)
    #expect(highlights.new?.ranges(forLine: 1).contains { $0.role == .number } == true)
}

@Test func oversizedDiffSnapshotFallsBackToNoMap() {
    let text = String(repeating: "a", count: 500_001)
    let highlights = DiffHighlights(
        oldText: nil, newText: text,
        language: CodeLanguageResolver.language(
            for: URL(fileURLWithPath: "/tmp/Test.swift")))
    #expect(highlights.new == nil)
}
```

- [ ] **Step 6: Run TillerCode tests to verify failure**

Run:

```bash
swift test --package-path Packages/TillerCode --filter DiffHighlight
```

Expected: FAIL because line maps and diff highlights do not exist.

- [ ] **Step 7: Implement UTF-16-safe line maps and actor cache**

`LineHighlightMap` pre-slices source-global ranges once, so every SwiftUI row lookup is O(1). Use `NSString.getLineStart(_:end:contentsEnd:for:)` so CRLF and Unicode match the NSRange coordinates returned by SwiftTreeSitter:

```swift
public struct LineHighlightMap: Sendable {
    private let rangesByLine: [[SyntaxHighlightRange]]

    public init?(text: String, language: TillerCodeLanguage) {
        guard let highlights = TreeSitterHighlighter.ranges(
            in: text, language: language) else { return nil }
        let source = text as NSString
        var contentRanges: [NSRange] = []
        var fullRanges: [NSRange] = []
        var cursor = 0
        while cursor < source.length {
            var start = 0
            var end = 0
            var contentsEnd = 0
            source.getLineStart(
                &start, end: &end, contentsEnd: &contentsEnd,
                for: NSRange(location: cursor, length: 0))
            contentRanges.append(NSRange(
                location: start, length: contentsEnd - start))
            fullRanges.append(NSRange(location: start, length: end - start))
            cursor = max(end, cursor + 1)
        }

        var rows = Array(repeating: [SyntaxHighlightRange](),
                         count: contentRanges.count)
        for highlight in highlights where highlight.range.length > 0 {
            guard let first = Self.lineIndex(
                    containing: highlight.range.location, in: fullRanges),
                  let last = Self.lineIndex(
                    containing: NSMaxRange(highlight.range) - 1, in: fullRanges)
            else { continue }
            for index in first...last {
                let line = contentRanges[index]
                let intersection = NSIntersectionRange(line, highlight.range)
                guard intersection.length > 0 else { continue }
                rows[index].append(SyntaxHighlightRange(
                    range: NSRange(
                        location: intersection.location - line.location,
                        length: intersection.length),
                    role: highlight.role))
            }
        }
        rangesByLine = rows
    }

    public func ranges(forLine lineNumber: Int) -> [SyntaxHighlightRange] {
        guard lineNumber > 0, lineNumber <= rangesByLine.count else { return [] }
        return rangesByLine[lineNumber - 1]
    }

    private static func lineIndex(
        containing location: Int,
        in ranges: [NSRange]
    ) -> Int? {
        var low = 0
        var high = ranges.count - 1
        while low <= high {
            let middle = (low + high) / 2
            let range = ranges[middle]
            if location < range.location { high = middle - 1 }
            else if location >= NSMaxRange(range) { low = middle + 1 }
            else { return middle }
        }
        return nil
    }
}

public struct DiffHighlights: Sendable {
    public static let byteLimit = 500_000
    public let old: LineHighlightMap?
    public let new: LineHighlightMap?

    public init(oldText: String?, newText: String?, language: TillerCodeLanguage) {
        old = Self.map(text: oldText, language: language)
        new = Self.map(text: newText, language: language)
    }

    private static func map(
        text: String?,
        language: TillerCodeLanguage
    ) -> LineHighlightMap? {
        guard let text, text.utf8.count <= byteLimit else { return nil }
        return LineHighlightMap(text: text, language: language)
    }
}
```

Add the bounded cache; its public API receives only sendable URL/String values so `CodeLanguage` never crosses actor isolation:

```swift
public actor DiffHighlightCache {
    public static let shared = DiffHighlightCache()

    private struct Key: Hashable {
        let path: String
        let oldHash: Int?
        let newHash: Int?
    }
    private var entries: [Key: DiffHighlights] = [:]
    private static let maxEntries = 32

    public func highlights(
        path: URL,
        oldText: String?,
        newText: String?
    ) -> DiffHighlights {
        let key = Key(
            path: path.standardizedFileURL.path,
            oldHash: oldText?.hashValue,
            newHash: newText?.hashValue)
        if let cached = entries[key] { return cached }
        let sample = newText ?? oldText ?? ""
        let language = CodeLanguageResolver.language(for: path, contents: sample)
        let value = DiffHighlights(
            oldText: oldText, newText: newText, language: language)
        if entries.count >= Self.maxEntries { entries.removeAll(keepingCapacity: true) }
        entries[key] = value
        return value
    }
}
```

- [ ] **Step 8: Run both package suites**

Run:

```bash
swift test --package-path Packages/TillerGit --filter GitDiff
swift test --package-path Packages/TillerCode --filter DiffHighlight
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add Packages/TillerGit Packages/TillerCode
git commit -m "feat: add syntax context for diffs"
```

---

### Task 6: Syntax-highlight the right-panel Git diff

**Files:**

- Modify: `App/RightPanel/GitDiffView.swift`
- Modify: `App/RightPanel/RightPanelView.swift`

**Interfaces:**

- Consumes: `GitFileDiff.oldText/newText`, `DiffHighlightCache`, `AttributedCodeRenderer`.
- Produces: `UnifiedDiffPane(diff:fileURL:)` with syntax-colored code and unchanged diff gutters/backgrounds.

- [ ] **Step 1: Run the existing renderer tests as a baseline**

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/AttributedCodeRendererTests
```

Expected: PASS, including semantic colors and out-of-bounds clipping established in Task 3.

- [ ] **Step 2: Make UnifiedDiffPane load highlights once per diff identity**

Change the call to:

```swift
UnifiedDiffPane(diff: diff, fileURL: selectedFileURL)
```

Inside the pane, store the result and derive a stable-in-process task identity:

```swift
import TillerCode

private struct UnifiedDiffPane: View {
    let diff: GitFileDiff
    let fileURL: URL?

    @Environment(\.colorScheme) private var colorScheme
    @State private var highlights: DiffHighlights?

private struct HighlightRequest: Hashable {
    let path: String?
    let oldHash: Int?
    let newHash: Int?
}

private var requestID: HighlightRequest {
    HighlightRequest(
        path: fileURL?.standardizedFileURL.path,
        oldHash: diff.oldText?.hashValue,
        newHash: diff.newText?.hashValue)
}

// Keep the existing `body`, `lineRow`, `foreground`, and `background`
// members below these properties; Step 4 replaces only the code text node.
```

This code block is the replacement declaration header; the existing members remain inside the same struct before its existing final brace.

Attach the load to the pane's outer view:

```swift
.task(id: requestID) {
    guard let fileURL else { highlights = nil; return }
    highlights = await DiffHighlightCache.shared.highlights(
        path: fileURL,
        oldText: diff.oldText,
        newText: diff.newText)
}
```

- [ ] **Step 3: Render each diff line from the correct document side**

Select the old or new map deliberately and render through the shared adapter:

```swift
private func ranges(for line: GitDiffLine) -> [SyntaxHighlightRange]? {
    switch line.kind {
    case .deletion:
        guard let number = line.oldLineNumber,
              let map = highlights?.old else { return nil }
        return map.ranges(forLine: number)
    case .addition, .context:
        guard let number = line.newLineNumber,
              let map = highlights?.new else { return nil }
        return map.ranges(forLine: number)
    case .hunk, .metadata:
        return nil
    }
}

@ViewBuilder
private func codeText(for line: GitDiffLine) -> some View {
    if let ranges = ranges(for: line) {
        Text(AttributedCodeRenderer.renderLine(
            line.text,
            ranges: ranges,
            theme: .tiller(isDark: colorScheme == .dark),
            font: .monospacedSystemFont(ofSize: 11, weight: .regular)))
    } else {
        Text(line.text)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(foreground(for: line))
    }
}
```

Change the existing `ForEach(lines)` to `ForEach(diff.lines)`. Use `codeText(for:)` in the line `HStack` after the old/new number columns, replacing the current `Text(line.text)`. Preserve hunk headers, text selection, and addition/deletion backgrounds. Do not apply a global red/green foreground when syntax ranges are available; background and gutter remain the diff signal. The fallback path retains the current monochrome/red/green foreground behavior while highlighting is loading or unavailable.

- [ ] **Step 4: Run App tests and build**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/AttributedCodeRendererTests
xcodebuild build -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS'
```

Expected: tests/build PASS; unsupported/large files compile through the monochrome path.

- [ ] **Step 5: Commit**

```bash
git add App/RightPanel
git commit -m "feat: syntax-highlight working tree diffs"
```

---

### Task 7: Show the complete syntax-highlighted tool-call diff

**Files:**

- Create: `App/Chat/ChatDiffPreviewView.swift`
- Create: `AppTests/ChatDiffPreviewTests.swift`
- Modify: `App/Chat/ToolCallCardView.swift:69-164`

**Interfaces:**

- Consumes: `DiffHighlightCache`, `AttributedCodeRenderer`, `AppModel.openFileReference`.
- Produces:
  - `ChatDiffPreviewModel.rows(oldText:newText:)` returning every line (no 40-line cap).
  - `ChatDiffPreviewView(path:oldText:newText:worktree:appModel:)`.

- [ ] **Step 1: Write the no-truncation test**

```swift
import Testing
@testable import Tiller

struct ChatDiffPreviewTests {
    @Test func keepsEveryOldAndNewLine() {
        let old = (1...55).map { "old \($0)" }.joined(separator: "\n")
        let new = (1...60).map { "new \($0)" }.joined(separator: "\n")
        let rows = ChatDiffPreviewModel.rows(oldText: old, newText: new)
        #expect(rows.filter { $0.side == .old }.count == 55)
        #expect(rows.filter { $0.side == .new }.count == 60)
        #expect(rows.count == 115)
    }
}
```

- [ ] **Step 2: Run test to verify missing model**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/ChatDiffPreviewTests
```

Expected: FAIL because `ChatDiffPreviewModel` does not exist.

- [ ] **Step 3: Implement the full row model**

```swift
struct ChatDiffPreviewRow: Identifiable, Equatable {
    enum Side: Equatable { case old, new }
    let id: Int
    let side: Side
    let lineNumber: Int
    let text: String
}

enum ChatDiffPreviewModel {
    static func rows(oldText: String?, newText: String) -> [ChatDiffPreviewRow] {
        let old = split(oldText).enumerated().map {
            ChatDiffPreviewRow(id: $0.offset, side: .old,
                               lineNumber: $0.offset + 1, text: $0.element)
        }
        let new = split(newText).enumerated().map {
            ChatDiffPreviewRow(id: old.count + $0.offset, side: .new,
                               lineNumber: $0.offset + 1, text: $0.element)
        }
        return old + new
    }

    private static func split(_ value: String?) -> [String] {
        guard let value, !value.isEmpty else { return [] }
        var lines = value.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        if lines.last?.isEmpty == true { lines.removeLast() }
        return lines
    }
}
```

- [ ] **Step 4: Implement ChatDiffPreviewView**

Implement the complete view without a preview limit:

```swift
import AppKit
import SwiftUI
import TillerACP
import TillerCode
import TillerCore

struct ChatDiffPreviewView: View {
    let path: String
    let oldText: String?
    let newText: String
    let worktree: Worktree
    let appModel: AppModel

    @Environment(\.colorScheme) private var colorScheme
    @State private var highlights: DiffHighlights?

    private var fileURL: URL {
        FileLink.resolve(path, worktreePath: worktree.path)
            ?? URL(fileURLWithPath: worktree.path).appendingPathComponent(path)
    }

    private var requestID: String {
        "\(fileURL.standardizedFileURL.path)|\(oldText?.hashValue ?? 0)|\(newText.hashValue)"
    }

    var body: some View {
        let stats = DiffStats.counts(oldText: oldText, newText: newText)
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 6) {
                Button {
                    appModel.openFileReference(path, in: worktree)
                } label: {
                    Label((path as NSString).lastPathComponent,
                          systemImage: "chevron.left.forwardslash.chevron.right")
                }
                .buttonStyle(.plain)
                .font(.caption2.weight(.semibold))
                Text("+\(stats.added)")
                    .font(.caption2).foregroundStyle(AppTheme.diffAddition)
                Text("−\(stats.removed)")
                    .font(.caption2).foregroundStyle(AppTheme.diffDeletion)
            }
            ForEach(ChatDiffPreviewModel.rows(oldText: oldText, newText: newText)) {
                row($0)
            }
        }
        .textSelection(.enabled)
        .task(id: requestID) {
            highlights = await DiffHighlightCache.shared.highlights(
                path: fileURL, oldText: oldText, newText: newText)
        }
    }

    private func row(_ row: ChatDiffPreviewRow) -> some View {
        HStack(alignment: .top, spacing: 0) {
            Text(row.side == .old ? "- " : "+ ")
                .foregroundStyle(row.side == .old
                    ? AppTheme.diffDeletion : AppTheme.diffAddition)
            highlightedText(for: row)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(row.side == .old
            ? AppTheme.diffDeletionBackground : AppTheme.diffAdditionBackground)
    }

    @ViewBuilder
    private func highlightedText(for row: ChatDiffPreviewRow) -> some View {
        let map = row.side == .old ? highlights?.old : highlights?.new
        if let map {
            Text(AttributedCodeRenderer.renderLine(
                row.text,
                ranges: map.ranges(forLine: row.lineNumber),
                theme: .tiller(isDark: colorScheme == .dark),
                font: .monospacedSystemFont(ofSize: 10, weight: .regular)))
        } else {
            Text(row.text)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(row.side == .old
                    ? AppTheme.diffDeletion : AppTheme.diffAddition)
        }
    }
}
```

Replace the `.diff` branch in `ToolCallCardView.contentBody`:

```swift
case .diff(let path, let oldText, let newText):
    ChatDiffPreviewView(
        path: path,
        oldText: oldText,
        newText: newText,
        worktree: worktree,
        appModel: appModel)
```

Then remove `previewLineLimit`, `diffView`, `lines`, and `diffLine`; no hidden-count or “more lines” branch remains.

- [ ] **Step 5: Run focused tests and build**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/ChatDiffPreviewTests
xcodebuild build -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS'
```

Expected: PASS; the old 40-line cap no longer exists.

- [ ] **Step 6: Commit**

```bash
git add App/Chat/ChatDiffPreviewView.swift App/Chat/ToolCallCardView.swift \
  AppTests/ChatDiffPreviewTests.swift
git commit -m "feat: show complete highlighted chat diffs"
```

---

### Task 8: Replace Highlightr in Markdown chat code blocks

**Files:**

- Modify: `App/Chat/CodeHighlighter.swift`
- Verify/modify: `App/Chat/MarkdownAttributedStringRenderer.swift:126-147`
- Modify: `AppTests/MarkdownAttributedStringRendererTests.swift:265-282`
- Modify: `project.yml:35-64`

**Interfaces:**

- Consumes: `CodeLanguageResolver.language(forFence:)`, `TreeSitterHighlighter.highlight`, `CodeHighlightTheme.tiller`.
- Produces: existing facade signature unchanged:
  - `CodeHighlighter.highlight(code:language:isDark:) -> NSAttributedString?`
- Removes: Highlightr dependency/import.

- [ ] **Step 1: Strengthen the renderer tests before changing the engine**

Keep `codeBlockIsHighlighted` and add a Swift/tree-sitter-specific assertion:

```swift
@MainActor
@Test func swiftCodeBlockHighlightsKeywordAndStringDifferently() throws {
    let rendered = MarkdownAttributedStringRenderer.render(
        "```swift\nlet value = \"hello\"\n```",
        appearance: .init(isDark: true, isStreaming: false))
    let source = rendered.string as NSString
    let keywordRange = source.range(of: "let")
    let stringRange = source.range(of: "\"hello\"")
    let keyword = try #require(rendered.attribute(
        .foregroundColor, at: keywordRange.location, effectiveRange: nil) as? NSColor)
    let string = try #require(rendered.attribute(
        .foregroundColor, at: stringRange.location, effectiveRange: nil) as? NSColor)
    #expect(keyword != string)
}
```

Retain `unknownLanguageStaysMonochrome` unchanged.

- [ ] **Step 2: Run tests against Highlightr as the baseline**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/MarkdownAttributedStringRendererTests
```

Expected: existing tests PASS; the new semantic assertion should also pass, establishing behavior parity.

- [ ] **Step 3: Replace CodeHighlighter internals but preserve cache/guards**

```swift
import AppKit
import TillerCode

@MainActor
final class CodeHighlighter {
    static let shared = CodeHighlighter()
    static let maxHighlightableLength = 20_000
    private static let maxCacheEntries = 200

    private struct Key: Hashable {
        let code: String
        let language: String
        let isDark: Bool
    }
    private var cache: [Key: NSAttributedString] = [:]

    func highlight(code: String, language: String, isDark: Bool) -> NSAttributedString? {
        guard code.count <= Self.maxHighlightableLength else { return nil }
        let key = Key(code: code, language: language, isDark: isDark)
        if let cached = cache[key] { return cached }
        let resolved = CodeLanguageResolver.language(forFence: language)
        guard resolved.id != .plainText,
              let attributed = TreeSitterHighlighter.highlight(
                code: code,
                language: resolved,
                theme: .tiller(isDark: isDark),
                font: .monospacedSystemFont(
                    ofSize: MarkdownAttributedStringRenderer.codeSize,
                    weight: .regular)) else { return nil }
        if cache.count >= Self.maxCacheEntries { cache.removeAll(keepingCapacity: true) }
        cache[key] = attributed
        return attributed
    }
}
```

`MarkdownAttributedStringRenderer.applyHighlighting` remains unchanged unless formatting requires only a naming/import cleanup; it continues copying `.foregroundColor` from the facade result.

- [ ] **Step 4: Run renderer tests before removing the package**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/MarkdownAttributedStringRendererTests
```

Expected: PASS with CodeHighlighter no longer importing Highlightr.

- [ ] **Step 5: Remove Highlightr and regenerate**

Delete from `project.yml`:

```yaml
  Highlightr:
    url: https://github.com/raspu/Highlightr
    from: 2.2.0
```

and its target dependency:

```yaml
      - package: Highlightr
```

Run:

```bash
xcodegen generate
xcodebuild build -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS'
```

Expected: build PASS with no Highlightr product or import.

- [ ] **Step 6: Commit**

```bash
git add App/Chat/CodeHighlighter.swift App/Chat/MarkdownAttributedStringRenderer.swift \
  AppTests/MarkdownAttributedStringRendererTests.swift project.yml
git commit -m "refactor: replace Highlightr with tree-sitter"
```

---

### Task 9: Run the repository-wide completion gate

**Files:**

- Verify: all files above
- Modify only if diagnostics or CI expose a concrete defect in the preceding tasks.

**Interfaces:**

- Consumes: the complete M0–M3 implementation.
- Produces: repository-wide evidence that the feature is shippable.

- [ ] **Step 1: Regenerate and inspect the dependency resolution**

Run:

```bash
xcodegen generate
git status --short
```

Expected: `Tiller.xcodeproj` remains ignored; no unintended generated files are staged; Highlightr is absent from `project.yml`.

- [ ] **Step 2: Run package suites**

```bash
swift test --package-path Packages/TillerCore
swift test --package-path Packages/TillerCode
swift test --package-path Packages/TillerGit
```

Expected: all PASS.

- [ ] **Step 3: Run the single repository gate**

```bash
Scripts/ci.sh
```

Expected final line: `CI OK`.

- [ ] **Step 4: Manually smoke-test the four entry points**

Open the generated Xcode project and run Tiller. Verify:

1. Git-status row click still opens the diff; its editor icon opens a tab.
2. GitDiff toolbar editor icon opens/reuses the selected file tab.
3. Edit-summary row click still opens the diff; its editor icon opens a tab.
4. Terminal cmd+click on `Sources/App.swift:42` opens/reuses a code tab.
5. `.md` still opens Markdown preview/code mode.
6. `⌘S`, dirty dot, dirty-close alert, external-change conflict, and deleted-file recreation work for code tabs.
7. Right-panel and tool-call diffs show syntax colors, all tool-call diff lines, and unchanged red/green backgrounds.
8. Chat fenced code blocks remain highlighted; an unknown fence remains monochrome.

- [ ] **Step 5: Record final evidence**

```bash
git status --short
git log --oneline -8
```

Expected: only intentional changes remain (normally a clean tree after task commits), and the task commits are visible in order. Do not create an empty final commit.
