# T8-ctrl-sid — build-fleet plan (F-CTRL, F-SID)

Read-only triage. No code changed, no verdicts set. Each section names the row's current
ledger verdict, what it actually needs, the files a fix would touch, and the approach.

All line numbers below were re-checked against the current `linux/gpui-waku` HEAD
(`rust/crates/tiller/src/main.rs` is 12298 lines; it has moved substantially since several
ledger rows' evidence was recorded — cite the numbers here, not the ones in
`INVENTORY-LEDGER.md`, if the two disagree).

---

## Shared cause — the browser cluster's evidence predates its own fix

`F-CTRL-BROWSER-02` through `F-CTRL-BROWSER-06` (5 of my 14 rows) all carry evidence
timestamped **03:02:56** on 2026-08-14, captured against the pre-`P90` binary. Commit
`988d9e9` ("fix: make browser control responses honest", landed **04:29:51**, same day)
rewrote the entire `browser.*` dispatch — `ControlAction::Browser` gained a `reply` channel it
previously lacked (`rust/crates/tiller/src/main.rs:317-321`), `browser.open` now validates and
returns `surface`/`url`/`title` (`main.rs:4476-4491`), and every method the app does not
implement now fails **immediately and honestly** at `browser_request_error`
(`main.rs:201-239`, gated by `BROWSER_CAPABILITIES = ["browser.open", "browser.navigate",
"browser.act"]` at `main.rs:197`) rather than returning `queued:true` for a no-op. Only
`F-CTRL-BROWSER-01` was re-exercised after the fix (ledger, "orchestrator headless-lane probe,
2026-08-14 05:05") and flipped to PASSED. Rows 02-06 were never re-run — their evidence
literally describes code that commit `988d9e9` already deleted (the `main.rs:4638-4643`
seven-way no-op arm the ledger quotes does not exist anymore; I grepped for it and read the
current `handle_browser_action`, `main.rs:4469-4528` — the no-op arm is gone, replaced by the
pre-dispatch honest-error gate).

There is also a **standing product-policy answer**, not just a bugfix, that changes how these
rows should be read: `docs/linux-rewrite/tasks/P90-the-socket-that-says-yes.md`'s orchestrator
reply (2026-08-14 10:55) rules explicitly: *"the target state is not 'three methods forever'
... three methods that work and are advertised, seven that are visibly unimplemented."* That
is a deliberate scope ruling for `browser.get`/`screenshot`/`snapshot`/`wait`/`eval`/`console`
and for `browser.navigate`'s back/forward/reload and `browser.act`'s click/fill/type/press/
scroll — not an oversight. Each row below says which half of its clause that ruling now covers
and which half is still a real gap under the clause's literal wording.

**Fleet implication:** whoever re-verifies this cluster should re-run the exact `dbus-monitor`-
style socket probe fresh (all ten methods, current binary) rather than trusting any of 02-06's
recorded evidence, and should read the P90 policy answer before deciding whether the surviving
gaps are `FAILED` or `N/A — platform` under the new ruling.

---

## F-CTRL-BROWSER-02 — `browser.open`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: reclassify**

Both defects the row cites are gone in current code:
- "returns no surface identifier, URL or title" — false now. `main.rs:4482-4490` returns
  `surface`, `url`, `title` from the created `BrowserSurface`.
- "`ControlAction::Browser` ... no `reply` field, so no result can ever be returned" — false
  now. `main.rs:317-321` shows `Browser { method, params, reply }`, and the dispatch arm at
  `main.rs:2462-2472` sends `reply.send(result)`.
- "does not reject a missing URL — silently defaults to `https://example.com`" — false now.
  Rejected twice: pre-dispatch at `main.rs:209-216` (`browser_request_error`) and again inline
  at `main.rs:4477-4481`.

What the clause still asks that current code does not do: reject on **missing workspace
context** or an **unavailable adapter**. `add_browser_tab` (`main.rs:4419-4452`) creates the
tab unconditionally — there is no workspace-context check, and there is no "adapter"
concept for a native-WebKit browser surface on this platform (no `browser.rs` equivalent of
an agent-adapter registry to be "unavailable"). Whether that half of the clause even
translates to this platform is a judgment call for whoever reclassifies this, not something I
should resolve read-only.

**files:** `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239,
`handle_browser_action` :4469-4528, `add_browser_tab` :4419-4452, `ControlAction::Browser`
:317-321, dispatch arm :2462-2472)

**approach:** re-run the live socket probe (missing URL, present URL, — there is no
"workspace" or "adapter" parameter in the current wire protocol to even send) against the
current binary; if the fleet decides workspace-context validation is in scope, that is a
`needs: build` addition to `browser_request_error`/`handle_browser_action`, not a reclassify.

**size:** S (verification); the possible workspace-context addition, if wanted, is S-M.

---

## F-CTRL-BROWSER-03 — `browser.navigate` / `browser.get`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: both** (reclassify the stale half, build the real gap)

`browser.navigate` with a URL works today (`main.rs:4497-4510`, via `submit_address`) and
returns `url`/`title` — that part of the ledger's "implements none of the clause's three
operations" is stale. But back/forward/reload genuinely are not implemented: any `action`
param is rejected up front with an honest error (`main.rs:217-219`,
`"{method} action is unsupported on Linux: only URL navigation is implemented"`) — there is no
route to them at all. `browser.get` is entirely unimplemented — it fails at the
`BROWSER_CAPABILITIES` gate (`main.rs:197`, `main.rs:202-206`) before ever reaching a handler;
no selector/format/text/HTML extraction code exists anywhere in `BrowserSurface`.

**files:**
- `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239, `handle_browser_action`
  :4496-4527, `BROWSER_CAPABILITIES` :197)
- the `BrowserSurface` type itself (search `struct BrowserSurface` — not in `main.rs`; it's the
  browser surface crate/module `add_browser_tab` constructs at `main.rs:4428`) would need new
  methods for back/forward/reload and for reading page text/HTML if this is built for real.

**approach:** decide against the P90 policy ruling first — "visibly unimplemented" was an
explicit, accepted target state for `browser.get`. If the fleet wants literal clause
compliance instead, back/forward/reload is the smaller half (wraps existing WebKit
navigation-history calls, if the surface exposes them) and `browser.get`'s text/HTML
extraction is the larger half (needs a real DOM/text-extraction hook into the WebKit view).

**size:** navigate back/forward/reload: S-M. `browser.get`: M-L (depends on what the WebKit
binding already exposes — I did not chase that into the browser surface's own crate).

---

## F-CTRL-BROWSER-04 — `browser.screenshot` / `browser.snapshot`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: both**

Both methods are honestly refused today (`BROWSER_CAPABILITIES` gate, `main.rs:197`,
`202-206`) rather than the old silent `queued:true`/no file/no data — the ledger's specific
"wrote no file" / "no generation and no nodes" complaint described the old no-op arm, which no
longer exists. Under the P90 policy these two are explicitly in the "seven visibly
unimplemented" bucket. Under the clause's literal wording they are still not built at all —
zero screenshot-to-disk code, zero accessibility-node/generation code anywhere reachable from
`handle_browser_action`.

**files:** `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239,
`handle_browser_action` :4469-4528) plus the `BrowserSurface` module for the actual
screenshot/accessibility-tree capture, which does not exist yet.

**approach:** re-verify honestly-refused behavior live first (cheap, closes the stale-evidence
half). Building screenshot support needs a WebKit surface-to-PNG capture call; snapshot needs
an accessibility-tree walk — both are new capability surface on `BrowserSurface`, not just
socket plumbing.

**size:** L. Neither is "wire it up" — both need new capture machinery in the browser surface
that nothing in this codebase does yet.

---

## F-CTRL-BROWSER-05 — `browser.act` / `browser.wait`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: both**

`browser.act` today supports exactly one thing — a `driving`/`agentDriving` boolean flag
toggling `set_agent_driving` (`main.rs:4512-4523`) — and is honest about the limit: missing the
flag returns `"browser.act is unsupported on Linux: only the driving flag is implemented"`
(`main.rs:229-236`, `4516-4519`). Click/fill/type/press/scroll are not implemented at all —
the P90 orchestrator's own reply flags this ambiguity and asks the builder to say plainly
whether "driving" belongs with the honest three or the honest seven; as shipped it is
advertised (`BROWSER_CAPABILITIES` includes `browser.act`, `main.rs:197`) but only does the one
thing. `browser.wait` is entirely unimplemented (fails the capabilities gate, same as
browser.get/screenshot/snapshot) — no selector/text/URL/load-state/function condition is
parsed anywhere.

**files:** `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239,
`handle_browser_action` :4512-4527), plus `BrowserSurface` for real click/fill/type/press/
scroll input-injection and for any wait-condition polling loop — neither exists today.

**approach:** the "driving flag" ambiguity is worth flagging to whoever reclassifies this row
even before new code — P90's own author never definitively settled it. Building the five
click/fill/type/press/scroll actions needs synthetic-input injection into the WebKit view
(the same category of primitive `Scripts/wayland-virtual-pointer.c` provides for real OS input,
but this would be a programmatic/DOM-level injection, not an OS-level one). `browser.wait`
needs a poll loop against whichever of those becomes available.

**size:** L.

---

## F-CTRL-BROWSER-06 — `browser.eval` / `browser.console`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: both**

Same shape as -04: both honestly refused today via the capabilities gate rather than the old
silent no-op; the ledger's "the script is never read from params" / "the cursor is ignored"
complaints describe code (`main.rs:4638-4643` in the old evidence) that no longer exists.
Neither is built — no JS-evaluation bridge into the WebKit view, no console-message buffer
with a cursor.

**files:** `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239,
`handle_browser_action` :4469-4528) plus `BrowserSurface` for a `evaluate_javascript`-style
call and a console-message ring buffer with cursor support — neither exists.

**approach:** re-verify the honest-refusal half live first. Building `eval` needs whatever
`webkit_web_view_evaluate_javascript`-equivalent binding this project's WebKit wrapper exposes
(I did not chase into the browser surface's dependency to confirm it's already linked).
`console` needs a `console-message` signal handler feeding a bounded ring buffer, keyed by
cursor.

**size:** L.

---

## F-CTRL-CLI-02 — installed `tillerctl` symlink + real agent hook

**Ledger verdict:** NOT EXERCISED.

**needs: exercise**

The code this row asks about is real and already wired into the agent-spawn path, not stubbed.
`resolve_tillerctl_for_process` / `resolve_tillerctl_path` (`main.rs:7868-7911`) installs a
symlink at `$XDG_DATA_HOME/TillerRust/bin/tillerctl` (`TILLERCTL_INSTALL_SUBPATH`,
`main.rs:7858`) pointing at the running app's sibling `tillerctl` binary, falling back to a
`PATH` search, and is called from both agent-tab creation sites
(`main.rs:4601` and `main.rs:5061` — i.e. `add_agent_tab` and the second agent-spawn call site)
before a hook is ever written, so every agent adapter's hook config genuinely receives this
resolved absolute path. `install_tillerctl` (`main.rs:7948-7975`) does the actual
`symlink_metadata`/`std::os::unix::fs::symlink` work. Three tests already exercise
`resolve_tillerctl_path` directly (`main.rs:10225`, `:10264`, `:10280`), which is exactly why
the row can't be more than NOT EXERCISED off tests alone.

What's actually missing is the live proof P116 didn't do: (1) launch the real app, open an
agent tab, and inspect `$XDG_DATA_HOME/TillerRust/bin/tillerctl` on disk — confirm it's a
symlink and that it resolves and executes; (2) let a real agent (not a manual socket client)
invoke it from inside its own hook — e.g. drive a Claude Code pane to a real status
transition and confirm the Layer-A `tillerctl notify` call the hook fires lands over the
socket, the same way `F-CTRL-SESSION-01`/`F-CTRL-SYS-02` were already proven live per the
ledger.

**files:** `rust/crates/tiller/src/main.rs` (`resolve_tillerctl_path`/`resolve_tillerctl_for_process`
:7868-7911, `install_tillerctl` :7948-7975, call sites :4601, :5061) — no changes anticipated;
listed for the fleet's reference only.

**approach:** open an agent tab in the running app, `ls -la $XDG_DATA_HOME/TillerRust/bin/tillerctl`
to confirm the symlink and its target, then drive that agent to a real status transition and
confirm the resulting `tillerctl notify` call (fired by the agent's own hook, not a manual CLI
invocation) reaches the control socket.

**size:** S.

---

## F-CTRL-NOTIFY-03 — `notification.create` posts a real notification

**Ledger verdict:** FAILED — defective ("orchestrator, live D-Bus capture + code trace,
2026-08-14" — timestamp-less, but the evidence text is the pre-fix diagnosis).

**needs: reclassify**

**Shared cause with two sibling rows already re-verified after the fix — this is the strongest
lead in the whole group.** The ledger's own `F-CTRL-NOTIFY-03`/`F-AUTO-06`/`F-USE-06` rows were
all scoped together as "six ledger entries for one piece" in
`docs/linux-rewrite/tasks/P100-the-notification-that-never-arrives.md`. Commit `a5d09d7`
("fix: wire notification delivery and restored agents", 2026-08-14 13:42:52) fixed both
defects that doc names: (1) `record_notification` now calls `(self.notification_poster)(...)`
(`main.rs:717-744`, the call is at `:737-742`), where `notification_poster` defaults to the
real `post_desktop_notification` → `notify-send` path (`main.rs:693`); (2) both restore paths
(`restore_tabs` and `restore_tabs_in_workspace`, `main.rs:7581` and `:7703`) now call
`register_restored_agent` → `activity.register_agent_id` (`main.rs:7567-7575`, call sites at
`:7608` and the equivalent line in the second function) instead of leaving `pane_agents`
unpopulated after a relaunch.

**Two of the six sibling rows already got fresh post-fix evidence and it confirms the fix
works live**: the ledger's current `F-CORE-ACT-19` and `F-CORE-ACT-20` entries both cite
"live D-Bus Notify" captures from **P120** (after `a5d09d7`) — ACT-19 shows a real `Notify`
call reaching the session bus (it now fails only on title *wording*, agent+status vs.
agent+worktree-label — a content bug, not a missing-delivery bug), and ACT-20 shows the
visible/hidden suppression gate working both directions live. Both use the exact same
`post_desktop_notification`/`notify-send` pipe `record_notification` now also calls. Nobody
went back and re-ran the `dbus-monitor` capture across `notification.create` specifically
(the technique P120 already used for the sibling rows) — `F-CTRL-NOTIFY-03`, `F-AUTO-06`, and
`F-USE-06` still carry pre-fix evidence.

**files:** `rust/crates/tiller/src/main.rs` (`record_notification` :717-744, `notification.create`
handler :1378-1386, `post_desktop_notification` :1604-1613) — no changes anticipated.

**approach:** repeat P120's exact technique — `dbus-monitor --session
"interface='org.freedesktop.Notifications',member='Notify'"` on the headless lane, call
`notification.create` with a title/body, and confirm a `Notify` call lands with that title/
body. Given the identical code path already proved live for `F-CORE-ACT-19/20`, this is very
likely a quick flip to PASSED rather than more building — but it needs the same live capture,
not an inference from a sibling row.

**size:** S (verification only, on current evidence).

---

## F-CTRL-WORK-01 — `worktree.set --comment` persists

**Ledger verdict:** FAILED — defective.

**needs: build**

The ledger's own corrected evidence is accurate on current code — I re-verified all of it.
`ControlWorkspace.comment` (`main.rs:392-404`) really is documented "intentionally not
persisted", and `set_worktree` (`main.rs:552-570`) only ever mutates that in-memory struct.
Tracing further than the ledger did: the gap is structural, not a one-line oversight.

- `ControlWorkspace` is rebuilt from scratch on every `sync_control_state()` call
  (`main.rs:2707-2720`) via `ControlState::from_catalog` (`main.rs:414-443`), which
  **hardcodes `comment: String::new()`** at `main.rs:432` — there is nowhere for a persisted
  value to flow back in even if one existed on disk, because the source struct it reads from,
  `session::CatalogWorktree` (`rust/crates/tiller/src/session.rs:365-368`), has **no `comment`
  field at all** (only `branch`, `path`, `is_primary`).
- The DB column is real and already wired at the `tiller_persistence` layer
  (`WorktreeRecord.comment`, `rust/crates/tiller_persistence/src/model.rs:73`; upsert at
  `db.rs:244`) — but nothing in `rust/crates/tiller/src/session.rs` ever reads or writes it.
  `write_layout` and `write_catalog` (`session.rs:701-728`, `:787-839`) both construct fresh
  `WorktreeRecord`s via `WorktreeRecord::new(...)` without ever setting `.comment`, so it is
  always saved as `None` regardless of what a user set.
- There is a **second, unrelated `comment` field** already sitting dead on
  `tiller_project::Worktree` (`rust/crates/tiller_project/src/worktree.rs:33`, always
  constructed as `None` at `:53` and never read or written anywhere else in the tree — I
  grepped `\.comment\b` across `rust/crates` and it has zero other hits). This is a decoy: it
  is not the type `session.rs`'s catalog actually uses (`session::CatalogWorktree`, not
  `tiller_project::Worktree`), so wiring through *that* struct would not fix this row. Worth
  flagging so nobody "fixes" the wrong `Worktree` type.
- Restoration doesn't even read `WorktreeRecord`s to rebuild the live catalog in the first
  place: `restore_catalog` (`session.rs:841-893`) rebuilds worktrees by **live git discovery**
  (`discover_project`/`catalog_project`, `session.rs:576-608`), not from the `worktree` table.
  So even a correct save wouldn't round-trip without also adding a merge step at load time that
  joins the git-discovered worktree list against the DB's `comment` (and this project already
  has one comparable merge for `is_primary`, worth checking whether that survives restart the
  same way before assuming it's a template to copy — I did not chase that far).
- Separately, `worktree.set` is handled **synchronously** inside the control-socket method
  match (`main.rs:1439-1468`), never going through `queue_action`/`ControlAction` the way every
  state-mutating control method that touches `self.project_catalog` does (`SelectWorktree`,
  `CreateWorkspace`, etc., `main.rs:246-326`). Even if `CatalogWorktree` grew a `comment` field,
  today's `set_worktree` has no path to the GPUI main thread's `self.project_catalog` to update
  it there and trigger `self.session.schedule_catalog(...)` — it only ever touches the
  socket-side `ControlState` snapshot.

**files:**
- `rust/crates/tiller/src/session.rs` (`CatalogWorktree` :365-368, `write_layout` :701-728,
  `write_catalog` :787-839, `catalog_project` :576-608, `restore_catalog` :841-893) — needs a
  new `comment` field and both a save-time write and a load-time merge.
- `rust/crates/tiller/src/main.rs` (`ControlWorkspace` :392-404, `set_worktree` :552-570,
  `ControlState::from_catalog` :414-443, the `"worktree.set"` handler :1439-1468, and likely a
  new `ControlAction` variant + queue/dispatch arm so the socket handler can reach
  `self.project_catalog` on the main thread) — needs the routing from socket to durable state.
- `rust/crates/tiller_persistence/src/model.rs`, `db.rs` — already correct, no change expected.

**approach:** first get a product decision — the evidence already flags this: does
`worktree.set --comment` want to persist at all, or is "runtime annotation, gone on restart"
the intended contract? If persistence is wanted: add `comment: Option<String>` to
`session::CatalogWorktree`; make `set_worktree`'s handler route through a queued
`ControlAction` that updates `self.project_catalog` on the main thread (not just
`ControlState`) and calls `self.session.schedule_catalog(...)`; thread `.comment` through
`write_catalog`'s `WorktreeRecord` construction; and add a git-discovery/DB merge step in
`restore_catalog` (or wherever `is_primary` is merged, if it is) so a discovered worktree
picks up its persisted comment by path.

**size:** L. Touches the control-routing layer, the live catalog model, and the
restore/save round-trip together — genuinely a small subsystem, not a one-file patch.

---

## F-SID-06 — collapsed-project descendant-activity badge

**Ledger verdict:** half-proven (worktree dots shown live; collapsed-project badge not
exercised, code gates dots to worktree rows).

**needs: build**

The row's evidence is already correctly diagnosed as "code gates dots to worktree rows" —
I confirmed this is a hard gate, not a missing UI affordance on top of existing data.
`Sidebar::set_worktree_status` (`rust/crates/tiller_ui/src/sidebar.rs:1166-1183`) filters to
`row.kind == RowKind::Worktree` explicitly; there is no `set_project_status` or any equivalent,
and `RowKind::Project`'s render path (`render_row`, `sidebar.rs:2024` onward) never reads an
`agent_status`-shaped field — `SidebarRow` (`sidebar.rs:96-133`) has exactly one
`agent_status: Option<ActivityStatus>` field, documented "only meaningful for
`RowKind::Worktree`."

The deeper reason nobody built this: **the host does not have the data to aggregate even if
the sidebar API existed.** `TillerWorkspace::sync_activity` (`main.rs:3707-3736`) computes
`worktree_status` (`main.rs:3334-3339`) purely from `self.tabs` — this workspace's own,
single, currently-mounted worktree's tabs. There is exactly one `cx.open_window` call in the
whole app (`main.rs:8174`) — this is a single-window, single-mounted-worktree design where
`self.tabs`/`self.activity` only ever describe `self.working_directory`. A collapsed project's
*other* worktrees (the ones this clause needs a badge for) have no live activity tracked
anywhere in this process while they are not the selected worktree — a Layer-A `tillerctl
notify` call for a background worktree's pane has nowhere to land today, since `self.activity`
belongs to the one mounted workspace.

**files:**
- `rust/crates/tiller_ui/src/sidebar.rs` (`SidebarRow` :96-133, `set_worktree_status`
  :1166-1183, `render_row` :2024 onward) — needs a project-level status field/setter and a
  badge-dot render branch for `RowKind::Project`.
- `rust/crates/tiller/src/main.rs` (`sync_activity` :3707-3736, `worktree_status` :3334-3339) —
  needs either a cross-worktree activity store or, at minimum, a way for Layer-A `notify` calls
  targeting a non-mounted worktree's pane to update *something* the sidebar can read when that
  project is collapsed.
- Possibly `rust/crates/tiller_activity` (`AgentActivityModel` itself) if the fix is to make
  activity tracking span more than the single mounted worktree, rather than bolt on a
  narrower "last known status per worktree id" cache in the app layer.

**approach:** do not treat this as a sidebar rendering task — the rendering half (a badge dot
on a `RowKind::Project` row, keyed by whether any child worktree row currently shows a
non-idle dot) is the easy 20%. The real work is deciding how a background/unmounted worktree's
activity gets tracked at all in a single-window, single-mounted-worktree app, and that answer
should probably be decided once and reused by any other row that needs cross-worktree
awareness, not solved narrowly for this badge alone.

**size:** L. This is architecture, not a widget — flag it as such rather than scheduling it
as a quick UI row.

---

## F-SID-11 — worktree row: branch/folder/primary/comment/status

**Ledger verdict:** half-proven (branch/path/Primary/status shown live; no folder-worktree row
exists, comment not rendered).

**needs: build**

Two separate gaps under one clause, confirmed against current code:

1. **Comment is not rendered — and this is the exact same dead field as `F-CTRL-WORK-01`.**
   `SidebarRow` and `SidebarWorktree` (`rust/crates/tiller_ui/src/sidebar.rs:96-133`,
   `:147-152`) have no `comment` field at all, so `render_row` has nothing to draw even if it
   wanted to. **Shared cause with `F-CTRL-WORK-01`:** both rows trace back to the identical
   missing plumbing — `session::CatalogWorktree` has no `comment` field, `worktree.set` never
   reaches the live catalog, and the DB column that already exists
   (`tiller_persistence::WorktreeRecord.comment`) is never read on restore. **One fix closes
   both rows' primary gap** — the persistence/routing work described in `F-CTRL-WORK-01`'s
   `approach` above (add `comment` to `CatalogWorktree`, route `worktree.set` to the main
   thread, thread it through save/restore) is a precondition for *this* row's display half too;
   this row additionally needs `SidebarRow.comment` threaded from that model into
   `render_row`'s draw.
2. **"Folder worktree" rows do not exist by design, not by omission.** `SidebarProject.is_git`
   is documented: "a non-git project has no worktrees" (`sidebar.rs:139-141`), and
   `context_menu_items`'s `Project` branch (`sidebar.rs:668-693`) only offers "Initialize Git
   repository" for a non-git project — there is no code path anywhere that synthesizes a
   folder-backed worktree row for a non-git project's own directory. This is a real design
   question (does the clause want a synthetic single "worktree" row representing the folder
   itself?), not a bug in existing code — flag it as such rather than assuming it's a small
   miss.

**files:**
- `rust/crates/tiller_ui/src/sidebar.rs` (`SidebarRow` :96-133, `SidebarWorktree` :147-152,
  `render_row` :2024 onward, `context_menu_items` :660-756) — comment field + render, and
  (if built) a folder-worktree row kind/branch.
- `rust/crates/tiller/src/session.rs` and `rust/crates/tiller/src/main.rs` — same files listed
  under `F-CTRL-WORK-01`'s persistence fix; this row's comment half rides on that fix rather
  than duplicating it.

**approach:** land the `F-CTRL-WORK-01` persistence/routing fix first (or as one combined
piece of work — see `sharedCause`), then add `comment: Option<String>` to `SidebarRow`/
`SidebarWorktree` and a rendered line in `render_row`. Get an explicit answer on the
folder-worktree question before building it — it changes `SidebarProject`'s and
`context_menu_items`'s shape, not just a render tweak.

**size:** M for the comment half (once `F-CTRL-WORK-01` lands — trivial on top of it, large if
done from scratch alongside it). The folder-worktree half is its own M-L depending on the
design answer.

---

## F-SID-12 — Set/Unset Primary from the worktree context menu

**Ledger verdict:** half-proven (worktree dots and catalog machinery real; right-click not yet
exercised on this lane).

**needs: exercise**

Confirmed the code is fully wired end to end — this is not a build gap. The context-menu item
exists (`context_menu_items`, `rust/crates/tiller_ui/src/sidebar.rs:694-708`, "Set Primary" /
"Unset Primary" toggling on `is_primary`), and the action reaches the model:
`main.rs:3209-3226` handles `SidebarContextAction::SetPrimary`/`UnsetPrimary` by calling
`set_worktree_primary` (`main.rs:3237-3248`), which flips `ProjectCatalog`'s marker and
schedules a save.

The ledger's blocker (`Scripts/wayland-virtual-pointer.c` hardcodes `BTN_LEFT`, no button
parameter, so a real right-click can't be synthesized on the headless-Wayland lane) is real,
but **there is already a second, working route to the identical code path that needs no new
input-driver work at all**: the command palette. `dispatch_sidebar_palette_action`
(`main.rs:7079-7130`) has a `SidebarPaletteAction::SetPrimary | UnsetPrimary` arm
(`main.rs:7110-7127`) that emits the exact same `SidebarEvent::ContextAction` with
`SidebarContextAction::SetPrimary`/`UnsetPrimary` that the context-menu click would — and the
palette is keyboard-only, which `wayland-drive.sh`'s existing `type`/`key` commands already
support without any right-click. Separately, `P104`'s sidebar sweep already proved right-click
context menus work on this project via a different technique (`xdotool click`, not the
`wayland-virtual-pointer.c` tool) for `F-SID-07`/`F-SID-09` — so a literal right-click gesture
is also available if the fleet wants the clause's named gesture exactly, without new driver
code.

**files:** none — `rust/crates/tiller_ui/src/sidebar.rs:694-708` and
`rust/crates/tiller/src/main.rs:3209-3248,7079-7130` are listed for reference only; no defect
found.

**approach:** drive it via the command palette (open palette on a selected worktree → invoke
"Set Primary" → confirm the primary pill renders → invoke "Unset Primary" → confirm it's
gone) for a pure-keyboard proof, or via `xdotool click` on the worktree row for the clause's
literally-named right-click gesture, following the technique P104 already used successfully
for sibling sidebar rows.

**size:** S.

---

## F-SID-15 — Remove Worktree from the context menu, with confirmation

**Ledger verdict:** FAILED — defective (named entry point absent; only door is hover ×, which
removes with zero confirmation and deletes the on-disk directory).

**needs: build**

Confirmed both halves against current code:

- **No "Remove Worktree" context-menu item exists.** `context_menu_items`'s `Worktree` branch
  (`rust/crates/tiller_ui/src/sidebar.rs:694-754`) offers Set/Unset Primary and the seven
  New-Tab variants only — grepped the whole match arm, there is no `RemoveWorktree` action or
  label anywhere in it.
- **The only door, the hover `×`, has no confirmation.** `remove_worktree_row`
  (`sidebar.rs:1438-1473`) calls `remove_worktree(&repo_root, &worktree_path)` — which deletes
  the on-disk worktree directory — immediately on its single call site
  (`sidebar.rs:2282-2287`, the `×` control's `on_click`), with no prompt of any kind in
  between.

The fix is close to mechanical: this exact codebase already has the right pattern one screen
away. `request_remove_project` (`sidebar.rs:1055-1074`) uses `window.prompt(PromptLevel::
Warning, "Remove project from Tiller?", Some(...), &["Remove from Tiller", "Cancel"], cx)` and
only emits its event if the user picks index 0 — a direct template for "Remove worktree and
delete its directory?" gating `remove_worktree_row`.

**files:**
- `rust/crates/tiller_ui/src/sidebar.rs` (`context_menu_items` :660-756 — add a
  `RemoveWorktree` item to the `Worktree` branch; `remove_worktree_row` :1438-1473 — gate
  behind a `window.prompt` confirmation, following `request_remove_project` :1055-1074 as the
  template; the `×` control's `on_click` :2282-2287; `SidebarContextAction` enum and its
  dispatch, e.g. around :761 and wherever `context_target`/action-handling switches on it).
- `rust/crates/tiller/src/main.rs` (wherever `SidebarContextAction::SetPrimary`/`UnsetPrimary`
  are dispatched, `main.rs:3209-3226` — add the `RemoveWorktree` arm alongside them).

**approach:** add `SidebarContextAction::RemoveWorktree` (menu item + dispatch, mirroring
`RemoveProject`'s shape but for a worktree target); route both the new menu item and the
existing hover-`×` handler through one `window.prompt` confirmation using
`request_remove_project`'s exact call shape, with wording that's honest about deleting the
on-disk directory (project removal's copy explicitly says files are *not* deleted — worktree
removal's should say the opposite, since `remove_worktree` does delete them).

**size:** S-M. Small, well-precedented change; the main cost is threading `window: &mut
Window` into `remove_worktree_row`'s call sites if it doesn't already have one.

---

## F-SID-17 — reorder worktrees by dragging, within a project

**Ledger verdict:** FAILED — defective (real 15-step press-move-release drag between two
worktrees in the same project produced no reorder, immediate or delayed; a follow-up capture
after a hover event ruled out stale repaint).

**needs: build**

I could not find an obvious code-level defect by static reading, and want to say that plainly
rather than guess — the reorder machinery is generic and shared across `RowKind::Project`
(`F-SID-16`, proven live to work with the identical technique) and `RowKind::Worktree`:
`row_drag` (`rust/crates/tiller_ui/src/sidebar.rs:527-538`), `reorder_rows` (`:539-628`), and
the `on_drag`/`on_drag_move`/`on_drop` wiring (`:2166-2185`) all branch on `row.kind` through
the same `ReorderScope`/`RowDrag`/`accepts_drop` contract
(`rust/crates/tiller_ui/src/row_reorder.rs`), with no `Worktree`-specific special case I could
find. I hand-checked `insertion_index`'s arithmetic against the P109 report's exact scenario
(two worktrees, "master" and "feature-test", dragging the second onto the first with
`before=true`) and it computes the correct target index. `reorder_group_for_row` for
`RowKind::Worktree` (`sidebar.rs:511-524`) also looks right — it scopes to the nearest
preceding `RowKind::Project` row, and the P109 drag was confirmed same-project (ruling out my
first hypothesis, a cross-project drag that would legitimately no-op).

Given the logic checks out under hand tracing, the likely failure mode is at the interaction
tier — GPUI hit-testing/drag-threshold behavior specific to nested (`depth: 1`) rows, or event
ordering with the worktree row's additional hover-revealed controls (the `×` remove button
sits in the same row, `sidebar.rs:2268-2290`) — rather than the reorder math itself. That is a
hypothesis, not a finding; I'm flagging it as the first thing to check rather than a diagnosis.

**files:** `rust/crates/tiller_ui/src/sidebar.rs` (`row_drag` :527-538, `reorder_rows`
:539-628, `on_drag`/`on_drag_move`/`on_drop` :2166-2185, the `×` control :2268-2290),
`rust/crates/tiller_ui/src/row_reorder.rs` (`accepts_drop`, `insertion_index` — read, not
obviously guilty).

**approach:** first re-drive with instrumentation — a debug `eprintln!` in `preview_reorder`/
`reorder_rows`'s early-return branches would immediately show whether the drag even reaches
the reorder logic for a worktree row (interaction-tier bug) or reaches it and computes `false`/
an unexpected index (logic bug I didn't find by hand). If it's interaction-tier, compare
against what makes `F-SID-16`'s project-row drag succeed — same handlers, different `row.kind`
and depth, so the delta between the two is the whole search space.

**size:** M. Narrow blast radius (one file, shared machinery already proven half-working), but
genuinely needs a live debug session to localize — not safely schedulable as a blind read of
the diff.

---

## F-SID-18 — "No Terminals" empty state for a selected worktree

**Ledger verdict:** FAILED — absent ("no 'No Terminals' empty state", pass 8).

**needs: reclassify**

**Stale — this was built today, after the recorded evidence.** Commit `7289c84` ("feat: add
no-terminals worktree state", 2026-08-14 17:32:57 — see `docs/linux-rewrite/P110-report.md`
§F-SID-18) added exactly what the clause asks for: a central empty state with the terminal
glyph, the literal text **"No Terminals"** (confirmed present at `main.rs:5438`,
`.child("No Terminals")`), explanatory copy, and a **New Terminal** primary action wired
through the existing shell-owned tab-creation path. It ships with a drawn test,
`drawn_selected_worktree_without_tabs_offers_a_new_terminal`
(`main.rs:11849`), that clicks the real action and asserts a terminal tab replaces the empty
state — this is not just a render-only stub.

The ledger's `pass 8` evidence predates this by a wide margin and should not be trusted as-is.
What P110's own report says is still owed, honestly: **the live pointer-click on New Terminal
was not exercised on the Wayland lane** (no input devices there) — only the drawn-test click
and a static capture (`reference/linux-progress/p110-f-sid-18/02-no-terminals.png`) exist as
proof today.

**files:** `rust/crates/tiller/src/main.rs` (the empty-state render, `:5438` and surrounding;
the drawn test, `:11849`) — listed for reference; no defect found, nothing to build.

**approach:** re-verify live — select a worktree with no open tabs, confirm the "No Terminals"
state renders with its New Terminal action, then click (or `xdotool click`/palette-invoke)
that action and confirm a real terminal tab replaces the empty state. This is very likely a
flip to PASSED or half-proven, not FAILED — absent.

**size:** S (verification only).

---
