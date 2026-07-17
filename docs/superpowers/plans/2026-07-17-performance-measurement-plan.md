# Performance Measurement — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

## Goal

Add `os_signpost` instrumentation gated by `UserDefaults` key `debug.signpostMetrics` (default: off) at the six measurement points defined in the approved spec. No optimization yet — this phase captures the baseline and provides the measurement infrastructure for all subsequent phases.

## Architecture

A shared `OSSignposter` instance (subsystem: `dev.tiller`, category: `performance`) is used at each instrumentation site. The gate is a `UserDefaults` boolean check — when false, `os_signpost` calls are compile-time no-ops (the `os` framework elides them when no trace is being collected). No terminal contents, sensitive paths, or commands appear in signpost payloads — only fixed labels and numeric counts.

## Tech Stack

- Swift 6, macOS 15+
- `os` module (`OSSignposter`, `OSLog`)
- `UserDefaults.standard.bool(forKey:)` for the gate
- `Scripts/ci.sh` as verification gate
- No new dependencies

## Global Constraints

- Signposts are disabled by default (gated on `debug.signpostMetrics == false`).
- They emit no sensitive content (no terminal output, paths, or commands in payloads).
- The code compiles and passes `Scripts/ci.sh` both with the default (off) state and, if feasible, with a test-controlled enable path.
- No hard-coded uncalibrated performance thresholds in unit tests.

---

## Interfaces

### Consumed

```swift
// PtyProcess (TillerTerminal)
func startReadLoop()  // line 155

// ScrollbackBuffer (TillerTerminal)
func append(_ data: Data)  // line 14
func snapshot() -> Data    // line 21

// PtyRuntime.emitContentSignal() (TillerTerminal, line 219)

// RightPanelModel (App)
func refresh(changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool)  // line 199

// SidebarView (App)
var body: some View  // line 14

// AppModel (App)
var selectedWorktree: Worktree?  // line 26, didSet
```

### Produced

```swift
// AppSettings (TillerCore) — new key
public static let signpostMetricsKey = "debug.signpostMetrics"

// Shared signposter utility (TillerTerminal or App)
let signposter = OSSignposter(subsystem: "dev.tiller", category: "performance")
```

---

## Tasks

### Task 1: Add `debug.signpostMetrics` key to AppSettings

**File:** `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`

Add after line 53 (`maxMountedWorktreesKey`):

```swift
/// UserDefaults key for the os_signpost metrics gate. Default: off.
/// Enable via `defaults write dev.tiller debug.signpostMetrics -bool YES`.
public static let signpostMetricsKey = "debug.signpostMetrics"
```

**Verification:**
```bash
cd Packages/TillerCore && swift test
```
Expected: all existing tests pass.

---

### Task 2: Add signposter utility to TillerTerminal

**File:** `Packages/TillerTerminal/Sources/TillerTerminal/SignpostMetrics.swift` (new)

```swift
import os
import Foundation

/// os_signpost instrumentation for the terminal pipeline, gated by
/// `UserDefaults.standard.bool(forKey: "debug.signpostMetrics")`.
///
/// When the gate is off, all calls are compile-time no-ops — zero runtime
/// overhead. When on, signposts are visible in Instruments (os_signpost
/// template) and via `oslog` with `OS_LOG_TYPE_DEBUG`.
///
/// No sensitive content (terminal output, paths, commands, identities) is
/// ever included in signpost payloads — only fixed labels and numeric counts.
public enum SignpostMetrics {
    private static let enabled = { UserDefaults.standard.bool(forKey: AppSettings.signpostMetricsKey) }()
    private static let log = OSLog(subsystem: "dev.tiller", category: .pointsOfInterest)
    private static let signposter = OSSignposter(log: log)

    public static func beginInterval(_ name: StaticString, id: OSSignpostID = .exclusive) -> OSSignpostIntervalState? {
        guard enabled else { return nil }
        return signposter.beginInterval(name, id: id)
    }

    public static func endInterval(_ name: StaticString, _ state: OSSignpostIntervalState?, message: String? = nil) {
        guard enabled, let state else { return }
        if let message {
            signposter.endInterval(name, state, "%{public}s", message)
        } else {
            signposter.endInterval(name, state)
        }
    }

    public static func makeSignpostID() -> OSSignpostID {
        signposter.makeSignpostID()
    }
}
```

**Note:** `AppSettings` is in `TillerCore`; `TillerTerminal` depends on `TillerCore`. The import is already present via the package dependency. Verify the import compiles.

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 3: Instrument `PtyProcess.startReadLoop()` — `ptyIngest`

**File:** `Packages/TillerTerminal/Sources/TillerTerminal/PtyProcess.swift`

Add signpost around the read in `startReadLoop()` (lines 157-166):

**Edit:**
```
oldString:         source.setEventHandler { [weak self] in
            guard let self, self.masterFD >= 0 else { return }
            var buffer = [UInt8](repeating: 0, count: 64 * 1024)
            let n = read(self.masterFD, &buffer, buffer.count)
            if n > 0 {
                self.onOutput(Data(buffer[0..<n]))
            } else {
                self.stopReadLoop()
                self.reapChildWithRetry()
            }
        }
newString:         source.setEventHandler { [weak self] in
            guard let self, self.masterFD >= 0 else { return }
            let sid = SignpostMetrics.makeSignpostID()
            let state = SignpostMetrics.beginInterval("ptyIngest", id: sid)
            var buffer = [UInt8](repeating: 0, count: 64 * 1024)
            let n = read(self.masterFD, &buffer, buffer.count)
            if n > 0 {
                self.onOutput(Data(buffer[0..<n]))
                SignpostMetrics.endInterval("ptyIngest", state, message: "bytes: \(n)")
            } else {
                SignpostMetrics.endInterval("ptyIngest", state, message: "eof")
                self.stopReadLoop()
                self.reapChildWithRetry()
            }
        }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 4: Instrument `ScrollbackBuffer.append` and `snapshot` — `scrollbackAppend` / `scrollbackSnapshot`

**File:** `Packages/TillerTerminal/Sources/TillerTerminal/ScrollbackBuffer.swift`

**Edit `append` (lines 14-18):**
```
oldString:     public func append(_ data: Data) {
        storage.append(data)
        if storage.count > capacity {
            storage.removeFirst(storage.count - capacity)
        }
    }
newString:     public func append(_ data: Data) {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("scrollbackAppend", id: sid)
        storage.append(data)
        if storage.count > capacity {
            storage.removeFirst(storage.count - capacity)
        }
        SignpostMetrics.endInterval("scrollbackAppend", state, message: "bytes: \(data.count)")
    }
```

**Edit `snapshot` (line 21):**
```
oldString:     public func snapshot() -> Data { storage }
newString:     public func snapshot() -> Data {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("scrollbackSnapshot", id: sid)
        let result = storage
        SignpostMetrics.endInterval("scrollbackSnapshot", state, message: "bytes: \(result.count)")
        return result
    }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 5: Instrument `PtyRuntime.emitContentSignal()` — `contentSignal`

**File:** `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`, lines 219-226

**Edit:**
```
oldString:     private func emitContentSignal() async {
        let data = await scrollback.snapshot()
        let text = stripANSI(String(decoding: data, as: UTF8.self))
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        let tail = lines.suffix(40).joined(separator: "\n")
        guard !tail.isEmpty else { return }
        onContentSignal?(paneId, tail)
    }
newString:     private func emitContentSignal() async {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("contentSignal", id: sid)
        let data = await scrollback.snapshot()
        let text = stripANSI(String(decoding: data, as: UTF8.self))
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        let tail = lines.suffix(40).joined(separator: "\n")
        guard !tail.isEmpty else {
            SignpostMetrics.endInterval("contentSignal", state, message: "empty")
            return
        }
        onContentSignal?(paneId, tail)
        SignpostMetrics.endInterval("contentSignal", state, message: "matched")
    }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 6: Instrument `RightPanelModel.refresh` — `panelRefresh`

**File:** `App/RightPanel/RightPanelModel.swift`, lines 199-223

**Edit:**
```
oldString:     private func refresh(
        changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool
    ) async {
        guard token == generation, let rootURL, let worktree else { return }
newString:     private func refresh(
        changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool
    ) async {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("panelRefresh", id: sid)
        defer { SignpostMetrics.endInterval("panelRefresh", state) }
        guard token == generation, let rootURL, let worktree else { return }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 7: Instrument `SidebarView.body` — `sidebarBody`

**File:** `App/SidebarView.swift`, line 14

**Edit:**
```
oldString:     var body: some View {
        Group {
            VStack(spacing: 0) {
newString:     var body: some View {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("sidebarBody", id: sid)
        defer { SignpostMetrics.endInterval("sidebarBody", state) }
        return Group {
            VStack(spacing: 0) {
```

**Note:** This adds a `let` binding and `defer` at the top of `body`. SwiftUI's `body` property must return a single view — the `return` keyword is required because the `defer` statement makes the implicit return ambiguous. Verify the edit compiles.

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 8: Instrument `AppModel.selectedWorktree.didSet` — `worktreeSwitch`

> **Cross-plan note:** This task and `docs/superpowers/plans/2026-07-17-empty-worktree-lifecycle-plan.md` Task 3 both edit `AppModel.selectedWorktree.didSet`. Phase 1 changes (Task 3: remove `ensureTabs`) must be applied **before** this Phase 2 change wraps the body in signpost intervals. Apply in order: first the empty-worktree-lifecycle Task 3, then this Task 8.

**File:** `App/AppModel.swift`, lines 26-35

**Edit:**
```
oldString:     var selectedWorktree: Worktree? {
        didSet {
            selectedProjectId = selectedWorktree?.projectId
            UserDefaults.standard.set(selectedWorktree?.id.uuidString, forKey: AppSettings.selectedWorktreeIdKey)
            guard let worktree = selectedWorktree else { return }
            if !openWorktreeIds.contains(worktree.id) { openWorktreeIds.append(worktree.id) }
            ensureTabs(for: worktree)
            evictIdleWorktreesIfNeeded()
        }
    }
newString:     var selectedWorktree: Worktree? {
        didSet {
            let sid = SignpostMetrics.makeSignpostID()
            let state = SignpostMetrics.beginInterval("worktreeSwitch", id: sid)
            defer { SignpostMetrics.endInterval("worktreeSwitch", state) }
            selectedProjectId = selectedWorktree?.projectId
            UserDefaults.standard.set(selectedWorktree?.id.uuidString, forKey: AppSettings.selectedWorktreeIdKey)
            guard let worktree = selectedWorktree else { return }
            if !openWorktreeIds.contains(worktree.id) { openWorktreeIds.append(worktree.id) }
            ensureTabs(for: worktree)
            evictIdleWorktreesIfNeeded()
        }
    }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 9: Baseline capture

Run the measurement protocol from the master plan:

1. Launch Tiller with 10 worktrees, 20 panes (2 per worktree).
2. Enable signposts: `defaults write dev.tiller debug.signpostMetrics -bool YES`
3. Restart Tiller.
4. Let all panes idle for 60 seconds.
5. Record: CPU (Activity Monitor / `top`), resident RAM (`vmmap`), `os_signpost` metrics via Instruments (os_signpost template).
6. Run heavy output in 4 panes simultaneously (`git clone` of a large repo, `npm install`).
7. Record same metrics during and after.
8. Switch between worktrees rapidly (5 switches in 2 seconds).
9. Record main-thread stall duration (Instruments Time Profiler, HID template).

Save baseline measurements to `docs/superpowers/notes/baseline-2026-07-17.md`.

---

## Acceptance Criteria

| AC | Verification |
|----|-------------|
| AC1 baseline | Idle CPU captured in baseline |
| AC2 baseline | Terminal pipeline CPU captured in baseline |
| AC3 baseline | Main-thread stall captured in baseline |
| AC5 baseline | Resident RAM captured in baseline |
| AC6 baseline | RAM after 10× pane open/close captured in baseline |
| AC12 | `Scripts/ci.sh` prints `CI OK` |

## Commit Messages

```bash
# After Task 1:
git add Packages/TillerCore/Sources/TillerCore/AppSettings.swift
git commit -m "feat: add debug.signpostMetrics key to AppSettings"

# After Tasks 2-8:
git add Packages/TillerTerminal/Sources/TillerTerminal/SignpostMetrics.swift
git add Packages/TillerTerminal/Sources/TillerTerminal/PtyProcess.swift
git add Packages/TillerTerminal/Sources/TillerTerminal/ScrollbackBuffer.swift
git add Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift
git add App/RightPanel/RightPanelModel.swift
git add App/SidebarView.swift
git add App/AppModel.swift
git commit -m "feat: add os_signpost instrumentation gated by debug.signpostMetrics"

# After Task 9:
git add docs/superpowers/notes/baseline-2026-07-17.md
git commit -m "docs: record baseline measurements for performance optimization"
```
