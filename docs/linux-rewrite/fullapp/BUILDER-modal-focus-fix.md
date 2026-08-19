# Builder fix — F-TERM-08 close-confirm dialog never took keyboard focus

Fixes the gap `CRITIC-modal.md`'s "Biggest remaining gap" section documented and P103 used to
refute the wave's PASSED verdict: `render_pane_close_confirm` drew "Close terminal?" as a plain
absolutely-positioned div with no `.track_focus()`/`window.focus()`, so every keystroke — Enter
included — fell straight through to the live PTY underneath whenever the dialog opened while the
terminal held focus (the ordinary case for anyone using the documented `ctrl-alt-w` chord).

Branch: `fix/modal-focus-1981730-waku`, worktree `/var/tmp/tt-modalfocus-1981730-waku`, built off
current `linux/gpui-waku` (not the modal branch — see "Branch reconciliation" below). Commit
`5bf7aadb` `fix(F-TERM-08): give the close-confirm dialog real keyboard focus`.

## The fix

Two layers, matching the codebase's own established belt-and-suspenders idiom rather than relying
on one mechanism:

1. **Real GPUI focus**, following the Set Title prompt's already-correct pattern exactly.
   `PendingPaneClose` gained a `focus: FocusHandle` and `needs_focus: bool`, both set the same way
   `PendingTitlePrompt` already sets them; `render()` claims the focus (`focus.focus(window, cx)`)
   right after `needs_focus` is seen true, in the same spot the title prompt's own claim block
   sits. `render_pane_close_confirm` is rebuilt on `render_modal`/`ModalSpec`, passing
   `focus: Some(ModalFocus::new(pending.focus.clone(), ...))` — a small addition (`ModalFocus`)
   to the shared `modal.rs` primitive itself, so both variants now share the exact same focus
   mechanism instead of Set-Title having one and Close-Confirm having none.
2. **A root-level keystroke swallow**, mirroring the existing `palette_open` special case in
   `handle_root_key_down`: `if self.pending_pane_close.is_some() { self.handle_pane_close_confirm_key(...); cx.stop_propagation(); return; }`.
   This is the layer that actually *guarantees* no leak regardless of focus-claim timing or
   correctness — it fires in the capture phase, before the terminal's own key handler ever sees
   the event, independent of which element GPUI considers focused at that instant.

`handle_pane_close_confirm_key` maps both `escape` and `enter`/`return` to
`cancel_pending_pane_close`. `cancel_pending_pane_close` now takes `window: Option<&mut Window>`
and restores focus to the terminal (`focus_tab_content`) when a window is available, so cancelling
via keyboard leaves the terminal focused and typeable again, not stuck.

## The Enter-key question: Enter cancels, does not confirm

Checked primary source before binding anything, per the task's explicit warning against binding
Enter to the destructive action by reflex:

- `App/TerminalContextMenuProvider.swift:58-69` (`showCloseConfirmAlert`) adds **Cancel before
  Close**. `NSAlert`'s documented convention gives the Return/Enter key-equivalent to the
  *first-added* button, and gives Escape to any button titled "Cancel" regardless of position —
  both land on the same button here, so on macOS **Enter and Escape both cancel**, and Close has
  no keyboard shortcut at all.
- `App/SidebarView.swift:580-608` (`TabRow`'s alert) and `:627-685` (`PaneRow`'s alert) are
  identically ordered (`Button("Cancel", role: .cancel)` before `Button("Close", role:
  .destructive)`), confirming this is the consistent convention across every close-confirmation
  surface in the Swift original, not a one-off.
- Contrast: `TerminalContextMenuProvider.swift:76-90` (`showSetTitleAlert`) adds **OK before
  Cancel**, so Enter confirms there. This is why the Set Title prompt and the close-confirm
  prompt intentionally end up with *opposite* Enter behavior in the port — each follows its own
  alert's own button order, not a single blanket rule.

`handle_pane_close_confirm_key`'s doc comment records this reasoning in the source itself.

## Verification

### Automated

- `cargo test -p tiller_ui --lib modal::` — **3 passed** (`confirm_variant_draws_no_text_field`,
  `set_title_variant_draws_a_text_field`, `clicking_a_button_invokes_only_its_own_callback`), all
  updated for the new `ModalSpec.focus` field, none regressed.
- `cargo test -p tiller --bin tiller -- rename escape_closes ctrl_w close_tab --test-threads=2` —
  **8 passed**, confirming no regression in tab-rename, palette-escape, ctrl-w-close, and the
  pre-existing dirty-tab confirmation from the `apply_manual_tab_title` extraction and the new
  `handle_root_key_down` special case.
- The close-confirm/set-title cluster — **8 passed together**:
  - `terminal_keystrokes_reach_the_pty_when_no_dialog_is_open` — **positive control**. Proves the
    detection mechanism (real PTY + terminal echo + scrollback-snapshot scanning) can actually see
    a keystroke land at all, before trusting any "keystrokes didn't leak" result from the next
    test. Types a canary string with no dialog open and confirms it lands in the shell's scrollback.
  - `pane_close_confirm_banner_blocks_keystrokes_from_reaching_the_focused_terminal` — the main
    regression test. Uses a real PTY-backed terminal (`TerminalView::with_shell`, not a mock),
    types into it first so it genuinely holds GPUI focus, drives the actual `ctrl-alt-w` chord
    (`cx.simulate_keystrokes("ctrl-alt-w")`, not a direct method call) to open the dialog, sends
    keystrokes plus a canary while it's open, then sends Escape, then types a second canary after
    the dialog closes. Asserts: nothing from the "while open" phase reached the PTY, Escape
    cancelled (dialog gone, terminal survives), and the post-close canary *does* land (proving
    focus was restored, not just that the dialog ate the keystrokes and left the terminal
    permanently unfocused).
  - `drawn_pane_close_prompt_is_held_for_every_status_including_idle_and_done`,
    `close_anyway_removes_a_tabs_sole_pane_instead_of_silently_no_opping`,
    `drawn_activity_row_close_holds_a_running_tab_and_lets_an_idle_one_go`,
    `set_title_prompt_opens_prefilled_and_enter_commits_the_edit`,
    `escape_cancels_the_set_title_prompt_without_changing_the_title`,
    `set_title_prompt_treats_a_blank_committed_draft_as_a_no_op` — all still pass, confirming the
    `request_close_terminal_at`/`request_close_activity` gate split and the Set Title prompt logic
    are undisturbed by the shared-primitive changes.

### Live drive

Not yet performed at the time of this write-up — the box had another agent (`tt-chat-971757`)
mid live-drive (3 running `debug/tiller` instances, load average 13-14) when this fix reached the
verification stage, and per the environment rules a quiet box for someone else's decisive
measurement takes priority over a non-essential parallel drive. This section will be updated (or
a follow-up note added) once the live `wayland-drive.sh` keyboard-chord and mouse-path checks are
run. The automated `ctrl-alt-w`-driven regression test above exercises the identical GPUI
event-dispatch path a real chord takes (`cx.simulate_keystrokes`, not a direct method call),
which is the same reason the codebase's other live-vs-test pairs (e.g. `CRITIC-modal.md`'s own
F-TERM-05 section) treat that path as trustworthy evidence, but it is not a substitute for the
task's explicit request to also drive the real chord end-to-end and check the mouse path.

## Branch reconciliation

The modal branch (`worktree-wf_29906a5b-fa0-1`, head `e25c0d8294e0e9433df58f254a559dc1ee56b978`)
could not be reconciled onto `linux/gpui-waku` by the git-merge path P103 assumed. Its `rust/`
tree predates the current one structurally — `main.rs` there declares `mod panes;` /
`mod session;` / `mod tab_machinery;` / `mod tray;` / `mod command_palette;` /
`mod display_backend;` as external files that don't exist anywhere in that branch's git tree, and
`tiller_ui/src/` has only `lib.rs` + `modal.rs` versus `linux/gpui-waku`'s ~18 files.
`git merge-base` between the two branches resolves to a commit with **no `rust/` directory at
all**, and an attempted `git merge linux/gpui-waku` in a worktree rooted at the modal branch
produced "add/add" conflicts (not a 3-way diff) on both `main.rs` and `tiller_ui/src/lib.rs` —
confirming there is no common ancestor content to merge against, a case P103's per-intent
resolution rule doesn't cover because it assumes a mergeable diff exists.

Resolution taken: abandoned literal merge; created a fresh worktree off current
`linux/gpui-waku` (`/var/tmp/tt-modalfocus-1981730-waku`, branch `fix/modal-focus-1981730-waku`)
and manually ported the three modal-branch commits' unique content onto it — the `modal.rs`
primitive, the Set Title prompt (F-TERM-05), and the close-confirm fix (F-TERM-08) built on top
with the focus fix this task asked for. One porting note: F-TAB-26 (referenced in `CRITIC-modal.md`
alongside F-TERM-05/-08) turned out to already be present and fixed independently on
`linux/gpui-waku`, so nothing needed porting for it.

An earlier worktree, `/var/tmp/tt-modalfocus-1981730` (branch `fix/modal-focus-1981730`), was
used only to draft the `ModalFocus` addition to `modal.rs` before this reconciliation problem was
understood; it is now superseded and was left as-is (WIP commit only, not merged into anything,
not further touched).

## Files changed

- `rust/crates/tiller_ui/src/modal.rs` — new. `ModalButton`/`ModalButtonTone`/`ModalTextField`
  (ported unchanged) plus new `ModalFocus` and `ModalSpec.focus: Option<ModalFocus>`.
- `rust/crates/tiller_ui/src/lib.rs` — `pub mod modal;`.
- `rust/crates/tiller/src/main.rs` — `PendingTitlePrompt`/`PendingPaneClose` focus fields,
  `render_pane_close_confirm`/`render_title_prompt` rebuilt on `render_modal`,
  `handle_pane_close_confirm_key`/`handle_title_prompt_key`, `handle_root_key_down`'s new
  `pending_pane_close` special case, `cancel_pending_pane_close`'s focus-restore, the real
  `set_terminal_title` implementation (replacing a placeholder stub), `apply_manual_tab_title`
  extracted from `commit_tab_rename`, and the test additions/rewrites described above.
