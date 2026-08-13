# P53 non-drawing chat door contract

Status: `builder-claimed, unverified` until the named socket transcript and
`Scripts/ci-linux.sh` are replayed.

## Owned seam

`tiller_acp::ChatSession` is the non-drawing adapter. It owns one chat tab's
live `AcpClient`, folds ACP events into the persisted rendered-entry model from
`tiller_persistence`, and exposes the same snapshot to GPUI and control-socket
callers. `main.rs` must not maintain a second transcript, status, composer, or
permission state machine.

The stable chat identity is the tab id. `session_ref` remains only the agent's
wire-session reference, and `tab_state` remains UI state; neither is copied
into the transcript. `ChatSession::launch` loads the tab transcript before it
starts the ACP process. `ChatSession::restore` reads a transcript without
starting an agent, for a relaunch/readback proof.

## Exact main.rs call order

For a mounted chat tab, `main.rs` performs these calls in order:

1. **Open:** build `ChatSessionConfig::new(tab_id, worktree_id, command,
   cwd, database_path)`, call `ChatSession::launch(config)`, then call
   `session.read()` to form the `surface.chat.open` response.
2. **Send:** call `session.send(text)`, immediately call `session.read()` for
   the user row/`streaming` state, and keep calling `session.read()` as the
   mounted surface or socket observer asks for incremental output.
3. **Compose:** call `session.compose(text)`, then `session.read()`; text
   entered while streaming is reported as `queuedText`.
4. **Permission:** when the transcript exposes a pending permission row, call
   `session.respond_permission(request_id, option_id)`, then `session.read()`
   so the selected outcome is observable before the next ACP events arrive.
5. **Stop:** call `session.stop()`, then poll `session.read()` until its
   status is `stopped`; do not translate this state to `completed`.
6. **Close/relaunch:** call `session.shutdown()` on close or quit. On a
   transcript-only relaunch call
   `ChatSession::restore(database_path, tab_id, worktree_id)` followed by
   `read()`; when a live agent is wanted, `ChatSession::launch` performs the
   persisted read before ACP startup.

Every control response is a readback of that snapshot. The handler must not
acknowledge `send`, `stop`, or permission resolution with an input-only result.

## Wire methods and result shape

The protocol builders in `tiller_control::protocol::request` are:

| Method | Parameters | Readback that proves it |
| --- | --- | --- |
| `surface.chat.open` | optional `worktree` | `surfaceId`, `status`, `composerText`, `queuedText`, `transcript` |
| `surface.chat.send` | `surfaceId`, `text` | user row and `status=streaming`, followed by incrementally growing assistant rows |
| `surface.chat.compose` | `surfaceId`, `text` | `composerText` while idle or `queuedText` while streaming |
| `surface.chat.permission` | `surfaceId`, `requestId`, `optionId` | permission row changes from `pending` to `selected` with `optionId` |
| `surface.chat.stop` | `surfaceId` | `status=stopped`, plus a retained partial turn/footer |
| `surface.chat.read` | `surfaceId` | the complete current snapshot |

`transcript` is a JSON-array string using the existing rows convention. Each
row has `kind` (`user`, `assistant`, `thought`, `tool`, `permission`, `turn`,
or `error`), with `text`, `id`, `status`, and `optionId` where applicable.
`stopped` and `completed` are distinct wire values.
After a normal turn ends, a non-empty `queuedText` is submitted as the next
user turn and remains observable as `streaming`; a stopped turn does not
silently submit its queue.

## Named proof and ledger rows

- `tiller_acp::chat_session_streams_tool_permission_stops_and_restores` proves
  the adapter itself with the real ACP fixture, including partial output,
  tool completion, selected permission, stop, and SQLite restore.
- `chat_door_streams_stops_and_restores_transcript_over_a_real_socket` in
  `rust/crates/tiller_control/tests/control_integration.rs` is the executed
  Unix-socket transcript: open → send → incremental read → permission →
  completed read → relaunch restore → second send → stop → stopped read.
- `F-CHAT-01`, `F-CHAT-07`, and `F-CHAT-08`: `builder-claimed, unverified` at
  this non-drawing seam; the UI still needs to wire the calls and draw the
  resulting states.
- `D-J1`: `builder-claimed, unverified` — the chat door is executable and
  observable; `main.rs` wiring and the drawn J1 journey remain.
