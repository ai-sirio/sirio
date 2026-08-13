# P76 — The comet top bar, and the icon set that goes with it

**This brief is everything you need; your context may have just reset. It comes from a direct user
instruction and it outranks P75 — do this first.**

## What the user asked for

Two things, in their words: take the icons from `https://github.com/zeronsh/comet.git`, and *"vorrei
la stessa top bar (la barra dove ci sono i 3 semafori)"* — the same top bar as the screenshot they
supplied, the one with the three traffic lights.

This is a visual directive from the user about the titlebar specifically. Where it and the standing
**Pop!\_OS COSMIC** bar disagree **for the titlebar**, the user's screenshot wins — they were explicit.
COSMIC remains the system everywhere else, and the tokens you already built are still the vocabulary
this must be assembled from.

## The one fact that changes the job — read before designing

**Neither comet nor Tiller draws traffic lights on Linux. Those three dots are macOS's own window
buttons.** Both codebases say so outright:

- comet, `crates/ui/src/shell.rs:74-79` — *"macOS only — on Linux/Windows there are no traffic lights
  and the cluster hugs the edge"*; `cluster_buttons_start` returns **10.0** on Linux against **88.0**
  on macOS.
- Tiller, `tiller_ui/src/titlebar.rs:1-6` — *"the transparent, GPUI-owned controls that sit beside
  macOS traffic lights … On Linux nothing occupies the macOS traffic-light [space]"*.

So the bar in the screenshot is, on Linux, a bar we have to **author**: GPUI on X11 gives us a
window, not decorations. The user asked for the traffic lights, so **draw them** — three circular
controls at the left that close, minimise and maximise. Do not substitute COSMIC-style square window
controls; that is not what was asked for. Do not skip them either.

This is the piece neither reference hands you, so it is also the piece most likely to be quietly
dropped. It is the first thing your report must answer.

## Measured spec, from comet's source rather than from the picture

| what | value | where |
|---|---|---|
| titlebar height | **38 px** | `Theme::TITLEBAR_HEIGHT`, comet `theme.rs:315` |
| control cluster | **three 24 px buttons, 2 px gaps** (76 px) | `CLUSTER_BUTTONS_WIDTH`, `shell.rs` |
| traffic lights (macOS) | at **{14, 15}**, cluster starts at **88 px** | `titlebar_cluster_start` |
| cluster start (Linux, comet) | **10 px**, no lights | `cluster_buttons_start` |

For Tiller on Linux **with** lights, the lights take the inset and the cluster starts after them —
you own that number; derive it, do not copy 88.

Row contents, left to right, as the screenshot shows them:

1. three traffic lights
2. the button cluster — sidebar toggle, back, forward — then `+`
3. an accent glyph, then the title in near-white
4. a muted secondary string beside it (`comet @ personal-metal` — for us, project @ worktree)
5. far right, over the panel: a scope dropdown pill, a branch pill, collapse, expand, panel toggle

It is **one continuous surface** — no border under the bar, no separate window frame, chrome and
content share a single dark ground. That unity is what makes it read as this app rather than a
generic title bar, and a 1px divider under it would undo the whole effect.

## The icons are already imported — 63 of them

`rust/assets/icons/comet/`, committed, with `ATTRIBUTION.md` beside them. **MIT, Copyright (c) 2026
Wing.** Assets only — no comet source was copied, and none may be.

They drop into the existing pipeline untouched: `viewBox="0 0 16 16"`, `stroke="currentColor"` at
`1.25`, round caps, and `fill="currentColor"` for the brand marks — so every one tints through
`paint_tinted_svg` in `tiller_ui/src/icons.rs` with no edit.

**This is a replacement, not an addition.** Tiller's 22 are Phosphor *thin* (`caret-down-thin`,
`chat-circle-thin`); comet's are Solar (`alt-arrow-down`, `settings-minimalistic`). Mixed, they read
as an unfinished port. Map every existing variant:

| `Icon::` | today | comet |
|---|---|---|
| `ChevronDown` / `Right` / `Left` | `caret-*-thin` | `alt-arrow-down` / `-right` / `-left` |
| `Close` | `x-thin` | `close` |
| `Plus` | `plus-thin` | `plus` |
| `Settings` | `gear-thin` | `settings-minimalistic` |
| `RefreshCw` | `arrow-clockwise-thin` | `refresh` (or `restart`) |
| `File` | `file-thin` | `document` |
| `FolderFill` | `folder-fill` | `folder` / `folder-with-files` |
| `GitBranch` | `git-branch-thin` | `git-branch` |
| `MessageSquare` | `chat-circle-thin` | `chat-round-line` |
| `SquareTerminal` | `terminal-window-thin` | `terminal` |
| `Globe` | `globe-thin` | `global` |
| `ClaudeCode` | `agent-claude` | **`claude-mark`** |
| `Codex` | `agent-codex` | **`openai-mark`** |
| `Pi` | `agent-pi` | **`pi-mark`** |

**Five have no equivalent and are yours to decide, not to guess silently:** `Sparkles`, `Shield`,
`SunMoon`, `OpenCode`, `OhMyPi`. Comet ships marks for Claude, OpenAI and Pi but none for opencode or
omp. Keeping the two Phosphor agent glyphs for those two is a defensible answer — agent marks are
brand identity, not set iconography, and nobody reads them as arrows. **Say which you chose and why**;
do not leave a silent mix in the generic icons.

Also newly available and worth using rather than re-drawing: `sidebar-minimalistic-left` (the exact
sidebar toggle in the screenshot), `expand-arrows`, `fold-vertical`, `magnifer`, `tuning`,
`danger-triangle`, `wifi-off`, `key-minimalistic`, `command`, `keyboard`, `stop`, `check`.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump) for the bar's structure
and for each window control actually doing its job — a close button that renders and does not close
is precisely the dead-control shape P75 exists to fix, and shipping a new one while fixing four old
ones would be a poor trade.

**And a screenshot.** This is a visual piece; a passing layout test does not show whether it looks
like the reference. Put the image in `reference/linux-progress/` — **not `/tmp`**, which is why the
critic's evidence is currently not replayable.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your
  pane may start in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Yours:** `titlebar.rs`, `controls.rs`, `composer.rs`, `tiller_theme/**`, `settings.rs` (new, P75),
  and **`icons.rs`** (claimed for you by this brief — say so in your report).
- **Do not edit** `chat.rs`, `sidebar.rs`, `status_bar.rs` (`pi`); `main.rs`, `session.rs`,
  `tiller_control/**`, `tab_bar.rs` (`codex12`); `changes.rs`, `right_panel.rs`, `editor.rs`,
  `file_view.rs`, `tiller_git/**`, `tiller_terminal/**`, `tiller_acp/**`, `tiller_agents/**`,
  `browser.rs` (`codex11`). If the bar needs the shell to give it something, **name it as a seam for
  `codex12`** rather than crossing into `main.rs`.
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those. Widening `Icon` will break arms across `tiller_ui`; those arms are yours to fix, and only
  those.
- **Never copy comet's Rust.** The icons are artwork and were explicitly requested; the code is not.
  Read it for dimensions and behaviour, then write your own. Transplanted code is a gap, always.
- Colours, spacing and radii from `tiller_theme::Theme`, **never a literal** — including the traffic
  lights' three colours and the bar height. You are the author of that vocabulary; add what is
  missing. Still outstanding from others: `menu-width`, `geometric-hairline`, `compact-action`.
- **Establish the build state with the gate's own commands.** Measured by the orchestrator: `cargo fmt
  --all -- --check` green, `cargo clippy --workspace --all-targets --exclude tiller --exclude
  tiller_ui -- -D warnings` = EXIT 0, `cargo build -p tiller -p tiller_control` = EXIT 0. System GTK
  headers are installed globally — no `PKG_CONFIG_PATH`, no sysroot.
- Mark rows `builder-claimed, unverified`, **never** `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: whether the three window controls actually close / minimise / maximise and the
test names that prove it; the screenshot path; the derived cluster-start number for Linux-with-lights
and how you derived it; which five unmapped icons you decided and why; whether any Phosphor icon
survives and where; tokens you added; any seam left for `codex12`; the gate run with its own
invocation; and the honest remainder.
