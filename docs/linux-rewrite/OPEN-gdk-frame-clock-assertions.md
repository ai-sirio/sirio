# Open — two GdkFrameClock assertions, same subsystem, neither explained

Three independent critics have now reported a GTK/GDK critical against
`GdkFrameClock`. They were found in different sections, by agents that had not read each other's
reports, and each was recorded as an isolated curiosity. Written down together because they are
almost certainly one thing, and because "reproducible, harmless so far" is how a real defect hides.

Neither has ever been correlated with a functional failure. Neither is a reason to hold a verdict.
Both are unexplained.

## 1. `_gdk_frame_clock_thaw: assertion failed` — on every browser close

Seen by `CRITIC-brw-unmap.md` (against `382bf383`) and again by `CRITIC-brw-close.md` (against
`474f266a`, the fix), in both of that critic's sessions. Fires on **every** close of a Browser tab,
before and after the teardown fix, so it is not something `close_native` introduced and not
something it repaired.

The brw-unmap critic originally offered it as a lead for the tab-close bug. It was not the cause:
the cause was Xlib buffering (see `fullapp/F-BRW.md`, defect 1, resolved). The assertion outlived
the bug it was suspected of.

## 2. `_gdk_frame_clock_freeze: assertion 'GDK_IS_FRAME_CLOCK (clock)' failed` — at startup, only when X11 is forced at runtime

Seen by `CRITIC-x11-forced.md` (against `7d4d2182`). Fires **once** at startup, and the condition is
specific enough to be a real clue:

| how the process reached X11 | assertion |
|---|---|
| `WAYLAND_DISPLAY` set at exec, cleared by `prepare_environment` (cases 2 and `TILLER_FORCE_X11=maybe`) | **fires** |
| X11 native from exec — no `WAYLAND_DISPLAY` ever (cases 1 and 5) | absent |
| nothing forced, app stays Wayland (case 3) | absent |

So it is not "forcing X11" and not "running on X11". It is specifically *changing our mind about the
display server after exec but before GTK initialises*.

## What connects them, as a hypothesis and not a finding

Both are `GdkFrameClock` assertions in a process where **GDK never owns a toplevel and no GTK main
loop ever runs**. Tiller calls `gtk::init()` and then drives GTK by hand — `while
gtk::events_pending() { gtk::main_iteration_do(false) }` from a GPUI task
(`tiller_ui/src/browser.rs`) — purely to service wry's child window. A frame clock belongs to a
realized GDK window driven by a running main loop; ours is neither.

That would explain both sites at once: a freeze/thaw pair asked to operate on a clock that was never
properly established. It would also explain why nothing breaks — the clock's job is frame pacing for
windows GDK is drawing, and GDK is not drawing any of ours.

**This is a hypothesis nobody has tested.** What would test it: whether the same assertions appear
in a minimal wry-child-in-a-foreign-window program with no GPUI at all; whether GDK's own
`gdk_window_get_frame_clock` returns NULL for wry's child in our setup; and, for #2, whether
initialising GTK before rather than after `prepare_environment` changes anything — GTK is currently
initialised lazily, inside `BrowserSurface::new`, long after `main` has rewritten `GDK_BACKEND`.

## What not to do with this

Do not silence it. An assertion that fires reproducibly under a named condition is the cheapest
diagnostic this codebase currently has for the GTK seam, and the seam is where the browser lives.
If it is ever going to matter, it will matter as the first sign of something else.
