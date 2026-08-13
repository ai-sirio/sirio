# D3 — The composer is one card

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Third in the design series by `fable` (`05-the-design-of-the-program.md`, decision D3). The code
and ledger outrank this file where they disagree — read `chat.rs` first, because P49 and D1 landed
in it after this brief was written.

## What you did with P49's six is why this piece is cheap

P49 handed you six absent entries and you built the four popover ones on **one machinery** — a
trigger character opens a filtered popup over the input, a selection inserts a token, the token
survives editing and submission — instead of four bespoke widgets. That choice is what makes this
piece consolidation rather than construction: the parts exist; this brief fixes **where they
live**.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh   # strict
```

**`tiller_ui/**` and `tiller_theme/**` are yours.** Your notes: `PI-HANDOFF.md`.

## The piece: `D-CHAT-01` — one card, everything a citizen of it

waku's composer anatomy, adopted wholesale as the anchor of the chat surface:

- **One card**: radius 13 (`theme.radii`), `composer` fill token, max-width 720, roomy
  placeholder. Everything below lives **inside** it. After this piece, an interactive composer
  control drawn outside the card is a defect against `D-CHAT-01`, not a style choice.
- **One labelled chip row**: agent · model · mode · **context ring** — plus the attach `+` at the
  row's start and the overflow `…` at its trailing end (F-CHAT-14's menu hangs off it).
- **Effort levels (F-CHAT-17) live inside the model picker's popover** as a labelled section — not
  a fifth always-visible chip. The chip row is the whole resting control surface; `D-CHROME-01`
  counts every control window-wide and the card gets no exemption.
- **Attachment chips (F-CHAT-12) render inside the card, above the input.** The slash and mention
  popups (F-CHAT-09/10) anchor to the input, opening upward over the transcript.
- **F-CHAT-19, behaviour half**: at ≥ 80% context the ring enters a stated warning state — a
  typed/state distinction a drawn test can read. Its colour is appearance and stays display-debt.
- The send/stop control from D1 keeps its `"send"` selector and its place at the row's end.

If P49 placed any of the six elsewhere — a second bar, a control beside the card, a popup anchored
to the window instead of the input — **move it in**. Nothing gets rebuilt; things get re-homed.

## The audit's holes on this ground (pass 12, folded in by fable — FABLE-04)

Five chat-segment `PASSED` rows fell in `PASSED-AUDIT.md`; the ledger already carries the pass-12
re-marks. F-CHAT-02/08 are D1's ground (see D1's addendum). **F-CHAT-16/18 sit in this piece's
popovers; F-CHAT-21/22/23 sit in the transcript beside it and ride this brief because they are one
build.** This brief was first written believing these rows had passed — build the real holes; the
journey order above stands.

- **F-CHAT-16 — the picker filters, or says nothing matched.** Today the picker proves
  select+escape only (`model_picker_selects_an_agent_advertised_model_and_escape_dismisses`). Add
  type-to-filter over the agent-advertised models and a stated no-match state — this is the slash
  popup's filtered-list machinery (chat.rs:2501) pointed at the picker popover, not a new widget.
  **The "Recommended" conjunct is absent-by-wire**: ACP's `SessionConfigSelectOption` is
  `{value, name, description?}` (agent-client-protocol-schema-1.5.0, v1/agent.rs:2294) — no
  recommendation marker exists. Do not invent a client-side model catalog; report that conjunct as
  absent-by-wire so the critic re-scopes the row.
- **F-CHAT-18 — do not build; report.** The clause demands an input/output/cache/cost breakdown;
  the ACP wire's `UsageUpdate` carries `{used, size, cost?}` and nothing else (schema v1/client.rs:302,
  v2 identical). The popover already renders the wire's entire payload. Padding it with rows the
  protocol cannot fill would be the exact fake this project refuses. One line in your report —
  clause overshoots the wire; the re-scope is the critic's call.
- **F-CHAT-21/22/23 — one expansion machinery, three rows.** One click-to-expand/collapse on
  transcript entries, applied three ways: `Entry::Thought` (chat.rs:70, render :1908) toggles
  (F-CHAT-21); consecutive `ToolCall` entries group into one collapsed "N steps" block
  (F-CHAT-22); a tool card click-expands and gains Dismiss (F-CHAT-23). For 23's
  output/diff/location links: the wire delivers `content`, `locations`, `raw_output` (schema
  v1/tool_call.rs:45–65) and `tiller_acp` currently drops them at the event mapping
  (tiller_acp/src/lib.rs:1020–1042). **That widening is `tiller_acp` ground, not yours** — it is
  flagged to the orchestrator. Give `Entry::ToolCall` optional content/location fields, fill them
  from the event when it carries them, render them as the expansion body; until the widening lands,
  report the links conjunct as `half-proven — blocked on the acp payload`, named exactly like that.

Their drawn-test evidence, same hardening rule as below: filter keystrokes narrow the picker and a
nonsense query shows the no-match state; a Thought entry toggles by real click; grouped steps
collapse and expand; a tool card expands and Dismiss removes it; an entry constructed with a
content payload renders it in the expansion (entry-constructed is honest cover until the acp event
can deliver one).

## Evidence

Drawn tests (`TestAppContext` + `VisualTestContext`, `.debug_selector(id)`,
`simulate_keystrokes`, full `run_until_parked()` hardening — unhardened is not evidence):

- the debug-bounds map shows every composer control's bounds **inside the card's bounds** — that
  containment assertion is the piece's core proof, cheap and total;
- the trigger-character popups still open via real keystrokes and still insert surviving tokens
  (their existing tests keep passing — do not weaken them to move them);
- effort is reachable: open the model popover, select an effort, assert the typed state changed;
- the ring's warning state flips at the threshold (feed the state, read the flag in the frame).

Appearance — radius, fill, the waku comparison — is marked in your report as **display-debt**, and
`SHOT-LIST.md` already schedules its frames (`chat-empty`, popover shots) for when a display
exists. Claim behaviour; never pixels.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  waku is the reference for this exact surface; take the anatomy, write the lines.
- Update F-CHAT-09/10/11/12/14/17/19 rows in `INVENTORY-LEDGER.md` and `D-CHAT-01` in
  `DESIGN-LEDGER.md` as **`builder-claimed, unverified` — never `PASSED`**. Same for
  F-CHAT-16 (the search/no-match half; the Recommended conjunct stays reported absent-by-wire)
  and F-CHAT-21/22/23 (the links conjunct of 23 stays `half-proven — blocked on the acp payload`
  until the event carries it). **F-CHAT-18 gets no row update from you** — it is report-only.
- Keep the gate green. Proceed without asking for design approval; this brief is the design.

## Reporting

Reply in **12 lines or fewer**: what moved and what already lived in the card, the containment
proof by test name, where effort ended up, the ring threshold proof, the gate result, and the
honest remainder.
