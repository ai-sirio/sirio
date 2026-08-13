# P51 chat surface control contract

`project.list` and `project.add` are the shell-owned front door for the first
half of J1. `workspace.select` remains the existing open-worktree transition.
The remaining J1 doors belong to the mounted `Chat` surface; the socket must
observe that entity's state rather than maintain a parallel transcript.

## Methods

All methods use the existing dotted `surface.*` convention. `surfaceId` is the
stable chat-pane id returned by `surface.chat.open` and is required by later
calls.

| Method | Parameters | Observable result |
| --- | --- | --- |
| `surface.chat.open` | optional `worktree` | `surfaceId`, `status=idle`, `composerText`, `queuedText`, and an empty `transcript` row array |
| `surface.chat.send` | `surfaceId`, optional `text` | The user row is present and `status=streaming` before the ACP turn ends; later reads expose streamed assistant text, tool rows, and permission rows |
| `surface.chat.compose` | `surfaceId`, `text` | Idle text is in `composerText`; text entered during `streaming` is in `queuedText` and is not in the sent transcript yet |
| `surface.chat.stop` | `surfaceId` | `status=stopped`, distinct from `completed`; the transcript retains the partial turn and the read before/after pair proves the transition |
| `surface.chat.read` | `surfaceId` | `status` (`idle`, `streaming`, `completed`, `stopped`, or `error`), `composerText`, `queuedText`, and transcript rows with `kind` (`user`, `assistant`, `tool`, `permission`, `turn`) |

When ACP reports a normal turn end, `status` becomes `completed` and queued
text is sent as the next user row. A stopped turn never reports as completed.
`surface.chat.read` is the observation door for every transition; no method is
considered complete if it only injects input.

The CLI mirror is `tillerctl surface chat open|send|compose|stop|read`; these
names must remain the wire method names with the space-separated CLI grouping,
as `tillerctl surface changes ...` already does.

## J1 handoff

P51 implements and advertises the project half as `builder-claimed,
unverified`: clean `project.list`, observable `project.add`, persisted catalog,
sidebar refresh, and the existing `workspace.select` route. The chat methods
above remain doorless until `Chat` exposes a non-drawing control adapter and
readback to `tiller/src/main.rs`. That adapter must be the same state used by
the composer, stop control, ACP event renderer, and transcript restoration;
the shell must not invent a second chat state machine.
