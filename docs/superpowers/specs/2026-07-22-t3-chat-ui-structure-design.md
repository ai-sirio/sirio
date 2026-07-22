# T3-style chat UI structure (timeline, composer, pickers, meter)

**Date:** 2026-07-22
**Status:** Approved (brainstorming session)
**Depends on:** `2026-07-22-native-chat-transports-design.md` (Spec 1 — provides
permission modes, model/effort state, context usage events)

## Goal

Adopt t3code's chat area structure — timeline row model, grouped tool work log,
composer-anchored approvals, model/effort picker, context meter — while keeping
Tiller's existing visual design tokens and the T3-style text rendering shipped on
2026-07-22 (typography, code cards, inline chips).

Scope decisions from brainstorming:
- **Chat area only.** No thread-first sidebar, no diffs/files/preview copies —
  Tiller already covers those elsewhere.
- **Their structure, our skin** — layout, hierarchy and behaviour from t3code;
  colors, fonts, materials from Tiller's design system.
- **Minimap excluded** (t3code's left-gutter navigation rail): high SwiftUI cost,
  low value once turns fold. Possible follow-up.

## Timeline: typed row model

Replace today's flat transcript item list with a typed row list, mirroring t3code's
`MessagesTimelineRow`:

- **`message`** — message card. Assistant meta: turn duration (computed from the
  user-message boundary, t3code's `computeMessageDurationStart` algorithm) and a copy
  button shown only on the turn's final assistant message once streaming ends.
- **`work` + `work-toggle`** — tool calls within a turn are **grouped**. Collapsed
  state shows only the latest entry (`MAX_VISIBLE = 1`) with a compact label
  (trailing "completed" stripped — "Read AppModel.swift"), preceded by a
  "▸ N steps" toggle that expands the group. This replaces today's one-card-per-tool
  sequence.
- **`turn-fold`** — older turns collapse into a single expandable "Turn: <label>"
  row; only the current and previous turn stay open by default.
- **`proposed-plan`** — plan card fed by Claude plan mode (Spec 1): title, steps,
  approve/reject buttons; approval unlocks the execution turn.
- **`working`** — trailing "working" indicator while streaming.

Content column: fixed max width (~700pt), centered — the equivalent of their 768px.

The row model is pure logic over the canonical transcript (like their
`MessagesTimeline.logic.ts`), independent of the view layer.

## Composer

- **Pending-approval panel above the input** (t3code's
  `ComposerPendingApprovalPanel`): permission requests anchor to the composer instead
  of appearing in the transcript; allow/deny actions; queued when multiple arrive.
- **Banner stack** above the composer: provider errors, "session resumed as
  history" (Spec 1 migration), plan awaiting approval.
- **Permission mode dropdown** next to the model picker: the four Tiller modes,
  filtered to what the active driver supports (Spec 1 mapping). Hidden for ACP
  agents.
- Slash menu and file mentions: existing Tiller implementations, unchanged.

## Model + effort picker

- Compact composer button showing the active model (+ effort badge when set).
- Popover: driver-supplied model list (Claude: `init`/`supportedModels`; Codex:
  protocol list; OpenCode: `/config/providers` grouped by provider), search, and a
  highlighted recommended model.
- Effort row below the list where supported (Codex native; Claude prompt prefix;
  OpenCode config option).
- Selection persists per session (Spec 1's v13 columns).
- ACP agents: picker limited to what ACP exposes (today's behaviour, no regression).

## Context meter

- Extends/replaces the current context ring: fill percentage of the context window,
  tooltip with exact tokens (input/output/cache), warning threshold at >80% with a
  color change.
- Fed by Spec 1's canonical `contextUsage` events (Claude `get_context_usage`,
  Codex `token_count`, OpenCode usage).

## Testing (swift-testing, TDD)

- **Row model (bulk of the value):** transcript fixtures → expected rows: grouping,
  fold boundaries, toggle counts, duration computation, copy-button visibility.
  Mirrors t3code's approach (their logic tests outweigh their view tests).
- **Interaction:** composer approval panel answers the correct request with multiple
  queued; picker persists selection; meter updates on event.
- **Done:** `Scripts/ci.sh` prints CI OK, plus a manual visual checklist — grouping,
  fold, plan card, picker, meter, mode dropdown across the three native drivers and
  one ACP agent.
