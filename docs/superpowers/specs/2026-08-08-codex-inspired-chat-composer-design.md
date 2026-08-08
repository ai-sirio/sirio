# Codex-inspired chat composer

**Date:** 2026-08-08
**Status:** Approved

## Goal

Restyle Tiller's user-input area as a single, centered, airy card inspired by
the Codex composer shown in the supplied reference. Preserve Tiller's existing
chat capabilities and blue focus/processing ring while removing the visual
split between the transcript and the input area.

This change is limited to the chat composer. The transcript redesign currently
in progress, the global palette, side panels, terminal panes, and workspace
split/divider behavior are outside this scope.

## Approved visual contract

### Relationship with the transcript

- Remove the explicit horizontal `Divider()` immediately above the composer in
  `ChatPaneView`.
- Keep the transcript, banners, pending-question bar, and composer in the same
  vertical flow. Use spacing rather than a line to separate the composer from
  the content above it.
- Do not change `NSSplitView`, workspace split dividers, resize cursors, pane
  hit-testing, or drop-target geometry. The removed divider is only the SwiftUI
  separator between the chat transcript and `ChatComposerView`.

### Composer geometry

- Center the composer horizontally inside the chat pane.
- Use a preferred width of 84% of the available chat-pane width, capped at
  1,440 pt, with a minimum 16 pt inset on both sides. Left and right insets must
  be equal at every supported pane width. The deterministic rule is
  `min(availableWidth * 0.84, 1_440, availableWidth - 32)` with the result
  clamped to a non-negative width.
- Replace the current nested outer-card/black-field composition with one rounded
  card. The text editor is transparent and shares the card surface with the
  bottom controls.
- Use a 22 pt card corner radius.
- Give the editor a 104 pt initial minimum height and allow it to grow to 180 pt
  before its existing internal scrolling behavior takes over.
- Keep the controls anchored to the bottom of the card. Text growth must not
  reorder the controls or change their horizontal alignment.

### Surface and border

- Reuse `AppTheme.cardFill` for the unified card surface. Do not add a sampled
  Codex-specific color and do not change the global palette.
- Stop applying `AppTheme.composerFieldFill` inside this composer. The token may
  remain available for other consumers.
- Preserve the existing blue `ComposerBorderView` identity: subdued blue at
  rest, stronger blue on focus, and the current animated blue accent while the
  agent is processing.
- Update the border mask to the approved 22 pt corner radius without changing
  its focus, processing, or Reduce Motion semantics.

## Control layout and behavior

The control row adopts Codex's hierarchy while retaining Tiller's capabilities:

- **Leading group:** attachment button, then permission/mode control.
- **Trailing group:** overflow menu, context-usage ring, model/effort control,
  then the primary action.
- Do not add a microphone or any other capability that Tiller does not already
  provide.
- Keep file attachments, model selection, effort selection, permission modes,
  context usage, follow mode, new conversation, and overflow actions wired to
  their existing controller/document paths.

Replace the text `Send` control with a 30 x 30 pt circular button:

- when sending is unavailable, use a subdued `arrow.up` icon. The local
  inactive/loading circle fill is
  `Color.secondary.opacity(0.18)`, which remains visible against the unified
  `AppTheme.cardFill` surface without adding a palette token;
- when sending is available, use the accent fill with a high-contrast
  upward-arrow icon;
- while connecting, show the existing spinner in the same 30 x 30 pt circle;
- while prompting, show the existing Stop action in the same 30 x 30 pt circle;
- state changes must not shift the surrounding controls.

The icon-only primary actions must expose appropriate accessibility labels and
help text (`Send`, `Starting the agent`, and `Stop the turn`). Existing keyboard
behavior remains unchanged: Return sends, Shift-Return inserts a newline, and
Escape stops an active turn.

## Component boundaries and data flow

### `ChatPaneView`

Owns the transcript-to-composer relationship. It removes only the local
`Divider()` and preserves banners, the pending-question bar, layout capture,
drop handling, controller activation, and the main chat surface.

### `ChatComposerView`

Owns the centered responsive container, unified surface, editor height, corner
radius, and composer border. It continues to own slash-command presentation,
file-mention presentation, queued drafts, attachments, and submission wiring.

The width calculation should live in a small deterministic layout helper so it
can be tested independently of SwiftUI rendering. The helper receives the
available pane width and returns a width that satisfies the 84%, 1,440 pt cap,
and 16 pt minimum-inset rules using the formula in the visual contract.

### `ComposerControlBar`

Owns the approved leading/trailing grouping and the fixed-size circular
Send/Loading/Stop controls. Its existing controller and document inputs remain
the public boundary; no new chat state is introduced.

### Existing state owners

`ComposerDocument` continues to own draft contents, focus, mentions, and chips.
`ChatController` continues to own connection, prompt, permission, model, effort,
usage, queue, and cancellation state. The visual change adds no persistence,
migration, network request, or new error path.

## State and exceptional conditions

- Authentication, disconnect, prompt-error, MCP-warning, plan-approval, and
  pending-question UI remains above the composer.
- A pending permission continues to disable input and file drops exactly as it
  does today.
- Queuing while the agent is prompting remains unchanged.
- Reduce Motion continues to replace the processing animation with its existing
  static treatment.
- At narrow supported widths, the composer keeps equal 16 pt-or-greater side
  insets. Existing truncation and overflow behavior may compress labels, but
  controls must not overlap or leave the card bounds.
- Light mode uses the existing appearance-aware `AppTheme.cardFill`; no new
  light-mode palette is introduced.

## Testing strategy

Follow the repository's test-first convention with `swift-testing`.

1. Add deterministic layout-helper tests covering:
   - 84% sizing below the maximum;
   - the 1,440 pt maximum width;
   - the 16 pt minimum-inset fallback at narrow widths;
   - equal derived left and right insets.
2. Extend the existing chat-pane layout probe at representative wide and narrow
   host sizes to verify that the composer frame is centered and remains inside
   the pane.
3. Preserve and extend `ComposerControlBarTests` for Send, Loading, and Stop
   state selection. Visual shape, icon contrast, and non-shifting geometry are
   confirmed by the manual checklist because the repository does not have a
   pixel-snapshot harness for these SwiftUI controls.
4. Run the existing composer behavior tests for submission, queueing, chips,
   attachments, slash commands, mentions, focus, and permission blocking.
5. Do not run `Scripts/ci.sh` or an equivalent all-repository gate. The user
   explicitly prohibited that command after approving this design. Run only the
   focused composer/chat suites and the app build named in the implementation
   plan, and report this narrower verification boundary.
6. Inspect the built app manually in dark and light appearances at wide and
   narrow pane widths. Compare the dark appearance with the supplied Codex
   reference and verify the unified surface, equal margins, increased height,
   control order, circular actions, blue focus ring, processing animation, and
   absence of the transcript/composer divider.

Static tests and parser/build checks do not constitute visual approval. If a
live app capture cannot be produced, report that limitation rather than claiming
pixel-level verification.

## Acceptance criteria

- No horizontal line separates the transcript from the composer.
- The composer is centered with equal left and right space and follows the
  approved 84%, 1,440 pt cap, and 16 pt minimum-inset rules.
- The composer is one 22 pt-radius surface; there is no black inset text field.
- The editor starts at 104 pt and grows to 180 pt before scrolling.
- The existing blue ring remains visible and retains focus, processing, and
  Reduce Motion behavior.
- Controls follow the approved leading and trailing groups.
- Send, Loading, and Stop occupy the same 30 x 30 pt circular frame and do not
  shift adjacent controls.
- All existing composer capabilities and keyboard interactions still work.
- Banner, permission, queue, attachment, drop, and error behavior is unchanged.
- Dark and light appearances remain legible, with no global palette change.
- Existing local work and the transcript redesign remain untouched.
- The focused composer/chat suites and app build named in the implementation
  plan pass, followed by successful manual visual inspection or an explicit
  report that live visual verification was blocked. No `Scripts/ci.sh` or
  equivalent all-repository gate is run.

## Out of scope

- Adding voice input or a microphone control.
- Copying Codex-specific product labels or unavailable controls.
- Changing transcript rows, chat message cards, or the in-progress Xcode-parity
  transcript work.
- Changing global colors, terminal colors, sidebar surfaces, or opacity.
- Changing workspace split dividers, cursor hit areas, pane resizing, or drop
  behavior.
- Refactoring `ComposerDocument`, `ChatController`, or persistence.
