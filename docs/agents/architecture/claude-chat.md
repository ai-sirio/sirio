# A Claude chat talks to Claude Code, not to a wrapper

A chat tab for Claude Code drives the `claude` already on the user's PATH
over the stdio protocol the Claude Agent SDK uses internally
(`claude -p --output-format stream-json --input-format stream-json`), not
the `claude-acp` registry wrapper. `sirio_claude` is that protocol as types
and pure functions; `sirio_acp::claude` owns the process and emits the same
`AcpEvent`s the ACP client does, so `sirio_ui::chat` cannot tell which
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

**The session panel** is a column beside a chat's transcript and composer
(`sirio_ui::chat::session_panel`, about 300 px, docked while the chat is at least
640 px wide and laid over the transcript's right edge, above the composer, below that;
open or closed is the chat's own and survives a relaunch). Every card but Environment
starts folded, and which cards are folded does not survive a relaunch. Its cards: Environment
(project, the chat's own directory, the branch read live), Context (what fills the window,
folded by origin, from `get_context_usage` through `sirio_claude::context`; asked once on open
when no report exists yet), Connectors (the session's MCP servers by origin, with Sign in and
Reconnect, from `sirio_claude::mcp` and `sirio_acp::McpControl`, whose calls wait for the CLI
and so run off the GPUI thread), Trace (below) and Change request (the branch's pull or merge
request through the forge source, sharing the right panel's per-host rate-limit pause, and read
only once the card is unfolded and a frame paints the column, so a chat in a background tab asks
the forge nothing). Opening the column runs a probe, `ChangeRequestSource::has_forge`, to decide
whether the Change request card is drawn at all; it reads local git config and the credential
file and never runs `gh` or `glab`, and a forge you are not signed in to shows a muted "Not
signed in to <host>" row rather than hiding the card. Connectors is Claude-native only and other
agents get a total-only Context. The debug-only `surface.chat.panel`
(`sirioctl surface chat panel <surfaceId> open|close|toggle <key>|activate <key>`) drives
it, and `surface.chat.read` carries the column's sections and rows as the `sessionPanel`
JSON string, which is what `Scripts/Tests/test-chat-session-panel-e2e.sh` asserts on.
`surface.chat.read` also carries `composerRow`, the composer band as a JSON string (`layout` `full`,
`compact` or `narrow`, the `band`, `root`, `send`, `chips` and `overflowButton` boxes as `[x, y, w, h]`,
`docked`, `overflow` (what the layout moved into ⋯: `fast`, `thinking`, `session-panel`,
`background-tasks`, `context`), `modeIcon` and `modeDanger` (what the render drew for the permission
pill, null and false when it drew no icon), `menu` (while ⋯ is open, the rows its render built, in order,
as `{id, enabled, tooltip}`; Fast's tooltip is the CLI's reason it is blocked) and `picker` (the open
composer picker: `mode`, `model`, `effort`, `thinking`, `overflow`, `history` or `context`; null when none
is); geometry, enums and labels only; not `composer`, which is the draft's text), and reports `firstVisibleEntry` and `lastVisibleEntry`, the first and last
transcript rows the list drew, so a reveal reads as the row being between them: a row that is
the transcript's last cannot reach the top of a list that fills its pane, and unpainted nothing is
drawn, so the two are equal.

**The Trace card** shows what the native Claude transport reports of the chat's latest turn, and
only once the chat has received a `TraceEvent`, which no ACP client ever sends. Folded it is one
line (`Live · 34s`, `Waiting for you · 46s`, `Done · 14s · $0.31`, `Interrupted · 12s`,
`Failed · 1m 02s`); unfolded it adds six numbers, one lane per agent on a shared axis with the
permission it waits on beneath it, and a row that opens the tab. A lane or a wait scrolls the
transcript to its row (`Chat::reveal_span`, `reveal_wait`); consecutive tool rows are one box,
drawn at the run's last row, so a reveal of any of them lands there. `sirio_acp::claude::trace`
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
still, and asks for nothing once the turn has ended or when nothing paints it. A folded card's title
ticks once a second while it is painted.
**The Trace tab** (`TabKind::Trace`, `sirio_ui::trace_tab`) is the full waterfall in the
Secondary half, one per chat, opened by the card's last row. It holds a handle to the chat's
trace, closes when its chat is released (`observe_release`, then `TraceTabEvent::Close`), and is
never saved and never restored, so `surface.layout.read` lists it as `trace` only while it is open.
`surface.trace.read <surfaceId>` (`sirioctl surface trace read`, every build) reports `open`, and for
a chat with a tab the shown `turn`, `turns`, `follow`, `dropped` (the spans the cap left out, 0 with
none, which its header also says) and the visible `spans`, each with `id`, `parent`, `kind`, `state`,
`ended`, `depth` and `folded`. `docs/superpowers/specs/2026-10-20-chat-session-panel-design.md`
has the panel's design and `docs/superpowers/specs/2026-10-21-chat-trace-waterfall-design.md`
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
