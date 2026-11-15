# Claude Code's `/ide` in Sirio: Sirio as the IDE of the Claude in its terminals

Status: design approved; planned (§10 records the calls made while planning). Brainstormed with Enzo on 2026-11-15.

## §0 Intent

Claude Code, run inside VS Code or JetBrains, is connected to the editor: it sees what the
user selects, the user can point it at lines (`@file#L10-20`), it reads the editor's
diagnostics, and it shows each proposed edit as a diff in the editor, where the user accepts
or rejects it. Run inside a Sirio terminal, it gets none of that. In Enzo's words, "some
integrations are missing, like the ones you listed".

This project makes Sirio the IDE that Claude Code's `/ide` connects to, for every Claude Code
running in a Sirio terminal: hand-typed or launched from the agent button.

## §1 Facts this design rests on

Read from the Claude Code **2.1.325** binary (minified JS embedded in the native executable),
not yet confirmed by a live run. §8.1 makes the live confirmation the plan's first task. The
public description of the protocol is the reverse-engineered
[claudecode.nvim PROTOCOL.md](https://github.com/coder/claudecode.nvim/blob/main/PROTOCOL.md);
there is no published contract, so the same posture as `claude-chat.md` applies: degrade, do
not reject.

1. **Discovery.** Claude scans `<CLAUDE_CONFIG_DIR or ~/.claude>/ide/*.lock`, and also
   `~/.claude/ide/` when `CLAUDE_CONFIG_DIR` is set. The file name is `<port>.lock`; the body is
   JSON `{ pid, workspaceFolders, ideName, transport: "ws", authToken, runningInWindows }`.
   At startup Claude deletes a lockfile whose `pid` is not alive.
2. **Auto-connect.** The TUI connects on its own when `CLAUDE_CODE_SSE_PORT` is set (or the
   `autoConnectIde` setting is on), unless `CLAUDE_CODE_AUTO_CONNECT_IDE=false`. A lockfile
   whose port equals `CLAUDE_CODE_SSE_PORT` is taken without any folder match; otherwise Claude
   takes the one lockfile whose `workspaceFolders` contain its cwd (and waits up to 30 s for
   exactly one). `/ide` lists the candidates by hand. The port comparison holds because Claude
   reads its environment through a typed registry that declares `CLAUDE_CODE_SSE_PORT` an
   integer (`H.int()`), so the number parsed from the file name meets a number. When Claude
   believes it runs in an IDE's own terminal (`TERM_PROGRAM`/`TERMINAL_EMULATOR` naming VS Code
   or JetBrains, or `FORCE_CODE_TERMINAL`), it also drops every lockfile whose `pid` is not one
   of its ancestors, except the port-equal one. `ENABLE_IDE_INTEGRATION` does not occur in the
   binary.
3. **Transport.** WebSocket to `ws://127.0.0.1:<port>`, header
   `x-claude-code-ide-authorization: <authToken>`, JSON-RPC 2.0 speaking MCP (`initialize`,
   `notifications/initialized`, `tools/list`, `tools/call`, `ping`). After `initialize` Claude
   sends the notification `ide_connected { pid: <claude's pid> }`.
4. **What the CLI actually uses.**
   - `tools/call openDiff { old_file_path, new_file_path, new_file_contents, tab_name }`, blocking
     until the IDE answers. Answers: `[{type:"text",text:"FILE_SAVED"},{type:"text",text:<final
     text>}]` is an accept (with the user's edits, if any); `DIFF_REJECTED` is a deny (Claude
     aborts the turn and waits for the user); `TAB_CLOSED` is an **accept** of the proposal as
     sent. A diff identical to the old text is a deny.
   - `close_tab { tab_name }` when the terminal prompt was answered first (the prompt's
     teardown), and `closeAllDiffTabs {}`.
   - `getDiagnostics { uri? }`: per file before and after an edit (500 ms budget; after 3
     consecutive timeouts Claude stops asking) and for the whole workspace (2000 ms budget).
     The answer is one text item holding a JSON array
     `[{ uri, diagnostics: [{ message, severity: "Error"|"Warning"|"Info"|"Hint", range:
     {start:{line,character}, end:{line,character}}, source, code }] }]`.
   - Notifications it listens for: `selection_changed { text, filePath, fileUrl, selection:
     {start, end, isEmpty} }` (shown as "⧉ N lines selected" in its prompt) and
     `at_mentioned { filePath, lineStart, lineEnd }` (0-based; Claude inserts
     `@path#L<start+1>-<end+1>` into its input).
   - The model sees only `mcp__ide__getDiagnostics` and `mcp__ide__executeCode` from the IDE.
     `openFile`, `getOpenEditors`, `getCurrentSelection`, `checkDocumentDirty`, `saveDocument`
     are never called by name by the CLI.
5. **The diff trap.** When any IDE is connected and Claude's `diffTool` setting is `"auto"`
   (the default; Enzo's too), **every** Edit/Write permission prompt hides its inline diff,
   shows "Opened changes in <ideName> ⧉", and calls `openDiff`. Nothing checks that the IDE
   offers `openDiff`. A failed call is only logged. `diffTool` lives only in `~/.claude.json`
   (no environment variable, flag or project setting). So a bridge without a working
   `openDiff` makes every edit approval blind. This is why the diff is in this project.
6. **Headless is different** (live-probed by the native-chat session, 2026-11-15): Claude in
   `-p` stream-json mode never connects through a lockfile, `CLAUDE_CODE_SSE_PORT`, `--ide` or
   an `--mcp-config` `ws-ide` entry, and never called `openDiff`. The native chat is a separate
   project (§6).

Facts about Sirio:

- Every terminal's extra environment is built by `worktree_environment_in`
  (`rust/crates/sirio/src/main.rs`), fixed at the session's creation (`AttachParams` carries no
  environment), and sessions live in the detached host across app restarts.
- `sirio_env::for_child()` strips only `SIRIO_WORKTREE_ID`, `SIRIO_PANE_ID`, `TILLER_PANE_ID`.
- `TerminalView::shell_pid()` and the process walks (`sirio_term_session::terminate::
  descendant_pids`, `sirio_activity::process::child_pids`) already map processes to panes.
- The Changes tab draws a `sirio_git::FileDiff` in two modes, `DiffViewMode::Unified` and
  `Split` (an app-wide global), with word-level highlights, and it already runs
  `git diff --no-index` (`sirio_git::diff_entry`). Its line comments (the composer under a line,
  `sirio_ui::diff_annotations`) exist for change-request review threads.
- `TabKind::Diff` exists; look-at surfaces open in the Secondary half, terminals in Primary.
- `ProblemStore` (`sirio_ui::drawer::problems`) holds every language server's findings per
  worktree-relative path; `ProblemStore::for_path` reads one file.
- Ely's `EditorEvent` has no selection event; `FileView::selection_query` is single-line only.
- "Send selection to agent" (`FileViewEvent::SendSelectionToAgent`) goes only to a native chat,
  and `active_chat_for` excludes terminals on purpose: "pasting into a busy agent's PTY is
  interference Sirio avoids everywhere else".

## §2 Goals, non-goals, anti-goals

Goals:

- A Claude Code started in any Sirio terminal connects to Sirio by itself; `/ide` lists
  "Sirio" for one started elsewhere inside an open worktree.
- Claude sees the user's selection in Sirio's editor; "Send selection to agent" reaches a
  connected Claude terminal as `@file#Lx-y`.
- Claude reads the diagnostics Sirio's language servers already produced.
- Each edit Claude proposes opens as a diff in Sirio, where the user can accept, reject, edit
  the proposal before accepting, or reject with line comments that land in Claude's prompt.

Non-goals:

- The native Claude chat (its own project, §6), Codex, OpenCode, Pi and Oh-My-Pi (no such
  protocol).
- `executeCode` (Jupyter), `openFile`, `getOpenEditors`, `checkDocumentDirty`, `saveDocument`:
  never called by the CLI.
- An editable side-by-side diff widget. The diff stays read-only; editing happens in an editor
  (§4.7).

Anti-goals (a failure even if it technically works):

- Sirio deciding an edit on the user's behalf: no automatic `DIFF_REJECTED`, `TAB_CLOSED` or
  `FILE_SAVED`, ever. Every answer comes from a user action.
- A terminal prompt left saying "Opened changes in Sirio" with nothing open in Sirio.
- Sirio typing into a busy agent's terminal.
- Any user-global Claude config written by Sirio (`~/.claude.json`, `settings.json`).
- A port reachable from another machine, or a connection accepted without the token.

## §3 Approaches

How it is built (the server):

1. **Chosen: one server in the app.** One port, one lockfile listing every open worktree, the
   same `CLAUDE_CODE_SSE_PORT` in every terminal, the connecting Claude mapped to its pane by
   pid ancestry; port and token persisted so terminals that survived a restart stay valid.
   Same shape as VS Code (one server per window).
2. One port per terminal: exact routing without process walks, but dozens of listeners and
   lockfiles, several "Sirio" entries in `/ide`, and every port to re-open after a restart.
3. Server in `sirio_host`: the port never changes, but selection and diagnostics live in the
   app, so the host protocol grows new verbs, a no-host fallback needs a second server, and the
   host stops being minimal.

What is built (scope): Enzo first chose "context now, diff later"; §1.5 showed that context
without `openDiff` blinds every edit approval, so he chose **context and diff together**.

How the diff is drawn: three new layouts were mocked (side-by-side, unified, inline in the
file tab); Enzo asked to **reuse Sirio's existing diff** (the Changes view), which already has
both side-by-side and unified modes. Editing the proposal: **an Edit button** that shows the
proposal in a normal Ely editor, rather than an editable diff. Line comments: **included**, as
"Reject with comments".

## §4 Design

### §4.1 The server, the lockfile, the switch

- A new leaf crate, `sirio_ide`: the listener, the WebSocket handshake and the token check, the
  MCP JSON-RPC types and dispatch, the lockfile writer, and the pure owner resolution (§4.2).
  No GPUI. Its one dependent is `sirio`, which turns requests into UI and answers them through
  channels, the way `sirio_opencode_watch` runs its watch on a thread of its own.
- Bind `127.0.0.1` only (IPv4 loopback; never `0.0.0.0`). The token is 128 bits from the OS
  CSPRNG, lowercase hex. An upgrade without the exact header is refused before any JSON-RPC.
- **Port and token persisted** in Sirio's own data. At startup Sirio binds the saved port; if it
  is taken, it picks a free one in 10000–65535 and saves it. Terminals born with the old port
  still find Sirio through the folder match of §1.2.
- **Lockfile** at `~/.claude/ide/<port>.lock` (always scanned by Claude, whatever
  `CLAUDE_CONFIG_DIR` an account uses): `pid` = the app's pid, `workspaceFolders` = the root of
  every worktree of every project in Sirio's sidebar, parked or not (rewritten when one is
  added or removed), `ideName` = `"Sirio"` (`"Sirio (debug)"` in a debug build),
  `transport: "ws"`, `authToken`, `runningInWindows: false`. Written atomically, owner-only
  permissions. Removed on quit and when the switch is turned off.
- **Which lockfile is Sirio's.** Exactly one: the file named by the port this instance has just
  bound. Two instances (the installed app and a debug build, each with its own data) can never
  bind the same port, so they never share a file name; a file with that name left by a dead
  process is overwritten, and no other lockfile is ever read, rewritten or deleted (Claude
  deletes dead-pid lockfiles itself). With two instances, each one's terminals connect to it
  through the port; `/ide` lists "Sirio" and "Sirio (debug)"; a `claude` started outside Sirio,
  in a worktree both list, does not auto-connect (Claude wants exactly one) and is pointed with
  `/ide`.
- **Environment.** Every terminal gets `CLAUDE_CODE_SSE_PORT=<port>` through
  `worktree_environment_in`. The name joins the list `sirio_env` strips from the inherited
  snapshot, so a Sirio launched from a VS Code terminal never hands VS Code's port to its panes,
  and the native chat never inherits it. `ENABLE_IDE_INTEGRATION`, which claudecode.nvim sets,
  is not set: Claude 2.1.325 never reads it.
- **Known, not changed:** a Sirio launched from a VS Code or JetBrains terminal passes that
  terminal's `TERM_PROGRAM`/`TERMINAL_EMULATOR` to its panes (it does today). Claude then
  believes it runs inside that IDE; the port-equal lockfile is exempt from the ancestor filter
  (§1.2), so Sirio's terminals still connect. Setting Sirio's own `TERM_PROGRAM` is a separate
  decision, out of scope.
- **Switch.** On by default: Settings, in the section that already holds Claude Code's
  settings, "Connect Claude Code to Sirio (/ide)". Off: no listener, no lockfile, no variables
  in new terminals; existing connections are closed (open proposal tabs close as in §4.7).
- The lockfile is a write outside Sirio's own data. `agent-adapters.md` allows those only as
  explicit buttons and startup steps; this becomes the third startup step, written there.

### §4.2 Who is talking: a connection's owner

- On `ide_connected { pid }` Sirio walks that pid's ancestors until it meets a terminal pane's
  `shell_pid()`. The owner is that pane (and through it, its worktree).
- The owner is resolved once per connection; a connection keeps its owner until it closes. A
  pane closed while connected leaves the connection ownerless.
- **No owner** (a `claude` started in an outside terminal inside a worktree, or an ancestry the
  platform cannot read): the worktree is the deepest open worktree containing the process's cwd
  where the platform can read it, else none. Such a connection gets diagnostics and `openDiff`
  (in the file's worktree); it gets no selection and no at-mention.
- Before `ide_connected` arrives, requests are served as for an ownerless connection.

### §4.3 The MCP surface

- `initialize`: echo the client's protocol version, `capabilities: { tools: {} }`,
  `serverInfo: { name: "sirio", version }`. `ping`: empty result.
- `tools/list`: `openDiff`, `close_tab`, `closeAllDiffTabs`, `getDiagnostics`, with the input
  schemas of §1.4. Any other tool or method: a JSON-RPC error (method not found), never a crash.
- Every answer follows §1.4's shapes byte for byte (Claude compares strings such as
  `"FILE_SAVED"`).
- `getDiagnostics` must answer inside 500 ms per file: it reads memory only, never waits for a
  language server.
- Each connection runs on its own thread; a slow UI never blocks the read loop. A blocked
  `openDiff` waits for its answer without holding other requests of the same connection.

### §4.4 Selection → `selection_changed`

- ~~A new Ely hook~~ (superseded by §10.1: the file view detects the change itself): the code
  editor's primary selection or caret moving is the trigger.
- `FileView` turns it into one app-level event `{ path, range, text }`, **consumable by more
  than one subscriber** (the native-chat project subscribes too, §6).
- The bridge sends `selection_changed` to every connection whose owner's worktree holds the
  file: line 0-based, character in UTF-16 code units (VS Code's convention), end exclusive,
  `isEmpty` for a bare caret. Coalesced: at most one notification per connection per 100 ms,
  always ending on the latest state.
- Focusing a file tab sends its current selection; focusing a non-file surface sends nothing
  (Claude keeps the last one, as with VS Code). Files outside every worktree reach no one.
- The text sent is the selection's text, capped at Claude's own display needs (the builder
  checks what Claude reads; the full text is not required for "N lines selected").

### §4.5 "Send selection to agent" → `at_mentioned`

- The target becomes **the eligible agent of the file's worktree the user used last**. Eligible:
  a native chat (as today), or a terminal pane that owns a live connection. A Codex, OpenCode or
  Pi pane, a Claude terminal that is not connected, and an ownerless connection are never
  targets; when no eligible agent exists, the command shows a toast ("No Claude terminal or
  chat in this worktree") instead of doing nothing silently. "Used" means focused.
- To a connected Claude terminal: `at_mentioned { filePath, lineStart, lineEnd }` (0-based,
  `lineEnd` the last selected line), then focus that terminal so the user can keep typing.
  Claude inserts the mention itself, when its input is ready: Sirio writes nothing into the PTY.
- This changes today's choice among several chats (first found → last used), deliberately.

### §4.6 Diagnostics → `getDiagnostics`

- One shared read: diagnostics for an absolute path, mapped through `worktree_and_relative` to
  `ProblemStore::for_path`, every server's findings for that file (§6 shares it).
- With `uri`: that file, or an empty array when it is outside every worktree or has no server.
  Without `uri`: every file with findings in the connection's worktree (§4.2: the owner's, or
  for an ownerless connection the one its cwd resolved to); an ownerless connection with no
  resolved worktree gets every worktree's findings.
- Severity maps to `"Error"`, `"Warning"`, `"Info"`, `"Hint"`; ranges convert from the store's
  representation to 0-based line and UTF-16 character.

### §4.7 The proposal tab (`openDiff`)

What opens:

- A tab of the owner's worktree (the file's worktree when ownerless; Sirio's current worktree
  when the file is in none), in the **Secondary** half, so the Claude terminal stays visible in
  Primary. It becomes the active Secondary tab, Secondary is revealed if hidden, and keyboard
  focus stays where it was (the user may answer in the terminal).
- Title: `✻ <file name>`. Body: the Changes diff of the file on disk against
  `new_file_contents`, drawn by the Changes view's diff (Unified or Split, following the
  app-wide mode, with its word highlights). The diff is computed the way Changes computes an
  untracked file's: the proposal written to a private temporary file, `git diff --no-index`,
  `sirio_git::parse_diff`. A file that does not exist yet diffs against nothing (all added).
- A bar above the diff: "Claude · <owner terminal's title> proposes a change · +a −d", and
  **Edit**, **Reject**, **Accept**.
- Paths in the request are untrusted: Sirio only reads them, only regular files (no FIFO,
  device or directory), with the Search drawer's caps in spirit. A proposal Sirio cannot draw
  (binary, too large, unreadable) still opens the tab, says why, and keeps Reject and Accept,
  so the terminal prompt is never left pointing at nothing.
- When the file is open and dirty in an editor tab, the bar warns that accepting lets Claude
  overwrite the file on disk (the editor's existing external-change handling takes it from
  there).

What the buttons answer:

- **Accept**: `FILE_SAVED` plus the proposal (or the edited text). The tab closes. Claude itself
  writes the file.
- **Reject**: `DIFF_REJECTED`. The tab closes.
- **Edit**: the body switches to an Ely code editor holding the proposal (highlighted as the
  file's language); a "Show diff" control switches back to the diff of disk against the edited
  text. Accept then sends the edited text, with the file's original line endings restored (the
  Ely buffer is LF only; a CRLF file must not come back as a whole-file change).
- **Closing the tab** (its ✕, or closing its worktree): asks "Closing rejects Claude's change"
  with Reject / Cancel. Closing never accepts, despite `TAB_CLOSED` meaning accept.

What Claude can do to it:

- `close_tab { tab_name }` (the terminal answered first): the tab closes without asking and
  Sirio answers `TAB_CLOSED`. Comments being written are copied to the clipboard with a toast
  saying so.
- `closeAllDiffTabs`: closes that connection's proposal tabs, answers `CLOSED_<n>_DIFF_TABS`.
- `openDiff` with the `tab_name` of an open proposal replaces it in place.
- The connection closing (Claude exited, crashed, or the switch went off): its proposal tabs
  close with a toast; no answer is owed any more.
- Several proposals at once (several Claudes, or one Claude's parallel subagents) are several
  tabs, each answering its own request.

### §4.8 Reject with comments

- The proposal's diff offers the Changes view's **+** beside each line (old or new side) and its
  composer; comments show under their lines. With at least one comment, Reject reads
  "Reject with N comments".
- On press: Sirio answers `DIFF_REJECTED`, closes the tab, then waits for the owner terminal's
  Claude to be back at its prompt, and types the comments into it **without Enter**, one per
  line, each as `@<path>#L<line or range> <comment>`, the path in the form Claude prints for an
  at-mention. The user reads it and sends it.
- This is a deliberate, narrow exception to "Sirio never types into an agent's terminal": only
  after the user's own Reject, only when that Claude is idle, never with Enter.
- Fallback, whenever it cannot be done safely (no owner, the pane is gone, Claude is not back at
  its prompt within a few seconds, or it is busy again): the text goes to the clipboard and a
  toast says "Comments copied: paste them into Claude".
- Which signal reliably means "back at its prompt" after a `DIFF_REJECTED`, and how to paste
  several lines so Claude shows them instead of collapsing them into "[Pasted text]", are
  verified live first (§8.1). **Decided by Enzo:** if either cannot be had reliably, the
  feature ships with the clipboard as its only delivery (comments still written beside their
  lines, copied already formatted on Reject, with the toast); it is not cut or deferred.

### §4.9 Restarts and the host

- Terminals outlive the app in `sirio_host`; their environment does not change. With the port
  persisted, a restarted Sirio listens where they expect it. Whether a Claude that lost its
  connection reconnects by itself, or needs `/ide`, is verified live (§8.1); if it needs `/ide`,
  the release notes say so.
- Nothing here touches `sirio_host` or its protocol.

### §4.10 Platforms

macOS is the reference platform. Owner resolution needs the process ancestry on each platform:
Linux `/proc`, macOS libproc (both already walked by `sirio_activity`), Windows the toolhelp
snapshot. The lockfile on Windows is `%USERPROFILE%\.claude\ide`. `runningInWindows` is for a
Windows IDE serving a WSL Claude, which Sirio does not do: always `false`.

## §5 Constraints

- `sirio_perf` labels stay content-free: no path, selection, diff text or comment in a label.
  No log line carries file contents, selections, comments or the token.
- `sirio_ide` is a leaf: `sirio_perf` and `sirio_env` at most below it, no GPUI.
- The control socket gets debug-only verbs for the E2E (read the bridge's state, press a
  proposal's buttons, write a comment); nothing that answers Claude is reachable over the socket
  in a release build.
- Never decide for the user (§2 anti-goals): a test asserts that no code path answers
  `openDiff` without a user action, a `close_tab`, or the connection closing.

## §6 Shared with the native-chat project

The native chat connects through an SDK-hosted MCP server named `ide` over its own stdio
(the other session's spec), not through this server or lockfile. It reuses two pieces built
here, which must therefore be shared APIs rather than bridge internals:

1. The editor's selection-changed event (§4.4), with more than one subscriber.
2. The per-file diagnostics read (§4.6).

This project lands both first and owns their contract: the event carries the absolute path,
the primary selection as 0-based line and UTF-16 character (end exclusive) and its text, and
lives as long as the file view; the read takes an absolute path and returns every server's
findings for it, already mapped to §1.4's severities and coordinates. The native-chat spec
(written by a separate session, not yet in the repo) builds on that contract and adapts to it.

## §7 Docs to update with the change

- `docs/agents/architecture/agent-adapters.md`: the third startup step (the lockfile), the two
  environment variables.
- `docs/agents/architecture/crates.md`: `sirio_ide`.
- A new `docs/agents/architecture/ide-bridge.md`, linked from `CLAUDE.md`'s Architecture list
  with its one-line rule ("never answer `openDiff` without the user").
- `docs/agents/e2e-tests.md`: the new E2E and the live test.
- ~~`rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`: the selection hook~~ (none, §10.1).
- `docs/agents/architecture/text-selection.md` if the selection hook touches what it covers.

## §8 How it is proven

### §8.1 First: confirm §1 live (throwaway)

Before any product code, a throwaway probe confirms against the real Claude Code 2.1.325, in a
private HOME, with `sirio_term_session/tests/live/fake_messages.py` as the model:

- a stub lockfile plus a stub WebSocket server: auto-connect through `CLAUDE_CODE_SSE_PORT`,
  `ide_connected`, the exact `openDiff`, `close_tab`, `getDiagnostics` traffic of one Edit;
- the four answers of §1.4 and what each does to the terminal prompt;
- what the terminal shows after `DIFF_REJECTED`, and which signal says Claude is back at its
  prompt (Sirio's activity status, OSC 7501, or neither); how pasted multi-line text appears;
- what Claude does when the server goes away and comes back on the same port;
- that the port-equal lockfile wins with two Sirio lockfiles listing the same worktree (the
  typed-integer reading of §1.2), and under an inherited `TERM_PROGRAM=vscode`.

Anything that contradicts §1 comes back to the design before the plan goes on.

### §8.2 The E2E

`Scripts/Tests/test-ide-bridge-e2e.sh`, printing `IDE BRIDGE E2E OK`, against the real Sirio
binary, isolated (private HOME, `SIRIO_DB`, socket path), with a scripted client playing Claude
from **inside** a Sirio terminal (so owner resolution is real). It covers: the lockfile and the
environment; the token refused; selection and at-mention arriving; diagnostics for a file with a
seeded finding; Accept, Edit then Accept (the edited text comes back), Reject, Reject with
comments (the text lands in the pane without Enter); `close_tab`, `closeAllDiffTabs`, the
connection dropping; the switch off; a restart keeping the port. Its artifact: the client's
JSON-RPC transcript, the socket transcript, the lockfile (token redacted), and PID-matched
captures of the proposal tab in both diff modes, under `--out-dir`.

### §8.3 The opt-in live test

The real `claude` in a Sirio terminal against `fake_messages.py` proposing one Edit: the
terminal shows "Opened changes in Sirio ⧉", the proposal tab opens, Accept makes Claude write
the file; a second Edit rejected with a comment lands the comment in Claude's prompt. By hand
only, like the other live tests; a missing CLI prints SKIP.

## §9 Left to the builder

- The WebSocket library (a synchronous one on threads fits Sirio; no async runtime is present)
  and the thread layout, within §4.3.
- Where port and token are persisted (a settings field or a small table).
- How the Changes diff body is extracted from `ChangesTab` for reuse (it carries comments,
  expansion and terminal-drag state today), and how its composer is reused outside a change
  request.
- The exact settings section and wording, within §4.1.
- The 100 ms coalescing and the comment-delivery wait, within §4.4 and §4.8, adjusted to what
  §8.1 finds.
- The scripted client's language (it needs a WebSocket client).

## §10 Decided while planning (2026-11-15)

Calls made while writing the plan, on what the code holds today. Each overrides the section
it names; the live probe (§8.1) adds its findings here before any product code.

1. **Selection without an Ely hook** (overrides §4.4's first bullet and §7's
   `LOCAL-CHANGES.md` line). `FileView` already observes its code editor on every notify
   (`file_view/code_surface.rs`, the observer that refreshes the caret line). That observer
   compares the primary selection's byte offsets with the last ones seen and, only when they
   differ, emits `FileViewEvent::SelectionChanged { path }`; focusing a file tab emits it too
   (no notify comes from a focus). The contract data of §6 is read on demand with
   `FileView::ide_selection(cx)`: absolute path, 0-based line and UTF-16 character, end
   exclusive, and the text. A drag changes the selection on every mouse move, so the UTF-16
   conversion and the text are computed only when the coalesced notification is sent, not
   per event. No vendor change.
2. **Selection text cap.** Claude 2.1.325 hands the selection's text to the model ("The user
   selected the lines X to Y from <file>: …") cut to 2000 characters with " ... (truncated)".
   Sirio sends at most 4096 UTF-16 code units, cut on a character boundary, so Claude's own
   marker still appears; the line range is always exact.
3. **Owner resolution.** Pure in `sirio_ide`: a walk **up** from the connecting pid through
   its parents (at most 32 steps) until it meets the `shell_pid()` of any terminal Sirio holds,
   mounted or parked, with the parent lookup injected. The app supplies it from a new public
   `sirio_activity::process::parent_pid` (Linux `/proc/<pid>/stat`, macOS libproc, Windows one
   toolhelp snapshot per resolution), on the background executor. A connection left ownerless
   is resolved again when a worktree's terminals attach and before any owner-dependent action
   (a restart mounts only the selected worktree at first).
   A process's cwd, for an ownerless connection (§4.2), is read on Linux only
   (`/proc/<pid>/cwd`); elsewhere it is none, and §4.6's every-worktree fallback applies.
4. **The answers Sirio gives, all of them.** `FILE_SAVED` and `DIFF_REJECTED` come only from
   the proposal's buttons or from the user's Reject in the close confirmation. `TAB_CLOSED` is
   only ever the result of the `close_tab` call itself, never of an `openDiff`.
   `closeAllDiffTabs` returns `CLOSED_<n>_DIFF_TABS`. A pending `openDiff` that is closed by
   `close_tab`/`closeAllDiffTabs`, or replaced by another with the same `tab_name`, gets a
   JSON-RPC error (`-32000`), which carries no decision. The probe confirms Claude reads that
   error as neither accept nor reject; if it does not, this comes back to Enzo.
5. **getDiagnostics.** The connection's thread asks the app over a channel and waits at most
   400 ms. The app reads the existing drawers by lookup (a worktree with no drawer has no
   findings; the read never creates one). Past the deadline the answer is a JSON-RPC error,
   not `[]`: an empty baseline would make pre-existing errors look caused by the edit. The
   probe checks whether Claude counts that error toward its three-timeouts cutoff.
6. **The diff.** New `sirio_git::diff_proposal`: Sirio reads the file on disk itself (regular
   file checked on the open handle, at most `sirio_ui::editor::MAX_FILE_BYTES`, 1 MiB), writes
   both sides to private temporary files under Sirio's data directory (beside
   `session::database_path()`), runs `git diff --no-index` with `diff_entry`'s flags, and parses
   with `parse_diff`; the parsed path is rewritten to the real file. An untrusted path never
   reaches git. A missing file diffs against `/dev/null`. The file is read, and the path
   canonicalised, on the connection's thread before the request reaches the app, so a stalled
   file system never blocks the UI.
7. **Reusing Changes.** `ChangesSource::Proposal(Arc<FileDiff>)` and
   `ChangesTab::for_proposal`: the Changes body, both diff modes, word highlights and the `+`
   comment affordance, with no stage or discard controls and a header of its own. The
   change-request composer is private to that tab, so the proposal gets a small composer of
   its own (an Ely input and two text buttons).
8. **The tab.** `TabKind::Proposal` and `PaneContent::Proposal`, never saved, like Trace. A
   proposal of a worktree that is not on screen is held tab-less in the workspace, parked with
   its worktree and put back with it (beside the Trace tabs), and never dropped by an
   eviction. **Every way of closing a proposal tab** (its ✕, Ctrl+W, the tab menu, Close
   Others, Close Tabs to the Right, `surface.tabs.close`) and closing a worktree or project that
   holds proposals first asks once, in the window, "Closing rejects Claude's change" (Reject /
   Cancel), before anything else is closed or any session ended. The release `workspace.close`
   verb, which cannot ask, refuses while the worktree holds a proposal.
9. **Where it starts.** The bridge starts from `main()`, after settings load and before the
   first terminal is created or restored, never from the workspace's constructor: in-process
   UI tests never bind a port or write a lockfile. The lockfile directory is a parameter of
   `sirio_ide`; the app passes `<home>/.claude/ide`. `CLAUDE_CODE_SSE_PORT` joins
   `sirio_env::PER_PANE` (both of its uses are wanted: the launch's value is dropped and the
   native chat never inherits it).
10. **Library and threads.** `tungstenite`, synchronous, without TLS. An acceptor thread and one
    thread per connection; each connection's loop reads with a short socket timeout and in
    between writes what the app queued and expires deadlines, so no request blocks another.
11. **Persistence.** Port and token in the settings key-value table (the typed pattern of
    `active_agent_account_id`), the switch as the setting `claude.ideBridge`, default on. No
    migration.
12. **Comment delivery.** The clipboard path is built first and always works; typing into the
    terminal is added on top only if the probe finds both a reliable idle signal and a paste
    Claude shows as text (§4.8, Enzo's decision), and even then only while the terminal's
    bracketed-paste mode is on: without it a multi-line paste would run each line in whatever
    is in the foreground.
13. **Copy fixed while planning.** A comment on the proposal's side reads
    `@<path>#L<n> (proposed) <comment>` (its line exists only in the proposal); one on the disk
    side `@<path>#L<n> <comment>`; a newline inside a comment becomes a space. Toasts: "Claude
    closed its proposed change" (its connection went away), alongside §4.5's and §4.8's. The
    dirty warning reads "Unsaved edits in this file: accepting lets Claude overwrite it on
    disk".
14. **A proposal for a worktree that is not on screen** is held without a tab (item 8) and,
    unless Enzo decides otherwise, announced with a toast "Claude proposes a change in
    <worktree name>"; the owner terminal's own attention state is not relied on, since a
    hand-typed `claude` may have no activity agent id.
