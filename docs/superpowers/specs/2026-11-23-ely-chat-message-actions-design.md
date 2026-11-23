# Ely chat, group A, delivery 2: message actions

Ely programme step 3, delivery 2. One PR, a `feat:` in the 0.48.1 cycle: version 0.49.0.

## 1. Intent

Enzo's programme moves Sirio's AI chat onto what Ely offers, one delivery per PR. Delivery 1 made each message say
more about itself. This one lets the user act on a message:
- **copy** any message, the user's as well as the agent's;
- **stop** a turn, and have a stopped Claude turn say it was stopped;
- **continue** an answer that was cut off: by the token limit, by the turn-request limit, or by Stop;
- **quote** a message, or a selection of one, into the next prompt.

**Programme rule:** an action shows only for agents whose transport supplies what it needs. Copy and quote need nothing
from the transport. Continue needs a conversation the agent still has, decided by the table that already decides Retry
(§5.3).

### Goals

- Every finished message has Copy and Quote. The last reply of the transcript shows its actions all the time; every
  other message shows them on hover.
- Continue appears only where pressing it continues the same conversation, and it sends what it shows.
- A stopped Claude turn ends as `Cancelled`, like an ACP one, and its footer says so.
- A quote survives everything a prompt survives: the queue, Retry, a hand-back to the composer, persistence and a
  restore.

### Non-goals

- **Regenerate moves to delivery 3**, with edit and branches (t2s for Enzo, Q1, 2026-11-23). A real regenerate replaces
  an answer: it truncates the conversation and keeps the old answer as an alternative, which is the branch machinery of
  delivery 3. No transport truncates history today (§2), and a coding agent has already run its tools: the files do not
  go back. Resending the same prompt at the end of the conversation would be "ask again", not "regenerate", and would
  redo the agent's edits. Ely's `regenerate_button` and `MessageActions::on_regenerate` stay unused here.
- No edit (`MessageActions::on_edit`) and no 👍/👎 (`MessageActions::rating`): edit is delivery 3, ratings are a
  programme exclusion.
- No second Stop on the row: Stop stays in the composer and on Escape, as today.
- No Continue on a restored chat (§5.3).
- Not the programme's exclusions: voice, link preview, ratings, shared conversations, project knowledge, follow-up
  suggestions, model parameters, audio/video, system-prompt editor.

### Anti-goals

These would make this delivery a failure even if every button is visible:
- a Continue that sends to an agent that has lost the conversation (an ACP chat that restarted);
- a Continue, or a quote, that reaches the agent by a path other than `submit_prompt`: a second resend path;
- a Continue that is drawn while pressing it would do nothing (a turn running, the agent connecting);
- a quote the agent never received, or a user row that shows a quote that was not sent;
- an action row that redraws the window while nothing changes (Ely animations, see delivery 1's `StillMotion`);
- Ctrl+C swallowed, or selectable text placed inside a clickable element (`docs/agents/architecture/text-selection.md`).

## 2. What exists today (recon on `a2096b3b8`, 2026-11-23)

- **Ely** (`rust/vendor/ely/crates/ely-chat/src/actions.rs`):
  - `MessageActions`: a quiet row with `CopyButton` always, plus edit, regenerate and 👍/👎 when their handler is set;
  - `regenerate_button`, `stop_button` and `continue_button`;
  - `QuoteReply`: a rule, an author, the first line of the text, and an optional ✕.

  Sirio uses none of these. `CopyButton` (`ely-core/src/buttons/copy.rs`) writes with `cx.write_to_clipboard`, as
  Sirio's `copy_local_text` does. It confirms with a one-shot `motion::FAST` animation and a timer, and keeps its state
  in `use_keyed_state` inside `RenderOnce`.
- **Rows** (`chat/transcript.rs:90`, `enum Entry` at `chat/mod.rs:521`):
  - A user row is a `MessageBubble` with no actions.
  - An assistant row has one hover "Copy" (`assistant-copy-{i}`, `transcript.rs:169-212`). It copies the Markdown
    source through `copy_local_text` (`mod.rs:5656`).
  - The transcript's context menu (`TranscriptContextItem`) has Copy, Select All, Open link and Copy link address.
  - Ctrl+C and Select All copy rendered text (`Entry::plain_text`).
  - A turn can hold several `Assistant` entries between tool calls, and ends with an `Entry::TurnFooter`.
- **Stop:**
  - The composer's send button becomes Stop while a turn streams. Escape and `surface.chat.stop` reach `cancel_turn`
    (`mod.rs:7265`).
  - `cancel_turn` cancels on the client, clears `streaming` and `last_prompt`, sets `stop_end_pending` and finishes the
    reply. The late end of the turn writes the footer (`settle_turn`, `mod.rs:4616`), labelled by `turn_end_label`
    (`mod.rs:9471`).
- **Claude's Stop is mislabelled** (pre-existing):
  - The real CLI answers an interrupt with `subtype: error_during_execution`, `is_error: true`, `stop_reason: null`,
    `terminal_reason: aborted_streaming` and no `result` text (the capture
    `sirio_claude/tests/fixtures/trace_result_interrupted.json`).
  - `Fold::apply_result` (`sirio_chat/src/claude/events.rs:958-1000`) knows it asked (`self.interrupted`). Still,
    `stop_reason_label` passes `error_during_execution` through.
  - So the footer reads "ended", not "cancelled", and `sirio_chat`'s `is_stopped_reason` says the turn was not stopped.
  - The synthetic fixtures (`sirio_chat/tests/fixtures/claude_fixture.py`, the richer stub) add a
    `result: "Interrupted by user"` that the real CLI does not send, and the test
    `a_stopped_turns_result_keeps_the_events_of_a_turn_that_ended` (`events.rs:2124`) pins it as a reply chunk.
- **The queue after a Stop:** `end_turn` sends the queue's next item on every turn end, a stopped one included. Stop
  with an item queued is the "redirect" gesture (`send_queued_entry_now`, test
  `stopping_via_click_with_a_queued_item_still_sends_it`). The UI does not consult `sirio_chat`'s own stopped rule.
- **Retry** (`chat/retry.rs`):
  - `LastPrompt`, `RetryAction` and the table `retry_action`.
  - `retry(index)` (`mod.rs:7627`) dispatches, with `retry_refusal` (`:7677`).
  - `resend_last_prompt` (`:7696`) calls `submit_prompt` (`:6692`), the one wire path: it pushes `Entry::User`, sets
    `last_prompt` and calls `client.prompt_content`.
  - `finish_retry` and `hand_prompt_back` cover a relaunch.
  - `last_prompt` is `None` once a turn ends or is stopped.
- **Transports:**
  - Claude is native. Codex, OpenCode, Pi and omp run over ACP. Native transports for the four are designed, not
    built.
  - `LaunchSpec::resumes_on_retry` is true for Claude (`--resume`) and false for ACP: Sirio only calls `session/new`.
  - No transport truncates or forks a conversation in Sirio. ACP's schema has `session/load` and `session/fork`
    (unstable), both unused. The turn rail's rewind restores files only.
  - Both transports surface `MaxTokens` and `MaxTurnRequests`.
- **Composer:**
  - `set_composer_text` (`composer_view.rs:439`) replaces the draft.
  - There is no quote, reply-to or insert API.
  - `hand_prompt_back` refills the composer from `LastPrompt.draft`.
- **Persistence:** `ChatEntry::UserMessage { text, at }` and `AssistantMessage { text }`, with no message ids.

## 3. Decisions (t2s for Enzo, 2026-11-23, all as recommended)

| # | Question | Decision |
|---|---|---|
| Q1 | What Regenerate means | Moved to delivery 3, with edit and branches. This delivery is copy, stop, continue and quote. |
| Q2 | Continue | On the last reply, after a token or turn-request limit or after Stop. It is sent through `submit_prompt` as a visible user row "Continue". It is hidden for an ACP chat that restarted. Claude's Stop label is fixed: `Cancelled`, with no "Interrupted by user" in the reply (the builder check found the real CLI sends no such text: the fix is the label, plus dropping any result text of a stopped turn). |
| Q3 | Quote | Quote on a whole message, and Quote selection in the context menu. One quote at a time. It becomes a `> ` block at the head of the prompt. |
| Q4 | Look | Ely's row: always visible on the last reply, on hover on the others. Copy on user messages too. |
| — | Row build (t2r's call, accepted) | Ely `MessageActions` unchanged for Copy, plus Ely's `IconButton` for Quote and `continue_button`, in a Sirio row. No vendor change. |
| — | Calls accepted with them | Stop stays in the composer. Edit and 👍/👎 are never set. The socket's actions exist in debug builds only. Continue's reason is kept in memory only. Continue and Retry share one table and one path. The quote lives in the prompt's text. |

## 4. Approaches considered (the action row)

- **A, chosen: compose in Sirio from Ely parts.**
  - `MessageActions::new(id, text)` with no handler set gives Ely's Copy, unchanged.
  - Beside it go an Ely `IconButton` (Ghost, `ControlSize::Sm`, tooltip "Quote") and, when due, Ely's `continue_button`.
  - Sirio's wrapper owns the visibility (always, or `group_hover`), the debug selectors and the debug readout.
  - No vendor diff.
- **B: extend `MessageActions` in the vendor** with `on_quote` and `on_continue`, as local change L16. It gives one
  component, at the cost of a vendor diff to carry through every upstream sync. Rejected.
- **C: hand-made Sirio buttons**, like today's Copy. Rejected: it goes against the programme, whose point is Ely's
  components.

## 5. Design

### 5.1 The action row

**Which rows have one:**
- every `Entry::User`;
- every `Entry::Assistant` that is not streaming.

A reply still streaming (`stream.is_some()`) has no row, because what it would copy is not final. A row has none of
these either: `Thought`, `ToolCall`, `Notice`, `Error` and the others keep what they have.

**What a row holds**, in this order:
- **Copy**, through Ely's `MessageActions`. It copies the entry's `text`:
  - for an assistant entry, the Markdown source, as today's Copy does;
  - for a user entry, the text as sent, a quote block included.
- **Quote** (§5.4).
- **Continue**, only on the row where §5.3 puts it.

**When it shows:**
- The row under the **last** `Assistant` entry of the transcript is always visible, once its turn is over.
- Every other row is invisible until its message is hovered, through `group_hover`, as today's Copy is.
- An invisible row keeps its space, so hovering does not move the transcript.
- The row is outside the selectable text of its message, so selection, Select All and Ctrl+C are unchanged.

**Replaces:** today's hand-made `assistant-copy-{i}` button and its "Copied ✓" go. Ely's `CopyButton` confirms by
itself, so `copy_local_text` and `CopyTarget::Assistant` lose their caller. Builder: delete what is left unused; keep
anything another caller still needs.

**Redraw cost:** the row adds no animation that runs without a press. Ely's `CopyButton` animates once per press, for
`motion::FAST`. The E2E measures the redraws of an idle chat whose last reply shows the row, and compares them with
main (§7).

### 5.2 Stop, and Claude's Stop label

- Stop is unchanged in the UI: the composer's button, Escape and `surface.chat.stop` all reach `cancel_turn`.
- **The fix is in `Fold::apply_result`.** When `stopped` (the worker sent the interrupt), the turn's stop reason is
  `"Cancelled"`, whatever the result's `subtype` or `stop_reason`.
- A stopped turn's result text, if a CLI ever sends one, is not pushed as an `AgentMessageChunk`: it would be the
  CLI's sentence about the stop, not the agent's. The real CLI sends none today.
- The rest of the stopped result is unchanged: usage, cost and the `ResultSummary`.
- So the footer reads "cancelled" and `is_stopped_reason` is true for Claude as for ACP. `ChatStatus` becomes
  `Stopped` for a stopped Claude turn, as it already is for ACP.
- **The queue is unchanged:** a Stop with an item queued still sends that item when the stopped turn ends (the
  redirect). The UI's drain never read `sirio_chat`'s stopped rule, and this delivery does not make it.
- **The fixtures follow the capture:** the richer stub and `claude_fixture.py` answer an interrupt with the captured
  shape (no `result` text). The test that pinned the chunk asserts `Cancelled` and no chunk.

### 5.3 Continue

**Text:** Continue sends the prompt `Continue`. It is visible in the transcript as an ordinary user row, because that
is what the agent receives.

**Reason**, kept in memory:
- `Chat` keeps `continue_reason: Option<ContinueReason>`, set in `settle_turn` from the stop reason of the turn that
  ended:
  - `MaxTokens` gives `TokenLimit`;
  - `MaxTurnRequests` gives `TurnLimit`;
  - `Cancelled` gives `Stopped`.
- Any other end clears it, and so does a failure (`fail_turn`), since that row has Retry.
- It is also cleared by:
  - every `submit_prompt`, which is the next turn;
  - every new connection, except the one Continue's own resume starts (an ACP restart is a new, empty session);
  - clearing or starting a new conversation.
- It is never persisted, so a restored chat has no Continue.

**Where:**
- On the action row of the last `Assistant` entry of the last turn.
- When that turn has no `Assistant` entry (stopped during a tool call), on the last turn's footer row instead, as a
  lone `continue_button`.
- Never on more than one row.

**When**, by the same table as Retry. `retry_action` gains no new arguments. Continue is decided as a `TurnFailed`-like
row whose prompt is the `Continue` prompt:
- the client is up: `Resend`;
- the client is gone, and the launch resumes with a known session (Claude): `ResumeThenResend`;
- the client is gone otherwise (ACP): `RelaunchToComposer`. Continue is **hidden** in this case: an ACP chat that has
  lost its process would restart without the conversation.

  Hiding it here also keeps `RelaunchToComposer` out of Continue's reach entirely.

Continue is also hidden whenever `retry_refusal` would refuse the action: a turn is running, or the agent is
connecting. It is hidden too when the queue is held, which a failed turn sets and which clears the reason anyway.

**The path:**
1. Pressing Continue sets `last_prompt` to the Continue prompt: text `Continue`, no mentions, no images, draft
   `Continue`.
2. It then dispatches exactly as `retry` does for that action:
   - `Resend` calls `resend_last_prompt`, then `submit_prompt`;
   - `ResumeThenResend` relaunches with `--resume`, then `finish_retry` resends.
3. If the resume is refused, `hand_prompt_back` puts `Continue` back in the composer with today's notice.

Builder: factor the dispatch out of `retry` so that both call one function, not a copy.

**Queue:** a Stop with an item queued sends that item when the stopped turn ends (today's redirect). That item's
`submit_prompt` clears the reason, so Continue never shows over it. With nothing queued, Continue sends at once; an item
queued later goes when the Continue turn ends, by today's rule.

**Per agent:**

| Agent | Transport | Continue while the client is up | After the client is lost |
|---|---|---|---|
| Claude | native | yes | yes (`--resume`) |
| Codex, OpenCode, Pi, omp | ACP | yes | hidden |

### 5.4 Quote

**Starting a quote:**
- **Quote** on a row quotes the whole message: its `text`.
- **Quote selection** is a new item in the transcript's context menu. It shows when the selection is not empty and lies
  within one message, and it quotes the selected text.
- Builder: if a selection spanning several messages is cheap to quote as one block, allow it. Otherwise hide the item
  for such a selection.

**In the composer:**
- The composer holds `quote: Option<Quote { author, text }>`. `author` is "You" for a user message and the agent's
  name for a reply.
- While it is set, Ely's `QuoteReply` sits above the field with its ✕ (`on_remove` clears it).
- A second quote replaces the first.
- The quote is in memory with the draft. It is not part of the stored draft unless the draft is stored (builder: follow
  what the draft does).
- Quoting focuses the composer.

**On send:**
- `assemble_prompt` puts the quote at the head of the prompt's text: every line of the quoted text prefixed with `> `
  (an empty line becomes `>`), then one empty line, then the typed text.
- The quote is cleared when the prompt is sent.
- A quote with no typed text is allowed: the prompt is the quote alone.
- `author` is not sent.
- From here the quote is ordinary prompt text, so `LastPrompt.text`, the queue, Retry, persistence and a restore carry
  it with no new field.
- `LastPrompt.draft` keeps the raw draft with the quote block in front of it, so a hand-back can restore both.

**Reading a quote back** (one function, used by every reader). A user text whose leading lines all start with `>`,
followed by an empty line or the end of the text, splits into a quote and the rest:
- **The user row** draws the quote as Ely's `QuoteReply` above the bubble's text, with author "Quote", because the
  author was not sent. The rest is drawn as today.
- **A hand-back** (`hand_prompt_back`) and a refill split the same way: the quote goes back in the composer's
  `QuoteReply`, the rest in the field.

**Accepted consequence:** a user who types a leading `>` block by hand sees it drawn as a quote. What the agent
receives is the same either way.

**Copy** on a user row copies the whole text, quote block included.

### 5.5 Debug surface (debug builds only)

**`surface.chat.read`**, per row: which actions the row draws now and whether they are visible (`always`, `hover` or
`none`), e.g. `actions: ["copy", "quote", "continue"]`, `actionsShown: "always"`. Assistant and user rows alike. This
is what the row shows, so the E2E asserts what is drawn, not only what an action maps to (delivery 1's lesson).

**`surface.chat.read`**, for the composer: the quote held, if any.

**`surface.chat.transcript`** gains:
- `action=copy|quote|continue entry=N`, which runs the same handler as the click and is refused with the reason when
  the row does not show that action;
- `action=quoteSelection` for the current selection.

Release builds serve none of these. `served::served_methods(debug)` stays the one list.

### 5.6 What stays as it is

- Retry, the held queue, the limit card, the reveal and the streamed build.
- The transcript menu's existing items, Select All, Ctrl+C, and code blocks' own Copy.
- The composer's Stop.

## 6. Constraints

- **No vendor change.** If one turns out to be needed, it is recorded in `LOCAL-CHANGES.md` as the next L number, and
  t2s hears of it first.
- **Diff main's calls in any rewritten function.** `retry` is factored and `assemble_prompt`, `settle_turn` and
  `hand_prompt_back` grow, so the diff of main's calls inside each is part of the review.
- **Text selection rules** (`docs/agents/architecture/text-selection.md`): the row's buttons are never inside
  selectable text, and selectable text is never inside a button.
- **Performance:** every label is a `&'static str` (`sirio_perf`), never message text.

## 7. Testing and evidence

Testing is end to end, against the real app with the stub agents of delivery 1 (`Scripts/Tests/stubs/richer`, stub
`claude` and `opencode acp`), extended where a scenario needs it.

**Unit tests come first, and only for the two units that must be proven alone.** Their failure modes are written
before the code:
- **The quote split and join:**
  - a round-trip of the head block;
  - an empty quoted line, `>` with and without a space, a `>` block not followed by an empty line;
  - a text that is all quote, CRLF, a quote inside the text rather than at its head.
- **Continue's decision:** it is shown and with which action for each reason × client up or gone × resumes × session
  known × streaming × connecting × queue held, and cleared on each of the events listed in §5.3.

The Claude Stop fix changes `a_stopped_turns_result_keeps_the_events_of_a_turn_that_ended` to assert the new
behaviour: `Cancelled`, no chunk.

**E2E: a new `Scripts/Tests/chat_message_actions_e2e.py`**, modelled on `chat_richer_messages_e2e.py`. Its success
line is `MESSAGE ACTIONS E2E OK`, and it ends in an artifact anyone can re-run: the socket transcript plus window
captures. Scenarios:
1. **Copy.** It presses Copy, with a real pointer click from the row's geometry, on a user row, on an assistant row
   shown on hover, and on the last reply's row, always shown. It reads the system clipboard and checks it. Builder:
   under Xvfb, with `env -u WAYLAND_DISPLAY`; see the identity E2E's clipboard trap.
2. **Rows show what they should:**
   - the last reply's row is always visible, the others only on hover;
   - a streaming reply has no row;
   - Ctrl+C and Select All still copy the same text as on main.
3. **Continue on each reason:**
   - a stub turn ending in `MaxTokens`, one in `MaxTurnRequests`, and a Stop, for Claude and for ACP;
   - Continue shows on the last reply only and sends a visible "Continue" user row;
   - the stub log shows the prompt in the same session.
4. **Continue's absences:**
   - not after an ordinary end;
   - not while a turn runs;
   - not after an ACP restart;
   - not on a restored chat.

   Claude with its process killed: Continue resumes with `--resume`, and the stub log shows it.
5. **Claude Stop:** with the stub answering in the captured shape, the footer reads "cancelled", no reply text is
   added, and Continue shows. A Stop with an item queued still sends the item (the redirect), and Continue does not
   show over it.
6. **Quote:**
   - Quote on a reply, and Quote selection, put Ely's `QuoteReply` above the composer, and ✕ removes it;
   - a sent quote reaches the stub as a `> ` block;
   - the user row shows the quote;
   - after a restore the row still shows it;
   - a Retry or a hand-back keeps it.
7. **Idle cost:** the redraw count of an idle chat whose last reply shows its row, over a fixed window, against the same
   on main. This catches an Ely animation that loops.

**Captures:** before and after, light and dark, of a transcript with the rows, a hovered row, Continue, and a quote in
the composer and in a sent row. They are taken with `Scripts/capture-markdown-surfaces.sh` or the E2E's own captures.

**Hand checks owed on macOS and Windows** (listed in the PR body):
- the hover row;
- Copy to the system clipboard;
- the Quote selection menu item;
- Continue after a Stop, with the real Claude and one ACP agent;
- the real Claude's result at `--max-turns 1` (§8).

## 8. Risks and open points

- **Claude's limits on the real CLI** (t2r's call). The fold maps `max_tokens` (stop reason) to `MaxTokens` and
  `error_max_turns` (subtype) to `MaxTurnRequests`, as it does today for the footer. The repo holds no real capture of
  either; the stub's are synthetic. Continue follows the same labels, so it appears exactly where the footer already
  says "token limit" or "turn-request limit". A live capture is a hand check: a `--max-turns 1` turn with the real CLI,
  and, if one can be provoked, a token-limit one. Should the real shapes differ, the fix is in `stop_reason_label`, and
  the footer and Continue follow it together.
- **The real Claude CLI on interrupt.** The capture is from one CLI version. The hand check with the real CLI repeats
  it.
- **The real agents' answer to "Continue".** Whether each agent picks a cut answer up well is the agent's business. The
  delivery promises only that the prompt reaches the same conversation.
- **Hover rows on every message** add elements to each drawn row. The list is culled, and the E2E's idle-cost scenario
  and the delivery-1 benches cover the frame.

## 9. Left to the builder

- Where the quote split lives (beside `retry.rs` or in a new `chat/quote.rs`), and the names.
- How the row's geometry reaches the E2E: `surface.chat.read`'s existing geometry, extended per row.
- Whether a selection across messages is quoted (§5.4).
- Which helpers become dead once today's Copy goes.
