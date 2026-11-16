# Resizing the composer: drag its top edge to set how tall it grows

**Date:** 2026-11-15
**Status:** design
**Supersedes:** in `2026-10-30-composer-band-design.md`, the non-goal "the field's
growth (3 to 12 rows, then it scrolls: `multi_line(3, 12)`)" and acceptance check 6
("The field still grows from 3 to 12 rows"). Everything else in that design stands.

## §0 Intent

The user writes long prompts. The composer's field grows by itself from 3 to 12 rows
(`multi_line(3, 12)` in `rust/crates/sirio_ui/src/chat/mod.rs`) and then scrolls. At
12 rows (216 px) a long prompt cannot be read back whole, and nothing lets the user
give it more room. They asked to be able to raise the composer by hand, with a
maximum all the same.

Decided with the user, in order:

1. **Why:** "quando il testo è lungo". The need is room to read and edit a long draft.
2. **Behaviour: the height the user drags to becomes the field's new maximum for
   its automatic growth, and it is remembered.** Short drafts still take 3 rows;
   longer ones grow up to the chosen height and scroll past it. Of three behaviours
   shown this was the user's pick, with its trade-off stated and accepted: a drag
   over a near-empty composer shows the height while held, but on release the field
   shrinks back to fit the text, so the drag can look as if it did nothing (§3.2).
   Not chosen: the height resets to automatic after every send; the height stays as
   a fixed tall box until the user lowers it.
3. **Approach** (§4): Sirio pushes the new maximum into Ely's field through a new
   setter. Presented as the one sensible way; the user said "procedi".

Calls made in the brainstorm, stated to the user and not contested, listed again in
§9 with the other calls for checking: the cap is half the chat's height; the chosen
value is one app-wide preference saved across relaunches, not one per chat.

## §1 Goals, non-goals, anti-goals

**Goals**

- A drag on the composer's top edge sets how many rows the field may grow to, from
  3 rows up to the cap of §3.3, which keeps half the chat above the band.
- The value holds for every chat, those open and those opened later, and survives a
  relaunch.
- A user who never drags sees today's behaviour, 3 rows growing to 12, in any chat
  at least 554 px tall. In a shorter chat the cap of §3.3 applies to the default
  too, so the field stops at the cap (§9, call 8).
- Every agent gets it: there is one composer (`Chat::render_composer`) for Claude's
  native transport and every ACP agent.

**Non-goals**

- An expand/collapse button (the band design already declined its ⤢), a keyboard
  shortcut, a menu entry or a Settings control for the size.
- A reset gesture (double-click on the edge and the like). Dragging is the way back;
  a profile that never dragged keeps the default of 12.
- The change request tab's composer (`sirio_ui/src/change_request_tab/composer.rs`)
  and every other text field.
- The field's minimum (3 rows), its 13/18 px text, how it wraps, or how it scrolls
  once the text passes its maximum.
- A value per chat, per worktree or per project.

**Anti-goals** (it fails even if it works)

- The composer takes the transcript's room: the cap keeps at least half the chat
  for everything above the field.
- Changing the maximum touches the draft: text, caret, selection, undo history and
  IME composition stay as they were. The field is never rebuilt.
- The field stays pinned at the dragged height after the drag is over, whichever way
  the drag ended (§3.2).
- The composer's own height feeds back into its cap. The cap reads the chat's height,
  never the composer's or the transcript's (the same rule the band's layout follows
  for width: it reads the chat's width, never the row's).
- The edge steals a press meant for the text: no part of it lies over the field's rows.
- A value the user never saw reaches the database: NaN, infinite, out of range, or
  above the cap drawn at the moment the drag ended.
- A drag moves anything but the field's height: the band's width, the control row's
  layout (Full/Compact/Narrow/Minimal) and the chips stay put.

## §2 How it works today

- **The field** is Ely's `TextInput` built with `.multi_line(3, 12)`. It measures
  itself: the wrapped text's rows clamped to `[min, max]`, times the line height
  (`rust/vendor/ely-gpui-component/src/forms/text/element.rs`, `request_layout`).
  Past `max` it scrolls itself and keeps the caret in view from its own bounds
  (`keep_in_view`). `rows` is set only by the builder method; there is no setter.
  Setters exist for other state: `set_placeholder`, `set_masked`, `set_disabled`
  (`forms/text/mod.rs`). Chat observes the field (`cx.observe(&composer_field, …)`
  calls `reread_composer`).
- **The composer** is `Chat::render_composer` in
  `rust/crates/sirio_ui/src/chat/composer_view.rs`: a `composer` div holding Ely's
  `PromptInput::custom("composer-shell", …)`, which draws the band (a 1 px top rule,
  then `BAND_TOP_PAD`, 10 px, above the content), attachment chips above the field,
  the field in `composer-input` (13 px text on 18 px rows, `FIELD_PAD_Y` 5 px above
  and below), and the tool row; plus a canvas that measures the band. A left press
  anywhere on `composer` focuses the field.
- **The chat's size** is measured every paint by an invisible probe
  (`render_session_panel_probe` in `chat/session_panel/mod.rs`). It stores the width
  (`session_panel.frame.width`) and the chat root's bounds (`composer_row.root`), and
  asks for a redraw only when the width crosses a boundary the layout depends on.
- **A dragged edge already exists**: the file drawer's top edge
  (`rust/crates/sirio_ui/src/drawer/view.rs`). A 6 px absolute strip with the
  row-resize cursor; the anchor (pointer y, height) is taken at mouse-down; the
  height follows `on_drag_move` on the parent; the drag ends on `on_drop`, on
  `on_mouse_up_out` and on the strip's own mouse-up. It keeps the chosen height apart
  from the drawn one (`height` / `drawn_height`, `clamp_height`), and its module doc
  lists the ways it can fail.
- **App-wide UI preferences** already persist: `appearance.sidebarWidth`,
  `appearance.rightPanelWidth`, `appearance.centerSplitRatio` in `AppSettings`
  (`sirio_persistence/src/model.rs`), clamped on load (`db.rs`, `clamp_setting` with
  `settings_ranges`). The workspace saves them by writing the whole settings row,
  starting from the stored one (`schedule_panel_width_save` in
  `rust/crates/sirio/src/main.rs`), and `app_settings_for_settings_save` carries them
  over from the stored row so a save from the Settings window never resets them.
  For publishing a preference to every view, `sirio_ui::editor_prefs::EditorPrefs`
  is a GPUI global installed by the host and read by every editor.
- **Readback**: `surface.chat.read` carries `composerRow`, the band's
  `ComposerSnapshot` (`chat/composer_row.rs`): geometry, enums and labels only.

## §3 Behaviour

### §3.1 The edge

- A 6 px strip along the composer's top edge, absolutely positioned (out of layout
  flow), the composer's full width, with the row-resize cursor on hover. Nothing
  else is drawn, as with the drawer's edge. Debug selector `composer-edge`.
- It lies inside the band's top rule and its 10 px top padding, so it never covers
  the field's first row.
- It works whenever the composer is drawn: while a turn streams, while the composer
  is disabled, with or without an agent.

### §3.2 Dragging

- A press on the edge takes the anchor: the pointer's y and the rows the field is
  drawn at in that moment (what the user sees, not the stored maximum).
- While the pointer moves, the field shows exactly
  `rows = clamp(anchor_rows + round((anchor_y − pointer_y) / 18), 3, cap)`, in whole
  18 px rows. During the drag the field is set to `min = max = rows`, so it shows
  that height even over a short or empty draft.
- On release, the maximum becomes the last `rows` shown and the minimum goes back
  to 3. A shorter draft then shrinks the field to fit it (the accepted trade-off of
  §0.2); a longer one keeps it at the new maximum and scrolls.
- Dragging down lowers the maximum, down to 3 rows.
- A press and release with no movement changes nothing and saves nothing.
- However the drag ends (released over the composer, released elsewhere in the
  window, released outside it, or ended by the platform without a release reaching
  the chat), the field goes back to `min = 3` and the value shown last is the one
  kept. How to guarantee the last case is the builder's (see §7).

### §3.3 The cap

- `cap_rows = max(3, floor((chat_height / 2 - BAND_ALLOWANCE) / 18))`, with
  `BAND_ALLOWANCE = 61` px, where `chat_height` is the chat
  root's height as the probe last measured it: the whole chat (transcript, question
  dock, queue and composer), never the composer or the transcript alone.
- The cap leaves half the chat for everything above the band. `BAND_ALLOWANCE` is a
  fixed constant, not a measurement: the band's height minus its field rows' text
  height when no chips and no attach error are shown. It covers the band's rule,
  its top padding, the field's padding, the gap to the tool row, the tool row and the
  bottom padding (1 + 10 + 10 + 8 + 24 + 8). Chips and the attach error still come
  on top of it when shown, so then the band can pass half the chat. The field is
  what the user drags, and chips come and go.
- Revised 2026-11-16 at the user's ruling: §1 and §3.3 disagreed; the cap now
  subtracts a fixed band allowance so §1 holds.
- Before the chat's first paint its height is unknown, and the field uses
  `min(stored, 12)` until the probe has measured it. A chat shown for the first
  time then grows into its cap instead of overshooting it for a frame. This was a
  call made while planning; the first version of this design left the stored value
  uncapped there, as the drawer does before it knows the tab's height.
- The probe asks for a redraw when `cap_rows` changes (a window or pane resize across
  a row boundary), as it does today for width boundaries, and at no other time.

### §3.4 Stored and drawn

- **Stored:** one integer, `chat.composerMaxRows`, default 12, clamped on load into a
  fixed range starting at 3 (the upper bound is the builder's; §8).
- **Drawn:** each chat draws its maximum as `min(stored, its cap_rows)`. Two chats of
  different heights draw different maxima from the one stored value. The default is
  not exempt: a profile that never dragged stores 12 like any other, and a chat
  under 554 px tall draws less. So nothing needs to tell "never dragged" from
  "dragged to 12", and the plain `i64` default of §5 is enough.
- A smaller window or pane lowers the drawn maximum and leaves the stored value
  alone; growing it back gives the stored value back.
- At the end of a drag that moved, the stored value becomes the rows shown last
  (already within the cap), saved once. Nothing is saved during the drag.

### §3.5 Every chat

- The new value reaches every open chat at once (each redraws with its own drawn
  maximum), and chats opened later start from it.

## §4 Approaches

**Chosen: A, Sirio pushes the maximum into Ely's field through a new setter.**
`TextInput` gains a setter for its rows beside `set_placeholder`, `set_masked` and
`set_disabled`. It takes both bounds (`set_rows(min, max, cx)` or similar), because
the drag holds `min = max`; it keeps the builder's `0 < min <= max` assertion and
notifies only when the value changes. The chat calls it when the rows it wants
differ from the field's. The field goes on measuring, wrapping, scrolling and
following the caret by itself. The change is logged in Ely's `LOCAL-CHANGES.md`.

**Rejected: B, cap the height from outside with the container's maximum height,
Ely untouched.** Ely's measure clamps the rows to its own maximum, so the field never
grows past 12 whatever the container allows. Raising Ely's maximum anyway and then
fighting its measure from outside gives two height rules, while the caret-following
scroll still reads the field's own bounds.

**Rejected: C, rebuild the field with a new `multi_line` on every drag.** It loses
undo history, focus, selection, IME composition and the observer Chat holds on it.

## §5 Design, per crate

- **Vendored Ely** (`rust/vendor/ely-gpui-component/src/forms/text/mod.rs`): the
  rows setter of §4. `LOCAL-CHANGES.md` gets an entry for it.
- **`sirio_persistence`**: a settings key `chat.composerMaxRows`, a range in
  `settings_ranges`, an `AppSettings` field (an `i64` like every numeric setting),
  default 12, clamped on load like `sidebar_width`, written on save.
- **`sirio` (the workspace)**: publishes the stored value app-wide at startup (as it
  installs `EditorPrefs`); persists a new value when a chat reports one, by writing
  the whole row from the stored one (as `schedule_panel_width_save` does);
  `app_settings_for_settings_save` carries the new field over from the stored row.
  `main.rs` has seven `save_settings` call sites today; every one that builds the
  row from something other than the stored row must carry the field, or a Settings
  save resets the composer to 12.
- **`sirio_ui`**: a global holding the value, read by every chat (in the manner of
  `EditorPrefs`); the drag state and the edge in `composer_view.rs`; the cap from
  the chat root's height measured by the existing probe, with the probe's redraw
  rule extended to cap changes; a way for the chat to report a new value to the
  workspace (a `ChatEvent` variant, as `SessionPanelToggled` reports a layout change,
  or an observer of the global). The exact mechanism is the builder's, as long as
  there is one value, it is live across chats, and it is saved once per drag.
- **Readback**: `ComposerSnapshot` gains the field's box, the edge's box, the rows
  the field is drawn at, the drawn maximum, the stored value and the cap, all
  numbers, as that struct's rule requires. No new write method on the control
  socket: the E2E drags for real.

## §6 Verification

Per the repo's conventions (`CLAUDE.md`, Testing), the feature is proven end to end
against the real binary, with an artifact anyone can re-run.

**E2E.** A `composer_resize` scenario in `Scripts/Tests/test-chat-session-panel-e2e.sh`,
which already drives a painted native-Claude chat over a stub `claude`, presses real
X keys and relaunches. Read `docs/agents/e2e-tests.md` first; the scroll-drag probe
(`Scripts/Tests/test-scroll-drag-probe.sh`) has real `xdotool` drags to copy. Steps,
each asserted on `composerRow`:

1. Fresh profile, empty draft, a chat at least 554 px tall: drawn maximum 12, field
   at 3 rows.
2. A 40-line draft: field at 12 rows.
3. A real drag (press on the edge's centre, move up N × 18 px in steps, release):
   field and drawn maximum at 12 + N. Capture a frame.
4. A drag far past the cap: field at `cap_rows`, computed from the root's height.
5. Clear the draft: field back to 3 rows.
6. On the empty composer, drag up M rows and hold: field at 3 + M mid-drag (capture);
   release: field at 3, drawn maximum 3 + M.
7. Press and release on the edge without moving: nothing changes.
8. Relaunch: drawn maximum is the last value; a second chat opened has it too.
9. Shrink the window below what the value needs: drawn maximum equals the new cap,
   stored value unchanged; grow it back: drawn maximum restored.

Frames under the scenario's `frames/composer-resize-*.png`, the success line the
script already prints, and the scenario added to `docs/agents/e2e-tests.md`.

**Unit tests.** None planned. If the builder extracts the drag-to-rows arithmetic or
the cap as a pure function, its `#[test]`s come first, starting from the ways it can
fail (NaN or infinite pointer positions, a pointer far outside the window, a chat
shorter than 3 rows, a stored value out of range), as the drawer's module doc lists
its own.

**Existing tests.** None pins the 12 rows today (checked `band_tests.rs`,
`ely_tests.rs`, `mod.rs`); `examples/ely_chat_probe.rs` builds its own field with
`multi_line(3, 8)` and is untouched.

## §7 Risks for the builder

- **A field left pinned.** If a drag ends without any of the end handlers running,
  `min = max` would stay. The panel-width code in `main.rs` lists such endings
  (Escape, a release outside the window); it covers them with a timer. Here the
  invariant is that no field stays pinned once no composer drag is active.
- **Settings writers.** See §5: a writer that rebuilds the row without the new field
  silently resets it.
- **Redraw loops.** The probe must notify only on a change of `cap_rows`; the setter
  must notify only on a change; calling the setter from inside `render` would notify
  the field, whose observer is the chat. Apply rows from events (drag moves, the
  global's change, the probe's deferred notify), not from `render`.
- **The press bubbles.** A press on the edge also reaches the composer's handler,
  which focuses the field. Keeping focus in the field during a drag is fine; the
  builder decides whether to stop it.
- **Popups.** The slash and mention completions hang above the token that opened
  them (`SHELL_TOP_INSET`); a taller field moves the token, and they follow it.

## §8 Left to the builder

- Names: the setter, the global, the event, the snapshot fields.
- The stored value's upper bound (generous; the cap does the real limiting).
- Whether the edge stops the press from focusing the field.
- How the "no drag active" invariant of §7 is enforced.

## §9 Calls made in the brainstorm, for the user to check

1. The cap keeps half the chat above the band: half the chat's height minus a fixed
   61 px band allowance, in rows (revised 2026-11-16, see §3.3).
2. One app-wide value, saved across relaunches; not per chat.
3. The edge shows nothing until hovered (the cursor changes), like the drawer's.
4. A drag starts from the height the field shows, not from the stored maximum:
   dragging a 3-row field up by 2 rows sets the maximum to 5, even if it was 20.
5. Heights snap to whole 18 px rows.
6. No reset gesture; a profile that never dragged keeps 12.
7. The edge works while a turn runs and while the composer is disabled.
8. The cap applies to the default of 12 as well: in a chat under 554 px tall, a
   profile that never dragged gets fewer than 12 rows, where today it gets 12. Raised
   by the builder check; settled this way because it keeps one rule (the transcript
   always keeps half the chat above the band) and needs no "never dragged" state.
