# Activity panel — design

Date: 2026-08-06
Status: approved

## Problem

The right panel's bottom section, "Agents", lists only tabs where an agent was
detected. `AgentTreeBuilder.build` drops every tab that lacks both an entry in
`agentStatus` and one in `paneAgents`, so a terminal running a plain shell is
invisible, and the section answers "which agents are running here?" rather than
"what do I have open?".

It is also scoped to the selected worktree, offers no way to close anything, and
occupies a fixed 25% of the panel height that cannot be reclaimed.

## Goal

Replace it with **Activity**: a flat list of every terminal and chat tab across
all open worktrees, each row closable, the whole section collapsible.

## Decisions

| Question | Decision |
|---|---|
| What counts as a process | Every terminal and chat tab, running agent or not |
| Scope | All worktrees in `openWorktreeIds`, not just the selected one |
| Structure | Flat list, ordered by worktree, worktree label on each row |
| Subagent rows | Removed — no children of any kind |
| Close semantics | Immediate when idle/done; confirmation alert when running/needs-input/error |
| Toggle placement | Disclosure chevron in the Activity header |

Rejected: undo toast after close. Closing a terminal tab terminates the PTY, so
the process cannot be restored — the most that is achievable is reopening a fresh
tab in the same worktree, which is a "reopen", not an undo. Combined with the
absence of any generic toast infrastructure (`UpdateToastView` is wired to
Sparkle) and of a `reopenTab` path, the cost was out of proportion to a
confirmation dialog that prevents the same mistake.

Rejected: a titlebar button for Activity. The right panel already has its own
toggle (⌃⌘I); a second chrome button for a subsection of a panel that may itself
be hidden would need to force the panel open to do anything visible. If the
global scope proves awkward in use, it can be promoted later with real usage
evidence in hand.

## Architecture

### TillerCore — `ActivityList.swift`

Replaces `AgentTree.swift`. `AgentTreeBuilder` has exactly one consumer
(`AgentsPanelModel.swift:32`) plus its own tests, so it is rewritten rather than
kept alongside. The subagent machinery (`subagentNodes`, `firstMatch`,
`collectMatches`) is deleted with it, as is `ChatSubagentInput`, whose only
consumer is that same file.

`ProcessNode` stays. It is shared with the Layer-D process detection
(`ForegroundProcessAgent`, `ProcessScanCoordinator`, `AppModel.paneProcessTrees`)
and merely happens to live in `AgentTree.swift`; it moves to its own
`ProcessNode.swift` rather than being deleted with its former neighbours.

```swift
public enum ActivityStatus: Sendable {
    case running, needsInput, done, error, idle

    public static func from(_ status: AgentStatus?) -> ActivityStatus
}

public struct ActivityRow: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable { case terminal, chat }

    public let id: String            // "\(worktreeId):\(tabId)"
    public let worktreeId: UUID
    public let tabId: UUID
    public let kind: Kind
    public let title: String
    public let agentId: String?      // nil when no agent was identified
    public let worktreeLabel: String // "project/branch"
    public let status: ActivityStatus
}

public enum ActivityListBuilder {
    public struct WorktreeInput: Sendable {
        public let worktreeId: UUID
        public let label: String
        public let tabs: [WorkspaceTab]
        public let livePaneIds: [WorkspaceTabID: UUID]
    }

    public static func build(
        worktrees: [WorktreeInput],
        agentStatus: [UUID: AgentStatus],
        paneAgents: [UUID: String]
    ) -> [ActivityRow]
}
```

`ActivityStatus` exists because `AgentStatus` has no case for "nothing is
running": a bare shell has no status at all. The mapping is total, with
`nil → .idle`.

Per tab:

- `.chat` — status and agent keyed by `tab.id.rawValue`; title is `tab.title`,
  which auto-rename keeps current.
- `.terminal` — status and agent keyed by the live pane id; a tab with no live
  pane is skipped. Title is `tab.title`.
- `.document` — excluded. Documents are not processes.

A missing `agentStatus` or `paneAgents` entry is not a reason to drop a row: the
row is emitted with `agentId == nil` and `status == .idle`. This removed guard is
the whole behavioural change.

Ordering is the caller's `worktrees` order, then tab order within each worktree.

### App — `ActivityPanelModel.swift`

Replaces `AgentsPanelModel.swift`. Iterates `appModel.openWorktreeIds` — only
mounted worktrees have live PTYs — resolves each through `appModel.worktree(byId:)`
and `workspaceCoordinator.layouts[id]`, builds the label from
`Project.displayName ?? Project.name` plus `Worktree.branch`, and collects live
pane ids via `workspaceCoordinator.liveControlPaneId(contentID:in:)`.

A worktree id with no layout, or a terminal tab with no live pane, is skipped
rather than treated as an error. This is what makes the transient state right
after closing a worktree harmless.

The ordering is `openWorktreeIds` order rather than selected-worktree-first, so
the list does not reorder itself under the pointer when the selection changes.

### App — `ActivitySectionView.swift`

Replaces `AgentsSectionView.swift`.

**Header.** Whole header is a button: chevron (`chevron.down` / `chevron.right`),
"Activity", and a trailing running count (`3 running`), hidden at zero. Expansion
state persists in `@AppStorage` under a new `AppSettings.activitySectionExpandedKey`,
default `true`.

**Row.** Icon (`AgentIcon(agentId:)` when an agent is known, otherwise `terminal`
or `bubble.left.and.text.bubble.right` by kind), title, worktree label as
secondary text, status indicator, close button.

Status indicators reuse the current vocabulary and add one: running is
`RunningDots` green, needs-input a yellow circle, done a secondary checkmark,
error a red `xmark.circle.fill`, idle a grey hollow circle.

The close button is always visible, not hover-only. Across a global list the
pointer does not pass over every row, and a hidden control on a "what is running"
list is a scavenger hunt.

**Interactions.** Tapping the row body focuses the owning tab through
`appModel.focusTab(tabId:in:)`. The close button routes by status:

- `.idle`, `.done` → `appModel.closeTab(tabId, in: worktree)` immediately
- `.running`, `.needsInput`, `.error` → confirmation alert, Close (destructive) /
  Cancel, following the `pendingDiscard` pattern already in `RightPanelView`

The branch itself lives in TillerCore as
`ActivityStatus.requiresCloseConfirmation`, so the rule is unit-testable without
driving SwiftUI alert state.

Close always goes through `appModel.closeTab`, the same path the rest of the app
uses; the coordinator owns content teardown and PTY termination. No new
close logic is introduced.

**Empty state.** `ContentUnavailableView("No activity", systemImage: "bolt.slash")`
with "Terminals and chats appear here as you open them."

### App — `RightPanelView.swift`

`RightPanelView.swift:29` currently pins the tools region to
`geo.size.height * 0.75`. It becomes conditional on the expansion flag: expanded
keeps 75/25, collapsed gives the tools region `maxHeight: .infinity` and leaves
only the Activity header below it, animated with the same
`.easeInOut(duration: 0.2)` used for the panel columns.

The placeholder shown when no worktree is selected ("No agents running") is
replaced by the Activity empty state, and the two Italian strings at
`RightPanelView.swift:79-80` ("Nascondi pannello destro") are translated, since
project convention is English for all app-facing strings and the file is being
edited anyway.

## Testing

`ActivityListBuilderTests` (TillerCore), rewriting `AgentTreeBuilderTests`:

- a terminal tab with no agent identity **is included** — this is the assertion
  that fails today and captures the whole behaviour change
- a chat tab is included, with its status and agent id
- a `.document` tab is excluded
- two worktrees produce rows in the caller's worktree order, each carrying its
  own label
- a terminal tab with no live pane is skipped
- `ActivityStatus.from`: all five cases, including `nil → .idle`
- `ActivityStatus.requiresCloseConfirmation`: true for running, needs-input and
  error; false for idle and done
- rows are flat: the type has no children to assert on, which is the point

AppTests:

- `ActivityPanelModel` returns rows from two open worktrees, proving the model
  reads `openWorktreeIds` rather than the selected worktree
- closing an `.idle` row removes the tab from the layout

## Migration notes

- File renames: `AgentTree.swift` → `ActivityList.swift`,
  `AgentsPanelModel.swift` → `ActivityPanelModel.swift`,
  `AgentsSectionView.swift` → `ActivitySectionView.swift`. xcodegen globs
  directories, so `xcodegen generate` after the rename is enough — no
  `project.yml` edit.
- New key in `AppSettings`: `activitySectionExpandedKey`, default `true`.
- `Scripts/ci.sh` must print `CI OK` before the work is considered done.

## Out of scope

Titlebar count badge, a keyboard shortcut for Activity, row reordering, and any
row action beyond focus and close.
