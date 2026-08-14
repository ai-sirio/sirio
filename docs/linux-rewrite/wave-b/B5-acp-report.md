# B5-acp — build report

Slice owner file: `rust/crates/tiller_acp/src/lib.rs` (only file touched — see
"File ownership" note below for one incident on a file I do **not** own).

All three rows are protocol-layer work only. Every row's own manifest text
also lists `rust/crates/tiller_ui/src/chat.rs` (and F-CHAT-02 additionally
lists `rust/crates/tiller_ui/src/settings.rs`) as expected files — that half
is out of this slice's ownership and is reported under `wantedForeignFiles`
per row below, not attempted.

## F-CHAT-02 — ACP auth-required detection

**Commit:** `5dcc3d4` — feat(acp): detect ACP auth-required and carry it as a
typed error/event

**What changed in `lib.rs`:**
- `InitializeInfo` gained `auth_methods: Vec<AuthMethodInfo>`, populated from
  `InitializeResponse.auth_methods` (new `AuthMethodInfo { id, name,
  description }` type, new `auth_method_info()` converter).
- `AcpError` gained `AuthRequired { methods: Vec<AuthMethodInfo> }`,
  detected by checking `agent_client_protocol::ErrorCode::AuthRequired`
  (wire code -32000) on whatever RPC failure aborted the connection before
  `started` was ever set — covers both `initialize` and `session/new`
  rejecting the handshake. `AcpClient::launch` now returns this typed
  variant instead of a generic `AcpError::Transport(String)`, with a
  `Display` impl that names the advertised auth method(s).
- The **mid-turn** case (a `PromptRequest` failing with the same error code
  after a session is already live — this is what the row's evidence
  actually shows: "real send confirmed... but transcript rendered nothing")
  reuses `AcpEvent::TransportError`, carrying `AcpError::AuthRequired`'s own
  `Display` text as its message, rather than a new `AcpEvent` variant — see
  the "AcpEvent cannot grow this wave" note below for why.

**Tests (in `lib.rs`'s own `#[cfg(test)] mod tests`, run via `cargo test -p
tiller_acp --lib`):**
- `session_creation_auth_required_error_becomes_typed_auth_required` — fixture
  agent advertises an auth method then rejects `session/new` with -32000;
  asserts `launch()` returns `AcpError::AuthRequired` with that method, and
  that the `{error:#}` text (what chat.rs already displays today) names it.
- `successful_session_still_carries_advertised_auth_methods` — an agent that
  advertises a method but still lets the session through still populates
  `InitializeInfo::auth_methods`.
- `prompt_auth_required_error_reaches_the_transcript_as_auth_guidance` —
  session created successfully, then `session/prompt` rejected with -32000;
  asserts the resulting event is `AcpEvent::TransportError` carrying the
  auth-guidance text, driven through the real worker/command-channel, not a
  direct function call.

**howToExercise (once the chat.rs half lands — not yet visible without it):**
Point a chat pane's agent command at a fixture/real CLI that rejects ACP's
`session/new` or a prompt with error code -32000. Today: `AcpClient::launch`
returns `Err` and chat.rs's existing catch-all renders `Entry::Error` with
whatever `{error:#}` says — which now *is* the auth-guidance text (e.g. "ACP
agent requires authentication (Login)") instead of a raw transport string,
so this is already a real, if unstyled, visible improvement over the status
quo through the existing generic error path. The full fix (a distinct
`ErrorKind::AuthRequired` banner with a "run `<agent> auth login`, then
Retry" CLI hint sourced from settings.rs's existing per-agent login argv) is
`wantedForeignFiles` work.

## F-CHAT-15 — session modes

**Commit:** `df463d1` — feat(acp): expose ACP session modes as a live,
queryable catalog

**Research finding:** ACP has a first-class "session modes" primitive
(`SessionModeState` on `NewSessionResponse.modes`, `CurrentModeUpdate`
session notifications, `session/set_mode` RPC) that is completely distinct
from the `SessionConfigOption` mechanism this crate already reads for
model/effort — and had zero references anywhere in `tiller_acp` before this
change. So: yes, the protocol supports a settable session mode; it was
simply never wired.

**What changed in `lib.rs`:**
- New `AgentMode { id, name, description }` / `ModeCatalog { current_id,
  options }` types and a `mode_catalog_from_state()` converter.
- `AcpClient::mode_catalog() -> Option<ModeCatalog>` — unlike
  `model_catalog()` (a fixed snapshot from startup), this is backed by a
  shared `Arc<Mutex<Option<ModeCatalog>>>` the worker updates in place, so a
  caller re-reading it after any signal sees the current state.
- `AcpClient::set_mode(mode_id)` sends `session/set_mode`; since
  `SetSessionModeResponse` carries no state back, a success updates
  `current_id` optimistically, and an agent-pushed `CurrentModeUpdate`
  notification corrects it authoritatively either way (both paths funnel
  through one `apply_current_mode()` helper).
- `notification_to_events` gained an explicit `SessionUpdate::CurrentModeUpdate`
  arm so it's distinguishable (`OtherSessionUpdate { kind:
  "CurrentModeUpdate(...)" }`) from the generic "other" bucket it fell into
  before.

**Tests:**
- `session_creation_carries_the_advertised_mode_catalog` — fixture's
  `session/new` response includes `modes`; asserts `mode_catalog()` matches.
- `agent_without_session_modes_reports_no_mode_catalog` — no `modes` field
  → `None`, not a synthesized empty catalog.
- `set_mode_updates_the_live_catalog_once_the_agent_confirms` — drives
  `client.set_mode("plan")` through the real worker and polls
  `mode_catalog()` until `current_id` flips, confirming the round trip
  through the command channel (not just the pure helper function).

**howToExercise (needs the chat.rs half — no user-visible surface yet):**
The status pill's mode label is currently a hardcoded 4-way UI-state string
per the row's own evidence, not agent-reported data, so nothing changes
in the running app from this commit alone. Once chat.rs polls
`client.mode_catalog()` and calls `client.set_mode()` from a popover
(modeled on the existing model/effort one, per the row's approach note),
the gesture to verify is: open a chat pane against an ACP agent that
advertises modes, click the mode segment of the status pill, see the
`available_modes` list open, click one, and see the pill update once the
`SetSessionModeResponse` (or a `CurrentModeUpdate`) confirms it.

## F-CHAT-33 — MCP-configuration-warning half

**Commit:** `5645b48` — feat(acp): capture MCP-configuration-flavored
stderr as queryable warnings

**Research finding:** ACP's wire protocol has **no** dedicated concept for
a misconfigured MCP server — I read the full v1 schema (`agent.rs`,
`client.rs`, `error.rs`, `mcp.rs`): no error code for it, no `SessionUpdate`
variant, nothing on `NewSessionResponse` beyond
`modes`/`config_options`/`meta`. The `unstable_mcp_over_acp` feature that
does exist is a JSON-RPC *proxy* (client relays MCP calls through the
agent), unrelated to reporting a connection/config failure. Tiller also
never populates `NewSessionRequest.mcp_servers` itself — any MCP server in
play is the underlying agent CLI's own separate config, entirely outside
ACP. So the only channel such a problem has ever been observed to use is
the agent subprocess's own stderr text.

**What changed in `lib.rs`:** `drain_stderr` previously read and discarded
every byte unconditionally (`while stderr.read(...).is_some_and(|n| n > 0)
{}` — no-op body). It now reads line by line and matches each against
`looks_like_mcp_warning()` — conservative on purpose: requires "mcp"
*and* a co-occurring failure word (error/fail/warn/unable/could not/
refused/timed out/invalid/not found), so benign "Connected to MCP server
'foo'" startup logging some adapters emit doesn't false-positive. Matches
land on a new bounded (20-entry), live `AcpClient::mcp_warnings() ->
Vec<String>` accessor, same live-cell pattern as `mode_catalog`.

**Tests:**
- `looks_like_mcp_warning_requires_mcp_and_a_failure_word` — direct
  positive/negative cases for the heuristic, including the "Connected
  successfully" negative case.
- `mcp_configuration_failure_on_stderr_is_captured_as_a_warning` — a fixture
  agent prints an MCP failure line to stderr before ever answering ACP
  requests; asserts `mcp_warnings()` eventually contains it, driven through
  the real subprocess/pipe/thread machinery.

**Outcome:** partially-built. The row's own approach text says the fix is "a
new ErrorKind variant reusing the existing Entry::Error render/OK-dismiss
path" — that render path and the `ErrorKind` enum both live entirely in
`tiller_ui/src/chat.rs`. This commit builds and tests the detection +
exposure half; nothing in the running app changes yet without the
chat.rs consumer (poll `client.mcp_warnings()`, dedupe against what's
already been shown, push a dismissible `Entry::Error` with a new
`ErrorKind::McpWarning`). No `howToExercise` gesture exists yet because
there is no render path to drive.

## Why no row added a new `AcpEvent` variant

`AcpEvent` is matched exhaustively (no wildcard arm) by two files I do not
own: `tiller_acp/src/chat.rs` (a sibling module in my *own* crate, folding
events into `EventFold`/`ChatSession` state — not listed in any wave-B
slice's owned-files list, so out of reach even though it's mine to compile)
and `tiller_ui/src/chat.rs` (owned by `B2-chat`, actively being edited this
wave). I found this out the concrete way: adding `AcpEvent::AuthRequired`
for F-CHAT-02 broke `cargo check -p tiller_acp` on `tiller_acp/src/chat.rs`
immediately. Rather than touch either file, every row's "report a new kind
of thing happened" need was rebuilt on primitives that don't require
widening that enum: a new `AcpError` variant (F-CHAT-02, only matched
within `lib.rs` itself), and live `Arc<Mutex<...>>`-backed accessors on
`AcpClient` a caller can re-read after any signal (F-CHAT-15, F-CHAT-33),
mirroring the pre-existing `model_catalog()` field but made live where a
static snapshot wouldn't be enough. `AcpEvent::TransportError` and
`AcpEvent::OtherSessionUpdate { kind }` (both pre-existing, safe to
construct anywhere) carry the human-readable signal in the two cases where
some event was still the right shape.

## File ownership incident (not a row, informational)

Mid-slice, a `git commit` I ran (before I started scoping every commit to
`-- rust/crates/tiller_acp/src/lib.rs`) picked up a staged-but-uncommitted
edit to `rust/crates/tiller_terminal/src/lib.rs` from another agent sharing
this git index (commit `df463d1`, 173 lines — an F-PER-06 process-group
fix, `terminate_process_group` → `terminate_descendant_process_groups`). I
never touched that file's content myself. I isolated the damage: another
agent's later commit (`538d543`, ostensibly unrelated "docs(B4-settings)")
already reverted that file back out of history — net effect on `main`
history is that both commits touching it cancel out, so nothing broken is
sitting in history. Whether the F-PER-06 *feature work* itself survived
elsewhere I can't verify from here (`git status` now shows that file
modified again in the working tree, i.e. someone is actively back on it).
As a safety net, in case it's needed, I saved the exact diff that got
swept up at
`/tmp/claude-1000/-home-enzopalmisano-Scrivania-Progetti-tiller/6c13680a-d3bb-4d5b-8c40-6c3bc45a5e73/scratchpad/terminal-sibling.patch`
before touching anything, and did not apply it back myself since I can't
tell whether it would conflict with that agent's current in-progress
state. Every commit after I noticed this used `git commit -m "..." --
rust/crates/tiller_acp/src/lib.rs` (pathspec-scoped, not just `git add`
scoped) to make the race structurally impossible going forward.
