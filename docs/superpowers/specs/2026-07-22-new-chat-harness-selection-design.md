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

The explicit selected-agent `AppModel` path revalidates the selected agent's
current availability before creating any chat state. If the agent is no longer
installed, the path creates no chat tab or controller, triggers an ACP
installation-state refresh, and sets `AppModel.lastError` to a clear
user-facing unavailable-agent message. The existing global Error alert surfaces
that message after the native menu closes. It never silently selects another
agent.

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
  and selection, the explicit `AppModel` path must revalidate availability
  before creating a tab or controller. Reject the selection, trigger an ACP
  installation-state refresh, and set `AppModel.lastError` to a clear
  user-facing unavailable-agent message so the existing global Error alert
  appears after the native menu closes. Do not silently fall back to another
  agent.
- **Valid selection but launch failure:** Keep the newly created chat tab
  created and selected in the current worktree. Surface the existing
  `ChatController` state inline in `ChatPaneView`: `.needsAuth` shows
  **Authentication required** with **Retry**, and `.disconnected` shows
  **Agent disconnected** with **Restart agent**. Do not roll back the tab,
  silently fall back, resume a prior session, or automatically retry.
- **Conversation identity:** Every selection uses
  `startNewConversation: true`; a prior ACP session is never resumed by this
  flow.

## Test plan

Use model-level tests for the explicit selected-agent path in `AppModel`.
Verify that the selected ACP ID is canonicalized, persisted as last-used,
registered for activity, and used to create the expected `WorkspaceTab` and
controller. The valid-selection acceptance assertion must explicitly verify
that the tab is both created and selected in the current worktree.

Add a no-resume assertion proving that the explicit path creates a new
conversation and does not load or resume a prior session. Retain coverage for
the existing no-argument `openChatTab(in:)` default fallback behavior.

Add stale-selection coverage proving that when the selected agent is no longer
installed, the explicit `AppModel` path shows the unavailable-agent error via
`AppModel.lastError`/the existing global Error alert, creates no tab or
controller, and triggers an ACP installation-state refresh. Add launch-failure
coverage proving that a valid selection leaves the newly created tab selected,
shows the appropriate `ChatPaneView` banner and action for `.needsAuth` or
`.disconnected`, and does not resume, roll back, silently fall back, or
automatically retry.

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
  selects that tab in the current worktree, persists it as last-used, registers
  activity, and starts a new conversation.
- A stale selection is revalidated by the explicit `AppModel` path, explicitly
  rejected with a clear unavailable-agent Error alert after the native menu
  closes, followed by an ACP installation-state refresh; no tab or controller
  is created and no other agent is selected.
- A valid selection whose launch fails leaves the newly created tab selected
  and shows the corresponding inline `ChatPaneView` error banner: `.needsAuth`
  with **Authentication required** and **Retry**, or `.disconnected` with
  **Agent disconnected** and **Restart agent**. The tab is not rolled back and
  the flow does not silently fall back, resume, or automatically retry.
- The legacy no-argument `openChatTab(in:)` behavior remains intact for
  internal callers.
- Model and menu seam tests explicitly cover stale-selection alert/no-tab/
  refresh behavior and launch-failure selected-tab/visible-error/no-resume
  behavior, and implementation verification passes `Scripts/ci.sh`.
