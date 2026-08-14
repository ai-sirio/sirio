# T1-core build plan — F-CORE, 23 rows

Read-only triage output. No verdicts changed, no code touched. Each section names what a row
actually needs and which files a fix would touch, per `docs/linux-rewrite/triage/T1-core.md`.
Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

---

## `F-CORE-ACT-02` — half-proven

**Needs: exercise.** The sidebar half (Terminal/pane-1 notify -> glyph change) is already
proven. The remaining half is the D-Bus notification path for a Layer-A transition, and it is
blocked by the same broken exercise gesture as ACT-06/07/11 (see their shared cause below) —
**but it does not need to be.** `tillerctl notify --session <pane-id> --status <status>` only
needs a pane that is already *registered* with `AgentActivityModel`; the driver's route to
registration was OSC-title injection via `wtype` (fragile, unquoted, breaks mid-command). A
spawn-owned pane — launch a real agent through Tiller's own "Add agent" action
(`add_agent_tab`, `rust/crates/tiller/src/main.rs:4621`) — registers with
`self.activity` at spawn time with no title injection required at all. Gesture: spawn a real
agent tab, then `tillerctl notify --session pane-N --status needs-input` targeting that pane,
and watch `dbus-monitor` for the `Notify` call this time reachable without the fragile route.

The manifest also records a companion claim ("pane-0's notify call also marked the Chat tab")
that source does not support: `tab_status`'s `TabContent::Chat` branch
(`rust/crates/tiller/src/main.rs:3260-3270`) computes its own status from
`chat.is_streaming()`/`chat.has_completed_turn()` and never reads `self.activity` at all,
unlike the `TabContent::Terminal` branch a few lines below it. This is architectural, not a
bug in this row's clause (ACT-02 is about `AgentActivityModel::notify()`'s pane-level
recording, not about Chat tabs specifically) — flagging so nobody re-opens it as a defect
against this row. If a future row *does* want `tillerctl notify` to reach a Chat tab's icon,
that branch is the file and the line.

- **files**: none (exercise only; the registration-then-notify route uses code that already
  exists and is already exercised elsewhere)
- **size**: S

---

## `F-CORE-ACT-06`, `F-CORE-ACT-07`, `F-CORE-ACT-11` — NOT EXERCISED (all three)

**Needs: exercise**, all three, **shared cause**.

The full ownership machinery these three rows exercise is present and unit-tested:
`title_owned_panes`/`process_owned_panes` sets, `handle_title_change`, `process_identified`,
`process_gone`, and the pane-close clearing of both sets together, all in
`rust/crates/tiller_activity/src/model.rs:63-64,187-344`. `panes::tests::terminal_events_feed_
title_and_settled_content_into_the_one_model` and
`panes::tests::process_refresh_preserves_process_ownership_until_process_gone` (pass 16, green)
already prove this at the unit level. Nothing here needs a code change.

**Shared cause**: all three rows are blocked on one broken exercise gesture, not on missing
code. The evidence (`02-07-title-set-claude.png`) shows the OSC-title injection command —
`wtype`-typed, unquoted, containing a bare `;` — splitting mid-command rather than reaching
the terminal as one title-setting sequence. No title-owned identity ever gets registered, so
none of these three can be driven past that first step. The fix is a properly shell-quoted
title-injection command in whatever drives the gesture (e.g. `printf '\033]0;%s\007'
'<title>'` passed as a single quoted argument rather than typed keystroke-by-keystroke through
`wtype`), not a change to `rust/crates/tiller_activity` or `rust/crates/tiller/src`.

Once title injection lands cleanly, each row's own gesture (already spelled out in
`02-inventory-packages.md`'s VERIFY column) follows directly:
- ACT-06: identify a pane by title, change only the title to unrelated text, confirm
  title-owned state clears; repeat for a process-identified pane and confirm unrelated title
  text does *not* clear it.
- ACT-07: send a hook status, immediately force a contradictory recognized title, confirm the
  hook status holds; repeat after >1.5s and confirm the title can change it then. (The notify
  half of this race is already reachable — see ACT-02.)
- ACT-11: exercise title, process, and spawn detection on three separate panes, alter/kill one
  signal source at a time, close each pane, confirm only the correct state clears. All three
  ownership classes must be exercised together for this row's claim, not just the
  reachable spawn-owned class alone.

- **files**: none (exercise only)
- **size**: S each

---

## `F-CORE-ACT-10` — builder-claimed, unverified

**Needs: exercise.** The claim is accurate: `rust/crates/tiller/src/panes.rs:22`
(`PROCESS_SIGNAL_INTERVAL = Duration::from_millis(500)`) and `refresh_process_signal`
(`panes.rs:60`) are called from production at `rust/crates/tiller/src/main.rs:2897`, not just
from tests — `process_signal_interval() == 500ms` is itself unit-tested
(`panes.rs:688`). This just has never been independently driven live. Gesture: launch a
native-binary agent (not a node/bun-hosted one — those are invisible to this layer by design,
per ACT-10's own clause), kill its child process directly from outside Tiller (not via the
app's own close-pane path), and confirm the process-owned status clears within roughly one
tick (~500ms-1s), not on the next unrelated activity.

- **files**: none (exercise only)
- **size**: S

---

## `F-CORE-ACT-19` — FAILED — defective

**Needs: build.** Confirmed at `rust/crates/tiller_activity/src/model.rs:448`:

```rust
title: format!("{agent_display_name} — {}", status.human_label()),
```

The clause requires `<agent display name> — <human worktree label>`; the code instead
interpolates the *status* label into the same slot. Body (`worktree_branch` + optional
`project_name`/`comment`) is correct already, several lines above. Fix: `build_payload`
already receives `worktree_branch: &str` as a parameter — use that (or a purpose-built human
worktree label, if one beyond the branch string is wanted) in the title's second half instead
of `status.human_label()`. No caller-side change needed; `worktree_branch` is already passed
in from `rust/crates/tiller/src/main.rs:3775` (`post_activity_notification`).

- **files**: `rust/crates/tiller_activity/src/model.rs` (the only file that needs to change)
- **size**: S

---

## `F-CORE-ACT-20` — half-proven

**Needs: both.** Live D-Bus already proved the `pane_visible` half of the gate in both
directions. What's missing is not just "drive the other two branches" — one of them cannot
currently be driven at all, because it is dead on arrival:

```rust
// rust/crates/tiller/src/main.rs:3750
if !NotificationPolicy::should_notify(transition.old, transition.new, true, visible) {
```

The third argument — `app_active` — is a **hardcoded literal `true`**, not read from any real
window-focus state (grepped `is_active`/`WindowFocus`/`cx.is_window_active` etc. across
`main.rs`: nothing wires focus into this call). `NotificationPolicy::should_notify`
(`rust/crates/tiller_activity/src/notification.rs:19-29`) is otherwise correct pure logic —
`new == Running || old == Some(new)` suppresses, else `!(app_active && pane_visible)` gates —
but with `app_active` permanently `true`, the clause "no notification when the app is active
and the pane is visible" can only ever be tested through the `pane_visible` half; if Tiller is
literally in the background (another app focused) but happens to have that pane's tab
selected internally, a real transition would still be wrongly suppressed. That's a genuine
defect the "half-proven" verdict's own phrase ("both directions of the app_active/pane_visible
gate") slightly overclaims — only `pane_visible` was actually varied, because `app_active`
*can't* be varied yet.

Two separate things to close this row:
1. **Build**: wire real window-activation state into the `app_active` argument at
   `main.rs:3750` (GPUI's window-focus/activation signal — check what's available on the
   `Window`/`App` context already in scope at the call site) instead of the literal `true`.
2. **Exercise** (once wired): the no-agent-running branch (`post_activity_notification`'s own
   early-return when `self.activity.agent_id(...)` is `None`, `main.rs:3739-3741` — separate
   code path from `should_notify`, already present) and the identical-status branch
   (`old == Some(new)` inside `should_notify` itself) — both are cheap to drive live once
   app-active is real, and the identical-status branch is pure logic that could also just be
   unit-tested directly in `notification.rs` (it currently has zero `#[cfg(test)]` coverage
   at all).

- **files**: `rust/crates/tiller/src/main.rs` (wire real `app_active`, call site at
  `main.rs:3750`), `rust/crates/tiller_activity/src/notification.rs` (optional: add unit
  coverage for the no-agent/identical-status branches directly, no logic change needed there)
- **size**: M

---

## `F-CORE-ACT-24` — FAILED — absent

**Needs: build.** Confirmed directly, live-shaped: `AgentAdapter::resume_command` exists on
every adapter (`rust/crates/tiller_agents/src/{claude,codex,omp,opencode,pi}.rs`) but has
**zero callers** anywhere in `rust/crates/tiller/src` — grepped every call site of `.command(`
and `.resume_command(` in `main.rs`; only two hits, both `.command(` (`main.rs:4621` in
`add_agent_tab`, and `main.rs:5084`), fresh-launch always. This matches the row's own live
finding exactly: two real restarts, two fresh random `--session-id`s, `resume_command` never
invoked.

To build this: `add_agent_tab` (or a new restore-specific path) needs to know, at restart
time, which of a worktree's persisted agent tabs have a prior session id it can resume, and
call `adapter.resume_command(...)` instead of `adapter.command(...)` for those. That
knowledge doesn't fully exist yet either — see ACT-25 below, same subsystem, same gap.
`session.ref`/`session_refs` (`main.rs:6883` `load_session_refs`, referenced in
`ADJUDICATION-BACKLOG.md`) already persists *some* session-reference data across restart and
is the most likely existing seam to extend, rather than inventing a new persistence path.

- **files**: `rust/crates/tiller/src/main.rs` (restart/restore path — `add_agent_tab` or its
  restore-time caller — must resolve resumable vs. fresh and call `resume_command`),
  `rust/crates/tiller_agents/src/*.rs` (resume_command already correct per-adapter, no change
  expected unless the call-site needs a different signature)
- **size**: L — this is a real restore-classification feature, not a one-line wiring fix; see
  `sharedCause` below.
- **sharedCause**: `F-CORE-ACT-24` and `F-CORE-ACT-25` are two clauses over one missing
  subsystem — "on restart, know which prior agent sessions/worktrees to resume/prioritize,
  and act on that knowledge." Building the classification once (which content IDs are
  resumable vs. prunable, which worktrees are priority vs. deferred) and threading it through
  both `add_agent_tab`'s command choice (ACT-24) and the worktree mount order at startup
  (ACT-25) is one piece of work, not two.

---

## `F-CORE-ACT-25` — NOT EXERCISED

**Needs: reclassify → build**, on stronger evidence than the manifest currently records.

The manifest's own reasoning ("single post-restart snapshot can't distinguish absent
priority/deferred split from one resolving faster than sampled") is honest about the *live*
capture being ambiguous — but it doesn't need to stay ambiguous, because the code answer is
unambiguous: `BootstrapRestoreOrder::partition` (`rust/crates/tiller_activity/src/
bootstrap.rs:16`) — the function this row's clause names — has **exactly one caller in the
entire workspace**, and it is its own test
(`rust/crates/tiller_activity/tests/activity_domain_integration.rs:179`). Nothing in
`rust/crates/tiller/src` calls it. There is no priority/deferred split for the live snapshot
to have resolved "faster than sampled" — the function that would produce that split is never
invoked at startup at all. This is the same shape DEAD-MODULES.md already established for
`ids_to_evict` (ACT-26, below) and for half a dozen other mechanically-unreachable
`tiller_activity` symbols: `NOT EXERCISED` reads as "code looks right, proof owed," but the
correct read here is `FAILED — absent` — the reachability check is stronger evidence than the
inconclusive live snapshot, and it settles the direction the snapshot couldn't.

Not changing the verdict per this pass's brief — flagging it. The build, when it happens, is
the same subsystem as ACT-24 (see sharedCause there): whatever startup path decides restore
order needs to call `BootstrapRestoreOrder::partition` with the real open/selected worktree
ids, then mount `priority` before `deferred`.

- **files**: `rust/crates/tiller/src/main.rs` (startup/restore path — wherever worktrees get
  remounted on launch needs to call `partition` and honor its ordering),
  `rust/crates/tiller_activity/src/bootstrap.rs` (no change expected; already correct and
  tested in isolation)
- **size**: L (same subsystem as ACT-24 — do not schedule as a separate small item)

---

## `F-CORE-ACT-26` — FAILED — absent

**Needs: build.** Confirmed: `WorktreeMountPolicy::ids_to_evict`
(`rust/crates/tiller_activity/src/mount.rs:9`) has exactly one caller in the workspace, its
own test (`activity_domain_integration.rs:211`). Nothing in `main.rs` calls it, and — this is
the sharper finding — the *setting* that would drive it is equally disconnected:
`AppSettings::mount_cap` (`rust/crates/tiller_project/src/settings.rs:12`, clamped 1-64,
default 8) has **zero references anywhere in `rust/crates/tiller/src/main.rs`**. It's
persisted, clamped, and completely inert — nothing reads it to decide when to evict, and
`ids_to_evict` — the function that would act on that decision — is never called. There is no
partial eviction path to find; both the policy and the trigger are absent together.

- **files**: `rust/crates/tiller/src/main.rs` (needs an eviction call site — likely wherever a
  worktree is opened/mounted, check the current mount_cap against open worktree count and call
  `ids_to_evict`), `rust/crates/tiller_activity/src/mount.rs` (no change expected; correct and
  tested in isolation), `rust/crates/tiller_project/src/settings.rs` (no change expected;
  `mount_cap` field already exists and clamps correctly)
- **size**: M
- **sharedCause**: shares its root with the mount-cap half of `F-CORE-SET-01` below — one
  missing "read `mount_cap`, call `ids_to_evict` when it's exceeded" call site closes both.

---

## `F-CORE-DOM-03` — NOT EXERCISED

**Needs: exercise**, and the instrument, not the code, is the blocker. `Add Project` calls
`cx.prompt_for_paths` (`rust/crates/tiller/src/main.rs:6491`) — GPUI's native file-dialog
wrapper, which on Linux routes through the xdg-desktop-portal `FileChooser`. That's the
correct, real call; there's nothing to build. `docs/linux-rewrite/ENVIRONMENT.md:77` already
documents why the capture came back empty: "the portal file picker is Wayland-side and
invisible to X captures... will not appear in a screenshot even when it is open." The nested
compositor this pass ran under can't render a portal dialog owned by the host desktop
session. Gesture: repeat the click from a lane with a real portal backend visible to the
capture tool (a host-desktop Wayland session, not the nested/headless one), or accept a
human-hand confirmation per `ENVIRONMENT.md`'s own guidance for portal/XDND cases.

- **files**: none (exercise only, environment-limited)
- **size**: S

---

## `F-CORE-DOM-07` — NOT EXERCISED

**Needs: build.** Confirmed: `AutoNamingThrottle`/`should_request`/`record_request`
(`rust/crates/tiller_project/src/domain.rs:92-114`) have zero callers outside `domain.rs`
itself and its own test. Separately, `rust/crates/tiller/src/main.rs:7822-7829`'s
`generated_worktree_branch()` — the `wt-<seconds>` fallback namer P116's evidence actually
observed — is a plain unrelated timestamp-based default branch name for a *new* worktree; it
has nothing to do with throttled LLM-generated naming from a growing chat transcript. The
`auto_naming` setting itself is real (a persisted boolean, `main.rs:1855,8026`,
`SummarizerChoice`-backed) but only gates *whether the feature is enabled* — no call site
anywhere feeds a growing transcript's length into `AutoNamingThrottle::should_request`, asks
the chosen summarizer agent for a name, or renames anything as a result. The whole
generate-and-apply path this row's clause describes doesn't exist yet; only its settings
plumbing (`auto_naming`, `summarizer_agent`) does.

- **files**: `rust/crates/tiller/src/main.rs` (new call site: on chat/terminal transcript
  growth in an auto-naming-enabled worktree, check `AutoNamingThrottle::should_request`, and
  on a hit, invoke the chosen `summarizer_agent` adapter and apply the generated name to the
  worktree/tab), `rust/crates/tiller_project/src/domain.rs` (no change expected; throttle
  logic already correct and tested in isolation)
- **size**: M

---

## `F-CORE-WSP-04` — NOT EXERCISED

**Needs: build.** Confirmed: `LayoutCommand`/`classify_layout_command`
(`rust/crates/tiller_project/src/layout.rs:282,333`) have no consumption anywhere outside
`layout.rs` itself — the only hits in `lib.rs` are `pub use` re-exports, which
`DEAD-MODULES.md`'s own rule 1 ("publication is not consumption") already establishes don't
count. Grepped `rust/crates/tiller/src` and `rust/crates/tiller_ui/src` directly: zero.
`panel.split` (what P116 actually drove) goes through `PaneRegistry::split`
(`main.rs:1131`) — a separate, simpler raw-PTY split mechanism with no relationship to
`LayoutCommand`'s insert/move/close/activate/divider/view-state/rename vocabulary. This
matches `SEAMS.md`'s open "Terminal pane composition" seam almost exactly: Half A
(`tiller_terminal`'s pane-cache/focus-by-content-id machinery) is credited there to `codex11`;
Half B — composing the recursive pane tree through this actual `LayoutCommand` vocabulary in
`main.rs` — is still unowned/unstarted. Whoever picks this up should read that SEAMS.md entry
first; it's the same gap from the pane-composition side, this row is it from the
command-vocabulary side.

- **files**: `rust/crates/tiller/src/main.rs` (the real integration point — replace or wrap
  `PaneRegistry::split`'s raw path with `classify_layout_command`-driven handling for insert,
  move, close, activate, divider-fraction, view-state, rename), `rust/crates/tiller_project/
  src/layout.rs` (no change expected; the command classification is already correct and
  tested in isolation)
- **size**: L — this is the SEAMS.md pane-composition Half B, not a small wiring fix.

---

## `F-CORE-WSP-08` — NOT EXERCISED

**Needs: build.** Confirmed, and worth being blunt about scope: `WorkspaceTabViewState`
(`rust/crates/tiller_project/src/layout.rs:113-123` — editor caret/selection/scroll/folds,
chat draft/attachments/transcript/follows-tail, terminal viewport) has **zero references
anywhere outside `layout.rs`** — it's part of the same unwired `LayoutCommand` subsystem as
WSP-04 above (same file, same dead cluster). But the deeper finding is that the app's *actual*
persisted tab model is a completely separate, parallel type: `SessionTab`/`SessionTabState`
(`rust/crates/tiller/src/session.rs:79,286`) — the one `main.rs` genuinely persists and
restores tabs through (33 references in `main.rs`) — has **none of these fields**. Its
`SessionTabState` carries only `root_id`, `pane_events` (a split/close/resize event log), and
bounded terminal `scrollback`. There is no editor caret/scroll/folds field, no chat
draft/attachments/transcript/follows-tail field, anywhere in the model that's actually wired
to persistence. This is exactly the "parallel-model trap" `DEAD-MODULES.md` already named for
`sidebar.rs:33`'s duplicated `ActivityStatus` — a real, working model exists in one crate,
and a second, unrelated, unused one sits in another, and only the second one has the shape
this row's clause describes.

Building this for real means adding these fields to the model that's actually live
(`SessionTabState`, not `WorkspaceTabViewState`), populating them from the editor/chat/terminal
views when a tab closes or the app quits, and restoring them when a tab reopens — a genuine
per-surface state-capture feature, not a wiring fix. `WorkspaceTabViewState` may still be worth
reading as a reference for the field shape, but adopting it as the live type would mean
threading `tiller_project::layout` into the actual persistence path (`tiller/src/session.rs`),
a larger structural change than adding the fields directly to `SessionTabState`.

- **files**: `rust/crates/tiller/src/session.rs` (`SessionTabState`/`SessionTab` need new
  fields and (de)serialization for them), `rust/crates/tiller/src/main.rs` (capture on
  tab-close/quit from `Editor`/`ChatSession`/terminal views, restore on tab-open — the editor,
  chat, and terminal view types all live in `tiller_ui`/`tiller_terminal` but are driven from
  here), `rust/crates/tiller_ui/src/editor.rs` and `rust/crates/tiller_ui/src/file_view.rs`
  (expose caret/selection/scroll/fold state to capture), `rust/crates/tiller_ui/src/chat.rs`
  (expose draft/attachments/follows-tail state to capture) — `rust/crates/tiller_project/src/
  layout.rs` itself needs no change; it's reference shape, not the file to extend
- **size**: L — a real per-surface state-capture feature across three UI surfaces, not a
  wiring fix.

---

## `F-CORE-FILE-03` — NOT EXERCISED

**Needs: exercise**, and again the instrument is the blocker, not the code. Per
`ENVIRONMENT.md:76-78`: "XDND drags are equally out of reach: `xdotool` has no source window
to negotiate the protocol, so file-drop rows are unexercisable by this harness (a human hand
can still do them — record NOT EXERCISED with the instrument reason, never FAILED)." The data
layer this clause names (`terminal_file_drop`/`classify_file_drop`,
`rust/crates/tiller_project/src/file.rs`) is real and, per `DEAD-MODULES.md`'s correction, is
consumed: `F-TERM-PTY-06`'s own evidence names `tiller_project::terminal_file_drop` directly.
Gesture: a human hand drags a file with spaces/quotes/non-ASCII in its name onto a real
terminal pane and confirms the shell receives one shell-quoted, space-separated path string
with no trailing newline — this cannot be synthesized through the current Wayland virtual
pointer (no press/motion/release primitive), so it needs either a real desktop session or a
different drive tool than `xdotool`/the current virtual pointer.

- **files**: none (exercise only, environment-limited)
- **size**: S

---

## `F-CORE-FILE-04` — FAILED — defective

**Needs: build.** Confirmed exactly as recorded: `resolve_file_link`
(`rust/crates/tiller_project/src/file_link.rs:12`) has zero callers anywhere outside its own
file — only its own tests and the `pub use` re-export in `lib.rs:62`. Grepped `on_click`,
link-related handlers, and `make_link` across `rust/crates/tiller_ui/src/{file_view,
editor}.rs`: the only link-shaped code found is `Editor::make_link`
(`rust/crates/tiller_ui/src/editor.rs:790`), which *writes* a new `[label](url)` Markdown link
into the buffer — nothing that reads an existing rendered link and opens it. No
click/cmd-click/ctrl-click handler on a rendered link span exists anywhere in the crate. The
line/column-stripping and path-resolution logic itself (`resolve_file_link`'s tests) is real
and correct — this is purely a missing UI wire, same shape as the terminal's own link click
convention, which already exists and could be mirrored directly:
`rust/crates/tiller_terminal/src/link_router.rs:4`'s `opens_terminal_link(platform_modifier:
bool)` — "Linux uses GPUI's `platform` modifier for the Super key" per the P82 ruling recorded
in `SEAMS.md`.

Build: add a click handler on rendered Markdown link spans in the file/editor preview surface
that extracts the raw link text, calls `resolve_file_link`, and — on a resolved in-worktree
target — opens it through whatever the app's existing "open a document tab" path is (the same
one the Files panel's right-click "Open" already uses, per `F-EDIT-10`'s route,
`right_panel.rs:369/375/385`).

- **files**: `rust/crates/tiller_ui/src/file_view.rs` (Markdown preview render — needs the
  click handler on link spans; this is also where the existing `MarkdownFormatOp::Link`
  handling lives), `rust/crates/tiller_ui/src/editor.rs` (if the preview delegates rendering
  through here), `rust/crates/tiller_project/src/file_link.rs` (no change expected;
  `resolve_file_link` is already correct and tested), `rust/crates/tiller/src/main.rs`
  (only if opening the resolved target needs a workspace-level dispatch not already reachable
  from within `tiller_ui`)
- **size**: M

---

## `F-CORE-FILE-06` — NOT EXERCISED

**Needs: exercise**, and the manifest's own evidence plus `DEAD-MODULES.md`'s characterization
of this symbol are both now stale against the live tree — worth flagging even though the
row's verdict already matches what I'd give it. `DEAD-MODULES.md` (correction section) frames
`FileSystemEventMonitor::poll` as "watcher built, editor never subscribes" and names
`F-CORE-FILE-06` as its unbuilt owner. That's no longer accurate: reading
`rust/crates/tiller_ui/src/file_view.rs` directly shows a real, wired subscription —
`FileSystemEventMonitor::new` is constructed per-file at `file_view.rs:104`, a background
`cx.spawn` task polls it every 100ms (`file_view.rs:105-118`) and calls
`poll_file_system_events` (`file_view.rs:199`), which drains events into
`handle_file_system_event` -> `check_external` (`file_view.rs:183-194`), which in turn calls
`editor.check_external()` — the same dirty/conflict-detection path the F-EDIT-05 Reload/Keep
banner already covers. This is genuinely built and wired, not a dead module; the manifest's
own "Report overclaim" phrase was about a *different* claim (conflating the git Changes panel
refresh, a separate component, with this one) — not about this code being absent.

Gesture: open a file in the editor, externally modify/delete/rename the underlying file (from
a shell, not through Tiller), wait for the 100ms poll, and confirm the F-EDIT-05 Reload/Keep
banner appears with the correct dirty/conflict/deleted distinction — all without local edits
first, then repeat with local edits present to hit the conflict branch specifically.

- **files**: none (exercise only — code already correct and wired)
- **size**: S

---

## `F-CORE-SET-01` — half-proven

**Needs: both**, split by clause — the four named remaining settings are not one shape.

- **`TILLER_SOCKET_ENABLE` env override — exercise only, and the manifest evidence plus both
  `SEAMS.md` and `DEAD-MODULES.md` are stale here.** Both documents describe
  `with_environment_override` as inert / "queued for codex12." Reading the live tree
  contradicts that: `rust/crates/tiller/src/main.rs:8076-8081`
  (`app_settings_with_environment_override`) calls
  `AppSettings::with_environment_override()` and is itself called at boot,
  `main.rs:8114` (`app_settings_with_environment_override(session_store.load_settings())`).
  There's even a passing unit test exercising exactly this env var,
  `main.rs:10443-10454` (`boot_settings_honor_tiller_socket_enable_environment_override`),
  which sets `TILLER_SOCKET_ENABLE=off`/`on` and asserts `control_socket_enabled` follows.
  This clause is wired and unit-proven; it only needs a live drive (set the env var, launch
  the real app, confirm the control socket really is enabled/disabled) to close, not a build.
- **`summarizerAgent` — exercise only.** `summarizer_agent`/`SummarizerChoice` is a real,
  wired settings field with UI (picker at `rust/crates/tiller_ui/src/settings.rs:2651-2735`)
  and persistence round-trip tests (`main.rs:10057-10176`, `settings.rs:3340,5167`). Just
  needs a live restart-and-reread pass like the five already-confirmed settings.
- **mount cap — build, and it shares its cause with `F-CORE-ACT-26`.** `mount_cap`
  (`rust/crates/tiller_project/src/settings.rs:12`) persists and clamps correctly but has
  **zero references in `rust/crates/tiller/src/main.rs`** — nothing reads it to decide
  anything. See ACT-26 above for the matching missing consumer
  (`WorktreeMountPolicy::ids_to_evict`, also uncalled). One fix — read `mount_cap`, call
  `ids_to_evict` — closes both this clause and ACT-26 entirely.
- **sidebar/right-panel widths — build.** Confirmed: `AppSettings::sidebar_width`/
  `right_panel_width` (`tiller_project/src/settings.rs:15-16`, clamped 160-480 / 220-640) has
  **zero references in `main.rs`**. The actual rendered sidebar width is a hardcoded
  constant, `SIDEBAR_WIDTH = 325.0`, defined independently in *two* places
  (`rust/crates/tiller/src/main.rs:175` and `rust/crates/tiller_ui/src/sidebar.rs:66`) —
  neither reads the settings field. There is no drag-to-resize handle for the sidebar or
  right panel at all: the only resize/drag code in `main.rs` is
  `DraggedPaneDivider`/`update_divider` (`main.rs:5195-5370`), which is for terminal *split*
  dividers, a different UI element entirely. This clause is fully unbuilt: no UI to resize
  by dragging, and the persisted field it would read/write is disconnected even if a fixed
  width were set programmatically.

- **files**: for mount cap — `rust/crates/tiller/src/main.rs` (see ACT-26 for the exact
  shape); for sidebar/right-panel widths — `rust/crates/tiller/src/main.rs` (replace the
  `SIDEBAR_WIDTH` const usage with the settings-backed value, add a drag handle analogous to
  `DraggedPaneDivider`), `rust/crates/tiller_ui/src/sidebar.rs` (same for its own
  `SIDEBAR_WIDTH` const), `rust/crates/tiller_project/src/settings.rs` (no change expected;
  fields already exist and clamp correctly)
- **size**: M (mount cap, shared with ACT-26) + M (sidebar/right-panel widths, a real
  drag-resize UI feature) — do not schedule as one S ticket; two of the four sub-clauses are
  real builds.
- **sharedCause**: the mount-cap sub-clause is the same gap as `F-CORE-ACT-26`.

---

## `F-CORE-TERM-02` — half-proven

**Needs: exercise**, environment-limited, matching the manifest exactly.
`rust/crates/tiller_terminal/src/lib.rs` wires `open_context_menu` only to
`MouseButton::Right` (cited at `lib.rs:1446,1504` per the manifest and reconfirmed by the
row's own evidence) — no keyboard path exists anywhere in the crate, and
`WAYLAND-LANE.md` documents that right-click on this harness needs `DISPLAY=:1`. Every named
per-item effect (copy, paste, copy context, set title, copy pane ID, copy terminal ID, split
right/down, clear, close) already has its own test coverage per pass 12
(`right_click_resolves_this_terminal_and_draws_all_context_actions`,
`terminal_context_app_actions_have_workspace_routes`) — this is a proof-of-live-gesture gap,
not a missing feature. Gesture: run the drive on the `DISPLAY=:1` lane specifically, right-
click a terminal, and confirm each menu item's real effect (clipboard, title, split, clear,
close) one at a time.

- **files**: none (exercise only, environment-limited to the DISPLAY=:1 lane)
- **size**: S

---

## `F-CORE-USG-05` — half-proven

**Needs: both**, split by clause. The load, real 401 -> refresh-fail -> LoggedOut chain, and
credential-shape parsing are all live-proven already. Two remaining pieces:

- **`needs_refresh` 8-day gate — build.** Confirmed:
  `CodexOAuthCredentials::needs_refresh` (`rust/crates/tiller_usage/src/codex.rs:52`, gate
  constant `REFRESH_AFTER = 8 * 24h` at `codex.rs:50`) has zero callers outside its own
  crate's tests. `CodexUsageFetcher::fetch()` (`codex.rs:315-350`) never calls it — it only
  refreshes *reactively*, after a live 401 (`Err(CodexApiFailure::Unauthorized) => {}` at
  `codex.rs:329`, followed by an unconditional `refresh_token` call). There is currently no
  proactive "credentials are old, refresh before even trying" path at all; the 8-day
  threshold is computed nowhere it can affect behavior.
- **merge-save-on-success — exercise only, code already looks correct.**
  `save_credentials`/`save_credentials_to` (`codex.rs:154-158`, doc comment: "Merges refreshed
  tokens into the auth file rather than overwriting it — `codex` itself may store other
  fields Tiller doesn't know about") is called at `codex.rs:339` right after a successful
  refresh, and reads-then-merges into the existing JSON object rather than replacing it
  (`codex.rs:158-172`). This looks correct; it just wasn't driven to a real refresh success
  this pass (only the refresh-*fail* path was). Gesture: force a real successful token
  refresh (a genuinely near-expiry or 401'd but valid credential) and confirm the auth file's
  unrelated fields (anything `codex` itself wrote) survive the save.

- **files**: `rust/crates/tiller_usage/src/codex.rs` — for the build half, add a
  `needs_refresh` check ahead of the first `fetch_usage` attempt in `CodexUsageFetcher::fetch`
  (or wherever a periodic background refresh loop would live, if one is added) so aging
  credentials get refreshed before they're used, not only after they're rejected
- **size**: M
- **sharedCause**: shares the same file and same `CodexUsageFetcher::fetch` pipeline as
  `F-CORE-USG-06` and `F-CORE-USG-07` below — all three land in
  `rust/crates/tiller_usage/src/codex.rs`.

---

## `F-CORE-USG-06` — half-proven

**Needs: build.** Confirmed exactly as recorded, with the precise discard site:
`classify_token_refresh_failure` (`rust/crates/tiller_usage/src/codex.rs:69-79`) correctly
classifies a failed-refresh response body into `Reused`/`Revoked`/`Expired`/`Other`, and is
genuinely called from production at `codex.rs:253` inside `refresh_token`. But its caller in
`CodexUsageFetcher::fetch` throws the result away:

```rust
// codex.rs:336-339
let refreshed = match refresh_token(&credentials) {
    Ok(refreshed) => refreshed,
    Err(_) => return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
};
```

`Err(_)` discards the `TokenRefreshFailure` variant entirely — every one of the four
classifications collapses to the identical `UsageReason::LoggedOut`. It isn't only the
call-site pattern match that loses the information, either:
`UsageReason` (`rust/crates/tiller_usage/src/model.rs:64-74`) has exactly four variants —
`NotInstalled`/`LoggedOut`/`TimedOut`/`Error` — and none of them can represent
reused/revoked/expired even if the caller wanted to. Fix needs both ends: either add
distinct `UsageReason` variants (or a nested field carrying `TokenRefreshFailure`) and match
on the real error in `fetch`, then extend `rust/crates/tiller_ui/src/status_bar.rs:283-286`
(the only renderer of `UsageReason`, currently a 4-arm match) to display the new
distinction.

- **files**: `rust/crates/tiller_usage/src/codex.rs` (line 338's `Err(_)` needs to match on
  the real `TokenRefreshFailure`), `rust/crates/tiller_usage/src/model.rs` (`UsageReason`
  enum needs new variants or a carried field), `rust/crates/tiller_ui/src/status_bar.rs`
  (render the new distinction, currently a plain 4-arm match at `status_bar.rs:283-286`)
- **size**: M
- **sharedCause**: same file/pipeline as USG-05/USG-07 — see USG-05.

---

## `F-CORE-USG-07` — half-proven

**Needs: exercise.** Both remaining states have real code paths already, and both are simpler
to reach than the manifest's framing of "refresh-needed" implies. "Missing-credentials" is
the `load_credentials()` `Err(_)` branch at `codex.rs:319`, which already maps to
`UsageReason::LoggedOut` — gesture: point `$CODEX_HOME` at a directory with no `auth.json` (or
delete it) and confirm the status bar shows the logged-out state. "Refresh-needed" as this
row's VERIFY describes it doesn't require the still-unwired `needs_refresh` 8-day proactive
gate (that's USG-05's build item, not a precondition for this row) — the *reactive* 401 ->
refresh -> retry path already exists and already ran live for USG-05's refresh-*fail* case;
what's missing here is only the refresh-*success* case: real credentials that get a 401,
refresh successfully, and retry into `UsageFetchOutcome::Success`. That's the same live
gesture USG-05's merge-save-on-success needs — worth driving once and crediting both rows.

- **files**: none (exercise only — code paths exist; see USG-05 for the one shared gesture
  that would close both)
- **size**: S
- **sharedCause**: same file/pipeline as USG-05/USG-06, and its outstanding half is literally
  the same live gesture USG-05's merge-save-on-success needs — drive once, close both.

---

## `F-CORE-AUTH-01` — half-proven

**Needs: exercise.** `AgentAccountIdentity::parse_claude_json`
(`rust/crates/tiller_usage/src/account.rs:52`) is genuinely called from production, not just
tests: `rust/crates/tiller_ui/src/settings.rs:548` —
`AgentAccountIdentity::parse_claude_json(&String::from_utf8_lossy(&output.stdout))?` — parses
the output of a real `claude` CLI invocation. This is wired correctly; the manifest's "never
exercised" is accurate only because the live drive deliberately stopped short of completing
the OAuth login (confirmed: real `claude auth login` spawn + genuine `oauth/authorize` PKCE
URL, "flow deliberately aborted pre-completion"). Gesture: carry the real login flow through
to completion (finish the browser OAuth consent) so `claude`'s own CLI writes real account
JSON to stdout, and confirm `settings.rs:548` parses it into a populated identity (email,
organization) rather than stopping at "button not dead."

- **files**: none (exercise only — code already correct and wired to a real call site)
- **size**: S

---

## Cross-cutting notes

**`main.rs` bottleneck.** Of the 23 rows, the following need `main.rs` changes: ACT-20,
ACT-24, ACT-25, ACT-26, DOM-07, WSP-04, WSP-08, SET-01 (both build sub-clauses). That's
**8 of 23** rows needing real `main.rs` changes, all in different regions of the file (activity
notification call site ~3750, restart/restore path ~4600s/startup, sidebar-width consts
~175, layout-command integration, mount-cap check). None of them overlap each other's line
ranges as far as this pass could tell, but they all land in the single file the rest of the
inventory's 29 rows also depend on — sequence with whoever owns other `main.rs`-touching
groups.

**Two seams already named in `SEAMS.md` account for four rows.** WSP-04 is the unbuilt Half B
of "Terminal pane composition." WSP-08's live model gap and ACT-24/25's restore-classification
gap are new seams this pass surfaced that aren't in `SEAMS.md` yet — worth registering there
per that file's own rule ("a brief that cuts a seam must register Half B here, in the same
commit").

**Three "NOT EXERCISED" rows turned out to already have real code with zero non-test
callers** (ACT-25, and — checked but confirmed correctly classified — ACT-26 already carried
the right verdict). ACT-25 is flagged as `reclassify` above: the live-snapshot ambiguity the
manifest recorded is real, but the reachability grep isn't ambiguous, and it points the same
direction ACT-26 and DOM-07 already point (`FAILED — absent`, not `NOT EXERCISED`).

**Two evidence corrections against other docs, not just the manifest.** `SEAMS.md` and
`DEAD-MODULES.md` both describe `TILLER_SOCKET_ENABLE`'s `with_environment_override` as
inert/unwired (SET-01) — it isn't; it's called at boot and unit-tested. `DEAD-MODULES.md`
also describes `FileSystemEventMonitor::poll` as unsubscribed (FILE-06) — it isn't; it's
polled every 100ms from a real background task in `file_view.rs`. Both docs were accurate
when written and the tree has moved under concurrent builders since; worth a note to whoever
maintains those two documents, since a build agent trusting either document's prose over the
current source would spend effort re-building something that already works.

**Biggest single leverage point:** the broken `wtype`-based OSC-title-injection gesture
blocking ACT-06/07/11 (and slowing ACT-02) is not a Tiller code defect at all — it's a
one-line quoting fix in whatever drives that exercise. Fixing the driver's gesture, not any
crate in `rust/crates/`, closes three rows' proof gap at once.
