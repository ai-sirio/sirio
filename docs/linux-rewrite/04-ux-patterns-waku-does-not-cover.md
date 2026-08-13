# UX patterns waku does not cover

waku is the visual bar, but it is a smaller program than Tiller: two columns, one conversation,
no diff surface, no terminal panes, no file tree, no multi-worktree tree. Those affordances still
have to exist, and copying waku cannot tell us how they should feel. For them the goal points at
two other references.

**These two are TypeScript/Electron applications.** Tiller is native GPUI — no webview, no HTML.
So they are usable **only** as evidence of interaction design. Nothing about their implementation
transfers, and there is nothing in them to transplant even if one wanted to.

| Reference | What it is | Why it is here |
|---|---|---|
| `_tiller-refs/orca` | `stablyai/orca`, ~10.9k TS files | **Tiller's direct ancestor** — `CLAUDE.md` describes Tiller as "a fork of Orca with reduced scope". Its surfaces are the ones Tiller inherited. |
| `_tiller-refs/t3code` | `pingdotgg/t3code`, ~13.2k TS files | Second opinion on the same surfaces. Note: its large images live under vendored `.repos/` and are other projects' screenshots, not its own UI. |

## Read so far

### orca — `docs/assets/feature-wall/annotate-diff.jpg`

The diff surface, which is the single biggest hole in waku as a reference.

- Changes open as **their own tab** (`All Changes`), not as a side panel. The tab strip is the
  same one that holds conversations, so a diff is a peer of a chat rather than an inspector.
- Header states the scope plainly: `8 changed files`, with `Collapse All` / `Expand All` on the
  right. Per-file header carries the path and its `+8 -10` counts.
- **Unchanged context is collapsed and labelled by size** — `18 hidden lines`, `37 hidden lines`
  — as a clickable band rather than a silent gap. The reader always knows how much was skipped.
- Unified diff with both old and new line numbers in separate gutters; removed lines on a red
  ground, added on green, and the marker (`-`/`+`) kept inside the text column.
- The cursor in this frame sits on a diff line for **annotation** — the diff is an input surface,
  not a read-only report.

Worth stealing conceptually: collapsed-context bands that state their own size, and treating
Changes as a first-class tab. Tiller's macOS build put Changes in the right inspector
(`00-ui-observed-from-screenshots.md`); orca's placement is the stronger idea and costs nothing
to adopt while the shell is being rebuilt anyway.

## Not yet read

orca's `cli-agents`, `codex-accounts`, `design-mode`, `file-drag`, `github-linear`,
`keyboard-native`, `markdown-editor` frames; t3code's own UI frames. Pull these when the piece
that needs them comes up rather than front-loading the reading.
