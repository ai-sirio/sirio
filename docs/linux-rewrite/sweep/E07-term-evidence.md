## F-TERM-04 (ledger line 322)

**Claim: could-not-reach.**

The remaining half of this row (per `UNPROVEN-ROWS-RECIPES.md`) requires opening the terminal
context menu, selecting `Copy`, pasting into a second pane, then selecting `Paste` from the
menu after copying elsewhere — i.e. it requires invoking the menu twice via right-click and
then clicking a specific menu item.

Checked the source directly: `rust/crates/tiller_terminal/src/lib.rs:1446` and `:1504` wire
`open_context_menu` only to `.on_mouse_down(MouseButton::Right, ...)`; there is no keyboard
alternative (no `Menu`/`Shift+F10` binding found for it). `Scripts/wayland-drive.sh`'s action
vocabulary (`ctl`, `click`, `move`, `type`, `key`, `shot`) implements only left-click
(`click <x> <y> — left-click absolute nested-output coordinates`, line 20 of the script); there
is no right-click, drag, or button-selection primitive available on this lane. This matches
`docs/linux-rewrite/WAYLAND-LANE.md`'s own statement: "Right-click, button-held drag, modifier
chords ... and IME input still require `DISPLAY=:1` until separately proven."

My instructions forbid using `Scripts/linux-drive.sh` or `DISPLAY=:1` for this slice. There is
no socket method (`system.capabilities`) that invokes a terminal context-menu action directly
without the click — the existing half-proven evidence (`p17-rclick-term.png`) was itself
produced on the X11 lane, not this one.

No drive performed; no new capture. Row is not reachable from the Wayland lane.
