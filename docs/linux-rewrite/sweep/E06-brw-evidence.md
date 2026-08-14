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

## F-BRW-06 (ledger line 255) — could-not-reach

**Route checked:** `Browser::request_permission` (`rust/crates/tiller_ui/src/browser.rs:588`,
wrapper at `:880`) sets `permission_prompt`, which the doorhanger UI at `:1242` renders with
`browser-permission-allow` / `browser-permission-deny` buttons wired to `allow_permission`
(`:597`) / `deny_permission` (`:605`).

**Why not reachable:** `grep -rln request_permission rust/ --include=*.rs` returns only
`browser.rs` itself — the unit test `permission_doorhanger_resolves_and_persists_by_origin`
(`:1664`) is the **only** caller of `request_permission` in the entire tree. There is no
production code path (webview permission-request callback, control-socket method, or any
other event) that ever calls it. This is not a Wayland-lane limitation specifically (the
manifest's earlier note about needing a browser-driving agent is consistent with this): the
doorhanger cannot currently be triggered by any live user action anywhere in this build,
because nothing in the running app invokes the function that would populate
`permission_prompt`. Confirmed by full-repository grep, not by giving up on the drive early.

No socket method exists to set `permission_prompt` directly (checked `system.capabilities`
output already recorded for F-BRW-07 in the manifest — no permission-grant/request action is
listed), and seeding this one via direct SQLite edit (as done for F-BRW-08) is not applicable
here because `permission_prompt` is in-memory `Browser` view state, not a persisted table.

`claim`: could-not-reach — the permission doorhanger has no live trigger anywhere in the
current build (production-code caller count is zero outside the unit test); nothing this lane
(or any lane, on this evidence) could click to raise it.

---

## F-BRW-07 (ledger line 256) — exercised-working

**Route:** `rust/crates/tiller/src/main.rs:8161` — `session_store.load_browser_origin_grants()`
runs at app construction time and is passed to `Settings::with_browser_origins` (`:8234`); the
same session store also seeds `browser_origins` for the live workspace (`main.rs:2547`,
`seed_browser_origins`) so a previously-allowed origin auto-skips the doorhanger on relaunch.

**Why the prior P96 evidence stayed NOT EXERCISED:** the control-socket surface exposes no
permission-grant action (confirmed again here — `system.capabilities` has no such method) and
raw `browser.open`/navigate does not trigger an Allow-origin prompt on this build (see F-BRW-06:
zero production callers of `request_permission` exist anywhere in the tree), so there was no way
to get a real grant into the table to retest persistence against.

**Drove:** confirmed the DB was empty (`browser_origin_grant` → `[]`) after the F-BRW-08 revoke
run, then seeded a **new, distinct** origin (`https://f-brw-07-relaunch-proof.example`) directly
into `/tmp/drive-E06-brw.sqlite` via Python's `sqlite3` module — no source edit, no compile, only
inserting into the already-migrated table. Then tore down and started a **completely fresh**
Wayland instance (label `drive-E06-brw`, new process, new socket) and drove:

1. `ctl surface.settings.open` → `ctl surface.settings.select section=permissions`
2. `shot f-brw-07-relaunch-persisted` — the rendered Permissions section shows
   `https://f-brw-07-relaunch-proof.example` under "Granted browser origins" **on first render
   of a brand-new process**, i.e. the grant survived a full relaunch and was loaded from disk
   into the live view, not carried over in memory from a previous run (previous run's DB state
   was independently confirmed empty first).

This is discriminating: the row's default/empty state and the persisted state are different
values, and the capture shows the persisted (non-default) one, produced only by disk state this
drive planted.

Capture: `reference/linux-progress/drive-E06-brw/02-f-brw-07-relaunch-persisted.png`

`claim`: exercised-working. `browser_origin_grant` rows written to the persistence DB are loaded
and rendered on a fresh app launch, proving the save/load-on-relaunch path independent of the
(currently unreachable, see F-BRW-06) live doorhanger Allow action.

---

## F-BRW-05 (ledger line 254) — partially-exercised

**Clause** (`docs/linux-rewrite/01-inventory-app.md:126`): "Show when an agent is driving the
browser — VERIFY: Start an agent browser action, observe the browser tab during the action, and
confirm the Agent driving indicator appears."

**Route:** `Browser::set_agent_driving`/`agent_driving()` (`rust/crates/tiller_ui/src/browser.rs:
581,454,905`) backs a toolbar pill rendered `.when(self.state.agent_driving(), ...)` at `:1148`.
The only wiring into this flag anywhere in the tree is the control method `browser.act`
(`rust/crates/tiller/src/main.rs:4512`), whose own comment says: `"browser.act is unsupported on
Linux: only the driving flag is implemented"` — i.e. there is currently no real ACP tool call
that drives the browser and flips this flag as a side effect; `browser.act driving=<bool>` is the
only lever that exists.

**Drove:**
1. `ctl browser.open url=https://example.com` → `ctl tab.select index=3` to bring the Browser tab
   forward.
2. `shot f-brw-05-browser-tab-driving-off` — toolbar shows only `Browser` / `Stop`, no pill.
3. `ctl browser.act driving=true` → `shot f-brw-05-agent-driving-on` — an orange **"Agent
   driving"** pill appears next to `Browser`/`Stop` in the toolbar.
4. `ctl browser.act driving=false` → `shot f-brw-05-agent-driving-off` — the pill disappears
   again. Positive+negative control both landed, so the render/toggle genuinely tracks the flag.

**What is proven / what is not:** the indicator's render logic is real and correctly wired to
`agent_driving()` — verified appearing and disappearing on command. What is **not** proven is the
VERIFY clause's actual trigger ("start an agent browser action") — no such live path exists yet
on this build; `browser.act` is a manual test hook, not a byproduct of a real ACP browser tool
call, so this cannot be closed to full `exercised-working` from any lane today, Wayland or X11.

Captures:
- `reference/linux-progress/drive-E06-brw/02-f-brw-05-browser-tab-driving-off.png`
- `reference/linux-progress/drive-E06-brw/03-f-brw-05-agent-driving-on.png`
- `reference/linux-progress/drive-E06-brw/04-f-brw-05-agent-driving-off.png`

`claim`: partially-exercised. The driving-indicator render is proven live (on and off, positive
and negative control); the "real agent browser action" half of the clause has no implemented
trigger anywhere in the codebase to drive.

---
