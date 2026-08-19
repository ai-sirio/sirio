# Builder re-drive — F-TAB-26 (context-menu entry point) and F-TERM-05 (Set Title, with screenshots)

Requested by team-lead after merging F-TERM-08 (`c0834159` on `linux/gpui-waku`): the merge left two
rows half-proven for want of evidence, not code. Both are closed here with fresh live drives.
Screenshots at `docs/linux-rewrite/fullapp/builder-verify-tab26-term05-shots/`.

Worktree `/var/tmp/tt-verify2-3486926`, branch `verify/f-term-05-08-tab-26-3486926`, off current
`linux/gpui-waku` HEAD `37f69491` (has F-TERM-08's fix, the chat-surface fix, and the shared
git-status resolver). No code changes — this is verification only. Built standalone
(`CARGO_TARGET_DIR=/var/tmp/cargo-target-verify2-3486926`), driven under `Scripts/wayland-drive.sh`
with a private label (`TILLER_WL_LABEL=v2t26`), never touching `DISPLAY=:1`/`wayland-0`/`wayland-1`.
Fully torn down afterward (matched by `TILLER_SOCKET`/`SWAYSOCK`/`TILLER_WL_LABEL`, confirmed by
`pgrep` finding nothing and no stray `/tmp/v2t26*` files).

## Drive 1 — F-TAB-26: the context-menu entry point, on an idle terminal

Team-lead's objection to the earlier evidence: the close-confirm keyboard fix was driven with
`ctrl-alt-w`, but this row's own reproduction was **right-click → "Close Terminal…"** — same
underlying function (`request_close_terminal_at`, `main.rs:4950`), but "same code path therefore
same result" is exactly the reasoning that let an earlier control pass from a state the bug
couldn't occur in. So this re-drives the actual context-menu path, not the chord.

Read the code first to confirm what should happen and where: `request_close_terminal_at` no longer
takes a `pane_close_needs_confirmation` gate — it is unconditional (`main.rs:4968-4976`).
`request_close_activity` (`main.rs:4987-5007`, the right panel's Activity-row close) **keeps**
its gate — only `Running`/`NeedsInput`/`Error` are held, `Idle`/`Done` close straight through. The
terminal's own right-click menu is `tiller_terminal/src/context_menu.rs`'s 13-item list; its
`"Close Terminal…"` entry routes through `TerminalContextEvent` to
`workspace.request_close_terminal_at` (`main.rs:4198-4200`) — confirmed by reading the dispatch,
then driven live:

1. **`00-worktree-selected.png` → `01-terminal-created.png`**: added a scratch git repo as its own
   project, selected its `master` worktree, opened a new terminal (`ctrl-t`) — a plain idle bash
   shell, nothing running, no agent.
2. **`02-context-menu-open.png`**: a real right-click (the wlr virtual-pointer's `BTN_RIGHT`, not
   a synthesized key) on the terminal content opens the 13-item menu, anchored at the click point.
3. **`03-close-terminal-clicked-idle-terminal.png`** (file: `05-...`): clicked **"Close Terminal…"**
   on this idle terminal. The "Close terminal? This tab's process will be terminated. Close
   anyway?" banner appears — on an idle terminal, through the context-menu path specifically, not
   inferred from the chord test. This is the row's defect made visible and its fix seen, not read.
4. **`06-cancel-idle-terminal-survives.png`**: clicked Cancel — banner gone, terminal alive,
   unchanged.
5. **`07-activity-panel-expanded.png` → `08-activity-close-idle-no-prompt.png`**: expanded the
   right panel's Activity section (one row, this same idle terminal, `verify2-repo/master`),
   clicked its own close (`×`). **No prompt at all** — the tab disappeared immediately, straight to
   "No Terminals" / "No activity". `request_close_activity`'s gate held: idle closes unconfirmed.

Two paths, deliberately different, both seen live on the *same* idle terminal in the *same*
session: the context-menu path always asks, the Activity-row path never does for idle. If they had
both prompted (or both gone straight through), that would be exactly the row's own defect — a
collapse of two paths that are supposed to stay separate. They didn't collapse.

## Drive 2 — F-TERM-05: Set Title, with screenshots that survive

The critic that originally drove this live deleted its own screenshot directory before copying
anything into the repo (`CRITIC-modal.md`'s own account). The behaviour was fine; the evidence
wasn't replayable. Re-driven here, on a **second** idle terminal in the same session (fresh
`ctrl-t`, title defaults to "Terminal"):

1. **`10-context-menu-for-set-title.png`**: right-click → the same 13-item menu.
2. **`11-set-title-modal-prefilled.png`**: clicked **"Set Title"**. Modal titled "Set Title", body
   `Enter the new title for "Terminal:"` (the Swift original's odd colon-inside-the-quote wording,
   `TerminalContextMenuProvider.swift:76`, kept verbatim on purpose), field prefilled with the
   tab's actual current title ("Terminal"), focused (visible orange focus ring), OK and Cancel.
3. **`12-typed-into-field-not-terminal.png`**: typed `" renamed-by-verify2"` — field now reads
   "Terminal renamed-by-verify2"; the terminal's own scrollback underneath is untouched (no leaked
   keystrokes), same guarantee as the close-confirm banner, exercised on the text-field variant.
4. **`13-ok-committed-tab-and-sidebar-updated.png`**: clicked OK. **Three places update at once**:
   the tab strip ("Terminal renamed-by-verify2"), the sidebar row, and the still-expanded Activity
   panel row — all read the same live title, none stale.
5. **`14-reopen-prefills-just-renamed-title.png`** — the check worth being deliberate about, per
   the brief: right-click → Set Title **again**, on the same tab. Body now reads
   `Enter the new title for "Terminal renamed-by-verify2:"`, and the field is prefilled with
   **`Terminal renamed-by-verify2`** — the just-renamed title, not the original "Terminal". This is
   what distinguishes reading live state from echoing a stale value: a naive implementation that
   captured the title once (at the tab's creation, or at first open) would have reprinted
   "Terminal" here instead.
6. **`15-escape-leaves-title-unchanged.png`**: pressed Escape with no further edit — modal closed,
   title still "Terminal renamed-by-verify2" everywhere (tab strip, sidebar, Activity row).
   Unchanged, not reverted-and-reapplied — there was nothing to revert.

Enter's behaviour here is the deliberate opposite of the close-confirm banner: `showSetTitleAlert`
(`TerminalContextMenuProvider.swift:76-90`) adds "OK" before "Cancel", so Enter confirms in the
Swift original, and `ModalButton::new("ok", "OK", ...)` is genuinely the first button in
`render_title_prompt` (`main.rs:8605`) — matching. Not re-driven live again here since it was
already covered by the automated `set_title_prompt_opens_prefilled_and_enter_commits_the_edit`
test and isn't the row's own point of doubt (the missing piece was replayable screenshots of the
behaviour, not the Enter/Escape convention, which F-TERM-08's evidence already established the
reasoning for).

## What matched expectations vs. what didn't

Everything above matched what the code predicted, with one procedural snag worth recording rather
than smoothing over: the very first `rightclick` action, issued as part of a single batched
`wayland-drive.sh` action script (worktree-select → `ctrl-t` → `rightclick` → `shot`), produced no
menu at all in the captured frame — not a defect, a drive-harness ordering issue. Re-issuing the
identical `rightclick` command a moment later, directly against the same live (kept-alive)
instance, opened the menu correctly every time afterward (four more rightclicks in this session,
four menus). Not chasing this further since it never recurred and the row under test has nothing
to do with pointer-event delivery timing; noted so a future pass doesn't mistake a single blank
frame for "right-click doesn't work" the way an over-hasty read of `07-04` almost did in the
F-TERM-08 evidence.

Also unaffected by anything in these two rows: the same `07-04`-style capture race from the
F-TERM-08 write-up shows up again in `15-escape-leaves-title-unchanged.png`'s first capture
attempt (dialog still visible immediately after `key Escape`) — a second capture, taken after a
1-second wait, shows the real settled state (dialog gone, title unchanged). Only the settled
capture is the one committed under that filename; the stale one was not kept, unlike the F-TERM-08
write-up's deliberate choice to keep and annotate its own stale frame — here there was no
ambiguity worth preserving evidence of, since the very next real check confirmed the same thing
twice.

## Environment

`uptime`/`pgrep -c -f 'bin/cargo|debug/tiller'` checked before the build (load ~4.6-9.8, one other
process) and before the drive (quiet). Own worktree, own `CARGO_TARGET_DIR`
(`/var/tmp/cargo-target-verify2-3486926`), own binary snapshot (`/tmp/tiller-verify2-3486926`, since
removed), own scratch git repo, own Wayland label. No shared branch was pulled, pushed, merged, or
reset — this worktree was created fresh from `linux/gpui-waku` and never touched again after the
drive. Not pushed; `docs/linux-rewrite/INVENTORY-LEDGER.md` not touched.
