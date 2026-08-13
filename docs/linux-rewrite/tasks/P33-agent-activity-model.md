# P33 — The agent activity model: 26 entries, all reachable without a display

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P30 landed and the split you made is the valuable part

`TerminalStateSnapshot`, scrollback capture and replay, nonce persistence coverage, 11 terminal
tests, strict clippy on your crates, and `session.rs` untouched — you stayed inside the seam codex12
routed you rather than widening it.

And you divided the 8 blocked entries honestly: **7 are state-backed (F-TERM-03…09) and exactly one
is genuinely pixel-only (F-TERM-02).** That is the line the brief asked you to hold, and holding it
is what makes the widening trustworthy: had you called all 8 state-backed, the number would have
gone up and its meaning would have gone down.

Your socket spec — `panel.state {id}` and `panel.scrollback {id, max_bytes?}`, with explicit
unknown-pane and closed-pane errors — has been routed to codex12, which is in `tiller_control`
right now.

The CI failure was concurrent edits to `main.rs` by another agent, not your work.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
cd rust && export TILLER_SOCKET=/tmp/codex11.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

No display presents. codex12 is in `tiller/src/main.rs` and `tiller_control/**`; pi is in
`tiller_ui/**` and `tiller_theme/**`.

## The piece

`docs/linux-rewrite/02-inventory-packages.md` holds **171 entries** from the domain packages, and
they are barely touched. They are also the most headless-friendly work left in the project, because
domain logic needs no pixels by definition.

Take **`F-CORE-ACT`: 26 entries**, the agent activity model. You already worked in
`tiller_activity` on the Linux truthfulness piece, so you know the crate.

This is the most intricate part of Tiller's domain, and the original's design notes are worth
reading before you touch it — `../tiller/CLAUDE.md` describes it, and the Swift sources named in
each entry's `SRC:` field are the reference for *behaviour*, never for code.

**Four layers of evidence, weakest overridden by strongest as it arrives:**

- **A — `tillerctl notify` hooks.** Authoritative when present. A recent Layer-A push suppresses
  Layer B for a debounce window.
- **B — the OSC terminal title.** Each CLI has its own title convention, captured empirically rather
  than guessed: Claude idles as `✳ …` and works as `. …` or a braille spinner; Pi titles `π - <cwd>`
  while its omp fork titles `π: <cwd>` — the colon is the only distinguishing mark; Codex 0.144+
  also writes a braille spinner, so **a bare spinner is ambiguous between Claude and Codex and must
  not assign identity.**
- **C — content signal.** Matched on output-settle; deliberately *not* debounced against Layer A,
  because a genuine content match is closer to ground truth than a stale title.
- **D — foreground process.** The only signal that catches agents with no usable title convention.
  On macOS this is libproc; **on Linux it is `/proc`, which is what you built in P15.** Node/Bun
  hosted CLIs are invisible here and rely on Layer B.

**And three kinds of pane ownership, which decide who may clear a pane's status:** spawn-owned
(cleared by watching the process exit), title-owned (cleared only when the title stops matching that
agent's conventions), process-owned (cleared only by `processGone`, never by an unrelated title
change). The original's notes say plainly that mixing these up reintroduces bugs where one layer's
signal wipes state another layer still relies on. **Test that specifically** — the ownership rules
are the part where a plausible implementation is wrong.

For each of the 26: exercise it, and record **PASSED** (with evidence) · **FAILED** · **UNREACHABLE**
(`opencode` and `omp` are not installed on this machine, so entries naming them are UNREACHABLE,
not FAILED; `claude`, `codex` and `pi` are installed) · **NOT EXERCISED — blocked on display**.

Several entries say to *observe the sidebar*. Apply the same line you held in P30: the model
transition is state and is testable; the sidebar showing it is pixels. An entry that requires seeing
the result is **half-proven** by a test that the transition happened — record it in those words, not
as a pass.

Where behaviour is missing, build it in `tiller_activity` — nobody else owns that crate. Where it
belongs elsewhere, say so and it will be routed.

## Evidence

Tests for the pure logic — priority ordering (error outranks needs-input outranks running outranks
done), the Layer-A debounce window, title parsing for each real CLI's actual conventions, the three
ownership rules. Then at least one **live** demonstration with a real agent: launch `claude`,
`codex` or `pi` in a pane through the socket, drive it, and show the status the model derives,
including a `tillerctl notify` round trip.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`), and
  the Swift original is a reference for behaviour, not a source to translate line by line.
- **A capability nobody exercised does not exist.**
- **Count the entries yourself.** 26 is my reading of the file; you were right about 11 versus 14
  and that habit is worth more than the correction.
- You own `tiller_terminal/**`, `tiller/src/panes.rs`, `tiller_activity/**`, and the `Scripts/` you
  created.

## Reporting

Reply in **12 lines or fewer**: how many `F-CORE-ACT` entries you exercised and the four counts,
what you built, the live agent demonstration, which entries are half-proven and why, and the honest
remainder.
