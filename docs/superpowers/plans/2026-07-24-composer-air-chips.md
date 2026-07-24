# Composer AIR-Style Inline Chips Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render slash commands and attachments as atomic inline chips in the chat composer, matching the AIR reference UI, and migrate the composer to TextKit 2 to make that possible.

**Architecture:** Chips are `NSTextAttachment`s occupying one `U+FFFC` character each; a `NSTextAttachmentViewProvider` mounts a SwiftUI `NSHostingView` for each. The `NSTextStorage` becomes the single source of truth for the draft, owned by a new `@Observable ComposerDocument` — replacing the `@Binding var text: String` that would otherwise wipe attachments on every SwiftUI update. Attachment view providers only work under TextKit 2, so `ChatTextEditor` stops reading `.layoutManager`.

**Tech Stack:** SwiftUI + AppKit (`NSViewRepresentable`), TextKit 2 (`NSTextLayoutManager`, `NSTextAttachmentViewProvider`), swift-testing, XcodeGen.

Spec: `docs/superpowers/specs/2026-07-24-composer-air-chips-design.md`

## Global Constraints

- Swift 6, macOS 15+ deployment target. Tests use **swift-testing** (`@Test` / `#expect`), never XCTest.
- All user-facing UI strings in **English**, even though project discussion happens in Italian.
- Domain types are **structs**; classes only for real identity (here: `NSTextAttachment` and `NSTextAttachmentViewProvider` subclasses, which AppKit requires to be classes).
- The app test target is named **`TillerTests`** (sources live in `AppTests/`, see `project.yml:107-116`). Test filter is therefore `-only-testing:TillerTests/<SwiftTypeName>` — the Swift type name, not the `@Suite("...")` display name.
- **Never pass `-quiet` to `xcodebuild test`** — it suppresses swift-testing's `✔`/`✘` output, making a filtered run look like it executed nothing.
- `Scripts/ci.sh` compiles `TillerTests` but **does not run it** (it only runs `swift test` per package). App tests must be run as a separate explicit gate.
- After adding any new file, run `xcodegen generate` before building — `Tiller.xcodeproj` is generated and gitignored.
- Commit messages follow Conventional Commits with a lower-case imperative subject.
- The wire protocol and DB schema do **not** change. No migration.

**Standard test command** (used by every task):

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/<TypeName> test 2>&1 | grep -E "✔|✘|Test run|error:"
```

**Known-flaky, not your fault:** `spawnCapturesOutput` in `Packages/TillerTerminal` fails intermittently under `Scripts/ci.sh`; it can need 5-6 retries. It is unrelated to this work.

---

### Task 1: Migrate the composer to TextKit 2

The only TextKit 1 dependency in the composer is `recalculateHeight`, which reads `textView.layoutManager`. Reading that property is what silently downgrades the text view to TextKit 1 compatibility mode, where `NSTextAttachmentViewProvider` is never called. Everything else in this plan depends on this task.

**Files:**
- Modify: `App/Chat/ChatTextEditor.swift:30-54` (extract `makeTextView`), `App/Chat/ChatTextEditor.swift:151-159` (`recalculateHeight`)
- Test: `AppTests/ComposerTextKit2Tests.swift` (create)

**Interfaces:**
- Consumes: nothing.
- Produces: `static func ChatTextEditor.makeTextView() -> NSTextView` — builds the configured text view with no delegate attached. Used by every later test that needs a real text view.

- [ ] **Step 1: Write the failing test**

Create `AppTests/ComposerTextKit2Tests.swift`:

```swift
import AppKit
import SwiftUI
import Testing

@testable import Tiller

@Suite("ComposerTextKit2")
@MainActor
struct ComposerTextKit2Tests {
    /// `text: .constant("")` is temporary — Task 5 deletes the `text` binding
    /// and this becomes `document: ComposerDocument()`.
    private func makeCoordinator() -> ChatTextEditor.Coordinator {
        ChatTextEditor.Coordinator(
            ChatTextEditor(text: .constant(""), isEditable: true, minHeight: 36,
                           maxHeight: 160, onSubmit: {}, onSlashKey: nil))
    }

    /// A TextKit 1 fallback is silent: no error, no warning, no crash — the
    /// only observable symptom is `textLayoutManager` going nil, and later,
    /// chips never rendering. This test is the guard for that.
    @Test func measuringHeightKeepsTextKit2() {
        let textView = ChatTextEditor.makeTextView()
        let scrollView = AutoSizingScrollView()
        scrollView.documentView = textView
        textView.string = "hello\nworld"

        makeCoordinator().recalculateHeight(textView: textView, scrollView: scrollView)

        #expect(textView.textLayoutManager != nil)
    }

    @Test func heightGrowsWithMoreLines() {
        let coordinator = makeCoordinator()
        let textView = ChatTextEditor.makeTextView()
        let scrollView = AutoSizingScrollView()
        scrollView.documentView = textView
        textView.frame = NSRect(x: 0, y: 0, width: 300, height: 36)

        textView.string = "one line"
        coordinator.recalculateHeight(textView: textView, scrollView: scrollView)
        let single = scrollView.computedHeight

        textView.string = "one line\ntwo lines\nthree lines\nfour lines"
        coordinator.recalculateHeight(textView: textView, scrollView: scrollView)

        #expect(scrollView.computedHeight > single)
    }

    @Test func intrinsicHeightClampsToMaxHeight() {
        let scrollView = AutoSizingScrollView()
        scrollView.minHeight = 36
        scrollView.maxHeight = 160
        scrollView.computedHeight = 9_000

        #expect(scrollView.intrinsicContentSize.height == 160)
    }
}
```

- [ ] **Step 2: Add the file to the project and run the test to verify it fails**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerTextKit2Tests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: a compile error — `makeTextView` does not exist yet. Add the extraction (Step 3), then this same command must show `measuringHeightKeepsTextKit2` **failing** with `textLayoutManager` nil, because `recalculateHeight` still reads `.layoutManager`.

If `measuringHeightKeepsTextKit2` passes *before* Step 4, stop and investigate — it means the fallback is not being triggered where the spec assumed, and Step 4's premise needs rechecking.

- [ ] **Step 3: Extract the text view factory**

In `App/Chat/ChatTextEditor.swift`, replace `makeNSView` with:

```swift
    /// Builds the configured text view. Separate from `makeNSView` so tests
    /// can get a real text view without an `NSViewRepresentableContext`,
    /// which cannot be constructed outside SwiftUI. Mirrors the seam
    /// `AgentMarkdownTextView.makeTextView()` already uses.
    ///
    /// Created with the plain `NSTextView()` initializer, which is TextKit 2
    /// on macOS 15. `init(frame:textContainer:)` would opt into TextKit 1.
    @MainActor
    static func makeTextView() -> NSTextView {
        let textView = NSTextView()
        textView.font = .systemFont(ofSize: NSFont.systemFontSize)
        textView.isRichText = false
        textView.drawsBackground = false
        textView.textContainerInset = NSSize(width: 0, height: 4)
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.widthTracksTextView = true
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        return textView
    }

    func makeNSView(context: Context) -> AutoSizingScrollView {
        let textView = Self.makeTextView()
        textView.delegate = context.coordinator
        textView.string = text
        textView.isEditable = isEditable

        let scrollView = AutoSizingScrollView()
        scrollView.minHeight = minHeight
        scrollView.maxHeight = maxHeight
        scrollView.documentView = textView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        return scrollView
    }
```

- [ ] **Step 4: Replace the TextKit 1 height measurement**

In the `Coordinator`, replace `recalculateHeight` with:

```swift
        /// Measured through TextKit 2. Reading `textView.layoutManager` here
        /// would silently downgrade the view to TextKit 1 compatibility mode,
        /// in which `NSTextAttachmentViewProvider` is never invoked and the
        /// composer's chips stop rendering with no error of any kind.
        /// `usageBoundsForTextContainer` is the TextKit 2 analogue of
        /// `NSLayoutManager.usedRect(for:)`.
        func recalculateHeight(textView: NSTextView, scrollView: AutoSizingScrollView) {
            guard let layoutManager = textView.textLayoutManager else { return }
            layoutManager.ensureLayout(for: layoutManager.documentRange)
            let used = layoutManager.usageBoundsForTextContainer.height
                + textView.textContainerInset.height * 2
            if scrollView.computedHeight != used {
                scrollView.computedHeight = used
                scrollView.invalidateIntrinsicContentSize()
            }
        }
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerTextKit2Tests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: `Test run with 3 tests in 1 suite passed`.

Also confirm the pre-existing suite still passes:

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ChatTextEditorSlashTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: `Test run with 9 tests in 1 suite passed`.

- [ ] **Step 6: Commit**

```bash
git add App/Chat/ChatTextEditor.swift AppTests/ComposerTextKit2Tests.swift project.yml
git commit -m "refactor: measure composer height through TextKit 2"
```

---

### Task 2: Chip model and draft parsing

Pure data. No UI, no AppKit views — just the enum, the attachment that carries it, and the function that turns a text storage back into the triple `controller.send` already accepts.

**Files:**
- Create: `App/Chat/ComposerChip.swift`
- Test: `AppTests/ComposerChipTests.swift`

**Interfaces:**
- Consumes: `ChatTextEditor.makeTextView()` (Task 1) is not needed here; only `NSTextStorage`.
- Produces:
  - `enum ComposerChip: Equatable { case skill(name: String), file(path: String), image(ImageAttachment) }`
  - `ComposerChip.iconName: String`, `ComposerChip.label: String`
  - `final class ComposerChipAttachment: NSTextAttachment` with `let chip: ComposerChip` and `init(chip: ComposerChip)`
  - `struct ComposerDraft: Equatable { var text: String; var mentionPaths: [String]; var images: [ImageAttachment] }`
  - `static func ComposerDraft.parse(_ storage: NSAttributedString) -> ComposerDraft`

- [ ] **Step 1: Write the failing test**

Create `AppTests/ComposerChipTests.swift`:

```swift
import AppKit
import Testing
import TillerACP

@testable import Tiller

@Suite("ComposerChip")
@MainActor
struct ComposerChipTests {
    private let png = ImageAttachment(mimeType: "image/png", base64Data: "AAAA")

    private func storage(_ pieces: [Any]) -> NSTextStorage {
        let result = NSTextStorage()
        for piece in pieces {
            switch piece {
            case let text as String:
                result.append(NSAttributedString(string: text))
            case let chip as ComposerChip:
                result.append(NSAttributedString(attachment: ComposerChipAttachment(chip: chip)))
            default:
                Issue.record("unsupported storage piece")
            }
        }
        return result
    }

    @Test func emptyStorageProducesEmptyDraft() {
        #expect(ComposerDraft.parse(NSTextStorage())
                == ComposerDraft(text: "", mentionPaths: [], images: []))
    }

    @Test func plainTextPassesThroughUnchanged() {
        let draft = ComposerDraft.parse(storage(["hello world"]))
        #expect(draft == ComposerDraft(text: "hello world", mentionPaths: [], images: []))
    }

    @Test func skillChipSerializesAsSlashCommand() {
        let draft = ComposerDraft.parse(storage([ComposerChip.skill(name: "brainstorm"), "do it"]))
        #expect(draft.text == "/brainstorm do it")
        #expect(draft.mentionPaths.isEmpty)
        #expect(draft.images.isEmpty)
    }

    @Test func fileChipsBecomeMentionPathsAndLeaveNoText() {
        let draft = ComposerDraft.parse(storage([
            ComposerChip.file(path: "App/Chat/ChatTextEditor.swift"),
            ComposerChip.file(path: "README.md"),
            "review these",
        ]))
        #expect(draft.text == "review these")
        #expect(draft.mentionPaths == ["App/Chat/ChatTextEditor.swift", "README.md"])
    }

    @Test func duplicateFileChipsAreCollapsed() {
        let draft = ComposerDraft.parse(storage([
            ComposerChip.file(path: "README.md"),
            ComposerChip.file(path: "README.md"),
        ]))
        #expect(draft.mentionPaths == ["README.md"])
    }

    @Test func imageChipsBecomeImageAttachments() {
        let draft = ComposerDraft.parse(storage([ComposerChip.image(png), "what is this"]))
        #expect(draft.text == "what is this")
        #expect(draft.images == [png])
    }

    @Test func allThreeKindsParseTogetherInOrder() {
        let draft = ComposerDraft.parse(storage([
            ComposerChip.skill(name: "review"),
            "look at ",
            ComposerChip.file(path: "a.swift"),
            " and ",
            ComposerChip.image(png),
            " please",
        ]))
        #expect(draft.text == "/review look at  and  please")
        #expect(draft.mentionPaths == ["a.swift"])
        #expect(draft.images == [png])
    }

    @Test func iconNamesAreFixedPerKind() {
        #expect(ComposerChip.skill(name: "x").iconName == "cube")
        #expect(ComposerChip.file(path: "dir/a.swift").iconName == "doc")
        #expect(ComposerChip.image(png).iconName == "photo")
    }

    @Test func fileChipLabelIsTheLastPathComponent() {
        #expect(ComposerChip.file(path: "App/Chat/ChatTextEditor.swift").label
                == "ChatTextEditor.swift")
        #expect(ComposerChip.skill(name: "brainstorm").label == "brainstorm")
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerChipTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: compile errors — `ComposerChip`, `ComposerChipAttachment`, `ComposerDraft` are undefined.

- [ ] **Step 3: Write the implementation**

Create `App/Chat/ComposerChip.swift`:

```swift
import AppKit
import TillerACP

/// A composer token rendered as an inline chip. Each chip occupies exactly
/// one `U+FFFC` attachment character in the composer's text storage, which is
/// what makes it atomic for free: AppKit already treats that character as
/// indivisible for caret movement, selection, and backspace.
enum ComposerChip: Equatable {
    case skill(name: String)
    case file(path: String)
    case image(ImageAttachment)

    /// Fixed per kind — `AvailableCommand` carries no icon metadata, and the
    /// file/image symbols match what the old chip row used.
    var iconName: String {
        switch self {
        case .skill: "cube"
        case .file: "doc"
        case .image: "photo"
        }
    }

    var label: String {
        switch self {
        case .skill(let name): name
        case .file(let path): (path as NSString).lastPathComponent
        case .image: "Image"
        }
    }
}

/// Carries a `ComposerChip` through the text storage. A class because
/// `NSTextAttachment` is one.
final class ComposerChipAttachment: NSTextAttachment {
    let chip: ComposerChip

    init(chip: ComposerChip) {
        self.chip = chip
        super.init(data: nil, ofType: nil)
    }

    /// Composer chips are never archived — the draft lives only in memory.
    /// Failing the initializer rather than trapping avoids adding a crash
    /// site for a path that is not exercised.
    required init?(coder: NSCoder) { nil }
}

/// What the composer hands to `ChatController.send`. Deliberately the exact
/// shape that call already takes, so chips change nothing below the UI.
struct ComposerDraft: Equatable {
    var text: String
    var mentionPaths: [String]
    var images: [ImageAttachment]

    /// Walks the storage once. Non-chip runs contribute their characters to
    /// `text`; chip runs contribute to the matching list instead, except the
    /// skill chip, which re-serializes as the `/name ` prefix the agent
    /// expects on the wire.
    static func parse(_ storage: NSAttributedString) -> ComposerDraft {
        var draft = ComposerDraft(text: "", mentionPaths: [], images: [])
        let string = storage.string as NSString
        storage.enumerateAttribute(
            .attachment, in: NSRange(location: 0, length: storage.length)
        ) { value, range, _ in
            guard let chip = (value as? ComposerChipAttachment)?.chip else {
                draft.text += string.substring(with: range)
                return
            }
            switch chip {
            case .skill(let name):
                draft.text += "/\(name) "
            case .file(let path):
                if !draft.mentionPaths.contains(path) { draft.mentionPaths.append(path) }
            case .image(let attachment):
                draft.images.append(attachment)
            }
        }
        return draft
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerChipTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: `Test run with 9 tests in 1 suite passed`.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ComposerChip.swift AppTests/ComposerChipTests.swift project.yml
git commit -m "feat: add composer chip model and draft parsing"
```

---

### Task 3: Chip rendering through a view provider

This is where the TextKit 2 work pays off, and where the spec's one open risk gets resolved: whether a programmatically inserted attachment renders while `isRichText` is false.

**Files:**
- Create: `App/Chat/ComposerChipAttachmentView.swift`
- Modify: `App/Chat/ComposerChip.swift` (add the `viewProvider` override)
- Test: `AppTests/ComposerChipViewTests.swift`

**Interfaces:**
- Consumes: `ComposerChip`, `ComposerChipAttachment` (Task 2).
- Produces:
  - `struct ComposerChipView: View` with `init(chip: ComposerChip)`
  - `final class ComposerChipViewProvider: NSTextAttachmentViewProvider`
  - `ComposerChipAttachment.viewProvider(for:location:textContainer:)` returning a `ComposerChipViewProvider`

- [ ] **Step 1: Write the failing test**

Create `AppTests/ComposerChipViewTests.swift`:

```swift
import AppKit
import SwiftUI
import Testing
import TillerACP

@testable import Tiller

@Suite("ComposerChipView")
@MainActor
struct ComposerChipViewTests {
    @Test func attachmentVendsAChipViewProvider() throws {
        let attachment = ComposerChipAttachment(chip: .skill(name: "brainstorm"))
        let textView = ChatTextEditor.makeTextView()
        let layoutManager = try #require(textView.textLayoutManager)

        let provider = attachment.viewProvider(
            for: textView, location: layoutManager.documentRange.location,
            textContainer: layoutManager.textContainer)

        #expect(provider is ComposerChipViewProvider)
    }

    @Test func providerLoadsANonEmptyHostingView() throws {
        let attachment = ComposerChipAttachment(chip: .file(path: "App/Chat/ChatTextEditor.swift"))
        let textView = ChatTextEditor.makeTextView()
        let layoutManager = try #require(textView.textLayoutManager)
        let provider = ComposerChipViewProvider(
            textAttachment: attachment, parentView: textView,
            textLayoutManager: layoutManager, location: layoutManager.documentRange.location)

        provider.loadView()

        #expect(provider.view is NSHostingView<ComposerChipView>)
        #expect(try #require(provider.view).fittingSize.width > 0)
        #expect(provider.tracksTextAttachmentViewBounds)
    }

    /// Guards the spec's open question: the composer keeps `isRichText = false`,
    /// and a chip inserted programmatically must still survive in the storage
    /// and keep its payload. If this fails, `isRichText` must be flipped to
    /// true and `pasteAsPlainText` / `importsGraphics = false` added — see the
    /// spec's "Open risk" section.
    @Test func chipSurvivesInsertionIntoAPlainTextView() {
        let textView = ChatTextEditor.makeTextView()
        #expect(!textView.isRichText)

        let chip = NSAttributedString(
            attachment: ComposerChipAttachment(chip: .skill(name: "review")))
        textView.textStorage?.append(chip)

        let stored = textView.textStorage?.attribute(
            .attachment, at: 0, effectiveRange: nil) as? ComposerChipAttachment
        #expect(stored?.chip == .skill(name: "review"))
        #expect(textView.textStorage?.length == 1)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerChipViewTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: compile errors — `ComposerChipView` and `ComposerChipViewProvider` are undefined.

If `chipSurvivesInsertionIntoAPlainTextView` fails once the other two compile, that is the `isRichText` risk materialising. In that case, and only then, add to `makeTextView()`:

```swift
        textView.isRichText = true
        textView.importsGraphics = false
```

and change the assertion in that test from `#expect(!textView.isRichText)` to `#expect(!textView.importsGraphics)`. Record the change in a commit message body so the deviation from the spec is traceable.

- [ ] **Step 3: Write the chip view and provider**

Create `App/Chat/ComposerChipAttachmentView.swift`:

```swift
import AppKit
import SwiftUI

/// The chip's visual: a rounded box with a symbol and a label, sized to fit.
struct ComposerChipView: View {
    let chip: ComposerChip

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: chip.iconName)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(chip.label)
                .font(.caption)
                .lineLimit(1)
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 2)
        .background(.quaternary.opacity(0.8), in: RoundedRectangle(cornerRadius: 5))
        .overlay(RoundedRectangle(cornerRadius: 5)
            .strokeBorder(.separator.opacity(0.6), lineWidth: 1))
        .fixedSize()
    }
}

/// Hosts `ComposerChipView` inside the text flow.
///
/// TextKit 2 only. Under TextKit 1 compatibility mode AppKit never asks for a
/// view provider, so the chip would silently not render — see
/// `ComposerTextKit2Tests.measuringHeightKeepsTextKit2`.
final class ComposerChipViewProvider: NSTextAttachmentViewProvider {
    override func loadView() {
        guard let chip = (textAttachment as? ComposerChipAttachment)?.chip else {
            view = NSView()
            return
        }
        let host = NSHostingView(rootView: ComposerChipView(chip: chip))
        host.frame.size = host.fittingSize
        view = host
        // Let the hosting view's own size drive the attachment's layout
        // bounds, instead of overriding `attachmentBounds`.
        tracksTextAttachmentViewBounds = true
    }
}
```

- [ ] **Step 4: Wire the provider onto the attachment**

In `App/Chat/ComposerChip.swift`, add to `ComposerChipAttachment`, after `init?(coder:)`:

```swift
    /// Overridden per-attachment rather than registered with
    /// `NSTextAttachment.registerViewProviderClass(_:forFileType:)`, because
    /// that registry is process-global and a composer chip is a detail of one
    /// view, not a document type.
    override func viewProvider(
        for parentView: NSView?, location: NSTextLocation,
        textContainer: NSTextContainer?
    ) -> NSTextAttachmentViewProvider? {
        ComposerChipViewProvider(
            textAttachment: self, parentView: parentView,
            textLayoutManager: textContainer?.textLayoutManager, location: location)
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerChipViewTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: `Test run with 3 tests in 1 suite passed`.

- [ ] **Step 6: Commit**

```bash
git add App/Chat/ComposerChipAttachmentView.swift App/Chat/ComposerChip.swift \
        AppTests/ComposerChipViewTests.swift project.yml
git commit -m "feat: render composer chips through a text attachment view provider"
```

---

### Task 4: ComposerDocument — the storage becomes the source of truth

**Files:**
- Create: `App/Chat/ComposerDocument.swift`
- Test: `AppTests/ComposerDocumentTests.swift`

**Interfaces:**
- Consumes: `ComposerChip`, `ComposerChipAttachment`, `ComposerDraft` (Task 2).
- Produces `@Observable @MainActor final class ComposerDocument` with:
  - `let storage: NSTextStorage`
  - `var isEmpty: Bool` (computed)
  - `private(set) var slashQuery: String?`
  - `private(set) var mentionQuery: String?`
  - `var isFocused: Bool`
  - `func refreshQueries()`
  - `func insert(_ chip: ComposerChip, replacing range: NSRange)`
  - `func replaceSlashToken(with chip: ComposerChip) -> NSRange?`
  - `func takeDraft() -> ComposerDraft`

- [ ] **Step 1: Write the failing test**

Create `AppTests/ComposerDocumentTests.swift`:

```swift
import AppKit
import Testing
import TillerACP

@testable import Tiller

@Suite("ComposerDocument")
@MainActor
struct ComposerDocumentTests {
    private let png = ImageAttachment(mimeType: "image/png", base64Data: "AAAA")

    private func document(_ text: String = "") -> ComposerDocument {
        let document = ComposerDocument()
        if !text.isEmpty {
            document.storage.append(NSAttributedString(string: text))
        }
        return document
    }

    @Test func freshDocumentIsEmpty() {
        #expect(document().isEmpty)
    }

    @Test func documentWithAChipIsNotEmpty() {
        let subject = document()
        subject.insert(.image(png), replacing: NSRange(location: 0, length: 0))
        #expect(!subject.isEmpty)
    }

    @Test func slashQueryIsTheLeadingTokenWithoutTheSlash() {
        let subject = document("/brain")
        subject.refreshQueries()
        #expect(subject.slashQuery == "brain")
    }

    @Test func slashQueryIsNilOnceTheTokenContainsWhitespace() {
        let subject = document("/brain storm")
        subject.refreshQueries()
        #expect(subject.slashQuery == nil)
    }

    @Test func slashQueryIsNilWhenTextDoesNotStartWithSlash() {
        let subject = document("hello")
        subject.refreshQueries()
        #expect(subject.slashQuery == nil)
    }

    @Test func mentionQueryIsTheTokenAfterTheLastAt() {
        let subject = document("look at @Chat")
        subject.refreshQueries()
        #expect(subject.mentionQuery == "Chat")
    }

    @Test func mentionQueryIsNilOnceTheTokenContainsWhitespace() {
        let subject = document("look at @Chat editor")
        subject.refreshQueries()
        #expect(subject.mentionQuery == nil)
    }

    @Test func replaceSlashTokenSwapsTheTokenForAChipAndASpace() {
        let subject = document("/brain")
        subject.refreshQueries()

        let caret = subject.replaceSlashToken(with: .skill(name: "brainstorm"))

        #expect(caret == NSRange(location: 2, length: 0))
        #expect(subject.storage.length == 2)
        let stored = subject.storage.attribute(
            .attachment, at: 0, effectiveRange: nil) as? ComposerChipAttachment
        #expect(stored?.chip == .skill(name: "brainstorm"))
        #expect(subject.slashQuery == nil)
    }

    @Test func replaceSlashTokenReturnsNilWithoutAToken() {
        let subject = document("hello")
        subject.refreshQueries()
        #expect(subject.replaceSlashToken(with: .skill(name: "x")) == nil)
    }

    @Test func insertPlacesTheChipAtTheGivenRange() {
        let subject = document("ab")
        subject.insert(.file(path: "README.md"), replacing: NSRange(location: 1, length: 0))

        #expect(subject.storage.length == 3)
        let stored = subject.storage.attribute(
            .attachment, at: 1, effectiveRange: nil) as? ComposerChipAttachment
        #expect(stored?.chip == .file(path: "README.md"))
    }

    @Test func insertRestoresDefaultTypingAttributes() {
        let subject = document()
        subject.insert(.skill(name: "x"), replacing: NSRange(location: 0, length: 0))
        #expect(subject.typingAttributes[.foregroundColor] as? NSColor == .textColor)
    }

    @Test func takeDraftReturnsTheParsedDraftAndEmptiesTheStorage() {
        let subject = document("hello")
        subject.insert(.file(path: "README.md"), replacing: NSRange(location: 5, length: 0))

        let draft = subject.takeDraft()

        #expect(draft.text == "hello")
        #expect(draft.mentionPaths == ["README.md"])
        #expect(subject.isEmpty)
        #expect(subject.storage.length == 0)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerDocumentTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: compile errors — `ComposerDocument` is undefined.

- [ ] **Step 3: Write the implementation**

Create `App/Chat/ComposerDocument.swift`:

```swift
import AppKit
import Observation

/// Owns the composer's draft as an `NSTextStorage`, which is the single source
/// of truth for both text and chips.
///
/// This replaces the `@Binding var text: String` the composer used before.
/// That binding is unusable once chips exist: `NSTextView.string` returns bare
/// `U+FFFC` characters for attachments, so the mirror comparison almost always
/// fails, and assigning `.string` wipes the whole attributed storage —
/// destroying every chip on any SwiftUI update.
///
/// SwiftUI reads the cheap derived values (`isEmpty`, `slashQuery`,
/// `mentionQuery`) that `refreshQueries` recomputes on each edit. The full walk
/// of the storage happens only in `takeDraft`, at send time.
@Observable
@MainActor
final class ComposerDocument {
    let storage = NSTextStorage()

    /// Mirrors the text view's `typingAttributes` so a chip insertion cannot
    /// leak attachment attributes into the text typed after it.
    var typingAttributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: NSFont.systemFontSize),
        .foregroundColor: NSColor.textColor,
    ]

    private(set) var slashQuery: String?
    private(set) var mentionQuery: String?
    var isFocused = false

    var isEmpty: Bool { storage.length == 0 }

    /// The leading `/token`, as an NSString range, when the draft is a single
    /// unbroken slash token. Whitespace anywhere in it means the user has
    /// moved on to arguments and the popup should be closed.
    private var slashTokenRange: NSRange? {
        let string = storage.string as NSString
        guard string.hasPrefix("/") else { return nil }
        let whitespace = string.rangeOfCharacter(from: .whitespacesAndNewlines)
        guard whitespace.location == NSNotFound else { return nil }
        return NSRange(location: 0, length: string.length)
    }

    func refreshQueries() {
        slashQuery = slashTokenRange.map {
            String((storage.string as NSString).substring(with: $0).dropFirst())
        }
        mentionQuery = Self.mentionToken(in: storage.string)
    }

    /// The active `@`-token runs from the last "@" to the end of the text, with
    /// no whitespace inside. Deliberately simple, matching the previous
    /// behaviour in `ChatComposerView.updateMentionQuery`.
    static func mentionToken(in text: String) -> String? {
        guard let at = text.lastIndex(of: "@") else { return nil }
        let token = String(text[text.index(after: at)...])
        guard !token.contains(where: \.isWhitespace) else { return nil }
        return token
    }

    /// Swaps the in-progress `/token` for a skill chip plus a trailing space.
    /// Returns the caret range to apply after the swap, or nil when there is
    /// no token to replace.
    func replaceSlashToken(with chip: ComposerChip) -> NSRange? {
        guard let range = slashTokenRange else { return nil }
        let replacement = NSMutableAttributedString(
            attachment: ComposerChipAttachment(chip: chip))
        replacement.append(NSAttributedString(string: " ",
                                              attributes: typingAttributes))
        storage.replaceCharacters(in: range, with: replacement)
        resetTypingAttributes()
        slashQuery = nil
        return NSRange(location: range.location + replacement.length, length: 0)
    }

    func insert(_ chip: ComposerChip, replacing range: NSRange) {
        storage.replaceCharacters(
            in: range,
            with: NSAttributedString(attachment: ComposerChipAttachment(chip: chip)))
        resetTypingAttributes()
    }

    /// Drains the draft. The returned triple is exactly what
    /// `ChatController.send(text:mentionPaths:images:)` takes, so nothing below
    /// the UI knows chips exist.
    func takeDraft() -> ComposerDraft {
        let draft = ComposerDraft.parse(storage)
        storage.setAttributedString(NSAttributedString(string: ""))
        slashQuery = nil
        mentionQuery = nil
        return draft
    }

    private func resetTypingAttributes() {
        typingAttributes = [
            .font: NSFont.systemFont(ofSize: NSFont.systemFontSize),
            .foregroundColor: NSColor.textColor,
        ]
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerDocumentTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: `Test run with 13 tests in 1 suite passed`.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ComposerDocument.swift AppTests/ComposerDocumentTests.swift project.yml
git commit -m "feat: add ComposerDocument owning the composer draft storage"
```

---

### Task 5: Point ChatTextEditor at the document, drop the String binding

**Files:**
- Modify: `App/Chat/ChatTextEditor.swift` (whole representable), `AppTests/ComposerTextKit2Tests.swift` (drop `text:` from the three call sites)
- Modify: `AppTests/ChatTextEditorSlashTests.swift` (delete the four accent-colour tests, drop `text:`)

**Interfaces:**
- Consumes: `ComposerDocument` (Task 4), `ChatTextEditor.makeTextView()` (Task 1).
- Produces: `ChatTextEditor(document:isEditable:minHeight:maxHeight:onSubmit:onSlashKey:)` — the `text` binding is gone. The coordinator gains `textDidBeginEditing`/`textDidEndEditing`, which drive `document.isFocused`.

- [ ] **Step 1: Write the failing test**

Add to `AppTests/ComposerTextKit2Tests.swift`, inside the suite:

```swift
    @Test func editingBeginAndEndDriveDocumentFocus() {
        let document = ComposerDocument()
        let coordinator = ChatTextEditor.Coordinator(
            ChatTextEditor(document: document, isEditable: true, minHeight: 36,
                           maxHeight: 160, onSubmit: {}, onSlashKey: nil))
        let textView = ChatTextEditor.makeTextView()

        coordinator.textDidBeginEditing(
            Notification(name: NSText.didBeginEditingNotification, object: textView))
        #expect(document.isFocused)

        coordinator.textDidEndEditing(
            Notification(name: NSText.didEndEditingNotification, object: textView))
        #expect(!document.isFocused)
    }

    @Test func textViewSharesTheDocumentStorage() {
        let document = ComposerDocument()
        document.storage.append(NSAttributedString(string: "seeded"))
        let editor = ChatTextEditor(document: document, isEditable: true, minHeight: 36,
                                    maxHeight: 160, onSubmit: {}, onSlashKey: nil)

        let textView = ChatTextEditor.makeTextView(document: document)
        _ = editor

        #expect(textView.string == "seeded")
        #expect(textView.textLayoutManager != nil)
    }
```

Also update the existing `makeCoordinator()` in that file to the new initializer:

```swift
    private func makeCoordinator() -> ChatTextEditor.Coordinator {
        ChatTextEditor.Coordinator(
            ChatTextEditor(document: ComposerDocument(), isEditable: true,
                           minHeight: 36, maxHeight: 160, onSubmit: {}, onSlashKey: nil))
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerTextKit2Tests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: compile errors — there is no `document:` parameter and no `makeTextView(document:)`.

**Risk to watch in this step.** `textViewSharesTheDocumentStorage` asserts that assigning `textContentStorage?.textStorage = document.storage` actually makes the document's storage the view's text. If that assertion fails once the code compiles, storage adoption is not supported the way this plan assumes. The fallback is to invert ownership: `ComposerDocument` stops creating its own `NSTextStorage` and instead holds `weak var storage: NSTextStorage?`, assigned from `makeNSView` with the view's existing storage. Every method in Task 4 keeps its signature; only the `storage` property changes, and the Task 4 tests would need a text view to supply one. Do not proceed past this step with a failing assertion.

- [ ] **Step 3: Rewrite the representable**

In `App/Chat/ChatTextEditor.swift`, replace everything from `struct ChatTextEditor: NSViewRepresentable {` through the closing brace of `updateNSView` — leaving `sizeThatFits`, `Coordinator`, and `AutoSizingScrollView` untouched — with:

```swift
struct ChatTextEditor: NSViewRepresentable {
    let document: ComposerDocument
    var isEditable: Bool
    var minHeight: CGFloat
    var maxHeight: CGFloat
    var onSubmit: () -> Void
    /// Returns true to consume the key (popup navigation); false restores
    /// the default behavior. nil behaves like always-false.
    var onSlashKey: ((SlashKey) -> Bool)? = nil

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    /// Builds the configured text view. Separate from `makeNSView` so tests
    /// can get a real text view without an `NSViewRepresentableContext`,
    /// which cannot be constructed outside SwiftUI. Mirrors the seam
    /// `AgentMarkdownTextView.makeTextView()` already uses.
    ///
    /// Created with the plain `NSTextView()` initializer, which is TextKit 2
    /// on macOS 15. `init(frame:textContainer:)` would opt into TextKit 1.
    ///
    /// When a document is supplied, its storage is adopted as the view's own,
    /// so the document and the view are literally the same text — no mirror,
    /// no sync, and nothing that can wipe an attachment.
    @MainActor
    static func makeTextView(document: ComposerDocument? = nil) -> NSTextView {
        let textView = NSTextView()
        if let document {
            textView.textContentStorage?.textStorage = document.storage
        }
        textView.font = .systemFont(ofSize: NSFont.systemFontSize)
        textView.isRichText = false
        textView.drawsBackground = false
        textView.textContainerInset = NSSize(width: 0, height: 4)
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.widthTracksTextView = true
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        return textView
    }

    func makeNSView(context: Context) -> AutoSizingScrollView {
        let textView = Self.makeTextView(document: document)
        textView.delegate = context.coordinator
        textView.isEditable = isEditable
        textView.typingAttributes = document.typingAttributes

        let scrollView = AutoSizingScrollView()
        scrollView.minHeight = minHeight
        scrollView.maxHeight = maxHeight
        scrollView.documentView = textView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        return scrollView
    }

    /// No text mirroring here on purpose: the document's storage *is* the text
    /// view's storage, so there is nothing to copy across, and the previous
    /// `textView.string = text` assignment would have erased every chip.
    func updateNSView(_ scrollView: AutoSizingScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = scrollView.documentView as? NSTextView else { return }
        textView.isEditable = isEditable
        textView.typingAttributes = document.typingAttributes
        scrollView.minHeight = minHeight
        scrollView.maxHeight = maxHeight
        context.coordinator.recalculateHeight(textView: textView, scrollView: scrollView)
    }
```

- [ ] **Step 4: Replace the highlight logic with query refresh and focus tracking**

In the same file's `Coordinator`, replace `textDidChange` and delete `applySlashHighlight` plus the `mutedAccentColor` static entirely (line numbers have shifted after Step 3 — go by symbol name), leaving:

```swift
        func textDidChange(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            parent.document.refreshQueries()
            parent.document.typingAttributes = textView.typingAttributes
            if let scrollView = textView.enclosingScrollView as? AutoSizingScrollView {
                recalculateHeight(textView: textView, scrollView: scrollView)
            }
        }

        /// For an editable text view these two are exactly the moments it gains
        /// and loses first-responder status, so the composer's focus ring can
        /// be driven from the delegate that already exists — no
        /// `window.firstResponder` observer to install or tear down.
        func textDidBeginEditing(_ notification: Notification) {
            parent.document.isFocused = true
        }

        func textDidEndEditing(_ notification: Notification) {
            parent.document.isFocused = false
        }
```

- [ ] **Step 5: Delete the four obsolete accent-colour tests**

In `AppTests/ChatTextEditorSlashTests.swift`, delete `slashTokenGetsMutedAccentColor` (lines 73-82), `tokenStaysHighlightedAfterTrailingArguments` (84-96), `nonSlashTextKeepsDefaultColor` (98-107), and `emptyTextDoesNotCrash` (109-115). They test `applySlashHighlight`, which no longer exists.

Update the file's `makeEditor` helper to the new initializer:

```swift
    private func makeEditor(
        onSlashKey: ((SlashKey) -> Bool)? = nil,
        onSubmit: @escaping () -> Void = {}
    ) -> ChatTextEditor {
        ChatTextEditor(
            document: ComposerDocument(), isEditable: true, minHeight: 36,
            maxHeight: 160, onSubmit: onSubmit, onSlashKey: onSlashKey)
    }
```

The five key-mapping tests (lines 19-71) stay unchanged.

- [ ] **Step 6: Run both suites to verify they pass**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerTextKit2Tests \
  -only-testing:TillerTests/ChatTextEditorSlashTests test 2>&1 \
  | grep -E "✔|✘|Test run|error:"
```

Expected: `ComposerTextKit2Tests` 5 tests pass, `ChatTextEditorSlashTests` 5 tests pass.

The app target will not compile yet — `ChatComposerView` still passes `text:`. Task 6 fixes that. If the build failure blocks the test run, do Task 6 before re-running and commit the two together.

- [ ] **Step 7: Commit**

```bash
git add App/Chat/ChatTextEditor.swift AppTests/ComposerTextKit2Tests.swift \
        AppTests/ChatTextEditorSlashTests.swift
git commit -m "refactor: drive ChatTextEditor from ComposerDocument instead of a String binding"
```

---

### Task 6: Wire ChatComposerView to the document

**Files:**
- Modify: `App/Chat/ChatComposerView.swift` — `@State private var text`/`mentionPaths`/`images` (lines 15-19), `editor` (70-87), `slashCandidates` (319-330), `handleSlashKey` (332-346), `slashPopup` (348-374), `attachmentChips` (378-395), `sendCurrent` (441-451), `updateMentionQuery` (455-474), `acceptMention` (476-483), `attachImage` (489-498)
- Test: `AppTests/ComposerSendTests.swift` (create)

**Interfaces:**
- Consumes: `ComposerDocument` (Task 4), `ChatTextEditor(document:…)` (Task 5), `ComposerChip` (Task 2).
- Produces: nothing new for later tasks — Task 7 only moves views around.

- [ ] **Step 1: Write the failing test**

Create `AppTests/ComposerSendTests.swift`:

```swift
import AppKit
import Testing
import TillerACP

@testable import Tiller

/// End-to-end over the draft path: chips go in, the triple that
/// `ChatController.send` accepts comes out, and the composer is left empty.
@Suite("ComposerSend")
@MainActor
struct ComposerSendTests {
    private let png = ImageAttachment(mimeType: "image/png", base64Data: "AAAA")

    @Test func acceptingASlashCommandLeavesAChipAndSendsTheCommand() {
        let document = ComposerDocument()
        document.storage.append(NSAttributedString(string: "/brain"))
        document.refreshQueries()

        _ = document.replaceSlashToken(with: .skill(name: "brainstorm"))
        document.storage.append(NSAttributedString(string: "an idea"))

        let draft = document.takeDraft()
        #expect(draft.text == "/brainstorm an idea")
        #expect(document.isEmpty)
    }

    @Test func acceptingAMentionRemovesTheAtTokenAndAddsAFileChip() {
        let document = ComposerDocument()
        document.storage.append(NSAttributedString(string: "look at @Chat"))
        document.refreshQueries()
        #expect(document.mentionQuery == "Chat")

        // The composer replaces the "@token" range with the chip.
        let at = (document.storage.string as NSString).range(of: "@Chat")
        document.insert(.file(path: "App/Chat/ChatTextEditor.swift"), replacing: at)

        let draft = document.takeDraft()
        #expect(draft.text == "look at ")
        #expect(draft.mentionPaths == ["App/Chat/ChatTextEditor.swift"])
    }

    @Test func attachingAnImageAddsAChipAndSendsTheAttachment() {
        let document = ComposerDocument()
        document.insert(.image(png), replacing: NSRange(location: 0, length: 0))

        let draft = document.takeDraft()
        #expect(draft.images == [png])
        #expect(draft.text.isEmpty)
    }

    @Test func draftWithOnlyAChipIsStillSendable() {
        let document = ComposerDocument()
        document.insert(.image(png), replacing: NSRange(location: 0, length: 0))
        #expect(!document.isEmpty)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerSendTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: a build failure from the app target, because `ChatComposerView` still passes `text:` to `ChatTextEditor`. That is the failure this task clears.

- [ ] **Step 3: Replace the composer's state**

In `App/Chat/ChatComposerView.swift`, replace the `text`, `mentionPaths`, `images`, `mentionQuery`, and `mentionCandidates` state properties with:

```swift
    @State private var document = ComposerDocument()
    @State private var mentionCandidates: [String] = []
```

Then replace `canSend` with:

```swift
    private var canSend: Bool { canInteract && !document.isEmpty }
```

- [ ] **Step 4: Rewire the editor, popups, and actions**

Replace `editor` with:

```swift
    private var editor: some View {
        ZStack(alignment: .topLeading) {
            if document.isEmpty {
                Text(editorPlaceholder)
                    .foregroundStyle(.secondary)
                    .allowsHitTesting(false)
            }
            ChatTextEditor(document: document, isEditable: canInteract, minHeight: 36,
                           maxHeight: 160, onSubmit: sendCurrent,
                           onSlashKey: handleSlashKey)
        }
        .disabled(!canInteract)
        .onChange(of: document.mentionQuery) { _, query in
            refreshMentionCandidates(query: query)
        }
        .onChange(of: document.slashQuery) {
            slashPopupDismissed = false
            slashSelectionIndex = 0
        }
    }
```

Replace `slashCandidates` with:

```swift
    /// Active while the draft is a single "/token": the query is what follows
    /// the slash, matched as a case-insensitive prefix of the command names.
    private var slashCandidates: [AvailableCommand] {
        guard canInteract, let query = document.slashQuery?.lowercased() else { return [] }
        let all = controller.availableCommands
        guard !query.isEmpty else { return Array(all.prefix(10)) }
        return Array(all.filter { $0.name.lowercased().hasPrefix(query) }.prefix(10))
    }
```

Replace the `.accepted` branch of `handleSlashKey` with:

```swift
        case .accepted(let index):
            acceptSlashCommand(slashCandidates[index])
```

Replace the button action inside `slashPopup` with:

```swift
                Button {
                    acceptSlashCommand(command)
                } label: {
```

Delete the `attachmentChips` property entirely and remove its call from `card` — attachments now live inline in the text. Also delete the `chip(label:systemImage:onRemove:)` helper, whose only callers were inside `attachmentChips`. (Task 7 rewrites `card` anyway; removing the call here just keeps this task compiling on its own.)

Replace `sendCurrent`, `updateMentionQuery`, `acceptMention`, and `attachImage` with:

```swift
    private func sendCurrent() {
        guard canInteract else { return }
        let draft = document.takeDraft()
        mentionCandidates = []
        controller.send(text: draft.text, mentionPaths: draft.mentionPaths,
                        images: draft.images)
    }

    private func acceptSlashCommand(_ command: AvailableCommand) {
        _ = document.replaceSlashToken(with: .skill(name: command.name))
    }

    private func refreshMentionCandidates(query: String?) {
        guard let query else {
            mentionCandidates = []
            return
        }
        let path = worktreePath
        Task.detached(priority: .userInitiated) {
            let hits = FileMentionIndex.candidates(
                worktreePath: path, query: query, limit: 8)
            await MainActor.run {
                if document.mentionQuery == query { mentionCandidates = hits }
            }
        }
    }

    /// Swaps the live "@token" for a file chip at the same position, so the
    /// chip lands where the user was typing.
    private func acceptMention(_ path: String) {
        guard let query = document.mentionQuery else { return }
        let range = (document.storage.string as NSString).range(of: "@\(query)",
                                                                options: .backwards)
        guard range.location != NSNotFound else { return }
        document.insert(.file(path: path), replacing: range)
        mentionCandidates = []
    }

    /// Opens a file picker for an image and inserts it as a chip at the caret.
    /// (Direct ⌘V clipboard-paste interception is a known gap — stray PNG data
    /// lingering on the general pasteboard from an unrelated copy would
    /// silently hijack every click of this button.)
    private func attachImage() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.png, .jpeg]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url,
              let data = try? Data(contentsOf: url) else { return }
        let mime = url.pathExtension.lowercased() == "png" ? "image/png" : "image/jpeg"
        let attachment = ImageAttachment(mimeType: mime,
                                         base64Data: data.base64EncodedString())
        document.insert(.image(attachment),
                        replacing: NSRange(location: document.storage.length, length: 0))
    }
```

Finally, in `body`, change the mention-popup guard from
`if let query = mentionQuery, !mentionCandidates.isEmpty` to
`if let query = document.mentionQuery, !mentionCandidates.isEmpty`. `mentionPopup(query:)` itself is unchanged.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerSendTests \
  -only-testing:TillerTests/ComposerTextKit2Tests \
  -only-testing:TillerTests/ChatTextEditorSlashTests test 2>&1 \
  | grep -E "✔|✘|Test run|error:"
```

Expected: 4 + 5 + 5 tests pass, no build errors.

- [ ] **Step 6: Commit**

```bash
git add App/Chat/ChatComposerView.swift AppTests/ComposerSendTests.swift project.yml
git commit -m "feat: insert composer attachments as inline chips"
```

---

### Task 7: AIR control bar layout and focus ring

**Files:**
- Create: `App/Chat/ComposerControlBar.swift`
- Modify: `App/Chat/ChatComposerView.swift` — rewrite `card`, delete `controlBar`, and move the twelve members listed in Step 3 out to the new file (line numbers are not given here because Task 6 already reshaped this file; go by symbol name)
- Test: `AppTests/ComposerControlBarTests.swift`

**Interfaces:**
- Consumes: `ComposerDocument.isFocused` (Task 4).
- Produces: `struct ComposerControlBar: View` with `init(controller: ChatController, document: ComposerDocument, onAttach: () -> Void, onSend: () -> Void, canSend: Bool, canInteract: Bool)`, and `static func ComposerControlBar.borderStyle(isFocused: Bool) -> (color: Color, width: CGFloat)`.

- [ ] **Step 1: Write the failing test**

Create `AppTests/ComposerControlBarTests.swift`:

```swift
import SwiftUI
import Testing

@testable import Tiller

@Suite("ComposerControlBar")
@MainActor
struct ComposerControlBarTests {
    @Test func focusedBorderUsesTheAccentColor() {
        let style = ComposerControlBar.borderStyle(isFocused: true)
        #expect(style.color == Color.accentColor)
        #expect(style.width == 1.5)
    }

    @Test func unfocusedBorderUsesTheSeparatorColor() {
        let style = ComposerControlBar.borderStyle(isFocused: false)
        #expect(style.color != Color.accentColor)
        #expect(style.width == 1)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerControlBarTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: compile error — `ComposerControlBar` is undefined.

- [ ] **Step 3: Extract the control bar into its own file with the AIR layout**

Create `App/Chat/ComposerControlBar.swift`. Move these members out of `ChatComposerView.swift` **verbatim** — they compile unchanged in the new type because they only read `controller`:

`statusDotColor`, `modePill`, `currentModeName`, `stateLabel`, `agentPill`, `currentModelName`, `contextUsageIndicator`, `effortLabel`, `agentDisplayName`, `sendButton`, `stopButton`, and the file-private `PillBackground` struct.

Also move `@State private var modelPickerShown = false` — `agentPill` presents its popover from it, so leaving it behind breaks the move. `ChatComposerView` has no other use for it.

Then wrap them in this layout:

```swift
import SwiftUI
import AppKit
import TillerACP
import TillerAgents

/// The composer's bottom row, in the AIR arrangement: attach on the left with
/// the model picker, and the secondary controls collapsed into an overflow
/// menu on the right so the row carries 5 elements instead of 8.
struct ComposerControlBar: View {
    let controller: ChatController
    let document: ComposerDocument
    let onAttach: () -> Void
    let onSend: () -> Void
    let canSend: Bool
    let canInteract: Bool

    @State private var modelPickerShown = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// The composer card's border: accent while the text view holds focus,
    /// separator otherwise.
    static func borderStyle(isFocused: Bool) -> (color: Color, width: CGFloat) {
        isFocused ? (.accentColor, 1.5) : (Color(nsColor: .separatorColor), 1)
    }

    var body: some View {
        HStack(spacing: 8) {
            Button(action: onAttach) {
                Image(systemName: "plus")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Attach image")
            .disabled(!canInteract)

            agentPill
            Spacer()
            overflowMenu
            contextUsageIndicator
            modePill
            if controller.state == .prompting {
                stopButton
            } else {
                sendButton
            }
        }
    }

    private var overflowMenu: some View {
        Menu {
            Button(controller.isFollowing ? "Stop Following" : "Follow Edited Files") {
                controller.isFollowing.toggle()
            }
            Button("New Conversation") {
                Task { await controller.newConversation() }
            }
        } label: {
            Image(systemName: "ellipsis")
                .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    // The twelve members listed above, pasted here unchanged.
}
```

`sendButton` changes from a circle to a labelled rectangle:

```swift
    private var sendButton: some View {
        Button(action: onSend) {
            Text("Send")
                .font(.caption.weight(.semibold))
                .foregroundStyle(canSend ? Color.white : Color.secondary)
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(canSend ? AnyShapeStyle(Color.accentColor)
                                    : AnyShapeStyle(.quaternary),
                            in: RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.return, modifiers: [])
        .disabled(!canSend)
    }
```

`stopButton` keeps its slot and gains the matching shape:

```swift
    private var stopButton: some View {
        Button {
            Task { await controller.cancelTurn() }
        } label: {
            Image(systemName: "stop.fill")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(.white)
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(Color.red.opacity(0.8), in: RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .keyboardShortcut(.escape, modifiers: [])
        .help("Stop the turn")
    }
```

- [ ] **Step 4: Apply the focus ring in the card**

In `App/Chat/ChatComposerView.swift`, replace `card` with:

```swift
    private var card: some View {
        let border = ComposerControlBar.borderStyle(isFocused: document.isFocused)
        return VStack(alignment: .leading, spacing: 8) {
            editor
            ComposerControlBar(
                controller: controller, document: document, onAttach: attachImage,
                onSend: sendCurrent, canSend: canSend, canInteract: canInteract)
        }
        .padding(12)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14)
            .strokeBorder(border.color, lineWidth: border.width))
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.15), value: document.isFocused)
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  -only-testing:TillerTests/ComposerControlBarTests test 2>&1 | grep -E "✔|✘|Test run|error:"
```

Expected: `Test run with 2 tests in 1 suite passed`.

- [ ] **Step 6: Commit**

```bash
git add App/Chat/ComposerControlBar.swift App/Chat/ChatComposerView.swift \
        AppTests/ComposerControlBarTests.swift project.yml
git commit -m "feat: rearrange composer controls and add a focus ring"
```

---

### Task 8: Full verification gate

**Files:** none modified — this task only runs checks and records results.

- [ ] **Step 1: Run the whole app test target**

`Scripts/ci.sh` compiles `TillerTests` but never executes it, so this run is not covered by the CI gate and must be done explicitly.

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -derivedDataPath DerivedData \
  test 2>&1 | grep -E "✘|Test run|error:"
```

Expected: a single `Test run with N tests … passed` line and no `✘`.

- [ ] **Step 2: Run the repo CI gate**

```bash
Scripts/ci.sh 2>&1 | tail -20
```

Expected: `CI OK`. If `spawnCapturesOutput` in `Packages/TillerTerminal` fails, re-run — it is known-flaky and can need 5-6 attempts. Any *other* failure is a real regression from this work.

- [ ] **Step 3: Build and run the app for the manual checks**

```bash
Scripts/build-dev.sh
```

Then open a chat pane and confirm each item. These are the behaviours no automated test covers:

1. Typing `/` opens the popup; accepting a command replaces the text with a **box containing an icon and the command name**, not blue text.
2. Pressing backspace with the caret right after a chip deletes the **whole chip**, not one character.
3. `⌘V` of formatted text copied from Safari brings **no fonts or colours** into the composer.
4. `@`-mentioning a file, then accepting it, leaves a file chip **at the position where you were typing**.
5. The `+` button attaches an image as a chip; sending it delivers the image (the agent describes it).
6. The card border turns **accent** when you click into the text and **separator** when you click away.
7. Follow and New Conversation are both reachable from the `⋯` menu and still work.

- [ ] **Step 4: Record the outcome**

If every check passes, commit nothing (no files changed) and report done. If any check fails, write the failure down with the exact observed behaviour before attempting a fix — a chip that does not appear at all is the TextKit 1 fallback signature, and the first thing to check is whether anything added a `.layoutManager` read.

## Self-Review

**Spec coverage:**

| Spec section | Task |
|---|---|
| TextKit 2 migration | 1 |
| Chip data model (three kinds, icons, labels) | 2 |
| `ComposerDraft` parse / skill re-serialization | 2 |
| Attachment + view provider rendering | 3 |
| `isRichText` open risk | 3, Step 2 |
| Storage as source of truth / `ComposerDocument` | 4 |
| Drop the `String` binding | 5 |
| Focus signal from editing delegates | 5 |
| Insertion semantics (skill replaces token, others at caret) | 4, 6 |
| Attachments inline instead of the chip row | 6 |
| Control bar layout | 7 |
| Focus ring | 7 |
| Deleted accent-colour tests | 5, Step 5 |
| Manual checklist | 8 |
