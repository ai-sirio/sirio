# Verdicts — E10-persist+sid+agent+per (F-PERSIST, F-SID, F-AGENT, F-PER)

Adjudicated against `docs/linux-rewrite/sweep/E10-persist+sid+agent+per-evidence.md` and the
captures under `reference/linux-progress/drive-E10-persist+sid+agent+per/`. I neither drove nor
built any part of this slice — no `rust/` edit, no compile. Verification used `Read` on every
cited capture (plus several the driver didn't cite, to reconstruct true chronological order from
`ls --time-style=full-iso`, since filenames alone are not reliable ordering), targeted `grep`/`sed`
reads of `rust/crates/...` source, and no app launches of my own (avoids drive-lock/socket
contention with sibling agents still driving other slices).

One row is promoted, one is upgraded off `NOT EXERCISED`, four keep their existing verdict with
independently reconfirmed (or sharpened) evidence.

---

## F-SID-19 (ledger line 88) — `NOT EXERCISED` → `PASSED`

Reconstructed the true capture order by mtime (filenames like `03-empty-retest.png` are not
chronological across the whole session). The decisive four-frame sequence, all within 2 seconds
of wall-clock time:

- `02-dlg-retest.png` (22:12:53) — "Close dirty tab?" for a Terminal pane opened 22:12:17 (the
  mouse-click positive control).
- `03-empty-retest.png` (22:12:54) — genuine "No Terminals" empty state, icon + heading + "New
  Terminal" button, confirmed on screen.
- `04-ctrlt-with-keyboard-init.png` (22:12:55) — a Terminal tab, live shell prompt, with the
  **pane's own internal clock reading `22:12:45`** — a different pane-creation timestamp than the
  22:12:17 pane from the positive control. That internal clock is the app's own state, not
  something a screenshot script can spoof, and it proves a *new* pane was created by the
  intervening ctrl-t keystroke rather than the empty state simply not having repainted.

This satisfies the row's VERIFY clause (`01-inventory-app.md:36` — "Select a worktree with no
tabs, press ⌘T, and confirm the empty state is replaced by a terminal tab"; Linux vocabulary is
ctrl-t per `P94`) end to end: real empty-state precondition, real chorded keypress, real
replacement, with a non-spoofable discriminator. The evidence log's account of *why* earlier
ctrl-t attempts silently failed (`wayland-drive.sh`'s virtual keyboard only initializes when
`type`/`key` appears as a literal action-script token) is independently plausible and is
consistent with an earlier frame in the same sequence, `04-after-ctrlt-attempt.png` (22:09:55),
which shows a literal **`t`** character typed into a live terminal's prompt alongside a "Close
dirty tab?" dialog — i.e. an earlier bare `wtype` call leaked a keystroke through as plain text
rather than firing the ctrl-modified shortcut, exactly the failure mode described.

Note for the record, not part of this row: the evidence log observes the driver's own
`03-empty-retest.png` visibly contradicts `F-SID-18`'s existing "no empty state" verdict (a
different, unassigned row). Flagging it here since it wasn't re-adjudicated.

`evidenceDiscriminates: true`.

Captures: `03-empty-retest.png`, `04-ctrlt-with-keyboard-init.png`,
`02-after-real-click-newterminal.png` (positive control), cross-checked against
`02-dlg-retest.png`, `04-after-ctrlt-attempt.png`.

---

## F-PERSIST-DB-11 (ledger line 514) — `half-proven` (unchanged, evidence sharpened)

Independently confirmed the schema fact: `rust/crates/tiller_persistence/src/migrations.rs`'s
`MIGRATIONS` array literally lists `migrate_v1` through `migrate_v12` (12 entries), so
`user_version` reaching `12` after one boot from a hand-built v1 file is exactly right — this is
a genuine, live, old-file-to-current-schema migration, stronger than the pre-existing test (which
only opens a fresh `TempDir`). That part of the row's "creates the expected current records" half
is now demonstrated live, not just in a trivial case. Real advance.

The "preserves data" half is not settled by this drive, for a reason the evidence log doesn't
fully draw out. Read `rust/crates/tiller/src/session.rs:839-878` (`restore_catalog`) directly: on
every load, regardless of schema version, a stored `project` row's `root_path` is checked to
still be a directory, then `discover_project(&root)` **recomputes** id/name/branch live from
git — the stored `record.name` and the worktree's stored `branch` are never read back into the
catalog at all. This is by design, not a migration defect: a **separate, tested, green** code
path (`restore_catalog`'s `CatalogProjectSettings`, exercised by
`session.rs`'s own `restore_catalog` test at line ~2019, which plants `display_name`/`color_hex`/
`icon_value` in a `ProjectRecord` and asserts all three survive restore verbatim) shows the DB
*does* faithfully round-trip the fields it is actually meant to persist for a project row —
just not raw `name`/`branch`, which are intentionally always re-derived from live git state.

So the driver's marker (`MIGRATION-MARKER-PROJECT` / `MIGRATION-MARKER-BRANCH` planted into
`project.name` / `worktree.branch`) tests a field pair that is overwritten on **every** boot,
migrated or not — it doesn't discriminate migration-time data preservation one way or the other.
The row's own description names the migrations' actual subject matter — "worktree/project
ordering with rowid backfill, ... chat/session fields, ... workspace tables, ... rename legacy
fields, ... browser content" — none of which is project identity. That harder conjunct (plant
`chat_turn`/`session_ref`/tab-ordering rows at `user_version=1`, migrate, confirm the *values*
survive) is still untested by any drive or test in the suite. `half-proven` stands; the still-owed
half narrows to migration-specific field content, not project/worktree naming.

`evidenceDiscriminates: true` (strengthens the proven half; the marker experiment is informative
about an orthogonal mechanism but doesn't move the owed half).

Captures: `02-migration-marker2.png` (sidebar showing regenerated `e10-fixture`/`master`, not the
planted marker strings — confirms the reconciliation-overwrite finding, not a migration failure).

---

## F-SID-12 (ledger line 81) — `half-proven` (unchanged)

Independently confirmed the instrumentation gap, not taken on trust: `grep -n "0x110\|button" Scripts/wayland-virtual-pointer.c`
shows `zwlr_virtual_pointer_v1_button(pointer, milliseconds(), 0x110, ...)` at both call sites
(lines 132, 137) with no button parameter anywhere in the file — `click` can only ever send
`BTN_LEFT`. `docs/linux-rewrite/WAYLAND-LANE.md:24` independently states "Pointer drags,
right-click, modifiers/chords ... are not yet exercised," and line 275 routes unexercised
gestures to `DISPLAY=:1` + the drive lock — out of scope for this slice by the task's own
instructions. Nothing about this row's reachability changed; the catalog half
(`set_primary_flips_the_application_level_marker`) is unaffected. This is a real, currently
un-closeable gap on this lane, not an excuse.

`evidenceDiscriminates: false` (reconfirms an existing, unchanged fact; no new information).

Captures: none (matches the row's own nature — nothing to photograph when the gesture cannot be
issued).

---

## F-AGENT-SAFE-01 (ledger line 472) — `half-proven` (unchanged)

Independently re-ran the grep rather than trust the driver's negative result:
`grep -rniE "management.marker|managed_by_tiller|overwrite.{0,10}refus|skill.{0,10}provision" rust/crates/tiller_agents/src rust/crates/tiller_project/src`
→ zero hits (exit 1). This matches the existing ledger note's own account exactly (pass 14: "no
management-marker or overwrite-refusal logic anywhere") and every corroborating doc
(`DEAD-MODULES.md`, `STALE-FAILED-CENSUS.md`) that already establishes `skill.rs`'s
`agent_skill_install_command` in `tiller_project` is only an `npx` command builder with zero
production callers — not the described "refuses to overwrite an existing unmanaged skill file"
behavior. The worktree-local half (pass 11, fake-HOME unchanged) is untouched by this drive.
Nothing changed.

`evidenceDiscriminates: false` (reconfirms known absence).

Captures: none (no code path to drive).

---

## F-PER-08 (ledger line 244) — `NOT EXERCISED` → `half-proven`

VERIFY (`01-inventory-app.md:243`): "Change a general setting and grant a browser origin,
quit/relaunch, and confirm both values remain." The prior verdict recorded restart being
exercised but the roundtrip itself never actually driven (isolated DB stayed at 0 rows
throughout). This drive changes that for one half.

**General-setting half — now proven live.** `02-pre-click-1715.png` shows "Auto-rename tabs and
agents" OFF; `03-post-click-1715.png` (2 seconds later, same window/session) shows it ON after
the mouse click. `02-after-relaunch-general.png` shows it still ON after what the evidence log
describes as a full process kill + fresh `wayland-drive.sh` invocation. I independently checked
for corroborating signs of a genuine restart rather than taking "killed and relaunched" on faith:
`01-baseline.png`, timestamped 22:25:01 — 20 seconds after the last Settings screenshot and named
with the convention this driver used only at the *start* of each fresh app instance throughout the
log — shows a freshly-enumerated project list (self-seeded `tiller`/`rust/gpui-rewrite` project,
`linux/gpui-waku` project with newly auto-opened Chat+Terminal tabs), exactly the from-scratch
state a real process start produces, immediately followed 2 seconds later by
`02-after-relaunch-general.png` navigating back into Settings → General with the toggle still on.
That is consistent with genuine kill+relaunch, not a soft in-process reset.

**Browser-origin half — still not reachable on this lane, confirmed independently.**
`docs/linux-rewrite/WAYLAND-LANE.md:25` states the embedded browser's chrome renders on this lane
but page content does not (needs an X11 window handle); no page can run the permission-prompt JS
that would call `save_browser_origin_grant`. `02-after-click-permissions-nav.png` shows the
Permissions panel correctly reading "No browser origins have been granted" — the absence is
consistent with the lane limitation, not a defect. That half belongs on the X11 lane.

`evidenceDiscriminates: true` — pre/post-click and pre/post-relaunch are visually and semantically
distinct in a specific, expected way.

Captures: `03-post-click-1715.png`, `02-after-relaunch-general.png`,
`02-after-click-permissions-nav.png`, cross-checked against `02-pre-click-1715.png` and
`01-baseline.png`.

---

## F-PERSIST-DB-06 (ledger line 509) — `half-proven` (unchanged)

Independently confirmed: `rust/crates/tiller_usage/src/account.rs` exists and is exported
(`tiller_usage/src/lib.rs:43`); its only production caller is
`tiller_ui/src/settings.rs:538-550` (`discover_claude_identity`), which shells out to
`claude auth status` and formats the result for a Settings display line only — read directly, it
never touches a database handle. `grep -rniE "account" rust/crates/tiller_persistence/src` returns
nothing: no account-shaped table, no `INSERT`/`SELECT` anywhere in the persistence crate. The
session-ref half (already green, pass 14) is untouched. Nothing changed; still a missing store,
not a missing gesture.

`evidenceDiscriminates: false` (reconfirms known absence).

Captures: none.

---

## Summary

| row | prior verdict | new verdict | changed? |
|---|---|---|---|
| `F-SID-19` | NOT EXERCISED | **PASSED** | yes |
| `F-PERSIST-DB-11` | half-proven | half-proven | evidence sharpened |
| `F-SID-12` | half-proven | half-proven | no |
| `F-AGENT-SAFE-01` | half-proven | half-proven | no |
| `F-PER-08` | NOT EXERCISED | **half-proven** | yes |
| `F-PERSIST-DB-06` | half-proven | half-proven | no |
