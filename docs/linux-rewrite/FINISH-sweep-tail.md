# FINISH-sweep-tail — thirteen half-proven rows nobody else owns (lane wf-sweep)

Fresh critic pass, 2026-08-18, this host (x86_64 desktop, COSMIC/Wayland, per `ENVIRONMENT.md`'s
2026-08-18 section). Evidence standard: `EVIDENCE-STANDARD.md` — a verdict without a named,
replayable transcript is not a verdict. Lane: `Scripts/wayland-drive.sh`, binary pinned per
`ENVIRONMENT.md`:

```bash
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-sweep-tiller && export TILLER_WL_BIN=/tmp/wf-sweep-tiller
```

`TILLER_WL_LABEL=wf-sweep`, `TILLER_DB=/tmp/wf-sweep.sqlite`. Screenshots referenced below are
committed under `reference/linux-progress/wf-sweep/`.

## A trap this pass hit and is recording for the next one

Every row here needed a right-click context menu, then a click on one of its items. The FIRST
attempt at every such gesture, done as `rightclick <x> <y>` immediately followed by `click <x> <y>`
with nothing between them, silently failed: the click visibly landed on the correct item (hover
highlight showed in the screenshot) but the menu never closed and no side effect occurred — the
click was accepted by the compositor and delivered, but arrived before the freshly-opened menu's
subtree was truly hit-testable (the same "~2 real frames before linking" issue `wayland-drive.sh`'s
own comments document for `tab_bar.rs`'s `deferred(...)` menus, evidently shared by the sidebar's
own context menu). A visible `sleep 1` between `rightclick` and the item `click` fixed it
consistently for every row below. **Do not trust a bare `rightclick`+`click` pair with nothing
between them; the menu opening and the item being clickable are not the same frame.**

Also hit repeatedly this pass: `MESA: error: ZINK: failed to choose pdev` / `Io error: Broken pipe`
at app startup — `ENVIRONMENT.md`'s documented GPU/compositor contention from ~5-6 concurrent
sibling lanes sharing `/dev/dri/renderD128`. Fully environmental (confirmed via `ps aux` showing
`wf-chg`, `wf-tab`, `wf-rest`, `wf-rest2`, `wf-act` all alive at once); resolved by a clean
kill+retry loop, never a code concern.

## Rows

### F-SID-08 — PASSED (upgraded)

Clause: right-click a non-Git project, choose **Initialize repository**, confirm the project
changes to Git-backed behavior. The half already proven (wave F) was the underlying action via the
Project Settings sheet's button; the context-menu entry point itself was unproven.

Live drive: added a real non-git folder project (`/home/enzopalmisano/wf-sweep-nongit`, confirmed
empty, no `.git`, via `ls -la` before). Right-clicked its sidebar row — menu showed **Initialize
Git repository** enabled (a sibling git-backed project's menu in the same drive showed it disabled
with reason "Git is already initialized", confirming the menu reads real per-project git state).
Clicked it (`sleep 1` between open and click — see trap above). Result, both halves of a hard
discriminator:

- **Disk**: `/home/enzopalmisano/wf-sweep-nongit/.git` now exists (`find` before: nothing under the
  folder; `find` after: `.git` present) — a real `git init` ran.
- **UI**: the row transformed from a plain folder project (no branch child) into an expandable
  git-backed project with a `master` worktree row and a `Primary` badge, matching the sibling
  project's own shape — `reference/linux-progress/wf-sweep/f-sid-08-context-menu-init-git.png`.

Both "confirm the project changes to Git-backed behavior" and the specific context-menu entry
point (not the Project Settings sheet) are now driven.

### F-CHAT-25 — PASSED (upgraded)

Clause: trigger a question, enter text and click Send; repeat with a listed option; repeat with
Cancel, confirming answered/cancelled states. The unproven leg named in the brief: "AskUserQuestion
... which a sibling explicitly did not re-drive." Reading the existing suite found two solid named
tests already covering two of the three arms — `a_text_answer_leaves_the_surface_and_clears_the_pending_bar`
and `cancel_on_a_question_closes_it_without_an_answer` (`rust/crates/tiller_ui/src/chat.rs`) — but
no test anywhere exercises a **listed option** click. Tracing the render code
(`Entry::Permission` in `chat.rs`) and the fixture confirmed why: `chat_fixture.py`'s existing
`question` mode always sends `"options": []` at the wire top level specifically so the client is
forced onto the free-text field (its own docstring says so); nothing in the fixture ever sent a
structured question with populated wire options, so the "render clickable pills instead of a text
field" branch (`else if let Some(input) = text_input ... } else { for option in options { ...
permission-option-<id> ... } }`) had no test at all.

Closed it properly rather than routing around it: added a `question-options` mode to
`rust/crates/tiller_ui/tests/fixtures/chat_fixture.py` (purely additive — a new
`request_question_with_options()` sending real wire `options: [blue, green]` alongside the
`rawInput.questions[...]` shape, no existing mode touched) and a new named test
`a_listed_option_leaves_the_surface_and_clears_the_pending_bar` in `chat.rs`, sibling to the two
above. It asserts `question-answer-input` (the text field) is **absent**, both
`permission-option-blue` and `permission-option-green` pills are drawn, clicks the Blue pill with
`cx.simulate_click`, and confirms: the card records `resolved == "Blue"`, `pending-question-bar`
disappears, and the agent's echoed reply (`"You picked: blue"`) lands in the transcript — the full
round trip, not just the click registering.

```
cargo test --manifest-path rust/Cargo.toml -p tiller_ui a_listed_option_leaves_the_surface_and_clears_the_pending_bar
test chat::tests::a_listed_option_leaves_the_surface_and_clears_the_pending_bar ... ok
```

Re-ran the two sibling tests plus the full `chat::` module (`cargo test -p tiller_ui --lib chat::`)
to confirm nothing regressed: 76 passed, 1 unrelated failure
(`stopping_via_click_with_a_queued_item_still_sends_it`) that reproduces only under full-suite
concurrency and passes clean in isolation — a pre-existing flake, not something this change
touched (confirmed by running it alone: `ok`).

All three arms of F-CHAT-25 now have named, replayable, real-event-dispatching evidence: text
answer, listed option, and cancel.

### F-TERM-PTY-04 — PASSED (upgraded, shell-fallback leg)

Clause: prefers `$SHELL` then `/bin/zsh`, chooses `xterm-ghostty` when available otherwise
`xterm-256color` (PLATFORM: terminfo choice is a macOS/reference concern — Linux's
`rust/crates/tiller_terminal/src/lib.rs` hardcodes `TERM=xterm-256color` unconditionally, no
ghostty branch exists to drive, so that half is N/A on this platform by the row's own PLATFORM
note). The unproven leg named in the brief: "shell-fallback chain confirmed by **code**" —
EVIDENCE-STANDARD.md is explicit that a code read is not a verdict.

This host has no `/bin/zsh` at all (`ls /bin/zsh` -> no such file), which turns "which shell did
the fallback choose" into an unusually sharp, unfakeable discriminator. Added a named test,
`system_shell_falls_back_to_bin_zsh_when_shell_is_unset` (`rust/crates/tiller_terminal/src/lib.rs`):
removes `$SHELL` from the process env (save/restore, matching the existing
`boot_settings_honor_tiller_socket_enable_environment_override` precedent in `main.rs`), spawns a
real `TerminalShell::System` PTY, and asserts the resulting spawn error's message contains the
literal string `/bin/zsh` — not merely "a shell failed", the *exact* fallback path.

```
cargo test --manifest-path rust/Cargo.toml -p tiller_terminal system_shell_falls_back_to_bin_zsh_when_shell_is_unset
test view_tests::system_shell_falls_back_to_bin_zsh_when_shell_is_unset ... ok
```

The other half of "prefers `$SHELL`" — the success path — was already live-evidenced this pass
incidentally: every `wayland-drive.sh` screenshot this session shows the auto-opened Terminal
tab's own neofetch banner reading `Shell: bash 5.2.21`, i.e. `$SHELL` (`/bin/bash` on this box) is
what a real spawned pane actually runs. Combined with the new test's fallback proof, both halves
of "prefers `$SHELL` then `/bin/zsh`" now have a real discriminator, not a code citation.

**Scrollback-restore leg**: unchanged from the existing evidence
(`terminal_state_can_capture_and_replay_a_nonce_without_writing_to_the_child`) — not re-touched
this pass, no new gap found there.

### Environmental emergency this pass hit and could not fix — read before continuing this lane

Partway through this sweep the shared machine's root disk (`/dev/nvme1n1p1`, 452G) filled to
**100%, 144M free**, and stayed pinned there for a sustained stretch (tens of minutes) during which
every `Bash`/`Edit`/`Write` call in this session failed with `ENOSPC`. Root cause, confirmed by
`du -h -d 2 /var`: **`/var/log/syslog` had grown to 357G and was climbing at roughly 24 GB/minute**
(measured twice, ~1 minute apart) — `rsyslogd` (`ps aux` showed it at 63.9% CPU, 447 CPU-minutes
accumulated) apparently caught in a runaway logging loop, most likely fed by the repeated
GPU/compositor-contention crashes (`MESA: error: ZINK: failed to choose pdev`, D-Bus
connection-refused) that every concurrent Wayland-lane agent on this box — mine included — was
generating in bursts. **This is not a Tiller defect and not this lane's to fix**: `/var/log/syslog`
is `root:adm` owned, mode `640`; this account is in group `adm` (read-only) and `sudo` requires an
interactive password not available here. `/var/crash` (apport) was checked and is not the culprit
(107M only) — `tiller` itself does not core-dump on the ZINK failure, it exits cleanly.

**This needs the user's direct attention** — truncating `/var/log/syslog` (not touching systemd,
not restarting rsyslogd, just emptying the file so it can keep appending) would very likely be
enough, but doing that from an agent session crosses well outside "drive your own nested lane" and
into machine-wide administration, so it was left undone. Every sibling agent sharing this box was
almost certainly hit by the same outage at the same time, not just this lane.

**Effect on this report**: the disk emergency hit mid-session and stayed down long enough that the
remaining rows below could not be driven or, in one case, could not be saved to disk at all:

- A new named test for **F-CORE-FILE-03A** (drop order) was drafted — construct `ExternalPaths`
  with `[BRAVO.txt, ALPHA.txt]` in that literal order via `chat.rs`'s existing
  `dropping_external_files_attaches_chips_and_rejects_the_oversized_one` pattern, and assert
  `draft.mention_paths` preserves that order rather than alphabetising — but every `Edit` attempt
  to write it failed with `ENOSPC` and nothing was saved. **F-CORE-FILE-03A is unchanged**: still
  half-proven, drop-ordering clause still undriven, exactly as the orchestrator's brief described
  it, through no new fault — the attempt left no trace to build on.
- **F-SID-10, F-SID-11, F-SID-14, F-SID-15, F-PRJ-14, F-TERM-03, F-TERM-SCR-02, F-TERM-UI-02,
  F-CORE-FILE-04** were not reached this pass. Each is unchanged from its prior ledger state; none
  were touched, degraded, or claimed.

The disk recovered enough (144M free) for one narrow window at the very end of this session, used
entirely to write this section and commit already-verified work (F-SID-08, F-CHAT-25,
F-TERM-PTY-04) rather than start new live drives that would very likely be interrupted mid-gesture
by the same outage recurring — a resize or click stranded mid-flight by an `ENOSPC` is exactly the
kind of half-completed state this project's own evidence standard warns against recording as
either a pass or a fail.

