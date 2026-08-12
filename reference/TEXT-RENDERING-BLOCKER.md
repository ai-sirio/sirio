# RESOLVED — text did not paint in our GPUI app

**Root cause: `gpui_platform`'s `font-kit` feature is opt-in (`default = []`) and we
never enabled it.** It forwards to `gpui_macos/font-kit`, which is the macOS font
backend. Without it GPUI has no way to rasterise a glyph, so quads paint and text
does not — exactly the signature we saw. The fix is one line in `rust/Cargo.toml`:

```toml
gpui_platform = { path = "...", default-features = false, features = ["font-kit"] }
```

`hello_world` misled the investigation: built inside zed-ref it works, because some
other crate in Zed's workspace turns the feature on and cargo unifies features across
a workspace build. The example's source was never the difference.

Found by the terminal builder, verified independently by rebuilding and screenshotting
both the terminal demo (legible powerline prompt) and the shell (145 text pixels where
the label sits). Kept below for the record.

**Status: was open; the biggest gap in the project.** Fills, borders, layout and
geometry all paint correctly and match the reference to the pixel. Glyphs never appear.

Three independent pieces show the identical signature:

| piece | fills | glyphs |
|---|---|---|
| `tiller` shell (integration) | correct | **absent** |
| `tiller_ui` sidebar demo | correct, probes match `MEASURED.md` exactly | **absent** |
| `tiller_terminal` demo | powerline cell backgrounds land on the right cells | **absent** |

## The one thing that DOES work

`zed-ref/crates/gpui/examples/hello_world.rs` paints its text correctly — both when
built inside zed-ref, and when its source is copied verbatim into **our** workspace as
`crates/tiller/examples/hw.rs`. So our workspace, our toolchain, our gpui build and the
machine's fonts are all fine. The defect is in how our own element trees are built.

## Hypotheses already eliminated — do not retest these

Each was tested by building, running, screenshotting and probing the pixels.

1. **Missing fonts / asset source.** Ran hello_world from `/tmp` instead of zed-ref:
   `probe.py diff` gave `mean_delta=0.01`. Working directory is irrelevant.
2. **gpui feature mismatch.** Zed uses `default-features = false`; we used defaults.
   Setting `default-features = false` on `gpui` and `gpui_platform` changed nothing.
3. **No text size set.** Adding `.text_size(px(13.))` changed nothing.
4. **No text colour set.** `.text_color(gpui::white())` was set throughout; the sidebar
   sets it in 10 places. Changed nothing.
5. **The transparent titlebar.** Removing `TitlebarOptions { appears_transparent, .. }`
   changed nothing (the extra bright pixels that appear are the native titlebar).
6. **`Theme::init` / the GPUI global.** Removing it and hardcoding `Theme::dark()`
   changed nothing.
7. **Binary vs example target.** The same shell source compiled as an example behaves
   identically.
8. **Window size.** hello_world at 1470x833 still paints its text.
9. **`size_full()` on the root.** Replacing it with a definite `w(1470) h(833)`
   changed nothing.

## Narrowest reproduction

In our shell, a `div` styled with the *exact* hello_world chain
(`.flex().flex_col().gap_3().bg(rgb(0x505050)).size(px(300.)).justify_center()
.items_center().text_xl().text_color(white()).child("...")`) renders its grey box at
the right size and position, and its text does not appear. The same chain as the
window's root element in hello_world does render.

Adding the text as a **direct child of our root** also failed (2 white pixels found,
both window-corner antialiasing at 1467-1469 x 827-829).

## The next thing to try

The remaining difference is the shape of the root element itself. Ours is a flex column
of fixed height whose children are `h(32)`, `flex_1()` and `h(40)`; hello_world's root
is a single styled div. A flex sibling with `flex_1` absorbing all free space, and text
nodes collapsing to zero measured height under flex-shrink, fits every observation:
quads keep the size their parent gives them, text measures itself and gets crushed.

Test it by bisecting from the known-good direction — start from `hw.rs`, which works,
and mutate it one property at a time toward our shell, screenshotting each step. Do not
start from our shell and mutate toward hello_world; that direction has already consumed
nine experiments without converging.

Measure with:

```bash
python3 reference/probe.py px <shot>.png <x> <y>
```

and count light pixels in the region where the text should be — do not eyeball a
thumbnail, and remember the traffic lights are bright and sit at x < 90.
