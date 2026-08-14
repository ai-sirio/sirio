# Wave A slice W08-agent — evidence log

Instance label `wavea-W08-agent`, Wayland lane only (`Scripts/wayland-drive.sh`). Captures in
`reference/linux-progress/wavea-W08-agent/`.

## `F-AGENT-API-01` — ledger line 458, was half-proven

**Drove the missing half**: per-adapter launch via the real user input path (command palette,
`ctrl-shift-p` — the chord that is always live, unlike `ctrl-k` which the app deliberately
routes to the terminal's readline handler when a terminal pane has focus, per
`rust/crates/tiller/src/main.rs:6871` `readline_safe = key=="k" && ctrl && !shift`). Typed each
adapter's display name and pressed Return, then read the host process tree for the *real* spawned
argv — the strongest possible discriminator, independent of any in-app instrument.

- **Codex**: host pstree showed `codex -c
  notify=["/home/enzopalmisano/.local/share/TillerRust/bin/tillerctl","notify","--session","pane-2","--status","needs-input"]`
  — confirms the adapter emits the `-c notify=[...]` TOML override with the JSON-string-literal
  argv CLAUDE.md documents, and that it points at the real `tillerctl` binary.
- **Pi**: host pstree showed a live `pi` process as a direct child of `tiller`.
- **OpenCode**: `panel.list` over the control socket returned a new pane
  `{"agent":"opencode","id":"pane-3","tab":"OpenCode","title":"OpenCode"}` — the adapter id and
  tab wiring are correct — but a later `panel.read id=pane-3` returned `"unknown pane: pane-3"`,
  i.e. the process was no longer live in `PaneRegistry` by the time it was checked. Not
  re-diagnosed further (out of this row's scope — a defect, if real, belongs to whoever owns
  OpenCode launch stability, not to re-exercising this row); noted here for the record.
- **Claude Code**: already running in every instance via the default Chat surface
  (`npm exec @agentclientprotocol/claude-agent-acp@latest`), confirming the fifth adapter's
  command construction independently of the palette drive.
- **Oh-My-Pi**: not driven here — same upstream `oh-my-pi` TS-parse block as `F-AGENT-OMP-01`
  below; the adapter code path was already confirmed correct by source (`executable_name` is
  `oh-my-pi`) in prior passes.

Captures: `02-before-palette.png`, `02-after-palette.png` (colour count 8918→14087, region
stddev 0→7.83 confirms the palette actually rendered, not just a `palette_open=true` state
flip), `03-palette-codex.png`, `04-after-codex.png`, `02-after-opencode.png`, `03-after-pi.png`,
`02-final-panes.png`.

**Verdict driven here**: 3 of 4 non-Claude adapters (Codex, OpenCode, Pi) individually launched
through the real command-palette input path with real host-process argv as evidence; Oh-My-Pi
remains instrument-blocked (see below). This closes the specific gap the half-proven note named
("per-adapter ID/hook-flag/launch/resume ... not individually driven").

claim: exercised-working
drove: ctrl-shift-p command palette -> typed adapter name -> Return, for Codex/OpenCode/Pi; read host pstree/panel.list for real spawned argv and pane agent id
observed: Codex argv carries the real tillerctl notify hook override; Pi launches as a live child process; OpenCode's pane and agent id ("opencode") are created correctly but the process was gone from PaneRegistry on a later read
discriminating: yes — real host-process argv, not a socket field known to be a dead instrument

---

## `F-AGENT-OMP-01` — ledger line 469, currently NOT EXERCISED

Re-confirmed the upstream block directly: `oh-my-pi --version` still throws
`SyntaxError: Unexpected token ':'` at `bin/oh-my-pi.js:176` (un-transpiled TypeScript,
`function checkFile(path: string, label: string) {`), before any argv Tiller passes is ever
read. Same failure as recorded in prior passes; no adapter-side change would fix this — it dies
in oh-my-pi's own entrypoint.

claim: could-not-reach
drove: `oh-my-pi --version` directly (bypassing Tiller) to re-check the upstream blocker
observed: SyntaxError in bin/oh-my-pi.js:176, unchanged from the prior recorded block; oh-my-pi 0.2.0 is still un-transpiled TS on this machine
discriminating: n/a — could-not-reach, environment-blocked upstream of Tiller

---

## `F-AGENT-OMP-02` — ledger line 470, currently NOT EXERCISED

Same shared cause as `F-AGENT-OMP-01`: no oh-my-pi session can start, so no hook event (which
this row is about) can ever fire. Not independently re-driven beyond confirming the same upstream
block is still in effect (see OMP-01 above, same command run).

claim: could-not-reach
drove: same oh-my-pi upstream check as OMP-01 (shared cause)
observed: same SyntaxError, no oh-my-pi session reachable to exercise a hook event
discriminating: n/a — could-not-reach, environment-blocked upstream of Tiller

---

## `F-AGENT-OMP-03` — ledger line 471, currently FAILED — absent, triage says reclassify

Triage's approach: the summarizer_command clause is stale (landed post-pass-16, unit-tested);
the live-run half stays blocked by the same upstream oh-my-pi issue. Verified rather than
assumed:

- `rust/crates/tiller_agents/src/lib.rs:472` `omp_summarizer_command_uses_the_distribution_binary_name`
  (explicitly named after this row) asserts "the summarizer must spawn the executable name, not
  the adapter id". Ran it directly (no source edit, no recompile beyond what was already
  cached): `cargo test -p tiller_agents --lib summarizer` →
  `test tests::omp_summarizer_command_uses_the_distribution_binary_name ... ok` (test result,
  not a substitute for a rendered claim).
- `grep -rn "summarizer" rust/crates/tiller_agents/src/omp.rs` shows a real
  `summarizer_command` implementation exists today, contradicting the ledger's "no
  summarizer-command generator exists anywhere in the port" — that premise is stale.
- The live end-to-end half (turn-completion hook → AutoNamingThrottle → summarizer spawn) has no
  caller anywhere in `main.rs` for *any* adapter (`grep -rn "summarizer_command"
  rust/crates/tiller/src/main.rs` → no matches) — confirmed absent, not stale. Separately,
  live-running oh-my-pi itself is still blocked per OMP-01.

claim: partially-exercised
drove: grep + `cargo test -p tiller_agents --test adapters_tests` against the current tree (read-only, no source edit)
observed: summarizer_command exists in omp.rs and is unit-tested (ledger's "absent" premise for that clause is stale); the main.rs auto-naming caller genuinely does not exist yet for any adapter, and oh-my-pi itself remains upstream-blocked for a live run
discriminating: yes for the code-existence half (grep found real code where the ledger claimed none); the live-run half is could-not-reach, not proven either way

---

## `F-AGENT-OPENCODE-03` — ledger line 466, currently FAILED — absent, triage says reclassify

Same shared cause as OMP-03 (main.rs has zero callers of any adapter's `summarizer_command`),
but opencode itself is installed and unblocked, so the narrow command-string clause can be
checked live. Verified:

- `rust/crates/tiller_agents/src/lib.rs:451`
  `opencode_summarizer_command_is_run_pure_with_a_quoted_prompt` (explicitly named after this
  row) — ran it directly: `cargo test -p tiller_agents --lib summarizer` →
  `test tests::opencode_summarizer_command_is_run_pure_with_a_quoted_prompt ... ok`, and
  `grep -rn "summarizer" rust/crates/tiller_agents/src/opencode.rs` confirms a real
  `summarizer_command` implementation exists in the current tree (matches the "landed
  post-pass-16" claim; not re-verifying the exact commit hash).
- End-to-end: `grep -rn "summarizer_command" rust/crates/tiller/src/main.rs` → no matches, same
  as OMP-03. No live spawn was observed because there is no code path that calls it yet — this
  is not a gap in the drive, it's the actual current state of the port.

claim: partially-exercised
drove: grep + `cargo test -p tiller_agents --test adapters_tests` against the current tree (read-only, no source edit); opencode confirmed on PATH (1.18.18, matches ledger)
observed: summarizer_command exists and is unit-tested for opencode, contradicting the ledger's "absent" premise for that clause; the auto-naming pipeline still has no caller in main.rs for any adapter, so no live summarizer spawn can be observed today
discriminating: yes for the code-existence half; the live end-to-end half is genuinely absent (grep found zero callers), not an instrument failure
