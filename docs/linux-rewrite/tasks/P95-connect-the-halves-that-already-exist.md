# P95 — six rows whose expensive half is already written

**Owner: `codex11`.** Start when `P93` is finished. Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

## Why these are one piece

Tonight's audit produced a reframing worth stating once: `DEAD-MODULES.md` and the
`FAILED — absent` rows are **two indexes of the same defects** — one by code, one by clause. Every
item below is a module that exists, is tested, and is not connected to the row that promised it.
The missing work is a subscription, a call site or a handler. **You are not building features here;
you are closing seams.**

Line numbers drift under four concurrent agents — `settings.rs` moved ~700 lines while I was
writing this. **Every citation below is a needle. Grep for it, do not trust the number.**

## 1. `on_install_skill` — one call site, closes `F-SET-09`

`settings.rs` has six callback seams. **Four are wired by the host and two are not** — that ratio is
the point, because it proves the search works rather than the host being generally unwired:

| seam | `main.rs` |
|---|---|
| `on_back`, `on_change`, `on_revoke_browser_origin`, `on_revoke_all_browser_origins` | wired |
| `on_manage_account` | **0** — `P93` is closing it |
| `on_install_skill` | **0** — this row |

After `P93` this is the last unset seam in the file. The provisioner already exists and is tested:
`tiller_project::agent_skill_install_command` (the `npx skills add …` command). The field's own doc
comment states the failure mode — *"Unset, the Install Skill button renders muted and does not
respond"*. `F-SET-09` is already `FAILED — defective` with exactly this diagnosis, so the ledger
agrees with you before you start.

While you are in the file: two handlers are the literal `|_, _, _| {}` — one on a
`refresh-permissions` **Refresh** button, one nearby. Find their rows (grep the clause files, not
the ledger — see §"How to find the row" below) and say in your report which rows they belong to,
even if you do not wire them. An identified dead control is worth more than an anonymous one.

## 2. `TerminalLinkEvent` has no subscriber — closes `F-TERM-UI-02`

Everything works except the last hop. `on_left_mouse_down` in `tiller_terminal/src/lib.rs` gates on
`opens_terminal_link(event.modifiers.platform)` (Super on Linux), computes row/column, calls
`link_at` — which reads an **OSC 8 hyperlink** from the cell and falls back to `url_at_column`
plain-text detection — and emits:

```rust
cx.emit(TerminalLinkEvent { target: self.identity.clone(), url });
```

`target` carries the clicked pane's identity, which is precisely the clause's requirement
(*"routed to the owning terminal view state rather than globally"*). **`TerminalLinkEvent` has
exactly three references in the workspace: the struct, this emit, and the `EventEmitter` impl.**
Nothing subscribes, so Super+click does nothing — `critic2` confirmed that live with screenshots.

The precedent for the subscription is in the same file you would edit: `subscribe_terminal_activity`
(`main.rs`, called from the terminal-construction path). The platform can open URLs already —
`cx.open_url` is used in `chat.rs`, and `xdg-open` in `main.rs` and `editor.rs`.

The clause wants **two panes** and each opening through its own pane's router, so wire it in a way
that keeps `target` meaningful rather than routing through one global handler.

## 3. `terminal_file_drop` — one dead module, three rows

`tiller_project/file.rs` holds `classify_file_drop` and `terminal_file_drop`, built and tested, with
zero production callers. Three rows own it:

- `F-CORE-FILE-03` — its clause *is* a prose description of the function: *"Accepted terminal file
  drops become one shell-quoted, space-separated path string with no trailing newline and are
  written to the pane."*
- `F-TERM-PTY-06` (`UNREACHABLE`) — names `tiller_project::terminal_file_drop` in its own evidence.
- `F-EDIT-12` — the drag source half.

**A near-trap recorded by the census, so you do not repeat it:** `shell_quote` in `tiller_agents` is
a *different* quoting function with real callers. The write-a-path-to-a-pane quoting is the one with
none. Do not let the wrong symbol convince you this is already wired.

`F-EDIT-12` also carries a correction from `P81`: the drag payload is the pair `(PathBuf, String)`,
not a bare `PathBuf`, and `changes.rs` already drags it while `tiller_terminal` already receives it.
So the changes-list → terminal path may be closer to done than the file-explorer one. **Establish
which of the two source surfaces is live before building either.**

## 4. `FileSystemEventMonitor` → editor auto-reload — closes half of `F-CORE-FILE-06`

`tiller_markdown/file_events.rs` implements the inotify watcher; it has zero consumers outside its
own crate. `F-CORE-FILE-06` owns it explicitly — the clause requires *"auto-reloads external changes
unless local edits require a conflict; external deletion/rename is surfaced"* and its PLATFORM note
says *"Linux needs inotify/fanotify or equivalent"*.

**Read `F-EDIT-05`'s ledger evidence before touching this.** The editor already has a
change-on-disk path: focus-regain triggers `check_external`, a dirty buffer gets a
`Reload`/`Keep` banner, and a clean buffer adopts silently. So part of the clause is live through a
*different* mechanism. Your job is the watcher-driven half (no focus change required) plus
**deletion/rename surfacing**, which nothing implements. Say clearly in your report which half you
closed — this row will otherwise collect the same "one half proven" ambiguity that cost us four
rows tonight.

## 5. The Files context menu ignores the pointer — a defect with no row

`right_panel.rs`: `struct FileContextMenu { path: PathBuf }` has **no position field**;
`open_file_context_menu` never receives the click point, and the menu is placed at a hardcoded
`.left(theme.spacing.titlebar_control_spacing)` / `.top(px(HEADER_HEIGHT + TOOLBAR_HEIGHT))`. Observed
live: right-clicking a file at y≈841 opened the menu at y≈149.

**The correct implementation is already in this repo**, in `tiller_terminal/src/lib.rs`: store
`event.position` on right-click, then `.left(position.x)` / `.top(position.y)`. Copy that shape —
it is ours, so this is not a transplant. Three lines.

Distinct from the P17 z-order overlap already recorded (that was the panel painting *over* the
menu); this is the menu appearing in the wrong *place*.

## How to find the row that owns a piece of code

Grep `01-inventory-app.md` and `02-inventory-packages.md` — **the clause files** — not the ledger.
A clause was written before the code and describes behaviour in prose, so it never contains symbol
names; the ledger records what was judged. Three "orphan" modules turned out to be owned this way
today, two of them by clauses that quote the dead function's behaviour almost word for word.

## The rules

- **Inspiration, never code.** waku, Zed, orca and comet are to look at, not to copy. A critic
  finding transplanted code counts as a gap, always. Copying *our own* patterns between our own
  files (§5) is not a transplant.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`.** Every item here is *made of*
  dead-but-tested code — a passing test is exactly what all six halves already had.
- **Do not edit `INVENTORY-LEDGER.md`.** Report what you built and what you exercised.
- Commit **path-scoped** (`git add <your paths>`, never `-A`) — three other agents have uncommitted
  work in this worktree, and this repo has already lost its object database once.
- `git status --short | grep '??'` before you finish.
- Prefer `cargo test -p <crate>` while the roster is busy; a red `--workspace` gate is usually
  someone else's intermediate state, and **a red gate needs its cause attributed before it is
  reported**.

## Done means

1. Each item either connected and exercised live, or reported unbuildable **with the evidence**.
2. For every row you touch, state **which half** you closed and which remains — these are all
   conjunctive clauses and a single "done" hides the other half.
3. Exercised live, not only under test: launch the app, perform the gesture, say what you observed.
   Take the drive lock (`ENVIRONMENT.md` §"Holding the lock yourself"); `fable` and `sonnet` drive too.
4. Do not idle on an approval gate — see `ENVIRONMENT.md` §"Working with the orchestrator".
