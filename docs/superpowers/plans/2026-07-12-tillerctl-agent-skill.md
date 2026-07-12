# tillerctl Agent Skill + Settings Install Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a `SKILL.md` that teaches coding agents (Claude Code, Codex, OpenCode) how to use `tillerctl`, plus a one-click "Install Skill" button in Tiller's Settings that opens Terminal.app and runs the install command immediately.

**Architecture:** Pure command/AppleScript-string construction lives in `TillerCore` (`AgentSkillInstall`, Foundation-only, unit-testable via `swift test`) — App has no test target, so anything worth asserting on must live in a package. `App/AgentSkillInstaller.swift` is a thin, untested wrapper that shells out to `/usr/bin/osascript` via `Process`, following the existing `AgentAccountStore.runProcess` style. `GeneralSettingsView` gets one new `Section("Agent Skill")` after the existing `Section("tillerctl")`. `skills/tillerctl-cli/SKILL.md` is static content, no code.

**Tech Stack:** Swift 6, swift-testing (`@Test`/`#expect`), SwiftUI, Foundation `Process`/`osascript`, [vercel-labs/skills](https://github.com/vercel-labs/skills) CLI (`npx skills add <owner/repo> --skill <name> -a <agents> -y`).

## Global Constraints

- Repo remote: `github.com/e-palmisano/tiller` — install command targets this repo by name.
- Install command (exact, verbatim): `npx skills add e-palmisano/tiller --skill tillerctl-cli -a claude-code,codex,opencode -y`
- Skill package path: `skills/tillerctl-cli/SKILL.md` (repo root, not inside `App/` or any `Packages/*`).
- `AgentSkillInstall`'s pure logic goes in `TillerCore` (Foundation-only, no AppKit) so it's covered by `swift test`; the AppKit/`Process` side effect stays in `App/`, untested (matches project's existing App/package split — see `CLAUDE.md`).
- No "already installed" detection or persisted state — `skills add` is idempotent by design (out of scope, per spec).
- `Scripts/ci.sh` must print `CI OK` before this is considered done.

---

### Task 1: `skills/tillerctl-cli/SKILL.md` content

**Files:**
- Create: `skills/tillerctl-cli/SKILL.md`

**Interfaces:**
- Consumes: nothing (static content).
- Produces: nothing consumed by later tasks — this file is standalone content, referenced only by the install command string in Task 2.

- [ ] **Step 1: Write the skill file**

Create `skills/tillerctl-cli/SKILL.md` with this exact content:

````markdown
---
name: tillerctl-cli
description: Use when running inside a Tiller pane and you need to orchestrate other panes/worktrees — list/create/split panes, send text or keys to another terminal, check your own worktree/pane context, or post notifications back to Tiller.
---

# Using tillerctl

`tillerctl` is a CLI that talks to a running Tiller.app over a local unix
socket. It lets an agent running inside one Tiller pane control other panes
and worktrees — split a new pane, send text or keystrokes to it, check what
it's doing, and get notified when things finish.

It only works when Tiller is running and its control socket is enabled
(default: on). If a command fails with "Tiller control socket is disabled
or Tiller is not running", stop — there's nothing to orchestrate.

## Self-identification

Every pane Tiller spawns has `$TILLER_WORKTREE_ID` and `$TILLER_PANE_ID` set
in its environment automatically. Use `identify` to resolve them to
human-readable info without parsing the env vars yourself:

```bash
tillerctl identify
# project<TAB>branch<TAB>path<TAB>workspaceId<TAB>surfaceId
```

Run this first when you need to know "where am I" before acting on another
pane (e.g. to avoid sending text to yourself).

## Command reference

### Workspaces (worktrees)

```bash
tillerctl list-workspaces                          # id / project / branch / path / selected
tillerctl current-workspace                         # the one selected in the sidebar
tillerctl new-workspace --project <uuid-or-name> [--branch <name>]
tillerctl select-workspace --workspace <uuid-or-path>
tillerctl close-workspace --workspace <uuid-or-path> # unmounts terminals; worktree stays in sidebar
```

### Panes (surfaces)

```bash
tillerctl list-panels                                # panes of the current worktree
tillerctl list-pane-surfaces                          # panes of the active tab only
tillerctl new-split <left|right|up|down>              # split the active pane
tillerctl focus-panel --panel <uuid>                  # bring a pane to the foreground
```

### Sending input

```bash
tillerctl send [--surface <uuid>] "some text"         # types text into a pane, no Enter
tillerctl send-key [--surface <uuid>] enter            # submits/confirms in that pane
```

If `--surface` is omitted, the target is your own pane (`$TILLER_PANE_ID`)
if set, else whatever pane is currently active in the UI. Always pass
`--surface` explicitly when acting on a pane other than your own.

**Gotcha — always use `send-key … enter` to submit, never rely on a
trailing newline in `send`.** Interactive terminal UIs (coding agents
included) run their input box in raw terminal mode, which only recognizes a
literal carriage return (`\r`, `0x0D`) as "Enter". A plain newline
(`\n`, `0x0A`) — which is what canonical-mode shells auto-translate `\r`
into, but raw-mode TUIs do not — will sit in the input box unsubmitted.
`send-key enter` sends the correct `\r` byte; `send` does not append
anything. Sequence for "type a task and run it":

```bash
tillerctl send --surface <uuid> "do the thing"
tillerctl send-key --surface <uuid> enter
```

### Notifications

```bash
tillerctl notify --title "Done" --body "Task finished"   # user-visible macOS notification
tillerctl list-notifications                               # date / title / subtitle / body
tillerctl clear-notifications
```

### Session restore

```bash
tillerctl restore-session   # re-applies the layout Tiller loaded at launch
```

## When to use this vs. not

Use `tillerctl` when you're **orchestrating something outside your own
pane**: spawning a helper agent in a new split, sending it a task,
polling/reading its output, or notifying the user when a long job finishes
elsewhere. Don't use it to talk to your own terminal — your normal
stdin/stdout already does that, and `tillerctl send-key` targeting yourself
adds nothing.
````

- [ ] **Step 2: Commit**

```bash
git add skills/tillerctl-cli/SKILL.md
git commit -m "docs: add tillerctl-cli agent skill"
```

---

### Task 2: `AgentSkillInstall` pure logic in TillerCore (TDD)

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/AgentSkillInstall.swift`
- Create: `Packages/TillerCore/Tests/TillerCoreTests/AgentSkillInstallTests.swift`

**Interfaces:**
- Consumes: nothing.
- Produces: `public enum AgentSkillInstall { public static let command: String; public static func appleScript(command: String = AgentSkillInstall.command) -> String }` — Task 3 (`App/AgentSkillInstaller.swift`) calls both.

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerCore/Tests/TillerCoreTests/AgentSkillInstallTests.swift`:

```swift
import Testing
@testable import TillerCore

@Test func installCommandIsExactExpectedString() {
    #expect(AgentSkillInstall.command ==
        "npx skills add e-palmisano/tiller --skill tillerctl-cli -a claude-code,codex,opencode -y")
}

@Test func appleScriptWrapsCommandInDoScript() {
    let script = AgentSkillInstall.appleScript(command: "echo hi")
    #expect(script.contains("tell application \"Terminal\""))
    #expect(script.contains("activate"))
    #expect(script.contains("do script \"echo hi\""))
}

@Test func appleScriptDefaultsToInstallCommand() {
    let script = AgentSkillInstall.appleScript()
    #expect(script.contains(AgentSkillInstall.command))
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter AgentSkillInstallTests`
Expected: FAIL — "cannot find 'AgentSkillInstall' in scope"

- [ ] **Step 3: Write minimal implementation**

Create `Packages/TillerCore/Sources/TillerCore/AgentSkillInstall.swift`:

```swift
import Foundation

/// The command that installs the tillerctl-cli agent skill via
/// https://github.com/vercel-labs/skills, and the AppleScript that runs it
/// in a new Terminal.app window. Foundation-only (no AppKit) so it stays
/// unit-testable; App/AgentSkillInstaller.swift owns the actual Process
/// launch.
public enum AgentSkillInstall {
    public static let command =
        "npx skills add e-palmisano/tiller --skill tillerctl-cli -a claude-code,codex,opencode -y"

    /// AppleScript source that opens/activates Terminal.app and runs
    /// `command` in it. `command` has no user input interpolated by any
    /// caller in this codebase — always a static string — so no escaping
    /// is performed here.
    public static func appleScript(command: String = AgentSkillInstall.command) -> String {
        """
        tell application "Terminal"
            activate
            do script "\(command)"
        end tell
        """
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter AgentSkillInstallTests`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AgentSkillInstall.swift Packages/TillerCore/Tests/TillerCoreTests/AgentSkillInstallTests.swift
git commit -m "feat: add AgentSkillInstall command/AppleScript builder to TillerCore"
```

---

### Task 3: `App/AgentSkillInstaller.swift` — Terminal launch

**Files:**
- Create: `App/AgentSkillInstaller.swift`

**Interfaces:**
- Consumes: `TillerCore.AgentSkillInstall.appleScript()` (from Task 2).
- Produces: `enum AgentSkillInstaller { static func openTerminalAndInstall() }` — Task 4 (`GeneralSettingsView`) calls this from a Button action.

- [ ] **Step 1: Write the implementation**

Create `App/AgentSkillInstaller.swift`:

```swift
import Foundation
import TillerCore

/// Opens Terminal.app and immediately runs the tillerctl-cli skill install
/// command. Not unit-tested: App has no test target, and this is a real
/// side effect (launches an external app) — the command/script it runs is
/// covered by AgentSkillInstallTests in TillerCore.
enum AgentSkillInstaller {
    static func openTerminalAndInstall() {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", AgentSkillInstall.appleScript()]
        try? process.run()
    }
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build 2>&1 | tail -20`
Expected: `** BUILD SUCCEEDED **`

(If `Tiller.xcodeproj` is stale relative to the new file, run `xcodegen generate` first — `App/AgentSkillInstaller.swift` is picked up automatically since `sources: [App]` in `project.yml` globs the whole directory, but regenerate if Xcode doesn't see the new file.)

- [ ] **Step 3: Commit**

```bash
git add App/AgentSkillInstaller.swift
git commit -m "feat: add AgentSkillInstaller Terminal launcher"
```

---

### Task 4: Settings UI — "Agent Skill" section

**Files:**
- Modify: `App/GeneralSettingsView.swift:37-58` (insert new `Section` after the existing `Section("tillerctl")`, before the closing `}` of the `Form`)

**Interfaces:**
- Consumes: `AgentSkillInstaller.openTerminalAndInstall()` (from Task 3).
- Produces: nothing consumed by later tasks (UI leaf).

- [ ] **Step 1: Edit `App/GeneralSettingsView.swift`**

The current end of the `Form` (lines 37-59) reads:

```swift
            Section("tillerctl") {
                Toggle(isOn: $controlSocketEnabled) {
                    Text("Enable control socket")
                    Text("Required by tillerctl and by agent lifecycle hooks — disabling it degrades agent status badges to title/process detection only.")
                }
                .onChange(of: controlSocketEnabled) { _, enabled in
                    model.setControlSocketEnabled(enabled)
                }
                LabeledContent("Bundled binary", value: tillerctlBundledPath)
                LabeledContent("Control socket", value: ControlSocket.defaultPath())
                LabeledContent {
                    Button(copied ? "Copied" : "Copy install command") {
                        let command = "sudo ln -sf '\(tillerctlBundledPath)' /usr/local/bin/tillerctl"
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(command, forType: .string)
                        copied = true
                    }
                } label: {
                    Text("Install on PATH")
                    Text("Symlinks tillerctl into /usr/local/bin so agents in any shell can reach it.")
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    }
```

Replace it with (adds the new `Section("Agent Skill")` right after `Section("tillerctl")`, before the `Form`'s closing brace):

```swift
            Section("tillerctl") {
                Toggle(isOn: $controlSocketEnabled) {
                    Text("Enable control socket")
                    Text("Required by tillerctl and by agent lifecycle hooks — disabling it degrades agent status badges to title/process detection only.")
                }
                .onChange(of: controlSocketEnabled) { _, enabled in
                    model.setControlSocketEnabled(enabled)
                }
                LabeledContent("Bundled binary", value: tillerctlBundledPath)
                LabeledContent("Control socket", value: ControlSocket.defaultPath())
                LabeledContent {
                    Button(copied ? "Copied" : "Copy install command") {
                        let command = "sudo ln -sf '\(tillerctlBundledPath)' /usr/local/bin/tillerctl"
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(command, forType: .string)
                        copied = true
                    }
                } label: {
                    Text("Install on PATH")
                    Text("Symlinks tillerctl into /usr/local/bin so agents in any shell can reach it.")
                }
            }
            Section("Agent Skill") {
                LabeledContent {
                    Button("Install Skill") {
                        AgentSkillInstaller.openTerminalAndInstall()
                    }
                } label: {
                    Text("tillerctl skill")
                    Text("Installa una skill che insegna a Claude Code, Codex e OpenCode come usare tillerctl per orchestrare pane e worktree.")
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    }
```

- [ ] **Step 2: Build to verify it compiles**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build 2>&1 | tail -20`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Manual verification**

Run the built app (`open Tiller.xcodeproj` and ⌘R, or launch the built `.app` from DerivedData), open Settings → General, confirm:
- New "Agent Skill" section appears directly below "tillerctl".
- Clicking "Install Skill" opens Terminal.app and the command starts running (network access required for `npx` to resolve the package — expect it to actually attempt installing against the live `e-palmisano/tiller` repo).

- [ ] **Step 4: Commit**

```bash
git add App/GeneralSettingsView.swift
git commit -m "feat: add Agent Skill install section to General settings"
```

---

### Task 5: Full CI verification

**Files:** none (verification only)

**Interfaces:**
- Consumes: everything from Tasks 1-4.
- Produces: nothing (terminal task).

- [ ] **Step 1: Run the full CI gate**

Run: `Scripts/ci.sh`
Expected: last line `CI OK`

- [ ] **Step 2: If CI fails, fix forward**

Re-run the specific failing suite (e.g. `cd Packages/TillerCore && swift test --filter AgentSkillInstallTests`) to isolate, fix, then re-run `Scripts/ci.sh` from Step 1 until it prints `CI OK`. Do not skip this step or mark the plan done without a clean `CI OK`.
