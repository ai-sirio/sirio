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

Performed once the box quieted down (load dropped from ~14-24 to ~5-8 after `tt-chat-971757`'s
own drive finished). Built the fix's binary standalone (`CARGO_TARGET_DIR=/var/tmp/cargo-target-modalfocus-1981730
cargo build -p tiller --bin tiller`, pinned to `/tmp/tiller-mf-1981730` per `wayland-drive.sh`'s
own snapshot advice) and drove it under `Scripts/wayland-drive.sh` with a private label
(`TILLER_WL_LABEL=mf1981730`), never touching `DISPLAY=:1`/`wayland-0`/`wayland-1`. Screenshots are
committed at `docs/linux-rewrite/fullapp/builder-modal-focus-shots/`.

The command (worktree/project selection landed, by chance, on this fix's own `linux-rewrite`
worktree row in the auto-registered sidebar — harmless, since nothing beyond typed-but-unsent text
touched it):

```
TILLER_WL_LABEL=mf1981730 TILLER_WL_BIN=/tmp/tiller-mf-1981730 TILLER_WL_KEEP=1 \
  Scripts/wayland-drive.sh docs/linux-rewrite/fullapp/builder-modal-focus-shots '
ctl project.add path=<scratch repo>
click 168 183
shot 00a-worktree-selected
chord ctrl t
shot 00-terminal-tab-created
click 1200 500
type PREAMBLE_1981730
shot 01-preamble-typed-no-dialog
chord ctrl+alt w
shot 02-dialog-open-via-real-chord
type LEAK_1981730
shot 03-leak-attempt-while-dialog-open
key Escape
shot 04-after-escape-cancels
type CANARY_1981730
shot 05-canary-after-cancel-focus-restored
chord ctrl+alt w
shot 06-dialog-reopened-for-enter-test
key Return
shot 07-after-enter-cancels-not-closes
chord ctrl+alt w
shot 08-dialog-open-for-mouse-click-test
' 8
```

Step by step, against `01-baseline.png` through `11-08-dialog-open-for-mouse-click-test.png`:

1. **Typed into the terminal first, proving real focus** (`03-00-terminal-tab-created.png` →
   `04-01-preamble-typed-no-dialog.png`): `PREAMBLE_1981730` lands at the bash prompt, unsent
   (no Enter). This doubles as the **positive control** the task asked for — it proves this
   detection method (typed text landing at a real, live PTY's prompt, read straight off the
   screenshot) can see a keystroke arrive at all, before trusting any "didn't arrive" result below.
2. **The real `ctrl-alt-w` chord** (`chord ctrl+alt w`, not a mouse click) opens the banner —
   `05-02-dialog-open-via-real-chord.png`: "Close terminal? This tab's process will be terminated.
   Close anyway?", Cancel / Close Anyway.
3. **`LEAK_1981730` typed while the banner is open never reaches the PTY.** The comparison that
   matters is the *final* state, not the frame immediately after Escape (see the timing note
   below): `08-05-canary-after-cancel-focus-restored.png` and every later screenshot show the
   prompt line as exactly `PREAMBLE_1981730CANARY_1981730` — `LEAK_1981730` is nowhere in it.
4. **Escape cancels.** `07-04-after-escape-cancels.png` — captured too early to show the closed
   banner (see below), but `08-05-...png`, captured after the canary's own `wtype` process-spawn
   delay, unambiguously shows the banner gone and the tab alive.
5. **Cancelling restores keyboard focus to the terminal.** `CANARY_1981730` (typed *after* Escape)
   lands right after `PREAMBLE_1981730` on the same prompt line — the terminal is typeable again,
   not stuck.
6. **Enter cancels, not closes** (`09-06-dialog-reopened-for-enter-test.png` → reopened via
   `ctrl-alt-w` again, then `key Return`): `10-07-after-enter-cancels-not-closes.png` shows the
   banner gone and the same tab still open with the same prompt content — Enter did not trigger
   "Close Anyway".
7. **Mouse path still works.** Reopened the banner once more (`11-08-dialog-open-for-mouse-click-test.png`),
   read the Cancel button's pixel position off that screenshot (~857,518), and sent one real click
   through the same persistent virtual-pointer device `wayland-drive.sh`'s own `click` action uses
   (written directly to its FIFO after the script's `TILLER_WL_KEEP=1` exit, since a second
   `wayland-drive.sh` invocation would have killed and restarted the instance) —
   `12-mouse-click-cancel-real-virtual-pointer.png` shows the banner closed by that click, cursor
   sitting on the now-empty spot where Cancel was, tab still alive.

**One timing artefact, called out rather than smoothed over:** `07-04-after-escape-cancels.png`
still shows the banner open, immediately after `key Escape`. This is a capture race in the drive
harness, not the app: `shot()` forces its repaint by resizing the window and grabs the frame as
soon as sway reports the new size laid out, with no guarantee the Escape key event (delivered
asynchronously over the Wayland wire) has been processed by the app yet by that instant. The next
real screenshot in the same run (`08-05-...png`, taken after `type CANARY_1981730`'s own
`wtype` process spawn — tens of milliseconds of real elapsed time), and the same live instance
queried directly with `grim` afterward, both show the banner gone and the terminal content already
advanced past that point. Recorded here rather than dropped, since a critic re-reading these
screenshots without this note could misread `07` as evidence Escape failed.

Every instance was torn down by matching its own `TILLER_SOCKET`/`SWAYSOCK`/`TILLER_WL_LABEL`
environment values (the same scoped `kill_ours` the script itself uses), confirmed by `pgrep`
finding nothing left and no stray `/tmp/mf1981730*` files afterward. `DISPLAY`/`wayland-0`/`wayland-1`
were never referenced.

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
