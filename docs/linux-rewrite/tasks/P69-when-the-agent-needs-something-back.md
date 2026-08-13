# P69 — The transcript when the agent needs something back from you

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.

## P66 closed the most dangerous defect in the project, and closed it the right way

The agent picker was cosmetic: pick Codex, get a tab titled "Codex" talking to Claude. It failed in
the way that looks like success — the acceptance test *passed*, against the wrong agent.

You delivered the per-adapter ACP mapping in `tiller_agents` and a public `Chat` constructor that
takes an `AgentCommand`, **and kept `main.rs` compiling untouched** so `codex12` was never
interrupted. That constraint was the whole reason the work could run in parallel, and you held it.

Three calls worth repeating:

- For adapters with no ACP server you chose an explicit `None` with a **visible badge**
  (`settings-agent-acp-{id}`, warning tone, existing tokens, text from `acp_status_label()`) rather
  than a silent downgrade to Claude. Silent downgrade *was* the bug; reproducing it in miniature
  would have been the easy mistake.
- You fixed 5 pre-existing warnings in your own files — your crates are now zero-warning — without
  touching anyone else's.
- You flagged that `@latest` is unpinned while the registry pins `0.66.0`/`1.2.0`, and left it as a
  named follow-up rather than quietly doing it. It is recorded and is not yours this pass.

`codex12` has been told the API is ready; the `add_chat_tab` line is theirs.

## The piece: `F-CHAT-24`, `F-CHAT-25`, `F-CHAT-26`, `F-CHAT-27`

Everything so far moves in one direction — you type, the agent streams back. **This piece is the
other direction: what the surface does when the agent needs something from the user.**

It matters more than its row count suggests. The goal's acceptance test is *"connect a real workspace
agent over ACP, send messages, verify streaming and replies."* A real agent asks questions. Today
that is a dead end in the UI, so the first genuinely interactive session hits a wall that no amount
of streaming polish hides.

- **`F-CHAT-26`** — pending-question bar. The persistent "the agent is waiting on you" state.
- **`F-CHAT-25`** — text answer and cancel on a question. Without this a question can be seen but not
  answered.
- **`F-CHAT-27`** — expired-question state. What the transcript shows when a question is no longer
  answerable. Getting this wrong strands the surface in a permanent wait.
- **`F-CHAT-24`** — the named **Plan** card. The row says only a generic Permission card exists.

Take them in that order — `26` and `25` are the interaction; `27` is its failure mode; `24` is a card
type.

**`F-CHAT-24` is on the stale-FAILED shortlist (`Scripts/assigned-but-absent.py`, risk 10): it was
named in four builder briefs and its verdict is pass 8. Check whether it exists before building it.**
That check costs a minute; rebuilding costs a piece. If it is already there, say so and report it as
a stale-FAILED candidate for `pireview` — **only the critic changes a verdict**.

## The substrate is proven, so a failure here bisects cleanly

`cargo test -p tiller_acp --test real_claude -- --ignored` passes in ~8s: real Claude, real ACP,
connect + stream + **tool-permission round trip** + a nonce written to disk. The permission
round-trip is the closest existing analogue to a question, so the transport is not your risk. If your
work fails, the defect is in the chat surface, not ACP.

That test now also runs in the gate behind `TILLER_ACP_REAL=1` (default prints `SKIP:`, so a pane
without credentials can still reach green).

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

The test that counts **answers the question and asserts the answer left the surface** — the bar
clears, the transcript records it, the pending state ends. A test that renders a question card and
asserts it is visible proves the card, not the interaction, and this project's most-produced defect
is exactly that: something drawn, tested, and connected to nothing. For `F-CHAT-27`, drive a question
to expiry and assert the surface is not stuck waiting.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller_ui/src/chat.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs`, `tiller_agents/**`.**
- **Do not edit** `main.rs`, `tab_bar.rs`, `tiller_control/**` (`codex12`, live in `main.rs` now);
  `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`, `tiller_terminal/**`
  (`codex11`); `tiller_theme/**`, `controls.rs`, `titlebar.rs`, `composer.rs` (`sonnet`).
  If a row needs `main.rs`, **name it as a seam in your report** — that is how P64 and P66 both
  worked, and it is the reason neither collided.
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms.
- Colours, spacing and radii from `tiller_theme::Theme`, never a literal. Visual bar is **Pop!_OS
  COSMIC**, not waku. `sonnet` has landed the token layer and converted `controls.rs`. Name any token
  you need and lack — that list is how `sonnet` learns.
- **Establish the build state with the gate's own commands**, not a paraphrase:
  `grep -n clippy Scripts/ci-linux.sh` and run exactly what it says. The orchestrator ran a weaker
  clippy without `-D warnings`, called the tree clean while the gate was red, and overruled three
  agents who were right — including you. Do not inherit that mistake.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: `F-CHAT-26`/`25`/`27` built and the drawn tests that prove an answer actually
*leaves* the surface (by name), whether `F-CHAT-24` already existed and so was reported stale rather
than built, any `main.rs` seam named for `codex12`, the gate run with its own invocation with
not-yours failures named separately, tokens `tiller_theme` still lacks, and the honest remainder.
