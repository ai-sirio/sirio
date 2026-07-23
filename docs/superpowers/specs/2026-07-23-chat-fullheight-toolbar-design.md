# Chat Full-Height Layout (OnPeer-style) — Design

Date: 2026-07-23
Status: approved

## Goal

Make the chat tab occupy the full vertical space of the detail column, like
OnPeer: no per-pane header above the transcript, agent selector in the window
titlebar (top right), conversation actions in the composer. The right panel
stays hidden by default (already the case).

## Decisions made during brainstorming

- Tab bar: **unchanged** (stays above the content for all tab types).
- Usage bar: **unchanged** (stays below the content when enabled in Settings).
- Chat header: **removed**; its controls are relocated (see below).
- Right panel: **no code change** — `AppSettings.defaultRightPanelVisible` is
  already `false`. A locally persisted `rightPanel.visible = true` preference
  is user state, not a default.

## Changes

### 1. ChatPaneView — remove the header

`App/Chat/ChatPaneView.swift`

- Delete the `header` view, the `stateChip` view, and the `Divider()` under
  the header.
- The transcript now starts directly below the tab bar.
- All state banners (needsAuth, disconnected, promptError, mcpWarning, plan
  awaiting approval) stay exactly where they are: they only appear in
  exceptional states and cost no space at rest.

### 2. Window toolbar — agent picker for the active chat tab

`App/ContentView.swift` + new `App/Chat/ChatAgentToolbarPicker.swift`

- When `model.route == .workspace` **and** the active tab of the selected
  worktree has `.chat` content, the toolbar's `.primaryAction` group gains a
  leading item: `ChatAgentToolbarPicker`.
- `ChatAgentToolbarPicker` receives the active tab's `ChatController`
  (obtained via `model.chatController(for:in:)`, same call ContentView's
  `terminalStack` already uses) plus `appModel`.
- Label: `AgentIcon` + agent display name + chevron, with a small status dot
  overlaid on the icon's bottom-trailing corner. The old header `stateChip`
  is replaced by this dot.
- Status dot colors (pure function, unit-tested):
  - `.ready` → green
  - `.prompting` → orange
  - `.connecting` / `.needsAuth` / `.disconnected` / `.idle` → gray
- Menu content is the same as today's header menu: one entry per
  `appModel.agentCenter.installedAgents` (current agent disabled, icon via
  `AgentMenuIconCache`), divider, "Other agents…" opening agents settings.
  Selecting an agent calls `controller.switchAgent` and
  `appModel.rememberChatAgent`, unchanged.
- When the active tab is not a chat (terminal/markdown), the item is absent
  and the toolbar looks exactly as it does today.

### 3. Composer — follow + new conversation

`App/Chat/ChatComposerView.swift`

- `controlBar` gains two borderless icon buttons in the trailing cluster,
  placed after `contextUsageIndicator` and before the paperclip:
  - **Follow agent**: `eye` / `eye.fill`, toggles `controller.isFollowing`,
    accent-tinted when active. Same `.help` text as today.
  - **New conversation**: `plus.bubble`, calls `controller.newConversation()`.
- Behavior identical to the removed header buttons; icon-only (no text
  labels) to match the compact pill row.

## Not in scope

- No changes to TabBarView, UsageBarView, SidebarView, RightPanelView.
- No new settings.
- No layout changes to the HSplitView / rounded-card detail column.

## Testing

- New pure function mapping `ChatController.State` (or its case) → dot color
  lives where it is testable without SwiftUI and gets a `swift-testing` test.
- Everything else is view plumbing: verified by `Scripts/ci.sh` (must print
  `CI OK`) plus a manual smoke check:
  1. Open a chat tab → no header row; transcript starts under the tab bar;
     agent picker with status dot appears top-right in the titlebar.
  2. Switch agent from the toolbar picker → conversation switches agent.
  3. Follow and new-conversation buttons work from the composer.
  4. Switch to a terminal tab → toolbar picker disappears.
  5. Fresh launch (or after resetting `rightPanel.visible`) → right panel
     hidden.
