# Wave E slice E-C-1 — critic verdicts

Critic pass over the builder's claims in `E-C-1-report.md`, against the wave-D root causes
recorded in `E-C-1.md`. Verified independently in a worktree the builder never saw.

## `F-CORE-ACT-25` — verdict: `FAILED — absent`

Independently re-confirmed the wave-D root cause at the current HEAD: `select_worktree`
(`rust/crates/tiller/src/main.rs:4063`) still calls `self.panes.set_external(&old_path,
Vec::new())` synchronously on every switch, and `AppModel` has a single `working_directory:
PathBuf` field, not a mounted-set. `grep -rn "BootstrapRestoreOrder"` outside
`tiller_activity/src/bootstrap.rs` and its own tests returns nothing but a `pub use`
re-export — zero production call sites. Builder made no code change and the report's own
diagnosis matches. No lane gesture (socket or synthetic input) can reach a concept the app
does not implement, so the specified behaviour is genuinely absent from the product, not
merely untestable.

## `F-CORE-ACT-26` — verdict: `FAILED — absent`

Same source read as ACT-25 (`select_worktree` tears down the outgoing worktree every time, one
worktree ever mounted). `grep -rn "WorktreeMountPolicy"` outside `tiller_activity/src/mount.rs`
and its own tests returns only the `pub use` re-export — zero callers. `mounted_worktrees` /
`limit_mounted_worktrees` remain persisted-only settings with no eviction consumer in
`main.rs`. Builder made no code change; root cause confirmed unchanged.

## `F-CORE-DOM-07` — verdict: `FAILED — absent`

`grep -rn "AutoNamingThrottle"` outside `tiller_project/src/domain.rs` and its own test module
returns only the `pub use` re-export in `lib.rs` — zero production callers. `main.rs` carries
`auto_naming`/`summarizer_agent` as persisted settings only; no transcript-driven renaming call
site exists anywhere to gate. Builder made no code change; the throttle has nothing to throttle
because the feature it gates was never built.

## `F-CHG-13` — verdict: `PASSED`

Instrument: `Scripts/wayland-drive.sh` against a fresh `tiller` build at current HEAD (`f05d158`
still the last touch to `changes.rs`), a real git repo (`/tmp/ec1proj`) with two real uncommitted
edits (`note.md`, `setup.md`). Drove the actual user gesture: selected the worktree, clicked the
Files panel's `Diff` affordance next to `note.md` (`click 1687 151`, coordinates read from a same-
resolution reference frame taken immediately prior). Result
(`/tmp/ec1d-shots/03-after-diff-click3.png`): the Changes tab opened with `note.md`'s section
already expanded showing its real diff hunk (`@@ -1 +1,2 @@`, `+edited note`), while `setup.md`
stayed collapsed — exactly the row's claimed behaviour and discriminating against the pre-fix
state (wave-D's own evidence showed both files listed, neither expanded, from the identical
gesture). Code review of `apply_focus`/`pending_focus` in `changes.rs` confirms the mechanism:
`focus_path` now defers when `entries` is still empty and replays once the first snapshot lands.

## `F-CORE-FILE-04` — verdict: `PASSED`

Instrument: `Scripts/wayland-drive.sh`, real git repo (`/tmp/ec1proj`) with `note.md` containing
`[setup](setup.md)`. Drove the actual default-mode gesture: opened `note.md` (double-click in
Files panel), confirmed it rendered in **Preview** (the file view's default mode, `Markdown` /
`Preview` segmented control active) showing the rendered `setup` link, then clicked the rendered
link text itself (`click 472 147`, coordinates read from the immediately-preceding same-resolution
frame). Result (`/tmp/ec1-shots/04-after-link-click.png`): a new `setup.md` tab opened showing
`# setup`, and the sidebar now lists both `note.md` and `setup.md` under the worktree — a new tab
existing where none did before is a state the default/broken build could not reach by accident.
This discriminates cleanly against wave-D's own evidence (identical gesture, no new tab, no
visible change). Also verified in source that this is on the real user path, not a token call
site: `Workspace::add_file_tab` (`main.rs:5130`) calls `Self::subscribe_file_view` for every
`FileView` it constructs, and `file_view.rs`'s Preview render path builds its `link_click` closure
from `Chat::render_markdown_document_with_link_override` — the same closure that emitted
`FileViewEvent::OpenFile` in this live drive. (`WAYLAND-LANE.md`'s P124 note that
`Workspace::add_file_tab` "never subscribes... dead code in production" predates this fix commit
and is now stale — the wiring is real.)
