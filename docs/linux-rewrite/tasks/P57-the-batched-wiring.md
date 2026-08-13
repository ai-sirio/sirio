# P57 — The batched wiring: two finished contracts nothing calls

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you need.

## P55 was your best piece

Five defects, five named regression tests, and two judgement calls that went the harder way:
you found the 1 MiB overshoot was a **real** defect rather than the documented fuzziness it could
easily have been written off as, and you diagnosed *why* P40 missed `workspace.close` — canonical
path validation failing before registry cleanup — instead of adding a second fix beside the first.
That was the specific thing the brief warned about and you did not do it.

The quarantine is the part to be proudest of: *"PTY delivery/debounce coverage is explicitly lost,
not passed."* A quarantine that reads like a pass is a false PASSED with extra steps, and you wrote
the sentence that stops the next person believing it.

## The piece

Two contracts are finished, tested, and **dead**. Verified just now, with a positive control so the
zero means something — the same pattern finds all six in `protocol.rs`:

```
grep -o '"surface\.chat\.[a-z_]*"' tiller_control/src/protocol.rs | sort -u
  -> surface.chat.compose, open, permission, read, send, stop   (6)
grep -o '"surface\.chat\.[a-z_]*"' tiller/src/main.rs | sort -u
  -> (nothing)                                                   (0)
grep -c 'chat_turn\|ChatTurn' tiller/src/main.rs
  -> 0
```

So `tillerctl` can *ask* six chat questions and the server answers none of them, and `chat_turn`
(migration v6) is a table nothing ever writes. This is the largest instance of the defect class
`pireview` named as the project's biggest: **a crate doing the right thing while nothing calls it.**
Seven dead models, six `F-GIT` behaviours with zero app callers, and now J1's whole chat leg.

**1. Dispatch the six `surface.chat.*` methods in `main.rs`**, alongside the existing
`surface.*` arms. `codex11` built them; the shapes are in `tiller_control/src/protocol.rs`
(~lines 400–452). `chat.read` is the one the critic will lean on hardest — it is how a
display-blocked judge sees a transcript at all.

**2. Persist turns through `chat_turn`.** Rendered transcript turns are owned by their chat tab.
A turn that completed must survive quit and relaunch and come back in order. `codex11` owns
`tiller_persistence` internals — call its API, do not reach into its tables.

## The judgement call this piece contains

Wiring `chat.send` means a socket client can drive a real agent turn. Decide and state:

- what happens when `chat.send` targets a surface whose agent is not connected — error, or queue;
- whether `chat.read` returns the live in-progress turn or only completed ones;
- whether a turn cancelled mid-stream is persisted at all, and if so how it is marked.

That last one is not cosmetic. The transcript must **state a cancellation rather than impersonate a
normal completion** — the failure this codebase has produced five times. `pi`'s
`escape_cancels_the_stream_and_the_transcript_states_it` refuses it at the UI tier; do not
reintroduce it at the persistence tier.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. For your tier that means a named test or an
**executed** socket transcript, and for anything touching stored data a **real relaunch**:

- an end-to-end transcript over a real socket: open a chat, send, read the streamed turn back,
  stop mid-turn, read again and see the cancellation stated;
- quit, relaunch, `chat.read`, and show the completed turns come back in order.

Your P55 note said the integration rerun hit a **sandbox Unix-socket EPERM**. If that recurs, say so
by name rather than substituting a unit test and calling the door proven — an unexercised door is
exactly what this piece exists to fix.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. Confirm
  with `git branch --show-current`.
- The gate is **`Scripts/ci-linux.sh`**. It currently fails for two reasons that are **not yours**:
  `tiller_ui` lib tests do not compile (two lifetime errors in `pi`'s in-flight D1 work), and
  `cargo fmt --check` trips on `pi`'s concurrently-edited `chat.rs`. Do not fix either. Report them
  by name, separately from your own result.
- **Do not edit** `tiller_ui/**` (`pi`), or `tiller_persistence` internals (`codex11`, mid-piece).
  `tiller/src/main.rs` is yours alone.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Proceed without asking for approval.** The three judgement calls above are yours to make and
  state.

## Reporting

**12 lines or fewer**: the six methods wired and how you proved each, your three judgement calls
with reasons, the relaunch transcript result, whether the EPERM recurred, the gate result with the
two not-yours failures named separately, and the honest remainder. Mark rows
`builder-claimed, unverified`, never `PASSED`.
