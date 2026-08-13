# P70 — The agent identity that was never retained

**This brief is everything you need; your context was just reset.**

## Why this piece exists

The agent picker was cosmetic: pick Codex, get a tab titled "Codex" talking to Claude. `pi` (P66) and
`codex12` (P67) closed it **on the New Chat path only**. Verified in the tree:

- `main.rs:3911-3922` resolves `adapter.acp_program()` → `acp_agent_command()` →
  `Chat::launch_with_command`, and the mapping is genuinely per-adapter — `claude.rs:126` yields
  `@agentclientprotocol/claude-agent-acp`, `codex.rs:60` yields `@agentclientprotocol/codex-acp`, and
  `opencode`/`pi`/`omp` return an honest `None`.

Three sites still construct the hardcoded Claude default: `main.rs:3965` (`resume_chat`),
`main.rs:6652` and `main.rs:6750` (session restore).

**They are not three repeats of the same one-line fix, and reading them that way is the trap.** The
adapter is not merely unused there — *it was never retained*:

- `RetainedChat` (`main.rs:293`) is `{ id, title, transcript }`.
- `TabRecord` (`tiller_persistence/src/model.rs:105-117`) is
  `{ id, worktree_id, title, kind, order_idx, is_active }`.

Neither carries an agent. So resuming or restoring a Codex chat replays a **Codex transcript into a
Claude connection** — the original defect surviving in the one place where the evidence of the wrong
agent is sitting directly beside it. And because `resume_chat` sets `agent_icon: None, agent_id:
None`, the tab never *claims* to be Codex, so it lies more quietly than the New Chat bug did and will
survive any check that looks for a mismatched label.

## The piece: the persistence half, and only that half

`tiller_persistence/**` is **unowned by every current brief**, which is why this is a clean piece.

Deliver the storage so the identity can survive a restart:

1. **`model.rs`** — add `pub agent_id: Option<String>` to `TabRecord`. `None` means "no agent
   recorded", which is exactly what every existing row means and must keep meaning.
2. **`migrations.rs`** — append `migrate_v10`. The table is **`tab`** (singular — see
   `chat_turn`'s `REFERENCES tab(id)`), and `migrate_v7` is your template for an `ALTER`:
   `ALTER TABLE tab ADD COLUMN agent_id TEXT;`. `CURRENT_SCHEMA_VERSION` derives from
   `MIGRATIONS.len()`, so appending the function is the whole version bump — do not hand-edit a
   constant.
3. **`db.rs`** — carry the column through every site that already carries `kind`: `map_tab` (`:983`),
   `save_tab` (`:339`), `save_tabs` (`:373`), and the `SELECT` column lists behind `tabs` (`:309`)
   and `tabs_of_worktree` (`:320`). **Miss one and the column round-trips as `None` forever** —
   silently, because `None` is a legal value.

**Keep it non-breaking.** `TabRecord::new` (`model.rs:120`) must still compile for its existing
callers, defaulting `agent_id` to `None`; add the identity through a separate builder
(`with_agent_id(...)` or equivalent). This is the same discipline that let P66 and P67 run in
parallel without colliding, and it is the reason `codex12` can land the consumer half later without
waiting on you.

## Say plainly that this lands as a dead model

On landing, **nothing writes a non-`None` `agent_id`** — the producers and consumers are
`main.rs:293`, `:3965`, `:6652`, `:6750`, all `codex12`'s. That is intended, but it means this piece
ships the project's most-produced defect *on purpose*.

So: **name it as a seam for `codex12` in your report, and mark no `F-` row as closed.** A migration
plus a column is not a feature; the feature is a restored Codex chat that reconnects to Codex. If
this gets counted as closing the resume/restore defect, the defect becomes invisible instead of
fixed — which is strictly worse than leaving it open.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`.

This crate is not a UI crate, so drawn tests do not apply — the test that counts here is a **real
save/load round trip against a real SQLite file**:

- Save a `TabRecord` carrying an agent id, reopen the database, read it back, assert the id survived.
  A test that only asserts the struct has a field proves the field, not the storage.
- **Migrate an existing v9 database and assert it opens and its old rows read back with
  `agent_id: None`.** `migrate_up_to` exists for exactly this and its doc comment says so. A forward
  migration that drops or corrupts existing rows is the one failure here that costs a user real data.
- Assert `CURRENT_SCHEMA_VERSION` is 10 and that a v10 database is not treated as newer-than-current.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your
  pane starts in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Yours: `tiller_persistence/**` and nothing else.**
- **Do not edit** `main.rs`, `tab_bar.rs`, `tiller_control/**` (`codex12`); `chat.rs`, `settings.rs`,
  `sidebar.rs`, `status_bar.rs`, `tiller_agents/**` (`pi`); `changes.rs`, `right_panel.rs`,
  `editor.rs`, `file_view.rs`, `tiller_git/**`, `tiller_terminal/**` (`codex11`); `tiller_theme/**`,
  `controls.rs`, `titlebar.rs`, `composer.rs` (`sonnet`).
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms. Widening a *struct* is the same bargain: if `TabRecord` gains a required field you own
  every construction site it breaks — which is the reason the field is optional.
- **Establish the build state with the gate's own commands**, not a paraphrase:
  `grep -n clippy Scripts/ci-linux.sh` and run exactly what it says. The orchestrator once ran a
  weaker clippy without `-D warnings`, called the tree clean while the gate was red, and overruled
  three agents who were right. Do not inherit that mistake. Note the gate excludes `tiller` and
  `tiller_ui` but **not** `tiller_persistence` — your crate must be clean on its first new warning.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the migration number landed and that `CURRENT_SCHEMA_VERSION` derives rather
than being hand-edited, the round-trip test and the v9→v10 upgrade test by name, every `db.rs` site
you carried the column through (all five, or which you skipped and why), confirmation that
`TabRecord::new`'s existing callers still compile untouched, the seam named for `codex12`, the gate
run with its own invocation with not-yours failures named separately, and the honest remainder —
including, explicitly, that no `F-` row is closed by this piece.
