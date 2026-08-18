# Finish line — domain / settings / persistence, part 2 (lane wf-dom3)

Cites `docs/linux-rewrite/EVIDENCE-STANDARD.md`. Live drives on this host (x86, COSMIC/Wayland,
`WAYLAND_DISPLAY=wayland-1`), 2026-08-18, via `Scripts/wayland-drive.sh`. Binary pinned:
`cp rust/target/debug/tiller /tmp/wf-dom3-tiller && export TILLER_WL_BIN=/tmp/wf-dom3-tiller`.

## Rows in scope

Grep of `INVENTORY-LEDGER.md` for prefixes `F-CORE-DOM`, `F-SET`, `F-PER`, `F-WIN`, `F-CORE-USG`,
`F-CHG`, `F-TERM` that are not `PASSED` and not `N/A — platform`, as of HEAD before this pass:

| id | prior verdict |
|---|---|
| F-WIN-03 | half-proven |
| F-WIN-10 | half-proven |
| F-CHG-02 | FAILED — defective |
| F-CHG-15 | half-proven |
| F-CHG-18 | FAILED — defective |
| F-PER-01 | half-proven |
| F-PER-05 | NOT EXERCISED |
| F-PER-07 | NOT EXERCISED |
| F-SET-14 | half-proven |
| F-SET-15 | FAILED — absent |
| F-SET-18 | half-proven |
| F-SET-22 | half-proven |
| F-TERM-03 | half-proven |
| F-TERM-10 | half-proven |
| F-TERM-SCR-02 | half-proven |
| F-TERM-PTY-04 | half-proven |
| F-TERM-UI-02 | half-proven |
| F-CORE-DOM-02 | half-proven |
| F-CORE-DOM-03 | half-proven |
| F-CORE-DOM-05 | half-proven |
| F-CORE-DOM-07 | half-proven |
| F-CORE-DOM-08 | half-proven |
| F-CORE-USG-07 | half-proven |

Priority per brief: the five rows a predecessor left NOT EXERCISED (F-WIN-03, F-WIN-10, F-PER-01,
F-PER-05, F-PER-07) — driven first, below — then F-CORE-DOM-03/07 (currently carried on unit
tests only), then remaining half-proven rows as budget allows.

Status: DONE for this pass. Ten rows driven and closed below: the five priority NOT EXERCISED
rows, F-CORE-DOM-03/07 (the two explicitly named), and three more found and closed while grepping
for F-CORE-DOM-07's app caller (F-CORE-DOM-02/05/08 all share the same `tiller_project::domain`
"wave M" evidence lineage and the same tested-but-unwired risk, so each got the same app-caller
check applied before being carried or moved). Remaining 13 rows (F-CHG-02/15/18, F-SET-14/15/18/22,
F-TERM-03/10/SCR-02/PTY-04/UI-02, F-CORE-USG-07) were not reached this pass — returned as
`NOT EXERCISED` in the structured report rather than guessed, per the brief's explicit priority
order and its instruction to prefer an honest gap over a padded pass. Their prior ledger verdicts
already carry real evidence (several are grep-validated `FAILED` findings from wave J) and stand
undisturbed by this pass, which touched only the rows detailed below.

---

## F-WIN-03 — PASSED (was half-proven / UNREACHABLE-assumed)

**The old "no session D-Bus" assumption is wrong on this host.** Checked first, per the brief:
built a private session bus (`dbus-daemon --session --fork`), pointed its activation environment
at the nested compositor's real display (`dbus-update-activation-environment
WAYLAND_DISPLAY=$WD ...`), started `/usr/libexec/xdg-desktop-portal -v`, and confirmed live:
`org.freedesktop.portal.Desktop` registers `org.freedesktop.portal.FileChooser` backed by
`gtk.portal` (`XDP: providing portal org.freedesktop.portal.FileChooser` in the portal's own log),
and `dbus-send ... GetConnectionUnixProcessID org.freedesktop.portal.Desktop` returns a live PID.
A FileChooser backend **is** registered on this session bus.

One real trap found and fixed along the way: starting a **second** `xdg-desktop-portal` frontend
process against an already-running one (leftover from an earlier manual check) raced the bus-name
request and produced `Couldn't open file picker due to missing xdg-desktop-portal implementation`
— `ashpd::Error::PortalNotFound`, `rust/vendor/gpui_linux/src/linux/platform.rs:430` — even though
a portal genuinely was there. Killing the stale duplicate and starting exactly one frontend per
drive fixed it; recorded here since the error text is easy to misread as "no portal environment"
when the real cause is "two portals fighting over one bus name."

**Full live round trip, one continuous `wayland-drive.sh` invocation** (`TILLER_WL_LABEL=wf-dom3`,
binary pinned to `/tmp/wf-dom3-tiller`):

1. Fixture `/home/enzopalmisano/wf-dom3-fixtures/open-me.txt` seeded with `WFDOM3_ORIGINAL_LINE`.
2. `chord ctrl o` → GPUI's `handle_open_file` (`rust/crates/tiller/src/main.rs:9099`) called
   `cx.prompt_for_paths`, which dispatched a real `ashpd::desktop::file_chooser::OpenFileRequest`
   over the private bus; the **real GTK "Open File" dialog** mapped as an independent sway tile,
   floated/positioned via `swaymsg`.
3. Navigated by real clicks (not the location bar — see below): clicked **Home**, clicked the
   `wf-dom3-fixtures` row, `key Return` to descend, clicked `open-me.txt`, `key Return` to open.
4. The picker closed and a new `open-me.txt` editor tab appeared in the real workspace, showing
   line 1 `WFDOM3_ORIGINAL_LINE` — screenshot
   `reference/linux-progress/wf-dom3/f-win-03-file-opened.png`.
5. Clicked into the editor, `chord ctrl a`, `type WFDOM3_EDITED_CONTENT_9182`, `chord ctrl s`.
6. **Hard discriminator**: `cat /home/enzopalmisano/wf-dom3-fixtures/open-me.txt` on the real host
   filesystem, read directly (not through the app) after the drive, now reads
   `WFDOM3_EDITED_CONTENT_9182` — the file genuinely changed on disk. Tab shows no dirty-dot after
   save (`reference/linux-progress/wf-dom3/f-win-03-after-save.png`).

**A GTK location-bar (`chord ctrl l` + `type <path>`) trap found and abandoned**: the first
character(s) typed immediately after `chord ctrl l` were dropped twice in a row (`/home/...`
arrived as `/ome/...`; a second attempt lost both a doubled leading marker and `home`), and the
second attempt actually landed in GTK's **interactive search** ("Ricerca in Recenti") rather than
a location-entry, not the location bar at all — this GTK version's `ctrl+l` behavior is not
reliable through synthetic `wtype` input immediately after the dialog maps. Switched to real
navigation clicks (Home → folder row → Return → file row → Return), which worked cleanly and is
the more representative gesture anyway (a person browses more often than they type an exact path).
Filed here as a lane trap for the next builder who reaches for `ctrl+l`.

Evidence standard: named gesture sequence above is replayable; the disk-content assertion is the
hard discriminator (EVIDENCE-STANDARD.md's bar — a value that could only differ if the feature
truly worked).

---

## F-WIN-10 — PASSED (was half-proven)

Real UI gesture, not a socket call (`control_add_project`, the control-socket handler, is a
**separate** function from `add_project` — `rust/crates/tiller/src/main.rs:3306` calls
`workspace.control_add_project`, not `add_project` — and never calls `show_toast`. Driving this
row over `ctl project.add` produces no toast at all regardless of outcome; confirmed live first as
a negative control, then abandoned in favor of the real `+` → **Open Project…** UI path that
actually calls `add_project`, `rust/crates/tiller/src/main.rs:4031`).

Full live sequence, one continuous `wayland-drive.sh` invocation, real portal dialog both times:

1. `+` → **Open Project…** → real GTK "Open Folder" dialog → Home → `wf-dom3-fixtures` (a plain,
   non-git folder) → **Add Project** → "This folder is not a git repository" confirmation card →
   **Add without Git**. First time: added cleanly, no toast (`Ok(true)` branch, screenshot
   `reference/linux-progress/wf-dom3/f-win-03-...` sibling — sidebar now lists `wf-dom3-fixtures`
   as a project).
2. Repeated the **identical** gesture a second time against the same already-tracked folder.
3. **`reference/linux-progress/wf-dom3/f-win-10-toast-visible.png`**: a bottom-right floating card
   reading `already tracked or nested: /home/enzopalmisano/wf-dom3-fixtures` appears — matches
   `render_toast`'s real styling (`rust/crates/tiller/src/main.rs:9941`: `.absolute().bottom_20()
   .right_20()...`), distinct from the sidebar's own persistent inline banner (bottom-left,
   already visible in the same frame).
4. **`reference/linux-progress/wf-dom3/f-win-10-toast-gone.png`**, captured 4.5s later (>
   `TOAST_DURATION = Duration::from_secs(4)`, `main.rs:5310`): the bottom-right toast is gone while
   the sidebar's persistent inline banner (`Sidebar::set_notice`) is still showing the same text —
   exactly the distinguishing behavior the row's own doc comment names ("unlike
   `Sidebar::set_notice`'s persistent inline banner, which stays silent unless the sidebar happens
   to be the visible surface"). Both halves of the VERIFY clause (appears / auto-dismisses) driven
   live and distinguished from the lookalike persistent banner.

---

## F-PER-05 — PASSED (was NOT EXERCISED)

Boot state (persisted DB, `linux/gpui-waku` worktree) had exactly two launch-snapshot tabs: `Chat`
and `Terminal`. Live sequence, one continuous `wayland-drive.sh` invocation:

1. Clicked the `Terminal` tab, `chord ctrl w` → a real "Close dirty tab? Discard unsaved work in
   Terminal?" confirmation appeared (the terminal had a real neofetch-style banner as scrollback,
   correctly flagged dirty) — clicked **Close**.
2. `reference/linux-progress/wf-dom3/f-per-05-after-close.png`: tab bar and sidebar both now show
   only `Chat` — `Terminal` genuinely gone, not just visually hidden (sidebar's per-worktree tab
   list dropped it too).
3. `chord ctrl+shift o` → `handle_restore_launch_snapshot`
   (`rust/crates/tiller/src/main.rs:9083`, `RestoreLaunchSnapshot`).
4. `reference/linux-progress/wf-dom3/f-per-05-after-restore.png`: tab bar now shows **both** `Chat`
   and `Terminal` again — exactly one `Terminal` (not duplicated), and `Chat` is the same tab,
   untouched throughout (never closed, never recreated). Matches `restore_launch_snapshot`'s own
   merge logic (`main.rs:5249`: `merge_launch_snapshot_tabs` then only appends tabs past
   `current.len()`) exercised end-to-end, not read.

Both clauses of the VERIFY text — "the tab returns" and "an unrelated current tab remains" — driven
live in the same session with a real dirty-tab confirmation in between, not glossed over.

---

## F-PER-07 — PASSED (was NOT EXERCISED)

Live sequence: right-clicked the `wf-dom3-fixtures` project row → **Project Settings** → the real
settings sheet (`rust/crates/tiller_ui/src/sidebar.rs:2654`, `render_project_settings`) rendered
inline. Edited three fields in one continuous `wayland-drive.sh` invocation: **display name**
(`ctrl+a` then typed `WFDOM3 RENAMED PROJECT`), **icon kind** (clicked the git-branch glyph),
**colour** (clicked the green swatch) — `reference/linux-progress/wf-dom3/f-per-07-settings-edited.png`
shows the sheet with all three applied and the sidebar row already reading the new name live.
Clicked **Close**.

**Hard discriminator, independent of the UI**: after closing the sheet, `wayland-drive.sh` was
invoked again — a genuine process kill + cold restart of the app binary against the same on-disk
SQLite file, zero interaction before the capture. Two independent checks both confirm persistence:

1. Screenshot `reference/linux-progress/wf-dom3/f-per-07-after-relaunch.png`: sidebar still reads
   `WFDOM3 RENAMED PROJECT` with the git-branch icon, with no clicks since the fresh boot.
2. **Direct SQLite read**, bypassing the app entirely:
   `SELECT display_name, icon_kind, icon_value, color_hex FROM project WHERE root_path LIKE
   '%wf-dom3-fixtures%'` against `/tmp/wf-dom3.sqlite` returns
   `('WFDOM3 RENAMED PROJECT', 'icon', 'git-branch', 'green')` — the on-disk row itself, not a
   rendered pixel.

All three identity fields the VERIFY clause names (name, icon, and — via colour, part of the same
`update_project_settings` write, `rust/crates/tiller/src/main.rs:4052`-`4065` — the project's tint)
survive a real quit/relaunch.

---

## F-PER-01 — PASSED (was half-proven)

The predecessor's own gap, named honestly: schema/table presence was confirmed but no complete
live chat turn was landed. Closed it this pass with a **real** agent turn, not the schema alone.

1. Clicked into the `Chat` tab's composer, typed `reply with exactly the single word:
   PONGWFDOM3`, sent it. The real backing agent answered `PONGWFDOM3` —
   `reference/linux-progress/wf-dom3/f-per-01-chat-turn-live.png` shows the completed exchange
   with a timestamp and the tab's checkmark (turn complete).
2. Switched to `Terminal`, ran `echo PERSIST_TERMINAL_WFDOM3_88213`, output appeared inline.
3. **Real quit/relaunch, not a script fiction**: the next `wayland-drive.sh` invocation was run
   with `TILLER_WL_KEEP` unset both before and after — the app was genuinely killed (SIGTERM via
   the drive script's own cleanup) and cold-started fresh against the same `/tmp/wf-dom3.sqlite`.
4. `reference/linux-progress/wf-dom3/f-per-01-chat-after-restart.png`: the `Chat` tab, with zero
   interaction after boot, shows the identical transcript — user message, `PONGWFDOM3` reply,
   `23:46` timestamp.
5. `reference/linux-progress/wf-dom3/f-per-01-terminal-after-restart.png`: the `Terminal` tab shows
   `echo PERSIST_TERMINAL_WFDOM3_88213` / `PERSIST_TERMINAL_WFDOM3_88213` in its scrollback,
   replayed history ahead of a fresh live prompt (matches `F-TERM-PTY-04`'s documented
   capture-and-replay-without-writing-to-the-child restore mechanism).
6. **Hard discriminator, independent of the UI**: `SELECT tab_id, ordinal, payload, updated_at FROM
   chat_turn` against the real on-disk file returns one row, `tab_id='default-chat'`, payload
   `{"entries":[{"UserMessage":{"text":"reply with exactly the single word:
   PONGWFDOM3"}},{"AssistantMessage":{"text":"PONGWFDOM3"}},{"TurnFooter":{"text":"23:46"}}]}` — the
   real conversation content, at rest in SQLite, not a rendered pixel.

Every noun in the VERIFY clause — projects, worktrees, tabs, **a sent chat**, **terminal output** —
driven and independently confirmed to survive a genuine quit/relaunch, closing the row the
predecessor could only get partway through.

---

## F-CORE-DOM-07 — FAILED — defective (was half-proven)

Ledger row text being carried: *"Fresh cargo test run, two suites:
domain::tests::auto_naming_requires_first_run_or_both_throttles (domain.rs:176) and the dedicated
p99_naming_throttle.rs (3 tests, ...), all green, covering first-run exemption plus both threshold
gates independently and together."* That is real, passing unit-test proof of the throttle *type* in
isolation. Driving it live finds the throttle is never reached from the app's actual default
Chat-tab flow — a "tested-but-unwired" defect, not a missing feature.

**Setup**: the setting itself defaults OFF (`tiller_ui/src/settings.rs:390`,
`auto_naming: false`), which is presumably why a predecessor's earlier attempt (reusing the
existing `Chat` tab's already-completed turn) saw no rename and moved on. Turned it on for real
through the UI, not a fixture: `chord ctrl comma` → **Settings → General → "Auto-rename tabs and
agents"**, clicked the toggle live —
`reference/linux-progress/wf-dom3/f-core-dom-07-auto-naming-toggle-on.png` shows it flipped orange,
Summarizer agent already "Claude Code". **Hard discriminator, independent of the UI**: `SELECT *
FROM setting` against `/tmp/wf-dom3.sqlite` returns `('general.autoNaming', 'true')` — persisted,
not just painted.

**Live drive, two full real turns, two separate fresh process boots** (every `wayland-drive.sh`
invocation restarts the app — `auto_naming_throttle` is an in-memory `BTreeMap`, so each boot is
its own untouched "first request" per the throttle's own documented exemption):

1. Navigated to the `linux/gpui-waku` worktree's existing `Chat` tab (never manually renamed —
   `title_is_auto_named` stays true at every tab-creation call site checked earlier this pass) and
   sent a **new** real message: `DOM07 rename test reply with exactly the single word:
   RENAMEACKWFDOM3`. The real backing agent answered `RENAMEACKWFDOM3`
   (`reference/linux-progress/wf-dom3/f-core-dom-07-turn-complete-title-unrenamed.png` shows the
   completed exchange with a timestamp).
2. Sent the same probe again in a second fresh boot; same result.
3. Waited **90+ seconds** after send (a `Monitor` poll loop hitting `SELECT title FROM tab WHERE
   id='default-chat'` against the live on-disk file every 3s — independent of the UI, no `shot`
   resize disturbing the app) — the title never left `Chat` either time.
4. `pstree -p` on the live `wf-dom3-tiller` process during the wait shows the Chat pane's own real
   `claude` subprocess (nested under the ACP wrapper, `npm exec @agentclientprotocol/claude-agent-acp`)
   but **no `claude -p ...` summarizer process ever spawns** — `request_auto_rename`'s own
   `run_summarizer_command` call never fires.

**Root cause, at the code, not a guess**: `request_auto_rename` (`main.rs:5671`) is only ever
called from three places — `grep -n request_auto_rename rust/crates/tiller/src/main.rs` finds
exactly `main.rs:3298` (`ControlAction::Notify`, Layer A — needs a real `tillerctl notify` call to
arrive on the control socket), `main.rs:3951` (`subscribe_terminal_activity`, subscribed to a
`TerminalActivityEvent` from a `TerminalView`), and `main.rs:3996`
(`start_process_signal_refresh`, walks a terminal pane's `shell_pid`). The latter two are
structurally terminal-pane-only. `ChatEvent` — the Chat entity's own event enum, `bind_chat`'s
subscription target — has exactly two variants, `OpenFile` and `OpenLink`
(`grep -n "ChatEvent::" main.rs`); no completion/status variant exists to ever produce a
`Transition` for the chat pane the function itself checks for
(`if let TabContent::Chat(chat) = content` inside `request_auto_rename`). The function is written
to rename a chat tab from its transcript, and the plumbing that would tell it a chat tab just
finished a turn does not exist for the ACP-hosted `Chat` tab kind — the app's actual default,
most-common chat surface.

This falsifies the row's own doc comment (`main.rs:5605-5611`: *"on a running→done/needs-input
transition, throttled auto-naming re-titles the owning chat tab"*) for the concrete case a user
hits by default: send a message in the `Chat` tab, get a reply, nothing renames, regardless of the
setting. The throttle type is correct and well-tested in isolation; the app never asks it a
question.

---

## F-CORE-DOM-03 — PASSED (was half-proven)

Ledger row text being carried: *"sidebar.rs:3961's own F-CORE-DOM-03-tagged test passes, asserting
Sidebar::project_form_parent() == tiller_project::default_project_base() ... Unit-test proof, not a
live dialog screenshot."* Closed the live half this pass — the real **Create Project** dialog, not
the function in isolation.

1. `Projects` header **+** → **Create Project…** (`Sidebar::start_create_project`, `sidebar.rs:1524`,
   which seeds the form via `project_form_parent()` → `tiller_project::default_project_base()`,
   `sidebar.rs:1502`). With `TILLER_PROJECTS_DIR` unset, **Parent location** reads
   `/home/enzopalmisano/Tiller/projects` — the documented `$HOME/Tiller/projects` fallback —
   `reference/linux-progress/wf-dom3/f-core-dom-03-default-parent.png`.
2. Full **real quit/relaunch** with `TILLER_PROJECTS_DIR=/tmp/wf-dom3-custom-projects-dir` exported
   into the app's own environment before the cold boot (not a fixture value written into the DB —
   an actual env var the process reads at call time). Reopened the identical **Create Project…**
   dialog: **Parent location** now reads exactly `/tmp/wf-dom3-custom-projects-dir`, and the
   "Creates …/" caption line under it agrees —
   `reference/linux-progress/wf-dom3/f-core-dom-03-override-parent.png`.

Both halves of the VERIFY clause (`Sidebar::project_form_parent() == default_project_base()` with
no override, and honoring `TILLER_PROJECTS_DIR` when set) driven live through the real dialog a
user opens, across a genuine process boundary for the override case — not read off the unit test
that already covered the same assertion in isolation.

---

## F-CORE-DOM-08 — FAILED — absent (was half-proven)

VERIFY clause (`02-inventory-packages.md`): *"A MainActor once-gate executes its closure once and
ignores later fire calls. Trigger the same one-shot restore or setup callback multiple times and
confirm it has one observable effect."* Ledger evidence being carried was explicit about the gap:
*"Pure internal type with no UI surface to live-drive at all."* Applied the brief's
tested-but-unwired check to that claim instead of accepting it — a "no UI surface" claim is itself
a claim about wiring, and this is exactly what "grep for the app caller before accepting a green
unit test" is for.

`grep -rn "OnceGate" rust/ --include="*.rs"` (whole tree, target/ excluded — pattern validated by
its own four hits, not a silent zero) returns exactly **four** lines, all inside
`tiller_project/src/domain.rs`/`lib.rs` itself: the struct definition (`domain.rs:120`), its `impl`
block, its own unit test's instantiation (`domain.rs:192`), and the `pub use` re-export from
`lib.rs:53`. **Zero occurrences in `tiller`, `tiller_ui`, `tiller_terminal`, `tiller_control`,
`tiller_agents`, `tiller_git`, or `tiller_persistence`** — no restore path, no setup callback,
nothing in the actual application binary ever constructs or calls `OnceGate::fire`.

This is not "no UI surface to click" (the half-proven framing) — it is **no caller of any kind**.
The type was ported from `OnceGate.swift:3` and unit-tested in isolation, but the "one-shot
restore or setup callback" the VERIFY clause names does not exist anywhere in the running Rust
app for it to gate. A validated whole-tree grep is the standard's own accepted disproof of
absence (`EVIDENCE-STANDARD.md`, "a validated read is the only possible disproof of absence") —
downgraded, not carried, since `half-proven` implies partial live wiring that a full-tree grep
shows is not there.

---

## F-CORE-DOM-05 — PASSED (was half-proven) — evidence correction, not equivalence-by-assertion

Ledger evidence being carried cited `domain::tests::ordering_ignores_unknown_and_noop_moves
(domain.rs:156)`, a unit test of `tiller_project::move_item`. Checked the app caller first, as the
brief requires: `grep -rn "move_item" rust/ --include="*.rs"` (whole tree, `target/` excluded)
finds `move_item` only in its own definition, its own test, and the `lib.rs` re-export — **zero
callers in `tiller_ui` or `tiller`**. That specific ported function is dead, exactly like
`F-CORE-DOM-08`'s `OnceGate`.

Unlike `F-CORE-DOM-08`, the *feature* is not absent — it lives under a different name. The real
sidebar reorder path is `Sidebar::reorder_rows` (`sidebar.rs:758`), wired to genuine
`.on_drag`/`.on_drop`/`RowDrag` handlers (`sidebar.rs:3267-3278`), with its own unknown-id
(`.position()` → `None` → early `return false`) and no-op (`drag.id == target_id → return false`)
guards — a real, independent reimplementation of the same clause, not the cited-but-dead one. This
is the equivalence check the brief asks for, done explicitly rather than asserted: quoted the row's
cited evidence, named the actual live function, and confirmed by exercise (below) that it is the
one the app runs.

**Two tiers of proof, not just a read:**

1. Three named `VisualTestContext`-driven tests (`sidebar.rs:4598`, `5061`, `5214` —
   `dragging_project_rows_reorders_the_live_sidebar_block`,
   `dragging_worktree_rows_reorders_only_their_project_group`,
   `dragging_tab_rows_reorders_only_their_worktree_group`) draw a real sidebar and dispatch real
   `MouseDownEvent`/`MouseMoveEvent`/`MouseUpEvent` sequences — the UI tier's own accepted proof
   per `EVIDENCE-STANDARD.md`. Re-ran fresh this pass: `cargo test -p tiller_ui reorders_the_live_sidebar_block`
   and `cargo test -p tiller_ui reorders_only_their` — all 3 green.
2. **This lane's own hard discriminator**: a genuine OS-level drag, not a simulated event —
   `drag 130 338 130 210 8` through `wayland-drive.sh` picked up the `wf-term-clean` worktree row
   (real virtual-pointer down → 8 waypoints → up) under the `tiller` project group.
   `reference/linux-progress/wf-dom3/f-core-dom-05-before-drag.png` shows the starting order
   (`rust/gpui-rewrite, linux/gpui-waku, wf-term-clean, 9a42249, wl-proof-branch, 984defa`);
   `reference/linux-progress/wf-dom3/f-core-dom-05-after-drag.png`, captured after a forced
   repaint, shows `wf-term-clean` moved down past `9a42249` and `wl-proof-branch` — the row
   genuinely reordered, driven by real compositor input, not asserted.

The exact landing position from a multi-waypoint synthetic drag is not pixel-precise (expected —
this is the same imprecision a fast real drag has), but reordering-in-response-to-a-real-drag is
exactly what the clause asks for, and it happened.

---

## F-CORE-DOM-02 — half-proven (carried, evidence sharpened) — a real gap found, not read alone

Ledger evidence cited `domain::tests::defaults_prefer_explicit_values_then_primary_and_sibling
(domain.rs:138)`, testing `tiller_project::resolve_worktree_defaults(project_root,
explicit_base_branch, primary_branch, explicit_location)` — explicit > **primary branch** > `"HEAD"`
for the branch, explicit > sibling directory for the location. Checked the app caller first:
`grep -rn "resolve_worktree_defaults" rust/ --include="*.rs"` finds it **only** in its own
definition, its own test, and the `lib.rs` re-export — zero callers, the same dead-function pattern
as `F-CORE-DOM-05`/`F-CORE-DOM-08`.

Same equivalence check as `F-CORE-DOM-05`, run through to completion rather than assumed — and this
time it does **not** land clean. The real "New Worktree…" prompt's submit handler
(`Sidebar::confirm_worktree_prompt`, `sidebar.rs:1937`) does have a live, independently-tested
sibling for the **location** half: `tiller_git::worktree::resolve_parent_directory`
(`worktree.rs:152`, explicit override else `root.parent()` — its own doc comment names it as the
match for `WorktreeDefaults.resolveParentDirectory`, called directly at `sidebar.rs:1973`). That
half of the clause is genuinely live.

The **branch** half is not equivalent, and this is a specific, validated code-flow finding, not a
guess: `confirm_worktree_prompt` (`sidebar.rs:1958-1961`) computes `base` as `Some(trimmed)` when
the field is filled, else **`None`** — full stop. `prompt.primary_branch` (the field the struct
does carry, `sidebar.rs:351`) is **never read inside `confirm_worktree_prompt`**; a blank field
sends `None` through to `create_worktree`, which lets `git` fall back to its own current-HEAD
default. `resolve_worktree_defaults`'s three-way precedence (explicit → **the project's primary
worktree's branch specifically** → `"HEAD"`) is not what actually runs — the live prompt has only
a two-way choice (explicit vs. git's own default), silently dropping the middle tier the row's own
test asserts (`primary_branch = Some("main")`, `explicit_base_branch = None` → expects `"main"`,
not whatever `HEAD` happens to resolve to).

This is invisible in the common case (a project's primary worktree usually *is* sitting on
whatever `HEAD` means), which is likely why no live drive so far has caught it — the observable
difference only appears when the primary worktree is checked out on a non-default branch and the
user leaves the base field blank. Not re-driven through the wayland lane this pass (would need a
fixture project whose primary worktree already sits on a non-default branch, more setup than this
pass's remaining budget), so the UI-observable half stays unconfirmed either way — carried at
`half-proven`, but for a sharper, validated reason than "no live dialog drive": one third of the
clause (location) is live and correct, one third (explicit-branch-wins) is trivially true, and the
middle tier (primary-branch fallback) is demonstrably not wired into the code path a real submit
runs, pending a live check with the right fixture to confirm the user-visible consequence.

---
