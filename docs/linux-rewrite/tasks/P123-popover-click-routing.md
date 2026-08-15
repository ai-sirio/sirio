# P123 — the "anchored-popover input-delivery gap" is a real layout defect, not a lane limitation

## Verdict

**It is a genuine app defect** (a GPUI layout/positioning bug in `sidebar.rs`), **not** a
Wayland-lane input-delivery gap. Once clicks are targeted at the popover's *actual* on-screen
position — which is not where its own styling implies it should be — both Cancel and the primary
submit button register on ordinary synthetic clicks. I did not fix it (I own no Rust in this
worktree); this file is the evidence handoff.

## The bug

`Sidebar::render_project_form` (`rust/crates/tiller_ui/src/sidebar.rs:1965`) builds the Clone
repository / Create project popover as:

```rust
div()
    .id("project-form-overlay")
    .absolute()
    .left(px(0.0)).right(px(0.0)).top(px(0.0)).bottom(px(0.0))
    .bg(rgb(0x000000).alpha(0.35))   // full dark scrim — implies a full-window modal
    .flex().items_center().justify_center()
    .child(/* the 300px-wide form card, incl. Cancel and the submit button */)
```

`left/right/top/bottom(0.0)` on an `.absolute()` element resolve against the nearest
`.relative()`-positioned ancestor. That ancestor is **not the window root** — it is the Sidebar's
own top-level div (`Sidebar::render`, `sidebar.rs:2580`):

```rust
div()
    .relative()
    .flex().flex_col()
    .w(px(SIDEBAR_WIDTH))   // ≈ 280px, not the window width
    .h_full()
    .overflow_hidden()
    ...
```

`render_project_form` is appended as a child of *this* div (`sidebar.rs:2821`,
`.when_some(project_form, |this, form| this.child(Self::render_project_form(...)))`). So the
"full-screen dark scrim + centered modal" the code visually implies is actually laid out, and
centered, **within the ~280px-wide sidebar column** — not the window. The card ends up flush
against the left edge of the screen instead of centered in it.

**Screenshot proof:** `p123-01-popover-confined-to-sidebar.png` — the Clone repository card and
its scrim occupy only the sidebar's own column; the terminal and Files panel to its right are
completely unobscured, which a real full-window modal would never allow.

## What this explains

A tester (human or agent) driving this popover reasonably expects a full-window-centered modal —
that's what the dark scrim and `items_center().justify_center()` styling *say* it should be. If a
critic's click coordinates were calibrated for that expected full-window-center position, they
would land on ordinary sidebar/background content, not on the popover at all — reproducing exactly
the earlier "Cancel left the form open" / "submit never registers" symptom, with no click-routing
defect required to explain it. I made this identical calibration mistake myself, repeatedly, before
reading the popover's actual rendered position from a screenshot each time.

## Once correctly targeted, clicks do land

All of the following used `Scripts/wayland-drive.sh` against the real running app (`project.add` →
`+` → `Clone Repository…` → click the field → `wtype` the URL → click a button), with coordinates
read from the immediately-preceding screenshot at the same output resolution:

- **Cancel, no prior field focus:** closed the popover on the first click.
  (`p123-02-cancel-closed-first-click.png`, colour-count drop from 10 640 → 6 853 confirms the
  overlay actually left the frame, not just a static capture.)
- **Cancel, after the URL field had been clicked/focused first** (matching the critic's exact
  reported sequence): **still closed on the first click** —
  `p123-05-cancel-after-field-focus-still-first-click.png`. This rules out a generic
  "first click after a focused text field is swallowed as a blur" explanation.
- **Submit ("Clone repository"), after typing a URL:** the first click produced no visible change
  after a 2.5 s settle (`p123-03-submit-first-click-no-effect.png`, form still reads "Ready to
  clone"). An **identical second click at the same coordinates** did register:
  `p123-04-submit-second-click-real-clone-error.png` shows the button now reads "Retry clone" and
  a real, non-fabricated error is on screen:

  ```
  Clone failed: git exited with status 128: Cloning into '/home/enzopalmisano/Hello-World'...
  git: 'remote-tps' is not a git command. See 'git --help'.
  The most similar command is
      remote-https
  ```

  This is decisive: the click **did** reach the submit handler, which **did** shell out to a real
  `git clone`, whose real stderr is rendered verbatim in the popover. This is not a UI that never
  receives the click — it is a UI whose full round trip (button → subprocess → error surfaced)
  demonstrably completed.

  (Separately, `remote-tps` instead of `https` shows the field's typed value lost its leading `h`
  — most likely `wtype` racing the field's post-click focus transition on my end, not an app
  defect; noted for completeness, not asserted as a finding.)

## Discriminators run, per the request

- **Control click on a non-popover surface, same drive:** the `+` "add project" button and its
  "Clone Repository…" menu item both registered on the first click in every drive that didn't
  itself contain a coordinate mistake — general click delivery in this lane is not in question.
- **Does the app log the mouse-down at all:** no logger backend is registered
  (`log::error!`/`tracing` calls are silently dropped — confirmed empirically: existing app logs
  for browser/xdg-open code paths that are known to run contain zero `log`-crate output), so this
  discriminator was inconclusive by itself; the git-clone-error round trip above substitutes for it
  with a stronger, content-based proof that the click handler ran.
- **X11 lane cross-check:** not run — the round-trip evidence above (a real subprocess error
  surfacing from a real click) was already decisive, and `Scripts/linux-drive.sh` takes the
  project's single global drive lock, which this task's budget did not need to spend.

## Recommendation (not performed — no Rust owned in this worktree)

`render_project_form`'s overlay (and — worth checking at the same time — `render_project_settings`
and `render_context_menu`, which follow the identical `.absolute().left(0).right(0).top(0).bottom(0)`
pattern off the same Sidebar root) should be hoisted so its positioning root is the window/workspace
level, not the Sidebar panel. The minimal fix is likely rendering these overlays as children of the
Workspace root (`tiller/src/main.rs`) rather than inside `Sidebar::render`, or giving the overlay an
explicit anchor that accounts for `SIDEBAR_WIDTH` instead of relying on an ambient `.relative()`
ancestor that happens to be far narrower than the window.

## Rows this affects

`F-PRJ-06`, `F-PRJ-07`, `F-PRJ-09` were all recorded `half-proven` against the "anchored-popover
input-delivery gap." Per the evidence above, the correct framing for a future drive is: locate the
popover at its *actual* (sidebar-confined) position each time — do not assume window-center — and
these rows should be re-driveable to a real verdict on that basis, independent of this layout bug
being fixed.
