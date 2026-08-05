# Stable Changes Diff Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve an expanded file’s last successful diff while `DiffLoadStore` refreshes it in the background, replacing it atomically only when the new load succeeds.

**Architecture:** Keep the change surgical inside `DiffLoadStore`. `reloadExpanded` invokes the existing private loader in a preserving mode; the loader captures the exact state it is responsible for and accepts a completion only if that state is still current. Initial loads and retries continue using the existing loading/failure path, while `RightPanelModel`, `ChangesListView`, and refresh scheduling remain unchanged.

**Tech Stack:** Swift 6, SwiftUI `@MainActor`/Observation, `swift-testing` (`@Suite`, `@Test`, `#expect`), `xcodebuild`, and the repository’s existing Tiller/TillerGit types.

## Global Constraints

- Modify only `App/RightPanel/DiffLoadStore.swift` and `AppTests/ChangesListTests.swift`.
- Keep `DiffLoadState` and the public `DiffLoadStore` API unchanged.
- Implement stale-while-revalidate solely in `DiffLoadStore`.
- Do not modify `RightPanelModel`, `ChangesListView`, `App/TillerApp.swift`, polling, or debounce behavior.
- Preserve per-path single-flight loading, actor isolation, collapse behavior, pruning behavior, reset behavior, and `defer { inFlight.remove(...) }`.
- Do not introduce detached tasks.
- Keep `retry` outside the diff; normal initial-load and retry behavior remains covered by existing tests.
- Use deterministic test coordination; do not use sleeps or timing assumptions.
- The final implementation commit, if execution is requested, contains only the two source/test files and uses `fix: preserve loaded diffs during refresh`.

---

## File map

- **Modify:** `App/RightPanel/DiffLoadStore.swift`
  - Keep `DiffLoadState`, the public API, actor isolation, and lifecycle methods intact.
  - Change only `reloadExpanded` and the private `load` flow needed to preserve a loaded state during background refresh.
- **Test:** `AppTests/ChangesListTests.swift`
  - Make `ChangesListTestFailure` conform to `Error, Sendable` if required by the deterministic loader.
  - Add a controllable actor helper and the two regression tests.
  - Retain the existing initial-load, initial-failure/retry, single-flight, collapse, and pruning tests.

No files are created, deleted, or modified outside this map.

## Task 1: Add and implement stale-while-revalidate for expanded diffs

**Files:**
- Modify: `App/RightPanel/DiffLoadStore.swift` (`reloadExpanded`, private `load`)
- Test: `AppTests/ChangesListTests.swift` (helper, `successfulReloadReplacesLoadedDiffAtomically`, `failedReloadPreservesPreviouslyLoadedDiff`)

**Interfaces:**
- Existing `DiffLoadStore.Loader` remains `@Sendable (GitStatusEntry, String) async throws -> GitFileDiff`.
- Existing `DiffLoadStore` public methods and `DiffLoadState` cases remain unchanged.
- The private loader gains only the defaulted parameter `preservingLoaded: Bool = false`.
- The deterministic test actor returns the scripted `Result<GitFileDiff, ChangesListTestFailure>` for each call and blocks only call 2.

### TDD: write the regression tests first

- [ ] **Step 1: Update the test failure type for the actor boundary**

In `AppTests/ChangesListTests.swift`, keep the existing case and make its conformance explicit:

```swift
enum ChangesListTestFailure: Error, Sendable { case boom }
```

Do not change existing failure/retry assertions.

- [ ] **Step 2: Add a deterministic two-call loader actor**

Add this actor near the existing test actors. It stores the complete scripted result list, returns call 1 immediately, blocks only call 2, and coordinates `waitUntilBlocked(2)`/`release(2)` without sleeps:

```swift
actor ControlledDiffLoader {
    private let results: [Result<GitFileDiff, ChangesListTestFailure>]
    private var callCount = 0
    private var blockedCalls: Set<Int> = []
    private var releasedCalls: Set<Int> = []
    private var blockedWaiters: [Int: [CheckedContinuation<Void, Never>]] = [:]
    private var releaseWaiters: [Int: [CheckedContinuation<Void, Never>]] = [:]

    init(results: [Result<GitFileDiff, ChangesListTestFailure>]) {
        self.results = results
    }

    func load(_ entry: GitStatusEntry, repoPath: String) async throws -> GitFileDiff {
        callCount += 1
        let call = callCount
        let result = results[call - 1]

        guard call == 2 else { return try result.get() }

        blockedCalls.insert(call)
        let waiters = blockedWaiters.removeValue(forKey: call) ?? []
        waiters.forEach { $0.resume() }

        if !releasedCalls.contains(call) {
            await withCheckedContinuation { continuation in
                releaseWaiters[call, default: []].append(continuation)
            }
        }

        blockedCalls.remove(call)
        return try result.get()
    }

    func waitUntilBlocked(_ call: Int) async {
        guard !blockedCalls.contains(call) else { return }
        await withCheckedContinuation { continuation in
            blockedWaiters[call, default: []].append(continuation)
        }
    }

    func release(_ call: Int) {
        releasedCalls.insert(call)
        let waiters = releaseWaiters.removeValue(forKey: call) ?? []
        waiters.forEach { $0.resume() }
    }
}
```

The actor’s `releasedCalls` set handles a release that races before the loader installs its continuation; the waiter dictionaries handle the opposite ordering. The test inputs contain exactly two results, so the second call is the only blocked reload.

- [ ] **Step 3: Add the successful atomic replacement test**

Add this test to `ChangesListTests`, constructing both diffs inline rather than introducing a `makeDiff` helper:

```swift
@Test func successfulReloadReplacesLoadedDiffAtomically() async throws {
    let entry = try makeEntry("a.swift")
    let oldDiff = GitFileDiff(
        path: entry.path, lines: [], additions: 1, deletions: 0,
        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
    let newDiff = GitFileDiff(
        path: entry.path, lines: [], additions: 7, deletions: 2,
        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
    let scripted = ControlledDiffLoader(results: [.success(oldDiff), .success(newDiff)])
    let store = DiffLoadStore(loader: { entry, repoPath in
        try await scripted.load(entry, repoPath: repoPath)
    })

    await store.expand(entry, repoPath: "/tmp")
    #expect(store.state(for: entry.path) == .loaded(oldDiff))

    let refresh = Task { @MainActor in
        await store.reloadExpanded(entries: [entry], repoPath: "/tmp")
    }
    await scripted.waitUntilBlocked(2)

    #expect(store.state(for: entry.path) == .loaded(oldDiff))

    await scripted.release(2)
    await refresh.value

    #expect(store.state(for: entry.path) == .loaded(newDiff))
}
```

- [ ] **Step 4: Add the failed background reload preservation test**

Add the failure variant with the same blocked second call. The initial diff must remain loaded both while the reload is blocked and after the reload fails:

```swift
@Test func failedReloadPreservesPreviouslyLoadedDiff() async throws {
    let entry = try makeEntry("a.swift")
    let oldDiff = GitFileDiff(
        path: entry.path, lines: [], additions: 1, deletions: 0,
        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
    let scripted = ControlledDiffLoader(
        results: [.success(oldDiff), .failure(.boom)])
    let store = DiffLoadStore(loader: { entry, repoPath in
        try await scripted.load(entry, repoPath: repoPath)
    })

    await store.expand(entry, repoPath: "/tmp")
    #expect(store.state(for: entry.path) == .loaded(oldDiff))

    let refresh = Task { @MainActor in
        await store.reloadExpanded(entries: [entry], repoPath: "/tmp")
    }
    await scripted.waitUntilBlocked(2)

    #expect(store.state(for: entry.path) == .loaded(oldDiff))

    await scripted.release(2)
    await refresh.value

    #expect(store.state(for: entry.path) == .loaded(oldDiff))
}
```

- [ ] **Step 5: Run the narrow tests and verify the expected RED state**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -clonedSourcePackagesDirPath DerivedData/SourcePackages -only-testing:TillerTests/ChangesListTests
```

Expected RED before the production change:

- In `successfulReloadReplacesLoadedDiffAtomically`, the blocked second call observes `.loading` instead of `.loaded(oldDiff)`; after release, the final replacement assertion cannot pass.
- In `failedReloadPreservesPreviouslyLoadedDiff`, the blocked second call observes `.loading`, and after release the state becomes `.failed(...)` instead of remaining `.loaded(oldDiff)`.

If the helper or tests fail to compile, correct only their local type/conformance/signature issues before proceeding; do not weaken the assertions or add timing waits.

### Minimal production change

- [ ] **Step 6: Change `reloadExpanded` to preserve loaded state**

In `App/RightPanel/DiffLoadStore.swift`, remove the explicit `.idle` assignment and pass the preserving flag to the existing private loader:

```swift
func reloadExpanded(entries: [GitStatusEntry], repoPath: String) async {
    for entry in entries where expanded.contains(entry.path) {
        await load(entry, repoPath: repoPath, preservingLoaded: true)
    }
}
```

This leaves collapsed paths untouched and preserves the existing expanded-path filtering.

- [ ] **Step 7: Add the expected-state guard to the private loader**

Change only the private signature and state-transition portion of `load`. Keep the in-flight guard, `defer`, actor isolation, loader call, and all public methods intact:

```swift
private func load(
    _ entry: GitStatusEntry,
    repoPath: String,
    preservingLoaded: Bool = false
) async {
    guard inFlight.insert(entry.path).inserted else { return }
    defer { inFlight.remove(entry.path) }

    let expectedState: DiffLoadState
    if preservingLoaded, case let .loaded(previousDiff) = states[entry.path] {
        expectedState = .loaded(previousDiff)
    } else {
        states[entry.path] = .loading
        expectedState = .loading
    }

    do {
        let diff = try await loader(entry, repoPath)
        guard states[entry.path] == expectedState else { return }
        states[entry.path] = .loaded(diff)
    } catch {
        guard states[entry.path] == expectedState else { return }
        if case .loaded = expectedState {
            states[entry.path] = expectedState
        } else {
            states[entry.path] = .failed(error.localizedDescription)
        }
    }
}
```

`expectedState` is non-optional and captures the exact existing `.loaded(previousDiff)` only for a preserving reload. All other loads publish and capture `.loading` as before. The equality guard prevents a completion from applying after `prune` or `reset` has removed/replaced the path state; it also prevents any other state transition from accepting a stale result. A successful preserving reload assigns `.loaded(diff)` in one state update. A preserving failure republishes the same loaded value, while an initial load failure retains `.failed(error.localizedDescription)` semantics.

- [ ] **Step 8: Rerun the narrow Changes list tests**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates -clonedSourcePackagesDirPath DerivedData/SourcePackages -only-testing:TillerTests/ChangesListTests
```

Expected: PASS, including both new regression tests and the existing tests for initial loading/success, initial failure/retry, single-flight, collapse, expanded-only refresh, and pruning. The existing tests must remain in place; do not replace them with the new regression coverage.

- [ ] **Step 9: Run the full repository verification gate**

Run:

```bash
Scripts/ci.sh
```

Expected: the command completes successfully and prints `CI OK`.

- [ ] **Step 10: Inspect the isolated diff**

Review the diff and verify that it contains only:

- `App/RightPanel/DiffLoadStore.swift`: the `reloadExpanded` call and private `load` state handling.
- `AppTests/ChangesListTests.swift`: the `Sendable` failure conformance, deterministic actor helper, and the two named regression tests.

Confirm that `RightPanelModel`, `ChangesListView`, `App/TillerApp.swift`, polling/debounce, `retry`, `DiffLoadState`, and public `DiffLoadStore` methods are unchanged.

- [ ] **Step 11: Commit only the implementation files**

If execution is requested after review, stage exactly the two implementation files and commit:

```bash
git add App/RightPanel/DiffLoadStore.swift AppTests/ChangesListTests.swift
git commit -m "fix: preserve loaded diffs during refresh"
```

Do not stage the design spec or this plan.

## Plan self-review

- **Spec coverage:** Covered stale-while-revalidate ownership in `DiffLoadStore`, loaded-state retention during reload and failure, atomic success replacement, unchanged initial loading/failure, single-flight, collapse/pruning/reset protection, and unchanged polling/UI/model scope.
- **Placeholder scan:** All task steps include concrete paths, commands, expected results, or compile-oriented Swift snippets; no incomplete implementation markers are used.
- **Type/signature consistency:** The snippets use the existing `DiffLoadState`, `GitFileDiff`, `GitStatusEntry`, `GitPath`, `DiffLoadStore.Loader`, `@MainActor` test suite, and the private `load(..., preservingLoaded: Bool = false)` signature consistently.
- **Scope check:** One reviewable TDD task modifies only the two requested files; no exploratory `retry` change or unrelated refactor is included.
