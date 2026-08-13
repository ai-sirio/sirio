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
