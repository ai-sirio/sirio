# P52 chat persistence contract

Status: `builder-claimed, unverified` until the named relaunch test and the
repository gate are replayed.

## Decisions

1. **Stored truth is the rendered transcript, not ACP wire traffic.** A
   `ChatTurn` is an ordered list of `ChatEntry` values that contain the text
   and card fields the surface renders. Tool cards retain their id, title, and
   terminal status. Permission cards retain their options and a typed
   `ChatPermissionOutcome` (`Pending`, `Selected`, `Cancelled`, or `TimedOut`).
   ACP messages are transport input, not durable truth: they are larger,
   protocol-specific, and may not replay into the same renderer after an
   adapter changes. The surface maps settled ACP events into these rendered
   entries before saving.

2. **The transcript is owned by the chat tab.** `ChatTranscript.tab_id` is the
   only identity used by this module and is a foreign key to `tab(id)`;
   `chat_turn` stores one row per turn ordered by `ordinal`. The existing
   `session_ref` machinery remains the agent's resumable wire/session handle,
   and `tab_state` remains UI state such as draft/selection. Neither is copied
   into transcript payloads, so a session resume failure cannot overwrite the
   rendered history and a UI-state migration cannot change transcript truth.
   Closing/removing the tab cascades its transcript rows.

3. **Growth is bounded at 1 MiB of serialized turn payloads per chat tab.**
   `save_chat_transcript` replaces the snapshot atomically and keeps the newest
   complete turns whose JSON payloads fit under the cap. It never slices a JSON
   entry; a single turn larger than the cap is omitted. This gives a hard,
   inspectable bound with no per-chunk writes and preserves the newest usable
   cards first.

4. **Migration is additive and forward-only.** Migration v6 creates
   `chat_turn` with `IF NOT EXISTS`, a tab foreign key, and a composite primary
   key. It runs in the existing migration transaction, so an older v1–v5
   database keeps all prior rows if the process fails. `NewerSchema` remains
   the refusal path for databases whose `user_version` is above the crate's
   migration count.

## Surface interface

The surface must use these public methods; it must not write SQLite rows
directly:

```rust
db.save_chat_transcript(&ChatTranscript { tab_id, turns })?;
let restored = db.load_chat_transcript(tab_id)?;
```

Save at turn settle, permission resolution, terminal tool-card completion,
tab close, and application quit. Load before starting the live agent so a
restored tab can render history without treating `session_ref` as transcript
content. The caller should persist only settled/permanent entries and leave a
live streaming partial out until the next settle point.

## Named proof and ledger row

- `chat_transcript_survives_process_relaunch_with_tool_and_permission_outcome`
  launches `chat_relaunch_helper` once to write, lets that process exit, then
  launches it again to read and compare the same tool-card and selected
  permission-outcome turns.
- `F-PER-01`: `builder-claimed, unverified` — transcript restoration is proved
  by the named test above, but the full gate still has to be replayed.
- `D-J1`: `builder-claimed, unverified` — the persistence half is complete at
  this seam; chat/UI wiring remains for the surface owner.
