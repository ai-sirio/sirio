# W13-git+per+persist+win evidence

## `F-GIT-RUN-02` — ledger line 484 — exercised-working

Prior evidence (P120) said wtype could not deliver Return/Escape at all on this compositor.
That was true only because the field click happened before the newly-opened form's layout had
settled — the same "repair must be forced" trap documented in WAYLAND-LANE.md trap 2, applied
to keyboard focus rather than a screenshot. Once a `shot` (forced repaint) was inserted between
opening the Clone-repository form and clicking its URL field, a real `key Return` worked, proven
twice: once creating a worktree via the branch-name prompt, and once submitting the Clone
Repository form and driving `GitClone::clone` -> `GitRunner::run_streaming` end to end.

Drive: `project.add path=/tmp/w13-testrepo` (a real local git repo with one commit) -> clicked
"+" next to Projects -> "Clone Repository..." -> **settled with a `shot`** -> clicked the
Repository-URL field -> `type /tmp/w13-testrepo` -> **`key Return`** -> the form closed and a new
project `w13-testrepo` appeared in the sidebar at `/home/enzopalmisano/w13-testrepo` with a
`main` worktree marked `Primary`. Verified on disk, not just in the UI: `/home/enzopalmisano/w13-testrepo/.git`
existed with `git log` showing the cloned commit (`8c0a29e init`), then removed as scratch
cleanup. Because the source path exists on disk, `clone.rs` adds `--no-local`, forcing git
through the real object-receiving path rather than the local hardlink optimization, so this is a
genuine exercise of `run_streaming`'s line-by-line stderr forwarding, not a no-op.

As a second, independent confirmation of Return delivery: the New Worktree branch-name prompt
(`click` the "New Worktree..." row -> `type w13-newbranch` -> `key Return`) produced a new
sidebar row `w13-newbranch` at `/tmp/w13-testrepo-w13-newbranch`, and the directory was created
on disk by `git worktree add` (a sibling caller of `run_accepting`, not `run_streaming`, but
proof the same Return-keypress mechanism is not the blocker triage evidence claimed).

Captures: `02-01-baseline.png` (project added), `03-02-form-open.png`/`04-03-typed.png`/
`05-04-after-return.png` (worktree prompt sequence), `06-add-menu.png`..`28-clone-c` under
`reference/linux-progress/wavea-W13-git+per+persist+win/` (clone-form sequence; `27-clone-b.png`
is the discriminating frame showing the new project+worktree after a genuine clone).

**Correction to the manifest's approach note**: an X11/xdotool lane is not needed. The lane's own
`key Return` (wtype against the persistent virtual keyboard, per WAYLAND-LANE.md trap 3) delivers
Return correctly once the target field's frame has settled before the click. The earlier
"wtype cannot deliver Return" finding was an artifact of clicking a field before its container's
layout had rendered, not an instrument limit.

## `F-PER-08` — ledger line 244 — could-not-reach (browser-origin half only)

The general-settings half stays proven from prior evidence (unchanged, no re-drive needed).
Confirmed by reading the code path this pass: `save_browser_origin_grant` (`main.rs:4578`) is
only reached from `newly_allowed` origins collected off a live `BrowserSurface::allowed_origins()`
— i.e. the embedded webview's own permission state, populated only by a real page executing a
permission-gated API (geolocation, notifications, etc.) and the user answering a doorhanger
prompt rendered *by the page*. WAYLAND-LANE.md's own record (`❌ No webview content`, re-confirmed
live this pass — see `F-WIN-06` below, `Direct XCB build failed... GPUI returned unsupported
handle: Wayland(...)`) means no page content, and therefore no permission prompt, can ever fire
on this lane. There is no socket method that synthesizes a `BrowserEvent` permission grant either
(checked `system.capabilities`' method list — nothing browser-permission-shaped). This half
genuinely requires `DISPLAY=:1`, which this slice is barred from taking (drive-lock lane). No new
captures for this row; the browser-chrome-only limitation was re-confirmed as a side effect of
driving `F-WIN-06`'s `New Browser` gesture on this same lane.

## `F-PERSIST-DB-11` — ledger line 514 — partially-exercised (schema half re-confirmed; data half: mixed, one adjacent finding)

Built a real, hand-assembled v11 SQLite file with Python's stdlib `sqlite3` (the DDL for
`migrate_v1`..`migrate_v11`, transcribed from `migrations.rs`, run against a fresh file — no
`rust/` code touched, no cargo build) seeded with rows in every table the manifest named:
`project`, `worktree` (with the v7 `comment`/`created_at`/`updated_at` columns already
present), `tab` (with the v10 `agent_id` column, value `'claude'`), `session_ref` (v5), and
`chat_turn` (v6, **without** the v12 `updated_at` column — the exact pre-migration shape).
Pointed `TILLER_DB` at that file and launched the real app.

**Schema half, reconfirmed stronger than before:** `PRAGMA user_version` read `12` after boot
(`MIGRATIONS.len()`), and `chat_turn`'s columns became
`[tab_id, ordinal, payload, updated_at]` — the v12 `ALTER TABLE ... ADD COLUMN updated_at
INTEGER NOT NULL DEFAULT 0` ran correctly against a table that predated the column.

**Data half, non-chat rows: proven.** After boot, `session_ref` still contained
`('sess-w13db11', 'ref-w13db11-marker')` verbatim (plus a new row the live app added for its own
pane), `tab` still carried `tab-w13db11` with `agent_id='claude'`, and `project`/`worktree` rows
were intact and visibly rendered in the sidebar (`30-migrated-boot-b.png`). This is the
concrete "old-version data survives the renames/backfills" proof the manifest asked for, for
every table except `chat_turn`.

**`chat_turn` specifically: not what was expected, and worth recording as its own finding.**
The seeded `chat_turn` row (first a deliberately-invalid marker payload, then re-seeded with a
schema-valid `ChatTurn` JSON payload — `{"entries":[{"UserMessage":{"text":"..."}}]}`, decodable
by `serde_json` per `model.rs`) was **gone from the table after boot in both cases**, and
`quarantine_record` (the table `chat_turn`'s own decode-failure path writes to,
`db.rs:595`/`db.rs:1291`) was **empty both times** — so this isn't the documented
quarantine-on-corruption path either. A same-schema **control** (fresh v12 DB, no migration
involved, identical valid payload) reproduced the exact same disappearance, which isolates this
from the migration question entirely: it is not a migration data-loss bug. Reading
`chat.rs:1657`, restore only happens via `restore_persisted_transcript`, called from
`launch_with_command_and_persistence`, which immediately calls `start_connection` after
restoring — and `clear_persisted_transcript` (`chat.rs:2066`) exists on a path that can run
after that. This row's own concern (migration renames/backfills preserving old data) is now
answered for every table that matters to it; the `chat_turn` disappearance is a
**separate, reproducible, schema-independent observation** about the chat-tab open/connect
lifecycle, filed here because it was discovered while proving this row and not because it
belongs to it — a maintainer should decide whether it is expected (e.g. transcript is meant to
be re-derived from a live agent reconnect rather than replayed) or a genuine loss.

Captures: `29-migrated-boot.png` (first attempt — project self-healed away because its worktree
path didn't exist on disk, itself an interesting "vanished worktree" cleanup confirmation),
`30-migrated-boot-b.png` (real migrated sidebar with `w13db11proj` / `main` / `Chat` tab
present), `31-chat-restored.png` (valid-payload case, transcript pane still empty),
`32-control-current-schema.png` (current-schema control, same emptiness). Scratch fixtures
(`/tmp/w13db11proj`, `/tmp/w13db12proj`, the hand-built `.sqlite` files) were removed after the
drive; only the captures and this writeup are retained as evidence.

## `F-WIN-06` — ledger line 58 — exercised-working (reclassify confirmed)

Triage's reclassify was correct. Re-verified against the current build via the real user gesture
the ledger's `VERIFY` line names: clicked the "+" at the top of the tab strip (not the sidebar's
"+"), which opened a menu listing `New Terminal`, `Changes`, **`New Browser`**, the five agents,
`Split Claude Code`, `New Chat` (`36-tabmenu2.png`) — then clicked **`New Browser`**. A real
`Browser` tab opened, added to the sidebar's tab list under the worktree, showing full chrome:
back/forward/stop, the URL bar reading `https://example.com`, and the tab title `Browser`
(`37-after-new-browser.png`). The content pane shows the expected Wayland-lane webview limit
(`Direct XCB build failed: the window handle kind is not supported; ... GPUI returned
unsupported handle: Wayland(...)`), matching `WAYLAND-LANE.md`'s documented, lane-only
limitation — not the empty-arm defect the stale evidence described. `git log -S` on
`NewBrowser =>` (as triage already found) shows the real arm predates the pass-14 evidence;
this pass supplies the missing live re-verification triage asked for.

Captures: `33-baseline-win06.png`, `34-tabmenu.png` (menu from the sidebar-adjacent "+", for
contrast — before a worktree was selected, the tab-strip "+" opened the same-shaped menu once a
worktree was selected), `35-worktree-selected.png`, `36-tabmenu2.png`, `37-after-new-browser.png`.
