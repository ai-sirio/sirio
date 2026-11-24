# End-to-end tests and probes

Each script drives the real binaries. The comment above each line says what it proves, its flags
and the line it prints on success; run the one for the area you changed. `CLAUDE.md` keeps the
gate and the per-crate commands.

```bash
# Live end-to-end test of the update chain: signs a manifest, serves it over
# loopback and drives a probe compiled the way the release job compiles the app.
# Publishes nothing and touches no installed copy. SIRIO_UPDATE_E2E_VERBOSE=1
# prints each stage the probe reported.
Scripts/Tests/test-update-e2e.sh   # -> prints "UPDATE E2E OK"

# The update card in a real, isolated Sirio compiled as Nightly against a
# loopback manifest: the launch card, Restart to update relaunching into the
# swapped stand-in AppImage (stage chain), every card state, Later and the
# indicator, the agent warning, the keyboard and focus (stage ui).
# --stage chain|ui|all (all runs chain, then ui); --display :N is required
# unless --state-only, and must be an Xvfb this user started, never $DISPLAY;
# --out-dir DIR, --appearance light|dark.
Scripts/Tests/test-update-ui-e2e.sh   # -> prints "UPDATE UI E2E OK"

# The host foundation: a real sirio-host started detached, adopted, killed
# around, idled out and drained across a protocol major; --out-dir DIR keeps
# every host's log and state; SIRIO_HOST_E2E_VERBOSE=1 prints each probe line. Cases
# program_is_named .. empty_argv0_is_kept_padded (Linux and macOS; they need python3) type
# real commands into a /bin/sh -i the host owns and read `term.list`'s program and status,
# the OSC 7501 probe answer (at most one a second, and before a DA1 reply), that bytes from
# `term.feed` or the prelude are never scanned, that no argv, script or cwd reaches host.log,
# the `program` and `status` events a `host_probe term-events <ms>` subscriber receives (a report
# repeated five times is one event), and that a program's argv is bounded to 4096 bytes in all.
# The last two, empty_argv0_is_kept and empty_argv0_is_kept_padded, also need perl: bash
# started with an empty argv[0] is listed with it, not with its argv shifted left, whether or
# not its exec path is padded in macOS's `KERN_PROCARGS2` reply. Only macOS tests #719's
# change to `parse_procargs2`: Linux reads argv from /proc/<pid>/cmdline, so there they pass
# with or without it.
Scripts/Tests/test-host-e2e.sh      # -> prints "HOST E2E OK (<n> cases, <s>s)"
# Whether a detached process survives its parent's world, row by row.
Scripts/Tests/probe-host-detach.sh  # -> prints "DETACH PROBE OK"

# The host's terminal sessions (SP2) with a real sirio-host held by a real client,
# driven only through the term.* verbs; one fresh data root per case. --out-dir DIR
# keeps each case's host logs, state files and transcript; SIRIO_TERM_E2E_VERBOSE=1
# prints each probe call.
Scripts/Tests/test-term-e2e.sh      # -> prints "TERM E2E OK (<n> cases, <s>s)"
# The app end to end: a debug sirio on an isolated display adopting or starting a
# real host, killed, quit and relaunched, observed through sirioctl and the host's
# probe. --out-dir DIR keeps logs, state and PID-matched captures; --state-only takes
# no captures (macOS runs that way); --xvfb or --display :N picks the display (Linux);
# --only N[,N...] runs some cases and prints "HOST SESSIONS E2E PARTIAL" instead.
# Case 33 (unread across a restart) has a codex stub that leaves, exit 0, when a file
# appears: the agent stops while the app is closed, and the relaunched app marks it unread
# and posts nothing. The mark survives a second restart and goes with the closed tab. A
# restored terminal opens its session when first drawn, so the agent tab is the active one
# and the window is forced inactive from the first frame (the debug-only launch knob
# SIRIO_DEBUG_WINDOW_ACTIVE=inactive). It needs a display on Linux (--xvfb or --display :N) and
# SKIPs under --state-only; macOS runs it, since its state-only run still draws a window. It
# leaves case33-restored.json, case33-saved.json and case33-closed.json (the
# `surface attention read` of each start) and case33-db.json (the saved `unread, last_running`
# row, read from the world's database between the quits).
# Case 34 (unread in an unmounted worktree) needs no display, so it runs under --state-only on
# every platform: a second worktree B holds a codex agent tab whose record saved running, the
# primary worktree is selected, the app quits and starts again (B is neither mounted nor parked),
# and a bare needs-input hook (Codex sends no Running first) reaches B's pane. B's session reads
# unread, the tray lists B with a leading dot though nothing of B was ever drawn, and `posted`
# holds exactly one entry for it (its tag, its body naming B's branch); selecting B and looking
# clears the mark, the session is listed once, and no second notification is posted. It leaves
# case34-restored.json and case34-shown.json (the `surface attention read` of each moment).
# Case 35 (restored scrollback rows) quits the app, shuts its host down and starts the app again:
# the terminal's session is a new one and its saved rows are above it. Every one starts at column
# zero, the last saved row is whole, and the new shell's first line is a row of its own. It leaves
# case35-restored.txt (the restored pane's text).
Scripts/Tests/test-host-sessions-e2e.sh  # -> prints "HOST SESSIONS E2E OK (<n> cases, <s>s)"
# The sirioctl end to end, each world in its own HOME, XDG directories, SIRIO_DB and
# socket: the stable copy and its PATH link, replacement, a foreign file, the AppImage
# mount kept out of a pane, quit and relaunch, every stub agent's hooks, the skill,
# SIRIO_WORKTREE_ID, OpenCode's global session plugin (case 8: Install Hooks follows `opencode
# --version` of the stub on the PATH, which a file switches between 1.18.33 and 2.3.0: for 1 it
# rewrites the plugin, for 2 it removes it, and says so, only when it is Sirio's generated file,
# and leaves a user's file byte for byte; the app, restarted under 2, removes the generated
# global plugin by itself at startup and logs `removed Sirio's generated global plugin`, and
# leaves a copy with one line of the user's added) and the flags sirioctl offers, `project add --display-name` (case 14:
# the label, no dialog, a duplicate). Case 15 (unread sessions) drives a claude stub
# through `surface attention read` and `act --focus`: a stop off screen marks the session
# until it is shown, a stop in a split pane marks the tab beside the other pane's running,
# and an inactive window keeps the mark. The same case counts the notifications the app
# decided to post (`posted`): one per stop, built from the session's own worktree, none for
# the agent's exit afterwards, and `act --click <tag>` opens the session the way a click on
# the notification does (the app's D-Bus is dead, so the E2E never reaches a desktop). Its
# step 15.7c (`parked_terminal_exit_reaches_status`) is the parked twin of a plain shell's
# exit: the shell exits 3 while its worktree is not selected, and its session reads error
# and is not unread. Its step 15.8a (`settings_cover_is_not_on_screen`) opens Settings with
# `surface settings open`: a stop of the shown agent tab under it marks the session and
# posts one notification, a look does not clear it, and Escape (a real key, pressed in the
# app's window: no verb closes Settings) shows the tab again, which clears the mark. Linux
# runs it with --xvfb or --display :N; --state-only skips cases 4, 4c, 5, 6, 10, 15 and 16 and
# is what macOS runs. --out-dir DIR keeps the transcript, logs and env dumps (and case 15's
# case15-attention.json); --only N[,N...] prints "SIRIOCTL E2E PARTIAL"; --mutate
# claude-prompt-hook is case 6's mutation check. Case 16 opens two Claude agent tabs in one worktree
# (the stub's per-pane mode, `claude.per-pane`, gives each its own handshake directory and a session
# id of its own per process, `stub-claude-<pane>-<pid>`, kept in `claude.session`): the worktree's
# `.claude/settings.local.json` holds no `--session`, both SessionStarts write their own tab's row
# in the world's database and both turns leave each row its own; again after a quit with the host
# down and a relaunch (both tabs prepared in tab order, the selected tab B runs its turn, then A
# once shown) and again after a Restart of tab A.
Scripts/Tests/test-sirioctl-e2e.sh   # -> prints "SIRIOCTL E2E OK"

# An agent typed by hand into a terminal shows its mark: a debug sirio on a private Xvfb (or
# --display :N), in an `env -i` world (its own HOME, XDG directories, SIRIO_DB, SIRIO_HOST_HOME and
# OpenCode service config) whose PATH holds only stubs. A stub (stubs/hand-agents/agent.py) plays
# claude, codex, omp and pi; `node` is python3 and `$RUN/bg/codex` a copy of it. Each case opens a
# terminal with `surface terminal open` (no --cmd: the host names its foreground program), types
# through `host_probe term-write`, and reads the pane's `agent` in `panel list --json`: the four
# agents typed by name (a frame of each), a package script run by `node`, leaving it, a background
# `codex` that Layer D would name but is not the foreground, and an omp title. It then types the
# stub's OSC 7501 reports (work, idle, done, block, error, clear) and `hook <status>` lines and
# asserts the status the tab shows, its unread mark, running flag and the notifications the app
# posted (a baseline per step): a silent startup, claude's, pi's and codex's turns (codex's `idle`
# right after its `done` keeps the finish) (a frame at running,
# needs-input and done), a first `block`, a short turn, an exit or a clear mid-turn, hooks that
# yield to a live report and rule without one, a Sirio-spawned `--agent claude` tab, a
# `system.quit` and relaunch mid-turn (the startup posts nothing on its own), and the argv kept out of the app's stderr. OpenCode, which
# reports nothing in the terminal, is read from its service: a stub (stubs/opencode-server.py: the
# real v2 endpoints, Basic auth, an SSE stream; driven by lines in $RUN/oc.cmd) registers itself
# in the world's own $XDG_STATE_HOME/opencode/service.json, and the cases cover a session linked
# by its title and one linked by being the first in its folder (a frame at running and at done), a
# permission ask, a link kept through a title an older session also has (a typed `opencode` never
# gets a session ref, however it is linked), two OpenCode 2 tabs Sirio launched in one folder (the
# stub's `POST /api/session` log line gives each tab's session: the pane's ref, read from the
# world's database, is that id before any title exists, the tab's process runs `opencode
# --session <id>`, `system.identify` returns it; the first turn shows its status with no title
# (the pane's created session links, +1 at its end) and a session opened inside the TUI
# (`create ses_tui`) takes the link away; a relaunch keeps the refs and the hidden tab
# linked), a service that cannot create the session (500, an answer later than the 3 s deadline
# with the control socket still answering in under a second, an OpenCode 1 on the PATH: plain
# `opencode`, no ref, and under 1 the worktree plugin written, reading its pane from the
# environment), a fresh OpenCode 2 tab whose session is created while a worktree switch has parked it (it
# still runs `opencode --session <id>` and keeps its ref, and nothing is deleted), one closed while its
# session is created (Sirio deletes that session through `DELETE /api/session/<id>`, writes no ref and
# starts no process), an OpenCode 2 tab launched with a first prompt whose Restart, from the context menu
# and from the Session lost overlay (the host killed under it), resumes `--session <id>` with no prompt,
# a claude tab launched with a first prompt whose two Restarts run `claude` without it, a claude tab whose pane id the host refuses (a control session holds it) and that opens again on a fresh id, still sending the first prompt, OpenCode 2 leaving no worktree
# plugin (the real v1 plugin an OpenCode 1 tab made Sirio write, and the legacy one that
# names a pane, removed; a copy of it whose
# sirioctl line is not a JSON string literal, a user's file, and one under Sirio's header line
# with the user's own body, untouched), an OpenCode that
# reports no readable version (`oc-version` `garbage`, the app relaunched: plain `opencode`, no
# POST, no ref, no plugin written and the real v1 plugin an earlier OpenCode 1 tab wrote, and
# the legacy one, removed), and, last because it stops the
# host, a relaunch with no host to attach to that starts `opencode --session <id>`, a turn under a second
# (finished, +1), two panes in one folder (no guess, then each by its title), a failed turn (the
# service's error text and password stay out of the app's stderr), a relaunch, and a service that
# goes away (the mark stays, no status, one `[opencode-watch] <kind>` line). Rows that hold one
# reference under two keys (an older build's) are dropped at startup, a lone one stays. A codex tab's session
# ref row goes with its tab (`surface tabs close`), a stale row under the key the next tab takes
# is dropped as that tab is made, and a ref written for a closed pane (Claude's `SessionEnd`,
# simulated with `sirioctl notify --agent-session`) is refused and logged; a hidden tab's ref
# survives a quit and relaunch, and closing its worktree drops it. Tabs no terminal
# holds are covered too: after a relaunch a turn typed into a tab never shown (codex, an exit mid-turn,
# OpenCode) or into a worktree never selected makes one stop and an unread mark, showing the tab adds
# none, a finished claude turn read before the quit stays silent, and a `panel create` session that
# belongs to no tab gets no state. The end of a hidden agent tab's session
# (a Sirio-launched tab with exit 0 and exit 3, a hand-typed `exec codex`), a host killed under an attached tab
# and a hidden one, and End all sessions end both the way they end the attached tab (one stop, the
# same notification); a session that exited while the app was down posts nothing at the relaunch; closing
# a worktree nobody selected, mid-turn, posts nothing and leaves nothing running; and a Show-session tab
# of a hidden tab's session makes one stop for one turn, the hidden tab's own, and closing it leaves the
# hidden tab fed; a Show tab on screen with the window active makes its owner's turn no news; a Restart of an agent tab mid-turn (launched or typed) posts nothing, leaves no status, and the launched one's next turn is one stop; a session's end is sent once even when
# the host publishes a title after the exit and then closes it. --out-dir DIR
# keeps transcript.log, steps.log (each step's `panel list` and `surface attention read`, and the
# session's status, unread, running and posted count), frames/ and the
# logs (default ~/.cache/st/hand-agents-e2e-<time>; TMPDIR defaults to ~/.cache/st/hand-agents-tmp,
# on disk); --only NAME[,NAME...] prints "HAND AGENTS E2E PARTIAL"; --no-build reuses the binaries.
Scripts/Tests/test-hand-agents-e2e.sh   # -> prints "HAND AGENTS E2E OK (<n> cases, <s>s)"

# The "Project added" dialog after every add from the UI, in a real debug Sirio per scenario (a
# fresh SIRIO_DB, private HOME and SIRIO_PROJECTS_DIR), driven by real X keys and clicks and
# asserted on the debug `surface project-identity read` verb, `project list --json` and the stored
# rows: focus, Enter and Esc, skip against save, the queue, the clone and non-git paths, a Clone
# form dismissed while git runs (by Esc, by a click inside a dialog over it, or replaced by the
# Create form), which still adds its project and opens its dialog like any clone, or reports its
# failure, with the URL's credentials redacted, in the sidebar's notice and a toast, a Create form
# replaced while its folder is still being made (Linux only: an LD_PRELOAD shim, built with cc,
# stalls the app's mkdir), which still adds its project or reports its failure, the Project
# Settings tab's label, the privacy of the perf trace, and the label across a focus refresh and a
# relaunch. A git wrapper in the debug build's own bin directory logs every git call and floods,
# stalls, holds (with a progress line every 5 s, so a held clone stays alive), or fails a clone, or
# stalls a discovery, when a scenario asks. Linux runs it with --xvfb or --display :N, also under
# --state-only, which skips only the captures and the real X input; --out-dir DIR keeps
# transcript.log, <scenario>/app.log, projects.tsv and git-calls.log, and
# frames/<scenario>-*.png; --appearance light|dark (default light); --only NAME[,NAME...] prints
# "PROJECT IDENTITY E2E PARTIAL (<n> scenarios)". It needs xset (x11-xserver-utils) and
# python3-gi with gir1.2-gtk-3.0 for the clipboard, and cc for the mkdir hold.
Scripts/Tests/test-project-identity-e2e.sh   # -> prints "PROJECT IDENTITY E2E OK (<n> scenarios, appearance <a>, state-only <0|1>)"

# Saved prompts in the right panel, in one real, isolated debug Sirio on a private Xvfb, driven by
# real X clicks and keys at the bounds the debug `surface prompts read` verb reports, and checked
# against the prompts folder, the X CLIPBOARD, `surface tabs read`, `surface editor state` and the
# workspace toast: the empty view creates no folder, New and typing save with no Ctrl+S, Copy (also
# right after a key), Edit brings back the prompt's one tab, an untouched New leaves nothing, Delete
# with the tab open never comes back, a plain .md keeps Preview and Ctrl+S, no worktree, an
# unwritable folder (Save failed and a dirty tab, the toast, the save after the fix, and a key with
# Ctrl+W at once making the close ask), a key then at once a worktree switch (one of up to five
# tries must have switched within 400 ms of the last read that did not show its key yet, so before
# its autosave) or Quit (the key is on disk), Delete
# of a prompt split beside a terminal (the tab left is a terminal tab that comes back with its
# session after a switch away and back) and Copy of an open prompt the app cannot read (the toast,
# the marker on the CLIPBOARD untouched). A second app on a fresh database seeded dark shows the
# same three prompts, then closes a prompt's leaf beside a terminal right after a key (the key is on
# disk; an untouched new prompt's leaf leaves nothing). gpui's close prompt and the "Close
# terminal?" banner take clicks only and no verb reports their buttons, so the script finds them in
# a capture by colour (magick). Linux only: --xvfb is required; --out-dir DIR (it must not hold a
# prompts folder) keeps transcript.log, prompts/ (the SIRIO_PROMPTS_DIR the script sets under it and
# refuses to run without), captures/{light,dark}-*.png and both app logs. It needs python3-gi with
# gir1.2-gtk-3.0 for the clipboard, which it reads through X11 even when WAYLAND_DISPLAY is set.
Scripts/Tests/test-prompts-e2e.sh --xvfb   # -> prints "ok 1" to "ok 15", then "PROMPTS E2E OK"

# Folded directories in the Files tree, in one real, isolated debug Sirio on a private Xvfb, driven
# by real X clicks and keys at the row bounds the debug `surface files read` verb reports for the
# last drawn frame. A Maven-like fixture proves: a file made under the third open level shows and
# goes (nested listings stay current), single-child chains are joined into one row at the right
# depth and no link of a chain is ever built as a row of its own (the verb's destructive `seen`
# record, which the script drains on every read into seen.log), the joined row carries the
# outermost link's git status, opening it shows its children joined, a sibling file splits it and
# removing the file rejoins it with the selection kept, Space closes the joined row and a refresh
# does not reopen it, hidden files decide the chain until they are shown (and a row whose links
# disagree, either way round, draws closed and one click opens it), and a worktree switch away and
# back keeps the rows, after a pause in which the precache may rebuild the repository's snapshot
# (the transcript logs whether it did). Linux only: --xvfb is required; --out-dir DIR keeps
# transcript.log (the script's notes and its logged sirioctl calls, not the quiet reads or the
# quit), captures/light-<n>.png, light-5-split.png and light-7-disagree.png (and failure.png on a
# failed run), seen-<n>.log snapshots, git-status-<n>.txt, light-app.log, xvfb.log, and seen.log
# and last-state.txt (the last Files read).
Scripts/Tests/test-files-fold-e2e.sh --xvfb   # -> prints "ok 1" to "ok 8", then "FILES FOLD E2E OK"

# The chat's session panel column in a real, isolated Sirio: a stub `claude`
# (Scripts/Tests/stubs/session-panel/claude over sirio_chat's claude_fixture.py in
# session_panel mode) and the fake forge, asserted on `surface.chat.read`'s `sessionPanel`:
# Environment, the Context fold (asked on open, a plugin node, the CLAUDE.md row
# opening a file tab), Connectors (reconnect, sign in), the Change request card, the
# shared rate-limit pause and the open state across a relaunch. The trace scenarios
# (trace_live, trace_cost, trace_request, trace_interrupt, trace_death, trace_hang,
# trace_subagent_permission, trace_background, trace_background_prompt, trace_reveal,
# trace_tab, trace_idle, trace_keys) use
# Scripts/Tests/stubs/trace/claude, the fixture's trace mode, whose scripted turns the
# first word of a prompt picks (TRACE_STEP_MS paces them): agents and a nested agent, a
# wait and its reveal, the cost of a second turn, Stop, a CLI that dies, a call that never
# reports, a subagent's permission, background agents that outlive the turn's result (the
# turn stays Live, its cost "at least" until they end, the CLI-started turns after them
# charged to it and never drawn, and a prompt written while one runs), the Trace tab across
# a worktree switch, a relaunch and its chat's close, the frames the trace asks for (SIRIO_PERF_TRACE) and real X
# clicks and keys on the tab.
# A10, subagent text: two more trace scenarios, subagent_text and subagent_text_floor, on the same
# stub. Its `subagent_text` prompt runs a foreground subagent that asks for a permission and a
# background one that writes after the turn's result. subagent_text asserts, on `surface.chat.read`
# and `surface.trace.read`: the Agent call is a folded card titled by its description (no plain
# row); the card's items in arrival order (the prompt first, the thought, the text, the calls);
# the lane's latest sentence; a grandchild's line marked with its name in the outer card; no
# hand-back frame anywhere; a background card that fills after the result and after a second turn
# began, and is Completed once its task ends; the end notice as before. subagent_text_floor does
# the same under SIRIO_FIXTURE_FLOOR=1 (the 2.1.257 floor: no line or ask names its agent). They
# write subagent-text.json and subagent-text-floor.json (the cards' final items, the lanes as
# read at the ask and at the first turn's end, a `mode` field of "forward" or "floor").
# The live check is `claude_forwards_a_subagents_text_under_sirios_initialize` (#[ignore],
# acp_conformance): the real CLI against tests/live/fake_subagent_api.py (127.0.0.1) forwards the
# subagent's text on stdout under Sirio's own `initialize`, for a foreground `Agent` call and one
# that omits `run_in_background`, within 60 s.
# The file_links scenario uses Scripts/Tests/stubs/file-links/claude,
# the fixture's file_links mode: one answer holding paths that exist in the worktree, ones that do
# not, a version, a fenced path and a web link, then real X clicks and drags on it, asserted on
# `transcriptLinks` of `surface.chat.read`, `surface.editor.state`, `surface tabs read` and the
# browser stub (a click opens the file, at its line, reusing its tab; a drag opens nothing; the
# web link still reaches the browser). The clipboard_keys scenario, on the same stub, presses the
# clipboard chords with real X keys against the display's real CLIPBOARD, read and owned through
# python3-gi's GTK: copy, cut and another application's paste in the composer, and Ctrl+C after a
# drag across the response (a plain click on it leaves typing in the composer). Each scenario is
# a fresh Sirio; --state-only skips the
# captures; --display :N is a private Xvfb (at least --window-size, default 2200x1000)
# with Mesa's lavapipe, and trace_tab and trace_idle need one (--state-only with it paints
# but takes no captures), trace_keys, file_links, clipboard_keys, composer_keys and composer_resize a framed run at --window-size 2200x1000;
# --appearance light|dark; --only NAME[,NAME...] prints "CHAT SESSION PANEL E2E PARTIAL". The
# composer_compact scenario uses Scripts/Tests/stubs/session-panel-bypass/claude, the fixture's
# session_panel_bypass mode (the CLI starts in bypassPermissions), and proves the native Claude
# chat's compact composer row at the default window (about 430 px) on the `composerRow` of
# `surface.chat.read`: Fast, Thinking and the session panel in the overflow menu, the mode pill
# the danger shield-off, the band spanning the chat, send inside it, the chips ending before send
# and level with it (one line); then a real click on the overflow button (the measured
# `overflowButton`) opens the menu, whose `menu` rows the render built list Fast disabled with the
# CLI's reason as its tooltip (the stub's handshake says `preference`) and Thinking enabled, a real
# Enter opens the thinking picker (`picker`) and Escape closes it. It needs a painted chat
# (--display :N) and captures frames/composer-compact.png, composer-overflow.png and
# composer-thinking.png. The composer_keys scenario (not the one of that name in
# test-ely-chat-ui-e2e.py) presses real X keys in a native Claude chat's composer, on the default
# stub, whose fixture logs every prompt it hears (SIRIO_FIXTURE_USER_LOG) and every control request
# (SIRIO_FIXTURE_CONTROL_LOG): after two prompts, the second with a file picked from the @ popup
# with Tab, Up recalls the second with its @path text, a second Up the first, Down twice goes back
# to the empty draft; the recalled prompt sent again reaches the CLI with the file as a mention
# link and no literal @path; an edited recall is an ordinary draft (Up moves the caret); after a
# relaunch Up still recalls that prompt, mention included, and sending it mentions the file again.
# Then six Shift+Tab walk the permission modes in the menu's order with Bypass left out (the
# fixture's set_permission_mode requests read acceptEdits plan auto default acceptEdits plan), no
# mode is drawn as the danger shield, and typing still reaches the composer, so no dialog holds the
# keyboard. It needs --display :N --window-size 2200x1000 and captures
# frames/composer-keys-*.png.
# The composer_resize scenario (this script's own, default stub) proves the composer's resizable
# maximum on the `composerRow` of `surface.chat.read` (`capRows`, `maxRows`, `storedMaxRows`,
# `fieldRows`, `field`, `band`, `resizing`, `edge`, `root`), which exists only once a frame paints. At 2200x1000 the
# stored 12 is drawn and the empty field shows 3 rows; a 40-line draft fills it to 12; a 2200x400
# window caps the field at half the chat minus the band's fixed 61 px allowance (4 rows) while the stored
# maximum stays 12, and the band takes at most half the chat. Then real X drags on the
# composer's top edge: 4 rows up sets 16 and stores it; pulled to the transcript it stops at the cap (21 rows at 2200x1000) with the band within half the chat;
# with the 40-line draft still in, a drag released below the window (outside it) ends too and the rows
# bottom out at 3; a drag held 5 rows up pins the field at 8 and its release keeps 8; a press that never
# moved, and one held 8 px up, save nothing. A real drag of the sidebar's divider then saves the whole
# settings row. Last, Sirio is stopped and started again under the same name: the restored chat (the
# same tab) and a second chat opened beside it both read a maximum, drawn and stored, of 8, a 40-line
# draft in each fills the field to 8 rows, and the sidebar keeps its dragged width. Then the second
# chat is closed and Sirio starts again at interface font 18 (23 px rows): the cap, the field, the
# band (61 px plus its rows) and a drag follow the larger row. It needs
# --display :N --window-size 2200x1000 on a screen taller than the window (2400x1200), so a
# release can land below it, and captures
# frames/composer-resize-{rest,full,short,taller,outside,held,relaunched,font18}.png.
# The four unread scenarios run in the default set and need no display: `unread_parked_chat` (a
# turn that ends while its worktree is not selected marks the chat unread, posts one notification
# naming its branch, and showing the worktree clears the mark), `unread_question` (a permission
# the turn asks while the worktree is not selected reads needs-input and unread, still open when
# the worktree is shown), and `unread_secondary_seen` (a chat in the shown Secondary half, beside a
# terminal in the Primary, stops with no mark and no notification), and `unread_restored_chat_quiet`
# (the chat side of a restart: two chats that ran a turn on screen come back with no mark and no
# notification, a chat whose turn ended off screen before the quit comes back unread, again with no
# notification, and looking at it clears the mark for good). Each leaves
# attention-<scenario>.json (so attention-unread_restored_chat_quiet.json), the last
# `surface attention read`, in the out dir.
# In every mode, a no-display --state-only run included, `unread_parked_chat` asserts the tray's
# roster through the debug-only `surface attention read` key `tray` (one object per roster entry,
# with the label the menu would show): the worktree is listed with `unread: true` and a label led
# by "● " while the chat is unread, and has no unread flag (or no entry) after it is shown. No
# frame maintains it, because a minimized window paints nothing and the tray is for exactly then.
# With --display :N (a window that paints, or --state-only --display :N) it also asserts what the
# sidebar holds, through two more keys: `rows` (each worktree row's number of unread pills, from
# `Sidebar::unread_marks`) names the chat's worktree with 1 while it is unread and with 0 once it is
# shown; `projects` (one object per project header as the tree draws it: `expanded` and the
# `unreadCount` the folded badge is drawn from) reads open with 0 before the fold. In a window that
# paints it also clicks the Sessions segment, the project's chevron and the chevron again, and
# asserts `projects` reads `expanded: false` with `unreadCount: 1`, then `expanded: true` with 0,
# so a click that misses fails. The socket reports no position for the chevron, so that click is
# at a fixed window pixel. It captures
# frames/unread_parked_chat-unread.png (dot, bold title, ring), unread-sessions-view.png (the same
# dot and weight on the Sessions card), unread-collapsed-project.png (the count badge on the folded
# project header) and unread_parked_chat-after.png. The pills' mark is refreshed by the reconcile a
# frame drives, so a --state-only run with no display prints a SKIP line for the `rows` and
# `projects` assertions and keeps the rest.
# The opt-in scenario `notification_click` (never in the default set; name it with --only, with
# --state-only) proves the Linux notification route against a real D-Bus server. It needs
# `dbus-daemon` and python3-gi with Gio 2.0, and the script refuses it without them. It starts a
# private session bus (an empty XDG_DATA_DIRS, so nothing can be activated on it) and, on it,
# Scripts/Tests/fake_notifications.py, a recording org.freedesktop.Notifications server (Notify,
# GetCapabilities, GetServerInformation and CloseNotification as the Desktop Notifications
# Specification 1.2 defines them, plus an InvokeDefault of its own that emits
# ActionInvoked(id, "default") as a desktop does for a body click). It is the only launch that
# gets a bus address: every other launch gets one that does not exist. A turn ends in a parked
# worktree; the scenario waits for one Notify whose app name is `Sirio`, whose body names
# feat/login and whose actions hold `default`, sends the click, and asserts that the selected
# worktree becomes the chat's, the chat is the shown tab and, after the focus act that stands in
# for the frame a --state-only run does not paint, the chat is no longer unread and no second
# notification was posted. It leaves notifications.jsonl (the recorder's log: id, app_name,
# summary, body, actions of every Notify) and attention-notification_click.json in the out dir.
# Whether a real daemon (mako) invokes `default` on a body click is a hand check.
# The script seeds only light or dark: the Onice base colour is a hand check.
# Four scenarios run the fixture's elicitation mode (stubs/elicitation) and drive the open MCP
# elicitation through the debug-only `surface.chat.elicitation` (row, text or cancel: the paths a
# digit key, Enter in the free-text row and Esc take), reading the fixture's responses log
# (SIRIO_FIXTURE_RESPONSE_LOG, copied to the out dir as <scenario>-responses.jsonl):
# `elicitation_form` (colour blue, "abc" refused on count with an error and the step kept, 2, both
# optionals skipped, Accept; the agent heard exactly accept with {colour: blue, count: 2}; the chat
# read needs-input while it was open), `elicitation_url` (the link offers example.com; the row's accept
# is answered (the responses log), and the link's open is checked per mode: a --state-only run
# reads the live app's /proc environ, which names no display, so gpui runs headless and its open_uri
# is a no-op; a --display run requires the xdg-open stub (first on the login PATH) to record the URL;
# the server's elicitation_complete reads completed; the agent heard accept with no content),
# `elicitation_withdrawn` (the agent withdraws the form: it reads withdrawn, still after the turn,
# and nothing answered it) and `unhandled_request` (an unknown control request gets an error answer
# naming future_thing, and the turn still ends).
# The rename_* scenarios (A4) use Scripts/Tests/stubs/rename/claude, the fixture answering
# `rename_session` as Claude Code 2.1.326 does. A manual rename through the debug-only `surface tabs
# rename` reaches the CLI with its title, `source` host and the session's id, before a turn and
# after one (rename_session); an error warns and the next rename still goes, and a late answer to an
# older rename moves nothing (rename_session_failed); a refusal retires the verb quietly
# (rename_session_retired); a rename made before the handshake waits and goes once on ready
# (rename_session_connecting); a restored chat sends nothing and adopts nothing
# (rename_session_restored); a summarizer title never reaches the CLI (rename_auto_title_quiet).
# The permission_* and permission_rules* scenarios (A5) use
# Scripts/Tests/stubs/permissions/claude, the fixture's `permission` mode (any prompt asks for
# Bash `rm -rf build`). The stub turns environment variables into the fixture's flags:
# SIRIO_FIXTURE_PERMISSION_FIELDS (--fixture-permission-fields, a JSON file merged into every
# can_use_tool), SIRIO_FIXTURE_PERMISSION_RULES (--fixture-permission-rules, what
# list_permission_rules answers; unset, the fixture refuses it as the 2.1.257 floor does),
# SIRIO_FIXTURE_RULES_LOG (--fixture-rules-log, one line per request heard),
# SIRIO_FIXTURE_RULES_GATE (--fixture-rules-gate, holds each rules answer until the file exists)
# and SIRIO_FIXTURE_RESULT_GATE (--fixture-result-gate, holds the turn's result). They are
# state-only. The card scenarios read `questionDock` of `surface chat read`: permission_detail
# (title, description, reason, path and MCP server in order; Always allow stays; digits on),
# permission_default_no and permission_default_no_reject (start row Reject, digits off, Always
# allow gone; row 1 allows, row 2 rejects), permission_visible_text (an OSC, a BEL and a bidi
# override shown as U+XXXX; an ANSI reason stripped) and permission_plain (no fields: the plain
# card). The Permissions card scenarios read `sessionPanel` and the rules log: permission_rules
# (every group, tag and note; a bidi rule shown; an unknown behavior skipped; the last good list
# kept after an unreadable answer), permission_rules_refused (no card, one request even after a
# turn), permission_rules_closed_panel (no request until the panel opens),
# permission_rules_coalesced (a held answer, an Always allow and a turn's end make two requests in
# all), permission_rules_after_always_allow (the card refreshes before the turn ends) and
# permission_rules_unreadable (a note, no rows). The connection-ready and restore triggers have no
# E2E. test-ely-chat-ui-e2e.py's `permission` scenario reads questionDock for ACP: the plain card.
# The live check is `claude_lists_permission_rules` (#[ignore], acp_conformance).
# A6, the connector switch: connector_switch (a switch per server read from mcp_status, a repeat sends
# nothing, Notion off loses its Sign in; three mcp_toggle and no mcp_authenticate after its mark),
# connector_switch_in_flight (switching…, a second flip fails and sends nothing), connector_switch_waiting
# (switching a waiting server off drops its poll: exactly one mcp_status after the mcp_toggle line), connector_switch_refused
# (every switch hidden, rows and notes unchanged), connector_switch_missing (the CLI's sentence as a note,
# the switch unchanged), connector_switch_unread (SIRIO_FIXTURE_TOGGLE=unread: the toggle applies, the next
# mcp_status fails, and the row's note says the new state could not be read) and connector_switch_click (framed, 2200x1000: a real X click at `switchButtons`
# toggles that server and starts no sign-in, with two captures). New fixture variables, passed by launch()
# and reset per scenario: SIRIO_FIXTURE_TOGGLE (refused, missing or unread) and SIRIO_FIXTURE_TOGGLE_GATE (a file
# whose absence holds every mcp_toggle answer). Every count reads only the lines after a control_log_lines mark.
# A6, the reload row, on the same stub (the twelve reload_* scenarios, each ending with no mcp_toggle after its
# mark): reload_plugins_and_skills (reload_plugins with the hold and refuse_if_busy, then reload_skills; the row is
# back to Reload and the plugin server joins the card), reload_floor (SIRIO_FIXTURE_FLOOR=1: the hold without
# refuse_if_busy, an answer with no `held` is applied), reload_held (SIRIO_FIXTURE_RELOAD=held: the cost warning,
# a bidi override shown as text, Reload anyway sends no hold and is followed by reload_skills; a framed run
# captures the held card), reload_held_then_busy (a turn during the hold adds the busy note and sends nothing; its
# end leaves the warning), reload_busy_refusal (SIRIO_FIXTURE_RELOAD=busy), reload_busy_cli
# (SIRIO_FIXTURE_CLI_BUSY_GATE: a turn the CLI started), reload_busy_turn, reload_busy_before_skills
# (SIRIO_FIXTURE_RELOAD_GATE holds reload_plugins while a turn begins: no reload_skills), reload_in_flight
# (reloading…, a second activate fails), reload_refused (SIRIO_FIXTURE_RELOAD=refused hides the row),
# reload_skills_refused (SIRIO_FIXTURE_SKILLS=refused hides it after an applied reload) and reload_errors
# (`2 plugins failed to load.`). New fixture variables, passed by launch() and reset per scenario:
# SIRIO_FIXTURE_RELOAD, SIRIO_FIXTURE_RELOAD_GATE, SIRIO_FIXTURE_SKILLS and SIRIO_FIXTURE_CLI_BUSY_GATE.
# A7, errors and notices: Scripts/Tests/stubs/notices/claude, the fixture's `cli_notices` mode
# (the prompt picks the turn: retry twice, retry out, notify, notify short, notify short again,
# refuse, refuse fallback, inform levels, midstream and more: see the stub's header for the full
# set; anything else answers ok). Two variables, passed by launch() and reset per scenario:
# SIRIO_FIXTURE_RETRY_GATE (--fixture-retry-gate, a file that holds `retry twice` after its
# retries until it exists) and SIRIO_FIXTURE_STARTUP (--fixture-startup=<kind>, a CLI that cannot
# start). The script runs `unset CLAUDE_CODE_STARTUP_FAILURE_RESULTS` at its top as belt and
# braces: launch() starts Sirio with `env -i` and an allowlist, so the variable cannot reach the
# CLI from the script's environment, and the opt-in proof (startup_opt_in) rests on that clean
# environment: only Sirio's own launch can set it. They read `statusLine` and the transcript rows
# of `surface.chat.read`.
# The five notice_* scenarios: notice_retry (`Retrying 2 of 10 in 2 s: overloaded` while the gate
# holds, empty with the reply drawn once it opens), notice_retry_exhausted (the sentence in one
# row, no line), notice_notification (the line outlives the turn; immediate over high over low; a
# same-key one replaced, held 3; each timer clears only its own entry), notice_refusal (info rows
# between the refused text and the answer; a bidi override shown as text; the levels read info,
# info, info-warning) and notice_midstream (a cut reply is not glued to the answer after it). The
# four startup_* scenarios wait for the chat's error row and read the last one:
# startup_config_invalid (the sentence, File and Backup, the override as text, no ACP),
# startup_opt_in (a reason the CLI writes only with the variable, so Sirio's launch sets it),
# startup_unknown_reason (`Claude Code could not start (some_future_reason).`) and
# startup_no_result (`Claude Code exited before it started (agent exited with code 1)` and the
# stderr tail). All nine run state-only.
# The live checks are `claude_reports_why_it_could_not_start` and
# `claude_reports_api_retries_and_unstreamed_answers`; run them one CLI at a time, with
# `--test-threads=1` (the command is in docs/agents/architecture/claude-chat.md, Proof and
# limits).
# A8, the Hooks card: the seven hooks_* scenarios run on the session-panel stub; hooks_card captures
# `frames/hooks.png` in a framed run; the other six also run in framed runs and take no captures.
# Three variables, passed by launch() and reset per scenario: SIRIO_FIXTURE_HOOKS (--fixture-hooks,
# the JSON file get_hooks_listing answers from, re-read on every request; unset, the fixture refuses
# it as the 2.1.257 floor does), SIRIO_FIXTURE_HOOKS_GATE (--fixture-hooks-gate, holds each answer
# until the file exists) and SIRIO_FIXTURE_HANDSHAKE_GATE (--fixture-handshake-gate, which the
# session-panel stub forwards too: it holds the handshake). The files are written by Python
# (json.dumps), so a hidden character is only ever built at run time. The scenarios read
# `sessionPanel` and count the get_hooks_listing lines of the control log after a mark: hooks_card
# (every group, row key, label, detail and tag; the bodies of the rows worth reading, with a bidi
# command, a hostile source label and a multi-line command shown as text and the command last; no
# `editable` anywhere in the read; the last good listing kept after an unreadable answer),
# hooks_policy (the note's lines in order, the disabled tags per row, the one-line organization
# note), hooks_refused (the panel opens during a held handshake, so only the connection-ready
# trigger can ask: one request, no card, and still one after a turn), hooks_closed_panel (no request
# until the panel opens, none from an idle open panel), hooks_coalesced (triggers during a held
# answer make one follow-up: two requests in all), hooks_refresh (a turn's end and a reload that
# applied each ask again; nothing polls) and hooks_unreadable (a note, no rows). Restoring the panel
# open has no E2E.
# The live check is `claude_lists_hooks` (#[ignore], acp_conformance; the command is in
# docs/agents/architecture/claude-chat.md, the Hooks card).
# A9, usage and limits: five scenarios on the notices stub (Scripts/Tests/stubs/notices/claude),
# which forwards the two fixture variables to the fixture's `cli_notices` mode. limit_notice (the
# prompts `limit warning`, `limit warning again`, `limit allowed`, `limit rejected`, `limit
# pooled`: one row for a warning and not a second, the same warning again after an allowed, the
# refusal, then the overage refusal with its scope; the CLI's own refusal text is still one
# assistant row), usage_text (`/usage` and `/context` are command-output rows with the CLI's text
# and no assistant row; a failing `/usage` whose result repeats the text is drawn once; a bidi
# or control character in the plain text shows in angle brackets),
# usage_floor (a `/usage` with no marker adds only the turn row, ends the turn, and the next
# prompt is answered), usage_card (a `usage_report` is drawn as a card: the read row's `card`
# lists the totals, the models, a heading per group and the extra usage, a bidi override in a
# scope name and a newline in a kind show as `U+202E` and `U+000A` in angle brackets, and
# `headline` is the first active row's name) and usage_unavailable (eight malformed or partial
# reports read as unavailable or none reported, never 0%: a row with no percent shows no `%`, a
# row's missing parts leave no stray separator, an empty row no line, a group of such rows no
# heading, totals with only removed lines read `2 lines removed`). All five
# run state-only; usage_card and usage_text also capture.
# Three variables, passed by launch() and reset per scenario: SIRIO_FIXTURE_LIMITS
# (--fixture-limits, JSON {"soon": epoch, "later": epoch}, the resets of the `limit ...` prompts),
# SIRIO_FIXTURE_USAGE (--fixture-usage, JSON {"marker": bool, "report": value, "fails": bool,
# "text": str}, re-read on every command, so a scenario changes the answer between two sends) and
# SIRIO_E2E_TZ (passed to Sirio as TZ). limit_notice and usage_card set SIRIO_E2E_TZ=UTC and put
# every reset 3 or 4 days ahead, so a time reads `%a %H:%M` whenever the run happens, and compute
# the expected text in UTC with a fixed English day list. The same-day and far-date formats are
# covered by the unit test `reset_time_formats`. The fixture files of limit_notice and usage_card
# are written by Python (json.dump), so a hidden character is only ever built at run time; the other
# three write theirs with printf.
# The capture: a framed run on a private Xvfb of 2400x1200 (--display :N), frames/usage-card.png
# and frames/usage-text.png:
#   Scripts/Tests/test-chat-session-panel-e2e.sh --display :N --only usage_card,usage_text
# The live checks are `claude_answers_usage_locally`,
# `claude_reports_rate_limits_through_a_gateway` and `claude_usage_report_on_account`; the last is
# run by hand by Enzo, never CI, never an agent. The commands are in
# docs/agents/architecture/claude-chat.md, Usage and limits.
Scripts/Tests/test-chat-session-panel-e2e.sh --state-only   # -> prints "CHAT SESSION PANEL E2E OK (<n> scenarios, <k> skipped, appearance <a>, state-only <0|1>, window <w>)"
Scripts/Tests/test-chat-session-panel-e2e.sh --state-only --only notification_click   # -> prints "CHAT SESSION PANEL E2E PARTIAL (1 of <n> scenarios: notification_click)"

# MCP Apps widgets (A11): a tool with a UI shows its widget inline in the native Claude chat, in a
# wry webview over the tool row. Widgets exist on Linux under X11 only, so these three scenarios
# need a framed run on a private Xvfb of your own (never $DISPLAY or :0), at --window-size
# 2200x1000, with Mesa's lavapipe:
#   Xvfb :97 -screen 0 2400x1200x24 &      # keep its exact PID; kill that PID when done
#   Scripts/Tests/test-chat-session-panel-e2e.sh --display :97 --window-size 2200x1000 \
#     --out-dir DIR --only mcp_widget      # then mcp_widget_flood, then mcp_widget_gate
# They run the trace stub with the fixture's MCP Apps knobs (SIRIO_FIXTURE_MCP_APPS=1, _WIDGET_HTML,
# _START_BYPASS, _EFFORT_LADDER, _MCP_APPS_CAPS, _STRAY_MCP_ASK) and serve the hand-written view
# Scripts/Tests/fixtures/mcp-apps/widget-view.html, which talks JSON-RPC to its parent and probes
# for escapes against a request logger on 127.0.0.1 that must stay empty. The widgets are read
# from the `mcpWidgets` key of `surface.chat.read`, the dock's `clickOnly`, `selected` and
# `rowBounds` give the card, and the debug-only `surface.chat.transcript` scrolls the list or folds a turn.
#   mcp_widget        two widgets, each with its own slot; a card that only a real click allows (Return,
#                     1, Down and Return do nothing, Escape denies; a real press on the Allow row
#                     that begins before card 2 armed and is released 0.7 s later leaves it pending; a
#                     real click on its Allow before the body is scrolled to its end does nothing
#                     (`allowArmed` "0"); a real wheel over the body arms it, and a real X click on
#                     the Allow row then runs the call); and at every step the placement invariant: `mapped` is "1"
#                     exactly when the slot is drawn wholly inside the list's viewport with no overlay,
#                     dock question or hidden tab, and `xwininfo -id <xid>` says IsViewable exactly then
#                     (no window counts as not viewable). At the step that puts widget 1 whole in the
#                     viewport it also checks pixels: the display is screenshotted (`import -window root`),
#                     widget 1's slot is cropped out, at least 90% of the crop must be the view's own
#                     background colour (#12ab34, which the chat never uses), and the crop is kept as
#                     `widget-slot-crop.png` (the X geometry check passed while the slot was black: the
#                     webview stayed 1x1 when `set_bounds` ran before `set_visible(true)`).
#                     The steps are the card open, a real click
#                     inside widget 1 (the page fills its slot, checked on pixels above), a third
#                     widget whose view has a real button (a real click on it raises the card for its
#                     `report` call, rejected over the verb) and real typing with the pointer over it
#                     (the view counts `keydown` events in the capture phase and reports the count in
#                     its height, 120 plus the count: it must read 120). Known limit: the count stays 0
#                     with the key handler disabled too (no run has seen a key reach the
#                     page, handler on or off), so that part cannot tell the handler's work
#                     from its absence.
#                     A fourth widget is a textarea whose value length goes out through its height (2000
#                     plus the length): the X PRIMARY selection is set from a python3-gi helper process
#                     (killed by its exact PID), a real `click 2` lands on the textarea, and the length
#                     must stay 0 (with the button-2 handler disabled the app stops answering reads: the
#                     paste engaged). `show-option-menu`, `run-color-chooser` and `print` have no E2E.
#                     Also a real click inside widget 1 then one in the composer then real typing (the
#                     composer still takes the keys), a half scroll and back, a folded turn,
#                     the effort list, a file tab over the chat, a terminal beside it, a call whose
#                     `tool_use_result` is a string (the fixture's `mcp_denied` turn: its widget is
#                     closed with "the call did not run" and never got input or result), a view that
#                     posts straight to the IPC handler (`mcp_forge`: closed with "the view posted
#                     outside its relay", `forged` "1" while every other widget's is "0"), and a dead
#                     CLI. It also checks the CLI heard six mcp_read_resource and only gated
#                     mcp_calls for `act`, and writes mcp-widget.json to the out dir.
#   mcp_widget_flood  a widget posting pings at about 125k a second for 3 s: surface.chat.read answers
#                     within 1000 ms throughout; within 30 s the widget's row is closed with "over the
#                     rate limit for 5 s" (the host's IPC backlog keeps it over 50 a second for 5 s of host
#                     time), no card ever appeared, its `forged` count is "0", the CLI heard no mcp_call, and the window it had is gone within 3 s;
#                     mcp-widget-flood.json has the latencies and the final row. The per-second cut is
#                     covered by the bridge test a_flood_is_cut_at_fifty_a_second.
#   mcp_widget_gate   the CLI lists `_meta.ui` and serves the resource but reports no capability:
#                     the rows are plain, no widget exists, and the CLI hears no read or call.
# Scripts/Tests/widget-isolation.sh --display :97 --out-dir DIR is the webview's own isolation
# proof (parent, IPC, network, devices, storage and navigation), run on the same kind of Xvfb.
# Its logger listens on four 127.0.0.1 ports and counts raw TCP accepts (accepts.log) before it
# reads a byte, so a bare preconnect, which sends no request line, cannot pass vacuously. The probe
# view adds `<link rel=preconnect>` and `dns-prefetch` (one port), `<link rel=prefetch>` and a
# speculation-rules prefetch (one port) and an `<a ping>` clicked by script (one port), each outside
# the view's connect-src. The run must see no accept on any of them; the control opens the CSP and
# turns the link features back on, and its checks and FACT lines show which probes reach their
# logger, so a 0 in the run is the CSP or the switches, not a dead probe. `<a ping>` is stopped by
# the CSP (the hyperlink auditing and DNS prefetching settings are documented no-ops on 2.54.1). The
# raw TCP logger cannot see a DNS lookup, so the dns-prefetch probe proves only that no TCP connect
# follows. `LinkPreconnect`, `LinkPrefetch` and `SpeculationRulesPrefetch` are turned off through
# `dlsym`-resolved WebKitGTK 2.42+ functions.
# Unverified, for a later enablement: macOS WKWebView and Windows WebView2 (view CSP header
# enforcement, the incognito store, IPC in subframes, WebRTC off, the permission handler, drops;
# on Windows wry's navigation handler sees only the top frame).

# The native Ely agent chat in a real, isolated Sirio against the Python agent fixture, over the
# control socket (docs/testing/ely-agent-chat.md). The effort and effort_light scenarios (dark and
# light) prove a chat asks its agent for High at start and again after a relaunch, and that the
# effort list opens and takes a pick through the debug-only `surface.chat.effort`; they capture it.
# effort_full and effort_full_light run the fixture's effort-full mode (Default, five levels,
# Ultracode), capture the whole list (the notes, the separator) and choose Default from it.
# effort_codex and effort_pi run the fixture's effort-codex and effort-pi modes, the wire shapes of
# codex-acp and pi-acp, as the `codex` and `pi` adapters (`Scenario(..., agent=...)`): the effort
# selector arrives in the `session/new` result, tagged with ACP's `thought_level` category under the
# id `reasoning_effort` (Codex, whose three real modes ride along) or `thought_level` (Pi, which also
# publishes the levels as session modes and answers a pick with a `current_mode_update`). Codex and
# Pi are reached through an installed manifest, so each scenario moves the instance's XDG_DATA_HOME
# into its own directory and writes `sirio/agents/<registry id>/manifest.json` there, its executable
# the --agent-proxy wrapper (the user's own agent store is never read; the manifest is kept as
# `agent-manifest.json` beside the scenario's artifacts, and `session/new` in agent-traffic.jsonl
# proves the run went through it). They prove the chat starts on the level its agent reports and
# asks it for nothing at all (no `session/set_config_option` or `session/set_mode` before a pick),
# that a pick from the list reaches the agent by that id, and by nothing else (every
# `session/set_config_option` and `session/set_mode` the app sends is checked), that Codex's real
# modes keep the mode pill and that Pi's modes, a copy of its levels, give none, even after the
# agent's own `current_mode_update` for the pick (the pill checks read a painted frame: Codex's pill
# check SKIPs under --state-only, and `effort_pi` is skipped whole, as composer_keys is, since its
# pill is read from a painted frame too); they capture the list and the level chosen. Under a full
# run `effort_pi` needs a display but not xdotool, since it never types. effort_pi then samples the
# chat for about 2 s after the agent's `current_mode_update` (not one read), failing on the first
# sample off Low, asked again (any `session/set_config_option` or `session/set_mode`), or with a mode
# icon. effort_codex and effort_pi wait until `session/new` reaches the fixture before they send `hi`.
# Six scenarios need a display (effort_pi above, composer_keys, reduced_motion and unfocused_spinner
# below, and the two composer_band ones after them) and are skipped under --state-only (`SKIP <name>: needs a
# display`, recorded as `skipped` in summary.json): composer_keys types through xdotool (Return,
# Shift+Return, Up and Down moving the caret, the slash popup taken with Down and Tab, closed by
# Escape, proved by a frame diff of its box against the frame before the `/`; and the composer's
# field paints nothing of its own (`composer_field_has_no_fill`: in the empty draft's frame every
# pixel of a band across the field is the band's colour, for the default 1470x833 window); its
# fixture is `composer`, because `plain` advertises no slash commands) and reduced_motion
# (SIRIO_REDUCED_MOTION=1: the transcript is pixel-identical in three frames 250 ms apart) and
# unfocused_spinner (a second window, `surfaces_probe --frame tooltip`, takes the X focus, and the
# sidebar row's status bloom, which only Sirio's frame clock (`sirio_ui::motion`) repaints, must still turn in three
# frames 250 ms apart; it logs `unfocused-spinner CPU <x>%` of one core over 10 s, and --cpu-ceiling
# PCT fails above PCT; --focus-thief-bin PATH names the second window's binary, by default the
# surfaces_probe example, built with the others unless --no-build; the CPU base and the ceiling
# formula `base × 1.25 + 1` are in docs/testing/bezel-off.md, "The unfocused spinner and its CPU
# base").
# composer_band and composer_band_light (dark and light, also display-only) prove the composer band in the
# real app on the `composer` fixture, from `composerRow` of `surface.chat.read` (layout, the band's, send's
# and the chips' boxes, the chat root's, docked, overflow): wide is the full row, narrow and floor (a lone
# chat pane never goes under 320 px) are the compact row with send inside the band and the chips ending
# before it, docked (the session panel open, a chat about 690 px wide) ends the band at the panel; every lane
# also asserts the row is one line (the band as tall as the default window's, the chips level with send);
# the minimal lane (last) splits the chat's pane with `pane.split` (focus it with `pane.focus left`, split
# right, until `layout` is `minimal`, about 209 px, since the window itself cannot go under 320 px), asserts
# the same geometry and that `overflow` starts `model`, `attach`, then clicks the measured `overflowButton`,
# reads `menu` (model first, attach second, both enabled), presses Enter (`picker` is `model`) and Escape;
# frames per lane (minimal, minimal-overflow, minimal-model). The composer_band scenarios run with a private HOME,
# XDG tree and SHELL=/bin/sh, so the terminals the splits spawn read none of the user's rc files (their frames
# show a plain sh prompt). Each then runs a narrow lane against the `ely-catalogue` fixture, which advertises
# modes (current `ask`): `modeIcon` is what the render drew for the pill, the shield every id
# without an icon gets, and `modeDanger` is false (the `composer` fixture, with no modes, draws none).
# The record also carries `overflowButton`, `menu` (the rows ⋯ drew, while open) and `picker`.
# The frame constants (FIELD_BAND, BAND_BESIDE_FIELD, COMPOSER_CLICK, POPUP_REGION) are for a 1470x833
# window: an Xvfb screen of 1474x833 opens one.
# --display :N (an Xvfb; export VK_DRIVER_FILES and VK_ICD_FILENAMES as lavapipe's lvp_icd.json)
# takes the captures, --state-only omits them,
# --only NAME[,NAME] runs some scenarios, --out-dir DIR keeps the artifact.
Scripts/Tests/test-ely-chat-ui-e2e.py --out-dir DIR --state-only   # -> prints "ELY CHAT UI E2E OK"

# The richer chat messages (spec 2026-11-21-ely-chat-richer-messages-design.md §7.2) in a real, isolated debug
# Sirio on a private Xvfb: its native Claude chat against a stub `claude` (Scripts/Tests/stubs/richer/claude, the
# stream-json fixture in its `richer` mode) and its ACP chat against a stub `opencode`
# (Scripts/Tests/stubs/richer/opencode, rust/crates/sirio_ui/tests/fixtures/chat_fixture.py in its `richer` mode).
# A prompt's text picks the turn (the stubs' headers list them) and what a retry resends is the same text, so a
# turn that fails does so on the first prompt of that text only, counted in the stub log
# ($SIRIO_STUB_LOG, `kind<TAB>pid<TAB>text`, one per world); `<prompt> hold` waits for the file `gate.<n>`. It reads
# `surface.chat.read`'s debug readouts (a reply's `live` and `shownBytes`, `transcriptText`, `transcriptTextRows`,
# `held`, `heldLine`, `daySeparators`, an error row's `errorKind` and `retry`, a `limitCard` row's `phase` and
# `resetsAt`, a user row's `at`, a thought's `durationMs`) and drives the debug `surface.chat.transcript` action
# `retry`; real X input (xdotool) does the two things the socket has no verb for, the transcript menu's Select All
# and Copy (read back through the display's real clipboard, GTK) and the queue's remove button (placed from
# `composerRow`). Each scenario is a fresh Sirio under `env -i` with its own HOME, XDG tree, database, socket and
# host root; ~/.config/mise/config.toml must not change.
#   stream             a 40 KiB reply (SHOWCASE, then parts that freeze) in 200 chunks 20 ms apart: while live,
#                      0 < shownBytes < len(text), it only grows and is still behind past 12 KiB (which needs 5 KiB of
#                      the reply frozen as it arrived: one with nothing frozen hands over to plain drawing once its
#                      tail passes 7 KiB),
#                      transcriptText holds the newest marker even where the reveal has not reached it, and at the
#                      turn's end live is false at once; the same bytes in one chunk give the same plain text. A reply selected whole and copied through the transcript menu while it
#                      streams (`showcase slow`, 8 s), with the transcript scrolled to its top so that the live reply is
#                      not drawn and the reveal stands still (shownBytes is read again after the Copy and has not
#                      moved) while the text arrives, puts text of the live reply on the clipboard that is past the
#                      revealed byte (shownBytes read right after Select All). A reply with no blank line (`compact`,
#                      28 KiB) never freezes a block, so once its tail passes the 7 KiB budget the reveal hands over to
#                      drawing as it arrives for the rest of the turn (spec 5.2): at least five live reads land from 14 KiB
#                      of text on (the phase is slow and short of chunks so that the UI thread has idle time, and the read
#                      latencies are logged) and each has shownBytes the whole text with live still true; it ends with live
#                      false and its text equals the same bytes in one chunk. SHOWCASE eight times over (`showcase x8`, every copy cites [^1] again; it freezes by
#                      a varying amount, so whether its reveal hands over depends on when the frames fall) is checked only
#                      for its end and its equivalence.
#   stop               Stop in mid-reveal: two reads that find shownBytes behind the text bracket the capture, the Stop
#                      follows the second at once, and at the first read after it live is false; no error row, nothing
#                      held, the text equals the same bytes in one chunk; the agent killed right after has nothing to
#                      resend (Retry only relaunches)
#   claude_turn_failed an is_error result is an error row `turnFailed` whose Retry resends (`retry: resend`) the
#                      same text on the same pid and the second answer arrives; the partial reply equals the same
#                      bytes in one chunk; `error_max_turns` keeps its reply and has no error row. Queue variant: two
#                      messages queued behind the failing turn are held (`held`, "Waiting: press Retry or send a
#                      message."), nothing is sent, one is removed with a real click on its X, Retry lifts the hold at
#                      the send, and the other is sent only at the retried turn's end
#   acp_prompt_failed  the same over ACP (the queue variant only over Claude): the failed `session/prompt` leaves the
#                      process up (its partial reply ended) and Retry resends to the same pid, which was asked for one
#                      session; an error that follows Stop is the late end of the stopped turn (no error row; the fixture's
#                      log shows it got `session/cancel` and answered with an error)
#   claude_lost_resume the CLI exits mid-turn: `retry: resume`, Retry relaunches with `--resume fixture-session-1` and
#                      resends; the partial reply equals the same bytes in one chunk. Killed while idle, after a finished
#                      turn: `retry: relaunch`, Retry starts a new process and nothing is sent
#   acp_lost_composer  the ACP agent exits mid-turn: `retry: composer`, Retry relaunches and the message is back in the
#                      composer with a notice, nothing sent; a launch that fails and then works (no turn in flight)
#                      is `retry: relaunch` and puts nothing back; with a draft already in the composer the draft stays.
#                      An ACP agent killed while idle is not noticed until the next send (as built, spec 7.2): that send
#                      lands on `retry: composer`, the dead agent never hears it and Retry puts it back in the composer
#   limit              a rejected limit a few seconds ahead is a `limitCard` row counting (`resetsAt`), its failed
#                      turn's row `coveredBy: limitCard` with `retry: none`, the socket's Retry refused, a held queue
#                      "Waiting: send a message."; then, once the reset has passed, the card reads `retry`, Try again
#                      resends and the card reads lifted (Retry refused); a rejection in a turn that did not fail ends
#                      lifted once the reset has passed; a prompt sent while it counts retires the row's Retry and the
#                      card ends lifted
#   day_separators     a restored transcript (rows written into `chat_turn`) whose user entries are on three days
#                      has separators "Today", "Yesterday" and a full date (worked out from the clock after the
#                      relaunch, midnight-proof), an undated turn has none and no `at` key
#   thought_end        the CLI closes its thinking block, then waits 2 s: `durationMs` is the block's (under 1 s)
#                      once the thought has settled and before the text arrives, and it does not move
# A check prints `OK: <scenario>: <what>`, a failed one `FAIL: <scenario>: <why>` and the run goes on with the next
# scenario; the run prints the OK line only if every check of every scenario passed. The Xvfb is the script's own,
# started and stopped on :140 to :159 (--display :N); --out-dir DIR (default artifacts/chat-richer/e2e) keeps
# transcript.log, checks.jsonl (each check with the newest readout of the chat when it was judged, `read`, and what it
# measured, `detail`), scenarios/<name>.json (the same per scenario, with the series each read) and .stub.log, logs/ and
# frames/*.png (PID-matched window captures); --only NAME[,NAME] runs some scenarios and prints "CHAT RICHER MESSAGES
# E2E PARTIAL". Needs python3 with gi (GTK 3), xdotool, xwininfo, xprop, xdpyinfo, import, identify, Xvfb and lavapipe.
# The `World` that starts a Sirio and the helpers around it live in Scripts/Tests/chat_e2e_harness.py, which the message
# actions E2E below imports too.
Scripts/Tests/test-chat-richer-messages-e2e.sh   # -> prints "CHAT RICHER MESSAGES E2E OK (<n> checks)"
Scripts/Tests/test-chat-richer-messages-e2e.sh --only limit,thought_end   # -> prints "CHAT RICHER MESSAGES E2E PARTIAL (2 of 9 scenarios: limit, thought_end; <n> checks)"

# The chat's message actions (spec 2026-11-23-ely-chat-message-actions-design.md §7): Copy, Continue and Quote on chat
# messages, and Claude's Stop labelled `cancelled`, in a real, isolated debug Sirio on a private Xvfb, against the stubs of the
# richer E2E above (`claude` and `opencode`, extended with `max tokens`, `slow`, `slow late` and a held `Continue`; ACP
# `max tokens`, `max turns`, `max tokens drop` and `slow`; their headers list them). It reads `surface.chat.read`'s debug
# readouts (a row's `actions`, `actionsShown` and `actionBounds.<copy|quote|continue|row|quoteCard>`, a user row's `quote`, the chat's
# `continue`, `continueRow`, `composerQuote`, `composerQuoteBounds`, `queued`, `transcriptText`) and drives the debug
# `surface.chat.transcript` verbs `continue`, `toggleTurn`, `copy`, `quote` and `quoteSelection`; every press a user makes is a real X click (xdotool) at the pixels
# the readout reports: Copy, Quote and Continue on a row, the quote card's cross, the transcript menu's Select All, Copy and Quote
# selection. The clipboard is read back through GTK on the E2E's own display (no WAYLAND_DISPLAY), another application owning
# it before each press. Each scenario is a fresh Sirio under `env -i` with its own HOME, XDG tree, database, socket and host root.
#   copy             Copy on a user row, an older reply (Markdown: the clipboard holds the source) and the last reply (always
#                    shown), each a real click, the first two after hovering; the clipboard equals the row's text; the `copy` verb
#                    does the same on a reply and is refused on a turn footer, leaving the clipboard alone
#   rows             the last reply `always`, the others `hover`, a footer none; a streaming reply none; Select All and Copy
#                    through the menu equal `transcriptText`, with no label of the row in it
#   idle_cost        redraws (`Chat.render` and `SirioWorkspace.render` spans of the performance trace) over 5 s of an idle
#                    chat with its last reply's row shown, before any press and after a Copy press has settled: after <=
#                    before x 1.5 + 2 and no more than 3 a second; a window with a press in it must count some (the control)
#   continue_claude  `max tokens`, `fail max turns` and a Stop of `slow`: the footer says why, Continue sits on the last
#                    reply's row only, a folded turn draws none and the verb is refused, a real press sends a visible `Continue`
#                    user row that the stub hears on the same process
#   continue_footer  a Stop before the first text (`think slow`) leaves a turn with a thought and no reply, so Continue sits alone on
#                    its footer, which is the click that folds the turn: a real press on it sends the visible `Continue` and does
#                    not fold the turn (the `toggleTurn` verb is the control that a fold shows in the readout)
#   continue_acp     the same over ACP, with a typed draft and an accepted mention that a Continue leaves alone; an agent's error
#                    answer to the cancel still ends the turn as cancelled and offers Continue
#   continue_hidden  `continue` is `none` after an ACP agent was lost and after it restarted, after an ordinary end, while a turn
#                    runs and on a restored chat
#   continue_resume  a Claude chat stopped by `max tokens` whose process is killed: Continue is `resume` and relaunches with
#                    `--resume <session>`, then sends the prompt
#   continue_once    a second `continue` verb is refused while the first one's turn runs; the stub hears one `Continue`
#   claude_stop      a Stop of a Claude reply says cancelled and adds no text; a Stop with an item queued sends it and offers no
#                    Continue; Stop, another prompt, then the stopped turn's late end offers none
#   quote            Quote on a reply, the card's cross, a send as a `> ` block with the user row reading its quote, Quote selection
#                    through the menu, a quote queued with its block while a turn streams, `/compact` sent without the quote and
#                    `/etc/hosts is wrong` with it, a quoted `@path` unchanged, the quote on the row after a relaunch, a hand-back,
#                    the `quote` verb (and its refusal on a footer), `quoteSelection` refused with nothing selected, Copy on a quoted
#                    user row (the block and the typed text), a quote sent with no typed text
# `--out-dir DIR` (default artifacts/message-actions-e2e) keeps transcript.log, checks.jsonl, scenarios/<name>.json and
# .stub.log, logs/ and frames/*.png as the richer E2E does; `--display :N` (:140 to :159, default :141) and `--only
# NAME[,NAME]` (prints "MESSAGE ACTIONS E2E PARTIAL") likewise. Needs the same tools. Run it under `env -u WAYLAND_DISPLAY`.
Scripts/Tests/test-chat-message-actions-e2e.sh   # -> prints "MESSAGE ACTIONS E2E OK (<n> checks)"
Scripts/Tests/test-chat-message-actions-e2e.sh --only quote   # -> prints "MESSAGE ACTIONS E2E PARTIAL (1 of 11 scenarios: quote; <n> checks)"

# Live end-to-end test of the forge layer (sirio_forge): the token transport
# and the real gh/glab against loopback fake forges. --out-dir DIR keeps the
# transcript and the fake forges' request logs; SIRIO_FORGE_E2E_VERBOSE=1
# prints every probe answer.
Scripts/Tests/test-forge-e2e.sh    # -> prints "FORGE E2E OK"

# The change request view and tab in a real, isolated Sirio against the fake
# forge, over the control socket; --state-only skips the window captures.
Scripts/Tests/test-forge-ui-e2e.sh  # -> prints "FORGE UI E2E OK"

# A change request's diff, the editor hand-off (local file or read-only
# snapshot, across a quit and relaunch) and the fetch with its failures,
# against a bare repository and the fake forge; includes unified/split review
# threads, resolved folds and outdated sections. Each scenario saves its first
# token while a stub gh holds the host's first probe (#613); same flags as above.
Scripts/Tests/test-forge-diff-e2e.sh   # -> prints "FORGE DIFF E2E OK"

# The Changes surface on Ely against a real repository: unified and split,
# git failing and recovering, the in-window Discard confirmation (confirmed
# through the debug-only `surface changes confirm`); same flags as above,
# plus --appearance light|dark.
Scripts/Tests/test-changes-e2e.sh   # -> prints "CHANGES E2E OK"

# A project whose disk is unplugged between launches: kept with its worktrees
# and tabs while the disk is away, whole again once it is back; --out-dir DIR,
# --display :N adds two PID-matched captures.
Scripts/Tests/test-unplugged-volume-e2e.sh   # -> prints "UNPLUGGED VOLUME E2E OK"

# The Secondary pane on demand and the right panel's Open list in a real,
# isolated Sirio: one centre pane at launch, the list and its Chat
# expansion, a pane that comes and goes with its tabs, across a quit and
# relaunch; same flags as test-changes-e2e.sh.
Scripts/Tests/test-open-list-e2e.sh   # -> prints "OPEN LIST E2E OK"

# The Primary rail and its peek (spec 2026-11-01-primary-rail-design.md §8) in
# a real, isolated Sirio, with real X clicks and keys at the rects
# `surface layout read` reports (railRects, debug builds): no collapse button
# without a Secondary tab; collapse to the 40 px rail; a codex stub terminal
# and the native chat (Scripts/Tests/stubs/trace/claude) producing output and
# changing status while collapsed (the terminal's 270 KB, more than a PTY
# buffer, read back before any peek draws it), all of it in their peeks; a
# peeked terminal keeping its columns and receiving Esc; Esc cancelling a
# peeked chat's turn;
# click outside, the same icon twice, the rail's + -> New Terminal, the pin;
# the collapsed flag across relaunches, cleared by the last Secondary tab and
# by Ctrl+Shift+B; a collapsed, unpeeked tab being off screen (its needs-input
# posts, it reads unread in `surface attention read` and in `railUnread`, and
# still reads unread 3 s later with the window active), then peeked it reads
# seen and its next status change posts nothing (the posts are read from the
# attention read's `posted` list: on Linux a post never reaches the notify-send
# stub, whose notify.log stays empty); a Browser tab blank under the
# peek and back after it; the peeked tab closed over the socket, a worktree
# switch, the last Primary tab closed while peeked. Each failure names its
# step. --display :N is a private Xvfb of at least 1600x1000 with Mesa's
# lavapipe; --state-only takes no display, runs step 1's reads only, prints
# SKIP for steps 2-15 and ends with "PRIMARY RAIL E2E OK (state only)";
# --out-dir DIR keeps the transcript, the app log, notify.log, stub.log, the
# step 10 attention reads (step10-*.json) and PID-matched captures;
# --appearance light|dark.
Scripts/Tests/test-primary-rail-e2e.sh   # -> prints "PRIMARY RAIL E2E OK"

# The Zed layout parity (spec 2026-11-08-zed-layout-parity-design.md §6.1) in a
# real, isolated Sirio under Xvfb, measured from the rects the app drew
# (`surface layout read`'s `shellRects`, debug builds): the sidebar 300 and the
# right panel 240, the two centre halves 455.5 / 575.5 wide (+-1) at 1470x833 /
# 1710x987, the title bar 34, the tab strips 32, the status bar 30, the right
# header 32, a sidebar section header 31 and a one-line card 31 high, the
# right header's last icon and the Secondary's close button inside their
# parents, the sidebar's card inside the panel less its border; at 1100x800
# the halves are laid out exactly 320 wide; at all three sizes the Secondary
# ends exactly at the centre card's content edge (#723); the width migration (a stored 280 / 320 at
# schema 26 draws and stores 300 / 240, 350 / 400 is kept, a stored 280 at
# schema 27 is kept, an unparseable width falls back to 300); a divider dragged
# with real X input is saved and drawn again after a relaunch; light theme at
# interface font 18 draws the exact heights (the card 58) and a light
# background (a patch of the right panel is dark in dark-1470, light in
# light-1470-font18), then font 13 in light at 1710x987 draws case 1's
# numbers again. Every assertion is a row of measurements.tsv and the run
# stops at the first wrong one. --display :N is a private Xvfb of at least
# 1720x1000 with Mesa's lavapipe; --out-dir DIR keeps transcript.log (the
# commit, the binary's sha256, every sirioctl call and its exit status),
# app.log, measurements.tsv (case, key, expected, drawn, ok), reads/ (the
# decoded shellRects of every successful assertion read, <case>.json then
# <case>-2.json ...) and PID-matched captures in frames/.
Scripts/Tests/test-zed-layout-e2e.sh   # -> prints "ZED LAYOUT E2E OK"

# The sidebar in Codex's shell (specs 2026-11-17-sidebar-codex-shell-design.md
# §3, §6 and 2026-11-20-sidebar-codex-rows-design.md §5) in a real, isolated
# Sirio under a private Xvfb, driven by real X clicks, wheel turns and keys at
# the rects the app reports (`shellRects`' sidebar keys) and read over the
# control socket (`surface attention read`'s `sidebar` object: listFocused,
# filterShown, hoverCardRow, bell, cards; and the served `primary` flag of
# `project list`). Steps: the header, New Chat row, section header, project row
# (31) and one-line card (31) drawn at their heights; the project row draws no
# band (one colour, the section header's), its folder is centred under New
# Chat's icon, and a 1 px guide runs from its bottom to its last card's bottom
# and stops there; the project row folds and unfolds (its cards and its guide
# leave the list); the project row and the section header change under the
# pointer, and each hover-only control (the row's + and three dots, expand all /
# collapse all, the card's cross), cropped at its own rect from the layout
# read, holds more than one colour under the pointer and exactly one at rest (the
# frames/ keep the by-eye record); the title menu
# switches to Sessions and back; the hover card opens beside the sidebar (4 px
# past its visible right edge) and closes when the pointer leaves; the main
# worktree is the Primary, a click on its star unsets it (served flag true then
# false, the star crop changes) and a second click restores it, the Primary's
# star being drawn at rest and under the pointer; a session left unread (a
# codex stub reports running then done in the other worktree) lights the bell,
# whose menu selects that session's tab; Search shows the filter and
# Esc hides it; a click on the centre pane, then a card, then the centre pane
# moves listFocused false, true, false, and the list's left edge is the same
# focused and unfocused (no focus ring is drawn); a plain folder project's card
# is named after its folder (fallback), and reads missing after the folder is
# moved away and Sirio relaunched; a worktree with 9 tabs keeps its star inside
# its 31 px card (its tabs opened over the control socket); a project of 40
# worktrees makes the list overflow, and on idle frames with the pointer off
# the list the bar is judged by direction: the visible frame's 14 px strip holds
# a vertical run of 30 px or more of one colour that is not the strip's ground
# (the thumb), the faded frame (2.2 s later) holds no 30 px run of it in that column, and
# the list body is the same in both (it fails when no bar is drawn, or when the bar
# is drawn only in the faded frame). Step 12 sets the interface font to 18 (the app
# is quit, appearance.uiFontSize written to its database, and relaunched), adds a
# fourth project of 40 worktrees, checks that a card is 36 px high, and turns the
# wheel until the list is scrolled past where the old row-height sum (a card counted
# at 31) named the fourth project and short of where its header is drawn at the top:
# the layout read's `sidebarList` ({sticky, scrolled}) must name the third project
# as the sticky one. --display :N is a private Xvfb
# of at least 1600x1000 with Mesa's lavapipe; --appearance light|dark (run both);
# --state-only runs with no display and checks only that the sidebar read is
# reported; --out-dir DIR (must be empty) keeps transcript.log, app.log,
# measurements.tsv (case, key, expected, drawn, ok), reads/, attention-*.json and
# PID-matched captures in frames/<what>-<appearance>.png.
Scripts/Tests/test-sidebar-shell-e2e.sh   # -> prints "SIDEBAR SHELL E2E OK (dark|light|default|state only)"; (default) is the line without --appearance

# The font setting (Settings, Appearance, Interface, Font) in a real, isolated Sirio under a
# private Xvfb, by real clicks and keys: a fresh database reads atkinson in every layer (the
# startup registration time goes to measurements.tsv); each of the seven pairs is chosen in turn
# through the Font select (arrow keys and Enter, figtree by a click on its row), then `fonts`
# names the pair's families in Sirio's typography, Ely and bezel, every measured width (ten `m`, `a`, `0`) is
# within 0.5 px of the advances Scripts/Tests/font_face_advance.py reads from the bundled file of
# that slot, and the sidebar's text and the editor's code crop differently from the previous
# pair's (plex chosen twice crops the same); Escape closes the open list only, a second Escape
# closes Settings; an interface size step keeps the pair, and the terminal's crop never changes;
# quit and relaunch brings the pair back, an unknown saved id reads as atkinson; at interface
# size 18 in Atkinson no digit of a five-digit diff line number crosses its cell's left edge
# (the pixel strip left of the cell). Where a width cannot tell a weight or a family (most
# monospace faces draw `m` at 0.600 em, which a fallback with the same advance also passes;
# Atkinson Mono, 0.632 em, and Fira Code, 0.615 em, are told apart) measurements.tsv says so in
# weight-limit, italic-limit and width-limit rows (status limit), and a bundled family also installed
# on the machine in an installed-limit row; the last check prints the ok, limit and skip counts. --diff-only runs just the diff-number step, which fails when
# `diff_number_width` is forced to the old 40 px (the proof it can). Frames, per-pair sheets,
# contact-sheet-<appearance>.png (the seven windows, labelled, scaled down) and
# contact-sheet-detail-<appearance>.png (each pair's chat reply and editor lines at 1:1) are in
# frames/; the editor's rust server is Scripts/Tests/fake_lsp.py, so no error covers the code.
# --display :N is a private Xvfb of at least 1600x1000 with Mesa's lavapipe; --appearance
# light|dark (run both; dark when omitted); --state-only runs the fresh and unknown-id reads
# with no display;
# --out-dir DIR (must be empty) keeps transcript.log, app.log, measurements.tsv, reads/, crops/
# and the frames.
Scripts/Tests/test-font-setting-e2e.sh   # -> prints "FONT SETTING E2E OK (dark|light|diff only|state only)"

# Acting on a change request: every write of sirio_forge on the wire (the token
# transport and the real gh/glab), then the tab's actions in a real, isolated
# Sirio driven by the debug-only `surface change-request act` verb. --stage NAME
# runs one stage (wire-github, wire-gitlab, failures, merge, metadata, cli,
# scopes, ci, ui); ci covers re-runs and job logs. --state-only skips captures;
# --appearance light|dark seeds the isolated app's appearance.
Scripts/Tests/test-forge-actions-e2e.sh   # -> prints "FORGE ACTIONS E2E OK"

# Checking a change request out into a worktree (change requests C1): same
# repository, forks that accept and refuse pushes, a gone branch and a closed
# request, reuse with fast-forward, refusals and the pinned card, against bare
# repositories and the fake forge; then handing it to an agent (C2): stub CLIs
# on PATH for every adapter, every purpose on GitHub and GitLab, the chat, the
# warning, the refusals and a worktree switch mid-hand-off; same flags as the
# other forge E2Es.
Scripts/Tests/test-change-request-handoff-e2e.sh   # -> prints "HANDOFF E2E OK"

# Ely's Tabs and git badges with Sirio's theme, on a private Xvfb with
# Mesa's lavapipe: PID-matched captures, a click per tab, the badge tooltip,
# dark and light (docs/testing/ely-forge-probe.md).
Scripts/Tests/test-ely-forge-probe.sh --xvfb   # -> prints "ELY FORGE PROBE OK"

# The selection painted under a Markdown body drawn by ely-documents through Sirio's host gather
# (a chat response, the Markdown Preview, a change request): a selected document whose rows
# soft-wrap, break and wrap a code span, on a private Xvfb; fails when any row's wash starts
# after its first glyph (sirio_ui::markdown::gather).
Scripts/Tests/test-markdown-selection-probe.sh --xvfb   # -> prints "MARKDOWN SELECTION PROBE OK"

# The eight Settings pages (and Git Hosting with a hand-written forge source), a
# tab context menu and a tooltip, drawn by the `surfaces_probe` example (one
# process per frame and appearance, seeded through the public builders only; a
# state they cannot reach prints `GAP <frame> <state>`), on a private Xvfb with
# Mesa's lavapipe: PID-matched captures, at least 200 colours a frame, the
# tooltip proven by its difference from the pre-hover capture, and a Settings
# page longer than the window scrolled with the wheel into -p2, -p3, ... frames
# (printed as `PAGES: <frame>-<appearance> <n>`; a page that still scrolls after 8
# fails; SURFACES_MAX_PAGES=N changes the cap).
# They are the probe frames of the bezel-off capture set
# (docs/testing/bezel-off.md). --xvfb | --display :N, --out-dir DIR,
# --appearance dark|light (without it both), --no-build.
Scripts/Tests/test-surfaces-probe.sh --xvfb   # -> prints "SURFACES PROBE OK (<n> frames, <g> gaps)"

# Dragging a scrollbar thumb, for the three bars the app draws (scroll::bar, the
# transcript's list_scrollbar, horizontal_scroll::bar), side by side in the `scroll_drag_probe`
# example on a private Xvfb with lavapipe: a real xdotool press at each thumb's centre, 3 px, 1
# px, then 120 px of travel; the thumb must have moved as far as the pointer did (4 px tolerance)
# and a second drag of +2000 px must leave it flush with the track's end. This is the "before"
# behaviour of every PR that changes what draws a bar (docs/testing/bezel-off.md).
# --xvfb | --display :N, --out-dir DIR, --no-build, --no-software-vulkan;
# SCROLL_DRAG_PROBE_SKEW=40 expects 40 px too much travel, and the run must fail.
Scripts/Tests/test-scroll-drag-probe.sh --xvfb   # -> prints "SCROLL DRAG PROBE OK (3 bars)"

# The horizontal bar sirio_ui::markdown draws under a fence or a table wider than
# its column (markdown/sideways.rs, vendored changes L8 and L9): a real xdotool
# drag of each thumb to the end, the fence's last glyph clear of the edge, a
# second document left unscrolled, and the press kept from the host's own
# handler (the chat starts a selection there); dark and light.
Scripts/Tests/test-markdown-scroll-probe.sh --xvfb   # -> prints "MARKDOWN SCROLL PROBE OK"

# The file tab's editor (Ely's CodeEditor under sirio_ui::file_view) in a real,
# isolated debug Sirio on a private Xvfb, driven through the debug-only
# `sirioctl surface editor` verbs, real X keys and clicks, the Settings page and
# the files on disk, with a fake language server (Scripts/Tests/fake_lsp.py):
# editing, CRLF, disk conflicts, find and replace, hover and definition, the
# breadcrumb, snapshots, git marks, the status row and its bell, the worktree
# drawer (Problems, Search, Replace all) and every Editor preference.
# --xvfb or --display :N picks the display (never your own $DISPLAY);
# --state-only takes no captures; --appearance light|dark; --out-dir DIR keeps the
# transcript and the PID-matched captures; --only NAME[,NAME...] runs some
# scenarios and prints "EDITOR E2E PARTIAL" instead; --no-file-watch starts the
# app with SIRIO_NO_FILE_WATCH=1 (as macOS has no watcher) and is meant for
# `--only replace_mixed`, which then proves Replace all tells the open tabs itself.
Scripts/Tests/test-editor-e2e.sh --xvfb   # -> prints "EDITOR E2E OK (<n> scenarios, appearance <a>, state-only <0|1>)"

# Sirio as Claude Code's IDE (/ide), in a real, isolated debug Sirio on a private Xvfb with a
# stand-in client (Scripts/Tests/ide_client.py: stdlib RFC 6455, the mcp subprotocol, the token
# in x-claude-code-ide-authorization) running in one of its panes. A private HOME (the lockfile
# goes to $HOME/.claude/ide/), database, socket, host root and a TMPDIR on disk: nothing of your
# real ~/.claude or a running Sirio is touched. Cases: lockfile_and_env (six keys, 0600, the
# port in a new pane; the state directory, made 0755 before the launch, is 0700 and the database, made 0644, and its -wal and -shm are 0600), token_refused (401, no connection), handshake (the mcp echo, exactly four
# tools), switch_off (lockfile gone, connection closed, then the same port and token),
# switch_flap (off, on, off, on from one process while a start is held in flight: the original
# port, one lockfile), port_survives_catalog_change, restart_keeps_port (same port and token across a quit; the old
# client reconnects) and inherited_port (a launch's CLAUDE_CODE_SSE_PORT never reaches a pane),
# then, with the clients in real application terminal tabs (`surface.terminal.open`): owner_resolved
# (the state names the pane the client runs in and its worktree), selection_utf16 (opening a file
# sends its selection once; selecting the `x` of `let s = "<emoji>x";` sends start {2,11}, end
# {2,12}), selection_coalesced (20 selections 25 ms apart: the bound of one notification per send window
# plus one, and the last is the final selection; the 100 ms window of a release build is proved
# by the unit tests of `sirio_ide::coalesce`), focus_sends_selection (a terminal tab sends
# nothing, the file tab again exactly one, each counted after the send window), at_mention (two Claude terminals, the first focused before its client
# starts: lines 5 to 7 reach it as at_mentioned lineStart 4, lineEnd 6, its tab is focused and
# nothing is typed into the pane), selection_while_terminal_active (a mouse click and Shift+Down
# in the file of the Secondary half, with the Primary's terminal active, reach the client),
# at_mention_to_new_terminal, no_eligible_agent_toast (with no Claude terminal or chat the state's
# toasts hold "No Claude terminal or chat in this worktree"), at_mention_without_focus_history (an
# owned terminal in no focus history still gets it) and owner_after_restart (a client whose pane is
# in a worktree not selected after a relaunch is ownerless, in the worktree its cwd is in, until
# that worktree is selected; no file tab is restored, so only the selection resolves it), at_mention_after_relaunch (the same relaunch with a file as the active tab: one at-mention reaches the client with no focus recorded, focuses its tab and raises no toast) and selection_not_lost_to_another_worktree (a file of another worktree in this strip: select here, then select that tab once the select is answered; the case fails unless that tab change was answered inside the send window, and the client still gets the first selection), diagnostics_file (the fake language server reports a warning on `hello` in a.py; getDiagnostics {uri} answers one item, uri echoed, severity "Warning", start {1, 4}, end {1, 9}), diagnostics_outside_worktree (file:///etc/hostname answers []), diagnostics_workspace (a.py of a second worktree has the same finding; getDiagnostics {} from a client owned by the first worktree's pane answers only that worktree's files), diagnostics_ownerless_cwd (after every earlier delayed resolution has fired, a client run by the script outside every Sirio shell, cwd in the second worktree, asks getDiagnostics {} at once: only that worktree's files, and the state's hook_resolutions, the count of resolutions a hook started rather than a request, is the same before the client started and right after the answer, else the case fails naming the broken precondition; that connection, found by pid, is then ownerless there) and diagnostics_ownerless_elsewhere (the same, with the same hook_resolutions guard, with its cwd outside every worktree: no worktree, and the answer holds the files of every sidebar worktree), then forty cases of `openDiff` (the Proposal tab; `surface.ide.act` presses what a user presses): accept (a tab titled "✻ p.txt" opens in Secondary, focus stays on the Claude terminal, nothing is answered until Accept, which answers FILE_SAVED and the proposal), edit_then_accept (a CRLF file comes back as exactly the edited text with CRLF), edit_keys (real xdotool keys typed into the Edit body), reject, reject_with_comments_clipboard (an Old-side comment on a line and a New-side comment on a range, written with `surface.ide.comment`, which opens the composer, sets its text and presses Add like a click and typing: the state counts two comments, two cards and no open composer; Reject answers DIFF_REJECTED, the state says last_delivery "clipboard", the toast says "Comments copied: paste them into Claude", and the CLIPBOARD, read from outside the app with python3-gi on the run's display, is exactly the two `@<path>#L<n>` lines, `#L<a>-<b>` for the range, `(proposed) ` before the New side's text), comment_composer_anchor (a 300-line file with every tenth line changed (line 150 starts below the viewport): `comment ... commit=false` opens the composer at Old line 150 with a seven-character draft, no comment and no card, and the state's in_view (read from the diff list's own gpui layout) turns true once the diff has scrolled that line into view; a Reject leaves the clipboard, which the case owned before, as it was and `deliveries` unmoved: a draft is never delivered), composer_typing_keeps_line (opening the composer focused its input: real keys with no click, the draft grows and in_view stays true; a proposal arriving then joins the strip and the typed-in tab stays shown), comment_refusals (each error leaves the tab as it was and names its reason: an unknown tab, a line outside the diff, a bad side, a malformed end_line or commit, a comment while Edit shows no diff, a New-side comment on the diff of the edited text), comments_on_withdraw (two comments and an open draft, then Claude's close_tab: one delivery, the clipboard holds the two comments and not the draft), comments_on_close_all (two proposals with a comment each, then closeAllDiffTabs: one delivery, both comments on the clipboard), comments_on_replace (an openDiff with the same tab_name delivers the old proposal's comment), comments_on_accept (Accept answers FILE_SAVED and the comment still reaches the clipboard), comments_on_confirmation_reject (a comment, `act close`, confirm_reject: DIFF_REJECTED, one delivery, the clipboard), comments_on_disconnect (a comment, then the client closes its socket: one delivery, the clipboard; `deliveries` in the state counts clipboard deliveries since launch), close_confirm (closing asks "Closing rejects Claude's change"; Cancel answers nothing; Reject answers DIFF_REJECTED), close_others_guarded (Close Others and `surface.tabs.close confirm=yes` ask before anything closes), close_tab_withdraws (TAB_CLOSED, the openDiff gets -32000), close_all, replace_in_place, disconnect_closes (the toast, no decision), unannounced_open_diff (a client that never sends ide_connected is a connection from its handshake, state pid null: its openDiff opens ownerless and Reject answers it; a second one killed with a comment on its proposal: the proposal closes, the toast, the comment on the clipboard), new_file, fifo_path (the refusal body, Edit unavailable, Reject answers), too_large (the refusal body, Accept answers Claude's text), dirty_warning (the bar warns as the editor tab turns dirty), disk_changed_warning (a file open in an editor tab with its proposal open: a focus changes nothing, a save puts "The file changed on disk since Claude proposed this change: the diff shows it as it was" on the bar, read back from the drawn warning, unsaved edits win while they last, the counts and Accept stay; a write from outside warns a second proposal), proposal_split_refused (`pane.split` on the active Proposal tab changes nothing; Accept closes only the proposal), remove_prompt_stable_rows (the sidebar's own "Remove worktree?" prompt, opened with `act ask_remove_worktree` and answered with a click, with a worktree added in front of the asked one while it is up: the asked one is removed, the others are intact), captures (Unified and Split), no_decision_without_user (switching off, the client dropping and quitting send no decision for a pending openDiff, read while the client still reads or just before it drops its own socket), reject_leaves_no_inert_tab (Reject in the confirmation closes the proposal's own tab even when the dirty-tab prompt is up and then cancelled), confirm_stable_tabs (a Close Others question survives a worktree switch without closing anything of the other worktree), confirm_stable_worktree (three worktrees made with git under one directory; the removal question of the middle one is open while the first is removed behind the app's back and the sidebar rescans, so its row moves up: Reject removes the asked worktree, not the one that took its old row), focus_in_secondary_keeps_editor (real clicks and keys: with the keyboard focus in the file shown in Secondary, an openDiff joins the strip and the keys typed after still land in that file's buffer), focus_in_primary_shows_proposal (focus in the Primary terminal: the proposal becomes Secondary's shown tab), bar_click_keeps_focus_out (a click on the proposal bar's text leaves the keyboard outside Secondary, so the next proposal is shown), symlink_title (a CLAUDE.md symlink shows as CLAUDE.md), worktree_close_guarded (`workspace.close` refuses; the sidebar's confirmed worktree removal asks before git touches the checkout or any session ends; Cancel keeps the checkout on disk and in `git worktree list`; Reject answers first, then the checkout is gone), project_close_guarded (the same for the confirmed project removal on a second project), and parked_worktree (a relaunch with the mounted-worktree cap seeded at 2 by `appearance_seed --limit-mounted 2`, the stale scratch directory swept at launch: a proposal for a worktree not on screen has no tab and a toast, comes back with its worktree and survives its eviction, the bar naming its owner terminal; a Claude outside every Sirio shell, with no owner, proposing a file of a worktree not on screen is held under the file's worktree too): sixty-five cases. `--proposals-only` runs only those forty and `--only a,b` only the named ones (development; the last line is IDE BRIDGE PROPOSALS ONLY). The run starts no language server but the fake one: the first-match languages.toml maps every extension the run opens (.py, .rs) to it and SIRIO_CONFIG_DIR is unset; an end check (a denylist over the live children of the app's last launch) is only a backstop.
# --xvfb or --display :N picks the display (never your own $DISPLAY); --state-only takes no
# captures; --out-dir DIR keeps transcript.log, app.log, client-*.jsonl (every frame), socket.jsonl,
# the lockfile with authToken redacted and the PID-matched captures. socket.jsonl is one line per
# request the script sent to the app's control socket, with the app's complete answer (sirioctl's
# calls pass through a recording proxy; a request with no answer is a line with an `error` and no
# `response`); not recorded: the `system.ping` probe sirioctl sends before `panel.*` calls. The
# run sets three debug-only knobs (read under cfg(debug_assertions), so absent from release
# builds): SIRIO_DEBUG_IDE_START_DELAY_MS, with which each listener start holds its bound port
# for that long, keeping a start in flight while switch_flap flips the switch; SIRIO_DEBUG_IDE_RESOLVE_AFTER_MS=3000 (the script's RESOLVE_AFTER_MS), which delays the first resolution a connection's announcing itself starts (never one a request runs; when it fires it resolves every ownerless connection, and nothing cancels it); and SIRIO_DEBUG_IDE_SEND_AFTER_MS=1000 (the script's SEND_AFTER_MS), with
# which the app waits 1000 ms, not 100 ms, before it sends a connection's coalesced selection,
# so a select and a tab change sent one after the other land in one send window (the case
# fails if the pair took longer); every selection case waits past that window.
# Run it with `env -u WAYLAND_DISPLAY` from a Wayland session.
# --proposals-only runs only the openDiff cases, --only a,b only the named ones (development
# shortcuts: they print "IDE BRIDGE PROPOSALS ONLY", not the evidence of a run) and --live is the mode below.
# Run it with the load under 4 and Zig 0.15.2 first on PATH. The run root is $SIRIO_E2E_TMP, else
# ~/.cache/st/ide-e2e-tmp (on disk: /tmp is a small tmpfs); TMPDIR does not choose it (it only affects
# the cargo build; the run sets its own TMPDIR inside the root).
Scripts/Tests/test-ide-bridge-e2e.sh --xvfb   # -> prints "IDE BRIDGE E2E OK (<n> cases, state-only <0|1>)"

# The live mode of the same script, by hand only (each run spends two real Claude turns against a
# local fake model API, so it is never part of a gate and never runs without the flag): the real
# `claude` in an application terminal tab of the isolated debug Sirio, against
# rust/crates/sirio_term_session/tests/live/fake_messages.py ("run edit"), in Claude's default
# permission mode so each Edit asks and calls openDiff. Two cases. live_accept: the pane shows
# "Opened changes in Sirio" (a debug build says "Sirio (debug)"), the state names the pane as the
# connection's owner, the tab "✻ probe.txt" opens, `surface.ide.act accept` makes Claude write the
# file (checked on disk: alpha becomes beta) and the turn ends. live_reject_with_comment: "run edit"
# again; one comment on Old line 1 is committed through `surface.ide.comment`, then Reject: the
# CLIPBOARD (owned with a sentinel first, read from outside the app) is exactly
# `@probe.txt#L1 keep beta here`, `deliveries` went up by one, the pane shows Claude's rejection
# ("ejected update to probe.txt": 2.1.325 printed "User rejected update", 2.1.326 prints "Rejected update")
# and the file is unchanged. The CLI is $SIRIO_LIVE_CLAUDE, else the newest real
# install under ~/.local/share/mise/installs/claude/<version>/claude; the ~/.local/bin wrapper (it
# runs `mise use -g`) and any `#!` file that mentions mise abort the run, as in
# program_status_live.rs. A missing CLI prints "SKIP: claude not found ..." and exits 0. Beyond the
# run's private HOME, database, socket, host root and Xvfb, Claude's configuration is seeded under
# the private HOME and its environment is built from nothing with a fake key and a dead proxy, the
# run aborts before anything starts when the private HOME is the real home or inside the real
# ~/.claude, the run root defaults to a fresh `mktemp -d /var/tmp/sirio-ide-live.XXXXXX` (mode 0700: /var/tmp is world-writable and the root holds the token and Claude's configuration) instead of ~/.cache/st (on disk,
# but outside the home: a real claude reads the files of every ancestor of its
# working directory; SIRIO_E2E_TMP overrides it, and a root inside the real home aborts before anything
# starts), and the cleanup kills, and logs, every process still carrying the run's CLAUDE_CONFIG_DIR. The artifact adds
# claude-version.txt (`claude --version` and the binary's path), fake.log and live-pane-*.txt.
Scripts/Tests/test-ide-bridge-e2e.sh --xvfb --live   # -> prints "IDE BRIDGE LIVE OK (claude <version>, 2 cases)", or "SKIP: ..."

# A remote image in the Markdown Preview, in the real app: a .md whose image is a 64x64 PNG of
# one colour, served over loopback by python3 -m http.server, opened in Preview on a private
# Xvfb. The PID-matched capture must hold that colour in a block of at least 32x32, and the
# server's log at least one GET of the image, one answered 200 (an engine without an HTTP client
# draws nothing there). --mutate missing-image names an image the server lacks: the run must
# FAIL, the proof it can go red. --xvfb | --display :N, --out-dir DIR.
Scripts/Tests/test-markdown-remote-image-e2e.sh --xvfb   # -> prints "MARKDOWN REMOTE IMAGE E2E OK (block <n>x<n>, <n> GET, display <d>)"

# One Markdown document (Scripts/Tests/fixtures/markdown-documents/showcase.md) drawn on the three
# surfaces that render Markdown, for a build-against-build comparison (the baseline was drawn by the removed bezel-markdown): the file Preview, a native
# chat reply (chat_fixture.py mode reply-file) and the description of change request #101 on the
# fake forge. Isolated instances on a private Xvfb (:120-:139, started and stopped by the script),
# a 1600x1000 window, the wheel turned in steps until a step changes nothing, PID-matched captures.
# Writes preview-<k>.png, chat-<k>.png, cr-description-<k>.png and transcript.json to --out-dir.
# --bin-dir holds sirio and sirio-host (sirioctl too, else rust/target/debug/sirioctl); the chat leg
# needs rust/target/debug/examples/ely_chat_seed. --display :N, --only preview,chat,cr-description.
# The chat leg also captures the states of the richer chat messages, each in an isolated world of the
# chat E2Es' harness (Scripts/Tests/chat_e2e_harness.py; the stub `claude` of Scripts/Tests/stubs/richer): state-day-separators and
# state-day-separators-top, state-limit-card, state-turn-failed, state-turn-failed-second, state-held-queue,
# state-thought-settled and state-mid-reveal-<k> (a reply at each received size in $REVEAL_AT, default
# 1800,6000,12000, which fails if the reply had ended when the frame was shot), and chat-states.json
# (what the readout said at each). They go through socket calls a debug 0.47.1 already served, so a
# build from before the delivery can be compared with the head: run both with debug builds. They need
# Mesa's lavapipe (the script stops without it), and they depend on the date and on timing, so a
# re-run differs in them: the separators' day labels, a limit card's reset time, the turn footers' time of day,
# and how far a reveal has come.
Scripts/Tests/capture-markdown-surfaces.sh --bin-dir DIR --out-dir DIR   # -> prints "MARKDOWN SURFACES CAPTURED (<n> frames)"

# Markdown on ely-documents in the real app (spec 2026-11-20-ely-markdown-documents-design.md §6.2): an
# isolated debug Sirio on a private Xvfb (:120-:139, started and stopped by the script) opens the
# showcase fixture plus a late section (the driver is Scripts/Tests/markdown_documents_e2e.py). Real X
# input aimed by the Preview's debug state (`surface editor state`: preview_nodes, preview_scroll,
# preview_texts, preview_links, preview_outline_at, preview_outline_toggle_at, preview_diagrams) and
# the display's real clipboard (GTK, python3-gi): node counts; a `[[toc]]` entry, a `#slug` link and
# an outline row each bring their heading to the top; the reading share grows; `<img width=120>` draws
# 120 px wide (a capture scan); Mermaid is rendered into the cache directory, PlantUML too when
# `plantuml` is on PATH, else `SKIP plantuml (not on PATH)`; a drag plus Ctrl+C and the right-click
# menu's Copy put the expected text on the clipboard; Ctrl+C in the terminal still echoes ^C at the
# PTY; a chat reply of at least 40 KiB, culled at the tail, is copied whole by the transcript menu's
# Select All and Copy (equal to the transcript text), and a jump to its top draws no blank band; the
# deepest nesting (MAX_NESTING levels) opens in a Preview and arrives as a chat reply, both drawn as quotes (a row of
# exactly `deep-end`, which the plain-paragraph fallback does not make), and the app lives. A third leg, `pictures`
# (a chat of its own), sends one reply of eleven pictures over 48 px single-colour files: a plain path and paths holding `#`,
# `?` or `%XX` draw the exact file (four colours, each a square of 40 px or more), and one that names a device (`COM1`,
# `nul.png`, `CONIN$`), ends a component in a dot or a space, holds a colon outside the drive, or is a `file:` URL
# decoding to a device name draws as its alt text (the drawn row is exactly the alt), with no decoy file's red or
# yellow square of 8 px or more; MARKDOWN_E2E_LEGS=pictures runs that leg alone.
# --mutate no-sections starts the app with SIRIO_DEBUG_MARKDOWN_NO_SECTIONS (a debug-build knob: the
# Preview is drawn without `sections`): the run must FAIL at anchor-toc. --display :N, --out-dir DIR
# (default artifacts/ely-md/e2e: transcript.json, frames/, the clipboards and expected texts).
Scripts/Tests/test-markdown-documents-e2e.sh --display :122   # -> prints "MARKDOWN DOCUMENTS E2E OK (<n> checks, <n> skipped, display <d>)"

# The environment of an opened link: the app starts through a fake AppImage mount's AppRun on a
# private Xvfb with both WAYLAND_DISPLAY and a DISPLAY set (so it forces X11 for itself), a
# Claude chat on the file-links stub draws a web link, and a real click on it reaches an
# xdg-open stub (first on the login PATH of the private ~/.profile) that dumps its environment.
# The dump must name no mount entry in PATH, LD_LIBRARY_PATH or XDG_DATA_DIRS, carry no APPIMAGE,
# APPDIR, GTK_THEME or SIRIO_LAUNCH_ENV, keep the user's WAYLAND_DISPLAY, and list the login PATH entries before the launch PATH's. Every
# failed assertion is printed. One click covers the engine's only spawn site Sirio reaches, `cx.open_url`
# (open_uri_internal); nothing calls reveal_path or open_with_system, so those two cannot be tested. Removing gpui_linux::set_child_environment from main.rs (by hand, then rebuild) must turn
# it red on a mount entry. Linux only. --xvfb | --display :N (2400x1200 or larger), --out-dir DIR.
Scripts/Tests/test-child-env-e2e.sh --xvfb   # -> prints "CHILD ENV E2E OK (display <d>, dump <path>)"

# How Sirio launches the built-in `jdtls` around Lombok (spec
# docs/superpowers/specs/2026-10-26-jdtls-lombok-design.md) in a real, isolated debug Sirio on
# a private Xvfb, with a fake `jdtls` on PATH that logs each launch's argv and project and
# speaks LSP through Scripts/Tests/fake_lsp.py, and fake Lombok jars in a private HOME. Eleven
# scenarios: restart (no jar at launch, one lands, the next publish stops the server with
# shutdown and exit and launches it again with the agent, sent the open file and one full
# build, and a newer jar afterwards causes no third launch), restart_held (the restart from a
# launch not yet answered its full build: the old router is cancelled, so the relaunch keeps
# the newer agent), agent_at_launch (a parent POM alone mentions Lombok: the newest jar of the
# Maven and Gradle caches as one argv element, one `java/buildWorkspace` after ServiceReady),
# agent_from_settings (the jar found through settings.xml's localRepository, a path with a
# space), no_lombok (no argument, no build, and later publishes restart nothing with a jar in
# the caches), pom_gains_lombok (a project whose build file gains Lombok after its launch is
# restarted once with the jar's agent at a later publish), fallback_init and fallback_early (a
# launch with the agent that fails, at once or after `initialized`, is launched once more
# without it), fallback_stuck (an `initialize` that fails while the process lives on: the
# process is killed before the launch without the agent), stderr_flood (200 KB on stderr and
# the server is still ready) and hand_written (a languages.toml entry is launched with exactly
# its own arguments, and sent no build).
# --xvfb or --display :N picks the display (never your own $DISPLAY); --out-dir DIR keeps the
# transcript, app.log, the launch log and each launch's request log; --only NAME[,NAME...]
# runs some scenarios and prints "JDTLS LOMBOK E2E PARTIAL" instead.
Scripts/Tests/test-jdtls-lombok-e2e.sh --xvfb   # -> prints "JDTLS LOMBOK E2E OK (11 scenarios)"

# The same feature on the real jdtls, opt-in and never part of Scripts/ci.sh: a debug Sirio on
# a private Xvfb launches the real jdtls (a `jdtls` stub on PATH that logs each launch and runs
# your launcher on your Java 25, with the private HOME as user.home) in a Spring Boot + Lombok
# Maven project the script writes, with an empty $HOME/.m2. Run 1: the first launch has no
# agent, jdtls's own import downloads Lombok, the next publish restarts it with the agent
# from $HOME/.m2, and the open file's diagnostics and the drawer's error count (which covers
# files nobody opened) read 0 and hold 0 for 60 s, counted only once the drawer shows the
# warning of an unused import in PegaService.java, a file nobody opens: the current process
# has published it, so a zero is not just the empty state after a restart (the full build
# Sirio sends after ServiceReady is what clears stored markers, inside the hold). Run 2: the
# app is ended with SIGTERM (Sirio installs no SIGTERM handler, so its quit path does not
# run), every jdtls is confirmed gone (the servers ended on their own), and the relaunch
# carries the agent from the start over the workspace Run 1 left, with the same condition.
# Run 3, the control: a hand-written java entry in languages.toml runs the same stub on a
# fresh -data directory with no agent, over the Maven cache Run 1 filled; the open file's
# diagnostics and the drawer's errors read at least 1, with PegaService.java among the files,
# and it stays one launch with no -javaagent. The summary reports the worst diagnostics and
# errors seen while each run waited for its clean state. It needs network access to Maven
# Central, a jdtls install and a Java 25 home, and takes about 5 minutes (the first
# Maven import downloads the dependencies). --jdtls PATH and --java-home DIR are required;
# --xvfb or --display :N; --out-dir DIR keeps transcript.log, jdtls-launches.log, app.log and
# jdtls's own workspace logs; --no-build uses the binaries already built.
Scripts/Tests/test-jdtls-lombok-live.sh --xvfb --jdtls PATH --java-home DIR   # -> prints "JDTLS LOMBOK LIVE OK"

# The editor's hosting hooks on a real frame, on a private Xvfb with Mesa's
# lavapipe: the host's per-line colour (and Ely's own lexer kept out), a git mark,
# a diagnostic, the gutter, and the events a host hears, each from a real xdotool
# gesture; the text cursor over the code (read from X with XFixes, so it needs
# libXfixes); and a line wider than the window scrolling sideways under a gutter
# that stays put (the wheel, Shift and the wheel, a drag of the bar's thumb, End
# and Home). --xvfb or --display :N, --out-dir DIR, --no-build,
# --no-software-vulkan.
Scripts/Tests/test-ely-editor-probe.sh --xvfb   # -> prints "ELY EDITOR PROBE OK"

# The contract of Scripts/capture-diff.py, the pixel diff of two capture sets (synthetic 64x64
# PNGs made with ImageMagick; no display, no build): identical trees, one changed pixel, the
# tolerance (1 level in one or every channel or in alpha is noise, 2 levels is a change, and
# --tolerance moves the line), a mask rectangle one pixel short or one pixel off, an
# allow rectangle that covers half the change or belongs to another frame, a frame only on one
# side, a size change, a malformed table, a tree with no frame, a file that is not an image, and
# what changed.tsv and index.html list. Needs `magick` and python3.
Scripts/Tests/test-capture-diff.sh   # -> prints "CAPTURE DIFF CONTRACT OK"

# The capture set a bezel-off PR is judged by (docs/testing/bezel-off.md): the frames of
# visual-sweep.sh (dark, light), the surfaces probe (once Scripts/Tests/test-surfaces-probe.sh
# exists), test-changes-e2e.sh (dark, light) and test-ely-chat-ui-e2e.py (its default only), taken
# from the tree it runs in, on lavapipe, with SIRIO_REDUCED_MOTION=1 (a debug-only switch that
# makes gpui snap every animation to its rest state). --out-dir DIR keeps captures.log,
# runs/<source>/ (each source's own artifact) and frames/<source>-<appearance>/ (the PNG set);
# --xvfb starts a private 1600x1000 Xvfb, --display :N uses one (the same one for the base and the
# PR runs). Scripts/capture-diff.py BEFORE/frames AFTER/frames --out DIR [--masks TSV]
# [--allow TSV] [--tolerance N] compares two sets (Scripts/capture-masks.tsv names the regions a clock drives)
# and prints CAPTURE DIFF IDENTICAL, CAPTURE DIFF OK (...) or CAPTURE DIFF FAILED (...).
Scripts/bezel-off-captures.sh --out-dir DIR --xvfb   # -> prints "CAPTURES OK: DIR (<n> frames)"

# The opt-in live tests of program status (hand-launched agents), never in Scripts/ci.sh: the
# real CLIs against local fake model APIs (tests/live/*.py, 127.0.0.1, a free port), each under a
# private HOME and XDG_*, with the child's environment built from nothing and each CLI resolved
# through a private bin/ of symlinks (override: SIRIO_LIVE_CLAUDE, SIRIO_LIVE_CODEX, SIRIO_LIVE_OMP,
# SIRIO_LIVE_PI, SIRIO_LIVE_OPENCODE; a missing CLI prints "SKIP: <cli> not found" and passes).
# program_status_live types the command into an engine shell session and prints the
# Event::Program / Event::Status sequence it saw: claude (idle once the probe is answered,
# working, done, and blocked kind=permission on a Bash ask), codex (idle unprompted, done,
# blocked on an escalation), omp (idle at start), pi (idle once answered, working, done) and
# the foreground program staying the agent while Claude and Codex run a 3 s shell tool.
# service_live starts a private OpenCode v2 service (`opencode service set port` before its first
# start, never the default 49374), drives one session over its REST API and expects the watch to
# say Busy, Waiting (the permission ask, answered once), Busy, Idle; it stops the service by the
# pid in its registration and checks the port is free. The binaries must be the real ones, not
# the mise wrappers of ~/.local/bin/claude and ~/.local/bin/pi. TMPDIR must be on disk and
# outside your home (/var/tmp/...).
cd rust && cargo nextest run -p sirio_term_session --test program_status_live --run-ignored only
cd rust && cargo nextest run -p sirio_opencode_watch --test service_live --run-ignored only
```
