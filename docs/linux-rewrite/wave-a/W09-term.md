# Wave A slice W09-term — 5 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-TERM-04` — ledger line 322, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Menu proven open with Copy/Paste visible. Owed: select text, rclick, click Copy, rclick again, click Paste, confirm terminal content actually changed (not just that the menu dismissed).
- **Shared cause:** terminal context-menu surface proven, per-item click still owed (see F-TERM-06, F-TERM-UI-01)
- **Evidence on record:** Re-confirmed (no new drive): open_context_menu bound only to MouseButton::Right (lib.rs:1446,1504), no keyboard path; wayland-drive.sh has no right-click primitive; checked full ControlAction enum in main.rs, no clipboard/copy-paste socket bypass exists. Row remains unreachable from this lane; owed half unchanged.

## `F-TERM-06` — ledger line 324, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Copy Pane ID / Copy Terminal ID both visible in the proven menu. Click each, paste into a visible field (terminal or Set Title), confirm a nonempty identifier landed.
- **Shared cause:** terminal context-menu surface proven, per-item click still owed (see F-TERM-04, F-TERM-UI-01)
- **Evidence on record:** Re-confirmed (no new drive): same open_context_menu right-click-only binding as F-TERM-04; no ControlAction variant reaches CopyPaneId/CopyTerminalId. Row remains unreachable from this lane; owed half unchanged.

## `F-TERM-08` — ledger line 326, currently **half-proven**

- **Triage says:** reclassify
- **Approach:** close_terminal_at has NO confirmation dialog anywhere (requires_close_confirmation has 2 refs total, both its own tests) and the single-pane guard (leaf_ids().len() <= 1) is a silent no-op -- the row's core clause is absent, not merely gesture-unproven. Once reclassified: build a confirmation dialog gated on requires_close_confirmation and fix the single-pane guard (likely close the tab, matching CloseTab).
- **Evidence on record:** New: on a multi-pane tab, pane.close (control socket) -- confirmed via source to invoke the identical close_terminal_at as the right-click 'Close Terminal…' item -- genuinely kills the child process (screenshots f-term-08-sleep-running.png / f-term-08-after-close.png show the split collapsing after close). Resolves whether Route B's kill logic works once its len()<=1 guard doesn't block it: it does, via the same SIGT

## `F-TERM-UI-01` — ledger line 535, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Same shared-cause gesture gap as F-TERM-04/06, broadened to all 12 menu items including Close Terminal (inherits F-TERM-08's build gap for that item). Also carries a real, already-recorded z-order defect: the Files panel paints over the open context menu (QUEUE.md), fixable in main.rs's paint order.
- **Shared cause:** terminal context-menu surface proven, per-item click still owed (see F-TERM-04, F-TERM-06)
- **Evidence on record:** Re-confirmed (no new drive): same open_context_menu right-click-only binding; noted pane.close reaches one item's delegation target without the menu itself (see F-TERM-08), which does not satisfy this row's menu-rendering-plus-delegation clause. Row remains unreachable from this lane; owed half unchanged.

## `F-TERM-UI-02` — ledger line 536, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** opens_terminal_link(event.modifiers.platform) + per-pane TerminalLinkEvent routing already matches SEAMS.md's P82 platform-modifier ruling exactly. NOT EXERCISED result is plausibly COSMIC intercepting Super at the compositor before it reaches the app; try both the X11/XWayland lane and the native Wayland lane before concluding total block.
- **Evidence on record:** NOT EXERCISED (unchanged): Super+click on a rendered URL (no hyperlink styling visible) produced no xdg-open twice; COSMIC plausibly intercepts Super before the client — instrument/WM ambiguity, not UNREACHABLE.

