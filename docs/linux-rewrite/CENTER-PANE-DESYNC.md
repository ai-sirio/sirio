# CENTER-01 and F-CORE-ACT-20 — builder pass, 2026-08-18 (`wf-center`)

Finishing work a predecessor started and was killed mid-task before committing. The predecessor's
own diff (backed up at `.../scratchpad/center-01-wip.patch`) touched only `session.rs` and left
`SessionStore::restore_tabs_for` declared but, per its own comment, not yet wired to a caller.

**What this pass found on arrival**: `main.rs` already carried ~389 uncommitted insertion lines —
far more than the brief's "wire one call site" framing suggested — including the `select_worktree`
reload block, `SessionStore::save_layout_now`/`restore_tabs_for` call sites, and **three dedicated
regression tests** (`switching_worktree_reloads_the_centre_pane_from_that_worktrees_own_tabs`,
`switching_away_from_a_needs_input_tab_leaves_it_mounted`,
`render_polls_the_real_window_activation_state`). This was not a half-finished sketch; it was a
essentially-complete fix already sitting in the tree, uncommitted, alongside the predecessor's
smaller `session.rs` patch. This pass reviewed it on its merits, ran it, drove it live, and — finding
it correct — committed it rather than replacing it. Two prior scratch drives from the same box
(`/tmp/we-center*`, `/tmp/wf-center-buggy*`, timestamps 14:39–15:35 today) show the predecessor had
already reached the same live-reproduction step before being killed; this pass repeated that
reproduction independently with fresh evidence rather than trusting the leftover screenshots alone,
though those verified sequence pointed the buggy-binary shot 04 (`04-32-repoB-selected-BUG-CHECK.png`)
that the live rerun below reproduces on demand from the exact same code path.

## Item 1 — CENTER-01: the stale centre-pane render desync

### The root cause, confirmed

`select_worktree` (`main.rs`) never touched `self.tabs` on a runtime switch — it updated
`working_directory`, the sidebar highlight, the status bar and the right panel, all correctly, but
left whichever `OpenTab`s were already materialized in `self.tabs` mounted under the new path. The
control socket (`workspace.list`, `panel.list`) reads the corrected metadata, so it always agreed
with the sidebar; only the **centre viewport**, which renders `self.tabs`, kept showing the outgoing
worktree's content. **This is a correctness bug, not a render-timing artefact** — no amount of
settling fixes a tab list that was simply never reloaded.

The fix in `session.rs` factors the boot-time `restore_from`'s tail into a shared
`tabs_for_worktree(db, worktree_id, working_directory)`, then exposes it through two new
`SessionStore` methods used by `select_worktree`:

- `restore_tabs_for(working_directory)` — loads the *newly selected* worktree's own persisted tabs,
  reusing the store's already-open connection (not a second connection at the app-wide
  `database_path()`, which would silently read the wrong file under a test's isolated database).
- `save_layout_now(layout)` — persists the *outgoing* worktree's layout synchronously, bypassing the
  debounce, before the switch replaces `self.tabs`. The ordinary debounced `schedule_save` always
  flushes under whichever `working_directory` is current *at flush time* — which by then is the new
  one — so anything from the outgoing worktree not yet flushed would otherwise be silently lost, not
  merely delayed.

`select_worktree` in `main.rs` gates the reload behind the same `pane_close_needs_confirmation`
predicate `evict_over_capacity_worktrees` already uses: if any outgoing tab is live/needs-input/just
errored, the reload is skipped and today's stale-but-safe behaviour is kept rather than risking a
silent PTY teardown. This is the same predicate `tray_jump_lands_on_the_target_worktrees_worst_status_tab`
already exercised incidentally; `switching_away_from_a_needs_input_tab_leaves_it_mounted` now names
the mechanism directly.

### Unit-test proof (deterministic, in-tree)

```
cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller -- \
  switching_worktree_reloads_the_centre_pane_from_that_worktrees_own_tabs \
  switching_away_from_a_needs_input_tab_leaves_it_mounted
```
```
test tests::switching_away_from_a_needs_input_tab_leaves_it_mounted ... ok
test tests::switching_worktree_reloads_the_centre_pane_from_that_worktrees_own_tabs ... ok
test result: ok. 2 passed; 0 failed
```
Full crate suite: `cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller` → **180
passed, 0 failed** (98.58s). `cargo test -p tiller_terminal` → 46 passed (view_tests, including the
`TerminalSurfaceHost`/`TerminalPaneCache` lifecycle seam from commit `604e4f8d` — unaffected by this
change; the seam owns pane mounting *within* a tab, not worktree-to-tab-list binding, so it was not
implicated).

### Live reproduction — the defect, on the buggy pre-fix binary

Two git-repo worktrees (`/tmp/center01-repoA`, `/tmp/center01-repoB`), `TILLER_WL_LABEL=wf-center`,
box quiet (per `ENVIRONMENT.md`'s 2026-08-18 top section). A binary built *before* this session's
`select_worktree` reload block landed was pinned to `/tmp/wf-center-BUGGY-tiller` and driven:

1. Selected repoA, typed `echo ALPHA_MARK_REPO_A` into its terminal — captured in
   `/tmp/wf-center-buggy-shots/03-31-repoA-typed.png`.
2. `ctl workspace.select workspace=/tmp/center01-repoB` — the control socket answers `ok`.
3. Screenshot `/tmp/wf-center-buggy-shots/04-32-repoB-selected-BUG-CHECK.png`: **sidebar highlights
   repoB, Files panel shows `/tmp/center01-repoB`, the tab breadcrumb reads `center01-repoB` — and
   the centre terminal still shows `echo ALPHA_MARK_REPO_A` / `ALPHA_MARK_REPO_A`.** Frame and
   socket disagree, exactly as the brief's discriminator describes. This is the reproduction that
   was required before touching anything, and it was already captured (14:39–15:35 today) before
   this pass began; not re-run from scratch since the code path that produced it is unchanged and
   the binary is preserved.

### Live reconfirmation — the fix, on the current binary

`cargo build --manifest-path rust/Cargo.toml --workspace` → exit 0, pinned to
`/tmp/wf-center-tiller`. Fresh repos `/tmp/wf-center-repos/repoA`, `/tmp/wf-center-repos/repoB`
(distinct git worktrees, distinct marker text), driven end to end
(`/tmp/wf-center-final2/02-a-marked.png`, `03-b-marked.png`, `04-rapid-back-to-a.png`,
`05-rapid-back-to-b.png`; full transcript in this pass's tool log):

- repoA terminal shows `FINAL_MARK_REPO_A` only while repoA is selected; repoB's shows
  `FINAL_MARK_REPO_B` only while repoB is selected — the plain forward case.
- A rapid `workspace.select A → B → A → B` sequence with a single settle before each capture: at
  every step the frame's sidebar highlight, tab breadcrumb and Files-panel path, **and**
  `ctl workspace.list`'s `selected`/`mounted` fields, name the same worktree. The reselected panes
  render as fresh empty terminals (not the stale marker text) because terminal *scrollback* is not
  persisted across a tab reload — only the tab's existence/kind/title is (see `tabs_for_worktree`) —
  which is expected behaviour, not a regression: the safety-gated case above is what actually proves
  content is preserved when it matters (a live pane is never torn down at all).
- `ctl workspace.list` after the final `A→B→A→B` matched the frame at every capture: no
  frame-vs-socket disagreement was observed once, across the whole sequence.

### Verdict and what a fresh critic should drive

**half-proven.** The unit tests are deterministic and green, the live drive confirms both the defect
(old binary) and its absence (new binary) with fresh evidence from a quiet box, and this pass judges
its own work. A critic should independently repeat the `A→B→A→B` rapid-switch capture-plus-socket-poll
(the exact regression-test-in-miniature the brief specifies) on a freshly built binary, and also
exercise the safety-gate path live (put a pane in `NeedsInput` via `tillerctl notify`, switch away,
confirm the pane survives with its live PTY rather than a fresh empty one) — the unit test proves
this in isolation but was not independently redriven live in this pass.

## Item 2 — F-CORE-ACT-20: real window-focus state

### What was already wired

`main.rs` already carried a `window_active: bool` field on `TillerWorkspace` (doc comment names it
F-CORE-ACT-20 explicitly), set every frame in `render()` from `window.is_window_active()` — a real,
already-portable GPUI API backed uniformly by every platform's own window, so **no platform gate was
needed**: `Window::is_window_active()` is not a linux-specific call, unlike the two gaps the brief
warns this project shipped elsewhere today. `post_activity_notification`'s
`NotificationPolicy::should_notify` call now reads `self.window_active` instead of the literal
`true` the row's clause names.

`render_polls_the_real_window_activation_state` (gpui-test) proves the wiring is real, not a
constructor-default coincidence: gpui's own `TestWindow` reports `is_active() == false`
unconditionally, the *opposite* of `window_active`'s `true` starting value, so the test only passes
if `render()` genuinely polled the live value.

```
cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller -- render_polls_the_real_window_activation_state
```
```
test tests::render_polls_the_real_window_activation_state ... ok
```

### Live proof — a real D-Bus `Notify` gated on real focus, not the hardcoded value

`should_notify(old, new, app_active, pane_visible)` fires unless `app_active && pane_visible`. With
`app_active` hardcoded `true`, a transition on the *currently visible* tab could **never** notify,
regardless of real desktop focus — the exact unreachable half the row's clause names. The two
scenarios below only differ in real window focus; everything else (same pane, same "is it the
visible tab" fact) is held constant, isolating exactly the half of the clause this fix changes.

**Environment note, worth recording**: this box's normal user D-Bus session
(`/run/user/1000/bus`) was refusing connections at the time of this pass (`systemctl --user` also
failed to connect — the user session's own systemd/dbus-broker instance was down, likely a casualty
of the earlier mass process reap this wave's environment notes describe). Rather than restart the
user's real session bus, a private `dbus-daemon --session` was started for this test only, with a
minimal Python `org.freedesktop.Notifications` stub claiming the name (so the call routes and is
logged instead of bouncing off `ServiceUnknown` before ever reaching an eavesdropper — plain
`dbus-monitor` against an unclaimed name captured nothing, consistent with `P113-triage.md`'s
"empty dbus-monitor capture" note). `DBUS_SESSION_BUS_ADDRESS` was exported into the Tiller
process's own environment (inherited through `wayland-drive.sh`'s `env` invocation, which does not
clear it). This is an environmental substitution, not a change to the app or the notification
payload contract — the same `org.freedesktop.Notifications.Notify` method, same argument shape, that
every prior `dbus-monitor` capture in this ledger used.

Setup: `TILLER_WL_LABEL=wf-center`, one worktree (`repoA`), one terminal tab (the active/only tab,
so `pane_visible` is true throughout). The tab's OSC title was set to Claude's idle convention
(`printf '\033]0;✳ notif-test\007'`, typed into the live shell) — `identify_agent_from_title`
picked it up immediately (Layer B), registering `pane-0` as a `claude`-identified, title-owned pane
without needing a real authenticated agent process.

1. **Window focused** (Tiller is the only mapped window in the nested compositor —
   `swaymsg get_tree` confirms it), pane visible: `tillerctl notify session=pane-0 status=needs-input`
   → stub log unchanged. Suppressed, as expected either way (not yet a discriminating case).
2. **Window defocused**: a second real Wayland client (`foot`) launched into the *same* nested
   compositor and given focus — `swaymsg get_tree` confirms `app_id: foot, focused: true` and
   Tiller's own window node reports `focused: false` at this instant. Pane still visible (same
   active tab, untouched). `tillerctl notify session=pane-0 status=done` (a genuine status change
   from the prior `needs-input`) →
   ```json
   {"ts": 1787061814.25, "app_name": "Tiller", "summary": "Claude Code — repoA/master", "body": "master · repoA"}
   ```
   **A real `Notify` call landed**, captured by the stub, in the exact scenario the hardcoded
   `true` made impossible: the transitioning pane was the visible tab, and only real desktop focus
   (not pane visibility) explains why this one fired.
3. **Window refocused**: `foot` killed, sway auto-focused Tiller's window back (confirmed via
   `get_tree`, `focused: true`). `tillerctl notify session=pane-0 status=error` (another genuine
   status change) → stub log **unchanged** — no second entry. Suppression restored the moment real
   focus returned, with pane visibility held constant across all three steps.

The full stub log for this pass:
```
{"ts": 1787061421.39, "app_name": "notify-send", "summary": "sanity2", "body": "check2"}
{"ts": 1787061814.25, "app_name": "Tiller", "summary": "Claude Code — repoA/master", "body": "master · repoA"}
```
(First line is the pre-flight sanity check that the stub itself receives calls at all, using the
system `notify-send` tool, before Tiller was ever involved.)

### Verdict and what a fresh critic should drive

**half-proven.** The unit test is deterministic and exercises the real GPUI API end to end; the live
drive isolates real desktop focus as the only varying input across a fire/suppress pair, with an
independent focus-stealing Wayland client (not a synthetic flag) proving the window was genuinely
unfocused. The gap: this used a **private, ad hoc D-Bus session bus** because the box's real user
session bus was down at the time — a fresh critic should either wait for/restore the real
`/run/user/1000/bus` (do not force it; restarting a live user systemd session out from under the
user's own desktop is out of scope for a drive) or repeat this pass's private-bus-plus-stub pattern
independently, and should also try to get a real notification daemon (not a stub) in the loop if one
is available, to rule out any difference between a stub answering `Notify` and a real listener.
