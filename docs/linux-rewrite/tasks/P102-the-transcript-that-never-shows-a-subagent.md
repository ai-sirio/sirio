# P102 — the transcript that never shows a subagent

**Owner: `pi`.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`. File: `rust/crates/tiller_ui/src/chat.rs`.

**Two gaps, both verified absent against today's tree on 2026-08-14 — not taken from the ledger.**
This matters, because most of what the ledger calls absent in this family is not:

| row | clause | today's tree |
|---|---|---|
| `F-CHAT-28` | Inspect subagent task cards **and expand their tool calls** | **ABSENT.** `grep -niE "subagent\|task_card\|TaskCard" chat.rs` → zero hits. |
| `F-CHAT-23` | Expand tool calls, inspect output, **and dismiss unrenderable permission calls** | **PARTIAL.** Expand + output/diff/locations are built and drawn-tested. Dismiss is absent: every `dismiss` in `chat.rs` (:709-1754) belongs to the slash popup. |

**Do not rebuild the expand half of `F-CHAT-23`.** It exists: `Entry::ToolCall` already carries
`content`, `locations`, `raw_input`, `raw_output`, `expanded`, `group_expanded` (:160-171);
`toggle_tool_call_expanded` (:874) is click-wired at :3367 with a drawn test at :6719. The row
still reads `FAILED — absent` because its evidence was written against a tree that no longer
exists. Same for `F-CHAT-21` and `-22` — both built, both wired, both tested. You own **one
conjunct** of `-23`, not the row.

## Gap 1 — nothing in the transcript knows a subagent exists

`Entry` (:133) has arms for User, Assistant, Thought, ToolCall, Permission, Question, Plan, Error.
A subagent's work arrives as ordinary tool calls with no parent, so a task that spawns one renders
as a flat run of steps with no indication that a different agent produced them.

The clause wants a card that shows **current action and status**, and whose **nested tool calls
expand**. You already own the two mechanisms it needs: the one-line-until-opted-in pattern from
`Thought`/`ToolCall`, and the run-tail grouping toggle from `group_expanded` (:155-159). Nesting
is the new part — a group whose members are themselves expandable.

Before you design the entry shape, **find out what the protocol actually reports.** `tiller_acp`
is where the answer is (`lib.rs:309` `kind`, :329, :347, :389, and the `ToolKind` mapping at
:1187). If ACP gives no parent/child relation for tool calls, say so in the commit message and
build the card against whatever signal does exist — do not invent a protocol field.

## Gap 2 — a permission call that cannot be rendered strands the turn

`Entry::Permission` (:175) is built and works: options, `Selected`/`Expired`/`Pending` outcomes,
persistence through `ChatPermissionOption`/`ChatPermissionOutcome`. That is your own P-piece and
it is fine.

What has no arm is the **unrenderable** case — a permission request whose options the UI cannot
present (empty option list, or a kind it has no rendering for). Today that entry either renders
blank or renders as something the reader cannot act on, and the turn cannot proceed. The clause
asks for a Dismiss control on exactly that case.

Decide, and write down in the commit message, **what Dismiss sends back to the agent.** A
dismissal is not the same as a denial, and `SelectedPermissionOutcome` / `RequestPermissionOutcome`
(`tiller_acp/src/lib.rs:10-13`) are where the vocabulary lives. If the protocol has no "dismissed"
outcome, choosing to map it onto cancellation is defensible — arguing it is required, picking it
silently is not.

## How to prove it, and what does not count

**A green drawn test is `NOT EXERCISED`, never `PASSED`.** Write the drawn tests — they are how the
next agent avoids breaking this — but they are not the proof.

The proof is a real ACP agent, live:

- **Gap 1** — give a real agent a task that spawns a subagent, and show the card in the transcript:
  its current action, its status, and a nested tool call expanding on click. If no agent on this
  machine spawns subagents, say that plainly and report what you drove instead. **Do not claim a
  row you could not reach** — an honest "unreachable, here is why" is worth more than a claim a
  critic will overturn.
- **Gap 2** — you need an unrenderable permission request. The honest route is to drive a real
  agent into one; if that is not reproducible, drive it through the fixture
  (`tiller_ui/tests/fixtures/chat_fixture.py`) and **say in the commit message that the live half
  is owed**, so the critic routes it rather than assuming it.

`WAYLAND-LANE.md` renders the app headlessly and `grim` captures real pixels — enough to see a card
render. It cannot click: synthetic input does not exist on that lane, and the tools that claim to
send it report success while doing nothing. Anything needing a click needs `DISPLAY=:1` and the
drive lock, which `sonnet` holds today for `P104`. Plan around that: build and render first, and
take the lock when it frees.

## The rules

- **Inspiration, never code.** waku, Zed, orca and comet are to look at. A transplant is a gap.
- **Do not edit `INVENTORY-LEDGER.md`.** Report what you built and what you exercised; a critic
  sets verdicts, and it will not be you.
- Commit path-scoped, never `git add -A` — `main.rs` and `sidebar.rs` are being edited by other
  agents right now. `grep '??'` before calling the piece done: an explicit-path commit silently
  drops a file nobody was thinking about.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
