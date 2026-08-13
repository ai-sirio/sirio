# CRITIC baseline — Linux GPUI rebuild of Tiller

Frozen snapshot judged: `/home/enzopalmisano/Scrivania/Progetti/_tiller-critic-snap`
Method: build the binary, run it on Xwayland (`DISPLAY=:1`), drive it with synthetic
X input (xdotool XTEST), and observe the results through (a) OCR of window captures
(tesseract.js), (b) the app's own SQLite session database, (c) the host process tree,
(d) host filesystem side effects, (e) the real agent's transcript files. No source
claims are ticked: every line below is something I ran and saw.

Instrumentation notes:
- Absolute-coordinate clicks were re-derived from the window origin each launch
  (window is 1715x972 on screen for 1470x833 logical points, scale 1.1667).
- OCR (tesseract) reads UI text reliably at 11-13pt; terminal glyphs are too small
  for OCR, so terminal behaviour is evidenced by process tree and filesystem
  side effects instead.
- Orchestrator rulings applied: opencode/omp not installed => UNREACHABLE for those
  entries; macOS TCC/System-Settings permissions => N/A platform (third state);
  snapshot has no `.git` (orchestrator's copy error) => git-dependent entries
  UNREACHABLE inside the snapshot unless exercised read-only in the live
  `tiller-linux` checkout.

Status vocabulary: PASSED = exercised, worked. FAILED = exercised, broke or absent
in the running program. UNREACHABLE = could not be exercised (missing .git in
snapshot, missing agent CLI, out of scope). N/A = macOS-only machinery, no Linux
equivalent.

## Build and launch

- F-WIN-01 (workspace window opens): PASSED. `cargo build -p tiller` completes
  (cargo 1.97.1, 13-crate workspace). `Scripts/linux-shot.sh` exits 0: window
  1715x972 mapped, 7039 distinct colours (not a blank frame). The workspace with
  sidebar / tab strip / centre pane / right panel / status bar renders and is
  navigable. Settings route opens via the status-bar gear and shows its own
  header + Back control (see F-SET-01).
- Build health: PASSED with 2 warnings (tiller_usage dead mut, tiller_terminal
  private-type). No test suite run as part of this pass.
- F-WIN-10 (toasts): NOT OBSERVED — no error/transient operation was triggered in
  this session; left unticked.
- F-WIN-08/F-WIN-09/F-WIN-12 (hide-on-close, title-bar pref, first-run permissions):
  N/A platform (macOS window/permissions semantics).

## F-CHAT — chat surface over ACP (the hardest test)

- F-CHAT-01 (transcript with user bubbles, agent markdown, turn timing): PASSED.
  The launch layout includes a Chat tab. Sending "Write a file named
  critic-check.txt ..." rendered a user bubble; the agent's response streamed in
  under it; a second and third turn worked the same way.
- F-CHAT-04 (Return sends, Shift+Return newline): PASSED for Return — pressing
  Enter in the focused composer sent the message and cleared the composer.
  Shift+Return not separately exercised.
- F-CHAT-07 (Stop / Escape): NOT EXERCISED this pass.
- F-CHAT-16/17 (model picker, effort): PARTIAL. The composer model pill rendered
  the ACP-provided model ("Claude Opus (1M context)") from the agent's model
  catalog — the picker menu itself was not opened.
- F-CHAT-23 (tool-call cards with output, pending/completed): PASSED. The Write
  tool call rendered as a card ("Write critic-check.txt") with Pending → Completed
  state after approval.
- F-CHAT-24/25 (permission plan/question cards): PASSED for permission requests.
  The Write call produced a rendered "Permission requested" card with Deny /
  Allow Once / Always Allow options; clicking Always Allow rendered
  "Answered: Always Allow" and the agent proceeded to write the file.
- F-CHAT-37 (empty new chat with usable composer): PASSED — composer shows
  "Message…" placeholder, attach (+) and send controls present, transcript empty
  until first turn.

The full chain, exercised with my own hands:
1. App launch spawns the ACP chain: tiller → `npm exec @agentclientprotocol/
   claude-agent-acp@latest` → `node .../claude-agent-acp` → the real Claude Code
   binary (`claude --output-format stream-json ... --session-id=fd685fd7-…`).
2. Prompt typed into the composer → user bubble → streamed agent chunks rendered
   in the transcript while the turn ran.
3. Agent tool call rendered as a card; permission options rendered; clicking
   Always Allow answered the ACP permission request.
4. The real agent wrote `/tmp/tiller-chat-demo-…/critic-check.txt` on this
   machine's filesystem, containing exactly the line the composer had contained
   (`---7391`; my input tool dropped uppercase glyphs, the app faithfully relayed
   what the composer held — confirmed by the agent's own transcript at
   `~/.claude/projects/-tmp-tiller-chat-demo-…/fd685fd7-….jsonl`, which records the
   same string).
5. Third turn: "say only the phrase acp-stream-ok-7391…" → agent response
   "acp-stream-ok-7391" rendered in the transcript.

Caveat observed: one Enter press in the composer failed to send (text stayed in
the composer); pressing Enter again sent it. Likely an input-timing artifact of
synthetic events, but recorded as seen.

## F-TERM — terminal

- F-TERM-01 (live terminal prompt): PASSED. The Terminal tab renders a real login
  shell: `tiller (pid) → /bin/bash -il` on `pts/11` (verified in the process
  tree), prompt visible, breadcrumb "in bash at HH:MM:SS" above the pane.
- F-TERM (input → execution → output): PASSED. Typing `touch
  /tmp/term-side-effect-test` into the pane created that file on the host
  filesystem — the PTY, shell, and command execution path all work end to end.
  (Rendered terminal glyphs are too small for my OCR; evidence is the side
  effect, not the pixels.)
- F-TERM-03 (exit statuses rendered per command): NOT OBSERVED in this shell —
  no per-command exit-status surface was seen; unticked.
- F-TERM-10 (panes alive across sidebar selection): NOT EXERCISED yet (single
  worktree in snapshot).
- F-TERM-PTY-01/02 (forkpty-equivalent, exit codes): PARTIAL PASS via the shell
  above; explicit exit-code display path not exercised.

## F-SET — settings

- F-SET-01 (categories): PASSED. Gear in the status bar opens Settings; the
  category list shows AI Providers, Agents, General, Permissions, Theme,
  Appearance; each one I clicked switched the detail view (AI Providers, Agents,
  General, Appearance all rendered and OCR-verified).
- F-SET-02 (Back / Escape): PARTIAL — the Back control is rendered in the header;
  click not exercised before the crash below.
- F-SET-03 (version + updates): PARTIAL. General shows "About / Version 0.1.0"
  and a "Check for Updates" button; clicking it was not completed before the
  crash.
- F-SET-04 (resume-agent-sessions toggle): PASSED visually — the toggle flips.
  FAILED for persistence: the `setting` table in the session DB remained empty
  (settings.rs documents itself as a fixture model; toggles are in-memory only).
- F-SET-10/11 (provider usage visibility/status): PARTIAL. AI Providers shows a
  Claude Code card with Status "Active" (real account identity "Lestat" read from
  this machine), Show-in-usage-bar toggle, Refresh interval stepper (5 min),
  Refresh now button, Accounts section ("System default / This device", "Use
  your current CLI login on this device", Add Account).
- F-SET-16 (agent registry): PASSED. Agents category renders Search agents, rows
  for omp (Oh My Pi), Claude Code, Codex, OpenCode, Pi, each with "Built-in: uses
  the X binary on your PATH." and an Available badge. Note: omp and opencode are
  NOT on this machine's PATH, yet both render "Available" — the badge appears to
  mean "built-in adapter exists", not "binary found".
- F-SET-19/20/21/22 (appearance: theme, translucency, fonts, icons, agent
  colours): PARTIAL. Appearance renders Interface font size 13pt, Terminal font
  size 13pt, Files icon theme, and Agent Colors rows for Claude Code/Codex/
  OpenCode/Pi/Oh-My-Pi with colour swatches; none were changed in this pass.
- FAILED (crash): the app process exited silently twice, both times while in
  Settings — once after General toggles + opening AI Providers, once on clicking
  the Theme category after viewing AI Providers. No panic, no error line in the
  log (only the radv Vulkan warning). This is a real stability gap in the
  settings surface.
- F-SET-23/24/25 (permissions): N/A platform (TCC/System Settings are macOS).

## F-USE — status bar / usage

- F-USE-01 (bottom bar with gear/refresh/worktree info): PASSED. Status bar
  renders settings gear, refresh button, three provider segments, and
  "HEAD - ~/Scrivania/Progetti/_tiller-critic-snap" worktree info at the right.
- F-USE-02/03 (provider segments with real data): PASSED for rendering. Segments
  show real fetched usage: Claude "—" (no CLI session), Codex "67% 5h", OpenCode
  Go "—" — Codex numbers parsed from this machine's real credentials
  (`~/.codex/auth.json`), i.e. the tiller_usage fetchers run for real. Stale/
  logged-out/error states not separately exercised.

## Persistence (initial observations)

- F-PER-01/02 (projects/worktrees/tabs persist): PARTIAL PASS. The session DB
  (TILLER_DB) is created at launch with schema v3 (project, worktree, tab,
  setting, sidebar_state, sidebar_expanded_project tables). Rows are written:
  one project `_tiller-critic-snap`, one worktree (branch "main"), two tabs
  (Chat, Terminal) with the active flag tracking the selected tab. Relaunch
  restores the workspace from this DB.
- FAILED for scrollback: the `tab` schema holds title/kind/active only — no
  scrollback column exists in this build, so pane scrollback does not persist
  across relaunch (F-PER-01's scrollback clause).
- F-PER-06 (flush on quit + child termination): NOT EXERCISED (no clean quit path
  driven; both session ends were the crashes above).

## Control socket / automation## Control socket / automation (F-AUTO-*, F-CTRL-*) — re-verified 02:30

Two distinct verdicts, because two distinct builds were exercised:

1. THE SNAPSHOT BUILD (this tree — the one being judged): still FAILED.
   `crates/tiller/src/main.rs` contains zero references to ControlServer, the
   tiller crate has no tiller_control dependency, and a snapshot-built app
   launched with TILLER_SOCKET=/tmp/snap-critic.sock creates NO socket file
   (checked 9 s after launch). A snapshot-built tillerctl can therefore never
   reach it. The tiller_control crate in the snapshot now ships a tillerctl
   client binary (built: rust/target/debug/tillerctl, commands ping,
   capabilities, identify, list-workspaces, new-workspace, select-workspace,
   current-workspace, close-workspace, list-notifications,
   clear-notifications, notify, session-ref) but no server side is wired into
   the snapshot app. => In the snapshot, F-CTRL-WIRE-02/04, F-CTRL-PANEL-*,
   F-CTRL-SYS-01/02, F-CTRL-WORK-*, F-CTRL-NOTIFY-*, F-AUTO-* all FAILED
   (absent), not merely untested.

2. THE LIVE WORKTREE BUILD (tiller-linux, builders' current code — reported
   for calibration only, NOT part of the snapshot verdict): the socket works.
   I exercised the live socket at /run/user/1000/TillerRust/control.sock with
   the snapshot-built tillerctl:
     tillerctl ping --socket …            -> pong
     tillerctl capabilities --socket …    -> system.ping, system.capabilities,
                                             system.identify, workspace.list,
                                             workspace.current, notify
     tillerctl identify --socket …        -> tiller-linux  linux/gpui-waku
                                             /home/enzopalmisano/Scrivania/Progetti/tiller-linux
                                             p-c6ce0efaea1b05ec-wt-1
     tillerctl list-workspaces --socket … -> 2 rows (tiller @ rust/gpui-rewrite,
                                             tiller-linux @ linux/gpui-waku, selected)
     tillerctl current-workspace …        -> the selected worktree row
   Six methods genuinely served; everything outside that list remains absent
   in the live build too (no panel.*, no workspace.create/select/close wiring,
   no notification store beyond the method row). F-CTRL-SYS-01 PASSED against
   the live build; F-CTRL-WORK-02/04 PARTIAL (list/current served, create/
   select/close absent); the rest of F-CTRL/F-AUTO still FAILED.

3. tillerctl CLI wrinkle (observed): its own usage text says
   `tillerctl [--socket <path>] <command>`, but the parser only accepts
   options AFTER the subcommand — `tillerctl --socket X ping` fails with
   "unknown command '--socket'" while `tillerctl ping --socket X` works.


## F-SID — sidebar, projects, worktrees — APPENDED 02:50 (live build on isolated :2)

Setup: live-worktree binary (tiller-linux, read-only), fresh TILLER_DB, dedicated
Xwayland :2 with no other agents, TILLER_SOCKET=/tmp/critic-live.sock as the
oracle. Window identified by XID, origin resolved each move, clicks verified by
pointer-window checks. IMPORTANT: earlier sidebar findings from the shared :1
display (dead rows, dead filter, spontaneous settings) are RETRACTED — multiple
instances stack at one position there and clicks/imports cross windows. All
sidebar claims below are from the isolated display.

- F-SID-01 (Projects header, list, Add control): PASSED. "Projects" header,
  "+" control, project row "tiller-linux" all render.
- F-SID-02 (filter): PASSED. Typing "waku" into the focused filter left only
  the linux/gpui-waku row (project + its path); Backspace clearing restored
  all rows (rust/gpui-rewrite, tab rows, New Worktree…).
- F-SID-03 (add project from sidebar): FAILED. Clicking "+" produces no
  dialog, no picker, no window (measured: 9x9-px hover diff; second click
  0-px diff). The Add Project flow is not reachable in this build.
- F-SID-04 (expand/collapse): PASSED. Chevron click hides all children
  (rows measured 24,24,24 canvas afterwards), second click restores them.
- F-SID-05 (select row changes surface): FAILED. Clicking a worktree row
  moves the local highlight (measured 37,37,37 selection fill moving between
  rows) but the app's selected workspace does not change: after clicking
  rust/gpui-rewrite, `tillerctl current-workspace` still reports
  linux/gpui-waku. Sidebar selection is cosmetic-only.
- F-SID-11 (worktree row identity/status): FAILED for completeness — rows
  render branch + path + selection, but the row model (SidebarWorktree) has
  only branch/path; primary pill, comment, and agent-status fields from the
  Swift contract are not representable in this build.
- F-SID-13 (New Worktree from project row): PARTIAL. The "New Worktree…" row
  opens the branch-name prompt ("New worktree in tiller-linux"); Escape
  cancels it cleanly. Creation itself NOT EXERCISED (would write a worktree
  into the read-only live repo).
- F-SID-16/17 (drag reorder): FAILED (absent) — no drag handlers exist in
  sidebar.rs (grepped: zero on_drag); reordering cannot be attempted.
- F-SID-06/07/08/09/10/12/14/15/18/19: NOT EXERCISED this batch (context
  menu and gear paths checked in the next batch).

UI-vs-socket contradiction (measured defect):
  sidebar highlight after launch  -> rust/gpui-rewrite row (srgb(37,37,37))
  socket current-workspace        -> linux/gpui-waku (p-c6ce0efaea1b05ec-wt-1)
  status bar right side           -> "linux/gpui-waku - ~/Scrivania/Progetti/tiller-linux"
  terminal shell cwd (/proc)      -> /home/enzopalmisano/Scrivania/Progetti/tiller-linux
The status bar, the socket, and the real shell agree; the sidebar initially
highlights the WRONG worktree and attaches the open tabs (Chat, Terminal) under
the wrong worktree row. Root cause visible in the code: sidebar rows are built
with `selected: index == 0` hardcoded, and MAIN_WORKTREE_SIDEBAR_ID assumes the
running worktree is catalog index 0 — false when the app runs from a linked
worktree (the main repo's worktree sorts first).

Internal ID-scheme mismatch (measured):
  DB sidebar_state.selected_worktree_id = w-c6ce0efaea1b05ec
  DB worktree.id                        = w-c6ce0efaea1b05ec (only ONE row persisted)
  socket workspace ids                  = p-c6ce0efaea1b05ec-wt-0 / p-…-wt-1
The persistence layer stores only the running worktree under one id scheme;
the catalog/socket uses another. Restoring a selection across relaunch cannot
match these schemes.

## Control socket — pane methods RE-VERIFIED 02:55 (supersedes 02:30 section for panels)

Setup: my own instance of the CURRENT live-worktree binary on isolated :2,
fresh TILLER_DB, TILLER_SOCKET=/tmp/critic-ctl.sock. All requests raw JSON over
the socket ({"id","method","params"}), read with a Python client.

- system.ping: PASSED -> {"status":"ok"}
- system.capabilities: PASSED -> 19 methods advertised (system.ping/capabilities/
  identify, workspace.list/current, notify, panel.create/split/list/write/key/
  read/wait/focus/close, notification.create/list/clear, session.ref)
- system.identify: PASSED -> project tiller-linux, branch linux/gpui-waku, path
  /home/enzopalmisano/Scrivania/Progetti/tiller-linux, workspaceId p-…-wt-1 —
  agrees with the status bar and the terminal shell's /proc cwd.
- workspace.list: PASSED -> two worktrees, selected=true only on linux/gpui-waku.
- panel.create: PASSED -> pane id returned; with cmd "sleep 0.4; exit 7".
- panel.write: PASSED -> "echo CRIT-8421-ROUNDTRIP" into the pane; panel.read
  returned base64 of the raw terminal stream; decoded + ANSI-stripped it
  contains the echoed nonce "CRIT-8421-ROUNDTRIP" (verified in Python).
- panel.read: PASSED (see above; also returns the prompt decoration stream).
- panel.wait: PASSED for exit codes -> wait on the "exit 7" pane returned
  {"exitCode":"7"}, twice. FAILED for the timeout clause: panel.wait with
  params timeout=1000 on a still-running shell never answered (client timed
  out at 15 s) — the timeout parameter is not honoured.
- panel.split: PASSED -> split from pane A direction right -> new pane titled
  "Split (right)"; writing "printf SPLIT-OK-771" into it and reading it back
  found the nonce in the split pane's own stream.
- panel.key / panel.focus: PASSED (ok on a live pane; clean "unknown pane"
  errors on bad ids).
- panel.close: PASSED (ok; afterwards panel.read and list report the pane gone).
- panel.list DEFECT (filed with codex12): returns ONLY socket-created panes
  (tab:"control"). While the app itself has Chat and Terminal tabs open, the
  list showed just the control panes — the automation surface cannot see the
  app's own panes. FAILED for F-CTRL-PANEL-03's "compare rows with visible
  panes".
- notification.create/list/clear: PASSED (title/body stored with a date,
  cleared afterwards).
- session.ref: PASSED (accepted; ref persisted to the instance's temp DB).
- notify agent mode: PASSED -> {"queued":"true"} for
  {session, status: needs-input}.
- notify user mode: FAILED -> {title, body} without session answers
  "notify requires session" — the user-notification path is not implemented.
- workspace.create/select/close: NOT EXERCISED (create/close write worktrees
  into the read-only live repo; select would move the builders' selection).
- ControlSocket socket file mode: 0600 (srw-------) as specified.
- tillerctl CLI: panel.* methods have no subcommands; only the raw socket
  speaks them. tillerctl's usage text claims --socket precedes the command but
  the parser only accepts options after it (verified: "--socket X ping" ->
  "unknown command '--socket'", "ping --socket X" -> pong).

## F-CHG — files, changes, diff, activity — APPENDED 03:00 (live build, isolated :2)

Ground truth for all counts: read-only `git status` in the tiller-linux
checkout (branch linux/gpui-waku).

- F-CHG-01 (right panel Files/Changes switch): PASSED. Files renders the real
  tree (github/, tiller/, app/, AppTests/, assets/, docs/, output/, packages/,
  plans/, reference/, rust/…); clicking the Changes segment switched the panel
  to the changes list.
- F-CHG-03/04 (file tree browse, expand/collapse, select): PARTIAL. Tree rows
  render with disclosure markers; directories expand on click (code path);
  file click opens an editor tab (open_file -> RightPanelEvent::OpenFile).
  Expand/click not individually measured this batch.
- F-CHG-07/08 (sections and counts): the panel shows a single header
  "Local changes (94)" with Stage all / Discard all. The count 94 EXACTLY
  matches `git status --porcelain -uall | wc -l` (25 modified + 69 untracked
  files, expanded) — the UI count agrees with git ground truth. FAILED for
  the section split: no separate Staged / Changes / Untracked sections exist
  in this build (F-CHG-08).
- F-CHG-12 (expand a changed file to show its diff): PASSED. Clicking
  "Scripts/linux-drive.sh" expanded an inline diff.
- F-CHG-17 (numbered old/new lines + hunk headers): PASSED. The expanded diff
  rendered "@@ -0,0 +1,145 @@" and numbered added lines ("1 #!/usr/bin/env
  bash", "2+ #Launch the app, DRIVE it, …"), matching the file's real content.
- F-CHG-10/11/14 (stage/unstage/discard): buttons render on hover (Stage,
  Discard) but NOT EXERCISED — they write to the live repo, which is
  read-only for this critic.
- F-CHG-13 (open file / open diff from a changed-file row): FAILED (absent) —
  the Changes rows expose only stage/discard; there are no open-file/open-diff
  actions.
- F-CHG-02/05/09/15/16/18: NOT EXERCISED (need an empty/error state, keyboard
  focus, binary files, conflicts, or drags not reachable in this setup).
- F-CHG-19/21/22 (Activity section): PASSED for rendering — rows for Chat and
  Terminal with subtitles ("tiller-linux / linux/gpui-waku") and status
  glyphs; clicking the Terminal row switched the active tab (DB tab.is_active
  moved to Terminal, 19,440-px frame change). The hover close-X control
  rendered on the row but two click attempts did not close the tab — recorded
  as unverified, not failed.
- F-CHG-20 (running count): NOT EXERCISED (no active agent during this batch).

## Third silent crash — isolated display, hands-off (03:01)

The live-build instance on the isolated :2 display died silently ~5 minutes
after launch (process gone, no panic line in the log, display left empty).
This is the third such death: twice before on :1 with the snapshot build
(settings area), now on :2 with the live build and NO input for the minute
before death. The timing is consistent with the status-bar usage refresh
cycle (first fetch at launch, next at +300 s) and the Claude usage fetcher
spawning a hidden claude PTY. A hands-off watchdog run is in progress to
confirm the ~5-minute correlation.

## TRANSPLANTED CODE (checked 03:05)

Method: compared every Rust source in the tiller-linux tree against the two
reference checkouts in /home/enzopalmisano/Scrivania/Progetti/_tiller-refs
(waku = the closest architectural reference, a single-tree GPUI app with the
same surface map; zed = the GPUI upstream, 242 crates; orca/t3code = TypeScript
apps).

1. File-name and hash comparison: no file in tiller is byte-identical to any
   reference file. Shared basenames exist (main.rs, sidebar.rs, settings.rs,
   right_panel.rs, terminal.rs, claude.rs, codex.rs, opencode.rs, pi.rs,
   worktree.rs) but every pair differs on essentially every line.
2. Substantive-line overlap (lines >= 24 chars, comments stripped), tiller vs
   waku: sidebar 44/870 (5%), right_panel 55/720 (7.6%), settings 36/631
   (5.7%), terminal 29/538 (5.4%), main 3/1228. Inspection of the shared lines
   shows only GPUI/Rust idiom (`.flex()`, `div()`, `})`, `cx.notify()`,
   `.child(`) — no block of two or more consecutive substantive lines is
   shared anywhere.
3. tiller vs zed internal crates (acp_thread/acp_tools for tiller_acp,
   editor/element for the terminal, etc.): best match 7 boilerplate lines
   (`use std::collections::HashMap;`, a fmt impl signature, closing braces) —
   zero algorithmic overlap. Tiller uses zed's GPUI as a git-pinned
   DEPENDENCY, which is the intended relationship, not copying.
4. orca/t3code: TypeScript/Electron codebases; cross-language wholesale
   copying is not a vector, and no translated blocks were spotted in the
   files inspected.

Conclusion: none found. The tree shares its architecture map and feature
surface with waku (as the rewrite brief intends) but the implementations I
sampled and diffed are independently written. This check covered every
tiller_* crate file against waku/src and a 2000-file sample of zed/crates;
it is a similarity screen, not a formal proof of authorship.

## CORRECTIONS AND FINAL BATCH — APPENDED 03:20 (isolated :2, live build)

RETRACTIONS (all three were contamination from stacked instances on :1,
where clicks and captures crossed windows; re-verified on the isolated :2):
- "Back button is dead" (F-SET-02): RETRACTED. On :2 the gear opens Settings
  (285,221-px change) and Back returns to the workspace (280,194-px change).
  F-SET-02: PASSED for Back. Escape: still not bound anywhere (no bind_keys in
  settings.rs) — the Escape half of the VERIFY line is absent.
- "Sidebar rows are click-dead": RETRACTED. On :2 the project chevron toggles
  (measured 7x7-px glyph flip), worktree rows move the selection highlight
  (measured 37,37,37 <-> 24,24,24), the filter field accepts text, and the
  tab rows switch the active tab (DB is_active moved, 153,536-px change).
- "App spontaneously entered Settings": RETRACTED (a foreign instance's
  window was being captured).

New F-TAB/F-WIN findings (live build, :2):
- F-TAB-19 (Ctrl-Tab / Ctrl-Shift-Tab): FAILED (absent) — both produced 0-px
  change; no such binding exists in the live build (grep: only cmd-alt-*).
- F-TAB-20 (Cmd-1..9): FAILED (absent) — no numeric tab bindings.
- F-TAB-10/23 via keyboard (Cmd-Alt-Shift-Right/Down splits): FAILED as
  exercised — super/ctrl/alt variants of the split chords produced no split
  (no 1-px divider measurable at the expected 50% positions, 0-px diffs),
  even though the binding exists in source. Menu-driven "Split Claude Code"
  not exercised.
- F-WIN-04/05 (Ctrl-Cmd-S sidebar, Ctrl-Cmd-I right panel): FAILED (absent) —
  no such bindings exist in the build.

## CRASH LOG — five silent deaths (closing evidence)

1. snapshot build, :1, ~13 min, during settings interactions (General toggles
   + AI Providers). 2. snapshot build, :1, ~4 min, after AI Providers view.
   3. live build, isolated :2, ~5 min, workspace/Changes, no input for the
   last minute. 4. live build, isolated :2, ~8 min, immediately at a
   super+alt+shift+Right keypress. 5. live build, isolated :2, ~4 min,
   ~1.5 s after clicking the settings gear.
Every death: process exits, log contains only the radv Vulkan warning, no
panic, no backtrace (RUST_BACKTRACE=1 was set on several runs), display left
empty. A hands-off watchdog run survived 5m47s, ruling out a deterministic
5-minute-cycle death; the trigger is a race, not a timer. Settings-adjacent
interaction is over-represented (3/5) but not exclusive. This is the
reliability finding of the pass.

## THE SINGLE BIGGEST GAP

The silent crash. The app kills itself without a word — five times across
both builds and both displays, roughly every 4-13 minutes of use, with no
panic message, no log line, and no deterministic repro (a hands-off run
survived the 5-minute refresh cycle, so it is a race, not a timer). Every
other defect in this report is survivable: the wrong sidebar highlight can
be re-clicked, the absent keyboard shortcuts can be added, the missing
sections can be built. A process that dies silently mid-session kills the
agents it hosts (the ACP claude, the panes' shells) and loses everything the
debounced persistence has not yet flushed — that alone disqualifies the app
as a home for running agents, which is Tiller's entire reason to exist.
Until the crash is found and fixed, no other entry in either inventory
matters.

## PASS 2 — tabs, panes, documents, browser, window shell, persistence, git — 09:04–10:10

**Snapshot.** Whole tree `cp -a`'d to `/tmp/critic-pass2-1786604669` at 09:04:30
(CEST); git resolves in the copy (`gitdir:` pointer), branch `linux/gpui-waku`,
39 modified files. Forced rebuild (`touch` + `cargo build -p tiller -p
tiller_control`) finished 09:09, 0 errors, 8.43s. The snapshot builds.

**Instrument & display truth.** Test repo `/tmp/critic-gitrepo` (3 commits,
branch `feature-x` + linked worktree `/tmp/critic-gitrepo-wt`, staged
README.md, unstaged src/app.rs + src/bigfile.txt with a 200-line unchanged
middle, untracked docs/newfile.txt) added as the app's project via cwd —
both worktrees surfaced in `tillerctl list-workspaces`. Isolated `Xwayland :2`.
Three display hazards consumed most of the pass, in order: (1) the output mode
flaps back to 640x480, re-centring the window at -413,-177 (all early "layout"
probes were of the window's bottom-right corner); (2) the app's presentation
freezes ~4–13 min after launch — process and control socket stay alive, the
window pixmap stops changing (observed on app2@~09:20, app5@~09:39, app8@
~09:52, app10@~10:06; app10 verified frozen: 0 px over 2 s while `ping`
pongs). This is the silent crash's visible form and the strongest crash-hunt
clue in this report. (3) from ~09:50 the builders' app windows overlap :2 at
the same geometry; root crops and XTEST clicks at overlapping positions land
on *their* build — verified two distinct 1470x833 windows, 832,705 px apart.
All later captures use `import -window <own-id>`; several "clicks do nothing"
results earlier in this pass are attributed to this overlap, not the app.
A root-crop-only frame is not evidence. ~30% brightness frames (theme coral
reading as dark brown) suggest a radv software-Vulkan present issue; treat
colour fidelity on :2 as unreliable, structure as readable.

**F-TAB (28).** PASSED: F-TAB-01 (strip renders with chips; active chip gets
raised bg + 2 px coral top bar; icons/titles render — Chat icon is coral per
code and observed; dirty/status dots unverifiable without OCR), F-TAB-10
(split right AND down via `super+alt+shift+Right/Down` — new panes appear in
`panel list` and on screen; on Linux `cmd` = Super, not Ctrl), F-TAB-15 (clean
tab close via the active chip's ✕ — clicking it removed the Terminal tab:
560,222 px changed and the pane left `panel list`), F-TAB-22 (pane focus via
`super+alt+arrows` — active pane flag moved pane-3→pane-2), F-TAB-23 (pane
direction creation — same chords as F-TAB-10). FAILED (capability absent):
F-TAB-02 (no overflow/All-Tabs control in any frame), F-TAB-03/04 (+ menu
never renders; clicks at the reserved 26 px strip slot open nothing — the
TabBar plus button is invisible), F-TAB-05/06/07/08 (the New Tab menu that
would hold them does not exist), F-TAB-09 (no pane menu / Open File), F-TAB-12
/13 (no move-tab UI), F-TAB-17 (right-click on a chip = 0 px, no context
menu), F-TAB-19 (Ctrl-Tab cycle: no binding anywhere in the code, and the
keystroke changes nothing), F-TAB-20 (Ctrl-1..9: no binding), F-TAB-21 (no
Tab menu), F-TAB-28 (Ctrl-W / Super-W: no binding; only `cmd-alt-w` ClosePane
exists and it did not close a tab). NOT EXERCISED: F-TAB-11 (split-disabled
reason needs the missing menu), F-TAB-14 (rename), F-TAB-16 (dirty close
prompt needs a dirty document), F-TAB-18 (drag reorder), F-TAB-24 (drag
escape), F-TAB-25 (attach-to-terminal), F-TAB-26 (context-menu close), F-TAB-27
(resume chat). The strip itself was verified interactive in the clean app2
window (chip click moved the active tab; ✕ closed it) before the overlap era.

**F-CHG (22).** F-CHG-01 FAILED: the right panel is Files + Activity only; the
Files/Changes segment that pass 1 saw (03:00) is gone — this build's
`right_panel.rs` has no Changes surface. F-CHG-03/04 PARTIAL: the file tree
renders ~10 rows (measured), but click/expand could not be verified under the
overlap/stale-frame conditions. F-CHG-05 NOT EXERCISED. F-CHG-06 NOT
EXERCISED (icons unreadable). F-CHG-07 through F-CHG-18 FAILED (absent): the
entire Changes surface — staged/changed/untracked sections, counts, stage/
unstage/discard, diff expansion, open/discard/conflict actions, numbered
diff lines — is NOT wired into the app. `changes.rs` exists as a component
plus a dev harness (`changes_preview`); the harness builds, renders, and
expands inline diffs on my scratch repo (green/red diff pixels appear on row
click), but the app never mounts it. F-CHG-19/20/21/22 PARTIAL: the Activity
section renders in the right panel (rows + status subtitles, pass 1 verified
row-click switching); running-count states not reachable this pass.

**F-EDIT (13).** No path to open a document could be exercised: no ⌘O/⌘S
bindings (code search), file-tree clicks landed unreliably, and the New Tab
menu is absent. `file_view.rs` exists (standalone file tab component). All 13
entries are FAILED/NOT EXERCISED on evidence — I could not make any editor
appear. F-EDIT-09/10/11 (double-click open, context menu, copy path) not
exercised; F-EDIT-01..08, 12, 13 not exercised (no editor reachable).

**F-PER (13).** F-PER-01 PARTIAL: tabs + active tab + worktrees restore across
two clean SIGTERM quit/relaunch cycles (verified: [Chat active, Terminal] and
both worktrees return; `tillerctl panel list` identical after restart). Chat
tabs restore as fresh identities (no transcript) and terminal scrollback is
explicitly dropped — the session code says scrollback "is expected to be
gone". F-PER-02 FAILED: sidebar selection does not persist — `sidebar_state`
in the DB is unchanged after selection clicks, and the code (main.rs
`SidebarEvent::SelectWorktree`) only re-highlights the sidebar row; it never
updates `working_directory`, the control state, or the DB (the P9 handoff's
"shell wiring" is still missing). F-PER-03 PARTIAL (worktrees restore;
layouts untested). F-PER-04 FAILED (scrollback not reattached, by design).
F-PER-05 FAILED (`session.restore` = "not implemented by the Linux shell").
F-PER-06 PARTIAL: SIGTERM quit flushed the session (2 verified restarts) and
no pane child processes survived the killed apps (no stray sleep/sh found).
F-PER-07 NOT EXERCISED (settings UI unreachable; `setting` table empty).
F-PER-08 N/A (no browser).

**F-WIN (12).** F-WIN-01 FAILED (no ⌘, binding; the status-bar gear click was
inconclusive — only a 291x32 px sidebar-band change, not a settings screen).
F-WIN-02 FAILED (no ⌘T). F-WIN-03 FAILED (no ⌘O/⌘S). F-WIN-04/05 FAILED (no
⌃⌘S/⌃⌘I bindings). F-WIN-06 N/A (no browser). F-WIN-07 FAILED (History/
restore unimplemented). F-WIN-08 N/A — platform (macOS hide-on-close
delegate; no Linux equivalent exists). F-WIN-09 N/A — platform (macOS
titlebar preference). F-WIN-10 NOT EXERCISED (no toast observed; machinery
unverified). F-WIN-11 N/A — platform (Sparkle updater; no Linux update
mechanism in this build). F-WIN-12 N/A — platform (TCC onboarding).

**F-BRW (9).** FAILED, plainly: there is no browser in this Linux build. No
`browser.*` methods in `system.capabilities`, no browser code path
(TabKind::Browser and Diff are `unreachable!()`), and the New Browser menu
item is unreachable. F-BRW-01..09 all FAILED (absent); F-CTRL-BROWSER-01..06
are likewise absent (the dispatcher accepts no browser method).

**Git-dependent entries (previously UNREACHABLE).** F-GIT-WT-01 PASSED via
the app's own discovery: `list-workspaces` returned my scratch repo's main
checkout AND the linked `/tmp/critic-gitrepo-wt` (branch wt-branch) — the
`git worktree list --porcelain` path works. F-CORE-DOM-01 PASSED (project +
both worktrees round-trip the DB across restart). F-CHG-07/08 FAILED (surface
absent). F-CTRL-WORK-02/04 list/current PASSED; F-CTRL-WORK-03/04/05
create/select/close FAILED ("not implemented by the Linux shell").
F-CTRL-SESSION-02 FAILED (restore unimplemented). F-CTRL-NOTIFY-01/02 PASSED
(agent + user notify, stdin-JSON agent-session extraction, invalid-status
rejection). F-CTRL-SESSION-01 FAILED for persistence: `session.ref` stores
only in memory (no DB table), verified — the reference survives one run but
not a restart. F-CTRL-PANEL-01..09 PASSED end to end on the socket: create
(with cmd), split (right/down/invalid), list, write (+--enter), key (all 9
symbolic keys mapped: CR/TAB/ESC/BS/CSI-3~//A//B//C//D — byte-verified with
`od`), read (base64, exact bytes), wait (exit 7, exit 0, SIGTERM→143,
timeout), focus, close. F-CTRL-SYS-01/02 PASSED (ping, capabilities,
identify). Package unit tests: 187 tests across tiller_control/project/
theme/terminal/activity/ui/persistence, all green.

**Findings that matter most.** (1) The Changes surface disappeared from the
app since pass 1 — the git worktree's entire stage/diff/discard UI is gone,
though the component and its harness survive. (2) Sidebar selection is
still cosmetic: it changes only the sidebar's own pixels and never the
centre, the control state, or the DB. (3) The presentation freeze is the
silent crash: process and socket alive, window frozen, recurring at 4–13 min
across five instances today — hunt the render/frame loop, not the process.
(4) No tab-level keyboard shortcuts exist at all; on Linux `cmd` is Super.
(5) The + new-tab button does not render; the only way to add a tab today is
a pre-seeded session DB. (6) `session.ref` is in-memory only.

## PASS 3 — re-verification, persistence defect, automation, freeze, F-EDIT — 10:11–12:10

**Snapshot.** Whole tree rsync'd to `/tmp/critic-pass3-1786608801` at **10:11:55–10:13:13 CEST**
(no `.git` in the copy; 30 rust files modified at copy time, +5567/−1902). Forced rebuild
`cargo build -p tiller -p tiller_control`: **5.30 s, 0 errors, 10 warnings** (all pre-existing).
`cargo test -p tiller_ui`: 45 green. The snapshot's `cargo test -p tiller` does **not** compile:
the P21 mount guard `restore_mounts_a_changes_surface_in_the_application_shell` references
`TabContent::Changes`, which the snapshot's `PaneContent` enum lacks — the snapshot caught the
P21 wiring mid-flight (guard written first, still red at 10:12).

**Live-tree movement (rule 2), checked before every finding.** At 10:11 the live tree differed
from the snapshot only in `main.rs` + `panes.rs` (P21 wiring). By 11:40 it had
`PaneContent::Changes`, "diff" restore arms, `NewTabAction::NewChanges` + `add_changes_tab` + the
menu item; `cargo test -p tiller` = **47 green incl. the mount guard**. `surface.changes.*` /
`surface.settings.*` entered the capabilities list and dispatcher at ~11:56–11:58; `main.rs` was
still being edited at 11:58. Every verdict below is stamped against the snapshot build unless it
says live.

**Display.** `:2` wedged ~10:20 (Xwayland alive in ep_poll; new clients block forever). The
operator killed two hung X clients and the app instance on it; the display did not recover. No
display on this machine presents, new ones cannot. Per operator ruling, **anything that must be
SEEN is NOT EXERCISED — blocked on display**; nothing was ticked from reading code. All socket,
state and persistence work below ran headless (the app serves its control socket with no display).

### 1. The three headline findings, re-verified

**Sidebar selection (F-PER-02) — RETRACTED, the fix is real; it ships a new persistence defect.**
Socket side, headless: `select-workspace wt-1` flips `current-workspace`/`identify` wt-0→wt-1
instantly, and back. Persistence: selection now writes the DB — after select,
`sidebar_state.selected_worktree_id` = the selected worktree id, and after a real TERM +
relaunch (new pid, fresh bind verified) `current-workspace` comes back on the selected worktree.
Pass-2's "sidebar_state unchanged" no longer holds. Pixel side (highlight, centre-follow):
NOT EXERCISED — blocked on display; the orchestrator's 09:43 screenshot is external confirmation,
mine is the socket + DB.

**Automation methods (P18) — all five landed, verified headless on the snapshot build.**
- `workspace.create` PASSED: created a real git worktree (branch `critic-p3-topic` at
  `/tmp/tiller-worktrees/…-2426431`), listed, persisted to the DB. A foreign path correctly
  answers "unknown project" (project must be in the catalog).
- `workspace.select` PASSED (above). `workspace.close` PASSED: clears the selected/mounted flag;
  re-`select` re-mounts.
- `session.ref` PASSED within-run; **pass-2's "in-memory only" finding STANDS**: no DB table
  (schema: project/setting/sidebar_expanded_project/sidebar_state/tab/worktree) and no readback
  method — a ref cannot survive restart by construction.
- `session.restore` PASSED: `{"restoredCount":"2","path":"/tmp/critic-gitrepo"}` and the snapshot
  worktree re-selected. F-AUTO-08's exact script (close a post-launch tab first) is unexercisable
  via the socket: `panel.close` on a restored tab fails **"unknown pane"** — the registry only
  closes control-created panes (intentional, see `set_external` comment), but the error hides why.
- `browser.open`/`browser.snapshot` PASSED: explicit `"… is unsupported on Linux: Tiller has no
  browser/webview surface in this build"`; none advertised in capabilities.
- `socketEnabled`/`socketPath` PASSED: capabilities returns both, live values (`true`, real path).
- `worktree.set` PASSED (comment+session echoed back); `notify` PASSED (user + agent status,
  invalid status rejected); `notification.create/list/clear` PASSED round trip (F-AUTO-06).

**Changes surface (P21) — CONFIRMED at snapshot time; the live tree now wires it.**
Snapshot (10:12): the only `ChangesTab` mount is still the demo binary; `PaneContent` has no
`Changes`; `TabKind::Diff` is `unreachable!`; the right panel is Files + Activity with no Changes
segment (F-CHG-01 as documented exists in neither build — the design moved Changes to a first-class
tab). Live (11:40): `Changes(Entity<ChangesTab>)`, "diff" restore arms in both `restore_tabs` and
`restore_tabs_in_workspace`, `NewTabAction::NewChanges` + menu item + `add_changes_tab`, mount
guard GREEN, 47 tiller tests green. Rendering/interaction: NOT EXERCISED — blocked on display.
`surface.changes.open/read`, `surface.settings.*` were entering the live dispatcher at ~11:58; the
running live binary at probe time was stale (answered "unknown control method" for `surface.*`),
and the live `tillerctl` binary predates `tab.*`/`pane.*`/`system.quit` — builders were mid-work.

### 2. NEW finding — every worktree duplicates in list-workspaces after selection + restart

**Reproduced on the snapshot build AND the live build, with a clean TERM, no crash:**
fresh DB → launch (cwd = repo) → `select-workspace wt-1` → quit → relaunch → `list-workspaces` =
**10 rows for 5 physical worktrees**, each once under project `critic-gitrepo` and once under
`critic-gitrepo-wt` (ids `p-db0a208c53356655-wt-*` and `p-f876a113e5cc063b-wt-*`). Selection still
restores correctly (current = wt-1).

Mechanism (code-verified): two writers, one table, two id conventions. `write_layout` (the
per-selection shell save) upserts a `w-{hash}` worktree + a `p-{hash}` **project row rooted at the
selected worktree's path**; `write_catalog` (called only on workspace create/close/add/remove —
**never on quit**) writes `p-{id}-wt-N` rows and removes projects not in the catalog. After a
selection change the DB holds both conventions, and `restore_catalog` re-runs git discovery under
*every* project row — a linked worktree's root is a git repo, so the same worktrees are discovered
again → duplicates. New since pass 2 because pass-2's selection never wrote the DB; the P9b fix is
what makes it reachable. The unit tests don't catch it (write/restore tests exercise one path each).
Second-order: a catalog save after a layout save deletes the `w-` row the selection id points at
(dangling `sidebar_state`), which restore resolves by path — works, but the id space is inconsistent.

### 3. The freeze — characterisation blocked on display, process side probed

Per operator ruling, the visual freeze cannot be reproduced or characterised without a display; I
did not guess. Two observations, honestly scoped. (a) The pass-2 freeze (0 px / 2 s, socket alive,
4–13 min) was measured on a display that was then presenting, so it stands as an observation whose
cause cannot be attributed today; my 10:18 boot capture of this build's window on `:2` was **1
colour from the first frame**, consistent with the display's import pipeline already failing
(STATE.md's compositor import errors predate the wedge), not with a 4–13 min app timer. The
operator's reading — the freeze and the wedge share a root cause outside the app — fits everything
I saw. (b) Process side (bounded headless probe, snapshot build, **15:55 elapsed**): `ping` ponged
throughout, `workspace.select` state changes applied instantly, the session DB was flushed, RSS
~75 MB / CPU ~1.2% flat. The process/state side stays healthy far past the pass-2 freeze window —
supports (does not prove) the presentation-layer framing, and contradicts nothing in pass-2.

### 4. F-EDIT (13 entries) — pass-2's unexercised block

No display → no clicks or keystrokes; anything pixel-required is NOT EXERCISED — blocked on
display. FAILED verdicts below are structural absences in the running snapshot build
(code-verified — the same evidence class pass-2 used for absent shortcut bindings):

- F-EDIT-01 FAILED — no Code/Preview modes: `FileView` is a read-only viewer (markdown renders;
  text renders as numbered monospace lines); there is no editor (no caret/input/selection/save).
- F-EDIT-02 FAILED — no formatting toolbar (bold/italic/headings/lists/links) anywhere.
- F-EDIT-03 FAILED — the documented "Large file — manual preview" state does not exist; a
  "too large to open" notice with a hardcoded 1 MiB limit (`MAX_FILE_BYTES`) is what the code
  shows (path header renders).
- F-EDIT-04 FAILED — no ⌘S (nothing to save; no cmd-s/mod-s binding anywhere).
- F-EDIT-05 FAILED — no Reload/Keep conflict banner (no external-change machinery).
- F-EDIT-06 FAILED — no save path, so no recreate-deleted-file behavior.
- F-EDIT-07 FAILED — code files render as plain numbered lines; no language detection, no
  no-wrap/4-space-indent editor behavior.
- F-EDIT-08 FAILED — no dedupe: `add_file_tab` unconditionally pushes a new tab (same path
  opened twice = two tabs).
- F-EDIT-09 NOT EXERCISED — blocked on display; the open path exists in code (file rows call
  `open_file` on click, directory rows toggle).
- F-EDIT-10/11/12 NOT EXERCISED — blocked on display (context menu, copy path, drag payloads).
- F-EDIT-13 FAILED — one `Unreadable` notice covers both missing and unreadable files; no distinct
  missing-file message (code-verified; the notice itself unviewable).

Other pass-2 NOT-EXERCISED leftovers, all still NOT EXERCISED — blocked on display:
F-TAB-11/14/16/18/24/25/26/27; F-CHG-03/04/05/06 (file tree) and F-CHG-19..22 (activity);
F-PER-07 (settings UI); F-WIN-01..05/07/10 (bindings/toast); F-AUTO-01 (settings toggle; the
capability side — socketEnabled/socketPath — is verified). F-CHG-07/08: the ChangesTab component
has 45 green crate tests and the live tree can now mount it; app-level observation blocked.

### 5. Minor observations

- The live `tiller`/`tillerctl` binaries were stale vs their source at probe time (tillerctl
  predates `tab.*`/`pane.*`/`system.quit`; tiller predates `surface.*`) — builders mid-work, not a
  finding.
- `tillerctl list-workspaces | head -3` panics with a Rust "Broken pipe" trace instead of exiting
  quietly (cosmetic).
- Live `tab.select index=99` and `pane.close <bogus id>` return ok — out-of-range and unknown ids
  are swallowed (minor protocol-truthfulness gap, P17 territory, in the new methods).

**Counts.** Exercised this pass (headless): F-AUTO-02..09 (panel create; panel list/close re-check;
notify + worktree.set + session.ref association; workspace list/current/select/create/close;
notification create/list/clear; ping/identify/capabilities; session.restore; browser.* error) —
**8 PASSED · 1 FAILED** (F-CTRL-SESSION-01 persistence: session.ref in-memory only)
**· 0 UNREACHABLE · 0 N/A**, plus F-CTRL-PANEL create/list/close PASSED and the sidebar-selection
re-verification PASSED — **12 PASSED · 1 FAILED · 0 UNREACHABLE · 0 N/A** among exercised
entries. Separately: the duplication finding FAILED on both builds, and 8 F-EDIT entries FAILED
by structural absence. **NOT EXERCISED — blocked on display: the entire visual tier**
(F-EDIT-09..12, F-TAB-11/14/16/18/24/25/26/27, F-CHG-03/04/05/06/07/08/19..22, F-PER-07,
F-WIN-01..05/07/10, F-AUTO-01, the Changes surface's pixels, the freeze's visual side).

**Biggest gap:** the display loss blocks the whole visual tier — final verification of the
newly-wired Changes surface, every UI affordance, and any further freeze characterisation cannot
happen until the compositor is restarted; until then the pass-3 verdicts are socket-deep and no
deeper, by design.

## PASS 4 — six pieces, headless — snapshot 12:13:56, exercised 12:15–12:40

Fresh-context critic; judged the six pieces built since pass 3 against a tree snapshot at
`/tmp/critic-pass4` (12:13:56). Live tree moved under me twice during the pass (see §6): findings
were re-checked against the live tree before filing, and the two that name live files still hold.

### 1. Changes surface — PASSED headless; the visible `+ → Changes` click stays display-blocked

Mounted as a first-class tab (`TabKind::Diff`, `add_changes_tab`); `surface.changes.open` calls the
same typed action (`open_action(NewTabAction::NewChanges)`) the tab bar's `+ → Changes` menu item
fires — the menu item exists in `tab_bar.rs` render. Built a fixture repo with one staged
(`A  staged.txt`), one modified (` M tracked.txt`) and one untracked (`?? newfile.txt`) file at
once: socket report after settle was Staged[staged.txt +1] Changed[tracked.txt +1]
Untracked[newfile.txt +1], `ready:true`, worktree `/tmp/critic4-fixture` — agrees with
`git status --porcelain`. Follows the selected worktree: after `workspace.select` to a second
worktree the report showed that worktree's path with 0/0/0 (verified clean via git), and back on
wt-0 again 1/1/1. Persists: after a real quit + relaunch the Changes tab is restored (kind=diff in
the store, mounted by `restore_tabs`). Tests: tiller crate 48 passed including the shell-level
mount guard `restore_mounts_a_changes_surface_in_the_application_shell` (claimed 42 — consistent,
grown). NOT EXERCISED — blocked on display: the actual `+ → Changes` click, and the stage/discard/
expand *clicks* (there is no socket door for those actions; `ChangesTab`'s own 59 tiller_ui tests
are green but that is the crate, not the click).

### 2. Socket methods — PASSED, and the converse is clean

Exercised live: pane.split/focus/close, tab.cycle/select (split made pane-3, focus moved, close
removed, cycle/select moved the active tab), panel.state/panel.scrollback (cwd, exitStatus,
scrollback, `--max-bytes` bound honoured), surface.changes.open/read, surface.settings.open/select/
read, plus ping/identify/quit/workspace.*/worktree.set/notify/session.ref/notification.*/
session.restore. `system.capabilities` advertises exactly the dispatched set (36 methods;
`workspace.new` is an undocumented alias; browser.* correctly absent). Converse: no advertised
method is undispatchable; the only soft spot is pass-3's minor gap, still open — `tab.select 99`
returns ok silently (out-of-range swallowed), while `panel.close/read <bogus>` now error properly
(fixed since pass 3). One asymmetry worth recording: `panel.write/key` address control panes only;
app panes are readable (`panel.state/scrollback`) but not writable over the socket — by design
(registry comment), but a caller gets "unknown pane: pane-1" for a pane that plainly exists.

### 3. Persistence — F-PER-03 PASSED; F-PER-01 FAILED end-to-end

F-PER-03 PASSED: fresh state, split the Terminal tab twice, store held
`[Split{1→2 horizontal}, Split{2→3 vertical}]`, quit, relaunch → pane-1/2/3 all present (was: one
silently lost). Minor: the pane focused before quit (pane-3) comes back as the root (focus is not
persisted; `SessionTabState` has no focused field).

F-PER-01 FAILED end-to-end. The claim was "scrollback should now persist, bounded at 256 KiB". The
bound is real (`SCROLLBACK_LIMIT`, applied on both encode and decode) and the terminal-side
capture/replay primitives are real and unit-tested with nonces — but **the shell never connects
them to the store**: there is no call site that writes live scrollback into
`SessionTabState.scrollback` (grep across the crate: only `bounded_scrollback` and tests), and
restore creates fresh `TerminalView::new` without ever calling `replay_scrollback`. After a session
with real terminal output, the DB row was `{"root_id":null,"pane_events":[],"scrollback":{}}`.
The nonce test the brief demands cannot even be set up over the socket (panel.write is control-only,
and control panes are not part of the session store). This is the characteristic failure a fourth
time: the crates do the right thing; the shell chose the simpler wrong thing. Files: `main.rs`
(no writer/replayer), `session.rs` (bound only) — re-checked against the live tree, still holds.

F-PER-06 re-confirmed: three quit cycles left zero stray processes.

### 4. Terminal state — facts real; the "7 state-backed, 1 pixel" claim overstates

TerminalStateSnapshot (working_directory, scrollback, exit_status Success/Code/Signal) is real and
readable: live `panel.state` showed `success`, `code:3`, and a killed shell as `code:137` (the
shell maps the signal before the PTY sees it — the Signal path is unit-tested, not reachable via
`sh -c`). Capture/replay round-trips with nonces in the crate's tests; 11 tiller_terminal tests
green. But the claim "7 of 8 previously display-blocked F-TERM entries are now state-backed, exactly
1 pixel-only" is only honest if the builder recorded every "see" clause as **half-proven** (the
P30 brief's own words). Against the inventory: F-TERM-03/07/08/09 have a state core now proven
(exit status, agent launch/identity, termination, indicator condition) but their entries are about
a user *seeing* the status — half-proven, not passed. F-TERM-02 (empty-pane prompt), F-TERM-11
(no-worktree state) and the context-menu trio F-TERM-04/05/06 are interaction+pixel entries no
state test can cover; "exactly 1 pixel-only" undercounts them. Nothing pixel was exercised.

### 5. Settings truthfulness — PASSED headless

Providers read back through `surface.settings.read`: claude available
(`/home/enzopalmisano/.local/bin/claude`), codex available, pi available, **opencode unavailable
("Not found on PATH")**, **omp unavailable** — two of five, matching `which` on this machine. The
report comes from the same `discover_availability()` rows the UI renders (provider_row/status_label;
no literals). Socket row: controlSocketEnabled=true, socketPath=/tmp/critic4.sock (the resolved live
path, routed from main into the settings snapshot). Permissions: `SettingsCategory::all()` is 4 on
non-macOS and `availableSections` via the socket confirms [ai-providers, agents, general,
appearance] — the screen is no longer offered. Not exercised: the badge pixels (display-blocked);
the data is proven truthful.

### 6. ci-linux.sh — honest gate, two sharp edges

Ran it four times. It fails honestly on a real break: the live tree does not compile right now
(`session.rs:419`, `path_ids` vs `PathBuf` — a builder's in-flight `catalog_root` refactor that
landed after my snapshot; my snapshot builds and its `cargo test --workspace` PASSED). Header
states it covers nothing visual; fmt drift is reported and continued; clippy warnings reported with
counts; build/test/python suites/visual-sweep contract all pass on the snapshot; the headless smoke
stage proves the app alive and a generated-nonce panel write→read round trip. Two sharp edges:
(1) **flaky false failure** — the start/end Rust fingerprint hashes every file under `rust/`
including `rust/crates/tiller/.remember/`, which the ambient Claude "remember" hook rewrites
(`session-start` appends to `memory-*.log` and rewrites `session-slug`) whenever a session starts in
the tree; the gate then fails "new Rust worktree drift appeared during the gate" with the tree
fine. Observed twice. (2) implicit precondition — the smoke stage needs the app's cwd to be a git
checkout or `current-workspace` returns "no current workspace" (observed on my non-git snapshot);
satisfied in the real worktree, not stated in the header. So CI OK means what it claims when it
prints, but it can be withheld while the tree is healthy — the "red on arrival" failure mode the
P26 brief warned about.

### Inventory sweep (budget)

Spot-checked against the live socket: F-TERM-SCR-01 (256 KiB retention: exactly 262144 bytes,
newest line-029999 present, oldest line-000000 evicted) PASSED; F-CTRL-WIRE-02 socket mode 0600
user-owned PASSED; most F-CTRL-* rows re-covered by this pass's live session; F-AGENT-API-01
catalog order consistent with the settings provider rows. The other ~160 domain entries remain
unverified headless.

### Counts

Among exercised entries: **PASSED** — Changes surface (mount/reach/bind/follow/persist/content/
48 tests), all 36 advertised socket methods, F-PER-03, F-PER-06, F-TERM-SCR-01, F-CTRL-WIRE-02,
exit-status facts, settings truthfulness, ci-linux stages. **FAILED** — F-PER-01 (scrollback
across restart, unwired end-to-end), plus the ci fingerprint flakiness and the F-TERM
classification claim (pixel entries counted as state-backed). **UNREACHABLE** — none new (opencode/
omp absence is piece 5's expected outcome, not a verdict on the app). **N/A — platform** — the
Permissions entries (TCC), whose screen is now correctly hidden. **NOT EXERCISED — blocked on
display** — the `+ → Changes` click, stage/discard/expand clicks, all badge pixels, F-TERM-02/04/05/
06/11 rendering, and the whole visual tier.

**Biggest gap that is not the display:** F-PER-01 — scrollback persistence. Every piece of it
exists in the crates (TerminalStateSnapshot capture/replay, SCROLLBACK_LIMIT bound, unit tests);
what is missing is the shell wiring: capture into `SessionTabState.scrollback` on save/quit and
`replay_scrollback` on restore. It is the same failure shape as the Changes surface before P21
and the provider badges before P23, and unlike the display it is ours to fix.

## PASS 5 — the Changes surface and the git tier — snapshot 12:35, exercised 12:38–12:51

Fresh-context critic again. Snapshot `/tmp/critic-pass5` copied 12:35 (12:35:00 checked),
`cargo build -p tiller -p tiller_control` clean. Daemon headless on a fresh `TILLER_DB` per fixture
(catalog seeds from cwd). Instrument: **the surface's own report vs `git status --porcelain` and
`git diff --numstat -z HEAD` on the same repository**, plus a probe binary
(`/tmp/critic-probe`, outside the tree, links the snapshot's `tiller_git`/`tiller_project` by path
— zero edits to any `rust/**` source) that calls the library functions the app calls. Live tree
re-checked at 12:48 before filing: only `main.rs`'s `layout`/`schedule_save` region moved since the
snapshot (a builder is wiring F-PER-01 scrollback capture); every file and line this section cites
is unchanged in the live tree.

**Fixture**: `git init` repo with, at once, a staged file, an unstaged-modified file, an untracked
file, a 300-line file modified at lines 2 and 290 (a 239-line unchanged middle), a staged rename,
then, as the pass progressed: an `MM` file (staged, then modified again), an `RM` file (renamed and
modified after), a staged deletion, an unstaged binary, an untracked file with a space in its name,
plus separate repos for: conflict (`UU`), no-HEAD with staged files, empty repo, and a broken
repo (`.git` removed).

### F-CHG (22) — the state tier is real and agrees with git; the error tier is a lie

- **F-CHG-08 PASSED.** Sections and counts matched porcelain exactly through every state thrown at
  it: the initial mixed set, the `MM` file appearing in **both Staged and Changed** (2/1 counts from
  `git diff HEAD`), the `RM` file in both sections with its rename source parsed
  (`orig=rm-dst.txt`), staged delete (`0/1`), pure rename (`R  oldname.txt -> newname.txt` staged,
  0/0), binary (`binary=true`, matching numstat `- -`), untracked with a space, and after an
  external `git commit` changed the repo under the mounted surface, the poll picked it up and still
  agreed. The surface tracked every state; git never disagreed.
- **F-CHG-07 PASSED (state).** Empty repo and clean worktree both report `0/0/0`, `ready=true`,
  `error=''`, matching `git status --porcelain | wc -l` = 0. The "Working tree clean" text itself
  is display.
- **F-CHG-12/15/17 PASSED (data tier).** The mounted surface fetches `diff_entry` with
  `CHANGES_CONTEXT_LINES = 24` per changed file; hunks carry correct old/new line numbers and
  headers (`@@ -1,26 +1,26 @@`, `@@ -266,35 +266,35 @@` on big.txt — the 239-line middle is exactly
  the collapsed-context band), untracked diffs are `@@ -0,0 +1 @@` via `--no-index /dev/null`,
  binary diffs set `is_binary`, and the report's per-file `+N −M` equals numstat. The expand/
  collapse clicks and the band label are display.
- **F-CHG-09 FAILED.** Loading is real (report shows `loading=true, ready=false` immediately after
  open), but the **error/Retry state cannot exist**. `load_snapshot` swallows every git failure
  (`status().unwrap_or_else(|_| empty)`, `stats().unwrap_or_default()`, `diff_entry().ok()`), and
  `refresh()` clears `git_error` rather than setting it. With `.git` removed — git itself answers
  `fatal: not a git repository` — the surface reports **`0/0/0`, `ready=true`, `error=''`**,
  indistinguishable from a clean repo. F-CHG-09's "Git-status error and Retry" half is dead code,
  and worse: the surface actively lies when git is broken. (Mutation failures do set `git_error` —
  only the status-poll path is dead.)
- **F-CHG-10/11/14 NOT EXERCISED — blocked on display** for the clicks; **no socket method exists**
  for stage/unstage/discard (`system.capabilities` has none), so the headless door the brief hoped
  for is absent. The library operations behind them are PASSED under F-GIT-ACT-01 below.
- **F-CHG-01 FAILED as documented**: the right panel has no Files/Changes segment in this build
  (`right_panel.rs` is Files + Activity); the design moved Changes to a first-class Diff tab, which
  is reachable and readable over the socket (tabId 2) — the pass-3 note stands.
- **F-CHG-13 FAILED as documented**: the row's `↗` is wired (`ChangesTabEvent::OpenFile` →
  `add_file_tab` → a read-only File tab, since there is no editor); no "Open diff" action exists.
- **F-CHG-16 FAILED (absent)**: no resolve-in-terminal action exists anywhere in `changes.rs` (only
  conflict *coloring*); the row actions are Discard / Stage-Unstage / ↗.
- **F-CHG-02/03/04/05/06/18/19/20/21/22 NOT EXERCISED — blocked on display** (tree browse,
  keyboard nav, status glyphs, activity section; pass 1 verified the Activity rendering on a
  display).

### F-GIT (16 — the inventory holds 16, not the brief's 19)

- **PASSED: RUN-01, REPO-01, WT-01, STATUS-01, ACT-01, DIFF-01, DIFF-02, PLAT-01.**
  - RUN-01: success/failure/timeout/launch-failure all exercised through real calls; the
    deadline-enforcement test is real (fake sleeping `git` on PATH, 200 ms budget, returns
    `TimedOut`). Divergences: **no output limits/truncation** (pipes are read unbounded) and **no
    cancellation** — both Swift features absent here.
  - REPO-01: `.git` dir → true, `.git` **file** (linked worktree) → true, non-repo → false;
    `has_head` accepts 0/1 (true on the fixture, false on an unborn HEAD). `rev-parse --git-dir`
    is never called — the port works off paths and lets git resolve.
  - WT-01: create with base and without base (branch created from HEAD), attach to an
    existing branch that has no worktree, duplicate-branch refusal
    (`BranchAlreadyCheckedOut` pre-check), remove of a dirty worktree refused, remove of a clean
    one succeeds; files materialize in every new worktree. **Divergence, deliberate and
    documented**: removal does **not** pass `--force`, so the inventory's "removal uses force"
    half has different observable behavior (dirty worktrees refuse to die).
  - STATUS-01: porcelain-v2 `-z` parse verified against v1: staged/changed/untracked split, `MM`
    two-state entries, rename original path, `UU` conflict, paths with spaces. Copy records share
    the rename `2` record shape (parser handles `C`); git does not emit copies by default, so not
    exercised live. Invalid path inputs (empty, `..`, absolute) are **rejected by git** with the
    worktree unchanged — the port surfaces git's own error rather than pre-validating (documented).
  - ACT-01: stage / unstage / stage_all / discard / discard_all all verified **in git after each
    call**: ` M`↔`M ` round-trips, `MM`→`M ` on discard (worktree side only), staged and untracked
    files left alone by `discard_all`, untracked deleted-staging via `add -A`, and the no-HEAD
    unstage path (`git rm --cached --force`, `A ` → `??`).
  - DIFF-01/02: tracked, untracked (`--no-index /dev/null`), binary, added, deleted, renamed,
    no-HEAD — all parse with correct hunks, old/new line numbers, counts, and markers.
  - PLAT-01: git is resolved via **PATH** (`GIT_BINARY = "git"`), not `/usr/bin`; the whole
    repo/worktree/status/diff/actions flow ran on Linux against that resolution; process-group
    kill on timeout verified.
- **PARTIAL: ACT-02, DIFF-04.**
  - ACT-02: empty/absolute/`..` paths are rejected (by git) and leave the worktree unchanged — but
    there is **no pre-validation and no stale/duplicate/conflicted rejection**: `stage` on a
    conflicted path **succeeds**, staging the conflict markers (`UU` → `M `), where the Swift entry
    requires rejection before executing.
  - DIFF-04: numstat parse, rename destination keying, and binary counts match git exactly; but
    **no-HEAD staged files get no stat at all** (0/0) — `stats()` skips numstat without HEAD and
    only counts *untracked* files from disk — while `diff_entry` for the same file correctly shows
    the added lines. The surface row lies (`+0 −0`) next to a real expanded diff. And the untracked
    disk count caps at **500,000 bytes**, not "500,000 lines": an untracked file over 500 KB
    silently reports 0/0.
- **FAILED (absent): RUN-02, BRANCH-01, CLONE-01, REMOTE-01, STATUS-02, DIFF-03.** The port has no
  streaming runner (RUN-02; the runner is synchronous whole-output, so `git clone`-style progress
  can never arrive incrementally), no `git branch --list` (BRANCH-01; only `--show-current` in
  `tiller_project`), no clone (CLONE-01), no GitHub-remote parsing (REMOTE-01), no directory-status
  aggregation (STATUS-02 — the file tree marks ancestor directories with a plain boolean, no
  conflict>changed>untracked precedence, and rename sources do not mark their old directories),
  and no side-by-side pairing code (DIFF-03). Supporting suite: `cargo test -p tiller_git` 22
  green, `-p tiller_project` 34 green.

### Disagreements the instrument found

1. **A broken repository renders as clean** (F-CHG-09): `.git` removed → surface `0/0/0, error=''`
   vs git `fatal: not a git repository`.
2. **No-HEAD staged files show `+0 −0`** (DIFF-04) while their expanded diff shows real added
   lines — a within-surface inconsistency, visible to every user of an unborn-HEAD repo.
3. **Conflicted paths are not protected** (ACT-02): the surface's Stage button on a conflicted file
   would mark it resolved with conflict markers in the index.
4. `has_head` returns `false` when the git binary is missing — a broken environment is
   indistinguishable from an unborn HEAD (practically inert: the next git call fails loudly).

### INVENTORY-STATUS corrections

- F-CHG row ("Mostly unverified"): the git-state tier is now critic-confirmed — F-CHG-07/08 fully,
  12/15/17 at the data tier, 09 FAILED, 01/13/16 FAILED as documented. The visual tier (02–06,
  10/11/14 clicks, 18–22) remains blocked on display.
- F-GIT row (inside "≈145 barely touched"): all **16** entries now judged — 8 PASSED, 2 PARTIAL,
  6 FAILED (structural absences). The brief says 19; the inventory file holds 16 `F-GIT-*` lines.
- The "not implemented in the Linux shell" set of *absent-by-design* features (clone, branch list,
  remote parsing, side-by-side, streaming, output limits, pre-validation) is a port-scope question,
  not a defect: none are wired to any door, so nothing in the running app regresses — but they are
  inventory entries that will never tick.

### Counts

F-CHG: **5 PASSED** (07, 08, and the 12/15/17 data tiers) · **4 FAILED** (01, 09, 13, 16) ·
**13 NOT EXERCISED — blocked on display**. F-GIT: **8 PASSED** · **2 PARTIAL** (ACT-02, DIFF-04) ·
**6 FAILED (absent)**. Supporting: 22 + 34 crate tests green; zero edits to the tree.

**Biggest gap that is not the display:** the Changes surface's error truthfulness — F-CHG-09's
status-error path is dead code, so the one moment the surface must not pretend (git is broken) is
the one moment it reports a clean tree. The same wiring-shaped hole the brief predicted is next
door: stage/unstage/discard are complete and verified in `tiller_git` but have no socket door, so
the entire F-CHG mutation tier stays headless-unverifiable.

## PASS 6 — the control package and the agent adapters — snapshot 12:52, exercised 12:55–13:22

Fresh-context critic again. Snapshot `/tmp/critic-pass6` copied 12:52:13, `cargo build -p tiller
-p tiller_control` clean, daemon headless on `TILLER_SOCKET=/tmp/critic6.sock` with a per-fixture
`TILLER_DB`, catalog seeded from the fixture cwd. Instrument: **the raw NDJSON wire itself** — a
python probe connecting to the unix socket byte-for-byte (malformed lines, huge lines, split
lines, multi-request connections, concurrent clients) — plus `tillerctl` for every command, the
fixture's own git repo for workspace mutation, and a probe binary outside the tree
(`/tmp/critic6-probe-agent`, links the snapshot's `tiller_agents` by path) for the adapters.
Real CLIs were used where the entries demand them: claude 2.1.231, codex-cli 0.147.0, pi 0.84.1
(all on PATH); opencode and omp are **not installed** (UNREACHABLE, as the brief said).
Live tree re-checked 13:20 before filing: `main.rs` gained ~107 lines (the P37 mutation door
`surface changes stage/unstage/discard` — the door pass 5 predicted now exists), `tillerctl.rs`
gained the same commands at 13:05; **every file and line this section cites is unchanged in the
live tree** (panel.rs/server.rs byte-identical; the cited dispatch lines verified present).

Counts, counted by me: **F-CTRL is 34 entries, not the brief's ~37; F-AGENT is 20, as guessed.**
The brief's 37 is somebody's recollection; the file holds 34 `F-CTRL-*` lines (9 panel, 5 work,
6 browser, 4 wire, 3 notify, 2 session, 2 sys, 2 cli, 1 plat).

### F-CTRL (34) — the wire tells the truth; two persistence/PTY lies

**PASSED (21):** WIRE-01 (NDJSON framing, id echo, sorted keys — BTreeMap, verified on the
wire), WIRE-03 (fresh connection per round trip; server-disconnected gives the canonical error,
rc=1), WIRE-04 (`$TILLER_SOCKET` wins; unset → `$XDG_RUNTIME_DIR/TillerRust/control.sock`
created 0600 with a 0700 leaf parent, client resolves the same default — both proven live).
PANEL-01..09: create with and without `--cmd` (bare shell, running state), split in all four
directions (bad direction → "unknown split direction"), list rows with id/tab/title/agent/active,
write + `--enter` (a `read -p` pane completed: `PROMPT:with-enter` then `GOT=with-enter`), the
nine symbolic keys byte-verified against a raw PTY catcher (`\r \x7f \t \x1b \x1b[A \x1b[B
\x1b[C \x1b[D \x1b[3~` — exactly the documented sequences), read base64 round-trips, wait
distinguishes exit code / timeout / unknown pane, focus flips the registry's `active` flag
(live, on two panes, both directions — the "visible pane change" is the rendered focus ring,
pixels), close removes and re-answers "closed pane" vs "unknown pane" distinctly. NOTIFY-01
(agent statuses running/needs-input/done/error accepted, unknown status rejected; user mode
creates listable notifications). SESSION-01 (`session.ref` **survived a relaunch**; identify
returns the ref; the ref **resumed a real claude session**). WORK-02 (rows sorted, selected
flag live), WORK-03 (non-git project rejected with the distinct "is not a Git repository" —
reached by seeding the catalog from a non-git cwd), WORK-04 (select by id and by exact path,
current, no-selection error). NOTIFY-03 (create/list/clear in-memory; system-notification
posting is platform N/A). SESSION-02 (restore-session returns success, no duplication of
rows). SYS-02 (explicit workspace, `TILLER_PANE_ID`/`TILLER_WORKTREE_ID` env context,
selected-worktree fallback, no-context error — all three precedence layers live). PLAT-01
(manifest is platform-neutral serde/libc; the same NDJSON contract ran on Linux).

**PARTIAL (5):**
- **WIRE-02 — the "1 MiB cap" is fuzzy by up to one 64 KiB read chunk.** A request line of
  1,048,631 bytes (over 1 MiB) was **accepted and executed**; rejection ("request line too
  large" + close) only bites lines whose newline has not arrived by the time the buffer passes
  1 MiB — measured threshold ≈1 MiB+64 KiB, and it depends on the client's send chunking. The
  advertised "caps a request line at 1 MiB" is not exactly true. Everything else in the entry —
  0600 socket, SO_PEERCRED owner check, stale-socket recovery, live-socket refusal ("a Tiller is
  already listening", first daemon unharmed), 12 concurrent clients all answered with correct
  ids, malformed errors bounded — is proven.
- **NOTIFY-02 — extraction is proven live through a real claude hook, but ambiguity is
  resolved, not rejected.** stdin JSON (`session_id`), payload-arg JSON (`thread_id`), and
  rollout paths inside JSON payloads all extracted and persisted (verified via identify), and
  `--agent-session` beats stdin. But the entry says "ambiguous or missing mode arguments are
  errors": `--title T --session s --status running` silently resolves to title mode, and
  `--title T` alone is accepted with an empty body. A bare rollout path as a positional arg
  extracts nothing (the positional scan is JSON-only).
- **SYS-01 — `system.ping` returns `{"status":"ok"}`, not the documented `{pong:true}`.** The
  CLI prints "pong" either way, so tillerctl is blind to it, but any raw client following the
  inventory looks for `pong:true` and finds nothing. Capabilities rows match all 37 advertised
  methods exactly (each answered), socket-enabled state present.
- **WORK-05 — closing a worktree does not terminate its control panes.** After
  `workspace.close` on the worktree holding `echo LEAKMARK >/tmp/leakmark.txt; sleep 120`:
  `mounted=false selected=false` (row retained — the sidebar half is right), but the `sleep`
  process was **still alive**, `panel.read` still returned the pane, `panel.wait` still timed
  out. The entry's own verify says "confirm its PTY is unmounted/terminated". Control panes are
  not in the UI tab tree, so `close_tab`'s Ctrl-C/Ctrl-D never reaches them; only
  registry shutdown (app quit) or explicit `panel.close` does. Compounding it: `terminate()`
  kills only the direct child pid — a compound command (`sh -lc 'echo x; sleep 120'`) leaves
  its process group orphaned even on `panel.close` (measured: SIGTERM'd shell, `sleep` alive
  after close). Same for app quit. This is the pass's biggest non-display finding.
- **CLI-01 — the documented `tillerctl [--socket <path>] <command>` placement does not work.**
  `main()` takes `args.first()` as the subcommand and parses `&args[1..]`, so `--socket` must
  follow the command; `tillerctl --socket /path ping` and `--socket=/path` both fail with
  "unknown command '--socket'". All 25 subcommands themselves ran and mapped to the right
  methods (help text matches reality otherwise).

**FAILED (2):**
- **WORK-01 — the worktree comment does not persist.** `worktree.set --comment` round-trips
  in-memory (by id and by path), but after a relaunch the comment is gone; the SQLite `worktree`
  table has no comment column and `set_worktree` only mutates `ControlWorkspace`. The entry's
  verify is "restart, and confirm the comment is persisted". It is not.
- **CLI-02 — no shim/install mechanism exists.** The entry's shim ("installs a tillerctl
  symlink under App Support/bin and uses that path for generated agent hooks") has no Linux
  counterpart at all — no install command, no XDG bin policy; `add_agent_tab` passes the bare
  name `tillerctl` and relies on PATH. The platform note anticipated this; the absence is the
  finding.

**N/A — platform (6):** BROWSER-01..06. All ten `browser.*` methods (the full set, incl.
`browser.errors`) answer the same explicit unsupported error; none is advertised;
`browser.errors` is accepted-but-unadvertised exactly as F-CTRL-BROWSER-01 documents. The
documented "unsupported response" is the permitted Linux behavior, recorded as such.

### F-AGENT (20) — adapters verified against real CLIs; three structural absences

**PASSED (9):** CLAUDE-01 — a real `claude` TUI **launched in a control pane** (running state),
a prompt written and Entered, both SessionStart and UserPromptSubmit hooks fired, the session
ref captured via `--stdin-json` and returned by identify, and **both** a transcript-file id and
a hook-extracted id resumed (`claude --resume <ref> -p` → same session). CLAUDE-02 — merge
semantics measured: seeded `settings.local.json` with `model`, `permissions`, PreToolUse,
OtherEvent, a stale Stop — after prepare only the five hook events were replaced, everything
else byte-identical. CLAUDE-03 — `claude -p '<prompt>'` ran live. CODEX-01 — prepare writes
nothing (tree empty); the generated `codex -c 'notify=[...]'` override **loaded in the real
codex 0.147.0** (the TOML trap is the claim and it holds: no `\/`, JSON-decodes, config load
succeeded), resume shape correct. CODEX-02 — `codex exec --output-last-message /dev/stdout`
ran live with and without the override. PI-01/02 — bare launch, resume shape; `pi --print
--no-tools '<prompt>'` ran live. SESSION-03 — shell_quote/json_string_literal round-trip paths
with spaces and apostrophes (`O'\''Brien`), no slash-escaping. PLAT-01 — platform-neutral
manifest, plain CLI processes, worktree-local outputs only.

**The no-user-global-writes invariant — measured, holds.** Probe seeded a fake `$HOME` with
`~/.claude/settings.json`, `~/.claude/settings.local.json`, `~/.codex/config.toml`,
`~/.config/opencode/global.json`; ran all five prepares; the fake HOME hash was **unchanged**
and the worktree contained exactly `.claude/settings.local.json`,
`.opencode/plugin/tiller-session.js`, `.tiller/omp-hook.ts`. No adapter writes outside the
worktree. (The global `npx skills add` command exists separately in `tiller_project` — it
installs *skills* user-wide by design and omits `omp`, as the Open questions already flag.)

**PARTIAL (2):** API-01 — the catalog is exactly [claude, codex, opencode, pi, omp] with hook
flags [true, true, false, false, true] and every adapter has prepare/command/resume — but the
trait has **no summarizer method**; the inventory says every adapter exposes an "optional
noninteractive summarizer command". The commands themselves work (claude -p, codex exec, pi
--print all ran) but nothing in `tiller_agents` exposes them. SAFE-01 — the worktree-local half
is proven (above); the skill-provisioning half (refuse to overwrite an unmanaged skill file,
management marker, unsupported agent ids) **does not exist** in the port — no provisioner at
all.

**FAILED (3) — absent by structure:** SAFE-02 (no ClaudeHookMigrator anywhere),
SESSION-01 (no session validator — no `~/.claude/projects/<slug>/<ref>.jsonl` or codex
sessions checks), SESSION-02 (no transcript extraction — `tiller_usage`'s claude.rs parses the
`/usage` panel, not session transcripts). None is wired to any door; these are inventory
entries that will never tick until built.

**UNREACHABLE (6):** OPENCODE-01/02/03 and OMP-01/02/03 — binaries not installed
(`command -v` empty), stated external reason. The hooks/plugins they would generate are
source-verified but not exercised.

### Disagreements the instrument found

1. **`workspace.close` leaks control panes and their process groups** (WORK-05/PANEL-09):
   the entry's own verify ("confirm its PTY is unmounted/terminated") fails for every
   control-created pane, and compound-command panes orphan their children even on
   `panel.close` because `terminate` targets only the direct pid. Every headless verdict in
   this session that creates a pane then closes its worktree has been leaking PTYs.
2. **`worktree.set` comments die on restart** (WORK-01): the one mutation tier in this group
   that claims persistence doesn't persist; no comment column in the schema.
3. **`system.ping`'s wire shape differs from the documented contract** (`{status:"ok"}` vs
   `{pong:true}`) — invisible to tillerctl, wrong for any other client.
4. **The documented `--socket` placement is broken** (CLI-01): help says
   `[--socket <path>] <command>`; only `<command> --socket <path>` works.

### The GPUI unblock, applied to this pass

The view-test harness (`gpui::TestAppContext` + `VisualTestContext`, pattern in
`tiller_ui/src/changes.rs`, live tree now proves F-CHG-09 with a drawn frame and a real Retry
click) changes the display line for interaction claims. For **this** pass the line did not move:
no F-CTRL or F-AGENT entry was written off as display-blocked — every behavior was asserted
over the socket (the registry's `active` flag for focus, exit codes for wait, rows for list),
and the only unasserted residue is pixels (the focus ring, tab-bar rendering). The boundary
correction matters for the *other* tiers' "click the X" entries (F-TAB chords, F-EDIT
double-click-to-open, F-CHG clicks) — those are now reachable as interaction and should be
re-judged by a pass that owns those files; I did not rewrite their verdicts here.

### INVENTORY-STATUS corrections

- F-CTRL row: all **34** entries now judged — 21 PASSED, 5 PARTIAL, 2 FAILED (WORK-01
  persistence, CLI-02 absent), 6 N/A — platform (browser, documented unsupported). The brief
  says ~37; the file holds 34 `F-CTRL-*` lines.
- F-AGENT row: all **20** judged — 9 PASSED, 2 PARTIAL (API-01 no summarizer in the trait,
  SAFE-01 skill half), 3 FAILED (SAFE-02, SESSION-01, SESSION-02 — structural absences), 6
  UNREACHABLE (opencode/omp not installed).
- The "browser tier" note stands confirmed: `F-BRW` and `F-CTRL-BROWSER-*` answer explicit
  unsupported errors on Linux, which the entry text permits.

### Counts

F-CTRL: **21 PASSED · 5 PARTIAL · 2 FAILED · 6 N/A — platform**. F-AGENT: **9 PASSED ·
2 PARTIAL · 3 FAILED · 6 UNREACHABLE**. Supporting: `cargo test -p tiller_control -p
tiller_agents` 80 green (8+20+35+8+1+8); claude/codex/pi exercised against their real CLIs;
zero edits to any rust/** source.

**Biggest gap that is not the display:** the control tier's own lifecycle lie —
`workspace.close` (and `panel.close` for compound commands, and app quit) leaves PTYs and
orphaned process groups running, so the door this whole project measures itself with leaks
state it claims to have unmounted. The second is `worktree.set`'s comment, which claims
persistence and has none.

## PASS 7 — audit of the interaction-tier reclamation — 13:35–13:52 (snapshot 13:28)

Fresh-context critic. The brief: the builders reclaimed ~70 entries previously held as
`NOT EXERCISED — blocked on display` after finding `gpui::TestAppContext` + `VisualTestContext`
draws the real tree and dispatches real input headlessly. Audit the reclamation: for each
reclaimed entry, does its VERIFY turn on seeing something, and does any test that now claims it
draw the frame and drive the input, or call a handler directly?

**Method.** Snapshot `/tmp/critic-pass7` (cp -a, source mtime 13:28; `target/` was stale from the
copy — a forced rebuild of `changes.rs`/`right_panel.rs` was required before judging, and it
changed the failures: 3 flaky-looking fails became 4 deterministic timing fails). App daemon
headless on `TILLER_SOCKET=/tmp/critic7.sock` (13:35): `system.ping` pongs, `tab.cycle`
forward/backward answer ok — the snapshot serves state. Live tree re-checked at 13:47 and 13:50:
**it moved during the pass** (`main.rs`, `panel.rs`, `protocol.rs`, `control_integration.rs`,
`changes.rs`, `right_panel.rs`), broke compilation mid-pass (`render_mode_switch` in tiller_ui),
and was fixed again by 13:50 — builders are actively editing the exact files this audit covers.
Every verdict below is stamped against the forced-rebuilt 13:28 snapshot unless it says live.

**The reclamation document is careful; the entry-by-entry reality is not.** The audit itself says
"Reclassification is not a pass; each reclaimed entry still needs an evidence test". So the
question is which reclassified entries have the code for what they claim to be "headlessly
reachable". Audited each row against the whole tree (bindings, handlers, drag/menu/rename code,
tests). Result: the claim "interaction is reachable" is **true for roughly half the entries and
false for the rest — where the code does not exist, the harness cannot reach it, and the
reclassification has converted structural absences into holds.**

### Upholdings — entries whose behaviour code exists and is drawn-testable (with honesty notes)

- **F-CHG-09 — UPHELD.** `the_error_state_renders_and_retry_is_clickable` (changes.rs) removes a
  real `.git`, asserts `changes-error`/`changes-retry` bounds in a drawn frame, clicks Retry at
  real bounds, asserts recovery. Passed in both full runs. This is the model the rest must match.
- **F-CHG-08/12 — UPHELD.** `drawn_changes_rows_expand_sections_and_context_bands` clicks the drawn
  section header (collapse), file row (expand diff), and context band (expand hidden lines), and
  asserts the state. Passed. Counts *seen* remain display (F-CHG-08's count text is state-asserted,
  not pixel-asserted).
- **F-CHG-10/11/14 — UPHELD as the right kind of evidence, but the tests are UNSTABLE.** The
  stage/unstage, stage-all/discard-all, and discard+prompt tests click real drawn buttons against a
  real checkout and assert git's index afterwards — exactly the evidence class the brief demands.
  But in the forced-rebuilt snapshot, all three **fail deterministically** — `wait_for_tab`'s
  pump budget expires while the final report already shows the waited-on state; the
  `apply_snapshot` continuation lands just after the loop gives up (the `cx.cx.run_until_parked()`
  path drives the background executor but the state lands late under the virtual-clock/real-git
  mix). Live tree (moved, has new diagnostics): stage/unstage passes, discard-all failed once then
  passed 6/6 in isolation. **Half-proven: the tests are right, the harness timing is broken.** The
  F-CHG-09 test works around it with a full `cx.cx.run_until_parked()` + a longer real budget; the
  mutation tests need the same hardening. F-CHG-10's claim "exercised through the actual drawn row
  controls" is not currently reproducible from the snapshot.
- **F-EDIT-09 — HALF.** `double_clicking_a_drawn_file_row_emits_open_file` finds the row at real
  bounds, dispatches a real double-click (`click_count: 2`), and asserts the `OpenFile` event with
  the exact path — real interaction. But (a) it asserts an **emitted event**, not a resulting
  editor tab — the shell wiring (`RightPanelEvent::OpenFile` → `add_file_tab`, main.rs:2000) exists
  but is not exercised end-to-end; (b) the test **failed deterministically in the snapshot**
  (file row not laid out within the pump budget) and passes in the live tree. Half-proven.
- **F-CHG-03/04 — HALF (state, not interaction).** The right_panel tests
  (`expanding_a_folder_is_asynchronous`, `a_superseded_walk_is_dropped`,
  `an_unreadable_directory_renders_an_error`) call `toggle_file` **directly on the handler** — no
  drawn click. They prove expansion/error STATE, the same class as the socket transcripts, not
  interaction. The whole-panel "Files unavailable + Retry" state and the Refresh button are untested.
- **F-TAB-03/04/05/07 — UPHELD (component tier).** `drawn_new_tab_menu_dispatches_every_item_action`
  clicks the drawn +, draws the menu, clicks every item, asserts the `NewTabAction` emitted.
  Honest scope: the proof ends at the emitted action — the shell effect (a tab appears) is not
  asserted. F-TAB-06 (browser) is N/A-unsupported on Linux, mislabelled as HEADLESS-BEHAVIOUR.
- **F-TAB-19/20/22/28 — UPHELD as *reachable*, still unexercised.** The shell now binds
  `ctrl-tab`/`ctrl-shift-tab`, `ctrl-1..9`, `ctrl-alt-*` focus/split, `ctrl-alt-w` (panes.rs:38-57)
  and the handlers (`cycle_tab`, `select_tab_position`) are the same code the socket's `tab.cycle`
  /`tab.select` drove in pass 4 — I re-ponged `tab.cycle` forward/backward on the snapshot daemon.
  But **no test anywhere dispatches any of these chords through the key path** (the only
  `simulate_keystrokes` calls in the tree are "escape"/"enter"). The audit's "can be dispatched"
  is capability, and INVENTORY-STATUS's "reachable, not automatically passed" is honest — they stay
  unproven, and F-TAB-28's binding is `ctrl-alt-w`, not the documented ⌘W.
- **F-TERM-03/09, F-CHG-15/17, F-EDIT-13, F-PER-07, F-WIN-01/07, F-AUTO-01, F-TAB-01/10/11/15,
  F-CHG-07/19/20/21 — UPHELD as MIXED or state-backed with the appearance half still owed.** The
  exit-status/activity state cores are real (passes 4/6); F-AUTO-01's drawn toggle test
  (`control_socket_row_shows_resolved_path_and_toggles`) proves the resolved path and the enabled
  state flip in a drawn frame — the socket's actual disable effect is untested; F-EDIT-13's
  missing-file test asserts the message in a drawn window but reads model state, no input driven.

### Overturns — reclassified as "headlessly reachable" where no code exists

Verified by tree-wide grep (bindings, drag handlers, menus, rename, toasts, empty states): the
harness cannot reach what is not built. These should be **FAILED (absent)**, not holds:

- **F-EDIT-01 — no Code/Preview switching exists.** The editor has `preview_locked`/`request_preview`
  (F-EDIT-03's large-file path only); the builder's own comment (file_view.rs:421) says the
  Code/Preview split is "display-bound". The audit's "Code/Preview switching and content-mode state
  are headless" is false for switching.
- **F-EDIT-10/11 — no context menu on file rows.** right_panel.rs has no menu, no Copy-path code.
  "Open/menu routing is headless" — there is no menu to route. (Finder/clipboard = external, as
  noted, but nothing exists behind the seam either.)
- **F-EDIT-12 / F-CHG-18 — no drag handlers anywhere.** `on_drag`/drag gesture: zero in
  right_panel.rs and changes.rs; the only drags in the tree are the pane divider (main.rs) and
  transcript scroll (chat.rs). "Drag dispatch is supported by the mouse-event sequence" is a
  capability claim about nothing.
- **F-TAB-02 (overflow), 08 (no-agent fallback submenu), 09 (pane menu), 12/13 (move tab),
  14 (rename), 16 (dirty-close confirmation), 17 (close others/right), 18 (drag reorder),
  21 (Tab menu), 23 (Pane menu), 24 (Escape cancels a drag), 25 (attach to terminal),
  26 (context-menu close), 27 (resume chat) — all absent.** The strip renders in main.rs
  (`render_open_tab`) with icon/title/✓/close-control only: no overflow control, no dirty
  indicator, no rename, no drag, no context menu. `close_tab` (main.rs) removes the tab with **no
  confirmation and no dirty check** — F-TAB-16's prompt does not exist. F-TAB-24 is vacuous: there
  is no drag to cancel. The audit's single row "F-TAB-03…27 — Menu, click, double-click, drag,
  confirmation, split, move, attach, and resume actions are no longer display-blocked" lumps 24
  entries into one claim; **15 of them have no implementation at all.**
- **F-TERM-02/11 (empty-pane and no-worktree empty states), F-TERM-04/05/06 (terminal context
  menu: Copy/Clear/Set Title/Copy-ID) — absent.** No empty-state rendering exists in the shell
  (grep across main.rs/terminal: nothing); the terminal view (tiller_terminal/src/lib.rs:815) has
  click-to-focus + a drawn exit-status pill only, no context menu.
- **F-TERM-08 — the worst single reclassification, and it contradicts pass 6.** The audit moves it
  to "MIXED: reclaim behaviour — …termination state is headless-testable". Pass 6 **measured**
  (WORK-05/PANEL-09) that compound-command panes leak their process groups on `panel.close` and on
  quit; the close-confirmation prompt this entry's VERIFY demands does not exist in `close_tab`.
  Termination is not "unproven-mixed", it is a measured FAILED that the reclassification masks.
- **F-CHG-05 (arrow/Space/Return navigation) — no keyboard handling at all in right_panel.rs**
  (no key_context, no bindings, no focus_handle). "Tree selection and keyboard navigation can be
  driven from drawn bounds" — there is no keyboard code to drive.
- **F-WIN-02/03/04/05 (⌘T, ⌘O/⌘S, sidebar toggle, right-panel toggle) — no bindings exist.**
  The full `KeyBinding::new` inventory (panes.rs + chat.rs + tab_bar.rs) has no cmd-t/cmd-o/cmd-s
  and no panel-toggle chords. F-WIN-10 (toasts) — no toast implementation anywhere, only a theme
  radius token.

### The other direction — still blocked, now reachable

- **F-CHAT-04's Shift+Return half** — composer binds `shift-enter`/`shift-return` (chat.rs:475-476);
  `simulate_keystrokes("shift-return")` would close it. Still only PARTIAL from pass 1.
- **F-SET-03 (Check for Updates click), F-CHG-20 (running-count state in the drawn activity
  section), F-TAB-15 (the strip's ✕ — drawn-reachable, pass 2 verified on a display)** — reachable
  now, still held.
- **F-SET-21 (Files icon theme)** — the audit missed it but the tree already proves it:
  `selecting_the_listed_file_icon_set_changes_the_snapshot` clicks the drawn segment and asserts
  the snapshot changes. An appearance entry whose behaviour half is now proven.

### Test-health finding

The drawn-frame mutation tests are **load/timing fragile**: 4 of them failed deterministically in
the forced-rebuilt snapshot under the pump budget even with an otherwise idle machine, and the
failing set differs between the stale and the rebuilt binaries — the harness's
virtual-clock-vs-real-git mix makes the `apply_snapshot` continuation land just after the budget
expires. The sidebar tests already document this class (`TILLER_GIT_TIMEOUT_MS` widening). Every
entry whose drawn proof sits on those tests (F-CHG-10/11/14, F-EDIT-09) is half-proven until the
pump is hardened, whatever the source says.

### INVENTORY-STATUS corrections

- **F-TAB row**: "clicks, menus, drags, confirmation flows, and key chords are headlessly
  dispatchable… They are reachable, not automatically passed" — the menu/click/chord half is fair;
  **drags, confirmation flows, and most menus have no code**: F-TAB-02/08/09/12/13/14/16/17/18/21/
  23/24/25/26/27 are FAILED (absent), not reachable. The strip itself (activation/close) is
  drawn-reachable in main.rs and untested.
- **F-EDIT row**: "the interaction halves of F-EDIT-01/09/10/11/12/13 are headlessly reachable" —
  **01/10/11/12 have no code** (FAILED absent); 09/13 have partial (event/state) evidence only.
- **F-TERM row**: "F-TERM-02/04/05/06/11 are headlessly reachable (clipboard is an external
  backend)" — **they have no code** (FAILED absent); "F-TERM-03/07/08/09 have headlessly testable
  state cores" — 08's termination core is a **measured FAILED** (pass 6), not a testable hold.
- **F-WIN row**: "headlessly testable command/state behaviour" — 02/03/04/05 have no bindings, 10
  has no implementation (FAILED absent); 01/07's routing/restore halves stand (01 display-verified
  pass 1, 07 socket-verified pass 3).

### Counts

Audited all 16 reclassification rows (70 entries). **Upheld (code exists, right-kind or
state-level evidence, or honestly-reachable-unexercised): 35**, the strongest being F-CHG-09
(drawn, passing), F-CHG-08/12, the F-TAB +-menu → action tests, the reachable-but-unproven chords,
and the mixed state cores. **Overturned (reclassified as reachable where the code is absent):
32** — F-EDIT-01/10/11/12, F-TAB-02/08/09/12/13/14/16/17/18/21/23/24/25/26/27, F-TERM-02/04/05/
06/08/11, F-CHG-05/18, F-WIN-02/03/04/05/10. **Half-proven additionally: F-CHG-10/11/14 and
F-EDIT-09** — right-kind drawn tests that fail under the harness's own pump budget in the snapshot
(flaky live). The audit document's "reclassification is not a pass" caveat is honest; the
entry-level claim "these are reachable" is not true for the absent half.

**Biggest gap that is not the display:** the reclamation's F-TAB mega-row — 21 entries under
one claim, 13 of them with no implementation (plus F-TAB-02 and F-TAB-24, 15 F-TAB entries
overturned in total) — plus F-TERM-08's reclassification, which converts pass
6's measured process-group leak into a "mixed, behaviour reclaimable" hold. The reclassification
process ran on lists, not per-entry code checks; the harness made interaction provable, and
simultaneously made absence hideable.

## PASS 8 — sweep the unjudged remainder per entry — 13:56–14:15 (snapshot 13:56)

Fresh-context critic. The brief: sweep what is still unjudged in `01-inventory-app.md` **per
entry, not per list** — F-TAB (28), F-WIN (12), F-BRW (9), plus spot-checks of F-SID/F-SET/
F-USE/F-CHAT, whose pass-1/5 verdicts are hypotheses with timestamps. The tree moved during
pass 7 (main.rs 13:36, tab_bar.rs 13:41, right_panel.rs 13:50, changes.rs 13:48); this pass is
stamped against the copy made at 13:56.

**Method.** Snapshot `/tmp/critic-pass8`, daemon headless on `TILLER_SOCKET=/tmp/critic8.sock`
(13:56): ping pongs, capabilities lists 37 methods, `panel.create`/`panel.write`/`panel.read`
(terminal tier live — wrote `echo hello-from-critic8` to a pane, read base64 output back),
`select-workspace` mounts the worktree's Chat+Terminal tabs (visible in `panel list`), and
`notification.create`/`list` deliver. **Full test suite green in this snapshot:** tiller_ui
105/105 ×3 consecutive runs, tiller 53/53. That includes the four drawn mutation tests pass 7
found flaky (`drawn_stage_and_unstage_buttons_mutate_the_real_checkout`,
`drawn_discard_button_requires_confirmation_then_mutates_git`,
`drawn_stage_all_button_stages_every_changed_file`, `double_clicking_a_drawn_file_row_emits_open_file`)
— the pump-timing defect is gone; F-CHG-10/11/14 and F-EDIT-09 are no longer half-proven on
timing. Pass 7's "no chord-dispatch test exists" is also stale: `modifier_chords_dispatch_actions_
through_the_real_key_path` (tab_bar.rs:524) dispatches ctrl-tab/ctrl-1/ctrl-w through GPUI's real
key path, and `a_payload_drag_reaches_the_drop_target_through_real_mouse_events` (right_panel.rs:
1187) drives a real down/move/up drag — both fixtures, both passing. The harness capabilities
pass 7 wanted are now proven; what they prove about the *production tree* is the question below.

### F-TAB — per-entry, the exact absent list (28)

**PASSED (7):** F-TAB-03/04 (+ menu → New Terminal/Changes/agent actions; drawn test proves the
dispatch, shell `open_action` creates real tabs — the same add-terminal path the live workspace
mounts, which I exercised), F-TAB-10 (split right/down: socket `pane.split` created pane-3 live,
active flag moved; chords bound — no Split button/menu exists, affordance differs), F-TAB-15
(active-tab ✕ `workspace-tab-close-{id}` → `close_tab_by_id`; pass 2 display-verified),
F-TAB-19 (ctrl-tab/ctrl-shift-tab bound in shell; handler = socket-exercised `tab.cycle` — active
flag moved forward AND backward live), F-TAB-20 (ctrl-1..9 bound; handler = `tab.select` — live:
`tab select 2` moved the active flag), F-TAB-22 (ctrl-alt-arrows bound; handler = `pane.focus` —
live: `pane focus left` moved focus back across a split).

**half-proven (5):** F-TAB-01 (strip renders icon/title/✓-done/close — but the VERIFY's **dirty
indicator does not exist** and the status indicator is only the Done ✓; the strip is otherwise
drawn-reachable and pass-2 display-verified), F-TAB-05 (menu lists all 5 adapters, action →
`add_agent_tab` with real prepare/command; the "starts **or reports its launch error**" half with
opencode/omp missing is unexercised — display), F-TAB-07 ("New Chat" item dispatches → chat tab,
but the **ACP-agent submenu does not exist** — the chevron is drawn with nothing behind it),
F-TAB-11 (`split_disabled_reason` is unit-tested and never rendered — no split menu exists to show
the reason), F-TAB-23 (right/down splits real via chords/socket — the **Pane menu and left/up
directions do not exist**, so the VERIFY's menu trials cannot hold).

**FAILED — absent (15):** F-TAB-02 (no All-Tabs overflow control), F-TAB-08 (no no-agent
fallback/"Other agents…"), F-TAB-09 (no pane menu / Open File), F-TAB-12 (no move-tab), F-TAB-13
(no move-tab menu or empty state), F-TAB-14 (no rename anywhere; the "Auto-rename tabs" settings
row is a different feature), F-TAB-16 (the editor exposes `is_dirty` but `close_tab` has **no
confirmation and no dirty check** — zero "dirty" references in the shell), F-TAB-17 (no close
others/right), F-TAB-18 (no tab drag — the only drags in the tree are the pane divider and a
test fixture), F-TAB-21 (no Tab menu), F-TAB-24 (vacuous — no tab drag to cancel), F-TAB-25 (no
attach-to-terminal code), F-TAB-26 (no context menu), F-TAB-27 (no resume-chat; the
`resumeAgentSessions` setting flag is not a resume surface), F-TAB-28 (no ⌘W; `ctrl-alt-w` =
`ClosePane` closes a *pane* — live-verified it removed a split pane and is a no-op on a
single-pane tab).

**N/A (1):** F-TAB-06 (browser tab — `NewBrowser` is a typed no-op with an explicit comment).

### F-WIN (12)

**PASSED (2):** F-WIN-01 (workspace↔Settings routing: status-bar gear → full-window settings,
Back wired in the shell; `surface.settings.open/select/read` exercised live over the socket),
F-WIN-07 (restore: `session.restore` socket path, pass 3; the three `restore_*` gpui tests pass
in this snapshot — missing-directory, scrollback replay, Changes mount).

**FAILED — absent (5):** F-WIN-02 (⌘T — no cmd-t binding anywhere in the tree), F-WIN-03
(⌘O/⌘S — no bindings; the editor saves through its own surface, not a shell command), F-WIN-04
(⌃⌘S sidebar toggle — no binding), F-WIN-05 (⌃⌘I right-panel toggle — no binding), F-WIN-10 (no
toast implementation — only the `radii.toast` theme token, used by chat/sidebar prompt cards).

**N/A — platform (5):** F-WIN-06 (no browser), F-WIN-08 (macOS hide-on-close delegate; the Linux
daemon keeps running headless — the platform has no close-to-Dock concept), F-WIN-09 (macOS
title-bar preference), F-WIN-11 (Sparkle updater — no update machinery exists on Linux, and no
general update code either), F-WIN-12 (TCC onboarding sheet).

### F-BRW (9) — the unsupported errors are specific

All nine are FAILED — absent (no browser surface, no permission store, no link-routing), and the
entry text's allowance holds: every one of the 10 `BROWSER_METHODS` answers
`"browser.<method> is unsupported on Linux: Tiller has no browser/webview surface in this build"` —
the method name is echoed and the reason is specific, **clearly distinct from the generic
`unknown control method: bogus.method`** (both exercised live over the socket). F-BRW-06/07/08
(browser-origin permission prompts/persistence/revocation) have no store behind them at all; the
Permissions settings page renders macOS TCC rows only, with an honest comment that it exists for a
permission system this platform does not have.

### F-SID / F-SET / F-USE / F-CHAT spot-checks

**F-SID (19).** New verdicts where the surface moved: F-SID-03 **PASSED** (drawn: + click → real
platform dir-picker prompt → `AddProject` event → shell `add_project`; pass 1's FAILED no longer
holds), F-SID-05 **PASSED** (drawn selection event test; live socket `select-workspace` mounted
Chat+Terminal tabs — the central surface changes), F-SID-10 **PARTIAL** (remove-project × +
confirmation prompt + `RemoveProject` + shell `remove_project` all real; the prompt flow is not
drawn-tested), F-SID-11 **PARTIAL** (row renders path + agent status dot; no branch/comment/
primary text), F-SID-13/15 **PASSED** (drawn tests against real git — worktree created/removed,
porcelain-verified), F-SID-16/17 **FAILED — absent by design** (a test proves a drag-shaped
interaction leaves order untouched; the affordance was deliberately removed — honest, but the
entry's reorder does not exist). Still **FAILED — absent**: F-SID-07 (no gear/project-settings
sheet), F-SID-08 (no init-git), F-SID-09 (no context menu; Finder external), F-SID-12 (no Set
Primary), F-SID-14 (no context menu), F-SID-18 (no "No Terminals" empty state anywhere in the
tree), F-SID-19 (no ⌘T). F-SID-01/02/04 keep pass-1 display verdicts (code present: header/Add,
`on_filter_key`, `toggle_project`); F-SID-06 stays half (status dot + activity model real,
collapsed-project badge appearance display-blocked).

**F-SET.** F-SET-08 **PASSED** (drawn toggle test: resolved socket path + enabled/disabled flip),
F-SET-16 **PASSED** (drawn discovery rows), F-SET-21 **PASSED** (drawn file-icon-set selection —
pass 7's catch holds), F-SET-01 **PASSED** (platform-gated category sidebar test + live
`surface.settings.select`), F-SET-02 **PARTIAL** (Back wired in shell; **Escape is still not
bound in settings**). **F-SET-03 is worse than pass 7 said:** the "Check for Updates" button is a
dead no-op (`|_, _, _| {}` — settings.rs:1266) — the click is drawn-reachable and *does nothing*,
and no checking/up-to-date/error state exists. Version "0.1.0" renders. Update half FAILED.
F-SET-04/05/06/07/09–15/17/18 keep pass-1 verdicts (untouched this pass); F-SET-19/20/22 remain
display-blocked appearance; F-SET-23/24/25 N/A platform (TCC).

**F-USE.** Pass-1 rendering verdicts (01/02/03) stand — `status_bar.rs` owns real provider
fetchers and segments, no drawn tests exist (appearance/display debt), F-USE-04/05/06 (menu-bar
roster, notifications) have no macOS menu-bar equivalent and no user-notification path on Linux —
not exercised.

**F-CHAT (surface only; protocol tier untouched).** The drawn tests healthy: F-CHAT-16 (model
picker select+escape) and F-CHAT-18 (context ring usage + popover + focus) PASSED; F-CHAT-02/08
(retry, connecting) PASSED (drawn); F-CHAT-04 stays PARTIAL (Return verified pass 1; the
shift-return binding exists in chat.rs:475-476 but **no test dispatches it**); F-CHAT-29/20
(copy, tail-follow) have real code. **Absent from this surface:** F-CHAT-17 (no effort levels),
F-CHAT-19 (no context-ring warning colour), F-CHAT-21 (Thought rows render but no Thinking
expand/collapse — no "Thinking" header anywhere), F-CHAT-22/26/27/28/31/32 (no WorkGroup/
pending-question bar / expired-question / subagent task cards / chat diff preview / edit summary),
F-CHAT-34/35 (no chat history menu or empty state), F-CHAT-36 (no no-models fallback), F-CHAT-14
(no overflow menu / Follow Edited Files), F-CHAT-09/10/11/12 (no slash commands, no file/image
mentions or attachment chips — the composer is input + model chip + context ring + send/stop),
F-CHAT-24/25 reduce to the generic Permission card (options/resolved); no Plan card, no
text-answer or cancel on questions. Pass-1's "F-CHAT-23/24/25 PASSED" was the permission-card
mechanism, which is real; the named cards are not.

### Counts

Judged 68 entries (28 F-TAB + 12 F-WIN + 9 F-BRW + 19 F-SID) plus spot-checks of F-SET/F-USE/
F-CHAT. **F-TAB: 7 PASSED · 5 half-proven · 15 FAILED (absent) · 1 N/A.** **F-WIN: 2 PASSED ·
5 FAILED (absent) · 5 N/A-platform.** **F-BRW: 9 FAILED (absent), unsupported errors specific.**
**F-SID: 7 PASSED · 2 PARTIAL · 9 FAILED (absent) · 1 half.** Of the 36 FAILED (absent)
entries on the books after this pass (F-TAB 15, F-WIN 5, F-BRW 9, F-SID 9 — minus F-SID-16/17's
two already counted in pass 1 = 34 distinct features), **seven are newly established this pass**:
F-SID-07/08/09/12/14/18/19, previously held NOT EXERCISED. The F-TAB 15 and F-WIN 5 are pass-7
overturns now verified per-entry; the F-BRW 9 carry pass 2's FAILED, now with the specific-error
contract exercised live. Two pass-1 FAILEDs (F-SID-03, F-SID-05) are upgraded to PASSED. The single most important correction:
**the exact F-TAB absent list is 02, 08, 09, 12, 13, 14, 16, 17, 18, 21, 24, 25, 26, 27, 28**
(15 entries), plus two half-absent UI halves (F-TAB-11's reason display, F-TAB-23's Pane menu /
left-up directions) — pass 7's "13" was the mega-row count; the build list should read these
fifteen names.

**Biggest gap that is not the display:** the shell has **no menu bar at all** — every one of the
F-TAB/F-WIN/F-SID context/overflow/close-others/move menus and the F-WIN command bindings
(⌘T/⌘O/⌘S/toggles) hangs off it, and the only interaction surface the shell owns is the tab strip
(activate/✕) plus the + menu; nothing else a user would point at exists. That single missing
layer explains 19 of the 29 absent non-browser features (F-TAB 02/08/09/12/13/14/17/21/26/27,
F-WIN 02/03/04/05, F-SID 07/08/09/12/14); the rest are distinct debts (F-TAB 16/18/24/25/28,
F-WIN-10 toasts, F-SID 16/17 drag-reorder-by-design and 18/19 empty-state/⌘T).

## PASS 9 — the definitive per-entry ledger (snapshot 2026-08-13T12:27:17Z, 14:27:17 CEST)

Fresh-context critic. Produced `docs/linux-rewrite/INVENTORY-LEDGER.md`: one row per entry for
all 388 (217 app + 171 packages), verdict + evidence + judged, from both inventory files.
Snapshot: tree copied to `/tmp/critic-pass9` at 12:27:17Z; `cargo build -p tiller -p tiller_control`
clean (4.87 s); headless launch served `TILLER_SOCKET=/tmp/critic9.sock` — ping pongs, 42 methods
advertised (the P37 mutation doors `surface.changes.stage/unstage/discard/stage_all/discard_all`
exist; `browser.*` answers its specific unsupported error, distinct from `unknown control method`;
`surface.changes.stage` on a bogus path answers git's own "selected paths are stale" — the door is
real). `opencode`/`omp` verified absent from PATH (14:27) — the UNREACHABLE set stands.

Nothing was re-exercised beyond that; the ledger consolidates passes 1–8. Every row whose
`judged` is not a pass number is marked `builder-claimed, unverified` or `never claimed` and
does not count toward done.

**Totals (the number nobody could state before):** PASSED **88** · half-proven **46** ·
FAILED — absent **90** · FAILED — defective **4** · UNREACHABLE **9** (opencode/omp not
installed) · N/A — platform **17** · NOT EXERCISED **127** · NOT EXERCISED — blocked on display
(appearance) **7**. Sum: 388. Entries never independently judged by the critic: **120**
(80 builder-claimed, unverified; 40 never claimed by anyone).

**Corrections the ledger makes to INVENTORY-STATUS.md:** (1) F-PER-06 is FAILED — defective, not
passed: pass 6 measured compound-command panes orphan their process groups on app quit (simple
panes flush — pass 4's "zero stray" only covered its own cases). (2) F-AUTO-02..08 are critic-
confirmed at pass 3 (not "awaiting pass 4"); F-AUTO-01 half-proven, F-AUTO-09 N/A. (3) F-SET-08
half-proven: the drawn toggle + resolved path pass 8 proved; the copy-install-command half has no
evidence and no install mechanism exists (pass 6 CLI-02). (4) F-CHG-10/14 PASSED, F-CHG-11
half-proven (no Unstage all / section-level actions). (5) F-CHAT-24/25 FAILED — absent as the
named Plan/Question cards (the generic Permission-card mechanism that pass 1 passed is real).
(6) "≈196 never touched" is now per-entry: 127 NOT EXERCISED + 7 appearance-blocked.

**Largest absent blocks:** the shell command layer — one missing menu bar counted 19 times
(F-TAB 02/08/09/12/13/14/16/17/18/21/24/25/26/27/28, F-WIN 02/03/04/05/10, F-SID 07/08/09/12/14),
plus the terminal context/empty-state quintet (F-TERM-02/04/05/06/11); the chat surface (19
F-CHAT absent: mentions/attachments/plan-question cards/history/thinking/subagent cards);
the editor (11 F-EDIT absent). Biggest single non-display gap stays the missing menu bar /
command layer; the biggest measured defect is the process-group leak on close/quit
(F-PER-06, F-TERM-08, F-CTRL-WORK-05 — same root).

## PASS 10 — the 127 nobody had ever exercised, in bulk (snapshot 2026-08-13T12:40:06Z)

Snapshot: whole tree copied to `/tmp/critic-pass10` at 12:40:06Z; `cargo build -p tiller
-p tiller_control` clean in 14.14 s; headless launch served `TILLER_SOCKET=/tmp/critic10.sock`
(ping→pong). Suites run green against the snapshot: tiller_activity 70, tiller_persistence 27,
tiller (lib) 61, tiller_project 49, tiller_usage 33, tiller_markdown 28, tiller_terminal 13.
Ledger rows updated as judged; 102 verdict changes total (99 NOT EXERCISED converted, 3 pass-9
UNREACHABLE overturned).

### F-CORE-ACT (23 + 3 overturns) — the model is real; the app only wires Layer A

All 26 activity rows judged. The 70-test suite is per-entry mapped, not a bulk claim: every
clause case in ACT-01..23 has a named test except where noted. Three pass-9 UNREACHABLE rows
were wrong: ACT-12/13 (title identity/status) and ACT-15 (content detection) are pure
classification functions — exercisable headless by feeding title/text through the model, and
the tests do exactly the VERIFY's "set each exact form" cases (✳/. /π/π:/name tokens/bare
spinner; proceed/esc/y-n/confirm/nonmatch). Real `opencode`/`omp` are not needed for those
claims; they stay environment-gated only for live-CLI observation.

The finding that matters most, recorded once here and reflected per-row: **in the running app,
only Layer A is wired.** `agent_spawned` + control-socket `notify` feed the model (verified
live: `tillerctl notify` accepted, socket path exercised); nothing in `tiller/**`,
`tiller_terminal/**`, or `tiller_ui/**` ever calls `handle_title_change`, `apply_content_signal`,
`detect_content_status`, or `refresh_process_signal`. Layers B (title), C (content), and D
(process) are complete, tested, and dead code in the app. That is why ACT-08 is half-proven
and why the sidebar today shows only hook/spawn-derived status. ACT-17 and ACT-19 are PASSED
with a note: the inventory clauses misstate the reference — identity selection follows status
priority (== Swift `agentIdForWorktree`), and the notification title is
`<display name> — <status label>`, not a worktree label; the Rust port matches
`AgentActivityModel.swift` line-for-line on both. ACT-24/25/26 are half-proven: their planner
halves are tested, but the session-restore relaunch, the launch remount, and the eviction side
effect have no app caller (`ids_to_evict` has zero call sites).

### F-PERSIST (13) — the layer under everything, finally judged

P38's 2/5/1/5 split verified per entry, with corrections. PASSED: DB-01 (open/in-memory/
forward-only migrations/refusals/64 MiB cap/two-process concurrency, 22 integration tests),
DB-04 (scrollback byte-exact round trip + 256 KiB bound at encode AND decode), DB-10
(session-ref upsert/load/delete + survive reopen), PLAT-01 (XDG checkout-scoped path policy
tested). FAILED — absent: DB-02 (seven entity families absent), DB-05 (no legacy tabs, no chat
persistence). half-proven: DB-03 (worktree comment/timestamps persisted nowhere), DB-06
(accounts absent), DB-07 (no quarantine; corrupt state dropped by fallback), DB-08 (no primary
exclusivity, no exact-path lookup), DB-09 (no per-record corrupt skip), DB-11 (Linux has its
own v1–v5 lineage; named v18 migrations absent). DB-12 UNREACHABLE — no `terminalTab` table
exists to mismatch.

### The P36 block (44) — mostly green, four real gaps

PASSED 27 · half-proven 8 · FAILED — absent 2 · N/A — platform 5 · (no display-blocked rows in
this block at pass 10). The dead-enum finding: `LayoutCommand` (WSP-04, all 8 commands) has
zero callers outside `tiller_project`. FILE-03's quoted-string builder is likewise callable
only from tests — nothing ever writes a drop to a pane (that is PTY-06's absence too).
FILE-08 has no per-file icon lookup. WSP-08's view-state struct exists but the session store
never persists it. USG-08 was **exercised live**: `ClaudeUsageFetcher` drove a real hidden
claude PTY and returned Success (session 4%, weekly 30%) in 11.6 s. USG-06/07 stay half-proven:
the live refresh path would rotate the user's real OAuth tokens, so only the bounded-fetch and
401-classification paths are evidenced.

### F-SET (9) — the no-op-button pattern, three more of them

SET-05/06/07 are FAILED — defective: the General settings fields exist in the report model
(auto_naming, chat_retention, mounted_worktrees) but are report-only — `SettingsSnapshot`/
persistence carries no such keys — and the summarizer/copy-install/install-skill buttons are
empty closures (`|_, _, _| {}`), the same dead-no-op shape as F-SET-03. SET-12/13/14/15/17/18
FAILED — absent (no cookie UI, no account management, no registry, no install/update actions).

### F-USE / F-TERM singles

F-USE-04/05 N/A — platform (menu-bar roster has no Linux surface). F-USE-06 FAILED — absent:
`NotificationPolicy` exists in the crate but `should_notify`/`build_payload` have zero app
callers — no status-change notification is ever delivered. F-TERM-10 **exercised live**:
a `sleep 300` pane survived select-workspace away and back with scrollback intact.
F-TERM-SCR-02/PTY-06/PTY-07/PTY-08/UI-02 FAILED — absent (no settle/resize debouncer, no
drop-to-pane, no stable host/generation, no pane cache, no URL router). TERM-02 absent at
snapshot but `tiller_terminal/src/context_menu.rs` appeared in the live worktree after the
snapshot — in-flight, do not re-judge until the next snapshot. TERM-PLAT-01 PASSED (gpui +
alacritty_terminal, no webview).

### What is left

NOT EXERCISED is now 28: F-PRJ-01..18 (the Add-Project sheet) and F-CHAT-03/05/06/07/13/15/
20/29/30/33 — all in `tiller_ui/**`, the crate the brief told me to avoid while it is being
rewritten. `tiller_ui` showed no file modifications during this pass; the next pass should
take it with a fresh snapshot. Display-blocked rows remain 7.

### The single biggest gap that is not the display

**Layers B, C, and D of agent activity detection are complete, tested, and unwired.** Every
pane status in the running app comes from Layer A alone (spawn + hook). Titles, scrollback
content, and `/proc` process evidence are ignored by the app even though the crate machinery
behind them is the most-tested code in the repository. One wiring pass in `main.rs` (feed the
PTY title event, the settled scrollback tail, and a process poll into the existing model
methods) would activate three evidence layers at once — and that, not the display, is the gap
worth the next hour.

New totals: PASSED 146 · half-proven 64 · FAILED — absent 106 · FAILED — defective 7 ·
UNREACHABLE 7 · N/A — platform 23 · NOT EXERCISED 28 · NOT EXERCISED — blocked on display 7.
