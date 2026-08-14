# P91 — the transcript that says *what* but never *what happened*

**Owner: `sonnet`, as builder.** Start it when your P82 verification is finished. Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

`chat.rs` and `tiller_acp` have no other claimant — every commit touching them is a recovery or
checkpoint commit — so this piece is yours end to end. You did not build either, which is also why a
critic can judge it afterwards.

Six ledger rows sit on one root cause: **the transcript renders that a tool ran, never what it did.**
`F-CHAT-21`, `-22`, `-23`, `-28`, `-31`, `-32`.

## The seam, and why it is the whole point

I traced this before writing the brief, because building the visible half against data that never
arrives is the single most common way work here dies.

`tiller_acp/src/lib.rs:1198` maps the protocol's tool-call update like this:

```rust
SessionUpdate::ToolCall(tool) => vec![AcpEvent::ToolCallStarted {
    id, title, status                       // <- three fields, and the rest of `tool` is dropped
}],
```

`AcpEvent::ToolCallStarted` / `ToolCallUpdated` / `ToolCallCompleted` (`lib.rs:258-287`) carry only
`{id, title, status}`. The update path in `chat.rs:706-726` therefore merges only a title and a
status, and `Entry::ToolCall { title, status, .. }` renders a rail card holding exactly those two
strings.

**So three of the six rows are blocked in the ACP layer, not the UI.** The protocol's `ToolCall`
value carries considerably more than three fields — it is a generated type, and I deliberately did
not enumerate its fields from a stale registry copy rather than hand you names I had not verified.
**Read the type yourself** (`agent_client_protocol` 2.0.0, as used at `lib.rs:1198`) and report what
it actually offers — content, kind, locations, raw input/output, or whatever it turns out to be. That
list determines what rows 23, 31 and 32 can honestly show.

**Do not invent the data.** If the protocol does not carry a diff, then `F-CHAT-31` is not buildable
today and the honest deliverable is a written finding saying so. A plausible-looking diff card fed by
nothing is worse than an absent one, because it survives being looked at.

## Part 1 — `F-CHAT-21`, and it is nearly free

Thinking already works end to end: `AcpEvent::ThoughtChunk` arrives, `chat.rs:695-700` accumulates
consecutive chunks into one `Entry::Thought(String)`, and `chat.rs:2438` renders it as italic body
text. **The data is there and the entry is live.** What is missing is only the control: the thought
renders permanently and fully expanded, with no way to collapse it.

Add expand/collapse with a collapsed summary. This is the cheapest row in the queue — do it first, so
the piece has something landed before the harder parts.

## Part 2 — `F-CHAT-23`, `-31`, `-32`, in that order

They share a spine: widen the ACP event, widen `Entry::ToolCall`, then render.

- `F-CHAT-23` — a tool call should show its detail, not just title and status.
- `F-CHAT-32` — an edit summary for a tool call that changed files.
- `F-CHAT-31` — a diff preview in the transcript.

Widen the seam once and all three land on it. Do the ACP half first and prove it with the event, not
with the UI: a test that asserts the mapping carries the new fields is the right first commit.

## Part 3 — `F-CHAT-22` and `-28`, investigate before building

`F-CHAT-22` is grouped-step expansion; `F-CHAT-28` is subagent task cards. Before writing UI for
either, establish whether the protocol delivers anything to group or any notion of a subagent. If it
does not, say so plainly and leave the rows alone — a finding is a real deliverable here and these
two are the likeliest of the six to be genuinely unbuildable.

## The rules that apply to every line

- **Inspiration, never code.** waku, Zed and orca are references to look at, not to copy from. A
  critic finding transplanted code counts as a gap, always.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`.** You are building, not judging.
- **Do not edit `INVENTORY-LEDGER.md`.** Report what you built and what you exercised; a critic moves
  the verdicts.

## Done means

1. `F-CHAT-21` landed and driven live — a real agent, a real thinking block, collapsed and expanded.
2. The ACP event widened, with the field list you found reported explicitly.
3. Whatever of `F-CHAT-23`/`-31`/`-32` the protocol actually supports, built and exercised against a
   **live ACP agent** — connect one from the workspace, send a message that makes it use a tool, and
   show the transcript rendering the result. The standing goal is explicit that a feature the critic
   has not successfully tried does not exist; the same standard applies to your own claim.
4. Parts 3's two rows either built or reported unbuildable with the evidence.
5. `cargo fmt`, suite green on what you touched, and `git status --short | grep '??'` before you
   finish — `tiller_terminal` currently has two untracked module files that `lib.rs` already
   declares, which is exactly the trap this check exists for.
