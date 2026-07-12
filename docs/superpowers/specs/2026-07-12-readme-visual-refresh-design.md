# README Visual Refresh — Design

**Date:** 2026-07-12
**Status:** Approved

## Problem

`README.md` already has a solid structure (logo, badges, features, install, FAQ,
contributing, acknowledgements) but is text-only — no screenshots of the app, no
visual sense of what running Tiller actually looks like, and no at-a-glance list of
which agent CLIs it supports. [Orca](https://github.com/stablyai/orca), the project
Tiller is forked from, solves both with a hero screenshot under its badge row and a
row of linked agent-logo badges under "Supported Agents."

## Goal

Bring over two specific elements from Orca's README, nothing else:

1. A hero screenshot of the running app.
2. A "Supported Agents" section: one badge per agent, real icon + name, linked to
   the agent's own site.

Everything else in the README — including the Features bullet list — stays as-is.
Explicitly not doing Orca's per-feature table-with-GIF layout.

## Decisions

1. **Hero image placement and content.** Inserted directly below the existing badge
   row (`<p align="center">` block), above the `---` separator that precedes
   Features. Shows the main window: sidebar with a project/worktree list, terminal
   pane with an agent actively running. Saved as `assets/readme-hero.png`,
   referenced with `<img src="assets/readme-hero.png" width="960" alt="..." />`
   matching Orca's hero sizing convention.

2. **Capture method.** Build the app (`xcodegen generate` if needed, then build via
   Xcode/xcodebuild), launch it against a real worktree with an agent running in a
   terminal pane, then capture the window with macOS `screencapture`. This is a live
   desktop action (launches a real process, takes a screen capture of the user's
   session) — confirm with the user immediately before doing it, even though the
   design itself is already approved.

3. **Supported Agents section.** New section inserted after Features, before
   Install (mirrors Orca's ordering: Features → Supported Agents → Install). One
   row of `<kbd><img .../> Name</kbd>` badges wrapped in links to each agent's
   site, one entry per adapter in `AgentCatalog`:
   - Claude Code → `https://docs.anthropic.com/claude/docs/claude-code`
   - Codex → `https://github.com/openai/codex`
   - OpenCode → `https://opencode.ai/docs/cli/`
   - Pi → `https://pi.dev`
   - Oh-My-Pi → `https://omp.sh`

   Icons sourced the same way Orca sources its own third-party CLI icons: Google's
   favicon service (`https://www.google.com/s2/favicons?domain=<domain>&sz=64`) for
   each, since none of these five already have a bundled logo asset in the Tiller
   repo (`assets/` only holds `tiller-logo.png`). No new binary assets to maintain
   beyond the one hero screenshot.

4. **No other structural changes.** Existing badges, Features bullet list, Install,
   First Launch, Keyboard Shortcuts, Repository Layout, Building from Source, FAQ,
   Contributing, License, and Acknowledgements sections are untouched.

## Out of scope

- Per-feature screenshot/GIF table (Orca's Features layout) — explicitly excluded.
- Additional screenshots beyond the single hero image.
- Discord/X badges, translated README variants, mobile companion references — none
  of these apply to Tiller.
