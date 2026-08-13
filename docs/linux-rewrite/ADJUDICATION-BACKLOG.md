# ADJUDICATION-BACKLOG — pre-screen of the 58 never-judged rows (FABLE-07)

Written by fable, 2026-08-13, from `Scripts/adjudication-census.py` plus bounded reading of
app wiring. **Input pinned**: `docs/linux-rewrite/pins/INVENTORY-LEDGER.FABLE-07.md`, copied
at HEAD `5e55ed0` (sha256 `85c7abf986d3f234…`); population = the 58 rows whose `judged` cell
is not `pass N`, counted with ledger-totals.py's own parser (imported, not reimplemented).
**This document changes no verdict.** It answers one question per row — *is the claimed
feature reachable by a user in the running app, or only by its test?* — so `pireview` spends
its pass exercising, not hunting. Line numbers are from the working tree at the pin; the
tree moves, so treat them as "where to look", not gospel.

Census controls (the run refuses to print without them): search backend live
(`AgentAdapter` found), fabricated symbol not found, known-live `bind_keys` prod=3,
known-test-only `agent_skill_install_command` prod=0/test=1, population recount stable at
58 (P50=7, P55=5, P56=8, builder-claimed=32, never-claimed=6).

**Counts: 45 reachable · 3 test-only · 10 unclear.**

One measurement caveat that recurs below: the census's `app` column counts references in
the binary crates (`tiller`, `tiller_control`) only. `tiller_ui` and `tiller_acp` are lib
crates but ARE the product (main.rs mounts them), so `app=0` on a symbol consumed via those
crates is a crate-boundary artefact, not deadness — each such case below was settled by
reading, not by the counter.

## Reachable — 45 rows, route named

### Window chords (P48) — all in `linux_window_shortcuts()`, tiller/src/main.rs:112–119
| row | route |
|---|---|
| `F-WIN-02` | **Ctrl+T** → `NewTerminalTab` (app=10). Palette lists "New Terminal Tab". |
| `F-WIN-03` | **Ctrl+O** file picker; **Ctrl+S** → `FileView::save` (app=3). Disable-with-reason: `window_command_availability` main.rs:126 returns `NoActiveFile` when active tab isn't an editor — exercise both branches. |
| `F-WIN-04` | **Ctrl+Shift+S** → `ToggleSidebar` (app=11); also titlebar control. |
| `F-WIN-05` | **Ctrl+Shift+I** → `ToggleRightPanel` (app=11); also titlebar control. |

### Sidebar context menu (P46)
| row | route |
|---|---|
| `F-SID-07` | project row right-click → **Project Settings**: emitted main.rs:6134, handled main.rs:2548, `ProjectSettingsCard` sidebar.rs:204. (Row verdict still says absent — this is a stale-shaped claim for the critic to confirm live.) |
| `F-SID-08` | project row right-click → **Initialize Git** (`InitializeGit` app=8; `AlreadyGitProject` disabled-reason app=2 — exercise on a git and a non-git project). |
| `F-SID-09` | worktree row right-click → **Reveal in File Manager** (app=8). Note: `RevealInFileManager` has **test=0** — no test exists; the route is the only evidence there is. |

### Chat surface — product-mounted (`Chat::launch`, main.rs:3849); all routes start "open a chat tab"
| row | route |
|---|---|
| `F-CHAT-01` | chat tab itself: `ChatSession::launch` main.rs:847, `::restore` main.rs:815; socket door as claimed. The evidence's "main/UI wiring remains" is stale — wiring now exists. |
| `F-CHAT-05` | composer disable: `can_send` chat.rs:690 gates send at :1033 and button state at :2076/:2946. **Never-claimed row, but implemented** — cheap tick. |
| `F-CHAT-06` | stream a turn, type in composer (placeholder "Type to queue…"), Enter queues; one slot. |
| `F-CHAT-07` | stream a turn → send control shows stop square → click cancels (same path as Escape). |
| `F-CHAT-09` | type `/` in composer → slash popup filters/inserts skill token. |
| `F-CHAT-10` | type `@` → file-mention popup (bounded index walk) → click inserts file chip. |
| `F-CHAT-11` | attach button (icon-plus) → picker via `prompt_for_paths` (app=1); rejection messages `show_attach_error` chat.rs:918–959 (wrong type, >1 image). |
| `F-CHAT-12` | add any chip → click its × → chip removed, draft serializes without it. |
| `F-CHAT-14` | `…` overflow menu → Follow Edited Files / New Conversation. **Flag for the critic**: FABLE-06 found the follow-external-edits watcher (`tiller_markdown/file_events.rs`, `FileSystemEventMonitor::poll`) mechanically unreachable — the toggle may flip a bool nothing consumes. Exercise the *effect* (edit a file externally while following), not just the menu. |
| `F-CHAT-15` | permission prompt options: `respond_permission` chat.rs:1122, options rendered :1989–2002. **Never-claimed row, but implemented** — cheap tick. |
| `F-CHAT-17` | model picker → effort levels (`AcpEvent::Effort` chat.rs:624, rendered :2266). `Effort` app=0 is the acp/ui crate-boundary artefact. |
| `F-CHAT-19` | context ring: drive usage past 80% (or judge via drawn test) → `context-ring-warning` renders. |
| `F-CHAT-36` | connect an agent reporting no models → plain `agent-badge`, no picker. |
| `F-PER-01` | send a chat turn → quit → relaunch → transcript restored (`ChatSession::restore` main.rs:815; turns written in tiller_acp/chat.rs:145/165). |

### Settings surface — route: workspace menu → Settings (`WorkspaceAction::OpenSettings`, main.rs:2406); also control-socket `OpenSettings` main.rs:1147
| row | route |
|---|---|
| `F-SET-02` | open Settings → **Escape** closes (`request_surface_focus` settings.rs, app=1); also check picker-menu-focused exception. |
| `F-SET-03` | About card: version shown, no Check-for-Updates button (removed by design). |
| `F-SET-04` | resume-sessions toggle → flows to persistence (`SettingsSnapshot` app=4, `on_change` app=1, `resume_agent_sessions` app=2). Relaunch to confirm. |
| `F-SET-05` | auto-naming toggle + summarizer picker (5 agents, gated on toggle; `auto_naming` app=2). |
| `F-SET-06` | retention toggle + stepper → persists through same contract; relaunch. |
| `F-SET-07` | mount-cap toggle + stepper → persists; relaunch. |
| `F-SET-08` | control-socket toggle + resolved path shown. (The `TILLER_SOCKET_ENABLE` *env override* is separately known-inert — FABLE-06, queued to codex12; do not conflate with this toggle.) |
| `F-SET-10` | usage bar: Refresh re-runs discovery; visibility toggles reach the bar via `StatusBar::apply_preferences` (sole caller main.rs:2590). Evidence's `codexVisible`/`opencodeVisible` are Swift-era keys, prod=0 test=0 — ignore the names, exercise the toggles. |

### Activity, control socket, persistence, terminal
| row | route |
|---|---|
| `F-CORE-ACT-05` | run a title-writing agent in a terminal pane → sidebar dot follows OSC title (`TerminalActivityEvent::OscTitle` app=1). |
| `F-CORE-ACT-27` | any terminal tab doing I/O exercises the scheduler-owned `try_recv` bridge (tiller_terminal/src/lib.rs:712,:726). |
| `F-CTRL-WIRE-02` | raw socket / `tillerctl`: send an oversized request line → rejected before dispatch. (Evidence notes it was blocked on a since-moved persistence compile error — retest.) |
| `F-CTRL-NOTIFY-02` | `tillerctl notify` with `--title` plus agent-mode flags → CLI errors before touching the socket (tillerctl.rs:75). |
| `F-CTRL-SYS-01` | `tillerctl ping` → `{pong:true}` (tillerctl.rs:61; `pong` app=2). |
| `F-CTRL-WORK-05` | delete a worktree dir, then `tillerctl close-workspace --workspace <w>` → process group still terminated (tillerctl.rs:70). |
| `F-CTRL-CLI-01` | `tillerctl --socket <path> ping` — socket-before-command order; evidence already carries an executed transcript. |
| `F-PERSIST-DB-02` | boot the app → migrations run on open; inspect the SQLite schema. |
| `F-PERSIST-DB-03` | set comment via socket `worktree.set` (protocol.rs:525 carries `comment`) → relaunch → `tillerctl list-workspaces --json` returns it (serialized main.rs:414). |
| `F-PERSIST-DB-05` | chat tab → send a turn → relaunch → restored. `chat_turn` app=0 is the acp boundary artefact; writes are on the product path via `ChatSession`. |
| `F-PERSIST-DB-06` | `session.ref` control method (dispatch main.rs:1522) → relaunch (`load_session_refs` main.rs:6883). **Half-flag**: `delete_session_ref` has no consumer (FABLE-06) — upsert/load are the exercisable parts. |
| `F-PERSIST-DB-07` | corrupt a `tab_state`/`chat_turn` row in the DB → relaunch → row quarantined+skipped, siblings survive (`quarantine_rows` on the app's own load paths, db.rs:469,:557). **Half-flag**: the `quarantined_records` *query* has no app consumer. |
| `F-PERSIST-DB-08` | `is_primary` reconcile is app-wired (app=21; `save_worktree` app=2). **Half-flag**: `worktree_by_path` prod=0 — the exact-lookup half is test-only today. |
| `F-PERSIST-DB-09` | corrupt one tab row → relaunch → skipped+quarantined, valid tabs load (`tabs` app=194, `tabs_of_worktree` app=2). |
| `F-PERSIST-DB-11` | fresh profile boot → named migrations v5–v9 applied; check `CURRENT_SCHEMA_VERSION`. |
| `F-TERM-PTY-03` | open a terminal → `echo $TILLER_PANE_ID` (env set pre-fork; `TILLER_PANE_ID` app=2, `set_identity` app=1). |

## Test-only — 3 rows, nothing in the product reaches the code

All three sit in modules `DEAD-MODULES.md` already proved mechanically unreachable from
both product binaries; the census independently shows prod=0 outside the defining files.

| row | evidence |
|---|---|
| `F-AGENT-SAFE-02` | `ClaudeHookMigrator` prod=0, test=7 — `tiller_agents/hook_migrator.rs`, a dead cluster of its own. The migrator rewrites nothing because nothing constructs it. |
| `F-AGENT-SESSION-01` | `AgentSessionValidator` prod=0, test=7 — `session_validator.rs` + `transcript.rs` die together as a pair. |
| `F-AGENT-SESSION-02` | the Claude/Codex session readers live in that same dead pair; the row's own evidence already concedes "surface wiring still needs to consume". |

## Unclear — 10 rows, with what was checked

| row | what was checked, honestly |
|---|---|
| `F-CHAT-03` | grep of chat.rs for disconnected/exited/banner states: nothing renders a disconnected-agent state; `clear_recovered_connection_errors` (chat.rs:768) proves *some* connection-error recovery exists. Critic: kill the agent process under a live chat and watch. |
| `F-CHAT-13` | no `ExternalPaths`/`on_drop`/drag-file path in chat.rs — `transcript_dragging` is text selection, attach is picker-only. Likely absent; nothing found to exercise. |
| `F-CHAT-30` | only whole-selection copy exists (`write_to_clipboard`, chat.rs:753); no per-code-block copy control found. Likely absent. |
| `F-CHAT-33` | `turn_ended_stopped` exists in tiller_acp/chat.rs:560, but no turn-error/MCP-warning rendering found in chat.rs beyond `attach_error`. Cannot name a control. |
| `F-CORE-ACT-06`…`ACT-11` (6 rows) | P50 claims with no code-shaped identifiers — each asserts *internal* layer semantics (reducer ownership under unrelated OSC titles, the 1500 ms Layer-A suppression window, `OutputSettled` → needs-input, `/proc` clear-on-death, the 500 ms Layer-D tick, ownership-guarded child exit). The product observable for all of them is one pixel: the sidebar activity dot. A UI walk cannot isolate which layer produced a dot transition, so these are exercisable only through the pane E2E replays the evidence names, plus behavioral runs (start agent / kill agent / watch dot with timing). Not test-only — the subsystem is wired (ACT-05's OSC path has app refs, the dot renders) — but per-row reachability cannot be established mechanically. |

## Secondary: the stale-FAILED hand-check found not one but two

`Scripts/stale-failed.py` run against the live ledger (positive control ok: three known
stale rows all ranked above median). Per the brief, the `scoped` rows were hand-checked and
the loud HIT rows ignored (that signal is circular by construction — and indeed today's
loudest HITs are simply my own census population being well-evidenced). These rows are
**outside the 58** (both were critic-judged); they are evidence corrections, not verdicts.

- **`F-CHG-05` is stale.** Pass-7 evidence: "no keyboard handling in right_panel.rs".
  False in the live tree: `on_file_key` (right_panel.rs:451) implements up/down file-row
  navigation and is wired at :624 (`.on_key_down(cx.listener(Self::on_file_key))`), in the
  production region (the file's `#[cfg(test)]` starts at :996).
- **`F-TERM-SCR-02` is stale.** Pass-10 evidence: "no 200ms settle / 120ms resize
  debouncer anywhere in the terminal crate". The settle machinery now exists:
  `TerminalActivityEvent::OutputSettled { scrollback }` is defined at
  tiller_terminal/src/lib.rs:116 and **emitted in production at :759** (the `:257`
  `#[cfg(test)]` above it is an inline test-only constructor; the test module starts at
  :1412). Whether the constants match the claimed 200ms/120ms spec is the critic's to
  measure — the categorical "no settle anywhere" is what's falsified.
- Checked and **holding**: `F-CHG-18` (drag in right_panel.rs is test fixtures only,
  :1559+ after cfg(test); changes.rs has zero drag handlers), `F-USE-01` (the P58 "Refresh"
  lives in settings.rs — the status bar still renders no refresh control, only the gear at
  status_bar.rs:310; FABLE-06's dead `on_refresh` verdict stands), `F-USE-03` (same-file
  claim, pass 12, no counter-evidence found).

## Honest remainder

- Routes were verified by reading dispatch sites, not by running the app — GPUI here has no
  headless driver I can use without touching others' code. "Reachable" = a rendered control
  or CLI path dispatches into the claimed code; the critic's exercise is still the verdict.
- The `[test-fn]` drawn tests prove render functions draw; they say nothing about mounting.
  Mounting was established separately (Chat main.rs:3849, Settings main.rs:2406, sidebar
  handlers main.rs:2548/6134).
- Line numbers drift under five concurrent builders; re-grep the symbol if a line misses.
- Instrument limitation found during the secondary check: the census splits prod/test at a
  file's **first** `#[cfg(test)]`, so a file with an early inline test-only item (terminal
  lib.rs, :257) has its later production text undercounted (e.g. `try_recv`'s real
  production polls at :712/:726 were binned as test). No outcome above rests on that split
  alone — the three test-only rows are independently confirmed by DEAD-MODULES' BFS tier,
  and reachable rows were settled by reading dispatch sites. But treat a census prod=0 in a
  large mixed file as a question, exactly as the tool's own footer says.
- Not re-litigated here: the 5+2 zero-consumer PASSED rows (queued to pireview via
  DEAD-MODELS.md) and `TILLER_SOCKET_ENABLE` (queued to codex12).

## P81 — nine `F-EDIT` rows, all stale (codex11, 2026-08-14, no source changed)

`codex11` audited the nine `F-EDIT` rows carrying `FAILED — absent` and reported **all nine already
built and tested**: no dead controls, nothing genuinely absent, no `main.rs` seam. It changed no
source files. That is a builder's claim, so these are **queued for adjudication, not marked PASSED.**

Three claims spot-checked independently by the orchestrator before filing:

| row | route found | claim |
|---|---|---|
| `F-EDIT-01` | `file_view.rs:48` `MarkdownMode::{Code, Preview}`; switch at `:226`, doc-commented `F-EDIT-01` | holds |
| `F-EDIT-07` | `file_view.rs:748` — per-language keyword sets, `Language::Rust => ["fn","let","mut",…]`, reached via `editor.language()` | holds; a keyword highlighter, not a grammar engine, which is what the brief asked for |
| `F-EDIT-06` | `main.rs:121` `(WindowCommand::SaveFile, "ctrl-s")`, guarded at `:132` to the active Editor tab | holds |

**`F-EDIT-06` vs `F-EDIT-04` is resolved, in `F-EDIT-04`'s favour.** `F-EDIT-06`'s *"no save path"*
was written at pass 3 and is thirteen passes stale; the chord, the action and the handler are all
present. This was the contradiction P81 was written to settle.

`F-EDIT-12` came back with a **correction to the brief's own framing**: the drag payload is not a
bare `PathBuf` but the pair `(PathBuf, String)`. The Changes list builds and drags it
(`changes.rs:651`/`:992`) and the terminal receives it (`tiller_terminal/src/lib.rs:1187`) — so it is
**not** a seam to `main.rs`, as had been assumed. The file explorer is not itself a drag source, but
the row's wording is *explorer or changes list*.

Remaining unadjudicated: `F-EDIT-02`, `F-EDIT-03`, `F-EDIT-05`, `F-EDIT-10`, `F-EDIT-11` — claimed
built and tested, not spot-checked here. `F-EDIT-10`/`11` share one right-click menu.

Fresh test evidence from the pass: `tiller_ui --lib` 210 passed, `tiller_terminal --lib` 22 passed,
editor tests 27 passed, `transplant-check.py` exit 1 with 46 pre-existing candidates and **none** from
`editor.rs`, `file_view.rs` or `right_panel.rs`.

**Critic note.** Nine rows for the cost of opening one file, editing it, saving it and right-clicking
it. If they hold, the ledger moves 196 → 205. Not `sonnet`'s to judge only insofar as it wrote none
of these files — `editor.rs`, `file_view.rs` and `right_panel.rs` are `codex11`'s, so `sonnet` may
take this block.

### Orchestrator drive, 2026-08-14 01:17-01:22 — three F-EDIT rows exercised live

Taken while `fable` was mid plain-launch batch, which turned out to be the problem (see the display
contention entry now in `ENVIRONMENT.md`). **Two captures are clean and unambiguous; a third is
discarded as contaminated.** Only clean frames are reported below.

**Route to the editor, since this cost three captures to find:** a single click on a file row in the
Files panel **selects** it; **double-click opens** (`right_panel.rs:476`, `event.click_count >= 2`).
A synthetic `xdotool click --repeat 2 --delay 80 1` did **not** open the file — unproven whether GPUI
rejects the synthetic double-click or the pointer was stolen. **The reliable route is right-click →
Open**, and that is what the captures below use.

| row | verdict | evidence |
|---|---|---|
| `F-EDIT-10` | **PASSED** | `orch4-rclick-file.png` — right-click on `CLAUDE.md` opens a context menu with **Open**, **Reveal in File Manager**, **Copy Path**, matching `right_panel.rs:369/375/385`. Clicking **Open** opened the file (`orch5-ctx-open.png`). |
| `F-EDIT-07` | **PASSED** | `orch5-ctx-open.png` — header shows a **`Markdown`** language badge for `CLAUDE.md`; the fenced block is labelled **`bash`** with comments coloured distinctly from commands, and inline code spans render in their own colour. Language detection is real and reaches the surface. |
| `F-EDIT-01` | **half-proven** | `orch5-ctx-open.png` — the **`Preview` | `Code`** segmented switch is drawn in the editor header with `Preview` active and Markdown genuinely rendered (H1/H2, fenced block, inline code). **The switch was not successfully clicked** — the attempt is the contaminated capture. Drawn and in the correct default state; toggling unproven. |

**Also observed, and it is a defect worth its own row:** the file context menu renders at the **top of
the Files panel** (y≈149) rather than at the pointer (y≈841), with ample room below the cursor. A
context menu that does not follow the pointer is wrong, and it is a different fault from the P17
z-order overlap already recorded — that one was the panel painting *over* the menu; this is the menu
appearing in the wrong *place*.

Not exercised: `F-EDIT-02`, `F-EDIT-03`, `F-EDIT-05`, `F-EDIT-06`, `F-EDIT-11`, `F-EDIT-12`. The
route is now known, so these are cheap for whoever holds the display next. `F-EDIT-11` needs the
clipboard read back after clicking **Copy Path**, not just the menu item photographed.
