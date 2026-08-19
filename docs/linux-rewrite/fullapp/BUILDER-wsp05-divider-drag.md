# Builder live drive — F-CORE-WSP-05, the divider-drag focus leg

Requested by team-lead as the third live drive owed on this surface: the fix
(`e685172b`, `fix(F-CORE-WSP-05): keep pane focus across a divider drag`) is
merged into `linux/gpui-waku` (it is an ancestor of `6f4de662`); what was
owed was the live proof, and the row names it precisely — a divider drag is
a fraction-class, nonstructural command, so it must leave focus alone.

Worktree `/var/tmp/tt-wsp05-4047521`, branch `verify/f-core-wsp-05-4047521`,
off `origin/linux/gpui-waku` at `6f4de662`. No code changes — verification
only. Built standalone (`CARGO_TARGET_DIR=/var/tmp/cargo-target-wsp05-4047521`),
driven under `Scripts/wayland-drive.sh` with a private label
(`TILLER_WL_LABEL=wsp05-4047521`), never touching `DISPLAY=:1` /
`wayland-0` / `wayland-1`. Fully torn down afterward — matched by
`TILLER_SOCKET` / `SWAYSOCK` / `TILLER_WL_LABEL`, confirmed by `pgrep`
finding nothing under this label and no stray `/tmp/wsp05-4047521*` files.

## The method: zero-click, probe/drag/probe, read back through `panel.scrollback`

The brief was specific about why: clicking a pane to check where focus went
*moves* focus, so it proves nothing about what the drag itself did. The only
honest check is to establish where keyboard input lands **before** the
gesture, run the gesture with no clicks of its own kind (a divider drag is a
`down`/`move`/`up` sequence, not a discrete click), then check where keyboard
input lands **after** — with the check itself being another zero-click typed
probe, not a click.

Concretely: type a unique marker (real Wayland key events via `wtype`,
landing wherever GPUI's real window focus currently is — not sent to a
specific pane by id), drag the divider handle, type a second unique marker,
then read every pane's actual PTY scrollback back through the control
socket's `panel.scrollback` — the same authoritative source `tillerctl`
itself would read, not a screenshot guess.

Setup, exactly as driven:

1. `ctl workspace.select workspace=p-c1fd7a5bbfd541af-wt-26` — this worktree
   was already auto-discovered as part of the `tiller` project's 27
   worktrees (all worktrees sharing one git common-dir become one project;
   `project.add` on this path returned `"added":"false"` because it already
   existed as `wt-26`). Selected directly by id rather than by a sidebar
   click, since selecting is setup, not part of the probe window.
2. `chord ctrl t` — new terminal (`pane-0`), a plain bash shell.
3. `chord ctrl+alt+shift Right` — the real Split Right keychord
   (`panes.rs`'s `ctrl-alt-shift-right`, the same binding the F-CORE-WSP-05
   regression test itself drives). Confirmed via `panel.list`: two panes,
   `pane-0` and `pane-1`, with `pane-1` reported `"active":"true"` — the
   split lands focus on the new pane with zero clicks, which is the row's
   own documented baseline and lines up with this drive's later readings.

`02-split-right-two-panes.png` shows both panes fully rendered (each
running `fastfetch`, the neofetch-style banner in this fixture's shell
init), roughly split 50/50, divider around x≈817-819 in the 1715×972
capture.

## The probe/drag/probe sequence itself

**Probe A** (before the gesture, zero clicks): `wtype "echo
WSP05_PROBE_A_4047521"` + Return. `03-probe-a-before-drag.png` shows the
echoed line landing in the **right** pane (`pane-1`) only — the left pane's
prompt is untouched. This also doubles as the positive control: it proves
typed input visibly reaches a real pane's PTY and is captured on screen,
before any claim is made about where it lands after the drag.

**The drag** (`down` at `(818, 500)`, four `move` waypoints out to
`(950, 500)`, `up` at `(950, 500)` — composing the same primitives
`Scripts/wayland-drive.sh`'s own `drag` helper uses). **No `shot` between
the `down` and the `up`** — the script's own header warns that `shot`'s
forced-repaint is a real window resize, and doing that mid-gesture is
exactly what produced false conclusions elsewhere in this codebase
(`PLUS-MENU-INVESTIGATION.md`). Captured only before and after.

`04-after-drag-ratio-changed.png`, taken immediately after the gesture
completes, shows the divider genuinely moved: the left pane widened from
≈485px to ≈618px and the right pane narrowed to match, with its `fastfetch`
text visibly rewrapped at the new width. This is the "or this test proves
nothing about the divider" check the row's own unit test insists on —
confirmed here by a real, visible resize, not inferred.

**Probe B** (after the gesture, zero clicks — no pane was clicked to check
anything): `wtype "echo WSP05_PROBE_B_4047521"` + Return, issued the moment
the drag's `up` acknowledged. `05-probe-b-after-drag-zero-click.png` shows
it landing directly under Probe A's output, in the same right pane, in the
now-narrower layout.

## The read-back, straight from `panel.scrollback`

Screenshots show what a person would see; this is what `tillerctl` itself
would read. Both panes queried after the whole sequence:

```
ctl panel.scrollback id=pane-0 maxBytes=4096
```
tail of `pane-0`'s scrollback — the `fastfetch` banner and an empty prompt.
**Neither probe marker appears.** Untouched, exactly as it was before the
split gave focus away.

```
ctl panel.scrollback id=pane-1 maxBytes=4096
```
tail of `pane-1`'s scrollback:
```
╭─ bash tt-wsp05-4047521  verify/f-core-wsp-05-4047521 ↓1  0ms
                    20,01:13
╰─ echo WSP05_PROBE_A_4047521
WSP05_PROBE_A_4047521
╭─ bash tt-wsp05-4047521  verify/f-core-wsp-05-4047521 ↓1  3ms
                    20,01:14
╰─ echo WSP05_PROBE_B_4047521
WSP05_PROBE_B_4047521
╭─ bash tt-wsp05-4047521  verify/f-core-wsp-05-4047521 ↓1  2ms
                20,01:15
╰─
```

Both probes landed in **the same pane** (`pane-1`), the one that held focus
before the gesture — exactly the row's requirement, read back through the
authoritative channel rather than eyeballed off a screenshot. A final
`panel.list` after the whole sequence still reports `pane-1` as
`"active":"true"`, `pane-0` as `"active":"false"` — the app model's own
notion of focused pane never moved either, consistent with the fix's own
doc comment (`refocus_focused_pane` restores at `on_drop`, so nothing ever
observably left).

## What this drive does and does not claim

This proves the **shipped, merged fix** behaves as its commit message says,
end-to-end in the real running app — not a re-litigation of the code-level
regression proof, which the row already has in the two-sided unit test
(`drawn_divider_drag_leaves_pane_focus_untouched`, previously asserting the
defect, inverted and green now that the defect is fixed). No revert-and-compare
was run here because none was asked for and the unit test already carries
that contrast at the code level; this drive's job was specifically to show
the live app, not the test harness, gets the same answer.

**Not in scope, not touched**: the row's other open leg (Insert and Move
having no identified real-app analog) was not exercised or observed during
this drive — nothing to report on it either way.

## A harness snag worth naming, not a defect

The first attempt at this drive ran `ctl workspace.select ...` then `shot`
with no `click`/`type`/`chord`-family actions in the same
`wayland-drive.sh` invocation. Per the script's own startup gating, neither
the persistent virtual pointer nor the persistent virtual keyboard is
started unless the action string itself contains a matching action — so
`chord ctrl t` issued moments later (directly against the still-live
instance, outside that first invocation) silently reached nothing: no
`wtype` process had ever bound a `wl_keyboard`. The screenshot looked
identical before and after two separate forced-repaints, which is the same
"unchanged capture" trap the script's own comments warn about, except here
the root cause was upstream of any repaint question — no input device
existed to consume the keystroke at all.

Fixed by tearing the instance down (matched by `TILLER_SOCKET` /
`SWAYSOCK` / `TILLER_WL_LABEL`, confirmed clean by `pgrep`) and reissuing a
single invocation whose action string included both a `move` (to trigger
the pointer) and a `chord` (to trigger the keyboard) alongside the real
setup steps. Once both devices exist, ordinary direct `wtype`/FIFO calls
against the same live instance work exactly as documented. Worth naming so
a future pass doesn't misread a blank-looking diff as "the app ignored
ctrl-t" when the actual cause is upstream of the app entirely.

## Cleanup

Torn down via the same `kill_ours`-style scoped match this codebase's other
drives use (`TILLER_SOCKET=/tmp/wsp05-4047521.sock`,
`SWAYSOCK=/tmp/wsp05-4047521-sway.sock`,
`TILLER_WL_LABEL=wsp05-4047521`), confirmed by `pgrep` finding nothing left
under this label and no stray `/tmp/wsp05-4047521*` paths. `/dev/shm/tt`
was never touched by this drive.
