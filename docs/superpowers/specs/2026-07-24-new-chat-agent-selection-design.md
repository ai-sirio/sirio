# New Chat Agent Selection — Design

**Date:** 2026-07-24
**Status:** Approved (brainstorming session)

## Goal

Make the agent for a chat a creation-time choice instead of a mutable per-chat
setting, and fix the chat tab icon that today always shows the Claude mark
regardless of the agent in use.

Three user-facing changes:

1. Remove the ability to switch agent inside an existing chat.
2. Clicking **New Chat** shows a submenu listing installed ACP agents to pick from.
3. Chat tab icons reflect the chat's actual agent.

## Root cause of the icon bug

`AppModel.openChatTab(in:)` always creates the tab as
`defaultChatAgentId ?? "claude-acp"`. Switching agent via
`ChatAgentToolbarPicker` calls `ChatController.switchAgent(to:)`, which mutates
only the controller — `tab.chatAgentId` (the persisted `TabContent.chat`
payload) is never updated. `WorkspaceTabIcon` reads `tab.chatAgentId`, so the
icon stays on the creation agent (almost always Claude).

Choosing the agent at creation and removing in-chat switching eliminates the
divergence: `tab.chatAgentId` is correct from the start and can never drift.

## Design

### 1. New Chat submenu (both entry points)

- `App/NewTabMenuItems.swift`: replace the flat `Button("New Chat")` with a
  `Menu("New Chat")` listing `appModel.agentCenter.installedAgents`, each item
  showing the agent icon via `AgentMenuIconCache` (same pattern as the
  terminal-agent section above it). Selecting an item calls
  `model.openChatTab(agentId: agent.id, in: worktree)`.
- Empty installed list → single item **Other agents…** →
  `appModel.openAgentsSettings()`.
- `App/SidebarView.swift` (worktree context menu, line ~44): same submenu.
  Extract a shared `NewChatMenuItems` view used by both entry points.

### 2. `openChatTab` takes the agent explicitly

- `AppModel.openChatTab(in:)` → `openChatTab(agentId:in:)`. No
  `"claude-acp"` fallback; the caller always supplies the id.
- `rememberChatAgent(_:)` stays (records last-used agent; harmless).
- `AppTests/AutoRenameWiringTests.makeChatTab` updated to pass an agent id.

### 3. Remove in-chat switching

- `App/Chat/ChatAgentToolbarPicker.swift`: strip the `Menu` wrapper. The
  toolbar keeps a **static indicator**: `AgentIcon` with fused status dot +
  agent display name. Chevron, `.help("Switch agent…")`, and the
  **Other agents…** entry are removed.
- `ChatController.switchAgent(to:displayName:)` deleted as dead code, plus any
  tests that exercise it.

### 4. Icon

- No change expected in `WorkspaceTabIcon`/`AgentIcon`: with `chatAgentId`
  correct from creation, the existing code renders the right mark. `AgentIcon`
  already normalizes the `-acp` suffix; registry agents outside the 5-agent
  catalog fall back to the colored monogram circle — by design.

## Non-goals / notes

- **No migration** for previously persisted tabs: restart already reconnects a
  chat with its creation agent today, so persisted state is self-consistent
  once switching is removed.
- Full 38-agent registry with disabled entries was considered and rejected —
  installed agents only (matches the current picker's list and the
  2026-07-22 harness-selection plan, which this design supersedes).
- Removing the toolbar indicator entirely was considered and rejected — the
  status dot is useful and costs nothing.

## Testing

- Pure menu-state seam (installed vs. empty list) covered in `AppTests`.
- Existing `WorkspaceTab`/`ProjectStore` round-trip tests untouched.
- Gate: `Scripts/ci.sh` prints `CI OK`.
