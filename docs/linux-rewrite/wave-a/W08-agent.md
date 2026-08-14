# Wave A slice W08-agent — 5 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-AGENT-API-01` — ledger line 458, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Launch each of Codex/OpenCode/Pi through the picker and inspect real PTY argv; unit tests already cover all 5 adapters' command/resume/hook/summarizer surfaces against the Swift source.
- **Evidence on record:** half-proven: live ctl system.capabilities (53 methods, none summarizer-related) + new-tab menu confirms 5-adapter catalog order live; per-adapter ID/hook-flag/launch/resume for Codex/OpenCode/Pi/Oh-My-Pi not individually driven. P120, 2026-08-14.

## `F-AGENT-OMP-01` — ledger line 469, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** Adapter code is correct (executable_name is oh-my-pi, confirmed by QUEUE.md). Blocked upstream: installed oh-my-pi@0.2.0 dies on un-transpiled TS before argv is read. Needs an unblocked oh-my-pi install, not a Tiller code change.
- **Evidence on record:** Not re-driven by P116, correctly - oh-my-pi@0.2.0 still dies on un-transpiled TS (bin/oh-my-pi.js:176) before argv is read, upstream of Tiller. No adapter change made. Instrument-blocked, not FAILED.

## `F-AGENT-OMP-02` — ledger line 470, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** Same upstream block as F-AGENT-OMP-01; hook template mapping is already correct in omp.rs.
- **Shared cause:** same upstream oh-my-pi TS-parse failure as F-AGENT-OMP-01
- **Evidence on record:** Not re-driven, same upstream oh-my-pi TS-parse failure as -01 - no hook event can fire before a session exists. Instrument-blocked, not FAILED.

## `F-AGENT-OMP-03` — ledger line 471, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Evidence is stale: summarizer_command landed post-pass-16 (commit 56977f2), matches spec, is unit-tested. Reclassify the command-string clause to proven; live-run exercise is blocked by the same upstream oh-my-pi issue as OMP-01. End-to-end reading needs the shared main.rs auto-naming caller.
- **Shared cause:** auto-naming pipeline has zero callers for any adapter (see F-AGENT-OPENCODE-03)
- **Evidence on record:** two layers: no summarizer-command generator exists anywhere in the port (grep summarizer → only the settings choice, no command), and the launch name `omp` doesn't match the distribution's `oh-my-pi` (OMP-01)

## `F-AGENT-OPENCODE-03` — ledger line 466, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Evidence is stale: summarizer_command landed post-pass-16 (commit 01c1739), matches spec, unit-tested. opencode is installed with no known blocker, so the narrow command-string clause can be closed live today. The end-to-end reading needs one main.rs pass: turn-completion hook -> AutoNamingThrottle -> SummarizerChoice -> adapter summarizer_command -> spawn -> parse -> rename tab.
- **Shared cause:** auto-naming pipeline has zero callers for any adapter; same main.rs wiring closes F-AGENT-OMP-03's end-to-end reading and gives F-AGENT-SESSION-02's transcript readers their first consumer
- **Evidence on record:** no summarizer-command generator exists in the port (grep `run --pure`/summarizer → nothing in tiller_agents/tiller_project; the only summarizer code is the settings SummarizerChoice, no command); opencode itself is now installed (1.18.18) so this is absence, not environment

