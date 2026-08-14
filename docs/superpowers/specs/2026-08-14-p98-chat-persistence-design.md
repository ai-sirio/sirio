# P98 — persistent chat history design

## Goal

Keep the GPUI chat transcript after a process restart and expose the saved
chat tabs through a small history browser. The existing `tiller_persistence`
transcript model and the ACP consumer remain the source of truth; this work
connects the UI path to them.

## Diagnosis

The UI `Chat` path currently stores only an in-memory `Vec<Entry>`. Its
`restore_transcript(&str)` method is used by `retained_chats`, which survives
only while the current process lives. The persistence code is used by
`tiller_acp::ChatSession`, not by the GPUI chat surface. The shell already
persists chat tabs, so the tab record is the natural session identity.

## Design

### Stable identity and persistence API

Persist the tab record ID through `SessionTab` and `OpenTab` instead of
recreating it from the tab's current array position. A chat passes that ID to
the UI persistence seam. `AppDatabase` adds a chat-session listing query over
chat tabs with transcript rows, including title, agent identity, turn count,
and a monotonic last-activity timestamp stored with each turn. It also adds a
delete operation that removes the transcript while leaving the shell tab
record intact.

The existing `chat_turn` rows remain the durable content format. A migration
adds `updated_at`; no duplicate transcript schema is introduced. Existing
ACP callers continue to use `save_chat_transcript` and `load_chat_transcript`.

### UI save/load seam

`Chat` receives an optional database path and stable tab ID. On construction it
loads a `ChatTranscript` and maps it back to rendered entries. On each
`TurnEnded`, after unanswered questions are expired and the footer is added,
it groups entries into completed turns and saves one snapshot. The save is
settled-turn based, not token based. `retained_chats` remains as the
same-process fallback for the existing close/reopen gesture.

### History browser

The workspace owns a lightweight overlay loaded from the persistence listing.
It renders a no-history state on a fresh database, one row per saved chat, and
an Open action that selects the matching tab (or recreates it with the stable
ID and loaded transcript). Delete opens the existing GPUI warning prompt and
only removes the transcript after the user confirms. The overlay is reachable
from the command palette and has stable debug selectors for UI tests.

## Error handling

Persistence is best effort, matching the existing session store: database
open/read/write failures are logged and leave the live chat usable. Corrupt
turns continue to use the existing quarantine behavior. A missing history row
is an empty state, not an error.

## Testing

- Persistence tests cover timestamped chat listing, ordering, deletion, and
  migration of existing `chat_turn` rows.
- Chat tests cover conversion/round-trip of rendered entries and saving only
  after a completed turn.
- Workspace tests cover the empty browser, opening a saved row, and the
  confirmation gate before deletion.
- `Scripts/ci.sh` remains the final repository gate; no live ledger row is
  changed by this implementation.
