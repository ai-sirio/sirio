# Slash-Command Highlight + Git File Colors — Design

Date: 2026-07-20
Status: approved

## Objective

Two independent UX improvements, one spec (small scope each, shared review cycle):

1. **Chat slash commands**: the `/command` token in the composer is rendered in the accent color while typed, and the existing suggestion popup becomes keyboard-navigable (↑/↓ to move, Tab or Enter to accept, Escape to dismiss).
2. **Files sidebar git colors**: file names are tinted by their git status (VS Code style) and parent folders show an aggregated status color + dot, so collapsed folders no longer hide changes.

## Current state

- `App/Chat/ChatComposerView.swift` already shows a `slashPopup` filtering `controller.availableCommands` (fed by ACP `availableCommandsUpdate`), but selection is mouse-only and the typed token is plain text.
- `App/Chat/ChatTextEditor.swift` wraps an `NSTextView`; its `Coordinator.textView(_:doCommandBy:)` already intercepts `insertNewline:` for send — the canonical AppKit hook for stealing keys.
- `App/RightPanel/FileExplorerView.swift` already shows a colored status letter badge (M/A/D/R/U/?) per file from `panelModel.statusByPath` (`GitStatusEntry` from `TillerGit`), with colors `AppTheme.gitModified/gitStaged/gitUntracked/gitConflict`. File names are uncolored; folders show nothing.

## Feature 1 — Slash-command highlight + keyboard navigation

### `ChatTextEditor.swift`

**Key interception.** New optional closure on the representable:

```swift
enum SlashKey { case up, down, tab, enter, escape }
var onSlashKey: ((SlashKey) -> Bool)?   // true = consumed
```

`doCommandBy` maps selectors → `SlashKey`:

| Selector | SlashKey |
|---|---|
| `moveUp:` | `.up` |
| `moveDown:` | `.down` |
| `insertTab:` | `.tab` |
| `insertNewline:` | `.enter` |
| `cancelOperation:` | `.escape` |

If `onSlashKey` returns `true`, the key is consumed. Otherwise fall through to current behavior (`insertNewline:` → `onSubmit`, everything else → default NSTextView). When the popup is closed the composer's handler returns `false` for every key, so behavior is unchanged.

**Token coloring.** In `textDidChange`, if the full text matches `^/\S*$` (single slash token, no whitespace — the same condition `slashCandidates` uses), apply `foregroundColor = NSColor.controlAccentColor` to the token range via `textStorage`; otherwise reset the foreground color of the whole storage to the default text color. Reset `typingAttributes` after applying so text typed after a space does not inherit the accent color.

### `ChatComposerView.swift`

- `@State private var slashSelectionIndex = 0` — reset to 0 whenever `slashCandidates` changes identity; clamped to `candidates.count - 1` when the list shrinks.
- `@State private var slashPopupDismissed = false` — set by Escape, reset on any text change. Popup renders only when `!slashCandidates.isEmpty && !slashPopupDismissed`.
- Popup row at `slashSelectionIndex` is highlighted with `AppTheme.selectionFill` (same treatment as the file explorer selection).
- Key handling (only while popup visible):
  - `.up` / `.down` → move `slashSelectionIndex` (clamped, no wrap).
  - `.tab` / `.enter` → accept selected command: `text = "/\(cmd.name) "`. The trailing space invalidates `slashCandidates`, closing the popup. Enter does **not** send while the popup is open.
  - `.escape` → `slashPopupDismissed = true`. With the popup closed, Escape keeps its current meaning (stop turn while running).
- Mouse click on a row keeps working as today.

### Testing

- Selection logic (move/clamp/reset) extracted into a pure helper, unit-tested in AppTests.
- Coordinator behavior tested against a real `NSTextView` in the AppTests host (selector → `SlashKey` mapping, consumed vs. pass-through, coloring applied/reset).

## Feature 2 — Git status colors in the Files sidebar

### `TillerGit` — new `DirectoryGitStatus.swift`

```swift
public enum DirectoryGitStatus: Sendable, Equatable {
    case conflicted
    case changed      // staged, modified, deleted, renamed — collapsed like VS Code
    case untracked
}

public enum DirectoryStatusAggregator {
    /// Walks each changed path's ancestor directories and aggregates
    /// with precedence conflicted > changed > untracked.
    public static func directoryStatuses(
        from statusByPath: [String: GitStatusEntry]
    ) -> [String: DirectoryGitStatus]
}
```

Pure function; O(changed files × path depth). Rename entries contribute both current and original paths' ancestors.

### `RightPanelModel`

New published property `directoryStatusByPath: [String: DirectoryGitStatus]`, recomputed in the same place `statusByPath` is refreshed (cached — never computed per render).

### `FileExplorerView`

- **Files** with a `statusByPath` entry: `Text(node.name)` takes `statusColor(entry)` (the existing helper). Letter badge unchanged.
- **Folders** with a `directoryStatusByPath` entry: name tinted (`conflicted → AppTheme.gitConflict`, `changed → AppTheme.gitModified`, `untracked → AppTheme.gitUntracked`) and a `●` dot shown on the right where files show the letter badge.
- Nodes without entries: unchanged appearance.

### Testing

`TillerGitTests` (swift-testing): precedence ordering, nested paths, root-level files (no ancestors), renames contributing both paths, empty input.

## Out of scope

- Strikethrough / explorer rows for deleted files (deleted files are absent from the tree).
- Gitignored-file dimming.
- Any change to the diff panel or git actions.
- Fuzzy matching in slash candidates (prefix match stays).

## Error handling

- No new failure paths in feature 1 (pure UI state).
- Feature 2 aggregation is pure; on git status load failure `statusByPath` stays empty and the sidebar simply renders uncolored, as today.
