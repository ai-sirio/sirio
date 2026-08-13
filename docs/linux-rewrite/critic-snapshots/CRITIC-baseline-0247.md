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
