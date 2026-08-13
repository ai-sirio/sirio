# P45 — The gate has been red for six hours, which means it is no longer a gate

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P42 re-proved the oldest claim in the project, and strengthened it

ACP works end to end, with a real Claude nonce: `tiller-acp-1786623346473812029`. That verdict was
first recorded around 01:00 and the app was rebuilt around it several times since; it survived.

Better, it now covers what nobody tested the first time: **5/5 deterministic failure paths** —
streaming with a tool call and a denial, malformed-frame rejection, mid-stream agent death, prompt
cancellation, permission cancellation. And you verified the `prepare` invariant the way it needed
verifying — **by mtime on the user-global files**, not by reading the code: Claude
`1786576594.444752722`, Codex `1786602045.304240035`, both untouched.

Adapters: 5 assessed, 3 available, 2 unreachable, 0 failed. The Codex slash/TOML invariant still
covered. And **no production fix was needed** — the protocol tier was already right, which is worth
saying out loud because a piece that finds nothing wrong is a real result and usually goes
unmentioned.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
```

pi is in `tiller_ui/**` on the chat surface; codex12 is in `tiller/src/main.rs` and
`tiller_control/**` building the shell command layer. **Everything else is effectively yours** —
`tiller_terminal`, `panes.rs`, `tiller_activity`, `tiller_persistence`, `tiller_project`,
`tiller_usage`, `tiller_markdown`, `tiller_acp`, `tiller_agents`, and the `Scripts/` you wrote.

## The problem

Every agent report for the last six hours has ended with some version of:

> *"Full CI remains red from pre-existing format/clippy drift."*

You built that gate, and its design was right: it separates pre-existing drift from new so that it
is not red on arrival. But the drift was never cleared, so **every run is red, every agent notes it
and moves on, and the sentence has become boilerplate.** The gate has stopped being a signal.

That matters more than tidiness. Four agents are changing this tree continuously, **nothing is
committed to git**, and the gate is the only automated thing standing between a real regression and
nobody noticing. It was judged honest by the critic and it correctly refused `CI OK` on a real
compile break — that is exactly the property being wasted while its output is ignored by habit.

There was also a **GPUI scheduler panic** in the workspace tests, reported as "outside this piece"
by two different agents. That is the shape of a defect nobody owns. Find out whether it still
happens; the drawn tests were hardened since, so it may already be gone — but *may already be gone*
is not a status.

## The piece

**Get `./Scripts/ci-linux.sh` to print `CI OK` on a clean tree, and keep it honest.**

1. **Format.** `cargo fmt` is deterministic — clear it everywhere. If a file is being edited by
   another agent right now, formatting it is still safe; the conflict, if any, is trivial and
   mechanical.
2. **Clippy — 61 warnings at last count.** Clear them in the crates listed as yours. For
   `tiller_ui/**` and `tiller/src/main.rs`, **do not edit** — count what remains per crate and put
   the list in your reply so it can be routed to its owner. A tidy diff landing in the middle of
   someone's refactor is not tidy.
3. **The scheduler panic.** Reproduce it or establish that it no longer reproduces, and say which
   with evidence. If it is real and lives in a crate you do not own, report it with the reproduction
   rather than patching it.
4. **Then make the gate hold its ground.** Once the tree is clean, drift is no longer "pre-existing"
   — it is new, and it should fail. Whatever mechanism the gate uses to grandfather existing
   warnings should be retired or reset, so that the next warning is somebody's to fix rather than
   everybody's to mention.

## The reasoning, so the shape of it survives

A gate that is red for a reason everyone has agreed to ignore is worse than no gate, because it
still costs a run and it teaches people that red means nothing. This one was commissioned with
exactly that warning attached — *a gate that is red on arrival gets ignored within a day* — and the
drift was left in place on the first pass for a good reason: it would have been a large diff across
crates other agents were editing. Six hours later the cost of that decision is that four agents have
learned to skip the last line of the output.

## Evidence

Paste the gate's full output with `CI OK`. State how long it takes — it was 12.8 seconds when built,
and a gate that gets slow gets skipped too. And say what remains in the crates you could not touch,
per crate, so it can be routed.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Do not silence a warning by weakening the check.** An `#[allow]` that hides a real problem is
  this codebase's characteristic failure wearing a different hat: the surface says clean, the state
  is not. Where an `#[allow]` is genuinely correct, put the reason next to it.
- Proceed without asking for design approval; the acceptance criteria above are the gate.

## Reporting

Reply in **12 lines or fewer**: the gate's output, its runtime, what remains per crate for other
owners, whether the scheduler panic reproduces, and the honest remainder — including any warning you
suppressed rather than fixed, and why.
