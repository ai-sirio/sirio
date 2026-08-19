# Critic pass — F-TERM-05, F-TERM-08, F-TAB-26 (the modal-sheet primitive)

**Verdict: PASSED**, all three rows. One real, precise gap remains in the primitive itself —
see "Biggest remaining gap" at the end.

Judged commits, worktree `/home/enzopalmisano/Scrivania/Progetti/tiller/.claude/worktrees/wf_29906a5b-fa0-1`,
branch `worktree-wf_29906a5b-fa0-1`, HEAD `e25c0d8294e0e9433df58f254a559dc1ee56b978`:

- `272ff995` `feat(tiller_ui): add a generic modal sheet primitive`
- `23312e0c` `fix(F-TERM-08): a terminal close always asks, unconditionally`
- `e25c0d82` `fix(F-TERM-05): open a prefilled Set Title modal from the terminal context menu`

Fresh critic, no relation to the builder. Read `docs/linux-rewrite/tasks/P100-four-defects-the-swift-source-already-answers.md`
first, and cross-checked its claims against `App/TerminalContextMenuProvider.swift` and
`App/SidebarView.swift` directly rather than trusting the brief's summary — one of its claims
turned out to need that check (see "A claim I almost got wrong" below).

## Compile and test

Built fresh from the judged worktree, `CARGO_TARGET_DIR=/var/tmp/tt-critic-modal` (never
`/dev/shm`, never the shared `rust/target`). `cargo build -p tiller`: clean, 8m04s, only two
pre-existing `nightly_coverage` cfg warnings from vendored `gpui_linux`, nothing from this wave.

- `cargo test -p tiller_ui --lib`: **368 passed, 0 failed** (includes `modal::tests::*`, 3 new
  tests for the primitive: both variants draw correctly, and a click routes to only its own
  button's callback).
- `cargo test -p tiller --bin tiller`: **206 passed, 1 failed** on the full run
  (`session::tests::the_debounce_collapses_a_burst_into_one_write`), but that test is unrelated
  to this wave (session-write debouncing, nowhere near `main.rs`'s close/title-prompt code) and
  passed cleanly alone (`cargo test ... --exact`, 0.54s) — a timing-sensitive test tripped by
  this box running many other agents' builds concurrently, not a regression from this diff. The
  8 tests this wave specifically added/touched all pass: `set_title_prompt_opens_prefilled_and_enter_commits_the_edit`,
  `clicking_ok_commits_the_set_title_prompts_draft`, `set_title_prompt_treats_a_blank_committed_draft_as_a_no_op`,
  `escape_cancels_the_set_title_prompt_without_changing_the_title`, plus the close-confirmation
  status-loop test and the new unrecognized-shell-terminal test the commit message describes.
- `cargo fmt -p tiller -p tiller_ui -- --check`: flags 27 pre-existing hunks elsewhere in
  `main.rs`/`chat.rs`/`sidebar.rs`/`tab_bar.rs`/`session.rs` — **none in `modal.rs`, none in any
  line range this wave touched** (4241–4310, 4859–4952, 8360–8460, 9330–9400, 10920–10935).
  Pre-existing debt, not this builder's.
- `cargo clippy -p tiller_ui --lib` and `-p tiller --bin tiller`: 27 + 6 warnings respectively,
  all at unrelated locations (`titlebar.rs`, `command_palette.rs:501`, `main.rs:4505/6283/8596/10233/11522`).
  **Zero warnings in `modal.rs` or in any touched range.**
- `git status --porcelain` in the judged worktree: clean against HEAD for the three tracked
  files; every other `??` line is the pre-existing untracked bulk copy of the rest of `rust/`
  (this worktree's `rust/` was never fully git-tracked to begin with — 0 files at the merge-base
  `7c81f75c`), not something this wave should have committed. No stray uncommitted change.

## Environment note: X11 is a dead end here, so I used the project's own headless-Wayland lane

Booted my own Xvfb with `-displayfd` first, per instructions. It came up (`:5`), the binary
launched and connected, but every capture was solid black
(`docs/linux-rewrite/fullapp/critic-modal-shots/00-x11-dead-end-black.png`) — matching
`ENVIRONMENT.md`'s already-documented, already-verified finding: GPUI's blade renderer cannot
present into an X drawable on this box (`vulkan: No DRI3 support detected`), and Xvfb/Xephyr are
a closed avenue here, not something to re-litigate. I killed that Xvfb and switched to
`Scripts/wayland-drive.sh`, which is **not** the user's desktop: it explicitly runs
`env -u WAYLAND_DISPLAY -u DISPLAY ... WLR_BACKENDS=headless ... sway` (verified by reading the
script before using it) — a private, software-rendered (pixman) compositor with no parent
Wayland connection, its own D-Bus, its own virtual pointer/keyboard. `DISPLAY=:1` and
`WAYLAND_DISPLAY=wayland-0/1` (the user's live COSMIC session) were never touched, never driven,
nothing on them was killed. Labels used: `critver2`, `critver3`, both fully torn down
(`kill_ours`-based cleanup) with no leaked processes at the end of the pass, verified by `ps aux`.

One environment trap cost time and is worth recording: a fresh instance's sidebar
**auto-registers every git worktree this box already knows about** (this is the same trap
`CRITIC-brw-close.md` already flagged) — a blind coordinate click on "the first worktree row"
landed on a real worktree (`tiller`'s `rust/gpui-rewrite`), not my scratch repo. No terminal was
actually created there (the click that would have opened one never registered — confirmed by
screenshot before touching anything further), and I killed that instance immediately rather than
risk it. Every other drive in this pass used an explicit scratch git repo
(`/…/scratchpad/critic-modal-verify2-repo`, added and selected by its own project-add response,
not a guessed row position) via `ctl project.add`.

## F-TERM-08 + F-TAB-26 — closing a terminal always asks

Live-driven twice independently (my first pass, label `critic-modal`, screenshots viewed but not
preserved — see note below; then a from-scratch repeat, label `critver2`, screenshots committed
in `critic-modal-shots/`). Both agree:

- **Right-click → "Close Terminal…" on a plain idle bash terminal → unconditional confirm.** No
  agent, no activity signal, nothing running but an idle shell — and the modal appears every
  time (`01-context-menu.png`, `02-close-confirm-idle-terminal.png`: "Close terminal? This tab's
  process will be terminated. Close anyway?"). This is the exact defect the row describes,
  proven fixed.
- **Cancel leaves the terminal alive.** `04-cancel-terminal-survives.png`: after clicking
  Cancel, the same terminal, same content, same tab — nothing closed.
- **Close Anyway actually closes it**, and correctly per `whole_tab`: a single-pane tab's "Close
  Anyway" removed the whole tab (empty "No Terminals" state); a multi-pane tab's "Close Anyway"
  removed only that one pane and the sibling pane survived, with the banner correctly wording
  itself "This **pane's** process will be terminated" (not "tab's") for that case — read directly
  off `render_pane_close_confirm`'s `subject` logic and confirmed live in the first pass.
- **The `ctrl-alt-w` keybinding was pressed, not read off the binding table**, and produced the
  identical confirm dialog (first pass, `panes.rs:121`'s `KeyBinding::new("ctrl-alt-w",
  ClosePane, None)` is the live binding — verified by reading the code, then by chording it).
- **A keypress does not leak to the terminal while the confirm banner is up.** Sent a bare `q`
  keystroke through the same virtual-keyboard mechanism that successfully typed into the
  Set-Title field elsewhere in this same pass (a working positive control, so this is a
  meaningful negative, not a dead input path) — nothing appeared at the shell prompt, and
  nothing in the dialog reacted (`03-confirm-ignores-keypress-q.png` is pixel-identical to the
  frame before the keypress). Reading the code explains why: neither `cancel_pending_pane_close`
  nor `confirm_pending_pane_close` has any keyboard entry point (only the two button `on_click`
  closures call them) — see "Biggest remaining gap" below.
- **The control socket's `ClosePane` still bypasses confirmation**, unchanged — read at
  `main.rs:3552`/`:3190`, not re-derived, since the row's own brief already checked this and
  there is no interactive gesture to re-drive here; a socket client has no one to answer a
  prompt.

**A claim I almost got wrong, caught by checking the reference instead of the brief's summary of
it.** Live-driving turned up a *third* way to close a terminal-holding tab that the P100 brief's
"exactly two live callers" scoping doesn't mention at all: the sidebar's own tab row has a
hover-revealed `✕` (`tiller_ui/src/sidebar.rs`'s `SidebarEvent::CloseTab`, wired at
`main.rs:3713` straight to `close_tab_by_id` — no confirmation, no dirty check, nothing). Driven
live: hover the sidebar's "Terminal" row, click the revealed `×`, and a running terminal closes
instantly with zero prompt (`05-sidebar-hover-x-revealed.png` →
`06-sidebar-hover-x-closes-unconfirmed-matches-swift.png`). My first reaction was that this is
the row's real remaining defect. **It is not** — `App/SidebarView.swift`'s own `TabRow` (the
Swift original, read directly, lines ~552–561) wires its identical hover-`×` straight to
`model.closeTab(tab.id, in: worktree)` with **no alert at all**, tooltipped "Close tab (⌘W)".
The unconditional alert this row is about belongs only to the **context-menu** "Close Terminal…"/
"Close Tab…" items (`TerminalPaneMenu`, gated behind `confirmingClose`), which is exactly what
`request_close_terminal_at` now matches. The port's quick-`×` correctly staying unconfirmed is
faithful parity with the original's own two-speed design (a low-friction quick-close next to a
deliberately-slower confirmed one), not a gap. Recorded here because it is exactly the kind of
finding that is easy to over-report without opening the reference file, and because a future
pass re-discovering the same `×` shouldn't have to re-derive this.

## F-TERM-05 — "Set Title…" opens a prefilled text-input modal

Driven live in the first pass (label `critic-modal`) end to end. **Caveat up front: I deleted
that session's screenshot directory before copying anything into this repo — a mistake, caught
against my own standing note that unpreserved evidence is unreplayable.** The `critver2`/`critver3`
repeat did not get back to this row before I stopped rather than risk touching another agent's
real worktree (see the auto-registration trap above), so what follows is a first-hand, directly
observed account, backed by the still-in-repo passing tests, not a re-preserved screenshot:

- Right-click → "Set Title…" opened a modal titled "Set Title", body `Enter the new title for
  "Terminal:"` (the Swift original's own odd colon-inside-the-quote wording, kept verbatim on
  purpose per the commit message — checked against `TerminalContextMenuProvider.swift:76` and
  it matches), with a text field **prefilled with the tab's actual current title** ("Terminal"),
  focused (visible focus ring), OK and Cancel.
- Typing appended to the prefilled text (field showed "Terminal renamed-by-critic") — landed in
  the field, not the terminal underneath.
- OK committed it: both the tab strip and the sidebar row updated to "Terminal renamed-by-critic"
  immediately.
- Reopening "Set Title…" on the same tab showed the field **prefilled with the just-renamed
  title**, not the original — confirming the prefill reads current state, not a stale value.
- Escape with no further edit closed the modal with the title unchanged.

This matches the four automated tests that exercise the same logic through the same
`simulate_keystrokes`/focus-handle path a real key event takes:
`set_title_prompt_opens_prefilled_and_enter_commits_the_edit`,
`clicking_ok_commits_the_set_title_prompts_draft`,
`set_title_prompt_treats_a_blank_committed_draft_as_a_no_op` (trim-then-empty is a no-op, not a
blank title — matches `apply_manual_tab_title`), and
`escape_cancels_the_set_title_prompt_without_changing_the_title`. All four pass. Combined with
the primitive's own `modal::tests::set_title_variant_draws_a_text_field`, I'm confident in this
row despite the lost screenshots for the live half; the biggest honest caveat is that I cannot
hand a reader a picture of it from this pass, only of the same modal in its no-field variant
(F-TERM-08's screenshots above, same code path, same `render_modal` primitive) and the passing
tests.

## Regressions

No regressions found. `tiller_ui`'s full 368-test suite and the 206 non-flaky `tiller` tests all
pass; clippy and fmt are clean on every line this wave touched; the pre-existing "dirty tab"
confirmation (`request_close_tab_by_id`, a different, older mechanism gated on
`tab_is_dirty` — live terminal / streaming chat / unsaved file — used by the tab-strip's own
`Ctrl-W`/`Cmd-W`) is untouched and still intact, confirmed by reading it, not assumed.

## Biggest remaining gap

**The close-confirmation variant of the new modal primitive has no keyboard path at all** —
Escape does not cancel it, Enter does not activate its default button, nothing does. Verified
live (a keypress while the dialog is open does nothing, screenshot pixel-identical before and
after) and confirmed in code: `cancel_pending_pane_close`/`confirm_pending_pane_close` are each
called from exactly one place, a button's `on_click`, in `render_pane_close_confirm`
(`main.rs:8381–8412`). This is an inconsistency inside the very primitive this task asked to be
"built once, used twice": the **Set-Title** variant of the same primitive *does* have a full key
handler (`handle_title_prompt_key`, Enter/Escape/Backspace/typed-character), because F-TERM-05's
own commit needed one for its text field — but F-TERM-08's confirm variant never got the
equivalent treatment, because it never needed a field to type into. On macOS this comes for free
from `NSAlert` (Escape and Return work on any native alert without a line of app code); rebuilding
it in GPUI means it has to be added by hand, and it wasn't for this variant. A keyboard-only user
— or a sighted user who reflexively hits Escape, which is the single most common way anyone
dismisses any dialog on any platform — cannot dismiss "Close terminal?" without reaching for the
mouse. The fix is small and self-contained: give `render_pane_close_confirm`'s backdrop the same
kind of `on_key_down` capture the Set-Title field already has, mapping `escape` to
`cancel_pending_pane_close` (and, if a default-button convention is wanted, `enter`/`return` to
whichever action the original's `NSAlert` treats as default — worth re-checking against
`showCloseConfirmAlert`'s button order before picking one, since binding Enter to a destructive
"Close Anyway" is the one wrong way to close this gap). A `modal.rs`-level test parallel to the
existing `clicking_a_button_invokes_only_its_own_callback` — an Escape-key-invokes-cancel test —
would catch a regression here the same way that one catches a click-routing regression.

## Evidence

`docs/linux-rewrite/fullapp/critic-modal-shots/`:
`00-x11-dead-end-black.png` (the documented X11 dead end, for anyone re-checking my method),
`01-context-menu.png`, `02-close-confirm-idle-terminal.png`, `03-confirm-ignores-keypress-q.png`,
`04-cancel-terminal-survives.png`, `05-sidebar-hover-x-revealed.png`,
`06-sidebar-hover-x-closes-unconfirmed-matches-swift.png`.
