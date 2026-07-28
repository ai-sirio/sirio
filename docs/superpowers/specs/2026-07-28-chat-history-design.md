# Chat history and session restore

Date: 2026-07-28

## Problem

Two defects, one root cause each.

**1. A restored chat tab comes back empty.** The transcript is persisted correctly and
loaded into `ChatController.restored`, then discarded a moment later:

```swift
// App/Chat/ChatController.swift:291
if handle.didResume, let record {
    restored = []          // expects the agent to replay history
    sessionRecordId = record.id
```

That expectation holds for real ACP: `session/load` replays the conversation
(`ACPSession.swift:87-99`). It does not hold for the native transports.
`ClaudeStreamJSONDriver.connect` returns `didResume: true` without sending anything
back — `claude --resume` restores context inside the CLI rather than reprinting it on
stdout. `PiRPCDriver:557` and `CodexAppServerDriver:102` behave the same way. One flag
carries two different meanings, and the wrong one wins.

**2. There is no way to reach past conversations.** The data exists — `chatSession` +
`chatItem` hold every turn — but nothing lists it. A chat tab finds its transcript
through `latestSession(worktreeId:)`, the most recently active session *of the whole
worktree*, so a second chat tab in the same worktree adopts the first one's history.

## Scope

- Chat history menu, scoped to the current worktree, opened from an icon beside the
  `+` in the tab bar.
- Correct restore of chat tabs across app restarts.
- Deleting a chat, and a configurable retention limit.

Out of scope: cross-worktree or global history, search within history, exporting a
conversation.

## Design

### 1. Split the resume flag

`SessionHandle` gains `didReplayHistory` alongside `didResume`:

- `didResume` — the agent picked up an existing session; do not create a new one.
- `didReplayHistory` — the agent re-sent the conversation; drop our copy to avoid
  showing it twice.

Only the `session/load` branch of `ACPSession` sets `didReplayHistory: true`. The
native drivers (`ClaudeStreamJSONDriver`, `PiRPCDriver`, `CodexAppServerDriver`,
`OpenCodeHTTPDriver`) keep `didResume: true` with `didReplayHistory: false`, and
`ChatController` clears `restored` only on the replay flag.

### 2. Bind each tab to its own session

Migration **v15** (current head is v14, `AppDatabase.swift:171`):

```sql
ALTER TABLE chatSession ADD COLUMN title TEXT;
ALTER TABLE terminalTab ADD COLUMN chatSessionId TEXT;
```

The same migration backfills: for each worktree, the most recent non-empty
`chatSession` is assigned to that worktree's active chat tab. Every other legacy chat
tab keeps `NULL` and creates a fresh session when first opened. The ambiguity is
resolved once, in SQL, instead of by a runtime heuristic.

In memory, `TabContent.chat` carries the id:

```swift
case chat(agentId: String, sessionId: String?)
```

`nil` means "legacy tab, not yet initialized". Every new tab has an id from birth
because `openChatTab` calls `createSession` immediately — one INSERT, no subprocess.

Code this removes:

- `latestSession(worktreeId:)` leaves `ChatController.start()`; the controller loads
  `loadTranscript(sessionId:)` for its own id.
- The `EXISTS (SELECT 1 FROM chatItem …)` clause (`ChatSessionStore.swift:27`) moves
  from that lookup to the history query, where its purpose is self-evident: do not
  list chats that were never used.

A tab closed without a single message leaves an empty session behind.
`teardownChatController` deletes it when it has no `chatItem`; whatever escapes that
is collected by retention.

### 3. Titles

The existing auto-rename (`AppModel.swift:1994+`, throttled, summarizer-driven)
already writes `tab.title`. The same call site also writes `chatSession.title`. No
additional LLM calls. When the title is absent — auto-naming disabled, or a chat too
young to have been named — the history row falls back to
`"<Agent name> · <time>"`.

### 4. Detached chats

New state `ChatController.ChatState.detached`: transcript visible, composer enabled,
no subprocess.

Detached at birth:

- every chat tab restored at launch,
- every chat opened from the history menu.

Live at birth: a chat created from the `+`. The user has just picked an agent and is
about to type, so starting the process while they write hides the latency — `session/new`
costs ~40s on codex-acp, and paying it after Return is worse than paying it during typing.

Waking up: `send()` currently drops the message on a non-ready state
(`guard state == .ready else { return }`, `ChatController.swift:388`). It becomes: if
`.detached`, `await start()` first, then send. The typed text is never lost, and the
existing connecting state covers the wait.

`ChatPaneView.task` no longer calls `start()` unconditionally; it calls an `activate()`
that returns immediately for a detached controller.

Reopening Tiller with six chat tabs then costs zero agent processes instead of six —
today each one spawns when its view mounts.

The risk is that `.detached` must be handled everywhere `state` is read. The sensitive
spots are the banners in `ChatPaneView` (a sleeping chat must not read as "Agent
disconnected") and `AgentActivityModel`, which must not report running or idle for a
pane with no process.

### 5. History menu

An icon (`clock.arrow.circlepath`) beside the `+` in `TabBarView.swift:79-89`, reusing
the `Menu { … } label: { Image }` + `.buttonStyle(.plain)` + `.menuIndicator(.hidden)`
+ `AppTheme.meta` pattern already used there by both the `+` and the overflow
`chevron.down`.

Contents — sessions of the current worktree, `lastActivityAt DESC`, filtered on
`EXISTS chatItem`:

```
[agent icon] Chat title              2h ago
[agent icon] Another chat            yesterday
──────────────────────
Delete ▸    (submenu, same titles)
```

Delete lives in a submenu because `NSMenu` items accept no context menu on macOS.
This keeps opening a chat at one click — the action taken 95% of the time — for about
eight extra lines. Delete asks for confirmation, then removes the `chatSession` and its
`chatItem` rows; if that chat is open in a tab, the tab closes with it.

Clicking a row calls `openChatSession(sessionId:in:)`: if a tab of this worktree
already carries that `chatSessionId`, focus it (`focusTab`, `AppModel.swift:1532`);
otherwise create a detached tab, title from `chatSession.title`, agent from
`chatSession.agentId`.

### 6. Retention

Settings → General: `chat.history.retentionCount`, default 50 per worktree, `0` means
unlimited. Pruning runs at bootstrap: for each worktree, delete sessions beyond the N
most recent. Follows the existing `AppSettings` conventions (`autoNaming.enabled`,
`session.maxMountedWorktrees`).

## Testing

TDD with swift-testing, per package:

| Where | What |
|---|---|
| `TillerPersistenceTests` | v15 adds both columns; backfill assigns the recent session to the active chat tab and leaves the others `NULL` |
| `TillerCoreTests` (ProjectStore) | `saveTabs`/`loadTabs` round-trip `chatSessionId`; a legacy row without it loads as `nil` without crashing |
| `TillerACPTests` (ChatSessionStore) | `sessions(worktreeId:)` orders by activity and excludes empty ones; `setTitle`; delete cascades to `chatItem`; prune keeps the N most recent |
| `AppTests` (ChatController) | restore loads its own session rather than `latestSession`; `didReplayHistory: false` keeps `restored`; `didReplayHistory: true` clears it; `.detached` spawns nothing; `send()` from detached starts and then sends without losing the text |

`Scripts/ci.sh` must print `CI OK`. Two known traps in this repo: `AppTests` is not part
of `ci.sh` and runs separately (its selector goes by struct, not by `@Suite`), and the
`TillerTerminal` PTY tests are flaky under parallel load — rerun before blaming these
changes.

Manual checks, none of which the automated tests cover:

1. Chat with 2-3 turns, ⌘Q, reopen: transcript intact, no agent process alive
   (Activity Monitor).
2. Type into the restored chat: the agent starts and answers in the same thread.
3. History, open an old chat: it opens detached. Open the same one again: it focuses,
   no duplicate tab.
4. Delete a chat that is currently open: its tab closes.
5. Retention set to 2, restart: the two most recent survive.

## Main risk

`didResume` is read by five drivers. Splitting it wrong reintroduces the bug inverted —
duplicated history on ACP. The pair of tests covering both branches is what holds that
shut.
