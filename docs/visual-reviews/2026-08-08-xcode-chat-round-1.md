# Xcode chat parity — visual review rounds 1–8

Date: 2026-08-08  
Theme: dark  
Fixture canvas: 1520 × 1720 px (2× ImageRenderer output)

## Evidence

The component fixture is rendered by `AppTests/ChatRowChromeTests.swift` and
keeps the existing markdown/TextKit renderer outside the visual chrome. The
latest full fixture and independently cropped pieces are in:

- `artifacts/chat-visual-round-2/sirio-components.png`
- `artifacts/chat-visual-round-2/thinking-row.png`
- `artifacts/chat-visual-round-2/thinking-expanded.png`
- `artifacts/chat-visual-round-2/tool-call-row.png`
- `artifacts/chat-visual-round-2/message-row.png`
- `artifacts/chat-visual-round-2/inline-diff.png`
- `artifacts/chat-visual-round-2/hover-row.png`

The Xcode references are the official dark Coding Assistant screenshots from
[Writing code with intelligence in Xcode](https://developer.apple.com/documentation/xcode/writing-code-with-intelligence-in-xcode?changes=_7):

- `artifacts/chat-visual-round-1/xcode-reference/coding-assistant-write-code-dark@2x.png`
- `artifacts/chat-visual-round-1/xcode-reference/coding-assistant-propose-code-dark@2x.png`
- `artifacts/chat-visual-round-1/xcode-reference/xcode-write-code-window-1520x1720.png`
- `artifacts/chat-visual-round-1/xcode-reference/xcode-propose-code-window-1520x1720.png`

The last two are letterboxed onto the same 1520 × 1720 comparison canvas as
the Sirio fixture. They are reference material, not a claim of a live Xcode
window capture.

The latest Xcode reference copies and the synchronized letterboxed canvases are
in `artifacts/chat-visual-round-2/xcode-reference/`.

## Blind critic loops

Each critic received only the objective, the relevant dark Xcode reference,
and the Sirio image. No critic was given the implementation reasoning.

| Piece | Round 1 verdict | Largest gap | Correction | Final round |
| --- | --- | --- | --- | --- |
| Thinking | Xcode initially won | Status read as a flat banner or telemetry line | Collapsible inset surface, compact phase label, live dots, bounded payload preview with a progress rail | Final loop still favors Xcode's richer assistant context; Sirio's block is now inspectable and coherent |
| Tool calls | Sirio won on the focused criterion | Xcode has no discrete tool-call list in the supplied reference | Shared row surface, status badge, line-count badge, and expandable content | Sirio wins scanability; Xcode is not a direct list reference |
| Inline diff | Sirio won | Diff initially read as a raw code gutter block | Proposal label, file identity, contextual lines, inset code slab, +/- gutter stripes, softened depth | Sirio won the final blind review |
| Message + timestamp | Xcode initially won | Flat slab and cramped footer | Inset assistant surface, bounded reading measure, divider before metadata, timestamp/copy footer with extra breathing room | Final loop is materially closer; the timestamp remains explicit and subordinate |
| Active/hover | Sirio coherent; Xcode not a direct reference | Active and hover could merge or over-announce | Separate active/hover fills, softer border/rail/shadow, lower contrast state hint | Coherent with Sirio's own dark row language; judge this piece internally |

Xcode is not used as a reference for Sirio-only worktree/session-tab UI; this
round covers only the shared transcript language.

## Verification boundary

The user granted Screen Recording during the run. An escalated full-display
`screencapture` now writes a 3440 × 1440 PNG, but the pixels are entirely black;
direct Xcode-window and region captures still fail with `could not create image
from window/rect`. The Computer Use path is also unavailable because neither
`orca` nor `orca-ide` is installed in this environment. A fresh usable live
Xcode/Sirio pair therefore remains unproven; the artifacts above are a
reproducible Sirio fixture plus official dark Xcode references, with that
boundary stated explicitly. No Tree-sitter code or NSTextView/TextKit 2
performance path was changed.

The non-escalated process still reports Screen Capture preflight `false`; the
escalated capture proves the permission request reaches macOS but not usable
window pixels in this headless/remote display session. Xcode was running, but
its off-screen window could not be rendered into a screenshot.

## Test evidence

- `xcodebuild ... test -only-testing:SirioTests/ChatRowChromeTests -only-testing:SirioTests/ChatDiffPreviewTests`: passed (6 Swift Testing tests; collapsed and expanded Thinking fixtures rendered after the final Thinking/message adjustments).
- `swift test --package-path Packages/SirioACP`: passed, 296 tests in 42 suites.
- `Scripts/ci.sh`: project generation, boundary checks, `sirioctl` build, and
  Sirio build passed; the full app-test runner was interrupted after 312.55s
  with no `xctest` child progress in the headless environment. Its existing
  CodeEditTextView/CodeEditSourceEditor SwiftLint scripts also report failures.
