# H1-gaps verdicts

Critic pass, independent of the H1-gaps builder. Read `docs/linux-rewrite/wave-h/H1-gaps-report.md`
as a route, not a conclusion; re-derived each verdict from the code at HEAD plus live
`wayland-drive.sh` exercise.

## `F-CORE-DOM-07` — FAILED — defective

The `AutoNamingThrottle` wiring itself (`main.rs`'s `request_auto_rename`/`apply_auto_title`,
commit `26a2c5d4`) is real: a per-tab throttle keyed off `transcript_for_resume()` growth, a real
`cx.spawn` that shells out to a summarizer command, `title_is_auto_named` correctly flipped false
by `commit_tab_rename` so a manual rename can never be clobbered. That much is not a token call
site.

But the route the row itself names — "open a chat tab (default auto-named), send a message, let
the turn finish" — cannot ever produce a title, by construction, on the actual default
configuration. Traced the exact runtime values reachable from that route:

- `SessionLayout::default_in` (`session.rs:335`) gives the default chat tab `agent_id: None` — the
  literal "default auto-named" chat tab has no adapter identity recorded, only a real ACP process
  underneath (`npx @agentclientprotocol/claude-agent-acp`).
- `SettingsSnapshot::default()` (`settings.rs:395`) sets `summarizer_agent: SummarizerChoice::Claude`.
- `request_auto_rename`'s `summarizer_candidate_commands` therefore resolves primary =
  `ClaudeCodeAdapter` and fallback = `None` (since `tab_agent_id` is `None`).
- `ClaudeCodeAdapter::summarizer_command`, `CodexAdapter::summarizer_command` and
  `PiAdapter::summarizer_command` are all unoverridden and fall through to the trait default,
  which returns `None` — confirmed by the port's own existing regression test,
  `unported_summarizers_answer_none_rather_than_guessing` (`tiller_agents/src/lib.rs:557-561`,
  `cargo test -p tiller_agents unported_summarizers_answer_none_rather_than_guessing`: passes).
  Only `OpenCodeAdapter` and `OhMyPiAdapter` implement it.
- Result: `commands` is empty, `request_auto_rename` returns before ever spawning anything —
  silently, no matter how many turns run.

This is a real regression against the Swift reference, not a design choice: `ClaudeCodeAdapter`,
`CodexAdapter` and `PiAdapter` in `/home/enzopalmisano/Scrivania/Progetti/tiller` (the reference to
read) all implement `summarizerCommand` with real commands (`claude -p …`, `codex exec
--output-last-message /dev/stdout …`, `pi --print --no-tools …`) — the Rust port ported the
throttle/call-site machinery but silently dropped 3 of 5 agents' actual summarizer invocation,
including both agents most users default to. `auto_naming: false` by default matches the Swift
reference (not itself a defect), but even with it turned on, the default chat tab plus default
Settings produces zero titles, ever — only explicitly switching Settings → Summarizer to OpenCode
or Oh-My-Pi makes the feature do anything.

Not independently re-run against a live agent turn (would need `npx`/network and an unbounded
completion wait) — the defect is upstream of that: the code returns before any process would ever
be spawned, provable deterministically from the default settings/tab values and the adapters'
existing test suite, so a live agent turn would add nothing but latency to the same conclusion.

## `F-CORE-WSP-04` — PASSED

Live, single-invocation `wayland-drive.sh` (`TILLER_WL_LABEL=h1c-wsp04c`): `project.add`, click the
terminal to focus it, right-click the tab strip, click `Rename` at its measured position, type
`RENAMEDXYZ`, `key Return`. The tab strip and sidebar both show the committed title
`TerminalRENAMEDXYZ`, and the very next `type Q` (no click in between) shows `Q` landed at the bash
prompt in the terminal — not dropped. A second single-invocation drive
(`TILLER_WL_LABEL=h1c-wsp04d`) repeated the gesture but pressed `Escape` instead of `Enter`: the
title reverted to `Terminal` (edit discarded) and the following `type Z` (no click) again shows `Z`
at the bash prompt. Both are the exact gesture the row names, and both discriminate against the
pre-fix behavior (focus stranded on the unmounted rename field, keystrokes silently vanishing)
described in the row's own root cause.

Code: `commit_tab_rename` builds a real `tiller_project::LayoutCommand::Rename` and consults
`classify_layout_command(&command).focus == FocusIntent::Tab` before calling
`focus_tab_content`; Escape shares the same `focus_tab_content` path
(`main.rs`, commit `8eb880f5`). Note `classify_layout_command` answers `FocusIntent::Tab` for every
`Rename` unconditionally (the match arm doesn't inspect the command's fields), so the boolean
check itself is a constant true today — but it is still a real, consulted call site (the first one
`LayoutCommand`/`classify_layout_command` has ever had outside `tiller_project` itself), and the
live drive proves the actual focus-restoration behavior works end-to-end, which is what the row
asks for. `cargo test -p tiller committing_a_tab_rename_returns_focus_to_the_terminal --lib`:
passes.

## `F-CORE-WSP-08` — PASSED

Re-drove the exact restart-survival bar independently (own draft text, own two separate
`wayland-drive.sh` invocations, same `TILLER_WL_LABEL=h1c-wsp08` so both share one `TILLER_DB`):
run 1 — `project.add`, `surface.chat.open`, `tab.select index=1`,
`surface.chat.compose surfaceId=default-chat text=critic_draft_check_ABC999` — control reply and
screenshot both show the draft in the composer. Run 2 — a **separate process** (confirmed:
`wayland-drive.sh`'s `kill_ours` tears down any prior instance sharing the label/socket before
launching a fresh one, so this is a real cold start, not a kept-alive session), only
`tab.select index=1; shot after`, **no compose call at all** — the very first frame already shows
`critic_draft_check_ABC999` in the composer, restored purely from SQLite by a process that never
received the text itself. This is a value the app would never reach on its own (a stock database
has an empty draft), so it discriminates.

Code checked and matches: `SessionTabState.chat_draft` (`session.rs:79-88`), captured live via
`Chat::draft_text()` at `main.rs:3482` inside `layout()`, restored via `Chat::control_compose` at
both `restore_tabs` call sites (`main.rs:9617-9618`, `9781-9782`), and `surface.chat.compose`
scheduling a save (`main.rs`, commit `03f5c889`). The `save_tabs` active-flag-ordering bug the
builder found and fixed while proving this live is real and has a regression test:
`save_tabs_moving_the_active_flag_to_an_earlier_tab_does_not_violate_the_unique_index`
(`tiller_persistence/tests/persistence_integration.rs`). `cargo test -p tiller_persistence --lib
--tests`: 39 passed, including that test. `editor_caret`/`editor_folds` remain unattempted, as the
report says — the row only requires one of the three legs to close the bar, and `chat_draft` does,
independently reproduced here.
