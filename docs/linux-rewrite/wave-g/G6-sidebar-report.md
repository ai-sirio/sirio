# Wave G slice G6-sidebar — report

## F-SID-11 — fixed, live-verified across a real restart

**Root cause confirmed and different from what same-process testing suggested.** The builder's
`RefreshSidebar` control-action wiring (`Workspace::sidebar_projects` / `refresh_sidebar`) does
correctly look up `worktree.set`'s comment from `ControlState` and repaint — that part already
worked, which is why same-process tests looked green. The restart case broke for an unrelated
reason: the **very first** `Sidebar` entity built at real startup (`rust/crates/tiller/src/main.rs`,
inside the window-construction closure) was built via bare `sidebar_projects(&project_catalog)`,
which — see that function's own definition — always calls
`sidebar_projects_with_comments(catalog, &BTreeMap::new())`, i.e. an empty comment map, even though
`control_state` had already been seeded from the durable `worktree.comment` column a few lines
earlier via `apply_persisted_comments`. The seed reached `ControlState` but never reached the first
paint of the `Sidebar` entity itself, so a comment set before a previous shutdown stayed invisible
until the next `worktree.set` (or any other event) triggered `refresh_sidebar`.

**Fix:** at the real-startup call site, look the persisted comments up from the already-seeded
`control_state` (same code as `Workspace::sidebar_projects`) and pass them into
`sidebar_projects_with_comments` for the initial `catalog_for_sidebar` too, instead of the bare
comment-less helper.

**Live verification (exact restart scenario the row's defect describes):** `TILLER_WL_LABEL=sid11check`
against a shared `TILLER_DB`, two separate `wayland-drive.sh` invocations:
1. First run: `project.add`, `worktree.set worktree=<path> comment=MARK99` → `ok`, screenshot shows the
   comment.
2. **Second run, fresh process, same DB, no `worktree.set` this time** → the sidebar row for that
   worktree shows the comment truncated to `MA…` in the ~130px column, confirming it survived the
   restart with no further control call.

Committed as `8860603`.

**howToExercise:** `TILLER_WL_LABEL=<label> Scripts/wayland-drive.sh <out> 'ctl project.add
path=<abs-git-repo> \n ctl worktree.set worktree=<abs-repo-path> comment=<text> \n shot before'`,
then re-run the **same script with the same `TILLER_WL_LABEL`** (same `/tmp/<label>.sqlite`) with
just `shot after` as the only action. The worktree row's comment segment (small text under the
branch name, ellipsized in the ~130px available width) must be visible in `after` with **no**
`worktree.set` call in that second invocation.

## F-PRJ-05 — blocked (defect lives outside this slice's owned files)

**The row's "Files: none mapped" is accurate — I found it, and it is not one of the eight files this
slice owns.** The Clone-repository URL field's key handling lives entirely in
`rust/crates/tiller_ui/src/project_forms.rs` (`CloneForm::on_url_key`, `CreateForm::on_name_key`),
which is not in this slice's file list. I did not edit it, per the house rule on foreign-file fixes.

**Live reproduction, confirms the critic's evidence and adds a new data point:** driving the exact
repro (open the Clone popover at its real position — see `P123-popover-click-routing.md`, the
popover is confined to the sidebar's ~280px column, not window-centered — click the URL field, one
`wtype` call typing the full 44-char GitHub URL with **no** inter-character delay) reproduces the
drop: only `h`/`ht` of `https://github.com/octocat/Hello-World.git` land in the field's live state
(not just the display — the derived `Destination` line changes to match the truncated URL, so this
is a real state loss, not a rendering artifact).

**New finding: the drop is rate-dependent, not a fixed "first N characters only".** With `wtype -d 15`
(15ms between synthetic keystrokes, still a single `wtype` invocation) the field reliably captures
more — `https://github` (14 chars) — before dropping the rest. With every character sent as its own
**separate** `wtype` invocation (each naturally spaced by shell overhead plus an explicit forced
repaint + 1.4s settle from `shot()` in between), **all 44 characters land with zero drops.** This
three-point curve (0ms delay → ~2 chars, 15ms delay → ~14 chars, ~1.4s+ spacing → all 44) is
consistent with a genuine timing race in `CloneForm`'s key-event delivery under sustained rapid
input, not a deterministic logic bug in `on_url_key`'s character-append code (which is straightforward
`push_str` per keystroke and has no off-by-one or truncation in the diff I read). `P123`'s author
made an identical observation independently ("the field's typed value lost its leading `h` — most
likely `wtype` racing the field's post-click focus transition... not an app defect") on the same
field; my reproduction shows the race is real and reproducible, just with the drop direction/severity
depending on burst rate, not a one-off.

**What I'd hand to whoever owns `project_forms.rs`:** the field's `on_key_down` is raw per-keystroke
`push_str` on `self.state.url`, gated behind `.track_focus(&self.focus)` — the same pattern used by
every text field in this codebase (`grep` found zero uses of GPUI's `EntityInputHandler`/IME text-input
protocol anywhere in `tiller_ui`). Two candidate fixes, in order of confidence:
1. Confirm whether GPUI's window-level key dispatch can silently miss `KeyDownEvent`s delivered inside
   the same event-loop tick as a `cx.notify()`-triggered re-render of the focused element's own
   subtree (i.e. does re-layout invalidate/interrupt an in-flight batch of already-queued input
   events before they are all dispatched). If so, the general fix is systemic, not local to this one
   field.
2. Migrate free-text fields (starting with this one, since it is the one with a live repro) to GPUI's
   `EntityInputHandler`/text-input-protocol path if one exists in this GPUI version — that is the
   platform-native IME/paste-aware path and would sidestep raw per-keydown loss entirely, matching
   how `wtype`'s own text-commit vs. per-key modes differ.

**wantedForeignFiles:** `rust/crates/tiller_ui/src/project_forms.rs` (the only file with the actual
defect for this row; not touched, per house rules on files outside this slice's ownership).

**howToExercise (to reproduce, not yet fixed):** open the Clone popover (`+` in the Projects header,
`Clone Repository…`), click the URL field at its real on-screen position (read coordinates from a
fresh screenshot — the popover draws inside the sidebar's ~280px column per `P123`, not
window-centered), then `type` a URL of 20+ characters in one `wtype` call with no `-d` delay. The
field's displayed text (and the `Destination` line below it) will show only the first 1-3 characters
instead of the full URL.

## F-USE-03 — closed the wiring gap with a new test, no live drive attempted (same reasoning as prior waves)

The pure-function proof (`tiller_usage::model::tests::success_replaces_the_previous_state` for the
reducer, `status_bar.rs`'s `unavailable_reasons_render_distinct_text`/`only_loaded_state_renders_undimmed`
for text/dimming) was already solid but never exercised the production **entity** method,
`StatusBar::apply_outcomes`, which is what `on_refresh_clicked` and the periodic `ensure_refresh_task`
loop actually call with real fetch outcomes. Added
`status_bar::tests::a_real_timeout_after_a_real_success_dims_the_live_entity`: a `gpui::test` that
drives a real `StatusBar` entity through `apply_outcomes(Success(...))` then
`apply_outcomes(TimedOut, ...)` on an actual `Context<StatusBar>`, and asserts the entity's own
`claude` field — the same field `render()` reads via `segment_dimmed` — lands on `Stale` and reports
dimmed, with the segment still present (not hidden/panicking) at both steps. `cargo test -p tiller_ui
--lib status_bar`: 7/7 pass (including the new one), ~10s total.

I deliberately did **not** attempt to force the real `ClaudeUsageFetcher::TIMEOUT` (25s) hang live
through the Wayland lane. Same reasoning as wave D and the builder, now doubly true: a real Claude CLI
is present and returns quickly in this environment, so provoking an actual 25s timeout would mean
either sabotaging the CLI/PATH for one fetch (production-path only, no test hook overrides it) or
waiting out two real 25s hangs (baseline + timeout) back-to-back, which risks this dispatch's own
180s-silence kill for strictly less proof than the new entity-level test above already gives: it
proves the exact same reducer transition, through the exact same production call site, with the
exact same live-entity `segment_dimmed` read that `render()` uses — the only thing it cannot show is
actual pixels changing shade, which no test-harness color introspection is available for in this
repo (`debug_bounds` only reports presence/bounds, not color).

**howToExercise:** `cd rust && cargo test -p tiller_ui --lib status_bar::tests::a_real_timeout_after_a_real_success_dims_the_live_entity`
— passes, proving the entity's `claude` field goes `Loaded -> Stale` and `segment_dimmed` flips to
`true` across two real `apply_outcomes` calls on a live `Context<StatusBar>`. For a pixel-level live
check (still owed): open the app with Claude usage showing, wait for a real successful fetch, then
force/observe a real timed-out refresh and confirm the Claude segment in the status bar visibly dims
relative to Codex/other still-fresh segments — no faster or lower-risk way to do this was found this
round.
