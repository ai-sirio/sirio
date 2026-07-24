# Composer: AIR-style inline chips and TextKit 2

Date: 2026-07-24
Status: approved

## Problem

Two gaps against the reference UI (AIR):

1. A slash command typed in the chat composer renders as blue-tinted text
   (`ChatTextEditor.applySlashHighlight`). It should render as a chip — a
   rounded box with an icon and the command name — and behave as one atomic
   token.
2. Attachments (`@`-mentioned files, images) render as a separate chip row
   above the editor. They should be inline in the text, at the caret.

Plus two smaller items in the same surface: the control bar layout, and a
focus ring on the composer card.

## Scope

Composer only. A follow-up spec covers compacting tool/event rows in the
transcript — that touches the area of the 2026-07-23 timeline streaming
freeze and needs its own anti-freeze test, so it is deliberately not bundled
here.

## Decisions

| Question | Decision |
|---|---|
| Chip model | Atomic token (`NSTextAttachment`), not text styling |
| Chip trigger | Popup acceptance only — typing `/foo` by hand stays plain text |
| Attachment chips | Real inline, inserted at the caret |
| Secondary controls | Follow and new-conversation move into a `⋯` overflow menu |
| TextKit version | Migrate the composer to TextKit 2 |

## TextKit 1 audit

Three app files use TextKit 1. They form two independent islands, because
**the TextKit version is per-`NSTextView`, not per-app** — the composer and
the transcript renderer are separate representables and can run different
versions in the same window.

**Island 1 — composer (migrates):**
- `App/Chat/ChatTextEditor.swift:152-154` — `layoutManager.ensureLayout` +
  `usedRect(for:)` in `recalculateHeight`. The only TextKit 1 touch point in
  the whole composer.

**Island 2 — transcript markdown renderer (unchanged):**
- `App/Chat/AgentMarkdownTextView.swift:70-77` — explicit TextKit 1 stack
  (`NSTextStorage` + `CodeBlockLayoutManager` + `NSTextContainer`)
- `App/Chat/AgentMarkdownTextView.swift:38-51` — `layoutCodeBlockHeaders` via
  `glyphRange` + `boundingRect`
- `App/Chat/AgentMarkdownTextView.swift:128-139` — `sizeThatFits`, with a
  cache that exists because `ensureLayout` caused multi-second main-thread
  hangs under `LazyVStack`
- `App/Chat/CodeBlockLayoutManager.swift` — `NSLayoutManager` subclass

Migrating island 2 would be a project of its own. It is out of scope.

### The migration

`NSTextView()` created without an explicit container is already TextKit 2 on
macOS 15. It currently falls back to TextKit 1 only because
`recalculateHeight` reads `.layoutManager`. Reading that property is the
fallback trigger.

```swift
// before
guard let layoutManager = textView.layoutManager,
      let container = textView.textContainer else { return }
layoutManager.ensureLayout(for: container)
let used = layoutManager.usedRect(for: container).height
         + textView.textContainerInset.height * 2

// after
guard let tlm = textView.textLayoutManager else { return }
tlm.ensureLayout(for: tlm.documentRange)
let used = tlm.usageBoundsForTextContainer.height
         + textView.textContainerInset.height * 2
```

`usageBoundsForTextContainer` is the direct analogue of `usedRect(for:)`.

## Design

### Chip data model

Three chip kinds, one mechanism. Each occupies a single `U+FFFC` character in
the text storage.

| Chip | Payload | Origin |
|---|---|---|
| Skill | command name | slash popup acceptance |
| File | path | `@` popup acceptance |
| Image | `ImageAttachment` | `+` button / file picker |

`ComposerChip` is an enum carrying the payload.
`ComposerChipAttachment: NSTextAttachment` transports it in the text and
overrides `viewProvider(for:location:textContainer:)` to return a provider
whose `loadView()` mounts `NSHostingView<ComposerChipView>`. No
`registerViewProviderClass` — that registry is process-global, and the chip
is a composer detail, not a document type.

Atomicity comes free: AppKit already treats `U+FFFC` as indivisible for
caret movement, selection, and backspace.

Icons are fixed per kind, since `AvailableCommand` carries no icon metadata:
`cube` for skill, the existing `doc` for file, `photo` for image — matching
the symbols `ChatComposerView.attachmentChips` uses today.

### The text storage is the source of truth

`mentionPaths` and `images` state in `ChatComposerView` is removed.

This is forced, not stylistic. `ChatTextEditor` currently mirrors a
`@Binding var text: String` and does `if textView.string != text {
textView.string = text }`. With attachments present, `textView.string`
returns bare `U+FFFC` characters, the comparison almost always fails, and
assigning `.string` **wipes the entire attributed storage** — chips
destroyed on every SwiftUI update. Not an edge case: it fires on the first
`@State` change in the composer.

`ChatComposerView` therefore passes a `ComposerDocument` (`@Observable`,
owner of the `NSTextStorage`) instead of a string. SwiftUI no longer mirrors
the text; it asks the document:

- `isEmpty` — for `canSend`, without parsing
- `slashQuery: String?` — the in-progress `/…` token, for the popup
- `mentionQuery: String?` — same for `@`
- `insert(_ chip: ComposerChip)` — inserts at the caret
- `takeDraft() -> ComposerDraft` — drains and returns text + attachments

The first three recompute in `textDidChange` (which already exists). The
full walk happens only in `takeDraft`.

`ComposerDraft` produces exactly the triple
`(text: String, mentionPaths: [String], images: [ImageAttachment])` that
`controller.send(text:mentionPaths:images:)` accepts today. The skill chip
re-serializes as `/name ` at the head of the text. **No wire protocol change,
no DB migration** — chips live only in the draft and are never persisted.

### Insertion semantics

- Skill chip replaces the `/query` range at the head and appends a space.
- File/image chip inserts at the caret.
- After every insertion, `typingAttributes` is reset to the system font and
  `.textColor`, or following text inherits the attachment's attributes.

Because an accepted skill command becomes a chip, the text no longer starts
with `/` and the slash popup closes on its own — no extra dismissal state.

### Open risk: `isRichText`

The composer sets `isRichText = false`. Whether a *programmatically*
inserted attachment still renders with that flag off is unverified — the
documentation frames `isRichText` as a constraint on the user, not on the
storage, but this is not assumed. Task 1 verifies it empirically before
anything is built on top.

If `isRichText = true` turns out to be required, it opens rich-text pasting.
Mitigation: force `pasteAsPlainText` and set `importsGraphics = false`.

### Control bar

From 8 elements in one row to 5 visible:

```
[+]  [model ⌄]                    [⋯]  (◐)  [● mode ⌄]  [ Send ]
```

- Left: `+` attach (was `paperclip`, right), model pill
- Right: `⋯` overflow, context ring, mode pill with status dot, Send/Stop
- Inside `⋯`: follow (eye) and new conversation, as labelled menu items

Send becomes a rectangular labelled button instead of a circle with an
arrow. Stop keeps the same slot when `isPrompting` so the button does not
jump.

### Focus ring

`ChatComposerView:59` already overlays
`RoundedRectangle.strokeBorder(.separator.opacity(0.5), lineWidth: 1)`. It
becomes conditional: accent at `lineWidth: 1.5` while the text view is first
responder, separator at `1` otherwise.

The signal comes from `textDidBeginEditing(_:)` / `textDidEndEditing(_:)` —
two `NSTextViewDelegate` methods writing to `document.isFocused`. No
`firstResponder` KVO, no observers to tear down, no retain cycle on the
window. For an editable text view `textDidBeginEditing` *is* the moment it
becomes first responder; when `isEditable` is false those callbacks do not
fire, which is also when the ring should stay off — the behaviours coincide.

The transition respects `accessibilityReduceMotion` (already in
`@Environment` at `ChatComposerView:23`).

## Files

New:

| File | Contents |
|---|---|
| `App/Chat/ComposerChip.swift` | `ComposerChip` enum, `ComposerDraft`, storage parsing |
| `App/Chat/ComposerChipAttachment.swift` | attachment, view provider, `ComposerChipView` |
| `App/Chat/ComposerDocument.swift` | `@Observable` storage owner, slash/`@` queries, `isFocused` |
| `App/Chat/ComposerControlBar.swift` | control bar extracted from `ChatComposerView` |

Modified: `ChatTextEditor.swift` (TextKit 2, binding → document, extract
`static makeTextView()`), `ChatComposerView.swift` (shrinks).

New files are added because `ChatComposerView.swift` is already 509 lines
against a 400-typical / 800-max project convention.

## Testing

The single most valuable test:

```swift
@Test func composerUsesTextKit2() {
    #expect(ChatTextEditor.makeTextView().textLayoutManager != nil)
}
```

This requires extracting text view construction out of `makeNSView` into a
`static func makeTextView() -> NSTextView`, the same seam
`AgentMarkdownTextView.makeTextView()` (line 69) already uses — an
`NSViewRepresentableContext` cannot be constructed from a test.

A TextKit 1 fallback produces no error, no warning, and no crash. If someone
later adds an innocuous `textView.layoutManager?.…`, the chips silently stop
appearing while the code still compiles and runs. This one-line test catches
exactly the regression a rendering test would miss.

Others:

- `ComposerDraft.parse` — storage with skill chip + 2 files + 1 image + text
  produces the correct triple; skill chip re-serializes as `/name `; empty
  storage produces an empty triple
- `ComposerDocument.slashQuery` / `mentionQuery` — correct derivation, `nil`
  when the token contains whitespace
- `insert(_:)` — inserts at the caret, restores `typingAttributes`
- height under TextKit 2 — grows with more lines, clamps at `maxHeight`
- `isFocused` — toggles on `textDidBeginEditing` / `textDidEndEditing`

Deleted: the 4 accent-colour tests in `AppTests/ChatTextEditorSlashTests.swift`
(lines 73, 84, 98, 109), which die with `applySlashHighlight`. The 5
key-mapping tests stay valid.

Gate: `Scripts/ci.sh` must print `CI OK`. The `spawnCapturesOutput` PTY test
in TillerTerminal is known-flaky and may need several retries; that is not a
regression from this work.

## Manual checklist

Things no automated test covers:

1. The skill chip renders as a box with an icon, not blue text
2. Backspace on a chip deletes the whole chip, not one character
3. `⌘V` of formatted text from Safari brings no fonts or colours into the
   composer
4. The ring turns accent on focus, separator on blur
5. Follow and new-conversation are reachable from `⋯`
