# P97 — the Project Settings card that persists nothing

**Owner: the next builder to free up (`codex11` or `codex12`).** Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

Five rows, one seam. **Both ends are already built and the entire middle is missing** — you are
writing the connection, not the feature.

| row | clause | now |
|---|---|---|
| `F-PRJ-12` | change a project's repository type and display name | `FAILED — absent` |
| `F-PRJ-13` | reset a project icon and choose a colour | `FAILED — defective` |
| `F-PRJ-14` | the Avatar tab | `NOT EXERCISED` |
| `F-PRJ-15` | choose a project SF-Symbol icon | `FAILED — defective` |
| `F-PRJ-16` | the Emoji tab | `NOT EXERCISED` |

`F-PRJ` is the weakest cluster in the app — 3 of its 18 rows pass. This piece is the largest
recoverable part of it.

## The severed nerve, and the underscore that marks it

`sidebar.rs`, in the project-settings open path:

```rust
let icon = Rc::new(RefCell::new(ProjectIcon::default()));
let icon_for_picker = icon.clone();
let icon_picker = cx.new(|cx| {
    ProjectIconPicker::with_value(icon.borrow().clone(), cx).on_change(move |value| {
        *icon_for_picker.borrow_mut() = value;
    })
});
self.project_settings = Some(ProjectSettingsCard { /* … */ icon, icon_picker });
```

The picker works. `on_change` is wired. The chosen value lands in an `Rc<RefCell<ProjectIcon>>`
owned by the card — **and stops there.** Further down the same file:

```rust
let _selected_icon = card.icon.borrow().clone();
```

An **underscore-prefixed binding**: the chosen icon is read back out and deliberately discarded, the
underscore silencing the unused-variable warning. Someone built up to the boundary and stopped.
That line is the whole defect, and it is why both critics saw the picker behave correctly *inside
itself* while the project never changed.

This is failure-mechanism **(e)** in `QUEUE.md` — *a control that mutates only local state,
discarded at the panel boundary.* Grep the needles, not the line numbers; this file moves.

## What already exists, so you do not rebuild it

**The schema is done. Do not write a migration.**

- `tiller_persistence/src/migrations.rs` already creates `icon_kind TEXT NOT NULL DEFAULT 'icon'`
  and `icon_value TEXT`, alongside `color_hex` and `display_name`.
- `tiller_persistence/src/model.rs` already carries `icon_kind` / `icon_value`.
- `tiller_persistence/src/db.rs` already `SELECT`s all four columns.
- `tiller_ui/src/project_identity.rs` has `ProjectIconPicker`, `ProjectIconValue`
  (`Symbol`/`Emoji`/`Avatar`), `on_change`, and `commit`.

**What does not exist anywhere:** `icon_kind`, `icon_value` and `color_hex` have **0 references
outside `tiller_persistence`**, and there is **no conversion between `ProjectIconValue` and the
persisted pair**. That conversion is the first thing you write.

## Two traps

1. **`display_name` greps 53 times and every hit is the wrong symbol.** They are all
   `AgentAdapter::display_name` in `tiller_agents`. The project's `display_name` column is a
   different thing that happens to share a name. Do not conclude either way from a bare grep —
   scope it to the crate. This is the same substring trap that has cost this project a false
   verdict before.
2. **Fixing only the write half will look like failure.** Line `:781` initialises the card to
   `ProjectIcon::default()` *every time it opens*, so the card never loads the project's current
   icon. If you persist on change but still initialise from `default()`, reopening Settings shows
   the folder again and it reads exactly like the bug you just fixed. **Both directions, or the row
   does not move.**

## Done means

1. `ProjectIcon` ⟷ (`icon_kind`, `icon_value`) conversion, with tests over all three variants.
2. Choosing an icon or colour writes through to the store; opening Settings reads the stored value
   back; the **sidebar row** reflects it (`row_icon` currently derives the glyph from `RowKind` and
   never consults the project — that is part of your job).
3. `F-PRJ-12`: say explicitly whether you closed repository-type, display-name, **or both**. It is a
   conjunctive clause and a single "done" hides the other half.
4. **The judgeable proof is a restart.** Choose an icon, quit, relaunch, and the icon is still
   there. That single gesture is what separates this from the state it is in now, where everything
   appears to work until the panel closes.
5. Report which rows you believe moved and what you exercised. **You do not set verdicts** — a
   critic does, and it will not be you.

## The rules

- **Inspiration, never code.** waku, Zed, orca and comet are to look at. Transplanted code is a gap.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`.**
- **Do not edit `INVENTORY-LEDGER.md`.**
- Commit path-scoped, never `git add -A`. `sidebar.rs` is shared — follow
  `ENVIRONMENT.md` §"A shared file is not a reason to leave work uncommitted".
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
