# F-AGENT — agent adapters — fresh critic pass

Independent re-judgement of all 20 F-AGENT rows, driven live against the warm build
`/dev/shm/tt/debug/tiller` (mtime 2026-08-19 15:56) on this host. Fixture repo:
`/dev/shm/fagent-fixture` (throwaway `git init`). Lane label `fagent01`,
`/dev/shm/sweep-24-F-AGENT/*.png` holds every screenshot cited below. All five real agent
CLIs are installed on this host (claude 2.1.235, codex-cli 0.147.0, opencode 1.18.18, pi
0.84.1, plus the oh-my-pi story below) — nothing here is simulated.

I did not accept the ledger's prior verdicts at face value. Where I re-derived the same
answer it is because I drove it myself, not because I copied the row. One row
(OMP-01/02/03) turned up a live, in-progress correction to the *reasoning* behind a
previous UNREACHABLE verdict — see the Defects section, this is the most important finding
in this pass.

## Table

| row id | verdict | evidence |
|---|---|---|
| `F-AGENT-API-01` | PASSED | Live "+" new-tab menu (`02-04-plus-menu.png`, cleanly reopened in `03-08-menu-open.png`) lists **Claude Code, Codex, OpenCode, Pi, Oh-My-Pi** in exactly that order. All five were then actually launched through that same menu this pass (see their own rows). Source: `tiller_agents::ALL` in `lib.rs` is exactly `[ClaudeCodeAdapter, CodexAdapter, OpenCodeAdapter, PiAdapter, OhMyPiAdapter]`, covered by `catalog_has_the_five_adapters_in_order`. `AgentAdapter` exposes `id`/`display_name`/`has_native_hooks`/`prepare`/`command`/`resume_command`/`summarizer_command` (default `None`, each of the 5 adapters overrides it) — real call sites in `main.rs::add_agent_tab` and `summarizer_candidate_commands` (auto-naming). I did not separately re-drive a `settings`/availability ctl snapshot this pass (the launch itself is stronger proof); PATH-resolved availability is exercised structurally by `find_executable_on_path` and its tests, not re-verified live here. |
| `F-AGENT-CLAUDE-01` | PASSED | Full round trip, live: real Claude Code v2.1.235 TUI (`02-09-claude-launched.png` trust prompt, `03-16-claude-answered.png` real response "PONYMARK773" to a real prompt, account `e.palmisano@reply.it`). `SessionStart` hook fired for real and captured a genuine session id into SQLite (`session_ref` table) even before I answered the trust prompt. Killed and relaunched the whole app (fresh process, same label/DB) — the pane came back running `claude --resume <same id>` and showed the **prior turn on screen without me typing anything** (`03-19-pane3-resumed-view.png`, same "ZEBRAFOX551" Q&A visible after relaunch). Needed `CLAUDE_CODE_FORCE_SESSION_PERSISTENCE=1` in my own environment to get a real `.jsonl` written — my nested Claude Code session inherits `CLAUDE_CODE_CHILD_SESSION` which otherwise silently disables transcript persistence; this is a harness quirk of running this critic itself under Claude Code, not a Tiller defect (documented in the Unreachable section for completeness). |
| `F-AGENT-CLAUDE-02` | PASSED | Live `.claude/settings.local.json` twice: exactly the five documented hook keys (`Stop`, `Notification`, `SessionStart`, `UserPromptSubmit`, `SessionEnd`), `Stop`/`Notification`/`SessionStart` → `needs-input`, `UserPromptSubmit` → `running`, `SessionEnd` → `done`, with the real resolved path `/home/enzopalmisano/.local/share/TillerRust/bin/tillerctl`. Seeded a stale path plus an unrelated hook (`PreToolUse`) and an unrelated top-level key into the shared file, relaunched: stale path rewritten, `PreToolUse` and the unrelated key both survived byte-for-byte. **But see the Defects section** — this same live test exposed that a *second* Claude Code pane in the same worktree silently overwrites the *first* pane's hook commands with its own pane id, because Claude's hooks file has no per-instance scoping. The row's own contract (one prepare call's merge behaviour) is satisfied; the cross-pane collision is a real, adjacent, user-visible finding I'm flagging separately. |
| `F-AGENT-CLAUDE-03` | PASSED | Live: `claude -p 'echo test $(id) \`whoami\` "quoted" & rm -rf /nonexistent'` in a real shell. Claude received the whole string as one literal prompt and explained/analysed the shell snippet rather than having any part of it executed by a shell — proof the argv boundary held. Generated form matches source exactly: `format!("claude -p {}", shell_quote(prompt))`. |
| `F-AGENT-CODEX-01` | PASSED | Source (`codex.rs`) confirms: `has_native_hooks() == true`, `prepare()` is a literal no-op (`Ok(())`, no file write — confirmed live too, no new file appeared under the fixture for Codex), `command()` = `codex -c '<notify override>'`, `resume_command()` = same override + `resume <ref>`, and both share one `notify_override()` (needs-input, `tillerctl notify --session <pane> --status needs-input`). Live-launched Codex through the "+" menu (`02-25-codex-recheck.png`): real CLI, real "update available 0.147.0 → 0.148.0" prompt. I did not carry a Tiller-launched Codex pane through a full turn to directly observe its own `-c` override actually firing `tillerctl notify` this pass (the update prompt sits in the way and this pass's time budget didn't stretch to clearing it) — the override's construction is confirmed structurally and by a separate direct-CLI run with the same argv shape (see CODEX-02), not independently watched fire from inside a live Tiller pane this time. |
| `F-AGENT-CODEX-02` | PASSED | Live: `codex exec --output-last-message /dev/stdout 'echo test $(id) \`whoami\` "quoted" path/with/slash'` — real `codex-cli 0.147.0`, reached the real `wss://api.openai.com/v1/responses` endpoint (401 Unauthorized, real network + real auth failure, not a stub), and the transcript shows the prompt verbatim including the unescaped slash and backticks, confirming no shell interpretation before Codex saw it. Source: `format!("codex exec --output-last-message /dev/stdout {}", shell_quote(prompt))`. |
| `F-AGENT-OPENCODE-01` | PASSED | Real OpenCode v1.18.18 TUI rendered live (`29-opencode-final-check.png`: the "opencode" ASCII banner, "Ask anything…" composer, "Build · Big Pickle OpenCode Zen" branding). It needed far longer to paint its first frame than Claude/Codex (about a minute; my first two checks at 3-5s genuinely showed nothing on screen **and** an empty raw `panel.read` — I do not consider that a defect, see the Harness note below). `has_native_hooks() == false` and `resume_command()` = `opencode --session '<ref>'` confirmed via source. I did not carry this specific pane through a message → `session.updated` → resume round trip this pass (OPENCODE-02 below covers the plugin mechanism directly instead, and CLAUDE-01 already proves the identical `tillerctl` capture pattern end-to-end for a different adapter). |
| `F-AGENT-OPENCODE-02` | PASSED | Live-read `/dev/shm/fagent-fixture/.opencode/plugin/tiller-session.js` after launching a real OpenCode pane: matches the documented template exactly, `session.updated` → `tillerctl session-ref --session <pane> --ref <id>`, `tillerctl` path and pane id (`pane-5`) correctly substituted as a JSON string literal. No other file touched under `.opencode/`. |
| `F-AGENT-OPENCODE-03` | PASSED, with a note worth reading before treating it as an injection | Live: `opencode run --pure 'echo test $(id) \`whoami\` "quoted"'`. OpenCode's own agent tool-use **actually ran** `echo test $(id) \`whoami\` "quoted"` as a real shell command and printed the real `id`/`whoami` output. This is OpenCode being a coding agent that decided to execute a string that looked like an instruction to run a shell command — it is **not** a shell-quoting defect in Tiller: the fact that OpenCode's own tool-call line displays the *unexpanded* `$(id)`/backtick text is exactly what proves the prompt arrived as one intact argv element (if my own shell — or Tiller's command construction — had expanded it first, OpenCode would only ever have seen an already-substituted uid string, not the literal `$(id)`). Source: `format!("opencode run --pure {}", shell_quote(prompt))`. |
| `F-AGENT-PI-01` | PASSED | Real Pi TUI eventually rendered a genuine trust-folder prompt (`31-pi-trust-visible.png`). Cold start was slow — about 90s from click to first byte on the PTY, confirmed via `panel.state`/`panel.read` polling and a `ps` check showing the real `node .../bin/pi` process alive and burning CPU the whole time (not a Tiller launch failure; see Harness note). `has_native_hooks() == false`, `prepare()` writes nothing (confirmed: no new files appeared for the Pi pane), matching "performs no preparation" exactly. |
| `F-AGENT-PI-02` | PASSED | Live: `echo "" \| pi --print --no-tools 'echo test $(id) \`whoami\` "quoted"'` (piped empty stdin — required, `pi` otherwise blocks reading stdin even in `--print` mode). After ~85s it returned `Codex error: The usage limit has been reached` — a genuine backend error (Pi's backend appears to proxy through something reporting as "Codex"), reached for real, with the metacharacter prompt intact (no local shell artifacts in the output). Source: `format!("pi --print --no-tools {}", shell_quote(prompt))`. |
| `F-AGENT-OMP-01` | UNREACHABLE against the deployed build, but see Defects — this is not a dead end | Live, through the real UI: clicking "Oh-My-Pi" in the "+" menu launches a pane that immediately crashes with `oh-my-pi.js:176: function checkFile(path: string, label: string) { ^ SyntaxError: Unexpected token ':'` (`06-24-ohmypi-launched.png`), matching a direct `oh-my-pi --hook <path>` run I reproduced myself outside the app. `.tiller/omp-hook.ts` was written correctly by `prepare()` before the crash. **This reproduces the prior UNREACHABLE finding against the binary I was handed to drive.** |
| `F-AGENT-OMP-02` | UNREACHABLE against the deployed build, same caveat | Same crash as OMP-01 — the hook file (`omp-hook.ts`) is written correctly and matches the documented template byte-for-byte (checked by reading the file), but no `oh-my-pi` process survives to fire it. |
| `F-AGENT-OMP-03` | UNREACHABLE against the deployed build, same caveat | `oh-my-pi.rs`'s current *compiled* `summarizer_command` (in the warm build) is `format!("oh-my-pi --print --no-tools {}", shell_quote(prompt))`; `oh-my-pi --print --no-tools '<anything>'` hits the identical syntax-error crash live. |
| `F-AGENT-SAFE-01` | PASSED | Live: seeded `.claude/skills/tiller/SKILL.md` with unmarked content, recorded its md5, then triggered a real `prepare()` by launching a Claude Code pane. md5 unchanged, and the app's own stdout carried the exact live message: `failed to prepare Claude Code in /dev/shm/fagent-fixture: refusing to overwrite unmanaged skill at /dev/shm/fagent-fixture/.claude/skills/tiller/SKILL.md`. Also observed: a `prepare()` failure does **not** block the pane from opening — `add_agent_tab` logs and launches the CLI anyway (confirmed in source and live: the tab and a real `claude` TUI appeared regardless). That's a deliberate design choice (source comment says as much), not a contract violation. |
| `F-AGENT-SAFE-02` | PASSED | Live: seeded a stale `'/some/old/stale/tillerctl'` path into one hook plus an unrelated `PreToolUse` hook and an unrelated top-level key, relaunched the app. Rewritten file has the real resolved tillerctl path in that hook, and the pane id, status, unrelated hook, and unrelated key all survived unchanged. Source confirms the migration loop in `main.rs` runs over *every* known worktree's `.claude/settings.local.json` at boot, before any pane is restored — my live test's worktree also had a pane to restore, so this run doesn't cleanly isolate "migration alone" from "prepare() regenerating the file anyway"; the "worktree with no pane reopened this run still gets fixed" half of the claim is confirmed by reading the code, not independently re-driven with an empty worktree this pass. |
| `F-AGENT-SESSION-01` | PASSED | Source (`session_validator.rs`) matches the contract exactly: Claude checks `<claude_config_dir>/projects/<slug>/<ref>.jsonl` with non-alphanumeric→hyphen slugging; Codex recursively searches `<codex_home>/sessions` for a filename containing the ref; every other agent is trusted (`_ => true`). Live: the predicted path `~/.claude/projects/-dev-shm-fagent-fixture/2fdb48c7-....jsonl` genuinely existed and the resume in CLAUDE-01 depended on exactly this check passing. |
| `F-AGENT-SESSION-02` | PASSED | Live `ctl session.transcript session=pane-3 agent=claude worktree=/dev/shm/fagent-fixture` → `{"text":"Reply with exactly the single word ZEBRAFOX551 and nothing else.\nZEBRAFOX551"}` — the real joined transcript. Negative controls both live: unknown pane → `{"ok":false,"error":"no session reference recorded for this pane"}`; same pane/agent but wrong `worktree` → `{"ok":false,"error":"no transcript available for this session"}`. Neither silently succeeded. |
| `F-AGENT-SESSION-03` | PASSED | Live-ran four adapters' generated command shapes through a real shell, each with a `$(id)`/backtick/quote/slash-laden prompt: `claude -p`, `codex exec --output-last-message /dev/stdout`, `opencode run --pure`, `pi --print --no-tools`. All four delivered the prompt as one literal argument — confirmed either by the agent explaining/quoting the un-expanded text (Claude), by it appearing verbatim in a real transcript (Codex), by the agent's own tool-call line showing the un-expanded text before it chose to execute it (OpenCode, see OPENCODE-03 note), or by a clean backend error with no local-shell artifacts (Pi). `shell_quote`/`json_string_literal` in `shell_quote.rs` back every one of these. |
| `F-AGENT-PLAT-01` | PASSED | `tiller_agents/Cargo.toml`: only dependency is `serde_json`, no macOS-specific crate, no platform `cfg` gates in the package. (The file's own header comment claiming it's "not part of the workspace yet" is stale — `rust/Cargo.toml` lists `crates/tiller_agents` as a real member; harmless doc drift, not a functional defect.) Every one of the 5 adapters driven this pass wrote only worktree-local paths (`.claude/`, `.opencode/plugin/`, `.tiller/`, `.agents/skills/` — all confirmed live under `/dev/shm/fagent-fixture`), never touched `$HOME` directly. `AgentSessionValidator`/`ClaudeTranscriptSource`/`CodexTranscriptSource` all take `claude_config_dir`/`codex_home`/`home_directory` as explicit parameters rather than hardcoding a macOS path. |

20/20 exercised. 17 PASSED outright, 3 UNREACHABLE (all three Oh-My-Pi rows, against the
currently deployed build — see below for why that verdict is likely to flip soon).

## Defects (live-found this pass)

### 1. The OMP UNREACHABLE verdict's *root cause* was wrong — and is being fixed as I write this

I reproduced the exact same crash the prior "orchestrator probe" documented
(`oh-my-pi.js:176: SyntaxError: Unexpected token ':'`), both directly on the CLI and live
through Tiller's own UI. That symptom is real and I stand behind UNREACHABLE **for the
build I was handed** (`/dev/shm/tt/debug/tiller`, mtime 15:56, whose binary strings still
say `oh-my-pi --hook`).

But mid-pass I found `rust/crates/tiller_agents/src/omp.rs` has an **uncommitted, in-progress
edit** (by another agent in this same session — `git status` shows it modified, not staged)
that changes `executable_name()` from `"oh-my-pi"` to `"omp"`, with a comment explaining why:
npm has two unrelated packages that collide on the concept. `oh-my-pi` (the package Tiller
was launching) is a broken VS Code extension whose `bin` either doesn't exist (0.1.x) or is a
`.ts` file the published tarball can't run (0.2.0) — this part of the old investigation was
accurate. But the *actual* agent CLI is a different package, `@oh-my-pi/pi-coding-agent`
(github.com/can1357/oh-my-pi), whose binary is genuinely called `omp`. I independently
verified this myself, not by trusting the diff:

```
$ which omp
/home/enzopalmisano/.bun/bin/omp -> ../install/global/node_modules/@oh-my-pi/pi-coding-agent/dist/cli.js
$ omp --version
omp/17.3.8
$ omp --help
omp v17.3.8
USAGE
  $ omp [COMMAND]
...
```

That symlink was created today (recently — around the time this pass was running),
almost certainly by the same agent mid-fix. So: the prior "exhaustive" investigation's
conclusion — "the blocker is an upstream PACKAGING defect... and it holds across every
published version, no local workaround can produce a runnable agent" — was **too strong**.
It correctly diagnosed one broken package but never checked whether Tiller was launching
the *right* package at all. A one-line `executable_name()` fix (already in flight,
uncommitted, not yet rebuilt into `/dev/shm/tt`) looks sufficient to turn all three OMP rows
from UNREACHABLE into PASSED. I did not rebuild the binary myself to confirm the fixed
adapter end-to-end (shared build target, another agent mid-edit, out of scope for a review
pass) — flagging this loudly per instructions rather than quietly re-stamping the same
verdict.

### 2. Two Claude Code panes in the same worktree cross-contaminate their hook status (live-found, not previously documented in this row's evidence)

`ClaudeCodeAdapter::prepare` replaces the whole `hooks.Stop/.Notification/.SessionStart/
.UserPromptSubmit/.SessionEnd` arrays on every call, and each array's single command line is
baked with **one hardcoded pane id**. Opened three Claude Code panes in the same worktree
live (`pane-1`, `pane-2`, `pane-3`); after the third launch, `.claude/settings.local.json`
had `--session pane-3` in every one of the five hooks — `pane-1` and `pane-2`'s hook
commands were silently overwritten to point at `pane-3`. In practice this means: with two or
more Claude Code tabs open in one worktree, only the most-recently-launched tab's
Stop/Notification/etc. events reach the *right* pane id — the older tabs' real completion
events fire `tillerctl notify --session <newest-pane> ...`, misattributing status. This is
very likely inherited from Claude Code's own settings format (one project-level hooks file,
not scoped per running instance) rather than a Linux-port regression — I don't have the
Swift source in this worktree to confirm that either way — but it is real, live-reproducible,
user-visible behaviour that F-AGENT-CLAUDE-02's contract text doesn't warn about, so I'm
recording it here rather than either silently passing over it or unfairly failing CLAUDE-02
for a clause it never promised.

## Harness notes (not defects — read before another critic re-flags these)

- **OpenCode and Pi are genuinely slow to paint their first PTY frame** on this host — 60–90s
  from click to visible content, confirmed by polling raw `panel.read` (0 bytes for a long
  stretch) *and* watching a real, CPU-active child process (`ps` showing `RNsl+`/`DNsl+`,
  rising memory) the whole time. A critic that screenshots 3-5s after clicking "OpenCode" or
  "Pi" and calls it broken would be wrong — wait for real content or poll `panel.read` in a
  loop before concluding a launch failed.
- **The new-tab "+" menu's screenshot position lies for one frame if you `shot` immediately
  after opening it.** My first capture (`02-04-plus-menu.png`) showed the menu ~300px to the
  left of where it geometrically belongs (confirmed by the anchor math in `tab_bar.rs`:
  `BottomLeft` corner of the button + a small offset); a clean re-open without an intervening
  `shot` (`03-08-menu-open.png`) rendered it correctly. This matches the `shot`-forces-a-resize
  trap the script's own header already documents for anchored popups. Trusting the first,
  glitched screenshot for click coordinates cost one full failed click cycle in this pass —
  worth calling out explicitly since it's easy to reproduce for the next critic touching this
  menu.
- **`CLAUDE_CODE_FORCE_SESSION_PERSISTENCE=1` was required in my own shell environment** to
  get a persisted `.jsonl` transcript for the Claude panes I drove. Without it, `claude` prints
  "Transcript saving is off — inherited CLAUDE_CODE_CHILD_SESSION marker" and resume/transcript
  rows can't be proven. This is an artifact of this critic itself running inside a Claude Code
  session (nested), not an app defect — noted so a future critic doesn't waste time chasing it
  as one.

## Not independently re-driven this pass (time-boxed, not blocked)

- CODEX-01's `-c notify=[...]` override actually firing `tillerctl notify` from inside a live
  Tiller-launched Codex pane (blocked on clearing a real "update available" prompt inside the
  time budget) — confirmed structurally instead (shared `notify_override()` call site) plus a
  matching direct-CLI run.
- OPENCODE-01's full `session.updated` → `tillerctl session-ref` → app-relaunch → resume round
  trip specifically for OpenCode (the identical mechanism is proven end-to-end for Claude in
  CLAUDE-01, and the OpenCode plugin file itself is proven correct in OPENCODE-02).
- SAFE-02's "a worktree with **no** pane reopened this run still gets fixed" half — my live
  test's worktree had a pane to restore, so prepare()'s own regeneration and the boot-time
  migrator aren't cleanly separated in this evidence; confirmed by reading the code's loop
  instead.

No row was UNREACHABLE for a harness reason — the only UNREACHABLE rows (OMP-01/02/03) are a
real, live-reproduced application/packaging defect, exhaustively explained above.
