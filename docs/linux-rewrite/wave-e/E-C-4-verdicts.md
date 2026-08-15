# Wave E slice E-C-4 — critic verdicts

## `F-SID-15` — PASSED

Live drive (Wayland lane, `TILLER_WL_LABEL=ecfourcritic1` and a repeat run `ecfourcritic2`):
`project.add` this repo, `rightclick 150 222` on the `linux/gpui-waku` worktree row (settled with
a `shot`), `click 82 340` on "Remove Worktree". Both runs: the real "Remove worktree?" dialog
(Remove Worktree/Cancel) appeared centred over the window and the context menu closed — the
`deferred(...)` fix works and reproduced twice.

New finding, not covered by the row's own acceptance clause: in both runs the same click **also**
opened the "New worktree in tiller" creation form in the sidebar underneath the dialog — a second,
unrelated element apparently receiving the same click event. Confirmed persistent: clicking
Cancel on the confirm dialog (second run) closed the dialog but left the New Worktree form open.
This looks like a real click-fan-out defect (the deferred menu item's handler doesn't stop
propagation to the sidebar row occupying the same pixels) — worth a follow-up row — but it does
not violate this row's own pass condition (dialog appeared centred, menu closed), so this row
itself is PASSED.

## `F-TAB-01` — pending (verifying)

## `F-TAB-11` — FAILED — absent

Source-confirmed at HEAD (matches builder's claim exactly): `TerminalContextItem`
(`tiller_terminal/src/context_menu.rs:30`) has only `label`/`action`/`route`, no `enabled`/
`disabled_reason` field; `ITEMS` (line 36) is a flat compile-time `const [TerminalContextItem; 12]`
that cannot vary per-invocation. `split_disabled_reason` (`tiller/src/panes.rs:218`) has exactly
one production reference at line 618, which is inside the `#[cfg(test)]` module (confirmed by
reading surrounding lines) — zero real callers. The row is genuinely unimplemented, and the
builder's own report correctly declines to file a partial/fake wire-up. Unexercisable in the app
because the behaviour does not exist; verdict is FAILED — absent (not UNREACHABLE), since the code
path is confirmed missing rather than merely hard to drive.

## `F-TERM-08` — pending (verifying)

## `F-TERM-UI-01` — pending (verifying)
