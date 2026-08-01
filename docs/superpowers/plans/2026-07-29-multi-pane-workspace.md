# Draggable Multi-Pane Workspace — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the terminal-only split tree with one worktree-scoped `WorkspaceLayout` of resizable pane groups holding local tabs, where terminal / agent-terminal / chat / Markdown / code tabs coexist, split, and move by pointer, menu, keyboard, accessibility, and control socket without losing live state.

**Architecture:** Pure domain model and mutation engine in `TillerCore`; a new content-neutral `TillerWorkspace` package owns the AppKit renderer, interaction, and accessibility; `TillerTerminal` shrinks to a single terminal surface keyed by `TerminalContentID`; `App` owns `WorkspaceCoordinator` (the only runtime source of truth), content adapters, the persistence mapper, and the control adapter. Exactly one layout engine ships: `TerminalSplitHost` is deleted at cutover.

**Tech Stack:** Swift 6 (strict concurrency), SwiftUI + AppKit (`NSSplitViewController`, `NSHostingController`), swift-testing (`@Test`/`#expect`), GRDB/SQLite via `TillerPersistence`, libghostty via `TillerTerminal`, XcodeGen (`project.yml`).

**Normative inputs:** issues [#2](https://github.com/tillerai/tiller/issues/2), [#3](https://github.com/tillerai/tiller/issues/3#issuecomment-5115217520), [#4](https://github.com/tillerai/tiller/issues/4#issuecomment-5115589274), [#5](https://github.com/tillerai/tiller/issues/5), [#6](https://github.com/tillerai/tiller/issues/6#issuecomment-5116832650), [#7](https://github.com/tillerai/tiller/issues/7#issuecomment-5118371955), [#8](https://github.com/tillerai/tiller/issues/8#issuecomment-5119622147), [#10](https://github.com/tillerai/tiller/issues/10#issuecomment-5117624679). Acceptance index: [#9](https://github.com/tillerai/tiller/issues/9#issuecomment-5120162016).

---

## Global Constraints

- macOS 15+, Swift 6.0, `SWIFT_VERSION: "6.0"`, `MACOSX_DEPLOYMENT_TARGET: "15.0"` (unchanged from `project.yml`).
- Tests are written **before** the production change they cover, using swift-testing (`@Test` / `#expect`), never XCTest.
- Conventional Commits, lower-case imperative subject (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, `perf:`).
- Every task ends with the repository compiling and its own verification command green. Every **phase** ends with `Scripts/ci.sh` printing `CI OK`.
- `Tiller.xcodeproj` is generated: never hand-edit it; edit `project.yml` and run `xcodegen generate`.
- Package dependency direction is one-way and enforced by CI: `TillerWorkspace → TillerCore` only; `TillerTerminal` must not import `TillerWorkspace`; workspace files in `TillerCore` import Foundation only; `App` is the sole composition root.
- Universal preferred usable pane-group size: **240 × 160 points**, including its local tab strip, excluding window/sidebar/inspector chrome.
- Migrated splits start at preferred fraction **0.5**. Preferred fractions are strictly between 0 and 1.
- Acceptance envelope (verification target, **not** a hard product limit): 16 pane groups, 64 tabs, all 4 content kinds, multiple open worktrees with independent gates.
- Optimistic-save retry schedule: **1, 2, 4, 8, 16, 30 seconds**. Keyboard/AX divider debounce: **250 ms**. Quit flush budget: **2 s**, then recovery sidecar.
- Drag threshold **4 points**; contextual edge band **22 %** of the pane body.
- Divider keyboard steps: **5 %**, **1 %** with Option.
- Persisted snapshot: canonical JSON with sorted keys, no compression, SHA-256 over the exact payload bytes, monotonic per-worktree revision.
- No `TBD` may be introduced. Any unresolved detail must be raised against the owning issue, not invented.

**How to read the test lists in this plan.** Each task lists its tests by name. Names with a body are shown in full because they pin a decision that is easy to get wrong (an exact error case, an exact ordering, an exact string). Names shown as `@Test func someBehaviour() { }` are **specifications, not stubs**: the implementer writes the real arrange/act/assert body from the linked acceptance ID and the referenced issue section, and a task is not complete while any body is empty. An empty test body at commit time is a task failure, not a shortcut.

---

## Acceptance ID → module ownership map

Every ID below is used verbatim in test names and commit bodies (`refs OB-S-6`).

### Observable structure and local tabs (#9 → "Structure and local tabs" 1–10)

| ID | Requirement | Owner | Phase |
|----|-------------|-------|-------|
| OB-S-1 | Worktree always exposes one valid layout, incl. empty single-root group | TillerCore | 1 |
| OB-S-2 | Every visible region = one `PaneGroupID` + one local tab stack + ≤1 mounted host | TillerWorkspace | 4 |
| OB-S-3 | Local activate/reorder does not rebuild geometric topology | TillerCore + TillerWorkspace | 2, 4 |
| OB-S-4 | Split Right/Down With always open the universal content menu before mutation | App | 11 |
| OB-S-5 | Fresh choices prepare new identity; files/resumed chats/move resolve existing | App | 10 |
| OB-S-6 | Successful split creates exactly one group + one split at 0.5, focuses content | TillerCore | 2 |
| OB-S-7 | Failed/cancelled preparation creates no tab/group/split/row/focus change | App | 10 |
| OB-S-8 | Closing last tab of non-root group collapses group + parent split atomically | TillerCore | 2 |
| OB-S-9 | Closing last root tab leaves valid empty root group | TillerCore | 2 |
| OB-S-10 | Selections, order, topology, fractions survive switch/remount/quit/restore | App + TillerPersistence | 8, 11 |

### Drag, drop, resize, cancellation (#9 → "#3 Edge Preview contract")

| ID | Requirement | Owner | Phase |
|----|-------------|-------|-------|
| OB-D-1 | 4-point drag threshold, grab offset preserved | TillerWorkspace | 5 |
| OB-D-2 | Only hovered group advertises a target | TillerWorkspace | 5 |
| OB-D-3 | Tab bars expose exact insertion position | TillerWorkspace | 5 |
| OB-D-4 | 22 % edge band previews the exact resulting half-pane | TillerWorkspace | 5 |
| OB-D-5 | Move + collapse + ownership + activation + focus commit as one Core transition | TillerCore | 2 |
| OB-D-6 | Illegal target never previewed as legal | TillerWorkspace | 5 |
| OB-D-7 | Escape / pointer cancel / focus loss / stale identity / illegal release = mutation-free | TillerWorkspace | 5 |
| OB-D-8 | Divider tracking live + AppKit-native; one preferred fraction stored on release | TillerWorkspace | 5 |
| OB-D-9 | Stable-ID reconciliation preserves controller, generation, first responder, state | TillerWorkspace | 4 |
| OB-D-10 | Pointer/menu/keyboard/AX/control converge on one coordinator; no parallel layout | App | 11, 12 |

### Compact window and overflow

| ID | Requirement | Owner | Phase |
|----|-------------|-------|-------|
| CO-1 | 240×160 preferred usable size per group | TillerWorkspace | 5 |
| CO-2 | Split legal only if both results fit; illegal actions disabled with a reason | TillerWorkspace + App | 5, 6 |
| CO-3 | Minimum-size canvas inside a native scroll container instead of slivers | TillerWorkspace | 5 |
| CO-4 | Overflow is presentation-only; never overwrites preferred fractions | TillerWorkspace | 5 |
| CO-5 | Activation / keyboard / VoiceOver focus scrolls minimum distance to reveal | TillerWorkspace | 6 |
| CO-6 | Tab strip scrolls, edge-autoscrolls during drag, keeps active tab visible; overflow menu | TillerWorkspace | 5 |
| CO-7 | Contrast / transparency / motion adaptations valid in overflow mode | TillerWorkspace | 6 |

### Content identity and lifecycle (#6)

| ID | Requirement | Owner | Phase |
|----|-------------|-------|-------|
| LC-1 | Move preserves tab ID, content ID, `ResourceGenerationID`, host, view state | App | 10 |
| LC-2 | One content identity ↔ at most one open tab per worktree | TillerCore + TillerPersistence | 1, 8 |
| LC-3 | Terminal hydrates with mounted worktree; chat/document hydrate lazily | App | 10 |
| LC-4 | Detached/inactive content is not torn down | App | 10 |
| LC-5 | Process exit keeps scrollback + `exited(code)`; relaunch = new generation | App + TillerTerminal | 7, 10 |
| LC-6 | Interrupted chat turns retained, never auto-reissued | App | 10 |
| LC-7 | Dirty/missing/conflicted documents keep buffer + require explicit choice | App | 10 |
| LC-8 | Failed hydration keeps tab + topology with Retry/Close | App | 10 |
| LC-9 | Stale async callbacks cannot mutate retried/replaced/moved/closed generation | App | 10 |
| LC-10 | Explicit close releases each owned runtime exactly once after durable commit | App | 10 |
| LC-11 | Quit checkpoints without close semantics | App | 10 |
| LC-12 | Confirmed container removal purges Tiller-owned artifacts, never source files | App + TillerPersistence | 8, 10 |
| LC-13 | Post-commit cleanup idempotent, restartable, retried on launch | App | 10 |

### Failure handling

| ID | Requirement | Owner | Phase |
|----|-------------|-------|-------|
| FH-B-1..5 | Pre-commit: cancellation, preparation failure, duplicate ownership, stale anchor/revision, disposal failure — no visible change | App | 10 |
| FH-C-1 | SQLite unavailable / constraint / checksum / revision conflict roll back the structural transition | App + TillerPersistence | 8 |
| FH-C-2 | UI stays on last durable revision, reports recoverable error | App | 10 |
| FH-C-3 | UI/menu/keyboard/AX/control get semantically equivalent failure outcomes | App | 11, 12 |
| FH-O-1 | Optimistic save failure marks dirty without reverting; retry 1/2/4/8/16/30 s | App | 8 |
| FH-O-2 | Stale completion never overwrites newer state | App | 8 |
| FH-R-1 | Malformed/hash-mismatch/unknown-version/duplicate/orphan/invalid never becomes live | App + TillerCore | 3, 8 |
| FH-R-2 | Invalid snapshot quarantined + replaced by deterministic salvage layout | App | 8 |
| FH-R-3 | Missing tab rows removed selectively; repair persisted as a new revision | App | 8 |
| FH-R-4 | Unavailable content stays as explicit recoverable placeholder | App | 10 |
| FH-R-5 | Valid newer sidecar offered/applied; invalid sidecar quarantined | App | 8 |
| FH-R-6 | Restore idempotent across relaunch and interruption at every write boundary | App | 9 |
| FH-F-1 | Focus failure after commit does not roll back topology; retried after attach | TillerWorkspace + App | 4, 10 |

### Persistence and migration matrix (#7 → 1–12)

| ID | Fixture | Owner | Phase |
|----|---------|-------|-------|
| PM-1 | New worktree, no legacy records | App | 9 |
| PM-2 | Round trips: empty, single group, deep mixed orientation, full envelope | App | 8 |
| PM-3 | Each structural command across quit/relaunch after durability ack | App | 8 |
| PM-4 | Resize debounce, forced-quit flush, recovery sidecar | App | 8 |
| PM-5 | Revision races from concurrent preparations and stale completions | App | 8 |
| PM-6 | Valid v15 single-pane terminal state | App | 9 |
| PM-7 | v15 nested SplitTree → depth-first leaf UUIDs as `TerminalContentID`, first leaf primary | App | 9 |
| PM-8 | Mixed legacy terminal/non-terminal tabs flattened per #7 | App | 9 |
| PM-9 | Missing scrollback/session/content rows, unavailable agents, missing files, interrupted chat | App | 9 |
| PM-10 | Corrupt legacy/current snapshots, interrupted, repeated, rolled-back migration | App | 9 |
| PM-11 | Worktree/project deletion cascades with no orphan rows | TillerPersistence | 8 |
| PM-12 | `legacyTerminalTab_v15` read-only for one release, no dual writes, no runtime fallback | App | 9, 14 |

### Keyboard and accessibility (#10)

| ID | Requirement | Owner | Phase |
|----|-------------|-------|-------|
| AX-1 | Menu + AX action for every pointer-only mutation | App + TillerWorkspace | 6, 11 |
| AX-2 | Direct shortcuts limited to local tab nav, spatial focus, Split Right/Down | App | 11 |
| AX-3 | `⌘⌥`+arrow spatial focus, no wrap, deterministic tie-break | TillerWorkspace | 6 |
| AX-4 | Exact-destination move commands (`Move Tab To…`), not directional guessing | App + TillerWorkspace | 6, 11 |
| AX-5 | Per-pane AX identity: title, selected tab, content kind, position, actions | TillerWorkspace | 6 |
| AX-6 | `Pane Groups` rotor in layout reading order | TillerWorkspace | 6 |
| AX-7 | Separate but synchronized operational / SwiftUI / first-responder / VoiceOver focus | TillerWorkspace | 6 |
| AX-8 | Focusable adjustable dividers, orientation + value, 5 % / 1 % steps | TillerWorkspace | 6 |
| AX-9 | Concise announcements: success, invalid action, retry/failure, restored placeholder | TillerWorkspace | 6 |
| AX-10 | Reduced Motion: instant commit + static highlight + announcement | TillerWorkspace | 6 |
| AX-11 | Increased Contrast / Reduced Transparency variants, no colour-only meaning | TillerWorkspace | 6 |
| AX-12 | Full Keyboard Access: complete operation with no pointer dependency | TillerWorkspace | 6 |
| AX-13 | Ghostty terminal-content AX not regressed or obscured | TillerTerminal | 7 |

### Control compatibility

| ID | Requirement | Owner | Phase |
|----|-------------|-------|-------|
| CT-1 | Existing panel/surface/session UUIDs keep meaning `TerminalContentID` | App | 12 |
| CT-2 | App resolves terminal content → tab → group at operation time | App | 12 |
| CT-3 | Moved terminal keeps control identity and live PTY | App | 12 |
| CT-4 | Control structural commands await the same durability gate | App | 12 |
| CT-5 | Stale/missing/non-terminal IDs → typed protocol error, no mutation | App | 12 |
| CT-6 | Disabling the socket disables no local feature and corrupts no activity ownership | App | 12 |

### Performance and resources

| ID | Budget | Owner | Phase |
|----|--------|-------|-------|
| PF-1 | Core command + invariant validation p95 ≤ 1 ms | TillerCore | 13 |
| PF-2 | Non-hydrating reconciliation, 8 groups / 32 tabs, p95 ≤ 8 ms on MainActor | TillerWorkspace | 13 |
| PF-3 | Drag preview + divider tracking p95 frame ≤ 16.7 ms, no two consecutive > 33.3 ms | TillerWorkspace | 13 |
| PF-4 | Structural SQLite commit, 32-tab snapshot, p95 ≤ 100 ms / p99 ≤ 250 ms | App | 13 |
| PF-5 | Restore decode+checksum+validate+first chrome reconcile, 16/64, p95 ≤ 250 ms | App | 13 |
| PF-6 | Worktree switch: no content regeneration, chrome within one frame | App | 13 |
| PF-7 | 10 000 generated valid mutations preserve invariants, no unbounded growth | TillerCore | 3 |
| PF-8 | 100 create/move/remount/close/retry cycles return counts to baseline | App | 13 |
| PF-9 | Deterministic CI assertions: operation counts, allocations/identity, no main-thread I/O | all | 13 |

### Verification gates and manual acceptance

| ID | Requirement | Owner | Phase |
|----|-------------|-------|-------|
| VG-1 | `Scripts/ci.sh` prints `CI OK` including App-target tests | repo | 0 |
| VG-2 | CI proves #8 dependency rules | repo | 0, 14 |
| VG-3 | CI proves absence of shipped `TerminalSplitHost` / legacy runtime path | repo | 14 |
| VG-4 | Migration fixture integrity check | repo | 9 |
| MA-1..10 | Signed manual checklist on real macOS 15+ | human | 14 |

**Out of scope (explicit, from #1 and #9):** cross-worktree drag, detached pane windows, named layout presets, duplicate simultaneous views of one content identity, sidebar/inspector rearrangement, replacing Ghostty's internal terminal-content accessibility.

---

## File structure

**New in `TillerCore`** (`Packages/TillerCore/Sources/TillerCore/Workspace/`) — Foundation-only:

| File | Responsibility |
|------|----------------|
| `WorkspaceIDs.swift` | `WorkspaceTabID`, `PaneGroupID`, `SplitID`, `TerminalContentID`, `ChatContentID`, `DocumentID`, `ResourceGenerationID` |
| `WorkspaceContentRef.swift` | `WorkspaceContentRef`, `DocumentEditorKind`, `WorkspaceContentKind` |
| `WorkspaceTab.swift` | `WorkspaceTab` (new), `WorkspaceTabViewState` |
| `WorkspaceLayout.swift` | `WorkspaceLayout`, `LayoutNode`, `PaneGroup`, `WorkspaceSplitAxis`, validating factory |
| `WorkspaceLayoutInvariants.swift` | invariant checks + deterministic repair (package-internal) |
| `WorkspaceLayoutCommand.swift` | `WorkspaceLayoutCommand`, `SplitPlacementSide`, `MoveDestination`, `SplitContentPayload` |
| `WorkspaceLayoutTransition.swift` | `WorkspaceLayoutTransition`, `WorkspaceLayoutDelta`, `FocusIntent` |
| `WorkspaceLayoutError.swift` | typed errors |
| `WorkspaceLayoutEngine.swift` | `apply(_:to:)` — the sole mutation seam |
| `WorkspaceSnapshot.swift` | Codable envelope + canonical encoding + pure version upgraders |

**New package `Packages/TillerWorkspace/`** — depends on `TillerCore` + system frameworks:

| File | Responsibility |
|------|----------------|
| `WorkspaceContentHost.swift` | `WorkspaceContentHost`, `WorkspaceHostProvider` seams |
| `WorkspaceIntent.swift` | `WorkspaceIntent`, `WorkspaceIntentSink`, `WorkspaceIntentDestination` |
| `WorkspaceView.swift` | SwiftUI facade (`NSViewControllerRepresentable`) |
| `WorkspaceViewController.swift` | root controller, publishes focused values |
| `WorkspaceReconciler.swift` | stable-ID controller reuse / reparent / prune |
| `PaneGroupController.swift` | one group: local tab strip + mounted host |
| `PaneTabStripView.swift` | tab chrome, insertion marker, overflow menu, autoscroll |
| `WorkspaceSplitController.swift` | `NSSplitViewController` subclass, preferred vs effective fraction |
| `DividerTracking.swift` | live tracking, clamps, keyboard/AX steps |
| `DragSession.swift` | threshold, grab offset, transient state, cancellation |
| `DropTargetResolver.swift` | 22 % edge band, centre/edge decision, legality |
| `SplitEligibility.swift` | 240×160 rule + disabled reasons |
| `OverflowCanvas.swift` | minimum-size canvas + scroll reveal |
| `SpatialNeighbors.swift` | directional focus resolution |
| `WorkspaceAccessibility.swift` | AX hierarchy, labels, actions, rotor |
| `WorkspaceAnnouncements.swift` | announcement strings (English) |
| `WorkspaceEnvironment.swift` | reduced motion / increased contrast / reduced transparency |

**Changed in `TillerTerminal`:** `TerminalSurfaceHost.swift` (new, one surface per `TerminalContentID`); `SplitViewRenderer.swift` deleted at Phase 14.

**New in `App/Workspace/`:** `WorkspaceCoordinator.swift`, `WorkspaceContentRegistry.swift`, `WorkspaceContentAdapter.swift`, `TerminalContentAdapter.swift`, `ChatContentAdapter.swift`, `DocumentContentAdapter.swift`, `WorkspaceLayoutPersistence.swift`, `SQLiteWorkspacePersistence.swift`, `WorkspaceMigrationV15.swift`, `WorkspaceRecoverySidecar.swift`, `WorkspaceMenuCommands.swift`, `WorkspaceHostAdapters.swift`, `WorkspaceControlRouting.swift`.

**Changed in `TillerPersistence`:** `AppDatabase.swift` (migration `v16`), `Records.swift` (`WorkspaceLayoutRecord`, `WorkspaceTabRecord`, `TerminalContentRecord`, `WorkspaceLayoutQuarantineRecord`).

**Changed at repo level:** `project.yml` (package + dependency), `Scripts/ci.sh`, new `Scripts/check-module-boundaries.sh`.

---

## Rollback and feature-gate strategy

- **Phases 1–10 are additive.** The new engine is built and unit-tested with no production call site; `TerminalSplitHost` keeps running. Rollback = revert the phase's commits; the app is unaffected.
- **Phase 11 is the cutover.** It is gated by `WorkspaceEngineGate.isEnabled`, backed by `UserDefaults` key `workspace.universalEngine` with a `TILLER_UNIVERSAL_WORKSPACE` environment override. Default flips to `true` in Task 11.6 only after Tasks 11.1–11.5 are green. Rollback during Phases 11–13 = set the key to `false` (legacy path still compiled) or revert the flip commit.
- **Phase 14 deletes both the gate and the legacy path.** After it, rollback is `git revert` of the cutover range, not a runtime switch — required by #8 ("the application never ships parallel legacy and universal layout engines").
- **Database rollback:** migration `v16` runs in one SQLite transaction and takes a timestamped file copy of the database into `Application Support/Tiller/backups/` before running. Failure rolls back completely, leaves v15 intact, disables new writes/restore, and surfaces a Retry with the backup path. Tiller never auto-resets the database.

---

# Phase 0 — Make the verification gate mean something (VG-1, VG-2)

### Task 0.1: Run the App-target tests inside `Scripts/ci.sh`

**Files:**
- Modify: `Scripts/ci.sh` (after the `xcodebuild … build` block, before the package batch)

**Interfaces:**
- Produces: a repo gate that fails when any `TillerTests` test fails.

**Context the implementer needs:** the App tests live in `AppTests/` but the *target* is `TillerTests` (declared in `project.yml`). Three verified traps: (a) adding `CODE_SIGNING_ALLOWED=NO` **or** `-derivedDataPath` makes the test host hang in dyld before test discovery — the build step may keep them, the test step must not; (b) `-only-testing:AppTests/...` fails, the target name is `TillerTests`; (c) selectors must be struct names, not `@Suite("display name")`, or the run is vacuously green.

- [ ] **Step 1: Add the test invocation**

Insert after the existing `xcodebuild … build | tail -5` line:

```bash
# App-target tests (TillerTests, sources in AppTests/). Deliberately NOT
# passing CODE_SIGNING_ALLOWED=NO or -derivedDataPath: with either one the
# test host hangs in dyld before test discovery on managed Macs.
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation | tee "$tmpdir_app/apptests.log" | tail -20
grep -qE "Test run with [0-9]+ tests" "$tmpdir_app/apptests.log" || {
    echo "FAILED: App test run reported no tests"; exit 1; }
```

Create `tmpdir_app=$(mktemp -d /tmp/tiller-apptests-XXXXXX)` next to the existing `tmpdir` and extend the `trap` to remove both.

- [ ] **Step 2: Run the gate and record the baseline**

Run: `Scripts/ci.sh`
Expected: either `CI OK`, or a list of currently failing App tests. `AppModelControlTests` is known to be red from earlier work — if it fails, fix it in this task (it is a pre-existing break in the module this plan rewrites) or, if the fix is non-trivial, quarantine it with an explicit `@Test(.disabled("pre-existing failure, tracked in <issue>"))` and record the issue link in the commit body. Do not leave a silently red gate.

- [ ] **Step 3: Commit**

```bash
git add Scripts/ci.sh
git commit -m "ci: run the App-target tests in the repository gate

refs VG-1"
```

### Task 0.2: Enforce the #8 module-boundary rules in CI

**Files:**
- Create: `Scripts/check-module-boundaries.sh`
- Modify: `Scripts/ci.sh` (call it before the build)
- Create: `Packages/TillerCore/Tests/TillerCoreTests/ModuleBoundaryTests.swift`

**Interfaces:**
- Produces: `Scripts/check-module-boundaries.sh` exits non-zero with a per-violation message.

**Context:** `TillerCore` as a *package* legitimately depends on `TillerPersistence` and GRDB (`ProjectStore.swift` imports GRDB directly, and the Xcode dynamic-framework build needs that edge explicit). So the "no GRDB in Core" rule from #8 can only be enforced **per file**, scoped to `Sources/TillerCore/Workspace/`.

- [ ] **Step 1: Write the checker**

```bash
#!/bin/bash
# Enforces the module boundaries decided in tillerai/tiller#8.
set -euo pipefail
cd "$(dirname "$0")/.."
fail=0
report() { echo "boundary violation: $1"; fail=1; }

# 1. Workspace domain files in TillerCore import Foundation only.
for f in Packages/TillerCore/Sources/TillerCore/Workspace/*.swift; do
    [ -e "$f" ] || continue
    while read -r line; do
        module=${line#import }
        case "$module" in
            Foundation) ;;
            *) report "$f imports $module (workspace domain is Foundation-only)";;
        esac
    done < <(grep -E '^import ' "$f" || true)
done

# 2. TillerWorkspace never imports app/content/persistence packages.
if [ -d Packages/TillerWorkspace ]; then
    if grep -rlE '^import (TillerTerminal|TillerACP|TillerCode|TillerPersistence|TillerControl|TillerAgents|GRDB|GhosttyTerminal)' \
        Packages/TillerWorkspace/Sources >/dev/null 2>&1; then
        report "TillerWorkspace imports a forbidden module"
    fi
fi

# 3. TillerTerminal never imports TillerWorkspace.
if grep -rlE '^import TillerWorkspace' Packages/TillerTerminal/Sources >/dev/null 2>&1; then
    report "TillerTerminal imports TillerWorkspace"
fi

[ "$fail" = 0 ] && echo "module boundaries OK"
exit "$fail"
```

- [ ] **Step 2: Verify it currently passes**

Run: `bash Scripts/check-module-boundaries.sh`
Expected: `module boundaries OK` (the `Workspace/` directory does not exist yet, so rules 1 and 2 are vacuous; rule 3 is real).

- [ ] **Step 3: Wire it into the gate**

Add `bash Scripts/check-module-boundaries.sh` immediately after `xcodegen generate` in `Scripts/ci.sh`.

- [ ] **Step 4: Run the gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`

- [ ] **Step 5: Commit**

```bash
git add Scripts/check-module-boundaries.sh Scripts/ci.sh
git commit -m "ci: enforce workspace module boundaries

refs VG-2"
```

---

# Phase 1 — Core domain model (OB-S-1, LC-2)

### Task 1.1: Rename the legacy tab model out of the way

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift` → renamed to `LegacyWorkspaceTab.swift`
- Modify: every call site (`App/AppModel.swift`, `App/ContentView.swift`, `App/TabBarView.swift`, `App/AppModel+Control.swift`, `App/NewTabMenuItems.swift`, `App/AutoNaming/*`, `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift`, `Packages/TillerCore/Sources/TillerCore/AgentTree.swift`, `Packages/TillerCore/Tests/**`, `AppTests/**`)

**Interfaces:**
- Produces: `LegacyWorkspaceTab`, `LegacyTabContent`. The names `WorkspaceTab` / `TabContent` become free for the new model. Both legacy types are deleted in Phase 14.

**Why:** the new and legacy models must coexist for ~13 phases. Two types called `WorkspaceTab` in one module is not an option; naming the *new* one awkwardly would leave the awkward name forever.

- [ ] **Step 1: Rename mechanically**

```bash
git mv Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift \
       Packages/TillerCore/Sources/TillerCore/LegacyWorkspaceTab.swift
grep -rl --include='*.swift' -E '\bWorkspaceTab\b|\bTabContent\b' App AppTests Packages \
  | xargs sed -i '' -E 's/\bWorkspaceTab\b/LegacyWorkspaceTab/g; s/\bTabContent\b/LegacyTabContent/g'
```

Then hand-fix the collateral: identifiers such as `WorkspaceTabIcon` (a view in `App/WorkspaceTabIcon.swift`) must keep their names — revert any rename that produced `LegacyWorkspaceTabIcon`.

- [ ] **Step 2: Run the gate**

Run: `Scripts/ci.sh`
Expected: `CI OK` (pure rename — behaviour identical). If `TillerTerminal`'s PTY tests flake, re-run that package alone: `cd Packages/TillerTerminal && swift test`.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor: rename the legacy tab model to LegacyWorkspaceTab

Frees WorkspaceTab/TabContent for the universal workspace model.
No behaviour change."
```

### Task 1.2: Typed identities

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceIDs.swift`
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceContentRef.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspaceIDsTests.swift`

**Interfaces:**
- Produces:

```swift
public struct WorkspaceTabID: Hashable, Sendable, Codable { public let rawValue: UUID; public init(_ rawValue: UUID = UUID()) }
public struct PaneGroupID: Hashable, Sendable, Codable { public let rawValue: UUID; public init(_ rawValue: UUID = UUID()) }
public struct SplitID: Hashable, Sendable, Codable { public let rawValue: UUID; public init(_ rawValue: UUID = UUID()) }
public struct TerminalContentID: Hashable, Sendable, Codable { public let rawValue: UUID; public init(_ rawValue: UUID = UUID()) }
public struct ChatContentID: Hashable, Sendable, Codable { public let rawValue: String; public init(_ rawValue: String) }
public struct ResourceGenerationID: Hashable, Sendable { public let rawValue: UUID; public init(_ rawValue: UUID = UUID()) }

public struct DocumentID: Hashable, Sendable, Codable {
    public let worktreeID: UUID
    public let canonicalPath: String
    /// Absolute, standardized, symlink-resolved at open time, worktree-scoped.
    public static func make(worktreeID: UUID, fileURL: URL) -> DocumentID
}

public enum DocumentEditorKind: String, Codable, Sendable { case markdown, code }
public enum WorkspaceContentKind: String, Codable, Sendable { case terminal, chat, document }

public enum WorkspaceContentRef: Hashable, Sendable, Codable {
    case terminal(TerminalContentID)
    case chat(ChatContentID)
    case document(DocumentID, editor: DocumentEditorKind)
    public var kind: WorkspaceContentKind { get }
    /// Stable string used by the unique (worktreeId, kind, contentId) constraint.
    public var contentIdentifierString: String { get }
}
```

- [ ] **Step 1: Write the failing tests**

```swift
import Testing
import Foundation
@testable import TillerCore

@Suite struct WorkspaceIDsTests {
    @Test func documentIDResolvesSymlinksAndStandardizesPath() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let real = dir.appendingPathComponent("real.md")
        try Data("x".utf8).write(to: real)
        let link = dir.appendingPathComponent("link.md")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: real)
        let worktree = UUID()

        let viaReal = DocumentID.make(worktreeID: worktree, fileURL: real)
        let viaLink = DocumentID.make(worktreeID: worktree, fileURL: link)

        #expect(viaReal == viaLink)
    }

    @Test func sameFileInDifferentWorktreesIsDistinct() {
        let url = URL(fileURLWithPath: "/tmp/a.md")
        #expect(DocumentID.make(worktreeID: UUID(), fileURL: url)
                != DocumentID.make(worktreeID: UUID(), fileURL: url))
    }

    @Test func contentIdentifierStringIsStableAcrossEncodingRoundTrip() throws {
        let ref = WorkspaceContentRef.chat(ChatContentID("session-1"))
        let data = try JSONEncoder().encode(ref)
        let decoded = try JSONDecoder().decode(WorkspaceContentRef.self, from: data)
        #expect(decoded == ref)
        #expect(decoded.contentIdentifierString == ref.contentIdentifierString)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter WorkspaceIDsTests`
Expected: compile failure — `cannot find 'DocumentID' in scope`.

- [ ] **Step 3: Implement the identity types**

`DocumentID.make` uses `fileURL.resolvingSymlinksInPath().standardizedFileURL.path`. `contentIdentifierString` returns `rawValue.uuidString` for terminal, `rawValue` for chat, `canonicalPath` for document (the worktree is already a separate column in the unique constraint).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter WorkspaceIDsTests`
Expected: PASS

- [ ] **Step 5: Verify boundaries and commit**

```bash
bash Scripts/check-module-boundaries.sh
git add Packages/TillerCore/Sources/TillerCore/Workspace Packages/TillerCore/Tests/TillerCoreTests/Workspace
git commit -m "feat: add typed workspace content identities

refs LC-2"
```

### Task 1.3: `WorkspaceTab`, view state, `PaneGroup`, `LayoutNode`, `WorkspaceLayout` with a validating factory

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceTab.swift`
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceLayout.swift`
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceLayoutInvariants.swift`
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceLayoutError.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspaceLayoutInvariantTests.swift`

**Interfaces:**
- Consumes: everything from Task 1.2.
- Produces:

```swift
public struct WorkspaceTabViewState: Equatable, Sendable, Codable {
    public var documentCaretOffset: Int?
    public var documentSelectionLength: Int?
    public var documentScrollAnchor: Double?
    public var documentFoldedRanges: [ClosedRange<Int>]
    public var editorMode: DocumentEditorKind?
    public var chatComposerDraft: String?
    public var chatAttachmentReferences: [String]
    public var chatTranscriptAnchor: Double?
    public var followsTail: Bool
    public var terminalViewportAnchor: Double?
    public static var empty: WorkspaceTabViewState { get }
}

public struct WorkspaceTab: Identifiable, Equatable, Sendable, Codable {
    public let id: WorkspaceTabID
    public var title: String
    public var titleIsAutoNamed: Bool
    public let content: WorkspaceContentRef
    public var viewState: WorkspaceTabViewState
    public init(id: WorkspaceTabID, title: String, titleIsAutoNamed: Bool,
                content: WorkspaceContentRef, viewState: WorkspaceTabViewState = .empty)
}

public enum WorkspaceSplitAxis: String, Sendable, Codable {
    /// Children sit side by side: `first` is left, `second` is right.
    case horizontal
    /// Children stack: `first` is above, `second` is below.
    case vertical
}

public indirect enum LayoutNode: Equatable, Sendable, Codable {
    case group(PaneGroupID)
    case split(id: SplitID, axis: WorkspaceSplitAxis, fraction: Double,
               first: LayoutNode, second: LayoutNode)
}

public struct PaneGroup: Identifiable, Equatable, Sendable, Codable {
    public let id: PaneGroupID
    public var tabs: [WorkspaceTab]
    public var activeTabID: WorkspaceTabID?
}

public struct WorkspaceLayout: Equatable, Sendable {
    public private(set) var root: LayoutNode
    public private(set) var groups: [PaneGroupID: PaneGroup]
    public private(set) var activeGroupID: PaneGroupID

    /// The only public way to build a layout. Rejects any value that would
    /// violate the invariants from #2.
    public static func make(root: LayoutNode, groups: [PaneGroupID: PaneGroup],
                            activeGroupID: PaneGroupID)
        -> Result<WorkspaceLayout, WorkspaceLayoutError>

    /// Valid empty layout for a worktree with no content.
    public static func empty(groupID: PaneGroupID = PaneGroupID()) -> WorkspaceLayout

    // Read-only queries used by renderer/persistence.
    public func group(_ id: PaneGroupID) -> PaneGroup?
    public func groupContaining(tab: WorkspaceTabID) -> PaneGroupID?
    public func tab(_ id: WorkspaceTabID) -> WorkspaceTab?
    public var orderedGroupIDs: [PaneGroupID] { get }   // depth-first reading order
    public var allTabs: [WorkspaceTab] { get }
    public func splitIDs() -> [SplitID]
    public func preferredFraction(for split: SplitID) -> Double?
}

/// Package-internal validation, plus one public entry point used by tests and
/// by restore-time repair. Both spellings exist deliberately: the tuple form
/// validates candidate parts before a layout exists, the value form re-checks
/// an already-built layout.
public enum WorkspaceLayoutInvariants {
    static func validate(root: LayoutNode, groups: [PaneGroupID: PaneGroup],
                         activeGroupID: PaneGroupID) -> WorkspaceLayoutError?
    public static func validate(_ layout: WorkspaceLayout) -> WorkspaceLayoutError?
}

public enum WorkspaceLayoutError: Error, Equatable, Sendable {
    case emptyGroupRegistry
    case orphanGroup(PaneGroupID)
    case unresolvedGroupLeaf(PaneGroupID)
    case duplicateID(String)
    case emptyNonRootGroup(PaneGroupID)
    case unknownActiveGroup(PaneGroupID)
    case activeTabNotInGroup(PaneGroupID)
    case invalidFraction(SplitID, Double)
    case duplicateContentOwnership(String)
    case unknownGroup(PaneGroupID)
    case unknownTab(WorkspaceTabID)
    case illegalSplitOfSoleTab(PaneGroupID)
    case staleRevision(expected: Int, actual: Int)
}
```

The fieldwise initializer of `WorkspaceLayout` is `internal`, so no partially coherent value crosses the public seam (#8).

- [ ] **Step 1: Write the failing invariant tests**

One test per invariant from #2 (1–12; 13–15 are covered in Phases 2 and 8):

```swift
@Suite struct WorkspaceLayoutInvariantTests {
    private func tab(_ title: String = "t") -> WorkspaceTab {
        WorkspaceTab(id: WorkspaceTabID(), title: title, titleIsAutoNamed: true,
                     content: .terminal(TerminalContentID()))
    }

    @Test func emptyRootGroupIsValid() {                       // invariant 7, OB-S-1
        let layout = WorkspaceLayout.empty()
        #expect(layout.groups.count == 1)
        #expect(layout.group(layout.activeGroupID)?.activeTabID == nil)
    }

    @Test func rejectsOrphanGroupNotPresentInTheTree() {       // invariant 3
        let leaf = PaneGroupID(), orphan = PaneGroupID()
        let t = tab()
        let result = WorkspaceLayout.make(
            root: .group(leaf),
            groups: [leaf: PaneGroup(id: leaf, tabs: [t], activeTabID: t.id),
                     orphan: PaneGroup(id: orphan, tabs: [tab()], activeTabID: nil)],
            activeGroupID: leaf)
        #expect(result == .failure(.orphanGroup(orphan)))
    }

    @Test func rejectsEmptyNonRootGroup() { /* invariant 6 */ }
    @Test func rejectsFractionOutsideExclusiveZeroOneRange() { /* invariant 10 */ }
    @Test func rejectsDuplicateTabIDAcrossGroups() { /* invariant 4 */ }
    @Test func rejectsTwoTabsOwningTheSameContentIdentity() { /* LC-2 */ }
    @Test func rejectsActiveTabThatIsNotInItsGroup() { /* invariant 9 */ }
    @Test func rejectsUnknownActiveGroup() { /* invariant 8 */ }
    @Test func orderedGroupIDsFollowDepthFirstReadingOrder() { /* AX-6 input */ }
}
```

Each stub must be filled with real arrange/act/assert code — no empty bodies at commit time.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter WorkspaceLayoutInvariantTests`
Expected: compile failure — `cannot find 'WorkspaceLayout' in scope`.

- [ ] **Step 3: Implement the model and validation**

`WorkspaceLayoutInvariants.validate(root:groups:activeGroupID:)` walks the tree once collecting leaf group IDs, split IDs, and fractions; then cross-checks the registry, tab IDs, content identities, and selections. Returns the first error in a **deterministic order** (registry → tree → IDs → selections → fractions) so tests are stable.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter WorkspaceLayoutInvariantTests`
Expected: PASS

- [ ] **Step 5: Run the phase gate and commit**

Run: `Scripts/ci.sh` → `CI OK`

```bash
git add Packages/TillerCore
git commit -m "feat: add the universal WorkspaceLayout domain model

refs OB-S-1, LC-2"
```

---

# Phase 2 — Core mutation engine (OB-S-3, OB-S-6, OB-S-8, OB-S-9, OB-D-5)

### Task 2.1: Commands, transition, delta, focus intent

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceLayoutCommand.swift`
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceLayoutTransition.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspaceLayoutDeltaTests.swift`

**Interfaces:**
- Produces:

```swift
public enum SplitPlacementSide: String, Sendable, Codable, Equatable { case right, down }

public enum SplitContentPayload: Sendable, Equatable {
    case newTab(WorkspaceTab)
    case existingTab(WorkspaceTabID)
}

public enum MoveDestination: Sendable, Equatable {
    case group(PaneGroupID, index: Int)
    case newSplit(anchor: PaneGroupID, placement: SplitPlacementSide,
                  newGroup: PaneGroupID, newSplit: SplitID)
}

public enum WorkspaceLayoutCommand: Sendable, Equatable {
    case insertTab(WorkspaceTab, into: PaneGroupID, index: Int?, activate: Bool)
    case splitGroup(anchor: PaneGroupID, placement: SplitPlacementSide,
                    newGroup: PaneGroupID, newSplit: SplitID, content: SplitContentPayload)
    case moveTab(WorkspaceTabID, to: MoveDestination)
    case closeTab(WorkspaceTabID)
    case activateTab(WorkspaceTabID)
    case activateGroup(PaneGroupID)
    case setPreferredFraction(SplitID, Double)
    case updateViewState(WorkspaceTabID, WorkspaceTabViewState)
    case renameTab(WorkspaceTabID, title: String, isAutoNamed: Bool)
}

public enum FocusIntent: Sendable, Equatable {
    case none
    case focusTab(WorkspaceTabID)
    case focusDivider(SplitID)
}

public struct WorkspaceLayoutDelta: Sendable, Equatable {
    public var insertedTabs: [WorkspaceTabID]
    public var removedTabs: [WorkspaceTabID]
    public var movedTabs: [WorkspaceTabID]
    public var reorderedGroups: [PaneGroupID]
    public var insertedGroups: [PaneGroupID]
    public var removedGroups: [PaneGroupID]
    public var insertedSplits: [SplitID]
    public var collapsedSplits: [SplitID]
    public var activeGroupChanged: PaneGroupID?
    public var activeTabChanges: [PaneGroupID: WorkspaceTabID?]
    public var preferredFractionChanges: [SplitID: Double]
    public var isStructural: Bool     // drives the durability gate in #7

    /// Single source of the structural classification, so the engine, the
    /// coordinator, and the persistence gate cannot drift apart.
    public static func isStructuralCommand(_ command: WorkspaceLayoutCommand) -> Bool
}

public struct WorkspaceLayoutTransition: Sendable, Equatable {
    public let layout: WorkspaceLayout
    public let delta: WorkspaceLayoutDelta
    public let focusIntent: FocusIntent
}
```

`isStructural` is `true` for `insertTab`, `splitGroup`, `moveTab`, `closeTab`; `false` for `activateTab`, `activateGroup`, `setPreferredFraction`, `updateViewState`, `renameTab`. The transition carries no controllers, closures, database handles, PTYs, or transports — enforced by the boundary checker plus a compile-time `Sendable` conformance.

- [ ] **Step 1: Write the failing test that pins the structural classification**

```swift
@Test func onlyOwnershipMutationsAreStructural() {
    #expect(WorkspaceLayoutDelta.isStructuralCommand(.closeTab(WorkspaceTabID())))
    #expect(!WorkspaceLayoutDelta.isStructuralCommand(.setPreferredFraction(SplitID(), 0.4)))
    #expect(!WorkspaceLayoutDelta.isStructuralCommand(.activateGroup(PaneGroupID())))
}
```

- [ ] **Step 2: Run to verify it fails** — `cd Packages/TillerCore && swift test --filter WorkspaceLayoutDeltaTests`
- [ ] **Step 3: Implement the command/transition value types**
- [ ] **Step 4: Run to verify it passes**
- [ ] **Step 5: Commit** — `git commit -m "feat: add workspace layout commands and semantic deltas"`

### Task 2.2: `WorkspaceLayoutEngine.apply` — insert, activate, reorder, rename, view state

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceLayoutEngine.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspaceLayoutEngineLocalTests.swift`

**Interfaces:**
- Produces: `public enum WorkspaceLayoutEngine { public static func apply(_ command: WorkspaceLayoutCommand, to layout: WorkspaceLayout) -> Result<WorkspaceLayoutTransition, WorkspaceLayoutError> }`

- [ ] **Step 1: Write the failing tests**

```swift
@Test func activatingATabDoesNotChangeTopology() {          // OB-S-3
    let (layout, tabB) = Fixtures.twoTabsInOneGroup()
    let before = layout.root
    let t = try #require(try WorkspaceLayoutEngine.apply(.activateTab(tabB), to: layout).get())
    #expect(t.layout.root == before)
    #expect(t.delta.insertedSplits.isEmpty && t.delta.removedGroups.isEmpty)
    #expect(t.delta.isStructural == false)
    #expect(t.focusIntent == .focusTab(tabB))
}

@Test func insertingATabRejectsDuplicateContentOwnership() {  // FH-B-3, LC-2
    let (layout, contentID) = Fixtures.singleTerminalTab()
    let clone = WorkspaceTab(id: WorkspaceTabID(), title: "clone", titleIsAutoNamed: true,
                             content: .terminal(contentID))
    let result = WorkspaceLayoutEngine.apply(
        .insertTab(clone, into: layout.activeGroupID, index: nil, activate: true), to: layout)
    #expect(result == .failure(.duplicateContentOwnership(contentID.rawValue.uuidString)))
}

@Test func reorderingWithinAGroupEmitsReorderNotMove() { /* OB-S-3 */ }
@Test func renameClearsAutoNamedFlagWithoutTouchingTopology() { }
@Test func updateViewStateIsNonStructural() { }
```

Add `Packages/TillerCore/Tests/TillerCoreTests/Workspace/Fixtures.swift` with `singleTerminalTab()`, `twoTabsInOneGroup()`, `deepMixedOrientationLayout(groups:tabs:)`, `acceptanceEnvelope()` (16 groups / 64 tabs / 4 kinds) — reused by Phases 3, 8, 9, 13.

- [ ] **Step 2: Run to verify they fail** — expected: `cannot find 'WorkspaceLayoutEngine'`
- [ ] **Step 3: Implement the local (non-topological) commands**
- [ ] **Step 4: Run to verify they pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: apply local workspace commands through one mutation seam\n\nrefs OB-S-3"`

### Task 2.3: Split, move, close, collapse (OB-S-6, OB-S-8, OB-S-9, OB-D-5)

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceLayoutEngine.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspaceLayoutEngineStructuralTests.swift`

- [ ] **Step 1: Write the failing tests**

```swift
@Test func splitCreatesExactlyOneGroupAndOneSplitAtHalf() {          // OB-S-6
    let (layout, _) = Fixtures.singleTerminalTab()
    let newGroup = PaneGroupID(), newSplit = SplitID()
    let fresh = Fixtures.freshTerminalTab()
    let t = try #require(try WorkspaceLayoutEngine.apply(
        .splitGroup(anchor: layout.activeGroupID, placement: .right,
                    newGroup: newGroup, newSplit: newSplit, content: .newTab(fresh)),
        to: layout).get())

    #expect(t.delta.insertedGroups == [newGroup])
    #expect(t.delta.insertedSplits == [newSplit])
    #expect(t.layout.groups.count == 2)
    if case .split(_, let axis, let fraction, _, _) = t.layout.root {
        #expect(axis == .horizontal)        // `right` places the new group beside
        #expect(fraction == 0.5)
    } else { Issue.record("root should be a split") }
    #expect(t.layout.activeGroupID == newGroup)
    #expect(t.focusIntent == .focusTab(fresh.id))
}

@Test func splitDownPlacesTheNewGroupSecondOnTheVerticalAxis() { }
@Test func movingTheLastTabOutOfANonRootGroupCollapsesItAndItsParent() {  // OB-S-8
    // one commit: moved tab, removed group, collapsed split, sibling promoted
}
@Test func closingTheLastRootTabLeavesTheValidEmptyRootGroup() {          // OB-S-9
    // groups.count == 1, activeTabID == nil, layout still valid
}
@Test func splittingASoleTabAwayFromItsOwnGroupIsRejected() {             // #3 rule 7
    #expect(result == .failure(.illegalSplitOfSoleTab(groupID)))
}
@Test func moveToUnknownGroupLeavesTheLayoutUntouched() { }               // FH-B-4
@Test func closeRepairsSelectionDeterministicallyToTheNeighbourOnTheLeft() { }
@Test func everySuccessfulCommandProducesAValidLayout() { }
```

- [ ] **Step 2: Run to verify they fail**
- [ ] **Step 3: Implement structural commands**

Rules: a split rebuilds only the path from the root to the anchor leaf. Collapse promotes the sibling **in place** of the parent split, preserving the grandparent fraction. Selection repair after close picks the previous tab in local order, else the next, else `nil` (root only). `activeGroupID` after a collapse moves to the promoted sibling's nearest group in reading order.

- [ ] **Step 4: Run to verify they pass** — `cd Packages/TillerCore && swift test --filter WorkspaceLayoutEngineStructuralTests`
- [ ] **Step 5: Run the phase gate and commit**

Run: `Scripts/ci.sh` → `CI OK`

```bash
git commit -m "feat: apply structural workspace commands transactionally

refs OB-S-6, OB-S-8, OB-S-9, OB-D-5"
```

---

# Phase 3 — Snapshot codec, upgraders, generated-sequence invariants (FH-R-1, PF-7)

### Task 3.1: Canonical Codable snapshot with sorted keys

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceSnapshot.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspaceSnapshotTests.swift`

**Interfaces:**
- Produces:

```swift
public struct WorkspaceSnapshot: Codable, Equatable, Sendable {
    public static let currentSchemaVersion = 1
    public let schemaVersion: Int
    public let root: LayoutNode
    public let groups: [PaneGroupSnapshot]        // ordered by reading order, not a dictionary
    public let activeGroupID: PaneGroupID

    public init(layout: WorkspaceLayout)
    /// Canonical bytes: sorted keys, no escaping slashes, no pretty printing.
    public func canonicalPayload() throws -> Data
    public static func decode(_ payload: Data) -> Result<WorkspaceSnapshot, WorkspaceLayoutError>
    public func materialize() -> Result<WorkspaceLayout, WorkspaceLayoutError>
}

public struct PaneGroupSnapshot: Codable, Equatable, Sendable {
    public let id: PaneGroupID
    public let tabIDs: [WorkspaceTabID]          // metadata lives in the workspaceTab table
    public let activeTabID: WorkspaceTabID?
}

public enum WorkspaceSnapshotUpgrader {
    /// Pure, testable upgrade chain. Unknown *newer* versions are reported as
    /// `.unsupportedFutureVersion`, never as corruption.
    public static func upgrade(_ payload: Data, from version: Int)
        -> Result<WorkspaceSnapshot, WorkspaceSnapshotUpgradeError>
}

public enum WorkspaceSnapshotUpgradeError: Error, Equatable, Sendable {
    case unsupportedFutureVersion(Int)
    case malformed(String)
}
```

The snapshot stores **structure only** — tab titles, content refs, and view state live in the relational `workspaceTab` table (#7). Runtime state (focus, clamps, hover, mount phase, generations) is never encoded.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func canonicalPayloadIsByteStableAcrossEncodes() throws {
    let layout = Fixtures.deepMixedOrientationLayout(groups: 8, tabs: 20)
    let a = try WorkspaceSnapshot(layout: layout).canonicalPayload()
    let b = try WorkspaceSnapshot(layout: layout).canonicalPayload()
    #expect(a == b)
}

@Test func roundTripPreservesTopologyIDsAndSelections() throws { }
@Test func snapshotExcludesRuntimeOnlyState() throws {
    let json = try #require(String(data: payload, encoding: .utf8))
    for forbidden in ["firstResponder", "generation", "hover", "mountPhase", "effectiveFraction"] {
        #expect(!json.contains(forbidden))
    }
}
@Test func unknownFutureVersionIsNotClassifiedAsCorruption() {     // FH-R-1
    #expect(WorkspaceSnapshotUpgrader.upgrade(payload, from: 999)
            == .failure(.unsupportedFutureVersion(999)))
}
@Test func decodingAMalformedPayloadNeverYieldsAPartialLayout() { }
```

- [ ] **Step 2: Run to verify they fail**
- [ ] **Step 3: Implement the codec** using `JSONEncoder` with `.sortedKeys` + `.withoutEscapingSlashes`
- [ ] **Step 4: Run to verify they pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: add the canonical workspace snapshot codec\n\nrefs FH-R-1"`

### Task 3.2: Generated valid command sequences preserve invariants (PF-7)

**Files:**
- Test: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspaceLayoutFuzzTests.swift`
- Create: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/CommandGenerator.swift`

**Interfaces:**
- Consumes: `WorkspaceLayoutEngine.apply`.
- Produces: `CommandGenerator.next(for layout: WorkspaceLayout, rng: inout SeededRandom) -> WorkspaceLayoutCommand` — only ever emits commands that are legal for the given layout.

- [ ] **Step 1: Write the failing test**

```swift
@Test func tenThousandGeneratedMutationsPreserveEveryInvariant() {   // PF-7
    var rng = SeededRandom(seed: 0xTILLER)
    var layout = WorkspaceLayout.empty()
    for step in 0..<10_000 {
        let command = CommandGenerator.next(for: layout, rng: &rng)
        switch WorkspaceLayoutEngine.apply(command, to: layout) {
        case .success(let transition):
            layout = transition.layout
            #expect(WorkspaceLayoutInvariants.validate(layout) == nil,
                    "invariant broken at step \(step) by \(command)")
        case .failure(let error):
            Issue.record("generator emitted an illegal command at \(step): \(command) -> \(error)")
        }
    }
    #expect(layout.groups.count <= 10_000)   // no unbounded growth
}
```

Use a deterministic seed so failures reproduce. `SeededRandom` is a 4-line SplitMix64 in the test target — no dependency.

- [ ] **Step 2: Run to verify it fails** (generator missing)
- [ ] **Step 3: Implement the generator**
- [ ] **Step 4: Run to verify it passes** — `cd Packages/TillerCore && swift test --filter WorkspaceLayoutFuzzTests` (budget: < 5 s)
- [ ] **Step 5: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "test: fuzz workspace command sequences against the invariants\n\nrefs PF-7"`

---

# Phase 4 — `TillerWorkspace` package and stable-ID renderer (OB-S-2, OB-D-9, FH-F-1)

### Task 4.1: Create the package and wire it into the build

**Files:**
- Create: `Packages/TillerWorkspace/Package.swift`
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceContentHost.swift`
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceIntent.swift`
- Create: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PackageSmokeTests.swift`
- Modify: `project.yml` (add package + `Tiller` dependency)
- Modify: `Scripts/ci.sh` — none needed: the loop globs `Packages/*/` and `TillerWorkspace` has no PTY tests, so it joins the parallel batch.

**Interfaces:**
- Produces:

```swift
@MainActor public protocol WorkspaceContentHost: AnyObject {
    var tabID: WorkspaceTabID { get }
    var viewController: NSViewController { get }
    func setVisible(_ isVisible: Bool)
    /// Returns false when focus could not be taken; the renderer retries once
    /// after attachment (FH-F-1) and reports the failure accessibly.
    @discardableResult func fulfill(_ intent: FocusIntent) -> Bool
}

@MainActor public protocol WorkspaceHostProvider: AnyObject {
    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost?
}

public enum WorkspaceIntentDestination: Sendable, Equatable {
    case group(PaneGroupID, index: Int)
    case edgeSplit(anchor: PaneGroupID, placement: SplitPlacementSide)
}

public enum WorkspaceIntent: Sendable, Equatable {
    case requestSplit(anchor: PaneGroupID, placement: SplitPlacementSide)
    case requestClose(WorkspaceTabID)
    case requestMove(WorkspaceTabID, to: WorkspaceIntentDestination)
    case activateTab(WorkspaceTabID)
    case activateGroup(PaneGroupID)
    case setPreferredFraction(SplitID, Double)
    case retryContent(WorkspaceTabID)
}

@MainActor public protocol WorkspaceIntentSink: AnyObject { func send(_ intent: WorkspaceIntent) }
```

Note the seam discipline from #8: the host protocol exposes **no** hydrate/prepare/checkpoint/retry/suspend/close. `TillerWorkspace` may attach or detach a controller; it never releases the host.

- [ ] **Step 1: Write the failing smoke test**

```swift
@Suite @MainActor struct PackageSmokeTests {
    @Test func fakeHostSatisfiesTheContentSeam() {
        let host = FakeContentHost(tabID: WorkspaceTabID())
        #expect(host.fulfill(.focusTab(host.tabID)))
    }
}
```

Add `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/FakeContentHost.swift` — a content-neutral fake used by every later renderer test (proves content neutrality, per #8's verification list).

- [ ] **Step 2: Run to verify it fails** — `cd Packages/TillerWorkspace && swift test` (no package yet)

- [ ] **Step 3: Create the package**

```swift
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerWorkspace",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerWorkspace", targets: ["TillerWorkspace"])],
    dependencies: [.package(path: "../TillerCore")],
    targets: [
        .target(name: "TillerWorkspace", dependencies: ["TillerCore"]),
        .testTarget(name: "TillerWorkspaceTests", dependencies: ["TillerWorkspace"])
    ]
)
```

Add to `project.yml` under `packages:` (`TillerWorkspace: {path: Packages/TillerWorkspace}`) and to the `Tiller` target's `dependencies:`. Then `xcodegen generate`.

- [ ] **Step 4: Run to verify it passes** — `cd Packages/TillerWorkspace && swift test`
- [ ] **Step 5: Verify boundaries + gate, then commit**

```bash
bash Scripts/check-module-boundaries.sh   # rule 2 now non-vacuous
Scripts/ci.sh                             # CI OK
git add Packages/TillerWorkspace project.yml
git commit -m "feat: add the TillerWorkspace package with the content-host seam"
```

### Task 4.2: Stable-ID reconciler

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift`
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift`
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceSplitController.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceReconcilerTests.swift`

**Interfaces:**
- Consumes: `WorkspaceLayout`, `WorkspaceLayoutDelta`, `WorkspaceHostProvider`, `FakeContentHost`.
- Produces:

```swift
@MainActor public final class WorkspaceReconciler {
    public init(hostProvider: WorkspaceHostProvider)
    /// Applies a new layout, reusing cached controllers by stable ID.
    public func reconcile(to layout: WorkspaceLayout, delta: WorkspaceLayoutDelta?) 
    public func groupController(_ id: PaneGroupID) -> PaneGroupController?
    public func splitController(_ id: SplitID) -> WorkspaceSplitController?
    public var mountedHostCount: Int { get }        // OB-S-2 assertion hook
}
```

- [ ] **Step 1: Write the failing tests**

```swift
@Test func movingATabKeepsTheSameControllerInstance() {              // OB-D-9
    let r = WorkspaceReconciler(hostProvider: provider)
    r.reconcile(to: layoutA, delta: nil)
    let before = ObjectIdentifier(try #require(provider.host(for: tabID)).viewController)
    r.reconcile(to: layoutAfterMove, delta: moveDelta)
    let after = ObjectIdentifier(try #require(provider.host(for: tabID)).viewController)
    #expect(before == after)
}

@Test func exactlyOneHostIsMountedPerVisibleGroup() {                // OB-S-2
    r.reconcile(to: Fixtures.fourGroupsWithThreeTabsEach(), delta: nil)
    #expect(r.mountedHostCount == 4)
}

@Test func collapsingAGroupPrunesOnlyItsOwnCachedControllers() { }
@Test func reorderWithinAGroupDoesNotRebuildSplitControllers() { }
@Test func firstResponderSurvivesADetachReattachCycle() { }          // OB-D-9
@Test func focusIntentIsRetriedOnceAfterHostAttachment() { }         // FH-F-1
```

The first-responder test reuses the technique already proven in `Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalSplitHostFocusTests.swift` (a `FocusableTestView` in an offscreen `NSWindow`) — port it, do not reinvent it.

- [ ] **Step 2: Run to verify they fail**
- [ ] **Step 3: Implement the reconciler**

Port the two hard-won behaviours from `SplitViewRenderer.swift`: (a) cache controllers by stable ID and `removeFromParent()` + `removeFromSuperview()` before reparenting; (b) capture `window.firstResponder` before the rebuild and restore it afterwards if the detach cycle dropped it (`restoreFocusIfNeeded`). Prune the cache only for IDs absent from the new layout.

- [ ] **Step 4: Run to verify they pass** — `cd Packages/TillerWorkspace && swift test --filter WorkspaceReconcilerTests`
- [ ] **Step 5: Commit** — `git commit -m "feat: reconcile the workspace tree by stable id\n\nrefs OB-S-2, OB-D-9, FH-F-1"`

### Task 4.3: Renderer facade, geometry, preferred vs effective fraction

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift`
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceView.swift`
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceMetrics.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceGeometryTests.swift`

**Interfaces:**
- Produces:

```swift
public enum WorkspaceMetrics {
    public static let preferredGroupSize = CGSize(width: 240, height: 160)
    public static let dividerThickness: CGFloat = 6
    public static let edgeBandFraction: CGFloat = 0.22
    public static let dragThreshold: CGFloat = 4
}

@MainActor public final class WorkspaceViewController: NSViewController {
    public init(hostProvider: WorkspaceHostProvider, intentSink: WorkspaceIntentSink)
    public func update(layout: WorkspaceLayout, delta: WorkspaceLayoutDelta?)
    public func effectiveFraction(for split: SplitID) -> Double?
    public private(set) var currentLayout: WorkspaceLayout { get }
}

public struct WorkspaceView: NSViewControllerRepresentable {
    public init(layout: WorkspaceLayout, delta: WorkspaceLayoutDelta?,
                hostProvider: WorkspaceHostProvider, intentSink: WorkspaceIntentSink)
}
```

- [ ] **Step 1: Write the failing tests**

```swift
@Test func effectiveFractionClampsWithoutOverwritingThePreferredValue() {  // CO-4
    controller.update(layout: layoutWithFraction(0.05), delta: nil)
    controller.view.frame = CGRect(x: 0, y: 0, width: 600, height: 400)
    controller.view.layoutSubtreeIfNeeded()
    #expect(controller.effectiveFraction(for: splitID)! > 0.05)      // clamped to 240 pt
    #expect(controller.currentLayout.preferredFraction(for: splitID) == 0.05)
}

@Test func horizontalAxisPlacesFirstChildOnTheLeft() { }
@Test func verticalAxisPlacesFirstChildOnTop() { }
@Test func dividerThicknessMatchesTheNativeThickDivider() { }
```

Note the axis semantics trap: the legacy renderer wrote `splitView.isVertical = (axis == .horizontal)`. `WorkspaceSplitAxis.horizontal` means *side by side*, which is `NSSplitView.isVertical == true`. Assert it explicitly so nobody re-inverts it.

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "feat: render the workspace tree with native split controllers\n\nrefs CO-1, CO-4"`

---

# Phase 5 — Interaction: drag, drop, resize, overflow (OB-D-1…8, CO-1…6)

### Task 5.1: Drop-target resolution and split eligibility

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/DropTargetResolver.swift`
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/SplitEligibility.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DropTargetResolverTests.swift`

**Interfaces:**
- Produces:

```swift
public enum DropTarget: Equatable, Sendable {
    case tabStrip(PaneGroupID, insertionIndex: Int)
    case center(PaneGroupID)
    case edge(PaneGroupID, placement: EdgePlacement)
    case none
}
public enum EdgePlacement: Equatable, Sendable { case left, right, top, bottom }

public enum DropTargetResolver {
    public static func resolve(pointInGroup: CGPoint, groupBounds: CGRect,
                               tabStripHeight: CGFloat, tabFrames: [CGRect],
                               draggedTab: WorkspaceTabID, sourceGroup: PaneGroupID,
                               groupTabCount: Int) -> DropTarget
}

public enum SplitEligibility {
    public enum Reason: Equatable, Sendable {
        case insufficientWidth(available: CGFloat, required: CGFloat)
        case insufficientHeight(available: CGFloat, required: CGFloat)
        case soleTabOfItsOwnGroup
    }
    public static func check(groupSize: CGSize, placement: SplitPlacementSide,
                             isSoleTabOfSourceGroup: Bool) -> Result<Void, Reason>
}
```

- [ ] **Step 1: Write the failing tests**

```swift
@Test func pointerInsideTheOuterTwentyTwoPercentPicksTheNearestEdge() {   // OB-D-4
    let bounds = CGRect(x: 0, y: 0, width: 1000, height: 500)
    #expect(DropTargetResolver.resolve(pointInGroup: CGPoint(x: 950, y: 250), groupBounds: bounds, …)
            == .edge(groupID, placement: .right))
    #expect(DropTargetResolver.resolve(pointInGroup: CGPoint(x: 500, y: 250), groupBounds: bounds, …)
            == .center(groupID))
}

@Test func pointerOverTheTabStripAlwaysPicksCenterWithAnExactInsertionIndex() { }  // OB-D-3
@Test func theSoleTabOfAGroupCannotEdgeSplitIntoItsOwnGroup() {                    // OB-D-6, #3 rule 7
    #expect(DropTargetResolver.resolve(…, groupTabCount: 1, sourceGroup: groupID, …) == .center(groupID))
}
@Test func splitIsIneligibleWhenEitherHalfWouldFallBelowTwoFortyPoints() {         // CO-2
    #expect(SplitEligibility.check(groupSize: CGSize(width: 400, height: 300),
                                   placement: .right, isSoleTabOfSourceGroup: false)
            == .failure(.insufficientWidth(available: 197, required: 240)))
}
@Test func eligibilityReasonsAreExposedForAccessibilityAndControlCallers() { }
```

- [ ] **Step 2–4: fail → implement → pass** — `cd Packages/TillerWorkspace && swift test --filter DropTargetResolverTests`
- [ ] **Step 5: Commit** — `git commit -m "feat: resolve workspace drop targets and split eligibility\n\nrefs OB-D-3, OB-D-4, OB-D-6, CO-2"`

### Task 5.2: Drag session lifecycle and mutation-free cancellation

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/DragSession.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripView.swift` (created here)
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DragSessionTests.swift`

**Interfaces:**
- Produces:

```swift
@MainActor public final class DragSession {
    public private(set) var isActive: Bool
    public private(set) var grabOffset: CGPoint
    public private(set) var currentTarget: DropTarget
    public func pressBegan(tab: WorkspaceTabID, at: CGPoint, inTabFrame: CGRect)
    public func pointerMoved(to: CGPoint, hitTest: (CGPoint) -> DropTarget)
    public func cancel(reason: CancelReason)
    /// Returns the intent to send, or nil when the release is not a legal target.
    public func release() -> WorkspaceIntent?
    public enum CancelReason: Equatable, Sendable { case escape, pointerCancelled, focusLost, staleIdentity }
}
```

- [ ] **Step 1: Write the failing tests**

```swift
@Test func aPressUnderFourPointsActivatesInsteadOfDragging() {         // OB-D-1
    session.pressBegan(tab: t, at: CGPoint(x: 10, y: 10), inTabFrame: frame)
    session.pointerMoved(to: CGPoint(x: 13, y: 10), hitTest: hit)
    #expect(!session.isActive)
    #expect(session.release() == .activateTab(t))
}

@Test func theGrabOffsetIsPreservedForTheWholeDrag() { }               // OB-D-1
@Test func onlyTheHoveredGroupHasATarget() { }                         // OB-D-2
@Test func escapeCancelsWithoutEmittingAnyIntent() {                   // OB-D-7
    session.cancel(reason: .escape)
    #expect(session.release() == nil)
    #expect(session.currentTarget == .none)
}
@Test func focusLossAndPointerCancellationAreAlsoMutationFree() { }    // OB-D-7
@Test func releasingOutsideALegalTargetEmitsNothing() { }              // OB-D-7
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: add the mutation-free tab drag session\n\nrefs OB-D-1, OB-D-2, OB-D-7"`

### Task 5.3: Divider tracking and preferred-fraction commit

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/DividerTracking.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DividerTrackingTests.swift`

**Interfaces:**
- Produces: `DividerTracking.fraction(forPosition:total:thickness:) -> Double`, `DividerTracking.clamp(_:total:minimum:) -> Double`, and the rule that a pointer gesture emits **exactly one** `.setPreferredFraction` intent, on release.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func pointerTrackingEmitsExactlyOneIntentOnRelease() {           // OB-D-8
    tracking.began(split: s, total: 800)
    (0..<40).forEach { tracking.moved(to: CGFloat(300 + $0)) }
    #expect(sink.intents.isEmpty)
    tracking.ended()
    #expect(sink.intents == [.setPreferredFraction(s, 0.424)])
}

@Test func clampNeverWritesBackThePreferredValue() { }                 // CO-4
@Test func keyboardStepMovesFivePercentAndOptionStepOnePercent() { }   // AX-8
@Test func furtherInputAtTheMinimumAnnouncesOnceAndDoesNotMutate() { } // AX-8
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: track dividers live and commit one preferred fraction\n\nrefs OB-D-8, AX-8"`

### Task 5.4: Overflow canvas and tab-strip overflow

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/OverflowCanvas.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripView.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/OverflowTests.swift`

**Interfaces:**
- Produces: `OverflowCanvas.minimumCanvasSize(for layout: WorkspaceLayout) -> CGSize`, `OverflowCanvas.isOverflowing(canvas:viewport:) -> Bool`, `PaneTabStripView.overflowMenuItems(for: PaneGroup) -> [TabMenuEntry]`, `PaneTabStripView.scrollToReveal(tab:animated:)`.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func minimumCanvasIsTheSumOfPreferredGroupSizesPlusDividers() {  // CO-3
    let layout = Fixtures.deepMixedOrientationLayout(groups: 16, tabs: 64)
    let size = OverflowCanvas.minimumCanvasSize(for: layout)
    #expect(size.width >= 240 * 4 && size.height >= 160 * 4)
}

@Test func enteringOverflowDoesNotChangeAnyPreferredFraction() { }     // CO-4
@Test func leavingOverflowRestoresFittingFromTheSamePreferredFractions() { }  // CO-4
@Test func revealingAnOffscreenGroupScrollsTheMinimumDistance() { }    // CO-5
@Test func theOverflowMenuListsEveryLocalTabInStableOrder() { }        // CO-6
@Test func theTabStripKeepsTheActiveTabVisibleAfterActivation() { }    // CO-6
@Test func edgeAutoscrollTriggersOnlyWhileADragIsActive() { }          // CO-6
```

**Risk to check in this task:** Ghostty surfaces inside an `NSScrollView` may mis-report occlusion (`SurfaceVisibility.apply` walks the view tree). Add an assertion that a group scrolled out of the viewport still reports `setVisible(true)` from the reconciler's point of view — visibility is *mount* state, not scroll state — and note any Ghostty-specific finding in the commit body.

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "feat: keep a minimum-size canvas and scrolling tab strips in compact windows\n\nrefs CO-3, CO-4, CO-5, CO-6"`

---

# Phase 6 — Keyboard, focus, accessibility (AX-1…12, CO-7)

### Task 6.1: Spatial neighbour resolution

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/SpatialNeighbors.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/SpatialNeighborsTests.swift`

**Interfaces:**
- Produces: `SpatialNeighbors.neighbor(of: PaneGroupID, direction: FocusDirection, frames: [PaneGroupID: CGRect], readingOrder: [PaneGroupID]) -> PaneGroupID?` and `public enum FocusDirection { case left, right, up, down }`.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func neighborPrefersGreatestOrthogonalOverlapThenShortestEdgeDistance() { }  // AX-3
@Test func focusNeverWrapsAtTheOuterEdge() {                                       // AX-3
    #expect(SpatialNeighbors.neighbor(of: rightmost, direction: .right, …) == nil)
}
@Test func tiesBreakByReadingOrderDeterministically() { }                          // AX-3
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: resolve spatial pane neighbours without wrapping\n\nrefs AX-3"`

### Task 6.2: Accessibility hierarchy, actions, rotor, announcements

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceAccessibility.swift`
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceAnnouncements.swift`
- Modify: `PaneGroupController.swift`, `PaneTabStripView.swift`, `WorkspaceSplitController.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceAccessibilityTests.swift`

**Interfaces:**
- Produces: per-pane `accessibilityLabel` = `"Pane \(n) of \(m) — \(activeTabTitle)"`; tab actions `Activate`, `Move Earlier`, `Move Later`, `Move Tab To…`, `Close`; pane actions `Focus Pane`, `Split Right With…`, `Split Down With…`; divider label `"Vertical divider between Pane 1 and Pane 2, 50 percent"` with `Increment`/`Decrement`; rotor `Pane Groups` in reading order.

All user-facing strings are **English** (project rule), and every one lives in `WorkspaceAnnouncements` so the test can assert exact text.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func paneLabelsUseVisualPositionAndActiveTabTitle() {            // AX-5
    #expect(controller.accessibilityLabel() == "Pane 2 of 4 — Codex")
}
@Test func eachTabExposesExactlyTheFiveApprovedActions() { }           // AX-1, AX-5
@Test func theRotorListsPaneGroupsInLayoutReadingOrder() { }           // AX-6
@Test func dividerExposesOrientationAdjacentPanesAndPercentage() { }   // AX-8
@Test func aSuccessfulMoveAnnouncesDestinationAndSourceCollapse() {    // AX-9
    #expect(announcer.last == "Moved Codex to Pane 3, position 2. Pane 1 closed.")
}
@Test func anInvalidActionAnnouncesItsReasonOnce() { }                 // AX-9, CO-2
@Test func reducedMotionCommitsInstantlyWithAStaticHighlight() { }     // AX-10
@Test func increasedContrastAndReducedTransparencyChangeBoundariesNotSemantics() { } // AX-11, CO-7
```

- [ ] **Step 2–4: fail → implement → pass** — `cd Packages/TillerWorkspace && swift test --filter WorkspaceAccessibilityTests`
- [ ] **Step 5: Commit** — `git commit -m "feat: expose the pane-group accessibility contract\n\nrefs AX-1, AX-5, AX-6, AX-8, AX-9, AX-10, AX-11"`

### Task 6.3: Focus memory and the four focus systems

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceFocusCoordinator.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceFocusTests.swift`

**Interfaces:**
- Produces: `WorkspaceFocusCoordinator` holding, per `PaneGroupID`, the last operational focus and the last accessibility focus separately; `focusPane(_:)` restores each system's own memory, falling back to the content-kind default (terminal → terminal responder, chat → composer, document → editor).

- [ ] **Step 1: Write the failing tests**

```swift
@Test func eachPaneRestoresItsOwnRememberedOperationalFocus() { }      // AX-7
@Test func voiceOverActivationOfATabLeavesVoiceOverFocusOnTheTab() { } // AX-7, #10
@Test func withoutMemoryTheContentKindDefaultIsUsed() { }              // #4 focus contract
@Test func focusFulfillmentFailureIsReportedAndRetriedOnceAfterAttach() { } // FH-F-1
@Test func fullKeyboardAccessReachesEveryPaneControlWithoutAPointer() { }   // AX-12
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "feat: synchronize the four workspace focus systems\n\nrefs AX-7, AX-12, FH-F-1"`

---

# Phase 7 — Reduce `TillerTerminal` to one surface (LC-5, AX-13)

### Task 7.1: `TerminalSurfaceHost` keyed by `TerminalContentID`

**Files:**
- Create: `Packages/TillerTerminal/Sources/TillerTerminal/TerminalSurfaceHost.swift`
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/TerminalPaneCache.swift` (key by `TerminalContentID`)
- Test: `Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalSurfaceHostTests.swift`

**Interfaces:**
- Produces:

```swift
@MainActor public final class TerminalSurfaceHost {
    public let contentID: TerminalContentID
    public private(set) var generationID: ResourceGenerationID
    public var viewController: NSViewController { get }
    public init(contentID: TerminalContentID, configuration: TerminalSurfaceConfiguration)
    public func relaunch() -> ResourceGenerationID       // LC-5
    public func focusTerminal() -> Bool
    public func teardown() async                          // idempotent
}

public struct TerminalSurfaceConfiguration: Sendable { /* workingDirectory, command, env, callbacks */ }
```

`TerminalSplitHost` stays in place and untouched: this task adds the single-surface host beside it. No recursive topology in the new public surface.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func oneContentIdProducesOneStableController() {                 // #8, LC-5
    let host = TerminalSurfaceHost(contentID: id, configuration: config)
    #expect(ObjectIdentifier(host.viewController) == ObjectIdentifier(host.viewController))
}
@Test func relaunchMintsANewGenerationUnderTheSameContentId() { }      // LC-5
@Test func teardownIsIdempotent() { }                                  // LC-10
@Test func theSurfaceHostExposesNoSplitTreeApi() { }                   // compile-level assertion
@Test func terminalViewAccessibilityExposureIsUnchanged() { }          // AX-13
```

- [ ] **Step 2–4: fail → implement → pass** — `cd Packages/TillerTerminal && swift test --filter TerminalSurfaceHostTests` (run this package alone: PTY tests are timing-sensitive)
- [ ] **Step 5: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK` (retry `TillerTerminal` alone on PTY flake); `git commit -m "feat: add a single terminal surface host keyed by content id\n\nrefs LC-5, AX-13"`

---

# Phase 8 — Persistence: schema v16, durability gate, optimistic checkpoint, restore (PM-2…5, PM-11, FH-C-1, FH-O-*, FH-R-*)

### Task 8.1: Schema `v16` and records

**Files:**
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift`
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift`
- Test: `Packages/TillerPersistence/Tests/TillerPersistenceTests/WorkspaceSchemaTests.swift`

**Interfaces:**
- Produces (migration `v16`, one transaction):

```
workspaceLayout(worktreeId TEXT PK REFERENCES worktree ON DELETE CASCADE,
                schemaVersion INTEGER NOT NULL, revision INTEGER NOT NULL,
                payload TEXT NOT NULL, checksum TEXT NOT NULL, updatedAt DATETIME NOT NULL)

workspaceTab(id TEXT PK, worktreeId TEXT NOT NULL REFERENCES worktree ON DELETE CASCADE,
             title TEXT NOT NULL, titleIsAutoNamed BOOLEAN NOT NULL,
             contentKind TEXT NOT NULL, contentId TEXT NOT NULL,
             viewStateJSON TEXT, viewStateVersion INTEGER NOT NULL,
             createdAt DATETIME NOT NULL,
             UNIQUE(worktreeId, contentKind, contentId))          -- LC-2

terminalContent(id TEXT PK, worktreeId TEXT NOT NULL REFERENCES worktree ON DELETE CASCADE,
                launchKind TEXT NOT NULL,          -- 'shell' | 'agent'
                agentId TEXT, commandJSON TEXT, createdAt DATETIME NOT NULL)

workspaceLayoutQuarantine(id TEXT PK, worktreeId TEXT NOT NULL REFERENCES worktree ON DELETE CASCADE,
                          payload TEXT NOT NULL, reason TEXT NOT NULL, createdAt DATETIME NOT NULL)
```

Plus: `ALTER TABLE paneScrollback RENAME COLUMN paneId TO terminalContentId`, same for `agentSession` (values unchanged — D4), and `ALTER TABLE terminalTab RENAME TO legacyTerminalTab_v15`.

**Migration ordering trap:** the table rename must come **after** Task 9.1's data migration reads it. Split accordingly: `v16` creates the new tables and renames the scrollback/session columns; `v17` (Task 9.2) performs the data migration and then renames `terminalTab`.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func v16CreatesTheWorkspaceTablesWithTheOwnershipConstraint() throws {  // LC-2
    let db = try AppDatabase.inMemory()
    try db.write { try WorkspaceTabRecord(…contentId: "c1"…).insert($0) }
    #expect(throws: DatabaseError.self) {
        try db.write { try WorkspaceTabRecord(…contentId: "c1"…).insert($0) }
    }
}
@Test func scrollbackAndAgentSessionKeepTheirValuesUnderTheNewColumnName() throws { }  // D4
@Test func deletingAWorktreeCascadesEveryWorkspaceRow() throws { }            // PM-11, LC-12
@Test func migratingAV15DatabaseTwiceIsIdempotent() throws { }               // PM-10
```

- [ ] **Step 2–4: fail → implement → pass** — `cd Packages/TillerPersistence && swift test`
- [ ] **Step 5: Commit** — `git commit -m "feat: add the workspace layout schema (v16)\n\nrefs PM-11, LC-2, LC-12"`

### Task 8.2: The App-side persistence seam and its SQLite adapter

**Files:**
- Create: `App/Workspace/WorkspaceLayoutPersistence.swift`
- Create: `App/Workspace/SQLiteWorkspacePersistence.swift`
- Create: `AppTests/Workspace/FakeWorkspacePersistence.swift`
- Test: `AppTests/Workspace/SQLiteWorkspacePersistenceTests.swift`

**Interfaces:**
- Produces:

```swift
/// Durable terminal launch intent (#7). Runtime generations are never persisted.
struct TerminalContentRecordValue: Sendable, Equatable {
    let id: TerminalContentID
    let worktreeID: UUID
    let launchKind: LaunchKind          // .shell | .agent(id:)
    let commandJSON: String?
    enum LaunchKind: Equatable, Sendable { case shell, agent(id: String) }
}

struct RestoredWorkspace: Sendable {
    let layout: WorkspaceLayout
    let tabs: [WorkspaceTabID: WorkspaceTab]
    let revision: Int
    let diagnostics: [RestoreDiagnostic]      // drives the one non-blocking banner (#7)
}

enum RestoreDiagnostic: Equatable, Sendable {
    case quarantinedSnapshot(reason: String)
    case removedMissingTab(WorkspaceTabID)
    case repairedSelection(PaneGroupID)
    case unavailableContent(WorkspaceTabID)
    case importedRecoverySidecar(revision: Int)
    case futureSchemaVersion(Int)             // worktree becomes non-mutable
}

protocol WorkspaceLayoutPersistence: Sendable {
    func restore(worktreeID: UUID) async -> RestoredWorkspace
    /// Structural: must complete before the transition is published (#7).
    func commitStructural(worktreeID: UUID, revision: Int,
                          snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab],
                          terminalContents: [TerminalContentRecordValue]) async throws
    /// Optimistic: selections and fractions. Latest-wins, never blocks the UI.
    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async
    func flush(worktreeID: UUID) async throws
    func writeRecoverySidecar(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws
    func purge(worktreeID: UUID) async throws          // LC-12
}
```

- [ ] **Step 1: Write the failing tests**

```swift
@Test func structuralCommitWritesSnapshotAndTabsInOneTransaction() async throws { }   // PM-2
@Test func aFailedStructuralCommitLeavesTheStoredRevisionUnchanged() async throws { } // FH-C-1
@Test func checksumMismatchOnRestoreQuarantinesAndSalvages() async throws {           // FH-R-2
    let restored = await persistence.restore(worktreeID: id)
    #expect(restored.layout.groups.count == 1)
    #expect(restored.diagnostics.contains(.quarantinedSnapshot(reason: "checksum")))
    #expect(try quarantineCount(worktreeID: id) == 1)
}
@Test func oneMissingTabRowIsRemovedWithoutFallingBackToASinglePane() async throws { } // FH-R-3
@Test func quarantineKeepsAtMostThreePayloadsPerWorktree() async throws { }            // #7
@Test func anOlderRevisionCompletingLateNeverOverwritesANewerOne() async throws { }    // FH-O-2, PM-5
@Test func fullEnvelopeRoundTripsExactly() async throws { }                            // PM-2
```

- [ ] **Step 2–4: fail → implement → pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -skipPackagePluginValidation -only-testing:TillerTests/SQLiteWorkspacePersistenceTests` — confirm the `Test run with N tests` line is non-zero.

- [ ] **Step 5: Commit** — `git commit -m "feat: add the workspace persistence adapter with a durability gate\n\nrefs PM-2, FH-C-1, FH-R-2, FH-R-3, FH-O-2"`

### Task 8.3: Optimistic checkpoint policy, retry ladder, sidecar, quit flush

**Files:**
- Create: `App/Workspace/WorkspaceCheckpointQueue.swift`
- Create: `App/Workspace/WorkspaceRecoverySidecar.swift`
- Test: `AppTests/Workspace/WorkspaceCheckpointQueueTests.swift`

**Interfaces:**
- Produces: `WorkspaceCheckpointQueue` (actor) with `enqueue(revision:snapshot:)`, `flush()`, `var isDirty: Bool`, injected `clock` and `retrySchedule: [Duration] = [.seconds(1), .seconds(2), .seconds(4), .seconds(8), .seconds(16), .seconds(30)]`; `WorkspaceRecoverySidecar.write/read/quarantine` under `Application Support/Tiller/recovery/<worktreeId>.json`.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func selectionEnqueuesImmediatelyAndPointerReleasePersistsAtOnce() async { }   // #7
@Test func keyboardDividerStepsCoalesceOnATwoHundredFiftyMillisecondTrailingEdge() async { }  // PM-4
@Test func aFailedSaveMarksDirtyWithoutRevertingTheVisibleState() async { }          // FH-O-1
@Test func retriesFollowOneTwoFourEightSixteenThirty() async {                       // FH-O-1
    #expect(clock.sleeps == [.seconds(1), .seconds(2), .seconds(4),
                             .seconds(8), .seconds(16), .seconds(30)])
}
@Test func laterMutationsReplaceTheQueuedPayloadRatherThanQueueingBehindIt() async { }
@Test func failureAndRecoveryAreNotifiedOnceEach() async { }                         // #7
@Test func quitWaitsAtMostTwoSecondsThenWritesTheSidecar() async { }                 // PM-4
@Test func aSidecarIsImportedOnlyWhenHashValidAndRevisionNewer() async { }           // FH-R-5
```

Use a fake clock — no wall-clock sleeps in CI.

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "feat: checkpoint workspace preferences optimistically with a retry ladder\n\nrefs FH-O-1, FH-R-5, PM-4"`

---

# Phase 9 — Legacy v15 migration (PM-1, PM-6…10, PM-12, FH-R-6, VG-4)

### Task 9.1: Pure migration function over legacy values

**Files:**
- Create: `App/Workspace/WorkspaceMigrationV15.swift`
- Create: `AppTests/Fixtures/WorkspaceMigration/*.json` (legacy row dumps)
- Test: `AppTests/Workspace/WorkspaceMigrationV15Tests.swift`

**Interfaces:**
- Produces:

```swift
enum WorkspaceMigrationV15 {
    struct LegacyTabRow: Sendable, Equatable {         // mirrors legacyTerminalTab_v15
        let id: UUID; let worktreeId: UUID; let title: String; let orderIdx: Int
        let isActive: Bool; let treeJSON: String; let kind: String
        let filePath: String?; let chatAgentId: String?; let chatSessionId: String?
        let titleIsAutoNamed: Bool
    }
    struct MigrationResult: Sendable {
        let layout: WorkspaceLayout
        let tabs: [WorkspaceTab]
        let terminalContents: [TerminalContentRecordValue]
        let diagnostics: [RestoreDiagnostic]
    }
    /// Pure: no database, no filesystem. `now` and `freshID` are injected so
    /// fixtures assert exact IDs.
    static func migrate(rows: [LegacyTabRow], worktreeID: UUID,
                        freshTabID: () -> WorkspaceTabID,
                        freshGroupID: () -> PaneGroupID,
                        freshSplitID: () -> SplitID,
                        freshChatID: () -> ChatContentID,
                        agentDisplayName: (String) -> String?) -> MigrationResult
}
```

Rules implemented verbatim from #7: active terminal tab's `SplitTree` becomes the topology; otherwise a single root pane. First depth-first leaf = primary group and keeps the legacy tab ID + title + auto-name flag; other leaves get fresh tab IDs and titles (`agentDisplayName` ?? `Terminale N`). Every legacy leaf UUID becomes the `TerminalContentID`. All other legacy tabs land in the primary group in `orderIdx` order; inactive terminal trees expand into individual tabs in depth-first order at their former global position. Every migrated split starts at 0.5. Selection: first valid `isActive` by `orderIdx`; if terminal, its first depth-first leaf; else first recoverable tab; else empty root with `nil` active. Chat with a valid session keeps it as `ChatContentID`; chat with NULL session gets a fresh `ChatContentID`. Document paths are canonicalized within worktree scope; duplicate document identities keep the active tab, else the first by legacy order, and report removals. Corrupt rows are skipped and reported.

- [ ] **Step 1: Write the failing fixture tests — one per PM ID**

```swift
@Test func newWorktreeWithNoLegacyRowsProducesTheEmptyRootLayout() { }          // PM-1
@Test func validV15SinglePaneTerminalKeepsItsTabIdAndLeafUuid() {               // PM-6
    let result = WorkspaceMigrationV15.migrate(rows: [Fixtures.singleTerminalRow], …)
    #expect(result.layout.groups.count == 1)
    #expect(result.tabs[0].id.rawValue == Fixtures.singleTerminalRow.id)
    #expect(result.tabs[0].content == .terminal(TerminalContentID(Fixtures.leafUUID)))
}
@Test func nestedV15SplitTreeBecomesTheTopologyWithDepthFirstLeafIdentities() { } // PM-7
@Test func mixedLegacyTabsFlattenIntoThePrimaryGroupInGlobalOrder() { }          // PM-8
@Test func chatRowWithNullSessionGetsAFreshChatContentId() { }                   // PM-9
@Test func missingFilesStayAsValidTabsInTheMissingPlaceholderState() { }         // PM-9
@Test func duplicateDocumentIdentitiesKeepTheActiveTabAndReportTheRemoval() { }  // PM-9
@Test func corruptTreeJsonIsSkippedAndReported() { }                            // PM-10
@Test func migrationIsPureAndDeterministicForTheSameInjectedIds() { }           // FH-R-6
```

Each fixture declares its **exact** expected topology, IDs, selections, placeholder states, and diagnostics — "app launches" is not evidence (#9).

- [ ] **Step 2–4: fail → implement → pass**

Run: `xcodebuild test … -only-testing:TillerTests/WorkspaceMigrationV15Tests`

- [ ] **Step 5: Commit** — `git commit -m "feat: migrate v15 tab rows into the universal layout\n\nrefs PM-1, PM-6, PM-7, PM-8, PM-9, PM-10"`

### Task 9.2: Wire the migration into schema `v17` with backup and rollback

**Files:**
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift` (migration `v17`)
- Modify: `App/Workspace/SQLiteWorkspacePersistence.swift` (run-once migration entry point)
- Create: `Scripts/check-migration-fixtures.sh`
- Modify: `Scripts/ci.sh`
- Test: `AppTests/Workspace/WorkspaceMigrationIntegrationTests.swift`

- [ ] **Step 1: Write the failing tests**

```swift
@Test func migrationRunsInOneTransactionAndRollsBackCompletelyOnFailure() async throws { } // PM-10
@Test func afterMigrationTerminalTabIsRenamedAndNeverWrittenAgain() async throws { }       // PM-12
@Test func interruptingTheMigrationAtEveryWriteBoundaryLeavesTheV15DbIntact() async throws { } // FH-R-6
@Test func aTimestampedBackupExistsBeforeTheMigrationRuns() async throws { }
@Test func repeatedLaunchesDoNotMigrateTwice() async throws { }                            // PM-10
```

- [ ] **Step 2–4: fail → implement → pass**

- [ ] **Step 5: Add the fixture-integrity check to the gate**

`Scripts/check-migration-fixtures.sh` asserts every JSON under `AppTests/Fixtures/WorkspaceMigration/` parses and is referenced by at least one test file (catches orphaned or renamed fixtures). Call it from `Scripts/ci.sh`.

- [ ] **Step 6: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "feat: run the v15 workspace migration behind a backup and rollback\n\nrefs PM-10, PM-12, FH-R-6, VG-4"`

---

# Phase 10 — `WorkspaceCoordinator` and content adapters (OB-S-5, OB-S-7, LC-*, FH-B-*, FH-C-2)

### Task 10.1: The adapter protocol, registry, and three real adapters

**Files:**
- Create: `App/Workspace/WorkspaceContentAdapter.swift`
- Create: `App/Workspace/WorkspaceContentRegistry.swift`
- Create: `App/Workspace/TerminalContentAdapter.swift`
- Create: `App/Workspace/ChatContentAdapter.swift`
- Create: `App/Workspace/DocumentContentAdapter.swift`
- Create: `App/Workspace/WorkspaceHostAdapters.swift`
- Test: `AppTests/Workspace/WorkspaceContentAdapterTests.swift`

**Interfaces:**
- Produces:

```swift
struct PreparedContent: Sendable {
    let tab: WorkspaceTab                 // domain metadata the Core command needs
    let generationID: ResourceGenerationID
    let opaqueToken: AnyObject            // adapter-private; coordinator never inspects it
}

enum ContentPhase: Equatable, Sendable {
    case dormant, preparing, active, inactive, closing
    case failed(reason: String)
}
enum TerminalDetail: Equatable, Sendable { case running, exited(code: Int32) }
enum ChatDetail: Equatable, Sendable { case idle, turning, disconnected, interrupted }
enum DocumentDetail: Equatable, Sendable { case available, dirty, conflicted, missing }

@MainActor protocol WorkspaceContentAdapter: AnyObject {
    var kind: WorkspaceContentKind { get }
    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent
    func hydrate(tab: WorkspaceTab, worktree: Worktree) async
    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost
    func checkpoint(tab: WorkspaceTab) async
    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID
    func close(tab: WorkspaceTab) async        // idempotent, exactly-once effects
    func dispose(prepared: PreparedContent) async   // idempotent candidate disposal
}

@MainActor final class WorkspaceContentRegistry {
    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost?   // WorkspaceHostProvider
    func adopt(_ host: WorkspaceContentHost, tab: WorkspaceTab, generation: ResourceGenerationID)
    func release(tabID: WorkspaceTabID) async                        // only after durable commit
    func generation(for tabID: WorkspaceTabID) -> ResourceGenerationID?
    var liveHostCount: Int { get }                                   // PF-8 assertion hook
}
```

- [ ] **Step 1: Write the failing tests** (fake adapters + real ones through fakes at every async boundary)

```swift
@Test func aDetachedHostIsNotTornDown() async { }                       // LC-4
@Test func terminalHydratesWithTheMountedWorktreeEvenWhenItsTabIsInactive() async { }  // LC-3
@Test func chatAndDocumentHydrateOnlyOnFirstActivation() async { }      // LC-3
@Test func staleGenerationCallbacksAreIgnored() async { }               // LC-9
@Test func closeReleasesEachRuntimeExactlyOnce() async { }              // LC-10
@Test func retryMintsANewGenerationAndClearsTheFailedPhase() async { }  // LC-8
@Test func interruptedChatTurnsAreRetainedAndNeverReissued() async { }  // LC-6
@Test func missingDocumentsKeepTheirBufferAndRequireAnExplicitChoice() async { } // LC-7
@Test func disposalOfAPreparedCandidateIsIdempotent() async { }         // FH-B-5
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: add workspace content adapters and the host registry\n\nrefs LC-3, LC-4, LC-6, LC-7, LC-8, LC-9, LC-10"`

### Task 10.2: `WorkspaceCoordinator` two-phase commit

**Files:**
- Create: `App/Workspace/WorkspaceCoordinator.swift`
- Test: `AppTests/Workspace/WorkspaceCoordinatorTests.swift`

**Interfaces:**
- Produces:

```swift
@MainActor @Observable final class WorkspaceCoordinator {
    init(persistence: WorkspaceLayoutPersistence, registry: WorkspaceContentRegistry,
         adapters: [WorkspaceContentKind: WorkspaceContentAdapter])

    private(set) var layouts: [UUID: WorkspaceLayout]        // per worktree
    private(set) var revisions: [UUID: Int]
    private(set) var dirtyWorktreeIDs: Set<UUID>
    private(set) var lastRecoverableError: String?

    func restore(worktree: Worktree) async
    func handle(_ intent: WorkspaceIntent, in worktree: Worktree) async
    func requestSplit(anchor: PaneGroupID, placement: SplitPlacementSide,
                      choice: ContentChoice, in worktree: Worktree) async
    func closeTab(_ id: WorkspaceTabID, in worktree: Worktree) async
    func checkpointOnQuit() async                            // LC-11
    func purge(worktree: Worktree) async                     // LC-12
}

enum ContentChoice: Sendable {
    case newTerminal
    case agentTerminal(agentID: String)
    case newChat(agentID: String)
    case resumeChat(ChatContentID)
    case openFile(URL, editor: DocumentEditorKind)
    case moveExistingTab(WorkspaceTabID)
}
```

The flow is exactly the eight steps from #8: capture stable IDs + preconditions → prepare **outside** the per-worktree gate → enter the gate → re-resolve and reject stale preconditions → apply the Core command → durability-gate structural transitions → publish layout + delta → attach prepared resources or run idempotent teardown → fulfil the focus intent after host attachment. Per-worktree gates are independent actors.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func preparationRunsOutsideTheWorktreeGate() async { }               // #8
@Test func aStaleAnchorDisposesTheCandidateAndChangesNothing() async {     // FH-B-4, OB-S-7
    #expect(coordinator.layouts[wt.id] == layoutBefore)
    #expect(await persistence.commitCount == 0)
    #expect(await adapter.disposeCount == 1)
}
@Test func cancelledContentChoiceCreatesNoTabGroupSplitRowOrFocusChange() async { } // OB-S-7, FH-B-1
@Test func aFailedStructuralCommitLeavesUiOnTheLastDurableRevision() async { }      // FH-C-1, FH-C-2
@Test func resourcesArePublishedOnlyAfterTheDurableCommit() async { }               // LC-10
@Test func twoWorktreesCommitConcurrentlyWithIndependentGates() async { }           // #9 envelope
@Test func aFailedTeardownLeavesADurableCleanupObligationRetriedOnLaunch() async { } // LC-13
@Test func quitCheckpointsWithoutApplyingCloseSemantics() async { }                 // LC-11
@Test func containerRemovalPurgesTillerArtifactsButNeverSourceFiles() async { }     // LC-12
@Test func movingATabPreservesTabIdContentIdGenerationHostAndViewState() async { }  // LC-1
```

- [ ] **Step 2–4: fail → implement → pass**

Run: `xcodebuild test … -only-testing:TillerTests/WorkspaceCoordinatorTests`

- [ ] **Step 5: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "feat: add the two-phase workspace coordinator\n\nrefs OB-S-5, OB-S-7, LC-1, LC-11, LC-12, LC-13, FH-B-*, FH-C-2"`

---

# Phase 11 — Cutover in the App (OB-S-4, OB-D-10, AX-1, AX-2, AX-4, FH-C-3)

### Task 11.1: The gate flag (temporary, deleted in Phase 14)

**Files:**
- Create: `App/Workspace/WorkspaceEngineGate.swift`
- Test: `AppTests/Workspace/WorkspaceEngineGateTests.swift`

**Interfaces:**
- Produces: `enum WorkspaceEngineGate { static var isEnabled: Bool }` reading `UserDefaults.standard.bool(forKey: "workspace.universalEngine")` with `TILLER_UNIVERSAL_WORKSPACE=1/0` overriding it. Default **false** until Task 11.6.

- [ ] **Step 1: Write the failing test** (environment overrides the default; defaults are honoured otherwise)
- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "chore: add the temporary universal-workspace engine gate"`

### Task 11.2: Mount `TillerWorkspace` from `ContentView`

**Files:**
- Modify: `App/ContentView.swift` (`terminalStack` → `workspaceStack`)
- Modify: `App/TillerApp.swift` (construct and inject `WorkspaceCoordinator`)
- Test: `AppTests/Workspace/WorkspaceMountTests.swift`

**Behaviour change to state explicitly (D7):** today every tab of every open worktree is mounted with `opacity(0)`. Under the universal engine the renderer mounts one host per **active group** and the registry keeps non-mounted hosts alive. Detach is not close (LC-4) — the chat controller keeps living in the registry exactly as `AppModel.chatControllers` keeps it alive today.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func withTheGateOnTheWorkspaceRendererIsMountedInsteadOfTheTabStack() { }
@Test func withTheGateOffTheLegacyTerminalSplitHostStillRenders() { }
@Test func detachingAChatTabKeepsItsControllerAlive() { }                 // LC-4, D7
@Test func switchingWorktreesRegeneratesNoContent() { }                   // PF-6
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: mount the universal workspace renderer behind the engine gate\n\nrefs OB-D-10"`

### Task 11.3: The universal split content menu

**Files:**
- Create: `App/Workspace/SplitContentMenu.swift`
- Modify: `App/NewTabMenuItems.swift` (reused as inventory, per #4)
- Test: `AppTests/Workspace/SplitContentMenuTests.swift`

**Interfaces:**
- Produces the menu tree from #4 verbatim: `New Terminal` (first, keyboard-selected) / `Agent Terminal ›` / separator / `New Chat ›` / `Resume Chat ›` / `Open File…` / separator / `Move Existing Tab ›` grouped by `[This Pane]` then `[Other Panes, in layout order]`. Empty submenus show a disabled explanatory item; `Configure Agents…` opens Settings without creating a split.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func splitRightAlwaysOpensTheMenuBeforeAnyMutation() async { }        // OB-S-4
@Test func newTerminalIsTheFirstAndDefaultItem() { }                       // #4
@Test func moveExistingTabExcludesTheTabThatOpenedTheMenu() { }            // #4
@Test func moveExistingTabNeverListsOtherWorktrees() { }                   // #4
@Test func anEmptySubmenuShowsADisabledExplanatoryItem() { }               // #4
@Test func openingAnAlreadyOpenFileMovesItsExistingTab() async { }         // OB-S-5
@Test func anIneligibleSplitIsDisabledWithItsReason() { }                  // CO-2
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: add the universal split content menu\n\nrefs OB-S-4, OB-S-5, CO-2"`

### Task 11.4: `Tab` and `Pane` scene menus via `FocusedValues`

**Files:**
- Create: `App/Workspace/WorkspaceMenuCommands.swift`
- Modify: `App/TillerApp.swift` (install `.commands`)
- Test: `AppTests/Workspace/WorkspaceMenuCommandsTests.swift`

**Interfaces:**
- Produces the exact #10 command set and shortcuts: `Next Tab` ⌃⇥, `Previous Tab` ⌃⇧⇥, `Tab 1…9` ⌘1–9, `Last Tab`, `Move Earlier`/`Move Later` (no wrap, disabled at the ends), `Move Tab To…` (exact destinations incl. `New Pane Left/Right/Above/Below`), `Close Tab`; `Focus Pane Left/Right/Above/Below` ⌘⌥arrow, `Split Right With…` ⌘⌥⇧→, `Split Down With…` ⌘⌥⇧↓, `Focus Next/Previous Divider` (no shortcut). Commands target the focused workspace via `FocusedValues`, never `AppModel.selectedWorktree`.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func tabCommandsScopeToTheActivePaneGroupOnly() { }                  // #10
@Test func moveEarlierIsDisabledAtTheStartAndNeverWraps() { }              // #10
@Test func moveTabToPreviewsTheDestinationWithoutMutating() { }            // AX-4
@Test func escapeFromMoveTabToPreservesTopologyOwnershipSelectionFocus() { } // AX-4
@Test func aMoveThatCollapsesItsSourceSaysSoInItsLabel() { }               // #10
@Test func unmodifiedTabIsNeverIntercepted() { }                          // #10
@Test func commandsTargetTheFocusedWorkspaceNotTheSelectedWorktree() { }   // #8
@Test func thereIsNoClosePaneCommand() { }                                // #10 scope
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "feat: add the Tab and Pane scene menus\n\nrefs AX-1, AX-2, AX-4"`

### Task 11.5: Route every path through the coordinator

**Files:**
- Modify: `App/AppModel.swift` — delete `tabs`, `activeTabId`, `splitCurrent`, `split(paneId:…)`, `closePane`, `moveTab`, `closeTabsToRight`, `selectTab`, `persistTabs`, `paneCache`, `liveLeafIds`, `canAdoptPane`, `tabContaining`, `rollbackCreatedPane`, `rollbackCreatedLeaf`, `openFileTab`, `openChatSession` — replaced by facade calls that forward to `WorkspaceCoordinator`.
- Modify: `App/TabBarView.swift` — the global tab bar becomes the **active pane group's** local strip, or is removed if `PaneTabStripView` fully replaces it (decide by reading both; prefer removal — two tab strips is the "parallel representation" #9 forbids).
- Modify: `App/SidebarView.swift`, `App/AutoNaming/*`, `App/RightPanel/*` call sites.
- Test: `AppTests/Workspace/AppModelFacadeTests.swift`

- [ ] **Step 1: Write the failing tests**

```swift
@Test func appModelKeepsNoTabStateOfItsOwn() { }                          // OB-D-10, #8
@Test func autoRenameWritesThroughTheCoordinator() async { }
@Test func fileDropOnTheWorkspaceOpensOrMovesTheDocumentTab() async { }   // OB-S-5
@Test func everyMutationRouteReachesTheSameCoordinatorSeam() async { }    // OB-D-10
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "refactor: route every workspace mutation through the coordinator\n\nrefs OB-D-10"`

### Task 11.6: Flip the gate default and run the full matrix

- [ ] **Step 1: Flip the default** in `WorkspaceEngineGate` to `true`.
- [ ] **Step 2: Run the whole gate** — `Scripts/ci.sh` → `CI OK`.
- [ ] **Step 3: Manual smoke** on a real build: create each content kind through Split Right and Split Down, reorder, move, edge-split, cancel, collapse, quit and restore. Record results in `docs/superpowers/notes/2026-XX-XX-workspace-cutover-smoke.md`.
- [ ] **Step 4: Commit** — `git commit -m "feat: enable the universal workspace engine by default\n\nrefs OB-D-10"`

---

# Phase 12 — Control-socket compatibility (CT-1…6)

### Task 12.1: Resolve control IDs through the coordinator

**Files:**
- Modify: `App/AppModel.swift` (`handleControl` — panel.*, surface.*, session.*)
- Modify: `App/AppModel+Control.swift` (`handleCmuxControl`, `activePaneId(in:)`)
- Create: `App/Workspace/WorkspaceControlRouting.swift`
- Test: `AppTests/Workspace/WorkspaceControlRoutingTests.swift`, plus repairs to `AppTests/AppModelControlTests.swift`

**Interfaces:**
- Produces: `WorkspaceControlRouting.resolveTerminal(_ uuid: UUID, in worktree: Worktree) -> Result<(WorkspaceTabID, PaneGroupID), ControlError>` — the `TerminalContentID → WorkspaceTabID → PaneGroupID` resolution happens **at operation time**, never cached.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func panelUuidsStillMeanTerminalContentId() async { }               // CT-1
@Test func aMovedTerminalKeepsItsControlIdentityAndLivePty() async {      // CT-3
    let before = try await control("panel.read", ["panel": id.uuidString])
    await coordinator.handle(.requestMove(tab, to: .edgeSplit(anchor: g, placement: .right)), in: wt)
    let after = try await control("panel.read", ["panel": id.uuidString])
    #expect(after.isSuccess); #expect(ptyPid(before) == ptyPid(after))
}
@Test func controlSplitAwaitsTheSameDurabilityGateAsTheUi() async { }     // CT-4
@Test func staleOrNonTerminalIdsReturnTypedErrorsWithNoMutation() async { } // CT-5
@Test func paneGroupIdIsNeverSubstitutedIntoAnExistingResponse() async { } // CT-1
@Test func disablingTheSocketDisablesNoLocalWorkspaceFeature() async { }  // CT-6
@Test func layerAActivityOwnershipSurvivesAWorkspaceMove() async { }      // CT-6
```

- [ ] **Step 2–4: fail → implement → pass**

Run: `xcodebuild test … -only-testing:TillerTests/WorkspaceControlRoutingTests` and `… -only-testing:TillerTests/AppModelControlTests`

- [ ] **Step 5: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "feat: resolve control-socket terminal ids through the workspace coordinator\n\nrefs CT-1, CT-3, CT-4, CT-5, CT-6"`

---

# Phase 13 — Performance and resource budgets (PF-1…6, PF-8, PF-9)

### Task 13.1: Deterministic CI performance assertions

**Files:**
- Create: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspacePerformanceTests.swift`
- Create: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/ReconcilerPerformanceTests.swift`
- Create: `AppTests/Workspace/WorkspaceResourceLifecycleTests.swift`

**Interfaces:**
- Produces deterministic, host-independent assertions (these run in the normal gate): operation counts, allocation/identity counts, no main-thread I/O, and leak-free cycles. Wall-clock thresholds live in Task 13.2 and do **not** run in the normal gate.

- [ ] **Step 1: Write the failing tests**

```swift
@Test func reconcilingTheAcceptanceEnvelopePerformsNoContentRegeneration() { }  // PF-6
@Test func aLocalActivationRebuildsZeroSplitControllers() { }                   // PF-2 proxy
@Test func oneHundredCreateMoveRemountCloseRetryCyclesReturnCountsToBaseline() async {  // PF-8
    let baseline = registry.liveHostCount
    for _ in 0..<100 { … }
    await quiesce()
    #expect(registry.liveHostCount == baseline)
    #expect(await persistence.pendingCleanupObligations.isEmpty)
}
@Test func noSqliteWorkHappensOnTheMainActor() async { }                        // PF-9
@Test func restoreDecodesAndValidatesOffTheMainActor() async { }                // PF-9
```

- [ ] **Step 2–4: fail → implement → pass**
- [ ] **Step 5: Commit** — `git commit -m "test: assert deterministic workspace performance and leak budgets\n\nrefs PF-6, PF-8, PF-9"`

### Task 13.2: Signposted reference benchmark job

**Files:**
- Create: `Scripts/bench-workspace.sh`
- Create: `docs/superpowers/notes/workspace-performance-baseline.md`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift` (signpost intervals)

**Interfaces:**
- Produces: signpost intervals `workspaceCommandApply` (PF-1), `workspaceReconcile` (PF-2), `workspaceDragFrame` (PF-3), `workspaceStructuralCommit` (PF-4), `workspaceRestore` (PF-5), emitted through the existing `SignpostMetrics` helper in `TillerTerminal`. `bench-workspace.sh` runs an optimized build, 5 warm runs, reports the **median run's p95**.

**Measurement trap (recorded from prior work):** the signpost gate keys off the bundle id `dev.tiller.Tiller`, not the subsystem, and `log show` needs `--signpost`. A/B comparisons must be interleaved or drift falsifies the numbers.

- [ ] **Step 1: Add signposts and the script**
- [ ] **Step 2: Capture the baseline** on the documented reference Mac; write OS/build/hardware into the notes file.
- [ ] **Step 3: Verify each budget** — PF-1 ≤ 1 ms, PF-2 ≤ 8 ms, PF-3 ≤ 16.7 ms with no two consecutive > 33.3 ms, PF-4 ≤ 100 ms p95 / 250 ms p99, PF-5 ≤ 250 ms. Any miss is fixed here, not deferred.
- [ ] **Step 4: Run the phase gate and commit** — `Scripts/ci.sh` → `CI OK`; `git commit -m "perf: add the workspace signpost benchmark and baseline\n\nrefs PF-1, PF-2, PF-3, PF-4, PF-5"`

---

# Phase 14 — Delete the second layout engine (VG-2, VG-3, PM-12, MA-1…10)

### Task 14.1: Remove `TerminalSplitHost`, the legacy model, and the gate

**Files:**
- Delete: `Packages/TillerTerminal/Sources/TillerTerminal/SplitViewRenderer.swift`
- Delete: `Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalSplitHostFocusTests.swift` (its coverage now lives in `WorkspaceReconcilerTests`)
- Delete: `Packages/TillerCore/Sources/TillerCore/LegacyWorkspaceTab.swift`, `Packages/TillerCore/Sources/TillerCore/SplitTree.swift` (and their tests)
- Delete: `App/Workspace/WorkspaceEngineGate.swift` and every reference
- Modify: `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift` — drop `saveTabs`/`loadTabs`
- Modify: `Scripts/check-module-boundaries.sh` — add rule 4

- [ ] **Step 1: Write the failing gate rule**

Add to `Scripts/check-module-boundaries.sh`:

```bash
# 4. No legacy layout engine may ship.
for symbol in TerminalSplitHost SplitTree LegacyWorkspaceTab WorkspaceEngineGate; do
    if grep -rlE "\\b$symbol\\b" App Packages/*/Sources >/dev/null 2>&1; then
        report "legacy symbol $symbol is still referenced"
    fi
done
```

- [ ] **Step 2: Run it to verify it fails** — `bash Scripts/check-module-boundaries.sh` reports four violations.
- [ ] **Step 3: Delete the legacy path** and fix every resulting compile error.
- [ ] **Step 4: Run it to verify it passes** — `module boundaries OK`
- [ ] **Step 5: Run the full gate** — `Scripts/ci.sh` → `CI OK`
- [ ] **Step 6: Commit**

```bash
git commit -m "refactor: remove the legacy terminal split layout engine

The universal workspace is now the only layout engine in the product.

refs VG-3, PM-12"
```

### Task 14.2: Manual acceptance checklist

**Files:**
- Create: `docs/superpowers/notes/2026-XX-XX-workspace-manual-acceptance.md`

**Interfaces:**
- Produces a signed checklist recording OS / build / hardware, fixture, expected result, observed result, and evidence link for each of MA-1…MA-10, mapped one-to-one onto #9's "Manual acceptance scenarios" list:

1. create every content type through Split Right and Split Down (MA-1);
2. reorder locally, move centre, edge-split in every direction the nested layout implies, cancel every route, collapse a source group (MA-2);
3. resize nested dividers, shrink into two-axis overflow, reveal offscreen groups, recover normal fitting (MA-3);
4. move a running agent terminal with no PTY / session / control-ID change (MA-4);
5. move a chat with a draft and an active turn, and documents with caret / scroll / dirty recovery intact (MA-5);
6. inject preparation, commit, restore, focus, and post-commit cleanup failures and observe the specified recovery (MA-6);
7. quit/restore and migrate representative v15 fixtures (MA-7);
8. complete the full keyboard, Full Keyboard Access, and VoiceOver matrix (MA-8);
9. inspect Reduced Motion, Increased Contrast, Reduced Transparency, light/dark, and compact-window states (MA-9);
10. capture performance signposts and resource counts against the budgets (MA-10).

- [ ] **Step 1: Write the checklist template with all ten scenarios and their evidence slots**
- [ ] **Step 2: Execute it on a real macOS 15+ machine and fill in observed results**
- [ ] **Step 3: Run the final gate** — `Scripts/ci.sh` → `CI OK`
- [ ] **Step 4: Commit** — `git commit -m "docs: record the multi-pane workspace manual acceptance run\n\nrefs MA-1..MA-10"`

---

## Risk register

| Risk | Signal | Mitigation |
|------|--------|-----------|
| Ghostty surfaces misbehave inside the overflow scroll container | Blank or stale terminal after scrolling | Assertion in Task 5.4; if it fails, keep the scroll container but re-apply `SurfaceVisibility` on scroll-settle rather than abandoning CO-3 |
| `TillerTerminal` PTY tests flake and mask real breaks | `Scripts/ci.sh` fails only on `TillerTerminal` | Known issue; re-run that package alone before treating it as a regression |
| App test runs are slow (~13 min cold per suite) | Long phase-gate turnaround | Use `-only-testing:` per task; run the full gate only at phase boundaries |
| Phase 11 touches `AppModel.swift` (96 KB) broadly | Merge pain, review fatigue | Task 11.5 is the only task allowed to delete AppModel state; do it in one sitting on a dedicated branch |
| Migration diverges between the pure function and the SQLite wiring | Fixtures green, real launch wrong | Task 9.2's integration test drives the same fixtures through the real database, not the pure function |
| Two tab strips coexist after cutover | Duplicate ownership model (#9 forbids it) | Task 11.5 removes `TabBarView` as a global strip; the boundary checker rule 4 does not cover it, so review it explicitly |
