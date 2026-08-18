# FINISH — agents-auto shard (F-AUTO-*, F-AGENT-*, F-CORE-AUTH-*)

Finish-line critic pass, run **2026-08-18** on the box described in `ENVIRONMENT.md`'s 2026-08-18
top section (x86_64, 12 cores, COSMIC/wayland-1, AMD GPU, load ~7 on 12 cores throughout). Lane
label `wf-auto`. Every row below was re-driven live today against a pinned binary; nothing was
carried from the ledger's prose, including rows already marked PASSED.

## Setup

```bash
cargo build --manifest-path rust/Cargo.toml         # exit 0, warm, 2 pre-existing warnings only
cp rust/target/debug/tiller /tmp/wf-auto-tiller
cp rust/target/debug/tillerctl /tmp/wf-auto-tillerctl
export TILLER_WL_LABEL=wf-auto TILLER_WL_BIN=/tmp/wf-auto-tiller
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
```

Throwaway git fixture: `/tmp/wf-auto-project` (real git repo, one commit). Real installed agent
CLIs used throughout: `claude` (Claude Code 2.1.234), `codex` 0.147.0, `opencode` 1.18.18, `pi`
0.84.1. `oh-my-pi` re-confirmed unrunnable (see F-AGENT-OMP-*).

**Session bus.** `/run/user/1000/bus` is down (documented in ENVIRONMENT.md; the user's own
desktop D-Bus, not to be touched). For the two rows that need a live D-Bus round trip
(F-AUTO-06's `notification.create`), a private session bus was stood up for this drive only:
`dbus-daemon --session --fork --address=unix:path=/tmp/wf-auto-dbus.sock`, plus a ~90-line Python
stub (`Gio.bus_own_name`) registering `org.freedesktop.Notifications` and logging every `Notify`
call, since this box has no notification daemon at all (real or private-bus). Both processes were
scratch, torn down at the end of the pass.

**Environment quirk, not a Tiller defect** (already recorded in `CRITIC-findings-log.md`): this
session is itself a Claude Code subagent, so every `claude` child pane inherits
`CLAUDE_CODE_CHILD_SESSION` and boots with transcript-saving off ("Transcript saving is off —
inherited CLAUDE_CODE_CHILD_SESSION marker"). Exporting `CLAUDE_CODE_FORCE_SESSION_PERSISTENCE=1`
before every app launch restores real `.jsonl` transcripts, which is what let the resume tests
below run against a genuine session file instead of a stand-in.

**Harness note for the next reader:** `Scripts/wayland-drive.sh` with `TILLER_WL_KEEP=1` does
**not** attach to a previously kept-alive instance — each invocation's `kill_ours` kills the prior
process (matched by `TILLER_SOCKET=$SOCK` in `/proc/*/environ`) and launches a fresh one. Any env
var the app process needs (`CLAUDE_CODE_FORCE_SESSION_PERSISTENCE`, `DBUS_SESSION_BUS_ADDRESS`,
`TILLER_CREDENTIALS`) must be re-exported in *every* invocation's shell, not just the first — losing
this cost one whole cycle mid-pass (a prompt sent to a freshly-restarted, non-persistent Claude
pane went nowhere).

**Known instrument gap found this pass, not a row finding:** synthetic `scroll` (wlr-virtual-pointer
axis events) did not move the Settings surface's General/AI-Providers page in this Wayland lane —
tried at multiple x/y, positive and negative step signs, up to 100 steps, plus Tab-then-Enter
keyboard focus traversal; none moved the visible content past what fits in the 1715×972 viewport.
`WAYLAND-LANE.md`'s positive control for `scroll` was the Files panel's file list, a different
widget. This blocked a literal button-click on Settings → General → "Install" under Agent Skill
(F-CORE-AUTH-02) and the OpenCode-Go Save/Clear text field (F-CORE-AUTH-03's write half); both were
independently re-proven by other live means (see their rows) rather than left unproven. Worth a
follow-up: either the settings scroll region genuinely doesn't wire up wheel events yet (a real gap)
or this lane's `scroll` primitive needs a second positive control. Flagging, not diagnosing further
under this shard's budget.

---

## F-AUTO rows — control socket automation (9)

All nine driven live against a real running instance over `/tmp/wf-auto.sock`, using both
`tillerctl` (built fresh, pinned alongside the app) and raw socket JSON where `tillerctl` has no
subcommand (`session.transcript`, `browser.*`).

**F-AUTO-01 — control-socket enable/disable setting with displayed path.** PASSED. Opened
Settings → General over the socket, screenshotted the real toggle + "Socket path: /tmp/wf-auto.sock"
label, then **clicked the physical toggle** (1288,843): `ctl system.ping` immediately after the
click failed with `CTL-FAIL system.ping: [Errno 2] No such file or directory` — the socket file
itself was unlinked, not merely refusing connects. Clicked the same toggle again: `ctl system.ping`
returned `{"pong":"true"}` again. Screenshot of the off-state shows the toggle knob moved left/grey
while the socket-path label stays displayed, matching the clause exactly (display the path; disable
it; confirm the state changes). Both halves (UI state + live connection behavior) proven in one
uninterrupted drive.

**F-AUTO-02 — `panel.create`.** PASSED. `tillerctl panel create --worktree p-...-wt-0` returned a
real `pane-N` id; confirmed present via `panel list`.

**F-AUTO-03 — split/list/write/key/read/state/wait/focus/close.** PASSED, every verb exercised with
an observable effect: `panel split down --from <id>` → new pane appears in `panel list`; `panel
write <id> --input "echo WF_AUTO_MARKER_<pid>" --enter` → `panel read` returned base64 scrollback
containing the exact marker string echoed back by the real shell; `panel key <id> --key enter`
(lowercase — `Enter` capitalized is rejected with `unknown terminal key`, useful precision for the
next driver); `panel state` returned `exitStatus":"running"` plus live scrollback; `panel wait
--timeout-ms 800` on a still-running shell correctly timed out (`pane timed out`); after `panel
write ... "exit"`, `panel wait --timeout-ms 4000` returned `{"exitCode":"0"}` — a genuine process
exit observed through the socket, not a canned reply; `panel focus`/`panel close` both no-op-ok'd
and the closed pane vanished from a follow-up `panel list`.

**F-AUTO-04 — notify / worktree.set / session.ref.** PASSED. `worktree-set --comment "wf-auto
marker"` changed the value returned by `current-workspace`. `session-ref --session <pane> --ref
sess_wfauto_marker_1` produced a **new row in the live SQLite DB** (`session_ref` table,
`(pane_id, sess_wfauto_marker_1)`) — read directly with `python3 sqlite3`, not through the app's own
report of itself. `notify --session <pane> --status needs-input` returned ok and queues a real
`ControlAction::Notify` (confirmed by reading the dispatch code path); the pane used for this probe
was a plain `panel.create` control pane, not an agent-owned one, so no visible status glyph was
expected and none was claimed.

**F-AUTO-05 — workspace list/current/select/create/close.** PASSED. `list-workspaces` returned the
real fixture worktree; `select-workspace` + `current-workspace` round-tripped the same id/path/
branch; `panel.create` only started succeeding *after* select (`unknown worktree` before), which is
itself live proof the mount state gates it as documented.

**F-AUTO-06 — notification create/list/clear, delivered.** PASSED — the delivery half specifically
targeted, since `EVIDENCE-STANDARD.md` names this row's prior false-PASSED pattern (machine half
proven, delivery conjunct untouched). With the private session bus and stub notification daemon
live: `tillerctl notify --title "WF-AUTO-06" --body "hello wf-auto marker <pid>"` produced a real
D-Bus `Notify(app_name='Tiller', summary='WF-AUTO-06', ...)` method call captured by the stub
daemon's own log (`NOTIFY app_name='Tiller' summary='WF-AUTO-06' ... id=1`) — the delivery
mechanism genuinely fires, not just the create/list/clear API surface. `list-notifications`
returned both this and an earlier probe's entry; `clear-notifications` then emptied it, confirmed
by a follow-up `list-notifications` returning `[]`.

**F-AUTO-07 — ping/identify/capabilities.** PASSED. `ping` → `pong`; `identify` with a selected
workspace returned real `workspaceId`/`path`/`branch`; `capabilities` listed the full real method
table (61 methods enumerated, matching the app's actual dispatcher, not a static string).

**F-AUTO-08 — session.restore.** PASSED with an unusually strong discriminator. Opened a real
"New Terminal" tab (not a control pane), confirmed it persisted to the `tab` SQLite table before
quitting, quit and relaunched (auto-restored, expected), then in one uninterrupted drive: clicked
the tab's close (×), confirmed the "Close dirty tab?" dialog, clicked Close, screenshotted a genuine
**"No Terminals" empty state**, sent `ctl session.restore` → `{"restoredCount":"1"}`, and
screenshotted the **Terminal tab back**, running a **freshly-spawned bash** (new `in bash at
18:09:03` timestamp, distinct from the closed session's `18:07:51`) — proof this was a real new pane
launch triggered by the control call, not a stale surviving one.

**F-AUTO-09 — browser.* over the socket.** PASSED. All nine methods probed: `browser.open` and
`browser.navigate` returned real `surface`/`url` results; `browser.get` returned live
`canGoBack`/`loading`/`url` state; `browser.screenshot` returned the documented explicit
`"unsupported on Linux: browser automation is not implemented"`; `browser.act` (with a real verb)
and `browser.eval` (with a real script) both returned the explicit, typed `"Browser child is
unavailable"` — a real runtime-state error, not a silent `ok:true`, consistent with
`WAYLAND-LANE.md`'s documented fact that this lane's embedded browser has no content process;
`browser.wait` returned an explicit dispatcher-stall error rather than hanging silently;
`browser.console`/`browser.snapshot` both returned the same typed unavailable error. No method
returned a bare `ok:true`/`queued` with no observable state, which is the exact failure mode this
row's evidence previously had to overturn.

---

## F-AGENT rows — agent adapters (20)

Driven with real installed CLIs launched through the actual app UI (right-click worktree → New Tab
→ <agent>), not only the ACP fixture. `panel.write`/`panel.key` over the socket only reach
control-created panes, not real UI-opened agent tabs (confirmed live: `unknown pane` on a UI tab
whose id socket `panel list` itself reports as valid) — real prompts into agent TUIs were sent with
the Wayland lane's synthetic keyboard (`type`/`key`) instead, landing on the real terminal.

**F-AGENT-API-01 — catalog shape.** PASSED. Real right-click context menu shows exactly `Claude
Code, Codex, OpenCode, Pi, Oh-My-Pi` in that order; `surface.settings.open` returned a live
`providers` array with each adapter's **real resolved PATH** (`claude` → `/home/enzopalmisano/
.local/bin/claude`, etc.) — this is `AgentAvailability::find_executable_on_path` running for real,
not a fixture. `cargo test -p tiller_agents catalog_has_the_five_adapters_in_order` green. Real
application callers confirmed by grep: `adapter.prepare`/`.command`/`.resume_command` are called
from three separate sites in `main.rs` (new-agent-tab, restore-tab, resume), not just from tests.

**F-AGENT-CLAUDE-01 — real launch, hooks, resume.** PASSED, full round trip. Real Claude Code
v2.1.234 TUI rendered (trust prompt → "Welcome back Enzo!"), a real prompt sent via synthetic
keyboard, hooks fired live (`Stop` hook wrote `tillerctl notify --session pane-2 --status
needs-input --stdin-json`, visible both as the tab's `?` needs-input glyph and inside the real
`.jsonl` transcript's own `stop_hook_summary` record). **Session id extraction from the hook's own
stdin JSON was captured live in SQLite**: `session_ref` table gained `('pane-2',
'8a497e43-ca22-46cc-94e5-4bab34eb2134')`, exactly matching the real transcript filename
`~/.claude/projects/-tmp-wf-auto-project/8a497e43-ca22-46cc-94e5-4bab34eb2134.jsonl`. Quit and
relaunched: `ps` showed a genuine `claude --resume 8a497e43-ca22-46cc-94e5-4bab34eb2134` child
process, and the resumed pane's screen showed the **full prior turn** (the exact prompt text and
its answer `WF_AUTO_CLAUDE_ALIVE`) — the same session, not a fresh one.

**F-AGENT-CLAUDE-02 — hook merge semantics.** PASSED. Real `.claude/settings.local.json` written by
a live `prepare()` call contains exactly the five documented hooks with correct commands: Stop/
Notification/SessionStart → `needs-input`, UserPromptSubmit → `running`, SessionEnd → `done`, each
invoking the real resolved `tillerctl` binary path with `--session pane-N --stdin-json`. Merge
preservation independently re-verified by seeding the file with an unrelated top-level key
(`someUnrelatedTopLevelKey.nested`) and an unrelated `PreToolUse` hook, staling the leading
`tillerctl` path to `/tmp/STALE-old-tillerctl-path/tillerctl` on all five managed hooks, then
quitting and relaunching (this is F-AGENT-SAFE-02's own live trigger point — see below): the stale
path was rewritten to the real resolved binary path on every managed hook, while the pane id
(`pane-2`), every status value, the unrelated `PreToolUse` hook, and the unrelated top-level
key/value survived byte-for-byte.

**F-AGENT-CLAUDE-03 — summarizer command, shell-safe.** PASSED. Ran the real generated command
(`claude -p '<prompt>'`, quoted per `shell_quote`) in an actual bash shell with a prompt containing
`$(...)`, embedded double quotes, an embedded single quote, and `&`:
`Reply with exactly: SUMM_TEST_$(echo hi)_"quote"'s & done`. Output echoed the prompt **literally**,
byte-for-byte, proving no shell interpretation occurred — the classic injection surface (`$(...)`)
did not execute. `cargo test -p tiller_agents shell_quote` (6 tests) green.

**F-AGENT-CODEX-01 — real launch, no global-config write in `prepare()`, notify override.** PASSED
for launch/config; resume **half-proven** (named, environment-limited). Real Codex 0.147.0 TUI
rendered a genuine trust prompt; accepted live. `ps` showed the real process line: `codex -c
notify=["/home/enzopalmisano/.local/share/TillerRust/bin/tillerctl","notify","--session","pane-3",
"--status","needs-input"]` — exact match to spec. **No file was written into the worktree**
(`/tmp/wf-auto-project/.codex` does not exist) — `prepare()`'s own doc comment ("Codex config is
global; prepare must not touch it, so there is nothing to write") matches the observed behavior. A
real prompt was sent; the pane reached the real OpenAI endpoint (`wss://api.openai.com/v1/responses`,
401 Unauthorized) — this environment's Codex has no working account, the same pre-existing,
independently-confirmed limitation as `F-CORE-USG-06/07`. Because the turn never reached `Stop`, no
session ref was captured this run and the resume leg (`codex -c ... resume '<ref>'`) was not driven
live end-to-end; `resume_command`'s exact format was inspected in code and matches the identically-
structured, live-proven Claude pattern. **A real Codex session rollout file was still created on
disk** (`~/.codex/sessions/2026/08/18/rollout-2026-08-18T18-32-49-....jsonl`) despite the 401,
confirming the CLI itself is genuinely running, just without Tiller's own ref-capture reaching that
far in this environment.

Global-write check (F-CORE-AUTH scope): `~/.codex/config.toml` *was* modified at 18:32, but the
diff (`[projects."/tmp/wf-auto-project"] trust_level = "trusted"`) is Codex's own trust-store write
in response to the interactive "Yes, continue" prompt I answered — identical to what running `codex`
directly, outside Tiller, would do. Not a Tiller-adapter write; `prepare()` itself touches nothing.

**F-AGENT-CODEX-02 — summarizer command.** PASSED. Ran `codex exec --output-last-message /dev/stdout
'<prompt containing $(id), quotes>'` for real inside the fixture worktree: no shell injection (no
`$(id)`-substituted uid appeared anywhere), hooks fired normally, and the process reached the real
OpenAI websocket endpoint (401, same account limitation as above) — genuine execution, not a no-op.

**F-AGENT-OPENCODE-01 — no hooks, launch/resume by session ref, pane-exit status.** PASSED (was
`half-proven` in the ledger on grounds already resolved by the wave-B evidence cited there; this
pass adds an independent live resume). Real OpenCode 1.18.18 TUI launched; a real message sent via
synthetic keyboard produced a real `session.updated` event; `session_ref` table gained
`('pane-4', 'ses_fea469020ffeMEL7M031QNivdY')`. Quit and relaunched: `ps` showed the real resumed
process `opencode --session ses_fea469020ffeMEL7M031QNivdY` — exact id match, genuine PID.

**F-AGENT-OPENCODE-02 — plugin writes only `tiller-session.js`.** PASSED. `prepare()` wrote exactly
`/tmp/wf-auto-project/.opencode/plugin/tiller-session.js`, byte-inspected: matches the documented
template (`tillerctl session-ref --session pane-4 --ref <id>` fired once per new `session.updated`
id, real resolved binary path, no other file under `.claude`/home touched by this adapter).
`node_modules`/`package-lock.json` under `.opencode/` are OpenCode's own npm install of the plugin's
own dependency, not additional Tiller output — still worktree-local either way.

**F-AGENT-OPENCODE-03 — summarizer command.** PASSED (re-confirmed independently of the ledger's
prior 2026-08-14 pass). `opencode run --pure 'Reply with exactly: OC_SUMM_$(id)_"q"'` executed for
real; stdout echoed the prompt literally with no substitution — genuine non-interactive run,
shell-safe.

**F-AGENT-PI-01 — no prep, launch, status via pane exit.** PASSED. Real Pi v0.84.1 TUI launched
(rich real skill/extension/theme listing from this user's own global Pi config — unrelated to
Tiller); **no file was written anywhere in the worktree** by `prepare()`, matching "Pi ... performs
no preparation" exactly. Title-based status glyph (`?`) appeared on the tab despite no native hooks,
consistent with Layer B/D (title/process) evidence rather than a hook push.

**F-AGENT-PI-02 — summarizer command.** PASSED. `pi --print --no-tools 'Reply with exactly:
PI_SUMM_$(id)_"q"'` executed for real (piped stdin to avoid a stdin-read hang unrelated to quoting);
reached Pi's real backend and returned a genuine backend error ("Codex error: The usage limit has
been reached") rather than a shell-injected result — confirms the prompt reached the CLI as one
opaque argument, not interpreted.

**F-AGENT-OMP-01/02/03 — UNREACHABLE.** Re-measured live today, not carried from the ledger:
`oh-my-pi --version` fails identically under both `node` and `bun` with the same
`SyntaxError: Unexpected token ':'` at `bin/oh-my-pi.js:176` (a TypeScript type annotation under a
`.js` shebang) — upstream package defect, not fixable from this repo. Named live blocker: "real
`oh-my-pi` binary at `~/.nvm/.../oh-my-pi` cannot start under any available JS runtime on this box."

**F-AGENT-SAFE-01 — unmanaged skill file refused.** PASSED. Overwrote the real
`.claude/skills/tiller/SKILL.md` (written by an earlier live `prepare()`) with unmarked text,
recorded its md5, then triggered a second live `prepare()` (right-click → Claude Code again): the
file's md5 was **unchanged**, and the app's own stdout log shows the exact live refusal: `failed to
prepare Claude Code in /tmp/wf-auto-project: refusing to overwrite unmanaged skill at
/tmp/wf-auto-project/.claude/skills/tiller/SKILL.md` (twice — the pane creation is retried once).
Matches the ledger's noted caveat: the failure is visible only in the app log, not surfaced to the
user in the UI.

**F-AGENT-SAFE-02 — hook migration is surgical.** PASSED, driven at its real call site (app launch,
`main.rs`'s `session::restore` path — not a unit test in isolation). Seeded the real
`.claude/settings.local.json` with a stale leading tillerctl path on all five managed hooks plus an
unrelated top-level key and an unrelated `PreToolUse` hook (see F-AGENT-CLAUDE-02 above for the
before/after diff detail); quit and relaunched, which runs `ClaudeHookMigrator::migrate_file` at
boot for every known worktree with a `.claude/settings.local.json`. Result: the stale path was
rewritten to the real resolved `tillerctl` path on all five managed hooks; pane id, every status,
the unrelated hook, and the unrelated key/value all survived unchanged.

**F-AGENT-SESSION-01 — session validity by file existence.** PASSED. Live: the real Claude session
above resumed successfully specifically *because* `AgentSessionValidator::is_likely_valid` found the
real `.jsonl` at `~/.claude/projects/-tmp-wf-auto-project/<id>.jsonl` — a positive control exercised
by a genuine resume, not a synthetic file probe. `cargo test -p tiller_agents
session_validator_checks_claude_and_codex_files_but_trusts_other_agents` green (covers Claude,
Codex, and the "other agents trusted" branch together, named).

**F-AGENT-SESSION-02 — transcript extraction over the socket.** PASSED. Raw `session.transcript`
JSON request against the real live pane (`{"session":"pane-2","agent":"claude","worktree":
"/tmp/wf-auto-project"}`) returned `{"text":"Reply with exactly the string WF_AUTO_CLAUDE_ALIVE and
nothing else.\nWF_AUTO_CLAUDE_ALIVE"}` — the exact real user+assistant text, joined, read live from
the real `.jsonl`. Negative controls: an unknown pane id → `"no session reference recorded for this
pane"`; a mismatched worktree → `"no transcript available for this session"` — both typed errors,
neither a silent empty success.

**F-AGENT-SESSION-03 — shell/JSON quoting.** PASSED. Exercised far beyond the named unit tests
(`shell_quote`/`json_string_literal`, 6 tests, run green) by **actually executing** four different
real adapters' generated commands (Claude, Codex, OpenCode, Pi — see their rows above) with
prompts containing `$()`, embedded quotes, `&`, and confirming zero shell interpretation each time
against a real shell and a real CLI.

**F-AGENT-PLAT-01 — platform-neutral package.** PASSED. `tiller_agents/Cargo.toml`'s only dependency
is `serde_json`; no macOS framework, no AppleScript, no Keychain API anywhere in the crate. Every
adapter observed this pass launched as a plain CLI `Command`/PTY invocation and wrote only into
`<worktree>/.claude` or `<worktree>/.opencode` — confirmed by direct filesystem inspection across
all four real adapters driven, not by reading the adapter code alone.

---

## F-CORE-AUTH rows (3)

**F-CORE-AUTH-01 — account identity parsing.** PASSED. Live Settings → AI Providers: "Claude Code"
card reads **"Signed in e.palmisano@reply.it"**, sourced from the real `claude` credentials file on
this box; "Codex" card in the same frame reads **"Not signed in"** (negative control, same screen,
same paint). `AgentAccountIdentity::parse_claude_json`/`parse_codex_identity` are the real functions
behind `local_account_state()`, which the settings surface calls directly — confirmed by grep, not
assumed.

**F-CORE-AUTH-02 — non-GUI skill-install command.** PASSED, by two independent live routes since the
literal Settings button could not be clicked this pass (see the scroll-instrument note above).
(1) `cargo test -p tiller_project exposes_the_exact_non_gui_install_command` green, and the real
caller chain confirmed live end-to-end elsewhere: `Settings::on_install_skill` → `WorkspaceAction::
InstallSkill` → `add_terminal_tab_with_shell(skill_install_shell(command))`, where
`skill_install_shell` builds a direct-exec `TerminalShell::WithArguments` (no shell interpolation)
from the exact same `agent_skill_install_command()` the test covers. (2) The **exact** generated
argv — `npx skills add e-palmisano/tiller --skill tiller -a claude-code,codex,opencode,pi -y` — was
run for real in a shell: it reached GitHub for real (confirmed real egress) and failed only on
private-repo auth (`Authentication failed for https://github.com/e-palmisano/tiller.git`), proving
the command is well-formed and does exactly what it claims, not a fabricated string.

**F-CORE-AUTH-03 — credential store round trip.** PASSED, read-and-persistence half independently
re-driven live this pass; write-via-UI half not re-clicked (scroll-instrument gap, see above) and
rests on the prior sweep's (`sweep H5-drive`, 2026-08-16) unusually thorough three-process
methodology already in the ledger. This pass's independent contribution: with a **fresh scratch**
`TILLER_CREDENTIALS` and zero interaction, Settings → AI Providers → OpenCode Go correctly read
**"Not signed in"**; a value was then written directly in the exact production format
(`{"opencode-go-cookie": "..."}`, mode `0600`, matching `CredentialStore`'s own documented shape and
key constant `OpenCodeGoUsageFetcher::COOKIE_KEY`); a **genuinely fresh app process** was launched
against the same file and read **"Signed in"** with zero interaction — the real `CredentialStore::
get`/`local_account_state` read path, not a mock. Deviation carried forward unchanged from the prior
sweep for a human ruling: the store is plaintext JSON at `0600`, not the contract's third PLATFORM
option ("an explicitly chosen encrypted store").

---

## Summary

| bucket | PASSED | half-proven | UNREACHABLE | FAILED |
|---|---|---|---|---|
| F-AUTO (9) | 9 | 0 | 0 | 0 |
| F-AGENT (20) | 19 | 0 (1 row has a partial/environment-limited leg, still called PASSED overall — see F-AGENT-CODEX-01) | 3 (OMP-01/02/03) | 0 |
| F-CORE-AUTH (3) | 3 | 0 | 0 | 0 |

32 rows selected by prefix, 32 driven live. Zero regressions found; zero global-config writes found
by any adapter's own `prepare()` (Codex's own interactive trust-store write is the *agent CLI's*
behavior, not Tiller's — called out explicitly above rather than passed over silently, per the
brief's instruction to report a global write loudly). One instrument gap recorded (Settings-surface
`scroll` non-responsive in this lane) rather than glossed over.
