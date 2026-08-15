# C-CHAT-1 report

## F-CHAT-15 — mode pill wired to the live ACP session-mode catalog

Fixed. `crates/tiller_ui/src/chat.rs`. The status pill (`chat-status`) rendered a
hardcoded `"Ask"` label and had no click handler at all — the "flips a hardcoded
4-way UI-state string" evidence describes an older version; by the time this
link started, the pill wasn't even clickable.

Rather than add a new `AcpEvent::ModeCatalog` variant (that path breaks the
exhaustive `match event` in `crates/tiller_acp/src/chat.rs`, a file owned by
`C-MAIN-3`, not this slice — tried it, reverted it, see `wantedForeignFiles`
below for the alternative that file's owner could still take), the fix polls
the already-live `AcpClient::mode_catalog()` accessor (its own doc comment:
"re-reading it after any event... picks up... an agent-pushed
`CurrentModeUpdate`") after connect and after every `handle_event`. New state:
`mode_catalog: Option<ModeCatalog>`, `mode_picker_open: bool`,
`mode_picker_focus: FocusHandle`. New methods: `toggle_mode_picker`,
`select_mode`. The pill's label now reads the agent's current mode name when
a catalog exists, falling back to the old `"Ask"`/`"idle"`/`"working"`/
`"offline"` words otherwise — no regression for agents that never advertise
`modes` (most ACP agents today, per the accessor's own doc comment).

**howToExercise**: `wayland-drive.sh` a live Chat tab against an ACP agent
that advertises `session/new`'s `modes` field, wait for `has_completed_turn`
(complete one turn), then `click` the status pill (`chat-status` bounds) —
a dropdown listing the agent's modes should open anchored above it; click one
of the `mode-option-<id>` rows and confirm the pill's label text changes to
that mode's name and the dropdown closes. Escape or a click outside also
closes it (same `Cancel`/`on_mouse_down_out` wiring as the model picker).
If no live agent in this environment advertises `modes` (Claude/Codex/Pi
ACP bridges observed so far don't), this can't be driven live on this
lane — the new test
`chat::tests::mode_picker_selects_an_agent_advertised_mode_and_updates_the_pill`
drives the same code path with a fixture `ModeCatalog` and passes.

Build: `cargo build -p tiller_ui` green; `cargo build -p tiller` also green
except for a **pre-existing, unrelated** break in `crates/tiller/src/main.rs`
(a `restore_tabs` call-site missing a new `saved_session_refs` argument) —
that file is mid-edit by a concurrent agent outside this slice's ownership,
not touched by this commit. `cargo test -p tiller_ui chat::` — 56/56 passing
before this change, 57/57 after.

Commit: `ac696cb fix(chat): wire the mode pill to the live ACP session-mode catalog`

## F-CHAT-14 — Follow Edited Files doesn't open a file tab on restored sessions — blocked, foreign file

Root cause confirmed exactly as the evidence on record describes, and it's
outside every file this slice owns. `TillerWorkspace::bind_chat` (the
subscription that turns a `ChatEvent::OpenFile` into a real file tab) is
called only from `add_chat_tab`/`resume_chat`. `TillerWorkspace::new`
(`crates/tiller/src/main.rs`, not owned by this slice) does the equivalent
wiring for `TabContent::Changes` tabs restored from a session (loop at
`crates/tiller/src/main.rs:2488-2494`, calling `Self::subscribe_changes_tab`
for every `TabContent::Changes`) but has no matching loop for
`TabContent::Chat` — so a chat tab that arrived via `restore_tabs` or
`restore_tabs_in_workspace` (both also in `main.rs`, not owned) never gets
`bind_chat`'d, and `ChatEvent::OpenFile` fires into the void.

**wantedForeignFiles**: `rust/crates/tiller/src/main.rs`
**Precise fix for the integrator**: in `TillerWorkspace::new`, right after
the existing
```rust
Self::bind_terminal_tabs(&tabs, cx);
for tab in &tabs {
    tab.panes.for_each(&mut |_, content| {
        if let TabContent::Changes(changes) = content {
            Self::subscribe_changes_tab(changes, cx);
        }
    });
}
```
add a matching loop that calls `Self::bind_chat` for every `TabContent::Chat`
found the same way (`bind_chat` takes `&Entity<Chat>, cx`, same signature
shape as `subscribe_changes_tab`):
```rust
for tab in &tabs {
    tab.panes.for_each(&mut |_, content| {
        if let TabContent::Chat(chat) = content {
            Self::bind_chat(chat, cx);
        }
    });
}
```
This covers both `restore_tabs` and `restore_tabs_in_workspace` in one place
since both feed into the same `tabs: Vec<OpenTab>` constructor argument.

**howToExercise** (once applied): toggle Follow Edited Files on via the
overflow menu on a *restored* chat tab (quit and relaunch Tiller, or use
`session.restore`), send a turn that edits a file, and confirm a file tab
opens — same gesture the evidence on record already used to prove the bug.

Not committed (no owned file changed for this row).

## F-CHAT-18 — context ring input/output/cache/cost breakdown — blocked, protocol ceiling

Re-confirmed against the vendored `agent-client-protocol-schema` crate
source directly (not just re-reading `tiller_acp`): `UsageUpdate` (the wire
type behind `SessionUpdate::UsageUpdate`, both v1 and v2 schemas) is
```rust
pub struct UsageUpdate {
    pub used: u64,
    pub size: u64,
    pub cost: Option<Cost>,
    // ...
}
```
There is no `input_tokens`/`output_tokens`/`cached_read_tokens` field on this
type at all — those field names exist only on an unrelated, separately-gated
`Usage` struct (`#[cfg(feature = "unstable_end_turn_token_usage")]`, a
different wire message, "end of turn" not "context window") that no code
path in `tiller_acp` currently requests or wires up. `tiller_acp::ContextUsage`
faithfully mirrors everything `UsageUpdate` carries — `used`/`size`/`cost` —
so there is no dropped data to surface; the breakdown the contract row asks
for does not exist on the wire this app's agents speak. This matches the
prior triage call (`docs/linux-rewrite/tasks/D3-composer-one-card.md`:
"do not build; report").

Not attempted as code — would require depending on the unstable feature flag
and an upstream agent that actually populates it, which is outside what any
file in this slice (or arguably this repo) controls. `wantedForeignFiles`:
none — this isn't a file-ownership gap, it's a protocol-data gap.

## F-CHAT-02 — no dedicated AuthRequired banner

Not attempted this pass (ran out of budget after F-CHAT-15/14/18). The
existing `ErrorKind::Connection` card is what a caller sees today; a
dedicated `AuthRequired` variant with CLI login-argv guidance would need a
new `ErrorKind` case plus a distinct render arm in `chat.rs` — both inside
files this slice owns, genuinely buildable, just not reached.

## F-CHAT-05, F-CHAT-13, F-CHAT-20 — not re-attempted

Left as recorded (half-proven / NOT EXERCISED). F-CHAT-13 and F-CHAT-20's
gaps are environment/tooling limits documented in `ENVIRONMENT.md` and the
wayland-drive DSL, not code gaps in this slice's files — no code change
available. F-CHAT-05's gap is a *live-driving* gap (no discriminating
evidence obtained), not a code gap — the guard code was already re-confirmed
structurally sound by the prior critic; this pass did not re-drive it.
