# Compact Floating Chat Composer Design

**Date:** 2026-08-09

**Status:** Approved

**Supersedes:** only the vertical-density and transcript-layout portions of `2026-08-08-codex-inspired-chat-composer-design.md`

## Context

The first Codex-inspired composer iteration correctly introduced a centered unified card, shared surface styling, a persistent blue ring, and compact circular controls. Live visual review showed that the 104-point minimum editor height was too airy for Tiller and that the transcript still behaved like a separate full-width region above the composer.

The approved follow-up keeps the established interaction and control design while making the composer compact and treating it as a floating surface over a transcript column with the same width.

## Goals

- Reduce the composer's vertical footprint without making multiline input cramped.
- Give transcript content and the composer one shared centered column.
- Let messages scroll behind the floating composer, with a soft Codex-like disappearance before they reach the card.
- Keep the last message fully reachable above the composer regardless of the composer's current height.
- Preserve the blue focus/processing ring, permission flow, autoscroll behavior, drag-and-drop, chips, slash commands, mentions, controls, and keyboard behavior.

## Non-goals

- No change to the approved 84% / 1440-point responsive width formula.
- No change to the order, actions, icons, or 30-point footprint of composer controls.
- No microphone.
- No transcript typography, message-card, Markdown, timeline, or grouping redesign.
- No change to the overall chat background color.
- No full-repository verification gate and no `Scripts/ci.sh` execution.

## Visual Contract

### Compact composer

- The editor's minimum height is **56 points**.
- The editor's maximum height is **128 points**.
- The composer remains a single rounded card with a **22-point corner radius**.
- The composer card surface is the fixed color **`#20232D`**.
- `#20232D` applies only to the composer surface; it does not replace the chat background.
- The existing blue focus ring and processing animation remain unchanged apart from following the existing 22-point card shape.

### Shared centered column

Transcript content and the composer use the same `ComposerLayoutMetrics` contract:

- preferred width: **84%** of available pane width;
- maximum width: **1440 points**;
- minimum horizontal inset: **16 points**;
- equal left and right insets.

The shared width applies to the transcript's content stack, not to the scroll view itself. The scroll view continues to occupy the full pane so its scrollbar and scroll gestures retain their current geometry.

### Floating overlay and fade

- The transcript occupies the full remaining chat-pane height.
- The bottom UI is rendered as an overlay above the transcript rather than consuming a separate row below it.
- The overlay stack retains the current order: transient bottom banners and permission UI above the composer, composer last.
- A non-interactive bottom gradient sits between transcript content and the bottom overlay.
- The fade begins approximately **36 points above** the measured overlay and resolves into the existing chat background color.
- The fade must not use `#20232D`; it blends into the chat surface so messages disappear naturally before passing behind the composer card.
- The composer itself stays fully opaque.

## Layout and Data Flow

`ChatPaneView` owns the overlay composition and the measured bottom-overlay height:

1. Top authentication/disconnection banners keep their current place above the chat body.
2. The chat body becomes a bottom-aligned overlay container.
3. `TranscriptView` remains the full-size scrollable base layer.
4. The gradient is drawn above the transcript and below the interactive bottom overlay, with hit testing disabled.
5. The bottom overlay contains the existing bottom banners, `PendingQuestionBar`, and centered `ChatComposerView` in their current semantic order.
6. A geometry preference reports the overlay's current height to `ChatPaneView`.
7. `ChatPaneView` passes that height plus **16 points** of clearance to `TranscriptView` as a bottom content inset.
8. `TranscriptView` adds the inset after its bottom anchor inside the lazy content, allowing the last real message to scroll fully above the overlay while earlier messages can visibly travel behind it.

The height measurement must respond to editor growth, permission prompts, transient banners, slash/mention popups, and state changes without introducing a live scroll-geometry observation loop.

## Interaction and Accessibility

- The bottom gradient uses `.allowsHitTesting(false)`.
- Transcript scrolling remains available across the full pane, including the region behind the gradient when not covered by the interactive card.
- The composer and permission controls retain normal hit testing and accessibility order.
- The transcript remains earlier than the bottom overlay in accessibility reading order.
- Reduce Motion does not alter the fade because it is static; existing processing-ring Reduce Motion behavior is preserved.
- Drop targeting remains pane-wide and unchanged.

## Testing Strategy

Focused tests must cover the new geometry without observing live scroll geometry in production:

- Extend the chat-pane layout capture roles to measure transcript content, transcript viewport, bottom overlay, approval UI, and composer as needed.
- At 640- and 2000-point pane widths, assert that transcript content and composer have the same calculated width and symmetric horizontal insets.
- Assert that the transcript viewport extends behind the composer overlay.
- Assert that the bottom content inset is derived from the measured overlay height and is large enough to bring the last message above the card.
- Preserve the existing permission-order assertion: approval UI remains above the composer while the prompt stays open.
- Add focused pure tests for the compact height constants and fade clearance where extraction improves deterministic coverage.
- Re-run only the affected composer, transcript, and chat-pane tests, followed by a normal Debug app build.
- Do not run `Scripts/ci.sh` or an equivalent all-repository gate.

## Acceptance Criteria

- The resting composer is visibly compact and uses a 56-point minimum editor height.
- Multiline input grows only to 128 points before its internal scrolling behavior takes over.
- The composer surface is exactly `#20232D` in supported appearances.
- Transcript content and composer edges align at compact and wide pane sizes.
- Messages scroll behind the floating composer and disappear through a soft fade into the chat background.
- The final message can be scrolled completely above the bottom overlay at every supported composer height.
- Permission prompts and transient bottom banners remain usable and visually above the composer.
- Autoscroll, explicit scroll targets, focus, drag-and-drop, attachments, chips, slash commands, mentions, send, stop, and loading behavior remain intact.
- Focused tests pass and a normal Debug build succeeds, with validation limits reported separately from implementation status.
