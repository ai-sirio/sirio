# P55 — Five control-tier defects the critic found by exercising

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you need.

## P54 landed, and you tested the right thing

`workspace_wires_real_process_signal_without_title_clobber` is the test that matters. The layers were
never the risk; the risk was one layer's signal wiping another's, and you proved the specific case
where that happens rather than proving the happy path twice. Keeping child exit to spawn-owned state
only is the decision `codex11` flagged as the hardest part of the wiring, and you honoured it.

You also ran `git branch --show-current` and reported `linux/gpui-waku`. Keep doing that — a piece was
lost to the wrong worktree today.

## The piece

`pireview` exercised the control tier live in pass 11 and found **five defects**. They are in your
crates, they were found by executing the protocol rather than reading it, and none of them is
speculative. Fix them.

1. **`workspace.close` leaks the PTY.** The most serious of the five, and the one to do first. This is
   the same class as P40 — *"closing a workspace leaves process groups running"* — which was closed
   earlier. Treat it as a possible regression of a fixed behaviour, and find out **why the P40 fix does
   not cover this path** before writing a second fix beside the first. Two overlapping fixes for one
   class of leak is how the third leak gets built.

2. **`{status:ok}` where the documentation says `{pong:true}`.** Decide which is correct and make the
   other match — but say which one you moved and why. A protocol whose docs and behaviour disagree has
   two truths, and `tillerctl` users are reading the docs.

3. **The 1 MiB scrollback cap overshoots to +64 KiB−2.** Check before you fix: the cap was
   *deliberately* given a stated fuzziness of one 64 KiB chunk, so this may be documented behaviour
   reported as a defect, or a real off-by-two on top of it. `−2` is suspicious in a way that a whole
   chunk is not. If the behaviour is right and the wording is wrong, fix the wording and say so.

4. **`--socket` placement in usage errors.** A usage message that shows an invocation the parser would
   itself reject.

5. **Mode ambiguity is not rejected.** An ambiguous mode should be an error, not a silent choice. Say
   what you chose to reject and what stays permissive.

## Evidence

Each fix needs a **named regression test that fails before it and passes after** — five of them, one
per defect. For the PTY leak that means observing the process is actually gone, not that a close call
returned success; that distinction is the whole of the P40 lesson.

Where you conclude a reported defect is not a defect (3 is the candidate), the evidence is a test that
pins the *correct* behaviour, plus the doc change. "Not a bug" without a test is an opinion.

Mark rows `builder-claimed, unverified`, never `PASSED` — `pireview` re-judges. It found these by
executing; it will re-check them the same way.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. Confirm
  with `git branch --show-current`.
- The gate is **`Scripts/ci-linux.sh`**. Your P54 report noted workspace tests failing on existing
  `panes.rs` PTY/process tests **and scheduler nondeterminism** — flag any test you find flaky by name
  in your report rather than absorbing it silently. A nondeterministic test is a defect that will be
  blamed on the next person.
- **Do not edit `tiller_ui/**`** (`pi`, mid-piece) or `tiller_persistence`/`tiller_acp` (`codex11`).
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Proceed without asking for approval.** Points 2, 3 and 5 are judgement calls and they are yours.

## What comes after, so you can plan

`codex11` is re-landing P52 (chat transcript persistence) and then building P53 (the `chat.*` socket
doors — `grep '"chat\.'` currently returns nothing, which is why `D-J1` cannot be exercised past its
project leg). Both produce contracts that need wiring into `main.rs`, and you will get them as **one
batch** once both exist. `main.rs` stays single-owner.

## Reporting

**12 lines or fewer**: each defect with its named regression test, why the P40 fix missed the
`workspace.close` path, which way you resolved the `{status:ok}`/`{pong:true}` disagreement and why,
your verdict on the 1 MiB overshoot, any flaky test by name, the `ci-linux.sh` result, and the honest
remainder.
