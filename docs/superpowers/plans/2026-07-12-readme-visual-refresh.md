# README Visual Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a hero screenshot of the running app and a "Supported Agents" badge row to `README.md`, matching two specific elements of Orca's README while leaving everything else (including the Features bullet list) untouched.

**Architecture:** No application code changes. Two artifacts: one PNG screenshot captured from a live build of the app, and markdown edits to `README.md`. The screenshot is captured by building the app with the existing `Scripts/ci.sh` xcodebuild invocation, launching it, driving the UI via `osascript`/`screencapture`, and visually verifying each step by reading back the captured PNG (there is no scripted shortcut for the "add project" flow, so this step is inherently interactive/visual rather than a fixed command sequence).

**Tech Stack:** Swift/SwiftUI app (unchanged), `xcodebuild`, macOS `screencapture` + `osascript`, `sips` (image inspection), Markdown.

## Global Constraints

- Spec source of truth: `docs/superpowers/specs/2026-07-12-readme-visual-refresh-design.md`.
- Hero image path: `assets/readme-hero.png`, referenced with `width="960"`.
- Agent icon source: Google favicon service, `https://www.google.com/s2/favicons?domain=<domain>&sz=64` — no new binary icon assets.
- Supported Agents section goes after Features, before Install.
- No other README section changes. Features stays a bullet list, not a table.
- Five agents/URLs, exact:
  - Claude Code → `https://docs.anthropic.com/claude/docs/claude-code`
  - Codex → `https://github.com/openai/codex`
  - OpenCode → `https://opencode.ai/docs/cli/`
  - Pi → `https://pi.dev`
  - Oh-My-Pi → `https://omp.sh`

---

### Task 1: Build Tiller and capture the hero screenshot

**Files:**
- Create: `assets/readme-hero.png`
- No source files modified.

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `assets/readme-hero.png` (PNG, hero image), consumed by Task 2's `<img>` tag.

- [ ] **Step 1: Regenerate the Xcode project and build Debug**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -5
```

Expected: build ends with `** BUILD SUCCEEDED **` (or the tail shows no `error:` lines).

- [ ] **Step 2: Launch the built app**

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Expected: the Tiller window appears (first-run onboarding may show — dismiss any
permission prompts that appear by clicking "Not Now"/"Later" equivalents; do not
grant new permissions as part of this task).

- [ ] **Step 3: Take a baseline screenshot and inspect it**

```bash
screencapture -x /tmp/tiller-step.png
```

Read `/tmp/tiller-step.png` (it's a PNG, viewable directly) to see the current
window state — sidebar empty state, whether an "Add Project" affordance is
visible, and its approximate screen position.

- [ ] **Step 4: Add the Tiller repo itself as a project**

Using the affordance found in Step 3 (sidebar "+" button or equivalent — click
via `osascript -e 'tell application "System Events" to click at {x, y}'` with
coordinates read off the screenshot, or `cliclick` if installed), open the add-project
flow and point it at `/Users/enzopiopalmisano/Desktop/Progetti/tiller` (the current
repo — it's already a real git repo with real worktrees, making for an authentic
screenshot). Re-run Step 3's screenshot-and-inspect after each click to confirm
the flow advanced as expected; repeat until the project appears in the sidebar
with its worktree(s) listed.

- [ ] **Step 5: Open a terminal tab and show an agent running**

Select the newly-added project's worktree row, press `⌘T` to open a new terminal
tab (per `README.md`'s existing keyboard shortcuts table), then in that terminal
run:

```bash
claude --help
```

(or `codex --help` / whichever agent CLI is installed locally — the goal is
authentic-looking CLI output in the pane, not a literal running agent). Screenshot
and inspect again to confirm the pane shows readable output, not a blank prompt.

- [ ] **Step 6: Capture the final hero screenshot**

With the sidebar (populated) and terminal pane (showing CLI output) both visible,
get the Tiller window's bounds and capture just that window:

```bash
osascript -e 'tell application "System Events" to tell (first process whose name is "Tiller") to get {position, size} of front window'
```

Use the returned `{x, y}` and `{w, h}` to crop:

```bash
screencapture -R<x>,<y>,<w>,<h> assets/readme-hero.png
```

- [ ] **Step 7: Verify the screenshot**

```bash
sips -g pixelWidth -g pixelHeight assets/readme-hero.png
```

Expected: both dimensions present and greater than 0 (a real window capture, not
an empty/failed one). Open the file with Read to visually confirm the sidebar and
terminal pane are both legible and no permission dialogs or onboarding sheets are
in frame.

- [ ] **Step 8: Quit the app and commit the asset**

```bash
osascript -e 'tell application "Tiller" to quit' 2>/dev/null || killall Tiller
git add assets/readme-hero.png
git commit -m "docs: add README hero screenshot"
```

---

### Task 2: Add the hero image to README

**Files:**
- Modify: `README.md` (badge block, ~line 18)

**Interfaces:**
- Consumes: `assets/readme-hero.png` from Task 1.
- Produces: nothing consumed by later tasks — independent of Task 3.

- [ ] **Step 1: Insert the hero image markup**

In `README.md`, immediately after the closing `</p>` of the existing badge block
(the block containing the macOS/license/Swift/stars/LinkedIn badges) and before
the `---` separator that precedes `## Features`, insert:

```markdown
<p align="center">
  <img src="assets/readme-hero.png" width="960" alt="Tiller running with a project sidebar and an active terminal pane" />
</p>
```

- [ ] **Step 2: Verify rendering**

```bash
grep -n "readme-hero.png" README.md
```

Expected: one match, the `<img>` line just added.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add hero screenshot to README"
```

---

### Task 3: Add the Supported Agents section

**Files:**
- Modify: `README.md` (insert new `## Supported Agents` section between `## Features` and `## Install`)

**Interfaces:**
- Consumes: nothing from Task 1/2 — independent, can be done in parallel with Task 2.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Insert the Supported Agents section**

In `README.md`, find the `---` separator that follows the Features section's
closing line (`**Deliberately not doing:** ...`) and, right after that separator
(before `## Install`), insert:

```markdown
## Supported Agents

<p>
  <a href="https://docs.anthropic.com/claude/docs/claude-code"><kbd><img src="https://www.google.com/s2/favicons?domain=docs.anthropic.com&sz=64" alt="Claude Code logo" width="16" valign="middle" /> Claude Code</kbd></a>&nbsp;
  <a href="https://github.com/openai/codex"><kbd><img src="https://www.google.com/s2/favicons?domain=openai.com&sz=64" alt="Codex logo" width="16" valign="middle" /> Codex</kbd></a>&nbsp;
  <a href="https://opencode.ai/docs/cli/"><kbd><img src="https://www.google.com/s2/favicons?domain=opencode.ai&sz=64" alt="OpenCode logo" width="16" valign="middle" /> OpenCode</kbd></a>&nbsp;
  <a href="https://pi.dev"><kbd><img src="https://www.google.com/s2/favicons?domain=pi.dev&sz=64" alt="Pi logo" width="16" valign="middle" /> Pi</kbd></a>&nbsp;
  <a href="https://omp.sh"><kbd><img src="https://www.google.com/s2/favicons?domain=omp.sh&sz=64" alt="Oh-My-Pi logo" width="16" valign="middle" /> Oh-My-Pi</kbd></a>
</p>

---
```

- [ ] **Step 2: Verify placement and content**

```bash
grep -n "^## " README.md
```

Expected: `## Supported Agents` appears immediately after `## Features`'s content
and before `## Install`, in that order.

```bash
grep -c "s2/favicons?domain=" README.md
```

Expected: `5` (one favicon per agent).

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add Supported Agents section to README"
```

---

### Task 4: Full verification pass

**Files:** none modified — read-only check.

**Interfaces:**
- Consumes: end state of Tasks 1-3.
- Produces: nothing (terminal task).

- [ ] **Step 1: Confirm CI still passes**

No application code changed, but run the repo's gate anyway since it's cheap and
catches accidental unrelated changes:

```bash
Scripts/ci.sh
```

Expected: `CI OK`.

- [ ] **Step 2: Render-check the README**

```bash
grep -n "readme-hero.png\|Supported Agents\|## Features" README.md
```

Expected: hero `<img>` line, `## Supported Agents` heading, and `## Features`
heading all present, with Features preceding Supported Agents in the file.

- [ ] **Step 3: Confirm no stray files**

```bash
git status --short
```

Expected: clean (everything from Tasks 1-3 already committed).
