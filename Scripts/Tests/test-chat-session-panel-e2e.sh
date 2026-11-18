#!/bin/bash
set -euo pipefail

# The CLI the native chat starts is given CLAUDE_CODE_STARTUP_FAILURE_RESULTS=1 by Sirio itself. This unset is
# belt and braces: launch() starts Sirio with `env -i` and an allowlist, so the variable cannot reach the CLI
# from here anyway, and only Sirio's own launch can give it (that is what startup_opt_in proves).
unset CLAUDE_CODE_STARTUP_FAILURE_RESULTS

# End-to-end test of the chat's session panel (spec
# 2026-10-20-chat-session-panel-design.md §9): a real, isolated debug Sirio, its
# Claude chat talking to a stub `claude` (Scripts/Tests/stubs/session-panel/claude,
# which runs rust/crates/sirio_chat/tests/fixtures/claude_fixture.py in its
# session_panel mode) and its change request card reading the loopback fake forge
# (Scripts/Tests/fake_forge.py). Everything is asserted on the `sessionPanel` JSON
# of `surface.chat.read` -- the column's sections, rows, details and actions --
# and driven through the debug-only `surface chat panel` verb, so no pixel is read.
# Framed runs add PID-matched window captures to look at.
#
# Every card but Environment starts folded, and the change request card is read only
# while it is unfolded and a frame paints the column, so a run with no display
# (--state-only alone) has nothing that paints and skips the scenarios that need a
# frame (default_folds, change_request, rate_limit_shared, trace_tab, trace_idle);
# --state-only with --display :N paints on that display without taking captures and
# runs all of them.
# A scenario that works inside a card unfolds it first (unfold_cards).
#
# Scenarios (each starts a fresh Sirio with its own database, host and HOME):
#   environment        the project, the chat's directory (the worktree's root) and the branch, tagged worktree
#   default_folds      Environment open and the other cards folded, Context and Connectors with their
#                      summaries, the folded change request card drawn while the forge sees no request
#                      at all, and the card read once it is unfolded; then a chat on a host nothing names
#                      (no setting, no token), where opening the column runs neither `gh` nor `glab`
#                      (stubs that record every call)
#   context            asked once on open with no prompt yet, then after a prompt: the first
#                      reducible group, a plugin node unfolding to its agents, the project
#                      CLAUDE.md row opening a file tab
#   connectors         a failed server reconnecting to connected, a server that needs
#                      sign-in waiting for it (the opened URL is looked for in a browser stub)
#   connector_switch   (A6) every server row has a switch read only from mcp_status: switching codegraph off
#                      reads disabled with no action, on reads starting then failed again, a repeat sends
#                      nothing, Notion off loses its Sign in, and no mcp_authenticate is sent; exactly one
#                      mcp_toggle per switch whose requested state differed from the shown one
#   connector_switch_in_flight  (SIRIO_FIXTURE_TOGGLE_GATE) a row reads switching… while its toggle is open,
#                      a second flip fails and sends nothing, and the row reads disabled once it answers
#   connector_switch_waiting    a server waiting for its sign-in is switched off: the sign-in poll is dropped
#                      (exactly one mcp_status after the mcp_toggle line, the switch's own)
#   connector_switch_refused    (SIRIO_FIXTURE_TOGGLE=refused) a refused mcp_toggle hides every switch and
#                      leaves the rows and their notes as they were
#   connector_switch_missing    (SIRIO_FIXTURE_TOGGLE=missing) a failed toggle puts the CLI's sentence under
#                      the row, and the switch keeps the state mcp_status reported
#   connector_switch_unread     (SIRIO_FIXTURE_TOGGLE=unread) the toggle applies but the mcp_status after it fails: the row
#                      carries a note that the switch was saved and the new state could not be read
#   connector_switch_click      (framed, 2200x1000) a real X click at the switch's drawn bounds toggles that
#                      server and starts no sign-in; a capture of the card and one with the pointer on the switch
#   reload_plugins_and_skills  (A6) the Connectors card's `Plugins and skills` row, Reload: reload_plugins with the hold and
#                      refuse_if_busy, then reload_skills; the row is back to Reload and the plugin server joins the card
#   reload_floor       (SIRIO_FIXTURE_FLOOR=1) the hold without refuse_if_busy; an answer with no `held` is applied
#   reload_held        (SIRIO_FIXTURE_RELOAD=held) the cost warning (names shown, a bidi override as text) and Reload
#                      anyway; no skills until the second request, which has no hold
#   reload_held_then_busy  (SIRIO_FIXTURE_RESULT_GATE) a held reload, a turn, Reload anyway: the warning and the busy
#                      note, nothing sent; the turn's end leaves the warning alone
#   reload_busy_refusal  (SIRIO_FIXTURE_RELOAD=busy) the CLI's busy refusal is the busy note; no skills
#   reload_busy_cli    (SIRIO_FIXTURE_CLI_BUSY_GATE) a turn the CLI started is busy: the note, nothing sent; its end
#                      clears the note and a Reload goes out
#   reload_busy_turn   (SIRIO_FIXTURE_RESULT_GATE) a streaming turn is busy: the note, nothing sent; its end clears it
#   reload_busy_before_skills  (SIRIO_FIXTURE_RELOAD_GATE, SIRIO_FIXTURE_RESULT_GATE) a turn that begins while
#                      reload_plugins is open stops the skills: the busy note, no reload_skills
#   reload_in_flight   (SIRIO_FIXTURE_RELOAD_GATE) the row reads reloading… with no action; a second activate fails
#   reload_refused     (SIRIO_FIXTURE_RELOAD=refused) a refused reload_plugins hides the row; no skills
#   reload_skills_refused  (SIRIO_FIXTURE_SKILLS=refused) a refused reload_skills after an applied reload hides the row
#   reload_errors      (SIRIO_FIXTURE_RELOAD=errors) an applied reload with load errors says how many plugins failed
#   change_request     the branch's pull request as a card, its row opening the tab
#   rate_limit_shared  a forge host paused by the right panel's list is paused for the card
#                      too: the row says so and the forge sees no read of it during the pause
#   restore            the column is open, Sirio quits and starts again: it is still open
#
# The notice_* scenarios (A7) use Scripts/Tests/stubs/notices/claude, the fixture in its cli_notices mode: the
# prompt picks the turn (the stub's header, Scripts/Tests/stubs/notices/claude, lists them all).
# SIRIO_FIXTURE_RETRY_GATE holds `retry twice` after its retries until the file exists;
# SIRIO_FIXTURE_STARTUP makes the CLI fail to start (the start-up scenarios). They are asserted on `statusLine` of
# `surface.chat.read` (`""` or {source, text, tone, held[, key, priority]}) and on the transcript's rows
# (`info` and `info-warning` are the CLI's refusal and informational notices).
#   notice_retry       the status line reads `Retrying 2 of 10 in 2 s: overloaded` while the CLI holds the retry, and
#                      is empty with the reply drawn once the gate opens
#   notice_retry_exhausted  the exhaustion sentence is in exactly one row and the line is empty
#   notice_notification  the line outlives the turn: immediate over high over low, a same-key one replaced (held 3);
#                      the newest immediate first; a replaced entry's timer leaves its replacement alone; the
#                      replacement's own timer returns the line to the older one
#   notice_refusal     a refusal is an info row between the refused text and the re-asked answer, one that holds
#                      "\n\nDetails:" and a bidi override shows the override as text, and the informational
#                      levels are info, info, info-warning
#   notice_midstream   a reply cut by an error is not glued to the answer that follows
#   limit_notice       (SIRIO_FIXTURE_LIMITS, SIRIO_E2E_TZ=UTC) the rate-limit rows say which window, how full and when
#                      it resets: a warning once (not again, but again after an allowed), then the refusal, then the
#                      overage refusal with its scope; the CLI's own refusal text is still one assistant row
#   usage_text         (SIRIO_FIXTURE_USAGE) a local command's answer (`/usage`, `/context`) is one command-output row
#                      holding the CLI's text, never an assistant row, also when the command's `result` fails and
#                      repeats the text
#   usage_floor        (SIRIO_FIXTURE_USAGE) a CLI that writes no marker for `/usage` (the floor's) adds only the turn
#                      row, ends the turn and still answers the next prompt
#   usage_card         (SIRIO_FIXTURE_USAGE, SIRIO_E2E_TZ=UTC) a `/usage` that carries the CLI's `usage_report` is drawn as
#                      a card: the read row's `card` lists the totals, the models, one heading per group with a line per
#                      row, and the extra usage; a hostile scope name and kind show as text; `headline` is the first
#                      active row's name
#   usage_unavailable  (SIRIO_FIXTURE_USAGE) a malformed report (`rate_limits` null or a number, `limits` empty or a
#                      string) reads as unavailable or none reported, a row with no `percent` shows no `%`, and a
#                      row's missing parts leave no stray `·`, an empty row no line and a group of such rows no heading
#   startup_*          (SIRIO_FIXTURE_STARTUP) the chat's error row is the CLI's own start-up text, with no ACP in it:
#     startup_config_invalid  the sentence, the file and its backup, and the override shown as text
#     startup_opt_in     a reason the CLI reports only when Sirio's launch sets CLAUDE_CODE_STARTUP_FAILURE_RESULTS
#                        (the script unsets it, so this proves the launch does)
#     startup_unknown_reason  `Claude Code could not start (some_future_reason).`
#     startup_no_result  no result before the exit: the exit code and the stderr tail
#
# The trace scenarios (spec 2026-10-21-chat-trace-waterfall-design.md §8) use another stub,
# Scripts/Tests/stubs/trace/claude, the same fixture in its trace mode: the first word of
# a prompt picks a scripted turn (subagents, second, request, cancel, hang, die, subperm,
# subperm_early, background, bgprompt) and TRACE_STEP_MS (400 by default) paces its lines, so a live state can be
# read mid-turn. They are asserted on the card's rows in `sessionPanel`, on
# `surface.trace.read` (the Trace tab) and on `firstVisibleEntry` and `lastVisibleEntry` of
# `surface.chat.read`.
#   trace_live         a turn with two parallel agents (one nesting a third), an Edit that asks and
#                      a Bash that fails and then works: the lanes, the waiting tag and its row, the
#                      reveal of the permission (the list's top at it unpainted, the card between the
#                      first and last row drawn painted), the failed and done calls, Done with $0.31
#   trace_cost         a second turn's cost is the difference of the process's totals: $0.12
#   trace_request      the CLI's `status: requesting` opens the main-thread model call, so the wait for
#                      the first token is part of one running call (the Trace tab shows it while the
#                      fixture waits), and the turn ends with that same call, done
#   trace_interrupt    Stop ends the turn Interrupted, its cost and the next turn's are unknown
#   trace_death        the CLI dying mid-call ends the turn Failed, every lane failed or interrupted
#   trace_hang         a call that never reports ends interrupted with the turn
#   trace_subagent_permission  a subagent's request is a waiting lane and one Permission card, never a
#                      tool row left pending, whether it comes after its call's line or before
#   trace_background   two background agents (`is_backgrounded`) outlive the turn's `result`: the turn stays
#                      Live with the agent lanes running and growing, its cost `≥ $1.00` (never an exact
#                      figure while an agent runs) through the CLI-started turn after the first agent
#                      ($0.20) and Done `≥ $1.20` once the second ends, then $1.30 after the CLI's own
#                      reaction ($0.10); no CLI-started call is drawn under the root, both agents end done
#   trace_background_prompt  a prompt written while a background agent runs: its turn is traced (a
#                      root model call, no agent lane, an unknown cost: the agent's spend is in its
#                      result) and the agent's lines and end stay out of it
#   trace_reveal       a lane reveals its Agent row, the root the user's message
#   trace_tab          the Trace tab: its tree, one per chat, kept across a worktree switch, never
#                      restored, closed with its chat
#   trace_idle         SIRIO_PERF_TRACE counts the frames the trace asks for (trace.frame): some while
#                      a turn runs and the card or the tab is drawn, none with it folded, with the tab
#                      or a third tab selected in its place, with the worktree parked or after the turn
#   trace_keys         real X input on the Trace tab: a click on a row and Enter on the selected row
#                      reveal its Agent row, a click on the Follow checkbox toggles it
#   unread_parked_chat a turn that ends while its worktree is not selected marks the chat unread and posts
#                      one notification naming its branch; showing the worktree clears the mark
#                      and the tray's roster lists the worktree with a leading dot until it is shown, in every
#                      mode, a no-display --state-only run included (with --display, also what the sidebar
#                      holds in the read's `rows` and `projects`, a click that folds the project, and captures
#                      of the row, the Sessions card and the folded project's badge)
#   unread_question    a permission the turn asks while the worktree is not selected reads needs-input and
#                      unread, and is still open when the worktree is shown
#   unread_secondary_seen  a chat in the shown Secondary half, beside a terminal in the Primary, stops
#                      with no mark and no notification
#   unread_restored_chat_quiet  a relaunch gives a seen chat no mark and no notification, and keeps the
#                      mark of a chat whose turn ended unseen before the quit
# The notification_click scenario is opt-in (never in the default set; name it with --only) and
# needs `dbus-daemon` and python3-gi (Gio 2.0): it starts a private session bus and, on it,
# Scripts/Tests/fake_notifications.py, a recording org.freedesktop.Notifications server, and the
# only launch of the whole script that is handed a bus address:
#   notification_click a turn that ends in a parked worktree posts one Notify over D-Bus (app name
#                      Sirio, a body that names feat/login, a `default` action); the recorder's
#                      ActionInvoked for it, the signal a desktop sends when the notification is
#                      clicked, selects that worktree. notifications.jsonl under the out dir is the
#                      recorder's log
# The file_links scenario (spec 2026-10-24-chat-file-links-design.md §6) uses a third stub,
# Scripts/Tests/stubs/file-links/claude, the fixture in its file_links mode: every prompt is
# answered with one text holding paths that exist in the worktree, ones that do not, a
# version, a fenced path and a web link. It is asserted on `transcriptLinks` of
# `surface.chat.read` (the links of the last paint, with the pixel of each), on
# `surface.editor.state`, on `surface tabs read` and on the browser stub's log.
#   file_links         real X clicks and drags on a chat response: its file paths and the
#                      web link are the links painted, a click opens the file (a Markdown
#                      one in Preview, or in the source at its line when the path names
#                      one), a second click reuses the tab, a Super+Shift click still goes
#                      to the editor, a drag selects and opens nothing, and the web link
#                      still reaches the browser stub
#   clipboard_keys     the clipboard chords with real X keys against the display's real
#                      CLIPBOARD (read and owned through python3-gi's GTK, as another
#                      application holds it): in the composer Ctrl+A then Ctrl+C copies,
#                      Ctrl+A then Ctrl+X cuts, Ctrl+V pastes another application's text;
#                      a drag across the response then Ctrl+C copies what it selected,
#                      and a plain click on the response leaves typing in the composer
# The composer_compact scenario (spec 2026-10-30-composer-band) uses a fourth stub,
# Scripts/Tests/stubs/session-panel-bypass/claude, the fixture in its session_panel_bypass mode
# (the CLI starts in bypassPermissions):
#   composer_compact   the native Claude chat at the default window (about 430 px): the compact
#                      composer row (Fast, Thinking and the session panel in the overflow menu),
#                      the mode pill the danger shield-off, the band spanning the chat, send inside
#                      it, the chips ending before send and level with it; read on `composerRow`
#                      of `surface.chat.read`, which exists only once a frame paints (--display :N).
#                      Then a real click on the overflow button (its measured `overflowButton`)
#                      opens the menu: `menu`, the rows the render built, lists Fast disabled with
#                      the CLI's reason as its tooltip and Thinking enabled; a real Enter chooses
#                      the first enabled row, Thinking, and `picker` is `thinking`; Escape closes it.
#                      The stub's handshake names `preference` as the reason Fast is refused
#
# The session_state scenarios (spec 2026-11-15-claude-protocol-parity-design.md, A2) use a stub of their
# own, Scripts/Tests/stubs/session-state/claude, the fixture in its session_state mode: the CLI's
# `session_state_changed` lines (SIRIO_FIXTURE_GATE and `.2` are the files the first scenario touches):
#   session_state_background  the CLI's `running` outlives the turn's `result` while a background agent
#                      works: status stays running past the turn and past the CLI's own follow-up turn
#                      (the Trace card reads Live on the CLI's state alone), and only the CLI's `idle`
#                      brings done and a Done card
#   session_state_ask  the CLI's `requires_action` before a permission request: `sessionState` says so,
#                      the status is needs-input and the card reads Waiting for you; allowing it ends in
#                      `idle`, done and a Done card
#   session_state_unprompted  turns the CLI starts on its own (a Monitor's events) after `idle` do not
#                      flip the status: three `running` cycles of its own never read running, and
#                      the chat, parked off screen, is not marked unread by them
# The background_* scenarios (A3) use Scripts/Tests/stubs/background-tasks/claude, the fixture in its
# background_tasks mode (`shell` and `agent` prompts start a background task; SIRIO_FIXTURE_FLOOR=1 makes
# the CLI the 2.1.257 floor, which refuses get_task_output; SIRIO_FIXTURE_REQUEST_LOG is the file the
# fixture appends every control request to). They read `backgroundTasks` and `taskOutputRetired` of
# `surface.chat.read` and drive the debug-only `surface.chat.paintTail` (test-only: it says a frame
# painted a task's tail, which is all the tail poll waits for) and `surface.chat.stopTask`:
#   background_shell   a backgrounded shell is a live `shell` entry; while its tail is painted the tail
#                      reaches `line 3` (the ANSI stripped); Stop reads stopping, then outcome killed,
#                      then not live, the tail ends `[killed]`, and a second press sends nothing: the
#                      fixture's request log holds exactly one stop_task
#   background_shell_unpainted  with no frame painting its tail the fixture's log holds no get_task_output
#   background_shell_floor      on the floor the first painted poll is refused (exactly one
#                      get_task_output ever, taskOutputRetired, no tail) and Stop still ends the task
#   background_agent_stop  a background agent is a live `agent` entry that is never read; Stop ends it
#                      killed and not live
#   background_ended_dropped  an ended entry is dropped by the next prompt
# The next scenarios (A3, the surfaces) never call paintTail: a real frame drives the poll, so they run
# painted (--state-only --display :N paints on a private display with no captures), except
# background_stop_click, which sends a real X click at the Stop button's drawn position and so needs a framed
# run at 2200x1000. The others press Stop with the panel verb `activate trace/stop/<task id>`, which the card's
# Stop buttons call too (the popover's and the tab's call `stop_background_task` directly; the GPUI tests click
# every button). The fixture's request log counts what the CLI was asked:
#   background_tail_card  with the Trace card folded a live shell is never read (3 s); unfolded, the
#                      row `trace/background/bshell001` gets `line 2` or later as its note; folded again,
#                      the request log stops growing
#   background_tail_tab   the same with the Trace tab opened (`tab()["background"]` carries the tail) and
#                      then closed, the card folded throughout
#   background_stop_click a real click on the card row's Stop button (`stopButtons` of the column's read
#                      gives where it drew it): the row reads stopping, then interrupted, a second click
#                      sends nothing and the log holds one stop_task
#   background_lane_stop  a background agent has no row in the group (its lane carries Stop): the lane row
#                      has `stop`, pressing it ends the agent, and it is never read
# The rename_* scenarios (A4, spec 2026-11-15-claude-protocol-parity-design.md) use
# Scripts/Tests/stubs/rename/claude, the fixture's normal mode answering `rename_session`
# (SIRIO_FIXTURE_TITLE_STORE is the CLI's own title file, SIRIO_FIXTURE_RENAME_ERROR answers every
# rename with that error, SIRIO_FIXTURE_HANDSHAKE_GATE holds a session's `initialize` answer until the file
# exists, SIRIO_FIXTURE_RENAME_GATE holds each rename's echo and answer until the file exists; a `-p` call is the summarizer and prints `Summarized title`). A manual rename is the debug-only
# `surface tabs rename N --title T`; `agentTitle` of `surface.chat.read` is the chat's title state. The
# requests the CLI heard are read from the fixture's control log (parsed as JSON, it escapes non-ASCII),
# counted from a line-count marker taken at each scenario's start. What each must catch, written before the code:
# the verb misses the chat; an auto title is sent; a held title is lost at connect; a refusal still sends; an
# error is silent; restore sends, or adopts the CLI's title; non-ASCII is mangled; a stale answer moves the state.
#   rename_session            before any turn a rename of "Café ☕ notes" gives the tab that title and sends one
#                      rename_session (source host, session_id the chat's own --session-id) that is accepted; after
#                      a turn "Release notes" sends a second, whose session_id is fixture-session-1
#   rename_session_failed     an error that is not a refusal: the tab keeps the title, the state is failed with the
#                      error and the transcript ends with the warning; then two renames sent while the CLI holds its
#                      answers are both heard, and only the last one's answer counts: one more warning, not two
#   rename_session_retired    a refusal ("Unknown control request subtype") retires the verb quietly: a second
#                      rename sends nothing, shows nothing, and the tab holds the second title (that a new connection
#                      sends the title again and clears the retirement is a GPUI test of chat/mod.rs: no state-only
#                      verb starts a new connection)
#   rename_session_connecting a rename while the handshake is held waits (pending, no request) and is sent, once
#                      and accepted, when the connection becomes ready
#   rename_session_restored   a restored chat sends nothing and adopts nothing: the CLI's title for the session
#                      differs, the tab keeps Sirio's, agentTitle is empty and no request is heard
#   rename_auto_title_quiet   the summarizer's title reaches the tab and never the CLI
# The permission_* scenarios (A5) use Scripts/Tests/stubs/permissions/claude, the fixture's `permission` mode with
# the extra can_use_tool fields a scenario writes to SIRIO_FIXTURE_PERMISSION_FIELDS. They read `questionDock`
# of `surface.chat.read` (the dock as painted) and are state-only. What each must catch, written before the code:
# a field the card drops; a digit that answers a card the CLI would rather refuse; Enter or a start row that lands
# on Allow; hidden characters hidden from the user (or an ANSI code left in a reason); a plain ask that changed.
#   permission_detail  title, description, reason, blocked path and MCP server reach the card in order, Always allow
#                      stays offered, digits stay on, row 1 is selected; allowing runs the tool
#   permission_default_no  default_to_no with suppress_always_allow_rule: two options, the start row is Reject,
#                      digits are off, the caption is the plain one; the control verb's row 1 (Allow) runs the tool
#   permission_default_no_reject  the same card, row 2 (Reject) denies it
#   permission_visible_text  a command with an OSC, a BEL and a bidi override shows each as <U+XXXX>, and an ANSI
#                      reason is stripped (only the reason)
#   permission_plain   no extra fields: no details, the plain caption, digits on, row 1 selected
# The permission_rules* scenarios read the Permissions card of the session panel. Each has its own rules file
# ($RUN_DIR/<name>-rules.json) and request log ($RUN_DIR/<name>-rules.jsonl, one line per list_permission_rules
# the fixture heard). What each must catch, written before the code: a card that asks with the panel closed; a
# refusal that is asked again or shown; a burst of triggers that sends more than one follow-up; Always allow that
# leaves the card stale until the turn ends; a success with no state shown as an empty list.
#   permission_rules  every group, row key, label, detail, tag and note, the section note, a bidi rule and a
#                      bidi source shown as <U+202E>, an unknown behavior skipped and not counted, and the
#                      last good list kept after an unreadable answer
#   permission_rules_refused   the floor refuses: no card, one request, and still one after a turn
#   permission_rules_closed_panel   no request with the panel closed through a turn; one when it opens, before unfolding
#   permission_rules_coalesced   Always allow and a turn's end during a held answer make one follow-up: two requests in all
#   permission_rules_after_always_allow   Always allow refreshes the card while the turn's result is still held
#   permission_rules_unreadable   a success with no rules is a note on the card, with no rows
# The hooks_* scenarios (A8) run on the session-panel stub with SIRIO_FIXTURE_HOOKS (the JSON file get_hooks_listing
# answers from, re-read on every request; unset: refused), SIRIO_FIXTURE_HOOKS_GATE (holds every answer until the
# file exists) and SIRIO_FIXTURE_HANDSHAKE_GATE (holds the handshake, for hooks_refused). Requests are counted in
# the control log after a mark:
#   hooks_card         every group, row key, label, detail, tag and the unfolded bodies (a bidi command and a
#                      multi-line command shown as text, the command last), no `editable` anywhere in the read,
#                      and the last good listing kept after an unreadable answer
#   hooks_policy       the section note's lines in order, the per-row disabled tags, and the one-line organization note
#   hooks_refused      the panel opens during a held handshake, so only the connection-ready trigger asks; the floor
#                      refuses: no section, one request, and still one after a turn
#   hooks_closed_panel no request with the panel closed through a turn; one when it opens
#   hooks_coalesced    triggers during a held answer make one follow-up: two requests in all
#   hooks_refresh      a turn's end and a reload that applied plugins each ask again; nothing polls
#   hooks_unreadable   a success with no hooks is a note on the card, with no rows
# The elicitation scenarios use Scripts/Tests/stubs/elicitation/claude, the fixture's elicitation mode.
# The composer_keys scenario (this script's own; test-ely-chat-ui-e2e.py has one of the same name)
# uses the default stub, whose fixture logs every prompt it hears (SIRIO_FIXTURE_USER_LOG) and every
# control request (SIRIO_FIXTURE_CONTROL_LOG), and presses real X keys in the composer:
#   composer_keys      Up and Down recall the chat's own prompts: after two prompts, the second with an
#                      `@` mention picked from the popup with Tab, Up shows it with its `@path` text, a
#                      second Up the first, Down twice goes back to empty; sending the recalled prompt
#                      again goes out as a mention (the fixture's log holds the link, not the literal
#                      `@path`); after a relaunch Up still recalls that prompt with its mention. Then
#                      Shift+Tab cycles the permission modes through a full lap and a bit more: the
#                      fixture's log of set_permission_mode requests is the menu's order with Bypass left
#                      out, no danger shield is drawn and no dialog holds the keyboard
# The composer_resize scenario (this script's own) uses the default stub and reads `composerRow` of
# `surface.chat.read`, which exists only once a frame paints:
#   composer_resize    The composer's field follows the chat's height. At 2200x1000 the cap is at least
#                      12 rows, the drawn maximum is the stored 12 and the empty field shows 3 rows; a
#                      40-line draft fills it to 12. With the window resized to 2200x400 the chat is
#                      under 554 px tall, so the cap (half the chat minus the band's fixed 61 px allowance, in 18 px
#                      rows: 4 at 294 px) is below 12 and the drawn maximum and the draft's rows follow it
#                      while the stored maximum stays 12 and the band stays within half the chat;
#                      at 2200x1000 again the field is back to 12. Then the edge on the composer's top is
#                      dragged with real X events: 4 rows up raises the maximum to 16 (released where
#                      the dragged edge now is); pulled to the transcript it stops at the cap (21 at 894 px; released
#                      over the transcript) with the band within half the chat; with the 40-line draft still in, a drag pulled down past
#                      the window's bottom edge and released there (outside the window) ends the drag and
#                      the rows bottom out at 3, saved; an emptied field is 3 rows; a drag held 5 rows up
#                      pins the field at 8 and the release frees it with the maximum kept at 8; a press
#                      that never moved changes nothing, and one held 8 px up pins the field at 3 rows
#                      and, released, saves nothing. A real drag of the sidebar's divider follows (its
#                      debounced save writes the whole settings row; the sidebar's new width is checked
#                      after the relaunch, so the save did run). Last, Sirio is stopped and started
#                      again under the same name (the same database): the chat it restores (the same
#                      tab, read before any tab is opened) and a second chat opened beside it both read
#                      a maximum, stored and drawn, of the 8 the drag left, and a 40-line draft in each
#                      fills the field to 8 rows, which proves the maximum reaches the field.
#                      Frames: rest, full, short, taller, outside, held, relaunched
# trace_tab and trace_idle need a painted column (--display :N): the tab closes when its chat is
# released (observe_release, then TraceTabEvent::Close), which in a window comes after the paint
# that replaces the chat's last frame, and a frame that is never painted asks for nothing
# whatever the code does.
# trace_keys, file_links, clipboard_keys, composer_keys, composer_resize and background_stop_click need a framed run at --window-size 2200x1000
# (no --state-only): they click at pixels or send real keys.
#
# Isolation: every Sirio starts under `env -i` with an allowlist (see launch()), the
# stub agent CLIs first on PATH, a private HOME, XDG directories, database, socket,
# credentials and host root. The run records the hash of the user's
# ~/.config/mise/config.toml before and after and fails if it changed (an
# unisolated `claude --version` goes through the user's mise wrapper). A display is
# only ever one the user started for this: --display :N, never $DISPLAY.
#
# The artifact: --out-dir DIR (default artifacts/chat-session-panel-e2e-<stamp>-<pid>)
# keeps transcript.log (every call and every read of the column), the app logs, the
# fake forge's request log, stub-calls.log, the performance traces of trace_idle
# (trace_idle-*.tsv) and -- unless --state-only -- PID-matched window captures in
# frames/. Rerunning the script reproduces it. The run's own directory is made under
# $TMPDIR (a unix socket path must stay under 108 bytes, so point TMPDIR at a short
# directory if /tmp is not wanted).
#
# Usage: Scripts/Tests/test-chat-session-panel-e2e.sh [--state-only] [--out-dir DIR]
#          [--display :N] [--appearance light|dark] [--window-size WxH] [--only NAME[,NAME...]]
#   --window-size WxH (framed runs only; default none) resizes the app's window. Without
#     it the window keeps Sirio's own default size, which leaves the chat about 430 px
#     wide, so the column is laid over the transcript (the snapshot says `overlay`);
#     a window of 2200x1000 leaves the chat wide enough for the column to dock.
#   --only runs some scenarios and prints CHAT SESSION PANEL E2E PARTIAL instead; the
#     scenarios that read the forge are refused without a display.
#   A framed run needs a private display, for example:
#     Xvfb :97 -screen 0 2400x1200x24 &
#   and Mesa's lavapipe (software Vulkan) for the window to draw on it.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
ALL_SCENARIOS=(environment default_folds context connectors connector_switch connector_switch_in_flight
  connector_switch_waiting connector_switch_refused connector_switch_missing connector_switch_unread connector_switch_click
  reload_plugins_and_skills reload_floor reload_held reload_held_then_busy reload_busy_refusal reload_busy_cli
  reload_busy_turn reload_busy_before_skills reload_in_flight reload_refused reload_skills_refused reload_errors
  change_request rate_limit_shared restore
  trace_live trace_cost trace_request trace_interrupt trace_death trace_hang trace_subagent_permission
  trace_background trace_background_prompt trace_reveal
  trace_tab trace_idle trace_keys file_links clipboard_keys composer_compact composer_keys composer_resize
  unread_parked_chat unread_question unread_secondary_seen unread_restored_chat_quiet
  elicitation_form elicitation_url elicitation_withdrawn unhandled_request
  session_state_background session_state_ask session_state_unprompted
  background_shell background_shell_unpainted background_shell_floor background_agent_stop
  background_ended_dropped background_tail_card background_tail_tab background_stop_click
  background_lane_stop
  rename_session rename_session_failed rename_session_retired rename_session_connecting
  rename_session_restored rename_auto_title_quiet
  permission_detail permission_default_no permission_default_no_reject permission_visible_text permission_plain
  permission_rules permission_rules_refused permission_rules_closed_panel permission_rules_coalesced
  permission_rules_after_always_allow permission_rules_unreadable
  hooks_card hooks_policy hooks_refused hooks_closed_panel hooks_coalesced hooks_refresh hooks_unreadable
  notice_retry notice_retry_exhausted notice_notification notice_refusal notice_midstream limit_notice usage_text usage_floor usage_card
  usage_unavailable
  startup_config_invalid startup_opt_in startup_unknown_reason startup_no_result)
# Scenarios that need more of the machine than the default set does, so only --only runs them.
OPT_IN_SCENARIOS=(notification_click)
STATE_ONLY=0
OUT_DIR=""
DISPLAY_TARGET=""
APPEARANCE=""
WINDOW_SIZE=""
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    --window-size) WINDOW_SIZE="$2"; shift 2 ;;
    --only) ONLY="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

refuse() { echo "refused: $*" >&2; exit 2; }
case "$APPEARANCE" in ""|light|dark) ;; *) refuse "--appearance is light or dark, not $APPEARANCE" ;; esac
[ -z "$WINDOW_SIZE" ] || [[ "$WINDOW_SIZE" =~ ^[0-9]+x[0-9]+$ ]] || refuse "--window-size is WxH, not $WINDOW_SIZE"
# The scenarios that read the forge through the change request card, which reads only
# while a frame paints the column, composer_compact (the composer's layout and the measured
# band exist only once a frame paints the chat), and the two that need a frame for what they assert:
# a Trace tab closes when its chat is released, which in a window comes after the paint
# that replaces the chat's last frame, and a trace that asks for no frame proves nothing
# when nothing is ever painted.
PAINTED_SCENARIOS=" default_folds change_request rate_limit_shared trace_tab trace_idle trace_keys file_links clipboard_keys composer_compact composer_keys composer_resize background_tail_card background_tail_tab background_stop_click background_lane_stop connector_switch_click "
# trace_keys, file_links and clipboard_keys send real X clicks, keys and drags at pixels, so they need a
# framed run (no --state-only) at the 2200x1000 window size.
FRAMED_SCENARIOS=" trace_keys file_links clipboard_keys composer_keys composer_resize background_stop_click connector_switch_click "
framed_run() { [ "$STATE_ONLY" -eq 0 ] && [ "$WINDOW_SIZE" = 2200x1000 ]; }
SCENARIOS=()
SKIPPED=()
for name in "${ALL_SCENARIOS[@]}"; do
  if [ -z "$DISPLAY_TARGET" ] && [[ "$PAINTED_SCENARIOS" == *" $name "* ]]; then
    SKIPPED+=("$name")
  elif [[ "$FRAMED_SCENARIOS" == *" $name "* ]] && ! framed_run; then
    SKIPPED+=("$name")
  else
    SCENARIOS+=("$name")
  fi
done
if [ -n "$ONLY" ]; then
  IFS=',' read -r -a SCENARIOS <<<"$ONLY"
  for name in "${SCENARIOS[@]}"; do
    case " ${ALL_SCENARIOS[*]} ${OPT_IN_SCENARIOS[*]} " in *" $name "*) ;; *) refuse "unknown scenario $name (${ALL_SCENARIOS[*]} ${OPT_IN_SCENARIOS[*]})" ;; esac
    [ -n "$DISPLAY_TARGET" ] || [[ "$PAINTED_SCENARIOS" != *" $name "* ]] || refuse "scenario $name needs a painted column: pass --display :N"
    [[ "$FRAMED_SCENARIOS" != *" $name "* ]] || framed_run || refuse "scenario $name sends real clicks at the 2200x1000 layout: pass --display :N --window-size 2200x1000 and no --state-only"
  done
fi
# The display number of a local X display: ":97", ":0097" and ":97.0" are all 97.
display_number() { # DISPLAY-string
  [[ "$1" =~ ^([A-Za-z0-9._-]*):([0-9]+)(\.[0-9]+)?$ ]] || return 0
  [ -z "${BASH_REMATCH[1]}" ] || [ "${BASH_REMATCH[1]}" = localhost ] || return 0
  echo $((10#${BASH_REMATCH[2]}))
}
if [ "$STATE_ONLY" -eq 0 ]; then
  for tool in import identify xwininfo xprop xdotool timeout; do
    command -v "$tool" >/dev/null || refuse "$tool is required for captures; pass --state-only to skip them"
  done
  [ -n "$DISPLAY_TARGET" ] || refuse "no --display: the captures need a display of their own; pass --display :N (a private Xvfb), or --state-only"
fi
if [ -n "$DISPLAY_TARGET" ]; then
  target_number=$(display_number "$DISPLAY_TARGET")
  [ -n "$target_number" ] || refuse "--display takes a local X display such as :97, not '$DISPLAY_TARGET'"
  ambient_number=$(display_number "${DISPLAY:-}")
  [ "$target_number" != "$ambient_number" ] || refuse "--display $DISPLAY_TARGET is this session's own \$DISPLAY: start a private display (Xvfb :$target_number -screen 0 1600x1000x24) and pass it"
  [ "$DISPLAY_TARGET" != "${WAYLAND_DISPLAY:-}" ] || refuse "--display $DISPLAY_TARGET is this session's \$WAYLAND_DISPLAY"
  ps -u "$(id -u)" -o args= | python3 -c 'import re, sys; n = sys.argv[1]; sys.exit(0 if any(re.search(r"(^|/)Xvfb :" + n + r"( |$)", line) for line in sys.stdin) else 1)' "$target_number" \
    || refuse "no Xvfb :$target_number is running as $(id -un): start a private one (Xvfb :$target_number -screen 0 1600x1000x24 &)"
  DISPLAY_TARGET=":$target_number"
  if [ "$STATE_ONLY" -eq 0 ] || [ -n "$WINDOW_SIZE" ]; then
    command -v xdotool >/dev/null || refuse "xdotool is required to resize the window"
    geometry=$(DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool getdisplaygeometry 2>/dev/null || true)
    [ -n "$geometry" ] || refuse "no X server answers on $DISPLAY_TARGET"
    [ -z "$WINDOW_SIZE" ] || { [ "${geometry% *}" -ge "${WINDOW_SIZE%x*}" ] && [ "${geometry#* }" -ge "${WINDOW_SIZE#*x}" ]; } \
      || refuse "the display is ${geometry% *}x${geometry#* }, smaller than --window-size $WINDOW_SIZE: start a larger Xvfb (Xvfb $DISPLAY_TARGET -screen 0 2400x1200x24 &)"
  fi
fi
# clipboard_keys reads and owns the display's CLIPBOARD through GTK 3.
if [[ " ${SCENARIOS[*]} " == *" clipboard_keys "* ]]; then
  env -i PATH=/usr/local/bin:/usr/bin:/bin python3 -I -c 'import gi; gi.require_version("Gtk", "3.0"); gi.require_version("Gdk", "3.0"); from gi.repository import Gtk' 2>/dev/null \
    || refuse "clipboard_keys needs python3-gi with GTK 3 (python-gobject and gtk3) to read and own the clipboard; drop it with --only"
fi
# notification_click starts a private session bus and a D-Bus server written with Gio.
if [[ " ${SCENARIOS[*]} " == *" notification_click "* ]]; then
  command -v dbus-daemon >/dev/null \
    || refuse "notification_click needs dbus-daemon for its private session bus; drop it with --only"
  python3 -c 'import gi; gi.require_version("Gio", "2.0"); from gi.repository import Gio, GLib' 2>/dev/null \
    || refuse "notification_click needs python3-gi (python-gobject) with Gio 2.0 for the recording notification server; drop it with --only"
fi

[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/chat-session-panel-e2e-$(date +%Y%m%d-%H%M%S)-$$"
if [ -d "$OUT_DIR" ] && [ -n "$(ls -A "$OUT_DIR")" ]; then
  echo "refusing to reuse $OUT_DIR: it already has content (pass a new --out-dir)" >&2
  exit 2
fi
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux"
command -v python3 >/dev/null || fail "python3 is required"
command -v curl >/dev/null || fail "curl is required"
echo "display: ${DISPLAY_TARGET:-none (--state-only)}; scenarios: ${SCENARIOS[*]}; appearance: ${APPEARANCE:-default}"
for name in "${SKIPPED[@]:-}"; do
  [ -z "$name" ] || [ -n "$ONLY" ] || echo "SKIP: scenario $name needs a framed run (a painted column; trace_keys, file_links and clipboard_keys also the 2200x1000 window and real X input)"
done

# ---- the user's mise config must not move ---------------------------------
MISE_CONFIG="$HOME/.config/mise/config.toml"
mise_hash() { sha256sum "$MISE_CONFIG" 2>/dev/null | cut -d' ' -f1 || true; }
MISE_BEFORE=$(mise_hash)
check_mise() { # when
  [ "$(mise_hash)" = "$MISE_BEFORE" ] || fail "$MISE_CONFIG changed $1 (was ${MISE_BEFORE:-absent}, is $(mise_hash))"
}
echo "mise config: ${MISE_BEFORE:-absent}"

# ---- the run's own directory and what must be stopped ---------------------
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-csp-XXXXXX")
APP_PID=""
FORGE_PID=""
# notification_click's private session bus (empty everywhere else: see launch()).
APP_SESSION_BUS=""
stop_hosts() {
  local state pid tries
  for state in "$RUN_DIR"/*-host/host-v*.json; do
    [ -f "$state" ] || continue
    pid=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("pid", ""))' "$state" 2>/dev/null) || continue
    [ -n "$pid" ] || continue
    case "$(cat "/proc/$pid/comm" 2>/dev/null || true)" in
      *sirio-host*) ;;
      *) continue ;;
    esac
    kill "$pid" 2>/dev/null || true
    tries=0
    while kill -0 "$pid" 2>/dev/null && [ "$tries" -lt 50 ]; do tries=$((tries + 1)); sleep 0.1; done
    kill -KILL "$pid" 2>/dev/null || true
  done
}
# Every process this run started carries SIRIO_CSP_E2E_RUN=<RUN_DIR>: the app, the
# stub claude, the host. Those still alive at the end are stopped, TERM then KILL.
stop_run_processes() {
  python3 - "$RUN_DIR" <<'PY'
import os, signal, sys, time
marker = b"SIRIO_CSP_E2E_RUN=" + sys.argv[1].encode() + b"\0"

def matches():
    found = []
    for entry in os.listdir("/proc"):
        if not entry.isdigit() or int(entry) == os.getpid():
            continue
        try:
            environ = open(f"/proc/{entry}/environ", "rb").read()
        except OSError:
            continue
        if marker in environ:
            found.append(int(entry))
    return found

def send(pids, sig):
    for pid in pids:
        try:
            os.kill(pid, sig)
        except OSError:
            pass

pids = matches()
send(pids, signal.SIGTERM)
deadline = time.time() + 3
while pids and time.time() < deadline:
    time.sleep(0.2)
    pids = matches()
send(pids, signal.SIGKILL)
PY
}
cleanup() {
  [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; }
  [ -z "$FORGE_PID" ] || kill "$FORGE_PID" 2>/dev/null || true
  stop_hosts
  stop_run_processes
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN_DIR"/*.tsv "$OUT_DIR/" 2>/dev/null || true
  cp "$RUN_DIR"/*.jsonl "$OUT_DIR/" 2>/dev/null || true
  rm -rf --one-file-system "$RUN_DIR"
}
trap cleanup EXIT

# ---- build -----------------------------------------------------------------
echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl) 2>>"$RUN_DIR/build.log" \
  || { tail -n 40 "$RUN_DIR/build.log" >&2; fail "the build failed"; }

# ---- the fake forge --------------------------------------------------------
FORGE_PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
python3 "$ROOT/Scripts/Tests/fake_forge.py" --flavor github --port "$FORGE_PORT" --log "$RUN_DIR/github-requests.log" &
FORGE_PID=$!
for _ in $(seq 1 50); do
  curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$FORGE_PORT/api/v3/meta" && break
  sleep 0.2
done
forge_post() { curl -s -o /dev/null -X POST "http://127.0.0.1:$FORGE_PORT$1"; }
# How many reads of the branch's change request the forge has seen.
card_reads() { grep -cE " (ChangeRequestForBranch|ChangeRequestByNumber) " "$RUN_DIR/github-requests.log" || true; }

# ---- the fixture repository ------------------------------------------------
# A primary checkout on main and a linked worktree on feat/login, whose origin is
# the host the fake forge answers for. The chat runs in the worktree's src/.
PRIMARY="$RUN_DIR/widgets"
LINKED="$RUN_DIR/login"
mkdir -p "$PRIMARY"
git -C "$PRIMARY" init -q -b main
git -C "$PRIMARY" config user.email t@example.com
git -C "$PRIMARY" config user.name Tester
git -C "$PRIMARY" commit -q --allow-empty -m first
git -C "$PRIMARY" remote add origin https://ghe.test/acme/widgets.git
git -C "$PRIMARY" worktree add -q -b feat/login "$LINKED"
mkdir -p "$LINKED/src"
printf '# Widgets\n\nThe project memory the stub claude reports.\n' > "$LINKED/CLAUDE.md"
printf 'fn main() {}\n' > "$LINKED/src/main.rs"
CHAT_CWD="$LINKED/src"
# A repository whose remote is on a host nothing names: no setting, no token. default_folds
# opens a chat on it with recording `gh` and `glab` (below).
STRANGER="$RUN_DIR/stranger"
mkdir -p "$STRANGER"
git -C "$STRANGER" init -q -b main
git -C "$STRANGER" config user.email t@example.com
git -C "$STRANGER" config user.name Tester
git -C "$STRANGER" commit -q --allow-empty -m first
git -C "$STRANGER" remote add origin https://git.example.invalid/acme/stranger.git

# ---- stubs: the claude shim first, the other agents, and a browser ---------
mkdir -p "$RUN_DIR/bin"
for name in codex opencode pi omp; do
  cat >"$RUN_DIR/bin/$name" <<STUB_EOF
#!/bin/sh
printf '%s\n' "$name \$*" >>"$RUN_DIR/stub-calls.log"
case "\$1" in --version|-V|version) echo "$name 0.0.0 (e2e stub)" ;; esac
exit 0
STUB_EOF
  chmod +x "$RUN_DIR/bin/$name"
done
# What opens a URL. gpui may hand it to the desktop portal instead, which a private
# run has no bus for; the scenario that needs it says whether it was seen.
for name in xdg-open browser-stub; do
  cat >"$RUN_DIR/bin/$name" <<STUB_EOF
#!/bin/sh
printf '%s\n' "$name \$*" >>"$RUN_DIR/browser.log"
exit 0
STUB_EOF
  chmod +x "$RUN_DIR/bin/$name"
done
# `gh` and `glab` that record their arguments and say nobody is signed in. They are on the
# PATH of the one launch that looks for them (default_folds), never of the others.
CLI_STUBS="$RUN_DIR/clistubs"
mkdir -p "$CLI_STUBS"
for name in gh glab; do
  cat >"$CLI_STUBS/$name" <<STUB_EOF
#!/bin/sh
printf '%s\n' "$name \$*" >>"$RUN_DIR/forge-cli.log"
exit 1
STUB_EOF
  chmod +x "$CLI_STUBS/$name"
done
# Which claude shim the next launch puts first on the app's PATH: session-panel (the
# column's own scenarios), session-panel-bypass (the same, the CLI in bypassPermissions), trace (the trace
# scenarios), file-links (file_links), elicitation (the elicitation scenarios), session-state
# (session_state_background), background-tasks (the background_* scenarios), notices (the notice_*
# scenarios) or rename (the rename_* scenarios). A scenario picks its stub by
# naming it in SIRIO_E2E_CLAUDE_STUB through use_stub, and the run loop puts the default
# back before the next scenario.
use_stub() { # session-panel|session-panel-bypass|trace|file-links|elicitation|session-state|background-tasks|rename|permissions|notices
  SIRIO_E2E_CLAUDE_STUB=$1
  local stubs="$ROOT/Scripts/Tests/stubs/$SIRIO_E2E_CLAUDE_STUB" resolved
  APP_PATH="$stubs:$RUN_DIR/bin:/usr/local/bin:/usr/bin:/bin"
  resolved=$(PATH="$APP_PATH" command -v claude || true)
  [ "$resolved" = "$stubs/claude" ] || fail "claude resolves to '$resolved' under the app's PATH, not the $SIRIO_E2E_CLAUDE_STUB shim"
}
for stub in session-panel session-panel-bypass trace file-links elicitation session-state background-tasks rename permissions notices; do
  use_stub "$stub"
  echo "OK: claude resolves to the $stub shim"
done
use_stub session-panel

VULKAN_ENV=()
if [ -n "$DISPLAY_TARGET" ]; then
  LVP=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)
  [ -n "$LVP" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required for a framed run on Xvfb; install vulkan-swrast or drop --display"
  VULKAN_ENV=("VK_DRIVER_FILES=$LVP" "VK_ICD_FILENAMES=$LVP")
  echo "vulkan: lavapipe"
fi

# ---- calling the app --------------------------------------------------------
CALL_TIMEOUT=30
ctl() { echo "+ sirioctl $*"; timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@"; }
reply() { timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" --json 2>/dev/null; }

# One helper for what sirioctl has no verb for (surface.chat.open|send|read, which
# every build serves) and for reading the column: it speaks the control socket's
# line-delimited JSON itself.
cat >"$RUN_DIR/e2e.py" <<'PY'
import json, os, re, socket, sys, time


def rpc(method, params):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(os.environ["SIRIO_SOCKET"])
    s.sendall((json.dumps({"id": "e2e", "method": method, "params": params}) + "\n").encode())
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = s.recv(1 << 20)
        if not chunk:
            break
        buf += chunk
    s.close()
    response = json.loads(buf)
    if not response.get("ok"):
        raise RuntimeError(response.get("error") or method)
    return response.get("result") or {}


def state(surface):
    """The column's JSON and the rest of the chat's own read: its status, its transcript
    (the rows decoded) and the first and last transcript row its list drew (`first`, `last`)."""
    result = rpc("surface.chat.read", {"surfaceId": surface})
    panel = json.loads(result["sessionPanel"])
    chat = {k: v for k, v in result.items() if k != "sessionPanel"}
    chat["transcript"] = json.loads(chat.get("transcript") or "[]")
    # The composer band's record (layout, geometry, overflow, mode icon) is JSON in a string.
    chat["composerRow"] = json.loads(chat.get("composerRow") or "{}")
    # The registry of background tasks, in start order (A3).
    chat["backgroundTasks"] = json.loads(chat.get("backgroundTasks") or "[]")
    # The title Sirio asked Claude Code for (A4): "" until a manual rename, then {title, state[, error]}.
    chat["agentTitle"] = json.loads(chat["agentTitle"]) if chat.get("agentTitle") else ""
    # The question dock as painted (A5): "" when no question is open, else {caption, body, details, options, selected, digits}.
    chat["questionDock"] = json.loads(chat["questionDock"]) if chat.get("questionDock") else ""
    # The line above the composer (A7): "" when nothing shows, else {source, text, tone, held[, key, priority]}.
    chat["statusLine"] = json.loads(chat["statusLine"]) if chat["statusLine"] else ""
    chat["first"] = int(chat.get("firstVisibleEntry") or 0)
    chat["last"] = int(chat.get("lastVisibleEntry") or 0)
    return panel, chat


def panel(surface):
    return state(surface)[0]


class Helpers:
    def __init__(self, p):
        self.p = p

    def S(self, key):
        return next((s for s in self.p["sections"] if s["key"] == key), None)

    def R(self, section, key):
        s = self.S(section)
        return next((r for r in (s["rows"] if s else []) if r["key"] == key), None)

    def rows(self, section):
        s = self.S(section)
        return s["rows"] if s else []


def evaluate(expr, surface, p, c):
    # The expressions are this script's own, written next to their scenario; the
    # only data they see is the chat's own reads: `p` the column's JSON, `c` the rest of
    # `surface.chat.read`, and `tab()` the Trace tab's `surface.trace.read`.
    h = Helpers(p)

    def lanes_of():
        return {r["key"][len("trace/lane/"):]: r for r in h.rows("trace") if r["key"].startswith("trace/lane/")}

    def wait_rows():
        return [r for r in h.rows("trace") if r["key"].startswith("trace/wait/")]

    def row_index(row_id):
        # The transcript's own index of a tool or Agent row.
        return next((i for i, r in enumerate(c["transcript"]) if r.get("kind") == "tool" and r.get("id") == row_id), None)

    def drawn_index(row_id):
        # The index the transcript draws a row at: consecutive tool rows are one box, drawn at
        # the run's last row, so that is where a reveal of any of them lands.
        at = row_index(row_id)
        while at is not None and at + 1 < len(c["transcript"]) and c["transcript"][at + 1].get("kind") == "tool":
            at += 1
        return at

    def pending():
        return [(i, r["id"]) for i, r in enumerate(c["transcript"]) if r.get("kind") == "permission" and r.get("status") == "pending"]

    def elicit():
        # The newest elicitation row of the transcript, as `control_entry_row` reports it.
        return next((r for r in reversed(c["transcript"]) if r.get("kind") == "elicitation"), {})

    def tab():
        result = dict(rpc("surface.trace.read", {"surfaceId": surface}))
        result["spans"] = json.loads(result.get("spans") or "[]")
        # The strip of background tasks below the tree (A3).
        result["background"] = json.loads(result.get("background") or "[]")
        return result

    def T(span_id):
        return next((s for s in tab()["spans"] if s["id"] == span_id), None)

    def bg(task_id):
        # One background task's entry in `surface.chat.read`, or None.
        return next((t for t in c["backgroundTasks"] if t["taskId"] == task_id), None)

    namespace = {
        "bg": bg, "p": p, "c": c, "S": h.S, "R": h.R, "rows": h.rows, "json": json, "re": re,
        "lane": lambda key: h.R("trace", "trace/lane/" + key), "lanes_of": lanes_of,
        "wait_rows": wait_rows, "row_index": row_index, "drawn_index": drawn_index, "pending": pending, "elicit": elicit,
        "tab": tab, "T": T,
        "title_state": lambda: c["agentTitle"]["state"] if c["agentTitle"] else "",
    }
    return eval(expr, namespace)


command = sys.argv[1]
if command == "rpc":
    params = dict(arg.split("=", 1) for arg in sys.argv[3:])
    print(json.dumps(rpc(sys.argv[2], params)))
elif command == "chat-surface":
    print(rpc("surface.chat.open", {})["surfaceId"])
elif command in ("wait", "find"):
    # wait SURFACE DESC SECONDS EXPR: poll until EXPR over the column's JSON is truthy.
    # find SURFACE DESC SECONDS EXPR: the same, printing what EXPR returned.
    surface, desc, seconds, expr = sys.argv[2], sys.argv[3], float(sys.argv[4]), sys.argv[5]
    deadline = time.time() + seconds
    last, error = None, None
    while True:
        try:
            last, chat = state(surface)
            got = evaluate(expr, surface, last, chat)
            if got:
                print("  read: " + json.dumps(last, separators=(",", ":")), file=sys.stderr)
                print(got if command == "find" else f"OK: {desc}")
                sys.exit(0)
        except Exception as e:  # a read that fails is tried again until the time is up
            error = e
        if time.time() > deadline:
            print("  read: " + json.dumps(last, separators=(",", ":")), file=sys.stderr)
            try:
                chat = rpc("surface.chat.read", {"surfaceId": surface})
                chat.pop("sessionPanel", None)
                print("  chat: " + json.dumps(chat, separators=(",", ":")), file=sys.stderr)
            except Exception as e:
                print(f"  chat: unreadable ({e})", file=sys.stderr)
            print(f"FAIL: {desc}: not true within {seconds:.0f} s: {expr}" + (f" ({error})" if error else ""), file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
elif command == "paint-wait":
    # paint-wait SURFACE DESC SECONDS TASK_ID EXPR: like wait, and says to the chat every 250 ms
    # that a frame painted TASK_ID's tail (the test-only surface.chat.paintTail), as the Trace
    # tab's frames do while they draw it: the tail poll reads only a task painted since its tick.
    surface, desc, seconds, task_id, expr = sys.argv[2], sys.argv[3], float(sys.argv[4]), sys.argv[5], sys.argv[6]
    deadline = time.time() + seconds
    last, error = None, None
    while True:
        try:
            rpc("surface.chat.paintTail", {"surfaceId": surface, "taskId": task_id})
            last, chat = state(surface)
            if evaluate(expr, surface, last, chat):
                print(f"  background tasks: " + json.dumps(chat["backgroundTasks"], separators=(",", ":")), file=sys.stderr)
                print(f"OK: {desc}")
                sys.exit(0)
        except Exception as e:  # a read that fails is tried again until the time is up
            error = e
        if time.time() > deadline:
            print(f"  background tasks: " + json.dumps(chat["backgroundTasks"] if last else None, separators=(",", ":")), file=sys.stderr)
            print(f"FAIL: {desc}: not true within {seconds:.0f} s: {expr}" + (f" ({error})" if error else ""), file=sys.stderr)
            sys.exit(1)
        time.sleep(0.25)
elif command == "composer-compact":
    # composer-compact SURFACE SECONDS: wait for the compact row the Claude chat draws at the
    # default window (its measured band, send and chips), assert it and print what was read.
    surface, seconds = sys.argv[2], float(sys.argv[3])
    deadline = time.time() + seconds
    row = None
    while True:
        try:
            row = state(surface)[1]["composerRow"]
            if row.get("band") and row.get("send") and row.get("chips") and row.get("layout") == "compact":
                break
        except Exception as e:  # a read that fails is tried again until the time is up
            print(f"  read failed: {e}", file=sys.stderr)
        if time.time() > deadline:
            print(f"FAIL: composer-compact: no measured compact row within {seconds:.0f} s: {json.dumps(row)}", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
    print("  composerRow: " + json.dumps(row, separators=(",", ":")), file=sys.stderr)

    def near(a, b):
        return abs(a - b) <= 1.0

    def fail(why):
        print(f"FAIL: composer-compact: {why}: {json.dumps(row)}", file=sys.stderr)
        sys.exit(1)

    # The spec's own text, as Enzo's native-Claude chat shows it: Fast, Thinking and the
    # session panel in the overflow menu, and Bypass as a struck-through shield, in danger.
    if row["overflow"] != ["fast", "thinking", "session-panel"]:
        fail("the overflow menu is not fast, thinking, session-panel")
    print("OK: the overflow menu holds fast, thinking and session-panel")
    if row["modeIcon"] != "shield-off" or row["modeDanger"] is not True:
        fail("the mode pill is not the danger shield-off")
    print("OK: bypass permissions is the shield-off icon, drawn as danger")
    root, band, send, chips = row["root"], row["band"], row["send"], row["chips"]
    if root is None:
        fail("the chat's root was not measured")
    if not (near(band[0], root[0]) and near(band[0] + band[2], root[0] + root[2]) and near(band[1] + band[3], root[1] + root[3])):
        fail("the band does not span the chat and end on its bottom")
    print("OK: the band spans the chat and ends on its bottom")
    if not (send[0] >= band[0] - 1 and send[1] >= band[1] - 1 and send[0] + send[2] <= band[0] + band[2] + 1 and send[1] + send[3] <= band[1] + band[3] + 1):
        fail("send is outside the band")
    if chips[0] + chips[2] > send[0] + 1:
        fail("the chips reach past send's left edge")
    if not (chips[0] >= band[0] - 1 and chips[1] >= band[1] - 1 and chips[0] + chips[2] <= band[0] + band[2] + 1 and chips[1] + chips[3] <= band[1] + band[3] + 1):
        fail("the chips are outside the band")
    print("OK: send is inside the band and the chips end before it")
    if not near(chips[1], send[1]) or not near(chips[3], send[3]):
        fail("the chips and send are not one line")
    print("OK: the chips and send share one line")
elif command == "composer-overflow-xy":
    # composer-overflow-xy SURFACE SECONDS: wait for the measured trigger of the overflow menu and
    # print the window pixel at its centre as "x y".
    surface, seconds = sys.argv[2], float(sys.argv[3])
    deadline = time.time() + seconds
    while True:
        try:
            box = state(surface)[1]["composerRow"].get("overflowButton")
            if box:
                print(f"{round(box[0] + box[2] / 2)} {round(box[1] + box[3] / 2)}")
                break
        except Exception as e:  # a read that fails is tried again until the time is up
            print(f"  read failed: {e}", file=sys.stderr)
        if time.time() > deadline:
            print("FAIL: composer-overflow-xy: the overflow trigger was never measured", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
elif command == "stop-button-xy":
    # stop-button-xy SURFACE SECONDS TASK_ID: wait for the Stop button of the card's Background row
    # for TASK_ID to be drawn and print the window pixel at its centre as "x y".
    surface, seconds, task_id = sys.argv[2], float(sys.argv[3]), sys.argv[4]
    deadline = time.time() + seconds
    while True:
        try:
            box = state(surface)[0].get("stopButtons", {}).get(task_id)
            if box:
                print(f"{round(box[0] + box[2] / 2)} {round(box[1] + box[3] / 2)}")
                break
        except Exception as e:  # a read that fails is tried again until the time is up
            print(f"  read failed: {e}", file=sys.stderr)
        if time.time() > deadline:
            print(f"FAIL: stop-button-xy: the Stop button of {task_id} was never drawn", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
elif command == "switch-button-xy":
    # switch-button-xy SURFACE SECONDS KEY: wait for the Connectors card to have drawn the switch of
    # the row KEY and print the window pixel at its centre as "x y".
    surface, seconds, key = sys.argv[2], float(sys.argv[3]), sys.argv[4]
    deadline = time.time() + seconds
    while True:
        try:
            box = state(surface)[0].get("switchButtons", {}).get(key)
            if box:
                print(f"{round(box[0] + box[2] / 2)} {round(box[1] + box[3] / 2)}")
                break
        except Exception as e:  # a read that fails is tried again until the time is up
            print(f"  read failed: {e}", file=sys.stderr)
        if time.time() > deadline:
            print("FAIL: switch-button-xy: the switch was never drawn", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
elif command == "composer-menu":
    # composer-menu SURFACE SECONDS REASON: wait for the overflow menu the compact row opens
    # (`picker` is `overflow` and `menu` the rows its render built), and assert them: Fast is
    # there, disabled, with REASON as its tooltip; Thinking is there and enabled.
    surface, seconds, reason = sys.argv[2], float(sys.argv[3]), sys.argv[4]
    deadline = time.time() + seconds
    row = None
    while True:
        try:
            row = state(surface)[1]["composerRow"]
            if row.get("picker") == "overflow" and row.get("menu"):
                break
        except Exception as e:  # a read that fails is tried again until the time is up
            print(f"  read failed: {e}", file=sys.stderr)
        if time.time() > deadline:
            print(f"FAIL: composer-menu: the overflow menu was not drawn within {seconds:.0f} s: {json.dumps(row)}", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
    print("  composerRow: " + json.dumps(row, separators=(",", ":")), file=sys.stderr)
    rows = {r["id"]: r for r in row["menu"]}

    def fail(why):
        print(f"FAIL: composer-menu: {why}: {json.dumps(row['menu'])}", file=sys.stderr)
        sys.exit(1)

    fast, thinking = rows.get("fast"), rows.get("thinking")
    if fast is None or fast["enabled"] is not False or fast["tooltip"] != reason:
        fail(f"Fast is not a disabled row whose tooltip is '{reason}'")
    print(f"OK: the menu lists Fast, disabled, with its reason '{reason}'")
    if thinking is None or thinking["enabled"] is not True:
        fail("Thinking is not an enabled row")
    print("OK: the menu lists Thinking, enabled")
    print("OK: the rows are " + ", ".join(r["id"] for r in row["menu"]))
elif command == "check":
    # check SURFACE DESC EXPR: one read, no waiting.
    surface, desc, expr = sys.argv[2], sys.argv[3], sys.argv[4]
    p, chat = state(surface)
    print("  read: " + json.dumps(p, separators=(",", ":")), file=sys.stderr)
    if not evaluate(expr, surface, p, chat):
        print(f"FAIL: {desc}: {expr}", file=sys.stderr)
        sys.exit(1)
    print(f"OK: {desc}")
elif command in ("links-check", "link", "link-width"):
    # The links `surface.chat.read` reports for the chat's last paint: rows with kind (file|web),
    # label, target, the window pixel (x, y) a few pixels into the label at its vertical middle, and w,
    # its painted width.
    # links-check SURFACE SECONDS WEB_LABEL FILE_LABEL...: poll until the file links are exactly
    #   FILE_LABEL... (in any order) and the web links exactly WEB_LABEL.
    # link SURFACE KIND LABEL: poll until a link of that kind and label is reported, print "x y".
    # link-width SURFACE KIND LABEL: the same link, print `w`, how many pixels wide its label painted.
    surface = sys.argv[2]

    def read_links():
        result = rpc("surface.chat.read", {"surfaceId": surface})
        if "transcriptLinks" not in result:
            raise RuntimeError("surface.chat.read has no transcriptLinks, so the chat reports no link of its last paint")
        return json.loads(result["transcriptLinks"] or "[]")

    deadline = time.time() + (float(sys.argv[3]) if command == "links-check" else 15)
    rows, error = [], None
    while True:
        try:
            rows = read_links()
            error = None
            if command == "links-check":
                files = sorted(r["label"] for r in rows if r["kind"] == "file")
                web = sorted(r["label"] for r in rows if r["kind"] == "web")
                if files == sorted(sys.argv[5:]) and web == [sys.argv[4]]:
                    print("  links: " + json.dumps(rows, ensure_ascii=False), file=sys.stderr)
                    print(f"OK: the file links are {sorted(sys.argv[5:])} and the web link is {sys.argv[4]!r}")
                    sys.exit(0)
            else:
                row = next((r for r in rows if r["kind"] == sys.argv[3] and r["label"] == sys.argv[4]), None)
                if row:
                    print(int(float(row["w"])) if command == "link-width" else f"{int(float(row['x']))} {int(float(row['y']))}")
                    sys.exit(0)
        except SystemExit:
            raise
        except Exception as e:  # a read that fails is tried again until the time is up
            error = e
        if time.time() > deadline:
            print("  links: " + json.dumps(rows, ensure_ascii=False), file=sys.stderr)
            if command == "links-check":
                print(f"FAIL: the links are not the file links {sorted(sys.argv[5:])} and the web link {sys.argv[4]!r}"
                      + (f" ({error})" if error else ""), file=sys.stderr)
            else:
                print(f"FAIL: no {sys.argv[3]} link labelled {sys.argv[4]!r}" + (f" ({error})" if error else ""), file=sys.stderr)
            sys.exit(1)
        time.sleep(0.3)
elif command == "perf-window":
    # perf-window FILE SECONDS: how many trace.frame events the app recorded in the next
    # SECONDS, by wall clock (the file's header names the instant its own clock began),
    # once its writer has flushed past the window, which it does about once a second. The
    # writer drops events rather than block and says so in its health rows: the total it
    # reports is part of the answer.
    path, seconds = sys.argv[2], float(sys.argv[3])

    def read_perf():
        epoch, rows = None, []
        with open(path) as handle:
            for line in handle:
                line = line.rstrip("\n")
                if line.startswith("# sirio-perf-v1"):
                    epoch = int(next(f for f in line.split("\t") if f.startswith("epoch_ns=")).split("=")[1])
                elif line and not line.startswith("#"):
                    start, _, kind, name, entity, _ = line.split("\t")[:6]
                    rows.append((epoch + int(start), kind, name, int(entity)))
        return rows

    t0 = time.time_ns()
    time.sleep(seconds)
    t1 = time.time_ns()
    deadline = time.time() + 10
    while True:
        rows = read_perf()
        if any(name == "trace.dropped" and wall > t1 for wall, _, name, _ in rows):
            break
        if time.time() > deadline:
            print("FAIL: the performance trace was not flushed past the window", file=sys.stderr)
            sys.exit(1)
        time.sleep(0.25)
    frames = {}
    for wall, kind, name, entity in rows:
        if name == "trace.frame" and t0 <= wall <= t1:
            frames[entity] = frames.get(entity, 0) + 1
    dropped = sum(entity for _, _, name, entity in rows if name == "trace.dropped")
    print(json.dumps({"frames": sum(frames.values()), "by_entity": frames, "dropped": dropped,
                      "frames_in_file": sum(1 for _, _, name, _ in rows if name == "trace.frame")}))
PY
E2E() { python3 "$RUN_DIR/e2e.py" "$@"; }
SURFACE=""
# wait DESC EXPR [SECONDS]: the column's JSON as `p`, with S(section), R(section, row), rows(section).
wait_panel() { E2E wait "$SURFACE" "$1" "${3:-30}" "$2" || fail "$1"; }
# Like wait_panel, and says every 250 ms that a frame painted the task's tail (see paint-wait).
paint_wait() { E2E paint-wait "$SURFACE" "$1" "$3" "$2" "$4" || fail "$1"; } # desc task seconds expr
find_panel() { E2E find "$SURFACE" "$1" "${3:-30}" "$2" || fail "$1"; }
check_panel() { E2E check "$SURFACE" "$1" "$2" || fail "$1"; }
panel_ctl() { ctl surface chat panel "$SURFACE" "$@"; }
# The column opens; on a display it is also drawn: over the transcript of a chat narrower
# than 640 px (Sirio's default window), beside it in a window resized wide (--window-size).
open_column() {
  panel_ctl open >/dev/null
  wait_panel "the column is open" 'p["open"]'
  if [ -n "$DISPLAY_TARGET" ]; then
    if [ -n "$WINDOW_SIZE" ]; then
      wait_panel "the column is drawn beside the transcript" 'p["drawn"] and not p["overlay"]'
    else
      wait_panel "the column is laid over the narrow chat's transcript" 'p["drawn"] and p["overlay"]'
    fi
  fi
}
# Unfolds the cards a scenario works inside: every card but Environment starts folded,
# and a card that needs the agent or the forge is drawn, folded, only once it has found
# what it needs.
unfold_cards() { # key...
  local key
  for key in "$@"; do
    wait_panel "the $key card is drawn" "S('$key') is not None"
    panel_ctl toggle "$key" >/dev/null
    wait_panel "the $key card is unfolded" "S('$key')['expanded'] is True"
  done
}
# Unfolds a card unless it is open already, where unfold_cards would toggle it shut.
ensure_unfolded() { # key
  wait_panel "the $1 card is drawn" "S('$1') is not None"
  if ! E2E check "$SURFACE" "the $1 card is unfolded" "S('$1')['expanded'] is True" >/dev/null 2>&1; then
    unfold_cards "$1"
  fi
}
# Folds the cards above the one a capture is of, so a narrow chat's column shows it whole.
fold_cards() { # key...
  local key
  for key in "$@"; do
    panel_ctl toggle "$key" >/dev/null
    wait_panel "the $key card is folded" "S('$key')['expanded'] is False"
  done
}
# Polls a sirioctl read until one of its fields is `want`; the reads are logged.
wait_field() { # desc key want seconds sirioctl-args...
  local desc=$1 key=$2 want=$3 seconds=$4 json="" got="" _
  shift 4
  for _ in $(seq 1 $((seconds * 3))); do
    json=$(reply "$@" || true)
    got=$(printf '%s' "$json" | python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$key" 2>/dev/null || true)
    if [ "$got" = "$want" ]; then echo "  read: $json"; echo "OK: $desc"; return 0; fi
    sleep 0.3
  done
  echo "  read: $json" >&2
  fail "$desc: $key is '$got', not '$want'"
}

# ---- window captures -----------------------------------------------------------
find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"], capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"], capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
}
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window="" _
  for _ in $(seq 1 50); do
    window=$(find_window)
    [ -n "$window" ] && break
    sleep 0.2
  done
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $1"
  timeout -k 5 20 import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png" || fail "capture of $1 failed"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours; pid $APP_PID)"
}

# ---- launching an isolated Sirio ---------------------------------------------------
# `env -i` and this allowlist, so nothing of the user's reaches the app (no
# CLAUDE_CONFIG_DIR, MISE_*, GH_TOKEN, GIT_*, a D-Bus session bus, the user's
# XDG_RUNTIME_DIR). Each entry is needed:
#   PATH          the claude shim and the other agent stubs first, then the system directories
#   HOME, XDG_*   private: the login shell's files, Sirio's config, data, state and cache
#   XDG_RUNTIME_DIR private, mode 700: no user bus under it, so the host starts with setsid
#   SHELL, TERM, LANG  the login shell Sirio reads PATH from; text
#   BROWSER       the stub that logs the sign-in URL, if gpui hands it to a browser at all
#   DISPLAY, GPUI_X11_SCALE_FACTOR, VK_*  the window and software Vulkan (framed runs only)
#   SIRIO_SOCKET, SIRIO_DB, SIRIO_CREDENTIALS, SIRIO_HOST_HOME  the run's own
#   SIRIO_FORGE_TEST_ENDPOINTS  ghe.test -> the loopback fake forge (debug builds only)
#   SIRIO_FIXTURE_REPO  where the stub claude says the project memory lives
#   SIRIO_STUB_LOG  the shim's call log; SIRIO_CSP_E2E_RUN marks this run's processes
#   SIRIO_FIXTURE_DONE  the file the monitor script writes after its last turn (session_state_unprompted)
#   SIRIO_FIXTURE_GATE  the file session_state_background touches to release the fixture's held steps
#   SIRIO_FIXTURE_STOP_DELAY  seconds the background-tasks stub waits before it answers a stop_task (default 1;
#                        background_stop_click sets 5 so that a framed run's slow frames cannot skip `stopping`)
#   SIRIO_FIXTURE_FLOOR  1 makes the background-tasks stub the 2.1.257 floor (background_shell_floor)
#   SIRIO_FIXTURE_REQUEST_LOG  the file the background-tasks stub appends every control request to
#   SIRIO_FIXTURE_TITLE_STORE, SIRIO_FIXTURE_RENAME_ERROR, SIRIO_FIXTURE_HANDSHAKE_GATE, SIRIO_FIXTURE_RENAME_GATE
#                  the rename stub's CLI title file, the error it answers every rename with, the file that holds its
#                  handshake (the session-panel stub forwards SIRIO_FIXTURE_HANDSHAKE_GATE too, for hooks_refused) and
#                  the file that holds every rename's answer (rename_*)
#   SIRIO_FIXTURE_PERMISSION_FIELDS, SIRIO_FIXTURE_PERMISSION_RULES, SIRIO_FIXTURE_RULES_LOG, SIRIO_FIXTURE_RULES_GATE,
#                  SIRIO_FIXTURE_RESULT_GATE  the permissions stub's files, each handed to the fixture as its
#                  --fixture-permission-fields / -permission-rules / -rules-log / -rules-gate / -result-gate flag:
#                  the JSON merged into every can_use_tool, the rules the CLI lists, the log of rules requests,
#                  the file that holds the rules answers and the file that holds the turn's result (permission_*)
#   SIRIO_FIXTURE_RELOAD, SIRIO_FIXTURE_RELOAD_GATE, SIRIO_FIXTURE_SKILLS, SIRIO_FIXTURE_CLI_BUSY_GATE  the session-panel stub's
#                  reload kind, the file that holds every reload_plugins answer, refused for reload_skills, and the file
#                  that holds the CLI's running state (reload_*)
#   SIRIO_FIXTURE_HOOKS, SIRIO_FIXTURE_HOOKS_GATE  the session-panel stub's file whose JSON answers get_hooks_listing,
#                  and the file that holds every one of those answers (hooks_*)
#   SIRIO_FIXTURE_RETRY_GATE, SIRIO_FIXTURE_STARTUP  the notices stub's file that holds `retry twice` after its retries,
#                  and the kind of start-up failure the CLI reports (notice_*)
#   SIRIO_FIXTURE_LIMITS  the notices stub's JSON {"soon": epoch, "later": epoch}: the resets the `limit ...` prompts report
#   SIRIO_FIXTURE_USAGE  the notices stub's JSON {"marker": bool, "report": value, "fails": bool}: how `/usage` and
#                  `/context` answer (usage_*)
#   SIRIO_E2E_TZ   when set, the TZ Sirio runs under, so local times are the same wherever the run happens
#   TRACE_STEP_MS  the pace of the trace fixture's scripted lines
#   SIRIO_FIXTURE_USER_LOG, SIRIO_FIXTURE_CONTROL_LOG  the files the fixture appends every prompt and
#                  every control request to (composer_keys reads them)
#   SIRIO_PERF_TRACE  trace_idle's performance trace (a file that does not exist yet)
#   DBUS_SESSION_BUS_ADDRESS  a socket path that does not exist, so a notification goes nowhere;
#                  notification_click alone passes its private bus's address
# How many `claude --version` probes the shim has recorded.
probe_count() { local n; n=$(grep -c '^claude --version$' "$RUN_DIR/stub-calls.log" 2>/dev/null || true); echo "${n:-0}"; }
launch() { # name (a relaunch under the same name keeps its database and host)
  local name=$1 before
  local home="$RUN_DIR/$name-home" data="$RUN_DIR/$name-data" state="$RUN_DIR/$name-state"
  local cache="$RUN_DIR/$name-cache" runtime="$RUN_DIR/$name-runtime"
  mkdir -p "$home/.config" "$state" "$cache" "$data"
  [ -d "$runtime" ] || mkdir -m 700 "$runtime"
  if [ -n "$APPEARANCE" ] && [ ! -e "$RUN_DIR/$name.sqlite" ]; then
    (cd "$ROOT/rust" && cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$RUN_DIR/$name.sqlite" --appearance "$APPEARANCE") || fail "could not seed the appearance"
  fi
  local pairs=(
    "PATH=$APP_PATH" "HOME=$home" "XDG_CONFIG_HOME=$home/.config" "XDG_DATA_HOME=$data"
    "XDG_STATE_HOME=$state" "XDG_CACHE_HOME=$cache" "XDG_RUNTIME_DIR=$runtime"
    "SHELL=/bin/bash" "TERM=${TERM:-xterm-256color}" "LANG=${LANG:-C.UTF-8}"
    "BROWSER=$RUN_DIR/bin/browser-stub"
    "SIRIO_SOCKET=$RUN_DIR/$name.sock" "SIRIO_DB=$RUN_DIR/$name.sqlite"
    "SIRIO_CREDENTIALS=$RUN_DIR/$name-credentials.json" "SIRIO_HOST_HOME=$RUN_DIR/$name-host"
    "SIRIO_FORGE_TEST_ENDPOINTS=ghe.test=http://127.0.0.1:$FORGE_PORT"
    "GH_CONFIG_DIR=$home/gh" "GLAB_CONFIG_DIR=$home/glab"
    "SIRIO_FIXTURE_REPO=$LINKED" "SIRIO_STUB_LOG=$RUN_DIR/stub-calls.log"
    "SIRIO_CSP_E2E_RUN=$RUN_DIR" "TRACE_STEP_MS=${TRACE_STEP_MS:-400}"
    "SIRIO_FIXTURE_USER_LOG=$RUN_DIR/fixture-user.log" "SIRIO_FIXTURE_CONTROL_LOG=$RUN_DIR/fixture-control.log"
    "SIRIO_FIXTURE_RESPONSE_LOG=$RUN_DIR/$name-responses.jsonl" "SIRIO_FIXTURE_GATE=${SIRIO_FIXTURE_GATE:-}" "SIRIO_FIXTURE_DONE=${SIRIO_FIXTURE_DONE:-}"
    "SIRIO_FIXTURE_FLOOR=${SIRIO_FIXTURE_FLOOR:-}" "SIRIO_FIXTURE_STOP_DELAY=${SIRIO_FIXTURE_STOP_DELAY:-}" "SIRIO_FIXTURE_REQUEST_LOG=$RUN_DIR/$name-requests.jsonl"
    "SIRIO_FIXTURE_TITLE_STORE=${SIRIO_FIXTURE_TITLE_STORE:-}" "SIRIO_FIXTURE_RENAME_ERROR=${SIRIO_FIXTURE_RENAME_ERROR:-}"
    "SIRIO_FIXTURE_HANDSHAKE_GATE=${SIRIO_FIXTURE_HANDSHAKE_GATE:-}"
    "SIRIO_FIXTURE_RENAME_GATE=${SIRIO_FIXTURE_RENAME_GATE:-}"
    "SIRIO_FIXTURE_PERMISSION_FIELDS=${SIRIO_FIXTURE_PERMISSION_FIELDS:-}" "SIRIO_FIXTURE_PERMISSION_RULES=${SIRIO_FIXTURE_PERMISSION_RULES:-}"
    "SIRIO_FIXTURE_RULES_LOG=${SIRIO_FIXTURE_RULES_LOG:-}" "SIRIO_FIXTURE_RULES_GATE=${SIRIO_FIXTURE_RULES_GATE:-}"
    "SIRIO_FIXTURE_RESULT_GATE=${SIRIO_FIXTURE_RESULT_GATE:-}"
    "SIRIO_FIXTURE_TOGGLE=${SIRIO_FIXTURE_TOGGLE:-}" "SIRIO_FIXTURE_TOGGLE_GATE=${SIRIO_FIXTURE_TOGGLE_GATE:-}"
    "SIRIO_FIXTURE_RELOAD=${SIRIO_FIXTURE_RELOAD:-}" "SIRIO_FIXTURE_RELOAD_GATE=${SIRIO_FIXTURE_RELOAD_GATE:-}"
    "SIRIO_FIXTURE_SKILLS=${SIRIO_FIXTURE_SKILLS:-}" "SIRIO_FIXTURE_CLI_BUSY_GATE=${SIRIO_FIXTURE_CLI_BUSY_GATE:-}"
    "SIRIO_FIXTURE_RETRY_GATE=${SIRIO_FIXTURE_RETRY_GATE:-}" "SIRIO_FIXTURE_STARTUP=${SIRIO_FIXTURE_STARTUP:-}"
    "SIRIO_FIXTURE_LIMITS=${SIRIO_FIXTURE_LIMITS:-}" "SIRIO_FIXTURE_USAGE=${SIRIO_FIXTURE_USAGE:-}"
    "SIRIO_FIXTURE_HOOKS=${SIRIO_FIXTURE_HOOKS:-}" "SIRIO_FIXTURE_HOOKS_GATE=${SIRIO_FIXTURE_HOOKS_GATE:-}"
    # Sirio posts its notifications over D-Bus: the app never reaches the user's own session bus.
    # Only notification_click names a bus of its own (APP_SESSION_BUS, the run's private one).
    "DBUS_SESSION_BUS_ADDRESS=${APP_SESSION_BUS:-unix:path=$RUN_DIR/no-bus}"
  )
  # A performance trace names a file that does not exist yet, one per launch.
  [ -z "${PERF_TRACE:-}" ] || pairs+=("SIRIO_PERF_TRACE=$PERF_TRACE")
  [ -z "${LC_ALL:-}" ] || pairs+=("LC_ALL=$LC_ALL")
  [ -z "${SIRIO_E2E_TZ:-}" ] || pairs+=("TZ=$SIRIO_E2E_TZ")
  if [ -n "$DISPLAY_TARGET" ]; then
    pairs+=("DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1" "${VULKAN_ENV[@]}")
  fi
  export SIRIO_SOCKET="$RUN_DIR/$name.sock"
  [ "${#SIRIO_SOCKET}" -lt 108 ] || fail "the control socket path is too long for a unix socket: $SIRIO_SOCKET"
  printf '%s\n' "${pairs[@]}" > "$RUN_DIR/$name-expected-env.log"
  before=$(probe_count)
  # Started from the chat's own directory: it is the launch directory that becomes
  # the chat's cwd.
  (cd "$CHAT_CWD" && exec python3 -c 'import os, sys; os.setpgid(0, 0); os.execvp(sys.argv[1], sys.argv[1:])' \
    env -i "${pairs[@]}" "$BIN" >>"$RUN_DIR/$name-app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 75); do [ -S "$SIRIO_SOCKET" ] && break; sleep 0.2; done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket ($name)"
  for _ in $(seq 1 150); do
    [ "$(probe_count)" -gt "$before" ] && break
    sleep 0.2
  done
  [ "$(probe_count)" -gt "$before" ] \
    || fail "$name: the app's claude --version probe never reached the shim"
  echo "OK: $name: pid $APP_PID answers; its claude --version probe reached the shim"
  # A window focused for sure, so that a session on screen reads on screen whatever the
  # display (or a --state-only run, which has none) says about focus.
  ctl surface attention act --focus active >/dev/null || fail "$name: surface attention act --focus active was refused"
  if [ -n "$DISPLAY_TARGET" ] && [ -n "$WINDOW_SIZE" ]; then
    local window="" _
    for _ in $(seq 1 100); do
      window=$(find_window)
      [ -n "$window" ] && break
      sleep 0.2
    done
    [ -n "$window" ] || fail "$name: no window with _NET_WM_PID=$APP_PID"
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowmove "$window" 0 0 windowsize "$window" "${WINDOW_SIZE%x*}" "${WINDOW_SIZE#*x}" \
      || fail "$name: could not resize the window to $WINDOW_SIZE"
    sleep 1
    echo "OK: $name: the window is $WINDOW_SIZE"
  fi
}
stop_app() {
  ctl quit >/dev/null 2>&1 || true
  for _ in $(seq 1 100); do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.2
  done
  kill -0 "$APP_PID" 2>/dev/null && fail "sirio did not quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
  stop_hosts
  check_mise "after the app stopped"
}

# The positions, highest first, of the tabs of the given kinds that `surface tabs read` lists.
tab_positions() { # kind...
  reply surface tabs read | python3 -c '
import json, sys
kinds = sys.argv[1:]
tabs = json.load(sys.stdin)[0]
print(*sorted((int(k.split(".")[1]) for k, v in tabs.items() if k.startswith("tab.") and v.split("|")[0] in kinds), reverse=True))' "$@"
}

# A fresh Sirio on the fixture, with a token for the fake forge and a Claude chat open.
chat_app() { # name
  launch "$1"
  forge_post /__reset
  ctl project add "$PRIMARY" >/dev/null
  ctl surface change-requests show >/dev/null
  local account
  account=$(reply surface change-requests token --host ghe.test --forge github --token $'good\n' | python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get("account", ""))')
  [ "$account" = "fake-user" ] || fail "the token signed in as '$account'"
  open_chat
}

# Opens a Claude chat as the first and only tab of a fresh Sirio.
open_chat() {
  # The Open list offers an agent once its CLI has been looked for.
  for _ in $(seq 1 50); do
    "$CTL" surface open-list open --entry agent:claude >/dev/null 2>&1 && break
    sleep 0.3
  done
  # `surface.chat.open` answers the first chat tab, and a fresh Sirio starts with a
  # default one that belongs to no agent, beside a terminal that would share the
  # width with the chat (the column goes over the transcript under 640 px): close both, so the
  # chat opened here is the first and the only tab.
  local old_tabs
  old_tabs=$(tab_positions chat terminal)
  ctl surface open-list open --entry agent:claude >/dev/null || fail "the Open list never offered a Claude chat"
  local position
  for position in $old_tabs; do ctl surface tabs close "$position" --confirm >/dev/null; done
  SURFACE=""
  for _ in $(seq 1 50); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "no chat tab opened"
  echo "OK: chat surface $SURFACE"
}

# ---- the trace scenarios' helpers -----------------------------------------------------
# A fresh Sirio on the fixture with the trace stub and a Claude chat, its column open.
# TRACE_STEP_MS (the optional second argument, in ms) paces the fixture's scripted lines;
# a scenario that reads no live state runs them fast.
trace_chat() { # name [step-ms]
  use_stub trace
  TRACE_STEP_MS=${2:-400}
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  open_column
}

# Sends a prompt once the chat is neither connecting nor streaming (a prompt sent while the
# agent connects is held back, and one sent while a turn streams is queued).
send_prompt() { # text
  wait_panel "the chat is ready for a prompt" 'c["status"] in ("idle", "completed")' 30
  E2E rpc surface.chat.send surfaceId="$SURFACE" text="$1" >/dev/null || fail "sending '$1' failed"
  echo "step: sent '$1'"
}

# Answers the permission card the turn waits on: allow_once or reject_once.
answer_permission() { # option
  local id
  id=$(find_panel "a permission card is pending" 'next((r["id"] for r in c["transcript"] if r.get("kind") == "permission" and r.get("status") == "pending"), "")' 30)
  E2E rpc surface.chat.permission surfaceId="$SURFACE" requestId="$id" optionId="$1" >/dev/null || fail "answering request $id with $1 failed"
  echo "OK: request $id answered with $1"
}

# ---- the elicitation scenarios' helpers ----------------------------------------------
# What the debug-only `surface.chat.elicitation` does to the open question: a digit key, Enter in
# the free-text row, Esc. The dock ignores a row for half a second after a question or a step
# appears, and the verb says so: such a refusal is tried again, any other is a failure.
elicit_rpc() { # description, then the verb's params after requestId
  local desc=$1 id out="" _
  shift
  id=$(find_panel "an elicitation is open" 'next((r["id"] for r in c["transcript"] if r.get("kind") == "elicitation" and r.get("status") == "pending"), "")' 30)
  for _ in $(seq 1 25); do
    if out=$(E2E rpc surface.chat.elicitation surfaceId="$SURFACE" requestId="$id" "$@" 2>&1); then
      echo "OK: $desc (request $id)"
      return 0
    fi
    case "$out" in *"not armed"*) sleep 0.2 ;; *) break ;; esac
  done
  fail "$desc (request $id) was refused: $(printf '%s' "$out" | tail -n 1)"
}
elicit_row() { elicit_rpc "row $1 chosen" row="$1"; }
elicit_text() { elicit_rpc "text '$1' typed" text="$1"; }
elicit_cancel() { elicit_rpc "the question cancelled" cancel=1; }
# Chooses the Skip row of the step on screen.
elicit_skip() {
  local n
  n=$(find_panel "the step offers Skip" 'elicit()["rows"].split("|").index("Skip") + 1' 10)
  elicit_row "$n"
}
# A private `.bash_profile` in launch NAME's HOME that puts the harness's stub openers
# ($RUN_DIR/bin) first on the login-shell PATH the app's `xdg-open` inherits, even if a system
# profile prepends directories: a link the app opens lands in $RUN_DIR/browser.log and never starts
# a real browser. Call it before `launch NAME`.
stub_openers_lead_login_path() { # name
  mkdir -p "$RUN_DIR/$1-home"
  printf 'export PATH="%s/bin:$PATH"\n' "$RUN_DIR" >"$RUN_DIR/$1-home/.bash_profile"
}
# The Open row calls `cx.open_url`. A --state-only run has no display: the app's LIVE environment
# (/proc/$APP_PID/environ) names neither DISPLAY nor WAYLAND_DISPLAY, so gpui runs headless and its
# `open_uri` is a no-op (gpui_linux headless/client.rs:117); the row's accept itself is checked by
# the responses log. A painted run (--display) must have the xdg-open stub record the URL.
elicit_url_not_opened() {
  local url='https://example.com/auth?x=1' _
  if [ -z "$DISPLAY_TARGET" ]; then
    python3 - "$APP_PID" <<'PY' || fail "the live app's environment names a display, so gpui is not headless and the link could reach a real opener"
import sys
names = {e.split(b"=", 1)[0] for e in open(f"/proc/{sys.argv[1]}/environ", "rb").read().split(b"\0") if e}
sys.exit(1 if names & {b"DISPLAY", b"WAYLAND_DISPLAY"} else 0)
PY
    echo "OK: no display in the live app's env, so gpui runs headless and its open_uri is a no-op"
    return 0
  fi
  for _ in $(seq 1 50); do
    grep -Fxq "xdg-open $url" "$RUN_DIR/browser.log" 2>/dev/null && { echo "OK: the xdg-open stub recorded $url"; return 0; }
    sleep 0.3
  done
  fail "the xdg-open stub never recorded $url (it recorded: $(cat "$RUN_DIR/browser.log" 2>/dev/null || echo nothing))"
}
# A fresh Sirio on the elicitation stub, a Claude chat in $LINKED, and the log its responses go to.
elicit_chat() { # name
  use_stub elicitation
  launch "$1"
  ELICIT_LOG="$RUN_DIR/$1-responses.jsonl"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
}
# Fails unless the responses log holds exactly COUNT lines whose control_response payload is JSON;
# prints each such payload. A line is `{"type":"control_response","response":{"subtype":...,
# "request_id":...,"response":{...}}}`; an error answer has `error` in place of `response`.
elicit_log_check() { # description python-predicate-over-the-list-named-answers
  python3 - "$ELICIT_LOG" "$1" "$2" <<'PY' || fail "$1: the responses log does not say so ($ELICIT_LOG)"
import json, sys
path, desc, expr = sys.argv[1:4]
lines = [json.loads(line) for line in open(path) if line.strip()]
answers = [line["response"] for line in lines if line.get("type") == "control_response"]
if not eval(expr, {"answers": answers, "lines": lines, "json": json}):
    print("FAIL: " + desc + ": " + json.dumps(lines), file=sys.stderr)
    sys.exit(1)
print("OK: " + desc)
PY
  cp "$ELICIT_LOG" "$OUT_DIR/$(basename "$ELICIT_LOG")"
}

# The Secondary half's tab kinds, comma separated, as `surface layout read` lists them. A read
# that failed, or one with no `secondaryKinds`, is an error and prints nothing: it is never an
# empty list, which would read as "no trace tab".
secondary_kinds() {
  local out
  out=$(reply surface layout read) || return 1
  python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["secondaryKinds"])' <<<"$out" 2>/dev/null
}
# How many trace tabs the Secondary half has (the chat's own tab is in it too). An error when
# the layout could not be read.
trace_tab_count() {
  local kinds
  kinds=$(secondary_kinds) || return 1
  printf '%s\n' "$kinds" | tr ',' '\n' | grep -c '^trace$' || true
}
# Fails unless the Secondary half has exactly WANT trace tabs, or the layout cannot be read.
expect_trace_tabs() { # desc want
  local got
  got=$(trace_tab_count) || fail "$1: the layout could not be read"
  [ "$got" = "$2" ] || fail "$1: $got trace tab(s), expected $2 ($(secondary_kinds || echo unreadable))"
  echo "OK: $1 ($got trace tab(s))"
}
# Polls the layout until the Secondary half has (present) or has not (absent) a trace tab.
# The tab closes one paint after its chat is released, so the wait is bounded and the
# window is painted. A read that fails is "not yet", never "absent": the wait can end in
# a match only on a layout it read.
wait_trace_tab() { # desc present|absent seconds
  local desc=$1 want=$2 seconds=${3:-20} kinds="" has="" read_ok=0 _
  for _ in $(seq 1 $((seconds * 3))); do
    if kinds=$(secondary_kinds); then
      read_ok=1
      case ",$kinds," in *,trace,*) has=present ;; *) has=absent ;; esac
      if [ "$has" = "$want" ]; then echo "  read: secondaryKinds=$kinds"; echo "OK: $desc"; return 0; fi
    else
      read_ok=0
    fi
    sleep 0.3
  done
  [ "$read_ok" = 1 ] || fail "$desc: the layout could not be read at the end of $seconds s"
  fail "$desc: secondaryKinds is '$kinds', a trace tab is not $want"
}

# How many trace.frame events the app recorded in the next SECONDS, `zero` or `some` of
# them expected. The performance trace is a file the run named for this launch.
frames_in_window() { # desc seconds zero|some
  local desc=$1 seconds=$2 want=$3 out frames dropped
  out=$(E2E perf-window "$PERF_TRACE" "$seconds") || fail "$desc: the performance trace could not be read"
  echo "  perf: $out"
  frames=$(printf '%s' "$out" | python3 -c 'import json, sys; print(json.load(sys.stdin)["frames"])')
  dropped=$(printf '%s' "$out" | python3 -c 'import json, sys; print(json.load(sys.stdin)["dropped"])')
  [ "$dropped" = 0 ] || fail "$desc: the trace dropped $dropped event(s), so no count can be trusted"
  case "$want" in
    zero) [ "$frames" = 0 ] || fail "$desc: $frames trace.frame event(s) in $seconds s, expected none" ;;
    # Software Vulkan paints a frame in well over the loop's 33 ms, so a drawn turn asks for
    # a handful in three seconds, not ninety.
    some) [ "$frames" -ge 3 ] || fail "$desc: $frames trace.frame event(s) in $seconds s, expected the animation's own (3 or more)" ;;
  esac
  # Who asked: the owners' entity ids of the window's frames, space separated, sorted.
  LAST_ENTITIES=$(printf '%s' "$out" | python3 -c 'import json, sys; print(*sorted(json.load(sys.stdin)["by_entity"]))')
  echo "OK: $desc: $frames trace.frame event(s) in $seconds s from entit(ies) [${LAST_ENTITIES:-none}] (dropped $dropped)"
}

# ---- scenarios ---------------------------------------------------------------------
scenario_environment() {
  chat_app environment
  check_panel "the column starts closed" 'p["open"] is False'
  open_column
  wait_panel "the project is named" "R('environment', 'environment.project') and R('environment', 'environment.project')['label'] == 'widgets'"
  # The app starts a chat in the worktree's root even when it is launched from
  # src/ (`initial_working_directory` walks up to the repository), so the card's
  # "the chat's own directory, not the root" is a unit-tested distinction here and
  # the directory this run can reach is the root.
  wait_panel "the directory is the chat's own, the linked worktree" \
    "(R('environment', 'environment.directory') or {}).get('label', '').endswith('/login')"
  wait_panel "the branch is feat/login, tagged worktree" \
    "(R('environment', 'environment.branch') or {}).get('label') == 'feat/login' and R('environment', 'environment.branch')['detail'] == 'worktree'"
  capture environment
  stop_app
}

scenario_default_folds() {
  chat_app default_folds
  open_column
  wait_panel "the change request card is drawn folded, with no row" "S('change_request') is not None and rows('change_request') == []"
  wait_panel "Context shows its summary" "S('context') is not None and S('context')['summary'] != ''"
  wait_panel "Connectors shows its summary" "S('connectors') is not None and S('connectors')['summary'] != ''"
  check_panel "Environment is open and Context, Connectors and Change request are folded" \
    "S('environment')['expanded'] is True and all(S(k)['expanded'] is False for k in ('context', 'connectors', 'change_request'))"
  capture default-folds
  # `chat_app` left the right panel on the change request list, which asks the forge
  # while it is shown: move it off, or the log would be the list's and not the card's.
  echo "step: the right panel leaves the change request list"
  ctl surface open-list open --entry show >/dev/null
  wait_field "the right panel shows the Open list" rightPanelView open 10 surface layout read
  sleep 2
  local before after
  before=$(wc -l < "$RUN_DIR/github-requests.log")
  echo "step: five seconds with the column open (forge requests so far: $before)"
  sleep 5
  after=$(wc -l < "$RUN_DIR/github-requests.log")
  [ "$after" = "$before" ] || fail "the forge saw $((after - before)) request(s) while the card was folded ($before -> $after)"
  check_panel "the card is still folded and unread" "S('change_request')['expanded'] is False and rows('change_request') == []"
  echo "OK: the forge's whole request log did not move with the column open and the card folded ($before lines)"
  echo "step: unfolding the card reads it"
  unfold_cards change_request
  wait_panel "the card is #112" "R('change_request', 'change_request/card') and R('change_request', 'change_request/card')['label'].startswith('#112')"
  [ "$(card_reads)" -ge 1 ] || fail "the card shows #112 but the forge saw no read of the branch's change request"
  echo "OK: the forge saw $(card_reads) read(s) of the branch's change request after the unfold"
  capture default-folds-unfolded
  stop_app

  # The probe asks nothing of a host: `gh auth status` would contact it. A worktree whose
  # remote is on a host with no setting and no token has gh and glab recording every call.
  echo "step: a host nothing names, with gh and glab that record every call"
  local stubbed="$CLI_STUBS:$APP_PATH"
  [ "$(PATH="$stubbed" command -v gh)" = "$CLI_STUBS/gh" ] || fail "gh does not resolve to the recording stub"
  [ "$(PATH="$stubbed" command -v glab)" = "$CLI_STUBS/glab" ] || fail "glab does not resolve to the recording stub"
  CHAT_CWD="$STRANGER" APP_PATH="$stubbed" launch default_folds_stranger
  ctl project add "$STRANGER" >/dev/null
  open_chat
  open_column
  sleep 5
  check_panel "no change request card is drawn for a host nothing names" "S('change_request') is None"
  [ ! -s "$RUN_DIR/forge-cli.log" ] || fail "opening the column ran gh or glab: $(tr '\n' ';' < "$RUN_DIR/forge-cli.log")"
  echo "OK: opening the column on git.example.invalid ran neither gh nor glab"
  stop_app
}

scenario_context() {
  chat_app context
  open_column
  unfold_cards context
  # Opened before any prompt: the report is asked for once, on open.
  wait_panel "a context section appears with no prompt sent" "S('context') is not None and len(rows('context')) > 0"
  wait_panel "the first reducible group is Memory files, 34.2k" \
    "rows('context')[0]['label'] == 'You can reduce it' and rows('context')[1]['label'] == 'Memory files' and rows('context')[1]['detail'] == '34.2k'"
  echo "step: one prompt"
  E2E rpc surface.chat.send surfaceId="$SURFACE" text="hello" >/dev/null
  for _ in $(seq 1 100); do
    [ "$(E2E rpc surface.chat.read surfaceId="$SURFACE" | python3 -c 'import json, sys; print(json.load(sys.stdin)["status"])')" = completed ] && break
    sleep 0.3
  done
  [ "$(E2E rpc surface.chat.read surfaceId="$SURFACE" | python3 -c 'import json, sys; print(json.load(sys.stdin)["status"])')" = completed ] || fail "the prompt's turn never completed"
  echo "OK: the turn completed"
  wait_panel "the context total is the stub's" "'16%' in S('context')['summary'] and '64.9k' in S('context')['summary']"
  wait_panel "Memory files is still the first reducible group" \
    "rows('context')[1]['label'] == 'Memory files' and rows('context')[1]['detail'] == '34.2k'"
  check_panel "free space closes the card, after Claude Code" \
    "rows('context')[-1]['key'] == 'context/free_space' and any(r['key'] == 'context/claude_code' for r in rows('context')[:-1])"
  check_panel "the memory group's remainder sits after its files" \
    "[r['label'] for r in rows('context') if r['key'].startswith('context/memory/')][-1] == 'Other'"
  echo "step: a plugin node unfolds to its agents"
  local plugin="context/plugins/plugin-dev"
  check_panel "the plugin node is folded" "R('context', '$plugin') is not None and R('context', '$plugin')['expanded'] is False"
  panel_ctl toggle "$plugin" >/dev/null
  wait_panel "the plugin's agent appears" "R('context', '$plugin/agent:agent-creator') is not None"
  capture context
  echo "step: the groups fold, so the Fixed group shows in the capture"
  fold_cards environment
  local node
  for node in context/memory context/plugins context/conversation; do
    panel_ctl toggle "$node" >/dev/null
    wait_panel "$node is folded" "R('context', '$node')['expanded'] is False"
  done
  capture context-fixed
  panel_ctl toggle context/memory >/dev/null
  wait_panel "Memory files is open again" "R('context', 'context/memory')['expanded'] is True"
  echo "step: the project CLAUDE.md opens as a file tab"
  local memory
  memory=$(find_panel "the project memory row" "next(r['key'] for r in rows('context') if r['key'].endswith('/CLAUDE.md') and r['tag'] == 'Project')")
  panel_ctl activate "$memory" >/dev/null
  wait_field "the active tab is a file tab on the project CLAUDE.md" path "$LINKED/CLAUDE.md" 20 surface file read
  wait_field "that file tab has loaded the file" state loaded 20 surface file read
  stop_app
}

scenario_connectors() {
  chat_app connectors
  open_column
  unfold_cards connectors
  local codegraph="connectors/user/codegraph" notion="connectors/claude.ai/claude.ai Notion"
  wait_panel "codegraph failed, offering Reconnect" \
    "R('connectors', '$codegraph') and R('connectors', '$codegraph')['detail'] == 'failed' and R('connectors', '$codegraph')['action'] == 'Reconnect'"
  wait_panel "Notion needs sign-in, offering Sign in" \
    "R('connectors', '$notion') and R('connectors', '$notion')['detail'] == 'needs sign-in' and R('connectors', '$notion')['action'] == 'Sign in'"
  fold_cards environment
  capture connectors-before
  panel_ctl activate "$codegraph" >/dev/null
  wait_panel "codegraph reconnected" "R('connectors', '$codegraph')['detail'] == 'connected'"
  panel_ctl activate "$notion" >/dev/null
  wait_panel "Notion waits for the sign-in" "R('connectors', '$notion')['detail'] == 'waiting for sign-in…'"
  capture connectors-after
  sleep 1
  if grep -q '127.0.0.1:9/authorize' "$RUN_DIR/browser.log" 2>/dev/null; then
    echo "OK: the sign-in URL reached the browser stub: $(grep '127.0.0.1:9/authorize' "$RUN_DIR/browser.log" | head -n 1)"
  else
    echo "NOTE: the sign-in URL was not observed through the BROWSER/xdg-open stubs (gpui opens URLs through the desktop portal when it can); the row's waiting state is what is asserted"
  fi
  stop_app
}

# ---- the connector_switch* scenarios (A6) ---------------------------------------------
# How many control requests of one subtype the CLI heard after a mark taken with control_log_lines.
control_since() { # mark subtype
  python3 -c '
import json, sys
mark, count = int(sys.argv[2]), 0
try:
    lines = open(sys.argv[1], encoding="utf-8").read().splitlines()
except FileNotFoundError:
    lines = []
for line in lines[mark:]:
    if json.loads(line).get("subtype") == sys.argv[3]:
        count += 1
print(count)' "$RUN_DIR/fixture-control.log" "$1" "$2"
}
# How many requests of SUBTYPE came after the first request of FIRST after the mark: with FIRST the
# toggle, whatever follows it was asked for by the switch itself or by something that outlived it.
control_after_first() { # mark first-subtype subtype
  python3 -c '
import json, sys
mark, count, seen = int(sys.argv[2]), 0, False
try:
    lines = open(sys.argv[1], encoding="utf-8").read().splitlines()
except FileNotFoundError:
    lines = []
for line in lines[mark:]:
    subtype = json.loads(line).get("subtype")
    if seen and subtype == sys.argv[4]:
        count += 1
    if subtype == sys.argv[3]:
        seen = True
print(count)' "$RUN_DIR/fixture-control.log" "$1" "$2" "$3"
}
# expect_since MARK SUBTYPE COUNT LABEL: exactly COUNT such requests came after the mark.
expect_since() {
  local got
  got=$(control_since "$1" "$2")
  [ "$got" = "$3" ] || fail "$4: the CLI heard $got $2 requests after the mark, not $3"
}
# The Connectors card of a fresh Sirio, unfolded, with both servers read.
switch_chat() { # name
  chat_app "$1"
  open_column
  unfold_cards connectors
  wait_panel "the user server failed and the claude.ai server needs sign-in" \
    "R('connectors', '$CODEGRAPH') and R('connectors', '$CODEGRAPH')['detail'] == 'failed' and R('connectors', '$NOTION') and R('connectors', '$NOTION')['detail'] == 'needs sign-in'"
}
CODEGRAPH="connectors/user/codegraph"
NOTION="connectors/claude.ai/claude.ai Notion"

scenario_connector_switch() {
  switch_chat connector_switch
  wait_panel "both server rows read switch on" \
    "R('connectors', '$CODEGRAPH')['switch'] is True and R('connectors', '$NOTION')['switch'] is True"
  local mark
  mark=$(control_log_lines)
  echo "step: the user server off"
  panel_ctl switch "$CODEGRAPH" off >/dev/null
  wait_panel "the user server reads switch off, disabled, with no action" \
    "R('connectors', '$CODEGRAPH')['switch'] is False and R('connectors', '$CODEGRAPH')['detail'] == 'disabled' and R('connectors', '$CODEGRAPH')['action'] is None"
  echo "step: the user server on: it starts, and the backoff finds it failed again"
  panel_ctl switch "$CODEGRAPH" on >/dev/null
  wait_panel "the user server reads switch on and starting" \
    "R('connectors', '$CODEGRAPH')['switch'] is True and R('connectors', '$CODEGRAPH')['detail'] == 'starting'" 10
  wait_panel "the user server reads failed again within the backoff" \
    "R('connectors', '$CODEGRAPH')['switch'] is True and R('connectors', '$CODEGRAPH')['detail'] == 'failed'" 5
  echo "step: the user server on again: the state is shown already, so nothing is sent"
  panel_ctl switch "$CODEGRAPH" on >/dev/null || fail "a switch to the state shown was refused"
  echo "step: the claude.ai server off"
  panel_ctl switch "$NOTION" off >/dev/null
  wait_panel "the claude.ai server reads switch off, disabled, and has lost its Sign in" \
    "R('connectors', '$NOTION')['switch'] is False and R('connectors', '$NOTION')['detail'] == 'disabled' and R('connectors', '$NOTION')['action'] is None"
  sleep 1
  expect_since "$mark" mcp_toggle 3 "two toggles for the user server and one for the claude.ai server"
  expect_since "$mark" mcp_authenticate 0 "no sign-in"
  echo "OK: connector_switch: a switch is read from mcp_status, a repeat sends nothing, three toggles and no sign-in"
  stop_app
}

scenario_connector_switch_in_flight() {
  SIRIO_FIXTURE_TOGGLE_GATE="$RUN_DIR/toggle-gate"
  rm -f "$SIRIO_FIXTURE_TOGGLE_GATE"
  switch_chat connector_switch_in_flight
  local mark
  mark=$(control_log_lines)
  panel_ctl switch "$CODEGRAPH" off >/dev/null
  wait_panel "the user server reads switching… with its switch off" \
    "R('connectors', '$CODEGRAPH')['detail'] == 'switching…' and R('connectors', '$CODEGRAPH')['switch'] is False"
  check_panel "the switch takes no click while the toggle is open" \
    "R('connectors', '$CODEGRAPH')['switchEnabled'] is False"
  if panel_ctl switch "$CODEGRAPH" on >/dev/null 2>&1; then
    fail "a second flip of a server whose toggle is open was accepted"
  fi
  sleep 1
  expect_since "$mark" mcp_toggle 1 "the second flip sent nothing"
  : >"$SIRIO_FIXTURE_TOGGLE_GATE"
  wait_panel "the user server reads disabled once the CLI answers" \
    "R('connectors', '$CODEGRAPH')['detail'] == 'disabled' and R('connectors', '$CODEGRAPH')['switch'] is False" 15
  check_panel "the switch takes a click again" "R('connectors', '$CODEGRAPH')['switchEnabled'] is True"
  expect_since "$mark" mcp_toggle 1 "one toggle in all"
  echo "OK: connector_switch_in_flight: switching… while the toggle is open, a second flip failed and sent nothing"
  stop_app
}

scenario_connector_switch_waiting() {
  switch_chat connector_switch_waiting
  panel_ctl activate "$NOTION" >/dev/null
  wait_panel "the sign-in server waits for the sign-in" "R('connectors', '$NOTION')['detail'] == 'waiting for sign-in…'"
  local mark after
  mark=$(control_log_lines)
  panel_ctl switch "$NOTION" off >/dev/null
  wait_panel "the sign-in server reads disabled" "R('connectors', '$NOTION')['detail'] == 'disabled' and R('connectors', '$NOTION')['switch'] is False"
  echo "step: past the sign-in poll's 3 s step"
  sleep 4
  expect_since "$mark" mcp_toggle 1 "one toggle"
  # The switch replaces the server's task before its toggle is written, so after the toggle line
  # the only mcp_status is the switch's own refresh; a second is a poll that survived.
  after=$(control_after_first "$mark" mcp_toggle mcp_status)
  [ "$after" = 1 ] || fail "the CLI heard $after mcp_status requests after the toggle, not 1 (a poll survived, or the refresh is missing)"
  echo "OK: connector_switch_waiting: switching a waiting server off dropped its sign-in poll"
  stop_app
}

scenario_connector_switch_refused() {
  SIRIO_FIXTURE_TOGGLE=refused
  switch_chat connector_switch_refused
  local mark
  mark=$(control_log_lines)
  panel_ctl switch "$CODEGRAPH" off >/dev/null
  wait_panel "every server row has lost its switch" \
    "[r for r in rows('connectors') if r['depth'] == 1] and all(r['switch'] is None for r in rows('connectors') if r['depth'] == 1)"
  check_panel "the details are unchanged and no row has a note" \
    "R('connectors', '$CODEGRAPH')['detail'] == 'failed' and R('connectors', '$NOTION')['detail'] == 'needs sign-in' and all(r['note'] == '' for r in rows('connectors'))"
  expect_since "$mark" mcp_toggle 1 "one refused toggle"
  echo "OK: connector_switch_refused: a refused mcp_toggle hides every switch and changes nothing else"
  stop_app
}

scenario_connector_switch_missing() {
  SIRIO_FIXTURE_TOGGLE=missing
  switch_chat connector_switch_missing
  local mark
  mark=$(control_log_lines)
  panel_ctl switch "$CODEGRAPH" off >/dev/null
  wait_panel "the row carries the CLI's sentence" "'Server not found' in R('connectors', '$CODEGRAPH')['note']"
  check_panel "the switch still reads on and the row still reads failed" \
    "R('connectors', '$CODEGRAPH')['switch'] is True and R('connectors', '$CODEGRAPH')['detail'] == 'failed'"
  expect_since "$mark" mcp_toggle 1 "one failed toggle"
  echo "OK: connector_switch_missing: a failed toggle is a note and the switch keeps the state mcp_status gave"
  stop_app
}

scenario_connector_switch_unread() {
  SIRIO_FIXTURE_TOGGLE=unread
  switch_chat connector_switch_unread
  local mark
  mark=$(control_log_lines)
  panel_ctl switch "$CODEGRAPH" off >/dev/null
  wait_panel "the row says the switch was saved and the state could not be read" \
    "R('connectors', '$CODEGRAPH')['note'].startswith('Switched, but the new state could not be read')" 15
  sleep 1
  expect_since "$mark" mcp_toggle 1 "one toggle"
  echo "OK: connector_switch_unread: a switch whose follow-up read failed says so on its row"
  stop_app
}

scenario_connector_switch_click() {
  stub_openers_lead_login_path connector_switch_click
  switch_chat connector_switch_click
  fold_cards environment
  local xy lines mark
  xy=$(E2E switch-button-xy "$SURFACE" 15 "$NOTION") || fail "the claude.ai server switch was never drawn"
  capture connector-switches
  echo "step: the pointer on the switch, for its tooltip"
  local origin x y
  origin=$(window_origin)
  x=$(( ${xy% *} + ${origin% *} ))
  y=$(( ${xy#* } + ${origin#* } ))
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove "$x" "$y" || fail "xdotool could not move to the switch"
  sleep 2
  capture connector-switch-hover
  lines=$(wc -l <"$RUN_DIR/browser.log" 2>/dev/null) || lines=0
  mark=$(control_log_lines)
  xy=$(E2E switch-button-xy "$SURFACE" 15 "$NOTION") || fail "the switch was never drawn after the captures"
  echo "step: a real click on the switch, drawn at window pixel $xy"
  click_at_window "$xy"
  wait_panel "the claude.ai server reads switch off and disabled" \
    "R('connectors', '$NOTION')['switch'] is False and R('connectors', '$NOTION')['detail'] == 'disabled'" 15
  sleep 1
  expect_since "$mark" mcp_toggle 1 "one toggle for the click"
  expect_since "$mark" mcp_authenticate 0 "the click started no sign-in"
  [ "$(( $(wc -l <"$RUN_DIR/browser.log" 2>/dev/null || echo 0) ))" = "$((lines))" ] || fail "the click opened a page in the browser stub"
  echo "OK: connector_switch_click: a real click on the switch toggled that server and started no sign-in"
  stop_app
}

# ---- the reload_* scenarios (A6) ------------------------------------------------------
RELOAD="connectors.reload"
PLUGIN_SERVER="connectors/plugins/plugin:pa:m"
BUSY_NOTE="Claude is busy with a turn. Try again when it ends."
# The held note for the fixture's impact, as python source. The second name carries a bidi override,
# which the note shows as U+202E between angle brackets (written here as escapes); it never carries it raw.
HELD_NOTE='"Reloading now changes Claude\x27s tools, which resets the prompt cache: the next turn costs more. Adds: plugin:pa:m, plugin:pa:⟨U+202E⟩x. Removes: old-server. May add the LSP tool."'
# The nth reload_plugins after a mark, as "hold=<v> busy=<v>": the two flags' values, "absent" for a key
# the request does not carry.
reload_flags() { # mark n
  python3 -c '
import json, sys
mark, nth, seen = int(sys.argv[2]), int(sys.argv[3]), 0
try:
    lines = open(sys.argv[1], encoding="utf-8").read().splitlines()
except FileNotFoundError:
    lines = []
for line in lines[mark:]:
    request = json.loads(line)
    if request.get("subtype") == "reload_plugins":
        seen += 1
        if seen == nth:
            def show(key):
                return json.dumps(request[key]) if key in request else "absent"
            print("hold=%s busy=%s" % (show("hold_on_cache_impact"), show("refuse_if_busy")))
            break
else:
    print("none")' "$RUN_DIR/fixture-control.log" "$1" "$2"
}
# Waits until COUNT such requests came after the mark (the CLI answers a request a moment after the host sends it).
wait_since() { # mark subtype count label
  local i got
  for i in $(seq 1 100); do
    got=$(control_since "$1" "$2")
    [ "$got" = "$3" ] && return 0
    sleep 0.2
  done
  fail "$4: the CLI heard $got $2 requests after the mark, not $3"
}
reload_chat() { # name
  switch_chat "$1"
  wait_panel "the reload row reads Reload" "R('connectors', '$RELOAD') and R('connectors', '$RELOAD')['label'] == 'Plugins and skills' and R('connectors', '$RELOAD')['action'] == 'Reload' and R('connectors', '$RELOAD')['note'] == ''" 10
}
reload_idle_row() { # label
  wait_panel "$1" "R('connectors', '$RELOAD') and R('connectors', '$RELOAD')['action'] == 'Reload' and R('connectors', '$RELOAD')['note'] == ''" 20
}
reload_no_toggle() { # mark
  expect_since "$1" mcp_toggle 0 "a reload sent no toggle"
}

scenario_reload_plugins_and_skills() {
  reload_chat reload_plugins_and_skills
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the plugin server is listed under Plugins" "R('connectors', '$PLUGIN_SERVER') is not None and R('connectors', 'connectors/plugins') is not None" 20
  reload_idle_row "the row is back to Reload with no note"
  wait_panel "the plugin server reads connected once the backoff has asked again" "R('connectors', '$PLUGIN_SERVER')['detail'] == 'connected'" 30
  expect_since "$mark" reload_plugins 1 "one plugin reload"
  expect_since "$mark" reload_skills 1 "one skill reload"
  [ "$(control_after_first "$mark" reload_plugins reload_skills)" = 1 ] || fail "reload_skills did not follow reload_plugins"
  [ "$(reload_flags "$mark" 1)" = "hold=true busy=true" ] || fail "the first reload_plugins did not carry both flags: $(reload_flags "$mark" 1)"
  reload_no_toggle "$mark"
  echo "OK: reload_plugins_and_skills: plugins with the hold and the busy refusal, then skills; the plugin server joined the card"
  stop_app
}

scenario_reload_floor() {
  SIRIO_FIXTURE_FLOOR=1
  reload_chat reload_floor
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_since "$mark" reload_skills 1 "the floor's applied answer led to the skills"
  reload_idle_row "the row is back to Reload with no note"
  expect_since "$mark" reload_plugins 1 "one plugin reload"
  [ "$(control_after_first "$mark" reload_plugins reload_skills)" = 1 ] || fail "reload_skills did not follow reload_plugins"
  [ "$(reload_flags "$mark" 1)" = "hold=true busy=absent" ] || fail "the floor's reload_plugins carried the wrong flags: $(reload_flags "$mark" 1)"
  reload_no_toggle "$mark"
  echo "OK: reload_floor: the hold without the busy refusal, and an answer with no held key is applied"
  stop_app
}

scenario_reload_held() {
  SIRIO_FIXTURE_RELOAD=held
  reload_chat reload_held
  local mark
  mark=$(control_log_lines)
  echo "step: the first Reload is held"
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the note is the cost warning and the action is Reload anyway" \
    "R('connectors', '$RELOAD')['note'] == $HELD_NOTE and R('connectors', '$RELOAD')['action'] == 'Reload anyway'" 20
  check_panel "the note shows the bidi override as text and never carries it" \
    "'\u202e' not in R('connectors', '$RELOAD')['note'] and 'U+202E' in R('connectors', '$RELOAD')['note']"
  fold_cards environment
  capture reload-held
  sleep 1
  expect_since "$mark" reload_skills 0 "a held answer sends no skill reload"
  echo "step: Reload anyway"
  panel_ctl activate "$RELOAD" >/dev/null
  reload_idle_row "the note is empty and the action is Reload"
  wait_panel "the plugin server's row label shows the bidi override as text" \
    "any(r['label'] == 'plugin:pa:⟨U+202E⟩x' for r in rows('connectors'))" 15
  expect_since "$mark" reload_plugins 2 "the held one and the one anyway"
  [ "$(reload_flags "$mark" 1)" = "hold=true busy=true" ] || fail "the first reload_plugins did not carry the hold: $(reload_flags "$mark" 1)"
  [ "$(reload_flags "$mark" 2)" = "hold=absent busy=true" ] || fail "Reload anyway still carried the hold: $(reload_flags "$mark" 2)"
  wait_since "$mark" reload_skills 1 "Reload anyway led to the skills"
  [ "$(control_after_first "$mark" reload_plugins reload_skills)" = 1 ] || fail "reload_skills did not follow reload_plugins"
  reload_no_toggle "$mark"
  echo "OK: reload_held: the cost warning and Reload anyway; the second request has no hold and is followed by the skills"
  stop_app
}

scenario_reload_held_then_busy() {
  SIRIO_FIXTURE_RELOAD=held
  SIRIO_FIXTURE_RESULT_GATE="$RUN_DIR/reload_held_then_busy-gate"
  rm -f "$SIRIO_FIXTURE_RESULT_GATE"
  reload_chat reload_held_then_busy
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the reload is held" "R('connectors', '$RELOAD')['note'] == $HELD_NOTE and R('connectors', '$RELOAD')['action'] == 'Reload anyway'" 20
  send_prompt hello
  wait_panel "the turn streams" 'c["status"] == "streaming"' 30
  panel_ctl activate "$RELOAD" >/dev/null || fail "a Reload anyway during a turn was refused rather than noted"
  wait_panel "the note is the cost warning, then the busy note" "R('connectors', '$RELOAD')['note'] == $HELD_NOTE + ' ' + '$BUSY_NOTE'" 10
  sleep 1
  expect_since "$mark" reload_plugins 1 "the busy click sent no second reload"
  : >"$SIRIO_FIXTURE_RESULT_GATE"
  wait_panel "the turn is over and the note is the cost warning alone, still offering Reload anyway" \
    "c['status'] != 'streaming' and R('connectors', '$RELOAD')['note'] == $HELD_NOTE and R('connectors', '$RELOAD')['action'] == 'Reload anyway'" 30
  expect_since "$mark" reload_skills 0 "no skill reload while held"
  reload_no_toggle "$mark"
  echo "OK: reload_held_then_busy: a turn adds the busy note to the warning, and its end takes it away"
  stop_app
}

scenario_reload_busy_refusal() {
  SIRIO_FIXTURE_RELOAD=busy
  reload_chat reload_busy_refusal
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the note is the busy note and the action stays Reload" "R('connectors', '$RELOAD')['note'] == '$BUSY_NOTE' and R('connectors', '$RELOAD')['action'] == 'Reload'" 20
  sleep 1
  expect_since "$mark" reload_plugins 1 "one plugin reload, refused as busy"
  expect_since "$mark" reload_skills 0 "a busy refusal sends no skill reload"
  reload_no_toggle "$mark"
  echo "OK: reload_busy_refusal: the CLI's busy refusal is the busy note and sends nothing more"
  stop_app
}

scenario_reload_busy_cli() {
  SIRIO_FIXTURE_CLI_BUSY_GATE="$RUN_DIR/reload_busy_cli-gate"
  rm -f "$SIRIO_FIXTURE_CLI_BUSY_GATE"
  reload_chat reload_busy_cli
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the note is the busy note" "R('connectors', '$RELOAD')['note'] == '$BUSY_NOTE'" 10
  sleep 1
  expect_since "$mark" reload_plugins 0 "a turn the CLI started sent no reload"
  : >"$SIRIO_FIXTURE_CLI_BUSY_GATE"
  wait_panel "the note clears when the CLI goes idle" "R('connectors', '$RELOAD')['note'] == ''" 20
  panel_ctl activate "$RELOAD" >/dev/null
  wait_since "$mark" reload_plugins 1 "the reload after the CLI went idle"
  reload_idle_row "the row is back to Reload"
  reload_no_toggle "$mark"
  echo "OK: reload_busy_cli: a turn the CLI started is busy; its end clears the note and the reload goes out"
  stop_app
}

scenario_reload_busy_turn() {
  SIRIO_FIXTURE_RESULT_GATE="$RUN_DIR/reload_busy_turn-gate"
  rm -f "$SIRIO_FIXTURE_RESULT_GATE"
  reload_chat reload_busy_turn
  local mark
  mark=$(control_log_lines)
  send_prompt hello
  wait_panel "the turn streams" 'c["status"] == "streaming"' 30
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the note is the busy note" "R('connectors', '$RELOAD')['note'] == '$BUSY_NOTE'" 10
  sleep 1
  expect_since "$mark" reload_plugins 0 "a turn's reload sent nothing"
  : >"$SIRIO_FIXTURE_RESULT_GATE"
  wait_panel "the turn is over and the note clears" "c['status'] != 'streaming' and R('connectors', '$RELOAD')['note'] == ''" 30
  reload_no_toggle "$mark"
  echo "OK: reload_busy_turn: a streaming turn is busy; its end clears the note"
  stop_app
}

scenario_reload_busy_before_skills() {
  SIRIO_FIXTURE_RELOAD_GATE="$RUN_DIR/reload_busy_before_skills-plugins-gate"
  SIRIO_FIXTURE_RESULT_GATE="$RUN_DIR/reload_busy_before_skills-result-gate"
  rm -f "$SIRIO_FIXTURE_RELOAD_GATE" "$SIRIO_FIXTURE_RESULT_GATE"
  reload_chat reload_busy_before_skills
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_since "$mark" reload_plugins 1 "the plugin reload is at its gate"
  send_prompt hello
  wait_panel "the turn streams" 'c["status"] == "streaming"' 30
  : >"$SIRIO_FIXTURE_RELOAD_GATE"
  wait_panel "the note is the busy note" "R('connectors', '$RELOAD')['note'] == '$BUSY_NOTE'" 20
  sleep 1
  expect_since "$mark" reload_skills 0 "a turn that began during the reload stopped the skills"
  : >"$SIRIO_FIXTURE_RESULT_GATE"
  wait_panel "the turn is over and the note clears" "c['status'] != 'streaming' and R('connectors', '$RELOAD')['note'] == ''" 30
  reload_no_toggle "$mark"
  echo "OK: reload_busy_before_skills: the busy gate is read again before the skills"
  stop_app
}

scenario_reload_in_flight() {
  SIRIO_FIXTURE_RELOAD_GATE="$RUN_DIR/reload_in_flight-gate"
  rm -f "$SIRIO_FIXTURE_RELOAD_GATE"
  reload_chat reload_in_flight
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the row reads reloading… with no action" "R('connectors', '$RELOAD')['detail'] == 'reloading…' and R('connectors', '$RELOAD')['action'] is None" 10
  if panel_ctl activate "$RELOAD" >/dev/null 2>&1; then
    fail "a second Reload while one runs was accepted"
  fi
  sleep 1
  expect_since "$mark" reload_plugins 1 "the second Reload sent nothing"
  : >"$SIRIO_FIXTURE_RELOAD_GATE"
  reload_idle_row "the row is back to Reload"
  expect_since "$mark" reload_plugins 1 "one plugin reload in all"
  reload_no_toggle "$mark"
  echo "OK: reload_in_flight: reloading… while the call is open, a second Reload failed and sent nothing"
  stop_app
}

scenario_reload_refused() {
  SIRIO_FIXTURE_RELOAD=refused
  reload_chat reload_refused
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the reload row is gone" "R('connectors', '$RELOAD') is None" 20
  check_panel "the server rows are unchanged" \
    "sorted(r['key'] for r in rows('connectors') if r['depth'] == 1) == sorted(['$CODEGRAPH', '$NOTION']) and R('connectors', '$CODEGRAPH')['detail'] == 'failed' and R('connectors', '$NOTION')['detail'] == 'needs sign-in'"
  sleep 1
  expect_since "$mark" reload_plugins 1 "one refused plugin reload"
  expect_since "$mark" reload_skills 0 "a refusal sends no skill reload"
  reload_no_toggle "$mark"
  echo "OK: reload_refused: a refused reload_plugins hides the row and changes nothing else"
  stop_app
}

scenario_reload_skills_refused() {
  SIRIO_FIXTURE_SKILLS=refused
  reload_chat reload_skills_refused
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_since "$mark" reload_skills 1 "the applied plugin reload led to the skills"
  wait_panel "the reload row is gone" "R('connectors', '$RELOAD') is None" 20
  expect_since "$mark" reload_plugins 1 "one plugin reload"
  [ "$(control_after_first "$mark" reload_plugins reload_skills)" = 1 ] || fail "reload_skills did not follow reload_plugins"
  check_panel "the server rows are unchanged but for the plugin's" \
    "set(r['key'] for r in rows('connectors') if r['depth'] == 1) - set(['$PLUGIN_SERVER', 'connectors/plugins/plugin:pa:\u202ex']) == set(['$CODEGRAPH', '$NOTION']) and R('connectors', '$CODEGRAPH')['detail'] == 'failed' and R('connectors', '$NOTION')['detail'] == 'needs sign-in'"
  sleep 1
  expect_since "$mark" reload_skills 1 "a refused skill reload is not sent again"
  reload_no_toggle "$mark"
  echo "OK: reload_skills_refused: a refused reload_skills hides the row after the plugins applied"
  stop_app
}

scenario_reload_errors() {
  SIRIO_FIXTURE_RELOAD=errors
  reload_chat reload_errors
  local mark
  mark=$(control_log_lines)
  panel_ctl activate "$RELOAD" >/dev/null
  wait_panel "the note counts the plugins that failed to load" "R('connectors', '$RELOAD')['note'] == '2 plugins failed to load.'" 20
  check_panel "the action is Reload" "R('connectors', '$RELOAD')['action'] == 'Reload'"
  reload_no_toggle "$mark"
  echo "OK: reload_errors: an applied reload with plugin load errors says how many"
  stop_app
}

scenario_change_request() {
  chat_app change_request
  open_column
  unfold_cards change_request
  wait_panel "the card is #112" "R('change_request', 'change_request/card') and R('change_request', 'change_request/card')['label'].startswith('#112')"
  fold_cards environment
  capture change_request
  panel_ctl activate change_request/card >/dev/null
  wait_field "a change request tab for #112 is open" label '#112' 20 surface change-request read
  wait_field "and it has loaded" state loaded 20 surface change-request read
  local tabs
  tabs=$(reply surface tabs read)
  echo "  read: $tabs"
  case "$tabs" in *'change_request|'*) echo "OK: the tab list names a change_request tab" ;; *) fail "the tab list has no change_request tab" ;; esac
  capture change_request-tab
  stop_app
}

scenario_rate_limit_shared() {
  chat_app rate_limit_shared
  echo "step: the right panel's list is rate limited, and pauses the host"
  forge_post "/__ratelimit?seconds=40"
  ctl surface change-requests show >/dev/null
  ctl surface change-requests filter mine >/dev/null
  for _ in $(seq 1 100); do
    [ "$(reply surface change-requests read | python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get("state", ""))')" = error ] && break
    sleep 0.3
  done
  local before
  before=$(card_reads)
  echo "step: the column opens during the pause (card reads so far: $before)"
  panel_ctl open >/dev/null
  wait_panel "the column is open" 'p["open"]'
  unfold_cards change_request
  wait_panel "the card row says it is rate limited" \
    "'rate limited' in (R('change_request', 'change_request/error') or {}).get('label', '').lower()" 10
  capture rate-limited
  sleep 2
  [ "$(card_reads)" = "$before" ] || fail "the forge saw a read of the branch's change request during the pause ($(card_reads) > $before)"
  echo "OK: the forge saw no ChangeRequestForBranch or ChangeRequestByNumber during the pause"
  forge_post /__reset
  stop_app
}

scenario_restore() {
  chat_app restore
  open_column
  # A saved layout is written shortly after a change; a quit flushes it as well.
  sleep 1
  local first_surface=$SURFACE
  stop_app
  echo "step: Sirio starts again on the same database"
  launch restore
  SURFACE=""
  for _ in $(seq 1 100); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "the chat was not restored"
  echo "OK: the restored chat is $SURFACE (was $first_surface)"
  wait_panel "the restored chat's column is open" 'p["open"]'
  if [ -n "$DISPLAY_TARGET" ]; then
    wait_panel "and drawn" 'p["drawn"]'
  fi
  wait_panel "and its Environment is drawn again" "R('environment', 'environment.project') is not None"
  capture restore
  stop_app
}

# ---- the trace scenarios -----------------------------------------------------------
# The latest turn is Done at $0.31: the summary of the card, and the turn's own number.
DONE_031='re.match(r"^Done · .+ · \$0\.31$", S("trace")["summary"])'

scenario_trace_live() {
  trace_chat trace_live
  send_prompt subagents
  wait_panel "the Trace card appears with a live turn" 'S("trace") is not None and S("trace")["summary"].startswith("Live · ")' 30
  check_panel "the card starts folded" 'S("trace")["expanded"] is False'
  unfold_cards trace
  wait_panel "the lanes are Claude Code, two agents and the third one of them started (depth 2)" \
    '{k: (v["label"], v["depth"]) for k, v in lanes_of().items()} == {"root": ("Claude Code", 0), "toolu_agent_a": ("Explore", 1), "toolu_agent_b": ("Plan", 1), "toolu_agent_c": ("general-purpose", 2)}' 30
  wait_panel "the Edit asks: the title, the root lane and one wait row say the turn waits for the user" \
    'S("trace")["summary"].startswith("Waiting for you · ") and lane("root")["tag"] == "waiting" and len(wait_rows()) == 1' 30
  local wait_key permission
  wait_key=$(find_panel "the wait row" 'wait_rows()[0]["key"]')
  permission=$(find_panel "the transcript's pending Permission entry" 'str(pending()[0][0])')
  check_panel "the wait row and the Permission entry are the same request" "pending()[0][1] == '${wait_key#trace/wait/}' and pending()[0][0] > 0"
  capture trace-live
  echo "step: the root reveals the user's message, then the wait reveals the Permission entry ($permission)"
  panel_ctl activate trace/lane/root >/dev/null
  wait_panel "the root lane reveals the turn's user message" 'c["first"] == next(i for i, r in enumerate(c["transcript"]) if r.get("kind") == "user")' 10
  # The fixture's long first reply makes the transcript taller than a window, so with the list
  # at the user's message the card is below the last row the list drew (unpainted nothing is
  # drawn, and the last row is the first).
  check_panel "with the list at the user's message the Permission entry is below its last row" "c['last'] < $permission"
  panel_ctl activate "$wait_key" >/dev/null
  # The Permission card is the transcript's last entry while it waits, so painted it cannot be the
  # list's top: gpui backs the top up from the end of the content until the pane is full. Painted,
  # the reveal is read as the card on screen: between the first and last row the list drew, the
  # list moved there from the user's message. Unpainted the list's top sits at the card.
  wait_panel "the wait reveals the Permission entry" "c['first'] == $permission or (p['drawn'] and c['first'] <= $permission <= c['last'])" 10
  echo "step: the user allows the Edit"
  E2E rpc surface.chat.permission surfaceId="$SURFACE" requestId="${wait_key#trace/wait/}" optionId=allow_once >/dev/null || fail "allowing the Edit failed"
  wait_panel "the turn is Done at \$0.31" "$DONE_031" 40
  check_panel "no wait is left, the root lane is done and the failed Bash is the one error" \
    'len(wait_rows()) == 0 and lane("root")["tag"] == "done" and R("trace", "trace/stat/errors")["detail"] == "1" and R("trace", "trace/stat/cost")["detail"] == "$0.31"'
  check_panel "the numbers are the script's: 11 model calls, 5 tool calls, the root's own 8 calls" \
    'R("trace", "trace/stat/llm_calls")["detail"] == "11" and R("trace", "trace/stat/tool_calls")["detail"] == "5" and re.match(r"^8 calls · ", lane("root")["detail"])'
  echo "step: the tab holds each call: the Bash that failed, then the one that worked"
  panel_ctl activate trace/open >/dev/null
  wait_panel "the Trace tab is open" 'tab()["open"] == "1"' 20
  check_panel "the first Bash failed and ended, the second is done, in that order" \
    'T("toolu_bash_fail")["state"] == "failed" and T("toolu_bash_fail")["ended"] == "1" and T("toolu_bash_ok")["state"] == "done" and [s["id"] for s in tab()["spans"] if s["id"] in ("toolu_bash_fail", "toolu_bash_ok")] == ["toolu_bash_fail", "toolu_bash_ok"]'
  stop_app
}

scenario_trace_cost() {
  trace_chat trace_cost 30
  send_prompt subagents
  answer_permission allow_once
  wait_panel "the first turn is Done at \$0.31" "$DONE_031" 40
  send_prompt second
  wait_panel "the second turn's cost is the difference of the process's totals: \$0.12" \
    're.match(r"^Done · .+ · \$0\.12$", S("trace")["summary"])' 30
  unfold_cards trace
  check_panel "the Cost number says \$0.12 too" 'R("trace", "trace/stat/cost")["detail"] == "$0.12"'
  stop_app
}

scenario_trace_request() {
  # The fixture holds the first token back for four steps: slow enough to open the tab in it.
  trace_chat trace_request 2500
  send_prompt request
  wait_panel "the turn is live" 'S("trace") is not None and S("trace")["summary"].startswith("Live · ")' 30
  unfold_cards trace
  panel_ctl activate trace/open >/dev/null
  wait_panel "the Trace tab is open" 'tab()["open"] == "1"' 20
  check_panel "the request opened one running call under the root, before any token came" \
    '[(s["parent"], s["state"], s["ended"]) for s in tab()["spans"] if s["kind"] == "llm" and s["id"].startswith("call-")] == [("root", "running", "0")] and len([s for s in tab()["spans"] if s["kind"] == "llm"]) == 1'
  CALL=$(find_panel "the call's id" 'next(s["id"] for s in tab()["spans"] if s["kind"] == "llm")' 10)
  wait_panel "the turn is Done" 'S("trace")["summary"].startswith("Done · ")' 40
  check_panel "the turn still has that one call, the same id, ended and done: its first token opened no second" \
    "[(s['id'], s['parent'], s['state'], s['ended']) for s in tab()['spans'] if s['kind'] == 'llm'] == [('$CALL', 'root', 'done', '1')]"
  stop_app
}

# The root lane's own model calls: the main thread's, never a turn the CLI started on its own.
ROOT_CALLS='len([s for s in tab()["spans"] if s["kind"] == "llm" and s["parent"] == "root"])'
BG_COST='R("trace", "trace/stat/cost")["detail"]'
# A background agent runs, with a model call and the tools it started under it.
bg_agent_running() { # agent
  echo "T('$1') is not None and T('$1')['kind'] == 'subagent' and T('$1')['state'] == 'running' and T('$1')['ended'] == '0' and T('$1_bash')['parent'] == '$1' and T('$1_read')['parent'] == '$1' and any(s['kind'] == 'llm' and s['parent'] == '$1' for s in tab()['spans'])"
}

scenario_trace_background() {
  # The fixture holds each state for twenty steps (ten seconds here); a read is under a second.
  trace_chat trace_background 500
  send_prompt background
  wait_panel "the turn is live" 'S("trace") is not None and S("trace")["summary"].startswith("Live · ")' 30
  unfold_cards trace
  panel_ctl activate trace/open >/dev/null
  wait_panel "the Trace tab is open" 'tab()["open"] == "1"' 20
  echo "step: the main thread's result has come; the agents work on"
  wait_panel "both agents have children, drawn after the main thread's result" \
    'T("toolu_bg_a_read") is not None and T("toolu_bg_b_read") is not None' 30
  check_panel "Live, the cost at least \$1.00 and not exact; both agents running with a call and tools; the root lane has the main thread's two calls only" \
    "S('trace')['summary'].startswith('Live · ') and $BG_COST == '≥ \$1.00' and $(bg_agent_running toolu_bg_a) and $(bg_agent_running toolu_bg_b) and $ROOT_CALLS == 2"
  echo "step: agent A ends and the CLI reacts in a turn of its own (\$0.20) while B still runs"
  wait_panel "agent A is done and ended" 'T("toolu_bg_a")["state"] == "done" and T("toolu_bg_a")["ended"] == "1"' 30
  # The reaction's result comes about a second after A's end (two steps), and the
  # fixture holds B for twenty: a read four seconds on is after the result and before B's end.
  sleep 4
  check_panel "still Live with B running, the cost still at least \$1.00 and never an exact figure, and A's reaction drew no call under the root" \
    "S('trace')['summary'].startswith('Live · ') and T('toolu_bg_b')['state'] == 'running' and T('toolu_bg_b')['ended'] == '0' and $BG_COST == '≥ \$1.00' and $ROOT_CALLS == 2"
  echo "step: agent B ends, the CLI's reaction to it has not come yet"
  wait_panel "the last agent ended: the turn is Done, its cost still at least \$1.20" \
    're.match(r"^Done · .+ · ≥ \$1\.20$", S("trace")["summary"])' 30
  check_panel "both agents are done and ended" \
    'all(T(a)["state"] == "done" and T(a)["ended"] == "1" for a in ("toolu_bg_a", "toolu_bg_b"))'
  echo "step: the CLI's reaction to B"
  wait_panel "the cost is final, \$1.30: the reaction turns' deltas are in it" \
    're.match(r"^Done · .+ · \$1\.30$", S("trace")["summary"]) and R("trace", "trace/stat/cost")["detail"] == "$1.30"' 30
  check_panel "the root lane still has the main thread's two calls and nothing the CLI started, and no span is left running" \
    "$ROOT_CALLS == 2 and all(s['state'] != 'running' and s['ended'] == '1' for s in tab()['spans'])"
  stop_app
}

scenario_trace_background_prompt() {
  trace_chat trace_background_prompt 500
  send_prompt bgprompt
  wait_panel "the turn is Live after the main thread's result: its agent runs" \
    'S("trace") is not None and S("trace")["summary"].startswith("Live · ") and c["status"] in ("idle", "completed")' 30
  unfold_cards trace
  echo "step: a second prompt while the agent runs"
  send_prompt second
  wait_panel "the second turn is Done, and its cost is unknown: the agent's spend is mixed into its result" \
    'S("trace")["summary"].startswith("Done · ") and "$" not in S("trace")["summary"] and R("trace", "trace/stat/cost")["detail"] == "—"' 30
  panel_ctl activate trace/open >/dev/null
  wait_panel "the Trace tab is open on the second turn" 'tab()["open"] == "1" and tab()["turn"] == "2" and tab()["turns"] == "2"' 20
  check_panel "the second turn has a root model call, and none of the agent's lines landed in it" \
    "$ROOT_CALLS >= 1 and not any(s['kind'] == 'subagent' or s['id'].startswith('toolu_bp') for s in tab()['spans'])"
  # The fixture ends the agent twenty steps (ten seconds) after the second turn's result.
  echo "step: the agent ends; the second turn does not change"
  sleep 15
  check_panel "the second turn is as it was: Done, cost unknown, no agent span in it" \
    "S('trace')['summary'].startswith('Done · ') and '\$' not in S('trace')['summary'] and $BG_COST == '—' and not any(s['kind'] == 'subagent' or s['id'].startswith('toolu_bp') for s in tab()['spans'])"
  stop_app
}

scenario_trace_interrupt() {
  trace_chat trace_interrupt
  send_prompt cancel
  wait_panel "the turn is live" 'S("trace") is not None and S("trace")["summary"].startswith("Live · ")' 30
  echo "step: Stop"
  E2E rpc surface.chat.stop surfaceId="$SURFACE" >/dev/null || fail "stopping the chat failed"
  wait_panel "Stop ends the turn Interrupted" 'S("trace")["summary"].startswith("Interrupted · ")' 30
  unfold_cards trace
  check_panel "an interrupted turn's cost is unknown" 'R("trace", "trace/stat/cost")["detail"] == "—" and lane("root")["tag"] == "interrupted"'
  send_prompt second
  wait_panel "the next turn is Done, and its cost is unknown too" \
    'S("trace")["summary"].startswith("Done · ") and "$" not in S("trace")["summary"] and R("trace", "trace/stat/cost")["detail"] == "—"' 30
  stop_app
}

scenario_trace_death() {
  trace_chat trace_death
  send_prompt die
  wait_panel "the CLI dying mid-call ends the turn Failed" 'S("trace") is not None and S("trace")["summary"].startswith("Failed · ")' 40
  unfold_cards trace
  wait_panel "the root and the agent it was waiting on are lanes" 'len(lanes_of()) == 2' 10
  check_panel "every lane is failed or interrupted" 'all(v["tag"] in ("failed", "interrupted") for v in lanes_of().values())'
  stop_app
}

scenario_trace_hang() {
  trace_chat trace_hang
  send_prompt hang
  wait_panel "the turn is Done though its call never reported" 'S("trace") is not None and S("trace")["summary"].startswith("Done · ")' 40
  unfold_cards trace
  panel_ctl activate trace/open >/dev/null
  wait_panel "the Trace tab is open" 'tab()["open"] == "1"' 20
  check_panel "the call that never reported is interrupted, and ended" \
    'T("toolu_hang") is not None and T("toolu_hang")["state"] == "interrupted" and T("toolu_hang")["ended"] == "1"'
  stop_app
}

# One subagent's request: DECISION is the option to answer with, AGENT the Agent call that
# owns the subagent and TOOL the call it asks about.
subagent_permission() { # prompt agent tool
  local prompt=$1 agent=$2 tool=$3 before
  before=$(find_panel "the Permission cards the transcript has so far" 'str(len([r for r in c["transcript"] if r.get("kind") == "permission"]))' 10)
  send_prompt "$prompt"
  wait_panel "the Trace card appears" 'S("trace") is not None' 30
  ensure_unfolded trace
  wait_panel "the subagent's lane waits for the user, with one wait row" \
    "lane('$agent') is not None and lane('$agent')['tag'] == 'waiting' and len(wait_rows()) == 1" 30
  check_panel "the transcript has exactly one more Permission entry and no row for the subagent's call" \
    "len([r for r in c['transcript'] if r.get('kind') == 'permission']) == $before + 1 and row_index('$tool') is None"
  answer_permission reject_once
  wait_panel "the turn is over, nothing waits and no card is pending" \
    'c["status"] == "completed" and len(wait_rows()) == 0 and len(pending()) == 0 and S("trace")["summary"].startswith("Done · ")' 40
  check_panel "no transcript row is left pending, and the call still has no row" \
    "all(str(r.get('status', '')).lower() != 'pending' for r in c['transcript']) and row_index('$tool') is None and len([r for r in c['transcript'] if r.get('kind') == 'permission']) == $before + 1"
}

scenario_trace_subagent_permission() {
  trace_chat trace_subagent_permission
  echo "step: the request comes after the line that holds its call"
  subagent_permission subperm toolu_agent_p toolu_sub_p
  echo "step: the request comes before that line (the order seen live)"
  subagent_permission subperm_early toolu_agent_pe toolu_sub_pe
  stop_app
}

scenario_trace_reveal() {
  trace_chat trace_reveal 30
  send_prompt subagents
  answer_permission allow_once
  wait_panel "the turn is Done at \$0.31" "$DONE_031" 40
  unfold_cards trace
  echo "step: each agent's lane reveals its Agent row (at the index the transcript draws it), the root the user's message"
  local user_row
  user_row=$(find_panel "the user's message" 'str(next(i for i, r in enumerate(c["transcript"]) if r.get("kind") == "user"))' 10)
  check_panel "the two Agent rows are rows of their own, below the user's message" \
    'row_index("toolu_agent_a") is not None and drawn_index("toolu_agent_a") == row_index("toolu_agent_a") and drawn_index("toolu_agent_b") == row_index("toolu_agent_b") and row_index("toolu_agent_a") < row_index("toolu_agent_b")'
  panel_ctl activate trace/lane/root >/dev/null
  wait_panel "the root lane reveals the user's message" "c['first'] == $user_row" 10
  panel_ctl activate trace/lane/toolu_agent_b >/dev/null
  wait_panel "the Plan lane reveals the Plan Agent row" 'c["first"] == drawn_index("toolu_agent_b")' 10
  panel_ctl activate trace/lane/toolu_agent_a >/dev/null
  wait_panel "the Explore lane reveals the Explore Agent row, which is above" \
    'c["first"] == drawn_index("toolu_agent_a") and drawn_index("toolu_agent_a") < drawn_index("toolu_agent_b")' 10
  stop_app
}

scenario_trace_tab() {
  trace_chat trace_tab 30
  send_prompt subagents
  answer_permission allow_once
  wait_panel "the turn is Done at \$0.31" "$DONE_031" 40
  unfold_cards trace
  expect_trace_tabs "no trace tab before one was opened" 0
  echo "step: the card opens the tab"
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the layout lists a trace tab in the Secondary half" present 20
  wait_panel "the tab reads turn 1 of 1, following the latest, with no span left out" 'tab()["open"] == "1" and tab()["turn"] == "1" and tab()["turns"] == "1" and tab()["follow"] == "1" and tab()["dropped"] == "0"' 20
  check_panel "the tab holds the whole tree, each span under its parent" \
    '{s["id"]: (s["parent"], s["kind"], s["depth"]) for s in tab()["spans"] if s["kind"] in ("root", "subagent", "tool")} == {"root": ("", "root", "0"), "toolu_agent_a": ("root", "subagent", "1"), "toolu_agent_b": ("root", "subagent", "1"), "toolu_agent_c": ("toolu_agent_b", "subagent", "2"), "toolu_a_bash": ("toolu_agent_a", "tool", "2"), "toolu_c_read": ("toolu_agent_c", "tool", "3"), "toolu_edit": ("root", "tool", "1"), "toolu_bash_fail": ("root", "tool", "1"), "toolu_bash_ok": ("root", "tool", "1")}'
  check_panel "it also holds the model calls, each under the agent that made it" \
    '{s["id"]: s["parent"] for s in tab()["spans"] if s["kind"] == "llm"} == {"msg_main_1": "root", "msg_main_2": "root", "msg_main_3": "root", "msg_main_4": "root", "msg_main_5": "root", "msg_a_1": "toolu_agent_a", "toolu_agent_a/final": "toolu_agent_a", "msg_b_1": "toolu_agent_b", "toolu_agent_b/final": "toolu_agent_b", "msg_c_1": "toolu_agent_c", "toolu_agent_c/final": "toolu_agent_c"}'
  capture trace-tab
  echo "step: a second open keeps one tab"
  panel_ctl activate trace/open >/dev/null
  sleep 1
  expect_trace_tabs "one trace tab after two opens" 1
  echo "step: another worktree, and back"
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  sleep 1
  ctl select-workspace --workspace "$LINKED" >/dev/null
  wait_trace_tab "the trace tab is still there after a worktree switch" present 20
  wait_panel "and still reads the chat's turn" 'tab()["open"] == "1" and tab()["turns"] == "1"' 20
  echo "step: Sirio quits and starts again"
  sleep 1
  local first_surface=$SURFACE
  stop_app
  launch trace_tab
  SURFACE=""
  for _ in $(seq 1 100); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "the chat was not restored"
  echo "OK: the restored chat is $SURFACE (was $first_surface)"
  sleep 3
  expect_trace_tabs "the relaunch restored no trace tab" 0
  check_panel "no trace tab came back, and the chat has no trace of the turn before" 'tab()["open"] == "0" and S("trace") is None'
  echo "OK: no trace tab is restored"
  echo "step: a turn after the relaunch gives the chat a trace again; closing the chat closes its tab"
  send_prompt second
  wait_panel "the turn is Done" 'S("trace") is not None and S("trace")["summary"].startswith("Done · ")' 40
  unfold_cards trace
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the trace tab is open again" present 20
  local position
  position=$(tab_positions chat)
  [ -n "$position" ] || fail "the chat tab is not in the tab list"
  ctl surface tabs close "$position" --confirm >/dev/null
  wait_trace_tab "closing the chat closes its trace tab" absent 20
  stop_app
}

scenario_trace_idle() {
  PERF_TRACE="$RUN_DIR/trace_idle-perf.tsv"
  trace_chat trace_idle
  [ -s "$PERF_TRACE" ] || fail "the app wrote no performance trace at $PERF_TRACE"
  send_prompt cancel
  wait_panel "the turn is live and will stay so" 'S("trace") is not None and S("trace")["summary"].startswith("Live · ")' 30
  unfold_cards trace
  sleep 1
  frames_in_window "the card unfolded and drawn, mid-turn (the control: the animation does ask)" 3 some
  local card_entity=$LAST_ENTITIES
  [ -n "$card_entity" ] && [[ "$card_entity" != *" "* ]] || fail "the card's frames come from [$card_entity], not one owner"
  fold_cards trace
  sleep 1
  frames_in_window "the card folded and no tab, mid-turn" 3 zero
  unfold_cards trace
  echo "step: the tab opens and is selected: the chat's tab is hidden"
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the trace tab is open" present 20
  sleep 1
  frames_in_window "the tab drawn, the chat's card hidden behind it, mid-turn (the control for the tab's loop)" 3 some
  local tab_entity=$LAST_ENTITIES
  [ -n "$tab_entity" ] && [[ "$tab_entity" != *" "* ]] && [ "$tab_entity" != "$card_entity" ] \
    || fail "the tab's frames come from [$tab_entity], and the card's owner is [$card_entity]: the hidden chat asked for frames, or the owners are not told apart"
  echo "step: the chat's tab is selected again: the trace tab is hidden"
  ctl surface tabs select "$(tab_positions chat)" >/dev/null
  sleep 1
  frames_in_window "the card drawn, the trace tab hidden behind the chat, mid-turn" 3 some
  [ "$LAST_ENTITIES" = "$card_entity" ] || fail "with the tab hidden the frames came from [$LAST_ENTITIES], not from the card alone [$card_entity]"
  echo "step: the trace tab open, and a third tab selected in the Secondary half"
  ctl surface open-list open --entry changes >/dev/null
  local kinds=""
  for _ in $(seq 1 60); do
    kinds=$(secondary_kinds || true)
    case ",$kinds," in *,diff,*) break ;; esac
    sleep 0.3
  done
  # The Changes tab's kind is `diff`.
  case ",$kinds," in *,diff,*) ;; *) fail "the Changes tab did not open in the Secondary half beside the chat and the trace: [$kinds]" ;; esac
  echo "OK: the Secondary half holds [$kinds], the Changes tab selected"
  sleep 1
  frames_in_window "the trace tab open but another tab selected beside it: neither the card nor the tab is drawn, mid-turn" 3 zero
  ctl surface tabs select "$(tab_positions chat)" >/dev/null
  sleep 1
  echo "step: the worktree parked mid-turn, with the card unfolded and the tab open"
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  sleep 1
  frames_in_window "the card and the tab both open but their worktree parked, mid-turn" 3 zero
  ctl select-workspace --workspace "$LINKED" >/dev/null
  sleep 1
  ctl surface tabs select "$(tab_positions chat)" >/dev/null
  echo "step: the turn ends"
  E2E rpc surface.chat.stop surfaceId="$SURFACE" >/dev/null || fail "stopping the chat failed"
  wait_panel "the chat is no longer streaming and the turn is over" 'c["status"] != "streaming" and S("trace")["summary"].startswith("Interrupted · ")' 30
  sleep 1
  frames_in_window "the turn over, the card drawn" 3 zero
  ctl surface tabs select "$(tab_positions trace)" >/dev/null
  sleep 1
  frames_in_window "the turn over, the tab drawn" 3 zero
  cp "$PERF_TRACE" "$OUT_DIR/" 2>/dev/null || true
  stop_app
}

# The tab's keys and mouse, with real X events (xdotool): a click on a row selects it and
# reveals its Agent row, Enter on the selected row reveals it again, and a click on the
# "Follow the latest turn" checkbox toggles it. The clicks are at pixels of the 2200x1000
# window's layout (PLAN_ROW_X/Y and FOLLOW_X/Y override them), so the scenario runs only
# framed at that size. Enter *while the checkbox has focus* is not reachable from a real
# keyboard in this build, and so is not asserted: the checkbox's mouse-down prevents the
# default, which keeps a click from focusing it, and the app binds no Tab to FocusNext.
click_at() { # x y
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove "$1" "$2" click 1 || fail "xdotool could not click at $1,$2"
  sleep 1
}
press_key() { # key
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool key "$1" || fail "xdotool could not press $1"
  sleep 1
}
# The screen pixel of the app's window's top-left corner: the control socket reports window pixels.
window_origin() { # prints "X Y"
  local window
  window=$(find_window)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$window" | awk '/Absolute upper-left X/ {x=$NF} /Absolute upper-left Y/ {y=$NF} END {print x, y}'
}
# A click at a window pixel ("X Y"), as the socket reports one.
click_at_window() { # "x y"
  local origin x y
  origin=$(window_origin)
  x=$(( ${1% *} + ${origin% *} ))
  y=$(( ${1#* } + ${origin#* } ))
  echo "  click: window pixel $1 is screen pixel $x $y" >&2
  click_at "$x" "$y"
}
scenario_trace_keys() {
  trace_chat trace_keys 30
  send_prompt subagents
  answer_permission allow_once
  wait_panel "the turn is Done at \$0.31" "$DONE_031" 40
  ensure_unfolded trace
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the trace tab is open" present 20
  wait_panel "the tab shows the turn, following the latest" 'tab()["open"] == "1" and tab()["follow"] == "1"' 20
  capture trace-keys-before
  echo "step: a real click on the Plan row reveals its Agent row"
  panel_ctl activate trace/lane/root >/dev/null
  wait_panel "the list is at the user's message" 'c["first"] == 0' 10
  click_at "${PLAN_ROW_X:-1157}" "${PLAN_ROW_Y:-323}"
  wait_panel "the click revealed the Plan Agent row" 'c["first"] == drawn_index("toolu_agent_b")' 10
  echo "step: the list goes back to the top, then a real Enter on the selected row reveals it again"
  panel_ctl activate trace/lane/root >/dev/null
  wait_panel "the list is at the user's message again" 'c["first"] == 0' 10
  press_key Return
  wait_panel "Enter on the selected row revealed the Plan Agent row" 'c["first"] == drawn_index("toolu_agent_b")' 10
  capture trace-keys-enter
  echo "step: a real click on the Follow checkbox toggles it, and another toggles it back"
  click_at "${FOLLOW_X:-1702}" "${FOLLOW_Y:-97}"
  wait_panel "the click toggled Follow off" 'tab()["follow"] == "0"' 10
  capture trace-keys-follow-off
  click_at "${FOLLOW_X:-1702}" "${FOLLOW_Y:-97}"
  wait_panel "the second click toggled Follow on" 'tab()["follow"] == "1"' 10
  stop_app
}

# A chat response's file links (spec 2026-10-24-chat-file-links-design.md §6), with real X
# input at the pixels `transcriptLinks` reports for the last paint. Opening a file selects its
# tab, which hides the chat, so each click is preceded by showing the chat tab again. The
# files are written into the worktree before the app starts; the stub's one answer names
# them, names one that is not there, a version, a fenced path and a web link.
# A row of `transcriptLinks` also carries `w`, the label's painted width: the second drag ends
# 14 px past the label's end (x already sits 4 px into the label, so x + w + 10), where plain
# text follows it on the line.
# The tabs: `surface tabs read` lists them (`count`, and `active` is the selected one's
# position); `surface editor state` reads the active tab's file, so it only answers while a
# file tab is selected. The steps that must open nothing therefore compare `count` and `active`
# with what they were, the chat tab being the one selected.
tabs_state() { # prints "COUNT ACTIVE"
  reply surface tabs read | python3 -c 'import json, sys; t = json.load(sys.stdin)[0]; print(t["count"], t["active"])'
}
tab_count() { tabs_state | cut -d' ' -f1; }
show_chat() {
  local position
  position=$(tab_positions chat)
  [ -n "$position" ] || fail "no chat tab to show"
  reply surface tabs select "$position" >/dev/null || fail "could not select the chat tab $position"
  sleep 1.5
}
# The pixel of a link of KIND and LABEL in the chat's last paint.
link_xy() { # kind label
  local xy
  xy=$(E2E link "$SURFACE" "$1" "$2") || fail "no $1 link '$2' to point at"
  echo "  link: $1 '$2' at $xy" >&2
  echo "$xy"
}
# A click on the link of KIND and LABEL, the chat shown first. The press and the release are
# 0.3 s apart, as a hand's are: a web link's text only listens for the release in the frame
# after the press, and this window paints in software, so an instant `click 1` outruns it.
click_link() { # kind label [modifiers...]
  local kind=$1 label=$2 xy
  shift 2
  show_chat
  xy=$(link_xy "$kind" "$label")
  if [ $# -gt 0 ]; then
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool keydown "$@" mousemove ${xy% *} ${xy#* } mousedown 1 sleep 0.3 mouseup 1 keyup "$@" || fail "xdotool could not click at $xy with $*"
  else
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove ${xy% *} ${xy#* } mousedown 1 sleep 0.3 mouseup 1 || fail "xdotool could not click at $xy"
  fi
  sleep 1
}
# A drag from (X1, Y1) to (X2, Y2): a press, two moves, a release.
drag_at() { # x1 y1 x2 y2
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool mousemove "$1" "$2" mousedown 1 sleep 0.3 \
    mousemove $((($1 + $3) / 2)) "$4" sleep 0.3 mousemove "$3" "$4" sleep 0.3 mouseup 1 \
    || fail "xdotool could not drag from $1,$2 to $3,$4"
  sleep 1
}
# The byte offset of the start of line N of FILE.
line_offset() { # file n
  python3 -c 'import sys; print(sum(len(l) for l in open(sys.argv[1], "rb").readlines()[:int(sys.argv[2]) - 1]))' "$1" "$2"
}
# Polls the active file tab until it is PATH, in MODE (any mode when it is empty) and with
# SELECTIONS, when given.
expect_editor() { # desc path mode [selections]
  local desc=$1 want_path=$2 want_mode=$3 want_selections=${4:-} out="" _
  for _ in $(seq 1 50); do
    out=$(reply surface editor state || true)
    if printf '%s' "$out" | python3 -c '
import json, sys
d = json.load(sys.stdin)[0]
ok = d.get("path") == sys.argv[1] and (not sys.argv[2] or d.get("mode") == sys.argv[2]) and d.get("state") == "loaded"
ok = ok and (not sys.argv[3] or d.get("selections") == sys.argv[3])
sys.exit(0 if ok else 1)' "$want_path" "$want_mode" "$want_selections" 2>/dev/null; then
      echo "  read: $(printf '%s' "$out" | python3 -c 'import json, sys; d = json.load(sys.stdin)[0]; print({k: d.get(k) for k in ("path", "mode", "state", "selections")})')"
      echo "OK: $desc"
      return 0
    fi
    sleep 0.3
  done
  echo "  read: $(printf '%s' "$out" | cut -c1-400)" >&2
  fail "$desc: the editor is not on $want_path in $want_mode${want_selections:+ with selections $want_selections}"
}
# Polls the active file tab until it is PATH and in the editor's missing-file state: `error`
# with the message `This file does not exist: PATH` (`Editor::load_message`).
expect_missing() { # desc path
  local desc=$1 want_path=$2 out="" _
  for _ in $(seq 1 50); do
    out=$(reply surface editor state || true)
    if printf '%s' "$out" | python3 -c '
import json, sys
d = json.load(sys.stdin)[0]
sys.exit(0 if d.get("path") == sys.argv[1] and d.get("state") == "error" and d.get("error") == "This file does not exist: " + sys.argv[1] else 1)' "$want_path" 2>/dev/null; then
      echo "OK: $desc"
      return 0
    fi
    sleep 0.3
  done
  echo "  read: $(printf '%s' "$out" | cut -c1-400)" >&2
  fail "$desc: the active tab is not $want_path in the missing-file state"
}
# The browser stub's log: how many calls it has recorded. gpui starts `xdg-open` without
# waiting for it, so a check that nothing was recorded lets it run first.
browser_calls() {
  sleep 1
  if [ -f "$RUN_DIR/browser.log" ]; then wc -l <"$RUN_DIR/browser.log"; else echo 0; fi
}
scenario_file_links() {
  use_stub file-links
  local docs_main="$LINKED/docs/NOME-FILE.md" docs_other="$LINKED/docs/ALTRO.md" main_rs="$LINKED/src/main.rs" readme="$LINKED/README.md"
  mkdir -p "$LINKED/docs"
  python3 - "$docs_main" "$docs_other" "$main_rs" "$readme" <<'PY'
import sys
docs_main, docs_other, main_rs, readme = sys.argv[1:]
for path in (docs_main, docs_other):
    open(path, "w").write("".join(f"Riga {n} di {path.rsplit('/', 1)[1]}\n" for n in range(1, 61)))
open(main_rs, "w").write("".join(f"// riga {n}\n" if n != 1 else "fn main() {}\n" for n in range(1, 61)))
open(readme, "w").write("# Widgets\n\nIl file che il progetto ha già.\n")
PY
  # What opens a URL is gpui's `xdg-open`, started in `sirio_env::for_child`'s PATH: the login
  # shell's (`$SHELL -lc`, run in this private HOME with the launch PATH, which it normally
  # extends) before the launch's. The harness's `xdg-open` stub leads both, and this private
  # `.bash_profile` guarantees that even if a system profile prepended directories, so any `xdg-open`
  # the app runs (a file:// as much as the web link) lands in $RUN_DIR/browser.log and never
  # starts a real browser on the display.
  stub_openers_lead_login_path file_links
  launch file_links
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  send_prompt "mostrami i file"
  wait_panel "the turn is over" 'c["status"] == "completed"' 40
  echo "step: the links of the response"
  E2E links-check "$SURFACE" 30 sito '/docs/NOME-FILE.md' 'src/main.rs:42' 'README.md' 'docs/NOME-FILE.md:30' 'docs/ALTRO.md:10' \
    || fail "the response's links are not the expected ones"
  capture file-links-response
  local before browser xy line _
  before=$(tab_count)
  [ "$before" = 1 ] || fail "the chat is not the only tab: $(tab_count) tabs"

  echo "step: a click on /docs/NOME-FILE.md opens the Markdown file in Preview"
  click_link file '/docs/NOME-FILE.md'
  expect_editor "/docs/NOME-FILE.md opened in Preview" "$docs_main" preview
  [ "$(tab_count)" = $((before + 1)) ] || fail "the click did not open exactly one more tab: $(tab_count) tabs, was $before"
  echo "OK: one more tab"

  echo "step: a click on src/main.rs:42 opens the file in code at its line 42"
  click_link file 'src/main.rs:42'
  expect_editor "src/main.rs:42 opened in code at line 42" "$main_rs" code "$(line_offset "$main_rs" 42)..$(line_offset "$main_rs" 42)"
  local after_main
  after_main=$(tab_count)
  [ "$after_main" = $((before + 2)) ] || fail "src/main.rs:42 did not open exactly one more tab: $after_main tabs"

  echo "step: a click on docs/NOME-FILE.md:30 reuses the tab of step 2, in code at line 30"
  click_link file 'docs/NOME-FILE.md:30'
  expect_editor "docs/NOME-FILE.md:30 switched its open tab to code at line 30" "$docs_main" code "$(line_offset "$docs_main" 30)..$(line_offset "$docs_main" 30)"
  [ "$(tab_count)" = "$after_main" ] || fail "the second click on the Markdown file opened a tab: $(tab_count) tabs, was $after_main"
  echo "OK: the tab was reused"

  echo "step: a click on docs/ALTRO.md:10, never opened before, opens it in code at line 10"
  click_link file 'docs/ALTRO.md:10'
  expect_editor "docs/ALTRO.md:10 opened in code at line 10, not in Preview" "$docs_other" code "$(line_offset "$docs_other" 10)..$(line_offset "$docs_other" 10)"
  [ "$(tab_count)" = $((after_main + 1)) ] || fail "docs/ALTRO.md:10 did not open exactly one more tab: $(tab_count) tabs"

  echo "step: a click on docs/ALTRO.md:10 after the file was deleted opens a tab that says it is missing"
  # The tab of step 5 is the selected one: closing it and deleting the file leaves a link
  # (built while the file existed) to a file that is gone.
  local altro_tab
  altro_tab=$(tabs_state | cut -d' ' -f2)
  reply surface tabs close "$altro_tab" >/dev/null || fail "could not close the tab $altro_tab of docs/ALTRO.md"
  rm -f "$docs_other"
  click_link file 'docs/ALTRO.md:10'
  expect_missing "the click on the vanished docs/ALTRO.md:10 opened its tab as missing" "$docs_other"
  [ "$(tab_count)" = $((after_main + 1)) ] || fail "the vanished file did not open exactly one tab: $(tab_count) tabs"

  echo "step: Super+Shift+click on README.md still opens the editor, not the browser"
  browser=$(browser_calls)
  click_link file 'README.md' super shift
  expect_editor "README.md opened in the editor" "$readme" ""
  [ "$(browser_calls)" = "$browser" ] || fail "Super+Shift+click on README.md called the browser: $(tail -n 3 "$RUN_DIR/browser.log")"
  echo "OK: the browser stub recorded no call"

  echo "step: a drag inside src/main.rs:42 selects and opens nothing"
  show_chat
  local count_active
  count_active=$(tabs_state)
  xy=$(link_xy file 'src/main.rs:42')
  drag_at "${xy% *}" "${xy#* }" $((${xy% *} + 17)) "${xy#* }"
  [ "$(tabs_state)" = "$count_active" ] || fail "a drag inside src/main.rs:42 changed the tabs: '$(tabs_state)', was '$count_active'"
  [ "$(browser_calls)" = "$browser" ] || fail "a drag inside src/main.rs:42 called the browser: $(tail -n 3 "$RUN_DIR/browser.log")"
  echo "OK: nothing opened (tabs '$count_active')"

  echo "step: a drag from inside src/main.rs:42 to just past its end opens nothing either"
  xy=$(link_xy file 'src/main.rs:42')
  local width
  width=$(E2E link-width "$SURFACE" file 'src/main.rs:42') || fail "no painted width for src/main.rs:42"
  [ "$width" -gt 20 ] || fail "src/main.rs:42 reports a painted width of $width px"
  drag_at "${xy% *}" "${xy#* }" $((${xy% *} + width + 10)) "${xy#* }"
  [ "$(tabs_state)" = "$count_active" ] || fail "a drag across src/main.rs:42 changed the tabs: '$(tabs_state)', was '$count_active'"
  [ "$(browser_calls)" = "$browser" ] || fail "a drag across src/main.rs:42 called the browser: $(tail -n 3 "$RUN_DIR/browser.log")"
  echo "OK: nothing opened (tabs '$count_active')"
  capture file-links-drag

  echo "step: a click on the web link still reaches the browser"
  click_link web sito
  [ "$(tabs_state)" = "$count_active" ] || fail "the web link opened a tab: '$(tabs_state)', was '$count_active'"
  sleep 1
  line=""
  for _ in $(seq 1 50); do
    line=$(grep -Fx 'xdg-open https://example.com/docs/NOME-FILE.md' "$RUN_DIR/browser.log" 2>/dev/null | head -n 1 || true)
    [ -n "$line" ] && break
    sleep 0.3
  done
  [ -n "$line" ] || fail "the harness's xdg-open stub never recorded 'xdg-open https://example.com/docs/NOME-FILE.md' (it recorded: $(cat "$RUN_DIR/browser.log" 2>/dev/null || echo nothing))"
  echo "OK: the xdg-open stub recorded: $line"
  echo "OK: file_links: the response's paths open their files, a drag opens nothing, and the web link reaches the browser"
  stop_app
  # The fixture worktree is shared with the scenarios that run after this one.
  rm -rf "$LINKED/docs" "$LINKED/README.md"
  printf 'fn main() {}\n' >"$main_rs"
}

# The clipboard chords with real X keys (xdotool), in the composer and on a selection dragged
# across a response, against the display's real CLIPBOARD selection. The harness reads it and
# owns it through GTK (python3-gi), the way another application holds it. Linux's chords are
# the ctrl ones; macOS's cmd ones resolve through the same bindings and stay a hand check.
# The clean environment pinned to X11 keeps a caller's WAYLAND_DISPLAY or GDK_BACKEND from
# steering GTK to the desktop's clipboard, which `own` would otherwise overwrite.
x_clipboard() { # get | own TEXT SECONDS
  timeout -k 2 $(( ${3:-5} + 5 )) env -i PATH=/usr/local/bin:/usr/bin:/bin "HOME=$RUN_DIR/clipboard-home" \
    GDK_BACKEND=x11 "DISPLAY=$DISPLAY_TARGET" python3 -I - "$@" <<'PY'
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk
clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
if sys.argv[1] == "get":
    text = clipboard.wait_for_text()
    sys.stdout.write("" if text is None else text)
else:
    clipboard.set_text(sys.argv[2], -1)
    GLib.timeout_add_seconds(int(sys.argv[3]), Gtk.main_quit)
    Gtk.main()
PY
}
# Owns the clipboard with TEXT in the background, as another application would, until
# SECONDS pass or the app takes the selection.
own_clipboard() { # text seconds
  x_clipboard own "$1" "$2" &
  CLIPBOARD_OWNER=$!
  local _
  for _ in $(seq 1 20); do
    [ "$(x_clipboard get)" = "$1" ] && return 0
    sleep 0.2
  done
  fail "the harness could not own the clipboard with '$1'"
}
release_clipboard() {
  [ -z "${CLIPBOARD_OWNER:-}" ] || { kill "$CLIPBOARD_OWNER" 2>/dev/null || true; wait "$CLIPBOARD_OWNER" 2>/dev/null || true; }
  CLIPBOARD_OWNER=""
}
expect_clipboard() { # desc want
  local got="" _
  for _ in $(seq 1 15); do
    got=$(x_clipboard get)
    [ "$got" = "$2" ] && { echo "OK: $1 (clipboard '$got')"; return 0; }
    sleep 0.3
  done
  capture "clipboard-keys-fail"
  fail "$1: the clipboard holds '$got', not '$2'"
}
scenario_clipboard_keys() {
  use_stub file-links
  local main_rs="$LINKED/src/main.rs"
  python3 -c 'import sys; open(sys.argv[1], "w").write("".join("// riga %d\n" % n if n != 1 else "fn main() {}\n" for n in range(1, 61)))' "$main_rs"
  launch clipboard_keys
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  show_chat
  wait_panel "the chat is ready" 'c["status"] in ("idle", "completed")' 30

  echo "step: typed text reaches the composer"
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool type --delay 60 "hello world" || fail "xdotool could not type"
  wait_panel "the composer holds the typed text" 'c["composerText"] == "hello world"' 10
  capture clipboard-keys-typed

  echo "step: Ctrl+A then Ctrl+C in the composer copies its text"
  own_clipboard "before copy" 8
  press_key ctrl+a
  press_key ctrl+c
  expect_clipboard "Ctrl+C copied the composer's selection" "hello world"
  release_clipboard

  echo "step: Ctrl+A then Ctrl+X in the composer cuts its text"
  own_clipboard "before cut" 8
  press_key ctrl+a
  press_key ctrl+x
  wait_panel "Ctrl+X emptied the composer" 'c["composerText"] == ""' 10
  expect_clipboard "Ctrl+X put the cut text on the clipboard" "hello world"
  release_clipboard

  echo "step: Ctrl+V in the composer pastes text another application owns"
  own_clipboard "from another app" 15
  press_key ctrl+v
  wait_panel "Ctrl+V pasted the other application's text" 'c["composerText"] == "from another app"' 10
  release_clipboard
  press_key ctrl+a
  press_key BackSpace
  wait_panel "the composer is empty again" 'c["composerText"] == ""' 10

  echo "step: a drag across the response, then Ctrl+C, copies what the drag selected"
  send_prompt "mostrami i file"
  wait_panel "the turn is over" 'c["status"] == "completed"' 40
  show_chat
  local xy width
  xy=$(link_xy file 'src/main.rs:42')
  width=$(E2E link-width "$SURFACE" file 'src/main.rs:42') || fail "no painted width for src/main.rs:42"
  own_clipboard "before transcript copy" 15
  drag_at "${xy% *}" "${xy#* }" $((${xy% *} + width + 10)) "${xy#* }"
  capture clipboard-keys-dragged
  press_key ctrl+c
  local got
  got=$(x_clipboard get)
  case "$got" in
    *main.rs:42*) echo "OK: Ctrl+C copied the transcript selection (clipboard '$got')" ;;
    *) capture clipboard-keys-fail; fail "Ctrl+C after a drag across the response left the clipboard at '$got'" ;;
  esac
  release_clipboard

  echo "step: a plain click on the response, then typing, still writes in the composer"
  click_at "$((${xy% *} - 40))" "${xy#* }"
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool type --delay 60 "dopo" || fail "xdotool could not type"
  wait_panel "typing after a click on the response reached the composer" 'c["composerText"] == "dopo"' 10
  echo "OK: clipboard_keys: copy, cut and paste work in the composer, and Ctrl+C copies a transcript selection"
  stop_app
  printf 'fn main() {}\n' >"$main_rs"
}

# The compact composer row on a native Claude chat (spec 2026-10-30-composer-band): the
# session-panel-bypass stub has the CLI start in bypassPermissions, and the chat at the default
# window (about 430 px wide) draws the compact row -- Fast, Thinking and the session panel in
# the overflow menu, the mode pill the danger shield-off -- on one line, with the chips clear
# of send. Asserted on the `composerRow` of `surface.chat.read`.
scenario_composer_compact() {
  use_stub session-panel-bypass
  launch composer_compact
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  send_prompt hello
  wait_panel "the turn completed, so the model controls show" 'c["status"] == "completed"' 30
  E2E composer-compact "$SURFACE" 30 || fail "the compact composer row is not what the spec draws"
  capture composer-compact

  echo "step: a real click on the overflow button opens the menu: Fast with its reason, Thinking"
  click_at_window "$(E2E composer-overflow-xy "$SURFACE" 15)"
  E2E composer-menu "$SURFACE" 15 preference || fail "the overflow menu is not what the spec draws"
  capture composer-overflow

  echo "step: a real Enter chooses the first enabled row, Thinking (Fast is blocked), and opens its picker"
  press_key Return
  wait_panel "Enter on the menu's marked row opened the thinking picker" 'c["composerRow"]["picker"] == "thinking"' 10
  capture composer-thinking

  echo "step: a real Escape closes the picker"
  press_key Escape
  wait_panel "Escape closed the thinking picker" 'c["composerRow"]["picker"] is None and c["composerRow"]["menu"] is None' 10
  stop_app
}

# What the composer's keys do, with real X keys (xdotool). Up and Down walk the chat's own
# prompts, a prompt keeps the file it mentioned (the mention goes out as one when the prompt is sent
# again, and survives a relaunch), and Shift+Tab cycles the permission modes without ever reaching
# Bypass. The fixture is the other witness: it logs every prompt it hears and every control request.
type_text() { # text
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool type --delay 60 "$1" || fail "xdotool could not type '$1'"
  sleep 0.5
}
# The composer holds exactly TEXT.
composer_is() { # desc text
  local literal
  literal=$(python3 -c 'import json, sys; print(json.dumps(sys.argv[1]))' "$2")
  wait_panel "$1" "c[\"composerText\"] == $literal" 10
}
# The user rows of the transcript, and the turn over.
turn_over() { # desc user-rows
  wait_panel "$1" "len([r for r in c['transcript'] if r.get('kind') == 'user']) == $2 and c['status'] == 'completed'" 40
}
# What the fixture heard: user_log_check INDEX EXPECT where EXPECT is link (the prompt carries the file as
# a mention link and no literal @path) or plain (the opposite).
user_log_check() { # index link|plain
  python3 - "$RUN_DIR/fixture-user.log" "$1" "$2" "$LINKED" <<'PY' || fail "the fixture's log of prompts is not what a $2 mention is"
import json, sys
path, index, want, root = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
lines = [json.loads(line) for line in open(path) if line.strip()]
text = "".join(b.get("text", "") for b in lines[index]["message"]["content"])
print(f"  heard[{index}]: {text!r}")
link = f"](file://{root}/src/main.rs)"
has_link, has_literal = link in text, "@src/main.rs" in text
if (want == "link") != (has_link and not has_literal):
    print(f"FAIL: expected a {want} mention: link={has_link} literal={has_literal}", file=sys.stderr)
    sys.exit(1)
PY
  echo "OK: prompt $1 reached the CLI with its mention as a $2"
}
# The permission modes the CLI was asked for, in order.
modes_asked() {
  python3 -c '
import json, sys
print(" ".join(r["mode"] for r in (json.loads(l) for l in open(sys.argv[1]) if l.strip()) if r.get("subtype") == "set_permission_mode"))' "$RUN_DIR/fixture-control.log"
}
# The chat tab is the only one and holds the focus: a character typed lands in the composer.
prove_composer_focus() {
  type_text "x"
  composer_is "the composer has the focus" "x"
  press_key BackSpace
  composer_is "the composer is empty again" ""
}
scenario_composer_keys() {
  : >"$RUN_DIR/fixture-user.log"
  : >"$RUN_DIR/fixture-control.log"
  launch composer_keys
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  show_chat
  wait_panel "the chat is ready" 'c["status"] in ("idle", "completed")' 30
  prove_composer_focus

  echo "step: two prompts, the second with a file picked from the @ popup"
  type_text "first prompt"
  press_key Return
  turn_over "the first turn is over" 1
  type_text "see @main"
  # The popup's walk of the worktree runs off the UI thread; Tab with no popup is not a no-op.
  sleep 2
  capture composer-keys-mention-popup
  press_key Tab
  composer_is "Tab picked the file: its path is the token" "see @src/main.rs "
  type_text "now"
  composer_is "the prompt reads on" "see @src/main.rs now"
  press_key Return
  turn_over "the second turn is over" 2
  user_log_check 1 link

  echo "step: Up brings the prompts back newest first, Down walks forward and then to the empty draft"
  press_key Up
  composer_is "Up shows the second prompt with its @path text" "see @src/main.rs now"
  capture composer-keys-recalled
  press_key Up
  composer_is "a second Up shows the first prompt" "first prompt"
  press_key Up
  composer_is "Up at the oldest prompt stays" "first prompt"
  press_key Down
  composer_is "Down shows the second prompt again" "see @src/main.rs now"
  press_key Down
  composer_is "Down past the newest is the empty draft" ""
  press_key Down
  composer_is "Down on the empty draft stays empty" ""

  echo "step: the recalled prompt, sent again, is a new turn with its mention as a mention"
  press_key Up
  composer_is "Up shows the second prompt" "see @src/main.rs now"
  press_key Return
  turn_over "the recalled prompt is a third turn" 3
  user_log_check 2 link
  composer_is "the composer is empty after the send" ""

  echo "step: an edited prompt is a draft: Up moves the caret instead of the history"
  press_key Up
  composer_is "Up shows the newest prompt" "see @src/main.rs now"
  type_text "!"
  press_key Up
  composer_is "after an edit Up no longer steps the history" "see @src/main.rs now!"
  press_key ctrl+a
  press_key BackSpace
  composer_is "the composer is empty" ""

  echo "step: Sirio starts again; Up still recalls the prompt with its mention"
  stop_app
  launch composer_keys
  SURFACE=""
  for _ in $(seq 1 100); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "the chat was not restored"
  echo "OK: the restored chat is $SURFACE"
  show_chat
  wait_panel "the restored chat is ready" 'c["status"] in ("idle", "completed")' 30
  prove_composer_focus
  press_key Up
  composer_is "after the relaunch Up recalls the prompt with its @path text" "see @src/main.rs now"
  press_key Up
  composer_is "and the one before it" "first prompt"
  press_key Down
  press_key Return
  turn_over "the prompt recalled after the relaunch is sent" 4
  user_log_check 3 link
  capture composer-keys-restored

  echo "step: Shift+Tab cycles the permission modes and never lands on Bypass"
  : >"$RUN_DIR/fixture-control.log"
  local n
  for n in 1 2 3 4 5 6; do
    press_key shift+Tab
  done
  local asked
  asked=$(modes_asked)
  echo "  asked: $asked"
  [ "$asked" = "acceptEdits plan auto default acceptEdits plan" ] \
    || fail "six Shift+Tab asked for '$asked', not the menu's order from Ask with Bypass left out"
  echo "OK: six Shift+Tab walked acceptEdits, plan, auto, default, acceptEdits, plan"
  wait_panel "no mode is drawn as the danger shield" 'c["composerRow"].get("modeDanger") is not True' 10
  capture composer-keys-modes
  type_text "ok"
  composer_is "no dialog holds the keyboard: typing reaches the composer" "ok"
  press_key BackSpace
  press_key BackSpace
  composer_is "the composer is empty" ""
  echo "OK: composer_keys: Up and Down recall prompts and their mentions across a relaunch, Shift+Tab cycles the modes short of Bypass"
  stop_app
}

# A wait on the composer row (`composerRow`) that records what it saw: the OK line is followed by
# the numbers the expression held for, so transcript.log can be re-checked without the frames.
wait_composer() { # desc expr [seconds]
  local desc=$1 got
  got=$(E2E find "$SURFACE" "$desc" "${3:-20}" "($2) and json.dumps({k: c[\"composerRow\"].get(k) for k in (\"capRows\", \"maxRows\", \"storedMaxRows\", \"fieldRows\", \"field\", \"band\", \"root\", \"resizing\")}, separators=(\",\", \":\"))") || fail "$desc"
  echo "OK: $desc"
  echo "  composerRow: $got"
}
# The composer's top edge and the chat root, in window pixels: "edge_x edge_y root_y root_h", the
# edge's centre and the root's top and height.
composer_edge_geometry() {
  E2E find "$SURFACE" "the composer's top edge is drawn" 10 'c["composerRow"]["edge"] and "%d %d %d %d" % (round(c["composerRow"]["edge"][0] + c["composerRow"]["edge"][2] / 2), round(c["composerRow"]["edge"][1] + c["composerRow"]["edge"][3] / 2), round(c["composerRow"]["root"][1]), round(c["composerRow"]["root"][3]))' || fail "the composer's top edge is not drawn"
}
# Presses the composer's top edge, moves 3 px up (this only starts GPUI's drag: the move that starts
# it never reaches the drag handlers), then in 9 px steps to window y $1, and releases unless $2 is
# "hold". The pointer moves at least twice, as a drag must.
drag_edge_to() { # window-y [hold]
  local geometry origin ex ey ox oy target here step args
  geometry=$(composer_edge_geometry) || fail "no composer edge to drag"
  origin=$(window_origin)
  ox=${origin% *}; oy=${origin#* }
  ex=$(( ${geometry%% *} + ox ))
  ey=$(( $(printf '%s' "$geometry" | awk '{print $2}') + oy ))
  target=$(( $1 + oy ))
  here=$(( ey - 3 ))
  args=(mousemove "$ex" "$ey" sleep 0.3 mousedown 1 sleep 0.2 mousemove "$ex" "$here" sleep 0.1)
  while [ "$here" -ne "$target" ]; do
    if [ "$target" -gt "$here" ]; then step=$(( target - here > 9 ? 9 : target - here )); else step=$(( here - target > 9 ? -9 : target - here )); fi
    here=$(( here + step ))
    args+=(mousemove "$ex" "$here" sleep 0.05)
  done
  [ "${2:-}" = hold ] || args+=(sleep 0.3 mouseup 1)
  echo "  drag: the edge at window pixel ${geometry%% *},$(printf '%s' "$geometry" | awk '{print $2}') (screen $ex,$ey) to window y $1" >&2
  DISPLAY="$DISPLAY_TARGET" timeout 30 xdotool "${args[@]}" || fail "xdotool could not drag the composer's edge to window y $1"
}
# Drags the edge $1 rows up (a negative count is down); a row is $COMPOSER_ROW_PX px, 18 by default.
drag_edge() { # rows [hold]
  local geometry
  geometry=$(composer_edge_geometry) || fail "no composer edge to drag"
  drag_edge_to "$(( $(printf '%s' "$geometry" | awk '{print $2}') - $1 * ${COMPOSER_ROW_PX:-18} ))" "${2:-}"
}

# The sidebar panel's rect in window pixels, "x y w h" (the debug `shellRects` of `surface layout read`).
sidebar_geometry() {
  reply surface layout read | python3 -c 'import json, sys
rects = json.loads(json.load(sys.stdin)[0]["shellRects"])
left = rects["left"]
print(*[round(v) for v in left])' || fail "the sidebar's rect is not drawn"
}

# A click at the centre of a part of the sidebar the debug `shellRects` report (sidebarTitle,
# sidebarSection, ...), moved by DX DY window pixels. The part must be drawn.
click_sidebar_part() { # key [dx dy]
  local xy
  xy=$(reply surface layout read | python3 -c 'import json, sys
rect = json.loads(json.load(sys.stdin)[0]["shellRects"]).get(sys.argv[1])
if rect is None:
    sys.exit(1)
print(round(rect[0] + rect[2] / 2 + int(sys.argv[2])), round(rect[1] + rect[3] / 2 + int(sys.argv[3])))' "$1" "${2:-0}" "${3:-0}") \
    || fail "the sidebar draws no $1 to click"
  click_at_window "$xy"
}

scenario_composer_resize() {
  launch composer_resize
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  show_chat
  wait_panel "the chat is ready" 'c["status"] in ("idle", "completed")' 30
  compose() { E2E rpc surface.chat.compose surfaceId="$SURFACE" text="$1" >/dev/null || fail "composing failed"; }

  echo "step: at 2200x1000 the cap is at least the stored 12 and the empty field shows 3 rows"
  wait_composer "the cap holds the default 12, the stored value is 12 and the field shows 3 rows" \
    '(c["composerRow"]["capRows"] or 0) >= 12 and c["composerRow"]["maxRows"] == 12 and c["composerRow"]["storedMaxRows"] == 12 and c["composerRow"]["fieldRows"] == 3'
  capture composer-resize-rest

  echo "step: a 40-line draft fills the field to its 12-row maximum"
  compose "$(printf 'line %d\n' $(seq 1 40))"
  wait_composer "the field grew to 12 rows" 'c["composerRow"]["fieldRows"] == 12'
  capture composer-resize-full

  echo "step: a window 400 px tall caps the field at half the chat less the band's allowance, below the stored 12"
  local window
  window=$(find_window)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowsize "$window" 2200 400 || fail "could not resize the window to 2200x400"
  # (A layout that kept the chat taller than 554 px would fail here, not further down.)
  wait_composer "the chat shrank under 554 px" 'c["composerRow"]["root"][3] < 554'
  wait_composer "the field follows the cap while the stored maximum stays 12" \
    '(r := c["composerRow"])["maxRows"] == r["capRows"] and r["capRows"] < 12 and r["capRows"] == max(3, int((r["root"][3] / 2 - 61) / 18)) and r["storedMaxRows"] == 12 and r["fieldRows"] == r["capRows"]'
  wait_composer "at the cap with no chips the band takes at most half the chat" \
    '(r := c["composerRow"])["fieldRows"] == r["capRows"] and r["band"][3] <= r["root"][3] / 2'
  capture composer-resize-short
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowsize "$window" 2200 1000 || fail "could not resize the window back to 2200x1000"
  wait_composer "back at 2200x1000 the field is 12 rows again" 'c["composerRow"]["maxRows"] == 12 and c["composerRow"]["fieldRows"] == 12'

  echo "step: dragging the edge 4 rows up (released where the dragged edge now is) raises the maximum to 16"
  drag_edge 4
  wait_composer "the field, the drawn maximum and the stored maximum are 16" \
    'c["composerRow"]["fieldRows"] == 16 and c["composerRow"]["maxRows"] == 16 and c["composerRow"]["storedMaxRows"] == 16 and not c["composerRow"]["resizing"]'
  capture composer-resize-taller

  echo "step: dragging the edge far up and releasing over the transcript stops at the cap"
  drag_edge_to "$(composer_edge_geometry | awk '{print int($3 + $4 / 4)}')"
  wait_composer "the field and the stored maximum are the cap" \
    '(r := c["composerRow"])["fieldRows"] == r["capRows"] and r["storedMaxRows"] == r["capRows"] and r["capRows"] > 16 and not r["resizing"] and r["band"][3] <= r["root"][3] / 2'

  echo "step: a release below the window ends the drag too, and the rows bottom out at 3"
  # The screen must be taller than the window (2200x1000): window y 1080 is then outside the window
  # and still on the screen, so X delivers the release through the press's implicit grab. On a screen
  # too short, xdotool would clamp the pointer back into the window, so the step refuses to run there.
  # The 40-line draft is still in: an empty field anchors at 3 rows and dragging it down would change nothing.
  local below_origin screen_h
  below_origin=$(window_origin)
  screen_h=$(DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool getdisplaygeometry | awk '{print $2}')
  [ "$(( ${below_origin#* } + 1080 ))" -lt "${screen_h:-0}" ] \
    || fail "window y 1080 is not on the ${screen_h:-unknown} px tall screen (window origin $below_origin), so the release would land inside the window: start a taller Xvfb (Xvfb $DISPLAY_TARGET -screen 0 2400x1200x24 &)"
  drag_edge_to 1080
  wait_composer "released below the window, the field, the drawn maximum and the stored maximum are 3 and nothing is held" \
    'c["composerRow"]["fieldRows"] == 3 and c["composerRow"]["maxRows"] == 3 and c["composerRow"]["storedMaxRows"] == 3 and not c["composerRow"]["resizing"]'
  capture composer-resize-outside

  echo "step: an empty field leaves nothing pinned"
  compose ""
  wait_composer "the empty field shows 3 rows" 'c["composerRow"]["fieldRows"] == 3'

  echo "step: a held drag pins the field, the release lets go and keeps the maximum"
  drag_edge 5 hold
  wait_composer "the held field is pinned at 8 rows" 'c["composerRow"]["fieldRows"] == 8 and c["composerRow"]["resizing"]'
  capture composer-resize-held
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mouseup 1 || fail "xdotool could not release the button"
  wait_composer "the released field is 3 rows, its maximum 8" \
    'c["composerRow"]["fieldRows"] == 3 and c["composerRow"]["maxRows"] == 8 and c["composerRow"]["storedMaxRows"] == 8 and not c["composerRow"]["resizing"]'

  echo "step: a press that never moved, and one held under half a row up, change nothing"
  local geometry origin ex ey
  geometry=$(composer_edge_geometry) || fail "no composer edge"
  origin=$(window_origin)
  ex=$(( ${geometry%% *} + ${origin% *} ))
  ey=$(( $(printf '%s' "$geometry" | awk '{print $2}') + ${origin#* } ))
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool mousemove "$ex" "$ey" sleep 0.3 mousedown 1 sleep 0.3 mouseup 1 || fail "xdotool could not press the edge"
  wait_composer "a press and release without moving keeps the maximum at 8" \
    'c["composerRow"]["maxRows"] == 8 and c["composerRow"]["storedMaxRows"] == 8 and c["composerRow"]["fieldRows"] == 3 and not c["composerRow"]["resizing"]'
  # Held 8 px up (under half a row): the drag has started, so the field is pinned at the anchor's 3 rows.
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool mousemove "$ex" "$ey" sleep 0.3 mousedown 1 sleep 0.2 mousemove "$ex" $((ey - 3)) sleep 0.1 mousemove "$ex" $((ey - 8)) sleep 0.3 || fail "xdotool could not nudge the edge"
  wait_composer "held 8 px up, the field is pinned at its 3 rows" \
    'c["composerRow"]["resizing"] and c["composerRow"]["fieldRows"] == 3 and c["composerRow"]["maxRows"] == 8'
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mouseup 1 || fail "xdotool could not release the button"
  wait_composer "released, the 8 px drag keeps the maximum at 8" \
    'not c["composerRow"]["resizing"] and c["composerRow"]["maxRows"] == 8 and c["composerRow"]["storedMaxRows"] == 8 and c["composerRow"]["fieldRows"] == 3'

  echo "step: a real divider drag saves the whole settings row, and the maximum survives it"
  # The sidebar's divider saves through `app_settings_for_settings_save`, the writer that once could
  # reset the rows to 12; the relaunch below proves the 8 survived it.
  local sidebar_before sidebar_after sidebar_after_relaunch sx sy
  sidebar_before=$(sidebar_geometry)
  origin=$(window_origin)
  sx=$(( $(printf '%s' "$sidebar_before" | awk '{print int($1 + $3 - 3)}') + ${origin% *} ))
  sy=$(( $(printf '%s' "$sidebar_before" | awk '{print int($2 + $4 / 2)}') + ${origin#* } ))
  echo "  drag: the sidebar's divider (screen $sx,$sy) 80 px right"
  DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool mousemove "$sx" "$sy" sleep 0.5 mousedown 1 sleep 0.3 mousemove $((sx + 5)) "$sy" sleep 0.3 mousemove $((sx + 40)) "$sy" sleep 0.3 mousemove $((sx + 80)) "$sy" sleep 0.5 mouseup 1 || fail "xdotool could not drag the sidebar's divider"
  for _ in $(seq 1 50); do
    sidebar_after=$(sidebar_geometry)
    [ "$(printf '%s' "$sidebar_after" | awk '{print int($3)}')" -ge "$(( $(printf '%s' "$sidebar_before" | awk '{print int($3)}') + 40 ))" ] && break
    sleep 0.2
  done
  echo "  sidebar: before '$sidebar_before', after '$sidebar_after'"
  [ "$(printf '%s' "$sidebar_after" | awk '{print int($3)}')" -ge "$(( $(printf '%s' "$sidebar_before" | awk '{print int($3)}') + 40 ))" ] \
    || fail "the divider drag did not widen the sidebar"
  wait_composer "the divider drag left the maximum at 8" 'c["composerRow"]["maxRows"] == 8 and c["composerRow"]["storedMaxRows"] == 8'
  # PANEL_WIDTH_SAVE_DEBOUNCE is 500 ms.
  sleep 2
  local chat_tabs_before
  chat_tabs_before=$(tab_positions chat terminal)

  echo "step: Sirio starts again; the restored chat and a second chat both keep the dragged maximum"
  stop_app
  launch composer_resize
  # The relaunch restores the tabs it had: no tab is opened, closed or counted new before the chat is
  # read, so the surface read next is the restored one.
  local chat_tabs_after
  for _ in $(seq 1 50); do
    chat_tabs_after=$(tab_positions chat terminal)
    [ -n "$chat_tabs_after" ] && break
    sleep 0.3
  done
  [ "$chat_tabs_after" = "$chat_tabs_before" ] \
    || fail "the relaunch restored tabs '$chat_tabs_after', not the '$chat_tabs_before' it had"
  show_chat
  SURFACE=""
  for _ in $(seq 1 50); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "no restored chat tab"
  [ "$(tab_positions chat terminal)" = "$chat_tabs_before" ] || fail "reading the restored chat opened a tab: $(tab_positions chat terminal)"
  echo "OK: the restored chat is surface $SURFACE, in tab $chat_tabs_before"
  local restored=$SURFACE
  wait_composer "the restored chat: the maximum and the stored maximum are 8 and the empty field shows 3 rows" \
    'c["composerRow"]["maxRows"] == 8 and c["composerRow"]["storedMaxRows"] == 8 and c["composerRow"]["fieldRows"] == 3' 30
  compose "$(printf 'line %d\n' $(seq 1 40))"
  wait_composer "the restored chat's 40-line draft fills the field to the restored 8 rows" 'c["composerRow"]["fieldRows"] == 8'
  capture composer-resize-relaunched
  compose ""
  wait_composer "the restored chat's empty field is back to 3 rows" 'c["composerRow"]["fieldRows"] == 3'
  sidebar_after_relaunch=$(sidebar_geometry)
  echo "  sidebar after the relaunch: '$sidebar_after_relaunch'"
  [ "$(printf '%s' "$sidebar_after_relaunch" | awk '{print int($3)}')" -ge "$(( $(printf '%s' "$sidebar_before" | awk '{print int($3)}') + 40 ))" ] \
    || fail "the relaunch drew the sidebar at its old width: the divider's save never ran, so it proves nothing about the maximum"

  # A second chat, opened beside the first (open_chat would close every tab).
  ctl surface open-list open --entry agent:claude >/dev/null || fail "the second chat did not open"
  local second="" id
  for _ in $(seq 1 50); do
    for id in $(reply surface attention read | python3 -c 'import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
for row in json.loads(reply["sessions"]):
    if row["kind"] == "chat":
        print(row["id"])'); do
      [ "$id" != "$restored" ] && second=$id
    done
    [ -n "$second" ] && break
    sleep 0.2
  done
  [ -n "$second" ] || fail "no second chat opened beside the restored one"
  SURFACE=$second
  echo "OK: the second chat is surface $second, the restored one stays open as $restored"
  wait_panel "the second chat is ready" 'c["status"] in ("idle", "completed")' 30
  wait_composer "the second chat: the maximum and the stored maximum are 8 and the empty field shows 3 rows" \
    'c["composerRow"]["maxRows"] == 8 and c["composerRow"]["storedMaxRows"] == 8 and c["composerRow"]["fieldRows"] == 3' 30
  compose "$(printf 'line %d\n' $(seq 1 40))"
  wait_composer "the second chat's 40-line draft fills the field to 8 rows" 'c["composerRow"]["fieldRows"] == 8'

  echo "step: at interface font 18 the row is 23 px: the cap, the field and a drag follow it"
  # The relaunch restores both chats; show_chat selects the one chat tab, so keep only the restored one.
  local position remaining=""
  for position in $(tab_positions chat); do
    [ "$position" = "$chat_tabs_before" ] || ctl surface tabs close "$position" --confirm >/dev/null
  done
  for _ in $(seq 1 50); do
    remaining=$(tab_positions chat)
    [ "$remaining" = "$chat_tabs_before" ] && break
    sleep 0.2
  done
  [ "$remaining" = "$chat_tabs_before" ] || fail "closing the second chat left the chat tabs at '$remaining', not only the restored one at $chat_tabs_before"
  stop_app
  # appearance.uiFontSize is stored in the setting table; 18 is the top of its 12..=18 range.
  python3 -I - "$RUN_DIR/composer_resize.sqlite" <<'PY' || fail "could not set appearance.uiFontSize to 18"
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
db.execute("INSERT OR REPLACE INTO setting (key, value) VALUES ('appearance.uiFontSize', '18')")
db.commit()
db.close()
PY
  launch composer_resize
  show_chat
  SURFACE=""
  for _ in $(seq 1 50); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "no restored chat tab at interface font 18"
  wait_composer "at font 18 the cap counts 23 px rows, the stored maximum is 8 and the empty field shows 3 rows" \
    '(r := c["composerRow"])["capRows"] == max(3, int((r["root"][3] / 2 - 61) / 23)) and r["maxRows"] == min(8, r["capRows"]) and r["storedMaxRows"] == 8 and r["fieldRows"] == 3' 30
  compose "$(printf 'line %d\n' $(seq 1 40))"
  wait_composer "the 40-line draft fills the field to its maximum, drawn on 23 px rows" \
    '(r := c["composerRow"])["fieldRows"] == r["maxRows"] and r["field"][3] == r["maxRows"] * 23 + 10 and r["band"][3] == 61 + r["maxRows"] * 23'
  drag_edge_to "$(composer_edge_geometry | awk '{print int($3 + $4 / 4)}')"
  wait_composer "dragged far up, the field and the stored maximum are the cap and the band takes at most half the chat" \
    '(r := c["composerRow"])["fieldRows"] == r["capRows"] and r["storedMaxRows"] == r["capRows"] and not r["resizing"] and r["band"][3] == 61 + r["capRows"] * 23 and r["band"][3] <= r["root"][3] / 2'
  COMPOSER_ROW_PX=23 drag_edge -2
  wait_composer "dragged 2 rows (46 px) down, the field and the stored maximum are 2 rows under the cap" \
    '(r := c["composerRow"])["fieldRows"] == r["capRows"] - 2 and r["storedMaxRows"] == r["capRows"] - 2 and not r["resizing"]'
  capture composer-resize-font18
  echo "OK: composer_resize: the field follows the stored maximum and the cap, the edge's drag sets and keeps it, a release outside the window ends a drag, a divider save keeps it, a relaunch restores it in the restored chat and in a new one, and at interface font 18 the cap, the field and a drag follow the 23 px row"
  stop_app
}

# ---- the unread scenarios ---------------------------------------------------------
# A fresh Sirio on the fixture with the trace stub and a Claude chat in $LINKED, no column.
# STEP-MS paces the fixture's scripted lines.
unread_chat() { # name [step-ms]
  use_stub trace
  TRACE_STEP_MS=${2:-400}
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
}
# A shown session is marked seen by the frame that draws it, and a --state-only run draws
# none: the focus act runs the same marking (`mark_shown_sessions_seen`) without one. The
# sirioctl E2E's case 15 proves the frame's own marking on a window.
look_at_shown() {
  ctl surface attention act --focus active >/dev/null || fail "surface attention act --focus active was refused"
}
# One field of the chat session `surface attention read` lists for $LINKED: strings as they
# are, everything else as JSON.
attention_chat() { attention_session chat "$1"; }
attention_session() { # kind field: the first session of that kind (chat|terminal) held for $LINKED
  reply surface attention read | python3 -c 'import json, os, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
for row in json.loads(reply["sessions"]):
    if row["kind"] == sys.argv[1] and row["worktree"] and os.path.realpath(row["worktree"]) == os.path.realpath(sys.argv[3]):
        value = row[sys.argv[2]]
        print(value if isinstance(value, str) else json.dumps(value))
        break' "$1" "$2" "$LINKED"
}
# Polls until the chat session's FIELD reads WANT.
wait_attention() { # desc field want seconds
  local desc=$1 field=$2 want=$3 seconds=$4 got="" _
  for _ in $(seq 1 $((seconds * 5))); do
    got=$(attention_chat "$field")
    [ "$got" = "$want" ] && { echo "OK: $desc ($field = $want)"; return 0; }
    sleep 0.2
  done
  fail "$desc: the chat session's $field is '$got', not '$want' after $seconds s; read: $(reply surface attention read | tr '\n' ' ')"
}
# One field of the session `surface attention read` lists with id $1 (any worktree, or none
# when the tab is not held): strings as they are, everything else as JSON. Prints nothing when
# the read does not list it.
attention_row() { # id field
  reply surface attention read | python3 -c 'import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
for row in json.loads(reply["sessions"]):
    if row["id"] == sys.argv[1]:
        value = row[sys.argv[2]]
        print(value if isinstance(value, str) else json.dumps(value))
        break' "$1" "$2"
}
# The ids of the chat sessions held for $LINKED, one per line, in the order the read lists them.
attention_chat_ids() {
  reply surface attention read | python3 -c 'import json, os, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
for row in json.loads(reply["sessions"]):
    if row["kind"] == "chat" and row["worktree"] and os.path.realpath(row["worktree"]) == os.path.realpath(sys.argv[1]):
        print(row["id"])' "$LINKED"
}
# Polls until session $2's FIELD $3 reads WANT.
wait_row() { # desc id field want seconds
  local desc=$1 id=$2 field=$3 want=$4 seconds=$5 got="" _
  for _ in $(seq 1 $((seconds * 5))); do
    got=$(attention_row "$id" "$field")
    [ "$got" = "$want" ] && { echo "OK: $desc ($field = $want)"; return 0; }
    sleep 0.2
  done
  fail "$desc: session $id's $field is '$got', not '$want' after $seconds s; read: $(reply surface attention read | tr '\n' ' ')"
}
# The notifications the app decided to post, as JSON objects one per line, oldest first. A
# read with no `posted` key is an error, never an empty list.
posted_rows() {
  reply surface attention read | python3 -c 'import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
if "posted" not in reply:
    sys.exit("the attention read has no posted key")
for row in json.loads(reply["posted"]):
    print(json.dumps(row))'
}
posted_count() { local rows; rows=$(posted_rows) || fail "the posted notifications could not be read"; [ -z "$rows" ] && echo 0 || printf '%s\n' "$rows" | wc -l | tr -d ' '; }
# The bodies of the notifications posted after the first $1 of them, one per line.
posted_bodies_since() { # baseline
  posted_rows | tail -n +"$(( $1 + 1 ))" | python3 -c 'import json, sys
for line in sys.stdin:
    print(json.loads(line)["body"])'
}
# The count of unread session pills the sidebar holds for the worktree at $1, from the `rows`
# key of `surface attention read` (what `Sidebar::unread_marks` reports): a number, or nothing
# when the read has no `rows` key or lists no such worktree.
attention_marks() { # worktree
  reply surface attention read | python3 -c 'import json, os, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
if "rows" not in reply:
    sys.exit(0)
for row in json.loads(reply["rows"]):
    if os.path.realpath(row["worktree"]) == os.path.realpath(sys.argv[1]):
        print(row["unread"])
        break' "$1"
}
# The sidebar's pills are refreshed by the reconcile, which a frame drives: a --state-only run
# with no display paints nothing, so its `rows` and `projects` reads have nothing to say and are
# not asserted. The `tray` read is not frame-driven (a minimized window paints nothing and the
# tray is meant for exactly then), so it is asserted in every mode.
rows_painted() { [ -n "$DISPLAY_TARGET" ]; }
# The fold and view clicks are real X input at window pixels: only a run that opened the window
# (no --state-only), which the display preflight checked, makes them.
window_clicks() { [ "$STATE_ONLY" -eq 0 ]; }
# One field of the first project header in the read's `projects` key (what the tree draws:
# `expanded`, and the `unreadCount` of the folded badge), as JSON; nothing when the key is absent.
attention_project() { # field
  reply surface attention read | python3 -c 'import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
if "projects" not in reply:
    sys.exit(0)
projects = json.loads(reply["projects"])
if projects:
    print(json.dumps(projects[0][sys.argv[1]]))' "$1"
}
# Polls until the first project header's FIELD reads WANT (JSON: true, false, 1).
wait_project() { # desc field want seconds
  local desc=$1 field=$2 want=$3 seconds=$4 got="" _
  for _ in $(seq 1 $((seconds * 5))); do
    got=$(attention_project "$field")
    [ "$got" = "$want" ] && { echo "OK: $desc (project $field = $want)"; return 0; }
    sleep 0.2
  done
  fail "$desc: the project header's $field is '$got', not '$want' after $seconds s; read: $(reply surface attention read | tr '\n' ' ')"
}
# One field of the tray roster entry for worktree $1, as the read's `tray` key lists it: strings
# as they are, everything else as JSON; nothing when the roster does not list it.
attention_tray() { # worktree field
  reply surface attention read | python3 -c 'import json, os, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
if "tray" not in reply:
    sys.exit(0)
for row in json.loads(reply["tray"]):
    if os.path.realpath(row["worktree"]) == os.path.realpath(sys.argv[1]):
        value = row[sys.argv[2]]
        print(value if isinstance(value, str) else json.dumps(value))
        break' "$1" "$2"
}
# Polls until the tray entry for worktree $2 reads WANT in FIELD, or, with field `label`, starts
# with WANT.
wait_tray() { # desc worktree field want seconds
  local desc=$1 worktree=$2 field=$3 want=$4 seconds=$5 got="" _
  for _ in $(seq 1 $((seconds * 5))); do
    got=$(attention_tray "$worktree" "$field")
    if [ "$field" = label ]; then
      case "$got" in "$want"*) echo "OK: $desc (tray label '$got')"; return 0 ;; esac
    else
      [ "$got" = "$want" ] && { echo "OK: $desc (tray $field = $want)"; return 0; }
    fi
    sleep 0.2
  done
  fail "$desc: the tray's $field for $worktree is '$got', not '$want' after $seconds s; read: $(reply surface attention read | tr '\n' ' ')"
}
# Polls until the tray lists worktree $2 with no unread mark: `unread` false, or no entry.
wait_tray_unread_gone() { # desc worktree seconds
  local desc=$1 worktree=$2 seconds=$3 got="" _
  for _ in $(seq 1 $((seconds * 5))); do
    got=$(attention_tray "$worktree" unread)
    [ "$got" != true ] && { echo "OK: $desc (tray unread = '${got:-not listed}')"; return 0; }
    sleep 0.2
  done
  fail "$desc: the tray still lists $worktree as unread after $seconds s; read: $(reply surface attention read | tr '\n' ' ')"
}
# Polls until the sidebar's unread count for worktree $2 reads WANT.
wait_marks() { # desc worktree want seconds
  local desc=$1 worktree=$2 want=$3 seconds=$4 got="" _
  for _ in $(seq 1 $((seconds * 5))); do
    got=$(attention_marks "$worktree")
    [ "$got" = "$want" ] && { echo "OK: $desc (rows count = $want)"; return 0; }
    sleep 0.2
  done
  fail "$desc: the sidebar's unread count for $worktree is '$got', not '$want' after $seconds s; read: $(reply surface attention read | tr '\n' ' ')"
}
# The last attention read as the scenario's artifact: attention-NAME.json under the out dir.
save_attention() { # name
  reply surface attention read | python3 -c 'import json, sys
reply = json.load(sys.stdin)
reply = reply[0] if isinstance(reply, list) else reply
reply["sessions"] = json.loads(reply["sessions"])
reply["posted"] = json.loads(reply.get("posted", "[]"))
json.dump(reply, open(sys.argv[1], "w"), indent=2)' "$OUT_DIR/attention-$1.json"
  [ -s "$OUT_DIR/attention-$1.json" ] || fail "no attention-$1.json was written"
  echo "OK: $OUT_DIR/attention-$1.json holds the last read"
}

scenario_unread_parked_chat() {
  # A turn of about four seconds: long enough to leave the worktree while it runs.
  unread_chat unread_parked_chat 1000
  wait_panel "the chat is ready for a prompt" 'c["status"] in ("idle", "completed")' 30
  [ "$(attention_chat unread)" = false ] || fail "the chat reads unread before any turn"
  local base
  base=$(posted_count)
  if rows_painted; then
    wait_marks "the sidebar holds no unread pill before any turn" "$LINKED" 0 10
  else
    echo "SKIP: no display, so no frame refreshes the sidebar: the rows and projects assertions are not run (the tray's are, in every mode)"
  fi
  capture unread_parked_chat-before
  send_prompt request
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  # The turn is still going and nothing shows the chat: the stop below is off screen.
  [ "$(attention_chat status)" = running ] || fail "the chat does not read running right after the select (status '$(attention_chat status)'), so the stop would not be off screen"
  [ "$(attention_chat onScreen)" = false ] || fail "the chat reads onScreen after the select"
  echo "step: the worktree was left mid-turn; the turn ends with the chat parked"
  wait_attention "the parked chat's turn ended" status done 30
  wait_attention "the parked chat's stop marked it" unread true 5
  local got="" _
  for _ in $(seq 1 50); do
    got=$(( $(posted_count) - base ))
    [ "$got" -ge 1 ] && break
    sleep 0.2
  done
  [ "$got" = 1 ] || fail "the parked chat's stop posted $got notification(s), not one; posted: $(posted_rows | tr '\n' ' ')"
  sleep 1
  [ "$(( $(posted_count) - base ))" = 1 ] || fail "the stop posted more than one notification: $(posted_rows | tr '\n' ' ')"
  case "$(posted_bodies_since "$base")" in *feat/login*) ;; *) fail "the notification's body does not name feat/login: $(posted_bodies_since "$base")" ;; esac
  echo "OK: one notification, '$(posted_bodies_since "$base")'"
  # The tray is for the user who is away, a minimized window that paints no frame: its roster
  # follows the unread marks without one, so it is asserted in every mode, a --state-only run
  # with no display included.
  echo "step: the tray lists the unread worktree, marked"
  wait_tray "the tray lists the parked worktree as unread" "$LINKED" unread true 10
  wait_tray "its menu label leads with the unread dot" "$LINKED" label "● " 10
  if rows_painted; then
    echo "step: the sidebar draws the unread mark on the worktree's row"
    wait_marks "the sidebar counts the unread chat under the worktree" "$LINKED" 1 10
    wait_project "the project header is open and draws no badge" expanded true 10
    wait_project "an open project header counts nothing" unreadCount 0 10
    capture unread_parked_chat-unread
    if window_clicks; then
      echo "step: the Sessions view draws the same mark on the session's card"
      # The title's menu: a click on the title opens it, and its two rows hang under it (6 px gap,
      # 4 px padding, rows of 30 px: Projects first, Sessions second). The socket reports the
      # title's rect (shellRects sidebarTitle), so the clicks follow the layout.
      click_sidebar_part sidebarTitle
      sleep 1
      click_sidebar_part sidebarTitle 0 $(( 14 + 6 + 4 + 30 + 15 ))
      capture unread-sessions-view
      click_sidebar_part sidebarTitle
      sleep 1
      click_sidebar_part sidebarTitle 0 $(( 14 + 6 + 4 + 15 ))
      echo "step: a folded project keeps the mark as a badge on its header"
      # The project row is the first one under the section header; a click on it folds the
      # project. A click that misses leaves the header open, which the read below fails on.
      click_sidebar_part sidebarSection
      wait_project "the click folded the project" expanded false 10
      wait_project "the folded header's badge counts the unread chat" unreadCount 1 10
      capture unread-collapsed-project
      click_sidebar_part sidebarSection
      wait_project "the second click opened the project again" expanded true 10
      wait_project "the open header counts nothing again" unreadCount 0 10
    fi
  fi
  echo "step: showing the worktree clears the mark"
  ctl select-workspace --workspace "$LINKED" >/dev/null
  look_at_shown
  wait_attention "showing the chat's worktree cleared the mark" unread false 10
  if rows_painted; then
    wait_marks "showing the worktree cleared the sidebar's count" "$LINKED" 0 10
    # A run that paints keeps listing the shown worktree, now with no unread flag.
    wait_tray "showing the worktree cleared the tray's unread flag" "$LINKED" unread false 10
  else
    # A run with no display never reconciles the control pane registry (a frame drives
    # `sync_control_panes`), so the shown worktree has no status to list and the roster drops
    # its entry once the flag is gone: the flag must read false or the entry be gone.
    wait_tray_unread_gone "showing the worktree cleared the tray's unread flag" "$LINKED" 10
  fi
  case "$(attention_tray "$LINKED" label)" in "● "*) fail "the tray label still leads with the unread dot: $(attention_tray "$LINKED" label)" ;; esac
  echo "OK: the tray label no longer leads with the unread dot"
  capture unread_parked_chat-after
  save_attention unread_parked_chat
  stop_app
}

scenario_unread_question() {
  # The question comes about three seconds after the prompt, with the worktree already left.
  unread_chat unread_question 1000
  wait_panel "the chat is ready for a prompt" 'c["status"] in ("idle", "completed")' 30
  capture unread_question-before
  send_prompt subperm
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  # The turn runs and has not asked yet: the question below comes with the worktree left.
  [ "$(attention_chat status)" = running ] || fail "the chat does not read running right after the select (status '$(attention_chat status)'), so the question would not come off screen"
  [ "$(attention_chat onScreen)" = false ] || fail "the chat reads onScreen after the select"
  echo "step: the worktree was left; the turn asks a permission and nobody answers"
  wait_attention "the parked chat waits for the user" status needs-input 30
  wait_attention "the question marked the parked chat" unread true 5
  sleep 2
  [ "$(attention_chat status)" = needs-input ] || fail "the parked chat left needs-input with no answer given: $(attention_chat status)"
  echo "step: the question is still open"
  ctl select-workspace --workspace "$LINKED" >/dev/null
  look_at_shown
  wait_panel "the permission card is still pending" 'len(pending()) == 1' 10
  wait_attention "showing the chat's worktree cleared the mark" unread false 10
  [ "$(attention_chat status)" = needs-input ] || fail "the shown chat is not waiting any more: $(attention_chat status)"
  capture unread_question-after
  save_attention unread_question
  answer_permission reject_once
  stop_app
}

scenario_unread_secondary_seen() {
  unread_chat unread_secondary_seen
  # `open_chat` opens the chat in the Secondary half: a terminal in the Primary half puts
  # two shown tabs in front of the user.
  local tabs
  ctl surface terminal open --cmd "sleep 600" >/dev/null || fail "no terminal opened"
  for _ in $(seq 1 50); do
    tabs=$(reply surface layout read | python3 -c 'import json, sys; r = json.load(sys.stdin)[0]; print(r["secondaryVisible"], r["secondaryKinds"])' 2>/dev/null || true)
    [ "$tabs" = "true chat" ] && break
    sleep 0.2
  done
  [ "$tabs" = "true chat" ] || fail "the chat is not alone in a visible Secondary half (secondaryVisible secondaryKinds: '$tabs')"
  echo "OK: the chat is alone in the visible Secondary half"
  local primary_tabs
  primary_tabs=$(reply surface layout read | python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["primaryTabs"])')
  [ "$primary_tabs" = 1 ] || fail "the Primary half holds $primary_tabs tabs, not the one terminal"
  for _ in $(seq 1 50); do
    [ "$(attention_session terminal onScreen)" = true ] && break
    sleep 0.2
  done
  [ "$(attention_session terminal onScreen)" = true ] || fail "the terminal in the Primary half does not read onScreen: both halves are not shown"
  echo "OK: the Primary half's one tab is the terminal, and it is on screen"
  wait_attention "the chat is on screen in the Secondary half" onScreen true 10
  local base
  base=$(posted_count)
  capture unread_secondary_seen-before
  send_prompt second
  wait_panel "the turn ended" 'c["status"] == "completed"' 30
  wait_attention "the chat's stop was decided" status done 10
  sleep 2
  [ "$(attention_chat unread)" = false ] || fail "a stop in the shown Secondary half marked the chat unread"
  [ "$(attention_chat onScreen)" = true ] || fail "the chat is no longer on screen"
  [ "$(( $(posted_count) - base ))" = 0 ] || fail "a stop on screen posted a notification: $(posted_rows | tr '\n' ' ')"
  capture unread_secondary_seen-after
  save_attention unread_secondary_seen
  echo "OK: the stop in the Secondary half left no mark and posted nothing"
  stop_app
}

scenario_unread_restored_chat_quiet() {
  # Turns of a few seconds, so the second half can leave the worktree while one runs.
  unread_chat unread_restored_chat_quiet 1000
  local first=$SURFACE second="" id _
  echo "step: a turn in each of two chats of one worktree, each shown while it runs"
  wait_row "the first chat is on screen" "$first" onScreen true 10
  send_prompt second
  wait_panel "the first chat's turn ended" 'c["status"] == "completed"' 30
  ctl surface open-list open --entry agent:claude >/dev/null || fail "the second chat did not open"
  for _ in $(seq 1 50); do
    for id in $(attention_chat_ids); do [ "$id" != "$first" ] && second=$id; done
    [ -n "$second" ] && break
    sleep 0.2
  done
  [ -n "$second" ] || fail "the worktree holds no second chat: $(attention_chat_ids | tr '\n' ' ')"
  SURFACE=$second
  wait_row "the second chat is on screen" "$second" onScreen true 10
  send_prompt second
  wait_panel "the second chat's turn ended" 'c["status"] == "completed"' 30
  look_at_shown
  [ "$(attention_row "$first" unread)" = false ] || fail "the first chat reads unread: its turn ended while it was shown"
  [ "$(attention_row "$second" unread)" = false ] || fail "the second chat reads unread: its turn ended while it was shown"
  echo "OK: both chats ran a turn on screen and neither is unread"

  echo "step: Sirio starts again; a restored chat is not news"
  stop_app
  launch unread_restored_chat_quiet
  for _ in $(seq 1 100); do
    [ "$(attention_chat_ids | wc -l | tr -d ' ')" = 2 ] && break
    sleep 0.3
  done
  [ "$(attention_chat_ids | wc -l | tr -d ' ')" = 2 ] || fail "the relaunch did not restore both chats: $(attention_chat_ids | tr '\n' ' ')"
  local hidden=""
  for id in $first $second; do
    [ "$(attention_row "$id" onScreen)" = false ] && hidden=$id
  done
  [ -n "$hidden" ] || fail "no restored chat is off screen: $(reply surface attention read | tr '\n' ' ')"
  echo "OK: both chats came back; $hidden is the one not shown"
  sleep 3
  [ "$(attention_row "$hidden" unread)" = false ] || fail "the restored chat that is not shown reads unread"
  [ "$(posted_count)" = 0 ] || fail "the relaunch posted a notification for a seen chat: $(posted_rows | tr '\n' ' ')"
  echo "step: each chat is shown in turn"
  local position shown_in_turn=0
  for position in $(tab_positions chat); do
    ctl surface tabs select "$position" >/dev/null || fail "selecting tab $position was refused"
    look_at_shown
    sleep 1
    [ "$(attention_row "$first" unread)" = false ] && [ "$(attention_row "$second" unread)" = false ] \
      || fail "showing tab $position marked a chat unread: $(reply surface attention read | tr '\n' ' ')"
    shown_in_turn=$((shown_in_turn + 1))
  done
  [ "$shown_in_turn" = 2 ] || fail "$shown_in_turn chat tab(s) were shown in turn, not both"
  [ "$(posted_count)" = 0 ] || fail "showing the restored chats posted a notification: $(posted_rows | tr '\n' ' ')"
  echo "OK: a seen chat stays quiet over a restart"

  echo "step: a turn ends unseen, with another worktree selected, and Sirio quits"
  local shown=""
  for id in $first $second; do
    [ "$(attention_row "$id" onScreen)" = true ] && shown=$id
  done
  [ -n "$shown" ] || fail "no chat is shown to run the last turn in"
  SURFACE=$shown
  send_prompt request
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  [ "$(attention_row "$shown" status)" = running ] || fail "the chat does not read running right after the select (status '$(attention_row "$shown" status)'), so the stop would not be off screen"
  wait_row "the parked chat's turn ended" "$shown" status done 30
  wait_row "the parked chat's stop marked it" "$shown" unread true 5
  # The write reaches the database in the same drain; a moment for the disk, then quit.
  sleep 1
  stop_app
  launch unread_restored_chat_quiet
  wait_row "the chat that ended unseen is still unread after the relaunch" "$shown" unread true 30
  [ "$(posted_count)" = 0 ] || fail "a saved mark posted a notification after the relaunch: $(posted_rows | tr '\n' ' ')"
  local other=$first
  [ "$shown" = "$first" ] && other=$second
  [ "$(attention_row "$other" unread)" = false ] || fail "the other chat reads unread after the relaunch"
  save_attention unread_restored_chat_quiet
  echo "OK: the saved mark survived the restart and nothing was posted"
  echo "step: looking at the chat clears the mark for good"
  ctl select-workspace --workspace "$LINKED" >/dev/null
  look_at_shown
  wait_row "showing the chat's worktree cleared the saved mark" "$shown" unread false 10
  sleep 1
  stop_app
  launch unread_restored_chat_quiet
  wait_row "the seen chat is still seen after another relaunch" "$shown" unread false 30
  sleep 2
  [ "$(attention_row "$shown" unread)" = false ] || fail "the cleared mark came back after a relaunch"
  [ "$(posted_count)" = 0 ] || fail "the relaunch posted a notification for a seen chat: $(posted_rows | tr '\n' ' ')"
  echo "OK: a mark cleared by looking stays cleared over a restart"
  stop_app
}

# ---- notification_click: a private session bus with a recording notification server ----
# A dbus-daemon of the run's own: a private runtime dir, and an XDG_DATA_DIRS that names an empty
# directory, so no service file exists and nothing can be activated on the bus (the user's own
# notification daemon, if any, is not reachable from here). It carries the run marker, so
# stop_run_processes stops it with everything else.
start_private_bus() {
  local bus_runtime="$RUN_DIR/bus-runtime" bus_data="$RUN_DIR/bus-data" bus_home="$RUN_DIR/bus-home"
  mkdir -p "$bus_data/empty-services" "$bus_home"
  mkdir -m 700 "$bus_runtime"
  [ "${#bus_runtime}" -lt 90 ] || fail "the private bus socket path is too long: $bus_runtime/bus"
  local address="unix:path=$bus_runtime/bus"
  env -i PATH=/usr/local/bin:/usr/bin:/bin "HOME=$bus_home" "XDG_RUNTIME_DIR=$bus_runtime" \
    "XDG_DATA_HOME=$bus_data" "XDG_DATA_DIRS=$bus_data/empty-services" "XDG_CONFIG_HOME=$bus_home" \
    "SIRIO_CSP_E2E_RUN=$RUN_DIR" \
    dbus-daemon --session --fork --address="$address" >>"$RUN_DIR/bus.log" 2>&1 \
    || fail "could not start a private session bus"
  [ -S "$bus_runtime/bus" ] || fail "the private session bus has no socket at $bus_runtime/bus"
  APP_SESSION_BUS=$address
  echo "OK: a private session bus at $address (no service files: nothing is activated on it)"
}

# The recording notification server, owning org.freedesktop.Notifications on that bus. It prints
# `ready` once it owns the name.
start_notification_recorder() {
  : >"$RUN_DIR/notifications.jsonl"
  env -i PATH=/usr/local/bin:/usr/bin:/bin "HOME=$RUN_DIR/bus-home" "DBUS_SESSION_BUS_ADDRESS=$APP_SESSION_BUS" \
    "SIRIO_CSP_E2E_RUN=$RUN_DIR" \
    python3 -I "$ROOT/Scripts/Tests/fake_notifications.py" serve --log "$RUN_DIR/notifications.jsonl" \
    >>"$RUN_DIR/notifications-server.log" 2>&1 &
  local _
  for _ in $(seq 1 50); do
    grep -q '^ready$' "$RUN_DIR/notifications-server.log" 2>/dev/null && break
    sleep 0.2
  done
  grep -q '^ready$' "$RUN_DIR/notifications-server.log" 2>/dev/null \
    || fail "the recording notification server never owned org.freedesktop.Notifications: $(cat "$RUN_DIR/notifications-server.log" 2>/dev/null | tr '\n' ' ')"
  echo "OK: the recording notification server owns org.freedesktop.Notifications on the private bus"
}

# The id of the first recorded Notify that is Sirio's, names $1 in its body and offers the default
# action; nothing when there is none yet.
recorded_notification_id() { # branch
  python3 -I -c 'import json, sys
for line in open(sys.argv[1]):
    row = json.loads(line)
    if row["app_name"] == "Sirio" and sys.argv[2] in row["body"] and "default" in row["actions"]:
        print(row["id"])
        break' "$RUN_DIR/notifications.jsonl" "$1"
}
recorded_count() { grep -c . "$RUN_DIR/notifications.jsonl" 2>/dev/null || true; }

# The path `current-workspace` reports, compared as a real path.
current_workspace_is() { # path
  python3 -I -c 'import json, os, sys
reply = json.loads(sys.argv[1])
reply = reply[0] if isinstance(reply, list) else reply
sys.exit(0 if os.path.realpath(reply.get("path", "")) == os.path.realpath(sys.argv[2]) else 1)' "$(reply current-workspace)" "$1"
}

scenario_notification_click() {
  start_private_bus
  start_notification_recorder
  # The one launch that gets the bus: launch() reads APP_SESSION_BUS.
  unread_chat notification_click 1000
  wait_panel "the chat is ready for a prompt" 'c["status"] in ("idle", "completed")' 30
  send_prompt request
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  [ "$(attention_chat status)" = running ] || fail "the chat does not read running right after the select (status '$(attention_chat status)'), so the stop would not be off screen"
  echo "step: the worktree was left mid-turn; the turn ends with the chat parked"
  wait_attention "the parked chat's turn ended" status done 30
  wait_attention "the parked chat's stop marked it" unread true 5
  local id="" _
  for _ in $(seq 1 100); do
    id=$(recorded_notification_id feat/login)
    [ -n "$id" ] && break
    sleep 0.2
  done
  [ -n "$id" ] || fail "the recording server holds no Notify from Sirio naming feat/login with a default action: $(cat "$RUN_DIR/notifications.jsonl" | tr '\n' ' '); app log tail: $(tail -n 5 "$RUN_DIR/notification_click-app.log" | tr '\n' ' ')"
  sleep 1
  [ "$(recorded_count)" = 1 ] || fail "the recording server holds $(recorded_count) Notify call(s), not one: $(cat "$RUN_DIR/notifications.jsonl" | tr '\n' ' ')"
  echo "OK: the server was sent one Notify (id $id): $(cat "$RUN_DIR/notifications.jsonl")"
  # The click must be what moves the app: another worktree is selected and the chat is unread.
  current_workspace_is "$PRIMARY" || fail "the selected worktree is not $PRIMARY before the click: $(reply current-workspace)"
  [ "$(attention_chat onScreen)" = false ] || fail "the chat reads onScreen before the click"
  [ "$(attention_chat unread)" = true ] || fail "the chat does not read unread before the click"
  echo "step: the server sends ActionInvoked(id, \"default\"), as a desktop does for a click"
  # The app's thread adds its ActionInvoked match rule right after the Notify reply, so a signal
  # sent in that gap reaches nobody: send it again until the selection moves. A repeat after the
  # app has taken the first one is ignored (its thread stops listening at the first).
  local clicked=0 try
  for try in 1 2 3 4 5 6; do
    env -i PATH=/usr/local/bin:/usr/bin:/bin "HOME=$RUN_DIR/bus-home" "DBUS_SESSION_BUS_ADDRESS=$APP_SESSION_BUS" \
      python3 -I "$ROOT/Scripts/Tests/fake_notifications.py" click --id "$id" >>"$RUN_DIR/notifications-click.log" 2>&1 \
      || fail "the recording server refused the click: $(tail -n 3 "$RUN_DIR/notifications-click.log" | tr '\n' ' ')"
    for _ in $(seq 1 15); do
      current_workspace_is "$LINKED" && { clicked=1; break; }
      sleep 0.2
    done
    [ "$clicked" = 1 ] && break
  done
  [ "$clicked" = 1 ] || fail "$try click(s) on the notification never selected $LINKED: $(reply current-workspace); app log tail: $(tail -n 5 "$RUN_DIR/notification_click-app.log" | tr '\n' ' ')"
  echo "OK: the click selected $LINKED (after $try signal(s))"
  wait_attention "the clicked session is the one shown" onScreen true 10
  [ "$(tabs_state | cut -d' ' -f2)" = "$(tab_positions chat | tr ' ' '\n' | head -n 1)" ] \
    || fail "the active tab is not the chat's: $(tabs_state); chat tabs at $(tab_positions chat)"
  echo "OK: the active tab is the chat"
  # A --state-only run paints no frame, and a shown session is marked seen by the frame that draws
  # it: this runs the same marking without one.
  look_at_shown
  wait_attention "seeing the clicked chat cleared its mark" unread false 10
  [ "$(recorded_count)" = 1 ] || fail "the click or the clearing posted another notification: $(cat "$RUN_DIR/notifications.jsonl" | tr '\n' ' ')"
  cp "$RUN_DIR/notifications.jsonl" "$OUT_DIR/notifications.jsonl"
  [ -s "$OUT_DIR/notifications.jsonl" ] || fail "no notifications.jsonl was written"
  save_attention notification_click
  echo "OK: notification_click: one Notify reached the D-Bus server, and its click opened the parked chat ($OUT_DIR/notifications.jsonl)"
  APP_SESSION_BUS=""
  stop_app
}

scenario_elicitation_form() {
  elicit_chat elicitation_form
  send_prompt form
  wait_panel "the form is open on its first field" 'elicit().get("status") == "pending" and elicit()["mode"] == "form" and elicit()["fields"] == "colour|count|note|ok" and elicit()["rows"] == "red|blue|Decline"' 30
  wait_attention "an open elicitation needs the user" status needs-input 10
  elicit_row 2
  wait_panel "the form moved to the count field" 'elicit()["step"] == "1"' 10
  elicit_text abc
  wait_panel "abc was refused on the count field, which stays" 'elicit()["error"] != "" and elicit()["step"] == "1" and elicit()["status"] == "pending"' 10
  elicit_text 2
  wait_panel "2 was taken as the count" 'elicit()["step"] == "2" and elicit()["error"] == ""' 10
  elicit_skip
  wait_panel "the note was skipped" 'elicit()["step"] == "3"' 10
  elicit_skip
  wait_panel "the review offers Accept and Decline" 'elicit()["rows"] == "Accept|Decline"' 10
  elicit_row 1
  wait_panel "the form was accepted" 'elicit()["status"] == "accepted"' 10
  wait_panel "the turn ended" 'c["status"] == "completed"' 30
  elicit_log_check "the agent heard accept with colour blue and count 2, and nothing else" 'len(answers) == 1 and answers[0]["response"] == {"action": "accept", "content": {"colour": "blue", "count": 2}}'
  stop_app
}

scenario_elicitation_url() {
  stub_openers_lead_login_path elicitation_url
  elicit_chat elicitation_url
  send_prompt url
  wait_panel "the sign-in link is open" 'elicit().get("status") == "pending" and elicit()["mode"] == "url"' 30
  wait_panel "the dock offers to open example.com" 'elicit()["openOffered"] == "true" and elicit()["host"] == "example.com"' 10
  elicit_row 1
  elicit_url_not_opened
  wait_panel "the server's elicitation_complete closed the request" 'elicit()["status"] == "completed"' 15
  wait_panel "the turn ended" 'c["status"] == "completed"' 30
  elicit_log_check "the agent heard accept with no content" 'len(answers) == 1 and answers[0]["response"] == {"action": "accept"}'
  stop_app
}

scenario_elicitation_withdrawn() {
  elicit_chat elicitation_withdrawn
  send_prompt withdraw
  wait_panel "the form was withdrawn by the agent" 'elicit().get("status") == "withdrawn"' 30
  wait_panel "the turn ended" 'c["status"] == "completed"' 30
  wait_panel "the withdrawn form still says so after the turn" 'elicit()["status"] == "withdrawn" and elicit()["rows"] == ""' 5
  # A late answer would land after the turn: give it a second to show in the log.
  sleep 1
  elicit_log_check "nothing answered the withdrawn request" 'answers == []'
  stop_app
}

scenario_unhandled_request() {
  elicit_chat unhandled_request
  send_prompt unknown
  wait_panel "the turn ended" 'c["status"] == "completed"' 30
  elicit_log_check "the agent heard an error naming future_thing" 'len(answers) == 1 and answers[0]["subtype"] == "error" and "future_thing" in answers[0]["error"]'
  stop_app
}

scenario_session_state_background() {
  use_stub session-state
  # The gate lives in the run's scratch directory; the stub hands it to the fixture, which holds
  # `background` after its main result until the file exists, and again until `<gate>.2`.
  SIRIO_FIXTURE_GATE="$RUN_DIR/session-state.gate"
  rm -f "$SIRIO_FIXTURE_GATE" "$SIRIO_FIXTURE_GATE.2"
  launch session_state_background
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  open_column
  send_prompt background
  wait_panel "the main result ended the turn" 'c["status"] == "completed"' 30
  wait_attention "the CLI still runs its background agent after the turn's result" status running 10
  wait_panel "the CLI said running" 'c["sessionState"] == "running"' 10
  sleep 2
  [ "$(attention_chat status)" = running ] || fail "the chat stopped reading running while the CLI's background agent works: $(attention_chat status)"
  echo "OK: the chat still reads running two seconds after the turn ended"
  touch "$SIRIO_FIXTURE_GATE"
  # The CLI's own follow-up turn (task_updated, no tasks, its own text) runs between the gates.
  # Wait until it provably started and the background agent ended: the turn's own state is Done
  # from here, so only the CLI's `running` can keep the status and the card Live.
  wait_panel "the agent ended and the CLI's follow-up turn spoke" \
    'lane("toolu_bg_1") is not None and lane("toolu_bg_1")["tag"] == "done" and any(r.get("kind") == "assistant" and "agent reported done" in r.get("text", "") for r in c["transcript"])' 15
  wait_panel "the card still reads Live on the CLI's running alone" 'S("trace")["summary"].startswith("Live")' 10
  [ "$(attention_chat status)" = running ] || fail "the chat read $(attention_chat status) during the CLI's follow-up turn"
  sleep 2
  [ "$(attention_chat status)" = running ] || fail "the chat read $(attention_chat status) two seconds into the CLI's follow-up turn"
  check_panel "the card is still Live two seconds on" 'S("trace")["summary"].startswith("Live")'
  touch "$SIRIO_FIXTURE_GATE.2"
  wait_attention "the CLI went idle" status done 10
  wait_panel "the CLI said idle" 'c["sessionState"] == "idle"' 10
  check_panel "the card reads Done once the CLI is idle" 'S("trace")["summary"].startswith("Done")'
  echo "OK: session_state_background: a chat stays running while the CLI's background agent works, and reads done once the CLI goes idle"
  stop_app
}

scenario_session_state_ask() {
  use_stub session-state
  launch session_state_ask
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  open_column
  send_prompt ask
  wait_panel "the CLI said requires_action" 'c["sessionState"] == "requires_action"' 30
  wait_attention "the CLI waits for the user" status needs-input 10
  wait_panel "the card reads Waiting for you" 'S("trace") is not None and S("trace")["summary"].startswith("Waiting for you")' 10
  answer_permission allow_once
  wait_panel "the CLI went idle" 'c["sessionState"] == "idle"' 30
  wait_attention "the answered chat is done" status done 10
  check_panel "the card reads Done" 'S("trace")["summary"].startswith("Done")'
  echo "OK: session_state_ask: the CLI's requires_action reads needs-input and Waiting for you, and its idle after the answer reads done"
  stop_app
}

scenario_session_state_unprompted() {
  use_stub session-state
  SIRIO_FIXTURE_DONE="$RUN_DIR/session-state.done"
  rm -f "$SIRIO_FIXTURE_DONE"
  launch session_state_unprompted
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  open_column
  send_prompt monitor
  wait_attention "the first turn is done" status done 30
  # The CLI's own turns start two seconds after its idle: park the chat before they do.
  ctl select-workspace --workspace "$PRIMARY" >/dev/null
  [ "$(attention_chat onScreen)" = false ] || fail "the chat reads onScreen after the select"
  [ "$(attention_chat unread)" = false ] || fail "the chat reads unread before the CLI's own turns"
  local samples=0 status _
  # Until the fixture writes its marker after the third turn's idle (about 6 s on, 60 s at most).
  for _ in $(seq 1 200); do
    [ -e "$SIRIO_FIXTURE_DONE" ] && break
    status=$(attention_chat status)
    samples=$((samples + 1))
    [ "$status" = running ] && fail "the chat read running during a turn the CLI started on its own (sample $samples)"
    [ "$(attention_chat unread)" = false ] || fail "a turn the CLI started on its own marked the parked chat unread (sample $samples)"
    sleep 0.15
  done
  [ -e "$SIRIO_FIXTURE_DONE" ] || fail "the fixture never finished the CLI's own turns"
  [ "$samples" -gt 0 ] || fail "the CLI's own turns ended before the chat was parked: nothing was sampled"
  echo "OK: $samples samples through the CLI's three own turns never read running or unread"
  [ "$(attention_chat status)" = done ] || fail "the chat does not read done after the CLI's own turns: $(attention_chat status)"
  [ "$(attention_chat unread)" = false ] || fail "the parked chat reads unread after the CLI's own turns"
  # The parked chat has no surface to read: bring it back to see that the turns happened.
  ctl select-workspace --workspace "$LINKED" >/dev/null
  wait_panel "the third turn of the CLI's own spoke" 'any("monitor event 3" in r.get("text", "") for r in c["transcript"])' 10
  wait_panel "the CLI is idle again" 'c["sessionState"] == "idle"' 10
  echo "OK: session_state_unprompted: turns the CLI starts on its own leave the status done and the parked chat unmarked"
  stop_app
}

# ---- the background_* scenarios (A3) ---------------------------------------------------
# A fresh Sirio on the background-tasks fixture with a Claude chat open.
background_chat() { # name
  use_stub background-tasks
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  open_column
}

# How many control requests of one subtype the fixture heard in the named launch.
fixture_requests() { # launch-name subtype
  python3 -c '
import json, sys
count = 0
try:
    for line in open(sys.argv[1]):
        if json.loads(line)["request"]["subtype"] == sys.argv[2]:
            count += 1
except FileNotFoundError:
    pass
print(count)' "$RUN_DIR/$1-requests.jsonl" "$2"
}

# Says to the chat that a frame painted the task's tail, as a drawn tail does.
paint_tail() { # task-id
  E2E rpc surface.chat.paintTail surfaceId="$SURFACE" taskId="$1" >/dev/null || fail "painting the tail of $1 failed"
}

stop_task() { # task-id
  E2E rpc surface.chat.stopTask surfaceId="$SURFACE" taskId="$1" >/dev/null || fail "stopping $1 failed"
}

scenario_background_shell() {
  background_chat background_shell
  send_prompt shell
  wait_panel "bshell001 is a live shell" 'bg("bshell001") is not None and bg("bshell001")["live"] and bg("bshell001")["kind"] == "shell" and bg("bshell001")["toolUseId"] == "toolu_s1"' 30
  paint_wait "the painted tail reaches line 3, its colour codes stripped" bshell001 20 '"line 3" in bg("bshell001")["tail"] and all("\x1b" not in line for line in bg("bshell001")["tail"])'
  check_panel "the CLI has not been refused: the tail is not retired" 'c["taskOutputRetired"] == "false"'
  stop_task bshell001
  check_panel "the entry reads stopping" 'bg("bshell001")["stopping"] and bg("bshell001")["outcome"] is None'
  stop_task bshell001
  wait_panel "the first terminal status is its outcome: killed" 'bg("bshell001")["outcome"] == "killed" and not bg("bshell001")["stopping"]' 15
  wait_panel "the CLI's rows no longer hold it: not live" 'not bg("bshell001")["live"]' 15
  paint_wait "the painted tail ends [killed]" bshell001 15 'bg("bshell001")["tail"][-1:] == ["[killed]"]'
  [ "$(fixture_requests background_shell stop_task)" = 1 ] || fail "the fixture heard $(fixture_requests background_shell stop_task) stop_task requests, not exactly one"
  # With its trailer read, the ended shell is read no more, painted or not.
  # Painted for 2 s only, inside the 5 s window the ended shell is read in, so that the trailer rule
  # and not the window's end is what stops the reads.
  local reads
  reads=$(fixture_requests background_shell get_task_output)
  for _ in $(seq 1 8); do paint_tail bshell001; sleep 0.25; done
  [ "$(fixture_requests background_shell get_task_output)" = "$reads" ] || fail "an ended shell whose trailer was read was read again"
  echo "OK: background_shell: a live shell's painted tail reaches line 3, Stop reads stopping, killed, not live with [killed] ending the tail, and sent one stop_task"
  stop_app
}

scenario_background_shell_unpainted() {
  background_chat background_shell_unpainted
  send_prompt shell
  wait_panel "bshell001 is a live shell" 'bg("bshell001") is not None and bg("bshell001")["live"]' 30
  sleep 4
  # The log exists and holds the handshakes (the usage probe's and the chat's own processes share it, so
  # more than one): a missing log would read as no request at all.
  [ "$(fixture_requests background_shell_unpainted initialize)" -ge 1 ] || fail "the fixture's request log holds no initialize"
  [ "$(fixture_requests background_shell_unpainted get_task_output)" = 0 ] || fail "the chat read a task's output while no frame painted it: $(fixture_requests background_shell_unpainted get_task_output) get_task_output requests"
  check_panel "no tail was kept for an unpainted task" 'bg("bshell001")["tail"] == []'
  echo "OK: background_shell_unpainted: four seconds of a live shell that nothing paints cost the CLI no get_task_output"
  stop_app
}

scenario_background_shell_floor() {
  SIRIO_FIXTURE_FLOOR=1
  background_chat background_shell_floor
  send_prompt shell
  wait_panel "bshell001 is a live shell" 'bg("bshell001") is not None and bg("bshell001")["live"]' 30
  paint_wait "the floor's refusal retired the tail" bshell001 20 'c["taskOutputRetired"] == "true"'
  for _ in $(seq 1 12); do paint_tail bshell001; sleep 0.25; done
  [ "$(fixture_requests background_shell_floor get_task_output)" = 1 ] || fail "the fixture heard $(fixture_requests background_shell_floor get_task_output) get_task_output requests, not exactly one"
  check_panel "a retired tail shows nothing" 'bg("bshell001")["tail"] == [] and bg("bshell001")["tailNote"] is None'
  stop_task bshell001
  wait_panel "Stop ends the task on the floor too: killed, not live" 'bg("bshell001")["outcome"] == "killed" and not bg("bshell001")["live"] and not bg("bshell001")["stopping"]' 15
  [ "$(fixture_requests background_shell_floor stop_task)" = 1 ] || fail "the fixture heard $(fixture_requests background_shell_floor stop_task) stop_task requests, not exactly one"
  echo "OK: background_shell_floor: the floor refused one read, the tail is retired quietly, and Stop still ends the task"
  stop_app
}

scenario_background_agent_stop() {
  background_chat background_agent_stop
  send_prompt agent
  wait_panel "aagent001 is a live agent" 'bg("aagent001") is not None and bg("aagent001")["live"] and bg("aagent001")["kind"] == "agent" and bg("aagent001")["toolUseId"] == "toolu_a1"' 30
  for _ in $(seq 1 8); do paint_tail aagent001; sleep 0.25; done
  [ "$(fixture_requests background_agent_stop get_task_output)" = 0 ] || fail "an agent's output was asked for: the CLI refuses it"
  stop_task aagent001
  wait_panel "Stop ends the agent: killed, not live" 'bg("aagent001")["outcome"] == "killed" and not bg("aagent001")["live"] and not bg("aagent001")["stopping"]' 15
  [ "$(fixture_requests background_agent_stop stop_task)" = 1 ] || fail "the fixture heard $(fixture_requests background_agent_stop stop_task) stop_task requests, not exactly one"
  echo "OK: background_agent_stop: a background agent is never read, and Stop ends it killed"
  stop_app
}

scenario_background_ended_dropped() {
  background_chat background_ended_dropped
  send_prompt shell
  wait_panel "bshell001 is a live shell" 'bg("bshell001") is not None and bg("bshell001")["live"]' 30
  stop_task bshell001
  wait_panel "the stopped shell has ended" 'bg("bshell001")["outcome"] == "killed" and not bg("bshell001")["live"]' 15
  wait_panel "the CLI is idle again" 'c["sessionState"] == "idle"' 10
  check_panel "the ended entry is still listed before the next prompt" 'bg("bshell001") is not None and bg("bshell001")["outcome"] == "killed"'
  send_prompt plain
  wait_panel "the next prompt dropped the ended entry" 'c["backgroundTasks"] == []' 15
  echo "OK: background_ended_dropped: an ended task stays until the next prompt, which drops it"
  stop_app
}

# How many get_task_output requests the fixture heard in the named launch, once SECONDS have passed
# with nothing asked of it: the log is sampled twice, 3 s apart, after a pause that lets a read in
# flight at the fold land (it is logged when sent, so at most one late send follows).
expect_reads_stop() { # launch-name desc
  local before after
  sleep 2.5
  before=$(fixture_requests "$1" get_task_output)
  sleep 3
  after=$(fixture_requests "$1" get_task_output)
  [ "$before" = "$after" ] || fail "$2: the log grew from $before to $after get_task_output requests"
  echo "OK: $2 ($after requests, none new in 3 s)"
}

scenario_background_tail_card() {
  background_chat background_tail_card
  send_prompt shell
  wait_panel "bshell001 is a live shell" 'bg("bshell001") is not None and bg("bshell001")["live"]' 30
  wait_panel "the Trace card is drawn, folded" "S('trace') is not None and S('trace')['expanded'] is False" 30
  sleep 3
  [ "$(fixture_requests background_tail_card get_task_output)" = 0 ] || fail "a folded card read the shell: $(fixture_requests background_tail_card get_task_output) get_task_output requests in 3 s"
  echo "OK: the folded card cost the CLI no get_task_output"
  unfold_cards trace
  wait_panel "the unfolded card's row reaches line 2 or later" 'R("trace", "trace/background/bshell001") is not None and re.match(r"line [2-9]", R("trace", "trace/background/bshell001")["note"])' 30
  [ "$(fixture_requests background_tail_card get_task_output)" -ge 1 ] || fail "the card shows a tail but the log holds no get_task_output"
  fold_cards trace
  expect_reads_stop background_tail_card "the folded card stopped reading"
  echo "OK: background_tail_card: no reads folded, the unfolded card's row shows the tail, folding stops the reads"
  stop_app
}

scenario_background_tail_tab() {
  background_chat background_tail_tab
  send_prompt shell
  wait_panel "bshell001 is a live shell" 'bg("bshell001") is not None and bg("bshell001")["live"]' 30
  wait_panel "the Trace card is drawn, folded" "S('trace') is not None and S('trace')['expanded'] is False" 30
  sleep 3
  [ "$(fixture_requests background_tail_tab get_task_output)" = 0 ] || fail "a chat with its card folded read the shell"
  panel_ctl activate trace/open >/dev/null
  wait_trace_tab "the layout lists a trace tab" present 20
  wait_panel "the tab's strip carries the shell's tail, line 2 or later, nothing else new" 'any(t["taskId"] == "bshell001" and t["tail"] and re.match(r"line [2-9]", t["tail"][-1]) for t in tab()["background"])' 30
  [ "$(fixture_requests background_tail_tab get_task_output)" -ge 1 ] || fail "the tab shows a tail but the log holds no get_task_output"
  echo "step: the trace tab is closed; the card stays folded"
  ctl surface tabs close "$(tab_positions trace)" --confirm >/dev/null
  wait_trace_tab "the trace tab is closed" absent 20
  expect_reads_stop background_tail_tab "the closed tab stopped reading"
  echo "OK: background_tail_tab: the tab's strip shows the tail while it is drawn, closing it stops the reads"
  stop_app
}

scenario_background_stop_click() {
  SIRIO_FIXTURE_STOP_DELAY=5
  background_chat background_stop_click
  send_prompt shell
  wait_panel "bshell001 is a live shell" 'bg("bshell001") is not None and bg("bshell001")["live"]' 30
  unfold_cards trace
  wait_panel "the group's row for it is running and carries its Stop" 'R("trace", "trace/background/bshell001") is not None and R("trace", "trace/background/bshell001")["tag"] == "running" and R("trace", "trace/background/bshell001")["stop"] == "bshell001"' 30
  local xy
  xy=$(E2E stop-button-xy "$SURFACE" 15 bshell001) || fail "the Stop button was never drawn"
  echo "step: a real click on the Stop button of the row, drawn at window pixel $xy"
  click_at_window "$xy"
  wait_panel "the row reads stopping" 'R("trace", "trace/background/bshell001")["tag"] == "stopping"' 5
  echo "step: a second real click at the same place"
  click_at_window "$xy"
  wait_panel "the row reads interrupted and has no Stop left" 'R("trace", "trace/background/bshell001")["tag"] == "interrupted" and R("trace", "trace/background/bshell001")["stop"] is None' 15
  [ "$(fixture_requests background_stop_click stop_task)" = 1 ] || fail "the fixture heard $(fixture_requests background_stop_click stop_task) stop_task requests, not exactly one"
  echo "OK: background_stop_click: a real click on the row's Stop reads stopping, then interrupted, a second click sent nothing, one stop_task"
  stop_app
}

scenario_background_lane_stop() {
  background_chat background_lane_stop
  send_prompt agent
  wait_panel "aagent001 is a live agent" 'bg("aagent001") is not None and bg("aagent001")["live"]' 30
  unfold_cards trace
  wait_panel "the agent's lane carries Stop" 'lane("toolu_a1") is not None and lane("toolu_a1")["stop"] == "aagent001"' 30
  check_panel "the group holds no second row for it" 'R("trace", "trace/background/aagent001") is None'
  panel_ctl activate trace/stop/aagent001 >/dev/null
  wait_panel "Stop ends the agent: killed, not live" 'bg("aagent001")["outcome"] == "killed" and not bg("aagent001")["live"]' 15
  check_panel "its lane has no Stop left" 'lane("toolu_a1")["stop"] is None'
  [ "$(fixture_requests background_lane_stop get_task_output)" = 0 ] || fail "an agent's output was asked for"
  [ "$(fixture_requests background_lane_stop stop_task)" = 1 ] || fail "the fixture heard $(fixture_requests background_lane_stop stop_task) stop_task requests, not exactly one"
  echo "OK: background_lane_stop: the agent's lane has the Stop, the group no row, and the agent is never read"
  stop_app
}

# ---- the rename_* scenarios (A4) -----------------------------------------------------
# The marker rename_requests counts from: the fixture's control log is shared by every process of the
# run (the chat's, the usage probe's), so a scenario counts only what came after its own start.
RENAME_MARK=0
control_log_lines() {
  local lines
  lines=$(wc -l <"$RUN_DIR/fixture-control.log" 2>/dev/null) || lines=0
  echo $((lines))
}

# A fresh Sirio on the rename stub, with a Claude chat open and its CLI connected. SIRIO_FIXTURE_RENAME_ERROR
# and SIRIO_FIXTURE_HANDSHAKE_GATE are set by the scenario before it calls this.
rename_chat() { # name [no-wait]
  use_stub rename
  CALLS_MARK=$(wc -l <"$RUN_DIR/stub-calls.log" 2>/dev/null) || CALLS_MARK=0
  CALLS_MARK=$((CALLS_MARK))
  SIRIO_FIXTURE_TITLE_STORE="$RUN_DIR/$1-titles.json"
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  open_column
  RENAME_MARK=$(control_log_lines)
  [ "${2:-}" = no-wait ] || wait_panel "the chat is connected" 'c["status"] in ("idle", "completed")' 30
}

# Waits until the CLI has heard that many rename_session requests since the marker.
wait_renames() { # count
  local _
  for _ in $(seq 1 60); do
    [ "$(rename_requests | python3 -c 'import json, sys; print(len(json.load(sys.stdin)))')" -ge "$1" ] && return 0
    sleep 0.25
  done
  fail "the CLI never heard $1 rename_session requests (heard $(rename_requests))"
}

# The rename_session requests the CLI heard since the marker, as one JSON array.
rename_requests() {
  python3 -c '
import json, sys
mark, requests = int(sys.argv[2]), []
try:
    lines = open(sys.argv[1], encoding="utf-8").read().splitlines()
except FileNotFoundError:
    lines = []
for line in lines[mark:]:
    request = json.loads(line)
    if request.get("subtype") == "rename_session":
        requests.append(request)
print(json.dumps(requests))' "$RUN_DIR/fixture-control.log" "$RENAME_MARK"
}

# check_renames DESC EXPR: EXPR over `r`, the rename_session requests since the marker.
check_renames() {
  local requests
  requests=$(rename_requests)
  python3 -c '
import json, sys
r = json.loads(sys.argv[1])
sys.exit(0 if eval(sys.argv[2]) else 1)' "$requests" "$2" || fail "$1 (requests: $requests)"
  echo "OK: $1"
}

# The listed tab's title, as `surface tabs read` gives it.
tab_title() { # position
  reply surface tabs read | python3 -c '
import json, sys
print(json.load(sys.stdin)[0]["tab." + sys.argv[1]].split("|", 2)[2])' "$1"
}

check_tab_title() { # position title
  local held
  held=$(tab_title "$1")
  [ "$held" = "$2" ] || fail "tab $1 is titled '$held', not '$2'"
  echo "OK: tab $1 is titled '$2'"
}

rename_tab() { # position title
  ctl surface tabs rename "$1" --title "$2" >/dev/null || fail "renaming tab $1 to '$2' failed"
}

# Sends a prompt and waits for the turn it starts to end, whatever the transcript held before it.
rename_turn() {
  local before
  before=$(find_panel "the assistant rows so far" 'str(len([r for r in c["transcript"] if r.get("kind") == "assistant"]))' 10)
  send_prompt hello
  wait_panel "the turn ended" "len([r for r in c['transcript'] if r.get('kind') == 'assistant']) > $before and c['status'] in ('idle', 'completed')" 30
}

# The --session-ids chats launched with since rename_chat began (the lines with --input-format and --session-id;
# the usage probe has no session id), as a python list. `open_chat` opens a second chat and closes the first,
# so which one the surface is cannot be told from the log's order: any of them is the chat's own.
launched_session_ids() {
  tail -n +$((CALLS_MARK + 1)) "$RUN_DIR/stub-calls.log" | grep -e '--input-format' | grep -e '--session-id' \
    | sed -E 's/.*--session-id ([^ ]+).*/\1/' | python3 -c 'import sys; print(sorted(set(sys.stdin.read().split())))'
}

scenario_rename_session() {
  rename_chat rename_session
  local launched
  launched=$(launched_session_ids)
  [ "$launched" != "[]" ] || fail "no launch line with --session-id in the stub log"
  echo "step: chats were launched as sessions $launched; renaming before any turn"
  rename_tab 1 "Café ☕ notes"
  check_tab_title 1 "Café ☕ notes"
  wait_panel "the title is accepted" 'title_state() == "accepted" and c["agentTitle"]["title"] == "Café ☕ notes"' 15
  check_renames "one rename_session reached the CLI: the title as typed, from the host, for the launch's own session" \
    "len(r) == 1 and r[0]['title'] == 'Café ☕ notes' and r[0]['source'] == 'host' and r[0]['session_id'] in $launched"
  rename_turn
  rename_tab 1 "Release notes"
  wait_panel "the second title is accepted" 'title_state() == "accepted" and c["agentTitle"]["title"] == "Release notes"' 15
  check_renames "the second rename_session names the session the first turn's init gave" \
    "len(r) == 2 and r[1]['title'] == 'Release notes' and r[1]['session_id'] == 'fixture-session-1'"
  check_tab_title 1 "Release notes"
  echo "OK: rename_session: a manual rename reached Claude Code with its non-ASCII title, before a turn and after one, for the right session"
  stop_app
}

scenario_rename_session_failed() {
  SIRIO_FIXTURE_RENAME_ERROR="session_id is not the current session"
  SIRIO_FIXTURE_RENAME_GATE="$RUN_DIR/rename-gate"
  touch "$SIRIO_FIXTURE_RENAME_GATE"
  rename_chat rename_session_failed
  local notice="Claude Code kept its earlier name: session_id is not the current session"
  rename_tab 1 "First name"
  check_tab_title 1 "First name"
  wait_panel "the title failed with the CLI's own sentence" 'title_state() == "failed" and c["agentTitle"]["error"] == "session_id is not the current session"' 15
  wait_panel "the transcript ends with the warning" "c['transcript'][-1].get('kind') == 'warning' and c['transcript'][-1]['text'] == '$notice' and len([r for r in c['transcript'] if r.get('kind') == 'warning']) == 1" 15
  check_tab_title 1 "First name"
  echo "step: the CLI now holds its answers; two renames go out before either is answered"
  rm -f "$SIRIO_FIXTURE_RENAME_GATE"
  rename_tab 1 "Name A"
  rename_tab 1 "Name B"
  wait_renames 3
  check_panel "both are waiting: the state is sent, holding the last title" 'title_state() == "sent" and c["agentTitle"]["title"] == "Name B"'
  touch "$SIRIO_FIXTURE_RENAME_GATE"
  wait_panel "the last rename failed" 'title_state() == "failed" and c["agentTitle"]["title"] == "Name B"' 15
  check_panel "A's late answer was dropped: one warning more, not two" "len([r for r in c['transcript'] if r.get('kind') == 'warning']) == 2"
  check_renames "an error is that one rename's failure: all three renames were sent" "[x['title'] for x in r] == ['First name', 'Name A', 'Name B']"
  echo "OK: rename_session_failed: an error shows beside the chat, the next renames are still sent, and a stale answer moves nothing"
  stop_app
}

scenario_rename_session_retired() {
  SIRIO_FIXTURE_RENAME_ERROR="Unknown control request subtype: rename_session"
  rename_chat rename_session_retired
  rename_tab 1 "First name"
  wait_panel "the refusal retired the verb" 'title_state() == "retired"' 15
  local rows
  rows=$(find_panel "the transcript's length" 'str(len([r for r in c["transcript"] if r.get("kind") not in ("user", "assistant", "turn")]))' 10)
  rename_tab 1 "Second name"
  check_panel "the state is still retired, holding the second title" 'title_state() == "retired" and c["agentTitle"]["title"] == "Second name"'
  check_tab_title 1 "Second name"
  # The worker serializes what it is told, so a rename it was going to send is written before this turn ends.
  rename_turn
  check_renames "the retired verb sent exactly one request" "len(r) == 1 and r[0]['title'] == 'First name'"
  check_panel "nothing was shown: no entry beside the turn's own, and no warning" "len([r for r in c['transcript'] if r.get('kind') not in ('user', 'assistant', 'turn')]) == $rows and not any(r.get('kind') == 'warning' for r in c['transcript'])"
  echo "OK: rename_session_retired: a refusal retires the verb quietly, the next rename is held, not sent, and the tab keeps its title"
  stop_app
}

scenario_rename_session_connecting() {
  SIRIO_FIXTURE_HANDSHAKE_GATE="$RUN_DIR/gate"
  rm -f "$SIRIO_FIXTURE_HANDSHAKE_GATE"
  rename_chat rename_session_connecting no-wait
  rename_tab 1 "Held name"
  check_tab_title 1 "Held name"
  check_panel "the title waits for the connection" 'title_state() == "pending" and c["agentTitle"]["title"] == "Held name"'
  check_renames "nothing is sent while the handshake is held" "len(r) == 0"
  # The fixture holds the handshake until the gate exists; releasing it is the connection's becoming ready.
  touch "$SIRIO_FIXTURE_HANDSHAKE_GATE"
  wait_panel "the held title is sent on ready and accepted" 'title_state() == "accepted"' 30
  check_renames "the held title was sent once, when the connection came up" "len(r) == 1 and r[0]['title'] == 'Held name' and r[0]['source'] == 'host'"
  echo "OK: rename_session_connecting: a rename made before the CLI was ready waited as pending and went out once on ready"
  stop_app
}

scenario_rename_session_restored() {
  rename_chat rename_session_restored
  rename_tab 1 "Sirio name"
  wait_panel "the title is accepted" 'title_state() == "accepted"' 15
  rename_turn
  # A saved layout is written shortly after a change; a quit flushes it as well (as in `restore`).
  sleep 1
  stop_app
  echo '{"fixture-session-1": "Name from elsewhere"}' >"$SIRIO_FIXTURE_TITLE_STORE"
  echo "step: Sirio starts again on the same database; the CLI's own title for the session is now another"
  RENAME_MARK=$(control_log_lines)
  local calls_mark
  calls_mark=$(wc -l <"$RUN_DIR/stub-calls.log")
  launch rename_session_restored
  SURFACE=""
  for _ in $(seq 1 100); do
    SURFACE=$(E2E chat-surface 2>/dev/null || true)
    [ -n "$SURFACE" ] && break
    sleep 0.3
  done
  [ -n "$SURFACE" ] || fail "the chat was not restored"
  open_column
  wait_panel "the restored chat is connected" 'c["status"] in ("idle", "completed")' 30
  tail -n +$((calls_mark + 1)) "$RUN_DIR/stub-calls.log" | grep -e '--input-format' | grep -q -e '--resume fixture-session-1' || fail "the relaunch did not resume fixture-session-1"
  echo "OK: the relaunch resumed fixture-session-1"
  rename_turn
  check_tab_title 1 "Sirio name"
  check_panel "no title state: the restore holds nothing, and adopted nothing" 'c["agentTitle"] == ""'
  check_renames "the restored chat sent no rename_session" "len(r) == 0"
  echo "OK: rename_session_restored: a restored chat keeps Sirio's name, sends nothing and does not adopt the CLI's"
  stop_app
}

scenario_rename_auto_title_quiet() {
  use_stub rename
  local name=rename_auto_title_quiet
  (cd "$ROOT/rust" && cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$RUN_DIR/$name.sqlite" --auto-naming on ${APPEARANCE:+--appearance "$APPEARANCE"}) || fail "could not seed auto-naming"
  rename_chat "$name"
  send_prompt "fix the login bug"
  # The first-prompt title is not this one: only the summarizer writes `Summarized title`.
  local held="" _
  for _ in $(seq 1 120); do
    held=$(tab_title 1)
    [ "$held" = "Summarized title" ] && break
    sleep 0.5
  done
  [ "$held" = "Summarized title" ] || fail "the summarizer's title never reached the tab (it is '$held')"
  echo "OK: the tab is titled 'Summarized title'"
  check_panel "the summarizer's title is not a manual rename: no title state" 'c["agentTitle"] == ""'
  check_renames "an auto title is never sent to the CLI" "len(r) == 0"
  echo "OK: rename_auto_title_quiet: the summarizer named the tab and Claude Code heard nothing"
  stop_app
}

# ---- the permission_* scenarios (A5) ---------------------------------------------------
# A fresh Sirio on the permissions stub with a Claude chat open and connected. FIELDS (optional) is the JSON
# the fixture merges into every can_use_tool; written to a file in the run's scratch directory.
permissions_chat() { # name [fields-json]
  use_stub permissions
  if [ -n "${2:-}" ]; then
    printf '%s' "$2" >"$RUN_DIR/$1-fields.json"
    SIRIO_FIXTURE_PERMISSION_FIELDS="$RUN_DIR/$1-fields.json"
  fi
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  wait_panel "the chat is connected" 'c["status"] in ("idle", "completed")' 30
}

# Acts on the pending permission card through the debug-only `surface.chat.elicitation`, as a digit key or a
# click would. With several cards pending it takes the first, the one the dock shows (`question_view`). The
# dock ignores a row for half a second after a question appears, and the verb says so: that refusal is tried
# again, any other is a failure, reported as the kind of refusal only (one refusal names the request id, so
# the reply itself is never printed).
dock_rpc() { # description, then the verb's params after requestId
  local desc=$1 id out="" _
  shift
  id=$(find_panel "a permission card is pending" 'next((r["id"] for r in c["transcript"] if r.get("kind") == "permission" and r.get("status") == "pending"), "")' 30)
  for _ in $(seq 1 25); do
    if out=$(E2E rpc surface.chat.elicitation surfaceId="$SURFACE" requestId="$id" "$@" 2>&1); then
      echo "OK: $desc"
      return 0
    fi
    case "$out" in *"not armed"*) sleep 0.2 ;; *) break ;; esac
  done
  local why
  case "$out" in
    *"not the open question"*) why="the card is not the open question" ;;
    *"rows, not row"*) why="no such row" ;;
    *"no free-text row"*) why="no free-text row" ;;
    *"not armed"*) why="still not armed" ;;
    *) why="another error" ;;
  esac
  fail "$desc was refused: $why"
}

# The card's command, as every permission_* scenario's fixture asks for it.
PERMISSION_COMMAND='rm -rf build'

scenario_permission_detail() {
  permissions_chat permission_detail '{"title":"Run the build","description":"make a file","decision_reason_type":"rule","decision_reason_rule_source":"userSettings","blocked_path":"/w/out.txt","mcp_server":{"name":"probe","source":"project"}}'
  send_prompt build
  wait_panel "the card shows the CLI's detail, in order" 'c["questionDock"] != "" and c["questionDock"]["caption"] == "Run the build" and c["questionDock"]["body"] == "'"$PERMISSION_COMMAND"'" and c["questionDock"]["details"] == [{"label": None, "text": "make a file"}, {"label": "Reason", "text": "Matches an ask rule in user settings"}, {"label": "Path", "text": "/w/out.txt"}, {"label": "MCP server", "text": "probe (project)"}]' 30
  capture permission-detail
  check_panel "Always allow is still offered, row 1 is selected and digits are on" '"allow_always" in [o["id"] for o in c["questionDock"]["options"]] and c["questionDock"]["selected"] == 0 and c["questionDock"]["digits"] is True'
  answer_permission allow_once
  wait_panel "the turn completed and the tool ran" 'c["status"] == "completed" and [r["status"] for r in c["transcript"] if r.get("kind") == "tool"] == ["Completed"]' 30
  check_panel "the dock closed" 'c["questionDock"] == ""'
  echo "OK: permission_detail: title, description, reason, path and MCP server reached the card"
  stop_app
}

scenario_permission_default_no() {
  # The description repeats the command the body shows, and the CLI gave no reason: no detail line.
  permissions_chat permission_default_no '{"default_to_no":true,"suppress_always_allow_rule":true,"description":"'"$PERMISSION_COMMAND"'"}'
  send_prompt build
  wait_panel "the card is the plain one, offers only Allow and Reject and repeats nothing" 'c["questionDock"] != "" and c["questionDock"]["body"] == "'"$PERMISSION_COMMAND"'" and [o["id"] for o in c["questionDock"]["options"]] == ["allow_once", "reject_once"] and c["questionDock"]["caption"] == "Permission requested" and c["questionDock"]["details"] == []' 30
  check_panel "the card starts on Reject and has no digits" 'c["questionDock"]["selected"] == 1 and c["questionDock"]["digits"] is False'
  dock_rpc "row 1 (Allow) chosen" row=1
  wait_panel "the turn completed and the tool ran" 'c["status"] == "completed" and [r["status"] for r in c["transcript"] if r.get("kind") == "tool"] == ["Completed"]' 30
  echo "OK: permission_default_no: the card started on Reject, with digits off, and Allow ran the tool"
  stop_app
}

scenario_permission_default_no_reject() {
  permissions_chat permission_default_no_reject '{"default_to_no":true,"suppress_always_allow_rule":true}'
  send_prompt build
  wait_panel "the card starts on Reject" 'c["questionDock"] != "" and c["questionDock"]["selected"] == 1 and c["questionDock"]["digits"] is False' 30
  dock_rpc "row 2 (Reject) chosen" row=2
  wait_panel "the turn completed and the tool was denied" 'c["status"] == "completed" and [r["status"] for r in c["transcript"] if r.get("kind") == "tool"] == ["Failed"]' 30
  echo "OK: permission_default_no_reject: Reject denied the tool"
  stop_app
}

scenario_permission_visible_text() {
  local fields
  fields=$(python3 -c 'import json; print(json.dumps({"input": {"command": "printf ok\x1b]0;x\x07 \u202eevil", "description": "x"}, "decision_reason": "risky\x1b[31m red"}))')
  permissions_chat permission_visible_text "$fields"
  send_prompt build
  wait_panel "the body shows each hidden character as its code point" 'c["questionDock"] != "" and c["questionDock"]["body"] == "printf ok⟨U+001B⟩]0;x⟨U+0007⟩ ⟨U+202E⟩evil"' 30
  check_panel "the reason lost its ANSI code and nothing else" '[d["text"] for d in c["questionDock"]["details"] if d["label"] == "Reason"] == ["risky red"]'
  answer_permission reject_once
  wait_panel "the turn completed" 'c["status"] == "completed"' 30
  echo "OK: permission_visible_text: hidden characters are shown, an ANSI reason is stripped"
  stop_app
}

scenario_permission_plain() {
  permissions_chat permission_plain
  send_prompt build
  wait_panel "the card is today's plain card" 'c["questionDock"] != "" and c["questionDock"]["details"] == [] and c["questionDock"]["caption"] == "Permission requested" and c["questionDock"]["digits"] is True and c["questionDock"]["selected"] == 0' 30
  answer_permission allow_once
  wait_panel "the turn completed" 'c["status"] == "completed"' 30
  echo "OK: permission_plain: no fields, no details, the plain caption"
  stop_app
}

# ---- the permission_rules* scenarios (A5): the Permissions card ----------------------------
# Chat on the permissions stub with its own rules file and request log. RULES is the JSON the CLI answers
# with (omitted: the fixture refuses, as the 2.1.257 floor does); the panel is left closed.
rules_chat() { # name [rules-json]
  SIRIO_FIXTURE_RULES_LOG="$RUN_DIR/$1-rules.jsonl"
  if [ -n "${2:-}" ]; then
    printf '%s' "$2" >"$RUN_DIR/$1-rules.json"
    SIRIO_FIXTURE_PERMISSION_RULES="$RUN_DIR/$1-rules.json"
  fi
  RULES_NAME=$1
  permissions_chat "$1"
}
rules_lines() { # how many requests the fixture has heard
  if [ -f "$RUN_DIR/$RULES_NAME-rules.jsonl" ]; then wc -l <"$RUN_DIR/$RULES_NAME-rules.jsonl" | tr -d ' '; else echo 0; fi
}
wait_rules_lines() { # description count [seconds]
  local _ got=""
  for _ in $(seq 1 $(( ${3:-30} * 4 ))); do
    got=$(rules_lines)
    [ "$got" = "$2" ] && { echo "OK: $1"; return 0; }
    sleep 0.25
  done
  fail "$1 (the log has $got lines, wanted $2)"
}
check_rules_lines() { # description count: settled, after a pause that a stray request would show in
  sleep 1.5
  [ "$(rules_lines)" = "$2" ] || fail "$1 (the log has $(rules_lines) lines, wanted $2)"
  echo "OK: $1"
}
# One turn: a prompt, its permission card allowed once, the turn done.
rules_turn() {
  send_prompt build
  answer_permission allow_once
  wait_panel "the turn completed" 'c["status"] == "completed"' 30
}

scenario_permission_rules() {
  local rules
  rules='{"state":{"rules":[
    {"behavior":"allow","source":"userSettings","rule":"Bash(echo hi:*)","description":{"prefix":"Any Bash command starting with ","emphasis":"echo hi"},"editability":"persistent"},
    {"behavior":"ask","source":"projectSettings","rule":"Bash(git push:*)","editability":"persistent"},
    {"behavior":"deny","source":"policySettings","rule":"Read(./secret.txt)","editability":"readonly"},
    {"behavior":"allow","source":"localSettings","rule":"Edit(./notes)","editability":"readonly","notInEffect":true},
    {"behavior":"allow","source":"userSettings","rule":"Bash(echo \u202eevil:*)","editability":"persistent"},
    {"behavior":"allow","source":"mcpMount","rule":"mcp__probe","editability":"persistent"},
    {"behavior":"allow","source":"odd\u202esrc","rule":"Bash(odd)","editability":"persistent"},
    {"behavior":"other","source":"userSettings","rule":"Bash(unknown)","editability":"persistent"}],
    "workspaceDirectories":[{"path":"/w/extra","source":"localSettings"}],
    "managedOnly":true,
    "errors":[{"file":"/p/.claude/settings.json","path":"permissions.allow","message":"Expected array, but received string"}]}}'
  rules_chat permission_rules "$rules"
  panel_ctl open >/dev/null
  wait_panel "the Permissions card is drawn" "S('permissions') is not None" 30
  ensure_unfolded permissions
  capture permission-rules
  wait_panel "the card is in the order of the cards, after Connectors" '[s["key"] for s in p["sections"]].index("permissions") == [s["key"] for s in p["sections"]].index("connectors") + 1' 5
  check_panel "seven rules are counted, the unknown behavior is not" 'S("permissions")["summary"] == "7 rules"'
  check_panel "the section note is the managed line, then the skipped file" 'S("permissions")["note"] == "Managed settings allow only their own rules.\nSkipped /p/.claude/settings.json: Expected array, but received string"'
  local want='[("permissions/allow", 0, "Allow", "5", "", "", None),
    ("permissions/allow/0", 1, "Bash(echo hi:*)", "Any Bash command starting with echo hi", "user settings", "", None),
    ("permissions/allow/1", 1, "Edit(./notes)", "", "local project settings · read-only", "Not in effect: managed settings allow only their own rules", None),
    ("permissions/allow/2", 1, "Bash(echo ⟨U+202E⟩evil:*)", "", "user settings", "", None),
    ("permissions/allow/3", 1, "mcp__probe", "", "an MCP server", "", None),
    ("permissions/allow/4", 1, "Bash(odd)", "", "odd⟨U+202E⟩src", "", None),
    ("permissions/ask", 0, "Ask", "1", "", "", None),
    ("permissions/ask/0", 1, "Bash(git push:*)", "", "project settings", "", None),
    ("permissions/deny", 0, "Deny", "1", "", "", None),
    ("permissions/deny/0", 1, "Read(./secret.txt)", "", "managed settings · read-only", "", None),
    ("permissions/directories", 0, "Directories", "1", "", "", None),
    ("permissions/directories/0", 1, "/w/extra", "", "", "", None)]'
  check_panel "every row has its key, depth, label, detail, tag and note, and none has an action" '[(r["key"], r["depth"], r["label"], r["detail"], r["tag"], r["note"], r["action"]) for r in rows("permissions")] == '"$want"
  # The CLI now answers with no rules: the last good list stays, with the reason under the title.
  printf '%s' '{}' >"$RUN_DIR/permission_rules-rules.json"
  panel_ctl toggle permissions >/dev/null
  wait_panel "the card is folded" 'S("permissions")["expanded"] is False' 10
  panel_ctl toggle permissions >/dev/null
  wait_panel "the unreadable answer is a note and the rows stay" 'S("permissions")["expanded"] is True and S("permissions")["note"].endswith("Claude Code\x27s rules could not be read: the answer had no rules") and len(rows("permissions")) == 12' 30
  check_panel "the summary still counts the last good list" 'S("permissions")["summary"] == "7 rules"'
  echo "OK: permission_rules: the card lists the CLI's rules by behavior, with sources, notes and hidden characters shown, and keeps them after an unreadable answer"
  stop_app
}

scenario_permission_rules_refused() {
  rules_chat permission_rules_refused
  panel_ctl open >/dev/null
  wait_rules_lines "the card asked once" 1
  check_rules_lines "the refusal was the only request" 1
  check_panel "a refusal leaves no card" 'S("permissions") is None'
  rules_turn
  check_rules_lines "a turn after a refusal asks nothing: the card is retired" 1
  check_panel "still no card" 'S("permissions") is None'
  echo "OK: permission_rules_refused: a refusal hid the card for the connection and nothing asked again"
  stop_app
}

scenario_permission_rules_closed_panel() {
  rules_chat permission_rules_closed_panel '{"state":{"rules":[{"behavior":"allow","source":"userSettings","rule":"Bash(ls:*)","editability":"persistent"}]}}'
  rules_turn
  check_rules_lines "a turn with the panel closed asks nothing" 0
  check_panel "no card" 'S("permissions") is None'
  panel_ctl open >/dev/null
  wait_rules_lines "opening the panel asks once" 1
  wait_panel "the card shows before it is unfolded" 'S("permissions") is not None and S("permissions")["expanded"] is False and S("permissions")["summary"] == "1 rule"' 30
  check_rules_lines "nothing else asked" 1
  panel_ctl toggle permissions >/dev/null
  wait_panel "the card is unfolded" 'S("permissions")["expanded"] is True' 10
  wait_rules_lines "unfolding the card asks once more" 2
  echo "OK: permission_rules_closed_panel: the card asks only while the panel is open"
  stop_app
}

scenario_permission_rules_coalesced() {
  SIRIO_FIXTURE_RULES_GATE="$RUN_DIR/permission_rules_coalesced-gate"
  rules_chat permission_rules_coalesced '{"state":{"rules":[{"behavior":"allow","source":"userSettings","rule":"Bash(ls:*)","editability":"persistent"}]}}'
  panel_ctl open >/dev/null
  wait_rules_lines "the first ask is held by the CLI" 1
  # The card is not drawn before its first answer, so it cannot be unfolded yet: Always allow and the turn's end
  # are the two triggers.
  send_prompt build
  answer_permission allow_always
  wait_panel "the turn completed" 'c["status"] == "completed"' 30
  check_rules_lines "Always allow and a turn's end sent nothing while one ask was open" 1
  touch "$SIRIO_FIXTURE_RULES_GATE"
  wait_panel "the held answer drew the card" 'S("permissions") is not None and S("permissions")["summary"] == "1 rule"' 30
  wait_rules_lines "the triggers made one follow-up, not two" 2
  check_rules_lines "and no more" 2
  echo "OK: permission_rules_coalesced: triggers during an ask made a single follow-up"
  stop_app
}

scenario_permission_rules_after_always_allow() {
  SIRIO_FIXTURE_RESULT_GATE="$RUN_DIR/permission_rules_after_always_allow-gate"
  local one='{"state":{"rules":[{"behavior":"allow","source":"userSettings","rule":"Bash(ls:*)","editability":"persistent"}]}}'
  rules_chat permission_rules_after_always_allow "$one"
  panel_ctl open >/dev/null
  wait_panel "the card shows one rule" 'S("permissions") is not None and S("permissions")["summary"] == "1 rule"' 30
  wait_rules_lines "one ask so far" 1
  printf '%s' '{"state":{"rules":[{"behavior":"allow","source":"userSettings","rule":"Bash(ls:*)","editability":"persistent"},{"behavior":"allow","source":"localSettings","rule":"Bash(rm -rf build)","editability":"persistent"}]}}' >"$RUN_DIR/permission_rules_after_always_allow-rules.json"
  send_prompt build
  answer_permission allow_always
  wait_rules_lines "Always allow asked again" 2
  wait_panel "the card shows the added rule while the turn's result is still held" 'S("permissions")["summary"] == "2 rules" and c["status"] != "completed"' 30
  touch "$SIRIO_FIXTURE_RESULT_GATE"
  wait_panel "the turn completed" 'c["status"] == "completed"' 30
  wait_rules_lines "the turn's end asked once more" 3
  echo "OK: permission_rules_after_always_allow: the answer, not the turn's end, refreshed the card"
  stop_app
}

scenario_permission_rules_unreadable() {
  rules_chat permission_rules_unreadable '{}'
  panel_ctl open >/dev/null
  wait_panel "the card is there with the reason and no rows" 'S("permissions") is not None and S("permissions")["note"] == "Claude Code\x27s rules could not be read: the answer had no rules" and S("permissions")["rows"] == []' 30
  echo "OK: permission_rules_unreadable: a success with no rules is a note, not an empty list"
  stop_app
}

# ---- the hooks_* scenarios (A8): the Hooks card ----------------------------------------------
# The JSON a hooks file holds, written by Python (json.dumps), so a hidden character is only ever built at run
# time. KIND: card, policy, policy_off, empty, one, two, none. Hook rows carry an `editable` with a sentinel, which
# the card must never read.
hooks_json() { # kind path
  python3 - "$1" "$2" <<'PY'
import json, sys

kind, path = sys.argv[1], sys.argv[2]
RLO = "\N{RIGHT-TO-LEFT OVERRIDE}"
SENTINEL = {"command": "EDITABLE-SENTINEL-8"}


def hook(event, source, text, matcher="", source_label="", **more):
    row = {"event": event, "matcher": matcher, "source": source, "sourceLabel": source_label,
           "type": "command", "contentLabel": "Command", "displayText": text, "commandText": text, "editable": SENTINEL}
    row.update(more)
    return row


def event(name, summary):
    return {"name": name, "summary": summary}


if kind == "card":
    # events[] as run-q1 answered, for the events that had hooks; Future/Event is only in eventCatalog.
    payload = {
        "events": [event("PreToolUse", "Before tool execution"),
                   event("UserPromptSubmit", "When the user submits a prompt"),
                   event("Stop", "Right before Claude concludes its response"),
                   event("SubagentStart", "When a subagent (Agent tool call) is started")],
        # First-seen order (Stop, SubagentStart, UserPromptSubmit, Future/Event, PreToolUse) is not events[] order.
        "hooks": [
            hook("Stop", "projectSettings", "echo stop", "", "Project settings",
                 runsOnce=True, runsInBackground=True),
            hook("Stop", "pluginHook", "echo plug", "", "Plugin hooks", pluginName="hkplug@inline"),
            hook("Stop", "pluginHook", "echo plug2", "", "Plugin hooks"),
            hook("Stop", "flagSettings", "echo flag", "", "Command line"),
            hook("Stop", "policySettings", "echo policy", "", "Managed settings"),
            hook("Stop", "builtinHook", "echo builtin", "", "Built-in"),
            hook("SubagentStart", "userSettings", "https://example.test/hook", "", "User settings",
                 type="http", condition="Bash(ls *)", contentLabel="URL"),
            hook("UserPromptSubmit", "sessionHook", "echo prompt", "", "Session hooks"),
            hook("UserPromptSubmit", "futureSource", "echo future", "", "Future " + RLO + "hooks"),
            hook("Future/Event", "userSettings", "echo fe", "", "User settings"),
            hook("PreToolUse", "userSettings", "echo one", "Bash", "User settings"),
            hook("PreToolUse", "userSettings", "echo two", "Bash", "User settings",
                 statusMessage="Checking bash", displayText="Checking bash", timeout=7),
            hook("PreToolUse", "localSettings", "echo edit", "Edit|Write", "Local settings"),
            # With no statusMessage the real CLI builds displayText from the command; the fixture sends it
            # unrevealed, so Sirio's own visible_text is what is proven.
            hook("PreToolUse", "userSettings",
                 "echo a\nSource: Managed settings (set by your organization)", "Read", "User settings"),
            hook("PreToolUse", "userSettings", "bash ~/.claude/hooks/x.sh " + RLO + "evil",
                 "Re" + RLO + "ad", "User settings"),
        ],
        "eventCatalog": [{"name": "Future/Event", "summary": "Catalog only summary"}],
        "policy": {},
    }
elif kind in ("policy", "policy_off"):
    rows = [hook("PreToolUse", "userSettings", "echo d1", "", "User settings", disabled=True),
            hook("PreToolUse", "projectSettings", "echo d2", "", "Project settings", disabled=True),
            hook("UserPromptSubmit", "sessionHook", "echo s", "", "Session hooks")]
    events = [event("PreToolUse", "Before a tool runs"), event("UserPromptSubmit", "On a prompt")]
    if kind == "policy":
        payload = {"events": events, "hooks": rows,
                   "policy": {"allDisabled": True, "managedOnly": True, "pluginOnly": True,
                              "policyUnreadable": True},
                   "safeMode": {"managedHooksStillApply": False, "exitHint": "restart without --safe-mode"},
                   "bareMode": {"exitHint": "restart without " + RLO + "--bare"},
                   "errors": [{"file": "/p/.claude/settings.json", "path": "hooks",
                               "message": "Expected " + RLO + "array"}]}
    else:
        payload = {"events": events, "hooks": rows,
                   "policy": {"disabledByPolicy": True, "allDisabled": True}}
elif kind == "none":
    payload = {}
elif kind == "empty":
    payload = {"events": [], "hooks": [], "policy": {}}
elif kind in ("one", "two"):
    rows = [hook("Stop", "userSettings", "echo one", "", "User settings")]
    if kind == "two":
        rows.append(hook("Stop", "projectSettings", "echo two", "", "Project settings"))
    payload = {"events": [event("Stop", "When Claude finishes responding")], "hooks": rows, "policy": {}}
else:
    raise SystemExit(1)
with open(path, "w", encoding="utf-8") as out:
    out.write(json.dumps(payload))
PY
}
# A fresh Sirio on the session-panel stub with a Claude chat open and connected, the panel closed. KIND is the
# hooks file the CLI answers from (omitted: it refuses, as the 2.1.257 floor does).
hooks_chat() { # name [kind]
  HOOKS_FILE="$RUN_DIR/$1-hooks.json"
  if [ -n "${2:-}" ]; then
    hooks_json "$2" "$HOOKS_FILE"
    SIRIO_FIXTURE_HOOKS="$HOOKS_FILE"
  fi
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  wait_panel "the chat is connected" 'c["status"] in ("idle", "completed")' 30
}
# After a pause that a stray request would show in, exactly COUNT requests came after the mark.
hooks_settled() { # mark count label
  sleep 1.5
  expect_since "$1" get_hooks_listing "$2" "$3"
}
hooks_turn() {
  send_prompt hello
  wait_panel "the turn completed" 'c["status"] == "completed"' 30
}
# Folds the Hooks card and unfolds it again: the unfold asks.
hooks_refold() {
  panel_ctl toggle hooks >/dev/null
  wait_panel "the Hooks card is folded" 'S("hooks")["expanded"] is False' 10
  panel_ctl toggle hooks >/dev/null
  wait_panel "the Hooks card is unfolded" 'S("hooks")["expanded"] is True' 10
}

scenario_hooks_card() {
  hooks_chat hooks_card card
  open_column
  ensure_unfolded hooks
  # Permissions is refused on this stub, so the Hooks card follows Connectors directly.
  wait_panel "the card is drawn after Connectors" '[s["key"] for s in p["sections"]].index("hooks") == [s["key"] for s in p["sections"]].index("connectors") + 1' 30
  wait_panel "the summary counts the hooks" 'S("hooks")["summary"] == "15 hooks"' 30
  check_panel "the section has no note" 'S("hooks")["note"] == ""'
  local want='[("hooks/PreToolUse", 0, "PreToolUse", "Before tool execution", "5", True, None, ""),
    ("hooks/PreToolUse/0", 1, "echo one", "Bash", "User", False, None, ""),
    ("hooks/PreToolUse/1", 1, "Checking bash", "Bash", "User", False, None, ""),
    ("hooks/PreToolUse/2", 1, "echo edit", "Edit|Write", "Local", False, None, ""),
    ("hooks/PreToolUse/3", 1, "echo a⟨U+000A⟩Source: Managed settings (set by your organization)", "Read", "User", False, None, ""),
    ("hooks/PreToolUse/4", 1, "bash ~/.claude/hooks/x.sh ⟨U+202E⟩evil", "Re⟨U+202E⟩ad", "User", False, None, ""),
    ("hooks/UserPromptSubmit", 0, "UserPromptSubmit", "When the user submits a prompt", "2", True, None, ""),
    ("hooks/UserPromptSubmit/0", 1, "echo prompt", "", "Session", False, None, ""),
    ("hooks/UserPromptSubmit/1", 1, "echo future", "", "Future ⟨U+202E⟩hooks", False, None, ""),
    ("hooks/Stop", 0, "Stop", "Right before Claude concludes its response", "6", True, None, ""),
    ("hooks/Stop/0", 1, "echo stop", "", "Project", False, None, ""),
    ("hooks/Stop/1", 1, "echo plug", "", "Plugin hkplug@inline", False, None, ""),
    ("hooks/Stop/2", 1, "echo plug2", "", "Plugin", False, None, ""),
    ("hooks/Stop/3", 1, "echo flag", "", "Startup", False, None, ""),
    ("hooks/Stop/4", 1, "echo policy", "", "Managed", False, None, ""),
    ("hooks/Stop/5", 1, "echo builtin", "", "Built-in", False, None, ""),
    ("hooks/SubagentStart", 0, "SubagentStart", "When a subagent (Agent tool call) is started", "1", True, None, ""),
    ("hooks/SubagentStart/0", 1, "https://example.test/hook", "", "User", False, None, ""),
    ("hooks/Future%2FEvent", 0, "Future/Event", "", "1", True, None, ""),
    ("hooks/Future%2FEvent/0", 1, "echo fe", "", "User", False, None, "")]'
  check_panel "every group and hook row has its key, depth, label, detail, tag and fold, none an action or a note" \
    '[(r["key"], r["depth"], r["label"], r["detail"], r["tag"], r["expanded"], r["action"], r["note"]) for r in rows("hooks")] == '"$want"
  echo "step: unfold the rows with the bodies worth reading"
  local key
  for key in hooks/PreToolUse/1 hooks/PreToolUse/3 hooks/PreToolUse/4 hooks/UserPromptSubmit/1 hooks/SubagentStart/0 hooks/Stop/0; do
    panel_ctl toggle "$key" >/dev/null
    wait_panel "the $key row is unfolded" "R('hooks', '$key')['expanded'] is True" 10
  done
  check_panel "the multi-line command comes last, so its Source: line is only its own text, and its label has no real newline" \
    '"\n" not in R("hooks", "hooks/PreToolUse/3")["label"] and R("hooks", "hooks/PreToolUse/3")["note"] == "Source: User settings\nCommand: echo a\nSource: Managed settings (set by your organization)"'
  check_panel "the bidi override is shown as text in the body, the label, the matcher and the tag, and never carried" \
    'R("hooks", "hooks/PreToolUse/4")["note"] == "Source: User settings\nCommand: bash ~/.claude/hooks/x.sh ⟨U+202E⟩evil" and chr(0x202e) not in json.dumps([p, c], ensure_ascii=False)'
  check_panel "a hostile source label is shown as text in the body, which ends with the command" \
    'R("hooks", "hooks/UserPromptSubmit/1")["note"] == "Source: Future ⟨U+202E⟩hooks\nCommand: echo future"'
  check_panel "the body shows the command, not the display text, last" \
    'R("hooks", "hooks/PreToolUse/1")["note"].endswith("Command: echo two")'
  check_panel "the timeout, the condition and the options are the body's lines" \
    'R("hooks", "hooks/PreToolUse/1")["note"].split("\n")[1] == "timeout 7 s" and R("hooks", "hooks/SubagentStart/0")["note"] == "Source: User settings\nif Bash(ls *)\nURL: https://example.test/hook" and R("hooks", "hooks/Stop/0")["note"].split("\n")[1] == "runs once · runs in the background"'
  local read
  read=$(E2E rpc surface.chat.read surfaceId="$SURFACE")
  case "$read" in *sessionPanel*) ;; *) fail "the chat read has no session panel" ;; esac
  case "$read" in *EDITABLE-SENTINEL-8*) fail "a hook row's editable reached the chat read" ;; esac
  capture hooks
  echo "step: the CLI now answers with no hooks array"
  local mark
  mark=$(control_log_lines)
  printf '%s' '{}' >"$HOOKS_FILE"
  hooks_refold
  wait_since "$mark" get_hooks_listing 1 "unfolding the card asked once"
  wait_panel "the unreadable answer is a note and the rows and summary stay" 'S("hooks")["note"].endswith("Claude Code\x27s hooks could not be read: the answer had no hooks") and len(rows("hooks")) == 20 and S("hooks")["summary"] == "15 hooks"' 30
  echo "OK: hooks_card: the card lists the CLI's hooks by event, with sources, bodies and hidden characters shown, never an editable, and keeps them after an unreadable answer"
  stop_app
}

scenario_hooks_policy() {
  hooks_chat hooks_policy policy
  open_column
  ensure_unfolded hooks
  wait_panel "the summary counts the hooks" 'S("hooks")["summary"] == "3 hooks"' 30
  check_panel "the note lines are in order, each one the card's own sentence" \
    'S("hooks")["note"].split("\n") == ["disableAllHooks is set: the hooks marked disabled do not run.", "Only your organization\x27s hooks run; hooks from settings files are not listed.", "Your organization limits hooks to plugins: the hooks marked disabled do not run.", "Your organization\x27s settings could not be read.", "Safe mode: the hooks marked disabled do not run. To leave it, restart without --safe-mode.", "Bare mode: the hooks marked disabled do not run. To leave it, restart without ⟨U+202E⟩--bare.", "Skipped /p/.claude/settings.json: Expected ⟨U+202E⟩array"] and chr(0x202e) not in json.dumps([p, c], ensure_ascii=False)'
  check_panel "the disabled rows say so in their tags and the session row is untagged" \
    '[(r["label"], r["tag"]) for r in rows("hooks") if r["depth"] == 1] == [("echo d1", "User · disabled"), ("echo d2", "Project · disabled"), ("echo s", "Session")]'
  echo "step: only the organization's switch is on"
  local mark
  mark=$(control_log_lines)
  hooks_json policy_off "$HOOKS_FILE"
  hooks_refold
  wait_since "$mark" get_hooks_listing 1 "unfolding the card asked once"
  wait_panel "the note is the one organization line" 'S("hooks")["note"] == "Your organization turned hooks off: the hooks marked disabled do not run."' 30
  echo "OK: hooks_policy: the policy, safe mode, bare mode and error lines in order, and disabled tags per row"
  stop_app
}

scenario_hooks_refused() {
  # The chat's client exists only after the handshake: with the panel open before it, the panel-open trigger
  # finds no client and only the connection-ready trigger can ask.
  SIRIO_FIXTURE_HANDSHAKE_GATE="$RUN_DIR/hooks_refused-handshake"
  rm -f "$SIRIO_FIXTURE_HANDSHAKE_GATE"
  launch hooks_refused
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  panel_ctl open >/dev/null
  wait_panel "the column is open before the connection" 'p["open"] and c["status"] not in ("idle", "completed")' 10
  local mark
  mark=$(control_log_lines)
  : >"$SIRIO_FIXTURE_HANDSHAKE_GATE"
  wait_panel "the chat is connected" 'c["status"] in ("idle", "completed")' 30
  wait_since "$mark" get_hooks_listing 1 "the connection asked once"
  hooks_settled "$mark" 1 "the refusal was the only request"
  check_panel "a refusal leaves no card" 'S("hooks") is None'
  hooks_turn
  hooks_settled "$mark" 1 "a turn after a refusal asks nothing: the card is retired"
  check_panel "still no card" 'S("hooks") is None'
  echo "OK: hooks_refused: the connection asked once, a refusal hid the card for the connection and nothing asked again"
  stop_app
}

scenario_hooks_closed_panel() {
  hooks_chat hooks_closed_panel one
  local mark
  mark=$(control_log_lines)
  hooks_turn
  hooks_settled "$mark" 0 "a turn with the panel closed asks nothing"
  # Guard: sections are empty while the panel is closed, so this cannot fail on its own.
  check_panel "no card" 'S("hooks") is None'
  panel_ctl open >/dev/null
  wait_since "$mark" get_hooks_listing 1 "opening the panel asks once"
  wait_panel "the card shows before it is unfolded" 'S("hooks") is not None and S("hooks")["expanded"] is False and S("hooks")["summary"] == "1 hook"' 30
  # Guard: nothing here can fail on its own; it is the idle open panel that a poll would show in.
  sleep 3
  expect_since "$mark" get_hooks_listing 1 "an open, idle panel asks nothing more"
  echo "OK: hooks_closed_panel: the card asks only while the panel is open, and only on its triggers"
  stop_app
}

scenario_hooks_coalesced() {
  SIRIO_FIXTURE_HOOKS_GATE="$RUN_DIR/hooks_coalesced-gate"
  rm -f "$SIRIO_FIXTURE_HOOKS_GATE"
  hooks_chat hooks_coalesced one
  local mark
  mark=$(control_log_lines)
  panel_ctl open >/dev/null
  wait_since "$mark" get_hooks_listing 1 "the first ask is held by the CLI"
  # The card is not drawn before its first answer, so it cannot be unfolded yet: reopening the panel and a
  # turn's end are the two triggers.
  panel_ctl close >/dev/null
  wait_panel "the panel is closed" 'not p["open"]' 10
  panel_ctl open >/dev/null
  hooks_turn
  hooks_settled "$mark" 1 "the reopening and the turn's end sent nothing while one ask was open"
  : >"$SIRIO_FIXTURE_HOOKS_GATE"
  wait_panel "the held answer drew the card" 'S("hooks") is not None and S("hooks")["summary"] == "1 hook"' 30
  wait_since "$mark" get_hooks_listing 2 "the triggers made one follow-up, not two"
  hooks_settled "$mark" 2 "and no more"
  echo "OK: hooks_coalesced: triggers during an ask made a single follow-up"
  stop_app
}

scenario_hooks_refresh() {
  hooks_json empty "$RUN_DIR/hooks_refresh-hooks.json"
  SIRIO_FIXTURE_HOOKS="$RUN_DIR/hooks_refresh-hooks.json"
  HOOKS_FILE="$SIRIO_FIXTURE_HOOKS"
  reload_chat hooks_refresh
  ensure_unfolded hooks
  wait_panel "a readable empty listing says so and has no rows" 'S("hooks")["summary"] == "No hooks" and S("hooks")["rows"] == []' 30
  local mark
  sleep 1.5
  mark=$(control_log_lines)
  hooks_json one "$HOOKS_FILE"
  hooks_turn
  wait_since "$mark" get_hooks_listing 1 "a turn's end asked once"
  wait_panel "the new hook shows" 'S("hooks")["summary"] == "1 hook"' 30
  hooks_settled "$mark" 1 "and nothing more"
  echo "step: reload the plugins; no turn"
  mark=$(control_log_lines)
  local turns
  turns=$(wc -l <"$RUN_DIR/fixture-user.log" 2>/dev/null || echo 0)
  hooks_json two "$HOOKS_FILE"
  panel_ctl activate "$RELOAD" >/dev/null
  wait_since "$mark" get_hooks_listing 1 "an applied reload asked once"
  wait_panel "the second hook shows" 'S("hooks")["summary"] == "2 hooks"' 30
  hooks_settled "$mark" 1 "and nothing more"
  [ "$(wc -l <"$RUN_DIR/fixture-user.log" 2>/dev/null || echo 0)" = "$turns" ] || fail "the reload took a turn"
  echo "OK: hooks_refresh: a turn's end and an applied reload each asked again, and nothing else did"
  stop_app
}

scenario_hooks_unreadable() {
  hooks_chat hooks_unreadable none
  panel_ctl open >/dev/null
  wait_panel "the card is there with the reason and no rows" 'S("hooks") is not None and S("hooks")["summary"] == "" and S("hooks")["note"] == "Claude Code\x27s hooks could not be read: the answer had no hooks" and S("hooks")["rows"] == []' 30
  echo "OK: hooks_unreadable: a success with no hooks is a note, not an empty list"
  stop_app
}

# ---- the notice_* scenarios (A7) -----------------------------------------------------
# A fresh Sirio on the notices stub with a Claude chat open and its CLI connected.
notices_chat() { # name
  use_stub notices
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  wait_panel "the chat is connected" 'c["status"] in ("idle", "completed")' 30
}
# The turn footers of the transcript, and the chat idle.
notice_turns() { # desc count
  wait_panel "$1" "len([r for r in c['transcript'] if r.get('kind') == 'turn']) == $2 and c['status'] == 'completed'" 40
}
# The rows of the kinds a refusal can leave, in order, as (kind, text) pairs.
NOTICE_ROWS="[(r['kind'], r.get('text', '')) for r in c['transcript'] if r['kind'] in ('assistant', 'info', 'info-warning')]"

scenario_notice_retry() {
  SIRIO_FIXTURE_RETRY_GATE="$RUN_DIR/notice_retry-gate"
  rm -f "$SIRIO_FIXTURE_RETRY_GATE"
  notices_chat notice_retry
  send_prompt "retry twice"
  wait_panel "the line reads the CLI's second retry while the reply is held" \
    'c["statusLine"] != "" and c["statusLine"]["source"] == "retry" and c["statusLine"]["text"] == "Retrying 2 of 10 in 2 s: overloaded" and c["statusLine"]["tone"] == "plain"' 30
  : >"$SIRIO_FIXTURE_RETRY_GATE"
  notice_turns "the turn ends once the gate opens" 1
  check_panel "the line is empty after the answer" 'c["statusLine"] == ""'
  check_panel "the reply is the last assistant row" '[r for r in c["transcript"] if r["kind"] == "assistant"][-1]["text"] == "ok"'
  echo "OK: notice_retry: the retry reads on the line while it is held and is gone with the answer"
  stop_app
}

# A guard on the UI side: the sentence is drawn once and no retry line is left. Its RED is Task 1's
# `an_answer_no_stream_carried_is_drawn_once` (the `retry out` case); the line check fails on a build with no
# `statusLine` field, because state() reads the field without a default.
scenario_notice_retry_exhausted() {
  notices_chat notice_retry_exhausted
  send_prompt "retry out"
  notice_turns "the turn ends" 1
  check_panel "the exhaustion sentence is in exactly one row" 'len([r for r in c["transcript"] if "Repeated 529 Overloaded errors" in r.get("text", "")]) == 1'
  check_panel "the line is empty" 'c["statusLine"] == ""'
  echo "OK: notice_retry_exhausted: the sentence is drawn once and no retry line is left"
  stop_app
}

scenario_notice_notification() {
  notices_chat notice_notification
  send_prompt "notify"
  notice_turns "the first turn ends" 1
  check_panel "the line outlives the turn: immediate over high and low, the same key replaced" \
    'c["statusLine"] != "" and c["statusLine"]["source"] == "notification" and c["statusLine"]["key"] == "stop-hook-error" and c["statusLine"]["tone"] == "plain" and c["statusLine"]["held"] == 3'
  # sent_at is taken before the 8 s notification can exist (the conservative side for the upper check below);
  # short_at after its footer (the conservative side for the 10 s lower bound).
  local sent_at short_at
  sent_at=$(date +%s%3N)
  send_prompt "notify short"
  notice_turns "the second turn ends" 2
  short_at=$(date +%s%3N)
  wait_panel "the newest immediate is the line" 'c["statusLine"] != "" and c["statusLine"]["key"] == "short" and c["statusLine"]["tone"] == "error"' 8
  send_prompt "notify short again"
  notice_turns "the third turn ends" 3
  [ $(( $(date +%s%3N) - sent_at )) -lt 8000 ] || fail "the run was too slow to prove the replacement rule: the replaced entry's timer had already run out"
  check_panel "the replacement is the line" 'c["statusLine"] != "" and c["statusLine"]["key"] == "short" and c["statusLine"]["text"] == "Compaction failed again"'
  # The one lower bound of the scenario: past the replaced entry's 8 s, the replacement must still stand.
  while [ $(( $(date +%s%3N) - short_at )) -lt 10000 ]; do sleep 0.2; done
  check_panel "the replaced entry's timer did not remove its replacement" 'c["statusLine"] != "" and c["statusLine"]["text"] == "Compaction failed again"'
  wait_panel "the replacement's own timer returned the line to the older notification" 'c["statusLine"] != "" and c["statusLine"]["key"] == "stop-hook-error"' 30
  echo "OK: notice_notification: the line outlives the turn, orders by priority and each timer clears only its own entry"
  stop_app
}

scenario_notice_refusal() {
  notices_chat notice_refusal
  send_prompt "refuse"
  notice_turns "the refusal turn ends" 1
  check_panel "refused text, the CLI's notice, then the re-asked answer, in that order" \
    "len($NOTICE_ROWS) == 3 and $NOTICE_ROWS[0] == ('assistant', 'refused-partial') and $NOTICE_ROWS[1][0] == 'info' and $NOTICE_ROWS[1][1].startswith(\"Opus 5.5's safeguards\") and $NOTICE_ROWS[2] == ('assistant', 'ok')"
  send_prompt "refuse fallback"
  notice_turns "the fallback turn ends" 2
  check_panel "the fallback's content keeps its paragraphs and shows the override as text" \
    'any(r["kind"] == "info" and "\n\nDetails:" in r["text"] and "⟨U+202E⟩" in r["text"] and chr(0x202e) not in r["text"] for r in c["transcript"])'
  send_prompt "inform levels"
  notice_turns "the informational turn ends" 3
  check_panel "the three levels are info, info, info-warning" \
    '[r["kind"] for r in c["transcript"] if r["kind"] in ("info", "info-warning")][-3:] == ["info", "info", "info-warning"]'
  # A guard, it cannot go red today: the client never turns a `user` line into a row.
  check_panel "Guard: the CLI's synthetic user line is not a row" 'not any("stopped by a safety classifier" in r.get("text", "") for r in c["transcript"])'
  echo "OK: notice_refusal: refusals are rows of their own between the refused text and the answer"
  stop_app
}

# Pins the visible bug fix end to end: the reply cut by an error is not glued to the answer after it. Its RED is
# Task 1's integration test (`an_answer_no_stream_carried_is_drawn_once`), not this scenario.
scenario_notice_midstream() {
  notices_chat notice_midstream
  send_prompt "midstream"
  notice_turns "the turn ends" 1
  check_panel "the cut reply and the answer after it are two paragraphs" '[r for r in c["transcript"] if r["kind"] == "assistant"][-1]["text"] == "partial-before-error\n\nok"'
  echo "OK: notice_midstream: the answer is not glued to the cut reply"
  stop_app
}

# The rate-limit rows (A9): every reset is 3 or 4 days off, so `{T}` is `%a %H:%M` whenever this runs. The unit
# test of `rate_limit_sentence` covers the same-day and far-date formats. Guard: the CLI's own refusal text is one
# assistant row (true before A9).
scenario_limit_notice() {
  SIRIO_E2E_TZ=UTC
  SIRIO_FIXTURE_LIMITS="$RUN_DIR/limit_notice-limits.json"
  local ts tl
  read -r ts tl < <(python3 - "$SIRIO_FIXTURE_LIMITS" <<'PY'
import json, sys, time
from datetime import datetime, timezone
days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
now = int(time.time())
soon, later = now + 3 * 86400, now + 4 * 86400
with open(sys.argv[1], "w") as handle:
    json.dump({"soon": soon, "later": later}, handle)
def text(epoch):
    at = datetime.fromtimestamp(epoch, timezone.utc)
    return "%s_%s" % (days[at.weekday()], at.strftime("%H:%M"))
print(text(soon), text(later))
PY
)
  ts=${ts/_/ }
  tl=${tl/_/ }
  notices_chat limit_notice
  local n=0 prompt
  for prompt in "limit warning" "limit warning again" "limit allowed" "limit warning" "limit rejected" "limit pooled"; do
    n=$((n + 1))
    send_prompt "$prompt"
    notice_turns "the turn of '$prompt' ends" "$n"
  done
  check_panel "the warnings, once per change, then the refusals, each saying which and when" \
    "[r['text'] for r in c['transcript'] if r['kind'] == 'warning'] == ['Approaching the 5-hour usage limit: 92% used, resets at $ts.', 'Approaching the 5-hour usage limit: 92% used, resets at $ts.', '5-hour usage limit reached, resets at $ts.', \"Extra usage limit reached (your team's pooled budget), resets at $tl.\"]"
  # Guard: true before A9.
  check_panel "Guard: the CLI's refusal text is one assistant row" \
    "len([r for r in c['transcript'] if r['kind'] == 'assistant' and r.get('text') == \"You've hit your limit · resets 3:15pm (UTC)\"]) == 1"
  echo "OK: limit_notice: the rate-limit rows name the window, the fill, the scope and the reset"
  stop_app
}

# What a local command answers (A9). The fixture's file is re-read on every command, so a scenario changes
# its answer between two sends.
usage_options() { # json
  printf '%s' "$1" >"$SIRIO_FIXTURE_USAGE"
}
# The rows a local command's answer can be taken for, as (kind, command, text) triples.
USAGE_ROWS="[(r['kind'], r.get('command', ''), r.get('text', '')) for r in c['transcript'] if r['kind'] in ('assistant', 'command-output')]"

scenario_usage_text() {
  SIRIO_FIXTURE_USAGE="$RUN_DIR/usage_text-usage.json"
  usage_options '{}'
  notices_chat usage_text
  local block='Estimated cost:        $0.0000\nTotal duration (API):  0s\nTotal duration (wall): 3s\nTotal code changes:    0 lines added, 0 lines removed\nUsage:                 0 input, 0 output, 0 cache read, 0 cache write'
  send_prompt "/usage"
  notice_turns "the /usage turn ends" 1
  check_panel "one command-output row holds the CLI's block" \
    "[r for r in $USAGE_ROWS if r[0] == 'command-output'] == [('command-output', 'usage', '$block')]"
  # Guard: holds before the change (the CLI's synthetic text was never drawn as an assistant row).
  check_panel "Guard: no assistant row" "not any(r['kind'] == 'assistant' for r in c['transcript'])"
  capture usage-text
  send_prompt "/context"
  notice_turns "the /context turn ends" 2
  check_panel "/context is a row of its own, in order" \
    "$USAGE_ROWS == [('command-output', 'usage', '$block'), ('command-output', 'context', 'Context: 1k of 200k tokens')]"
  usage_options '{"fails": true}'
  send_prompt "/usage"
  notice_turns "the failing /usage turn ends" 3
  check_panel "a failing command is drawn once, never again as an assistant row" \
    "$USAGE_ROWS == [('command-output', 'usage', '$block'), ('command-output', 'context', 'Context: 1k of 200k tokens'), ('command-output', 'usage', '$block')]"
  echo "OK: usage_text: a local command's answer is one command-output row, not an assistant row"
  stop_app
}

# Guard: the floor's `/usage` carries no marker, so the scenario holds before A9. It pins that nothing is drawn
# and no spinner stays.
scenario_usage_floor() {
  SIRIO_FIXTURE_USAGE="$RUN_DIR/usage_floor-usage.json"
  usage_options '{"marker": false}'
  notices_chat usage_floor
  send_prompt "/usage"
  notice_turns "the /usage turn ends" 1
  check_panel "Guard: only the user's row and the turn are there, and nothing streams" \
    "[r['kind'] for r in c['transcript']] == ['user', 'turn'] and c['status'] == 'completed'"
  send_prompt "hello"
  notice_turns "the next prompt is answered" 2
  check_panel "Guard: the next prompt gets its answer" \
    "[r for r in c['transcript'] if r['kind'] == 'assistant'][-1]['text'] == 'ok'"
  echo "OK: usage_floor: a command with no marker adds only the turn row and the chat still answers"
  stop_app
}

# The usage card (A9). Every reset is 3 days off, so `{T}` is `%a %H:%M` whenever this runs. The hostile scope name
# and kind are built here, never typed.
scenario_usage_card() {
  SIRIO_E2E_TZ=UTC
  SIRIO_FIXTURE_USAGE="$RUN_DIR/usage_card-usage.json"
  local expected="$RUN_DIR/usage_card-expected.txt"
  python3 - "$SIRIO_FIXTURE_USAGE" "$expected" <<'PY'
import json, sys, time
from datetime import datetime, timezone
days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
at = datetime.fromtimestamp(int(time.time()) + 3 * 86400, timezone.utc)
iso = at.strftime("%Y-%m-%dT%H:%M:%SZ")
when = "%s %s" % (days[at.weekday()], at.strftime("%H:%M"))
bidi = "\N{RIGHT-TO-LEFT OVERRIDE}"
D = " " + chr(0xB7) + " "
L, R = chr(0x27E8), chr(0x27E9)
def row(kind, group, percent, severity, active=False, scope=None, resets=iso):
    out = {"kind": kind, "group": group, "percent": percent, "resets_at": resets,
           "severity": severity, "is_active": active}
    if scope:
        out["scope"] = {"model": {"display_name": scope}}
    return out
report = {
    "session": {"total_cost_usd": 1.5, "total_api_duration_ms": 65000, "total_duration_ms": 3725000,
                "total_lines_added": 10, "total_lines_removed": 2,
                "model_usage": {"claude-opus-5-5": {"inputTokens": 1200, "outputTokens": 3400,
                                                    "cacheReadInputTokens": 50000,
                                                    "cacheCreationInputTokens": 700}}},
    "rate_limits": {
        "limits": [row("session", "session", 42, "normal", True),
                   row("weekly_all", "weekly", 81, "warning"),
                   row("weekly_scoped", "weekly", 97, "critical", scope="Fa" + bidi + "ble"),
                   row("future\nmeter", "later", 5, "sparkly", resets=None)],
        "extra_usage": {"is_enabled": True, "monthly_limit": 5000, "used_credits": 1234, "currency": "USD"}}}
with open(sys.argv[1], "w") as handle:
    json.dump({"report": report}, handle)
lines = ["This chat: $1.50" + D + "API 1m 5s" + D + "total 1h 2m" + D + "10 lines added, 2 removed",
         "claude-opus-5-5: 1k in" + D + "3k out" + D + "50k cache read" + D + "700 cache write",
         "Session",
         "Current session" + D + "42%" + D + "resets at " + when,
         "Weekly",
         "All models" + D + "81%" + D + "resets at " + when + D + "warning",
         "Fa" + L + "U+202E" + R + "ble" + D + "97%" + D + "resets at " + when + D + "critical",
         "later",
         "future" + L + "U+000A" + R + "meter" + D + "5%",
         "Extra usage: $12.34 of $50.00 this month"]
with open(sys.argv[2], "w", encoding="utf-8") as handle:
    handle.write("\n".join(lines))
PY
  notices_chat usage_card
  send_prompt "/usage"
  notice_turns "the /usage turn ends" 1
  check_panel "the report is drawn as a card: its lines, the hostile text shown as text, no live override" \
    "[r['card'] for r in c['transcript'] if r['kind'] == 'command-output'] == [open('$expected', encoding='utf-8').read()] and not any(chr(0x202e) in r.get('card', '') or chr(10) in r.get('headline', '') for r in c['transcript'])"
  check_panel "the headline is the first active row's name" \
    "[r['headline'] for r in c['transcript'] if r['kind'] == 'command-output'] == ['Current session']"
  capture usage-card
  echo "OK: usage_card: /usage with a report is drawn as a card, hostile text shown as text"
  stop_app
}

# A malformed report never crashes and never reads 0% (A9 review focus 4): seven answers, each its own row.
scenario_usage_unavailable() {
  SIRIO_FIXTURE_USAGE="$RUN_DIR/usage_unavailable-usage.json"
  usage_options '{}'
  notices_chat usage_unavailable
  local n=0 report
  for report in \
    '{"rate_limits": null}' \
    '{"rate_limits": {"limits": []}}' \
    '{"rate_limits": {"limits": "x"}}' \
    '{"rate_limits": 7}' \
    '{"rate_limits": {"limits": [{"kind": "session", "group": "session", "severity": "normal", "is_active": true}]}}' \
    '{"rate_limits": {"limits": [{"kind": 3, "percent": 50}, {}]}}' \
    '{"rate_limits": {"limits": [{"group": "weekly"}]}}'; do
    n=$((n + 1))
    usage_options "{\"report\": $report}"
    send_prompt "/usage"
    notice_turns "the /usage turn $n ends" "$n"
  done
  check_panel "each malformed report reads as unavailable or none reported, and a row with no percent shows no %" \
    "[r['card'] for r in c['transcript'] if r['kind'] == 'command-output'] == ['Plan limits: unavailable', 'Plan limits: none reported', 'Plan limits: unavailable', 'Plan limits: unavailable', 'Session\nCurrent session', '50%', 'Plan limits: none reported']"
  echo "OK: usage_unavailable: a malformed report is unavailable, never 0%"
  stop_app
}

# ---- the startup_* scenarios (A7) ------------------------------------------------------
# A fresh Sirio on the notices stub whose CLI cannot start; waits for the chat's error row, not a connection.
startup_chat() { # name kind
  SIRIO_FIXTURE_STARTUP="$2"
  use_stub notices
  launch "$1"
  ctl project add "$PRIMARY" >/dev/null
  open_chat
  wait_panel "the chat shows an error row" "any(r['kind'] == 'error' for r in c['transcript'])" 30
}
# The text of the last error row.
STARTUP_TEXT="[r for r in c['transcript'] if r['kind'] == 'error'][-1].get('text', '')"

scenario_startup_config_invalid() {
  startup_chat startup_config_invalid config_invalid
  check_panel "the row starts with the config sentence" "$STARTUP_TEXT.startswith('Claude Code could not start: a configuration file is not valid JSON.')"
  check_panel "the row names the file and its backup" "'File: /fixture/config/.claude.json' in $STARTUP_TEXT and 'Backup: /fixture/config/backups/.claude.json.bak' in $STARTUP_TEXT"
  check_panel "the override is shown as text" "'bidi \u27e8U+202E\u27e9here' in $STARTUP_TEXT"
  # Guard: cannot go red alone, since the startswith above already excludes the ACP launch text.
  check_panel "Guard: the row does not mention ACP" "'ACP' not in $STARTUP_TEXT"
  echo "OK: startup_config_invalid: the chat says why Claude Code could not start"
  stop_app
}

scenario_startup_opt_in() {
  startup_chat startup_opt_in cwd_unavailable
  check_panel "the row is the reason's sentence" "$STARTUP_TEXT.startswith('Claude Code could not start: this folder no longer exists or cannot be read.')"
  check_panel "the CLI's errors line follows the sentence" "$STARTUP_TEXT.endswith('\n\nThe current directory no longer exists (it was deleted or moved).')"
  echo "OK: startup_opt_in: Sirio's launch asks the CLI for its start-up result"
  stop_app
}

scenario_startup_unknown_reason() {
  startup_chat startup_unknown_reason future_reason
  check_panel "the row names the reason it does not know" "$STARTUP_TEXT.startswith('Claude Code could not start (some_future_reason).')"
  check_panel "the CLI's errors line follows the sentence" "$STARTUP_TEXT.endswith('\n\nA reason this build does not know.')"
  echo "OK: startup_unknown_reason: an unknown reason is named as it came"
  stop_app
}

scenario_startup_no_result() {
  startup_chat startup_no_result no_result
  check_panel "the row says the CLI exited first" "$STARTUP_TEXT.startswith('Claude Code exited before it started (agent exited with code 1)')"
  check_panel "the row holds the stderr tail" "'fixture: last line' in $STARTUP_TEXT"
  echo "OK: startup_no_result: the exit and the stderr tail are shown"
  stop_app
}

ran=0
for name in "${SCENARIOS[@]}"; do
  echo "== scenario $name"
  use_stub session-panel
  TRACE_STEP_MS=400
  PERF_TRACE=""
  SIRIO_FIXTURE_GATE=""
  SIRIO_FIXTURE_DONE=""
  SIRIO_FIXTURE_FLOOR=""
  SIRIO_FIXTURE_STOP_DELAY=""
  SIRIO_FIXTURE_TITLE_STORE=""
  SIRIO_FIXTURE_RENAME_ERROR=""
  SIRIO_FIXTURE_HANDSHAKE_GATE=""
  SIRIO_FIXTURE_RENAME_GATE=""
  SIRIO_FIXTURE_PERMISSION_FIELDS=""
  SIRIO_FIXTURE_PERMISSION_RULES=""
  SIRIO_FIXTURE_RULES_LOG=""
  SIRIO_FIXTURE_RULES_GATE=""
  SIRIO_FIXTURE_RESULT_GATE=""
  SIRIO_FIXTURE_TOGGLE=""
  SIRIO_FIXTURE_TOGGLE_GATE=""
  SIRIO_FIXTURE_RELOAD=""
  SIRIO_FIXTURE_RELOAD_GATE=""
  SIRIO_FIXTURE_SKILLS=""
  SIRIO_FIXTURE_CLI_BUSY_GATE=""
  SIRIO_FIXTURE_RETRY_GATE=""
  SIRIO_FIXTURE_STARTUP=""
  SIRIO_FIXTURE_LIMITS=""
  SIRIO_FIXTURE_USAGE=""
  SIRIO_E2E_TZ=""
  SIRIO_FIXTURE_HOOKS=""
  SIRIO_FIXTURE_HOOKS_GATE=""
  "scenario_$name"
  check_mise "after scenario $name"
  ran=$((ran + 1))
done
check_mise "at the end"
echo "mise config unchanged: ${MISE_BEFORE:-absent}"
cp "$RUN_DIR/github-requests.log" "$OUT_DIR/" 2>/dev/null || true
echo "artifact: $OUT_DIR"
if [ -n "$ONLY" ]; then
  echo "CHAT SESSION PANEL E2E PARTIAL ($ran of ${#ALL_SCENARIOS[@]} scenarios: ${SCENARIOS[*]})"
else
  skipped=$(( ${#ALL_SCENARIOS[@]} - ran ))
  echo "CHAT SESSION PANEL E2E OK ($ran scenarios, $skipped skipped, appearance ${APPEARANCE:-default}, state-only $STATE_ONLY, window ${WINDOW_SIZE:-default})"
fi
