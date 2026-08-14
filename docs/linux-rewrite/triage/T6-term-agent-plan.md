# T6-term-agent build plan — F-TERM, F-AGENT, 18 rows

Read-only triage output. No verdicts changed, no code touched. Each section names what a row
actually needs and which files a fix would touch, per `docs/linux-rewrite/triage/T6-term-agent.md`.
Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

## Cross-cutting findings (read this first)

**Shared cause #1 — the auto-naming pipeline has zero callers, for any agent.**
`F-AGENT-OMP-03` and `F-AGENT-OPENCODE-03` both currently read `FAILED — absent` on evidence
that says "no summarizer-command generator exists anywhere in the port." That evidence is
stale: commits `56977f2` and `01c1739` (both landed after the pass-16 sweep that produced the
evidence) added `OhMyPiAdapter::summarizer_command` and `OpenCodeAdapter::summarizer_command`,
byte-faithful to the Swift originals and covered by dedicated unit tests
(`crates/tiller_agents/src/lib.rs:451-491`, named after these two rows). The generator half is
built. What is genuinely still missing — for every adapter, not just these two — is a caller:
`grep -rn summarizer_command rust/crates` outside tests turns up only the three trait
implementations, never an invocation. `AutoNamingThrottle`
(`rust/crates/tiller_project/src/domain.rs:92`) has the identical shape — built, unit-tested,
zero app references. Nothing in `rust/crates/tiller/src/main.rs` reads a completed chat/agent
turn, checks the throttle, resolves `SummarizerChoice` to an adapter, calls
`summarizer_command`, spawns it, or renames a tab from the result. See rows 4–5 and 9 below;
one integration pass in `main.rs` (plus wiring `AutoNamingThrottle`) closes all three at once.

**Shared cause #2 — the terminal context menu's surface is proven; per-item clicks are the one
gesture still owed, across three rows.** `F-TERM-04`, `F-TERM-06`, and `F-TERM-UI-01` are all
`half-proven` for the same reason: `Scripts/linux-drive.sh`'s `rclick` (X11 lane, `xdotool
click 3`) genuinely opens the menu — proven live, `reference/linux-progress/p17-rclick-term.png`
and `p17-f1-menu.png` show all twelve items (Copy, Paste, Copy Context, Set Title, Copy Pane
ID, Copy Terminal ID, four Splits, Clear Terminal, Close Terminal…) rendered over a real PTY.
`docs/linux-rewrite/QUEUE.md:2138-2171` already recovered this and moved all three from `NOT
EXERCISED` to `half-proven` on 2026-08-14 08:20 — the current ledger verdicts match that
finding exactly, they are not stale. What never happened, per that same entry, is a **click**
on any item. `wayland-drive.sh` (the lock-free lane) has no right-click primitive at all, which
is what the sweep-E07-term evidence is reporting when it says "unreachable from this lane" —
true for that lane, not true for `linux-drive.sh`. The next drive should use the X11 lane and
click each item, not treat the menu as closed.

**Shared cause #3 — `main.rs` is where almost every fix in this group lands, confirming its
bottleneck status.** Of the 11 rows below classified `build` or `both`, 8 touch
`rust/crates/tiller/src/main.rs` as their primary or sole file: `F-TERM-08`, `F-TERM-09`,
`F-TERM-11`, `F-TERM-SPLIT-01`, `F-AGENT-SAFE-02`, `F-AGENT-SESSION-01`, `F-AGENT-SESSION-02`,
and the auto-naming half of `F-AGENT-OMP-03`/`F-AGENT-OPENCODE-03`. None of the package-level
code these rows depend on is missing — `tiller_agents`, `tiller_activity`, `tiller_terminal`
all have the primitives built and unit-tested. The debt is entirely in `main.rs`'s integration
layer, exactly the shape `SEAMS.md` already names.

---

## `F-AGENT-API-01` — half-proven

**Needs: exercise.** The claim ("every adapter exposes ID, display name, hook flag, prepare,
launch, resume, optional summarizer; catalog order is Claude/Codex/OpenCode/Pi/Oh-My-Pi") is
fully proven at the unit level: `crates/tiller_agents/tests/adapters_tests.rs` asserts
`command`/`resume_command`/`prepare`/hook-flag for all five adapters against the literal Swift
source strings, and `crates/tiller_agents/src/lib.rs:445-491` now covers the summarizer clause
for all five too (three `None`, two `Some`, all tested — see shared cause #1 above for the
still-missing *caller*, which is a different clause than this row's). The half genuinely owed
is live: open the new-tab/agent picker, launch Codex, OpenCode, and Pi (all installed per
manifest evidence) each in turn, and inspect the actual PTY argv against what the adapter
should have produced. Oh-My-Pi's launch is separately blocked — see `F-AGENT-OMP-01`.

- **files**: none (exercise only; `crates/tiller_agents/src/{claude,codex,opencode,pi,omp}.rs`
  are the reference for what argv to expect, not files to change)
- **size**: S

---

## `F-AGENT-OMP-01` — NOT EXERCISED

**Needs: exercise, environment-blocked today.** Code reading confirms the adapter is correct:
`executable_name()` returns `"oh-my-pi"` (`crates/tiller_agents/src/omp.rs:43`), both
`command`/`resume_command` spawn that name, and `QUEUE.md:2178-2184` already corrected an
earlier misreading that thought the adapter sought a nonexistent `omp` binary. The remaining
block is upstream, not Tiller's: the installed `oh-my-pi@0.2.0` package dies on un-transpiled
TypeScript (`bin/oh-my-pi.js:176`) before it ever reads argv, so no session can start regardless
of what Tiller launches. No code in this repo can fix that; the exercise needs either an
upstream fix/reinstall of `oh-my-pi`, or a locally patched/transpiled copy on PATH before the
row can move past `NOT EXERCISED`.

- **files**: none (the adapter is not the blocker; do not touch
  `crates/tiller_agents/src/omp.rs` for this row)
- **size**: S once unblocked; the unblock itself is outside this repo

---

## `F-AGENT-OMP-02` — NOT EXERCISED

**Needs: exercise, same block as `F-AGENT-OMP-01`.** The hook template itself
(`crates/tiller_agents/src/omp.rs:12-25`) already encodes the session_start/turn_start/
turn_end/session_shutdown → running/running/needs-input/done mapping the row describes, and
`prepare` writes it to `<worktree>/.tiller/omp-hook.ts`. No event can fire until a session
exists, and no session can start until the upstream TS-parse failure above is resolved.

- **files**: none
- **size**: S once unblocked

---

## `F-AGENT-OMP-03` — FAILED — absent → **reclassify**

**The current verdict is stale.** Its evidence ("no summarizer-command generator exists
anywhere in the port") was accurate at pass 16 and is not accurate today: commit `56977f2`
("feat(agents): port the oh-my-pi noninteractive summarizer command (F-AGENT-OMP-03)") added
`OhMyPiAdapter::summarizer_command` (`crates/tiller_agents/src/omp.rs:94-99`), producing
`oh-my-pi --print --no-tools '<prompt>'` — matching the row's spec with the same deliberate
`omp`→`oh-my-pi` executable-name substitution already accepted for `F-AGENT-OMP-01`/`-02`, and
it is unit-tested byte-for-byte (`crates/tiller_agents/src/lib.rs:472-483`,
`omp_summarizer_command_uses_the_distribution_binary_name`). The row's narrow spec (the command
string itself) is therefore code-complete and test-proven; `needs: reclassify` on that clause
alone. What is still missing is the wiring that would ever call this function — see shared
cause #1 — which is a `both` situation once reclassified: **exercise** by actually running
`oh-my-pi --print --no-tools 'test'` and checking stdout (the row's own VERIFY recipe) —
currently blocked by the same upstream TS-parse issue as OMP-01/02 — and **build** the
auto-naming caller in `main.rs` if the row is read as covering the end-to-end feature rather
than just the command string.

- **files**: `rust/crates/tiller_agents/src/omp.rs` (reference only, already correct),
  `rust/crates/tiller/src/main.rs` (only if the row is read to include the auto-naming caller;
  see shared cause #1)
- **size**: S to re-verify the command string live once oh-my-pi is unblocked; the wiring half
  is scoped under `F-AGENT-OPENCODE-03` below to avoid double-counting

---

## `F-AGENT-OPENCODE-03` — FAILED — absent → **reclassify**

**Same stale-evidence shape as `F-AGENT-OMP-03`.** Commit `01c1739` ("feat(agents): port the
opencode noninteractive summarizer command") added `OpenCodeAdapter::summarizer_command`
(`crates/tiller_agents/src/opencode.rs:85-88`), producing `opencode run --pure '<prompt>'`
with correct shell-quoting including the embedded-single-quote edge case, unit-tested at
`crates/tiller_agents/src/lib.rs:449-461`. `needs: reclassify` on the command-string clause.
Unlike Oh-My-Pi, opencode is now installed (1.18.18, per the manifest's own note) and has no
known upstream blocker, so this row is the cheaper of the pair to close: run
`opencode run --pure 'test prompt'` for real and confirm stdout is just the answer — that
alone would move this row to `PASSED` on its narrow reading. The auto-naming caller (shared
cause #1) is the separate, larger piece if the row is read end-to-end.

- **files**: `rust/crates/tiller_agents/src/opencode.rs` (reference only, already correct),
  `rust/crates/tiller/src/main.rs` (auto-naming caller — see shared cause #1; this is the one
  place to build it once, closing both this row and `F-AGENT-OMP-03`'s end-to-end reading
  together)
- **size**: S for the live command re-verify; **M** for the shared `main.rs` auto-naming
  wiring (turn-completion hook → `AutoNamingThrottle` check → `SummarizerChoice` → adapter
  `summarizer_command` → spawn → parse stdout → rename tab)

---
