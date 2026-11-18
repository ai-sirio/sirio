# A Claude chat talks to Claude Code, not to a wrapper

A chat tab for Claude Code drives the `claude` already on the user's PATH
over the stdio protocol the Claude Agent SDK uses internally
(`claude -p --output-format stream-json --input-format stream-json`), not
the `claude-acp` registry wrapper. `sirio_claude` is that protocol as types
and pure functions; `sirio_chat::claude` owns the process and emits the same
`ChatEvent`s the ACP client does, so `sirio_ui::chat` cannot tell which
transport a tab uses. The resolution lives in `sirio`'s `claude_transport`:
native when `claude` is on PATH at or above `sirio_claude::MIN_CLAUDE_VERSION`
(2.1.257, the version the official wrapper's SDK bundles and therefore
certifies), the wrapper otherwise, with the reason shown in Settings →
Agents. `SIRIO_CLAUDE_TRANSPORT=acp` forces the fallback for diagnosis.

**That protocol is not a published contract.** It is versioned with the
Agent SDK, and `claude` self-updates, so everything in `sirio_claude` is
written to degrade rather than reject: an unknown `type` or `subtype`
becomes an `Other`, unknown fields are ignored, a malformed line is skipped,
and a control request the CLI refuses degrades that one feature rather than
the session. `claude_answers_the_native_handshake_it_claims`
(`sirio_agents/tests/acp_conformance.rs`) is what notices the day the claim
stops holding; like its two siblings it SKIPs without the binary and is on
the release gate's skip list. The terminal pane is unaffected — it has
always run `claude` interactively.

**Control requests from the CLI** are answered by `sirio_chat::claude::worker`. The chat
answers `can_use_tool` (a permission card) and `elicitation` (an MCP server's question, a form
or a page to visit). Any other request gets an error that names its subtype,
`Sirio does not handle <subtype>`, and a `can_use_tool` or `elicitation` line that cannot be
read gets `Sirio could not read this … request`. An error answer to an elicitation reaches its
MCP server as `cancel`, and the turn goes on. A `control_cancel_request` closes the matching
card, which reads `withdrawn`, and the worker writes nothing back. An open elicitation counts as
an open question, as a pending permission does: it holds off the prompt watchdog and, unanswered,
expires on the permission clock (`PERMISSION_TIMEOUT`, five minutes) as `cancel`. Server text is
shown as plain text with control and bidi characters removed. A link is offered to open only when
it is an http(s) URL with a host, and that host is shown on its own line; a URL card completed in
the browser before anyone answered is accepted for the user. `claude_sends_mcp_elicitation_to_its_host`
(`sirio_agents/tests/acp_conformance.rs`) is the opt-in live check, and `elicitation_form`,
`elicitation_url`, `elicitation_withdrawn` and `unhandled_request` in
`Scripts/Tests/test-chat-session-panel-e2e.sh` cover the app end to end.

**The permission card's detail (A5).** A `can_use_tool` request carries more than the tool and its
input, and the worker keeps it as a `PermissionDetail` (`permission::detail_for`) on the card.
Missing fields leave the plain card; ACP, a restored card and a structured question have none.
- **Caption.** The CLI's `title` when one comes, else "Permission requested". `display_name` is
  only the tool's name (`Bash`, `Echo`), so it is not shown.
- **Detail lines,** under the body and inside its capped, scrolling area. In order: the
  description, `Reason:`, `Path:` and `MCP server: name (source)`. A description that is the
  command after trimming is not repeated, and the model's Bash description never replaces the
  command. `blocked_path` comes only for Bash and is the path the command writes, so it reads as
  "Path".
- **Reason.** The CLI's `decision_reason` when it has text. Otherwise `decision_reason_type` in
  plain words (`Matches an ask rule in user settings` from `decision_reason_rule_source`; also
  `workingDir`, `safetyCheck`, `subcommandResults`, `hook`, `mode` and `classifier`). An unknown
  type shows no line.
- **`default_to_no`.** The card starts on its first rejection (`question_dock::start_row`, shared
  by the card and the control snapshot), so Enter declines, and the digit keys and badges are off,
  because a digit answers without looking. **`suppress_always_allow_rule`** drops "Always
  allow". Neither field, nor `title`, reaches a stdio host on 2.1.326 (probe: they come from a
  desktop host's worded asks behind a server flag, from auto mode's fallbacks, or from an
  incomplete safety check), so only the fixture and unit tests exercise them.
  `requires_user_interaction` is parsed and not shown: the two tools that set it are already
  questions.
- **Visible text.** `sirio_ui::chat::visible_text` never removes a byte of what the user approves.
  `visible_text` shows each hidden character as `⟨U+XXXX⟩`: controls (ESC included), bidi
  controls, zero-width and invisible characters, U+00AD, U+180E, U+2028 to U+2029, U+2061 to
  U+2064, U+206A to U+206F, U+FFF9 to U+FFFB and the tag block U+E0000 to U+E007F. The body, the
  title, the caption, the description, the path and the server go through it; a command with an
  OSC or a bidi override reads as carrying one. Only the reason, which is prose the CLI may colour,
  also loses its ANSI escape sequences (`strip_escape_sequences`: CSI and OSC; a lone ESC stays,
  and is shown).
- **Read and proof.** `surface.chat.read` has `questionDock`: `""`, or the JSON of the open card
  (`caption`, `body`, `details` as `{label, text}`, `options`, `selected`, `digits`). The
  `permission_*` scenarios in `Scripts/Tests/test-chat-session-panel-e2e.sh` read it, and the Ely
  `permission` scenario reads it for ACP, where the card stays the plain one.
- **Out of scope (#783).** What "Always allow" does is unchanged. A safetyCheck's only suggestion
  is `setMode acceptEdits`, and suggestions have a `localSettings` destination.

**The session panel** is a column beside a chat's transcript and composer
(`sirio_ui::chat::session_panel`, about 300 px, docked while the chat is at least
640 px wide and laid over the transcript's right edge, above the composer, below that;
open or closed is the chat's own and survives a relaunch). Every card but Environment
starts folded, and which cards are folded does not survive a relaunch. Its cards: Environment
(project, the chat's own directory, the branch read live), Context (what fills the window,
folded by origin, from `get_context_usage` through `sirio_claude::context`; asked once on open
when no report exists yet), Connectors (the session's MCP servers by origin, with Sign in and
Reconnect and a switch on every server row, from `sirio_claude::mcp` and `sirio_chat::McpControl`,
whose calls wait for the CLI and so run off the GPUI thread; the switch is below), Permissions
(below), Trace (below) and Change request (the
branch's pull or merge request through the forge source, sharing the right panel's per-host
rate-limit pause, and read
only once the card is unfolded and a frame paints the column, so a chat in a background tab asks
the forge nothing). Opening the column runs a probe, `ChangeRequestSource::has_forge`, to decide
whether the Change request card is drawn at all; it reads local git config and the credential
file and never runs `gh` or `glab`, and a forge you are not signed in to shows a muted "Not
signed in to <host>" row rather than hiding the card. Connectors is Claude-native only and other
agents get a total-only Context. The debug-only `surface.chat.panel`
(`sirioctl surface chat panel <surfaceId> open|close|toggle <key>|activate <key>|switch <key> on|off`)
drives it, and `surface.chat.read` carries the column's sections and rows as the `sessionPanel`
JSON string, which is what `Scripts/Tests/test-chat-session-panel-e2e.sh` asserts on.
The connector switch (A6): every server row ends with an Ely `Checkbox`, whatever the server's
origin, and its tooltip reads `Saved in Claude Code's settings for this folder`. A click sends
`mcp_toggle` through `McpControl::toggle`, then reads `mcp_status` again; the debug action `switch`
(`surface.chat.panel` with `action: "switch"`, `key`, and `on` of `"true"` or `"false"`) does the same.
Sirio sends `mcp_toggle` only for that click or that action, never on connect, restore, reconnect,
reload, retry or a new session. The switch's state is read only from `mcp_status`: a server reading
`disabled` is off, so a row's `switch` in the `sessionPanel` JSON is `true`, `false` or, once the
switches are hidden, `null`, and `switchEnabled` says whether the switch takes a click (`false` while a call
about the server is open, bar the sign-in wait, which a switch ends; `null` with no switch); while a toggle is open the row reads `switching…` and shows the state
asked for, and a second flip of that server fails and sends nothing. A switch to the state the row
already shows sends nothing. The saved state is Claude Code's: it writes it per folder, in its own
settings, and a sibling worktree, being another folder, does not share it. For a user or project
server this was seen on a live run; for claude.ai and plugin servers it rests on a read of the
2.1.326 binary, not a live run. Sirio never reads or writes `.claude.json` or any other
Claude Code settings file; it learns the state only from the CLI's `mcp_status`. Flipping a
server off drops its sign-in poll, and a toggle that fails leaves the CLI's sentence under the row
while the switch keeps what `mcp_status` last said; a toggle that succeeds but whose follow-up read fails
says `Switched, but the new state could not be read: <error>` under the row. Row labels and these notes
go through `visible_text`. A CLI that refuses `mcp_toggle`
(`sirio_chat::is_refusal`) hides every switch until the next connection (`switches_retired`). The
switch sits in a wrapper that stops a mouse-down, so a click on it never reaches the row's own
Sign in or Reconnect; its element id is a hash of the row key, never the key or the row's index, since
Ely logs a checkbox's id on every click. `switchButtons` in the `sessionPanel` JSON holds each
switch's drawn bounds, as `stopButtons` does for Stop.
The reload row (A6): the Connectors card ends with a row `Plugins and skills` whose action is
`Reload` (`session_panel/reload.rs`, key `connectors.reload`), drawn only once the card has read the
servers (`Connectors::is_read`); the debug `activate` follows the same condition. While no turn runs it sends
`reload_plugins` and then, only if that applied, `reload_skills`; the slash list takes the `commands` of
the `reload_plugins` answer (`replace_commands`, then `AvailableCommands`), and a changed skill list
arrives as `commands_changed`; the card reads `mcp_status` again afterwards. The first `reload_plugins` carries `hold_on_cache_impact: true`,
so a reload that would reset the prompt cache is held: nothing is reloaded, no `reload_skills` is sent,
the row's note gives the cost warning with the servers the reload would add or remove and the LSP tool
change (names go through `visible_text`), and the action becomes `Reload anyway`, which sends the
request without the hold. `refuse_if_busy: true` is added only when the CLI's `initialize` answer lists
`reload_plugins_refuse_if_busy_v1`; a flag that is not sent is absent, never `false`. The chat is busy
while `is_streaming()` or while `cli_state()` (never `deciding_cli_state()`, so a turn the CLI started
counts) is `running` or `requires_action`: the row then sends nothing and says
`Claude is busy with a turn. Try again when it ends.`, which a turn's end or the CLI going idle clears.
The gate is read again after `reload_plugins` and before `reload_skills`, and the CLI's own busy
refusal is recognised by its `error_code` (`reload_plugins_busy`), never its text. A reload that
applied with load errors says `<n> plugins failed to load.`; any other failure says `Reload failed:`
and the CLI's sentence. A second Reload while one runs fails and sends nothing, the row reading
`reloading…`. A refusal of either verb (`sirio_chat::is_refusal`) hides the row until the next
connection. On the 2.1.257 floor the row stays: it has both verbs and refuses only `reload_output_styles`,
which is never sent. The floor has no capabilities, so `refuse_if_busy` is not sent there, and it ignores the
hold and sends no `held` key, which reads as applied.
`surface.chat.read` also carries `composerRow`, the composer band as a JSON string (`layout` `full`,
`compact`, `narrow` or `minimal`, the `band`, `root`, `send`, `chips`, `overflowButton`, `field` (the text field's box, its vertical padding
included) and `edge` (the strip along the composer's top that a drag resizes by) boxes as `[x, y, w, h]`,
`fieldRows` (the rows the field is drawn at, measured from the painted field), `maxRows` (the most rows it is drawn
to grow to, at rest), `storedMaxRows` (the app-wide `chat.composerMaxRows` the chat holds, before the cap),
`capRows` (half the chat's height minus a fixed 61 px band allowance, in rows of the field's drawn line height (18 px at the default interface font size), at least 3; null before the first paint), `resizing` (a drag
of the edge has moved and is held, so the field is pinned to its rows),
`docked`, `overflow` (what the layout moved into ⋯: `model` or `agent`, `attach` (both only in `minimal`), `fast`, `thinking`, `session-panel`,
`background-tasks`, `context`), `modeIcon` and `modeDanger` (what the render drew for the permission
pill, null and false when it drew no icon), `menu` (while ⋯ is open, the rows its render built, in order,
as `{id, enabled, tooltip}`; Fast's tooltip is the CLI's reason it is blocked) and `picker` (the open
composer picker: `mode`, `model`, `effort`, `thinking`, `overflow`, `history` or `context`; null when none
is); geometry, enums and labels only; not `composer`, which is the draft's text), and reports `firstVisibleEntry` and `lastVisibleEntry`, the first and last
transcript rows the list drew, so a reveal reads as the row being between them: a row that is
the transcript's last cannot reach the top of a list that fills its pane, and unpainted nothing is
drawn, so the two are equal.

**The Permissions card (A5)** lists the rules the session runs under, as the terminal's
`/permissions` does, from `list_permission_rules` (`sirio_claude::permissions`). It is read-only
and is drawn only for native Claude, where the client has rules control (`RulesControl`, a
detached handle like `McpControl`); an ACP chat has none. It starts folded. The section is absent
before the first answer, after a refusal and with no rules control.
- **Rows.** Groups Allow, Ask and Deny, then Directories, each with a count; each group folds on
  its own and starts open (row keys `permissions/allow`, `permissions/allow/0`,
  `permissions/directories`, ...; no row has an action). A rule row shows the rule and the CLI's
  description, tagged with its source in words (`user settings`, `managed settings`, ...), plus ` ·
  read-only` or ` · this session` from its editability; a source of `session` reads just "this
  session". A `notInEffect` rule says managed settings allow only their own rules. An empty list has
  no rows, and the summary counts every allow, ask and deny rule, folded groups included, and skips
  unknown behaviors. Rule and path text go through `visible_text`. Directories show the path only,
  because the CLI's `source` for a directory is unreliable. `withheld` is not shown: only a cloud or
  self-hosted connection sets it, never stdio.
- **Notes.** `managedOnly` adds "Managed settings allow only their own rules." Each skipped
  settings file is `Skipped <file>: <message>`.
- **Triggers.** The ask runs only while the panel is open, and the section's state is that of the
  current client: it is cleared with each new one. The triggers are opening the panel, restoring
  it open, unfolding the card, a connection becoming ready, a turn's end, and an "Always allow"
  answer (which adds a rule, so the card reads it at once, not at the turn's end). The connection
  and restore triggers are not exercised by an E2E.
- **Coalescing.** One ask is open at a time. A trigger while one runs leaves a single follow-up,
  however many come, so a held answer plus an "Always allow" and a turn's end make two requests in
  all. The ask runs in `background_spawn`, never on the GPUI thread, and is bounded at 10 s.
- **Refusal and error.** A refusal (`is_refusal`, the 2.1.257 floor's `Unsupported control request
  subtype`) retires the card until the next connection and is never asked again or shown. Any other
  failure, including a success with no `state.rules`, keeps the last good list and adds
  `Claude Code's rules could not be read: <error>`; with no list yet, the note stands alone.
- **Stale answers.** The open ask is a task held by the card. A new connection drops it, so the old
  connection's answer never lands.
- **The fixture's default.** With no `--fixture-permission-rules` file the fixture refuses the
  request as the floor does, so a scenario that wants rules must name a file. Live,
  `claude_lists_permission_rules` (`sirio_agents/tests/acp_conformance.rs`, `#[ignore]`) checks
  a real answer on a configured `CLAUDE_CONFIG_DIR`; below 2.1.325 it accepts a refusal or a
  parse, and it has run only on 2.1.326.

**The Trace card** shows what the native Claude transport reports of the chat's latest turn, and
only once the chat has received a `TraceEvent`, which no ACP client ever sends. Folded it is one
line (`Live · 34s`, `Waiting for you · 46s`, `Done · 14s · $0.31`, `Interrupted · 12s`,
`Failed · 1m 02s`); unfolded it adds six numbers, one lane per agent on a shared axis with the
permission it waits on beneath it, and a row that opens the tab. A lane or a wait scrolls the
transcript to its row (`Chat::reveal_span`, `reveal_wait`); consecutive tool rows are one box,
drawn at the run's last row, so a reveal of any of them lands there. `sirio_chat::claude::trace`
folds the CLI's lines into `TraceEvent`s, stamped with the instant the worker read them because
no line carries a time (the CLI's own durations win where it sends one); a main-thread model call
runs from the CLI's `status: requesting` line (the request going out) to `message_stop`, and
`message_start` only names its model, so the wait for the first token is part of the call; and
`sirio_ui::chat::trace::TraceStore` keeps 20 turns of at most 2 000 spans: past that a span is counted
and not kept, the card's LLM calls, Tool calls and Errors read as at least (`1998+`) and a muted row
says how many were left out (`3 spans past the 2000 cap not drawn`). An agent that runs in the
background as a subagent (`is_backgrounded` with a `task_type` of `local_agent`, or none) is traced
until the CLI says it ended: its launch `tool_result` does not end the call, its parented lines keep
feeding its lane, and it ends at the first terminal `task_updated` or `task_notification`. Its turn
stays `Live` past the main thread's `result` until its last agent has ended, even behind a later
turn. Any other background task (a `task_type` other than `local_agent`) is traced as its launch
only: its `tool_result` ends the call at once, with no report, and what it writes after is not
traced. The turns the CLI starts on its own once an agent completes are not drawn: their spend is
added to the latest prompt-started turn, whose cost reads `≥ $x` while its agents run, drops the
`≥` at the first of their results after the last agent ended, and still grows by later ones. A
turn's cost is its `result`'s `total_cost_usd` less the previous one's in the same process, and
unknown (a dash) for a resumed process's first turn, an interrupted turn and the turn after it; a
prompt written while agents run leaves the old turn's cost at `≥` what was known and gives the new
turn a dash, as does one written while a foreign agent (one a CLI-started turn launched) still runs
or after an agent ended and no `result` has read its last spend. A subagent's permission opens no
tool row of its own, only the one Permission card its lane's wait points at. The trace asks for
frames only from one loop per painted card or tab (`trace_frame::frame_driver`, each frame counted
as a `trace.frame` event in the performance trace).
The loop runs at 30 frames a second at most while a span runs (`Turn::is_moving`: a model call
streaming, a tool running or a subagent working, not one waiting for the user), ticks once a second
while the turn only waits, which keeps its clock and its "now" line moving with the marks held
still, and asks for nothing when nothing paints it or when the turn has ended and the CLI does not
show it as Live or Waiting (the paragraph below). Only the card follows that headline: the Trace
tab's loop still stops once its turn has ended, whatever the CLI says. A folded card's title ticks once a second while it
is painted and reads Live or Waiting.
**The CLI's own session state** is the second source of a chat's status, next to the turns Sirio
derives. Sirio launches the CLI with `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1`, which makes it write
`system` / `session_state_changed` lines (`running`, `idle`, `requires_action`; an unknown value is
ignored and the last known state stands). `ChatEvent::CliState` carries them to the chat, and
`surface.chat.read` reports the last one as `sessionState` (`""` until one arrives, and again after
every clear below). `Chat::claim()` reads it so that a chat stays Running while the CLI's background
agent works after the main `result`, which no turn shows: `running` and `requires_action` override
the turn-derived status (Running and NeedsInput; an open card outranks `running`), while `idle`,
like no state at all, only hands the decision back to the turns. `idle` cannot win outright because
one can land before the turn's held `TurnEnded` (the worker holds it for the `get_context_usage`
answer), or be stale from a turn that ended while a queued prompt streams; read as Done it would
fake a stop and a desktop notification between two queued turns. The cost is that Done lands with
`TurnEnded`, at most `CONTEXT_USAGE_GRACE` (2 s) after the result, as before. A CLI that writes no
state lines (one launched without the variable, or older than the event) keeps today's derivation;
the SDK bundle at 2.1.257 already knows the variable, and the behaviour was probed on 2.1.325.
`initialize`'s own `session_state` is ignored: the CLI reports it (`idle`) even without the
variable, so reading it would make `sessionState` read `"idle"` on a CLI that never sends state
lines, against the empty-when-none rule. The CLI does not resend `running` while it is already
running, for example while a background agent works, so a prompt typed or dequeued then clears the
state only if it is `idle`; `running` and `requires_action` stand. A `running` that arrives with no
prompt streaming, after `idle` or no state, starts a turn the CLI began on its own (a Monitor task's
event, a shell's natural end); the claim and the Trace card read it as `idle` until the CLI says
anything else, a prompt is sent or the state clears, because the stop policy has no debounce and
each such turn would be a stop and a desktop notification (`requires_action` still overrides, and
an open card makes it read as `requires_action`; `sessionState` stays the raw state). The state
clears on a transport error, a timeout, a failed send, the event source closing and a new connection
(`start_connection`, the path retry, a new conversation, a launch and a failed launch share), so a
CLI that dies never leaves the chat Running;
it does not clear on `TurnEnded`, Stop or a history open, since the same process lives on, and a
line that arrives after the client was taken is ignored. The Trace card's title, pill and tick
follow the same rule (`headline_state`): `running` reads Live and `requires_action` Waiting for you,
and an ended turn the CLI still shows so keeps its title's elapsed seconds running to now, folded or
open (an open card's Duration stat and "now" line follow the turn). The Trace tab keeps each turn's
own state, as it is a history of turns, so a Live card can sit above a Done row during the CLI's
follow-up turn. Accepted costs: a card that `TurnEnded` expires while the CLI still waits on it (a
background agent's permission) leaves the dot at needs-input with no dock, until the CLI's next
state line or `permission_timeout` (#761); Stop pressed during `requires_action` and a history open
with a pending card can do the same; and closing a chat tab asks for confirmation only while
`is_streaming()`, so it can close while the CLI's background agent runs (#762). The watchdog (#699:
`turn_in_flight`, `open_permissions`, `idle_for`) never reads the CLI state.

**Background tasks** (shells, Monitors and background agents the CLI runs) live in a registry on the
`Chat` (`sirio_ui::chat::background`), not as trace lanes. The CLI says `idle` as soon as a shell
starts, so a span that outlived its turn would break the trace's own rules; only agents survive a
turn's end, and `end_main_thread_work`, `Turn::close`, a span's child sweep, the 20-turn eviction
and the waterfall's axis all assume a span ends with its turn. The producer still traces a
background `Bash` or `Monitor` as its launch only (above), and a shell draws no bar of its own. An
entry is created by a `background_tasks_changed` row or by a `task_started` with `is_backgrounded:
true`. Its `live` flag means the CLI's last rows list it (a new entry starts live), and it has an
outcome once a terminal line arrives. A task has ended when it is not live or has an outcome, and is
running otherwise (its state word is `running`, or `stopping` after a Stop); `live` can stay true
after an outcome, since the rows may come after the terminal line. A task reads one of six state
words, none of them the CLI session state `running` above: `running` and `stopping` while it is live
with no outcome, `done` for `completed`, `interrupted` for `killed` or `stopped`, `failed` for
`failed`, and `ended` for any other outcome or a task the rows dropped with no terminal line. The
outcome is the first terminal `task_updated` or `task_notification` status and a later one never
changes it (a stop says `killed`, then `stopped`); rows and terminal lines may come in either order.
An ended entry stays until the next prompt is submitted, then it is dropped. The registry is
cleared, with its stopping marks, painted, in-flight, ended-window and trailer sets and refusal
flag, and its poll loop dropped, on a transport error, a timeout, a failed prompt send, the event
source closing and a new connection (`start_connection`); a late task event or read answer is
dropped unless the chat still has a client and the answer's connection generation is the current
one. The composer chip's number is the CLI's own count (`BackgroundTasksUpdate`), not the
registry's, and the clear sets it to 0 until the next connection reports its own.
`surface.chat.read` reports the registry as the `backgroundTasks` JSON string (`taskId`,
`toolUseId`, `kind`, `description`, `live`, `outcome`, `stopping`, `tail`, `tailNote`), whether the
CLI refused to read output as `taskOutputRetired`, and whether the popover is drawn as
`composerRow.backgroundPopover`.

**A shell's tail** is read only for a task whose tail a frame painted since the last tick. A draw
calls `Chat::paint_tail`: the Trace card's Background row (its last line) and the Trace tab's strip
(its last 6 lines). In a debug build `surface.chat.paintTail` marks a tail painted and
`surface.chat.stopTask` presses Stop. A poll loop ticks once a second while the registry holds a
candidate, a running `local_bash` task (a Monitor is one) or one that ended less than 5 s ago and
whose output has no trailer yet. An agent is never read. The exception to the painted rule is a read
still pending at a tick: its task keeps its mark. While the caller waits (up to 5 s) no read of that
task is sent; after the caller's timeout a tick that holds its mark asks again, and the worker
answers `busy` with no write while it still holds the read. Once the CLI has answered that read or
the worker has expired it, the next ask is a real read even if nothing paints any more. Each answer
but `busy` updates the entry, notifies the chat and refreshes the Trace tab, so the next frame
paints it and registers the paint again; this costs about one repaint of the chat a second while a
tail is on screen. `tail_lines` cleans the CLI's last 8 KiB: it strips ANSI sequences, keeps what
the last non-empty piece between `\r`s says (`abc\rX` reads `X`, where a terminal would show `Xbc`),
drops the leading U+FFFDs of its first line and, when the answer is truncated and another line
follows, the first line, and keeps 100 lines. A non-empty tail is never replaced by an empty one.
After the end the CLI appends `[killed]` or `[exited with code N]` a moment later, so an ended shell
is still read on painted ticks until its tail ends with a trailer or 5 s pass; after that it is
never read again. Any other error, or the caller's 5 s timeout, shows as that task's note, `output
unavailable: <first line>`; after a timeout the read stays pending in the worker until the CLI
answers it (the answer is lost, the caller has gone) or it expires. A refusal
(`sirio_claude::is_refusal`) retires the tails for the session, with no error shown (ADR 0001); they
come back with the next connection. At the 2.1.257 floor the CLI refuses `get_task_output` this way,
so a chat on the floor lists its tasks and can stop them but shows no tail. `stop_task` works at the
floor.

**The prompt watchdog** does not count the answer to a pending `get_task_output` as activity
(`touch_activity`), because a 1 Hz poll would otherwise hold off `prompt_timeout` for a turn that
hung on the model; every other line still counts, one that does not parse included, and #699's
watchdog is otherwise unchanged. The worker keeps at most one pending read per task: a second
request answers `Err("busy")` at once without a write, and the poll treats that as a tick that
repaints nothing. A read the CLI never answers stays pending past the caller's timeout, which costs
one stdin write per task and not one a second, and the worker expires it after 30 s.

**Stop** is fire and forget. Pressing it sets the entry's `stopping` mark and sends one `stop_task`;
the button then reads `Stopping…` and takes no click, so one press is one request. The CLI answers
`success` either way, and the mark clears when its own `task_*` lines end the task or the rows drop
it. A Stop the CLI ignores reads `Stopping…` until the task ends. Sirio does not declare
`perTaskStopAffordance`, so the CLI still kills background agents when a turn is interrupted; on
2.1.326 an interrupt killed no background shell, flag or no flag. The composer's task chip is a
button that opens a popover listing every running task with its description, a kind word (`shell`,
which a Monitor reads too, `agent`, the CLI's own `task_type` for any other, or none) and its Stop,
and no tail. It closes when no task is running, and Escape closes it before it would cancel the
turn. The Narrow and Minimal composer rows keep the count as a disabled row of ⋯ and have no
popover. On the Trace card, an agent's lane carries a Stop while its task is running; every task
whose Agent call the card does not draw as a lane (a shell, a Monitor, an agent of an earlier turn)
is a row of the card's Background group, so each task has one place there. The Trace tab lists every
task in a strip under the tree, drawn once the tab has a turn, with its state, a Stop while the task
is running and, for a shell with no refusal, its tail.

**A Monitor's own turns** follow A2's unprompted rule above. The CLI starts a turn of its own for
each Monitor line and for a shell's natural end; a shell or Monitor stopped with `stop_task` goes
`idle` with no turn, and a stopped agent does start one, with no `running` line before it in the
probe, so A2's rule does not see it. Any `result` is a `TurnEnded`, whether or not Sirio has a
prompt in flight: it adds a turn footer, stops streaming, sends the next queued prompt and disarms
the watchdog. So each CLI-started turn leaves a footer, and a prompt sent while one runs can be
shown finished, and the next queued prompt sent, at that turn's `result`. That is existing
behaviour, not changed here. A Monitor is a `local_bash` task, so it has the shell's entry, tail and
Stop. How a Monitor expires is not probed; an expiry with no terminal line still ends its entry,
because the rows are the liveness signal.

**A background shell outlives a chat's close and a reconnect (#765, a known limit).** It runs in its
own session and process group, so killing the CLI's group does not reach it, and the registry's
clear does not end it: after a reconnect Sirio neither shows it nor stops it.

**Errors and notices (A7): what each frame becomes.** Five CLI frames are new to the chat: the
`system/api_retry`, `system/notification`, `system/model_refusal_fallback`,
`system/model_refusal_no_fallback` and `system/informational` subtypes. The floor already sends
the first three. All of them ride the existing `SessionNotice` queue; there is no new `ChatEvent`.

- `system/api_retry` sets the status line to `Retrying {attempt} of {max_retries} in {seconds} s:
  {cause}`. Seconds are `retry_delay_ms` over 1000, rounded up, at least 1. The cause is `no
  response` when `no_response` is an object, then by `error`: `overloaded`, `server error` or
  `rate limited`; any other error reads `error {status}`, or `connection lost` with no status. The
  CLI sends the frame before any content and never for a stream that already began.
- `system/notification {key, text, priority, color?, timeout_ms?}` sets the status line to the
  CLI's `text`.
- `system/model_refusal_fallback` and `system/model_refusal_no_fallback` become an `info` row with
  the frame's `content`, as it is. The CLI's own sentence already names both models, so Sirio
  builds none.
- `system/informational` becomes an `info` row, or an `info-warning` row at level `warning`.
  An empty `content` adds nothing.

An `info` or `info-warning` row is a notice like the rate limit's, but it is stored (`"info"`,
`"info-warning"`) and restored; the rate limit's `Warning` is still never persisted. All CLI text
goes through `visible_text`: `text` of a notification as one line, every `content` and the startup
text with newlines kept. Rows and the status line are plain text, never markdown.

**The status line.** One line above the composer, selectable text. It shows a retry, or the best
live notification. A notification is kept for `timeout_ms`, or 10 s without it. The order is
`immediate`, `high`, `medium`, `low`, newest first among equals; an unknown or missing priority
counts as `medium`. A notification with the key of one already held replaces it and restarts its
timer. The tone is the frame's `color` when that is `error` or `warning`, else plain.
`surface.chat.read` has it as `statusLine`: `""` when nothing shows, else `{source, text, tone,
held}` (`key` and `priority` too for a notification); `held` counts live notifications, the one
shown included.

A retry line clears on content (text, thought, a tool call started, updated or completed, a plan),
turn end, an interrupt (`cancel_turn`), a `TransportError`, a `Timeout`, the event source closing
(`event_source_closed`), a new connection (`start_connection`, through `clear_all`) and its own
safety expiry. A notification clears only on its own timer, or on `start_connection`, so it can
outlive the turn (the CLI sends `stop-hook-error` just before the `result`). Clearing is per
source, since one drain can hold both. Each timer removes only its own entry, matched by a
sequence number from a counter that never resets in the chat's life, so an old timer never clears
a newer line.

**An answer the CLI did not stream is drawn.** This is the spec's A7 deviation "A stream that
fails midway". When a stream breaks midway, the CLI asks again without streaming and sends the
answer as one `assistant` message with no stream events. Sirio draws text from stream events, so
that answer used to be invisible. An `assistant` line now draws its text unless one of these
holds: the line has no id at all; the message's id was streamed; the line is synthetic (the
exhaustion's sentence); a text delta arrived outside any open message (the fixture's
`state_text`, which repeats its text in the `assistant` line); a message with no id is open. When
the last row that turn was a chunk, the text starts with `"\n\n"`, so it is not glued to the cut
reply.

Known limit: the notice queue is drained on any signal, so a notice can be applied before chunks
queued ahead of it. A `Retrying` can then be cleared at once. An `Info` row can land above the end
of the text it follows; the tail of the refused text then opens a row below the `Info` row, and an
unstreamed answer joins that tail without a `"\n\n"`. The CLI sends a refusal notice after its
stream ends, which keeps this rare.

**Why Claude Code could not start.** A CLI that fails before the session writes an error `result`
as its first stdout line, in place of the `initialize` answer, with `startup_failure_reason` and
`errors`. Sirio reads it during the handshake, because it comes there and nowhere later (every one
seen live was stdout line 0; a result after the `initialize` answer is out of scope). The launch
fails with `ChatError::Startup(text)`. The text is built from these parts, joined by a blank line:
the reason's sentence, the `errors` lines, and, for `invalid_config_file`, `File: {path}` with
`Backup: {path}` on the next line when there is a backup. An unknown reason reads
`Claude Code could not start ({reason}).` and then the CLI's lines. A result with no reason but
with errors, on an attempt that is not retried, reads `Claude Code could not start.` and its
errors. With no result, or a reasonless result with no errors and no config file, the text is
`Claude Code exited before it started` followed by the exit code and the stderr tail. These two
parts (`child_exit_report`, `stderr_tail_report`) are the ones a death mid-session also shows, after
its own `the Claude agent stopped` opening. The launch error branch in `sirio_ui::chat` shows
`visible_text(text, true)` as the row, with no prefix, kind `Connection` (not
`classify_connection_error`, since the CLI's lines could say "requires authentication") and still
retryable. Every other launch error keeps `could not launch ACP agent: …`.

A reason is final on any attempt. A resume that fails with no reason (a session the CLI does not
know) still retries without the resume, as before. A refused resume no longer leaves a turn behind:
the fresh session has no "ended" footer.

**The variable.** Without `CLAUDE_CODE_STARTUP_FAILURE_RESULTS=1` the CLI writes a result only for
`org_config_required_unavailable`, `org_config_refused` and `config_file_invalid`; every other
reason ends with stderr and exit 1. So `LaunchLine::chat().env` sets it, and `worker.rs` applies
that env to the CLI process the native chat starts. It is not set in Sirio's own environment, in a
settings file, in `LaunchLine::usage_probe()` or in an ACP launch. The CLI's own children (tools,
hooks, MCP servers) inherit it, as they inherit the other two variables in that list. The floor
(2.1.257) ignores it and writes no result with a startup reason (it still writes the reasonless
error result for an unknown `--resume`, which the resume retry rests on). Sirio never reads or
writes `.claude.json`.

**Proof and limits.** E2E: the `notice_*` and `startup_*` scenarios in `docs/agents/e2e-tests.md`.
Live (`#[ignore]`, `acp_conformance.rs`): `claude_reports_why_it_could_not_start` and
`claude_reports_api_retries_and_unstreamed_answers`, last run against CLI 2.1.327.
`session_writes_lost` (sent only on ENOSPC or EDQUOT, as the CLI exits) is not shown; issue #793
keeps it.

**A session's name (A4).** A manual rename of a tab (the tab bar's inline editor, the Set Title
modal, or the debug-only `surface.tabs.rename`) reaches every native Claude chat of the tab through
`apply_manual_tab_title`, and the chat asks the CLI to carry it:
`rename_session {title, source:"host", session_id}`
(`ControlRequest::rename_session`). Both fields go to every CLI, because the 2.1.257 floor ignores
them, so there is no version gate. The worker fills `session_id` at write time from its own id (the
launch's, replaced by `init` and by a refused-resume retry), never from the chat's
`agent_session_id`, which is unset until the first turn; 2.1.326 refuses an id that is not its
current session. Auto titles (the first prompt's, the summarizer's) and restore write the tab title
directly and never reach the CLI. A restored chat sends nothing, because whether its title was typed
by hand is not persisted.

**The chat's title state** (`sirio_ui::chat::agent_title`; `agentTitle` in `surface.chat.read`, `""`
until a manual rename, then a JSON string `{title, state[, error]}`). With no client the wanted
title is `pending`. Every connection that becomes ready sends it once (`sent`), since a new process
knows nothing of the last one's answer, and a connection that ends puts it back to `pending` and
clears its refusal and error. An answer (`ChatEvent::SessionRenamed`, carrying the title Sirio sent)
counts only with a client, while the state is `sent` and the title is the one wanted: anything else
is a late answer to an older rename or connection. Success is `accepted`. A refusal (`is_refusal`)
is `retired`: later renames wait unsent until the next connection, and nothing is shown. Any other
error is `failed`, with the warning `Claude Code kept its earlier name: <error>` in the transcript;
a rename that keeps failing warns once per connection. No answer changes the tab's title.

**One way only.** The CLI's `system/session_title_changed` (the echo of an accepted rename, and the
line 2.1.326 prints first when it resumes a titled session, before the handshake answer) stays an
`OtherSessionUpdate` that the chat drops: Sirio never adopts a name the CLI reports. Where the name
lands, per the probe: on 2.1.326 `claude agents --json` lists a live session under it and `claude -p
--resume "<title>"` resolves it, and the transcript gets a `custom-title` line once a turn has
created the file. The floor writes that line and a sidecar but never the `agents` registry, and
sends no echo. The CLI turns control characters into spaces, trims, keeps 200 code points and
refuses an empty title; the tab keeps Sirio's own text.

**The Trace tab** (`TabKind::Trace`, `sirio_ui::trace_tab`) is the full waterfall in the
Secondary half, one per chat, opened by the card's last row. It holds a handle to the chat's
trace, closes when its chat is released (`observe_release`, then `TraceTabEvent::Close`), and is
never saved and never restored, so `surface.layout.read` lists it as `trace` only while it is open.
`surface.trace.read <surfaceId>` (`sirioctl surface trace read`, every build) reports `open`, and for
a chat with a tab the shown `turn`, `turns`, `follow`, `dropped` (the spans the cap left out, 0 with
none, which its header also says) and the visible `spans`, each with `id`, `parent`, `kind`, `state`,
`ended`, `depth` and `folded`. It also reports `background`, the strip's rows as a JSON string
(`taskId`, `kind`, `description`, `state`, `stop`, `tail`, `tailNote`), whenever the tab is open.
`docs/superpowers/specs/2026-10-20-chat-session-panel-design.md` has the panel's design and
`docs/superpowers/specs/2026-10-21-chat-trace-waterfall-design.md`
the folded cards, the Trace card and the tab; `claude_streams_the_lines_the_trace_reads`
(`sirio_agents/tests/acp_conformance.rs`, opt-in because it spends a real turn, off the release
gate) notices the day the CLI stops streaming the lines the trace reads.

**What a response may make clickable or load** is an allowlist, enforced in one place:
`sirio_ui::chat::file_links::link_file_paths`, run over an assistant entry's document whenever it is built
(streamed chunk, restore). The text an agent writes can be prompt-injected, so a link, chip or picture in it
survives only as a web URL (`http`, `https`, `mailto`, any case) or as a checked local file (an existing
regular file, retargeted to a `file://` URL; a picture also non-empty and at most 10 MB, and read by its
absolute path). Everything else (`smb:`, `vscode:`, `tel:`, a drive or UNC path, an unresolved path, an anchor)
loses its mark and its text stays, a non-web bookmark or picture block becomes a paragraph, and a network or
device path (`//host/…`, `\\host\…`, `\\?\`, `\\.\`) is never even looked up on disk. The disk is asked while
the document is built (memoised per entry), never while it paints, and once at a click (`may_open_on_click`). A
file link is taken in `MarkdownBody`'s capture phase before bezel's text can hand its URL to the system opener,
so a change that touches either half (the link pass, or the capture handler) must keep the other's
assumption: nothing but web URLs reaches `cx.open_url`.
`Scripts/Tests/test-chat-session-panel-e2e.sh --only file_links` drives it with real clicks and drags.
