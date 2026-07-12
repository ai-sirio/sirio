# tillerctl agent skill + Settings install — Design

Date: 2026-07-12
Status: approved (brainstorming session)

## Goal

Give coding agents (Claude Code, Codex, OpenCode) running inside a Tiller
pane a packaged, discoverable skill that teaches them how to use `tillerctl`
to orchestrate other panes/worktrees — instead of relying on an agent
happening to already know the CLI exists. Make it one click to install from
Tiller's own Settings.

## Skill content

New file `skills/tillerctl-cli/SKILL.md` at the repo root (matches the
`skills add`/`skills init <name>` convention: `<name>/SKILL.md`).

Frontmatter:

```yaml
name: tillerctl-cli
description: Use when running inside a Tiller pane and you need to orchestrate other panes/worktrees — list/create/split panes, send text or keys to another terminal, check your own worktree/pane context, or post notifications back to Tiller.
```

Body covers:

- **Self-identification** — `tillerctl identify` resolves the caller's own
  worktree/pane from the `$TILLER_WORKTREE_ID`/`$TILLER_PANE_ID` environment
  variables Tiller injects into every pane.
- **Command reference** — workspace commands (`list-workspaces`,
  `new-workspace`, `select-workspace`, `current-workspace`,
  `close-workspace`), surface commands (`list-panels`, `list-pane-surfaces`,
  `new-split`, `send`, `send-key`, `focus-panel`), notification commands
  (`notify`, `list-notifications`, `clear-notifications`),
  `restore-session`.
- **Key-input gotcha** — for raw-mode interactive TUIs (most coding agents),
  use `send-key <surface> enter` to submit input (sends `\r`), not
  `panel write --enter`/plain `send` for the confirming keystroke. Documents
  the byte-level reason (canonical vs raw terminal mode) so the agent
  doesn't rediscover the bug fixed in `Tillerctl.swift`'s `Panel.Write`.
- **When to reach for it vs. not** — orchestrating a second agent/pane,
  programmatic splits, cross-pane notifications: yes. Talking to your own
  pane, where direct output/input already works: no.

No automated test for this file — it's prompt content, reviewed manually
like any other documentation.

## Settings UI

`App/GeneralSettingsView.swift` gets a new `Section("Agent Skill")`
immediately after the existing `Section("tillerctl")` (same screen, same
place agents' PATH wiring already lives):

```swift
Section("Agent Skill") {
    Text("Installa una skill che insegna a Claude Code, Codex e OpenCode come usare tillerctl per orchestrare pane e worktree.")
    Button("Install Skill") {
        AgentSkillInstaller.openTerminalAndInstall()
    }
}
```

No installed/not-installed state is tracked or persisted — `skills add` is
idempotent, and the user can re-run to pick up an updated skill. Out of
scope: detecting whether the skill is already installed.

## Terminal launch

New file `App/AgentSkillInstaller.swift`:

```swift
enum AgentSkillInstaller {
    static let installCommand =
        "npx skills add e-palmisano/tiller --skill tillerctl-cli -a claude-code,codex,opencode -y"

    static func openTerminalAndInstall() {
        let script = """
        tell application "Terminal"
            activate
            do script "\(installCommand)"
        end tell
        """
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", script]
        try? process.run()
    }
}
```

`installCommand` is a static string with no user input interpolated into
the AppleScript literal, so there is no injection surface. Clicking
"Install Skill" opens Terminal.app and runs the command immediately (no
copy-paste step) — matches the existing `AgentAccountStore.runProcess`
`Process`-based style already used in `App/`.

Command: `npx skills add e-palmisano/tiller --skill tillerctl-cli -a
claude-code,codex,opencode -y`, using the [vercel-labs/skills](https://github.com/vercel-labs/skills)
CLI (`npx skills add <owner/repo> --skill <name> -a <agents> -y`, confirmed
via `npx skills add --help`). Requires the user to have `npx`/Node and git
access to `github.com/e-palmisano/tiller` locally — same access they
already need to work on the repo.

## Testing

- `AgentSkillInstaller.installCommand` — swift-testing `#expect` asserting
  the exact string (repo, skill name, agent list, flags).
- `AgentSkillInstaller.openTerminalAndInstall()` — not unit-tested (real
  side effect, opens an external app); verified manually.
- `SKILL.md` — manual review only.

## Out of scope

- Detecting/reporting whether the skill is already installed.
- Supporting agents beyond Claude Code, Codex, OpenCode (Pi/omp have no
  skill-consumption mechanism today).
- A "prefill but don't run" fallback mode — Terminal always runs the
  command immediately per user decision.
