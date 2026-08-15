# Wave C slice C-P1 — verdicts

Critic pass (not the builder of this slice). Ran every named unit test per-crate (never
`--workspace`), read the production code each test exercises to confirm real dispatch (not a
synthetic field mutation), and independently drove the running app over the Wayland lane
(`TILLER_WL_LABEL` unique per drive, no lock taken) for the two rows where a live drive was
plausible. Binary was already built at HEAD; no build was triggered.

## F-CORE-TERM-02 — half-proven (unchanged)

`cargo test -p tiller_terminal shift_f10_opens_the_context_menu_from_the_keyboard` — 1 passed.
Read the test: it spawns a real `TerminalView` with a live PTY, focuses it with a real
`simulate_click`, then drives `cx.simulate_keystrokes("shift-f10")` through the actual
`on_key_down` dispatch (lib.rs on_key_down, not a direct field write) and asserts
`context_menu.is_some()` plus that `terminal-context-item-0` is actually drawn. This is real
production dispatch, not a stand-in. Independently confirmed the live gesture is genuinely
unreachable on this lane: `docs/linux-rewrite/WAYLAND-LANE.md` states plainly "Right-click,
button-held drag, modifier chords (including Shift+Tab)... still require DISPLAY=:1", and
`Scripts/wayland-drive.sh`'s `key <name>` action only wraps `wtype -k <name>` for a single named
key — no chord/modifier composition exists in the script. X11 lane is off-limits per this task's
constraints. Verdict unchanged: code path proven by real dispatch, live chord instrument-blocked.

## F-EDIT-12 — NOT EXERCISED (unchanged)

`cargo test -p tiller_ui a_drawn_change_row_drags_its_diff_payload_to_a_drop_target` — 1 passed.
Read the test: renders a real `ChangesTab` over a real git repo, locates the real
`changes-file-row` via `debug_bounds`, and drives a real `MouseDownEvent`/`MouseMoveEvent`
sequence /`MouseUpEvent` onto a `DiffDropTargetFixture` (a stand-in only because `tiller_ui`
cannot depend on `tiller_terminal` per the crate dependency direction in CLAUDE.md) — genuine
GPUI event simulation, not a synthetic call. Confirmed `wayland-drive.sh` still exposes only
`ctl/click/move/type/key/title/shot` — no drag primitive of any kind (not even the button-held
motion needed to compose one) — so this remains genuinely un-driveable live on this lane, exactly
as recorded. Not re-attempted on X11 (out of scope for this slice). Verdict unchanged.

## F-TERM-PTY-06 — half-proven (unchanged)

`cargo test -p tiller_terminal a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files`
— 1 passed. Confirmed no code changed for this row (builder's "already-correct" claim holds):
`on_drop::<gpui::ExternalPaths>` is wired on the running-terminal branch and `receive_file_drop`
takes `Vec<PathBuf>`; the test drives the real handler with GPUI's actual multi-file XDND payload
type. `docs/linux-rewrite/ENVIRONMENT.md` independently states XDND drags are out of reach of the
harness on any lane ("xdotool has no source window to negotiate the protocol... unexercisable by
this harness"), which is a platform/tooling limitation broader than just this Wayland lane.
Verdict unchanged.

## F-TERM-UI-02 — NOT EXERCISED (unchanged)

`cargo test -p tiller_terminal platform_modifier_click_opens_a_terminal_link` — 1 passed. Read
the test: spawns a real PTY that prints an actual URL, waits for it to land in the live
alacritty grid, then drives two real `cx.simulate_mouse_down` calls at the link's actual screen
position — once with `Modifiers::none()` (asserts no event), once with `platform: true` (asserts
the emitted `TerminalLinkEvent` carries the exact URL). Real dispatch through
`on_left_mouse_down`, not a stand-in. Confirmed `wayland-drive.sh`'s `click` action has no
modifier parameter and WAYLAND-LANE.md lists modifier chords as still requiring DISPLAY=:1.
Genuinely un-driveable live here. Verdict unchanged.

## F-GIT-REMOTE-01 — PASSED (upgraded from half-proven)

`cargo test -p tiller_ui a_github_origin_remote_prefills_the_avatar_field` — 1 passed. More
importantly: the integrator (per `INTEGRATION.md` section 4, commit `fa1a330`) already landed the
one-line foreign-file fix this row's own report asked for — `sidebar.rs`'s
`open_project_settings` now calls `ProjectIconPicker::with_value_and_repo(icon, &path, cx)`
instead of `with_value(icon, cx)`. `grep -n with_value_and_repo crates/tiller_ui/src/sidebar.rs`
confirms it live at line 976, inside `open_project_settings`.

I drove this end to end through the real running app on the Wayland lane (label
`cp1verify4`/`cp1verify5`, no lock held): created a real git repo with
`git remote add origin git@github.com:octocat/Hello-World.git`, `ctl project.add path=<repo>`,
clicked the real sidebar gear icon (real coordinate click, not a socket call), clicked the
Avatar tab, and photographed the result — the GitHub field showed the literal text `octocat`,
pre-filled, not the field's own placeholder styling (contrast with the still-empty Favicon field
right below it showing its placeholder "Domain, like example.com"). This is discriminating: a
checkout with no GitHub remote (the default/control case) would leave the field on its
placeholder, exactly like the Favicon field in the same frame. Went one step further and clicked
"Use GitHub Avatar" with zero typing — the panel updated to "Current: GitHub avatar for octocat",
proving the full commit path. Frames:
`/tmp/.../scratchpad/shots4/02-avatar-tab.png` (pre-fill) and
`/tmp/.../scratchpad/shots5/02-committed-avatar.png` (commit). Both halves of this row (owner
parsing reachable from `tiller_ui`, and now full app-reachability from the gear menu) are proven
live. Upgraded to PASSED.

## F-PRJ-06 — half-proven (unchanged, independently re-confirmed with fresh live evidence)

`cargo test -p tiller_ui the_drawn_clone_button_cannot_start_a_second_clone` — 1 passed. Read
the test: mounts a real `CloneForm` directly (bypassing the popover chrome, exactly as the report
says), clicks the real `clone-submit` button twice back to back against a real local git repo,
asserts exactly one `Complete` rather than a destination-collision `Failed`. Real dispatch, real
guard.

I independently re-drove the actual popover chrome live (label `cp1verify7`/`cp1verify8`/
`cp1verify9`) to re-check the claimed input-delivery gap rather than take the prior evidence's
word for it: opened the sidebar "+" menu, clicked "Clone Repository...". Text entry actually
DOES land when the URL field is clicked directly first (`https://example.test/repo.git` appeared
correctly, Destination auto-derived to `/home/enzopalmisano/repo`) — this narrows the prior
report's "drops clicks/keystrokes... generally, not just text entry" framing. But the buttons do
not: clicking "Cancel" (real coordinate, real hover-highlight visible in the frame) left the form
open and unchanged across two separate drives, and clicking "Clone repository" with a valid URL
already typed left the status on "Ready to clone" with no state change — the submit click never
registered. So the specific guard this row needs (repeated real button clicks starting/blocking a
clone) remains genuinely unreachable live on this lane — re-confirmed with new evidence, not
inherited. Verdict unchanged at half-proven: guard proven only via the direct-mount test, popover
submit-button click delivery is the confirmed gap, structural and out of this slice's file
ownership (sidebar.rs).

## F-PRJ-09 — half-proven (unchanged)

`cargo test -p tiller_ui the_drawn_create_button_cannot_start_a_second_creation` — 1 passed, same
shape as F-PRJ-06's test (direct-mounted `CreateForm`, two real clicks on `create-submit`, exactly
one `Complete`). Did not re-drive the Create popover separately, but `sidebar.rs`'s
`render_project_form` wraps both `ProjectFormSurface::Clone` and `ProjectFormSurface::Create` in
the identical `.absolute()` overlay (`project-form-overlay`, sidebar.rs ~1965) — the same code
path whose submit-button click-delivery gap I just re-confirmed live for F-PRJ-06. No reason to
expect Create's button to behave differently; verdict unchanged at half-proven on that basis.

---

## Summary

Six of seven verdicts unchanged after independent re-verification (all seven named tests re-run
per-crate, all pass; every test read to confirm it drives real production dispatch rather than a
synthetic stand-in). One upgraded: F-GIT-REMOTE-01 half-proven to PASSED, driven live end-to-end
through the real app (gear -> Avatar tab -> pre-filled GitHub field -> committed avatar), because
the integrator had already landed the row's own requested foreign-file fix in `sidebar.rs`. Also
independently re-confirmed, with fresh live evidence rather than inherited claims, the popover
submit-button input-delivery gap behind F-PRJ-06/F-PRJ-09's half-proven status — narrowing but not
overturning the prior report's characterization (text fields do accept input; only button clicks
fail to register in the Clone-repository popover).
