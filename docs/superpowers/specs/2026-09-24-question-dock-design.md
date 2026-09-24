# The question dock — design

**Date:** 2026-09-24
**Status:** implemented on `worktree/calm-harbor-e5a1`
**Parent work:** F-CHAT-25 (question cards answered in place), F-CHAT-26 (the
pending-question bar), F-CHAT-24 (plan approval on the Plan card)
**Scope of this document:** moving the answer to an agent's question out of
the transcript and into one panel above the composer, which shows the whole
question and lists its answers vertically, keyboard-first, with no warning
colour anywhere.

## What is there today

A question the agent puts to the user (a tool permission, an
`AskUserQuestion`, a plan approval) is drawn in two places, neither of which
is where the user is looking:

- **An inline card in the transcript** (`Entry::Permission`,
  `rust/crates/sirio_ui/src/chat/mod.rs`, the `render_entry` arm near
  line 6042). A 2px `theme.warning` left border, the title, the prompt, and
  the options as a horizontal `flex_wrap` row of buttons, rejections tinted
  `theme.danger`. Plan approvals attach the same button row to the Plan card
  (near line 6239).
- **A reminder bar above the composer** (`pending-question-bar`, near
  line 9196): a `theme.warning` border, a yellow `?`, the text
  `Question waiting · <title>` cut with an ellipsis, and a `Show` link that
  scrolls the transcript to the card.

The composer is disabled for as long as a question is open
(`composer_disabled`, `composer_view.rs`), so the one surface that says
"answer me" is a one-line bar that cannot be answered, and the surface that
can be answered is somewhere up the transcript. For a tool permission the bar
shows a path truncated at the pane's width; for a question it shows the
generic title `Question`, because the native transport's tool title for
`AskUserQuestion` is that constant (`sirio_claude::tools::describe`) and the
question's own `header` is dropped at ingestion.

## What changes

One panel — the **question dock** — replaces the bar in the same slot, above
the queue and the composer. It carries the whole question and every answer,
and it is the only place a question can be answered. The transcript keeps a
record of the question and its outcome, with no buttons.

```
╭──────────────────────────────────────────────────────╮
│ Approach                                  (caption)  │  footnote · text_muted
│ Which layout should the panel use?                   │  callout · text, wraps
│                                                      │
│ ┌──────────────────────────────────────────────────┐ │
│ │ [1]  Docked                                      │ │  selected: menu_row_nav highlight
│ │      Sits above the composer                     │ │  footnote · text_muted
│ └──────────────────────────────────────────────────┘ │
│  [2]  Floating                                       │
│       Overlays the transcript                        │
│  [3]  Other…  [ Type something else…            ]    │
│                                                      │
│                    ↑↓ select · ⏎ confirm · esc cancel │  caption2 · text_faint
╰──────────────────────────────────────────────────────╯
```

## 1. Data

### `sirio_acp`: option descriptions

`PermissionOption` (`rust/crates/sirio_acp/src/lib.rs:448`) gains
`description: Option<String>`. It has exactly two construction sites:

- `claude::permission::option` — `question_options` passes each
  `AskUserQuestion` option's `description` string when present; the ordinary
  permission and plan options pass `None`.
- `permission_option` (`lib.rs:2210`) — the ACP protocol option has no
  description field, so `None`.

Nothing else about the wire changes: the option id is still the label for a
question and the fixed id for a permission, and `answer_for` /
`answer_for_question` are untouched.

### `sirio_acp`: who may offer free text

Typed text reaches the agent through the same channel as a chosen option:
`respond_permission(request_id, text)`. On the ACP path that string goes out
as a `SelectedPermissionOutcome`'s option id (`lib.rs`, near line 1544), so
an ACP agent handed free text receives an option id it never offered. Only
the native Claude transport accepts arbitrary text, because
`answer_for_question` writes whatever arrives into the tool's `answers`.

So the free-text affordance is declared by the layer that knows it is safe:
a new pure helper in `claude::permission` fills in
`PermissionQuestion::text_input` (placeholder `None`, prefill `None`) for an
`AskUserQuestion` that did not declare one, and the native worker applies it
where it builds the question (`claude/worker.rs`, near line 746). The ACP
path is unchanged: there, free text exists only when the agent declares
`_sirioTextInput`, as today.

### `sirio_ui`: ingestion

`AnswerOption` gains `description: Option<String>`, carried from
`PermissionOption` in the `AcpEvent::PermissionRequest` arm. Its seven
literals in `chat/mod.rs` take the field.

`Entry::Permission` gains `is_question: bool` — true when the request carried
a structured question. Without it a Pi question (whose prompt is blanked
because it repeats the header) and a plain permission (which has no prompt)
are indistinguishable. Its four literals take the field; an entry restored
from disk gets `false`, which is harmless because a restored entry is never
open.

A structured question's `header` becomes the entry's `title`. The header
always exists — `parse_permission_question` falls back to the prompt — so
this turns the native transport's constant `Question` into the question's own
header (`Approach`), and leaves Pi's title what it is today, the question
text.

Descriptions are **not persisted**. `ChatPermissionOption`
(`sirio_persistence`) is unchanged: an entry restored from disk is already
answered or expired, and the dock never draws it.

### The view-model

A new module, `rust/crates/sirio_ui/src/chat/question_dock.rs`, owns a pure
projection of the transcript onto what the dock draws:

```rust
pub(super) struct QuestionView {
    pub request_id: u64,
    pub entry_index: usize,
    pub caption: String,
    pub body: String,
    pub rows: Vec<DockRow>,
}

pub(super) enum DockRow {
    Option(AnswerOption),
    FreeText(AnswerTextInput),
    Dismiss,
}

pub(super) fn question_view(entries: &[Entry]) -> Option<QuestionView>
```

`question_view` selects the same entry `pending_question()` does — the
**first** unanswered, unexpired `Entry::Permission` or `Entry::Plan` approval
— so the dock, `composer_disabled` and `composer_placeholder` cannot
disagree. The agreement is structural, not a convention: both find the entry
through one predicate, `question_dock::is_open(&Entry)`, and
`pending_question()` keeps its signature and callers.

Caption and body:

| Entry | caption | body |
|---|---|---|
| `is_question`, non-empty prompt | the `title` (its header) | the prompt |
| `is_question`, empty prompt (Pi) | `Question` | the `title` |
| not `is_question` (plain permission) | `Permission requested` | the `title` (path, command) |
| plan approval | `Plan approval` | the approval's `title` |

Rows, in order:

1. one `DockRow::Option` per offered option;
2. a `DockRow::FreeText` exactly when the entry has a `text_input` — which,
   after the change above, means a native `AskUserQuestion`, an agent that
   declared `_sirioTextInput`, or a structured question with no options (the
   existing ingestion fallback). Never otherwise: a plain permission's
   `answer_for` treats an unknown id as a refusal, and an ACP agent receives
   it as an option id it never offered;
3. a single `DockRow::Dismiss` when the first two produced nothing (the
   "unrenderable" request that today gets an inline Dismiss button). It calls
   `dismiss_permission`: a cancellation, never a rejection.

The free-text row's placeholder is the declared one; otherwise
`Type something else…` when options precede it, and today's
`Type an answer` when it is the only row.

## 2. Appearance

- **Slot:** where `pending-question-bar` was — a child of the bottom column,
  before the queue and the composer, `w_full().max_w(TRANSCRIPT_WIDTH)`,
  `mb(8px)`. Drawn only while `question_view` is `Some`. Element id and debug
  selector `question-dock`.
- **Container:** the composer card's own frame, so the two read as a pair —
  `bezel::theme::Theme::surface_radius()`, `border_1` in the bezel theme's
  `border` (the same `composer_border` hairline), `card_glass_bg()`, 4px
  padding. Bezel first, as `CLAUDE.md` requires: nothing here is a
  hand-picked colour. **No `theme.warning` anywhere**, and no `?` glyph.
- **Caption:** `footnote`, `text_muted`.
- **Body:** `callout`, `text`, wrapping, never truncated. Capped at 160px with
  its own vertical scroll, so a multi-line Bash command cannot push the
  composer off the pane.
- **Rows:** a vertical list, full width, 2px apart. Each row is bezel's own
  menu row, `bezel::ui::popover::menu_row_nav(theme, false, highlighted,
  fade)` — the row the chat's pickers already use, with its padding, inset
  radius and hover fade — with `debug_selector` `permission-option-{id}` for an option
  (the selector the inline buttons used to carry, so existing tests find the
  one clickable surface), `question-dock-other` for the free-text row,
  `question-dock-dismiss` for Dismiss.
  - Left: a 20px number badge (`1`…`9`), 1px `border` outline, `text_faint`;
    filled (`border_strong` background, `text`) on the selected row. Rows
    past the ninth have no badge.
  - Label: `callout`, `text`. Description, when present, below it in
    `footnote`, `text_muted`, aligned with the label.
  - The selected row is `menu_row_nav`'s `highlighted` row (bezel's
    `card_selected_bg`), and hovering selects (§3), so there is only ever
    one lit row.
  - **Rejections are not tinted.** `is_rejection` stays in the data; the
    colour goes. A red "Reject" reads as an error on a legitimate choice.
- **Free-text row:** its badge, then the existing hand-rolled answer field
  (`question_answer`, caret and all), without the Send and Cancel buttons —
  Enter sends, Escape cancels, as in the field today.
- **Hint line:** `↑↓ select · ⏎ confirm · esc cancel`, `caption2`,
  `text_faint`, right-aligned under the rows.
- The `Show` link is gone: the dock already holds the whole question.

## 3. Keyboard and focus

### State

```rust
struct QuestionDockState {
    focus: FocusHandle,
    selected: usize,
    for_request: Option<u64>,
}
```

One field on `Chat`. When `for_request` differs from the visible
`QuestionView::request_id`, `selected` resets to 0 and `for_request` takes
the new id: every new question starts on its first row. The index is clamped
to the row count on every read. The clamp and the key→action mapping are pure
functions in `question_dock.rs`.

### Keys

The dock root carries `key_context("ChatQuestionDock")` and tracks
`QuestionDockState::focus`. Bindings registered next to the chat's others
(`mod.rs`, near line 2499):

| key | action |
|---|---|
| `up` / `down` | move the selection; stops at the ends, no wrap |
| `enter` / `return` | activate the selected row |
| `escape` | `cancel_question(request_id)` — today's semantics |

Digits `1`–`9` go through a raw `on_key_down` on the dock (nine bindings
would be noise): a digit activates that row if it exists and is ignored
otherwise.

Activating a row:

- `Option` → `respond_permission(request_id, option)`, as the inline button
  did;
- `FreeText` → focus `question_answer.focus` with `for_request` set and the
  prefill seeded, exactly as clicking the field does today;
- `Dismiss` → `dismiss_permission(request_id)`.

In the free-text field, everything behaves as today; in addition, `up`
(bound in `ChatQuestionAnswer`) returns focus to the dock with the last
option selected.

### Mouse

Moving over a row selects it — notifying only when the index actually
changes, so a pointer resting on a row costs no frames; clicking a row
activates it.

### Taking focus

The dock must never take focus from another pane — a terminal in another
worktree the user is typing into. On the first frame that shows a new
`request_id`, and only if focus is **within this chat** (the composer field
or the transcript), the dock focuses itself, deferred to after the frame.
Otherwise it is drawn unfocused and reachable by click.

### Arming

A question's option rows ignore activation by Enter, digits, or clicks for
500ms after the dock first shows that request. The delay counts from the frame
that first draws a request, and an activation that arming blocks neither
answers nor moves the selection. The free-text row, Dismiss, and Escape are not
delayed. The dock takes focus from the composer and sits where the last question
sat, so a key or click already in flight must not answer a question the user has
not seen.

### After an answer

If another question is open, the dock shows it and keeps focus. If none is,
and the dock or the free-text field held focus, focus returns to the
composer, which re-enables with the same frame.

## 4. The transcript record

- **`Entry::Permission`:** the left border becomes `border_strong`, as on the
  Plan and Rewind cards. Title and prompt stay. **No buttons in any state.**
  The status line reads `Waiting for your answer below` (`text_faint`) while
  open, then — unchanged — `Answered: {choice}`, `Dismissed — request
  cancelled`, or `No answer — the turn ended`. The inline Dismiss button is
  gone (it is a dock row now).
- **`Entry::Plan`:** the entries stay on the card; the approval buttons move
  to the dock. The status line reads `Waiting for your approval below` while
  open, then `Approved: {choice}` or `No answer — the turn ended`, unchanged.
- `render_question_answer_row` loses its Send and Cancel controls and is
  drawn by the dock only.
- `pending-question-bar` and `pending-question-show` are deleted.

## 5. Tests and verification

Tests first, per the repo's convention.

**Pure** (`question_dock.rs`):
- `question_view` picks the first open question and skips answered, expired
  and dismissed ones; it sees a plan approval.
- The four caption/body rows of the table in §1, including a Pi question
  (`is_question`, empty prompt) against a plain permission.
- The free-text row exists exactly when `text_input` does — never for a
  plain permission, a plan approval, or a question without one.
- `Dismiss` is the only row of a request with no options and no text input.
- The selection clamps, resets on a new `request_id`, and does not wrap.
- Digits past the row count, and `0`, map to nothing.

**`sirio_acp`** (`claude/permission.rs`): `question_options` carries each
option's description, and `None` where the option has none; the free-text
helper adds a `text_input` to a question that declared none and keeps a
declared one as it is.

**gpui** (`VisualTestContext`, alongside the existing chat tests):
- The dock draws the whole body of a question longer than one line — no
  ellipsis.
- `down` then `enter` answers with the second option; `3` answers with the
  third; `escape` cancels.
- The inline card has no `permission-option-*` descendant while the dock is
  up.
- Neither the dock nor the card paints `theme.warning`.
- The dock does not take focus when focus is outside the chat.
- The existing tests that address `pending-question-bar` /
  `pending-question-show` move to `question-dock`.

**Verification:**
- `cargo nextest run -p sirio_ui -p sirio_acp` — nextest, not `cargo test`.
- `cargo build --workspace --all-targets`: a public struct field breaks
  literals in other crates' test targets, which a per-crate build misses.
- A look at the real app with `Scripts/build-dev.sh`.
- `Scripts/ci.sh` only at the user's request.

## Out of scope

- `AskUserQuestion` payloads with more than one question: only the first is
  surfaced today (`parse_permission_question`) and that stays; Claude
  Desktop's tabbed multi-question layout is a separate change.
- Appearance animation.
- Persisting option descriptions.
