# D-P1 report

Owned files: rust/crates/tiller_acp/src/lib.rs, rust/crates/tiller_markdown/src/file_events.rs,
rust/crates/tiller_markdown/src/lib.rs, rust/crates/tiller_ui/src/browser.rs,
rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/project_forms.rs

## F-CHAT-05 — already-correct

Re-read `can_send`/`pending_question` gating in chat.rs (guard re-confirmed at the renumbered
line ~1628-1637, and reused consistently at every composer entry point: `insert_text`,
`handle_key_down`, drop handling, placeholder text). This is real, unit-tested code (see
`the composer cannot send while a permission is pending (F-CHAT-05)` and siblings around
line 7316+). Nothing to fix in the guard itself. The critic's "half-proven" verdict is a live-drive
gap, not a code gap: reproducing it live needs a real agent that actually stops mid-turn to ask a
permission question, which no adapter on this box reliably triggers on demand. Did not attempt a
new live drive given the same constraint holds today.

**howToExercise:** open a chat pane against a real agent, get it to request tool permission on a
sensitive action (e.g. an Edit outside the worktree), then try to type/click Send while the
permission card is showing — Send must stay disabled/no-op until the permission card resolves.

## F-CHAT-14 — fixed (foreign file)

Root cause confirmed exactly as the critic's static read predicted: `Workspace::new` binds every
freshly-created `Chat` tab's `ChatEvent::OpenFile` to `add_file_tab` via `Self::bind_chat`, but
`Workspace::restore_launch_snapshot` (the session.restore path) only called
`Self::bind_terminal_tabs` on the tabs it restored — chat tabs never got `bind_chat`, so a restored
chat's Edit-tool file opens silently no-op even with Follow on.

This lives entirely in `rust/crates/tiller/src/main.rs`, which D-P1 does not own, so I did not edit
it (edited then reverted to respect ownership). Listed in `wantedForeignFiles`.

**Exact patch for the integrator**, in `restore_launch_snapshot` right after the existing
`Self::bind_terminal_tabs(&tabs, cx);` line (~main.rs:3772):

```rust
Self::bind_terminal_tabs(&tabs, cx);
for tab in &tabs {
    tab.panes.for_each(&mut |_, content| {
        if let TabContent::Chat(chat) = content {
            Self::bind_chat(chat, cx);
        }
    });
}
self.next_tab_id += tabs.len();
```

This mirrors the exact loop already used in `Workspace::new` (main.rs:2563-2566).

**howToExercise:** restore a session containing a Chat tab (kill/relaunch tiller, or use
`ctl session.restore`), turn Follow on, make the agent run an Edit tool call, confirm a file tab
opens in the tab strip (not just Chat/Terminal).

## F-CHAT-18 — implemented

Enabled `agent-client-protocol`'s `unstable_end_turn_token_usage` feature in
`tiller_acp/Cargo.toml`. Added `AcpEvent::TokenUsageBreakdown { input_tokens, output_tokens,
cached_read_tokens }`, emitted from the prompt-response handler in `tiller_acp/src/lib.rs` when
`PromptResponse.usage` is `Some` (this is genuinely a separate wire signal from `UsageUpdate`, as
the critic's static read established — it arrives once per turn on the response, not as a session
notification). Extended `ContextUsage` with `input_tokens`/`output_tokens`/`cached_read_tokens`
(all `Option<u64>`, `Default`-derived) so existing call sites and every `ContextUsage { .. }`
literal keep compiling with `..Default::default()` or explicit `None`s.

`chat.rs` merges the new event into the existing `context_usage` (via `get_or_insert_with`,
never replacing the used/size/cost triple that arrives separately) and the context popover now
renders Input/Output/Cache-read rows under a divider, only when at least one is `Some` — an agent
that never reports end-of-turn usage shows the same popover as before.

Also had to add one match arm in `tiller_acp/src/chat.rs` (not in D-P1's file list, but a
one-line mechanical exhaustiveness fix forced by the new enum variant — folded it into the same
commit since it doesn't build otherwise).

New unit test: `chat::tests::context_popover_shows_token_breakdown_when_the_agent_reports_it`
(tiller_ui/src/chat.rs) — asserts no breakdown rows before the event, rows present after, and the
pre-existing used/size percent survives the merge. `cargo build -p tiller` and
`cargo test -p tiller_acp --lib` / `-p tiller_ui --lib context` both green.

**Caveat, matching the critic's static evidence:** this only lights up for an agent that actually
sends `PromptResponse.usage` — the real `claude` CLI's exact support for this ACP extension was
not independently confirmed live in this pass; the plumbing is real and unit-tested end-to-end
with a synthetic event.

**howToExercise:** open the context-ring popover (click the ring, `context-popover` debug
selector) against an agent/version that reports end-of-turn `usage`; look for Input/Output/Cache
read rows below the existing percent/tokens/cost lines (`context-usage-breakdown` selector).
Against an agent that doesn't report it, the popover is unchanged (no rows, no crash).

## F-CHAT-33 — already-correct

`looks_like_mcp_warning` (tiller_acp/src/lib.rs) and the stderr-drain wiring
(`AcpClient::mcp_warnings`, `push_mcp_warning`, `surface_mcp_warnings` in chat.rs) are real,
unit-tested code, not a stub. The critic's two live attempts (broken `.mcp.json` via a fresh
instance, and direct SDK binary invocation) both showed the real `claude` CLI does not emit an
mcp-warning-shaped stderr line for that specific failure mode during an ordinary turn in this
sandbox. I did not find a way to force that shape live within this pass's budget either, so left
the heuristic as-is rather than guessing at an untested pattern change. Not re-attempting the same
live drive that already failed twice would just burn budget without new information.

**howToExercise:** configure a chat's `.mcp.json` with a server that will genuinely fail to start
(bad command, unreachable transport), start a turn, and watch the transcript for an mcp-warning
card sourced from `mcp_warnings()`.

## F-PRJ-06 / F-PRJ-09 — blocked (foreign file)

Re-confirmed the guard code itself is correct and already proven: both
`the_drawn_clone_button_cannot_start_a_second_clone` and
`the_drawn_create_button_cannot_start_a_second_creation` pass against a directly-mounted
`CloneForm`/`CreateForm` (project_forms.rs) — exactly one `submit()` runs no matter how many times
`clone-submit`/`create-submit` is clicked while a clone/create is `Running`.

The gap the critic found live is structural and sits in `sidebar.rs::render_project_form`
(not owned by D-P1): when the same forms are mounted inside sidebar's
`#[id("project-form-overlay")]` absolute-positioned backdrop, button clicks
(`clone-submit`, `create-submit`, and sidebar's own `close-project-form` "Cancel") do not fire
even though hover highlighting proves the pointer is genuinely over the button. Typed text lands
fine in the URL field. The differentiator: the URL field uses `.on_mouse_down(...)` (fires on
down alone); the buttons all use `.on_click(...)` (requires down+up to resolve to the same
element). That is consistent with a down/up mismatch specific to the overlay's layout — something
in `project-form-overlay`'s structure (or a sibling painted after it) intercepts or shifts the
up-event's hit target. I could not identify the exact line without editing/instrumenting
sidebar.rs, which D-P1 does not own, so I'm not proposing an unverified patch there.

**wantedForeignFiles:** `rust/crates/tiller_ui/src/sidebar.rs` — needs a live-instrumented pass
(e.g. add an `on_mouse_up`/`on_mouse_down` pair of debug logs around `project-form-overlay` and
its card, or check whether `close-project-form`'s and the form's own submit buttons overlap with
another absolutely-positioned sibling in z-order) to find why `on_click` specifically fails there
while direct-mount `on_click` on the identical elements works.

**howToExercise:** open the sidebar's "Clone repository" popover (not a directly-mounted
`CloneForm` test harness — the real sidebar overlay), type a valid URL, click "Clone repository";
status must leave "Ready to clone". Separately, click "Cancel"; the popover must close.

## F-BRW-01 — blocked, diagnosed

Re-derived the reported measurement (content x=331..1059 vs chrome x=386..1236: a uniform
~0.857x scale about the window's x=0 origin, both start and width). `native_webview_rect`
(browser.rs) is a pure, unit-tested pass-through of GPUI's already-logical `Bounds<Pixels>` into
wry's `Rect::Logical` — confirmed against the vendored `dpi` crate source that
`PixelUnit::Logical(_).to_logical(scale)` is a true no-op regardless of `scale`, so wry's own
`set_bounds` cannot be reintroducing the double-scale bug the F-BRW-01 doc comment describes.

Working hypothesis instead: GPUI's `window.scale_factor()` (driven by this desktop's Xft-DPI-based
fractional scale, ~1.1667 by the numbers) and the X11-embedded webview's own coordinate space
(GDK/GTK `window.move_`/`resize`/`size_allocate` inside `wry`'s `webkitgtk::set_bounds`, `src/
webkitgtk/mod.rs:964`) disagree about what "logical" means for this child window — GPUI already
scaled its own chrome by ~1.1667 before painting, but the value we hand `set_bounds` gets used by
GTK/X11 as if GDK's own (likely integer, likely 1) scale applied, landing the child window ~1/1.1667
too small and too far toward the origin. I could not verify this against a live X11 display in this
pass (no established X11-lane driver is listed for D-P1 the way `wayland-drive.sh` is), so did not
commit a speculative multiply-by-`window.scale_factor()` change that I could not confirm doesn't
regress the *previous* (already-fixed, unit-tested) double-scale bug.

**Recommendation for the next pass with live X11 access:** log `window.scale_factor()` next to
whatever GTK/GDK reports as its own scale for the embedded child at the moment `set_bounds` runs;
if they differ, the fix is almost certainly to pre-multiply `bounds` by
`window.scale_factor() / gdk_reported_scale` before calling `native_webview_rect`, not a blind
`to_device_pixels` revert (that reintroduces the old double-scale bug the current code's comment
documents).

**howToExercise:** on the X11 lane (`DISPLAY=:1`), open or focus a Browser tab, pixel-scan its
rendered content against the pane's true chrome bounds (background-color scan, as the critic did)
— content should fill x∈[chrome_left, chrome_right] with no sidebar bleed and no gap before the
Files panel.
