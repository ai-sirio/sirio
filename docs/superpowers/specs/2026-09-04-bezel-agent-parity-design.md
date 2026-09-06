# Bezel agent-pattern parity — design

Date: 2026-09-04
Status: approved
Umbrella: `docs/superpowers/specs/2026-08-31-bezel-gallery-adoption-design.md`.
Follows `2026-09-01-bezel-identity-patterns-design.md` (chat cards and
`bezel-markdown`), which kept behaviour identical; this spec deliberately
changes behaviour where the gallery does, and says so per section.

## Goal

Bring the chat pane to parity with the four **Agent** pattern pages of the bezel
gallery — Composer, Tool calls, Activity, Transcript — as they ship at tag
`v0.1.4` of `crabtalk/bezel` (`apps/gallery/src/patterns/agent.rs`,
`apps/gallery/src/patterns/transcript.rs`). bezel.gallery runs the same
version, so it is the visual reference and the source is the structural one.
Sirio already pins `bezel =0.1.4`; every API the four pages use exists in that
pin (`input::TextField` + `Shape::Grow`, `popover::{Filter, popover_card,
menu_row, anchored_menu_above*, menu_at}`, `Theme::{step_row, step_output,
disclosure}`, `widgets::Takeover`, `loaders::orb`, `scroll::{follow,
scrollbar}`, `card_glass_bg`). No dependency changes.

Reference screenshots taken from bezel.gallery on 2026-09-04 live in the
session scratchpad (`refs/gallery-{composer,activity,toolcalls,transcript}.jpg`)
and are the "expected" side of every visual review below.

## Decisions taken with the user

1. **Scope: the whole Agent group**, not only the Composer page.
2. **The composer field becomes bezel `TextField`.** The hand-rolled chip
   document (`sirio_ui/src/composer.rs`), the custom `ComposerText` element,
   `ComposerPaintTrace` and the composer caret blink are deleted. Skill and
   file mentions become text tokens; images become an attachment strip.
3. **The gallery's card, with the chip row in its bottom row** (revised
   2026-09-04 (22:58) — the user rejected the toolbar above the card):
   the card is field on top, then one wrapping chip row ending in the
   send/stop disc — pill · model · effort · context, then attach ·
   overflow · send. No hint row (the placeholder already carries `/`
   and `@`). The placeholder is the gallery's sentence:
   `Ask anything, or @ to attach a file`, or
   `Ask anything, / for commands, or @ to attach a file` when commands
   exist; the agent's name no longer appears (the pill and the model chip
   already name it).
4. **Four sequential sub-projects, one new module each**, each rewriting its
   code *out of* `chat.rs` into `sirio_ui/src/chat/<module>.rs` as it goes.
   No preliminary refactor step, no parallel agents on `chat.rs`.

## Architecture

`sirio_ui/src/chat.rs` moves to `sirio_ui/src/chat/mod.rs` in sub-project 1
(a `git mv`, no content change) so the new modules can be siblings:

```
sirio_ui/src/chat/
  mod.rs            Chat entity, ACP event ingestion, persistence, tests that stay
  composer_view.rs  sub-project 1 — the card, control row, popups, token parsing
  tool_calls.rs     sub-project 2 — step_row rows, run boxes, verb folds, bodies
  thought.rs        sub-project 3 — Thinking header, Takeover, reasoning box
  transcript.rs     sub-project 4 — turn split, Work zone, day headings
```

Each module is `impl Chat { … }` blocks plus free functions and its own
`#[cfg(test)] mod tests`; `Chat`'s fields stay in `mod.rs`. Every element id
and `debug_selector` that the `sirio` crate's integration tests use today
(`composer`, `slash-popup`, `mention-popup`, `attach-image`, `queued-*`,
`tool-call-toggle-N`, `thought-toggle-N`, `user-bubble-N`, …) is preserved, so
those tests are not touched.

## Sub-project 1 — Composer (`composer_view.rs`)

### Field

- `Chat.composer_field: Entity<TextField>`, built with
  `Shape::Grow { min: 3, max: 12 }` (the gallery's rows), placeholder
  `"Message <agent>… — / for commands, @ for files"` (agent name from
  `agent_badge_name`, updated via `set_placeholder`), and
  `.with_key_context("ChatComposer")`.
- Focus: `Chat`'s composer focus handle **is** the field's
  (`field.read(cx).focus_handle(cx)`); `composer_focus` goes away.
- Changes are observed, not evented (`FieldEvent` is a 0.1.5 API):
  `cx.observe(&field, |chat, _, cx| chat.reread_composer(cx))`.

### Keys (`Chat::bind_keys`)

Context `ChatComposer` keeps only: `enter`/`return` → `Send`,
`shift-enter`/`shift-return` → `bezel::ui::input::InsertNewline`, `escape` →
`Cancel`, `up`/`down` → `PopupPrevious`/`PopupNext`, `tab` → `PopupAccept`.
The three popup actions call `cx.propagate()` when no popup is open, so the
field keeps vertical motion and tab focus traversal. Removed: `Backspace`,
`Delete`, `Left`, `Right`, `SelectLeft`, `SelectRight`, `SelectAll`, `Home`,
`End` and the composer-scoped `ctrl-c` (the field's own `Copy` takes it;
`CopyTranscript` stays on `ChatTranscript`). The `Newline` action and the
corresponding Sirio action types are deleted.

### Tokens replace chips

`reread_composer` runs on every content or caret change, reading
`field.content()` and `field.cursor()` the way the gallery's `reread` does:

- **Slash**: the draft is exactly `/tok` with no whitespace → the command
  popup opens filtered on `tok`. Accepting writes `/name ` as text. The wire
  payload is unchanged (the Skill chip already serialised as `/name `).
- **Mention**: the `@` nearest behind the caret with no whitespace between it
  and the caret → the file popup opens filtered on the token; the candidate
  walk (`mention_task`) is unchanged. Accepting replaces `@tok` with
  `@<path> ` and records `path` in `Chat.accepted_mentions: Vec<String>`.
- **Send**: for each recorded path whose `@<path>` token is still present in
  the text, the token is removed from the text and the path goes into the
  prompt's `mention_paths` (deduplicated) — the same triple
  `ChatPromptBuilder` gets today. A token the user edited is plain text and
  its path is dropped from the record. Recorded paths are cleared on send and
  on `set_draft_text`.
- **Images**: `Chat.attachments: Vec<ImageAttachment>`; the attach button and
  paste keep filling it (`attach_image`), and it serialises into the prompt's
  `images`. Rendered as a chip strip inside the card above the field: each
  chip is `PAPERCLIP` icon + "Image" + a ✕ (`attachment-remove-N`), on
  `surface_raised` at `Theme::control_radius()`.
- **Draft persistence** (F-CORE-WSP-08): `draft_text` returns the field text,
  `set_draft_text` sets it; attachments and recorded paths are not persisted,
  as today.
- **Queue** (D-CHAT-03): `commit_queued_item` reads the same triple, clears
  the field and appends the text to `Chat.queue: VecDeque<String>` — every
  Enter during a turn adds an entry, front first. The queue is drawn as its
  own block **above the card** (between the transcript and the composer,
  where Zed keeps its queued messages), never inside it: a header
  ("1 message queued" / "N messages queued") with a chevron that folds the
  entries and a ghost `Clear all`, then one row per entry — a dot (bright only
  on the front entry, the one the running turn's end sends), the text on one
  ellipsised line, `Send now` and ✕. The list caps at 160 px and scrolls.
  Each turn end — completed or cancelled alike — sends exactly the front
  entry; the rest wait for that turn's end. `Send now` moves its entry to
  the front and cancels the running turn, so the cancelled turn's end sends
  it through that same rule (with no turn running it sends straight away).
  The control snapshot keeps `queuedText` as the front entry and adds
  `queued`, the whole queue as encoded rows. Selectors: `queue`,
  `queue-toggle`, `queue-count-N`, `queue-clear`, `queue-entry-N`,
  `queue-text-<text>`, `queue-send-N`, `queue-remove-N`.

### Card and chip row

The card is field on top, then one wrapping chip row ending in the
send/stop disc. No hint row; the placeholder already carries `/` and `@`.
With no agent configured the row holds only the send disc.

```
div rounded(Theme::surface_radius()) border_1 border_color(theme.border)
    bg(theme.card_glass_bg()) px 4 pt 4 pb 6 flex_col gap 4
  ├ attachment strip (only when non-empty)
  ├ field
  ├ attach error (only when present; the queue is a block above the card)
  └ chip row: one flat wrapping row (flex_row flex_wrap items_center
      gap 6 gap_y 4 px 6) whose direct children are, in order, the mode
      pill (flex_none), the Model chip (the one shrinkable child:
      min_w_0 + text_ellipsis, 56 px floor), the Effort chip
      (flex_none), the context chip (ring + label + percent,
      flex_none), and last the attach · overflow · send trio as one
      flex_none ml_auto group; when a line is full the next chip wraps
      to the next line and the trio follows to the end of whichever line
      it lands on; nothing is ever clipped, and the disc never paints
      over a neighbour.
```

- The chips sit on the card's surface: no fill and no border at rest,
  only the hover wash, so they still read as pressable. The send disc
  keeps its own look.
- The pill and chips keep their current content, selectors and click
  behaviour; they are re-measured to 24 px height at
  `Theme::control_radius()` so they sit level with the send disc.
- **Send disc**: 24 px, `rounded_full`, `ARROW_UP` at 14 px. Ready
  (`can_send()`): `bg(theme.solid)`, icon `theme.on_solid`, `cursor_pointer`,
  hover opacity 0.9. Not ready: `bg(theme::ink(0.06))`, icon `text_faint`, no
  hover, no pointer. Selector `chat-send`. While a turn streams the disc shows
  `STOP` and calls `cancel_turn`.
- Attach button: `PAPERCLIP` at 14 px in a 24 px square, hover
  `element_hover`; selector `attach-image` as today.

### Popups

All on `bezel::ui::popover`; the hand-rolled absolute cards go.

- **Command and file pickers**: state is a `popover::Filter` over the
  candidates (`refilter` on the token, `step(±1)` on up/down,
  `active_item()` on enter/tab/click). Rows are `menu_row(theme, active,
  Fade)` with the name; a command's description is its tooltip (as in the
  uncommitted work this supersedes). The card is `popover_card(theme)` at
  280 px, opened **upward** with `anchored_menu_above_at(id, point, …)` at
  the top-left of `field.offset_bounds(token_start, window)` minus 4 px, so
  the picker follows the caret as the field grows. Selectors `slash-popup`,
  `mention-popup`, `slash-option-<name>` stay.
- **Model, mode, context and overflow**: `anchored_menu_above` for the pill
  and the model chip, `anchored_menu_above_end` for the context ring and the
  overflow button (right-side triggers open leftward). Contents are
  `popover_card` + `menu_row`/`menu_row_nav`; the model search row is a
  `TextField` with `Shape::Line` (the hand-rolled search caret,
  `model_search_blink` and `caret::field_value` use in the chat go). The
  effort choices keep their wrapping chip row (#233).
- Escape closes the topmost open surface (as `cancel` does today).

### Out of this sub-project

The question-answer field (F-CHAT-25) keeps its own caret for now; moving it
to `TextField` is a follow-up.

### Deletions

`sirio_ui/src/composer.rs` and its `pub mod composer`; `ComposerText`,
`ComposerPaintTrace`, `composer_blink`, `composer_caret_sig`,
`composer_paint`, `composer_focus`, `model_search_blink`,
`model_search_caret_visible`; the composer-scoped key bindings listed above.

### Tests

- Unit: token reader (slash / mention / none, backspace over the `@` closes
  the picker, mention behind whitespace is not a mention), send serialisation
  (recorded path present → `mention_paths`; edited token → text only;
  attachments → `images`), `can_send` on empty / whitespace / attachments-only
  drafts, key routing (`up` propagates with no popup, steps with one).
- View tests drive the field with `set_content` and `simulate_keystrokes` as
  `browser.rs` does; each existing composer test is rewritten in place
  against the same selectors.
- Every `TestAppContext` that mounts a `Chat` calls `bezel::ui::input::init`
  before `chat::init` — the app already does at boot (`main.rs`).

## Sub-project 2 — Tool calls (`tool_calls.rs`)

### Row

One call is `Theme::step_row(icon, verb, detail, meta, failed, expanded)`:

| slot     | value |
|----------|-------|
| icon     | from `kind`: Read → `BOOK`, Edit → `PEN`, Execute → `TERMINAL`, Search → `MAGNIFER`, Fetch → `DOWNLOAD`, Think → `CPU`, Delete → `TRASH_BIN_MINIMALISTIC`, Move → `ARROW_RIGHT`, anything else → `WIDGET` |
| verb     | the `kind` word (the protocol's category), "Tool" when the kind is unknown |
| detail   | `title`, `min_w_0` + ellipsis |
| meta     | duration when known (`412ms` under a second, `1.4s` over — `took()` copied from the gallery); otherwise the status word (`running`, `pending`, `failed`, `cancelled`) |
| failed   | status is `failed` or `cancelled` |
| expanded | `Some(open)` when the call has a body (text output, diff, locations, raw output); `None` — no chevron — when it has nothing to open |

The row is `.id(("tool-call-toggle", index))` with the existing selector, and
its click toggles `expanded` on the entry as today.

### Duration

`Chat.tool_started: HashMap<String, Instant>` is written when a `ToolCall`
entry is created and consumed on the first terminal status, storing
`duration_ms: Option<u64>` on `Entry::ToolCall`. Persistence adds
`duration_ms: Option<u64>` with `#[serde(default)]` to
`ChatEntry::ToolCall`; restored calls without it show the status word.

### Grouping

- A **run** — consecutive `ToolCall` entries, the range
  `tool_call_run_bounds` already computes — is one box:
  `rounded(Theme::panel_radius()) border_1 border_color(theme.border)
  overflow_hidden`, with `border_t_1` between rows (`first` skips it). A
  single call is the same box with one row.
- Inside a run, consecutive calls of the **same verb** fold into one header
  row `step_row(icon, verb, "· N", None, any_failed, Some(open))` with the
  members under a hairline at `pl 16` — `chunk_by` on `kind`, the gallery's
  own finding. Fold state is view state: `Chat.open_verb_folds:
  HashSet<usize>` keyed by the fold's first entry index, not persisted,
  default closed.
- `SubagentTask` renders as a fold whose header is its own row (`CPU`,
  "Task", the title) and whose members are its `tool_calls`.

### Body

Under the row, inside the box, when `expanded`: text output through
`Theme::step_output(id, text)` (bezel's capped, scrolling well); diff
previews, file links and the edit summary keep Sirio's renderers
(`render_tool_diff`, the F-CHAT-23 location links, `render_edit_summary`)
unchanged, stacked with `gap 6` under `px 12 pb 8`.

### Deletions

`render_tool_call_card`'s header, `render_tool_call_group`,
`collapsed_tool_row_text`, `TOOL_CALL_GROUP_*` constants. `group_expanded`
was removed in sub-project 2 itself (nothing read it once the run box
replaced the "N steps" header); sub-project 4 adds the Work zone's own
`work_open`.

### Tests

Icon and verb mapping per kind; `took()` formatting; run boxing (one box per
run, hairlines between rows, single call boxed); verb folding (`Read, Read,
Read, Run` → one fold of three plus one row; a prose entry between calls
breaks the run); duration recorded on completion, absent for restored
entries; failed rows flagged; no chevron for a call without a body. Existing
tool-call tests are rewritten against the same selectors.

## Sub-project 3 — Thinking / Activity (`thought.rs`)

### Header

The gallery's `Activity::header`, verbatim in structure: `self_start`,
`flex_row items_center gap 6 px 4 py 5 rounded(Theme::control_radius())
cursor_pointer hover(bg ink(0.03))`; a 14 px glyph slot holding
`loaders::orb(Orb::Cluster, …)` while the thought streams (the current
`loading::thinking_indicator`) and `Theme::disclosure(open)` once settled;
label `TextStyle::Callout` in `text_muted`: `"Thinking"` while streaming,
`loading::thought_label(elapsed)` — `"Thought for Ns"`, or `"Thought"` when
no duration is known — once settled. Selector `thought-toggle-N` stays.

### Duration

`Entry::Thought` gains `started: Option<Instant>` (view state) and
`duration_ms: Option<u64>`; the first chunk sets `started`, the entry settles
(next non-thought entry or turn end) and stores the elapsed time.
`ChatEntry::Thought` gains `duration_ms: Option<u64>` `#[serde(default)]`.

### Open state — behaviour change

`expanded: bool` becomes `open: widgets::Takeover`. `open.get(streaming)` is
what paints: a live thought is open while it streams and folds when it
settles, unless the reader pressed the header, after which their choice
holds. This replaces F-CHAT-21's "collapsed until opted in, live or
historical" for the live case only; a restored thought still opens closed.
Persistence stores nothing for it.

### Body

`ml 10 pl 12 border_l_1 border_color(theme.border)` around a `relative`
container capped at `max_h 160`: the scrolling well (`overflow_y_scroll`,
`track_scroll`) holding the text at `Callout` in `text_muted.opacity(0.7)`
through `render_plain_text` (selection, F-CHAT-31), a 20 px top strip painted
with `linear_gradient(180, theme.bg → theme.bg.opacity(0))`, then
`scroll::follow(&scroll, &follow)` (pinned to the newest line while
streaming; a new thought re-follows) and `scroll::scrollbar`. `FollowState`
and `ScrollbarState` live on the entry's view state keyed by entry index
(`Chat.thought_scroll: HashMap<usize, ThoughtScroll>`).

The transient generating spinner (#239, `chat-generating-spinner`) adopts the
same header row so a run-in-progress and a settled thought share one shape.

### Tests

Label by state (streaming / settled with / without duration); Takeover
semantics (auto-open while streaming, folds on settle, a press holds);
duration measured and persisted, absent on restore; the body renders the
gradient strip and scroll pair only when open.

## Sub-project 4 — Transcript (`transcript.rs`)

### Turn split — behaviour change

A turn is a `User` entry and everything up to the next `User` (the existing
`TurnSegment`). The gallery's rule: **the answer is the prose after the last
tool call; everything before it is interim.** `answer_from` = one past the
last `ToolCall` or `SubagentTask` in the turn, or the turn start when there
is none. Entries before `answer_from` — thoughts, interim `Assistant` prose,
tool runs — make the **Work zone**; `Assistant` entries from `answer_from` on
are the answer and render as today (`MarkdownBody`). `Permission`, `Plan`,
`Error` and `TurnFooter` entries render where they are, outside the zone.

### Work zone

- Header: `Theme::disclosure(open)` + `"Worked · N steps"` (Callout,
  `text_muted`; `tool_group_label` stays), the gallery's `work_header` shape;
  selector `work-toggle-<turn>`. `N` counts tool calls in the zone (subagent
  members included). A turn with no tool calls has no header; its thoughts
  render on their own as in sub-project 3.
- Open state: `Chat.work_open: HashMap<usize, widgets::Takeover>` keyed by
  the turn's `User` entry index; `get(auto)` with `auto` = the turn is the
  streaming one, so the zone is open while the agent works and folds when the
  turn ends unless pressed. Not persisted.
- Body: `ml 10 pl 12 border_l_1 border_color(theme.border) flex_col gap 8`,
  holding in order: thought headers (sub-project 3), interim prose as
  `Callout` in `text_muted`, and each tool run in its box (sub-project 2).
  Interim prose keeps `render_plain_text` selection.
- The virtualised `list()` stays: a zone's body is drawn by the turn's first
  row, with the zone's member entries contributing no rows while it is
  folded and their own rows while it is open — the same trick the current
  tool-call groups use with `tool_call_run_bounds`.

### Day headings

Where the calendar day changes between one turn and the next, a heading row:
`py 8`, `TextStyle::Subheadline`, `FontWeight::MEDIUM`, `text_faint`,
`popover::tracked_upper(label)`. Labels are Sirio's words (bezel has no
clock): `"Today"`, `"Yesterday"`, otherwise `"%a %-d %b"` (`Mon 2 Sep`).
`Entry::User` becomes `Entry::User { text, at: Option<DateTime<Local>> }`;
`ChatEntry` gains `at: Option<i64>` (unix seconds) `#[serde(default)]`.
Turns without a timestamp get no heading and never break the sequence.

### Kept from Sirio

The user bubble (already the gallery's raised, right-aligned pill),
`TurnFooter`, the F-CHAT-22 "Turn: …" fold for older turns (it folds the
whole turn; the Work zone folds only the interim), the copy affordance on
answers, the pending-question bar and the virtualised list with its own
follow logic. `scroll::follow`/`scrollbar` from bezel are not adopted for the
transcript itself.

### Deletions

`group_expanded` on `Entry::ToolCall` and in persistence (superseded by
`work_open`); whatever of `render_tool_call_group` survived sub-project 2.

### Tests

`split`: last tool call decides `answer_from`; a turn with no tools is all
answer; a `SubagentTask` counts as a tool. Zone open while streaming, folds
on turn end, press holds. Step count. Day heading appears exactly where the
day changes and not for undated turns. Existing transcript tests rewritten
against the same selectors.

## Cross-cutting constraints

- bezel and gpui pins unchanged; the reference is tag `v0.1.4`, not `main`.
- Every persistence change is additive with `#[serde(default)]`; a database
  written by today's build restores unchanged.
- Sirio's `Icon` enum stays for the rest of the app; the chat rows use
  `bezel::ui::icons` directly because `step_row` takes bezel asset paths.
- Selectors used by `sirio`'s integration tests are preserved verbatim
  (listed under Architecture).
- Workspace gates (`Scripts/ci.sh`, `Scripts/ci-linux.sh`) only on the user's
  explicit request; iteration is `cargo test -p sirio_ui` and
  `cargo clippy -p sirio_ui`.

## Delivery

- One branch per sub-project from `main`, in order 1 → 4; each ends in a PR
  reviewed against the reference screenshots and the acceptance list below.
- Sub-project 1 landed on branch `feat/composer-bezel-textfield` (2026-09-04).
- Revised 2026-09-04: the control row inside the card was rejected by the user; the card is the gallery's and the controls sit in a toolbar above it.
- Revised 2026-09-04 (22:58): the toolbar above the card was rejected by the user; the chip row lives in the card's bottom row, in place of the hint text.
- Sub-project 2 landed on branch `feat/tool-calls-step-row` (2026-09-04).
- Sub-project 3 landed on branch feat/thought-takeover (2026-09-04).
- Sub-project 4 landed on branch feat/transcript-work-zone (2026-09-04).
- Implementation by a pi or opencode agent in a Herdr pane (models
  `opencode-go/gpt-5.6-luna` and `opencode-go/muse-spark-1.3-contributor`),
  one writer per sub-project, driven by a task file that carries this spec's
  section, the acceptance list and the TDD order. The main session
  orchestrates and reviews: diff review, `cargo test -p sirio_ui`, a run of
  the app on Windows in isolation (`SIRIO_DB`, `SIRIO_SOCKET_ENABLE=0`) and a
  side-by-side screenshot against `refs/gallery-*.jpg`.
- **Precondition**: the working tree's uncommitted changes (tab redesign in
  `sirio/src/main.rs`, installer fixes in `sirio_registry`, the skill-popup
  tooltip work in `chat.rs`) are committed or stashed before branch 1 opens.
  The skill-popup change is superseded by sub-project 1.

### Acceptance per sub-project

1. Composer: card matches the reference (glass, radius, spacing, disc); `/`,
   `@`, attach, queue, draft persistence and send payload behave as before;
   `up`/`down` move the caret with no popup open; IME composition works.
2. Tool calls: rows match the reference (icon, verb, detail, meta, red on
   failure, chevron only with a body); runs boxed; same-verb folds; durations
   on new calls.
3. Thinking: header and box match the reference; live thoughts open and
   fold; "Thought for Ns" on new thoughts.
4. Transcript: Work zone per turn with the gallery's split; day headings;
   long transcripts still virtualise.

## Out of scope

- The Diff, Terminal, Thinking orbs and Blob avatars gallery pages.
- The question-answer field's caret (F-CHAT-25).
- Any bezel or gpui version change; any new chat feature not on the four
  reference pages.
