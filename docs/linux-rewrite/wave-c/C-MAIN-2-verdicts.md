# C-MAIN-2 critic verdicts

Critic pass over slice C-MAIN-2 (15 rows). **The builder returned nothing usable**: no
`C-MAIN-2-report.md` exists, and `git log -- rust/crates/tiller/src/main.rs` shows no commit
between `f3b6169..HEAD` attributable to this slice (the only four wave-C `main.rs` commits —
`32fffaf`, `0e7672d`, `e17f3f7`, `14e9eee` — belong to F-CHAT/F-AGENT-SESSION/F-PRJ/notify-title
work, none of which touches any of this slice's rows). So this pass graded the current HEAD
(post wave-C-integration, commit `f7ec060`) cold, against the slice brief's prior evidence,
using fresh grep/live-drive instruments of my own rather than trusting any builder claim.

Instruments used: `grep -rn` over `rust/` for caller counts; `Scripts/wayland-drive.sh` (label
prefix `cmain2*`) for live control-socket and click-through drives against
`rust/target/debug/tiller` (built `2026-08-15 14:02`, unchanged by this pass); one genuine
kill+relaunch against the same `/tmp/cmain2wrk.sqlite` for the persistence row. 10 PNG frames
read and inspected directly.

## Rows

### `F-CORE-ACT-25` — verdict: **FAILED — absent** (unchanged)

Re-grepped `BootstrapRestoreOrder` workspace-wide on current HEAD: the only hits are its own
definition/impl in `tiller_activity/src/bootstrap.rs`, the `pub use` re-export in
`tiller_activity/src/lib.rs`, and its own test in
`tiller_activity/tests/activity_domain_integration.rs:179`. `restore_tabs_in_workspace`
(`main.rs:7967`) still does not reference it. No wave-C commit touched `bootstrap.rs` or the
restore path. Prior verdict stands on unchanged code.

### `F-CORE-ACT-26` — verdict: **FAILED — absent** (unchanged)

Re-grepped `ids_to_evict` workspace-wide: only `tiller_activity/src/mount.rs:9` (definition) and
its own test at `activity_domain_integration.rs:211`. Zero callers elsewhere, unchanged since the
prior pass; no wave-C commit touched `mount.rs`.

### `F-CORE-DOM-03` — verdict: **UNREACHABLE** (upgraded from NOT EXERCISED)

Live-drove it today (`cmain2dom3`, click sequence `click 293 50` then `click 178 84` in one
`wayland-drive.sh` invocation, no intervening `shot` to avoid the resize-triggered popover
dismissal that corrupted an earlier attempt): the add-project-menu popover **does** reliably
open on a plain left-click at the sidebar's `+` — screenshot `cmain2-dom-shots/02-after-plus-
click.png` shows "Open Project…", "Clone Repository…", "Create Project…" fully rendered. This
directly contradicts the prior record ("the add-project-menu popover itself is never seen
open") — that was a click-coordinate/procedure artifact in the earlier attempt, not a real
absence; corrected today with a clean repro.

Clicking "Open Project…" does close the menu and invoke `cx.prompt_for_paths` (`sidebar.rs:
1043`), but no portal window, no visible dialog, and no new project appears
(`cmain2-dom3-shots/02-after-open-project.png` — focus lands back on the sidebar Filter field,
nothing else changes). Per this project's own `ENVIRONMENT.md`: "The portal file picker is
Wayland-side and invisible to X captures" and the Wayland lane's own headless `sway` here runs no
`xdg-desktop-portal` service either — so the picker step is structurally invisible to *every*
harness this project has, not a defect in the app. `UNREACHABLE` (not `NOT EXERCISED`, since the
popover half was actively driven and found working; not `FAILED`, since there is no instrument
available anywhere in this project that can observe the picker succeed or fail).

### `F-CORE-DOM-07` — verdict: **NOT EXERCISED** (unchanged)

Re-grepped `AutoNamingThrottle`/`should_request`/`record_request` workspace-wide: definition and
impl in `tiller_project/src/domain.rs`, the `pub use` re-export in `tiller_project/src/lib.rs`,
its own unit test in `domain.rs`, and its own integration test
`tiller_project/tests/p99_naming_throttle.rs`. No caller in `main.rs` or anywhere else. Unchanged
since the prior pass; no wave-C commit touched `domain.rs` or `p99_naming_throttle.rs`. Kept as
`NOT EXERCISED` rather than reclassified to `FAILED — absent`: unlike ACT-25/26 (where a live
restart affirmatively observed zero evictions happening), nobody has driven a real transcript
past the 200-char/30s growth gate through the actual auto-naming UI flow to watch it fail to
fire — the zero-caller grep proves the wiring is missing, not that a live drive was attempted and
came up empty. I did not attempt that live drive this pass either (no obvious socket verb exists
to grow a chat transcript and observe a rename); recording the gap plainly rather than promoting
a static-analysis inference to a `FAILED` finding.

### `F-CORE-FILE-04` — verdict: **FAILED — defective** (unchanged)

Re-grepped `FileViewEvent` workspace-wide: emitted at `file_view.rs:379`
(`cx.emit(FileViewEvent::OpenFile(...))` from `open_markdown_link`), and the only `cx.subscribe`
of it is inside `file_view.rs`'s own `#[gpui::test]` at line 2398 — the module proving its own
event fires, not a real subscriber. `main.rs` has zero references to `FileViewEvent` (grep
confirmed); it does wire the sibling `RightPanelEvent::OpenFile` (line 2812) and
`ChatEvent::OpenFile` (line 2831) to `add_file_tab`, exactly as the prior record described. I
attempted a live click-through today (create `note.md` with a `[setup](setup.md)` link in a
throwaway repo, open it via the Files tree, click the in-content link) but lost state twice to
`wayland-drive.sh`'s own-label kill-on-relaunch behavior and a "Loading files…" race before
completing the sequence in one shot; did not spend further budget chasing the live click given
the grep evidence is unambiguous and unchanged from the recorded finding (an emitted event with a
self-test subscriber and zero real ones is a clean, direct proof of "wired but not connected,"
matching this same ledger's own bar for ACT-25/26). Verdict and reasoning stand on re-confirmed
code.

### `F-CORE-SET-01` — verdict: **half-proven** (unchanged)

No wave-C commit touched `tiller_ui/src/settings.rs` or `tiller_project/src/settings.rs`'s
load/clamp paths (the only settings-related wave-C commits, `cddf094`/`6af7ff4` by the
integrator, added the Translucency toggle and account-identity cache — different fields
entirely). Did not re-drive the 5 already-confirmed malformed-value fields (font sizes, theme,
socket-enable, refresh interval) given they're unchanged and already proven live via restart per
the prior record. Did not attempt the remaining untested fields (mount cap, sidebar widths,
`TILLER_SOCKET_ENABLE` env override, summarizerAgent) this pass: the `TILLER_SOCKET_ENABLE`
check specifically can't be driven through `Scripts/wayland-drive.sh` without extending the
script, since the harness's own readiness gate (`[ -S "$SOCK" ]`) requires the control socket to
be enabled to confirm the app started at all — disabling it to test the override would make the
harness report a false `FAIL: no control socket`. Recording the gap rather than guessing.

### `F-CORE-WSP-04` — verdict: **NOT EXERCISED** (unchanged)

Re-grepped `LayoutCommand`/`classify_layout_command` workspace-wide: definition/impl in
`tiller_project/src/layout.rs`, `pub use` re-export in `lib.rs`, and layout.rs's own tests. Zero
references in `main.rs` or `tiller_ui`. Unchanged since the prior pass; no wave-C commit touched
`layout.rs`. `PaneRegistry::split` (the thing P116 actually drove) remains a distinct code path
from `classify_layout_command` — confirmed by the same grep. Prior record stands.

### `F-CORE-WSP-08` — verdict: **NOT EXERCISED** (unchanged)

Re-grepped `WorkspaceTabViewState` workspace-wide: only `tiller_project/src/layout.rs`
(definition, field, default) and its `pub use` re-export in `lib.rs`. Zero references in
`main.rs`, `session.rs`, or any `tiller_ui` file. Unchanged since the prior pass; no wave-C
commit touched `layout.rs` or `session.rs`. Nobody has exposed a control-socket or UI path to
this type since the prior record; the pane-restore evidence P116 gathered (control panes surviving
quit+relaunch) remains a different subsystem than per-tab caret/scroll/fold/draft state. Prior
record stands.

### `F-CTRL-BROWSER-02` — verdict: **FAILED — defective** (unchanged, re-confirmed live today)

Live-drove it twice today: a session with a project already added (`cmain2a`) and, separately, a
genuinely fresh instance with **zero** prior `project.add` (`cmain2fresh`, socket path never
touched before this drive). On the fresh instance, `ctl workspace.current` returns
`{"ok":false,"error":"no current workspace"}` as expected, but `ctl browser.open
url=https://example.com` immediately after still returns `{"ok":true,"result":{"surface":
"surface:2","title":"","url":"https://example.com"}}` — a real browser tab is created with zero
workspace context. Confirms the prior record's specific claim ("browser.open still succeeds with
zero workspace context... no check exists in source") exactly, on current HEAD, with a fresh
instrument rather than reusing the prior evidence. No wave-C commit touched
`handle_browser_action`/`add_browser_tab`.

### `F-CTRL-BROWSER-03`, `F-CTRL-BROWSER-04`, `F-CTRL-BROWSER-05`, `F-CTRL-BROWSER-06` — verdict: **FAILED — absent** (reclassified from "FAILED — defective"; the recorded evidence for all four is stale)

**The specific defect on record for all four rows is gone**, and I want to be explicit that this
is a correction, not a nitpick: the recorded evidence for every one of these rows describes
`browser.get`/`.screenshot`/`.snapshot`/`.wait`/`.eval`/`.console` each returning
`{"queued":"true"}` with no real work done — a silent no-op that lies about success, sourced from
"main.rs:4638-4643, `let _ = surface.state();`". That line range is `drain_browser_events` today,
an unrelated function; the described no-op arm does not exist on current HEAD.

I drove all six methods live today (`cmain2a`, after a real `browser.open`) and every one came
back an explicit, honest `ok:false`:

```
browser.get        -> "browser.get is unsupported on Linux: browser automation is not implemented"
browser.screenshot  -> "browser.screenshot is unsupported on Linux: browser automation is not implemented"
browser.snapshot    -> "browser.snapshot is unsupported on Linux: browser automation is not implemented"
browser.wait        -> "browser.wait is unsupported on Linux: browser automation is not implemented"
browser.eval        -> "browser.eval is unsupported on Linux: browser automation is not implemented"
browser.console     -> "browser.console is unsupported on Linux: browser automation is not implemented"
browser.act         -> "browser.act is unsupported on Linux: only the driving flag is implemented"
```

Traced why: `git log -p -L4600,4622:rust/crates/tiller/src/main.rs` shows the honest-rejection
code (`browser_request_error` gating every method outside `BROWSER_CAPABILITIES = [open,
navigate, act]`, `queue_action` returning the real `Result` instead of a fire-and-forget
`queued:true`) landed in commit `988d9e9` ("fix: make browser control responses honest"),
**2026-08-14 04:29**. `git merge-base --is-ancestor 988d9e9 f3b6169` confirms that commit is an
*ancestor* of wave C's own start boundary — it predates this entire wave. The `queued:true`
no-op the ledger's evidence describes was already fixed before wave C began; the evidence
recording it as current must have been sourced from a stale checkout or misattributed to the
wrong line range.

**What has not changed: the underlying capability is still not implemented at all**, now honestly
reported as such instead of faked. That is a materially different failure mode from "defective"
(built, wired wrong) — there is no `get`/`screenshot`/`snapshot`/`wait`/`eval`/`console`
implementation to be defective, only a stub that says so in its own error text. Reclassifying to
`FAILED — absent` for all four rows on that basis, with today's live transcript as the new
evidence of record. The clause each row makes (browser.get should return real page text,
browser.screenshot should write a real file, etc.) remains unmet either way — this is a
correction to the *how*, not the overall pass/fail.

### `F-CTRL-CLI-02` — verdict: **PASSED** (upgraded from half-proven)

The prior record already re-confirmed the XDG symlink half live (`ls -la
~/.local/share/TillerRust/bin/tillerctl`) and flagged the real-agent-hook half as unexercised,
noting the driver's "only reachable via ctrl-shift-p" claim was false because the tab-bar's plain
`+` menu offers agents directly. I drove exactly that today, no chord, and it closes the gap
completely:

1. `click 974 50` (tab-bar `+`) then `click 1357 174` ("Claude Code") in one drive, with an
   intermediate `shot` to let the menu open before the second click and two trailing `shot`s to
   let the frame catch up (the first capture after an action can show the stale pre-action frame
   — confirmed here: capture 1 still showed the menu open with a blank terminal underneath,
   capture 2 caught the real result).
2. Capture 2 (`cmain2-cli3-shots/04-after-click-2.png`) shows a genuine new "Claude Code" tab, and
   its content is not a stub — it's the **real Claude Code CLI's own first-run trust prompt**:
   "Accessing workspace: `/tmp/.../cmain2-repo` — Quick safety check: Is this a project you
   created or one you trust?... 1. Yes, I trust this folder / 2. No, exit — Enter to confirm ·
   Esc to cancel." That text does not exist anywhere in this codebase (grepped to confirm) — it
   can only be the actual `claude` binary talking, spawned by `AgentAdapter::command`/`prepare`
   through the plain-click path.
3. On disk, the freshly-created worktree's `.claude/settings.local.json` (never touched by me,
   written by the app's own `prepare` step during that same click) contains four real hook
   entries, e.g. `"command": "'/home/enzopalmisano/.local/share/TillerRust/bin/tillerctl' notify
   --session pane-2 --status needs-input --stdin-json"` — the installed XDG binary from step 1,
   wired into the spawned session's own hook config, exactly as the contract requires.

Both halves proven live in one pass, through the exact plain-click path the prior critic named:
the installed CLI (re-confirmed) and the real-agent-hook wiring (now proven, not just argued).

### `F-CTRL-WORK-01` — verdict: **FAILED — defective** (unchanged, upgraded to a direct restart proof)

The prior record already read the source correctly (`main.rs:400-401`, `comment: String` field
doc-commented "Runtime annotation from `worktree.set`; intentionally not persisted", re-confirmed
unchanged on current HEAD — no wave-C commit touched this struct or `set_worktree`). I upgraded
this to the house rule's preferred instrument — a genuine kill+relaunch against the same DB,
rather than trusting the source comment alone:

1. Drive 1 (label `cmain2wrk`, fresh DB): `project.add`, then `worktree.set
   worktree=<path> comment=RESTART_PROOF_MARKER_9182` — echoed back immediately as
   `"comment":"RESTART_PROOF_MARKER_9182"`. The script's own cleanup trap then fully killed the
   `tiller` process (no `TILLER_WL_KEEP`).
2. Drive 2 (same label `cmain2wrk`, same `/tmp/cmain2wrk.sqlite`): a fresh `tiller` process starts
   against the identical DB file (`wayland-drive.sh` derives `DB=/tmp/$LABEL.sqlite`, so re-using
   a label is a real restart against the same on-disk state, not a new instance). `ctl worktree.set
   worktree=p-90a9905bd933cf90-wt-0 session=probe-readback` (passing only `session`, never
   touching `comment`, so the response echoes whatever `comment` currently holds) comes back
   `"comment":""`.

The marker is gone after a real restart against the same database — direct, live confirmation
that the comment is lost by design, not merely "the source says so."
