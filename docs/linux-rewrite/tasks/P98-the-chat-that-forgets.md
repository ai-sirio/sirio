# P98 — the chat that forgets

**Owner: a builder (`codex11` or `codex12`), after `P95`/`P96`.** Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

**Quit Tiller with a chat open and the conversation is gone.** Not truncated — gone. Everything
needed to prevent that is already written, tested, and connected to the wrong path.

| row | clause | now |
|---|---|---|
| `F-PERSIST-DB-05` | chat sessions/items persist transcript metadata and content | `FAILED — defective` |
| `F-CHAT-34` | browse existing chat sessions, open one, delete one with confirmation | `FAILED — absent` |
| `F-CHAT-35` | see the no-past-chats empty state | `FAILED — absent` |

`F-CHAT-34` is unbuildable until the first part lands: **a session browser over sessions that were
never saved has nothing to list.** That ordering is the whole reason these three are one piece.

## The finding this rests on

There are **two chat paths in production**, and the persistence was built on the one the user never
touches:

| path | persistence |
|---|---|
| `tiller_acp::ChatSession` — what the control socket drives | **both** production callers of `load_chat_transcript`; saves structured turns to `chat_turn`; has a working `restore()` |
| `tiller_ui::chat::ChatView` — what the user types into | **none** |

`ChatView::restore_transcript` takes a `&str`, not a `ChatTranscript`, and it is fed from exactly one
place: `retained_chats` in `main.rs`, an **in-memory** list that restores a transcript when a closed
tab is reopened *inside one process*. `session.rs` contains **zero** occurrences of `transcript`, so
the session snapshot cannot carry one either.

Verify all of that yourself before building — grep the needles, not the line numbers. **If you find
I am wrong about any of it, say so and stop**; the design below is only correct if the diagnosis is.

## What exists, so you build none of it again

- `chat_turn` table, with `save_chat_transcript` / `load_chat_transcript` and quarantine handling.
- `ChatTranscript` / `ChatTurn` / `ChatEntry` / `ChatPermissionOutcome` — the full persisted shape.
- A working reference consumer in `tiller_acp/src/chat.rs`: how to save on settle, how to restore.
- Chat-turn round-trip tests in `persistence_integration.rs`, including a relaunch test.

**You are writing a connection and a browser, not a persistence layer.**

## The trap that makes this look done when it is not

`retained_chats` already restores a transcript on tab reopen. **Within one session it will look like
you have succeeded before you have written anything.** The only gesture that distinguishes the
working state from the current one is a **real process restart**:

> open a chat, send a turn that produces a response, **quit the app**, relaunch — the turn is there.

That gesture is also the verdict for `F-PERSIST-DB-05`, which is currently flagged
*awaiting live confirmation*. Do it and report what you saw.

## Order of work

1. **Save.** `ChatView` writes settled turns through `save_chat_transcript`. Follow the existing
   consumer's shape — save on settle, not per token. Say what you chose as `tab_id` and why it is
   stable across a restart; **an unstable key makes every restart look like a fresh chat and will
   pass your unit tests.**
2. **Load.** A restored chat tab rehydrates from `load_chat_transcript` rather than from
   `retained_chats` alone. Keep the in-memory path for same-process reopen if it is cheaper — but
   say which path served which case, or the next critic cannot tell them apart.
3. **List.** There is no "list sessions" API — `load_chat_transcript` takes one `tab_id`. Add the
   listing (distinct sessions with enough metadata to render a row: title, last activity). This is
   new code in `tiller_persistence`, with tests.
4. **Browse.** `F-CHAT-34` is a **three-part conjunction** — browse, open, **and delete with a
   confirmation**. Report each separately; a single "done" hides whichever you touched last.
5. **Empty state.** `F-CHAT-35`. Cheap once (3) exists, and easy to forget precisely because it is
   the state you stop seeing as soon as your own testing creates a session. Test it on a fresh DB.

## The rules

- **Inspiration, never code.** waku, Zed, orca and comet are to look at. Transplanted code is a gap,
  always. Reusing **our own** `tiller_acp` consumer as the model is not a transplant.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`** — this whole piece is made of
  dead-but-tested code, which is exactly what a green test already covered.
- **Do not edit `INVENTORY-LEDGER.md`.** Report what you built and what you exercised.
- Commit path-scoped, never `git add -A`; `main.rs` and `chat.rs` are shared — follow
  `ENVIRONMENT.md` §"A shared file is not a reason to leave work uncommitted".
- `sonnet` is working in `chat.rs` on the ACP entry types (`P91`). **Coordinate before restructuring
  that file**; your work is the persistence seam, not the entry model.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
