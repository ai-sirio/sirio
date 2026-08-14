# P94 — twelve rows marked absent that are already built

**Owner: `sonnet`, as critic.** Start when `P91` lands. Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. You wrote none of
`sidebar.rs`, `changes.rs`, `right_panel.rs` or the tab code in `main.rs`, which is why you can
judge them.

## What this is

Twelve rows carry `FAILED — absent`. **All twelve are built.** The `FABLE-08` census cited a needle
for each; I re-ran `Scripts/stale-failed-census.py` against today's tree on 2026-08-14 and every
needle re-verified, then spot-checked four by hand (`F-SID-07`, `F-TAB-14`, `F-CHG-18`, `F-CHG-22`)
— all four in production regions, not behind `#[cfg(test)]`.

**So your job is exercising, not construction.** Every row is already annotated **do not rebuild**
in the ledger. If you find yourself writing a feature, stop — you are duplicating something that
exists, which is the specific harm this census was created to prevent.

| row | where it lives |
|---|---|
| `F-SID-07` | `sidebar.rs:1722` `render_project_settings`, called at `:2387` |
| `F-SID-09` | `sidebar.rs:481` `Show in File Manager` (palette twin says *Reveal*) |
| `F-SID-19` | `main.rs:116` `WindowCommand::NewTerminalTab` on **ctrl-t** — the row says CMD-T |
| `F-TAB-13` | `main.rs:5123` `Move to This Pane` + Other Pane / Pane N, with disabled reasons |
| `F-TAB-14` | `main.rs:5912` `begin_tab_rename`, `:5926` commit, `:5949` enter/return |
| `F-TAB-16` | `main.rs:5015` `Close dirty tab?` prompt + bulk `request_close_ids` |
| `F-TAB-21` | `main.rs:4879` `open_tab_menu` |
| `F-CHG-11` | `changes.rs:930` `section_action_button` — *Unstage all* + per-section |
| `F-CHG-13` | `changes.rs:1069` `Open diff` — **read the warning below before driving this** |
| `F-CHG-16` | `changes.rs:1082` `Resolve in terminal` |
| `F-CHG-18` | `changes.rs:993` production `on_drag` with a `DiffDragPreview` payload |
| `F-CHG-22` | `right_panel.rs:855/1087/1097` — all five activity statuses mapped and rendered |

## `F-CHG-13` — a defect I found while writing this, do not let it pass

`main.rs:4068`:

```rust
ChangesTabActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx),
```

The variant's own doc comment says *"Ask the host to open the path in a **dedicated Diff tab**."*
The handler **discards the path** and opens the generic changes tab instead. Click *Open diff* on
`foo.rs` and a tab appears — just not a diff of `foo.rs`.

This is the trap the whole piece turns on. **Something happens, so the row looks alive.** Judge the
*effect against the clause*, not against "did the UI respond". Compare with `F-CHG-16` immediately
after it: `ResolveInTerminal` at `main.rs:4069` uses its path properly, so the two rows sit one line
apart in the same match and only one of them works. That contrast is your calibration — if your
method scores them the same, the method is wrong.

## What killed features in this project, so you know what to look for

Read `QUEUE.md`'s two entries from today (10:40 and 11:20) before you start. In short:

- **A control that mutates only local state.** Tonight `F-CHAT-14`'s *Follow Edited Files* turned
  out to flip a bool that only its own label reads.
- **An event with no subscriber.** `F-TERM-UI-02` emits `TerminalLinkEvent` and nothing in the
  workspace listens, so Super+click on a URL does nothing. Both were `PASSED`.
- **Conjunctive clauses.** Where a clause says *and* or *then*, exercise and record **each
  conjunct**. A single verdict over a conjunction takes its colour from whichever half you touched
  last. `F-TAB-16` (prompt **and** bulk close) and `F-TAB-13` (move **and** disabled reasons) are
  both conjunctions.
- **But the mirror error is just as bad**: a consumerless function is *not* a defect unless the
  clause requires a consumer. I nearly manufactured three false negatives that way this morning
  before checking what each clause actually asked for.

## The rules

- **Only drive under the lock** — `Scripts/linux-drive.sh` takes it; for a multi-step drive hold it
  yourself (`ENVIRONMENT.md` §"Holding the lock yourself"). `fable` is driving `P92` and `codex11`
  drives for `P93`. `import -window` photographs your own window correctly *even while another
  agent steals focus*, so an unlocked driver's frames look fine while corrupting someone else's.
- **A frame showing no change gets a second capture** before you believe it (paint lag).
- **You may move verdicts** — you are the critic here. Works live → `PASSED`. Dispatches into
  nothing, or acts on the wrong thing like `F-CHG-13` → `FAILED — defective`. Surface exists but the
  host never mounts it → `UNREACHABLE`. The harness genuinely cannot perform it → `NOT EXERCISED`
  **with the instrument reason**, never `FAILED`.
- `F-SID-19` is a **vocabulary** row, not a feature row: the clause says CMD-T, Linux binds ctrl-t.
  Judge the binding that exists on this platform and say so in the evidence.
- **Do not edit `INVENTORY-LEDGER.md` totals by hand** — recount with the one-liner in its Totals
  block.

## Done means

1. Every one of the twelve driven live, or explicitly deferred with the count you reached — a
   partial pass reported honestly beats a full pass claimed.
2. Evidence naming a frame, a file on disk, or an observed effect. Screenshots to
   `reference/linux-progress/`.
3. `F-CHG-13` judged on its effect, with `F-CHG-16` as the paired control.
4. Report the verdict deltas.
