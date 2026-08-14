# P88 — the settings that erase each other

**Owner: `codex12`** (`main.rs` is yours). Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

**Queued behind P85 (the gate) and P87 (the hook).** Do not start it until those are finished or
reported blocked.

This is a small change — two functions, no schema migration — with an unusually large payoff: it is
the missing "DB half" of five `half-proven` ledger rows at once.

## The finding

`SettingsSnapshot` carries 17 persisted fields. `AppSettings` carries 16 keys. `save_settings`
(`tiller_persistence/src/db.rs:745`) writes **all 16** in one transaction. But the two converters in
`main.rs` map only **five** of them:

```rust
main.rs:7582  fn app_settings_from_snapshot(snapshot: SettingsSnapshot) -> AppSettings {
                  AppSettings {
                      appearance, ui_font_size, terminal_font_size,
                      file_icon_theme, control_socket_enabled,
                      ..AppSettings::default()      // <-- eleven fields defaulted
                  }
              }
```

The eleven that fall through `..Default::default()`: `resume_agent_sessions`, `auto_naming`,
`limit_chat_history`, `chat_retention`, `limit_mounted_worktrees`, `mounted_worktrees`,
`summarizer_agent`, `claude_show_in_bar`, `codex_show_in_bar`, `opencode_show_in_bar`,
`refresh_interval`.

**This is not merely "the value isn't saved". It is destructive.** Because the writer persists every
key unconditionally, changing *any* setting writes defaults over the other eleven. Toggling the
theme silently resets your chat retention, your summarizer agent, your usage-bar visibility, your
worktree mount cap, auto-naming and resume-sessions — all in the same transaction.

The load side (`main.rs:7556`) has the mirror stub, and it is honest about it:

> P58: the remaining snapshot fields … start at their defaults **until the persisted schema carries
> them** — the `tiller_persistence` extension decided in P58 lands as codex11's piece, and this
> mapping then grows to cover it.

That comment was correct when written. **Its premise has since expired**: the schema *does* carry
them now — `AppSettings` has all sixteen fields and `save_settings` writes all sixteen keys
(`db.rs:745-860`, keys `RESUME_AGENT_SESSIONS` … `REFRESH_INTERVAL_MIN`). The mapping never grew to
match. Nobody noticed because the comment reads as a deliberate decision rather than a debt, which
is exactly what makes this class of stub durable.

**So there is no migration to write and no schema work to do.** Both halves already exist; only the
two converters in the middle are stubbed.

## Why the test suite is green over this

`session.rs:1436`'s round-trip test builds its expected value with the *same*
`..AppSettings::default()` idiom and then asserts `load_settings() == settings`. Both sides share the
blind spot, so the assertion holds while eleven fields are never really exercised. A test written in
the shape of the bug cannot see the bug. Whatever you add must not repeat that shape — assert
against **explicitly non-default values for all sixteen fields**, spelled out, no struct-update
syntax on the expected side.

## What to build

1. **Grow both converters to cover all sixteen fields**, `main.rs:7556` and `main.rs:7582`. Keep the
   clamps the existing code already applies and extend them to the new numeric fields, matching the
   ranges documented on `AppSettings` itself (`model.rs:348` retention 5..=500, `:353` mounted
   2..=50). `summarizer_agent` needs a real mapping in both directions — it is `String` in
   `AppSettings` and `SummarizerChoice` in the snapshot; make the unknown-string case explicit rather
   than silently defaulting.
2. **Delete `..AppSettings::default()` and `..Default::default()` from both converters** once every
   field is named. Leaving them means the next field added to either struct reintroduces this bug
   silently. `socket_path` is runtime state, not persisted — it stays `String::new()` on load, with a
   comment saying why it is the one deliberate exception.
3. **Remove the stale P58 comment** at `:7571-7577` and the now-false
   `#[cfg_attr(not(test), allow(dead_code))]` on `session.rs:1031` — `main.rs:7754` is a real
   production caller, so the attribute is a leftover that would hide a genuinely dead writer later.

**Out of scope, and say so in your report:** `agent_colors` (F-SET-22) has no `AppSettings` field at
all, so persisting it *would* need a schema change. Do not start that here. Name it as the follow-up.

## Done means

1. Both converters name all sixteen fields; neither uses struct-update fallthrough.
2. A round-trip test that fails against today's code — sixteen explicitly non-default values written,
   read back, all sixteen asserted individually.
3. **A test for the destructive property specifically**: persist non-default values, then save a
   snapshot that changes only the theme, and assert the other fifteen survived. That is the
   regression that matters, and nothing in the suite covers it today.
4. **Live proof, because this has never once run on this machine.** The `setting` table is empty in
   all seven databases under `~/.local/state/TillerRust/checkouts/` — `save_settings` has never
   executed here, which is why the defect stayed latent. Launch the app, change two settings in
   different sections, quit, relaunch, and confirm both survived. Read the DB back directly:

   ```bash
   python3 -c "
   import sqlite3
   c=sqlite3.connect('<path>/tiller.sqlite')
   for k,v in c.execute('SELECT key,value FROM setting ORDER BY key'): print(k,v)
   "
   ```

   `sqlite3` the CLI is **not installed** on this box; use Python's stdlib as above.
5. `cargo fmt`, suite green on what you touched, `git status --short | grep '??'` before you finish.
6. Report which ledger rows you believe this closes — `F-SET-04`, `-05`, `-06`, `-07`, `-10` are the
   candidates, each currently `half-proven` on exactly this missing DB half. **Do not edit
   `INVENTORY-LEDGER.md`**; only a critic moves a verdict.
