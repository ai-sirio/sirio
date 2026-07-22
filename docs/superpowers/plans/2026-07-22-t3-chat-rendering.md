# T3-Style Chat Text Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ACP chat assistant messages render like T3 Code chat — airy typography, real list bullets, inline-code chips, code-block cards with a language/copy header bar, and Highlightr syntax highlighting — while keeping the single-`NSTextView` continuous-selection architecture.

**Architecture:** All rendering stays in one `NSTextView` per message. `MarkdownAttributedStringRenderer` gains custom `NSAttributedString` attributes marking inline-code and code-block ranges; a new `CodeBlockLayoutManager` (TextKit 1) reads those attributes at draw time to paint rounded cards and chips; header bars (language label + copy button) are `NSHostingView` subviews positioned from the same ranges; `CodeHighlighter` wraps Highlightr with a cache.

**Tech Stack:** Swift 6, AppKit TextKit 1, SwiftUI (`NSHostingView`), Highlightr (new SPM dep), swift-testing.

**Spec:** `docs/superpowers/specs/2026-07-22-t3-chat-rendering-design.md`

## Global Constraints

- Swift 6, macOS 15+, strict concurrency — new shared-state classes must be `@MainActor`.
- Tests first, swift-testing (`@Test` / `#expect`), never XCTest.
- AppTests are **hosted** (run inside the app). Never touch `UserDefaults.standard` in AppTests.
- AppTests run via: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/<SuiteName> 2>&1 | tail -20` (expect `** TEST SUCCEEDED **`). `Scripts/ci.sh` does NOT run AppTests — run them explicitly.
- After editing `project.yml`, run `xcodegen generate` before building.
- `Scripts/ci.sh` must print `CI OK` before the work is considered done.
- Conventional Commits, lower-case imperative subject.
- All UI strings in English.
- TextKit 1 only — never let the text view fall to TextKit 2 (custom `NSLayoutManager` requires TextKit 1).

## File Structure

- `App/Chat/CodeBlockStyle.swift` (create) — attribute keys, metrics, dynamic colors shared by renderer, layout manager, and header views.
- `App/Chat/MarkdownAttributedStringRenderer.swift` (modify) — typography, bullets, custom attributes, code-block paragraph metrics, highlight application.
- `App/Chat/CodeHighlighter.swift` (create) — Highlightr wrapper + cache.
- `App/Chat/CodeBlockLayoutManager.swift` (create) — card + chip drawing.
- `App/Chat/CodeBlockHeaderView.swift` (create) — SwiftUI header (language + copy).
- `App/Chat/AgentMarkdownTextView.swift` (modify) — explicit TextKit 1 stack, `MarkdownTextView` subclass with header subviews and appearance handling.
- `project.yml` (modify) — Highlightr package.
- `AppTests/MarkdownAttributedStringRendererTests.swift` (modify/extend).
- `AppTests/CodeHighlighterTests.swift` (create).

---

### Task 1: Typography rhythm + CodeBlockStyle scaffold

**Files:**
- Create: `App/Chat/CodeBlockStyle.swift`
- Modify: `App/Chat/MarkdownAttributedStringRenderer.swift`
- Test: `AppTests/MarkdownAttributedStringRendererTests.swift`

**Interfaces:**
- Produces: `CodeBlockStyle` enum with `inlineCodeAttribute`, `codeBlockAttribute` (`NSAttributedString.Key`), `headerHeight: CGFloat = 28`, `cardPadding: CGFloat = 12`, `cardCornerRadius: CGFloat = 8`, `chipCornerRadius: CGFloat = 4`, `cardFill/cardBorder/chipFill: NSColor`; `CodeBlockInfo` struct `{ language: String, index: Int }`. Renderer body `lineSpacing = 4.5`, `paragraphSpacing = 9`.

- [ ] **Step 1: Write/adjust failing tests**

In `AppTests/MarkdownAttributedStringRendererTests.swift`, add:

```swift
@Test("body paragraphs use the T3 line and paragraph rhythm")
func bodyRhythm() {
    let result = MarkdownAttributedStringRenderer.render("Hello world")
    let style = result.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle
    #expect(style?.lineSpacing == 4.5)
    #expect(style?.paragraphSpacing == 9)
}
```

If an existing test asserts the old `paragraphSpacing == 6` or `lineSpacing == 3` (check `consecutiveParagraphsAreSeparated`), update its expected values to 9 / 4.5.

- [ ] **Step 2: Run tests to verify failure**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/MarkdownAttributedStringRendererTests 2>&1 | tail -20`
Expected: FAIL on `bodyRhythm`.

- [ ] **Step 3: Implement**

Create `App/Chat/CodeBlockStyle.swift`:

```swift
import AppKit

/// Shared metrics, attribute keys and colors for T3-style code rendering.
/// Custom attributes travel with the attributed string: the renderer writes
/// them, `CodeBlockLayoutManager` draws from them, and `MarkdownTextView`
/// positions header overlays from the same ranges.
enum CodeBlockStyle {
    /// Marks inline `code` runs. Value: `true`.
    static let inlineCodeAttribute = NSAttributedString.Key("tiller.inlineCode")
    /// Marks fenced code-block ranges. Value: `CodeBlockInfo`.
    static let codeBlockAttribute = NSAttributedString.Key("tiller.codeBlock")

    static let headerHeight: CGFloat = 28
    static let cardPadding: CGFloat = 12
    static let cardCornerRadius: CGFloat = 8
    static let chipCornerRadius: CGFloat = 4

    static let cardFill = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor.black.withAlphaComponent(0.25)
            : NSColor.black.withAlphaComponent(0.04)
    }
    static let cardBorder = NSColor.separatorColor
    static let chipFill = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor.white.withAlphaComponent(0.09)
            : NSColor.black.withAlphaComponent(0.06)
    }
}

/// Metadata attached to a code block's range via `codeBlockAttribute`.
/// `index` keeps adjacent same-language blocks from merging into one
/// attribute range and identifies the block's header overlay.
struct CodeBlockInfo: Equatable {
    let language: String
    let index: Int
}
```

In `MarkdownAttributedStringRenderer.attributes(for:)` change:
- `paragraphStyle.lineSpacing = 3` → `paragraphStyle.lineSpacing = 4.5`
- `case .paragraph: paragraphStyle.paragraphSpacing = 6` → `= 9`
- `case .codeBlock:` / `case .blockQuote:` keep `paragraphSpacing = 6` for now (code blocks get their own metrics in Task 4).

- [ ] **Step 4: Run tests to verify pass**

Same command. Expected: PASS (whole suite).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/CodeBlockStyle.swift App/Chat/MarkdownAttributedStringRenderer.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: t3-style typography rhythm and code style scaffold in chat renderer"
```

---

### Task 2: Real list bullets

**Files:**
- Modify: `App/Chat/MarkdownAttributedStringRenderer.swift`
- Test: `AppTests/MarkdownAttributedStringRendererTests.swift`

**Interfaces:**
- Consumes: nothing new.
- Produces: rendered list items begin with `•\t` (unordered) or `<ordinal>.\t` (ordered); nested items indent by 16pt per depth level.

**Background for the implementer:** `AttributedString(markdown:, interpretedSyntax: .full)` throws away the literal `-` / `1.` markers; list structure survives only as `presentationIntent` components (`.unorderedList` / `.orderedList` / `.listItem(ordinal:)`). The renderer must re-insert visible markers.

- [ ] **Step 1: Write failing tests**

```swift
@Test("unordered list items get a visible bullet")
func unorderedBullet() {
    let result = MarkdownAttributedStringRenderer.render("- one\n- two")
    #expect(result.string.contains("•\tone"))
    #expect(result.string.contains("•\ttwo"))
}

@Test("ordered list items get their ordinal")
func orderedMarker() {
    let result = MarkdownAttributedStringRenderer.render("1. first\n2. second")
    #expect(result.string.contains("1.\tfirst"))
    #expect(result.string.contains("2.\tsecond"))
}

@Test("nested list items indent one level deeper")
func nestedListIndent() {
    let result = MarkdownAttributedStringRenderer.render("- outer\n    - inner")
    let range = (result.string as NSString).range(of: "inner")
    let style = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle
    #expect(style?.headIndent == 32)
}
```

Existing tests `unorderedListItem`, `listItemsAreSeparated`, `listItemsHaveNoExtraSpacing` assert on strings like `"one"` — update their expectations to include the `•\t` prefix where they compare `result.string`.

- [ ] **Step 2: Run tests to verify failure**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/MarkdownAttributedStringRendererTests 2>&1 | tail -20`
Expected: FAIL on the three new tests.

- [ ] **Step 3: Implement**

In `render(_ parsed: AttributedString)`, prepend the marker when a new block that is a list item begins:

```swift
private static func render(_ parsed: AttributedString) -> NSAttributedString {
    let result = NSMutableAttributedString()
    var previousBlockIdentity: Int?
    for run in parsed.runs {
        var substring = String(parsed[run.range].characters)
        let blockIdentity = run.presentationIntent?.components.first?.identity
        let isNewBlock = blockIdentity != previousBlockIdentity
        if previousBlockIdentity != nil, isNewBlock {
            substring = "\n" + substring
        }
        if isNewBlock, let marker = listMarker(for: run.presentationIntent) {
            substring = previousBlockIdentity == nil ? marker + substring
                : substring.replacingOccurrences(of: "\n", with: "\n" + marker,
                                                 range: substring.range(of: "\n"))
        }
        if let blockIdentity { previousBlockIdentity = blockIdentity }
        result.append(NSAttributedString(string: substring, attributes: attributes(for: run)))
    }
    return result
}

/// A list item's marker ("•\t" or "3.\t"), or nil for non-list blocks.
/// Markdown markers are stripped by `AttributedString(markdown:)`; only the
/// presentation intent knows an item's ordinal and whether its list is ordered.
private static func listMarker(for intent: PresentationIntent?) -> String? {
    guard let components = intent?.components else { return nil }
    var ordinal: Int?
    var isOrdered = false
    for component in components {
        if case .listItem(let n) = component.kind { ordinal = ordinal ?? n }
        if case .orderedList = component.kind { isOrdered = true }
    }
    guard let ordinal else { return nil }
    return isOrdered ? "\(ordinal).\t" : "•\t"
}
```

In `attributes(for:)`, replace the `.listItem` case body with depth-aware indent (count `listItem` components for depth) and a tab stop:

```swift
case .listItem:
    paragraphStyle.paragraphSpacing = 0
    let depth = intent.components.filter {
        if case .listItem = $0.kind { return true } else { return false }
    }.count
    let indent = CGFloat(depth) * 16
    paragraphStyle.firstLineHeadIndent = indent - 16
    paragraphStyle.headIndent = indent
    paragraphStyle.tabStops = [NSTextTab(textAlignment: .left, location: indent)]
```

(`.listItem` appears once per nesting level in the components stack, so `depth` is 1 for a top-level item, 2 for its child.)

- [ ] **Step 4: Run tests to verify pass**

Same command. Expected: PASS (whole suite, including the updated legacy list tests).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/MarkdownAttributedStringRenderer.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: re-insert visible list bullets and ordinals in chat markdown"
```

---

### Task 3: Inline-code chip attributes

**Files:**
- Modify: `App/Chat/MarkdownAttributedStringRenderer.swift`
- Test: `AppTests/MarkdownAttributedStringRendererTests.swift`

**Interfaces:**
- Consumes: `CodeBlockStyle.inlineCodeAttribute`, `CodeBlockStyle.chipFill` (Task 1).
- Produces: inline-code runs carry `.backgroundColor = CodeBlockStyle.chipFill`, `CodeBlockStyle.inlineCodeAttribute = true`, foreground `.labelColor.withAlphaComponent(0.9)`, 12pt mono font. Teal is gone from inline code.

- [ ] **Step 1: Write failing tests**

```swift
@Test("inline code is marked as a chip with a background fill")
func inlineCodeChip() {
    let result = MarkdownAttributedStringRenderer.render("`code`")
    #expect(result.attribute(CodeBlockStyle.inlineCodeAttribute, at: 0, effectiveRange: nil) != nil)
    let bg = result.attribute(.backgroundColor, at: 0, effectiveRange: nil) as? NSColor
    #expect(bg == CodeBlockStyle.chipFill)
}
```

Update the existing `inlineCodeIsColored` test: it expects the teal `codeColor`. New body:

```swift
@Test("inline code uses near-label foreground, not teal")
func inlineCodeIsColored() {
    let result = MarkdownAttributedStringRenderer.render("`code`")
    let color = result.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
    #expect(color == NSColor.labelColor.withAlphaComponent(0.9))
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/MarkdownAttributedStringRendererTests 2>&1 | tail -20`
Expected: FAIL on both.

- [ ] **Step 3: Implement**

In `attributes(for:)`, in the `inline.contains(.code)` branch:

```swift
if inline.contains(.code) {
    font = NSFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
    color = .labelColor.withAlphaComponent(0.9)
    attrs[CodeBlockStyle.inlineCodeAttribute] = true
    attrs[.backgroundColor] = CodeBlockStyle.chipFill
}
```

Do NOT remove the `codeColor` static yet — code blocks still use it until Task 4.

- [ ] **Step 4: Run tests to verify pass**

Same command. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/MarkdownAttributedStringRenderer.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: mark inline code as chip with background fill in chat renderer"
```

---

### Task 4: Code-block attribute, language capture, card paragraph metrics

**Files:**
- Modify: `App/Chat/MarkdownAttributedStringRenderer.swift`
- Test: `AppTests/MarkdownAttributedStringRendererTests.swift`

**Interfaces:**
- Consumes: `CodeBlockStyle.codeBlockAttribute`, `CodeBlockInfo`, `CodeBlockStyle.headerHeight/cardPadding` (Task 1).
- Produces: every fenced block's range carries `CodeBlockInfo(language:index:)` (language from the fence info string, fallback `"text"`; index increments per block). The separator `\n` before a block does NOT carry the attribute. First code line has `paragraphSpacingBefore == CodeBlockStyle.headerHeight + 10`; interior lines 0; all code lines `headIndent == firstLineHeadIndent == CodeBlockStyle.cardPadding`; last line `paragraphSpacing == CodeBlockStyle.cardPadding + 8`. Code foreground becomes `.labelColor.withAlphaComponent(0.85)` (teal removed).

- [ ] **Step 1: Write failing tests**

```swift
@Test("code block carries language metadata from the fence")
func codeBlockLanguage() {
    let result = MarkdownAttributedStringRenderer.render("```swift\nlet a = 1\n```")
    let range = (result.string as NSString).range(of: "let a = 1")
    let info = result.attribute(CodeBlockStyle.codeBlockAttribute, at: range.location, effectiveRange: nil) as? CodeBlockInfo
    #expect(info?.language == "swift")
    #expect(info?.index == 0)
}

@Test("code block without a language falls back to text")
func codeBlockLanguageFallback() {
    let result = MarkdownAttributedStringRenderer.render("```\nplain\n```")
    let range = (result.string as NSString).range(of: "plain")
    let info = result.attribute(CodeBlockStyle.codeBlockAttribute, at: range.location, effectiveRange: nil) as? CodeBlockInfo
    #expect(info?.language == "text")
}

@Test("separator newline before a code block does not join the card")
func separatorNewlineExcluded() {
    let result = MarkdownAttributedStringRenderer.render("para\n\n```\ncode\n```")
    let newlineLocation = (result.string as NSString).range(of: "\n", options: []).location
    // find the newline directly before "code"
    let codeLocation = (result.string as NSString).range(of: "code").location
    #expect(result.attribute(CodeBlockStyle.codeBlockAttribute, at: codeLocation - 1, effectiveRange: nil) == nil)
    _ = newlineLocation
}

@Test("code block first line reserves header space, interior lines do not")
func codeBlockHeaderSpace() {
    let result = MarkdownAttributedStringRenderer.render("```swift\nlet a = 1\nlet b = 2\n```")
    let first = (result.string as NSString).range(of: "let a = 1").location
    let second = (result.string as NSString).range(of: "let b = 2").location
    let firstStyle = result.attribute(.paragraphStyle, at: first, effectiveRange: nil) as? NSParagraphStyle
    let secondStyle = result.attribute(.paragraphStyle, at: second, effectiveRange: nil) as? NSParagraphStyle
    #expect(firstStyle?.paragraphSpacingBefore == CodeBlockStyle.headerHeight + 10)
    #expect(secondStyle?.paragraphSpacingBefore == 0)
    #expect(firstStyle?.headIndent == CodeBlockStyle.cardPadding)
    #expect(secondStyle?.paragraphSpacing == CodeBlockStyle.cardPadding + 8)
}
```

Update the existing `codeBlockIsColored` test (expects teal):

```swift
@Test("code block text uses near-label monochrome by default")
func codeBlockIsColored() {
    let result = MarkdownAttributedStringRenderer.render("```\ncode\n```")
    let range = (result.string as NSString).range(of: "code")
    let color = result.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? NSColor
    #expect(color == NSColor.labelColor.withAlphaComponent(0.85))
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/MarkdownAttributedStringRendererTests 2>&1 | tail -20`
Expected: FAIL on the new/updated tests.

- [ ] **Step 3: Implement**

a) Track a block index in `render(_ parsed:)` and pass it to `attributes(for:blockIndex:)`. Increment when a run starting a new block contains a `.codeBlock` component:

```swift
var codeBlockIndex = -1
// inside the loop, where isNewBlock is computed:
let isCodeBlock = run.presentationIntent?.components.contains {
    if case .codeBlock = $0.kind { return true } else { return false }
} ?? false
if isNewBlock, isCodeBlock { codeBlockIndex += 1 }
result.append(NSAttributedString(string: substring,
    attributes: attributes(for: run, blockIndex: codeBlockIndex)))
```

b) In `attributes(for:blockIndex:)` replace the `.codeBlock` case:

```swift
case .codeBlock(let languageHint):
    font = NSFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
    color = .labelColor.withAlphaComponent(0.85)
    attrs[CodeBlockStyle.codeBlockAttribute] =
        CodeBlockInfo(language: languageHint ?? "text", index: blockIndex)
```

Delete the `codeColor` static (nothing references it after this task — the compiler will confirm).

c) The separator `\n` prepended to a block's first run must not carry chip/card keys. In the render loop, append the separator as its own string with sanitized attributes instead of gluing it to `substring`:

```swift
if previousBlockIdentity != nil, isNewBlock {
    var sep = attributes(for: run, blockIndex: codeBlockIndex)
    sep.removeValue(forKey: .backgroundColor)
    sep.removeValue(forKey: CodeBlockStyle.inlineCodeAttribute)
    sep.removeValue(forKey: CodeBlockStyle.codeBlockAttribute)
    result.append(NSAttributedString(string: "\n", attributes: sep))
    substring = /* no longer prepend "\n" here */ substring
}
```

(Adjust the Task 2 marker logic accordingly: with the separator appended separately, the marker is simply prepended to `substring` for new list-item blocks.)

d) After the loop, apply per-line card metrics and return:

```swift
private static func applyCodeBlockLayoutMetrics(_ result: NSMutableAttributedString) {
    let text = result.string as NSString
    result.enumerateAttribute(CodeBlockStyle.codeBlockAttribute,
                              in: NSRange(location: 0, length: result.length)) { value, range, _ in
        guard value is CodeBlockInfo else { return }
        var lineStart = range.location
        var isFirst = true
        while lineStart < NSMaxRange(range) {
            let lineRange = text.lineRange(for: NSRange(location: lineStart, length: 0))
            let clipped = NSIntersectionRange(lineRange, range)
            let style = NSMutableParagraphStyle()
            style.firstLineHeadIndent = CodeBlockStyle.cardPadding
            style.headIndent = CodeBlockStyle.cardPadding
            style.lineSpacing = 2
            if isFirst {
                style.paragraphSpacingBefore = CodeBlockStyle.headerHeight + 10
                isFirst = false
            }
            if NSMaxRange(lineRange) >= NSMaxRange(range) {
                style.paragraphSpacing = CodeBlockStyle.cardPadding + 8
            }
            result.addAttribute(.paragraphStyle, value: style, range: clipped)
            lineStart = NSMaxRange(lineRange)
        }
    }
}
```

Call it as the last step of `render(_ parsed:)` before returning.

- [ ] **Step 4: Run tests to verify pass**

Same command. Expected: PASS (whole suite — including `unclosedFenceFallsBackToPlainText`, which must still pass unchanged).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/MarkdownAttributedStringRenderer.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: code block card metadata and paragraph metrics in chat renderer"
```

---

### Task 5: Highlightr dependency + CodeHighlighter

**Files:**
- Modify: `project.yml` (packages + Tiller target dependencies)
- Create: `App/Chat/CodeHighlighter.swift`
- Test: `AppTests/CodeHighlighterTests.swift` (create)

**Interfaces:**
- Produces: `@MainActor final class CodeHighlighter` with `static let shared`, `static let maxHighlightableLength = 20_000`, and `func highlight(code: String, language: String, isDark: Bool) -> NSAttributedString?`. Returns `nil` for oversized code, unknown languages, or Highlightr init failure — callers keep monochrome. Same-key calls return the identical cached instance.

- [ ] **Step 1: Add the dependency**

In `project.yml` under `packages:` add:

```yaml
  Highlightr:
    url: https://github.com/raspu/Highlightr
    from: 2.2.0
```

Under `targets: Tiller: dependencies:` add:

```yaml
      - package: Highlightr
```

Run: `xcodegen generate` — expect no errors.

- [ ] **Step 2: Write failing tests**

Create `AppTests/CodeHighlighterTests.swift`:

```swift
import AppKit
import Testing
@testable import Tiller

@MainActor
struct CodeHighlighterTests {
    @Test("swift code gets more than one foreground color")
    func highlightsSwift() {
        let result = CodeHighlighter.shared.highlight(
            code: "let x = \"hi\"", language: "swift", isDark: true)
        #expect(result != nil)
        var colors = Set<NSColor>()
        result?.enumerateAttribute(.foregroundColor,
                                   in: NSRange(location: 0, length: result?.length ?? 0)) { value, _, _ in
            if let c = value as? NSColor { colors.insert(c) }
        }
        #expect(colors.count > 1)  // keyword vs string literal
    }

    @Test("identical requests hit the cache")
    func cacheHit() {
        let a = CodeHighlighter.shared.highlight(code: "let y = 2", language: "swift", isDark: true)
        let b = CodeHighlighter.shared.highlight(code: "let y = 2", language: "swift", isDark: true)
        #expect(a != nil)
        #expect(a === b)
    }

    @Test("oversized code is not highlighted")
    func oversizedGuard() {
        let big = String(repeating: "a", count: CodeHighlighter.maxHighlightableLength + 1)
        #expect(CodeHighlighter.shared.highlight(code: big, language: "swift", isDark: true) == nil)
    }

    @Test("unknown language returns nil, never crashes")
    func unknownLanguage() {
        let result = CodeHighlighter.shared.highlight(
            code: "hello", language: "not-a-real-language-xyz", isDark: true)
        #expect(result == nil)
    }
}
```

- [ ] **Step 3: Run tests to verify failure**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/CodeHighlighterTests 2>&1 | tail -20`
Expected: FAIL (compile error, `CodeHighlighter` not defined).

- [ ] **Step 4: Implement**

Create `App/Chat/CodeHighlighter.swift`:

```swift
import AppKit
import Highlightr

/// Wraps Highlightr (highlight.js via JavaScriptCore) with a cache so
/// streaming re-renders don't re-highlight unchanged blocks. `nil` means
/// "keep the monochrome fallback" — this class never surfaces an error.
@MainActor
final class CodeHighlighter {
    static let shared = CodeHighlighter()
    /// Blocks beyond this stay monochrome until complete (streaming guard).
    static let maxHighlightableLength = 20_000
    private static let maxCacheEntries = 200

    private let highlightr: Highlightr?
    private var cache: [Key: NSAttributedString] = [:]
    private var currentThemeIsDark: Bool?

    private struct Key: Hashable {
        let code: String
        let language: String
        let isDark: Bool
    }

    init() {
        highlightr = Highlightr()
    }

    func highlight(code: String, language: String, isDark: Bool) -> NSAttributedString? {
        guard let highlightr, code.count <= Self.maxHighlightableLength else { return nil }
        let key = Key(code: code, language: language, isDark: isDark)
        if let cached = cache[key] { return cached }
        if currentThemeIsDark != isDark {
            highlightr.setTheme(to: isDark ? "atom-one-dark" : "atom-one-light")
            highlightr.theme.setCodeFont(
                .monospacedSystemFont(ofSize: MarkdownAttributedStringRenderer.codeSize,
                                      weight: .regular))
            currentThemeIsDark = isDark
        }
        guard let result = highlightr.highlight(code, as: language, fastRender: true) else {
            return nil
        }
        if cache.count >= Self.maxCacheEntries { cache.removeAll() }
        cache[key] = result
        return result
    }
}
```

Note: Highlightr's `highlight(_:as:)` returns `nil` for languages highlight.js doesn't know — that is the unknown-language fallback path.

- [ ] **Step 5: Run tests to verify pass**

Same command. Expected: PASS. If `unknownLanguage` fails because Highlightr auto-detected instead of returning nil, change the assertion to `#expect(result == nil || result != nil)` is NOT acceptable — instead pre-check with `highlightr.supportedLanguages()` inside `highlight(...)`:

```swift
if !highlightr.supportedLanguages().contains(language) { return nil }
```

- [ ] **Step 6: Commit**

```bash
git add project.yml App/Chat/CodeHighlighter.swift AppTests/CodeHighlighterTests.swift
git commit -m "feat: add Highlightr-backed code highlighter with cache"
```

---

### Task 6: Wire highlighting into the renderer

**Files:**
- Modify: `App/Chat/MarkdownAttributedStringRenderer.swift`
- Modify: `App/Chat/AgentMarkdownTextView.swift` (pass-through only)
- Test: `AppTests/MarkdownAttributedStringRendererTests.swift`

**Interfaces:**
- Consumes: `CodeHighlighter.shared.highlight(code:language:isDark:)` (Task 5), `CodeBlockStyle.codeBlockAttribute` (Task 4).
- Produces: `MarkdownAttributedStringRenderer.render(_ markdown: String, isDark: Bool = MarkdownAppearance.isDark)` — highlighted foreground colors overlaid on code-block ranges; fonts/paragraph styles/custom attributes untouched. `enum MarkdownAppearance { static var isDark: Bool }` in the renderer file. Renderer becomes `@MainActor`.

- [ ] **Step 1: Write failing test**

```swift
@Test("swift code block gets syntax-highlighted foreground colors")
func codeBlockIsHighlighted() {
    let result = MarkdownAttributedStringRenderer.render("```swift\nlet x = \"hi\"\n```", isDark: true)
    let keyword = (result.string as NSString).range(of: "let").location
    let literal = (result.string as NSString).range(of: "\"hi\"").location
    let keywordColor = result.attribute(.foregroundColor, at: keyword, effectiveRange: nil) as? NSColor
    let literalColor = result.attribute(.foregroundColor, at: literal, effectiveRange: nil) as? NSColor
    #expect(keywordColor != nil)
    #expect(keywordColor != literalColor)
}

@Test("unknown fence language keeps monochrome foreground")
func unknownLanguageStaysMonochrome() {
    let result = MarkdownAttributedStringRenderer.render("```zzznotalang\nplain\n```", isDark: true)
    let range = (result.string as NSString).range(of: "plain")
    let color = result.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? NSColor
    #expect(color == NSColor.labelColor.withAlphaComponent(0.85))
}
```

The whole test suite needs `@MainActor` on the struct once the renderer is `@MainActor` — add it.

- [ ] **Step 2: Run tests to verify failure**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/MarkdownAttributedStringRendererTests 2>&1 | tail -20`
Expected: FAIL (no `isDark:` parameter yet).

- [ ] **Step 3: Implement**

a) Mark `MarkdownAttributedStringRenderer` as `@MainActor` (it now calls `CodeHighlighter.shared`). `AgentMarkdownTextView.makeNSView/updateNSView` already run on the main actor, so call sites compile unchanged.

b) Add the appearance helper in the same file:

```swift
@MainActor
enum MarkdownAppearance {
    static var isDark: Bool {
        NSApp?.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }
}
```

c) Change the entry point and thread `isDark` through:

```swift
static func render(_ markdown: String, isDark: Bool = MarkdownAppearance.isDark) -> NSAttributedString {
    // ... existing parse ...
    return render(parsed, isDark: isDark)
}
```

d) At the end of `render(_ parsed:isDark:)`, after `applyCodeBlockLayoutMetrics`, apply colors:

```swift
private static func applyHighlighting(_ result: NSMutableAttributedString, isDark: Bool) {
    result.enumerateAttribute(CodeBlockStyle.codeBlockAttribute,
                              in: NSRange(location: 0, length: result.length)) { value, range, _ in
        guard let info = value as? CodeBlockInfo else { return }
        let code = (result.string as NSString).substring(with: range)
        guard let highlighted = CodeHighlighter.shared.highlight(
                  code: code, language: info.language, isDark: isDark),
              highlighted.length == range.length else { return }
        highlighted.enumerateAttribute(.foregroundColor,
                                       in: NSRange(location: 0, length: highlighted.length)) { color, sub, _ in
            guard let color = color as? NSColor else { return }
            result.addAttribute(.foregroundColor, value: color,
                                range: NSRange(location: range.location + sub.location,
                                               length: sub.length))
        }
    }
}
```

The `highlighted.length == range.length` guard skips blocks Highlightr normalized differently (never corrupts offsets).

- [ ] **Step 4: Run tests to verify pass**

Same command, plus `-only-testing:TillerTests/CodeHighlighterTests`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/MarkdownAttributedStringRenderer.swift App/Chat/AgentMarkdownTextView.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: apply syntax highlighting to chat code blocks"
```

---

### Task 7: CodeBlockLayoutManager — cards and chips

**Files:**
- Create: `App/Chat/CodeBlockLayoutManager.swift`
- Modify: `App/Chat/AgentMarkdownTextView.swift`
- Test: `AppTests/MarkdownAttributedStringRendererTests.swift` (one wiring test)

**Interfaces:**
- Consumes: `CodeBlockStyle` keys/metrics/colors (Task 1), attributes written in Tasks 3-4.
- Produces: `final class CodeBlockLayoutManager: NSLayoutManager`; `AgentMarkdownTextView.makeTextView() -> NSTextView` (static, also used by `makeNSView`) whose `layoutManager` is a `CodeBlockLayoutManager`. Task 8 subclasses the text view — `makeTextView` must instantiate `MarkdownTextView` (introduced there; for this task use `NSTextView` and switch in Task 8).

- [ ] **Step 1: Write failing wiring test**

```swift
@Test("agent markdown text view installs the code block layout manager")
@MainActor
func installsCustomLayoutManager() {
    let textView = AgentMarkdownTextView.makeTextView()
    #expect(textView.layoutManager is CodeBlockLayoutManager)
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/MarkdownAttributedStringRendererTests 2>&1 | tail -20`
Expected: FAIL (no `makeTextView`, no `CodeBlockLayoutManager`).

- [ ] **Step 3: Implement**

Create `App/Chat/CodeBlockLayoutManager.swift`:

```swift
import AppKit

/// TextKit 1 layout manager that paints the T3-style decorations the
/// renderer only *marks*: rounded cards behind code-block ranges (including
/// the header zone reserved via `paragraphSpacingBefore`) and rounded chips
/// behind inline code. Reading the custom attributes at draw time keeps the
/// attributed string the single source of truth.
final class CodeBlockLayoutManager: NSLayoutManager {

    override func drawBackground(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        drawCodeBlockCards(at: origin)
        super.drawBackground(forGlyphRange: glyphsToShow, at: origin)
    }

    /// Inline-code chips: `.backgroundColor` set by the renderer routes
    /// here; rounded corners instead of the square default.
    override func fillBackgroundRectArray(_ rectArray: UnsafePointer<NSRect>,
                                          count: Int,
                                          forCharacterRange charRange: NSRange,
                                          color: NSColor) {
        let isChip = textStorage?.attribute(CodeBlockStyle.inlineCodeAttribute,
                                            at: charRange.location,
                                            effectiveRange: nil) != nil
        guard isChip else {
            super.fillBackgroundRectArray(rectArray, count: count,
                                          forCharacterRange: charRange, color: color)
            return
        }
        color.setFill()
        for i in 0..<count {
            let rect = rectArray[i].insetBy(dx: -2, dy: -0.5)
            NSBezierPath(roundedRect: rect,
                         xRadius: CodeBlockStyle.chipCornerRadius,
                         yRadius: CodeBlockStyle.chipCornerRadius).fill()
        }
    }

    private func drawCodeBlockCards(at origin: NSPoint) {
        guard let storage = textStorage, let container = textContainers.first else { return }
        storage.enumerateAttribute(CodeBlockStyle.codeBlockAttribute,
                                   in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard value is CodeBlockInfo else { return }
            let glyphRange = self.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            var rect = self.boundingRect(forGlyphRange: glyphRange, in: container)
            rect.origin.x = 0
            rect.size.width = container.size.width
            rect.origin.y -= CodeBlockStyle.headerHeight + 6
            rect.size.height += CodeBlockStyle.headerHeight + 6 + 8
            rect = rect.offsetBy(dx: origin.x, dy: origin.y)
            let path = NSBezierPath(roundedRect: rect,
                                    xRadius: CodeBlockStyle.cardCornerRadius,
                                    yRadius: CodeBlockStyle.cardCornerRadius)
            CodeBlockStyle.cardFill.setFill()
            path.fill()
            path.lineWidth = 1
            CodeBlockStyle.cardBorder.setStroke()
            path.stroke()
        }
    }
}
```

In `AgentMarkdownTextView`, extract view construction into a static factory that builds an explicit TextKit 1 stack (an `NSTextView()` default init would use TextKit 2 and drop the custom manager):

```swift
@MainActor
static func makeTextView() -> NSTextView {
    let storage = NSTextStorage()
    let layoutManager = CodeBlockLayoutManager()
    storage.addLayoutManager(layoutManager)
    let container = NSTextContainer(size: CGSize(width: 0, height: .greatestFiniteMagnitude))
    container.widthTracksTextView = false
    container.lineFragmentPadding = 0
    layoutManager.addTextContainer(container)
    let textView = NSTextView(frame: .zero, textContainer: container)
    textView.isEditable = false
    textView.isSelectable = true
    textView.drawsBackground = false
    textView.textContainerInset = .zero
    textView.isVerticallyResizable = true
    textView.isHorizontallyResizable = false
    textView.autoresizingMask = [.width]
    return textView
}
```

`makeNSView` becomes: `let textView = Self.makeTextView()` + the existing `setAttributedString`/coordinator lines.

- [ ] **Step 4: Run tests to verify pass**

Same command. Expected: PASS (whole `MarkdownAttributedStringRendererTests` + `CodeHighlighterTests`).

- [ ] **Step 5: Visual smoke check**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -5`
Expected: `** BUILD SUCCEEDED **`. (Full visual QA happens in Task 9.)

- [ ] **Step 6: Commit**

```bash
git add App/Chat/CodeBlockLayoutManager.swift App/Chat/AgentMarkdownTextView.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: draw code block cards and inline chips via custom layout manager"
```

---

### Task 8: Header bar — language label + copy button

**Files:**
- Create: `App/Chat/CodeBlockHeaderView.swift`
- Modify: `App/Chat/AgentMarkdownTextView.swift`

**Interfaces:**
- Consumes: `CodeBlockStyle.headerHeight`, `codeBlockAttribute`, `CodeBlockInfo` (Task 1/4), `makeTextView()` (Task 7).
- Produces: `struct CodeBlockHeaderView: View { let language: String; let code: String }`; `final class MarkdownTextView: NSTextView` with `func rebuildCodeBlockHeaders()` and `var onAppearanceChanged: (() -> Void)?`. `makeTextView()` now returns a `MarkdownTextView` (still typed `NSTextView` externally is fine, but Task 9 needs `MarkdownTextView` — return `MarkdownTextView`).

No unit test — header positioning is draw-time geometry, covered by the Task 9 manual QA checklist. (The Task 7 wiring test keeps protecting the stack.)

- [ ] **Step 1: Create the SwiftUI header**

Create `App/Chat/CodeBlockHeaderView.swift`:

```swift
import AppKit
import SwiftUI

/// The non-selectable header bar hosted above each code-block card:
/// language label on the left, copy button on the right. Overlaid as an
/// `NSHostingView` subview of `MarkdownTextView` — it is not part of the
/// text, so drag-selection flows straight through the code block (same as
/// T3 web).
struct CodeBlockHeaderView: View {
    let language: String
    let code: String
    @State private var copied = false

    var body: some View {
        HStack {
            Text(language)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            Spacer()
            Button {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(code, forType: .string)
                copied = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
            } label: {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 11))
                    .foregroundStyle(copied ? .green : .secondary)
            }
            .buttonStyle(.plain)
            .help("Copy code")
        }
        .padding(.horizontal, 10)
        .frame(height: CodeBlockStyle.headerHeight)
    }
}
```

- [ ] **Step 2: Add the MarkdownTextView subclass**

In `App/Chat/AgentMarkdownTextView.swift` add:

```swift
/// NSTextView subclass owning the code-block header overlays. Headers are
/// rebuilt whenever the attributed text changes and repositioned on every
/// layout pass from the same attribute ranges the layout manager draws from.
final class MarkdownTextView: NSTextView {
    private var headerViews: [NSHostingView<CodeBlockHeaderView>] = []
    var onAppearanceChanged: (() -> Void)?

    func rebuildCodeBlockHeaders() {
        headerViews.forEach { $0.removeFromSuperview() }
        headerViews.removeAll()
        guard let storage = textStorage else { return }
        storage.enumerateAttribute(CodeBlockStyle.codeBlockAttribute,
                                   in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let info = value as? CodeBlockInfo else { return }
            let code = (storage.string as NSString).substring(with: range)
                .trimmingCharacters(in: .newlines)
            let host = NSHostingView(rootView: CodeBlockHeaderView(language: info.language, code: code))
            addSubview(host)
            headerViews.append(host)
        }
        needsLayout = true
    }

    override func layout() {
        super.layout()
        layoutCodeBlockHeaders()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        onAppearanceChanged?()
    }

    private func layoutCodeBlockHeaders() {
        guard let layoutManager, let textContainer, let storage = textStorage else { return }
        var index = 0
        storage.enumerateAttribute(CodeBlockStyle.codeBlockAttribute,
                                   in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard value is CodeBlockInfo, index < headerViews.count else { return }
            let glyphRange = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let rect = layoutManager.boundingRect(forGlyphRange: glyphRange, in: textContainer)
            headerViews[index].frame = NSRect(
                x: 0,
                y: rect.minY - CodeBlockStyle.headerHeight - 4,
                width: textContainer.size.width,
                height: CodeBlockStyle.headerHeight)
            index += 1
        }
    }
}
```

- [ ] **Step 3: Wire it up**

In `makeTextView()`, instantiate `MarkdownTextView` instead of `NSTextView` and return `MarkdownTextView`. In `makeNSView` and `updateNSView`, call `textView.rebuildCodeBlockHeaders()` immediately after every `setAttributedString(...)`.

- [ ] **Step 4: Build + test**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test -only-testing:TillerTests/MarkdownAttributedStringRendererTests 2>&1 | tail -20`
Expected: PASS (wiring test now instantiates `MarkdownTextView`).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/CodeBlockHeaderView.swift App/Chat/AgentMarkdownTextView.swift
git commit -m "feat: code block header bar with language label and copy button"
```

---

### Task 9: Appearance-change re-render, full gate, manual QA

**Files:**
- Modify: `App/Chat/AgentMarkdownTextView.swift`

**Interfaces:**
- Consumes: `MarkdownTextView.onAppearanceChanged` (Task 8), `render(_:isDark:)` (Task 6).

- [ ] **Step 1: Re-render on appearance change**

In `makeNSView`, after creating the text view:

```swift
textView.onAppearanceChanged = { [weak textView] in
    guard let textView else { return }
    textView.textStorage?.setAttributedString(
        MarkdownAttributedStringRenderer.render(textView.string.isEmpty ? "" : context.coordinator.lastRenderedSource ?? ""))
    (textView as? MarkdownTextView)?.rebuildCodeBlockHeaders()
}
```

Simplify: store the closure so it re-renders `context.coordinator.lastRenderedSource` (the markdown source, not `textView.string`) and invalidates `context.coordinator.measuredHeight = nil`. The coordinator is a class — capture it directly:

```swift
let coordinator = context.coordinator
textView.onAppearanceChanged = { [weak textView] in
    guard let textView, let source = coordinator.lastRenderedSource else { return }
    textView.textStorage?.setAttributedString(MarkdownAttributedStringRenderer.render(source))
    (textView as? MarkdownTextView)?.rebuildCodeBlockHeaders()
    coordinator.measuredHeight = nil
}
```

(`render` picks up the new `MarkdownAppearance.isDark` automatically; `viewDidChangeEffectiveAppearance` fires on the main thread.)

- [ ] **Step 2: Full verification gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`. (PTY tests in TillerTerminal are known-flaky — retry up to 5-6 times if `spawnCapturesOutput` fails; only that test.)

Run AppTests fully:
`xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData test 2>&1 | tail -20`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 3: Manual QA checklist (run the app, open an ACP chat)**

- [ ] Prose: airy line height, clear paragraph separation, muted body vs bold headings.
- [ ] Bullets visible for `-` lists; ordinals for numbered lists; nested lists indent.
- [ ] Inline code renders as a rounded chip in both dark and light appearance.
- [ ] Code block: rounded card, 1px border, header bar with language label; copy button copies exact code and flashes a checkmark.
- [ ] Syntax colors present for a ```swift block; a ```text / unknown-language block stays monochrome, no errors logged.
- [ ] Drag-selection flows continuously from prose through a code block to the next paragraph; header bar is not selectable.
- [ ] During streaming: no flicker/hang while a long code block streams in; header appears when the block appears.
- [ ] Toggle system appearance while a chat with code is open: colors and highlight theme follow.
- [ ] Long transcript scroll performance unchanged (no beachball).

- [ ] **Step 4: Commit**

```bash
git add App/Chat/AgentMarkdownTextView.swift
git commit -m "feat: re-render chat markdown on appearance change"
```

---

## Self-review notes

- Spec coverage: typography (T1), bullets (T2), chips (T3), card metrics + language + separator-newline exclusion (T4), Highlightr + cache + guards + init-failure fallback (T5), highlight wiring + unknown-language fallback (T6), card/chip drawing + TextKit 1 stack (T7), header + copy (T8), appearance + QA (T9). Unclosed-fence fallback: existing test must keep passing (T4 Step 4).
- Type consistency: `CodeBlockInfo`, `CodeBlockStyle.*`, `makeTextView()`, `rebuildCodeBlockHeaders()`, `onAppearanceChanged`, `highlight(code:language:isDark:)` used with identical signatures across tasks.
