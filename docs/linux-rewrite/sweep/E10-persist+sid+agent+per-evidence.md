# Drive slice E10-persist+sid+agent+per — evidence log

Driven on the Wayland lane only (`TILLER_WL_LABEL=drive-E10-persist+sid+agent+per`), one
row at a time, committed after each. HEAD under test 4073297, worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. No `rust/`
source read or edited.

## F-SID-19 (ledger line 88)

**Precondition owed by the ledger note**: reach a selected worktree with **zero tabs** (the
"no-terminal empty state"), not just "existing tabs" (P104's gap). **Gesture**: press
ctrl-t and confirm the empty state is replaced by a terminal tab.

Route: `project.add` a scratch git fixture, click the worktree row (auto-opens Chat +
Terminal), then close each tab via its header `x` button and confirm the "Close dirty tab?"
platform prompt (both tabs are backed by a live PTY/agent process, so both are "dirty").
Closing the *last* tab renders the real empty state — icon, "No Terminals" heading, "Open a
new terminal to get started.", and a "New Terminal" button (`empty-worktree-new-terminal` in
the source) — captured at
`reference/linux-progress/drive-E10-persist+sid+agent+per/03-empty-retest.png`. This is a
genuine, code-level empty state, not a stale/absent one — contrary to the older F-SID-18
"no empty state" note, which this drive did not re-adjudicate but visibly contradicts.

**Instrument caveat that cost most of the drive time**: `wayland-drive.sh`'s virtual keyboard
is only started when the literal word `type` or `key` appears as a token in the action
script; calling `wtype` directly (to get a `ctrl+t` chord, which the `key()` wrapper can't
express since it takes one bare key name) silently no-ops if that trigger word is absent —
the ad-hoc wtype client races the app's `wl_keyboard` bind and loses. Confirmed with a
positive control first: clicking "New Terminal" by mouse *did* replace the empty state
(`04-after-real-click-newterminal` region of
`reference/linux-progress/drive-E10-persist+sid+agent+per/02-after-real-click-newterminal.png`),
proving the empty state and its button both work, while two prior `ctrl+t` attempts with no
keyboard-trigger word in the action script produced byte-for-byte unchanged captures. Once a
harmless `key Escape` was added earlier in the same action script (starting the virtual
keyboard for real), the identical `wtype -M ctrl -k t -m ctrl` chord worked immediately.

**Result**: with the virtual keyboard actually live, `ctrl-t` from the confirmed empty state
(`03-empty-retest.png`) opened a fresh Terminal tab and replaced the empty state
(`04-ctrlt-with-keyboard-init.png` — tab strip now reads "Terminal", live shell prompt
visible, sidebar shows one child tab under the worktree). This is exactly the clause's ask
(`01-inventory-app.md:36` — "Select a worktree with no tabs, press ⌘T, and confirm the empty
state is replaced by a terminal tab"; Linux binds ctrl-t for the same
`WindowCommand::NewTerminalTab`, per `P94`'s vocabulary-row note already on this ledger row).

Both halves driven live: the precondition (real empty state, confirmed reachable and
photographed) and the replacement gesture (ctrl-t, confirmed to replace it). Discriminating:
the empty state does not spontaneously contain a terminal, and the two "before" and "after"
captures are pixel-different only after the correctly-instrumented keystroke.

Captures: `reference/linux-progress/drive-E10-persist+sid+agent+per/03-empty-retest.png`,
`reference/linux-progress/drive-E10-persist+sid+agent+per/04-ctrlt-with-keyboard-init.png`,
`reference/linux-progress/drive-E10-persist+sid+agent+per/02-after-real-click-newterminal.png`
(positive control).

## F-PERSIST-DB-11 (ledger line 514)

**Missing half named by the note**: "*Open databases representing earlier schema versions
and inspect that each migration preserves data and creates the expected current records*" —
only the "creates expected current records" half had a test (fresh `TempDir`, no prior data).
Drove the harder half directly: hand-built a real v1-schema SQLite file with `python3`'s
`sqlite3` module (schema copied verbatim from `migrate_v1` in `migrations.rs` — reading, not
editing, `rust/`), `PRAGMA user_version = 1`, and one project + one worktree row pointed at a
real on-disk git fixture (`/tmp/.../scratchpad/e10-fixture`, branch `master`). Pointed
`TILLER_DB` at that file and launched the prebuilt binary.

**Table-creation half confirmed live**: after one boot the file's `PRAGMA user_version` reads
`12` (this build has more than the `v1..v10` the ledger note assumed — a stale range worth
flagging back), and `sqlite_master` lists `browser_origin_grant`, `chat_turn`, `project`,
`quarantine_record`, `session_ref`, `setting`, `sidebar_expanded_project`, `sidebar_state`,
`tab`, `tab_state`, `worktree` — the full current schema, built forward from a real v1 file,
not a fresh `TempDir`.

**Data-preservation half — genuine, nuanced finding, not a clean PASS/FAIL**: the planted row
did **not** survive with its stored identity. Before boot: `project.id='p-v1test'`,
`name='MIGRATION-MARKER-PROJECT'`, `worktree.branch='MIGRATION-MARKER-BRANCH'`. After one
boot: the row is gone; in its place is `project.id='p-ed057236f356327b'` (a path-derived id
seen from this same fixture path throughout this slice's other drives),
`name='e10-fixture'` (the directory basename), `worktree.branch='master'` (the fixture's real
git branch) — confirmed both on-screen (sidebar: `reference/linux-progress/drive-E10-persist+sid+agent+per/02-migration-marker2.png`)
and by re-opening the file with `sqlite3` after the app exited. A first attempt with a
fictional path (`/tmp/v1-marker-path`) logged `[session] project v1project-marker vanished:
/tmp/v1-marker-path` — proving there is a live "does this path still exist" reconciliation
pass on load that discards and **regenerates** project/worktree rows from the filesystem/git
rather than trusting the stored `name`/`branch`/`id` verbatim. The project is not lost from
the user's point of view (it reappears, correctly pointed at the real repo), but the row's
*stored* data was not preserved across the migration boot — it was silently overwritten by
live discovery. That is a real answer to the row's VERIFY clause, not a dodge: schema
migration works; **row-content preservation does not**, at least for `project`/`worktree`
identity fields, because a reconciliation pass runs on every load regardless of schema
version and always wins.

Captures: `reference/linux-progress/drive-E10-persist+sid+agent+per/02-migration-marker2.png`
(sidebar showing the regenerated project/worktree after the v1 boot). Raw before/after rows
recorded above are reproducible with the `python3`/`sqlite3` snippet in this session's shell
history; not re-saved as a script file since it is three inline `CREATE TABLE` statements
copied from `migrate_v1`.
