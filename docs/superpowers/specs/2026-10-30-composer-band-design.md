# The composer as a band: Zed's bottom strip in Sirio's chat

**Date:** 2026-10-30
**Status:** design
**Mockups:** `2026-10-30-composer-band-mockups.html` (beside this file; layout A,
then icon family A)

## §0 Intent

The user likes how Zed's agent panel draws its message editor: it takes **all the
space under the chat**, a strip from edge to edge, not a card floating in the
middle. They do not like how Sirio's composer looks today, **above all when the
chat pane is narrow because other panes have the room**: the floating card keeps
its margins, and its control row wraps onto two or three lines, so the composer
grows taller exactly where space is shortest.

Decided with the user, in order:

1. **Mockup A, "as in the mockup".** A full-width band with a single top rule,
   the field above and every control on one row below. The row compacts instead
   of wrapping. Variants B (settings in a strip above the field) and C (one chip
   for mode, model and effort) were shown and not chosen.
2. **No expand button.** The ⤢ drawn in the mockup's field is not built.
3. The rest of the play-back was accepted as written ("va bene"): the band
   spans the chat while its content stays aligned to the transcript column in
   a wide pane; the order in which the row compacts; and attachment chips going
   inside the band, above the field.
4. **Permission modes get icons** ("metti delle icone che indicano i permessi",
   icons found online). Of three Lucide families shown, the user chose **A, by
   meaning**: a hand for Ask, a file with a pen for Accept edits, a list for
   Plan, sparkles for Auto, a struck-through shield for Bypass permissions.
   This answers how a dangerous mode stays visible in a narrow pane (§3).

Everything else below that is not one of these four is marked as a call made in
the brainstorm, listed again in §9 for the user to check.

## §1 Goals, non-goals, anti-goals

**Goals**

- The composer is a band at the bottom of the chat column: a 1px rule on top,
  no side margins, no rounded corners, no gap below it.
- The control row is **one line at every width the chat can take**. It compacts;
  it never wraps. Send/stop is always drawn and reachable.
- In a wide pane the band runs edge to edge while its content keeps to the
  transcript's reading column (`TRANSCRIPT_WIDTH`, 760 px).
- Every agent gets it. There is one composer (`Chat::render_composer`) for
  Claude's native transport and every ACP agent.

**Non-goals**

- The ⤢ expand-to-fill control (declined by the user).
- Restyling the question dock or the queue card. Both stay as they are: cards in
  the reading column, above the band (Ely chat spec §6's order is unchanged:
  request, queue, composer).
- Any change to what a picker offers or does, to the field's growth (3 to 12
  rows, then it scrolls: `multi_line(3, 12)`), or to placeholders.

**Anti-goals** (it fails even if it works)

- A control that a narrow pane makes unreachable, or a compact control that
  opens nothing. Everything that leaves the row lands in ⋯.
- The row wrapping again at some width nobody tried.
- Ely's upstream defaults changing: `PromptInput::new`, `PromptInput::custom`
  as used elsewhere, and `ChatContainer` without the new option render exactly as
  today.

## §2 The band

From the top of the chat column down: transcript, then the question dock and the
queue (unchanged cards in the reading column), then the band, flush with the
pane's bottom edge.

- **Extent.** The band is as wide as the chat column. With the session panel
  docked, the band ends at the panel's left edge (the panel is a sibling of the
  chat, not over it). The overlaid panel of a narrow chat keeps ending above the
  composer, as `a_narrow_chat_gets_the_column_over_its_transcript_not_its_composer`
  requires today.
- **Paint.** Fill is the theme's `surface`, the card colour the composer has
  today, so the band still reads as the place you type and the field still paints
  nothing of its own over it. The top rule is `border`, the colour of today's
  card border. Focus does not change the rule, as it does not change today's card
  border (the caret says where focus is).
- **Spacing.** The 18 px bottom padding under the composer (`mod.rs`) and
  `ChatContainer`'s `px_4`/`pb_4` around its composer slot no longer apply to the
  band. Inside the band, the content column is `max_w(TRANSCRIPT_WIDTH)`, centred,
  with horizontal padding chosen so the field's first character sits on the
  transcript text's left edge. In a narrow pane the padding is roughly 10 to 16 px
  each side, and vertically about 10 px. Exact values come from Sirio's tokens
  (builder).
- **Inside, top to bottom:** attachment chips (as today, `attachment-strip`), the
  field, the attach error line when there is one, the control row.
- **Field.** Unchanged: Ely's raw `TextInput`, 13/18 px text, 3 to 12 rows.
- **Popups.** The `/` and `@` popups still hang above the token that opened them.
  `SHELL_TOP_INSET` encodes today's 1px border plus 12px padding and is
  recomputed for the band's rule plus top padding.
- **Pointer.** A left click anywhere on the band focuses the field (today's
  `composer` mouse-down). A right click on the field opens its context menu.

## §3 The control row

**Left group:** attach (icon changes from the paperclip to `+`; same action,
same "Attach image" tooltip), the session panel toggle (drawn "on" while open),
then ⋯ (the overflow menu and the Chat History popover hang from it as today).

**Right group**, in today's order, followed by send/stop: mode, model, effort,
Fast, background tasks, Thinking, context ring.

- The "Model" and "Effort" key labels go away; the value speaks for itself, as in
  Zed. "Thinking" keeps its label, because its values do not read on their own.
  "Fast" is already its own label.
- With no agent configured the row holds only send (today's rule). Before models
  arrive the agent badge stands in for the model chip (today's rule, #206).

**Full and compact.** The row has two layouts.

- **Full:** as above. The mode shows its icon, name and chevron. Effort shows
  bars, level and chevron.
- **Compact**, whenever Full does not fit on one line:
  - mode: icon and chevron only, with the name in the tooltip;
  - effort: bars and chevron only;
  - Fast and Thinking leave the row and become rows in ⋯. Fast is a checkable
    toggle that keeps its blocked reason. Thinking opens the same picker;
  - the session panel toggle leaves the row and becomes a row in ⋯;
  - the model name ellipsizes down to its existing 56 px floor;
  - the background-task count and the context ring stay.
- **Which one applies** is decided from the width the band actually has. The
  probe in `render_session_panel_probe` measures the whole chat root, and a docked
  session panel (`COLUMN_WIDTH`, 300 px) sits beside the band inside that root.
  So when the panel is docked, the band is the probe's width minus the column.
  Deriving it that way or putting a measurement on the band itself is the
  builder's call. Either way, crossing the threshold must redraw. Today the probe
  notifies only when docked/overlay flips, so its notify rule grows to cover the
  threshold. The threshold is the narrowest band at which Full still fits on one
  line, given the chips actually present. How to compute it (measured once, or
  derived from what is present) is the builder's call. The layout must not flip
  back and forth when a single redraw lands on the boundary.
- **Floor.** Compact fits on one line down to a 280 px chat with every chip
  present. Below that the model name is what gives. If Sirio lets a chat pane get
  narrower than that, the builder states the real minimum and checks the row
  there.

**Deliberate.** `controls.rs` defends a readable mode name (F-CHAT-15, #136).
Full keeps it. Compact trading it for the icon plus tooltip is this design's
decision, not an accident to fix back.

**Mode icons** (the user's choice, family A). Lucide, the set Ely already
bundles (`lucide-static` 1.48.0, ISC), keyed by the mode's id:

| Mode id | Name shown | Lucide icon |
|---|---|---|
| `default` | Ask | `hand` |
| `acceptEdits` | Accept edits | `file-pen` |
| `plan` | Plan | `clipboard-list` |
| `auto` | Auto | `sparkles` |
| `bypassPermissions` | Bypass permissions | `shield-off` |
| any other id (ACP agents' own modes) | the agent's name for it | `shield` |

- The icon takes the place of today's dot, in both layouts.
- Bypass permissions (`BYPASS_PERMISSIONS_MODE`) is drawn in the theme's danger
  colour, icon and name, on a faint danger fill. It stays that way in compact,
  so a narrow pane still shows that every tool runs without asking.
- The connection state that today's dot carries (ready, busy, offline:
  `PillDot`) moves to a small dot on the icon's bottom-right corner, in today's
  colours.
- Without a mode catalogue the pill has no mode to draw. It keeps today's dot
  and state word ("connecting", "working", "idle", "offline"), and in compact
  only the dot.

## §4 Ely changes (vendored)

The look lives in `rust/vendor/ely-gpui-component`, so both changes are recorded
in its `LOCAL-CHANGES.md`. The upstream defaults stay untouched. Today the only
caller of `PromptInput::custom` is the chat composer. `PromptInput::new` and
`ChatContainer`'s composer slot are also used by `sirio_ui/examples/ely_chat_probe.rs`,
which must keep rendering as it does now.

- **`PromptInput`** (`chat/composer.rs`). A band look for the host-editor shell:
  no radius, top rule only, and a one-line tool row (tools, spacer, trailing +
  send). Today's `custom` row uses `flex_wrap`. Sirio then passes attach, panel
  and ⋯ as tools (left) and the chips as trailing (right, beside send). The
  existing `LOCAL-CHANGES.md` sentence "the tool row wraps" is revised. Whether
  this is an option on `custom` or a constructor of its own is the builder's call.
- **`ChatContainer`** (`chat/message.rs`). Its composer slot adds `px_4`, `pb_4`
  and `max_w(column)`. The band needs a slot without them: an option on
  `ChatContainer`, or Sirio drawing the band outside that slot. Builder's call,
  under the same "upstream default unchanged" rule.
- **Icons.** `hand`, `sparkles` and `shield` are already in Ely's
  `assets/icons` and `IconName` (`primitives/icon.rs`). `file-pen`,
  `clipboard-list` and `shield-off` are added from `lucide-static` 1.48.0,
  the version the bundled set carries, with their `IconName` variants, and
  recorded in `LOCAL-CHANGES.md`.

## §5 What stays exactly as it is

Everything in the Ely chat spec §6 about behaviour: Enter submits or queues,
Shift+Enter is a newline, mentions, slash commands, model search, image paste
and drop, attachment removal, capability checks, picker contents and keyboard
navigation, focus restoration. The pane-wide "Drop files to attach" overlay
(`chat-drop-overlay`) is drawn over the whole chat, not the composer, and does
not change.

Debug selectors stay on the element they name today: `composer`,
`composer-input`, `ely-prompt-input` (now on the band), `attach-image`,
`session-panel-toggle`, `composer-overflow`, `chat-status`/`chat-connecting`,
`model-chip`, `model-chip-chevron`, `agent-badge`, `effort-chip`,
`fast-mode-chip`/`fast-mode-chip-blocked`, `thinking-chip`,
`background-tasks-chip`, `attachment-strip`, `slash-popup`, `mention-popup-card`.
A control that compact moves into ⋯ is absent from the row in compact, and its
⋯ row gets a selector of its own (builder names it). `model-effort-label` exists
only in Full. The mode icon carries a selector that names the mode it draws
(builder names it), so a test can tell which icon is on screen.

## §6 What this supersedes

- Ely chat spec (`2026-09-30-ely-agent-chat-design.md`) §4, *Responsive*: "at
  narrow pane widths … controls wrap" no longer holds for the composer's control
  row, which compacts. It still holds for everything else.
- The comments in `controls.rs` that describe the wrap ("the chip wraps whole to
  the next line", "the name ellipsizes while the row wraps around it") are
  rewritten to describe compacting.
- `LOCAL-CHANGES.md`, as in §4.

## §7 Testing

**End to end, with an artifact.** `Scripts/Tests/test-ely-chat-ui-e2e.py` gets a
`composer_band` scenario, dark and light, in a wide window (the default 1470x833)
and a narrow one (its `--window-size` lane, chat around 300 px). It captures each
under `--out-dir` and asserts:

- the row is one line: every row control shares one vertical band;
- send is wholly inside the pane;
- the band's rule runs from the chat column's left edge to its right edge;
- the band is flush with the pane's bottom edge;
- in the narrow window the compact layout applies, and the mode chip draws the
  icon of the current mode;
- a third lane opens the session panel docked, in a window wide enough to dock
  (chat at least 640 px) but with the band too narrow for Full. There compact
  applies, and the band ends at the panel's edge.

Fast and Thinking exist only on Claude's native transport (`AgentClient::fast_mode`
and `thinking_display` return `None` for ACP), and this script runs the ACP
fixture. So their ⋯ rows are proven on the native-Claude stub that
`Scripts/Tests/test-chat-session-panel-e2e.sh` already drives
(`sirio_acp/tests/fixtures/claude_fixture.py`, which reports Fast as blocked,
`sdk_opt_in_required`): in a narrow chat, ⋯ lists Fast with its blocked reason and
Thinking opens its picker. The same narrow lane switches to Bypass permissions
(the bypass confirmation answered) and asserts that the compact chip draws
`shield-off` in the danger colour. Whether that is a scenario in that script or a
lane of its own is the builder's call. The ACP scenario asserts the rest.

How each test reads geometry is the builder's call: pixels from the captures, or
a debug-only addition to `surface.chat.read`. Each prints its OK line, and
`docs/agents/e2e-tests.md` gains the entries.

**Tests that change on purpose.** They pin the wrap or the card and are rewritten
to the band's behaviour (one line, send reachable, every control reachable in the
row or in ⋯), not deleted:

- in `sirio_ui/src/chat/mod.rs`:
  - `composer_card_carries_the_chip_row_and_the_send_disc`
  - `narrow_control_row_keeps_every_essential_control_reachable`
  - `narrow_composer_stays_inside_chat_pane_and_keeps_send_reachable`
- `ely_narrow_catalogue_popup_keeps_send_and_focus_reachable` (`ely_tests.rs`)
- in `test-ely-chat-ui-e2e.py`:
  - `composer_field_has_no_fill`: its band is placed for the card's geometry;
  - `COMPOSER_CLICK`;
  - the slash popup's expected box.

**A new unit test only where CLAUDE.md allows one.** If the Full/compact choice
becomes a pure function (width and the chips present in, layout out), its
`#[test]` lists every way it can fail before the function is written.

Iterate with `cargo test -p sirio_ui` (under nextest for the whole crate). Never
launch `Scripts/ci.sh`: ask the user when the gate is needed.

## §8 Delivery

One PR. It is the first `feat:` of the 0.37.1 cycle, so it moves the workspace
version to **0.38.0** (`Scripts/set-workspace-version.sh`). macOS is the reference
platform: a hand check of the band on a Mac (wide and narrow, dark and light) is
owed before release and listed in the PR.

## §9 Calls made in the brainstorm, for the user to check

1. The band's fill is `surface` and its rule is `border`, with no focus change.
2. Attach shows `+` instead of the paperclip, with the same action.
3. "Thinking" keeps its label. "Model" and "Effort" lose theirs.
4. Compact moves Fast, Thinking and the session panel toggle into ⋯.
5. The question dock and the queue stay as cards.
6. The field still grows from 3 to 12 rows.
7. With the session panel docked, the band stops at the panel.
8. The compact floor is 280 px.
9. Full or compact follows the band's own width (so a docked panel counts), not
   the whole pane.
10. The Fast and Thinking rows in ⋯ are proven on the native-Claude stub; the
   band itself is proven on the ACP fixture.
11. Connection state moves to a dot on the mode icon's corner (shown in the
   icon mockup; the user chose the family without commenting on the dot).
12. ACP agents' own modes all get the plain `shield`, even one as permissive as
   Bypass. Mapping other agents' mode ids to icons is a follow-up.
13. Without a mode catalogue the pill keeps today's dot and state word.

**Left to the builder:** exact paddings, how the threshold is computed, the shape
of the Ely API, the ⋯ rows' and mode icon's selectors, the icons for Fast and
Thinking in ⋯, and how the E2E reads geometry.
