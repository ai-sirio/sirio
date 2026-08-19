# Critic pass — P102 "the top bar belongs to the OS" (Decorations-gated traffic lights)

**Verdict: CLEARED.** The fallback is real, both branches are proven deterministically, the window
stays closable in the Client fallback, the live `_MOTIF_WM_HINTS` hint is unchanged, macOS is
untouched, and no BrowserChrome token this diff touches is now dead. One pre-existing dead token
unrelated to this diff was found and is reported below. Biggest remaining gap: **the visual claim
about a WM-decorated real desktop is still unverified anywhere** — not by this pass, not by the
builder, not by anyone, because no window manager exists on this machine, and that is the one part
of the directive ("one row, OS chrome + our icons") that nobody has actually looked at.

Fresh critic, no relation to the commits under judgement. Work judged: `85270f5e`
(`fix(P102): draw window controls only under client-side decorations`), on top of `85e2ae35`, in
the worktree at `/var/tmp/tt-topbar-3776508-28221`, branch
`linux/topbar-server-decorations-3776508`. I did not write any of this commit and owe the builder
nothing. Verified in my own worktree at `/var/tmp/critic-topbar-verify-4014_528534` (branch
`critic-topbar-verify-4014_528534`, created from `85270f5e` directly), `CARGO_TARGET_DIR=/var/tmp/critic-topbar-target-4014_528534`.

## What changed (read from the diff, not the commit message)

Only two files: `rust/crates/tiller_ui/src/titlebar.rs` (+238/-52) and
`rust/crates/tiller_theme/src/lib.rs` (+40/-3, doc comments only). `git diff 85e2ae35..85270f5e
--stat -- rust/crates/tiller/src/main.rs` is empty — the macOS window-open path
(`TitlebarOptions { appears_transparent: true, traffic_light_position: Some(point(px(12.),
px(12.))) }` at `main.rs:11943-11945`) is byte-for-byte untouched. **Item 4 confirmed by diff, not
by trust.**

`Titlebar::render` now asks `window.window_decorations()` (or a test-only `decorations_override`)
fresh every render, exactly Zed's `platform_title_bar.rs` idiom — I read
`~/.cargo/git/checkouts/zed-a70e2ad075855582/c05e346/crates/platform_title_bar/src/platform_title_bar.rs`
myself and confirmed line-for-line: `window.window_decorations()` called inside `Render::render`
(not cached at construction), a `PlatformStyle::Mac => None` carve-out in
`render_right_window_controls` that this commit's `cfg!(target_os = "macos")` mirrors, and — this
matters for item 2 below — the drag-to-move block (`on_mouse_down`/`on_mouse_move` →
`window.start_window_move()`) and the double-click dispatch sit *outside* any `Decorations` match
in Zed's own code, unconditional in both branches. The Tiller diff's drag/double-click block is
likewise outside the `show_traffic_lights.then(|| …)` closure — confirmed by reading the render
body directly, not inferred from the commit message.

`show_traffic_lights = cfg!(target_os = "macos") || matches!(decorations, Decorations::Client {
.. })`. When false, the `traffic_lights` div is not constructed at all (`.then(||…)`, not a hidden
element), and the icon cluster's leading padding switches from `traffic_light_cluster_gap` to
`traffic_light_inset` so it becomes the row's own leftmost control rather than sitting in a gap
that assumed a light group ahead of it.

## Item 1 — both branches, deterministically

The test seam is real, not fabricated. `TestWindow` never overrides `window_decorations()`, so it
inherits gpui's own trait default — I read this at
`~/.cargo/git/checkouts/zed-a70e2ad075855582/c05e346/crates/gpui/src/platform.rs:903`:
```rust
fn window_decorations(&self) -> Decorations {
    Decorations::Server
}
```
Without `Titlebar::with_decorations()`, every existing test before this diff could only ever have
exercised the Server arm. The new seam (`decorations_override: Option<Decorations>`, `None` in
production, always calling the real platform method) is the only way to drive `Decorations::Client`
under test, and it has no effect on macOS (the `cfg!` check short-circuits first).

The assertions read real layout, not a shadow structure. `VisualTestContext::debug_bounds` (gpui's
own `crates/gpui/src/app/test_context.rs:888-890`) reads `window.rendered_frame.debug_bounds`,
populated only for elements carrying an explicit `.debug_selector(...)` — I confirmed `traffic_light()`
and `cluster_button()` both set `.debug_selector(move || id.to_owned())` matching their `.id(...)`,
so `debug_bounds("titlebar-close")` genuinely reflects whether that div was painted this frame, not
a stand-in.

Compiled the branch myself (`cargo build -p tiller_ui -p tiller_theme`, dev profile, clean, 28m19s
— slow only because this machine had ~8 other agents' cargo builds contending, load average
regularly above 40 on 12 cores) and ran the suite (`cargo test -p tiller_ui --lib titlebar`):

```
test titlebar::tests::traffic_lights_draw_only_under_client_side_decorations ... ok
test titlebar::tests::the_cluster_shifts_left_when_no_traffic_lights_are_drawn ... ok
test titlebar::tests::tiled_edges_do_not_change_whether_the_fallback_controls_draw ... ok
test titlebar::tests::the_close_control_closes_the_real_window ... ok
test titlebar::tests::the_minimize_control_invokes_its_wired_handler ... ok
test titlebar::tests::the_maximize_control_invokes_its_wired_handler ... ok
...
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 348 filtered out
```

`traffic_lights_draw_only_under_client_side_decorations` constructs `Decorations::Server` and
`Decorations::Client { tiling: Tiling::default() }` in the same test and asserts the close/minimize/
maximize divs are absent under Server and present under Client, and that `titlebar-sidebar` (the
icon cluster) draws in *both* — the "OS chrome + our icons, not OS chrome only" half of the
directive, proven for both branches in one place. **Both branches genuinely exercised, not half the
work.**

## Item 2 — window still closable in the fallback

`the_close_control_closes_the_real_window`, `the_minimize_control_invokes_its_wired_handler`, and
`the_maximize_control_invokes_its_wired_handler` were all updated by this diff to add
`.with_decorations(client_side_decorations())` — without that addition they would silently degrade
to testing nothing (the traffic lights wouldn't draw under the TestWindow's real Server default, so
`debug_bounds` would return `None` and any subsequent click-simulation would hit nothing). All three
pass. The close test's own doc comment states the discipline directly: "drive the control, not the
function behind it" — it simulates a real click on the hit region gpui computed for the div, not a
direct call to the handler closure. Default handlers are unchanged: `on_close: Rc::new(|window|
window.remove_window())`, `on_minimize: … minimize_window()`, `on_maximize: … zoom_window()`
(`titlebar.rs:215-217`).

## Item 3 — the live `_MOTIF_WM_HINTS` hint, read back myself

Built the full `tiller` binary myself (`cargo build -p tiller`, same target dir, reused the cached
dependency tree from the lib build — 15 more minutes under the same contention). Booted my own
`Xvfb -displayfd 30 -screen 0 1280x800x24` (got `:3`; never touched `:1`, `:2`, or any
`wayland-*` display — confirmed no process of mine referenced them). Launched the binary against a
throwaway `$HOME` and a short-path `$TILLER_SOCKET` (the first attempt's socket path under the
session scratch dir was 140 bytes and the control server refused it outright — a real, if narrow,
robustness gap worth a one-line mention: control-socket paths should probably be validated or
shortened rather than silently failing to start, but this is pre-existing and off this diff's scope).

The app creates three top-level windows under this bare-Xvfb/no-WM setup (not six as the task brief
described — likely a smaller count on this GPUI revision/config, or the other three are children of
these three rather than siblings of root; `xwininfo -root -tree` only shows top-level children).
Two are `1x1` and `IsUnMapped`; exactly one, `0x200001`, is `1470x833` and `IsViewable`. Matching on
`IsViewable` first — the exact discipline the task brief calls out — avoided the false-negative trap
of reading a property off one of the two unmapped placeholder windows. `xprop -id 0x200001
_MOTIF_WM_HINTS` on the live, mapped window returned:

```
_MOTIF_WM_HINTS(_MOTIF_WM_HINTS) = 0x2, 0x0, 0x1, 0x0, 0x0
```

Verbatim what P102 measured before this fix, and what `x11/window.rs:1874`'s `WindowDecorations::
Server` arm writes. **This diff did not change what we ask the window manager for — confirmed live,
not just by reading the code.**

**Beyond what was asked, but worth recording:** I also got a real rendered screenshot of the app's
own row (not the OS-drawn titlebar, which is unavailable here for the reason below). The first
capture was solid black — this GPUI/wgpu backend needs a size-change event before its first real
paint under headless Xvfb, not just elapsed time (`vulkan: No DRI3 support detected`, matching
`docs/linux-rewrite/tasks/P127-browser-child-unavailable.md`'s finding; the same
`LIBGL_ALWAYS_SOFTWARE=1` fix used in `docs/linux-rewrite/fullapp/CRITIC-commands.md` applies here
too, plus an `xdotool windowsize` nudge to force a repaint — the same trick that document
independently discovered). With that, `import -window root` captured real content: the top row
shows the sidebar-toggle/back/forward/+ cluster flush against the left edge, refresh/right-panel
icons at the right edge, and **zero traffic-light dots anywhere in the row** — live, pixel-level
confirmation of the Server branch under the exact runtime condition this build actually hits
(`x11/window.rs`'s `window_decorations()` returns `Decorations::Server` whenever
`client_side_decorations_supported` is false, which a bare Xvfb with no compositor always is — read
directly from `vendor/gpui_linux/src/linux/x11/window.rs:1784-1791`). This is not proof of the
WM-decorated case (see the gap below) — it is proof that *our own* row does what the Server-branch
code says it does, under a real running process, not just under `#[gpui::test]`.

Cleaned up: killed only the app PID and my own Xvfb PID (`:3`), verified both gone, never touched
`:1`/`:2`/any `wayland-*` process.

## Item 4 — macOS untouched

`git diff 85e2ae35..85270f5e -- rust/crates/tiller/src/main.rs` is empty. Inside `titlebar.rs`,
`show_traffic_lights` short-circuits to `true` for macOS before `Decorations` is even consulted
(`cfg!(target_os = "macos") || matches!(…)`), so macOS keeps drawing its three dots exactly as
before P102 — the literal pre-diff code path, per the "MUST NOT BE TOUCHED" constraint.
`Spacing::traffic_light_inset` (the *other* field, `tiller_theme/src/lib.rs:500`, distinct from
`BrowserChrome::traffic_light_inset` at `:660`) is untouched by this diff's hunks (which only edit
lines 607-674) — but see the dead-token finding below, because "untouched" and "still needed" turn
out to be two different claims.

## Item 5 — dead tokens

None of `traffic_light_diameter` / `_gap` / `_cluster_gap` / `cluster_start()` are dead: all four
are still read by `titlebar.rs`'s `show_traffic_lights.then(||…)` closure (exercised live by the
Client-decorations tests above), and `cluster_start()`'s 70px derivation is still asserted by
`conformance.rs:250` and `tiller_theme/src/lib.rs:2255`. `traffic_light_inset` (the `BrowserChrome`
field) is correctly documented as dual-use and is read in both branches (`titlebar.rs:451` for the
lights, `:487` for the cluster's own leading gap when the lights are absent) — confirmed by grep,
not by trusting the doc comment.

**One dead token found, pre-existing, not introduced by this commit.**
`Spacing::traffic_light_inset` (`tiller_theme/src/lib.rs:500`, default `px(14.0)` at `:535`) has
**zero consumers anywhere in the Rust tree** — `grep -rn "traffic_light_inset" rust --include=*.rs`
finds only its own field declaration, its own `Default` initializer, and a bare value assertion
(`assert_eq!(spacing.traffic_light_inset, px(14.0))`, `tiller_theme/src/lib.rs:2047`). No file reads
`spacing.traffic_light_inset` to place anything on screen — not `titlebar.rs`, not `main.rs`,
nowhere. This predates `85270f5e` (the diff's hunks are at lines 607-674, nowhere near 484-535) so
it is not a regression this commit introduced, but it is exactly the failure mode the task brief
warned about: a token with a live test and no drawing consumer, sitting one `grep` away from
someone reviving dead layout under the belief the test proves it matters. Worth a follow-up ticket,
not a blocker for this one.

## Item 6 — regressions

Ran the full suite for both touched crates myself: `cargo test -p tiller_ui -p tiller_theme --lib`
→ **368 passed / 0 failed** (tiller_ui) + **57 passed / 0 failed** (tiller_theme) = 425 passed, 0
failed, 0 ignored. `cargo clippy -p tiller_ui -p tiller_theme --lib --tests` → 25 pre-existing
warnings in `tiller_ui` (type-complexity notes on `Rc<dyn Fn(...)>` struct fields, `needless_borrow`
in `sidebar.rs` — all outside this diff's hunks) and zero new ones; the one `titlebar.rs` warning
clippy does flag (`type_complexity` at line 439, `on_right_panel`'s type) is on a pre-existing field
untouched by this diff. `cargo fmt --check` on `titlebar.rs` alone: clean. (`tiller_ui`/`tiller_theme`
as a whole are not fmt-clean — pre-existing drift in `chat.rs`/`sidebar.rs`/`tab_bar.rs`/one
`tiller_theme` test, all outside this diff's touched lines; also present as *uncommitted* changes
sitting in the builder's worktree, unrelated to `85270f5e`.)

The "347 rows" figure in my brief is the `INVENTORY-LEDGER.md` PASSED feature-count (I read it,
did not touch it), not a `cargo test` count — a different unit than what I could reproduce directly.
Re-verifying 347 hand-checked feature rows live is outside this pass's scope. What I can say
concretely: `git diff --stat` shows this commit touches exactly two files
(`titlebar.rs`, `tiller_theme/src/lib.rs`), so no other ledger-tracked surface was modified by it,
and the automated coverage over those two files — the only files that *could* regress — is fully
green.

## The biggest remaining gap

**Nobody has ever seen this change under an actual window manager.** The whole premise of P102 is
"a real WM draws a titlebar; stop drawing a second row underneath it" — and every load-bearing piece
of that claim (the WM honours `_MOTIF_WM_HINTS`, decorates the window, and the result reads as one
coherent row rather than a naked, undecorated, unclosable box) is still resting entirely on
inference from the Motif hint's bit pattern, never on a screenshot of a compositor actually drawing
something. This isn't a gap in this diff specifically — the task brief says so explicitly, and I
hit the identical wall the builder did: `openbox`/`marco`/`xfwm4`/`mutter`/`i3`/`fluxbox`/`metacity`/
`twm`/`icewm` are all absent from this machine, and `cosmic-comp` is off limits. But it means the
directive's literal ask — "one row, OS chrome + our icons" — is still unverified as a *visual*
outcome by anyone, on any pass, ever. The code now does the right thing *conditionally on what GPUI
reports*, which is the only half of the contract inference and unit tests can reach. Whether a real
WM's titlebar plus this app's now-single icon row actually reads as one coherent bar, or as an
orphaned icon strip floating under someone else's chrome, is a design judgment that needs a real
screenshot and remains completely open.

## Files

- Judged: `/var/tmp/tt-topbar-3776508-28221` (branch `linux/topbar-server-decorations-3776508`,
  commit `85270f5e`) — not owned by me, not modified.
- Verification worktree: `/var/tmp/critic-topbar-verify-4014_528534` (branch
  `critic-topbar-verify-4014_528534`, `CARGO_TARGET_DIR=/var/tmp/critic-topbar-target-4014_528534`).
- Live screenshots (not committed — informal verification artefacts):
  `/tmp/claude-1000/-home-enzopalmisano-Scrivania-Progetti-tiller-linux/125287a8-12fe-48c7-a48e-83f66c7a0bb9/scratchpad/serverroot2-4014_528534.png`
  (full window) and `topbar-big-4014_528534.png` (cropped top row, 2x).
