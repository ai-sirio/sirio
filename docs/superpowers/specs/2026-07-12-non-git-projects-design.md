# Non-Git Projects — Design

**Date:** 2026-07-12
**Status:** Approved

## Problem

Tiller assumes every added folder is a git repository. `AppModel.addProject` calls
`GitWorktrees.list` and, when it fails, silently inserts a fake main-worktree row with
branch `"main"`. A plain (or empty) folder therefore enters the app disguised as a repo:
the sidebar offers "New Worktree…", branch labels, and worktree settings that can only
fail.

## Goal

Support adding empty folders and non-git folders as first-class projects:

- When adding a non-git folder, ask the user whether to initialize a git repository.
- If they decline, add the folder as a plain project with all git features hidden.
- Allow later conversion to git, both in-app and externally.

## Decisions

1. **Git detection is derived at runtime, never persisted.** A pure check
   (`.git` entry exists at the project root) runs at bootstrap, on add, and after
   in-app git init. No DB schema change. External `git init` from a terminal is
   picked up automatically on the next bootstrap/refresh.
2. **Prompt at add time, plus an in-app conversion action.** The add flow asks
   "initialize git?"; a project context-menu item ("Inizializza repository git",
   shown only for non-git projects) covers later conversion for GUI-first users.
3. **Empty folders get no special treatment.** An empty non-git folder follows the
   same prompt flow; an empty git repo (zero commits) enters as a normal git project
   with the default branch.

## Architecture

### Detection — `TillerGit`

New pure function:

```swift
public enum GitRepoDetection {
    /// True if `path` contains a `.git` entry (directory = normal repo,
    /// file = linked worktree/submodule — both count as git).
    public static func isGitRepository(path: String) -> Bool
}
```

`FileManager`-only, no shell-out.

### State — `AppModel` (App target)

- `gitProjectIds: Set<UUID>` — recomputed at bootstrap, in `addProject`, and after
  the init-git action.
- `func isGitProject(_ project: Project) -> Bool` — read by views and control handlers.
- The main-worktree DB row keeps its current shape (branch fallback `"main"`); the UI
  simply ignores the branch for non-git projects.

### Add flow — `AddProjectSheet`

"Browse folder" → if the chosen folder is not a git repo, show a three-way dialog:

| Choice | Effect |
|---|---|
| Inizializza git | `git init` in the folder, then normal `addProject` |
| Aggiungi senza git | `addProject` in non-git mode (skips `GitWorktrees.list`) |
| Annulla | Nothing happens |

If `git init` fails: alert with the error, project not added.

### In-app conversion — project context menu

Shown only when `isGitProject(project) == false`:

"Inizializza repository git" →

1. `GitRunner.run(["init"], in: project.rootPath)`
2. Re-read the actual branch via `GitWorktrees.list` and update the main worktree
   row in the DB (respects a non-`main` `init.defaultBranch`).
3. Recompute `gitProjectIds`.

On failure: alert with the error, state unchanged.

### Hidden UI for non-git projects

| Feature | Non-git project |
|---|---|
| "New Worktree…" sidebar row | Hidden |
| Branch label on the worktree row | Hidden (folder name only) |
| Project settings: worktree base / worktree location | Hidden |
| Project context menu | Adds "Inizializza repository git" |
| Terminals, tabs, agents, markdown editor | Work normally |

### CLI — control socket

`workspace.create` (and the ergonomic aliases that create worktrees) on a non-git
project returns `failure: "project is not a git repository"`.

## Error handling

- `git init` failure → user-visible alert; no partial state (add flow) or unchanged
  state (context menu).
- Detection never throws: missing folder ⇒ `false`.

## Testing

- **TillerGit:** `isGitRepository` against temp dirs — empty dir, dir with `.git`
  directory (after `git init`), dir with `.git` file. `swift-testing`.
- **CLI gate:** `workspace.create` on a non-git project returns the failure message.
- **Gate:** `Scripts/ci.sh` prints `CI OK`.

## Out of scope

- Multi-repo folders ("folder with many repos" in the browse subtitle).
- Persisting an `isGit` flag in the DB.
- Any git feature beyond hiding/gating the existing ones.
