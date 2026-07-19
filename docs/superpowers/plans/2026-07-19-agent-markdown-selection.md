# Continuous Selection In Agent Markdown Replies Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix drag-selection stopping at block boundaries (paragraph/heading/code/list) inside a single agent chat reply by rendering `.agentMessage` markdown through one `NSTextView` instead of MarkdownUI's multi-view block sequence.

**Architecture:** `MarkdownAttributedStringRenderer` (pure function, `App/Chat/`) converts a markdown string to a single `NSAttributedString` via Foundation's `AttributedString(markdown:options:)`, mapping `PresentationIntent`/`InlinePresentationIntent` to `TillerMarkdownTheme`-equivalent fonts/colors/paragraph styles. `AgentMarkdownTextView` (`NSViewRepresentable`, same shape as `ChatTextEditor`) hosts that string in a non-editable, selectable `NSTextView` with no internal scroller. `TranscriptView.swift`'s `.agentMessage` case swaps to this new view.

**Tech Stack:** Swift 6, SwiftUI + AppKit (`NSViewRepresentable`/`NSTextView`), Foundation `AttributedString` markdown parsing, swift-testing.

## Global Constraints

- macOS 15+ (project deployment target) — `AttributedString(markdown:options:)` and `PresentationIntent` are available since macOS 12, no floor issue.
- Zero new package dependencies (spec decision — Foundation-only for the primary path).
- `App/Chat/` is where this logic lives, not `TillerCore` — it depends on AppKit types (`NSFont`, `NSColor`, `NSAttributedString`), which `TillerCore` deliberately avoids per `CLAUDE.md`'s package-boundary rule.
- Tests for this feature live in `AppTests/` (the `TillerTests` Xcode unit-test bundle target, not a SwiftPM package) — **`Scripts/ci.sh` does not run `TillerTests`** (it only builds the `Tiller` app target and runs `swift test` for `Packages/*/`). Verify each task's tests with `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/<TestClass> CODE_SIGNING_ALLOWED=NO`, then still run `Scripts/ci.sh` at the end of every task as the project's mandated build gate (must print `CI OK`). This mirrors the pre-existing situation for `AppTests/AppModelControlTests.swift` — not something this plan changes.
- Follow `ChatTextEditor.swift`'s existing `NSViewRepresentable` pattern (same file, `App/Chat/ChatTextEditor.swift`) for structure and naming conventions.
- Commit after every task with a Conventional Commit message (lower-case imperative subject), no AI attribution trailer.

---

### Task 1: Renderer core — plain text + inline styles

**Files:**
- Create: `App/Chat/MarkdownAttributedStringRenderer.swift`
- Test: `AppTests/MarkdownAttributedStringRendererTests.swift`

**Interfaces:**
- Produces: `enum MarkdownAttributedStringRenderer { static func render(_ markdown: String) -> NSAttributedString }` — the only entry point later tasks (`AgentMarkdownTextView`) call.

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/MarkdownAttributedStringRendererTests.swift
import AppKit
import Testing
@testable import Tiller

struct MarkdownAttributedStringRendererTests {
    @Test("plain paragraph uses 13pt body font")
    func plainParagraph() {
        let result = MarkdownAttributedStringRenderer.render("Hello world")
        #expect(result.string == "Hello world")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 13)
    }

    @Test("bold text gets a bold font")
    func boldText() {
        let result = MarkdownAttributedStringRenderer.render("**bold**")
        #expect(result.string == "bold")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == true)
    }

    @Test("italic text gets an italic font")
    func italicText() {
        let result = MarkdownAttributedStringRenderer.render("*italic*")
        #expect(result.string == "italic")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.fontDescriptor.symbolicTraits.contains(.italic) == true)
    }

    @Test("inline code uses 12pt monospaced font")
    func inlineCode() {
        let result = MarkdownAttributedStringRenderer.render("`code`")
        #expect(result.string == "code")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 12)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.monoSpace) == true)
    }

    @Test("strikethrough text gets the strikethrough attribute")
    func strikethroughText() {
        let result = MarkdownAttributedStringRenderer.render("~~gone~~")
        #expect(result.string == "gone")
        let style = result.attribute(.strikethroughStyle, at: 0, effectiveRange: nil) as? Int
        #expect(style == NSUnderlineStyle.single.rawValue)
    }

    @Test("link text carries the .link attribute with the right URL")
    func linkText() {
        let result = MarkdownAttributedStringRenderer.render("[Tiller](https://example.com)")
        #expect(result.string == "Tiller")
        let link = result.attribute(.link, at: 0, effectiveRange: nil) as? NSURL
        #expect(link?.absoluteString == "https://example.com")
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodegen generate && xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/MarkdownAttributedStringRendererTests CODE_SIGNING_ALLOWED=NO 2>&1 | tail -30`
Expected: FAIL — `MarkdownAttributedStringRenderer` does not exist ("cannot find 'MarkdownAttributedStringRenderer' in scope").

- [ ] **Step 3: Write minimal implementation**

```swift
// App/Chat/MarkdownAttributedStringRenderer.swift
import AppKit
import Foundation

/// Converts a markdown string to an `NSAttributedString` styled to match
/// `TillerMarkdownTheme`, for rendering inside a single `NSTextView` so
/// drag-selection stays continuous across block boundaries. See
/// docs/superpowers/specs/2026-07-19-agent-markdown-selection-design.md.
enum MarkdownAttributedStringRenderer {

    static let bodySize: CGFloat = 13
    static let codeSize: CGFloat = 12

    static func render(_ markdown: String) -> NSAttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            allowsExtendedAttributes: true,
            interpretedSyntax: .full,
            failurePolicy: .throwError)
        guard let parsed = try? AttributedString(markdown: markdown, options: options) else {
            return NSAttributedString(
                string: markdown,
                attributes: [.font: NSFont.systemFont(ofSize: bodySize)])
        }
        return render(parsed)
    }

    private static func render(_ parsed: AttributedString) -> NSAttributedString {
        let result = NSMutableAttributedString()
        for run in parsed.runs {
            let substring = String(parsed[run.range].characters)
            result.append(NSAttributedString(string: substring, attributes: attributes(for: run)))
        }
        return result
    }

    private static func attributes(for run: AttributedString.Runs.Run) -> [NSAttributedString.Key: Any] {
        var font = NSFont.systemFont(ofSize: bodySize)
        var color: NSColor = .labelColor
        let paragraphStyle = NSMutableParagraphStyle()
        var attrs: [NSAttributedString.Key: Any] = [:]

        if let inline = run.inlinePresentationIntent {
            if inline.contains(.code) {
                font = NSFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
            }
            if inline.contains(.stronglyEmphasized) {
                font = NSFontManager.shared.convert(font, toHaveTrait: .boldFontMask)
            }
            if inline.contains(.emphasized) {
                font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
            }
            if inline.contains(.strikethrough) {
                attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            }
        }

        if let link = run.link {
            attrs[.link] = link as NSURL
            color = .linkColor
        }

        attrs[.font] = font
        attrs[.foregroundColor] = color
        attrs[.paragraphStyle] = paragraphStyle
        return attrs
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/MarkdownAttributedStringRendererTests CODE_SIGNING_ALLOWED=NO 2>&1 | tail -30`
Expected: PASS (all 6 tests). If `NSFontManager.convert(_:toHaveTrait:)` doesn't combine bold+italic as expected in the (not-yet-written) combined-styling case, that's fine — no test in this task exercises bold+italic together; Task 2/3 will surface it if it matters there.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/MarkdownAttributedStringRenderer.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: render inline markdown styles to a single NSAttributedString"
```

---

### Task 2: Renderer — headings

**Files:**
- Modify: `App/Chat/MarkdownAttributedStringRenderer.swift`
- Modify: `AppTests/MarkdownAttributedStringRendererTests.swift`

**Interfaces:**
- Consumes: existing `attributes(for:)` from Task 1.
- Produces: same public `render(_:)` signature; no change for callers.

- [ ] **Step 1: Write the failing tests**

Add to `MarkdownAttributedStringRendererTests`:

```swift
    @Test("h1 heading is 15pt semibold")
    func heading1() {
        let result = MarkdownAttributedStringRenderer.render("# Title")
        #expect(result.string == "Title")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 15)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == true)
    }

    @Test("h2 heading is 14pt semibold")
    func heading2() {
        let result = MarkdownAttributedStringRenderer.render("## Section")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 14)
    }

    @Test("h3 heading is 13pt semibold")
    func heading3() {
        let result = MarkdownAttributedStringRenderer.render("### Subsection")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 13)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == true)
    }

    @Test("h4 falls back to h3 sizing")
    func heading4FallsBackToH3() {
        let result = MarkdownAttributedStringRenderer.render("#### Deep")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 13)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == true)
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/MarkdownAttributedStringRendererTests CODE_SIGNING_ALLOWED=NO 2>&1 | tail -30`
Expected: FAIL on the 4 new heading tests (font stays 13pt/non-bold since no heading handling exists yet — `heading3`/`heading4FallsBackToH3` may already pass by coincidence on size but fail on `.bold`).

- [ ] **Step 3: Implement heading mapping**

In `App/Chat/MarkdownAttributedStringRenderer.swift`, add the heading size table and wire block-level `presentationIntent` handling into `attributes(for:)`:

```swift
    /// h1/h2/h3 match `TillerMarkdownTheme`; h4-h6 fall back to h3 sizing —
    /// the theme itself only styles up to h3, so there's no richer source of
    /// truth to copy for deeper levels.
    private static let headingSizes: [Int: CGFloat] = [1: 15, 2: 14, 3: 13]
    private static let headingMargins: [Int: (top: CGFloat, bottom: CGFloat)] =
        [1: (12, 4), 2: (10, 4), 3: (8, 2)]
```

Insert this block at the top of `attributes(for:)`, before the existing `inlinePresentationIntent` handling (block-level styling establishes the base font that inline styling then layers on top of):

```swift
        if let intent = run.presentationIntent {
            for component in intent.components {
                switch component.kind {
                case .header(let level):
                    let size = headingSizes[level] ?? headingSizes[3]!
                    font = NSFontManager.shared.convert(
                        NSFont.systemFont(ofSize: size), toHaveTrait: .boldFontMask)
                    let margin = headingMargins[level] ?? headingMargins[3]!
                    paragraphStyle.paragraphSpacingBefore = margin.top
                    paragraphStyle.paragraphSpacing = margin.bottom
                default:
                    break
                }
            }
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/MarkdownAttributedStringRendererTests CODE_SIGNING_ALLOWED=NO 2>&1 | tail -30`
Expected: PASS (all 10 tests so far).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/MarkdownAttributedStringRenderer.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: map markdown headings to TillerMarkdownTheme sizes"
```

---

### Task 3: Renderer — code blocks, blockquote, lists, thematic break

**Files:**
- Modify: `App/Chat/MarkdownAttributedStringRenderer.swift`
- Modify: `AppTests/MarkdownAttributedStringRendererTests.swift`

**Interfaces:**
- Consumes: the `switch component.kind` block added in Task 2.
- Produces: same public `render(_:)` signature.

- [ ] **Step 1: Write the failing tests**

Add to `MarkdownAttributedStringRendererTests`:

```swift
    @Test("fenced code block uses 12pt monospaced font")
    func codeBlock() {
        let result = MarkdownAttributedStringRenderer.render("```\nlet x = 1\n```")
        #expect(result.string.contains("let x = 1"))
        let range = (result.string as NSString).range(of: "let x = 1")
        let font = result.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 12)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.monoSpace) == true)
    }

    @Test("blockquote is indented and secondary-colored")
    func blockquote() {
        let result = MarkdownAttributedStringRenderer.render("> quoted")
        #expect(result.string.contains("quoted"))
        let range = (result.string as NSString).range(of: "quoted")
        let style = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle
        #expect(style?.headIndent == 12)
        let color = result.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? NSColor
        #expect(color == .secondaryLabelColor)
    }

    @Test("unordered list item is indented")
    func unorderedListItem() {
        let result = MarkdownAttributedStringRenderer.render("- one\n- two")
        #expect(result.string.contains("one"))
        let range = (result.string as NSString).range(of: "one")
        let style = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle
        #expect(style?.headIndent == 16)
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/MarkdownAttributedStringRendererTests CODE_SIGNING_ALLOWED=NO 2>&1 | tail -30`
Expected: FAIL on the 3 new tests (no `.codeBlock`/`.blockQuote`/`.listItem` handling yet).

- [ ] **Step 3: Implement code block / blockquote / list handling**

Extend the `switch component.kind` added in Task 2 with three more cases (keep `case .header` as-is from Task 2):

```swift
                case .codeBlock:
                    font = NSFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
                case .blockQuote:
                    color = .secondaryLabelColor
                    paragraphStyle.headIndent = 12
                    paragraphStyle.firstLineHeadIndent = 12
                case .listItem:
                    paragraphStyle.headIndent = 16
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/MarkdownAttributedStringRendererTests CODE_SIGNING_ALLOWED=NO 2>&1 | tail -30`
Expected: PASS (all 13 tests so far).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/MarkdownAttributedStringRenderer.swift AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "feat: map markdown code blocks, blockquotes and lists"
```

---

### Task 4: Renderer — malformed markdown fallback

**Files:**
- Modify: `AppTests/MarkdownAttributedStringRendererTests.swift` (no production code change expected — this task proves the fallback from Task 1's `guard let parsed = try? ...` already works, and locks it in with a regression test)

**Interfaces:**
- Consumes: `MarkdownAttributedStringRenderer.render(_:)`, `bodySize`.

- [ ] **Step 1: Write the failing test**

```swift
    @Test("unclosed code fence falls back to plain selectable text")
    func unclosedFenceFallsBackToPlainText() {
        let input = "before\n```\nlet x = 1"
        let result = MarkdownAttributedStringRenderer.render(input)
        #expect(result.string == input)
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == MarkdownAttributedStringRenderer.bodySize)
    }
```

- [ ] **Step 2: Run test to check current behavior**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/MarkdownAttributedStringRendererTests CODE_SIGNING_ALLOWED=NO 2>&1 | tail -30`
Expected: Likely PASS already, since Task 1's `guard let parsed = try? AttributedString(...) else { return ... }` fallback exists and cmark-gfm's underlying parser (used transitively) is lenient enough that some malformed inputs still throw with `.throwError`. If it FAILS because the parser silently auto-closes the fence instead of throwing, replace the fixture with one that reliably throws under `.full` + `.throwError` (e.g. an unterminated inline `` ` `` code span mid-sentence: `"before `unterminated"`) and re-run until it exercises the fallback branch — the goal is proving the `guard`/`else` path is reachable and correct, not a specific fixture string.

- [ ] **Step 3: No implementation step** — this task is a regression test for existing Task 1 behavior. If Step 2 already passes, skip straight to commit.

- [ ] **Step 4: Commit**

```bash
git add AppTests/MarkdownAttributedStringRendererTests.swift
git commit -m "test: lock in plain-text fallback for malformed markdown"
```

---

### Task 5: AgentMarkdownTextView

**Files:**
- Create: `App/Chat/AgentMarkdownTextView.swift`

**Interfaces:**
- Consumes: `MarkdownAttributedStringRenderer.render(_ markdown: String) -> NSAttributedString` (Task 1-3).
- Produces: `struct AgentMarkdownTextView: NSViewRepresentable { init(markdown: String) }` — what Task 6 instantiates from `TranscriptView`.

No automated test for this file — it's a thin `NSViewRepresentable` wrapper exercising AppKit view lifecycle (`makeNSView`/`updateNSView`/`sizeThatFits`), the same category of code as `App/Chat/ChatTextEditor.swift`, which also has no test file in this codebase. Verified instead by Task 6's manual QA.

- [ ] **Step 1: Implement the view**

```swift
// App/Chat/AgentMarkdownTextView.swift
import AppKit
import SwiftUI

/// Renders an agent chat message's markdown inside a single `NSTextView` so
/// drag-selection stays continuous across block boundaries (paragraph, code
/// block, list, heading) — see
/// docs/superpowers/specs/2026-07-19-agent-markdown-selection-design.md.
/// Unlike `ChatTextEditor`, this view is read-only and uncapped in height:
/// it reports its full content height and relies on `TranscriptView`'s
/// enclosing `ScrollView`, matching how `Markdown(text)` behaved before it.
struct AgentMarkdownTextView: NSViewRepresentable {
    var markdown: String

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSTextView {
        let textView = NSTextView()
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.textContainerInset = .zero
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.widthTracksTextView = true
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textStorage?.setAttributedString(MarkdownAttributedStringRenderer.render(markdown))
        context.coordinator.lastRenderedSource = markdown
        return textView
    }

    func updateNSView(_ textView: NSTextView, context: Context) {
        guard context.coordinator.lastRenderedSource != markdown else { return }
        textView.textStorage?.setAttributedString(MarkdownAttributedStringRenderer.render(markdown))
        context.coordinator.lastRenderedSource = markdown
    }

    /// Same technique as `ChatTextEditor.sizeThatFits`: SwiftUI consults this
    /// (not Auto Layout / `intrinsicContentSize`) to size an
    /// `NSViewRepresentable`. No height cap here — the outer `ScrollView` in
    /// `TranscriptView` owns scrolling, this view just reports how tall its
    /// text actually is.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextView,
                      context: Context) -> CGSize? {
        guard let layoutManager = nsView.layoutManager, let container = nsView.textContainer else {
            return nil
        }
        layoutManager.ensureLayout(for: container)
        let height = layoutManager.usedRect(for: container).height
        return CGSize(width: proposal.width ?? nsView.frame.width, height: height)
    }

    final class Coordinator {
        var lastRenderedSource: String?
    }
}
```

- [ ] **Step 2: Confirm it builds**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -20`
Expected: `** BUILD SUCCEEDED **`. `AgentMarkdownTextView` isn't referenced anywhere yet, so this only proves it compiles standalone.

- [ ] **Step 3: Commit**

```bash
git add App/Chat/AgentMarkdownTextView.swift
git commit -m "feat: add NSTextView-backed markdown view for agent messages"
```

---

### Task 6: Wire into TranscriptView

**Files:**
- Modify: `App/Chat/TranscriptView.swift:44-46`

**Interfaces:**
- Consumes: `AgentMarkdownTextView(markdown: String)` (Task 5).

- [ ] **Step 1: Swap the agent-message case**

In `TranscriptView.itemView(_:)`, replace:

```swift
        case .agentMessage(_, let text, _):
            Markdown(text)
                .markdownTheme(.tiller)
                .textSelection(.enabled)
```

with:

```swift
        case .agentMessage(_, let text, _):
            AgentMarkdownTextView(markdown: text)
```

`import MarkdownUI` at the top of `TranscriptView.swift` stays — `TillerMarkdownTheme.swift` (a separate file, extending `Theme`) still needs it, and this plan doesn't touch that file. Check with `grep -n "Markdown(" App/Chat/TranscriptView.swift` that no other `Markdown(...)` call remains in this file before assuming the import is still needed elsewhere; if it genuinely becomes unused in this file specifically, Swift will warn at build time — remove the import only if the build step below flags it.

- [ ] **Step 2: Build and run the full CI gate**

Run: `Scripts/ci.sh`
Expected: Output ends with `CI OK`. This is the project's mandated gate — it builds the `Tiller` app target (exercising this exact code path) and runs every package's `swift test`, though (per Global Constraints) it does not execute `TillerTests`/`AppTests` itself.

- [ ] **Step 3: Commit**

```bash
git add App/Chat/TranscriptView.swift
git commit -m "fix: render agent markdown in one NSTextView for continuous selection"
```

---

### Task 7: Spike gate — GFM table/task-list fidelity

**Files:** None (manual verification task, per spec's documented spike gate — no code change).

- [ ] **Step 1: Gather real fixtures**

Find or produce at least one real (or realistic) agent reply containing a GFM table and one containing a task list (`- [ ] item`). If none exist in recent transcript history, write two short representative markdown snippets by hand covering: a 3-column table with a header row, and a 3-item task list with one checked box.

- [ ] **Step 2: Compare renderings**

Build and run the app (`open Tiller.xcodeproj`, ⌘R), paste/trigger both fixtures through the agent-message path (or temporarily hardcode one into a `TranscriptView` preview/test harness), and visually compare `AgentMarkdownTextView`'s output against the same markdown rendered by the old `Markdown(text).markdownTheme(.tiller)` (e.g. render both side by side temporarily, or compare against a screenshot taken before Task 6's swap).

- [ ] **Step 3: Decide**

- If tables/task-lists render acceptably (content readable, no crash, no garbled structure — exact pixel parity is explicitly not required per spec) → mark this task done, no further code changes needed for this plan.
- If unacceptable → this plan's scope stops here; open a follow-up spec iteration for the `cmark-gfm` AST-walk fallback documented in `docs/superpowers/specs/2026-07-19-agent-markdown-selection-design.md`'s Decisions section, rather than silently shipping degraded table rendering.

- [ ] **Step 4: Record the outcome**

Add one line to the spec's Testing section noting the spike result (pass, or link to the follow-up spec), and commit:

```bash
git add docs/superpowers/specs/2026-07-19-agent-markdown-selection-design.md
git commit -m "docs: record GFM table/task-list spike gate outcome"
```

---

### Task 8: Manual QA

**Files:** None.

- [ ] **Step 1: Continuous selection across block boundaries**

In a real (or triggered) agent reply containing at least a paragraph, a fenced code block, and a list, click-drag from inside the paragraph through the code block into the list. Confirm one continuous highlighted selection and that ⌘C copies all of it as plain text (`⌘V` into a text editor to confirm).

- [ ] **Step 2: Links still open**

Confirm a markdown link (`[text](url)`) in an agent reply is still clickable and opens the URL, matching current `Markdown(text)` behavior.

- [ ] **Step 3: Streaming doesn't break selection or flicker**

Trigger a new agent reply and watch it stream in; confirm the view grows smoothly (no visible re-layout jank beyond what the current `Markdown(text)` already has) and that selecting text works both mid-stream and after `isComplete` becomes `true`.

- [ ] **Step 4: Empty and malformed edge cases**

Confirm an empty agent message renders with zero height (no visible gap), and if a reply happens to stream through a temporarily-unclosed code fence, the text stays visible (unstyled is fine) rather than disappearing.

- [ ] **Step 5: Final CI gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`.
