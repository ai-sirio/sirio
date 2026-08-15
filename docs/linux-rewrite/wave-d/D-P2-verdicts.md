# D-P2 critic verdicts

Critic pass, independent of the builder. HEAD at verify time: `4560076` (post wave-D integration,
clean tree). Instruments: `cargo test -p tiller_usage` (per-crate, `--test-threads=1`),
`Scripts/wayland-drive.sh` (labels `dp2critic2`/`dp2v`/`dp2w`/`dp2x`/`dp2y`/`dp3c`/`dp3d`/`dp3e`/
`dp4a`/`dp4b`, sockets `/tmp/dp2*`/`dp3*`/`dp4*`), and source inspection of
`tiller_usage/src/claude.rs`, `tiller_terminal/src/lib.rs`, `tiller/src/main.rs`. All frames are
fresh captures from this pass; none cited from `reference/linux-progress/sweep-D*/`.

## F-SET-11 — PASSED

Reran `cargo test -p tiller_usage --test usage_tests -- --test-threads=1`: 10/10 pass, including
the three new tests. Read the test bodies and `fetch_with_env` in `claude.rs`: each spawns a real
`Pty` running `bash --noprofile --norc -c claude` (or zsh `-f -c`) against an isolated `PATH` with
no dotfile re-sourcing — a genuine subprocess, not the pure `transcript_outcome` stand-in used
elsewhere in the same file. Three distinct real-world setups (empty PATH dir; fake `claude` script
printing a login prompt; fake script printing an error) each drive a distinct, correct
`UsageFetchOutcome::Unavailable(reason)` through actual shell "command not found" text and
`classify_failure`. Discriminating: three different process-tree setups produce three different
real outcomes, matching production classification logic exactly.

## F-SET-20 — PASSED

Static: `grep -n translucency rust/crates/tiller/src/main.rs` shows `settings_report_pairs` now
inserts a `"translucency"` entry (lines 2223-2227) — the integrator already landed the builder's
exact patch pre-verify. Live, fresh drive (`dp2critic2`): `ctl surface.settings.select
section=appearance` before any click returns `"translucency":"false"` (both top-level and in
`values`); a single real `click` on the Translucency toggle's on-screen position, followed by the
same `ctl surface.settings.select` call, returns `"translucency":"true"` in both places. Toggle
round-trip captured over the control socket on a running app — a genuine before/after state flip,
not a static grep alone.

## F-USE-03 — half-proven (unchanged)

Reran `cargo test -p tiller_usage --lib`: 40/40 pass including `success_replaces_the_previous_state`
(the `TimedOut` -> `Stale(last_usage)` reducer transition) and the bounded-timeout PTY test. Both
files this slice owns (`claude.rs`, `model.rs`) are confirmed correct and unchanged since the prior
sweep. The live transition still requires `tiller_ui/src/status_bar.rs` re-polling and timing out
after a prior success, which is foreign to this slice's owned files and was not attempted live this
pass (a real 25s-timeout live drive was out of budget). No regression, no new proof of the live
half — stays half-proven on the same footing as the last sweep.

## F-EDIT-12 — PASSED

The lane gained `drag` since the last critic pass (P124), closing exactly the "no drag primitive"
blocker the prior evidence cited. Live, fresh drive (`dp3e`): added a throwaway one-line diff to
`README.md`, opened Changes, right-clicked its tab and used **Move to New Pane** to place Changes
and a real running Terminal side by side (a production feature, not a test-only affordance), waited
for the terminal to boot a shell prompt, then `drag`ged the `README.md` change row from the Changes
list onto the terminal pane. The terminal displayed a real toast, **"Dropped diff: README.md"** —
this is production `TerminalView::receive_diff_drop` firing from a genuine
`.on_drop::<(PathBuf, String)>` handler at `tiller_terminal/src/lib.rs:1551`, not the test-only
`DiffDropTargetFixture` the unit test exercises. Discriminating: the toast only appears on a
successful typed-payload drop; no such text exists in any idle frame. Throwaway `README.md` change
reverted via `git checkout -- README.md` immediately after capture; working tree is clean.

## F-CORE-TERM-02 — PASSED

The lane gained `chord <mod> <key>` since the last critic pass. Live, fresh drive (`dp2y`): clicked
a real running terminal pane to focus it, then `chord shift F10`. The full 12-item context menu
(Copy, Paste, Copy Context, Set Title, Copy Pane ID, Copy Terminal ID, Split Left/Right/Above/Down,
Clear Terminal, Close Terminal) appeared at the cursor position — the identical menu a `rightclick`
on the same pane produces (verified in the same pass, `dp2x`), and the exact gesture
`shift_f10_opens_the_context_menu_from_the_keyboard` exercises in the unit harness, now driven
through real synthetic keyboard input end-to-end. Both halves closed: state (menu renders) and
gesture (real chord dispatch).

## F-TERM-PTY-06 — UNREACHABLE (unchanged)

Re-checked the current `wayland-drive.sh` vocabulary (added `rightclick`/`down`/`up`/`drag`/
`scroll`/`chord`/`modclick` this wave, per `WAYLAND-LANE.md` P124): none of the new primitives
produce a real XDND external-file drag — `drag` composes the same virtual-pointer `down`/`move`/`up`
sequence used for in-app GPUI drags (proved live for F-EDIT-12 above), which yields the app's own
`on_drop::<PathBuf>` typed payload, not `gpui::ExternalPaths` (a real desktop file-manager drop).
`ENVIRONMENT.md` still records XDND as out of reach of any harness on any lane. `on_drop::<
ExternalPaths>` remains wired at `tiller_terminal/src/lib.rs:1567` (confirmed present, unchanged).
Genuinely unreachable, not merely unattempted.

## F-TERM-UI-02 — FAILED — defective (was NOT EXERCISED / builder claimed already-correct)

The lane gained `modclick <mod> <x> <y>` since the last critic pass, so this row is now reachable —
overturning the prior "no modifier parameter" blocker the builder's report repeated uncritically.
Source-confirmed the bug `WAYLAND-LANE.md` P124 documents:
`TerminalView::on_left_mouse_down` (`tiller_terminal/src/lib.rs:1071-1079`) computes the clicked
grid row/column directly from `event.position` (window coordinates) without subtracting the
terminal element's own `bounds.origin`, while the paint path a few lines away does subtract it —
the two are asymmetric. Live, fresh drive (`dp4b`): printed `https://example.com` in a real running
terminal, took `panel.list` before, then `modclick ctrl` at the URL's actual on-screen text
position. `panel.list` after is byte-identical to before (`Chat`+`Terminal` only, no new Browser
surface) and the capture shows no visible reaction — the click produced no link-open effect. This
reproduces the report's own exhaustive-sweep finding with an independent live drive, using a real
multi-pane window (terminal not at origin), not the isolated `gpui::test` window (origin ≈ 0,0)
where the same code coincidentally works. The builder's "already-correct" claim is wrong for the
live app; the row is a genuine, reachable, reproducible defect.
