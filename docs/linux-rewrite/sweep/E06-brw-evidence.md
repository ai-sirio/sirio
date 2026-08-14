# E06-brw evidence — F-BRW

Lane: Wayland (`Scripts/wayland-drive.sh`), label `drive-E06-brw`. Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`, HEAD
`4073297`. No source edits made (Wayland-only sweep, binary untouched).

## F-BRW-08 (ledger line 257) — exercised-working

**Route:** `Settings::render_browser_grants` (`rust/crates/tiller_ui/src/settings.rs:3035`)
renders "Browser origin grants" under the Permissions settings section, with a per-origin
Revoke button and a "Revoke all" button, driven by `on_revoke_browser_origin` /
`on_revoke_all_browser_origins` callbacks that call into
`tiller_persistence::Db::revoke_browser_origin` / `revoke_all_browser_origins`.

**Why the previous NOT-EXERCISED evidence could not be closed by socket alone:** there is no
control-socket method to create a browser-origin grant (confirmed by F-BRW-07's finding), and
the app has no scripted way to reach the Allow-origin doorhanger in this lane (webview content
does not render under Wayland — see `WAYLAND-LANE.md`). So the Permissions section can only
ever show the empty state via pure socket/UI driving, which is discriminating-evidence-null
(empty is also the default with no grants, so a screenshot of "No browser origins have been
granted" proves nothing new).

**Drove:** seeded a real row directly into the running instance's own SQLite DB
(`/tmp/drive-E06-brw.sqlite`, `TILLER_DB` for this label) — `INSERT INTO browser_origin_grant
(origin, granted_at) VALUES ('https://e06-brw-proof.example', <now>)` — using Python's stdlib
`sqlite3` module (no compile, no source edit; this only exercises the already-existing
`browser_origin_grant` table created by migration v11). This puts the persistence layer in the
same state a real Allow-origin decision from the doorhanger would leave it in. Then, in a live
Wayland-driven instance:

1. `ctl surface.settings.open` → `ctl surface.settings.select section=permissions`
2. `shot f-brw-08-before-revoke` — screenshot shows the "Browser origin grants" card with
   `https://e06-brw-proof.example` listed and a `Revoke` button next to it.
3. `click 1272 202` — the real left-click gesture on the rendered `Revoke` button for that row
   (not a socket call; this exercises the actual `on_click` handler wired to
   `revoke_browser_origin`).
4. `shot f-brw-08-after-revoke` — screenshot shows the card now reads "No browser origins have
   been granted." — the row is gone from the rendered UI.
5. Re-opened the DB file after the drive completed and re-queried
   `SELECT * FROM browser_origin_grant` — **0 rows**. The click did not just repaint an
   in-memory list; it deleted the row from the persisted store via
   `Db::revoke_browser_origin`.

Both the visible-repaint half and the persistence half are proven by one click, driven through
the real gesture path (mouse click on the rendered button, not a control-socket shortcut for
the revoke action itself — no such shortcut exists).

Captures:
- `reference/linux-progress/drive-E06-brw/02-f-brw-08-before-revoke.png`
- `reference/linux-progress/drive-E06-brw/03-f-brw-08-after-revoke.png`

`claim`: exercised-working. The Permissions settings surface renders granted browser origins
and its Revoke control removes the origin from both the live render and the persisted
`browser_origin_grant` table, driven by a real click on the rendered button.

---
