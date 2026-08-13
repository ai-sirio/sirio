# P37 Git Mutation Doors and Conflict Refusal

## Goal

Make the existing Changes mutation tier observable through the control socket and `tillerctl`, while refusing to stage conflicted paths. Keep the UI and automation on the same `tiller_git` mutation functions, and leave `tiller_ui/src/changes.rs` to pi.

## Scope and ownership

- Owned for this piece: `rust/crates/tiller_git/**`, `rust/crates/tiller_control/**`, and `rust/crates/tiller/src/main.rs` plus the app crate dependency declaration.
- Explicitly out of scope: `rust/crates/tiller_ui/src/changes.rs`. Pi owns the rendering of Git errors and the Retry affordance.
- The UI already invokes the public `tiller_git::{stage, unstage, discard, stage_all, discard_all}` functions. Socket mutations will invoke those same functions, so the conflict guard and Git behavior cannot diverge between the button and automation.

## Git-layer behavior

Add a typed `GitError::ConflictedPaths { paths: Vec<PathBuf> }` variant. Its display text must say that staging was refused and list every conflicted path, making the error actionable for both a single-path request and a batch request.

Before `git add -A`:

- `stage(repo, path)` runs the existing status query and refuses if the requested entry is conflicted.
- `stage_all(repo)` runs the existing status query and refuses before mutation if any entry is conflicted. The error contains all conflicted paths in stable status/path order.
- If the status query itself fails, return that original `GitError` unchanged; callers can distinguish a broken Git/repository invocation from an empty or clean snapshot.
- If no conflict is present, preserve the current literal-pathspec behavior and Git command shape.

The batch refusal is deliberately preflight-only: a checkout with three conflicts receives one error naming all three and no path is staged. No conflict guard is added to unstage or discard; P37 only closes the unsafe staging door.

## Control-socket API

Add these methods to `system.capabilities`:

```text
surface.changes.stage
surface.changes.unstage
surface.changes.discard
surface.changes.stage_all
surface.changes.discard_all
```

Each method accepts an optional `worktree` selector, resolved by the existing mounted-worktree lookup. The single-path methods require a repo-relative `path`; the all-path methods take no path. An unknown or unmounted worktree returns a control error without invoking Git. A Git-layer error becomes the control response's actionable error string.

The app-side handler resolves the repository path and calls the public `tiller_git` function directly on the control worker. A mounted Changes surface observes the mutation through its existing refresh loop; no second UI mutation state machine is introduced.

## CLI API

Extend the existing `surface changes` command family:

```text
tillerctl surface changes stage <path> [--worktree <selector>]
tillerctl surface changes unstage <path> [--worktree <selector>]
tillerctl surface changes discard <path> [--worktree <selector>]
tillerctl surface changes stage-all [--worktree <selector>]
tillerctl surface changes discard-all [--worktree <selector>]
```

The request builders use the wire names above and preserve the existing `--socket`, response, and error conventions. CLI errors must retain the path list from `GitError::ConflictedPaths`.

## Error handoff to pi

The git layer will preserve distinctions that `ChangesTab` can render:

| Git result | Meaning available to the surface |
| --- | --- |
| `Ok(StatusSnapshot)` with zero entries | A genuinely clean repository, or an unborn/empty repository with no untracked files; it is not a Git failure. |
| `GitError::Spawn` | Git could not be started, normally a missing binary or permission problem. |
| `GitError::CommandFailed` | Git ran and rejected the repository/command, including a missing or invalid `.git` directory. The stderr remains available in the display text. |
| `GitError::TimedOut` | Git did not finish within its bound and was killed. |
| `GitError::InvalidOutput` | Git succeeded but the parser rejected its output. |
| `GitError::ConflictedPaths` | A requested staging mutation was refused before it changed the index. |

`load_snapshot` currently collapses every `Err` into an empty snapshot. Pi should instead retain the error as `ChangesReport.error` (or the equivalent surface state), keep the distinction between `loading`, clean, and failed, and show Retry only for a failed load. P37 does not edit that code. If pi finds that `ChangesReport` cannot carry the distinction, the required seam change is to preserve `Result<GitSnapshot, GitError>` (or a value containing the typed error) until `ChangesTab::report`; no Git command or parser change is required on pi's side.

## Verification

1. Add real-repository tests proving a conflicted single path is refused and remains conflicted.
2. Add a real-repository batch test with multiple conflicts proving `stage_all` names every path and leaves all paths unchanged.
3. Retain and rerun normal stage, unstage, and discard tests; verify the postcondition with real `git status --porcelain` rather than only the crate's snapshot.
4. Add control protocol, capability, and `tillerctl` mapping tests for all five methods.
5. Run a headless fixture through the live socket: conflict `stage` refusal, normal `stage`, `unstage`, `discard`, `stage_all`, and `discard_all`, checking Git's status after each operation.
6. Run `./Scripts/ci-linux.sh` and require its `CI OK` marker.
