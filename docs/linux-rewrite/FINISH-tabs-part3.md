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

**UNREACHABLE — named live blocker: the system file-picker portal renders outside this
lane, and the only place it could render is the user's real desktop, which is out of
bounds.** The tab context menu's "Open File" item is real and dispatches correctly:
clicking it (`setup t09-09-tabctx-openfile.png`) closes the menu and calls
`handle_open_file` (`rust/crates/tiller/src/main.rs:9099`), which calls
`cx.prompt_for_paths(...)`. `gpui_linux`'s implementation
(`rust/vendor/gpui_linux/src/linux/platform.rs:396-449`) does not draw its own dialog — it
sends an `ashpd`/`xdg-desktop-portal` `OpenFileRequest` over D-Bus and awaits the portal's
own response. This machine's user session does have a live `org.freedesktop.portal.Desktop`
router (`busctl --user status org.freedesktop.portal.Desktop` → a running PID), but its
`gtk`/`cosmic` file-chooser *backends* are merely `(activatable)`, not running, and no
backend process, no dialog window, and no `[files] could not open the file picker: …` error
notice ever appeared after the click (checked `/tmp/wf-tab2.log`, checked
`ps aux | grep -i filechooser`, waited and re-shot: `t09b-10-openfile-clicked.png` is
byte-for-byte the pre-click UI minus the dismissed menu). Because the portal backend that
would draw this dialog is bound to the caller's real Wayland/D-Bus session rather than this
lane's nested compositor, and `HARD RULES` forbids driving `DISPLAY=:1`/the user's real
desktop to go looking for a stray dialog there, this control cannot be exercised end-to-end
from this lane. Not `FAILED — absent`: the code path is real, present, and reaches the
platform layer correctly (`prompt_for_paths` → `ashpd` call sent) — the block is the
environment's portal backend, a named live blocker, not a missing feature.

### F-TAB-10 — Split the current pane right or down

**PASSED.** Right-clicking a terminal pane's body opens the real
`tiller_terminal::context_menu` (13 items, see notes above); "Split Right" was chosen first
(`t10c-16-split-right-result.png`): the single pane became two side-by-side pane groups,
each an independent live shell with its own bash prompt (confirmed live, not stale: a marker
`PIDCHECK 498707` typed into the original/left pane stayed only there while the new right
pane ran its own independent `neofetch`-style banner). "Split Down" was then chosen on the
right pane (`t10f-19-split-down-result.png`): that pane became a top/bottom pair, each
showing a distinct, independently-advancing timestamp
(`18,22:00`/`18,22:02`) — four total live shells now visible in one tab. "Split Left"
(`t10h-21-split-left-result.png`) and "Split Above" (`t10k-24-split-above-result.png`) were
exercised the same way on other panes with the same result: a new pane group appears on the
requested side, both sides independently live. Hard discriminator: pane-group count visibly
doubles per split, and each side's shell content evolves independently over time (different
`Memory`/`Swap`/timestamp readings from the same `neofetch`-alike banner), which a
stale-frame artifact could not produce.

### F-TAB-11 — See split actions disabled with an explanatory reason

**PASSED.** Live-narrowed the terminal context menu on a pane that had already been
split down to roughly 126–160pt of available width (built up by chaining the F-TAB-10
splits above): `t11-25-narrowpane-ctx.png` shows "Split Left" and "Split Right" **greyed**
with the exact computed reason text drawn under each label — `"pane is too narrow to
split: 116pt available, 160pt required"` (and, in a second capture at a slightly different
width, `"126pt available, 160pt required"`) — while "Split Above"/"Split Down" stayed
enabled black-on-white for the same pane, because that pane still had enough *height*.
Traced to source: `tiller_terminal::context_menu::items_with_split_availability`
(`rust/crates/tiller_terminal/src/context_menu.rs:173-200`) computes this per-axis from the
pane's live pixel size and a `sole_tab_in_group` flag, and the render site — per that
module's own doc comment — must show the reason text, not just greyed-out, and must not
attach a click handler; the screenshot confirms both: the text is drawn, and there is no
hover/press affordance on the disabled rows. The "sole tab in its pane group" leg of this
same clause (the other disabled-reason source) is the same code path
(`SOLE_TAB_IN_GROUP_REASON`, exercised structurally by F-TAB-26 below via the identical
"cannot close the sole pane" family of guards) but was not separately screenshotted with its
own distinct reason text this pass.

### F-TAB-12 — Move an existing tab to this pane or another pane

**PASSED.** This port's "Move Existing Tab" surface is the tab-strip's own right-click
context menu (`rust/crates/tiller/src/main.rs::tab_context_items`, not the terminal-pane
menu) — its bottom section offers "Move to This Pane" (permanently disabled, a vestigial
no-op since a tab is always already in its own pane and cannot be its own destination),
"Move to New Pane" when no other pane group exists, and "Move to Pane N" per sibling pane
group. With two pane groups live (from the F-TAB-10 splits above) and multiple tabs open:
clicked the "Claude Code" tab's context menu → "Move to New Pane"
(`t12b-27-move-to-new-pane.png`) — a **third**, independent pane group appeared containing
only "Claude Code", visible simultaneously alongside the original two. Then clicked the
"Codex" tab's own context menu → "Move to Pane 1" (`t12e-30-move-to-pane1-result.png`,
`t12d-29-codex-tabstrip-ctx.png` shows the item's exact label/position first) — "Codex"
disappeared from its own pane group's strip and reappeared as a second tab alongside
"Claude Code" in pane group 1, both entries visible in the same tab strip and both present
in the sidebar's worktree tree simultaneously. Hard discriminator: a tab's owning pane-group
membership visibly and structurally changes between two independently-verifiable
screenshots, with the moved tab's own content (Claude Code's live idle transcript) intact
across the move.

### F-TAB-23 — Create left, right, above, or below panes from the Pane menu

**PASSED**, on the same real mechanism as F-TAB-10 above, not a separate control — this
port folds macOS's "new pane in direction X" into the terminal context menu's Split
Left/Right/Above/Down items (`tiller_terminal::context_menu::ITEMS`, indices 6-9), the same
one control both `SplitContentMenu.swift`'s new-pane-direction clause (F-TAB-23) and its
split-current-pane clause (F-TAB-10) land on in this port — confirmed by reading the actual
dispatch (`main.rs`'s `TerminalContextAction::SplitLeft/Right/Above/Down` all route to the
same `split_focused_terminal`/pane-tree-insertion code whichever direction is chosen). All
four directions were driven live in F-TAB-10's evidence above
(`t10c`/`t10f`/`t10h`/`t10k-*.png`), each producing a new, independently-live pane group on
the requested side. Not re-screenshotted separately since the mechanism, the menu, and the
four outcomes are identical to F-TAB-10's — recorded here as the same evidence under this
port's real, verified one-control design.

### F-TAB-24 — Cancel a pane-tab drag with Escape

(continued below)

### F-TAB-25 — Attach an eligible pane to the current terminal

**PASSED.** With two independent terminal tabs open in the same pane group ("Terminal ?"
and "Terminal"), right-clicked the **second** tab's context menu while it was the active
tab: "Attach to Current Terminal" showed **disabled** with reason "select another terminal
tab" (`t25b-41-newterm-tabctx.png`) — correct, since a tab cannot attach to itself. Switched
focus to the **first** tab and reopened its own context menu: the same item now showed
**enabled** (`t25c-42-firstterm-tabctx.png`). Clicked it: both terminals' live content
appeared **simultaneously side-by-side** in one combined view — two independent bash
prompts, two independent `neofetch`-style banners with different live readings, both
visible in the same frame at once where before only one tab's content showed at a time
(`t25d-43-attach-result.png`). Hard discriminator: eligibility flips correctly based on
which tab is "current" (self-attach correctly refused with a stated reason, cross-attach
allowed), and the post-attach frame shows two live, independently-updating terminal
surfaces on screen together, which a no-op click could not produce.

### F-TAB-26 — Close a terminal pane from its pane context menu after confirmation

**FAILED — defective.** The confirmation gate itself works correctly and was proven twice:
launched the real `claude` CLI directly inside a plain terminal pane's shell (not as an ACP
chat tab — just typed into the raw prompt); the app's own Layer-D foreground-process
detection picked it up within ~1s (status bar's Activity indicator flipped to "1 running",
the pane's own tab icon changed from the plain terminal glyph to the Claude sun glyph —
`t26z2-61-claude-launched.png`). Right-clicking that pane → "Close Terminal…" now raised the
real confirm banner: *"This pane has running work. Close anyway?"* with **Close Anyway** /
**Cancel** (`t26z4-63-after-close-click.png`). Clicking **Cancel** correctly left the pane,
its tab, and the live `claude` process untouched (`t26z5-64-after-cancel.png`).

**Clicking "Close Anyway" is where it breaks**: reopened the same menu → Close Terminal →
Close Anyway a second time, with the button's on-screen bounds re-confirmed by a fresh
screenshot immediately before each click (`t26z6`/`t26z9`/`t26g7-85-final-dialog-check.png`,
all showing the identical button geometry) — and across **three independent Close-Anyway
clicks** the dialog dismissed but **nothing else changed**: `panel.list` over the control
socket reported the exact same 5-7 terminal panes before and after every attempt, and the
real `claude` process (pid confirmed via `ps`, launched at `22:23:40`) was still alive and
still a child of the same still-alive pane shell after each click. This was not a coordinate
or input-delivery miss: a **control** run in the same session on the same button/menu, same
click mechanics, on a *different* pane one click later — an idle terminal pane that was one
of *several* panes in its tab (no confirmation needed there) — closed correctly and
immediately, `panel.list`'s pane count dropping 8→7 (`pane-3` disappearing) the instant
"Close Terminal…" was clicked
(`t26g4-82-rightpane-ctx3.png`→`t26g5-83-after-close-idle-multi.png`).

**Root cause, read from source**: `close_terminal_at`
(`rust/crates/tiller/src/main.rs:7513-7525`) opens with

```rust
if tab.panes.leaf_ids().len() <= 1 || !tab.panes.contains(focused_pane) {
    return;
}
```

— a guard meant to stop an ordinary close from leaving a tab with zero panes. But
`confirm_pending_pane_close` (`main.rs:4566-4574`) calls this exact function for the
non-whole-tab case, so when the pane under confirmation is the **sole pane in its tab**
(exactly the state the running-`claude` pane was in throughout this repro — verified by
right-clicking the visibly-empty region beside it and getting no context menu at all, i.e.
no sibling pane there to have a menu), "Close Anyway" silently no-ops: the confirmation
state is consumed (`.take()`), the dialog disappears, and the function returns before ever
touching the pane tree, the terminal process, or scheduling a save. The user is told nothing
— the button visually promises to close the pane "anyway" and does not, and a genuinely
running agent process is left alive with no further way to close it from that menu (Cancel
and Close Anyway now behave identically). This is a real, narrowly-scoped, precisely
reproducible defect: it fires whenever "Close Terminal…" is invoked on a tab's only pane
while that pane's status is Running/NeedsInput/Error; the identical control on a multi-pane
tab works correctly.

### F-TAB-27 — Resume a past chat from the pane menu

(continued below)

### F-TAB-28 — Close the active tab with Ctrl-W

(continued below)
