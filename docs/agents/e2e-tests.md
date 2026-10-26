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
# every host's log and state; SIRIO_HOST_E2E_VERBOSE=1 prints each probe line.
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
Scripts/Tests/test-host-sessions-e2e.sh  # -> prints "HOST SESSIONS E2E OK (<n> cases, <s>s)"
# The sirioctl end to end, each world in its own HOME, XDG directories, SIRIO_DB and
# socket: the stable copy and its PATH link, replacement, a foreign file, the AppImage
# mount kept out of a pane, quit and relaunch, every stub agent's hooks, the skill,
# SIRIO_WORKTREE_ID and the flags sirioctl offers. Linux runs it with --xvfb or
# --display :N; --state-only skips cases 4, 4c, 5, 6 and 10 and is what macOS runs.
# --out-dir DIR keeps the transcript, logs and env dumps; --only N[,N...] prints
# "SIRIOCTL E2E PARTIAL"; --mutate claude-prompt-hook is case 6's mutation check.
Scripts/Tests/test-sirioctl-e2e.sh   # -> prints "SIRIOCTL E2E OK"

# The chat's session panel column in a real, isolated Sirio: a stub `claude`
# (Scripts/Tests/stubs/session-panel/claude over claude_fixture.py's session_panel
# mode) and the fake forge, asserted on `surface.chat.read`'s `sessionPanel`:
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
# web link still reaches the browser). Each scenario is a fresh Sirio; --state-only skips the
# captures; --display :N is a private Xvfb (at least --window-size, default 2200x1000)
# with Mesa's lavapipe, and trace_tab and trace_idle need one (--state-only with it paints
# but takes no captures), trace_keys and file_links a framed run at --window-size 2200x1000;
# --appearance light|dark; --only NAME[,NAME...] prints "CHAT SESSION PANEL E2E PARTIAL".
Scripts/Tests/test-chat-session-panel-e2e.sh --state-only   # -> prints "CHAT SESSION PANEL E2E OK (<n> scenarios, appearance <a>, state-only <0|1>)"

# The native Ely agent chat in a real, isolated Sirio against the Python agent fixture, over the
# control socket (docs/testing/ely-agent-chat.md). The effort and effort_light scenarios (dark and
# light) prove a chat asks its agent for High at start and again after a relaunch, and that the
# effort list opens and takes a pick through the debug-only `surface.chat.effort`; they capture it.
# effort_full and effort_full_light run the fixture's effort-full mode (Default, five levels,
# Ultracode), capture the whole list (the notes, the separator) and choose Default from it.
# Three scenarios need a display and are skipped under --state-only (`SKIP <name>: needs a
# display`, recorded as `skipped` in summary.json): composer_keys types through xdotool (Return,
# Shift+Return, Up and Down moving the caret, the slash popup taken with Down and Tab, closed by
# Escape, proved by a frame diff of its box against the frame before the `/`; its fixture is
# `composer`, because `plain` advertises no slash commands) and reduced_motion
# (SIRIO_REDUCED_MOTION=1: the transcript is pixel-identical in three frames 250 ms apart) and
# unfocused_spinner (a second window, `surfaces_probe --frame tooltip`, takes the X focus, and the
# sidebar row's status bloom, which only bezel's animation clock repaints, must still turn in three
# frames 250 ms apart; it logs `unfocused-spinner CPU <x>%` of one core over 10 s, and --cpu-ceiling
# PCT fails above PCT; --focus-thief-bin PATH names the second window's binary, by default the
# surfaces_probe example, built with the others unless --no-build; the CPU base and the ceiling
# formula `base × 1.25 + 1` are in docs/testing/bezel-off.md, "The unfocused spinner and its CPU
# base").
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

# Dragging a scrollbar thumb, for the three bars the app draws (file_view::scrollbar, the
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
```
