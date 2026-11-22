# Ely chat, group A, delivery 1: richer messages

Ely programme step 3, delivery 1. One PR, version 0.48.0. It absorbs #831.

## 1. Intent

Enzo's programme moves Sirio's AI chat onto what Ely offers, one delivery per PR. This one makes each message in the
transcript say more about itself:
- a reply streams in as Markdown at a steady pace;
- the day each turn happened is marked;
- a failed turn can be tried again and really resends;
- hitting a usage limit says when it lifts;
- a thought says how long it took.

It also pays the debt #828 left: `handle_event` re-parses the whole reply on every chunk, so a long reply costs more
per chunk the longer it gets (#831).

**Programme rule:** a feature shows only for agents that supply its data. Every feature here is driven by data: no
event, no feature. The one place the UI decides without an event is whether Retry can continue the conversation, and
that is a method on `ChatClient` (§5.3).

### Goals

- Each chunk of a streamed reply costs the same at 40 KiB as at 1 KiB.
- Every existing chat feature stays: the nesting guard, links, block culling, and Select All + Copy of the whole reply.
- Retry resends what failed, in the same conversation whenever the agent still has it.
- What each feature shows matches what the agent actually said, per transport.

### Non-goals

- No token or cost footer.
- Not the later deliveries' features: message actions, edit and branches, web sources, composer and media, the welcome
  screen.
- Not the programme's exclusions: voice, link preview, ratings, shared conversations, project knowledge, follow-up
  suggestions, model parameters, audio/video, system-prompt editor.
- No rate-limit data for agents that do not send any (all ACP agents today).

### Anti-goals

These would make this delivery a failure even if every feature is visible:
- a streamed reply whose final tree differs from what `markdown::build` gives for the same text;
- a copy, or a debug readout, that misses text the agent already sent because the reveal has not reached it;
- a Retry that silently sends a prompt to an agent that has lost the conversation;
- a reveal that keeps the user waiting after the turn has ended or been stopped.

## 2. What exists today (recon on `cdd7cc68c`, 2026-11-21)

- **Transports:**
  - Claude is the only native transport. Codex, OpenCode, Pi and omp all run over ACP (`sirio/src/main.rs:4602-4611`).
  - `LaunchSpec` has two variants, `Acp` and `Claude` (`sirio_chat/src/lib.rs:34-39`).
  - Native Pi and omp exist only on unmerged branches.
- **Streaming:**
  - The `AgentMessageChunk` arm (`sirio_ui/src/chat/mod.rs:3332-3351`) appends the text, then runs `markdown::build` on the whole reply.
  - The highlight memo, the file-link memo and the cull memory survive across chunks; the parse does not.
  - Measured (spec 2026-11-20 §7): `handle_event` costs 2.98 ms per chunk on SHOWCASE, against bezel's 0.81 ms, and grows O(n) with the reply.
- **Ely `StreamingMarkdown`** (`ely-chat/src/stream.rs:175-209`):
  - It re-parses its full source on every frame: `MarkdownRenderer::new(source)` with no blocks, `render.rs:598`.
  - It bypasses `parse_markdown_guarded`, the chat links, culling and the diagram switches.
  - What it adds that is visible is the reveal: 90 graphemes/s, catching up within 350 ms, all at once under reduced motion, with a `▍` caret.
- **Day headings:** they already exist. `transcript.rs:36-66` reads the user entry's `at` and shows "Today", "Yesterday" or `Mon 2 Sep`, uppercased.
- **Thought duration:** it already exists for every transport that sends `ThoughtChunk`.
  - Ely's `ThinkingBlock` shows "Thought for N seconds".
  - The UI measures it from the first chunk to the next non-thought entry or to the turn's end (`mod.rs:3049-3060`, `thought.rs:232-245`).
  - Claude's stream also marks the thinking block's own end, and nothing reads it.
- **Rate limits:** native Claude only.
  - `SessionNotice::RateLimited { status, window, resets_at, used_percent, limit_scope }` becomes a one-line `Notice { kind: Warning }` (`mod.rs:2676-2697`, `limit_notice.rs`).
  - It is not persisted.
- **Errors and Retry:**
  - Retry (`mod.rs:6795`) calls `start_connection`, which relaunches the agent and resends nothing.
  - An ACP relaunch is always a new, empty session: `sirio_acp` never loads one.
  - A Claude relaunch resumes only in a chat restored from a stored session (`continue_on_relaunch`, `mod.rs:4263`).
  - A failed Claude turn (`is_error` result) is streamed as ordinary reply text (`claude/events.rs:905-913`).
  - A failed ACP prompt RPC is a `TransportError`, which drops a client that is still alive (`sirio_acp/src/lib.rs:1088-1107`, `mod.rs:3920-3934`).
  - `submit_turn(text, mention_paths, images)` (`mod.rs:5948`) keeps only the text in `Entry::User`.

## 3. Decisions (t2s for Enzo, 2026-11-21, all as recommended)

| # | Question | Decision |
|---|---|---|
| Q1 | How streaming becomes incremental | Sirio-side: closed blocks are kept and only the open tail is re-parsed; one full build at turn end. The proof is the same tree as a full build at every byte cut, plus #828's bench re-run. |
| Q2 | What is visible while a reply streams | Ely's reveal, with the `▍` caret, on two conditions: at turn end or Stop, the rest shows at once; copy and debug readouts always see all received text, not only what has been revealed. |
| Q3 | What Retry does | It resends in the same conversation when the agent still has it. Otherwise it relaunches and puts the message back in the composer with a note. A failed Claude turn becomes the error card. An ACP client stays up when its process is alive. |
| Q3b | Claude Retry and resume | Retry resumes the conversation (`--resume`) in every Claude chat, not only restored ones. Restart agent is unchanged. |
| Q4 | Rate limit | When the limit is reached (`status: rejected`), an Ely-style card counts down to `resets_at`, in Sirio's words, with no "Raise the limit" button. Warnings stay the line of today. |
| Q5 | Day separators | Ely's look (rule, label, rule), with the same logic. |
| Q6 | Thought duration | No visible change. For Claude, the end comes from the close of the thinking block. |
| — | Debug surface | The readouts and the Retry action for E2E exist in debug builds only, like the others. |

## 4. Approaches considered (Q1)

- **A, chosen: incremental build in Sirio.**
  - The chat keeps the blocks that can no longer change and re-builds only the tail.
  - It keeps every step-2 rewrite (guard, chat links, pictures, culling) and needs no vendor change for the parse.
  - Its risk is equivalence: some Markdown reaches backwards. §5.1 lists each case and how the build rewinds for it, and a byte-cut oracle proves the rest.
- **B: Ely's `StreamingMarkdown`, changed in the vendor** to take pre-built blocks and Sirio's hooks. It is the same splitting work plus a vendor diff and a second renderer path to keep in step. Rejected.
- **C: `StreamingMarkdown` as it is.** It parses on every frame and loses the guard, links and culling, so it makes #831 worse. Rejected.
- **D: coalesce chunks to one build per frame.** It cuts the count, not the O(n), and the bench feeds chunks faster than frames anyway. Not enough on its own.

## 5. Design

### 5.1 Incremental streaming build (#831)

**Two halves of `build`.** `markdown::build` is split, with no change to what it returns:
- **parts:** the guarded parse and the per-block rewrites: diagrams, pictures, chat links, block HTML. This is what runs per part.
- **join:** headings and slugs, `rewrite::tocs` (`[[toc]]` and `[toc]`), node counts and the plain text. This always runs over the joined list, never per part.

So a table of contents, heading ordinals and slug de-duplication stay global without reaching back into a part.

**State.** While a reply streams, its `Entry::Assistant` carries a streaming state next to `text` and `document`:
- `cut`: a byte offset into `text`. Everything before it is **frozen**.
- The frozen blocks: the top-level blocks the parts half built from `text[..cut]`. Each keeps its source byte range, which the rewind below and the reveal (§5.2) need.
- `whole: bool`: set only when the nesting guard refuses a part. From then on, every chunk builds the whole reply, as today. The full build makes the whole reply one plain paragraph then, which is cheap.

**Per chunk:**
1. Run the parts half on the tail, `text[cut..]` (Chat policy, same `file_link_memo`).
2. `document` = the join half over the frozen blocks followed by the tail's blocks.
3. Move `cut` forward: every top-level block of the tail except the last becomes frozen, and `cut` moves to the source start of the tail's last top-level block.

   Block boundaries must be found in the text Ely reads (`$$` fences rewritten, a BOM stripped, lone CR as LF) and mapped back to source offsets. A boundary that cannot be mapped is not used.

**Rewind.** Some Markdown changes a block above it. When the tail holds such a construct, `cut` moves back to the first frozen block it can affect, and those blocks are built again as part of the tail. A rewind costs once per construct, not per chunk. The cases:
- **A definition** of a link reference or a footnote whose label has no earlier definition (the first definition wins): rewind to the first frozen block whose source contains `[` followed by that label, compared case-insensitively. Over-matching only costs a larger rewind.
- **An open `$$` line:** the cut never passes an unmatched `$$`, because Ely's rewrite folds the blocks between a pair into one math block. Until it closes, the tail starts at it.
- **A `\(`:** if Ely's swap round pairs `\(` and `\)` across blocks, the cut never passes an unmatched `\(`. Read `tex.rs` and decide; the oracle has a case either way.
- **Block HTML room:** `room = MAX_NESTING - deepest` is global (`markdown/mod.rs:182-184`). When the reply's deepest Markdown grows, rewind to the first frozen block holding block HTML.
- **The nesting guard refusing a part:** `whole`, as above.

**As built (deviation recorded 2026-11-22, Task 2 and its review).** The rules above are not enough for equivalence; the oracle and a review probe found counterexamples. The code keeps the model (freeze, hold, rewind) and changes these points:
- **Where a cut may fall.** Only before a top-level block that follows a blank line, that does not start on the line still being written, that is not inside a `$$` pair or after a run with no partner yet, and that does not begin with U+FEFF. Setext underlines, a table's delimiter row, a definition's title and lazy lines reach back only across lines with no blank between, so they never cross a cut. Blocks stacked without a blank line freeze together, and a reply with no blank line never freezes. Those cost a little more per chunk than before this delivery, because the scans run on top of the one parse. Measured in `handle_event`: a compact 40 KiB reply +6% (2.99 against 2.83 ms), an open 40 KiB fence +35% (187 against 139 µs, under a 9.9 ms frame).
- **`$$` pairing.** It is Ely's (`flow::paired`): every run of two or more dollars outside code pairs with the next one of the same length, from the top, prose included. So a stray `$$` moves the pairing, and the cut stays out of every pair, not only out of an unmatched line.
- **Labels and footnotes.** A frozen part cannot see a definition elsewhere, and footnotes are numbered by first reference across the reply. So, beside the late-definition rewind: there is no cut between the first and last mention of a label defined in the tail, or between the first and last footnote reference; and a later mention of a label already in a frozen segment rewinds to that segment. Segments may span several blocks. Labels are matched after the padding inside `[ ]` and with case folding; a non-ASCII label matches any bracket. Over-matching only keeps more in the tail, so a long reply that reuses a footnote re-parses from its first use.
- **Block HTML room.** Each part is built with the deepest Markdown of the rest of the reply as its floor. A segment is rebuilt when that depth changes in either direction, not only when it grows.
- **Source offsets** in the nodes (code, pictures, inline sources) are moved by the part's offset into the reply.
- **`\(`** pairs only within one paragraph, heading or table cell (`tex.rs`), so it needs no hold. A swap round the guard refuses after the first one turns the stream `whole`.
- **A text that does not continue the frozen bytes** starts over.

**At turn end** (`TurnEnded`, a failure, Stop, or the client lost mid-reply): one full `markdown::build` of the reply replaces `document`, and the streaming state is dropped. A stored or restored reply is always a full build.

**Equivalence oracle** (written first, §7.1):
- For every byte cut `k` of each fixture, feeding `text[..k]` in the fixture's chunks gives a `document` whose nodes, headings, kinds and plain text equal `markdown::build(text[..k])`.
- The fixtures are the existing cut test (`markdown_tests.rs:80-127`), SHOWCASE (it has `[[toc]]`, and a footnote whose reference comes before its definition), and one per rewind case above.
- Plus: after turn end, `document` equals the full build, always.

**Cost target**, measured with #828's bench (`markdown_tests.rs:299-352`, release, 40 KiB in 200 chunks, median of three alternating rounds, the same as §7 of the step-2 spec):
- **`handle_event` per chunk:** flat in the reply length (the last 20 chunks within 1.5x of the first 20 on SHOWCASE), and its median at or below bezel's recorded 0.81 ms. If the machine's load makes 0.81 unreachable, report the ratio to #828's 2.98 ms measured in the same rounds.
- **Parse + frame per chunk:** at or below #828's final figures within noise: SHOWCASE 7.95 / 15.31 ms, open `rust` fence 11.02 / 22.91 ms (median / p95). Never above bezel's 29.45 / 44.17 and 25.16 / 44.84.
- **The rewinds:** a fixture of reference definitions, each one late, is reported too. Its median must stay flat; a rewind shows as a one-chunk spike in the p95.

**Flatness, as measured (Task 3, ruled 2026-11-22, confirmed by t2s).** The target of 1.5× is not met. Measured over 3 release rounds on SHOWCASE at 40 KiB:
- **The median target is met:** 0.33 ms against 0.81 ms, which is 0.11× #828's 2.98 ms.
- **Parse + frame:** 4.17 / 12.22 ms.
- **`handle_event`, last 20 against the first 20 chunks:** 3.65× (0.46 against 0.12 ms).
- **What grows:** the held tail stays flat. The growth comes from the deep clone of the frozen nodes into the flat `Vec` the view takes, and from the join over the whole tree (plain text, tables of contents).
- **Why it stays:** a per-segment cache would reach about 2× at best, because the clone alone sets a floor near 1.7×. Reaching 1.5× needs the view to hold shared segments instead of one flat `Vec`, a data-shape change outside this delivery.

So this PR reduces #831 and does not close it: the body says "refs #831". The shared segments are the next step, and t2s records that on #831 after the merge. A chunk that rewinds must still cost no more than a full build of the text so far (t2s).

**Rewinds, as measured (Task 3 fix round 1, release, 3 rounds).**
- **On SHOWCASE×8:** a rewinding chunk costs 0.94–1.03× a full build of the text so far. It cost 3.7–5.7× before the fix, and the worst chunk was 22.7 ms.
- **Where the cost was:** a scan for `$$` runs that looked up the held ranges once per byte, so it grew with the square of the text (15.8 ms against 0.41 ms at 37 KB). The fix also reads the tail before it builds it, so whatever freezes is parsed once.
- **A rewind that leaves more than half the reply to parse** is a plain full build.
- **The chunk right after a rewind** costs 1.1–1.33× a full build. It parses the region it froze once, as the segment it freezes. That happens once per late definition, not on every chunk. Accepted by t2s on 2026-11-22.
- **The late-definitions fixture:** median 278 µs and p95 493 µs. Its rewinding chunks cost at most 0.17× a full build.

**The steady bound (Task 3 fix round 2, t2s: "no chunk steadily worse than a full build").**
- **The measure.** Each bench prints the ratio of `handle_event` to a full build at the same index, the number of chunks above 1.15× and the longest run of such chunks. Across SHOWCASE, the open fence, the compact reply, the late definitions and three label-heavy replies, no run is longer than 2 chunks. The median ratio stays at or below 0.97.
- **Labels match whole.** A mention matches the whole label under pulldown's normalisation, so `[1]` is no longer taken for a mention of `[10]`. A reply with 130 such labels went from 2.3–3.0× to 0.2× a full build.
- **Waiting after a fruitless look.** If an analysis freezes nothing while at least half the reply is still in the tail, the stream serves the full build until the reply has grown by a quarter. This applies only to replies of 4 KiB or more. While it waits, `cut()` and `segments()` can lag by up to a quarter of the reply; the document is still the build of the text.
- **Isolated spikes, accepted.** A spike is a single chunk, once per late definition:
  - the chunk right after a deep rewind, in a reply that has both `\(` / `\[` math and block HTML, costs 2.1–2.6×;
  - the chunk right after a rewind once reached 1.60× in a reply with late definitions;
  - an open fence costs about 2× a 0.15 ms build when it looks again after growing.
- **Ely's `$$` pairing** looked up the held ranges once per byte (vendored change L12). Every full build paid for it: 23–28 ms against 6.4–7.0 ms on a document with many fences and code spans, and 4.0–4.5 ms against 3.2–3.6 ms on SHOWCASE.

The open `rust` fence of the bench is a single last block that never freezes. Its tail is the whole fence, so the
parse stays O(fence). That is accepted: a fence is one block, and the highlight memo already holds its closed
predecessors. It is reported as measured.

### 5.2 Reveal

**Pacing:** Ely's, unchanged: 90 graphemes/s, catching up within 350 ms, all at once under reduced motion.
- `advanced`, `next_shown` and `grapheme_end` are `pub(crate)` in `ely-chat/src/stream.rs`.
- They become public, with the pace constants, as local change **L11** in `LOCAL-CHANGES.md`.
- Sirio does not copy them.

**What is drawn** while a reply is live:
- every frozen block whose source end is at or before the revealed byte;
- then a build of `text[from..revealed]` with `▍` appended. `from` is the end of the last of those blocks.

That build is small by construction: at most the blocks that are not yet frozen, plus the one being revealed. It runs
the parts half, then the join half over the drawn list. It runs on each animation frame while the reveal is behind,
and it does not touch `document`.

**The tail's budget (ruled 2026-11-22, after Task 3 as built).** The tail is not always small. A reply with no blank lines never freezes. Labels cited from the first section to the last hold the whole body. The wait after a fruitless look, and a deep rewind, leave the frozen blocks short. In those cases the tail can be the whole reply, about 3 ms per frame at 40 KiB. So when `revealed - from` exceeds 7 KiB, the reply stops revealing for the rest of its turn. It then draws `document` as it arrives, with no caret and no pacing, the way every reply was drawn before this delivery. A reply of a turn the CLI started on its own has no stream, and it never reveals. The budget was 8 KiB at first. Measured in release at the edge, a frame cost 0.80 ms on SHOWCASE, 1.08 ms on the compact reply and 1.19 ms on a label-heavy reply, over the 1 ms line. At 7 KiB the same frames cost 0.78, 0.86 and 0.92 ms (medians). The compact reply's p95 is 1.5 ms, on a machine that was not idle.

A reply gone `whole` has no frozen blocks. A refused reply (nested past the cap) reveals as plain text, because it is one plain paragraph by then. A reply that went whole any other way (declined, or the math-with-HTML check) has a formatted `document`, so it stops revealing and draws `document` as it arrives, like a reply over the budget. A body that ends in a block with no text (a rule, a picture, a formula, a table of contents, front matter) draws no caret rather than a misplaced one. The caret sits inline after the
text, as in Ely, including inside an open fence.

**What reads the received text, never the revealed one:**
- `document`: Select All + Copy, and the transcript copy path;
- the control-socket rows `transcriptText` and `transcript`;
- the file-link memo, which only a build of the received text fills. The revealed tail only looks links up in it and never stats a path.

`transcriptTextRows` and `transcriptLinks` report what the last frame painted, so they show the revealed text. They also leave out culled text. Neither one is the readout that the anti-goal protects; `transcriptText` is.

A drag selection copies what it covers on screen. While a reply is live, that is the text of the blocks last drawn. The caret is drawn on the built tree and is never part of the parsed source, so it cannot change how the text parses, and no copy contains it.

A reply is live only while every entry after it is an aside: a compaction, background-task or warning notice. Any other entry pushed after it finishes the reply. That covers a tool call, a thought, a permission request, an error, a user message, and an info notice, warning or not. This is the same rule that decides whether a later chunk still grows the reply.

**When it ends.** On `TurnEnded`, a failure, Stop, or a lost client, the reply is no longer live:
- the next frame draws `document` whole, with no caret and no pacing;
- the reveal state is dropped.

A reply that is not live (stored, restored, ended) never reveals.

**Culling:** it applies to the drawn list as today. Its memory is keyed by block index, and frozen indices do not move.

### 5.3 Failed turns and Retry

**New event:** `ChatEvent::TurnFailed { message: String, stop_reason: String }` (`sirio_chat_vocab`).
- **What it means:** the turn failed and the agent is still up. It ends the turn **in place of** `TurnEnded`, so nothing that `TurnEnded` does follows a failure: in particular, the queue is not drained (below). `stop_reason` feeds the footer as today.
- **Claude emits it** for an `is_error` result whose text is not a local command's echo, except the subtypes `error_max_turns` and `error_max_budget_usd`. Those keep today's text and footer: a resend would hit the same limit.

  It replaces both today's `AgentMessageChunk` of that text and the `TurnEnded` that `apply_result` sends after it (`claude/events.rs:905-913`, `:965`).

  **A Stop is not a failure.** The result that closes a turn the worker interrupted (Stop sent the `interrupt` control
  request) keeps today's events, whatever its subtype and text: the real CLI answers an interrupt with
  `error_during_execution`, `is_error: true` and no text. The worker knows it sent the interrupt; no string in the
  result decides it. (Controller's call during Task 5's review, 2026-11-22.)

  **A turn nobody prompted is not a failed turn either.** An `is_error` result that closes a turn the CLI started on
  its own (a background agent's wake) keeps today's events: there is no prompt to retry.
  The worker counts the prompts it wrote and not yet closed (`pending`). A result that carries `queued_turn_count = q`
  closed one of Sirio's prompts only if `q < pending`; after it, `pending = q`, since the CLI can fold queued prompts
  into one turn. A result without the field closed a prompt if one was pending, and takes one off. So a CLI turn that
  ends while a prompt of Sirio's waits behind it is spared too. (Controller's call, 2026-11-23.) Known gap: the count
  is read after the CLI's end-of-turn wait, so a prompt still in the pipe at that moment is mis-paired; pairing by
  the result's `user_message_uuids` would close it.
- **ACP emits it** for a `session/prompt` RPC error that is not auth-required. `sirio_acp` no longer turns that error into `TransportError`, and the client stays.

`TransportError` keeps its other uses unchanged: auth-required over ACP (`sirio_acp/src/lib.rs:1074-1082`), Claude's
"model selection failed" (`claude/worker.rs:1170`), a closed transport, a failed send. Only the failed prompt moves to
`TurnFailed`.

**The last prompt** is held in memory and not persisted: `last_prompt: Option<LastPrompt>`.
- **What it holds:**
  - what was sent: text, mention paths, images;
  - what refills the composer: the raw draft and its accepted mentions. `assemble_prompt` strips the `@path` tokens from the sent text (`composer_view.rs:37-63`).
- **When it changes:**
  - It is set by `submit_turn`, including a queued item's send.
  - It is cleared by a turn that ends with `TurnEnded`.
  - It survives `TurnFailed` and any loss of the client in mid-turn.
- So an agent that dies while idle leaves nothing to resend.

**The queue after a failure.** Queued items are held, not sent:
- A successful Retry, meaning one whose turn ends with `TurnEnded`, drains them as today.
- So does any later send that succeeds.
- Clear all still empties the queue.
- Without this, the next queued item would replace `last_prompt` and Retry would resend the wrong text. This is visible change V7.
- **The wait shows.** A held queue has one line under it: "Waiting: press Retry or send a message." (t2s, 2026-11-21). It goes when the queue is released or emptied.
- **The held items stay editable and removable** as today: ✕ on one, Clear all.

**The error row.** `TurnFailed` pushes an `Entry::Error` with a new `ErrorKind::TurnFailed`, `retryable: true`. It
ends the turn (`streaming = false`, the trace turn failed) and leaves the client alone. The row keeps today's look:
Sirio's error row with its Retry button. That is my call, and §6 lists it as a visible non-change.

**What Retry does.** The row's button (labelled "Retry", or "Restart agent" on a `Disconnected` row) calls `retry`,
which chooses among these:

| Situation | Agent still has the conversation? | Retry |
|---|---|---|
| `TurnFailed` | yes, the client is alive | Removes the error row, pushes a new `User` row with the same text and a new `at`, and sends `last_prompt` on the same client. |
| Client lost while `last_prompt` is set (a `Connection`, `Disconnected` or timeout error, or a failed send, in mid-turn), the launch resumes on Retry, and a session id is known | yes, after `--resume` (Q3b) | Relaunches with `--resume` of the current `agent_session_id`, then, once connected, sends `last_prompt` as above. If the resume is refused (`resume_refused`), it falls to the next row. |
| The same, but the launch cannot resume (ACP, or Claude with no session id) | no | Relaunches. Once connected, it puts the raw draft, its mentions and the images back in the composer, only if the composer is empty. It adds a `Notice`: "The agent restarted without this conversation. Your message is back in the composer." If the composer was not empty, nothing is put back and the notice says "The agent restarted without this conversation." |
| No `last_prompt`: an idle death (`Disconnected` with no turn running), an error restored from disk, a failed launch | — | As today: relaunch only. This is the case "Restart agent is unchanged" (Q3b) refers to. |
| AuthRequired, Unavailable, McpWarning | — | Unchanged. |

**Q3b, the Claude resume on Retry.**
- `continue_on_relaunch` keeps its rule for every other relaunch.
- Retry with a `last_prompt` alone builds its launch with `--resume` of the chat's current `agent_session_id`, restored or not.

**Capability:** `LaunchSpec::resumes_on_retry(&self) -> bool`, true for `Claude`.
- It sits on `LaunchSpec` and not on `ChatClient`, because the case that needs it is the one where the client is gone. The chat keeps `agent_launch` across a loss.
- The UI asks it, and whether a session id is known, to choose between the second and third rows.

**Rate-limited turns** are handled in §5.4: the countdown card takes the error row's place.

### 5.4 Usage limit reached (native Claude)

- **When the card shows.** On `SessionNotice::RateLimited` with `status == "rejected"` and a `resets_at`, the transcript shows a **limit card** in place of today's warning line. Other statuses keep today's line.
- **What it is made of.** It is built from Ely's `Alert` (warning severity) and `Countdown` (`ely-core/src/feedback/clock.rs:165`). It does not use `RateLimitNotice`, whose words and button are about a message count.
- **Its words.**
  - Title: "Usage limit reached".
  - Body: `limit_notice::rate_limit_sentence` as today (window, scope, reset time).
  - The countdown runs to `resets_at`.
- **When the turn failed with it.** If the same turn fails (`TurnFailed`, which Claude sends with the limit text), its `Entry::Error` is still pushed and persisted.
  - While the card exists (in memory, from the `RateLimited` of that turn), the transcript draws the card in that row's place.
  - Its control row says `coveredBy: "limitCard"`.
  - After a restore there is no card, so the row is drawn.
- **When the countdown ends.**
  - The card shows "Try again", which does what §5.3's `TurnFailed` Retry does.
  - If no turn failed with it, the card says "Usage limit lifted" and shows no button.
- **Persistence.** Not persisted, as today's warning. A restored transcript shows the error row if one was stored.
- **Who gets it.** It is driven by data: only Claude sends `RateLimited`. ACP agents show nothing new.

### 5.5 Day separators

- **Component.** Ely's `DateSeparator::new(day, today)` (`ely-chat/src/list.rs:228-284`) replaces `render_day_heading`.
- **Logic.** Unchanged: `heading_for` decides where a separator goes, from the user entries' `at`. The `chrono` date becomes a `jiff::civil::Date`.
- **Words.** Ely's: "Today", "Yesterday", `Monday, September 2` in the same year, `September 2, 2025` otherwise. They replace `day_label` and the uppercase tracking. This is a visible change (§6).
- **The control row's label.** It uses Ely's own `day_name` (`list.rs:250`). `day_name` is `pub(crate)` and becomes public in L11, so Sirio does not keep a copy of the words.
- **When `today` is read.** At draw time.

### 5.6 Thought end (native Claude)

- **New event:** `ChatEvent::ThoughtEnded`. The Claude transport emits it when a `content_block_stop` closes a block that a `content_block_start` of type `thinking` opened. The transport tracks block index to type.
  - `sirio_claude/src/message.rs:661-681` recognises `content_block_stop` but does not parse it yet. It learns to read the block index there.
  - `sirio_chat/src/claude/events.rs` keeps the index-to-type map and emits the event.
- **In the UI.** An open `Thought` settles its `duration_ms` on that event. Today's rules (the next non-thought entry, turn end) stay as the fallback, and they are all ACP has.
- **Visibly:** nothing changes, except that the number is right when the reply's first text is slow to arrive.

### 5.7 Debug surface (debug builds only, like the existing ones)

`control_entry_row` (`mod.rs:7554`) gains fields:
- `user`: `at`, Unix seconds or null.
- `thought`: `durationMs`.
- `error`: `errorKind` (`turnFailed`, `connection`, `disconnected`, `authRequired`, `unavailable`, `mcpWarning`) and `retry` (`resend`, `resume`, `composer`, `relaunch`, `none`): what its Retry will do now.
- `assistant`: `live` (bool) and `shownBytes`. That is the revealed byte count, for proving pacing. `text` stays the received text.
- **New row kind** `limitCard`, with `resetsAt` and `phase` (`counting`, `retry`, `lifted`).
- **The queue:** its readout gains `held` (bool) and `heldLine`, the line's text when it is shown. If the snapshot has no queue readout yet, add one with the item texts.
- `daySeparator`: rows for the separators, with their label. The existing day headings had no row; check first.

**New `surface.chat.transcript` action:** `retry { entry }`. It presses the Retry of that error row or limit card, as a
click would. It is refused when the entry has no Retry.

All of these are added to `served.rs`'s debug list only.

## 6. Visible changes

| # | Change |
|---|---|
| V1 | A streaming reply reveals at a steady pace with a `▍` caret; at turn end or Stop the rest appears at once. |
| V2 | Day separators: a rule, the label, a rule, in Ely's words ("Monday, September 2"), not uppercase `MON 2 SEP`. |
| V3 | A failed Claude turn shows the error row with Retry, instead of the error text as the reply. |
| V4 | A failed ACP prompt leaves the agent running; Retry resends in the same conversation. |
| V5 | Retry after a lost connection: Claude resumes the conversation and resends. ACP restarts and puts the message back in the composer with a note. |
| V6 | Claude usage limit reached: a card with a countdown, then "Try again" or "Usage limit lifted". |
| V7 | After a failed turn, queued messages wait, with the line "Waiting: press Retry or send a message." under the queue. A successful Retry or send releases them. Today a failed Claude turn still sends the next queued message. |
| — | Not changed, by my call: the error row's look (Sirio's row with Retry, not Ely's `ErrorMessage`); the thought header. |

## 7. Testing

The repo's rules apply: E2E first, an artifact at the end, and unit tests only for units that need isolation,
written before their code.

### 7.1 Unit, written first

- **The equivalence oracle (§5.1).** Its failure list is written before the incremental code. Each bullet of "when the cut may not move" is a fixture, plus SHOWCASE, a long list, a table built line by line, a setext heading, lazy continuation, and an HTML block.
- **The reveal's draw list.** Frozen blocks up to the revealed byte, then the tail with a caret; not live means the whole `document` and no caret. A cut in the middle of a grapheme never happens.
- **Retry's choice, per row of §5.3's table.** A pure function from (error kind, `last_prompt` present, `resumes_on_retry`, session id known, composer empty) to the action.
- **The Claude events:**
  - a thinking `content_block_stop` gives `ThoughtEnded`, and a text block's does not;
  - an `is_error` result gives `TurnFailed`, with no `AgentMessageChunk` and no `TurnEnded`;
  - `error_max_turns` and `error_max_budget_usd` keep today's events.
- **The queue:** after `TurnFailed` it does not drain; after the next `TurnEnded` it does.
- **ACP.** A prompt RPC error gives `TurnFailed` and keeps the client; a closed transport still gives `TransportError`.

### 7.2 E2E: `Scripts/Tests/test-chat-richer-messages-e2e.sh`

It prints `CHAT RICHER MESSAGES E2E OK`, runs with `TMPDIR=~/.cache/st/rail-e2e-tmp`, and is isolated like the
chat-session-panel E2E. It uses its Claude stub fixture (a new mode) and the ACP fixture.

Scenarios, each proved through the socket readouts of §5.7 and a capture:
1. **`stream`:**
   - a 40 KiB SHOWCASE reply in chunks;
   - while live, `shownBytes < len(text)` at least once, and `transcriptText` already holds every received byte;
   - Select All + Copy during the stream gives the received text;
   - at `TurnEnded`, `live` is false at once and `shownBytes == len(text)`;
   - the final rows equal a reply delivered in one chunk.
2. **`stop`:** Stop in mid-reveal shows the rest at once.
3. **`claude_turn_failed`:**
   - an `is_error` result gives an error row with `errorKind: turnFailed, retry: resend`;
   - `retry` resends the same text, which the stub records, on the same process (same pid);
   - the second answer arrives.
   - **Queue variant:**
     - two messages are queued while the failing turn runs;
     - after the failure, nothing more is sent, the queue reads `held: true`, and `heldLine` is "Waiting: press Retry or send a message.";
     - one queued item is removed with ✕ and the readout shows it gone;
     - `retry` resends the failed text, and after its `TurnEnded` the remaining item is sent and `held` is false.
4. **`acp_prompt_failed`:** the same, over ACP. The process stays alive and the same session gets the prompt.
5. **`claude_lost_resume`:**
   - the stub exits mid-turn, and the error row says `retry: resume`;
   - `retry` relaunches with `--resume <id>`, which the stub records;
   - the prompt is resent.
6. **`acp_lost_composer`:**
   - the ACP fixture exits in mid-turn, and the row says `retry: composer`;
   - a variant where it exits while idle says `retry: relaunch`;
   - after `retry`, the composer holds the text and the notice row is present;
   - nothing was sent;
   - a second run, with a draft already in the composer, keeps the draft.
7. **`limit`:**
   - a rejected rate limit with a `resets_at` a few seconds ahead gives a `limitCard` `counting`;
   - it turns into `retry`, and `retry` resends;
   - a variant with no failed turn ends as `lifted`.
8. **`day_separators`:** a restored transcript with user entries on three days gives separator rows labelled "Today", "Yesterday" and a full date.
9. **`thought_end`:**
   - the Claude stub sends thinking, then waits 2 s before the text;
   - `durationMs` reflects the block's close, under 1 s, not the 2 s wait.

**Mutation proofs**, recorded in the PR:
- with the freeze turned off, so every chunk builds whole, the bench numbers show O(n) again;
- with copy reading the revealed build, `stream` fails;
- with Retry relaunching only, scenarios 3 and 5 fail.

### 7.3 Captures and the bench

- **Before and after captures** of the chat surfaces with `Scripts/Tests/capture-markdown-surfaces.sh` and `Scripts/capture-diff.py`, extended with a frame of each new state: a day separator, a limit card counting, an error row from a failed turn, and a reply in mid-reveal. Every difference is attributed to V1–V6.
- **#828's bench, re-run** on the final head, before and after, in alternating rounds (§5.1 targets). The numbers go in the PR.

### 7.4 Existing suites to re-run

- `test-ely-chat-ui-e2e.py`, `test-chat-session-panel-e2e.sh` (its `notice_*` and retry scenarios), `test-markdown-documents-e2e.sh` (`chat-deep`, the culling checks), `TEXT SELECTION E2E`.
- `cargo nextest` on `sirio_ui`, `sirio_chat`, `sirio_chat_vocab`, `sirio_claude`, `sirio_acp` and `sirio`.

## 8. Constraints

- **Version:** the cycle's first `feat:`, 0.47.1 → 0.48.0, once, with `Scripts/set-workspace-version.sh 0.48.0` and `(cd rust && CARGO_SCOPE=0 cargo update --workspace --offline)`.
- **Merge order:** t22's #832 (`sirio_ui::markdown`, `chat/file_links.rs`) merges first. Rebase onto it before the one push.
- **Vendor:** the only Ely change is L11: the reveal's pacing (`advanced`, `next_shown`, `grapheme_end` and the pace constants) and `day_name` made public. `check-vendor.sh` must print OK.
- **Machine rules:**
  - `CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0`, one build at a time.
  - Never `Scripts/ci.sh`, `ci-linux.sh`, `cargo fmt`, `rustfmt` or `clippy`.
  - `rm`, `mv` and `git clean` with absolute paths only.
  - No Claude attribution.
- **Performance trace labels** stay content-free `&'static str`.

## 9. Left to the builder

- The data structure of the frozen blocks, and how source offsets are mapped from the text Ely reads.
- Whether the plain text is joined from per-part pieces or recomputed over the joined nodes, within the cost target.
- How `ThoughtEnded` is matched to the open thought when a subagent's thinking interleaves. The main thread only; a subagent's thoughts keep today's rule.
- The exact wording of the notices, within the sentences above.
- The stub fixture modes and how the ACP fixture is made to fail a prompt or exit.
