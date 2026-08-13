# P74 — The ACP processes nobody reaps, and the channel closed on the wrong thread

**This brief is everything you need; your context may have just reset.**

## Why this piece exists now: three independent reports converged on one crate

This is not a hunch. Three observations, from three different places, none of which were looking for
each other:

1. **`pireview`, pass 14, live on the running app:** *"orphaned `-acp` processes on app kill"* — the
   agent servers outlive the binary that spawned them. Also, at a single launch it counted **four
   `claude-agent-acp` processes**, including one for a tab whose agent was Codex.
2. **`codex12`, just now, in a drawn test:** the P73 restore test *starts* correctly and then dies in
   **teardown**, not on its assertion — *"il restore avvia l'ACP reale e il thread `tiller-acp` chiude
   un canale sul thread sbagliato (Detected activity … test scheduler …)"*. A worker thread wakes the
   GPUI test scheduler after the test believes it is finished.
3. **The orchestrator, verifying the build:** restore spawns real agent subprocesses eagerly, so a
   test that merely *restores a session* is a test that *launches agents*.

Observation 2 is the mechanism for observation 1. A channel closed from the wrong thread means the
shutdown path is not running where it thinks it is — which is precisely the shape that leaves a child
process unparented when the parent goes away.

**`tiller_acp/**` and `tiller_agents/**` are yours** (claimed in P42, confirmed in P54's do-not-edit
list for everyone else). This defect is in your crate and nobody else can touch it.

## What to establish first, before changing anything

Do not start from the fix. Start from the count, because the count is the thing that can be shown to
have changed:

- Launch the app, open two chats with **different** agents, and record `ps` output for `*-acp`
  processes. `pireview` saw four servers where two were expected — find out whether that is one
  server per tab, one per launch, or one per message.
- Kill the app. Record `ps` again. The orphans are the delta.
- Note that **P73 has just landed** (`codex12`, `SessionTab.agent_id` + `RetainedChat` + restore and
  resume). Before P73 every restored chat was secretly Claude, which on its own explains *four
  `claude-agent-acp`*. So **re-measure on current `main`**: part of observation 1 may already be
  fixed, and claiming a fix for something P73 fixed would be a false positive. Say which part
  survived.

## The two things that are probably distinct

Keep them apart in your head and in your report — they may have one cause or two:

- **Shutdown ordering** — the app exits without giving the ACP client a chance to terminate its
  child, or terminates the wrong side first. This is what orphans processes.
- **Thread affinity on drop** — a channel closed from a thread that does not own it. This is what
  breaks `codex12`'s test teardown, and it is the more diagnostic of the two because it has a
  reproducible trigger that needs no GUI.

`codex12` has worked around the second one **in its own test** by clearing tabs before teardown. That
is legitimate for a test and it is **not** a fix — the workaround lives in `main.rs` and the cause
lives in yours. Do not treat that test going green as evidence.

## What "done" looks like

- A named test in your crate that **fails before your change and passes after it**. State both runs.
  A drop/shutdown ordering bug is exactly the class where a test can pass for the wrong reason, so
  the before-run matters more than usual here.
- Killing the app leaves **no** `*-acp` process behind — shown as a `ps` count before and after, not
  asserted in prose.
- `codex12` can remove its teardown workaround. **Tell it when that is true**; do not edit `main.rs`
  yourself to remove it.

## Also yours, and small: the frame P72 still owes

Your spike proved the composite — `reference/linux-progress/p72-browser-spike-xlib-bridge.png` shows
a native GPUI sidebar and a live WebKitGTK child in one window, and that is a real result.

**But the brief asked for a GPUI element deliberately overlapping the web view, and that frame has no
overlap** — the sidebar and the page sit side by side. You reported *"child WebKit sempre sopra
GPUI"*, which is the known bad outcome, but the screenshot does not show it. That one frame decides
whether the nine `F-BRW` rows are a browser **tab** or a browser **window**: if a GPUI popover, menu
or dialog renders *underneath* the page, an in-app browser tab is not viable as designed, and
`F-BRW-09` becomes a question for the user rather than a build.

So: draw a GPUI element that crosses the web view — a dropdown, a floating card, anything with a
z-index above the page area — screenshot it, and put the image next to the first one. **Two outcomes,
both worth having.** If it renders above, say so and the browser tab is viable. If it renders below,
say so plainly; that is the single most valuable sentence in the whole P72 spike and it costs one
screenshot.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your
  pane may start in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Yours:** `tiller_acp/**`, `tiller_agents/**`, `tiller_ui/src/browser.rs`, and your usual set
  (`changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`,
  `tiller_terminal/**`).
- **Do not edit** `main.rs`, `session.rs`, `tiller_control/**`, `tab_bar.rs` (`codex12` — actively in
  them right now); `chat.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs` (`pi`); `tiller_theme/**`,
  `controls.rs`, `titlebar.rs`, `composer.rs` (`sonnet`). **`chat.rs` is `pi`'s and it is where
  `AcpClient` is consumed** — if the fix needs a change on that side, name it as a seam.
- **Standing rule:** whoever widens a struct or enum owns every construction site and match arm it
  breaks, in any file — but only those.
- **Establish the build state with the gate's own commands.** Measured by the orchestrator minutes
  ago: `cargo fmt --all -- --check` green (you fixed it), `cargo clippy --workspace --all-targets
  --exclude tiller --exclude tiller_ui -- -D warnings` = EXIT 0, `cargo build -p tiller
  -p tiller_control` = EXIT 0. `cargo test --workspace` is the one unmeasured stage and `codex12` is
  in it now.
- The system GTK headers are installed globally now. **No `PKG_CONFIG_PATH`, no sysroot.**
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the `ps` counts before and after app kill, measured on current `main` so P73's
effect is not credited to you; which of the two defects (shutdown ordering, thread affinity) you
found and whether they had one cause; the test that **failed before and passes after**, by name, with
both runs stated; whether `codex12` can drop its teardown workaround; what the overlap frame showed
and its path; any seam you left for `pi` in `chat.rs`; the gate run with its own invocation with
not-yours failures named separately; and the honest remainder.
