# I1-autoname report

## F-CORE-DOM-07 — auto-naming title never produced on the default chat tab

**Action:** fixed.

**Diagnosis confirmed at HEAD.** `AutoNamingThrottle` was already wired
(throttle, transcript signal, `apply_auto_title`), but `ClaudeCodeAdapter`,
`CodexAdapter`, and `PiAdapter` all fell through `AgentAdapter::summarizer_command`'s
trait default (`None`). `summarizer_candidate_commands("claude", None, prompt)`
— the exact shape a default chat tab (`agent_id: None`) with default settings
(`summarizer_agent: Claude`) produces — returned an empty candidate list, so
`run_summarizer_command` was never called and no title was ever generated.
The repo's own test, `unported_summarizers_answer_none_rather_than_guessing`,
documented this as intentional rather than as the bug it was.

**Fix.** Ported `summarizer_command` for all three adapters from the Swift
reference, byte-faithful to the real CLI invocations:

- `ClaudeCodeAdapter::summarizer_command` → `claude -p <shell_quote(prompt)>`
  (`ClaudeCodeAdapter.swift:99`)
- `CodexAdapter::summarizer_command` → `codex exec --output-last-message /dev/stdout <shell_quote(prompt)>`
  (`CodexAdapter.swift:32`)
- `PiAdapter::summarizer_command` → `pi --print --no-tools <shell_quote(prompt)>`
  (`PiAdapter.swift:28`)

All three use the shared `shell_quote` helper (single-quote shell quoting) —
none of them build a `-c key=value` TOML override, so `json_string_literal`
doesn't apply here (that's Codex's `notify=[...]` override only, untouched).
All five adapters (`claude`, `codex`, `pi`, `opencode`, `oh-my-pi`) now
implement a real `summarizer_command`.

**Tests.** Replaced `unported_summarizers_answer_none_rather_than_guessing`
(`rust/crates/tiller_agents/src/lib.rs`) — which asserted the bug — with
`claude_codex_pi_summarizer_commands_match_the_swift_reference`, asserting
the exact argv for all three. Updated
`summarizer_candidates_prefer_the_selected_agent_then_the_tab_agent`
(`rust/crates/tiller/src/main.rs`) to cover the previously-empty default-tab
case (`summarizer_candidate_commands("claude", None, "prompt")` now returns
`["claude -p 'prompt'"]`) plus the newly-non-empty two-candidate cases.

**Build:** `cargo build -p tiller_agents` and `cargo build -p tiller` both
green. `cargo test -p tiller_agents` (32 unit + 14 integration, all pass).
`cargo test -p tiller summarizer` (4 tests, all pass) run per-crate per house
rules, not `--workspace`.

**Commit:** `ae20a3a1` — `fix(I1-autoname): port summarizer_command for Claude Code, Codex, Pi`
— `rust/crates/tiller_agents/src/{claude,codex,pi,lib}.rs`,
`rust/crates/tiller/src/main.rs`.

**howToExercise:** Open Settings and confirm Summarizer is left at its
default (Claude). Open a project's default chat tab (no explicit agent
selected — `agent_id: None`), drive a short exchange so the transcript has
content, and wait past the auto-naming throttle window (or force a
transcript-changed signal). The tab title should update from its default to
an auto-generated 2-5 word summary — previously it never did on this route.
Unit coverage: `cargo test -p tiller_agents claude_codex_pi_summarizer_commands_match_the_swift_reference`
and `cargo test -p tiller summarizer_candidates_prefer_the_selected_agent_then_the_tab_agent`.

No foreign files needed — all touched paths are owned by I1-autoname per
`docs/linux-rewrite/wave-i/manifest.json`.
