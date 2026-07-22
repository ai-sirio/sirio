# New Chat ACP Harness Selection — Design

Date: 2026-07-22
Status: Approved

## Context

Tiller has two existing New Chat entry points: the tab-bar/menu action and the
worktree sidebar context-menu action. Both currently need to converge on the
same ACP-aware choice of harness. The available choices must come from the
current installation state managed by `AcpAgentCenter`, rather than from the
legacy terminal-agent catalog.

## Goals

- Make every New Chat action present a menu or submenu for choosing an ACP
  harness.
- List only dynamically installed ACP harnesses from
  `AcpAgentCenter.installedAgents`.
- Start a genuinely new conversation with the selected harness, with no
  session-resume fallback.
- Keep both entry points consistent and responsive to installation changes.
- Provide explicit loading, empty, invalid-selection, and launch-failure
  behavior.

## Non-goals

- Replacing or expanding the terminal `AgentCatalog`.
- Auto-creating a chat when an agent is installed or configured.
- Silently choosing another harness when the selected harness is unavailable.
- Changing existing internal callers that use the no-argument/defaulting
  `openChatTab(in:)` behavior.
- Introducing a new launch-error presentation path beyond the existing
  `ChatController` behavior.

## UX interaction

The tab-bar/menu New Chat action and the worktree sidebar context-menu New Chat
action expose identical submenu content and state. The submenu contains only
the dynamically installed ACP harnesses. Each row uses the installed agent's
existing display name, icon, and branding; no hard-coded terminal-agent list is
used.

While `AcpAgentCenter` is initially refreshing or loading installation state,
the submenu shows a loading state. It must not briefly present an empty state
that implies no harnesses are installed.

When no ACP harnesses are installed, the submenu shows an explanatory message
and an **Open Agents Settings** action. That action navigates to agent settings.
Installing or configuring an agent there does not automatically create a chat;
the user must invoke New Chat again.

Selecting an installed harness immediately creates and selects a chat tab in
the current worktree. The new tab is initialized with the selected ACP agent
and starts an explicitly new conversation. It never resumes a prior session.

## Architecture and data flow

The UI owns presentation of the menu state and emits the selected agent ID. It
does not resolve launch commands, apply fallback rules, or create tabs itself.

`AppModel` gains an explicit selected-agent chat-creation path. That path:

1. Validates and canonicalizes the selected ID using the ACP rules.
2. Rejects the request if the agent is no longer installed or otherwise no
   longer valid; it does not substitute another agent.
3. Creates a `WorkspaceTab` in the current worktree for the selected ACP agent.
4. Persists the selected agent as the last-used agent.
5. Registers the tab's activity.
6. Creates `ChatController(startNewConversation: true)` for the new tab.

The existing no-argument/defaulting `openChatTab(in:)` remains available for
internal callers and retains its current fallback behavior. The explicit
selected-agent path is the path used by both New Chat entry points.

The shared menu reads the live ACP installation state from
`AcpAgentCenter.installedAgents`. Both entry points therefore render the same
dynamic source rather than maintaining separate lists or snapshots.

## Edge and error handling

- **Loading:** During the initial refresh/loading interval, render a loading
  state instead of an empty state.
- **No installed harnesses:** Show the explanatory empty state and
  **Open Agents Settings**. Settings navigation has no chat-creation side
  effect.
- **Agent removed after display:** If an agent disappears between menu display
  and selection, reject the selection explicitly and refresh the installation
  state. Do not silently fall back to another agent.
- **Valid selection but launch failure:** Use the existing
  `ChatController` launch-error behavior. Do not silently resume a previous
  session or retry with another harness.
- **Conversation identity:** Every selection uses
  `startNewConversation: true`; a prior ACP session is never resumed by this
  flow.

## Test plan

Use model-level tests for the explicit selected-agent path in `AppModel`.
Verify that the selected ACP ID is canonicalized, persisted as last-used,
registered for activity, and used to create the expected `WorkspaceTab` and
controller.

Add a no-resume assertion proving that the explicit path creates a new
conversation and does not load or resume a prior session. Retain coverage for
the existing no-argument `openChatTab(in:)` default fallback behavior.

Test the menu state/action seam for loading, no-agent, settings navigation, and
selection actions. Verify that the tab-bar/menu and sidebar context-menu use
the same dynamic ACP source and produce the same installed-agent rows.

During implementation verification, run `Scripts/ci.sh` and require its
`CI OK` result.

## Affected areas

- `App/NewTabMenuItems.swift` — shared New Chat menu/submenu content and state
  actions.
- `App/SidebarView.swift` — worktree sidebar context-menu integration.
- `App/AppModel.swift` — explicit selected-agent creation path, ACP
  canonicalization, persistence, activity registration, and controller setup.
- `App/AcpAgentCenter.swift` — dynamic installed-agent source and refresh state.
- `App/Chat/ChatController.swift` — existing new-conversation and launch-error
  behavior used by the flow.
- `Packages/TillerACP/.../AgentLaunchSpec.swift` — ACP launch rules used to
  validate and canonicalize the selected agent.
- `Packages/TillerCore/.../WorkspaceTab.swift` — selected-agent tab state.

No product code, tests, or project configuration is changed by this design
document itself.

## Acceptance criteria

- Both New Chat entry points expose the same submenu content.
- The submenu lists only currently installed ACP harnesses from
  `AcpAgentCenter.installedAgents` and uses existing agent branding.
- Loading state is visible during initial refresh; no false empty state appears.
- The no-agent state explains the situation and offers **Open Agents Settings**
  without creating a chat.
- Selecting an available agent immediately creates a current-worktree chat tab,
  persists it as last-used, registers activity, and starts a new conversation.
- A stale selection is explicitly rejected and followed by a refresh, with no
  fallback to another agent.
- Launch failures use existing `ChatController` error behavior, with no
  silent resume or fallback.
- The legacy no-argument `openChatTab(in:)` behavior remains intact for
  internal callers.
- Model and menu seam tests cover the behavior above, and implementation
  verification passes `Scripts/ci.sh`.
