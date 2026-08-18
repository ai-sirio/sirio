# FINISH — tabs shard, part 3 (the remaining 16 rows)

Lane `wf-tab2`. Continuation of `docs/linux-rewrite/FINISH-tabs-part2.md` (lane `wf-tab`,
which closed F-TAB-01 and F-TAB-22 `PASSED` before its API-error kill). This shard drives
the 16 rows assigned to `wf-tab2`: F-TAB-07, 08, 09, 10, 11, 12, 14, 18, 19, 20, 23, 24,
25, 26, 27, 28.

Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-tab2-tiller
export TILLER_WL_BIN=/tmp/wf-tab2-tiller TILLER_WL_LABEL=wf-tab2
```

Driven via `Scripts/wayland-drive.sh` under `TILLER_WL_KEEP=1`, continued against the same
live instance with a small continuation driver
(`/tmp/.../scratchpad/wf-tab2-run.sh`) that re-derives `SOCK`/`WAYLAND_DISPLAY`/`SWAYSOCK`
from the fixed `wf-tab2` label and starts the persistent virtual pointer/keyboard the first
time a gesture is needed (idempotent — `wayland-drive.sh` itself refuses to be re-invoked
against a live instance because its own top-of-script `kill_ours` would tear the instance
down). Project under test: a disposable fixture repo `/home/enzopalmisano/wf-tab2-fixture`
(`app.py`, `extra.py`/`extra2.py`/`extra3.py`, `README.md`, one `master` branch), isolated
from the real `tiller-linux` checkout. Captures under
`/tmp/.../scratchpad/wf-tab2-shots/` and `wf-tab2b-shots/` (ephemeral, not committed).

A second, separate instance (`wf-tab2b`) was booted once, with `PATH` stripped to
`/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin` (verified by `which claude
codex` returning nothing under that `PATH`, in the same shell that launched the app) for
F-TAB-08's "no installed ACP agents" clause, which needs the state fixed at `TabBar::new()`
launch time, then torn down once that row closed.

## Source-grounding notes (read before the per-row results)

- **The "pane menu" in this port is the terminal pane body's own right-click context menu**
  (`rust/crates/tiller_terminal/src/context_menu.rs`'s `ITEMS`, rendered in
  `tiller_terminal/src/lib.rs`), distinct from the **tab-strip's** right-click context menu
  (`rust/crates/tiller/src/main.rs`'s `tab_context_items()`, drawn via
  `tiller_ui::tab_bar::render_tab_context_menu`). macOS's `SplitContentMenu` clauses
  (F-TAB-09/10/11/12/23/25/26/27) land on whichever of the two this port actually wired the
  action to — confirmed per-row below by reading the real dispatch site, not assumed from
  the macOS SRC file name.
- The terminal context menu's 13 items, in the fixed on-screen order
  (`context_menu.rs::ITEMS`): **0 Copy, 1 Paste, 2 Copy Context, 3 Set Title, 4 Copy Pane
  ID, 5 Copy Terminal ID, 6 Split Left, 7 Split Right, 8 Split Above, 9 Split Down, 10 Clear
  Terminal, 11 Restart Terminal, 12 Close Terminal…**. Rows citing "index N" below cite this
  fixed order.

## Per-row result

### F-TAB-07 — Create an ACP chat from the New Chat submenu

**PASSED.** `+` → New Chat expands an inline chevron submenu listing every ACP-chat-capable
adapter (`tiller_ui/src/tab_bar.rs`'s `available_chat_agents` — gated on `is_available() &&
acp_program().is_some()`, which in this build resolves to exactly two: Claude Code and
Codex). Clicked "Claude Code": a new "Claude Code" tab appeared in both the tab strip and
the sidebar's worktree tree, composer showing `connecting → Claude Code`
(`setup4-05-chat-tab-created.png`, tab count 0→1). Reopened `+` → New Chat → clicked
"Codex": a second, independent "Codex" tab appeared alongside the first — both tabs visible
simultaneously with distinct icons/titles in the strip and the sidebar
(`setup5-08-codex-chat-created.png`, tab count 1→2). Hard discriminator: two structurally
distinct tab rows exist post-click where zero existed before, each named after the agent
clicked.

### F-TAB-08 — See the no-installed-agent fallback in the New Chat submenu

**PASSED.** Booted a fresh instance (`wf-tab2b`) with the launching shell's `PATH` stripped
of `~/.local/bin` (where `claude` and `codex` live — confirmed `which claude codex` return
nothing under the stripped `PATH` in the same shell used to launch the app; `opencode`/`pi`
live under `~/.nvm/...`, also stripped). `+` → New Chat now renders `render_chat_empty`:
"Other agents…" / "No supported agent found on PATH" in place of any agent row
(`setup-01-newchat-empty.png`). Clicked it: the whole surface switched to
**Settings → Agents**, listing all five adapters each tagged `Not found on PATH`, Claude
Code and Codex additionally tagged `ACP chat available` vs. OpenCode/Pi/Oh-My-Pi's `No ACP
server` (`setup2-02-other-agents-clicked.png`). Hard discriminator: the visible surface
changed from the tab-bar's New Chat popover to a completely different screen (Settings,
with its own `Back` control and left-hand section list) — not a state flag, a navigation.

### F-TAB-09 — Open a file tab from the pane menu

(continued below — driving in progress)
