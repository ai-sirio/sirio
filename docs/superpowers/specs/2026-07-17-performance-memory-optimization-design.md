# Performance and Memory Optimization — Design

**Date:** 2026-07-17
**Status:** approved

## Executive Summary

Tiller's terminal pipeline, SwiftUI observation graph, and auxiliary services
were built for correctness first. Under the representative load of 10 worktrees
and 20 active panes, several hotspots produce avoidable CPU, memory, and
rendering overhead. This spec targets four areas:

1. **Terminal pipeline** — PTY read buffer, scrollback storage, content-signal
   extraction, and hidden-pane rendering.
2. **Background/UI coalescing** — filesystem and Git refresh, sidebar
   animations, agent-activity observation, and timer-driven polling.
3. **Empty-worktree lifecycle** — zero-pane worktrees that persist, restore,
   and consume negligible resources.
4. **Measurement** — `os_signpost` instrumentation for before/after comparison.

No agent or terminal is ever automatically terminated. No new dependencies
unless native and explicitly justified. Swift 6 / macOS 15+ only.
`Scripts/ci.sh` must print `CI OK`.

---

## Current-State Evidence

### Proven (measured or read from source)

| # | Hotspot | File | Lines | Cost |
|---|---------|------|-------|------|
| 1 | PTY read buffer | `PtyProcess.swift` | 159 | 64 KB `[UInt8]` allocated per read event. Each read on a busy pane allocates + zeroes 64 KB, then copies into `Data`. |
| 2 | Scrollback trim | `ScrollbackBuffer.swift` | 17 | `storage.removeFirst(storage.count - capacity)` is O(n) over the full 256 KB capacity — `Data`'s contiguous storage shifts all remaining bytes on every overflow. |
| 3 | Content-signal snapshot | `PtyTerminalPane.swift` | 219–226 | Every output settle: full 256 KB `snapshot()` → `String(decoding:as:)` → `stripANSI` → `split` → `suffix(40)` → `joined`. Only the last ~40 lines are used; the other ~255 KB are decoded, stripped, and discarded. |
| 4 | Right-panel refresh | `RightPanelModel.swift` | 199–223 | On every filesystem event: reloads *all* loaded directories, then runs `GitStatus.load` (spawns `git status`). A burst of FS events (agent writing files) triggers repeated Git process spawns. |
| 5 | UsageStore timer | `UsageStore.swift` | 41–51 | `Task` loop remains alive even when all four providers are disabled (`showInBar = false`). `refreshAll()` checks `showInBar` but the timer still fires and the task is never cancelled. |
| 6 | Sidebar animations | `SidebarView.swift` | 73–75 | Three `.animation(.easeInOut(duration: 0.18))` modifiers on `expandedProjectIds`, `worktrees`, and `tabs`. These trigger collection-wide transitions on every structural change — visible as a 180 ms hitch on every tab open/close. |
| 7 | closeTab replacement | `AppModel.swift` | 639, 649–654 | Closing the last tab creates a replacement shell tab unconditionally. The worktree can never be empty. |
| 8 | ensureTabs on selection | `AppModel.swift` | 32, 598–608 | `selectedWorktree.didSet` calls `ensureTabs`, which creates a shell tab if `tabs[id]` is empty. Selecting a worktree with no tabs always spawns a PTY. |
| 9 | bootstrap empty-tab filter | `AppModel.swift` | 212, 225 | `guard !restoredTabs.isEmpty` skips worktrees with zero restored tabs; `openWorktreeIds` filters out worktrees with empty tab lists. Empty worktrees are never mounted. |

### Hypotheses (to be confirmed by instrumentation)

| # | Suspect | Rationale |
|---|---------|-----------|
| H1 | `AgentActivityModel` observation cascades | `@Observable` on a `final class` with four dictionaries and two `Set`s. Every status change triggers re-evaluation of every `WorktreeRow` and `PaneRow` that reads `agentStatus` or `paneAgents`. Under 20 panes with active agents, this may produce redundant body recomputations. |
| H2 | `SidebarView` nested `ForEach` | Four levels of `ForEach` (projects → worktrees → tabs → panes) inside a `LazyVStack`. Each level reads `model.worktrees`, `model.tabs`, `model.agentActivity.*`. A single property change at any level re-evaluates the entire chain. |
| H3 | `ContentView.terminalStack` opacity rendering | All non-selected worktrees and non-active tabs are rendered with `opacity(0)` and `allowsHitTesting(false)` but their `TerminalSplitHost` (NSViewControllerRepresentable) is still alive and receiving libghostty surface updates. Hidden panes' rendering cost is unknown. |
| H4 | `ForegroundProcessAgent` on content signals | `handleContentSignal` calls `checkForegroundAgent` for unregistered panes, which calls `proc_listchildpids`/`proc_name` off-main. Under heavy output, this runs on every settle (~200 ms). Cost per call is microseconds, but frequency × pane count may add up. |

---

## Goals and Non-Goals

### Goals

1. **Idle CPU < 1%** on reference Mac (M-series, 16 GB) with 10 worktrees / 20
   panes, excluding brief system spikes (sub-second).
2. **≥ 20% terminal-pipeline CPU-time reduction** under heavy continuous output
   (e.g., `git clone`, `npm install`, `make`).
3. **No main-thread stalls > 100 ms** during sidebar or worktree switching.
4. **No file tree or Git refresh for invisible worktrees** — one coalesced
   refresh on activation.
5. **≥ 15% lower resident RAM** in the representative scenario.
6. **No progressive memory growth** after repeated pane open/close cycles.
7. **No lost bytes** — hidden panes show all accumulated output on reveal.
8. **Empty worktree restores correctly** — zero-pane worktrees persist, restore,
   and show empty state.

### Non-Goals

- Reducing libghostty's own memory footprint (documented separately).
- Reducing per-pane PTY process memory (shell + agent).
- Reducing SwiftUI framework overhead outside the identified hotspots.
- Replacing `LazyVStack` with `UICollectionView` or other AppKit bridges.
- Syntax highlighting, font rendering, or terminal-surface GPU cost.
- Agent-launch latency or agent-process memory.

---

## Invariants

1. **Never automatically terminate active agents or terminals.** The user's
   agents and shells are never killed by Tiller. Only explicit user action
   (close tab, close worktree, remove project) or the opt-in
   `maxMountedWorktrees` cap (evicts only idle worktrees — never the selected
   worktree, one with a running or input-waiting agent, or one with unsaved
   tabs) may terminate a PTY.
2. **Hidden panes continue execution.** A pane whose worktree is not selected,
   or whose tab is not active, continues to receive PTY output, accumulate
   scrollback, and run agent detection. Only rendering is throttled/coalesced.
3. **Full synchronization on visibility.** When a hidden pane becomes visible,
   its scrollback, agent state, and terminal surface must reflect all output
   produced while hidden.
4. **Empty worktrees have no PTY.** A worktree with zero tabs has no
   `PtyProcess`, no `ScrollbackBuffer`, no `TerminalPaneCache`, and no
   `PaneRegistry` entry. Its resource cost is bounded by the model-layer
   entries it owns (empty tab array, nil activeTabId, UUID in
   `openWorktreeIds`). Right-panel and FSEvents state is owned by
   `RightPanelModel` and is separate — it must be measured independently.
5. **No data loss.** Closing the final tab of a worktree must not discard
   scrollback or agent state. The empty worktree persists and can receive a new
   terminal.

---

## Component Design

### A — Measurement (`os_signpost`)

Add `os_signpost` intervals around these operations, gated by a compile-time
flag or a runtime `UserDefaults` key (`debug.signpostMetrics`). No terminal
contents, sensitive paths, or commands in metric payloads.

| Point | Signpost | What it measures |
|-------|----------|------------------|
| PTY read → scrollback append | `ptyIngest` | Time from `read()` return to `ScrollbackBuffer.append` completion |
| Content-signal extraction | `contentSignal` | `snapshot()` + decode + strip + split + suffix |
| Agent state detection (all layers) | `agentDetect` | Title parse + content match + foreground process probe |
| Right-panel FS refresh | `panelRefresh` | Directory reload + Git status spawn |
| Sidebar body evaluation | `sidebarBody` | `SidebarView.body` re-evaluation (SwiftUI `body` property) |
| Worktree switch | `worktreeSwitch` | `selectedWorktree` didSet → ensureTabs → eviction → view rebuild |

Implementation sketch:

```swift
// In a shared utility, gated by UserDefaults:
let signposter = OSSignposter(subsystem: "dev.tiller", category: "performance")

// Usage:
let state = signposter.beginInterval("ptyIngest", id: signposter.makeSignpostID())
defer { signposter.endInterval("ptyIngest", state) }
```

The flag defaults to off. Enable via `defaults write dev.tiller
debug.signpostMetrics -bool YES` and inspect with `oslog` / Instruments.

**Verification:** Signposts are disabled by default (gated on
`debug.signpostMetrics == false`). They emit no sensitive content (no terminal
output, paths, or commands in payloads). The code compiles and passes
`Scripts/ci.sh` both with the default (off) state and, if feasible, with a
test-controlled enable path.

### B — Terminal Pipeline

#### B1 — Right-sized reusable PTY read buffer

**Current:** `PtyProcess.swift:159` — 64 KB `[UInt8]` allocated and zeroed on
every read event.

**Change:** Replace the per-event array with a reusable stack buffer. The
`DispatchSourceRead` handler already runs on a serial queue, so a single
reusable buffer is safe.

```swift
// PtyProcess gains a stored property:
private var readBuffer = [UInt8](repeating: 0, count: 64 * 1024)

// startReadLoop reuses it:
let n = read(self.masterFD, &readBuffer, readBuffer.count)
if n > 0 {
    self.onOutput(Data(readBuffer[0..<n]))
}
```

The buffer is allocated once per `PtyProcess` instance and never re-allocated.
`Data(readBuffer[0..<n])` still copies, but avoids the per-event allocation +
zeroing of a fresh `[UInt8]`.

**Verification:** `os_signpost` `ptyIngest` interval shows reduced allocation
count in Instruments (Allocations template). No change in correctness — the
buffer is consumed synchronously on the serial queue.

#### B2 — Ring buffer for auxiliary scrollback

**Current:** `ScrollbackBuffer.swift:17` — `Data.removeFirst` shifts all
remaining bytes on every overflow.

**Change:** Replace `Data` storage with a ring buffer of fixed-size chunks.
The public API remains `actor ScrollbackBuffer { append(_:), snapshot() }`.

Design:

```swift
public actor ScrollbackBuffer {
    private let capacity: Int
    private let chunkSize: Int  // e.g., 4096
    private var chunks: [Data] = []
    private var headOffset: Int = 0  // bytes consumed from first chunk
    private var totalBytes: Int = 0

    public func append(_ data: Data) {
        // Append data, splitting into chunkSize pieces.
        // When totalBytes > capacity, drop oldest chunks.
    }

    public func snapshot() -> Data {
        // Concatenate chunks [headOffset...] into a single Data.
        // This is O(total bytes) but called only on demand.
    }

    /// Extract the last N bytes without copying the full buffer.
    public func tail(_ maxBytes: Int) -> Data {
        // Walk chunks from the end, accumulating up to maxBytes.
        // Returns a single Data of at most maxBytes.
    }
}
```

The `tail(_:)` method is the key addition: it extracts only the bytes needed
for content-signal matching, avoiding the full-buffer copy + decode that
`emitContentSignal` currently does.

**`tail(_:)` contract:**
- **Input:** `maxBytes: Int` (> 0).
- **Output:** A single `Data` containing the last `min(maxBytes, totalBytes)`
  bytes in append order (oldest-to-newest within the returned window).
- **Capacity/eviction:** At most `capacity` bytes are retained. When
  `totalBytes > capacity`, the oldest chunks are dropped deterministically.
  Bytes are never lost except via oldest-byte eviction at capacity.
- **Complexity target:** O(number of chunks traversed), which is at most
  `ceil(capacity / chunkSize)` in the worst case, and typically O(1) for
  small `maxBytes` relative to capacity.

**Verification:** `os_signpost` `contentSignal` interval shows reduced
duration. Memory: the ring buffer uses at most `capacity + chunkSize` bytes
(no fragmentation). No change to `snapshot()` semantics for scrollback
persistence.

#### B3 — Direct tail extraction in content signal

**Current:** `PtyTerminalPane.swift:219–226` — copies full 256 KB, decodes to
`String`, strips ANSI, splits, takes suffix(40), joins.

**Change:** Use `ScrollbackBuffer.tail(10 * 1024)` (10 KB is generous for 40
lines), then decode + strip + split only that tail.

```swift
private func emitContentSignal() async {
    let tailData = await scrollback.tail(10 * 1024)  // ~10 KB
    let text = stripANSI(String(decoding: tailData, as: UTF8.self))
    let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
    let tail = lines.suffix(40).joined(separator: "\n")
    guard !tail.isEmpty else { return }
    onContentSignal?(paneId, tail)
}
```

**Verification:** `os_signpost` `contentSignal` interval. Expected: ~96%
reduction in bytes decoded per settle (10 KB vs 256 KB).

#### B4 — Coalesced rendering for hidden panes (investigation spike)

**Current:** `ContentView.swift:287` — hidden panes (`opacity(0)`) still
receive libghostty surface updates via `TerminalSplitHost`.

**Investigation scope:** Determine whether libghostty exposes a documented,
tested API to suspend/resume rendering for a `TerminalSurface` without data
loss. The following are **not** acceptable outcomes:

- Using an undocumented or untested internal API.
- Disconnecting and reconnecting the surface (safety and no-loss properties
  are unproven).
- Setting `alphaValue = 0` on the NSView (this is not an optimization — CA
  rendering is not the dominant cost, and the surface continues to process
  input).

**Acceptable outcomes:**
1. A proven, documented, tested no-loss API exists → implement suspend/resume
   with full-surface refresh on resume.
2. No such API exists → retain current rendering behavior. No rendering-lifecycle
   change is made.

**Gate for B4:** B4 cannot ship unless byte-for-byte / full-screen
resynchronization verification passes (see AC7 below). If verification fails,
retain current rendering behavior.

**Process execution, PTY byte capture, scrollback, content signal, and agent
detection remain fully active regardless of the rendering state.**

**Verification:** Instruments GPU template. Expected: reduced Metal render
passes for hidden surfaces (if suspend/resume is viable).

### C — Background and UI Coalescing

#### C1 — Inactive dirty marker

**Current:** `RightPanelModel.swift:199–223` — every FS event reloads all
loaded directories and runs `git status`.

**Change:** When the right panel is hidden, or the worktree is not selected,
mark the worktree as "dirty" instead of performing refresh. On activation
(panel shown or worktree selected), perform one coalesced refresh.

The inactive-selected policy applies whether the worktree has zero or many
panes. Events mark a per-worktree dirty bit. The existing 250 ms debounce
remains for visible worktrees only. Inactive worktrees do no refresh.

On activation, the dirty bit is atomically consumed and triggers exactly one
refresh cycle covering both file-tree and Git status. If additional FS events
arrive during the refresh, they set the dirty bit again, resulting in at most
one follow-up cycle after the first completes (race behavior: the follow-up
cycle is triggered by the next activation or, if the panel remains visible,
by the next debounce window).

Empty worktrees (zero panes) follow the same rule: their right-panel state
is separate and follows the same dirty/refresh lifecycle.

```swift
// RightPanelModel gains:
private var dirtyWorktrees: Set<UUID> = []

func markDirty(worktreeId: UUID) {
    dirtyWorktrees.insert(worktreeId)
}

func activate(worktree: Worktree?, isGitRepository: Bool) async {
    // ... existing setup ...
    if dirtyWorktrees.remove(worktree.id) != nil {
        await refresh(changedPaths: [], token: token, forceAllLoadedDirectories: true)
    }
}
```

The FSEvent monitor continues to run (it is cheap — kernel pushes events), but
the handler only marks dirty instead of performing I/O.

**Unclassifiable FS event rule:** If the event monitor cannot determine which
paths changed (e.g., a bulk event with no path list), mark the worktree dirty.
This is safe: the next activation performs a full refresh.

**Verification:** `os_signpost` `panelRefresh` interval. Expected: zero
`git status` spawns while the panel is hidden.

#### C2 — Refresh on visibility

**Current:** `ContentView.swift:62–70` — `RightPanelModel.activate` is called
via `.task(id: rightPanelContext)`, which fires on every context change.

**Change:** The `.task(id:)` pattern already cancels and re-creates the task
when the context changes. The dirty-marker pattern (C1) adds new behavior:
activation triggers a full refresh only when the worktree was dirtied while
hidden. This is a behavioral change (previously every activation reloaded
everything) and requires tests.

**Verification:** `RightPanelModelTests`: activation with dirty flag performs
refresh; activation without dirty flag does not.

#### C3 — Local sidebar animations

**Current:** `SidebarView.swift:73–75` — three collection-wide
`.animation(.easeInOut(duration: 0.18))` modifiers.

**Change:** Remove the collection-wide animations. Keep only the essential
per-row transition:

```swift
// Before:
.animation(.easeInOut(duration: 0.18), value: model.expandedProjectIds)
.animation(.easeInOut(duration: 0.18), value: model.worktrees.mapValues { $0.map(\.id) })
.animation(.easeInOut(duration: 0.18), value: model.tabs.mapValues { $0.map(\.id) })

// After: remove all three. The LazyVStack already handles insert/remove
// with the default transition. If a subtle animation is desired, apply
// .transition(.opacity) on individual rows instead.
```

If the product owner wants to keep some animation, apply it only to the
affected row (e.g., `ProjectRow` expand/collapse) rather than the entire
collection.

**Verification:** Visual inspection — no visible hitch on tab open/close.
Instruments SwiftUI template shows reduced body evaluation time.

#### C4 — Compute row status once per render pass

**Current:** `SidebarView.swift:28` — `AttentionSort.sorted(...)` is called
inside the `ForEach`, which re-evaluates on every `model.worktrees` or
`model.agentActivity` change.

**Change:** Lift the sorted list into a computed property on `SidebarView`:

```swift
private var sortedWorktrees: [Worktree] {
    guard let project = ... else { return [] }
    return AttentionSort.sorted(
        model.worktrees[project.id] ?? [],
        statusOf: model.statusForWorktree
    )
}
```

This is already a computed property on the view struct, so SwiftUI's
dependency tracking applies. The key is to ensure `statusForWorktree` reads
`agentActivity.agentStatus` through a single access point so SwiftUI can
narrow the dependency.

**Verification:** Instruments SwiftUI template. Expected: fewer body
evaluations for `WorktreeRow` when an unrelated pane's status changes.

#### C5 — Stop timer when all providers disabled

**Current:** `UsageStore.swift:41–51` — timer loop runs regardless of
`showInBar` state.

**Change:** Gate the timer start/stop on the combined visibility of all four
providers. The `@AppStorage` properties live in `ContentView` and
`UsageBarView`; the timer is owned by `UsageStore`. Use `.onChange(of:)` in
`ContentView` for each provider-enabled key, calling an idempotent
`UsageStore.updatePolling(enabledProviders:)` method. The timer/task is absent
when all providers are disabled and is owned/cancelled explicitly.

```swift
// UsageStore gains:
func updatePolling(enabledProviders: Set<ProviderKind>) {
    let anyEnabled = !enabledProviders.isEmpty
    if anyEnabled {
        start()
    } else {
        stop()
    }
}

func stop() {
    timer?.cancel()
    timer = nil
}
```

In `ContentView`:

```swift
.onChange(of: showClaudeInBar) { _, _ in
    model.usage.updatePolling(enabledProviders: computeEnabledProviders())
}
.onChange(of: showCodexInBar) { _, _ in
    model.usage.updatePolling(enabledProviders: computeEnabledProviders())
}
// ... same for opencodeGo and ollamaCloud
```

Where `computeEnabledProviders()` reads the four `@AppStorage` values and
returns the set of enabled provider kinds. The `refreshAll()` method already
checks `showInBar` per-provider, so the timer firing with all disabled is
harmless but wastes a task wake-up every N seconds.

**Verification:** Instruments Time Profiler. Expected: zero `UsageStore`
timer wake-ups when all providers are disabled.

### D — Empty Worktree Lifecycle

#### D1 — Zero-pane worktrees

**Current:** `AppModel.swift:639, 649–654` — `closeTab` creates a replacement
shell tab when the last tab is closed. `ensureTabs` (called from
`selectedWorktree.didSet` at line 32) creates a shell tab if `tabs[id]` is
empty.

**Change:** Remove the automatic replacement-tab creation from `closeTab`.
Allow `tabs[worktree.id]` to be `[]` and `activeTabId[worktree.id]` to be
`nil`.

```swift
func closeTab(_ tabId: UUID, in worktree: Worktree) {
    // ... existing cleanup ...
    list.removeAll { $0.id == tabId }
    teardownMarkdownDocument(tabId: tabId)
    // REMOVED: if list.isEmpty { create replacement tab }
    tabs[worktree.id] = list
    if !list.contains(where: { $0.id == activeTabId[worktree.id] }) {
        activeTabId[worktree.id] = list.last?.id  // nil when list is empty
    }
    persistTabs(for: worktree.id)
}
```

Change `ensureTabs` to a no-op when tabs are empty:

```swift
func ensureTabs(for worktree: Worktree) {
    // No-op: empty worktrees are valid.
    // Tabs are created explicitly by newShellTab, spawnAgent, or panel.create.
}
```

**Impact analysis:**

| Caller | Current behavior | After change |
|--------|-----------------|--------------|
| `selectedWorktree.didSet` (line 32) | Calls `ensureTabs` → creates shell tab | No-op for empty worktrees |
| `bootstrap()` (line 212) | `guard !restoredTabs.isEmpty` skips empty worktrees | Must NOT skip — empty worktrees should be mounted |
| `bootstrap()` (line 225) | `openWorktreeIds.filter { !(tabs[$0] ?? []).isEmpty }` | Must include worktrees with empty tab lists |
| `closeTab` (line 649) | Creates replacement tab | Leaves worktree empty |
| `activeTab(for:)` (line 610) | Returns `nil` for empty list | Already safe — returns `nil` |
| `ContentView.terminalStack` (line 237) | `ForEach(model.tabs[worktreeId] ?? [])` | Empty `ForEach` renders nothing — already safe |
| `persistTabs` (line 783) | Saves `[]` and `nil` activeTabId | Already handles empty list |
| `paneCaches` prune (line 786) | `prune(keeping: empty set)` → removes all | Correct — no panes to keep |

#### D2 — Empty state UI

**Current:** `ContentView.swift:227` — shows "No worktree selected" when
`openWorktreeIds` is empty.

**Change:** Add an empty state for a selected worktree with no tabs:

```swift
// In terminalStack, after the existing empty-worktree check:
if model.openWorktreeIds.isEmpty {
    ContentUnavailableView("No worktree selected", ...)
} else if let worktree = model.selectedWorktree,
          let tabs = model.tabs[worktree.id], tabs.isEmpty {
    EmptyWorktreeView(worktree: worktree, onNewTerminal: {
        model.newShellTab(in: worktree)
    })
}
```

`EmptyWorktreeView` is a minimal view with:
- Worktree name and branch
- "New Terminal" button (calls `model.newShellTab(in:)`)
- Keyboard shortcut hint (the existing New Terminal command is `⌘T`,
  verified in `TillerApp.swift:45`)

The empty state must not auto-create a pane. The user must explicitly request
a new terminal.

#### D3 — Persistence and restore

**Current:** `bootstrap()` at line 212 skips worktrees with zero restored
tabs, and line 225 filters them from `openWorktreeIds`.

**Change:**

```swift
// bootstrap, after loading tabs for a worktree:
tabs[worktree.id] = restoredTabs  // may be empty
activeTabId[worktree.id] = loaded.activeTabId.flatMap { active in
    restoredTabs.contains { $0.id == active } ? active : nil
} ?? restoredTabs.first?.id  // nil when empty
// Remove the `guard !restoredTabs.isEmpty else { continue }` guard.

// openWorktreeIds filter:
openWorktreeIds = storedOpenIds.filter { id in
    worktree(byId: id) != nil  // keep all valid worktrees, even with empty tabs
}
```

`PersistTabs` already handles `[]` and `nil` activeTabId. The pane-cache
prune in `persistTabs` (line 786) with an empty `liveLeafIds` set correctly
removes all cached controllers.

#### D4 — Resource cost of empty mounted worktree

An empty mounted worktree has:
- One entry in `tabs[UUID: [WorkspaceTab]]` — an empty array
- One entry in `activeTabId[UUID: UUID]` — `nil`
- One entry in `openWorktreeIds` — a UUID (16 bytes)
- No `PtyProcess`, no `ScrollbackBuffer`, no `TerminalPaneCache`, no
  `PaneRegistry` entry, no `TerminalSplitHost`, no `NSViewController`

Right-panel and FSEvents state is owned by `RightPanelModel` and is separate
— it must be measured independently. The monitor lifecycle is: the
`RightPanelModel` owns a `FileSystemEventMonitor` that is started on
`activate()` and stopped on `deactivate()`. An empty worktree that is not
selected has no active `RightPanelModel` monitor.

**Persistence verification (Phase-3 precondition):** Confirm that
`persistTabs` consumes only `snapshot()` for scrollback data. If this is not
yet proven by code inspection, mark it as a Phase-3 precondition rather than
a claim. `snapshot()` is defined as the logical oldest-to-newest concatenation
of all retained bytes after wrap.

---

## Data Flows

### PTY output (after optimization)

```
PtyProcess.read (reusable buffer)
  → ScrollbackBuffer.append (ring buffer, O(1) amortized)
  → libghostty InMemoryTerminalSession.receive (thread-safe)
  → [if pane visible] libghostty renders surface
  → [on settle] emitContentSignal:
      ScrollbackBuffer.tail(10 KB)
      → stripANSI + decode + split + suffix(40)
      → onContentSignal callback
```

### Worktree switch (after optimization)

```
User clicks worktree in sidebar
  → selectedWorktree.didSet
  → ensureTabs (no-op for empty)
  → evictIdleWorktreesIfNeeded
  → ContentView.terminalStack re-evaluates
  → RightPanelModel.activate (if panel visible)
      → if dirty: full refresh (FS + Git)
      → else: no-op
  → ForegroundProcessAgent resumes for visible panes
```

### Filesystem event (after optimization)

```
FSEvent fires
  → RightPanelModel.handleEvent
  → if panel visible and worktree selected:
      debounce 250 ms → refresh changed directories + git status
  → else:
      mark worktree dirty
```

---

## Measurement Methodology

### Baseline

1. Launch Tiller with 10 worktrees, 20 panes (2 per worktree).
2. Let all panes idle for 60 seconds.
3. Record: CPU (Activity Monitor / `top`), resident RAM, `os_signpost`
   metrics.
4. Run heavy output in 4 panes simultaneously (`git clone` of a large repo,
   `npm install`).
5. Record same metrics during and after.
6. Switch between worktrees rapidly (5 switches in 2 seconds).
7. Record main-thread stall duration (Instruments Time Profiler, HID template).

### After each change

Repeat the same scenario. Compare:

| Metric | Baseline | Target |
|--------|----------|--------|
| Idle CPU (10 wt / 20 panes) | Captured in Phase 2 before optimization | < 1% |
| Terminal pipeline CPU (heavy output) | Captured in Phase 2 before optimization | ≥ 20% reduction |
| Main-thread stall (worktree switch) | Captured in Phase 2 before optimization | < 100 ms |
| Resident RAM | Captured in Phase 2 before optimization | ≥ 15% reduction |
| RAM after 10× pane open/close | Captured in Phase 2 before optimization | No growth |

Percentage acceptance gates are evaluated only after the Phase-2 baseline is
recorded on a documented reference Mac configuration and workload.

### Instrumentation

All `os_signpost` points are gated by `UserDefaults.standard.bool(forKey:
"debug.signpostMetrics")`. Default: off. Enable via:

```bash
defaults write dev.tiller debug.signpostMetrics -bool YES
# Restart Tiller, profile with Instruments (os_signpost template)
```

---

## Acceptance Criteria

| # | Criterion | Verification |
|---|-----------|-------------|
| AC1 | Idle CPU < 1% on reference Mac (10 wt / 20 panes) | `top` / Activity Monitor, 60 s sample |
| AC2 | ≥ 20% terminal-pipeline CPU reduction under heavy output | `os_signpost` `ptyIngest` + `contentSignal` intervals |
| AC3 | No main-thread stalls > 100 ms during worktree switch | Instruments HID template, 5 switches |
| AC4 | No file tree or Git refresh for invisible worktrees | `os_signpost` `panelRefresh` — zero intervals while panel hidden |
| AC5 | ≥ 15% lower resident RAM in representative scenario | `vmmap` / Activity Monitor before and after |
| AC6 | No progressive RAM growth after 10× pane open/close | `vmmap` before and after cycle |
| AC7 | No lost bytes — hidden pane shows all accumulated output | **Automated:** Feed a known sentinel sequence (e.g., `seq 1 1000` with unique markers at first and last line) to a hidden pane. Use `tillerctl panel.read` or a package-level test harness to assert first sentinel, last sentinel, expected count, order, and byte-level scrollback data. If UI/libghostty verification must remain integration/manual, pair it with automated raw-byte tests at the `ScrollbackBuffer` level. This gate is tied specifically to B4: B4 cannot ship unless this verification passes. |
| AC8 | Correct agent state and notifications for hidden panes | **Objective:** Identify which agent/test adapter and hook path is used. With a hidden pane running an agent that supports native hooks (e.g., Codex with `hasNativeHooks == true`), the agent calls `tillerctl notify --session <paneId> --status <status>`. Verify: (a) the expected Layer-A notification is received by the control socket handler; (b) the pane's status transitions correctly (e.g., running → idle); (c) an in-app or macOS indication fires (e.g., notification banner, status change in sidebar). If the exact current API for notification observation is uncertain, specify a package/integration test against the existing `notify` control request handling and a deterministic user-observable check (e.g., sidebar badge or notification center entry). |
| AC9 | Empty worktree persists and restores | Close all tabs, quit, relaunch — worktree is present with empty state |
| AC10 | "New Terminal" button and keyboard shortcut create first tab | Click button / press `⌘T` → shell tab appears |
| AC11 | Empty mounted worktree has no PTY process | `ps aux | grep tiller` — no shell process for empty worktree |
| AC12 | `Scripts/ci.sh` prints `CI OK` | Run `Scripts/ci.sh` |

---

## Phased Rollout and Test Strategy

### Phase 1 — Tests for empty worktree (TillerTerminal + AppModel tests)

| Test | Location | What it verifies |
|------|----------|------------------|
| `closeTab removes last tab, activeTabId is nil` | `AppModelTests` | `closeTab` on last tab → `tabs[id]` is `[]`, `activeTabId[id]` is `nil` |
| `closeTab does not create replacement` | `AppModelTests` | No new `WorkspaceTab` created |
| `ensureTabs is no-op for empty worktree` | `AppModelTests` | `ensureTabs` with empty `tabs[id]` does nothing |
| `activeTab returns nil for empty list` | `AppModelTests` | `activeTab(for:)` returns `nil` |
| `persistTabs handles empty list` | `AppModelTests` | `persistTabs` with `[]` and `nil` activeTabId does not crash |
| `bootstrap restores empty worktree` | `AppModelTests` | Worktree with zero restored tabs is mounted with empty tab list |
| `openWorktreeIds includes empty worktrees` | `AppModelTests` | `bootstrap` includes worktrees with empty tabs in `openWorktreeIds` |
| `empty worktree shows empty state` | Manual smoke | Selecting empty worktree shows `EmptyWorktreeView` |
| `New Terminal creates first tab` | Manual smoke | Button/shortcut → shell tab appears |
| `Control socket panel.create works on empty worktree` | Manual smoke | `tillerctl panel.create` creates a tab in empty worktree |

### Phase 2 — Baseline and instrumentation

1. Add `os_signpost` points (gated by `debug.signpostMetrics`).
2. Run baseline measurement on current code.
3. Commit instrumentation (gated, off by default).

### Phase 3 — Terminal pipeline

| Change | Tests |
|--------|-------|
| Reusable PTY read buffer | Existing `PtyProcessTests` pass; no behavioral change |
| Ring buffer scrollback | `ScrollbackBufferTests`: capacity, wrapping, `tail()` correctness, UTF-8 truncation at chunk boundary, ANSI sequences spanning chunks |
| Direct tail in content signal | `PtyTerminalPaneTests`: `emitContentSignal` returns correct tail; no data loss |
| Coalesced hidden-pane rendering | Investigation spike (B4); no-loss verification gate |

**Ring buffer test details:**
- Feed known byte sequences split at every relevant chunk/capacity boundary.
- Compare `snapshot()` and `tail()` against exact expected bytes.
- Include: multibyte UTF-8 split across chunks, incomplete leading codepoint
  after eviction, ANSI escape sequences spanning chunks, wrap-around,
  overflow, zero/oversized `tail()`, and repeated append.
- Bytes are never lost except deterministic oldest-byte eviction at capacity.
  Decoding handles incomplete boundaries without corrupting retained bytes.

### Phase 4 — Hidden activity and dirty FS coalescing

| Change | Tests |
|--------|-------|
| Inactive dirty marker | `RightPanelModelTests`: FS event while hidden marks dirty; activation triggers full refresh |
| Unclassifiable FS event → dirty | `RightPanelModelTests`: event with no path list marks dirty |
| Refresh on visibility | `RightPanelModelTests`: activation with dirty flag performs refresh; without dirty flag does not |
| Stop timer when all providers disabled | `UsageStoreTests`: timer cancelled when all `showInBar` are false |

### Phase 5 — SwiftUI and service auxiliary optimization

| Change | Tests |
|--------|-------|
| Remove collection-wide sidebar animations | Visual inspection; Instruments SwiftUI template |
| Compute row status once per render pass | Instruments SwiftUI template — fewer body evaluations |
| Empty worktree lifecycle (D1–D4) | Phase 1 tests + manual smoke |

### Risk controls

| Risk | Mitigation |
|------|------------|
| Ring buffer loses bytes at chunk boundary | `tail()` must handle partial UTF-8 sequences and ANSI escape codes spanning chunks. Test with deliberate boundary-crossing sequences. |
| Dirty marker misses an FS event | Unclassifiable events always mark dirty. This is safe: the next activation performs a full refresh. |
| Hidden-pane rendering change breaks libghostty | B4 is an investigation spike, not a promised implementation. If no safe API exists, retain current rendering. |
| Empty worktree breaks control socket | `panel.create` must work on empty worktrees. Test with `tillerctl panel.create`. |
| Empty worktree breaks session restore | Bootstrap must mount empty worktrees. Test with quit/relaunch cycle. |

---

## Affected Files and Symbols

### TillerTerminal

| File | Symbol | Change |
|------|--------|--------|
| `PtyProcess.swift` | `startReadLoop()` | Replace per-event `[UInt8]` allocation with reusable `readBuffer` |
| `PtyProcess.swift` | (new stored property) | `private var readBuffer: [UInt8]` |
| `ScrollbackBuffer.swift` | `storage` | Replace `Data` with ring buffer of chunks |
| `ScrollbackBuffer.swift` | `append(_:)` | O(1) amortized chunk append |
| `ScrollbackBuffer.swift` | `snapshot()` | Concatenate chunks (O(total bytes)) |
| `ScrollbackBuffer.swift` | (new method) | `tail(_ maxBytes: Int) -> Data` |
| `PtyTerminalPane.swift` | `emitContentSignal()` | Use `scrollback.tail(10*1024)` instead of full `snapshot()` |
| `PtyTerminalPane.swift` | (new) | Hidden-pane rendering investigation (B4) |

### TillerCore

| File | Symbol | Change |
|------|--------|--------|
| `AppSettings.swift` | (new key) | `debug.signpostMetrics` |

### App

| File | Symbol | Change |
|------|--------|--------|
| `AppModel.swift` | `closeTab(_:in:)` | Remove replacement-tab creation |
| `AppModel.swift` | `ensureTabs(for:)` | Make no-op for empty worktrees |
| `AppModel.swift` | `bootstrap()` | Remove empty-tab guard; include empty worktrees in `openWorktreeIds` |
| `AppModel.swift` | (new) | `os_signpost` intervals |
| `ContentView.swift` | `terminalStack` | Add `EmptyWorktreeView` for selected worktree with no tabs |
| `ContentView.swift` | (new) | `EmptyWorktreeView` |
| `ContentView.swift` | `.onChange(of:)` | Gate `UsageStore.updatePolling` on provider-enabled keys |
| `SidebarView.swift` | (animations) | Remove collection-wide `.animation()` modifiers |
| `SidebarView.swift` | `sortedWorktrees` | Lift into computed property |
| `RightPanelModel.swift` | `refresh(changedPaths:token:forceAllLoadedDirectories:)` | Add dirty-marker gating |
| `RightPanelModel.swift` | (new) | `dirtyWorktrees: Set<UUID>`, `markDirty(worktreeId:)` |
| `UsageStore.swift` | `start()` / `stop()` | Gate timer on provider visibility |
| `UsageStore.swift` | (new) | `updatePolling(enabledProviders:)`, `stop()` |

---

## Out of Scope

- libghostty memory profiling and optimization (documented separately in
  `docs/superpowers/notes/libghostty-memory.md`).
- Per-pane process memory (shell, agent CLI, language server).
- SwiftUI framework overhead outside the identified hotspots.
- Replacing `LazyVStack` with `UICollectionView` or `NSCollectionView`.
- Syntax highlighting, font rendering, or terminal-surface GPU rendering.
- Agent-launch latency or agent-process CPU/memory.
- Reducing the 64 KB PTY read buffer size (it is the kernel's preferred size
  for PTY reads; changing it may increase syscall count).
- Scrollback compression (the 256 KB cap is already a bound; compression would
  add CPU cost on every append).
- Disk I/O for scrollback persistence (already async and off-main).
