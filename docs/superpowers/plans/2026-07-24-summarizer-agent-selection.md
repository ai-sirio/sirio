# Auto-Naming Summarizer Agent Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user pick in Settings which built-in agent CLI generates auto-naming tab titles, with runtime fallback to the tab's own agent, unlocking auto-rename for registry-agent chat tabs.

**Architecture:** A new `AppSettings` key + pure resolver (TillerCore) feeds a `SummarizerSelection` pure helper (App/AutoNaming) that returns an ordered adapter list [selected primary, tab-agent fallback]. `requestAutoRename` in `AppModel` drops its `AgentCatalog` gate and iterates that list through the existing `AutoNamer.summarize`; a Picker in `GeneralSettingsView` stores the choice.

**Tech Stack:** Swift 6, SwiftUI (`@AppStorage`), swift-testing (`@Test`/`#expect`), xcodegen project, `Scripts/ci.sh` gate.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-24-summarizer-agent-selection-design.md`.
- UI strings in English (do NOT touch the pre-existing Italian strings in the About section — out of scope).
- Tests must never touch `UserDefaults.standard` (AppTests are hosted inside Tiller.app — `.standard` is the user's real domain).
- No test may spawn a real agent CLI process.
- Default summarizer id is `"claude"`. Stored values are AgentCatalog short ids (`claude`, `codex`, `opencode`, `pi`, `omp`).
- Commit messages: Conventional Commits, lower-case imperative.
- `Scripts/ci.sh` must print `CI OK` before the feature is considered done. Note: `TillerTerminal` test `spawnCapturesOutput` is known-flaky — retry `Scripts/ci.sh` up to 5-6 times on that specific failure.

---

### Task 1: AppSettings key + resolver (TillerCore)

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift` (after the `autoNamingEnabled` resolver, line ~44)
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`

**Interfaces:**
- Consumes: nothing new.
- Produces: `AppSettings.summarizerAgentIdKey: String` (= `"autoNaming.summarizerAgentId"`), `AppSettings.defaultSummarizerAgentId: String` (= `"claude"`), `AppSettings.summarizerAgentId(defaultsValue: String?) -> String`. Tasks 3 and 4 rely on these exact names.

- [ ] **Step 1: Write the failing test**

Append inside the existing suite in `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift` (open the file first and match its suite/struct style):

```swift
@Test func summarizerAgentIdDefaultsToClaude() {
    #expect(AppSettings.summarizerAgentId(defaultsValue: nil) == "claude")
    #expect(AppSettings.summarizerAgentId(defaultsValue: "") == "claude")
    #expect(AppSettings.summarizerAgentId(defaultsValue: "codex") == "codex")
    #expect(AppSettings.summarizerAgentIdKey == "autoNaming.summarizerAgentId")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter summarizerAgentIdDefaultsToClaude`
Expected: compile error `type 'AppSettings' has no member 'summarizerAgentId'`

- [ ] **Step 3: Write minimal implementation**

In `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`, directly after the `autoNamingEnabled(defaultsValue:)` function:

```swift
/// UserDefaults key for the auto-naming summarizer agent (an AgentCatalog
/// short id). Missing or empty value means the default, "claude".
/// Validation against the actual adapter list happens at the App layer
/// (TillerCore does not know the catalog).
public static let summarizerAgentIdKey = "autoNaming.summarizerAgentId"
public static let defaultSummarizerAgentId = "claude"

/// Resolve the stored summarizer agent id, falling back to the default
/// for missing or empty values.
public static func summarizerAgentId(defaultsValue: String?) -> String {
    guard let id = defaultsValue, !id.isEmpty else {
        return defaultSummarizerAgentId
    }
    return id
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter summarizerAgentIdDefaultsToClaude`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSettings.swift Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift
git commit -m "feat: summarizer agent id setting with claude default"
```

---

### Task 2: SummarizerSelection helper (App/AutoNaming)

**Files:**
- Create: `App/AutoNaming/SummarizerSelection.swift`
- Test: `AppTests/SummarizerSelectionTests.swift`

**Interfaces:**
- Consumes: `AgentCatalog.all: [any AgentAdapter]` (TillerAgents; ids `claude`, `codex`, `opencode`, `pi`, `omp`, each with `displayName`); `AgentIdMigration.catalogId(_: String) -> String` (TillerACP; maps `claude-acp`→`claude`, `codex-acp`→`codex`, `pi-acp`→`pi`, everything else unchanged).
- Produces: `SummarizerSelection.adapters(selectedId: String, tabAgentId: String) -> [any AgentAdapter]` — ordered [primary, fallback?], deduplicated. Task 3 relies on this exact signature.

- [ ] **Step 1: Write the failing test**

Create `AppTests/SummarizerSelectionTests.swift`:

```swift
import Testing
import TillerAgents

@testable import Tiller

@Suite struct SummarizerSelectionTests {
    @Test func selectedAgentComesFirstThenTabAgent() {
        let ids = SummarizerSelection
            .adapters(selectedId: "codex", tabAgentId: "claude-acp").map(\.id)
        #expect(ids == ["codex", "claude"])
    }

    @Test func sameAgentIsDeduplicated() {
        let ids = SummarizerSelection
            .adapters(selectedId: "claude", tabAgentId: "claude-acp").map(\.id)
        #expect(ids == ["claude"])
    }

    @Test func registryOnlyTabAgentHasNoFallback() {
        let ids = SummarizerSelection
            .adapters(selectedId: "claude", tabAgentId: "gemini").map(\.id)
        #expect(ids == ["claude"])
    }

    @Test func unknownSelectedIdFallsBackToClaudePrimary() {
        let ids = SummarizerSelection
            .adapters(selectedId: "nonexistent", tabAgentId: "codex-acp").map(\.id)
        #expect(ids == ["claude", "codex"])
    }

    @Test func canonicalPiIdResolvesAsFallback() {
        let ids = SummarizerSelection
            .adapters(selectedId: "opencode", tabAgentId: "pi-acp").map(\.id)
        #expect(ids == ["opencode", "pi"])
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -only-testing:AppTests/SummarizerSelectionTests test -quiet 2>&1 | tail -20`
Expected: compile error `cannot find 'SummarizerSelection' in scope`

- [ ] **Step 3: Write minimal implementation**

Create `App/AutoNaming/SummarizerSelection.swift`:

```swift
import TillerACP
import TillerAgents
import TillerCore

/// Ordered summarize candidates for one auto-naming pass: the user-selected
/// agent first, then the tab's own agent as runtime fallback. Pure — no
/// defaults access, no process spawning — so the ordering rules stay
/// testable without touching AutoNamer.
enum SummarizerSelection {
    static func adapters(
        selectedId: String, tabAgentId: String
    ) -> [any AgentAdapter] {
        let catalog = AgentCatalog.all
        let primary = catalog.first { $0.id == selectedId }
            ?? catalog.first { $0.id == AppSettings.defaultSummarizerAgentId }
        let fallback = catalog.first {
            $0.id == AgentIdMigration.catalogId(tabAgentId)
        }
        var result: [any AgentAdapter] = []
        if let primary { result.append(primary) }
        if let fallback, fallback.id != primary?.id { result.append(fallback) }
        return result
    }
}
```

Note: `App/AutoNaming/` already exists (holds `AutoNamer.swift`); after creating the file, run `xcodegen generate` so the Xcode project picks it up.

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -only-testing:AppTests/SummarizerSelectionTests test -quiet 2>&1 | tail -20`
Expected: 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add App/AutoNaming/SummarizerSelection.swift AppTests/SummarizerSelectionTests.swift
git commit -m "feat: ordered summarizer adapter selection with tab-agent fallback"
```

---

### Task 3: requestAutoRename wiring (AppModel)

**Files:**
- Modify: `App/AppModel.swift` — function `requestAutoRename(paneId:from:to:)` (line ~1686)

**Interfaces:**
- Consumes: `SummarizerSelection.adapters(selectedId:tabAgentId:)` (Task 2), `AppSettings.summarizerAgentIdKey` / `summarizerAgentId(defaultsValue:)` (Task 1), existing `AutoNamer.summarize(transcript:worktreePath:adapter:)`, existing `AgentIdMigration.catalogId(_:)`, `self.defaults: UserDefaults` (injected instance already on AppModel).
- Produces: chat tabs of ANY registry agent now reach the summarize step; behavior change only, no new API.

- [ ] **Step 1: Replace the gate and the summarize dispatch**

In `App/AppModel.swift`, the current body of `requestAutoRename` reads (locate it exactly; written by commit ac0babf):

```swift
guard tab.titleIsAutoNamed,
      let agentId = agentActivity.paneAgents[paneId]
          .map(AgentIdMigration.catalogId),
      let adapter = AgentCatalog.all.first(where: { $0.id == agentId })
else { return }

let source: TranscriptSource?
switch tab.content {
case .chat:
    source = chatControllers[tab.id].map { ChatTranscriptSource(controller: $0) }
case .terminal:
    source = await resolveFileTranscriptSource(
        paneId: paneId, worktree: worktree, agentId: agentId
    )
case .markdown:
    source = nil
}
guard let source, let text = source.recentText() else { return }

let throttle = autoNamingThrottle[paneId] ?? AutoNamingThrottle()
let now = Date()
guard throttle.shouldRun(transcriptLength: text.count, now: now) else { return }
autoNamingThrottle[paneId] = throttle.recording(
    transcriptLength: text.count, now: now
)

let worktreePath = worktree.path
Task { [weak self] in
    guard let title = await AutoNamer.summarize(
        transcript: text, worktreePath: worktreePath, adapter: adapter
    ) else { return }
    await MainActor.run {
        self?.applyAutoTitle(tab.id, in: worktree.id, title: title)
    }
}
```

Replace it with:

```swift
guard tab.titleIsAutoNamed,
      let tabAgentId = agentActivity.paneAgents[paneId]
else { return }
let catalogAgentId = AgentIdMigration.catalogId(tabAgentId)

let source: TranscriptSource?
switch tab.content {
case .chat:
    source = chatControllers[tab.id].map { ChatTranscriptSource(controller: $0) }
case .terminal:
    source = await resolveFileTranscriptSource(
        paneId: paneId, worktree: worktree, agentId: catalogAgentId
    )
case .markdown:
    source = nil
}
guard let source, let text = source.recentText() else { return }

let throttle = autoNamingThrottle[paneId] ?? AutoNamingThrottle()
let now = Date()
guard throttle.shouldRun(transcriptLength: text.count, now: now) else { return }
autoNamingThrottle[paneId] = throttle.recording(
    transcriptLength: text.count, now: now
)

let selectedId = AppSettings.summarizerAgentId(
    defaultsValue: defaults.string(forKey: AppSettings.summarizerAgentIdKey)
)
let adapters = SummarizerSelection.adapters(
    selectedId: selectedId, tabAgentId: tabAgentId
)
let worktreePath = worktree.path
Task { [weak self] in
    for adapter in adapters {
        guard let title = await AutoNamer.summarize(
            transcript: text, worktreePath: worktreePath, adapter: adapter
        ) else { continue }
        await MainActor.run {
            self?.applyAutoTitle(tab.id, in: worktree.id, title: title)
        }
        return
    }
}
```

Also update the doc comment above `requestAutoRename` — replace the sentence "Copre le chat tab ACP per tutti e 5 gli agenti e le terminal tab dei 3 adapter con hook nativi." with: "Copre le chat tab ACP di qualunque agente del registry e le terminal tab claude/codex; il titolo lo genera l'agente summarizer scelto in Settings, con fallback all'agente della tab."

- [ ] **Step 2: Verify the existing auto-rename tests still pass**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -only-testing:AppTests/AutoRenameWiringTests -only-testing:AppTests/SummarizerSelectionTests test -quiet 2>&1 | tail -20`
Expected: all PASS (the skip-path tests exercise the new gate: disabled setting, wrong transition, manual rename)

- [ ] **Step 3: Commit**

```bash
git add App/AppModel.swift
git commit -m "feat: auto-rename uses selected summarizer agent with tab-agent fallback"
```

---

### Task 4: Settings picker UI + CI gate

**Files:**
- Modify: `App/GeneralSettingsView.swift` (Automation section, line ~39-44)

**Interfaces:**
- Consumes: `AppSettings.summarizerAgentIdKey`, `AppSettings.defaultSummarizerAgentId` (Task 1), `AgentCatalog.all` (TillerAgents).
- Produces: user-visible Picker; no API.

- [ ] **Step 1: Add the picker**

In `App/GeneralSettingsView.swift`:

1. Add `import TillerAgents` after `import TillerCore`.
2. Add the property next to the other `@AppStorage` lines:

```swift
@AppStorage(AppSettings.summarizerAgentIdKey)
private var summarizerAgentId = AppSettings.defaultSummarizerAgentId
```

3. Replace the Automation section:

```swift
Section("Automation") {
    Toggle(isOn: $autoNamingEnabled) {
        Text("Auto-rename tabs and agents")
        Text("Summarizes each session's conversation into a short tab title using the selected summarizer agent. Manual renames always win.")
    }
    Picker(selection: $summarizerAgentId) {
        ForEach(AgentCatalog.all, id: \.id) { adapter in
            Text(adapter.displayName).tag(adapter.id)
        }
    } label: {
        Text("Summarizer agent")
        Text("The agent CLI that generates tab titles. Falls back to the session's own agent when it fails.")
    }
    .disabled(!autoNamingEnabled)
}
```

- [ ] **Step 2: Full CI gate**

Run: `Scripts/ci.sh`
Expected: `CI OK` (retry up to 5-6 times only if the known-flaky `spawnCapturesOutput` PTY test fails)

- [ ] **Step 3: Commit**

```bash
git add App/GeneralSettingsView.swift
git commit -m "feat: summarizer agent picker in general settings"
```

---

## Manual smoke (user-side, after deploy)

1. Settings → Automation: picker visible under the toggle, disabled when toggle off, defaults to Claude Code.
2. Pick Codex; run a Claude chat turn → title appears (generated by Codex CLI).
3. Chat with a registry-only agent (e.g. Gemini) → tab now gets auto-renamed.
4. Break the primary (e.g. select an agent whose CLI is not installed) → title still appears via the tab agent fallback.
