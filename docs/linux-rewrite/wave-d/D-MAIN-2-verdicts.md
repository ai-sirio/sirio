# D-MAIN-2 critic verdicts

Critic pass, independent of the builder. HEAD at verify time: `4560076` (post wave-D
integration). Instruments used: `Scripts/wayland-drive.sh` (labels `f04c3`/`f04c5`/`chgcrit3`/
`chgcrit5`, sockets `/tmp/f04c*.sock` and `/tmp/chgcrit*.sock`), a scratch git repo at
`/tmp/f04-proj` (two committed markdown files, later given real uncommitted edits for the
diff-focus test), a real `codex` foreground-process drive, and source inspection of `main.rs`,
`tiller_ui/src/file_view.rs`, `tiller_ui/src/chat.rs`, `tiller_ui/src/changes.rs`,
`tiller_activity/src/{bootstrap,mount}.rs`, `tiller_project/src/domain.rs`.

## F-CORE-FILE-04 — FAILED — defective

Builder claims fixed; code and a live drive disagree. `FileView`'s default mode is
`MarkdownMode::Preview` (file_view.rs:149), and Preview renders links through
`Chat::render_markdown_document` -> `render_inline`, whose `on_click` (chat.rs:3871, taken
whenever `interaction` is `None`, which file-tab call sites always pass) calls `cx.open_url(target)`
directly — never `FileViewEvent::OpenFile`. The builder's fix (`subscribe_file_view` ->
`add_file_tab`) only reaches `open_markdown_link`, which is exclusively wired to the raw-text
"Code" mode's platform-modifier-held click handler (file_view.rs:1500-1506), a different, non-default
view the row's route never asks for. Live: `wayland-drive.sh` (label `f04c3`/`f04c5`) opened
`note.md` (containing `see [other](other.md)`) in its default Preview tab and clicked squarely on
the rendered "other" link (measured by crop, not eyeballed) — no new tab appeared, sidebar/tab
strip unchanged (`/tmp/f04-shots/02-7-opened.png` before, `03-8-linkclicked.png` after). The
builder's own unit test only calls `open_markdown_link` directly, never simulates a real Preview
click, so it never exercised the path a user actually takes.

## F-CORE-ACT-10 — PASSED

Builder claims already-correct; live drive confirms. `wayland-drive.sh` (label `f04c5`) opened a
plain Terminal tab (no agent), clicked in, typed the real `codex` binary (`/home/enzopalmisano/
.local/bin/codex`, a genuine ELF, not a wrapper script) and Enter, waited 2s (4 poll cycles at the
500ms `PROCESS_SIGNAL_INTERVAL`), then forced a repaint. Before: sidebar `master` row and
`Terminal` tab icon plain, no status dot (`05-d-terminal.png`). After: `master` row shows the
orange `ActivityStatus::Running` dot and the `Terminal` row's icon is highlighted the same color
(`06-e-codex-running.png`, confirmed by side-by-side crop) — a real idle-to-running flip produced
only by a recognized catalog binary becoming a foreground child of the pane's shell, no OSC title
or notify hook involved.

## F-CHG-02 — PASSED

Builder claims blocked (correctly, as of its own commit); overtaken by the wave-D integrator's
follow-up commit `fb86099`, landed after the builder's report and confirmed still present at HEAD
(`changes.rs:1296-1322`, `id("changes-empty")`). Live: `wayland-drive.sh` (label `chgcrit3`)
selected the clean `/tmp/f04-proj` worktree and called `ctl surface.changes.open` (`sections`
all empty in the reply) — the rendered Changes tab shows a centered "No changes" message
(`/tmp/chg-shots/04-c-changes-open.png`), not the previously blank panel body.

## F-CHG-13 — FAILED — defective

Builder claims blocked; the integrator's `fb86099` did land the wiring, but it has a real ordering
bug the integrator's "Verified" note missed. `add_changes_tab` (main.rs:5102) calls
`changes.update(cx, |tab, cx| tab.focus_path(&path, cx))` synchronously, immediately after
`ChangesTab::new(...)` — but `ChangesTab::new` starts with `entries: Vec::new()` and only
populates it via an async `git_task` kicked off inside `refresh` (changes.rs:275-291).
`focus_path`'s match loop (`entries.iter().any(|entry| entry.path == path)`) therefore always runs
against an empty list and no-ops on every real invocation; nothing re-calls it once the task
completes. `changes.rs`'s own test masks this by `pump_until`-ing entries to populate *before*
calling `focus_path` — a step `main.rs` never performs. Live confirms it: `wayland-drive.sh`
(label `chgcrit5`) put real uncommitted edits in both `note.md` and `other.md`, clicked the
Files-panel "Diff" affordance for `note.md` specifically — the Changes tab opened with both files
correctly listed (git status loaded fine by capture time) but *neither* file's diff row expanded
(`/tmp/chg-shots/04-c-diff-clicked.png`, both rows show the same collapsed `>` chevron) — exactly
the symptom the race predicts.

## F-CORE-ACT-25 — FAILED — absent

Re-grepped at HEAD: `BootstrapRestoreOrder` has zero callers outside `bootstrap.rs`/its own
re-export. Confirmed the builder's architectural claim by reading `select_worktree`
(main.rs ~4009): it calls `self.panes.set_external(&old_path, Vec::new())` on every worktree
switch, tearing down the previous worktree's panes synchronously — there is exactly one mounted
worktree at a time, so there is no "concurrently mounted worktrees" set for a restore-order policy
to act on. `limit_mounted_worktrees`/`mounted_worktrees` remain persisted-only settings values
with no consumer. No gesture added to the Wayland lane creates this concept; unreachable because
the feature itself does not exist, not because of a driving-tool gap.

## F-CORE-ACT-26 — FAILED — absent

Same architectural gap as ACT-25, same instrument (source read of `select_worktree` and a fresh
grep for `WorktreeMountPolicy`): zero callers outside `mount.rs`/its own re-export, no eviction
consumer anywhere in `main.rs`.

## F-CORE-DOM-07 — FAILED — absent

Promoted from the prior sweep's `NOT EXERCISED`: re-grepped `AutoNamingThrottle` at HEAD — still
zero callers outside `domain.rs`/tests, and `main.rs` has no "propose a name from transcript
growth" call site anywhere to gate. This is the same class of gap as ACT-25/26 (a pure, tested
policy module wired to nothing), not a lane-input limitation — no new Wayland gesture creates an
auto-naming feature to drive, so there is nothing a more thorough drive could have found.
