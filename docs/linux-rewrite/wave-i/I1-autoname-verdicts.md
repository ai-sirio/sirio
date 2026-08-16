# I1-autoname verdicts

Critic pass, independent of the I1-autoname builder. Read `I1-autoname-report.md` as a route, not a
conclusion; re-derived from the code at HEAD (commit `ae20a3a1`) plus live `wayland-drive.sh`
exercise on the exact default route the row names.

## F-CORE-DOM-07 — PASSED

**Code match, byte-exact against the Swift reference.** `git show ae20a3a1` adds
`summarizer_command` overrides to `ClaudeCodeAdapter`, `CodexAdapter`, `PiAdapter`
(`rust/crates/tiller_agents/src/{claude,codex,pi}.rs`). Diffed each against the reference in
`/home/enzopalmisano/Scrivania/Progetti/tiller`:
`claude -p <shell_quote(prompt)>` == `ClaudeCodeAdapter.swift:99`;
`codex exec --output-last-message /dev/stdout <shell_quote(prompt)>` == `CodexAdapter.swift:32`;
`pi --print --no-tools <shell_quote(prompt)>` == `PiAdapter.swift:28`. All use the existing shared
`shell_quote` (single-quote shell escaping) already used by these adapters' other command builders
— no `-c key=value` TOML override involved here, so `json_string_literal`/`.withoutEscapingSlashes`
correctly does not apply (that's Codex's separate `notify=[...]` override). The regression test that
documented the bug, `unported_summarizers_answer_none_rather_than_guessing`, is gone (grep confirms
zero hits); replaced by `claude_codex_pi_summarizer_commands_match_the_swift_reference`, which
asserts the exact argv for all three, and `summarizer_candidates_prefer_the_selected_agent_then_the_tab_agent`
now directly covers `summarizer_candidate_commands("claude", None, "prompt")` — the exact
default-tab/default-settings shape the row names — asserting it is non-empty.

**Both named tests re-run independently, per-crate, both green:**
`cargo test -p tiller_agents claude_codex_pi_summarizer_commands_match_the_swift_reference` (1
passed) and `cargo test -p tiller summarizer_candidates_prefer_the_selected_agent_then_the_tab_agent`
(1 passed).

**Live, single-invocation `wayland-drive.sh` exercise on the real default route**
(`TILLER_WL_LABEL=i1auto1`), discriminating, not a token call site. Opened the repo as a project;
opened Settings -> General and confirmed Summarizer agent already reads "Claude Code" (default,
untouched) and clicked the "Auto-rename tabs and agents" toggle on (it defaults off, matching the
Swift reference — not itself a defect, and the row's own route presupposes the feature is enabled).
Selected the default Chat tab (`tab.select index=1`, tab id `default-chat`, `agent_id: None` —
confirmed via `session.rs:335`'s `default_in`), sent a real user turn over
`surface.chat.send`, forced a `running`->`done` transition via `tillerctl notify` (Layer A, the same
mechanism a real agent hook uses), and captured after a 10s settle. The tab title in **both the tab
strip and the sidebar** changed from the default "Chat" to "Auto-naming" — the actual assistant
reply visible in the transcript literally quotes back the truncated user text ("... your message
just came through as \"Auto-naming\" with no actual request attached...") proving a real `claude -p`
process ran against the real transcript and produced this exact 2-word title, not a hardcoded or
guessed string. Before this fix this route produced an empty `commands` list and no title, ever, per
the wave-H critic's own deterministic trace. Screenshots:
`/tmp/i1auto2/04-chat-tab.png` (baseline, title "Chat"), `/tmp/i1auto2/06-after-notify-pane0.png`
(title changed to "Auto-naming" in strip + sidebar).

Additionally spot-checked the other two ported adapters' real binaries directly in a shell (not
through the app): `codex exec --output-last-message /dev/stdout '...'` parses the flags cleanly
(only errors on an unrelated git-trust guard, not on the flags), and `pi --print --no-tools '...'`
accepts the flags with no "unrecognized option" error — both consistent with the argv the port now
builds, not hallucinated flags.

**Verdict: PASSED.** The fix is on the path a user actually takes (default chat tab, default
Summarizer=Claude), matches the Swift reference byte-for-byte for all three previously-unported
adapters, the regression test that encoded the bug is gone and replaced with one that encodes the
fix, and a live drive on the named route produced a real, transcript-derived auto-generated title
where none was possible before.
