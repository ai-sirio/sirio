# Changes diff refresh — design

## Context and symptom

The Changes list shows an expanded file’s diff while its contents are available. When
Tiller refreshes Git status, an already-visible diff briefly disappears and the row
shows the loading state. A refresh therefore interrupts review even when the previous
diff is still valid enough to display.

Relevant paths and symbols:

- `App/RightPanel/RightPanelModel.swift` — `performRefresh`
- `App/RightPanel/DiffLoadStore.swift` — `reloadExpanded` and `DiffLoadState`
- `App/RightPanel/ChangesListView.swift` — diff-state rendering

## Root cause

`RightPanelModel.performRefresh` always calls `DiffLoadStore.reloadExpanded`. That
reload transitions each expanded path from `.loaded` to `.idle`/`.loading` before the
new result is available. `ChangesListView` renders the loading state without the
previous diff, so every refresh replaces visible content with a loading view.

## Approved behavior

Use stale-while-revalidate for expanded diffs:

- If a path already has a loaded diff, keep that diff rendered while the background
  reload is in flight.
- Replace the visible diff atomically only after the reload succeeds.
- If the background reload fails, preserve the previous loaded diff. The failure must
  not discard the last successful result or turn the row into an error state.
- A path with no loaded diff keeps the existing initial-load behavior: show loading,
  then either the loaded diff or the existing error state.
- Existing explicit retry behavior remains an initial/failed-load concern and does
  not change the stale-while-revalidate rule.

## Data flow and concurrency

`RightPanelModel.performRefresh` remains unchanged: it continues to refresh Git status
and request reloads for expanded paths. The stale-while-revalidate behavior belongs
solely in `DiffLoadStore`, which owns the per-path state and publishes a completed
replacement as one state update. `ChangesListView` remains unchanged and continues
to render the state for each expanded path independently.

There is at most one in-flight load per `GitPath`. A refresh must not start a second
load for a path already being revalidated. Pruning paths that disappear from status
and the existing collapse behavior remain unchanged: collapsed paths do not reload,
and pruning removes their stale state as it does today.

## Scope and non-goals

Included:

- Preserve loaded diff content during background refresh.
- Atomically swap in a successful replacement.
- Preserve loaded content after a background error.
- Keep initial loading and initial error semantics unchanged.

Not included:

- Changes to polling, filesystem-event debounce, or refresh frequency.
- Changes to `RightPanelModel` or `ChangesListView`.
- A new loading, stale, or failure indicator in the UI.
- Unrelated changes to Git diff parsing, status loading, or panel structure.

## Test strategy

Use `swift-testing` (`@Test` / `#expect`) for the existing Changes/diff state tests.
Cover the state machine at the `DiffLoadStore`/Changes-list boundary:

- A blocked reload keeps a previously loaded path in the loaded/renderable state.
- A successful reload replaces the old diff with the new diff.
- A failed background reload preserves the previous diff.
- Initial loading still transitions to the existing loaded state on success.
- Initial loading failure still produces the existing error semantics.

Tests should use controllable load completion rather than timing-based sleeps.

## Acceptance criteria

- Refreshing Git status never replaces an already-loaded expanded diff with the
  loading view while its replacement is being fetched.
- A successful background reload becomes visible as one atomic replacement.
- A failed background reload leaves the last successful diff visible.
- Initial load success and failure behave exactly as before.
- Each path has at most one in-flight diff load.
- Existing pruning and collapse behavior is unchanged.
- No polling, UI indicator, or unrelated refactor is introduced.
