# Non-Git Projects Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Tiller add empty/non-git folders as first-class projects — prompting to `git init` at add time, hiding all git features when declined, and allowing later conversion.

**Architecture:** A pure `.git`-existence check lives in `TillerGit` (leaf package). The App-layer `AppModel` derives `gitProjectIds: Set<UUID>` at runtime (bootstrap, add, init-git action) — never persisted. Views and the control-socket handler gate git features on `model.isGitProject(...)`.

**Tech Stack:** Swift 6, SwiftUI, swift-testing (`@Test`/`#expect`), GRDB (via existing `ProjectStore`), xcodegen.

**Spec:** `docs/superpowers/specs/2026-07-12-non-git-projects-design.md`

## Global Constraints

- Tests first, `swift-testing` (`@Test` / `#expect`) — never XCTest.
- Conventional Commits, lower-case imperative subject (`feat:`, `fix:`, `test:`, …).
- No DB schema change / migration — git-ness is derived, never persisted.
- The App target has **no test target**; App-layer tasks are verified by `xcodebuild` build success. Package logic is unit-tested.
- User-facing strings for the new git-init dialog and menu item are Italian, exactly as written in the spec: "Inizializza git", "Aggiungi senza git", "Annulla", "Inizializza repository git".
- Final gate for the whole feature: `Scripts/ci.sh` prints `CI OK`.
- Do not hand-edit `Tiller.xcodeproj`; new App/package files under existing directories are picked up by `xcodegen generate` (run by `ci.sh`).

---

### Task 1: `GitRepoDetection` in TillerGit

**Files:**
- Create: `Packages/TillerGit/Sources/TillerGit/GitRepoDetection.swift`
- Test: `Packages/TillerGit/Tests/TillerGitTests/GitRepoDetectionTests.swift`

**Interfaces:**
- Consumes: nothing (leaf).
- Produces: `GitRepoDetection.isGitRepository(path: String) -> Bool` (public, synchronous, never throws). Used by Tasks 3 and 4.

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerGit/Tests/TillerGitTests/GitRepoDetectionTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerGit

private func makeTempDir() throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-detect-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

@Test func emptyFolderIsNotGit() throws {
    let dir = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: dir) }
    #expect(GitRepoDetection.isGitRepository(path: dir.path) == false)
}

@Test func folderWithGitDirectoryIsGit() throws {
    let dir = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: dir) }
    try FileManager.default.createDirectory(
        at: dir.appendingPathComponent(".git"), withIntermediateDirectories: true
    )
    #expect(GitRepoDetection.isGitRepository(path: dir.path))
}

@Test func folderWithGitFileIsGit() throws {
    // Linked worktrees and submodules have a `.git` *file* pointing at the real gitdir.
    let dir = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: dir) }
    try "gitdir: /elsewhere/.git/worktrees/x"
        .write(to: dir.appendingPathComponent(".git"), atomically: true, encoding: .utf8)
    #expect(GitRepoDetection.isGitRepository(path: dir.path))
}

@Test func missingFolderIsNotGit() {
    #expect(GitRepoDetection.isGitRepository(path: "/nonexistent/tiller-test-path") == false)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerGit && swift test --filter GitRepoDetection`
Expected: compile FAILURE — `cannot find 'GitRepoDetection' in scope`.

- [ ] **Step 3: Write minimal implementation**

Create `Packages/TillerGit/Sources/TillerGit/GitRepoDetection.swift`:

```swift
import Foundation

public enum GitRepoDetection {
    /// True if `path` contains a `.git` entry. A directory is a normal
    /// repo; a file is a linked worktree/submodule — both count as git.
    public static func isGitRepository(path: String) -> Bool {
        FileManager.default.fileExists(
            atPath: (path as NSString).appendingPathComponent(".git")
        )
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerGit && swift test --filter GitRepoDetection`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerGit/Sources/TillerGit/GitRepoDetection.swift \
        Packages/TillerGit/Tests/TillerGitTests/GitRepoDetectionTests.swift
git commit -m "feat: add GitRepoDetection.isGitRepository to TillerGit"
```

---

### Task 2: `ProjectStore.setWorktreeBranch` in TillerCore

Needed by the in-app "Inizializza repository git" action: after `git init`, the
main worktree row's branch (fallback `"main"`) may not match the repo's real
`init.defaultBranch`, so the row must be updatable.

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift` (add method after `setWorktreeComment`, ~line 150)
- Test: `Packages/TillerCore/Tests/TillerCoreTests/ProjectStoreTests.swift` (append)

**Interfaces:**
- Consumes: existing `ProjectStore` actor, `AppDatabase.inMemory()`.
- Produces: `func setWorktreeBranch(_ id: UUID, branch: String) throws` on `ProjectStore`. Used by Task 3.

- [ ] **Step 1: Write the failing test**

Append to `Packages/TillerCore/Tests/TillerCoreTests/ProjectStoreTests.swift`:

```swift
@Test func setWorktreeBranchUpdatesRow() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    try await store.setWorktreeBranch(w.id, branch: "master")
    #expect(try await store.worktrees(of: p.id).first?.branch == "master")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter setWorktreeBranchUpdatesRow`
Expected: compile FAILURE — `value of type 'ProjectStore' has no member 'setWorktreeBranch'`.

- [ ] **Step 3: Write minimal implementation**

In `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift`, after `setWorktreeComment` (line ~150), add:

```swift
    /// Fixes the main-checkout row after an in-app `git init`, whose real
    /// default branch may differ from the "main" fallback stored at add time.
    public func setWorktreeBranch(_ id: UUID, branch: String) throws {
        try database.write { db in
            try db.execute(
                sql: "UPDATE worktree SET branch = ? WHERE id = ?",
                arguments: [branch, id.uuidString]
            )
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter setWorktreeBranchUpdatesRow`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/ProjectStore.swift \
        Packages/TillerCore/Tests/TillerCoreTests/ProjectStoreTests.swift
git commit -m "feat: add ProjectStore.setWorktreeBranch"
```

---

### Task 3: AppModel git-detection state and actions

**Files:**
- Modify: `App/AppModel.swift` — property + helpers near other state (~line 124), recompute in `bootstrap()` (line 142) and `addProject(at:)` (line 322), two new methods after `createProject` (line 354).

**Interfaces:**
- Consumes: `GitRepoDetection.isGitRepository(path:)` (Task 1), `ProjectStore.setWorktreeBranch(_:branch:)` (Task 2), existing `GitRunner.run(_:in:)`, `GitWorktrees.list(repoPath:)`, `resyncSelection(projectId:)`.
- Produces (used by Tasks 4–7):
  - `var gitProjectIds: Set<UUID>`
  - `func isGitProject(_ project: Project) -> Bool`
  - `func isGitProject(id: UUID) -> Bool`
  - `func initGitAndAddProject(at url: URL) async throws`
  - `func initializeGitRepository(for project: Project) async`

- [ ] **Step 1: Add state and helpers**

In `App/AppModel.swift`, next to `var tabs: [UUID: [WorkspaceTab]] = [:]` (~line 124), add:

```swift
    /// Projects whose root currently contains a `.git` entry. Derived at
    /// runtime (bootstrap, add, in-app git init) — never persisted, so an
    /// external `git init` is picked up on the next launch.
    var gitProjectIds: Set<UUID> = []

    func isGitProject(_ project: Project) -> Bool { gitProjectIds.contains(project.id) }
    func isGitProject(id: UUID) -> Bool { gitProjectIds.contains(id) }

    private func refreshGitDetection() {
        gitProjectIds = Set(
            projects.filter { GitRepoDetection.isGitRepository(path: $0.rootPath) }.map(\.id)
        )
    }
```

- [ ] **Step 2: Recompute at bootstrap**

In `bootstrap()`, right after the `for project in projects { ... }` loop that loads worktrees/tabs (after line 173, before `launchSnapshot = ...`), add:

```swift
            refreshGitDetection()
```

- [ ] **Step 3: Gate the git shell-out in `addProject` and recompute**

Replace the body of `addProject(at:)` (lines 322–336) with:

```swift
    func addProject(at url: URL) async {
        guard let store else { return }
        do {
            let project = try await store.addProject(name: url.lastPathComponent, rootPath: url.path)
            // The repo's main checkout is itself the first "worktree" entry.
            // Non-git folders skip the git shell-out and keep the "main"
            // placeholder; the UI ignores it while the project is non-git.
            let isGit = GitRepoDetection.isGitRepository(path: url.path)
            let branch = isGit
                ? ((try? await GitWorktrees.list(repoPath: url.path).first?.branch) ?? nil)
                : nil
            let main = try await store.addWorktree(
                projectId: project.id, branch: branch ?? "main", path: url.path
            )
            projects.append(project)
            worktrees[project.id] = [main]
            refreshGitDetection()
        } catch {
            lastError = "Add project failed: \(error)"
        }
    }
```

- [ ] **Step 4: Add the two git-init actions**

After `createProject(name:in:)` (line ~364), add:

```swift
    /// Add-flow path: user chose "Inizializza git" for a non-git folder.
    /// Throws so the sheet can alert without adding the project.
    func initGitAndAddProject(at url: URL) async throws {
        _ = try await GitRunner.run(["init"], in: url.path)
        await addProject(at: url)
    }

    /// Context-menu path: converts an already-added non-git project.
    func initializeGitRepository(for project: Project) async {
        guard let store else { return }
        do {
            _ = try await GitRunner.run(["init"], in: project.rootPath)
            // Respect a non-"main" init.defaultBranch: re-read the real
            // branch and fix the placeholder stored at add time.
            if let branch = try? await GitWorktrees.list(repoPath: project.rootPath).first?.branch,
               let main = (worktrees[project.id] ?? []).first(where: { $0.path == project.rootPath }),
               main.branch != branch {
                try await store.setWorktreeBranch(main.id, branch: branch)
                worktrees[project.id] = try await store.worktrees(of: project.id)
                resyncSelection(projectId: project.id)
            }
            refreshGitDetection()
        } catch {
            lastError = "Git init failed: \(error)"
        }
    }
```

- [ ] **Step 5: Verify by building**

Run:
```bash
xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller \
  -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -3
```
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 6: Commit**

```bash
git add App/AppModel.swift
git commit -m "feat: derive git detection state and add git-init actions in AppModel"
```

---

### Task 4: Add-flow dialog in AddProjectSheet

**Files:**
- Modify: `App/AddProjectSheet.swift` — `AddProjectSheet` struct (state at line ~15, `body` at 17–31, `browseFolder()` at 84–95).

**Interfaces:**
- Consumes: `GitRepoDetection.isGitRepository(path:)` (Task 1; `TillerGit` already imported), `model.addProject(at:)`, `model.initGitAndAddProject(at:)` (Task 3).
- Produces: user-visible three-way dialog; no new API.

- [ ] **Step 1: Add error state and alert**

In `AddProjectSheet`, under `@State private var step: AddProjectStep = .menu`, add:

```swift
    @State private var errorMessage: String?
```

Attach to the outer `Group` in `body` (after `.background(AppTheme.background)`):

```swift
        .alert(
            "Git init failed",
            isPresented: .init(get: { errorMessage != nil }, set: { if !$0 { errorMessage = nil } })
        ) {
            Button("OK", role: .cancel) { errorMessage = nil }
        } message: {
            Text(errorMessage ?? "")
        }
```

- [ ] **Step 2: Branch the browse flow on git detection**

Replace `browseFolder()` (lines 84–95) with:

```swift
    private func browseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Choose a project folder"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if GitRepoDetection.isGitRepository(path: url.path) {
            Task {
                await model.addProject(at: url)
                dismiss()
            }
            return
        }
        let alert = NSAlert()
        alert.messageText = "Questa cartella non è un repository git"
        alert.informativeText =
            "Vuoi inizializzare un repository git in \(url.path)? "
            + "Senza git il progetto non avrà worktree né branch."
        alert.addButton(withTitle: "Inizializza git")
        alert.addButton(withTitle: "Aggiungi senza git")
        alert.addButton(withTitle: "Annulla")
        switch alert.runModal() {
        case .alertFirstButtonReturn:
            Task {
                do {
                    try await model.initGitAndAddProject(at: url)
                    dismiss()
                } catch {
                    errorMessage = "\(error)"
                }
            }
        case .alertSecondButtonReturn:
            Task {
                await model.addProject(at: url)
                dismiss()
            }
        default:
            break
        }
    }
```

Note: `panel.message` changes from "Choose a git repository folder" to "Choose a project folder" — non-git folders are now valid.

- [ ] **Step 3: Verify by building**

Run:
```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -3
```
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 4: Commit**

```bash
git add App/AddProjectSheet.swift
git commit -m "feat: prompt to git init when adding a non-git folder"
```

---

### Task 5: Sidebar — hide git features, add init action

**Files:**
- Modify: `App/SidebarView.swift` — `NewWorktreeButton` call site (line 65), `ProjectRow.contextMenu` (lines 272–276), `WorktreeRow.body` title `HStack` (lines 334–352).

**Interfaces:**
- Consumes: `model.isGitProject(_:)`, `model.isGitProject(id:)`, `model.initializeGitRepository(for:)` (Task 3).
- Produces: UI behavior only.

- [ ] **Step 1: Gate the "New Worktree…" row**

At line 65, wrap the button:

```swift
                                if model.isGitProject(project) {
                                    NewWorktreeButton { branchPromptProject = project }
                                }
```

- [ ] **Step 2: Add "Inizializza repository git" to the project context menu**

Replace `ProjectRow`'s `.contextMenu` (lines 272–276) with:

```swift
        .contextMenu {
            if !model.isGitProject(project) {
                Button("Inizializza repository git") {
                    Task { await model.initializeGitRepository(for: project) }
                }
                Divider()
            }
            Button("Remove Project", role: .destructive) {
                Task { await model.removeProject(project) }
            }
        }
```

- [ ] **Step 3: Swap branch label for folder name on non-git worktree rows**

In `WorktreeRow.body`, add below `let idle = ...` (line 328):

```swift
        let isGit = model.isGitProject(id: worktree.projectId)
```

Then in the title `HStack` (lines 334–342), make the glyph and title conditional
and hide the primary pill for non-git rows (a lone folder row has no
branch/primary semantics):

```swift
                HStack(spacing: 6) {
                    Image(systemName: isGit ? "arrow.triangle.branch" : "folder")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(isSelected ? AppTheme.titleSelected : AppTheme.meta)
                    Text(isGit ? worktree.branch : (worktree.path as NSString).lastPathComponent)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(isSelected ? AppTheme.titleSelected : AppTheme.title)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if worktree.isPrimary && isGit {
                        Text("primary")
                            .font(.system(size: 9.5))
                            .textCase(.uppercase)
                            .foregroundStyle(AppTheme.title)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 1)
                            .background(Capsule().fill(AppTheme.primaryPillBg))
                    }
                }
```

- [ ] **Step 4: Verify by building**

Run:
```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -3
```
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 5: Commit**

```bash
git add App/SidebarView.swift
git commit -m "feat: hide git features in sidebar for non-git projects"
```

---

### Task 6: Project settings — hide worktree sections, real repo type

**Files:**
- Modify: `App/ProjectSettingsSheet.swift` — `body` sections (lines 28–34), `IdentitySection` (lines 70–105).

**Interfaces:**
- Consumes: `model.isGitProject(_:)` (Task 3).
- Produces: UI behavior only.

- [ ] **Step 1: Hide worktree sections for non-git projects**

Replace the section list in `ProjectSettingsSheet.body` (lines 28–34) with:

```swift
                header
                IdentitySection(model: model, project: currentProject)
                Divider().overlay(AppTheme.hairline)
                RepoIconSection(model: model, project: currentProject)
                if model.isGitProject(currentProject) {
                    Divider().overlay(AppTheme.hairline)
                    WorktreeBaseSection(model: model, project: currentProject)
                    Divider().overlay(AppTheme.hairline)
                    WorktreeLocationSection(model: model, project: currentProject)
                }
```

- [ ] **Step 2: Show the real repository type**

In `IdentitySection` (line 91), replace the hardcoded label:

```swift
            Text("Git").font(.system(size: 12)).foregroundStyle(AppTheme.meta)
```

with:

```swift
            Text(model.isGitProject(project) ? "Git" : "Folder")
                .font(.system(size: 12)).foregroundStyle(AppTheme.meta)
```

- [ ] **Step 3: Verify by building**

Run:
```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -3
```
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 4: Commit**

```bash
git add App/ProjectSettingsSheet.swift
git commit -m "feat: hide worktree settings for non-git projects"
```

---

### Task 7: CLI gate on `workspace.create` + full CI

**Files:**
- Modify: `App/AppModel+Control.swift` — `case "workspace.create"` (lines 132–150).

**Interfaces:**
- Consumes: `isGitProject(_:)` (Task 3; `AppModel+Control.swift` is an extension of `AppModel`).
- Produces: `workspace.create` on a non-git project returns `failure` with exactly `"project is not a git repository"` (spec wording; surfaces through `tillerctl new`/`workspace create`).

- [ ] **Step 1: Add the guard**

In `case "workspace.create"`, right after the `guard let project = ...` that resolves the project (before `let branch = ...`), add:

```swift
            guard isGitProject(project) else {
                return .failure(id: request.id, error: "project is not a git repository")
            }
```

- [ ] **Step 2: Run the full verification gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`.
Known flake: `TillerTerminal` `spawnCapturesOutput` can fail under parallel load — retry `ci.sh` up to 5–6 times before treating a failure as real (see memory `tiller-flaky-pty-tests`).

- [ ] **Step 3: Commit**

```bash
git add App/AppModel+Control.swift
git commit -m "feat: reject workspace.create on non-git projects"
```

---

## Manual smoke checklist (post-implementation, in the running app)

1. Add an empty folder → dialog appears → "Aggiungi senza git" → project appears with folder icon row, no branch label, no "New Worktree…", settings show "Folder" and no worktree sections.
2. Right-click that project → "Inizializza repository git" → row gains branch label, "New Worktree…" appears, context-menu item disappears.
3. Add another empty folder → "Inizializza git" → enters directly as git project.
4. `tillerctl new --project <non-git-name>` → error `project is not a git repository`.
5. `git init` in a non-git project's folder from an external terminal → relaunch Tiller → project shows as git.
