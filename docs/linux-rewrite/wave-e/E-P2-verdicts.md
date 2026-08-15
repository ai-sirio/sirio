# Wave E slice E-P2 — critic verdicts

Critic instance, independent of the builder. Both rows arrived claimed
**already-correct / no code change**; verified by re-driving each claim's
own instrument rather than trusting the builder's report.

## `F-CORE-ACT-11` — ledger line 347 — **PASSED**

Re-read `tab_bar.rs`: the new-tab menu is `deferred(anchored()...)`, which
escapes to the window root at paint time — a different mechanism from the
Sidebar's plain `.absolute()` overlay that P123 documents as confined to its
280px parent. `cargo test -p tiller_activity --lib --tests` — 35 + 25 = 60
tests pass (two binaries), model.rs's ownership-set tests included.

Then independently live-drove the exact gesture the wave-D critic could not
reproduce, in my own fresh, uncontended `wayland-drive.sh` instance
(`TILLER_WL_LABEL=epcritic3`, unrelated to the builder's `ep2row1c`):
`ctl project.add path=<abs>` then a synthetic `click` at the `+` button's
measured on-screen position (1289,49, located by cropping/zooming the
pre-click frame, not guessed). The dropdown opened on the **first attempt**,
correctly anchored under the button and drawn over the Files panel (not
squeezed into the sidebar column the way the P123-class bug would render
it), listing New Terminal, Changes, New Browser, Claude Code, Codex,
OpenCode, Pi, Oh-My-Pi, Split Claude Code, and New Chat. Screenshot:
`/tmp/claude-1000/.../scratchpad/wdcritic/03-after-click.png` (not
committed — ephemeral scratch capture, same convention as the builder's).
This corroborates the builder's claim with a second, independently-run
reproduction and settles the wave-D non-reproduction as session-load
flakiness, not a defect.

## `F-USE-03` — ledger line 266 — **half-proven**

Re-ran `cargo test -p tiller_usage --lib` myself: 40/40 pass, including
`model::tests::success_replaces_the_previous_state` (TimedOut -> Stale) and
`status_bar.rs`'s `unavailable_reasons_render_distinct_text` /
`only_loaded_state_renders_undimmed`, which together prove `Stale` renders
`"Claude 12% 5h"` (identical text to `Loaded`) and is the only non-`Loaded`
state distinguished purely by `segment_dimmed`, not by text change. No
regression from wave D; both owned files unchanged.

Did not attempt a live end-to-end drive. A real `claude` CLI is present on
this machine (`/home/enzopalmisano/.local/bin/claude`, unlike what the
builder's excuse implied), so the missing ingredient isn't the binary — it's
`Claude::TIMEOUT` = a real, non-shortenable 25s wall-clock wait with no
control-socket method to force or accelerate a usage refresh (grepped
`main.rs` for a `usage.*` request, found none). Forcing a live Stale
transition needs a first successful real fetch, then a PATH-shadowed
hanging `claude` binary and a second real 25s wait — real risk of this
subagent's 180s-silence kill mid-wait for marginal gain over the existing
unit-level proof. Deferring, same as wave D and the builder: reducer and
render logic are proven at the unit level; the live-UI dimming transition
stays owed to a slice with dedicated time budget for it.
