# P73 — The identity that is stored, then thrown away in transit

**This brief is everything you need; your context was just reset.**

## The shape, and why the last two attempts did not close it

Pick Claude Code from **New Chat** and you get Claude. Pick Codex and you get Codex — the critic has
now proved this on the running app, not by test: two tabs from the same binary in the same session,
one reporting `idle · Sonnet` and the other `idle · GPT-5.6-Luna`, and those model names come from
the ACP server that answered, not from our code.

**Then quit and reopen, or resume the chat, and every chat comes back as Claude.**

This has been called "the `main.rs` consumer half" twice and it did not close, because the missing
link is not in `main.rs`. It is a **three-link chain, and link 2 is the broken one**:

| # | where | state |
|---|---|---|
| 1 | `tiller_persistence` `TabRecord.agent_id` | **exists** — P70, column added in migration v10, round-trip test passing |
| 2 | `tiller/src/session.rs:286` `SessionTab` | **has no `agent_id` field** — built at `session.rs:804` from a `TabRecord` whose `agent_id` is simply not read |
| 3 | `main.rs:6656`, `main.rs:6754` `restore_tabs` | `"chat" => TabContent::Chat(cx.new(Chat::launch))`, and `agent_id: None` on the `OpenTab` |

So the identity **is** written to SQLite correctly and then dropped on the floor one layer before
`main.rs` could use it. Whoever looks only at `main.rs` sees a `SessionTab` with no identity on it
and concludes the persistence work never landed. It did. Verify link 1 yourself before you start —
`grep -n agent_id rust/crates/tiller_persistence/src/model.rs` — so you are not taking my word for it.

## The fourth site: resume

`main.rs:293`:

```rust
struct RetainedChat {
    id: usize,
    title: String,
    transcript: String,
}
```

`main.rs:3965` `resume_chat` does `let mut chat = Chat::launch(cx);` and then sets
`agent_id: None` at `:3975`. Same defect, different path: a closed chat's agent is not retained,
so reopening it silently downgrades to the hardcoded default. This one needs no persistence at
all — the identity is in memory and is discarded when the tab is retained.

## What is already there and must not be rebuilt

- `chat.rs:474` `pub fn launch_with_command(command: AgentCommand, cwd: PathBuf, cx) -> Self` —
  landed in P66, **already called** at `main.rs:3920`. This is the constructor you want.
- `chat.rs:382` `pub fn acp_agent_command(program: tiller_agents::AcpProgram) -> AgentCommand`
- `AgentAvailability::acp_program() -> Option<AcpProgram>` in `tiller_agents`
- `main.rs:3908` `chat_tab_identity(adapter)` already returns `(title, agent_icon, agent_id)`
- The live `OpenTab` already carries `agent_id: Option<String>` (`main.rs:280`), set at `:4184`
  and `:4510` and read at `:3174`

**The `None` branch at `main.rs:3922` is correct as written — do not touch it.** `tab_bar.rs:467`
filters the picker with `.filter(|agent| agent.is_available() && agent.acp_program().is_some())`,
so a non-ACP agent is never offered there. That branch guards a state a user cannot produce. A
previous brief claimed otherwise and was withdrawn; the withdrawal stands.

## What "done" looks like

An agent chosen once survives **both** round trips:

- close the tab and resume it → same agent
- quit and relaunch → same agent

and an unknown or absent `agent_id` still opens something sane rather than panicking, because v9
rows exist and carry no identity by construction (P70's own test
`v9_tab_rows_upgrade_to_v10_with_no_recorded_agent_identity` covers that at the storage layer).

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

The test that counts **round-trips a non-default agent**: persist a chat tab whose agent is Codex,
restore, and assert the restored tab's identity is Codex — not that a setter was called. A test
that restores a Claude tab and finds Claude proves nothing, because Claude is what the bug
produces. **Use Codex, or any non-default, or the test cannot fail.**

Note: `pump_until(..)` in `changes.rs`/`right_panel.rs` ends in `panic!`, so a wait *is* an
assertion — a legitimate way to prove an async restore arrived.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your
  pane starts in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Yours:** `tiller/src/main.rs`, `tiller/src/session.rs`, `tiller_control/**`, `tab_bar.rs`.
  All three links are inside your own crate, so this needs no seam and no hand-off.
- **Do not edit** `chat.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs`, `tiller_agents/**` (`pi`);
  `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`,
  `tiller_terminal/**` (`codex11`); `tiller_theme/**`, `controls.rs`, `titlebar.rs`,
  `composer.rs` (`sonnet`). `tiller_persistence/**` is done and needs nothing.
- **Standing rule:** whoever widens a struct or enum owns every construction site and match arm it
  breaks, in any file — but only those. Widening `SessionTab` will break its literals; they are
  yours to fix.
- Colours, spacing and radii from `tiller_theme::Theme`, never a literal. Visual bar is **Pop!_OS
  COSMIC**, not waku. Name any token you need and lack; that list is how `sonnet` learns.
- **Establish the build state with the gate's own commands**, not a paraphrase:
  `grep -n clippy Scripts/ci-linux.sh` and run exactly what it says. The orchestrator once ran a
  weaker clippy without `-D warnings`, called the tree clean while the gate was red, and overruled
  three agents who were right. Do not inherit that mistake.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Also yours, and small: P71 Half B

`codex11` landed Half A — `file_view.rs:66` now has `notice`, `set_notice`, `clear_notice`, with
the constructor signature unchanged, so nothing of yours is blocked. Three `eprintln!` sites become
visible messages:

- `main.rs:~2939-2940` `add_project` → `self.sidebar.update(cx, |s, cx| s.set_notice(…, cx))`.
  Both arms. `Ok(false)` is *"already tracked or nested"* — a normal thing a user does, not an
  error. **This closes `F-PRJ-04` and is roughly two lines.**
- `[files] save failed: {error}` and `[files] could not open the file picker: {error}` → Half A's
  setter.

`Sidebar::set_notice` is public at `sidebar.rs:613`, rendered as `sidebar-notice`, and **already
used** at `sidebar.rs:662` for the folder picker's failure — you are calling it, not editing
`sidebar.rs`. `F-PRJ-04`'s ledger row blames a missing error surface; the surface exists and is
wired, and the cause is a disconnected call. Say that in your report.

**Leave the other 34 `eprintln!` alone.** `[control] …`, `[session] …` and the window-open failure
are diagnostics for whoever runs the binary from a terminal. If you think a fourth belongs, name it
rather than doing it — one was already withdrawn for exactly that reason.

The test that counts here **provokes the real failure**: add a project that is already tracked and
assert the sidebar notice renders with that text; make a save actually fail with a read-only path
rather than injecting an error.

## Reporting

**12 lines or fewer**: which of the three links you widened and what broke when you widened
`SessionTab`, the drawn test that round-trips a **non-default** agent through quit-and-relaunch and
through resume (by name), whether `RetainedChat` now carries identity, the `F-PRJ-04` sites you
converted and that its cause was a disconnected call rather than a missing surface, the gate run
with its own invocation with not-yours failures named separately, tokens `tiller_theme` still
lacks, and the honest remainder.
