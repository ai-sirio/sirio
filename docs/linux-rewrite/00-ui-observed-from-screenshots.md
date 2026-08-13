# Tiller macOS — UI observed from screenshots

Direct visual reading of `reference/shots/*.png` (the shipped SwiftUI app). Recorded by the
orchestrator, which is the only participant able to see images; the builder agents derive their
inventories from source and cannot check them against these frames.

**What this document is:** the *functional* contract — what the app lets a person do, and what
state it shows them. It is evidence for the "done" checklist.

**What this document is NOT:** the visual target. The rewrite takes its look from waku. Nothing
here should be reproduced because it looked like this; reproduce it only because the *capability*
is on the inventory. Where this file describes colour, radius or chrome, treat it as a record of
the old app, not as a spec.

Frames read: 01-chat-empty, 02-project-expanded, 03-new-tab-menu, 04-terminal-pane,
06-settings, 16-chat-streaming, 20-changes-expanded-hunks, 21-context-ring-popover.

---

## Window shell

Three columns plus a full-width status bar. At a 1470pt window: left sidebar ~325pt (22%),
centre ~740pt, right panel ~405pt, status bar ~28pt tall.

Title bar is the macOS one — traffic lights top-left, then a sidebar-toggle glyph. Top-right
carries two panel-toggle glyphs and a shield (permissions). There is no in-window menu bar,
because on macOS the menu lives in the system bar.

> PLATFORM: this is the single biggest structural unknown for Linux. Traffic lights, the system
> menu bar, and the unified title bar have no equivalent. Decide deliberately: client-side
> decorations with our own close/minimise/maximise, or server-side decorations. Every frame
> below assumes chrome that Linux will not hand us for free.

## Left sidebar — projects

- Header row: `Projects` + `+` (add project).
- Filter field, magnifier icon, placeholder `Filter`.
- Project rows: disclosure chevron, folder icon **tinted per project** (blue, purple, red,
  green observed), name wrapping to two lines when long. A gear appears at row-right on the
  selected/hovered project.
- Expanding a project reveals its worktrees (branch glyph + name, e.g. `main`), and expanding a
  worktree reveals its open tabs (`Chat`, `Terminal`) with per-agent glyphs. Last row is
  `+ New Worktree…`.
- Selection is a rounded filled rect on the row.
- Activity is shown *on the worktree row*: `•••` at the left and the agent glyph at the right
  while running; a green dot next to the branch glyph after work finishes.

## Centre — tabs and chat

- Tab strip: icon + title per tab, active tab marked by a rule above it, `+` at far right.
- Tab title area doubles as status: `•••` while streaming, `✓` when the turn completed.
- `+` opens a menu, in this order: `New Terminal` — then the agents `Claude Code`, `Codex`,
  `OpenCode`, `Pi`, `Oh-My-Pi`, each with its brand glyph — then `New Browser` — then
  `New Chat ›` (submenu).
- Transcript: the user turn is a rounded grey block spanning the column; the assistant reply is
  plain text with no container. Below the reply, a timestamp and a copy button. Turns are
  separated by a hairline rule with the time centred on it.
- In-flight indicator is `•••  Thinking`.
- Composer: large rounded box, border lights up (amber) on focus. Placeholder is `Message…`,
  and becomes `Type to queue for the next turn…` while a turn is running — i.e. typing during a
  run queues rather than interrupts.
- Composer controls, left to right: `+` (attach); either a status pill (`● idle`) or a mode
  dropdown (`Ask`); then `…` overflow, a **context ring** (donut), the model picker
  (`Claude Code`, `Default (recommended)` ▾), and the send button — a circled up-arrow that
  becomes a **red square stop** while running.

### Context ring popover (frame 21)

Clicking the ring opens a card: `9% of context used`, `86.648 / 1.000.000 tokens`,
`Cost: 0,52 US$`, and a breakdown line `Input: 2 · Output: 5 · Cache write: 86.641 · Cache read: 0`.
Numbers are formatted in the user's locale (`.` thousands, `,` decimal).

## Right panel — inspector

Segmented `Files` | `Changes` (active segment filled blue) and an `X` that closes the panel.

**Files:** breadcrumb of the absolute worktree path + refresh; scrollable tree with chevrons;
directories containing modifications carry an amber dot at row-right and an amber-tinted name.

**Changes:** `Local changes` + refresh; `Changes (3)` with `Stage all` / `Discard all`; each file
row has chevron, status glyph, amber path, per-file `Discard` / `Stage`, a `<>` glyph, a trash
glyph, and a `−3 +25` counter. Expanding shows a unified diff with both-side line numbers, the
`@@` header highlighted, and added lines on a green ground.

**Activity** is pinned under whichever tab is selected: header with a `N running` counter, then
one row per open pane *across all worktrees* — icon, title, `project/worktree` subtitle, a status
glyph and an `X` to close that pane.

## Status bar

Left: a settings glyph, a refresh glyph, then one segment per provider — glyph, name and usage
figures in a rolling-window format: `Claude 48% 5h · 40% wk`, `Codex 11% 5h`,
`OpenCode Go 0% 5h · 70% mo`. Right: the active worktree and its path, `main · ~/Desktop/Progetti/tiller`.

## Settings (frame 06)

A full-window takeover, not a sheet or modal: header is `‹ Back   Settings`. Left nav:
`AI Providers`, `Agents`, `General`, `Permissions`, `Appearance`.

The AI Providers pane is a stack of per-provider cards (`Claude Code`, `Codex`, `OpenCode Go`, …),
each with: `Status` + `● Active`, `Last read <time>`, `Show in usage bar` toggle,
`Refresh interval` stepper (`5 min`), a `Refresh now` button, and an `Accounts` subsection with
`Add Account` plus account rows badged `This device` / `Active`.

## Behaviours implied by the frames

These are the entries most easily missed by reading source alone, and each is directly visible
above:

1. Typing during a run **queues** the next turn instead of interrupting it.
2. The send button is modal — send vs. stop — and the stop affordance is red.
3. Activity is tracked per-pane and aggregated globally, not per-tab.
4. Changed-file state propagates *up* the file tree as an amber dot on ancestor directories.
5. Projects carry a user-visible colour identity.
6. Usage/quota is always-on chrome, not a screen you navigate to.
7. Cost and token accounting are surfaced live, per turn, with cache reads and writes split out.

## Not yet read

Frames not inspected at time of writing: 05, 07, 09-15, 17, 19, 22, 23 — settings subpages,
light theme, model picker, composer focus, chat response, changes list, post-escape state.
