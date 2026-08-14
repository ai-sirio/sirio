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

## `F-AGENT-SAFE-01` — half-proven

**Needs: build.** Confirmed by grep (`management.marker`, `managed_by_tiller`,
`overwrite.*refus`, `skill.*provision` — zero hits in `tiller_agents/src` and
`tiller_project/src`) and by reading every adapter's `prepare`: none of the five write a
management marker into the files they generate, and none check for or refuse to overwrite an
existing *unmanaged* file at the same path. `ClaudeCodeAdapter::prepare`
(`crates/tiller_agents/src/claude.rs:36-100`) does the closest thing — it merges into an
existing `settings.local.json` rather than clobbering it wholesale — but that is a JSON-merge
convenience, not the marker-based managed/unmanaged distinction the row describes (which
implies a plain-text marker comment for the TS/JS hook and plugin files, since
`omp-hook.ts`/`tiller-session.js` aren't structured merge targets the way JSON is). This needs
a marker constant, a written-marker check before overwrite, and a refusal error variant, added
per-adapter.

- **files**: `rust/crates/tiller_agents/src/error.rs` (new `PrepareError` variant for "existing
  unmanaged file, refusing to overwrite"), `rust/crates/tiller_agents/src/omp.rs`,
  `rust/crates/tiller_agents/src/opencode.rs` (both write a single generated file today with no
  marker or overwrite check — these are the two adapters most exposed to clobbering a user's
  own file), `rust/crates/tiller_agents/src/claude.rs` (extend the existing merge with an
  explicit marker check rather than relying on JSON-merge as an implicit safety net)
- **size**: M — five adapters' `prepare` functions plus a shared marker/refusal helper and its
  tests (temp-dir fixtures already exist in `crates/tiller_agents/tests/adapters_tests.rs` to
  extend)

---

## `F-AGENT-SAFE-02` — NOT EXERCISED

**Needs: build**, not exercise — the callee that would need exercising does not exist yet.
`ClaudeHookMigrator::rewritten_settings` (`crates/tiller_agents/src/hook_migrator.rs:16-40`) is
fully implemented and unit-tested against matching/stale/malformed/unrelated fixtures (per
`ADJUDICATION-BACKLOG.md`'s own census: prod=0, test=7). Reading `ClaudeCodeAdapter::prepare`
shows why nobody reached for it: `prepare` already unconditionally overwrites its five owned
hook keys with the *current* `tillerctl_path` on every call
(`crates/tiller_agents/src/claude.rs:59-95`), so a worktree that gets `prepare`d again after a
tillerctl relocation self-heals without the migrator. The migrator only matters for a worktree
whose `.claude/settings.local.json` was written *before* a tillerctl path change (e.g. the P87
XDG-install move) and is never `prepare`d again — e.g. at app startup, before any pane in that
worktree is relaunched. No such startup sweep exists: `main.rs` has no call site that iterates
known worktrees and calls `ClaudeHookMigrator::rewritten_settings` on each one's settings file.

- **files**: `rust/crates/tiller/src/main.rs` (add the startup/upgrade sweep — iterate
  persisted worktrees, read each `.claude/settings.local.json`, call
  `ClaudeHookMigrator::rewritten_settings` with the current tillerctl path, write back only on
  `Some`), `rust/crates/tiller_agents/src/hook_migrator.rs` (already correct, reference only)
- **size**: S — the migrator and its tests are done; this is one iteration loop plus a call

---

## `F-AGENT-SESSION-01` — NOT EXERCISED

**Needs: build.** `AgentSessionValidator::is_likely_valid`
(`crates/tiller_agents/src/session_validator.rs:8-26`) is implemented and matches the row's
spec exactly (Claude: `<config>/projects/<slug>/<ref>.jsonl` with non-alphanumeric→hyphen
slugging; Codex: recursive filename search under `<codexHome>/sessions`; other agents trusted).
It has zero callers outside its own tests. The resume path that should consult it is
`handle_resume_chat` (`rust/crates/tiller/src/main.rs:6793`), reached from the tab
context-menu's `TabContextAction::ResumeChat` (`main.rs:5872-5943`) — it reads `session_refs`
directly and calls `resume_command` with no pre-flight check, so resuming against a deleted or
never-existed session file is only ever discovered by the CLI's own failure inside the pane,
not by Tiller. Wiring this also needs the app to know `claude_config_dir`/`codex_home`, which
is not currently threaded anywhere in `main.rs` — that path resolution is the other real piece
of this row, not just the validator call.

- **files**: `rust/crates/tiller/src/main.rs` (call `is_likely_valid` in `handle_resume_chat`
  before invoking `resume_command`; resolve/thread `claude_config_dir`/`codex_home`, which
  today has no home in this file), `rust/crates/tiller_agents/src/session_validator.rs`
  (already correct, reference only)
- **size**: M — the validator call is small, but resolving the two config directories and
  deciding the UX for an invalid ref (disable Resume vs. warn vs. fail after attempting) is a
  real design surface, not a one-line change

---

## `F-AGENT-SESSION-02` — NOT EXERCISED

**Needs: build.** `ClaudeTranscriptSource`/`CodexTranscriptSource`
(`crates/tiller_agents/src/transcript.rs:8-96`) implement the row's spec — JSONL text/block
join under `~/.claude/projects/<slug>/<session>.jsonl` for Claude, recursive
`-<session>.jsonl` search under `~/.codex/sessions` for Codex — and are unit-tested, with zero
production callers (same dead pair as `F-AGENT-SESSION-01`, per `ADJUDICATION-BACKLOG.md`).
Unlike `F-AGENT-SESSION-01`, there is no existing UI affordance this obviously slots into
today — no tooltip, preview, or resume-menu text currently shows "recent text" for a session.
The most plausible consumer is the auto-naming pipeline (shared cause #1): a
CLI-hosted agent's summarizer prompt has to come from *somewhere*, and for Claude/Codex panes
(which have no ACP chat transcript in the DB, only a native JSONL file) these two readers are
the only code in the tree that can produce that prompt text. Building the auto-naming caller
and this row's wiring together avoids inventing two different "read recent conversation text"
paths.

- **files**: `rust/crates/tiller/src/main.rs` (the same auto-naming caller from shared cause
  #1 — feed `ClaudeTranscriptSource`/`CodexTranscriptSource` output in as the summarizer
  prompt for terminal-hosted Claude/Codex panes), `rust/crates/tiller_agents/src/transcript.rs`
  (already correct, reference only)
- **size**: M, folded into the same `main.rs` pass as `F-AGENT-OPENCODE-03`'s wiring rather
  than sized separately

---

## `F-TERM-04` — half-proven

**Needs: exercise.** See shared cause #2. The menu is proven open with `Copy`/`Paste` visible
as the first two items (`p17-rclick-term.png`, `p17-f1-menu.png`). The owed half: select
terminal text, `rclick` at the pane, click `Copy` (menu item 1, offset from the click point
per `p17-f1-menu.png`'s layout), then `rclick` again and click `Paste`, and confirm the
terminal's own content changed — proving the actual clipboard round trip, not just that the
menu dismissed. `p17-f2-copied.png` shows the menu closing after a click but does not itself
prove clipboard content changed; that check (`xclip -o` or a Paste-and-diff) is still owed.

- **files**: none (exercise only; the underlying actions are `TerminalContextAction::Copy`/
  `::Paste` in `rust/crates/tiller_terminal/src/context_menu.rs`, already wired)
- **size**: S

---

## `F-TERM-06` — half-proven

**Needs: exercise.** See shared cause #2. `Copy Pane ID` and `Copy Terminal ID` are both
visible in the proven-open menu (`p17-f1-menu.png`, items 5–6). The owed half: click each,
then paste into a visible text field (the terminal itself, or the `Set Title` field opened
from the same menu) and confirm a nonempty identifier landed — `p17-term06-pasted.png` and
`p17-term06-d1/d2.png` exist in `reference/linux-progress/` from the same pass-17 session and
are worth reading first (per `QUEUE.md`'s "a frame is evidence about everything in it" rule)
before re-driving from scratch, though their own logs don't record which item was clicked.

- **files**: none (exercise only)
- **size**: S

---

## `F-TERM-08` — half-proven → **reclassify (undersold)**

**Needs: build**, not "gesture owed" as the current half-proven framing implies. Reading
`close_terminal_at` (`rust/crates/tiller/src/main.rs:5149-5185`) shows two things the evidence
doesn't surface:

1. **No confirmation dialog exists anywhere in the port for closing a terminal.**
   `close_terminal_at` goes straight from "remove the pane" to sending `[3, 4]` (Ctrl-C/Ctrl-D)
   to the terminal — there is no prompt, no accept/cancel step, nothing a user could "confirm."
   `ActivityStatus::requires_close_confirmation` (`rust/crates/tiller_activity/src/activity.rs
   :30`) is the only piece of the mechanism that exists, and it has exactly two references in
   the whole workspace — both its own tests (confirmed independently by `F-CORE-ACT-23`'s
   evidence in the ledger, same symbol, same zero-caller finding). The row's clause is "See a
   terminal/process close confirmation" — that surface is absent, not merely unexercised.
2. **The single-pane case is a silent no-op.** `if tab.panes.leaf_ids().len() <= 1 ||
   !tab.panes.contains(focused_pane) { return; }` (`main.rs:5159`) returns immediately with no
   feedback when the tab has only one pane — the overwhelmingly common case for a terminal tab.
   Clicking "Close Terminal…" on a fresh single-pane tab does nothing at all. This matches the
   manifest's own note ("Prior single-pane no-op defect (guard short-circuits) stands").

What the manifest's evidence *does* correctly establish is narrower than the row: that
`pane.close` over the control socket, for a multi-pane tab, genuinely kills the child process
via the same `close_terminal_at` path (proven by `f-term-08-sleep-running.png`/
`f-term-08-after-close.png`). That's real, but it's the removal mechanism, not the
confirmation the row asks for — and it doesn't fire for the single-pane case either, since the
guard short-circuits before reaching it.

- **files**: `rust/crates/tiller/src/main.rs` (add confirmation-dialog state gated on
  `requires_close_confirmation`, wired into `close_terminal_at`/`close_focused_pane`; fix the
  single-pane guard to actually close instead of no-op — likely by removing the *tab* rather
  than just the pane when it's the last one, matching `CloseTab`'s existing behavior), `rust/
  crates/tiller_activity/src/activity.rs` (already correct — `requires_close_confirmation`
  needs a caller, not a change)
- **size**: M

---

## `F-TERM-09` — FAILED — defective

**Needs: build.** The verdict is correct and the live evidence (four real agent launches,
named screenshots) is stronger proof than anything a static read could add — this is not a
reclassify. What's worth adding: the wiring the evidence describes as absent ("no title/
content/process layer wired to the badges") is not quite accurate as a code claim, and getting
that right matters for scoping the fix. All three layers *are* wired:
`subscribe_terminal_activity` (`main.rs:2843-2867`) routes `TerminalActivityEvent` through
`panes::apply_terminal_activity_event`, which dispatches OSC-title (Layer B, via
`tiller_activity::title.rs`), settled-output (Layer C, via `content.rs`), and child-exit
signals; `start_process_signal_refresh` (`main.rs:2872-2905`) polls Layer D
(`process.rs::inspect_foreground_agent`) every 500 ms. All of this is unit- and even real-PTY
integration-tested green (`panes.rs`'s `real_pty_activity_status_follows_osc_title_then_
settled_content`, `process_refresh_preserves_process_ownership_until_process_gone`). So the
skeleton is not missing — it produces wrong results against real CLI output. The most likely
cause, per CLAUDE.md's own note that each CLI's title convention was "captured empirically, not
guessed" for the Swift original: `tiller_activity/src/title.rs`'s patterns
(`identify_agent_from_title`, `detect_status_from_title`) were ported from the Swift source but
may not match what the real installed Claude/Codex CLIs actually emit in their OSC titles today
— exactly the kind of thing only a live drive against the real binaries (which pass 17 did)
can catch. The sidebar-dot-never-clears half additionally points at `AgentActivityModel`'s
clearing paths (`is_process_owned`/`is_title_owned`/`process_gone` in
`tiller_activity/src/model.rs`) not being reached for a *dead* process the way
`refresh_process_signal`'s own test proves it can be for a *live* one that exits — worth
checking whether `start_process_signal_refresh`'s polling loop is even still running once the
terminal itself reports exited (its `keep_running` check returns `false` and stops the loop —
if that happens before the clearing transition is observed, the dot is orphaned).

- **files**: `rust/crates/tiller_activity/src/title.rs` (verify/correct real CLI title
  patterns — likely primary root cause), `rust/crates/tiller_activity/src/model.rs` (clearing
  paths for a process that's already gone by the time the poll loop would have caught it),
  `rust/crates/tiller/src/main.rs` (`subscribe_terminal_activity`/`start_process_signal_refresh`
  at ~2843–2905, and wherever `agent_spawned` is/isn't called for a plain non-split agent
  launch — confirm it's reached on every launch path, not only the split-with-agent path at
  `main.rs:5127`), `rust/crates/tiller_terminal` (confirm `TerminalActivityEvent::OscTitle` is
  actually emitted for real CLI output, not only for the synthetic shells the tests use)
- **size**: L — this is a debugging task against real agent binaries with four independently
  broken symptoms (badge doesn't show working, doesn't clear on idle, doesn't clear on death,
  survives relaunch), not a single fix

---

## `F-TERM-11` — FAILED — absent

**Needs: build.** `SEAMS.md` already names this exact seam and its owner: Half A
(`TerminalView::empty_prompt()`, rendering "No terminal in this pane" with New Terminal/New…
buttons and emitting `TerminalPromptEvent` — visible at `rust/crates/tiller_terminal/src/
lib.rs:1370-1420`, the `TerminalState::Pending` + `self.empty_prompt` branch) is done. Half B —
mount that prompt when no worktree is selected at all, and handle the resulting
`NewTerminal`/`NewTerminalWithCommand` events in the app-level tab/pane owner — was never
picked up. This is a clean, scoped, already-diagnosed row; no new investigation needed, just
the `main.rs` integration.

- **files**: `rust/crates/tiller/src/main.rs` (mount `TerminalView::empty_prompt()`-driven
  state for the no-worktree-selected case; subscribe to `TerminalPromptEvent` and handle both
  variants), `rust/crates/tiller_terminal/src/lib.rs` (already correct, reference only —
  `empty_prompt` and `TerminalPromptEvent`/`TerminalPromptAction`)
- **size**: M

---

## `F-TERM-PTY-06` — NOT EXERCISED → **both, not purely instrument-blocked**

**Needs: both**, and the manifest's framing conflates two different drop mechanisms that need
splitting. `receive_file_drop` (`rust/crates/tiller_terminal/src/lib.rs:765-780`) is real,
wired via `.on_drop::<PathBuf>` on the terminal's root element
(`lib.rs:1453-1457`), and has a passing drawn test
(`a_drawn_terminal_inserts_a_quoted_file_drop_without_a_newline`, `lib.rs:2638`). But that
`on_drop::<PathBuf>` is GPUI's **typed in-app drag payload** mechanism — the same one
`right_panel.rs:1921`'s Files-panel rows and `changes.rs:993`'s changed-file rows use via
`.on_drag(path, ...)`. It fires only for a drag that originated inside this GPUI app. `grep -rn
ExternalPaths rust/crates` returns nothing anywhere in the workspace — GPUI's OS-level
drag-and-drop payload type (what a real XDND drop from Nautilus/a file manager delivers) is
never registered on anything. So:
- The **in-app** path (drag a Files-panel row onto a terminal pane) is built and unit-tested,
  and — unlike true XDND — it is *not* blocked by `ENVIRONMENT.md`'s XDND limitation, since
  it's a synthetic in-app drag `xdotool` can drive with a held-button mouse move. This half is
  `exercise`, and the current "instrument-blocked" verdict is too pessimistic for it.
  ENVIRONMENT.md's XDND caveat correctly applies only to the true OS-drop case.
- The **OS-level XDND** path (the row's literal spec, ported from macOS Finder-drop behavior)
  does not exist in the code at all — this half is `build`, not merely blocked-and-waiting.

- **files**: `rust/crates/tiller_terminal/src/lib.rs` (register a `gpui::ExternalPaths`
  `.on_drop` handler alongside the existing `PathBuf`/`(PathBuf, String)` ones, routing through
  the same `receive_file_drop`/classification logic) for the build half; no files needed for
  the in-app exercise half
- **size**: S for the in-app exercise; M for adding real XDND support

---

## `F-TERM-SPLIT-01` — FAILED — defective

**Needs: build.** Both defects the ledger names are confirmed live in current code, with the
first one now precisely located:

1. **"Left inserts on the right" is real, and it's the control-socket handler specifically —
   not the menu, and not the underlying tree logic.** `PaneNode::split_focused_with_placement`
   (`rust/crates/tiller/src/panes.rs:319-380`) correctly honors `SplitPlacement::Before`/
   `After`, and the terminal context-menu's `SplitLeft`/`SplitAbove` items thread `Before`
   correctly (`main.rs:2191-2205`). But the `pane.split` control-socket handler — what P106's
   drive actually used — discards placement entirely:
   ```rust
   "pane.split" => {
       let direction = match direction.as_str() {
           "right" | "left" => SplitDirection::Horizontal,   // main.rs:1297
           "down" | "up" => SplitDirection::Vertical,
           ...
       };
       self.queue_action(request, move |reply| ControlAction::SplitPane { direction, reply })
   }
   ```
   `ControlAction::SplitPane` (`main.rs:302`) carries no placement field, and its handler
   (`main.rs:2446-2449`) calls `workspace.split_focused_terminal(direction, None, cx)`, which
   calls `split_terminal_at` with `SplitPlacement::After` hardcoded (`main.rs:4972-4984`). So
   `"left"` and `"right"` are literally the same request over the socket today — both produce a
   rightward split. The same is true of the `SplitPaneRight`/`SplitPaneDown` keyboard-chord
   actions, which share this same `After`-only path — there is currently no socket or
   keyboard-chord way to split left/above at all, only the context menu reaches it.
2. **`TerminalPaneCache` remains unintegrated**, confirmed unchanged since P106/RECENSUS:
   `rust/crates/tiller_terminal/src/lifecycle.rs:63`'s `TerminalPaneCache<T>` and
   `move_within_worktree` (`:131`) have callers only in their own module's tests. The recursive
   pane tree in `main.rs` (`render_pane_tree` at `main.rs:5362`, confirmed still the
   production renderer) recreates leaves rather than reusing cached ones, so terminal content
   identity, PTY controllers, and focus are not preserved across a tree restructuring the way
   `SEAMS.md` specifies.

- **files**: `rust/crates/tiller/src/main.rs` — this is the primary file for both defects: add
  a placement parameter to `ControlAction::SplitPane` (`:302`) and thread it from the
  `pane.split` handler (`:1292-1310`) through to `split_focused_terminal`/`split_terminal_at`
  (`:4956-4990`, currently `After`-hardcoded); separately, integrate `TerminalPaneCache` into
  `render_pane_tree` (`:5362`) and the split/close/move call sites so leaves are looked up
  rather than recreated. `rust/crates/tiller_terminal/src/lifecycle.rs` (already correct,
  reference only — `TerminalPaneCache`, `move_within_worktree`)
- **size**: L — two independent defects in one row, the second (cache integration) touching
  the recursive tree-rendering path broadly enough that `WORK-BREAKDOWN.md` already sizes its
  containing effort (`B-07`) as M on its own; the placement fix is comparatively small (S) but
  do not fold the sizes together into one estimate

---

## `F-TERM-UI-01` — half-proven

**Needs: exercise**, same shared cause #2 as `F-TERM-04`/`F-TERM-06`, plus one already-recorded
defect to not re-discover: the row's clause is broader than either single-item row (menu
renders, *and* every item delegates correctly), so its remaining half is the full twelve-item
walk — `Copy`/`Paste` (→ `F-TERM-04`), `Copy Pane ID`/`Copy Terminal ID` (→ `F-TERM-06`),
`Close Terminal…` (→ `F-TERM-08`, which is `build` not `exercise` — this row inherits that
same gap for its "offers close" clause), the four `Split …` items, `Clear Terminal`, `Copy
Context`, and `Set Title`. `QUEUE.md:2166-2171` already recorded a real, separate defect on
this row while recovering the pass-17 evidence: the Files panel paints **over** the open
context menu, truncating long labels at the panel's edge (visible in `p17-f1-menu.png`, e.g.
"Copy C…", "Set Titl…"). Both are GPUI elements, so this is a paint-order bug Tiller owns, not
a platform limitation like the P72 webview occlusion — whoever drives this row next should fix
or file that separately rather than reporting a truncated label as a missing menu item.

- **files**: exercise itself needs none; the recorded z-order defect, if picked up alongside,
  is in `rust/crates/tiller/src/main.rs` (paint/z-order of the Files panel vs. the terminal
  context menu — both GPUI elements in the same window, order is app-controlled)
- **size**: S for the exercise; S for the z-order fix if bundled

---

## `F-TERM-UI-02` — NOT EXERCISED

**Needs: exercise.** Code reading confirms the platform-modifier routing is implemented as
`SEAMS.md`'s P82 ruling specifies: `opens_terminal_link(event.modifiers.platform)`
(`rust/crates/tiller_terminal/src/lib.rs:988`) gates the click, and `TerminalLinkEvent`
(`lib.rs:90,999`) carries the URL to the owning terminal's own subscriber
(`main.rs:2830-2841`'s `subscribe_terminal_links`, matched against the clicked pane's id) —
per-pane routing, not global `open_url`. This matches the row's clause exactly. The manifest's
"no xdg-open twice" result is most plausibly COSMIC intercepting the Super key at the
compositor level before it ever reaches the app's `Modifiers.platform` bit, which is a window-
manager/environment question, not a code defect — consistent with the row's own "NOT EXERCISED
— blocked on display" framing rather than a FAILED. Worth trying on both drive lanes before
concluding the block is total: the X11 lane (`DISPLAY=:1`, an XWayland client) may see Super
differently than the native Wayland lane, since XWayland's key routing to a focused client can
differ from what a Wayland-native compositor grabs globally for its own shell chrome.

- **files**: none (exercise only; `rust/crates/tiller_terminal/src/lib.rs:988` is already
  correct against the P82 platform ruling)
- **size**: S
