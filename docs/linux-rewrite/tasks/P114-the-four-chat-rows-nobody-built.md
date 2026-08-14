# P114 — the four chat rows the census found genuinely absent

**Owner: `codex11`, as builder.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. Follows your `P107`.

## First, `P107` is confirmed

I read both captures. `p107-rendered-transcript.png` shows the socket-driven turn in the **visible**
transcript — the `pwd` user message, the `Execute pwd` card marked `Completed`, its output, and the
assistant's reply — and the composer marker lands too. You took design (1) and removed the replica,
which was the right call for the stated reason: an API that drives an invisible copy of the app is
worse than no API, because it manufactures evidence. `WAYLAND-LANE.md` is updated; that lane can now
judge chat content, and the whole `F-CHAT` family is drivable again.

One loose end from that change, worth a minute: `cargo test -p tiller` emits
`type chat::Entry is more private than the item chat::Chat::transcript_from_entries`. Either widen
`Entry` or narrow the fn — don't leave a new warning behind.

## The four rows

You own `chat.rs` and nobody else is in it. These are the chat rows `P105` confirmed genuinely
absent, with the needle that found nothing:

| row | clause | what the census found |
|---|---|---|
| `F-CHAT-29` | Hover an assistant response, click Copy, paste elsewhere, confirm the copied text **and** a transient checkmark | the only copy path in `chat.rs` is the transcript-wide `CopyTranscript` (`:58`, bindings `:911-912`, `copy_transcript` `:1390`). No per-message hover control, no confirmation |
| `F-CHAT-30` | Click a code-block Copy control, paste, confirm the code **and** the control's confirmation state | no code-block Copy control anywhere; `tiller_markdown/src/model.rs:11` has a copied-text model fn with no render control attached |
| `F-CHAT-32` | Trigger an edit summary, choose Open file, then Revert, confirm the confirmation / reverted / error states | no edit-summary card exists. `Entry::ToolCall`'s doc at `:149` name-drops this row for its diff/raw surface, but the summary card is not there |
| `F-CHAT-35` | Open Chat History with no retained sessions and confirm **No past chats** | zero hits for the string; **and there is no history list surface to carry an empty state** |

Clauses, `VERIFY:` lines and `SRC:` pointers into Tiller's Swift are in
`docs/linux-rewrite/01-inventory-app.md` (`MessageRowView.swift:18`, `CodeBlockHeaderView.swift:17`,
`EditSummaryCardView.swift:26`, `ChatHistoryMenu.swift:15`). Swift is the functional contract;
`COSMIC-DESIGN.md` governs the look.

## `F-CHAT-35` may not be yours to finish

Its clause presupposes a Chat History surface, and the census says none exists — the nearest thing
is the palette's Resume Chat entry gating on `has_retained_chat` (`main.rs:6742`), which *hides*
rather than explains. The related `F-CHAT-34` came back PARTIAL for the same reason.

So decide and say which you did: built the history surface (large, and arguably `F-CHAT-34`'s job,
not this row's), or established that the empty state cannot exist without it and left the row absent
with the dependency named. **The second is a legitimate outcome.** What is not legitimate is
attaching a "No past chats" string to something a user cannot open.

## Both copy rows are two conjuncts, and the second one is the one that gets skipped

`F-CHAT-29` and `F-CHAT-30` each want the copy **and** a confirmation state. A Copy control that
copies but never acknowledges is half the row, and it is the half that always gets built. Report
them separately.

Both also need a hover and a click to prove, and `sonnet` holds the `DISPLAY=:1` lock for `P109`.
Build them, pin them with drawn tests, and record `owed: gesture — <exact gesture>` for the click
path. Do not approximate a hover with a socket call and call it exercised.

`F-CHAT-32` is the one you can prove furthest on your own: an edit summary is triggered by agent
output, so `surface.chat.send` can reach it now that `P107` has landed. Photograph it.

## Proving it

`Scripts/wayland-drive.sh <outdir> '<actions>'`, `TILLER_WL_LABEL=codex11` — lock-free and parallel.
Green tests are the floor; every row that renders needs a capture. `tab.select` brings the Chat tab
forward, `surface.chat.open` does not focus.

## What to produce

`docs/linux-rewrite/P114-report.md`: per row, what you built, the drawn test, the capture, and
**which conjuncts you did not exercise**. Commit per row, path-scoped, never `git add -A`.
`grep '??'` before calling a row done.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** `pireview` owns it.
- `sidebar.rs`/terminal/changes are `codex12`'s (`P110`); `settings.rs`/`tiller_agents` are
  `fable`'s (`P111`). Stay in chat.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
