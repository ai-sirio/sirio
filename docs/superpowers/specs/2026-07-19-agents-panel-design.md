# Agents Panel in Right Sidebar — Design

Date: 2026-07-19
Branch: `agent_panel`

## Goal

Add an "Agents" section to the right sidebar showing agents created and currently running in the selected worktree, including a tree of subagents spawned by a parent agent. The existing worktree tools (Files/Diff/Status) keep the top 2/3 of the panel; the new Agents section takes the bottom 1/3.

## Decisions

- **Layout**: vertical split — existing tools on top (2/3), Agents section below (1/3). Divider draggable (`VSplitView`), initial fraction persisted via `@AppStorage`.
- **Scope**: selected worktree only (chat tabs + terminal panes of that worktree).
- **Subagents**: both worlds from the start —
  - Chat (ACP): derived from `ToolCall` items of Task/agent-spawn kind in the transcript.
  - Terminal: recursive child-process walk (libproc), matching comm names against `AgentCatalog`.
- **Interaction**: click a top-level node → focus the corresponding tab/pane. Subagent click selects its parent. Expand/collapse for the tree. No stop/kill actions.
- **Lifecycle**: finished agents remain listed with `done`/`error` status while their pane/tab stays open; they disappear when the pane/tab closes (consistent with `paneClosedClearsAllState`).

## Architecture

Chosen approach: **pure tree-building logic in TillerCore + thin observable aggregator in App**. Respects package boundaries (no SwiftUI/AppKit in packages), fully testable, reads from — never mutates — the fragile `AgentActivityModel`.

### 1. Data model (TillerCore)

New file `Packages/TillerCore/Sources/TillerCore/AgentTree.swift`:

```swift
public struct AgentNode: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case chat(tabId: UUID)          // ACP session
        case terminal(paneId: UUID)     // PTY pane
        case subagent                   // child node, not directly focusable
    }
    public enum Status: Equatable, Sendable {
        case running, idle, needsInput, done, error
    }
    public let id: String        // stable: "chat:<tabId>", "term:<paneId>",
                                 // "term:<paneId>:pid:<pid>", "chat:<tabId>:tool:<toolCallId>"
    public let agentId: String   // AgentCatalog id ("claude", "codex", …) or process name for unknown subagents
    public let title: String     // display name + task description for chat subagents
    public let kind: Kind
    public let status: Status
    public let children: [AgentNode]
}
```

`AgentTreeBuilder` — pure struct with a static build function. Inputs (snapshots, all value types):

1. Terminal panes of the worktree with `agentStatus`/`paneAgents` (from `AgentActivityModel`) plus a child-process tree `[paneId: [ProcessNode]]` where `ProcessNode = (pid, name, children)`.
2. Chat tabs of the worktree with `ChatController` state plus active Task-type tool calls from the transcript.

Output: `[AgentNode]` ordered chat first, then terminals, by tab creation order.

Status mapping:

| Source | AgentNode.Status |
|---|---|
| ChatState `.prompting` | `running` |
| ChatState `.ready` / `.idle` | `idle` |
| ChatState `.needsAuth` | `needsInput` |
| ChatState `.disconnected` | `done` |
| Terminal `AgentStatus` | passed through; exit ≠ 0 → `error` (already mapped by `AgentActivityModel`) |
| ToolCall `in_progress` | `running` |
| ToolCall `completed` | `done` |
| ToolCall `failed` | `error` |

Chat subagents: a `ToolCall` whose kind/name identifies an agent spawn (for Claude Code ACP: the `Task` tool — exact names emitted by the adapter to be verified during planning). Title = task description when present, fallback `"Subagent"`.

Terminal subagents: recursive descendants of the pane shell whose comm name matches `AgentCatalog`, plus descendants of an already-matched agent. Node/Bun-hosted CLIs (pi, omp) are invisible here — known limitation, same as Layer D.

### 2. Data collection (App layer)

**`AgentsPanelModel`** (`App/RightPanel/AgentsPanelModel.swift`, `@MainActor @Observable`):

- Receives the selected worktree; reads chat tabs (with their `ChatController`s) and terminal panes from `AppModel`.
- Observes `AgentActivityModel` for terminal statuses.
- Receives the process tree from `ForegroundProcessAgent`.
- Rebuilds `[AgentNode]` via `AgentTreeBuilder` on change; exposes `nodes`.
- Holds `expandedIds: Set<String>` for disclosure state.

**`ForegroundProcessAgent` extension**: today it walks only direct children of the pane shell once per poll. Extend the existing poll to a recursive walk (max depth 5, cap 50 processes — guards against fork bombs/huge trees) and publish a `[paneId: [ProcessNode]]` snapshot. Same timer/loop as today, no new poller. Matching against `AgentCatalog` stays in the builder (TillerCore); the libproc walk stays in App (platform code).

**`ChatController`**: already exposes transcript and `state`. Add a computed `activeSubagentTasks` derived from the reducer — filters Task-type tool calls. No change to the ACP flow.

Flow: sources change → `AgentsPanelModel` recomputes → view updates. No debounce needed (chat events are sparse; the process poll is already throttled).

### 3. UI

**`RightPanelView`**: current content (picker + Files/Diff/Status) becomes the top branch of a `VSplitView`; `AgentsSectionView` below. Initial 2/3–1/3, divider position persisted as a fraction in `@AppStorage`. The Agents section is always visible — worktrees without agents show an empty state.

**`AgentsSectionView`** (`App/RightPanel/AgentsSectionView.swift`):

- Small header: "Agents" + running count (e.g. "2 running").
- List with `OutlineGroup`/`DisclosureGroup` over `nodes` — same visual pattern as the left sidebar (indentation, chevron).
- Row: agent icon (reuse `AgentIcon`) + name/title + status dot on the right (pulsing green = running, reuse `RunningDots` if suitable; yellow = needsInput; grey = idle; check/× = done/error).
- Click on top-level node → `AppModel` selects the corresponding tab/pane (chat → chat tab, terminal → pane focus). Subagent click selects the parent.
- Empty state: `ContentUnavailableView` "No agents running".

All UI strings in English.

## Error handling / edge cases

- libproc walk failure or process vanishing mid-walk → partial snapshot, no UI error (same as Layer D today).
- Pane/tab closed → node disappears.
- Malformed/untitled Task tool call → fallback title "Subagent".
- Depth 5 / 50-process caps → excess silently truncated.

## Testing (swift-testing, tests first)

- `AgentTreeBuilderTests` (TillerCore): chat/terminal status mapping, subagent tree from process snapshots, chat subagents from tool calls, ordering, stable IDs, depth/count caps.
- `ChatController`: `activeSubagentTasks` derivation from a fixture transcript with Task tool calls.
- UI not auto-tested (repo convention); manual smoke checklist in the implementation plan.
- Gate: `Scripts/ci.sh` → "CI OK".
