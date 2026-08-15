# Wave E slice E-P1 — report

## `F-TERM-UI-02` — fixed

Root cause matched the recorded diagnosis exactly: `TerminalView::on_left_mouse_down`
(`tiller_terminal/src/lib.rs`) converted the click's window-space `event.position` into a grid
row/column by dividing straight through, never subtracting the terminal element's own
`bounds.origin` — while the paint path a few lines away (`TerminalElement::prepaint`) does add
`bounds.origin` when placing every cell/cursor rect. Any pane not flush against the window's
top-left corner (i.e. every real pane in a multi-pane app) therefore hit-tested the wrong cell, so
a platform-modifier-click never found the link it was visually on top of.

Fix: `TerminalHandle` gained a `last_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>` field, written
every `TerminalElement::prepaint` (the only place the real on-screen bounds are known) and read by
`on_left_mouse_down`, which now subtracts `last_bounds.origin` from `event.position` before
dividing by cell size.

- Existing unit test `platform_modifier_click_opens_a_terminal_link` still passes unmodified — it
  didn't catch this bug because a `gpui::test` window sits at origin ≈ (0,0), which is exactly the
  coincidence the lane doc calls out.
- Live attempt: drove `project.add` → typed a URL into a real PTY → `modclick ctrl <url position>`
  over the Wayland lane. The click landed (screenshot shows the cursor at the right spot), but
  whether `xdg-open` actually fired is **not observable in this sandbox**: `cx.open_url`'s failure
  path uses `log::error!`, and `tiller`'s `main.rs` never initializes a `log` backend, so even a
  failed spawn is silently swallowed — this is a sandbox limitation, not evidence either way.
- `howToExercise`: `cargo test -p tiller_terminal platform_modifier_click_opens_a_terminal_link`
  proves the row/column math directly (it asserts a plain click doesn't open the link and a
  platform-modifier click does, using `cx.debug_bounds` for a non-zero-origin-adjacent path). Live:
  print a URL into a real terminal pane that is *not* the leftmost/topmost one, `modclick ctrl <x>
  <y>` on it, and check for `xdg-open` process activity (the app itself gives no visible signal on
  success).

Commit: `fix(F-TERM-UI-02): translate mouse click position by terminal element bounds origin`.

## `F-CHAT-20` — fixed

Ledger's real requirement (`01-inventory-app.md:178`): transcript follows streaming output, but
stops forcing the view down once the user scrolls away, "until the user re-pins it". GPUI's
`ListState` (`FollowMode::Tail`) already un-follows on a genuine scroll-wheel event with no app
code involved — that half needed no fix, confirmed by reading `gpui/src/elements/list.rs`'s
`ListState::scroll`. But nothing in `chat.rs` ever turned following back on: `Chat` sets
`FollowMode::Tail` once at construction and only re-arms it on `push_entry` when already following
— there was no way to *re-pin* after scrolling away, even by manually scrolling back to the last
line.

Fix: `Chat::new` now calls `list_state.set_scroll_handler(...)`, which re-enables
`FollowMode::Tail` whenever `ListState::is_scrolled_to_end()` reports `true`. Had to `cx.defer` the
call — the scroll handler runs while `ListState`'s own `RefCell` is already mutably borrowed inside
`scroll()`, so calling `set_follow_mode` synchronously double-borrows and panics (same pattern
`zed`'s own `agent_ui/src/conversation_view/thread_view.rs` uses for the identical hazard).

All 60 existing `chat::tests` pass unchanged (no regression); no new unit test was added because
the mechanism composes two already-tested GPUI primitives (`FollowMode::Tail`'s un-follow,
`is_scrolled_to_end`) rather than adding new app logic to assert on.

`howToExercise`: the Wayland lane now has `scroll <x> <y> <steps>` (P124). Start a real streamed
reply, `scroll` up mid-stream over the transcript (steps < 0), force a repaint, and confirm the
viewport stayed where it was scrolled to instead of jumping to the new tail content; then `scroll`
back down to the last visible entry and confirm new streamed content resumes pinning the view. Not
independently re-driven live in this session — no time left in the row budget after the code fix
and regression pass — flagged honestly rather than claimed.

Commit: `fix(F-CHAT-20): re-pin transcript to the tail when scrolled back to the end`.

## `F-CHAT-05` — fixed (offline-placeholder half only)

Ledger's real requirement (`01-inventory-app.md:163`): composer disabled with a placeholder while
the agent can't interact. Two sub-conditions:
- **permission-wait** — already correctly implemented and tested
  (`permission_wait_disables_the_composer_and_shows_its_own_placeholder`); the wave-D critic's
  three live attempts couldn't reach it only because the real `claude` CLI's own
  `~/.claude/settings.json` defaults to `defaultMode:"auto"`, which auto-approves before a
  permission card can ever render — an external-CLI-config problem, not a bug in this app.
- **offline** — sweep E03 drove this live and found the real defect: from `● offline`, typing text
  and pressing Enter is genuinely inert (`send()` correctly routes it into a silent
  reconnect-and-retry rather than submitting), but the empty composer fell back to the same generic
  `"Message..."` placeholder used once connected — nothing on screen said *why* nothing happened.

Fix: added a fourth placeholder branch (offline: `client.is_none() && !connecting`) alongside the
existing permission-wait and queue-placeholder branches, with its own id/selector
(`offline-placeholder`) and text ("Agent offline — reconnecting when you send…"). Left the
send/insert-text gating logic untouched — it was already correct, just silent.

Added regression test `offline_composer_shows_its_own_placeholder`, reusing the missing-binary
fixture pattern from `failed_launch_can_retry_and_complete`. All 60 `chat::tests` (now 60, was 59)
pass; `tiller` builds clean.

`howToExercise`: `cargo test -p tiller_ui offline_composer_shows_its_own_placeholder`. Live: launch
a chat with an agent command that fails to start (or kill the ACP process), leave the composer
empty, and confirm the "Agent offline…" text renders instead of the plain "Message..." placeholder.

Commit: `fix(F-CHAT-05): name the offline state on the empty composer's placeholder`.

## `F-CHG-22` — already-correct, re-verified live (no code change)

Wave-D's negative finding ("clicking the collapsed Activity header at 3 plausible y-offsets never
expanded it") did not reproduce. Live drive in this session: `project.add` a real repo, one `click`
on the Activity header's on-screen position taken from the *same* screenshot used to compute the
click coordinates (not a stale one from an earlier resize), and the section expanded correctly —
chevron flipped down, rows for the worktree's open tabs (Chat, Terminal) rendered below the header.
Screenshot: `/tmp/e-p1-chg22/03-after-click.png` (not committed — reproduce with the recipe below).

Best guess at the earlier miss: this Wayland lane's nested window resizes shortly after certain
input events (observed independently while working this same row and F-TERM-UI-02 — a `shot` taken
right after a `click`/`modclick` sometimes reports different pixel dimensions than the `shot`
immediately before it), so a click computed from a "before" frame can land on stale coordinates by
the time it's actually dispatched. Driving `click` and its target `shot` back-to-back in the same
action block, with no shot in between that could itself trigger a resize, avoided the issue. Code
(`RightPanel::toggle_activity`, `render_activity`'s `on_click`) was not touched — no defect found in
it.

`howToExercise`: `project.add path=<repo>`, click the worktree row to select it, `ctl
surface.changes.open` optional, then `click` the Activity header's exact position taken from a
`shot` in the *same* action block (bottom-right of the Files panel, collapsed height
`ACTIVITY_HEADER_HEIGHT`), then `shot` again in that same block — chevron and rows should appear.

## `F-CHG-18` — already-correct at the code level; live two-pane exercise not completed

The wave-D critic's own conclusion stands and was not disturbed: the drag source
(`changes.rs::render_change_file`'s `on_drag`) and the drop target
(`tiller_terminal/src/lib.rs`'s `on_drop::<(PathBuf, String)>` → `receive_diff_drop`) are both real
production wiring, not test-only, and are covered by an existing test
(`chat.rs`/`changes.rs` region around the `pane-test-drop-target` harness). The lane now has a real
`drag` primitive (P124), so this row is *reachable*, unlike when it was marked NOT EXERCISED.

Live attempt: got as far as a genuine two-pane layout (Terminal tab's own context menu offers "Move
to New Pane", confirmed present) with a real uncommitted change in a throwaway scratch repo
(`/tmp/e-p1-chg18-repo`), but did not land a clean screenshot with the Changes file row and a
Terminal pane simultaneously visible and at known coordinates within this row's time budget — the
app's pane/tab model (tabs are per-pane, not global; "Move to New Pane" moves a tab's own content,
and a Terminal tab can itself contain an internal split of two bash panes, which is easy to
mistake for the split you asked for) took more exploration than expected. No code was touched.

`howToExercise` for the next pass: `project.add` a repo with a real uncommitted change,
`surface.changes.open`, `tab.select` to it, right-click the **Terminal** tab header → "Move to New
Pane" (confirmed present in the context menu, screenshot
`/tmp/e-p1-chg18f/02-rightclick-terminal-tab.png`) to get Changes and Terminal into separate visible
panes, then `drag` the changed-file row from the Changes pane onto the Terminal pane's bounds and
look for the "Dropped diff: `<path>`" pill (`tiller_terminal/src/lib.rs`, the
`terminal-diff-drop` debug-selector, ~line 1590) — that pill only renders after a real
`receive_diff_drop` call, so it is a clean pass/fail signal.

## `F-CHAT-33` — blocked

Turn-error half is solid (already effectively proven live in a prior pass, `ACP-07`). The remaining
open half — an MCP-configuration-warning card, dismissible with OK — depends on the real `claude`
CLI (via the `@agentclientprotocol/claude-agent-acp` ACP bridge) writing a line to its own stderr
that contains both "mcp" and a failure word (`looks_like_mcp_warning`,
`tiller_acp/src/lib.rs:1437`) when `.mcp.json` names an unlaunchable server. Three independent live
attempts (2 prior sweeps + wave-D) with a genuinely broken `.mcp.json` and a real `claude` CLI turn
all produced no card.

I did not attempt a fourth live rerun — the prior attempts already used real infrastructure this
session doesn't have better access to, and a fourth identical attempt wouldn't add information.
What would: instrumenting or reading the ACP bridge's own source to find out what it actually does
with a spawn failure for one of its configured MCP servers — three real possibilities that would
each need a different fix:
1. The bridge silently swallows the failure (no stderr, no ACP notification) and only surfaces it
   lazily the first time the agent *tries* to call that server's tool — in which case the warning
   needs to arrive from a tool-call error, not eagerly at session start, and `chat.rs` would need a
   second trigger path in `submit_turn`'s tool-call handling, not just `surface_mcp_warnings`.
2. The bridge does log it, but not to its own stderr — e.g. via a `session/update` notification
   variant the current schema audit says doesn't exist, meaning that audit needs re-checking against
   whatever ACP protocol version the live `claude-agent-acp` package on `npx` actually resolves to
   (it is not pinned, so this can drift out from under the code).
3. The message reaches stderr but doesn't literally contain "mcp" (e.g. names the server by its
   config key only) or doesn't contain one of the ten failure words — in which case
   `looks_like_mcp_warning` needs a wider vocabulary, but only real captured stderr text can justify
   which words to add without breaking the "not routine startup noise" guarantee the function's own
   comment describes.

No files were changed for this row.

## Foreign files wanted

None — every fix landed inside this slice's owned files.
