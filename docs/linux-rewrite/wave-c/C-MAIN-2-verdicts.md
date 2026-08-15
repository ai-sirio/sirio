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
