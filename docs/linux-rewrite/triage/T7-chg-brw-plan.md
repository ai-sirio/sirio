# T7-chg-brw build plan — F-CHG, F-BRW, 16 rows

Read-only triage output. No verdicts changed, no code touched. Each section names what a row
actually needs and which files a fix would touch, per `docs/linux-rewrite/triage/T7-chg-brw.md`.
Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

**Architecture note read before the rows below**: in this rewrite "Files" (the tree browser) and
"Activity" live in `rust/crates/tiller_ui/src/right_panel.rs`, a persistent right-hand panel. Git
"Changes" is a **separate, first-class tab** (`ChangesTab`, `rust/crates/tiller_ui/src/changes.rs`,
mounted by `Workspace::add_changes_tab`, `rust/crates/tiller/src/main.rs:4394`) — a deliberate
pivot away from the Swift original's single right-inspector toggle, recorded in
`docs/linux-rewrite/tasks/P21-mount-the-changes-surface.md` and
`docs/linux-rewrite/04-ux-patterns-waku-does-not-cover.md:35` (orca precedent: "a diff is a peer
of a chat, not an inspector"). Several `F-CHG` rows were written against the old single-panel
model and now straddle two different files; that split is called out per-row below.

---

## `F-CHG-01` — FAILED — absent

**Needs: reclassify.** The row's VERIFY clause (`01-inventory-app.md:134`) is "toggle the right
panel, click Files and Changes, and confirm each view replaces the other" — a single-panel
toggle. That toggle genuinely does not exist: `right_panel.rs`'s own toggle only ever shows/hides
Files (confirmed live, shots/215-216 per the manifest evidence). But this is not an oversight —
it is a **documented, still-current architecture decision**: `docs/linux-rewrite/tasks/
P21-mount-the-changes-surface.md` ("What to build" section) states plainly *"Changes is a
first-class tab, not a right-panel section"* and cites `04-ux-patterns-waku-does-not-cover.md:35`
(orca's diff-as-tab precedent) as the reason. `docs/linux-rewrite/tasks/P62-the-changes-panel.md`
independently lists `F-CHG-01` under "Leave these" with the note "(Changes moved to a Diff tab —
architectural, its own piece)". Both documents agree this is intentional, not a gap.

The user-visible capability the row is actually checking for — see the file tree and see git
changes for the same worktree without losing context — is present via two peer surfaces (the
right-panel Files tree, and a `ChangesTab` bound to the same `working_directory` and reachable
from the same tab strip), just not as one panel with an internal toggle. Building a redundant
toggle that duplicates `ChangesTab` inside `right_panel.rs` would be new, maintainable-forever
surface area for a clause the architecture already satisfies a different way.

**Recommend**: the ledger owner re-word or retire this row against the tab-based model rather
than dispatch it as a build. If it is dispatched anyway, treat it as a build (add a Changes mode
to `right_panel.rs`'s toggle, sharing `ChangesTab`'s git-status data rather than re-querying) —
but flag that this contradicts P21/P62's explicit ruling, and confirm the ruling wasn't itself
overturned before doing it.

- **files**: none (reclassify only); if dispatched as build: `rust/crates/tiller_ui/src/right_panel.rs`, `rust/crates/tiller_ui/src/changes.rs`
- **size**: n/a (reclassify) / L if built anyway

---

## `F-CHG-02` — FAILED — absent

**Needs: build.** The row's evidence ("close-workspace → current-workspace = none, then `surface
changes open` still serves the last worktree's data") is accurate, and the mechanism is a real,
already-half-built seam. `RightPanel` (right_panel.rs) already models a no-worktree empty state
correctly: a `worktree_selected: bool` field, a `clear_worktree(cx)` method (`right_panel.rs:193`)
that clears the panel's bound state and flips the flag, and a render branch
(`right_panel.rs:979-1004`, `debug_selector("right-panel-no-worktree")`) that shows "No worktree
selected" / "Select a worktree to inspect its files and changes." when the flag is false. **But
`clear_worktree` has zero callers anywhere in `rust/` outside its own definition** — `main.rs`
never calls it, so the state can never actually go false in production; every `RightPanel` is
constructed via `RightPanel::new`/`with_activity` (always `worktree_selected: true`,
`main.rs:3475`, `:8467`, `:8641`, `:8736`), and switching worktrees just replaces the panel with a
fresh always-selected instance. `Workspace.working_directory: PathBuf` (not `Option`) has no
model-level representation of "no worktree" at all, which is why it silently keeps serving the
last value.

`ChangesTab` (changes.rs) has no equivalent state whatsoever: `ChangesTab::new(repo_root, cx)`
takes a concrete path unconditionally and never re-checks whether that worktree is still current.

**Approach**: (1) find or add the point in `main.rs` where the last worktree closes / none stays
selected (near `close_workspace`, `main.rs:3608`, and `current_workspace()`'s `Option`-returning
logic, `main.rs:507`) and call `self.right_panel.update(cx, |panel, cx| panel.clear_worktree(cx))`
there — this closes the Files/Activity half essentially for free, since the render branch already
exists and is presumably tested. (2) Give `ChangesTab` an equivalent bound-worktree flag and empty
render branch (mirroring `right_panel.rs`'s pattern), and have `main.rs` clear it the same way, or
gate `surface changes open`/`add_changes_tab` on there being a current worktree at all.

- **files**: `rust/crates/tiller/src/main.rs` (`close_workspace`/`current_workspace`, wire the missing call), `rust/crates/tiller_ui/src/right_panel.rs` (verify `clear_worktree` re-entry: nothing currently re-sets `worktree_selected: true` on the *same* instance, so confirm the fix doesn't strand a panel permanently cleared), `rust/crates/tiller_ui/src/changes.rs` (new no-worktree state)
- **size**: M

---

## `F-CHG-03` — FAILED — defective

**Needs: reclassify** (exercise as the fallback). This is about the **Files** tab's Refresh
(`01-inventory-app.md:136`, `FileExplorerView.swift:23`), not `ChangesTab`. `right_panel.rs`
already has a real, drawn loading state and Retry path, not a computed-but-undrawn flag: a
`refresh_started: bool` field gates the render (`right_panel.rs:698-736`) between a
`"files-loading"` div ("Loading files…"), a `"files-error"` div with a `"files-retry"` button on
failure, and the file list otherwise — and there are two **named tests for this exact row**:
`right_panel.rs:1425` ("F-CHG-03: starting must be visible as a drawn loading state, with a
Refresh action visible") and `right_panel.rs:1451` ("F-CHG-03: a failed root refresh renders Retry
and a subsequent click..."). This matches `docs/linux-rewrite/tasks/P62-the-changes-panel.md`'s
brief, which asked exactly for this and describes it as done.

The live-drive evidence only exercised the happy path (click Refresh on a local, already-cached
file listing) and explicitly says the Retry/failure trial "was not attempted." A local directory
walk is fast enough that `refresh_started` can flip back to `false` well inside one paint frame —
the same "stale-frame lag" this very manifest documents elsewhere in this group (F-CHG-11's
evidence: "the on-screen change did not appear in the first capture... a repeat of the session's
known stale-frame lag"). A live drive that never induced the slow/error path and only checked an
inherently-instant success path is not strong evidence the loading state is absent.

**Approach**: re-drive specifically the Retry path (`chmod 000` the repo root, or point Refresh at
an inaccessible path) and capture the `"files-error"`/`"files-retry"` state; for the loading flash,
either capture at a much shorter interval after the click, or synthetically slow the walk (a huge
directory, or a `strace`/ptrace-style artificial delay) to give the "Loading files…" state time to
paint before it resolves.

- **files**: none if the reclassify holds; `rust/crates/tiller_ui/src/right_panel.rs` only if a genuine defect turns up under the Retry re-drive
- **size**: S (exercise)

---

## `F-CHG-05` — FAILED — defective

**Needs: reclassify** (exercise as the fallback). Keyboard handling in `right_panel.rs` is real
and matches the spec, not absent — `docs/linux-rewrite/STALE-FAILED-CENSUS.md:179` independently
flags this exact row "already built" citing `on_file_key` (`right_panel.rs:585`), wired via
`.on_key_down(cx.listener(Self::on_file_key))` at `right_panel.rs:758`. Reading it: `Down`/`Up`
move the selection cursor by one row (correctly clamped at the ends); `Space` toggles
expand/collapse **only on a directory row** (a no-op on a file, matching "expansion... changes
accordingly" for directories); `Enter`/`Return` opens a file or toggles a directory. This is
exactly the row's own spec (`01-inventory-app.md:138`): arrows select, Space/Return act depending
on row type.

The P104 live-drive evidence used `main.rs` — a **file**, not a directory — and only tried `Space`
on it (correctly a no-op per the code) plus `Down`/`Up`; it never tried `Enter`, which is the
gesture the code actually uses to open a file. The anomalies reported (click on `main.rs` not
visibly moving the highlight off `README.md`; `Down` showing "no visible change") are also fully
consistent with the same capture-timing lag flagged in `F-CHG-03`/`F-CHG-11` above — and the one
anomaly that *did* register (`Up` moving the highlight to `main.rs`, the row above `README.md`) is
exactly what `on_file_key`'s `"up"` arm produces from `current = README.md`'s index, which is
positive evidence the handler is live and correct, not defective.

**Approach**: re-drive with the full gesture set the spec calls for — `Down`/`Up` on both a file
and a directory row, `Space` on a directory (confirm expand/collapse), and specifically `Enter` on
a **file** row (not yet tried) to confirm it opens. Capture with a short delay after each key to
rule out paint lag.

- **files**: none if the reclassify holds; `rust/crates/tiller_ui/src/right_panel.rs` (`on_file_key`, `:585-621`) only if Enter-on-a-file turns out not to open it live
- **size**: S (exercise)

---

## `F-CHG-13` — FAILED — defective

**Needs: build.** Confirmed exactly as the manifest evidence states, and this is a known,
previously-misdiagnosed bug (`docs/linux-rewrite/QUEUE.md`'s "self-inflicted" note: an earlier
`head -5` grep on `OpenDiff` wrongly concluded no handler existed; the real defect is narrower and
worse-shaped than "unwired"). Both call sites that handle "Open diff" discard the path they were
given:

```
main.rs:2745   RightPanelActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx),
main.rs:3926   ChangesTabActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx),
```

`add_changes_tab` (`main.rs:4394`) always constructs a fresh, unfiltered `ChangesTab::new(self.
working_directory.clone(), cx)` titled literally `"Changes"` — the full multi-file Staged/
Changed/Untracked list, not a diff scoped to the one file the user clicked. `ChangesTab` has no
single-file constructor or filter mode to call instead (`changes.rs:275`, `ChangesTab::new` takes
only a `repo_root`). This matches the live evidence precisely: clicking "Open diff" adds a second,
identically-labelled `Changes` Activity entry whose content is the same collapsed multi-file list.

**Approach**: give `ChangesTab` (or a new sibling type) a path-scoped mode — either a
`with_focus(repo_root, path, cx)` constructor that opens with only that file's section
pre-expanded and the tab titled with the file's name, or a genuinely separate single-file diff
view. Wire both discarding call sites to pass the real path through instead of dropping it, and
title the resulting tab/activity entry with the filename (not the literal string `"Changes"`) so
it reads as an isolated diff, not a duplicate of the list surface.

- **files**: `rust/crates/tiller/src/main.rs` (`:2745`, `:3926`, `add_changes_tab`/new `add_diff_tab`, `:4394`), `rust/crates/tiller_ui/src/changes.rs` (`ChangesTab::new`, `:275`, needs a scoped-construction path)
- **size**: M

---

## `F-CHG-18` — NOT EXERCISED

**Needs: exercise.** The drag-and-drop machinery is real, wired production code on both ends, not
test fixtures — `docs/linux-rewrite/QUEUE.md`'s cross-check already settled a direct contradiction
between two frozen references here (`changes.rs:993` is a production `on_drag`, not test-only) and
independently confirmed the row holds. Reading it now: `changes.rs`'s changed-file row is a real
drag source carrying a `(PathBuf, String)` payload (repo-relative path + unified diff text,
`changes.rs:94`, wired at `:992-994`), and `tiller_terminal/src/lib.rs`'s `TerminalView::render`
(the production `Render` impl, not `#[cfg(test)]`) is a real drop target for that exact payload
type: `.on_drop::<(PathBuf, String)>(...)` at `lib.rs:1448`, calling `receive_diff_drop` (`:756`),
which emits `TerminalDropEvent::Diff` and draws a `"terminal-diff-drop"` chip (`:1465` region).
`right_panel.rs`'s own `#[cfg(test)]` drag fixture (`:1877-1995`) is a separate, explicitly-labeled
*harness capability proof*, not the production path — its comment says so directly ("what this
crate owns is the proof that the harness can express a payload drag at all").

The manifest's own evidence is consistent with this: a real drag on the live Wayland lane produced
a floating `"Diff"` chip (the source's drag preview) that tracked the gesture, and the block is
explicitly environmental — no synthetic-drag primitive on the Wayland compositor lane, and the one
route that has one (`DISPLAY=:1`, real X11 mouse events) is barred to this slice. That is a driving
capability gap, not a code gap.

**Approach**: prove it the same way the in-process test harness already does (`right_panel.rs
:1940`'s recipe — real `MouseDownEvent`/`MouseMoveEvent`/`MouseUpEvent` past the drag threshold,
dispatched through `simulate_event`) but end-to-end across the two real production types: drag a
`changes.rs` row's payload onto a live `TerminalView` and assert `last_dropped_diff()`
(`lib.rs:739`) is `Some` and the `"terminal-diff-drop"` chip renders — OR, if only a live desktop
drive counts as proof for this row, get access to a lane with real synthetic mouse-drag support
(the `DISPLAY=:1` X11 route this manifest already names, currently out of reach for this slice).

- **files**: none (exercise only)
- **size**: S–M depending on which proof route is available

---

## `F-CHG-20` — FAILED — defective

**Needs: build** (with genuine uncertainty flagged). The empty-state element is real and present —
`right_panel.rs:850-862` draws an `"activity-empty"` div with `"No activity"` at the same
`ACTIVITY_ROW_HEIGHT` (48px) and the same `theme.typography.footnote`/`theme.meta` tokens used
elsewhere in this file (including the no-worktree text discussed in `F-CHG-02`, which the live
evidence never flagged as illegible) — so this is not a missing-element bug, and there is a
passing test (`right_panel.rs:2002`, `activity_section_states_no_activity_when_empty`) asserting
the div's bounds exist. But that test only constructs a panel that starts empty
(`RightPanel::with_activity(dir, Vec::new())`) — **it never exercises a transition from a
populated Activity section down to empty**, which is what the live evidence actually drove
("Running state confirmed working (2 live labelled rows). Nothing-running state: ... illegible
sub-pixel specks"). A container-bounds assertion cannot catch a glyph-level repaint artifact, and
the live evidence explicitly ruled out the check itself being the artifact (edge-crop/contrast/
repaint checks).

The most likely mechanism, unverifiable from static reading alone, is a stale partial-repaint
region left over from the transition: GPUI re-renders `render_activity` each frame from
`self.activity`, but if the row count shrinks (2 real rows → 0) while `activity_expanded` stays
true, whatever invalidation region GPUI computes for the now-shorter `section` div may not fully
clear the pixels the old rows occupied, corrupting the antialiasing under the new "No activity"
text — a class of bug a bounds-only test is structurally blind to.

**Approach**: run two real activity items to completion so the section shrinks from 2 rows to 0
while expanded (not a fresh empty construction), capture immediately and again after a forced full
repaint (resize the window, or collapse/re-expand the section) to see whether the specks are a
one-time stale-paint artifact that a subsequent full invalidation clears — that would confirm the
repaint-region theory and point the fix at forcing a full redraw of the activity `section` div
when `self.activity` transitions to/from empty (e.g. an explicit key change on the container, or
`cx.notify()` plus checking whether the element structure diffing needs a fresh `ElementId`).

- **files**: `rust/crates/tiller_ui/src/right_panel.rs` (`render_activity`, `:780-862`)
- **size**: M (rendering/repaint-class bug; genuinely uncertain without a live repro)

---

## `F-CHG-22` — half-proven

**Needs: exercise.** The data-tier half the manifest cites is real and correctly wired: `main.rs
:1991` maps `AgentStatus::NeedsInput => ActivityStatus::NeedsInput` (the mapping `docs/
linux-rewrite/tasks/P62-the-changes-panel.md` flagged as broken — "NeedsInput absent from the
activity panel" — is fixed; `ActivityStatus` now has the variant and `right_panel.rs` renders a
distinct glyph/color for all five statuses: `activity_status`/`activity_status_glyph`,
`right_panel.rs:1142-1156`, covering `Idle`/`Running`/`NeedsInput`/`Done`/`Error` each with their
own `theme.tab_*` color and a distinct glyph character). Nothing here needs building.

What's owed is purely visual corroboration of the **already-expanded** Activity section, per the
manifest evidence: the three most recent captures all show the unrelated Changes tab instead. The
fix is a drive, not code.

**Approach**: open a worktree, get one agent to Running, one to Done (or Error), and one to
NeedsInput; open the right panel and click the `"activity-header"` row to expand the Activity
section (not the Changes tab — a different surface per the architecture note at the top of this
document); capture the expanded section showing all three status glyphs distinctly colored/shaped
in the same frame.

- **files**: none (exercise only)
- **size**: S

---

## `F-BRW-01` — FAILED — defective

**Needs: build.** The GPUI layout hierarchy around the native child is correct in principle —
`BrowserSurface::render` (`browser.rs:1198`) puts the embedded `NativeWebViewElement` inside a
`div().relative().flex_1()` (`browser.rs:1288-1295`, comment: "the native child is the final
layout region, not a sibling overlapped by GPUI"), sized by the pane after a fixed 48px toolbar —
so the *region* the webview should fill is computed by ordinary GPUI layout. The mis-sizing is in
how that region's bounds get converted for the native embed:
`NativeWebViewElement::prepaint` (`browser.rs:1567-1579`) calls
`native_webview_rect(bounds, window.scale_factor())`, which converts the GPUI layout `bounds`
(logical pixels) to device pixels via `Bounds::to_device_pixels(scale_factor)` before handing them
to `webview.set_bounds(...)`.

The evidence's own numbers point straight at this conversion: native child **729×679** against a
content area of **850×792** — `729/850 = 679/792 ≈ 0.857143`, i.e. **exactly 6/7**, the *inverse*
of the `7.0/6.0` scale factor this same file's own test hardcodes at `browser.rs:1706`
(`let rect = native_webview_rect(bounds, 7.0 / 6.0);`). That is the signature of a logical/device
pixel inversion: multiplying by the reciprocal of the intended scale instead of the scale itself
(or, as plausibly, GTK/X11 widget geometry expecting *logical* pixels here rather than the raw
device pixels `to_device_pixels` produces — unlike Win32, GTK widgets on X11 are typically sized
in already-scaled "application pixels," so feeding an *additionally* device-scaled rect through
`wry`'s `set_bounds` would double-apply the scale and shrink the embed exactly this way).

**Approach**: instrument or log the actual `window.scale_factor()` value in the live session that
produced `orch21-base.png`/`orch21-link.png` and compare it against what `native_webview_rect`
computes; check whether `wry`'s `WebView::set_bounds` on the GTK/X11 backend expects logical or
device pixels (its own docs/source, since this is the *only* native-child-embed code path in the
app — terminal rendering does not embed a native window, so this conversion is uniquely
under-exercised). Fix the conversion direction (or drop `to_device_pixels` entirely if GTK wants
logical pixels) and re-check the reported position offset `(331,114)` against the pane's expected
origin once the size matches — the offset error may resolve as a side effect of the same fix, or
need separate origin-math correction if it doesn't.

- **files**: `rust/crates/tiller_ui/src/browser.rs` (`native_webview_rect`, `:1510`; `NativeWebViewElement::prepaint`, `:1567-1579`)
- **size**: M

---

## `F-BRW-02` — FAILED — defective

**Needs: build.** The manifest's own framing ("the code says it should have") holds up under
reading: `BrowserState`'s history model is correct in isolation — `go_back()`/`go_forward()`
(`browser.rs:523-544`) only mutate `history_index` and return `Some(address)` when `can_go_back()`/
`can_go_forward()` (`history_index > 0` / `history_index + 1 < history.len()`) are true, and
`record_navigation` (`browser.rs:641`) is called from `did_start_navigation` on every real
navigation, including in-page link clicks (`pump_web_events`'s `WebEvent::NavigationRequested`/
`PageLoad(Started, _)` arms both route through it, `browser.rs:1025-1033`). The Back button itself
is only rendered clickable when `can_go_back()` is true — `browser_button`'s `.when(enabled, |this|
this.hover(...).on_click(...))` (`browser.rs:1190-1192`) gates **both** the hover style and the
click handler on the same `enabled` flag, so the manifest's positive control (hover background
rendered) is proof `can_go_back()` was genuinely true and `on_click` was genuinely attached — the
click landing was not the failure.

What stands out reading the call chain is an asymmetry: `on_address_key`'s `"enter"` arm — the one
gesture already proven to work live (F-BRW-03's Return-navigates evidence) — explicitly calls
`cx.notify()` after acting (`browser.rs:997`). `on_back`/`on_forward`/`navigate_history`
(`browser.rs:918-939`) take no `Context<Self>` at all and have **no path to call `cx.notify()`
anywhere** in the chain from click to `load_url`. If GPUI does not implicitly schedule a repaint
on every `Entity::update()` call (it does not, in general — views must call `cx.notify()` to mark
themselves dirty), the underlying state (history index, `webview.load_url()`) could mutate
correctly while the GPUI-painted parts of the surface (the address field) never redraw to reflect
it. That alone would explain the address field staying frozen; whether the *native* WebKit child
also failing to visibly move needs checking too — either `self.webview` was unexpectedly `None` at
that moment (`load_url`, `browser.rs:909`, silently returns `Ok(())` in that case, doing nothing),
or wry's `load_url` on the real webview didn't take effect for another reason.

**Approach**: add `cx: &mut Context<Self>` to `on_back`/`on_forward`/`navigate_history` and an
explicit `cx.notify()` after they run (matching `on_address_key`'s pattern), and confirm live that
the address field updates. If the *native page* still doesn't visibly change after that fix, dig
into whether `self.webview` was populated at click time and whether `wry`'s `load_url` call is
actually taking effect on the GTK/X11 backend at all — that would be a second, independent defect.

- **files**: `rust/crates/tiller_ui/src/browser.rs` (`on_back`/`on_forward`/`navigate_history`, `:918-939`)
- **size**: S–M

---

## `F-BRW-03` — FAILED — defective

**Needs: build.** The `AddressEditor` model (`browser.rs:264-378`) and its click-to-position
wiring are both genuinely well-implemented, not stubs: `select_all()` sets `anchor=0,
caret=text.len()`; `replace_selection` correctly deletes the current selection range before
inserting; `on_address_key` (`browser.rs:963`) has a real `ctrl+a`/`cmd+a` handler calling
`select_all()`. Click-to-position is not a naive parent `on_click` — `AddressTextElement` is a
custom `Element` that shapes the text itself and hit-tests the exact click x-coordinate against
glyph positions (`line.closest_index_for_x(...)`, `browser.rs:1472`, `:1488`) via its own
`window.on_mouse_event` handlers for `MouseDownEvent`/`MouseMoveEvent`/`MouseUpEvent`
(`browser.rs:1465-1503`), calling `begin_address_drag`/`update_address_drag`/`end_address_drag` —
real selection-by-drag support, not a stub.

Given the model is correct in isolation but the live behavior contradicts it exactly ("ctrl+a does
not select", "caret always end-of-text"), the defect is most likely in event delivery or a
state-reset racing the user's input, not in this logic. Two concrete leads worth checking first:
(1) `pump_web_events` (`browser.rs:1021`) calls `self.address_editor.set_text(...)` on every
`WebEvent::PageLoad(Finished, _)` (`browser.rs:1035-1039`), which resets the caret to end-of-text
(`AddressEditor::set_text` → `move_to(text.len(), false)`, `browser.rs:289-292`) — if the
underlying page keeps re-delivering "finished" events (sub-resource/iframe loads are common on
real pages) while the user is mid-edit, every such event would silently snap the caret back to the
end, which would look exactly like "select-all does nothing, typed text always appends." (2)
Whether `ctrl+a` genuinely reaches `on_address_key` at all in the live app — no global keybinding
in `main.rs`/`tiller_ui` currently shadows it for this context (`chat.rs:1102`'s `"ctrl-a"` binding
is scoped to the `"ChatComposer"` key context and shouldn't apply here), but that should be
confirmed live rather than assumed from a static context-scoping read.

**Approach**: reproduce live and narrow which of the two leads is real — log/print each
`WebEvent::PageLoad(Finished, _)` delivery during a manual edit session to see if it's firing
repeatedly and stomping the caret; separately confirm `address_focus` genuinely has window focus
and `on_address_key` is being invoked at all when `ctrl+a` is pressed (e.g. a debug print inside
the handler). Fix is likely either debouncing/guarding the `set_text` reset in `pump_web_events`
(only reset if the address actually changed and the field isn't focused/being edited), or a focus/
dispatch fix if `on_address_key` isn't firing.

- **files**: `rust/crates/tiller_ui/src/browser.rs` (`pump_web_events`, `:1021-1051`; `on_address_key`, `:963-999`)
- **size**: M

---

## `F-BRW-04` — FAILED — defective

**Needs: build.** Both halves of the manifest evidence are confirmed and independently rooted.

**`browser.open` with `url=https://` silently falls back**: `normalize_address("https://")`
(`browser.rs:667`) correctly computes an empty `host` and returns `Err(InvalidAddress(...))` — the
*validation* is right. But `BrowserSurface::new` (`browser.rs:775`) catches that error and
silently substitutes a hardcoded fallback instead of propagating it: `let (state, startup_error) =
match BrowserState::new(initial_url) { Ok(state) => (state, None), Err(error) =>
(BrowserState::new("https://example.com").expect(...), Some(error.to_string())) }`
(`browser.rs:776-782`). The resulting `startup_error` is stored purely for the in-UI error banner
(`self.startup_error`, rendered at `browser.rs:1214` as `"browser-error"`) — it has **no public
accessor**, so `handle_browser_action`'s `"browser.open"` arm (`main.rs:4476-4490`) has no way to
see it and unconditionally returns `Ok(...)` with `url: state.address()` (the silently-substituted
`https://example.com`) and no `error` key.

**`browser.navigate` to an unreachable/invalid host returns `ok:true` with no error**: the control
handler's `"browser.navigate"` arm (`main.rs:4497-4512`) calls `surface.submit_address(address)`
and replies with success as soon as that call returns `Ok`. But `submit_address` (`browser.rs:858`)
only validates and *starts* the navigation (`self.state.submit_address` normalizes the URL string;
`self.load_url` just calls `webview.load_url(address)`, which kicks off an async load) — it cannot
know yet whether the host resolves or the connection succeeds. That outcome arrives later, if at
all, via `WebEvent::PageLoad(Finished)` or a `did_fail_navigation` call reachable only through the
async `pump_web_events` loop. The synchronous control-socket response is sent before that outcome
exists, so `ok:true` is structurally the best the current design can say about reachability.

**Approach**: for `browser.open`, add a `pub fn startup_error(&self) -> Option<&str>` accessor on
`BrowserSurface` and have `handle_browser_action`'s `"browser.open"` arm check it and either fail
the request or include an `"error"` key in the reply instead of reporting the silent fallback as
success. For `browser.navigate`, this needs real async handling: block (with a bounded timeout) on
a channel/oneshot signaled by the next `did_finish_navigation`/`did_fail_navigation` for that
surface before replying, or explicitly document/return a "pending" status and require callers to
follow up with `browser.wait`/`browser.get` (currently both blanket-unsupported on Linux per the
`BROWSER_METHODS` catch-all, `main.rs:4519-4521`) — the latter is a larger, separate feature.

- **files**: `rust/crates/tiller/src/main.rs` (`handle_browser_action`, `:4469-4524`), `rust/crates/tiller_ui/src/browser.rs` (`startup_error` field needs an accessor, `:766`; navigation-outcome signaling for the async half)
- **size**: M (the `browser.open` fallback fix) / L (the `browser.navigate` async-outcome half, if built)

---

## `F-BRW-05` — half-proven

**Needs: build.** Confirmed exactly as the manifest states, and the code says so itself:
`BrowserSurface::set_agent_driving`/`BrowserState::set_agent_driving` (`browser.rs:905`, `:581`)
are both doc-commented `"Updates the F-BRW-05 activity marker"` / `"F-BRW-05 agent-driving
indicator"` — i.e. this setter was built *specifically and only* for this row. The **only**
caller anywhere in `rust/` is `handle_browser_action`'s `"browser.act"` arm (`main.rs:4514-4522`),
which is explicitly a manual stub: its own error message reads *"browser.act is unsupported on
Linux: only the driving flag is implemented"* when the `driving` param is absent. There is no ACP
protocol type or dispatch for a real browser action anywhere — `rust/crates/tiller_acp/src/*.rs`
has zero references to "browser" at all. So the render wiring (pill on/off) is fully proven, but
the thing that's supposed to *trigger* it — an agent actually driving the browser via ACP — has no
implementation to trigger from, matching the row's own "half-proven" verdict precisely.

**Approach**: this needs a real ACP-side browser action: a tool-call/action type in `tiller_acp`
that an agent session can invoke, dispatch from the ACP event loop into the relevant
`BrowserSurface` (calling `set_agent_driving(true)` for the duration of the action and `false`
after), separate from the manual `browser.act` control-socket stub which can stay for direct
testing. This is a new protocol capability spanning the ACP crate, the dispatch in `main.rs`, and
using the browser setter that already exists — genuinely more than a wiring fix.

- **files**: `rust/crates/tiller_acp/src/*.rs` (new browser-action type/dispatch — none exists today), `rust/crates/tiller/src/main.rs` (route the real ACP action to `set_agent_driving`, near `handle_browser_action`, `:4469`), `rust/crates/tiller_ui/src/browser.rs` (setter already exists, `:905`)
- **size**: L

---

## `F-BRW-07` — half-proven

**Needs: exercise.** The persisted-reload half the manifest cites is real and independently
re-confirmed (seeded origin, fresh process, Permissions section shows it on first render via
`load_browser_origin_grants()`/`with_browser_origins`, `main.rs:8161`/`:8234`). The owed half
("trigger access, confirm no new prompt") cannot currently be exercised for a reason outside this
row: it depends on `F-BRW-06` (not in this triage group), which the ledger already grades
**UNREACHABLE** — `BrowserSurface::request_permission` (`browser.rs:588`, `:880`) and the
doorhanger render (`browser.rs:1242`-ish) are complete, but the only caller anywhere in `rust/` is
a unit test; nothing in `main.rs` calls `request_permission` from a real navigation or ACP action.
Without a production trigger for the prompt in the first place, there is no live "no new prompt"
gesture to drive.

**Approach**: no code to write for this row itself. It is blocked on `F-BRW-06`'s seam closing
first (giving `request_permission` a real production caller — presumably on navigating to a new
origin not yet granted, or on an ACP browser action targeting a new origin, which would also feed
`F-BRW-05`'s agent-driving trigger above). Once that lands, re-drive: navigate to the
already-granted seeded origin and confirm no doorhanger appears, matching the row's remaining
clause.

- **files**: none (exercise only, blocked)
- **blockedBy**: `F-BRW-06` (`request_permission` has zero production callers — ledger: UNREACHABLE)
- **size**: S once unblocked

---

## `F-BRW-08` — half-proven

**Needs: exercise.** "Revoke all" is real, wired production code, not a stub — confirmed reading
`settings.rs`: the button (`settings.rs:3038-3056`) is gated `when(!origins.is_empty())`, calls
`revoke_all_browser_origins(cx)` (`settings.rs:1016`), which clears the local `BTreeSet` and
invokes the `on_revoke_all_browser_origins` callback; that callback is wired in `main.rs:8253-8255`
to `session_store.revoke_all_browser_origins()`, the same persistence layer the single-origin
Revoke path (already proven per the manifest) uses. Only the single-origin path has been clicked
live so far.

**Approach**: seed two or more distinct origins into the DB (not one, so the "all" behavior is
distinguishable from the single-origin path already proven), open Settings → Permissions, click
"Revoke all," and confirm both the card flips to "No browser origins have been granted" and a
direct DB re-query shows zero rows.

- **files**: none (exercise only)
- **size**: S

---

## `F-BRW-09` — FAILED — defective

**Needs: build.** Confirmed exactly as the manifest states, with the precise mechanism. Chat
transcript rendering branches by author: `Entry::User(text)` (`chat.rs:3606-3628`) renders via
`Self::render_plain_text` (`:3620`) — no link parsing at all; only `Entry::Assistant { text,
document }` (`chat.rs:3629-3677`) renders via `Self::render_markdown` (`:3677`), which is the only
path that ever populates a `links` list. Both places that *do* handle a parsed link click —
`TranscriptSelectableText`'s mouse-up handler (`chat.rs:777`) and the plain `InteractiveText::
on_click` path used when there's no selection interaction (`chat.rs:3324-3329`) — call
`cx.open_url(target)` **unconditionally**, with no modifier check and no internal-tab routing
anywhere in the file. This is the reverse of the row's spec (`01-inventory-app.md:127`): "click an
HTTP link in chat, confirm an internal browser tab opens, then use the documented modifier gesture
to open the system browser" — today every link, in every message type that has links at all, opens
externally and unconditionally; user messages don't even get link parsing to click on in the first
place.

`docs/linux-rewrite/SEAMS.md`'s "P82 platform ruling" already establishes this project's
convention for a modifier-bypass gesture on Linux: the terminal link router uses GPUI's `platform`
modifier (Super key) as the Linux equivalent of the Swift original's Cmd, with no separate Control
fallback. The same convention should apply here for consistency.

**Approach**: (1) make `Entry::User` render through `render_markdown` (or otherwise run the same
link-extraction pass `render_markdown` uses) so user messages get clickable links too. (2) Change
both click sites so a plain click emits a new event (mirroring the existing `ChatEvent::OpenFile`
pattern, `chat.rs:536`/`:3500`/`:6515`) asking the host to open/focus an internal browser tab at
that URL, and only fall through to `cx.open_url(target)` (system browser) when the platform
modifier is held at click time — the reverse of today's unconditional default.

- **files**: `rust/crates/tiller_ui/src/chat.rs` (`render_entry`/`Entry::User` branch, `:3606-3628`; both click handlers, `:755-785`, `:3300-3330`; new `ChatEvent` variant near `:536`), `rust/crates/tiller/src/main.rs` (handle the new event and route to `add_browser_tab`/existing browser surface, alongside the existing `ChatEvent::OpenFile` handler near `:6515`)
- **size**: M

---

## Cross-row notes

- **F-CHG-01/02 share a root cause worth naming even though only one is a build**: the project's
  pivot from "Changes as a right-panel toggle" to "Changes as a first-class tab" (P21, documented
  and deliberate) left `right_panel.rs` and `changes.rs` as two independently-worktree-bound
  surfaces with no shared "is a worktree currently selected" signal. F-CHG-02's fix (propagate a
  no-worktree state into both) is the concrete piece of that; F-CHG-01 is the row whose own spec
  assumes the pre-pivot single-panel model and should be re-worded rather than built.
- **F-CHG-03/05 share a root cause**: both rows' live-drive evidence was gathered against the
  Files tab's happy/simple path only (an already-cached local refresh; arrow keys and Space on a
  file, never Enter) while the actual code — including named tests written specifically for
  each row — implements the fuller spec correctly. Both are flagged `reclassify` for the same
  reason: incomplete gesture coverage, not absent code. If a build fleet re-drives both and finds
  a genuine defect, it would likely show up in the same file (`right_panel.rs`) and be worth
  fixing together.
- **F-BRW-01 is the single highest-value browser row to fix first**: every other browser
  interaction row (`F-BRW-02` through `F-BRW-09`) is only exercisable once the native child is
  correctly positioned inside its pane — a mis-sized/mis-positioned webview makes click-position
  math for **F-BRW-03** (which already does its own independent hit-testing in GPUI coordinates,
  so it's not directly affected) less trustworthy to verify visually, and makes every other
  browser screenshot-based proof (F-BRW-02, 04, 05, 07, 08) harder to read cleanly. Fix `F-BRW-01`
  before re-driving the others that depend on visual confirmation.
- **`rust/crates/tiller/src/main.rs` is touched by five of these eight `F-BRW`/`F-CHG` build
  rows** (`F-CHG-02`, `F-CHG-13`, `F-BRW-04`, `F-BRW-05`, `F-BRW-09`) — consistent with the
  manifest's own note that this file is the fleet's bottleneck. None of the five edits overlap in
  the same function, but all land in the same file and should be sequenced, not parallelized
  blindly, if dispatched to different builders.

