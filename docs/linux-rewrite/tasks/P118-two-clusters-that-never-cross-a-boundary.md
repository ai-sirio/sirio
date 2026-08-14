# P118 — two clusters that never cross a boundary

**Owner: `codex12`, as builder.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. Both clusters come from `pireview`'s `P113-triage.md`, which ranked them
high-confidence with the location already named. You built neither, and `pireview` will judge both.

Four `FAILED — defective` rows, two independent causes, one shape: **a control works right up to
the edge of its own component and the result never crosses into the thing that owns it.**

## Cluster A — project-icon selection never reaches the project

**Rows:** `F-PRJ-13`, `F-PRJ-15`.

Selection works inside the six-glyph settings grid. Close it and the sidebar still shows the
original icon. `F-PRJ-13` names an unwired `on_change(ProjectIcon)` seam; `F-PRJ-15` says the grid's
output is discarded.

`P113` points at `rust/crates/tiller_ui/src/sidebar.rs` plus the project-settings persistence seam
named in `SEAMS.md`. Wire the selection through the close/apply path into the project model and the
sidebar row. `F-PRJ-13` also carries a **clipped Colour row** — if that survives the wiring fix,
treat it as a separate defect and say so rather than folding it in.

## Cluster B — `notification.create` records but never delivers

**Rows:** `F-AUTO-06`, `F-CTRL-NOTIFY-03`.

`record_notification` pushes onto an in-memory vector. `F-CTRL-NOTIFY-03` additionally recorded an
**empty `dbus-monitor` capture**, and found the only caller of the poster is activity-transition
code — never `create`. Location: the notification control handler in `rust/crates/tiller/src/main.rs`
and `post_desktop_notification`.

**Decide the contract first, in writing:** is `notification.create` an inbox API or a delivery API?
Then read what the inventory rows actually require and make the code match. One change can satisfy
both rows. If you conclude the rows only require an inbox and the current behaviour is already
correct, **that is a legitimate finding** — say it with the row text quoted, and do not build
delivery nobody asked for.

## Proving it, given you cannot see

This is the part that decides whether the work counts.

- Cluster B has a genuinely text-observable proof and you should use it: **`dbus-monitor`**. An
  empty capture is what put the row in the ledger; a capture containing your notification is what
  takes it out. Record the command and its output verbatim.
- Cluster A is visual. Find a text-observable assertion if one exists — `project.list` or a settings
  read that reports the icon, or the persisted row in the SQLite DB. **Prefer proof that survives a
  restart**, since the row is about the icon actually belonging to the project.
- If no text proof exists for a conjunct, drive it on the Wayland lane, leave the capture in place,
  and **name the file and the exact thing to look for** — the orchestrator reads frames and will
  close it. Do not claim a visual result you could not observe.
- **A green test is not the proof.** Today a transcript that draws nothing passed 134 tests
  (`CRITIC-visual-baseline.md`, 17:40 section). Report tests as tests.

## Stay out of `chat.rs`

`codex11` is editing `rust/crates/tiller_ui/src/chat.rs` for `P117` right now. Neither of your
clusters needs it. You will both touch `rust/crates/tiller/src/main.rs` — commit path-scoped, keep
your hunks in the notification handler, and expect `index.lock` contention with six panes
committing: **retry, never delete the lock.**

## The rules

- **Do not edit `INVENTORY-LEDGER.md`** and do not set verdicts.
- Commit path-scoped, never `git add -A`. `grep '??'` before calling a row done.
- Report to `docs/linux-rewrite/P118-report.md`, per row: what you changed, what you drove, and what
  the output or the capture actually showed.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
