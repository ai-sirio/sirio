# P50 activity wiring contract

This is the handoff from the terminal/pane layer to the single
`AgentActivityModel` owned by `TillerWorkspace`. The terminal context-menu
event is not part of this contract: `TerminalContextAction::SetTitle` is a
user-set pane label, while `TerminalActivityEvent::OscTitle` is title text
received from the PTY's OSC 0/2 stream.

## Terminal source

`TerminalView` emits `TerminalActivityEvent` after the existing PTY event
coalescing window (`4 ms`):

| Event | Meaning | Activity call |
| --- | --- | --- |
| `OscTitle(String)` | A real OSC title from `alacritty_terminal::Event::Title`; `ResetTitle` is represented by an empty string | `panes::apply_terminal_activity_event(&mut workspace.activity, pane_key, event, Instant::now())` |
| `OutputSettled { scrollback: String }` | PTY output was quiet for the coalescing window; text is the emulator's retained plain-text scrollback | The same reducer; it calls `detect_content_status` and then `apply_content_signal` |
| `ChildExited { status: TerminalExitStatus }` | The PTY child emitted its final exit signal | The same reducer; it maps the status to an exit code and applies the spawn-owned exit path |

Subscribe to `TerminalActivityEvent` on every live terminal entity in
`TillerWorkspace::bind_terminal`, alongside (but separately from) the existing
`TerminalContextEvent` subscription. Use `pane-{pane_id}` as the activity key,
the same key used by `add_agent_tab` and `tab_status`. If the reducer returns
a transition, call `workspace.sync_activity(cx)` so the sidebar row and tab
surface redraw from the one model.

Do not route `TerminalContextEvent::SetTitle` to the activity reducer and do
not use the tab label as an OSC title.

## Layer-D refresh

Start one detached GPUI task for each live terminal in `bind_terminal`. Every
`panes::PROCESS_SIGNAL_INTERVAL` (`500 ms`) read
`terminal.read(cx).shell_pid()` and call:

```text
panes::refresh_process_signal(
    &mut workspace.activity,
    "pane-{pane_id}",
    shell_pid,
)
```

The interval is deliberately 500 ms: it catches a native foreground agent or
its disappearance quickly enough for a sidebar badge, while keeping the
bounded `/proc` walk out of redraw frequency. The constant is the single
change point if profiling or UX evidence calls for another cadence.

On every successful refresh call `workspace.sync_activity(cx)`, including an
`Ok(None)`: an unmatched scan can have invoked `process_gone` and cleared a
process-owned row without returning a transition. Stop the task when the weak
workspace is gone or its `(tab_id, pane_id)` no longer resolves to that
terminal. Ignore a failed refresh after logging it; `NotFound` is already
treated as process disappearance by the pane helper.

## Ownership rules preserved by this seam

- OSC title mismatch clears only title-owned state through
  `AgentActivityModel::handle_title_change`.
- A settled content match changes status but never changes ownership.
- A child-exit event is applied only to spawn-owned state by
  `apply_terminal_activity_event`; title-owned and process-owned panes keep
  their independent clearing paths.
- Layer D's refresh is the only caller that can clear process-owned state:
  `refresh_process_signal` invokes `process_gone` when the matching descendant
  disappears. An unrelated OSC title must leave that status visible.

## Evidence already in the owned crates

- `tiller_terminal::view_tests::real_pty_emits_osc_title_and_settled_output`
  uses a real `/bin/sh` PTY and observes both typed events.
- `tiller::panes::tests::real_pty_activity_status_follows_osc_title_then_settled_content`
  subscribes to that PTY and observes `Running` from `. working`, then
  `NeedsInput` from settled `Do you want to proceed?` text.
- `tiller::panes::tests::real_child_refresh_clears_process_owned_status_only_on_process_gone`
  discovers a real matching child under a shell, survives an unrelated OSC
  title, and clears only after the process refresh observes its disappearance.
- `tiller::panes::tests::layer_a_debounce_still_suppresses_two_title_events_in_order`
  proves the first contradictory title is suppressed and the second applies
  after the 1.5-second Layer-A window.

