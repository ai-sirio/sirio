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
Scripts/Tests/test-host-sessions-e2e.sh  # -> prints "HOST SESSIONS E2E OK (<n> cases, <s>s)"
# The sirioctl end to end, each world in its own HOME, XDG directories, SIRIO_DB and
# socket: the stable copy and its PATH link, replacement, a foreign file, the AppImage
# mount kept out of a pane, quit and relaunch, every stub agent's hooks, the skill,
# SIRIO_WORKTREE_ID and the flags sirioctl offers, `project add --display-name` (case 14:
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
# runs it with --xvfb or --display :N; --state-only skips cases 4, 4c, 5, 6, 10 and 15 and
# is what macOS runs. --out-dir DIR keeps the transcript, logs and env dumps (and case 15's
# case15-attention.json); --only N[,N...] prints "SIRIOCTL E2E PARTIAL"; --mutate
# claude-prompt-hook is case 6's mutation check.
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
# permission ask, a link kept through a title an older session also has, a turn under a second
# (finished, +1), two panes in one folder (no guess, then each by its title), a failed turn (the
# service's error text and password stay out of the app's stderr), a relaunch, and a service that
# goes away (the mark stays, no status, one `[opencode-watch] <kind>` line). Tabs no terminal
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
# rows: focus, Enter and Esc, skip against save, the queue, the clone and non-git paths, the
# Project Settings tab's label, the privacy of the perf trace, and the label across a focus refresh
# and a relaunch. A git wrapper in the debug build's own bin directory logs every git call and
# floods or stalls a clone or a discovery when a scenario asks. Linux runs it with --xvfb or
# --display :N, also under --state-only, which skips only the captures and the real X input;
# --out-dir DIR keeps transcript.log, <scenario>/app.log, projects.tsv and git-calls.log, and
# frames/<scenario>-*.png; --appearance light|dark (default light); --only NAME[,NAME...] prints
# "PROJECT IDENTITY E2E PARTIAL (<n> scenarios)". It needs xset (x11-xserver-utils) and
# python3-gi with gir1.2-gtk-3.0 for the clipboard.
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
# clicks and keys on the tab. The file_links scenario uses Scripts/Tests/stubs/file-links/claude,
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
# but takes no captures), trace_keys, file_links, clipboard_keys and composer_keys a framed run at --window-size 2200x1000;
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
Scripts/Tests/test-chat-session-panel-e2e.sh --state-only   # -> prints "CHAT SESSION PANEL E2E OK (<n> scenarios, <k> skipped, appearance <a>, state-only <0|1>, window <w>)"
Scripts/Tests/test-chat-session-panel-e2e.sh --state-only --only notification_click   # -> prints "CHAT SESSION PANEL E2E PARTIAL (1 of <n> scenarios: notification_click)"

# The native Ely agent chat in a real, isolated Sirio against the Python agent fixture, over the
# control socket (docs/testing/ely-agent-chat.md). The effort and effort_light scenarios (dark and
# light) prove a chat asks its agent for High at start and again after a relaunch, and that the
# effort list opens and takes a pick through the debug-only `surface.chat.effort`; they capture it.
# effort_full and effort_full_light run the fixture's effort-full mode (Default, five levels,
# Ultracode), capture the whole list (the notes, the separator) and choose Default from it.
# Five scenarios need a display (composer_keys, reduced_motion and unfocused_spinner below, and the two
# composer_band ones after them) and are skipped under --state-only (`SKIP <name>: needs a
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
# threads, resolved folds and outdated sections; same flags as above.
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
# header 32, a sidebar section header 31 and a two-line card 53 high, the
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

# The selection bezel-markdown paints (a chat response, the Markdown Preview):
# a selected document whose rows soft-wrap, break and wrap a code span, on a
# private Xvfb; fails when any row's wash starts after its first glyph, the
# defect the vendored bezel_markdown fixes (rust/vendor/README.md).
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

# The horizontal bar bezel-markdown draws under a fence or a table wider than
# its column (the vendored change of rust/vendor/README.md): a real xdotool
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
