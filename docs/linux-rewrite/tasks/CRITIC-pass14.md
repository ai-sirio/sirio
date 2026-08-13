# Critic pass 14 — the visual bar moved, and 70 rows are waiting on you specifically

**You are pireview, pane `w1:p6`.** Your context was just reset, so this brief is everything you
need. You judge; you do not build, and you never judge a piece you built.

## Pass 13 cleared the display backlog — here is the state you left

You will not remember any of this. **All 8 display-blocked rows: resolved, 0 remain.** You scripted
the totals (`Scripts/ledger-totals.py --write`, now the authoritative counter), exercised all 18
`F-PRJ` rows on a live display, re-established or downgraded `F-SID-01/02/04` in the live repo, and
quantified the colour debt against the shot list instead of asserting it. Never-judged fell **77 →
58**.

You also landed five harness fixes — the sweep's stale gap list, catalog-gated `workspace.select`,
the PID fallback, the sccache stale-rmeta trap, and the XTEST click-batching trap. That last class
matters disproportionately: an instrument that lies makes every verdict downstream of it worthless,
and you fixed the instrument rather than working around it.

Your stated remainder was the **light-theme frame** and the **zbus-blocked bin test**, both waiting
on builders. Neither has moved; do not re-derive them.

## Read this before you judge a single pixel

**The visual reference is no longer waku.** The user changed it: the UI target is now the **Pop!_OS
COSMIC** design language. Features stay Tiller's; the look is COSMIC.

If you judge new UI against waku screenshots you will fail work that is correct, which costs more
than a missed defect because it sends a builder to undo something right. The token contract is
`docs/linux-rewrite/COSMIC-DESIGN.md`.

**And know where the conversion actually stands, or you will mis-date every UI verdict.** As of
2026-08-13 ~20:25, `tiller_theme` had a complete `cosmic/` module — palette, spacing, radii,
semantic colours, plus a dependency-free reader of COSMIC's on-disk RON config — and
`grep -n cosmic crates/tiller_theme/src/lib.rs` returned exactly **one line**: `pub mod cosmic;`.
`Theme` consumed none of it. Not one pixel was drawn from a COSMIC token.

`sonnet` is wiring it now (COSMIC-02) and converting `controls.rs`, the shared widget vocabulary
behind ~80 call sites. **Check whether that landed before judging any surface against COSMIC.** A
surface built before the token layer was consumable is waku-styled because it could not be anything
else — marking it FAILED for that is verdict-by-adjacency.

## The thing that should reshape this pass

Your own `Scripts/ledger-totals.py`, run after pass 13:

```
TOTAL                               389
builder-claimed, unverified          34
never independently judged            58
```

Those 34 are built, carry named tests in the tree, and **have never been exercised by a critic**.
Under this project's own rule they do not exist. But each is a tick available for the cost of
*exercising* rather than *building* — and you are the only agent who can convert one.

**This is the cheapest route to inventory coverage that exists right now**, and it is bottlenecked
entirely on you. Construction is bottlenecked on five builders; adjudication is bottlenecked on one
critic. Spend this pass accordingly.

Two warnings that come with it:

- **`builder-claimed` is not evidence.** It is a claim with a test name attached. The 21%/0% split
  stands: rows judged by *reading* were 21% false, rows judged by *executing* ~0%. Replay the named
  test, then exercise the feature in the running app. A test that passes while the feature is
  unreachable from the UI is the dead-model defect, and the census has found 58 of those.
- **Trust `ledger-totals.py`, not a hand grep.** The orchestrator counted this backlog with a raw
  string match and got **70** — inflated, because the phrase also appears in the ledger's own totals
  block. Your script parses rows and reconciles against that block ("Totals block matches the body"),
  which is why its 34 is the number in this brief. `stale-failed.py`'s copy of this figure has been
  removed rather than left to disagree with yours. Two tools reporting different counts for the same
  question is how a ledger stops being believed.

## Absence in this ledger is usually asserted, not demonstrated

Of 123 `FAILED — absent` rows, **2 cite a workspace-wide search and ~120 cite none at all.**

That is not a claim the rows are wrong. `DEAD-MODULES.md` concluded independently that "absent mostly
means absent", and it is almost certainly right. It is a claim that the rows **do not record enough
for anyone to tell which is which** — which is why all three known stale FAILEDs were found by
accident, when a builder went to build the thing and tripped over it:

| row | said | truth |
|---|---|---|
| `F-EDIT-04` | "no ⌘S binding" | ⌘S was already wired in `main.rs` |
| `F-SET-09` | "no skill provisioner in port" | `skill.rs` has a tested provisioner |
| `F-AGENT-SAFE-01` | "no skill code **in the crate**" | it is in `tiller_project`, not `tiller_agents` |

All three need re-marking. `Scripts/stale-failed.py` ranks the rest by how the absence was
established. **Use it as a reading order, not as a verdict:**

- The signal that survived scrutiny is **`scoped`** — a search bounded to one crate, which is the
  exact shape that produced all three above.
- Its **`HIT` signal is circular** and the script header says so: a row that cites an identifier in
  its evidence (`F-SID-15`: *"removal is the hover × → `remove_worktree`"*) gets flagged for being
  specific. The loudest HITs on the first run were all this shape and all of them were right. `HIT`
  means "read this row", never "this row is wrong".

## Still owed from earlier passes

- **`DEAD-MODELS.md`'s 5 full + 2 partial undisclosed zero-consumer PASSEDs**, all judged pass 10:
  `F-CORE-ACT-17`, `-18`, `-23`, `F-CORE-AUTH-01`, `F-CORE-DOM-07`, plus partials `F-CORE-ACT-22`,
  `F-CORE-USG-05`. Pass 12 set the precedent in `F-GIT`: a package row may stay PASSED **with "zero
  app callers — wiring owed" disclosed in evidence**. These seven carry no such disclosure, so today
  they read as delivered. Re-mark or annotate — either restores honesty.
- **FABLE-04's four-plus-partial are still unapplied**: `F-CORE-ACT-19`, `F-CORE-ACT-20`,
  `F-CTRL-NOTIFY-03`, `F-CORE-FILE-04`, and the notification half of `F-CORE-ACT-02`. Pass 12's "10
  re-marked" were your own UI audit, a different set.
- **Pin any sweep before adjudicating from it.** `DEAD-MODELS.md` is pinned for exactly this reason,
  and `DEAD-MODULES.md` was deliberately made a separate file so the target does not move underneath
  you. Re-run and diff before trusting either.

## The finish line runs through ACP

The goal's acceptance test is explicit: the critic **connects a real workspace agent over ACP, sends
messages, and verifies streaming and replies**. Everything else is preamble.

**The picker has landed** — `codex12` reported P63 complete at ~20:55. New Chat opens a picker over
the adapter catalog, `discover_availability()` gates it so no item promises an agent that is not on
PATH, and the chosen adapter reaches the tab as **title, icon and `agent_id`**. That last part is
what makes "connect *a particular* agent" expressible; before it, New Chat produced a generic tab and
the acceptance test could not be performed on any agent at all.

**So the blocker is gone, and this is the single most valuable thing you can do this pass.** Pick an
adapter, open a chat, send a message, watch it stream, get a reply. It is `builder-claimed,
unverified` — drawn tests exist for the picker, but nobody has driven a real agent through it.

That one exercise is worth more than any number of static verdicts, because until someone performs it
the project cannot be finished **by definition**, no matter what the other 388 rows say. If it fails,
that failure is the most important finding available and outranks everything else in this brief.

### The ACP substrate is proven live — so a UI failure bisects cleanly

Run by the orchestrator at 21:06, so you inherit it rather than spending your pass on it:

```
$ cargo test -p tiller_acp --test real_claude -- --ignored --nocapture
real Claude ACP nonce: tiller-acp-1786646735826596877
test real_claude_streams_tool_permission_and_writes_nonce ... ok
test result: ok. 1 passed; 0 failed;  (8.16s)
```

That is **real Claude, over real ACP**: connected, streamed, handled a tool-permission request, and
wrote a nonce to disk — the nonce being the part that distinguishes "the agent replied" from "the
agent did work". The transport, the event stream and the permission round-trip all function on this
machine.

**Use it as a bisection.** The substrate works, so if your UI-level ACP exercise fails, the defect is
in the wiring between the chat surface and `tiller_acp` — not in ACP. That is a much smaller place to
look, and it means "ACP is broken" is an answer you can now rule out without re-deriving it.

**And note the gap it exposes**, which no ledger row owns: that test is `#[ignore = "requires the
installed Claude credentials and an ACP adapter download"]`, so **the only real-agent proof in the
tree never runs in the gate.** `Scripts/ci-linux.sh` can be fully green while the ACP path is dead.
Worth a row.

Environment, measured at 21:04, so you know what the picker *should* show: **`claude`, `codex` and
`pi` are on PATH; `opencode` and `omp` are not.** `discover_availability()` should therefore offer
exactly three of five. If the picker shows five, the availability gating is broken — and that is a
sharper test of `F-TAB-07` than counting menu items.

## Standing rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Compile, launch and screenshot the app yourself.** `Scripts/linux-shot.sh` writes a **fixed
  path** and every agent clobbers it — copy anything worth keeping to `reference/linux-progress/`
  under your own name, immediately. The pre-COSMIC baseline is preserved at
  `reference/linux-progress/baselines/2026-08-13-pre-cosmic.png`.
- **Only you change a verdict.** `builder-claimed, unverified` is not PASSED.
- **A feature you have not successfully exercised does not exist**, however good the code looks.
- **Establish the build state yourself; do not trust this brief for it.** At 20:12 the workspace
  compiled clean. At 20:47 it did not — `CosmicTheme` errors in `tiller_theme/src/cosmic/theme.rs`
  and `tiller_ui/src/titlebar.rs`, both `sonnet`'s own files, mid-wiring on COSMIC-02. Five builders
  edit this tree continuously and it moves in and out of red on a timescale of minutes.
  **A transient red caused by another agent is not a finding**; it has been mistaken for one three
  times today. Re-run, and if it is still red, name the crate and the owner rather than the symptom.
- Separately, `Scripts/ci-linux.sh` is red on `cargo fmt --check` in `main.rs` and `settings.rs` —
  queued to their owners. Do not report that as a build failure and do not fix it: those files have
  live authors.
- **Never copy code from the reference checkouts.** Transplanted code is a gap, always.
- **Proceed without asking for approval.**

## Reporting

**14 lines or fewer**: how many of the 70 builder-claimed rows you converted and to what, the three
stale FAILEDs re-marked, the 5+2 and FABLE-04's set resolved or annotated, whether ACP was exercised
end to end and what actually happened, whether COSMIC had reached production before you judged any
UI, screenshots with the paths you saved them to, the single largest remaining gap named outright,
and the honest remainder.
