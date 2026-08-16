# E-P1 critic verdicts

Critic pass, independent of the builder. Instrument: `Scripts/wayland-drive.sh` against fresh
`project.add`-ed instances of the current HEAD binary, plus direct source review and a
revert-and-rerun discriminator experiment on the cited unit test. Session was repeatedly disrupted
by the shared worktree directory itself vanishing and reappearing (twice, once for ~2 minutes) —
almost certainly a sibling critic agent's concurrent git operation on this same shared tree, not
anything this pass did. Where that cost a clean live proof, it's called out below rather than
papered over.

## `F-TERM-UI-02` — half-proven

Code review: the diff (`9a13174`) is structurally sound — `TerminalHandle.last_bounds` is written
every real `prepaint` and subtracted from `event.position` in `on_left_mouse_down` before dividing
by cell size, mirroring the paint path exactly.

But the builder's own proof does not discriminate. I reverted only the two origin-subtraction
lines (falling back to raw `event.position`, i.e. the pre-fix bug) and reran
`platform_modifier_click_opens_a_terminal_link` unmodified: **it still passes**. The test's own
`cx.debug_bounds("terminal-drop-target")` sits at the isolated test window's origin ≈ (0,0) — the
exact coincidence `WAYLAND-LANE.md` warns about — so the test cannot tell the fixed code from the
buggy code it was written to catch. Restored the file after the experiment;
`git status --porcelain` on `tiller_terminal/src/lib.rs` confirmed clean before continuing.

Live: also independently discovered that `modclick ctrl` (used in every prior live attempt on this
row, by wave-D and by the builder) tests the **wrong modifier** — `tiller_terminal/src/link_router.rs`
and gpui's own `Modifiers::platform` doc comment ("the super key, on linux") confirm only `platform`
(Super/`logo` in `wtype` terms) opens a link; Ctrl does nothing regardless of the position-math fix.
Reran with `modclick logo` on a URL printed into a real PTY, watching `ps --ppid <tiller-pid>` and
`pgrep xdg-open` at 50ms resolution through the click — no spawn observed, but the attempt was cut
short by the worktree-vanishing incident before a second try, so this is inconclusive, not a
negative result. No live proof either direction currently exists for the fixed code.

## `F-CHAT-20` — half-proven

`chat.rs`'s `Chat::new` scroll handler (re-arms `FollowMode::Tail` via `cx.defer` when
`ListState::is_scrolled_to_end()` is true) reads as mechanically correct and matches the documented
GPUI double-borrow hazard. Not independently live-driven this pass (the Wayland lane's `scroll`
primitive is proven in general but was not spent on this row given time lost to the environment
instability above) — no new evidence beyond the builder's own honest "not independently re-driven
live" admission and a code read that found nothing wrong.

## `F-CHAT-05` — half-proven

Confirmed the `offline-placeholder` branch (`client.is_none() && !self.connecting`) sits directly in
`Chat::render`'s live `composer_parts` construction — the real composer's render path, not a
test-only duplicate — alongside the existing permission-wait and queue-placeholder branches.
`offline_composer_shows_its_own_placeholder` is a discriminating test (real missing-binary launch
fixture, not a flag) and passes. Not independently live-driven this pass: no live drive was
performed against a real disconnected agent to see the pixels this session, so this stays
half-proven rather than PASSED per the "a green test is never a pass" rule, despite the code review
being clean.

## `F-CHG-22` — PASSED

Reproduced live, independently, twice. Root cause of the collapsed-vs-expanded flakiness (both
wave-D's original miss and two of my own early attempts) is a genuine Wayland-lane hazard, not an
app defect: `shot` alternates the compositor's output resolution to force a repaint, so a click
computed from a screenshot in a *different* `wayland-drive.sh` invocation (or separated from its own
confirming `shot` by another `shot`) lands at the wrong pixel once the resolution has already
flipped underneath it. Driving `shot → click (same coords) → shot`, all three in one invocation with
nothing intervening, gets a clean result every time I tried it: chevron flips from `›` to `⌄` and
real rows (`Chat`, `Terminal`, `Changes` — the worktree's actual open tabs) render below the header.
This is discriminating — collapsed is the section's default, and the click reliably drives it to the
non-default expanded state and back. Frames: `/tmp/critic-ep1-chg22/02-07-fresh-before.png` (collapsed)
and `03-08-fresh-after.png` (expanded, same invocation, immediate click-then-shot).

## `F-CHG-18` — half-proven

Got further live than the builder did. Real-CLI-driven `rightclick` on the Terminal tab → `click` on
"Move to New Pane" (single `wayland-drive.sh` invocation, no intervening `shot`) reliably produced a
genuine two-pane layout — Changes list on the left, a real live-PTY Terminal pane on the right,
confirmed by on-screen bash prompt activity — reproduced twice. This confirms the row's setup step,
which the builder never completed. The final step — dragging a changed-file row onto the Terminal
pane and checking for the "Dropped diff: `<path>`" pill — was attempted twice and neither attempt is
conclusive: the first used a stale drag-source coordinate carried over from a separate invocation
(missed the row, no payload picked up); the second's own same-invocation screenshot showed "Local
changes (0)" — the shared worktree's changed-file list was legitimately empty at that moment
(concurrent sibling-agent commits clean the tree under this pass), leaving nothing to drag. No pill
was observed either time, but neither attempt is a clean negative. `on_drop::<(PathBuf, String)>` →
`receive_diff_drop` wiring in `tiller_terminal/src/lib.rs` was not re-audited this pass beyond
confirming (via `grep`) it is still present, unchanged since wave-D's real/non-test-only finding.

## `F-CHAT-33` — half-proven (unchanged)

`looks_like_mcp_warning` (`tiller_acp/src/lib.rs:1437`) and its call site are confirmed still present
and unmodified by `grep`. This row's negative evidence is not just this builder's word — it is three
independent live attempts across two prior sessions (2 sweeps + wave-D) with a real broken
`.mcp.json` and the real `claude` CLI, none of which produced the MCP-warning card. I did not spend
this pass's remaining time budget on a fourth identical live attempt (needs the real `claude` CLI
plus `npx`-resolved ACP bridge plus a broken `.mcp.json`, and a fourth try with the same setup would
not add information per the builder's own reasoning, which I checked and agree with). No new
evidence added by me this pass on the MCP-warning half; leaving the ledger's existing half-proven
characterization in place rather than re-asserting it as if newly confirmed.
